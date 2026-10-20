//! Api trait：四端注入不同实现，store 只认 trait
//! Desktop → ReqwestApi, Web → FetchApi(gloo-net), Test → MemoryApi
//!
//! 用 `async_trait(?Send)` 而不是默认的 `async_trait`（带 `Send` 约束）：
//! WASM 是单线程，`Send` 约束会让基于 `Rc`/`RefCell` 的实现无法编译。
//! `async_trait` 会把 `async fn` 展开成 `fn -> Pin<Box<dyn Future>>`，
//! 所以 trait 仍然是 **object safe**，可以存进 `Rc<dyn Api>`。

use async_trait::async_trait;

use crate::model::{CreateRecordRequest, DailyStat, PomodoroRecord, UpdateRecordRequest};

/// 统一错误类型，平台实现可映射为 String
#[derive(Clone, Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network: {0}")]
    Network(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("server: {0}")]
    Server(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

/// 云端 CRUD 接口。四端各实现一份，`Store` 只认这个 trait。
#[async_trait(?Send)]
pub trait Api: 'static {
    /// 拉取记录，`from`/`to` 是 unix 秒区间，None 表示不限。
    async fn list(&self, from: Option<i64>, to: Option<i64>) -> ApiResult<Vec<PomodoroRecord>>;
    /// 新建一条记录。服务端会回填权威的 `id` 与时间戳。
    async fn create(&self, req: CreateRecordRequest) -> ApiResult<PomodoroRecord>;
    /// 改标题。
    async fn update(&self, id: &str, req: UpdateRecordRequest) -> ApiResult<PomodoroRecord>;
    /// 删除。
    async fn delete(&self, id: &str) -> ApiResult<()>;
    /// 按天统计。
    async fn get_stats(&self, days: u32) -> ApiResult<Vec<DailyStat>> {
        let _ = days;
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// MemoryApi：纯内存实现，用于 M0 本地 CRUD + 单元测试，无网络
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct MemoryApi {
    // 用 Rc<RefCell> 模拟共享数据库，Clone 后共享同一份数据
    inner: std::rc::Rc<std::cell::RefCell<Vec<PomodoroRecord>>>,
}

impl MemoryApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_records(records: Vec<PomodoroRecord>) -> Self {
        Self {
            inner: std::rc::Rc::new(std::cell::RefCell::new(records)),
        }
    }
}

#[async_trait(?Send)]
impl Api for MemoryApi {
    async fn list(&self, from: Option<i64>, to: Option<i64>) -> ApiResult<Vec<PomodoroRecord>> {
        let all = self.inner.borrow().clone();
        Ok(all
            .into_iter()
            .filter(|r| {
                if let Some(f) = from {
                    if r.completed_at < f {
                        return false;
                    }
                }
                if let Some(t) = to {
                    if r.completed_at > t {
                        return false;
                    }
                }
                true
            })
            .collect())
    }

    async fn create(&self, req: CreateRecordRequest) -> ApiResult<PomodoroRecord> {
        let mut r = PomodoroRecord::new_with_phase(req.phase, req.title, req.duration_secs);
        // 保持与服务端一致：id 和时间由客户端给出
        r.id = req.id.clone();
        if req.completed_at > 0 {
            r.completed_at = req.completed_at;
            r.created_at = req.completed_at;
        }
        self.inner.borrow_mut().push(r.clone());
        Ok(r)
    }

    async fn update(&self, id: &str, req: UpdateRecordRequest) -> ApiResult<PomodoroRecord> {
        let mut borrow = self.inner.borrow_mut();
        let rec = borrow
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| ApiError::NotFound(id.to_string()))?;
        rec.title = req.title;
        Ok(rec.clone())
    }

    async fn delete(&self, id: &str) -> ApiResult<()> {
        let mut borrow = self.inner.borrow_mut();
        let before = borrow.len();
        borrow.retain(|r| r.id != id);
        if borrow.len() == before {
            return Err(ApiError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

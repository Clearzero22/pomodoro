//! Api trait：四端注入不同实现，store 只认 trait
//! Desktop → ReqwestApi, Web → FetchApi(gloo-net), Test → MemoryApi

use async_trait::async_trait;

use crate::model::{CreateRecordRequest, PomodoroRecord, UpdateRecordRequest};

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

/// `?Send` 兼容 WASM 单线程
#[async_trait(?Send)]
pub trait Api: Clone {
    async fn list(&self, from: Option<i64>, to: Option<i64>) -> ApiResult<Vec<PomodoroRecord>>;
    async fn create(&self, req: CreateRecordRequest) -> ApiResult<PomodoroRecord>;
    async fn update(&self, id: &str, req: UpdateRecordRequest) -> ApiResult<PomodoroRecord>;
    async fn delete(&self, id: &str) -> ApiResult<()>;
    async fn get_stats(&self, days: u32) -> ApiResult<Vec<crate::model::DailyStat>> {
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
        // 保持与服务端一致：id 由客户端生成
        r.id = uuid::Uuid::new_v4().to_string();
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

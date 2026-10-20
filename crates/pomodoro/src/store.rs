//! Store：CRUD 状态机 + 乐观更新 + 离线队列
//!
//! 用 `Rc<dyn Api>` / `Rc<dyn Persistence>` 而不是泛型参数，这样
//! `Pomodoro` 视图里的 store 字段是**一个具体类型**，桌面端注入
//! `ReqwestApi + FilePersistence`、Web 端注入 `FetchApi + LocalStoragePersistence`
//! 时，视图代码一行都不用改。
//!
//! `Api` 与 `Persistence` 都是 object safe 的 trait（`async_trait` 把
//! `async fn` 展开成返回 `Pin<Box<dyn Future>>` 的普通方法）。

use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::api::{Api, ApiError};
use crate::model::{CreateRecordRequest, PomodoroRecord, UpdateRecordRequest};

/// 离线操作队列
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PendingOp {
    Create(PomodoroRecord),
    Update { id: String, title: Option<String> },
    Delete(String),
}

/// 本地持久化 trait。
///
/// 平台差异留在入口 crate：
/// - Desktop → `FilePersistence`（`~/.config/pomodoro/records.json`）
/// - Web → `LocalStoragePersistence`（`localStorage["pomodoro:records"]`）
pub trait Persistence: 'static {
    fn load_records(&self) -> Vec<PomodoroRecord>;
    fn save_records(&self, records: &[PomodoroRecord]);
    fn load_pending(&self) -> Vec<PendingOp>;
    fn save_pending(&self, ops: &[PendingOp]);
}

/// 内存持久化（测试与降级用）
#[derive(Clone, Default)]
pub struct MemoryPersistence {
    records: Rc<std::cell::RefCell<Vec<PomodoroRecord>>>,
    pending: Rc<std::cell::RefCell<Vec<PendingOp>>>,
}

impl Persistence for MemoryPersistence {
    fn load_records(&self) -> Vec<PomodoroRecord> {
        self.records.borrow().clone()
    }
    fn save_records(&self, records: &[PomodoroRecord]) {
        *self.records.borrow_mut() = records.to_vec();
    }
    fn load_pending(&self) -> Vec<PendingOp> {
        self.pending.borrow().clone()
    }
    fn save_pending(&self, ops: &[PendingOp]) {
        *self.pending.borrow_mut() = ops.to_vec();
    }
}

/// CRUD Store
#[derive(Clone)]
pub struct Store {
    api: Option<Rc<dyn Api>>,
    persistence: Rc<dyn Persistence>,
    pub records: Vec<PomodoroRecord>,
    pub pending: Vec<PendingOp>,
    pub last_error: Option<String>,
    /// 是否有联网后端。没有时 `sync()` 直接返回，界面照常用本地数据。
    pub online: bool,
}

impl Store {
    /// 用具体实现构造。`api` 为 `None` 表示纯本地模式。
    pub fn new(api: Option<impl Api + 'static>, persistence: impl Persistence + 'static) -> Self {
        let records = persistence.load_records();
        let pending = persistence.load_pending();
        Self {
            api: api.map(|a| Rc::new(a) as Rc<dyn Api>),
            persistence: Rc::new(persistence),
            records,
            pending,
            last_error: None,
            online: false,
        }
    }

    /// 纯本地模式（无后端）。
    pub fn local(persistence: impl Persistence + 'static) -> Self {
        Self::new(None::<crate::api::MemoryApi>, persistence)
    }

    fn persist(&self) {
        self.persistence.save_records(&self.records);
        self.persistence.save_pending(&self.pending);
    }

    // ---- 本地乐观 CRUD (M0) ----

    /// 本地创建（乐观），入队列等待 sync
    pub fn create_local(
        &mut self,
        title: Option<String>,
        phase: crate::Phase,
        duration_secs: u32,
    ) -> PomodoroRecord {
        let rec = PomodoroRecord::new_with_phase(phase, title, duration_secs);
        self.records.push(rec.clone());
        self.pending.push(PendingOp::Create(rec.clone()));
        self.persist();
        rec
    }

    pub fn update_local(&mut self, id: &str, title: Option<String>) -> bool {
        if let Some(r) = self.records.iter_mut().find(|r| r.id == id) {
            r.title = title.clone();
            self.pending.push(PendingOp::Update {
                id: id.to_string(),
                title,
            });
            self.persist();
            return true;
        }
        false
    }

    pub fn delete_local(&mut self, id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        if self.records.len() != before {
            self.pending.push(PendingOp::Delete(id.to_string()));
            self.persist();
            return true;
        }
        false
    }

    pub fn list(&self) -> &[PomodoroRecord] {
        &self.records
    }

    /// 按日期 YYYY-MM-DD 过滤
    pub fn list_by_date(&self, date: &str) -> Vec<PomodoroRecord> {
        self.records
            .iter()
            .filter(|r| r.date_str() == date)
            .cloned()
            .collect()
    }

    /// 待同步的操作数（界面可显示「N 条待同步」）。
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    // ---- 联网同步 (M2) ----
    //
    // 拆成「网络 IO」与「应用结果」两步，是为了让 await 期间不持有
    // `&mut self`：`update(cx, |this, _| this.store.sync())` 会在 await
    // 处借用 `&mut Pomodoro`，编译不过。
    // 网络 IO 在 store 的快照上跑，结果回来后再一次性写回。

    /// 拿到底层 `Api`（纯本地模式是 None）。
    pub fn api(&self) -> Option<Rc<dyn Api>> {
        self.api.clone()
    }

    /// 推送待同步队列，返回**成功推送**的操作列表。
    ///
    /// 服务端返回 404 也算成功：远端已经没有这条，本地队列里也该作废。
    /// 其他错误直接返回 Err，队列保持原样等下次重试。
    pub async fn push_pending(&self) -> Result<Vec<PendingOp>, ApiError> {
        let Some(api) = &self.api else {
            return Ok(Vec::new());
        };
        let mut done = Vec::new();
        for op in &self.pending {
            let res: Result<(), ApiError> = match op {
                PendingOp::Create(rec) => api
                    .create(CreateRecordRequest {
                        id: rec.id.clone(),
                        title: rec.title.clone(),
                        phase: rec.phase,
                        duration_secs: rec.duration_secs,
                        completed_at: rec.completed_at,
                    })
                    .await
                    .map(|_| ()),
                PendingOp::Update { id, title } => {
                    api.update(
                        id,
                        UpdateRecordRequest {
                            title: title.clone(),
                        },
                    )
                    .await
                    .map(|_| ())
                }
                PendingOp::Delete(id) => api.delete(id).await,
            };
            match res {
                Ok(()) => done.push(op.clone()),
                Err(ApiError::NotFound(_)) => done.push(op.clone()),
                Err(e) => return Err(e),
            }
        }
        Ok(done)
    }

    /// 拉取远端记录。
    pub async fn pull_remote(&self) -> Result<Vec<PomodoroRecord>, ApiError> {
        let Some(api) = &self.api else {
            return Ok(Vec::new());
        };
        api.list(None, None).await
    }

    /// 应用推送结果：从待同步队列里移除已完成的操作。
    pub fn apply_push(&mut self, done: &[PendingOp]) {
        if done.is_empty() {
            return;
        }
        self.pending.retain(|op| !done.contains(op));
        self.persist();
    }

    /// 合并远端记录。
    ///
    /// 远端为准，但本地**还在待同步队列里**的记录要保留：那些是本地刚建、
    /// 还没推上去的，或推上去还没被服务端确认的。
    pub fn merge_remote(&mut self, remote: Vec<PomodoroRecord>) {
        let pending_ids: std::collections::HashSet<String> = self
            .pending
            .iter()
            .filter_map(|op| match op {
                PendingOp::Create(r) => Some(r.id.clone()),
                PendingOp::Update { id, .. } => Some(id.clone()),
                PendingOp::Delete(id) => Some(id.clone()),
            })
            .collect();

        let mut merged = remote;
        for r in &self.records {
            if pending_ids.contains(&r.id) && !merged.iter().any(|x| x.id == r.id) {
                merged.push(r.clone());
            }
        }
        merged.sort_by_key(|r| r.completed_at);
        self.records = merged;
        self.persist();
    }

    /// 记录同步状态。
    pub fn set_status(&mut self, online: bool, error: Option<String>) {
        self.online = online;
        self.last_error = error;
    }

    /// 一次性完成同步：推送 + 拉取。
    ///
    /// 返回 (已推送的操作, 远端记录, 错误信息)。
    /// 网络 IO 在 store 的快照上跑，调用方拿到结果后用 `apply_push` /
    /// `merge_remote` 写回真实 store。
    pub async fn sync_snapshot(
        store: &Store,
    ) -> (Vec<PendingOp>, Option<Vec<PomodoroRecord>>, Option<String>) {
        let done = match store.push_pending().await {
            Ok(v) => v,
            Err(e) => return (Vec::new(), None, Some(e.to_string())),
        };
        let remote = match store.pull_remote().await {
            Ok(v) => Some(v),
            Err(e) => return (done, None, Some(e.to_string())),
        };
        (done, remote, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MemoryApi;
    use crate::Phase;

    fn store() -> Store {
        Store::local(MemoryPersistence::default())
    }

    #[test]
    fn create_adds_to_list() {
        let mut s = store();
        s.create_local(Some("write".into()), Phase::Focus, 1500);
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.pending.len(), 1);
    }

    #[test]
    fn update_changes_title() {
        let mut s = store();
        let r = s.create_local(Some("old".into()), Phase::Focus, 1500);
        assert!(s.update_local(&r.id, Some("new".into())));
        assert_eq!(s.list()[0].title.as_deref(), Some("new"));
    }

    #[test]
    fn delete_removes() {
        let mut s = store();
        let r = s.create_local(None, Phase::Focus, 1500);
        assert!(s.delete_local(&r.id));
        assert!(s.list().is_empty());
        assert!(!s.delete_local(&r.id));
    }

    #[test]
    fn pending_survives_reload() {
        let persist = MemoryPersistence::default();
        let mut s1 = Store::local(persist.clone());
        s1.create_local(Some("a".into()), Phase::Focus, 1500);
        let s2 = Store::local(persist);
        assert_eq!(s2.list().len(), 1);
        assert_eq!(s2.pending.len(), 1);
    }
}

//! Store：CRUD 状态机 + 乐观更新 + 离线队列
//! 零 #[cfg]，四端复用。Api 注入后可切换 Memory/Reqwest/Fetch。

use serde::{Deserialize, Serialize};

use crate::api::{Api, ApiError, ApiResult};
use crate::model::{CreateRecordRequest, PomodoroRecord, UpdateRecordRequest};

/// 离线操作队列
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PendingOp {
    Create(PomodoroRecord),
    Update { id: String, title: Option<String> },
    Delete(String),
}

/// 本地持久化 trait（M0 用内存，M2 用 file/localStorage）
pub trait Persistence: Clone {
    fn load_records(&self) -> Vec<PomodoroRecord>;
    fn save_records(&self, records: &[PomodoroRecord]);
    fn load_pending(&self) -> Vec<PendingOp>;
    fn save_pending(&self, ops: &[PendingOp]);
}

/// 内存持久化（测试用）
#[derive(Clone, Default)]
pub struct MemoryPersistence {
    records: std::rc::Rc<std::cell::RefCell<Vec<PomodoroRecord>>>,
    pending: std::rc::Rc<std::cell::RefCell<Vec<PendingOp>>>,
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
pub struct Store<A: Api, P: Persistence> {
    api: A,
    persistence: P,
    pub records: Vec<PomodoroRecord>,
    pub pending: Vec<PendingOp>,
    pub last_error: Option<String>,
}

impl<A: Api, P: Persistence> Store<A, P> {
    pub fn new(api: A, persistence: P) -> Self {
        let records = persistence.load_records();
        let pending = persistence.load_pending();
        Self {
            api,
            persistence,
            records,
            pending,
            last_error: None,
        }
    }

    fn persist(&self) {
        self.persistence.save_records(&self.records);
        self.persistence.save_pending(&self.pending);
    }

    // ---- 本地乐观 CRUD (M0) ----

    /// 本地创建（乐观），入队列等待 sync
    pub fn create_local(&mut self, title: Option<String>, phase: crate::Phase, duration_secs: u32) -> PomodoroRecord {
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

    // ---- 联网同步 (M2) ----

    /// 拉取远端并合并（远端为准，本地 pending 保留）
    pub async fn sync_pull(&mut self) -> ApiResult<()> {
        match self.api.list(None, None).await {
            Ok(remote) => {
                // 简单策略：远端全量覆盖本地非 pending 的部分
                // M2 再做更细的 merge
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
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// 推送 pending 队列
    pub async fn sync_push(&mut self) -> ApiResult<()> {
        let ops = self.pending.clone();
        for op in &ops {
            let res: ApiResult<()> = match op {
                PendingOp::Create(rec) => self
                    .api
                    .create(CreateRecordRequest {
                        title: rec.title.clone(),
                        phase: rec.phase,
                        duration_secs: rec.duration_secs,
                    })
                    .await
                    .map(|_| ()),
                PendingOp::Update { id, title } => self
                    .api
                    .update(
                        id,
                        UpdateRecordRequest {
                            title: title.clone(),
                        },
                    )
                    .await
                    .map(|_| ()),
                PendingOp::Delete(id) => self.api.delete(id).await,
            };
            if let Err(e) = res {
                // 网络错保留队列，下次重试；404 视为已删，移除
                if matches!(e, ApiError::NotFound(_)) {
                    self.pending.retain(|x| x != op);
                    self.persist();
                } else {
                    self.last_error = Some(e.to_string());
                    return Err(e);
                }
            } else {
                self.pending.retain(|x| x != op);
                self.persist();
            }
        }
        self.last_error = None;
        Ok(())
    }

    pub async fn sync(&mut self) -> ApiResult<()> {
        self.sync_push().await?;
        self.sync_pull().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MemoryApi;
    use crate::Phase;

    fn store() -> Store<MemoryApi, MemoryPersistence> {
        Store::new(MemoryApi::new(), MemoryPersistence::default())
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
        let mut s1 = Store::new(MemoryApi::new(), persist.clone());
        s1.create_local(Some("a".into()), Phase::Focus, 1500);
        let s2 = Store::new(MemoryApi::new(), persist);
        assert_eq!(s2.list().len(), 1);
        assert_eq!(s2.pending.len(), 1);
    }
}

//! 联网同步的行为测试。
//!
//! 用 `MemoryApi` 当作「远端」，验证离线队列的推送与合并逻辑。
//! 真实 HTTP 由 `server/tests/api_test.rs` 覆盖。

use pomodoro::api::{Api, MemoryApi};
use pomodoro::model::{CreateRecordRequest, PomodoroRecord};
use pomodoro::store::{MemoryPersistence, PendingOp, Store};
use pomodoro::Phase;

fn local_store() -> Store {
    Store::local(MemoryPersistence::default())
}

fn online_store(remote: MemoryApi) -> Store {
    Store::new(Some(remote.clone()), MemoryPersistence::default())
}

#[tokio::test]
async fn push_sends_pending_creates() {
    let remote = MemoryApi::new();
    let mut store = online_store(remote.clone());

    store.create_local(Some("a".into()), Phase::Focus, 1500);
    store.create_local(Some("b".into()), Phase::Focus, 1500);
    assert_eq!(store.pending_count(), 2);

    let done = store.push_pending().await.unwrap();
    assert_eq!(done.len(), 2);

    // 远端现在有两条
    let remote_records = remote.list(None, None).await.unwrap();
    assert_eq!(remote_records.len(), 2);

    // 应用推送结果后本地队列清空
    store.apply_push(&done);
    assert_eq!(store.pending_count(), 0);
}

#[tokio::test]
async fn pull_merges_remote_records() {
    let remote = MemoryApi::new();
    // 远端预置一条
    remote
        .create(CreateRecordRequest {
            id: uuid::Uuid::new_v4().to_string(),
            title: Some("远端的".into()),
            phase: Phase::Focus,
            duration_secs: 1500,
            completed_at: 0,
        })
        .await
        .unwrap();

    let mut store = online_store(remote.clone());
    let (done, pulled, error) = Store::sync_snapshot(&store).await;
    assert!(error.is_none());
    assert!(done.is_empty());
    let pulled = pulled.unwrap();
    assert_eq!(pulled.len(), 1);

    store.merge_remote(pulled);
    assert_eq!(store.list().len(), 1);
    assert_eq!(store.list()[0].title.as_deref(), Some("远端的"));
}

#[tokio::test]
async fn local_pending_survives_pull() {
    // 远端有一条，本地有一条还没推上去
    let remote = MemoryApi::new();
    let mut store = online_store(remote.clone());
    let local = store.create_local(Some("本地的".into()), Phase::Focus, 1500);

    let (done, pulled, error) = Store::sync_snapshot(&store).await;
    assert!(error.is_none());
    store.apply_push(&done);
    store.merge_remote(pulled.unwrap_or_default());

    // 本地那条还在（它在待同步队列里）
    assert!(store.list().iter().any(|r| r.id == local.id));
}

#[tokio::test]
async fn local_mode_sync_is_a_noop() {
    let mut store = local_store();
    store.create_local(Some("x".into()), Phase::Focus, 1500);
    let (done, pulled, error) = Store::sync_snapshot(&store).await;
    assert!(done.is_empty());
    assert_eq!(pulled.unwrap().len(), 0);
    assert!(error.is_none());
    // 队列还在，等以后配了后端再推
    assert_eq!(store.pending_count(), 1);
}

#[tokio::test]
async fn apply_push_removes_only_completed_ops() {
    let mut store = local_store();
    let a = store.create_local(Some("a".into()), Phase::Focus, 1500);
    let b = store.create_local(Some("b".into()), Phase::Focus, 1500);
    assert_eq!(store.pending_count(), 2);

    let done = vec![PendingOp::Create(a.clone())];
    store.apply_push(&done);
    assert_eq!(store.pending_count(), 1);
    assert!(store.pending.contains(&PendingOp::Create(b)));
}

#[tokio::test]
async fn delete_then_sync_removes_from_remote() {
    let remote = MemoryApi::new();
    let mut store = online_store(remote.clone());
    let rec = store.create_local(Some("gone".into()), Phase::Focus, 1500);

    // 先推上去
    let (done, _, _) = Store::sync_snapshot(&store).await;
    store.apply_push(&done);
    assert_eq!(remote.list(None, None).await.unwrap().len(), 1);

    // 再删
    store.delete_local(&rec.id);
    let (done, _, _) = Store::sync_snapshot(&store).await;
    store.apply_push(&done);
    assert_eq!(remote.list(None, None).await.unwrap().len(), 0);
}

#[tokio::test]
async fn update_then_sync_changes_remote_title() {
    let remote = MemoryApi::new();
    let mut store = online_store(remote.clone());
    let rec = store.create_local(Some("old".into()), Phase::Focus, 1500);

    let (done, _, _) = Store::sync_snapshot(&store).await;
    store.apply_push(&done);

    store.update_local(&rec.id, Some("new".into()));
    let (done, _, _) = Store::sync_snapshot(&store).await;
    store.apply_push(&done);

    let remote_records = remote.list(None, None).await.unwrap();
    assert_eq!(remote_records.len(), 1);
    assert_eq!(remote_records[0].title.as_deref(), Some("new"));
}

#[tokio::test]
async fn pending_op_serializes_and_deserializes() {
    let rec = PomodoroRecord::new_with_phase(Phase::Break, Some("x".into()), 300);
    let op = PendingOp::Create(rec.clone());
    let json = serde_json::to_string(&op).unwrap();
    let back: PendingOp = serde_json::from_str(&json).unwrap();
    assert_eq!(op, back);
}

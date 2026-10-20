//! M0 本地 CRUD 集成测试（无网络，MemoryApi + MemoryPersistence）
//! 覆盖 009 §7 的 4 个必过用例

use pomodoro::api::MemoryApi;
use pomodoro::model::PomodoroRecord;
use pomodoro::store::{MemoryPersistence, Store};
use pomodoro::Phase;

fn store() -> Store {
    Store::local(MemoryPersistence::default())
}

#[test]
fn create_adds_to_list() {
    let mut s = store();
    let r = s.create_local(Some("write docs".into()), Phase::Focus, 25 * 60);
    assert_eq!(s.list().len(), 1);
    assert_eq!(s.list()[0].id, r.id);
    assert_eq!(s.pending.len(), 1);
}

#[test]
fn update_changes_title() {
    let mut s = store();
    let r = s.create_local(Some("old".into()), Phase::Focus, 1500);
    assert!(s.update_local(&r.id, Some("new".into())));
    assert_eq!(s.list()[0].title.as_deref(), Some("new"));
    assert!(!s.update_local("nope", Some("x".into())));
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
fn pending_survives_reload_via_persistence() {
    let persist = MemoryPersistence::default();
    let mut s1 = Store::local(persist.clone());
    s1.create_local(Some("a".into()), Phase::Focus, 1500);
    s1.create_local(Some("b".into()), Phase::Break, 300);
    let s2 = Store::local(persist);
    assert_eq!(s2.list().len(), 2);
    assert_eq!(s2.pending.len(), 2);
}

#[test]
fn list_by_date_filters() {
    let mut s = store();
    let mut r1 = PomodoroRecord::new_focus(Some("today".into()), 1500);
    r1.completed_at = chrono::Utc::now().timestamp();
    let mut r2 = PomodoroRecord::new_focus(Some("old".into()), 1500);
    r2.completed_at = 1_000_000; // 1970
    s.records.push(r1.clone());
    s.records.push(r2);
    let today = r1.date_str();
    assert_eq!(s.list_by_date(&today).len(), 1);
}

#[test]
fn online_store_uses_the_given_api() {
    let api = MemoryApi::new();
    let s = Store::new(Some(api), MemoryPersistence::default());
    assert!(s.api().is_some());
    // 纯本地模式没有 api
    assert!(store().api().is_none());
}

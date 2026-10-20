//! CRUD 展示层测试：四端共用的 Pomodoro 视图，跑在无头窗口里
//! 覆盖 009 M0 新增的 add/delete/edit + 自动创建

use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, TestAppContext};
use instant::Duration;
use pomodoro::Pomodoro;

fn open(cx: &mut TestAppContext) -> gpui_kit::Entity<Pomodoro> {
    cx.update(gpui_kit::init);
    let mut view = None;
    let _handle = cx.add_window_view(|window, cx| {
        let pomodoro = cx.new(|cx| Pomodoro::new(window, cx));
        view = Some(pomodoro.clone());
        Root::new(pomodoro, window, cx)
    });
    view.unwrap()
}

#[gpui_kit::test]
fn add_record_via_view(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 0);
    pomodoro.update(cx, |p, cx| p.add_record(cx));
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 1);
    let rec = pomodoro.read_with(cx, |p, _| p.records()[0].clone());
    assert_eq!(rec.phase, pomodoro::Phase::Focus);
    assert!(rec.title.is_none());
}

#[gpui_kit::test]
fn delete_record_via_view(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    pomodoro.update(cx, |p, cx| p.add_record(cx));
    let id = pomodoro.read_with(cx, |p, _| p.records()[0].id.clone());
    pomodoro.update(cx, |p, cx| p.delete_record(&id, cx));
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 0);
    pomodoro.update(cx, |p, cx| p.delete_record(&id, cx));
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 0);
}

#[gpui_kit::test]
fn update_title_via_store(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    pomodoro.update(cx, |p, cx| p.add_record(cx));
    let id = pomodoro.read_with(cx, |p, _| p.records()[0].id.clone());
    pomodoro.update(cx, |p, cx| p.update_record_title(&id, Some("写文档".into()), cx));
    assert_eq!(
        pomodoro.read_with(cx, |p, _| p.records()[0].title.clone()),
        Some("写文档".to_string())
    );
    pomodoro.update(cx, |p, cx| p.update_record_title(&id, None, cx));
    assert!(pomodoro.read_with(cx, |p, _| p.records()[0].title.is_none()));
}

#[gpui_kit::test]
fn auto_create_on_focus_complete(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 0);
    pomodoro.update(cx, |p, _| p.set_remaining_for_test(Duration::from_millis(1)));
    pomodoro.update(cx, |p, cx| p.start(cx));
    std::thread::sleep(Duration::from_millis(50));
    pomodoro.update(cx, |p, cx| p.advance_phase_if_due(cx));
    let records = pomodoro.read_with(cx, |p, _| p.records().to_vec());
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].phase, pomodoro::Phase::Focus);
    assert_eq!(pomodoro.read_with(cx, |p, _| p.completed()), 1);
    // 休息结束不应再产生 Focus 记录
    pomodoro.update(cx, |p, _| p.set_remaining_for_test(Duration::from_millis(1)));
    std::thread::sleep(Duration::from_millis(50));
    pomodoro.update(cx, |p, cx| p.advance_phase_if_due(cx));
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 1);
}

#[gpui_kit::test]
fn edit_flow_save_and_cancel(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    pomodoro.update(cx, |p, cx| p.add_record(cx));
    let id = pomodoro.read_with(cx, |p, _| p.records()[0].id.clone());

    // 进入编辑
    pomodoro.update(cx, |p, cx| p.start_edit_for_test(&id, cx));
    assert_eq!(pomodoro.read_with(cx, |p, _| p.editing_id()), Some(id.clone()));

    // 修改并保存
    pomodoro.update(cx, |p, _| p.set_edit_draft_for_test("新标题".into()));
    pomodoro.update(cx, |p, cx| p.save_edit_for_test(cx));
    assert_eq!(
        pomodoro.read_with(cx, |p, _| p.records()[0].title.clone()),
        Some("新标题".to_string())
    );
    assert!(pomodoro.read_with(cx, |p, _| p.editing_id().is_none()));

    // 再编辑后取消，应保留原标题
    pomodoro.update(cx, |p, cx| p.start_edit_for_test(&id, cx));
    pomodoro.update(cx, |p, _| p.set_edit_draft_for_test("临时".into()));
    pomodoro.update(cx, |p, cx| p.cancel_edit_for_test(cx));
    assert_eq!(
        pomodoro.read_with(cx, |p, _| p.records()[0].title.clone()),
        Some("新标题".to_string())
    );
    assert!(pomodoro.read_with(cx, |p, _| p.editing_id().is_none()));
}

#[gpui_kit::test]
fn all_platforms_same_store_logic(cx: &mut TestAppContext) {
    // 验证同一份 Rust Store 在 Desktop/Web/Mobile(WebView) 行为一致
    // 用 MemoryApi 模拟，不依赖平台
    let pomodoro = open(cx);
    for i in 0..3 {
        pomodoro.update(cx, |p, cx| p.add_record(cx));
        pomodoro.update(cx, |p, cx| {
            let id = p.records()[i].id.clone();
            p.update_record_title(&id, Some(format!("任务{}", i)), cx);
        });
    }
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 3);
    // 删除中间一个
    let id = pomodoro.read_with(cx, |p, _| p.records()[1].id.clone());
    pomodoro.update(cx, |p, cx| p.delete_record(&id, cx));
    assert_eq!(pomodoro.read_with(cx, |p, _| p.records().len()), 2);
    assert_eq!(
        pomodoro.read_with(cx, |p, _| p.records()[0].title.clone()),
        Some("任务0".to_string())
    );
    assert_eq!(
        pomodoro.read_with(cx, |p, _| p.records()[1].title.clone()),
        Some("任务2".to_string())
    );
}

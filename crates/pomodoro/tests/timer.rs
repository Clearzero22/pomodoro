//! 计时行为测试：跑在无头窗口里，直接断言状态，不依赖截图或真实点击。
//!
//! 这些测试盯的是「丢拍会冻结界面」那个回归：只要剩余时间来自截止时刻，
//! 即使 ticker 一拍都没跑，暂停后剩余时间也必须真的减少了。

use instant::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, TestAppContext};
use pomodoro::{BREAK_SECONDS, FOCUS_SECONDS, Phase, Pomodoro, TimerEvent};

fn open(cx: &mut TestAppContext) -> gpui_kit::Entity<Pomodoro> {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_root, _cx) = cx.add_window_view(|window, cx| {
        let pomodoro = cx.new(|cx| Pomodoro::new(window, cx));
        view = Some(pomodoro.clone());
        Root::new(pomodoro, window, cx)
    });
    view.unwrap()
}

#[gpui_kit::test]
fn starts_paused_at_the_full_focus_duration(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    pomodoro.read_with(cx, |pomodoro, _| {
        assert!(!pomodoro.is_running());
        assert_eq!(pomodoro.phase(), Phase::Focus);
        assert_eq!(pomodoro.remaining_seconds(), FOCUS_SECONDS);
        assert_eq!(pomodoro.completed(), 0);
    });
}

#[gpui_kit::test]
fn running_clock_derives_from_the_deadline(cx: &mut TestAppContext) {
    let pomodoro = open(cx);

    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    pomodoro.read_with(cx, |pomodoro, _| assert!(pomodoro.is_running()));

    std::thread::sleep(Duration::from_millis(1200));

    // 就算 ticker 一拍都没跑，剩余时间也必须按墙上时钟真的减少。
    pomodoro.read_with(cx, |pomodoro, _| {
        assert!(
            pomodoro.remaining_seconds() < FOCUS_SECONDS,
            "remaining stayed at {} after 1.2s of running",
            pomodoro.remaining_seconds()
        );
    });
}

#[gpui_kit::test]
fn pause_freezes_and_start_resumes(cx: &mut TestAppContext) {
    let pomodoro = open(cx);

    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    std::thread::sleep(Duration::from_millis(300));
    pomodoro.update(cx, |pomodoro, cx| pomodoro.pause(cx));

    let paused = pomodoro.read_with(cx, |pomodoro, _| {
        assert!(!pomodoro.is_running());
        pomodoro.remaining()
    });

    // 暂停期间时钟不动。
    std::thread::sleep(Duration::from_millis(300));
    pomodoro.read_with(cx, |pomodoro, _| {
        assert_eq!(pomodoro.remaining(), paused);
        assert!(paused < Duration::from_secs(FOCUS_SECONDS));
    });

    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    pomodoro.read_with(cx, |pomodoro, _| assert!(pomodoro.is_running()));
}

#[gpui_kit::test]
fn reset_returns_to_the_phase_start(cx: &mut TestAppContext) {
    let pomodoro = open(cx);

    pomodoro.update(cx, |pomodoro, cx| {
        pomodoro.start(cx);
        std::thread::sleep(Duration::from_millis(250));
        pomodoro.reset(cx);
    });

    pomodoro.read_with(cx, |pomodoro, _| {
        assert!(!pomodoro.is_running());
        assert_eq!(pomodoro.remaining_seconds(), FOCUS_SECONDS);
    });
}

#[gpui_kit::test]
fn switching_phase_resets_the_clock(cx: &mut TestAppContext) {
    let pomodoro = open(cx);

    pomodoro.update(cx, |pomodoro, cx| {
        pomodoro.start(cx);
        pomodoro.set_phase(Phase::Break, cx);
    });

    pomodoro.read_with(cx, |pomodoro, _| {
        assert_eq!(pomodoro.phase(), Phase::Break);
        assert!(!pomodoro.is_running());
        assert_eq!(pomodoro.remaining_seconds(), BREAK_SECONDS);
    });
}

#[gpui_kit::test]
fn a_finished_focus_phase_advances_to_break(cx: &mut TestAppContext) {
    let pomodoro = open(cx);

    // 把剩余时间压到 0 附近，让阶段边界在一次推进里就跨过去。
    pomodoro.update(cx, |pomodoro, _| {
        pomodoro.set_remaining_for_test(Duration::from_millis(1));
    });
    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    std::thread::sleep(Duration::from_millis(50));
    pomodoro.update(cx, |pomodoro, cx| pomodoro.advance_phase_if_due(cx));

    pomodoro.read_with(cx, |pomodoro, _| {
        assert_eq!(pomodoro.completed(), 1);
        assert_eq!(pomodoro.phase(), Phase::Break);
        assert!(pomodoro.is_running(), "the next phase keeps running");
    });
}

/// 应用在后台时 ticker 不跑；回到前台不能把整段时间丢掉，也不能把当前阶段重计。
#[gpui_kit::test]
fn a_long_absence_carries_the_overshoot_into_later_phases(cx: &mut TestAppContext) {
    let pomodoro = open(cx);

    // 专注刚结束就推到「已过去 92 分钟」的状态。
    // 时间线：专注→5′ 休息→25′→5′→25′→5′→25′→（5′ 休息进行到第 2 分钟）
    // 也就是跑完 4 个专注，正处于休息阶段的第 2 分钟。
    pomodoro.update(cx, |pomodoro, _| {
        pomodoro.set_remaining_for_test(Duration::from_millis(1));
    });
    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    std::thread::sleep(Duration::from_millis(50));
    pomodoro.update(cx, |pomodoro, cx| {
        pomodoro.rewind_deadline_for_test(Duration::from_secs(92 * 60));
        pomodoro.advance_phase_if_due(cx);
    });

    pomodoro.read_with(cx, |pomodoro, _| {
        assert_eq!(pomodoro.completed(), 4, "four focus phases finished in 92 minutes");
        assert_eq!(pomodoro.phase(), Phase::Break);
        // 关键断言：剩余 3 分钟（而非重计满 5 分钟）证明超出的时间被结转了。
        assert_eq!(
            pomodoro.remaining_seconds(),
            BREAK_SECONDS - 2 * 60,
            "overshoot must be carried into the running break"
        );
    });
}

#[gpui_kit::test]
fn timer_events_report_start_pause_reset_and_completion(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    pomodoro.update(cx, |pomodoro, _| {
        pomodoro.set_on_event(move |event| sink.borrow_mut().push(event));
    });

    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    pomodoro.update(cx, |pomodoro, cx| pomodoro.pause(cx));
    pomodoro.update(cx, |pomodoro, cx| pomodoro.reset(cx));

    // 预约通知靠的就是 Started 里的秒数：接收方在阶段开始时就排好。
    assert_eq!(
        events.borrow().first().copied(),
        Some(TimerEvent::Started {
            phase: Phase::Focus,
            seconds: FOCUS_SECONDS as u32,
        })
    );
    assert!(matches!(
        events.borrow().as_slice()[1],
        TimerEvent::Paused { phase: Phase::Focus, .. }
    ));
    assert!(matches!(
        events.borrow().as_slice()[2],
        TimerEvent::Reset {
            phase: Phase::Focus,
            seconds,
        } if seconds == FOCUS_SECONDS as u32
    ));

    // 阶段结束时报告上一段与下一段，让接收方为下一段重新预约。
    pomodoro.update(cx, |pomodoro, _| {
        pomodoro.set_remaining_for_test(Duration::from_millis(1));
    });
    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    std::thread::sleep(Duration::from_millis(50));
    pomodoro.update(cx, |pomodoro, cx| pomodoro.advance_phase_if_due(cx));

    assert!(events.borrow().iter().any(|event| matches!(
        event,
        TimerEvent::PhaseCompleted {
            finished: Phase::Focus,
            next: Phase::Break,
            seconds,
        } if *seconds == BREAK_SECONDS as u32
    )));
}

/// 没有注册回调时不应该有任何副作用。
#[gpui_kit::test]
fn timer_runs_without_an_event_handler(cx: &mut TestAppContext) {
    let pomodoro = open(cx);
    pomodoro.update(cx, |pomodoro, cx| pomodoro.start(cx));
    std::thread::sleep(Duration::from_millis(200));
    pomodoro.update(cx, |pomodoro, cx| pomodoro.pause(cx));
    pomodoro.read_with(cx, |pomodoro, _| assert!(!pomodoro.is_running()));
}

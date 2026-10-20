//! 番茄钟：一套业务代码，同时运行在桌面端和浏览器端。
//!
//! 这个 crate 只放业务和界面，不含任何平台相关代码。平台差异（窗口、字体、
//! 资源加载）全部留在两个入口 crate 里：
//!
//! - `pomodoro-desktop`：桌面应用入口；
//! - `pomodoro-web`：浏览器（WASM）入口。
//!
//! # 计时怎么保证不丢拍
//!
//! 计时状态以「截止时刻」为准，而不是「每秒把计数器减一」：
//!
//! - 运行中：`deadline = Instant::now() + remaining`，剩余时间由墙上时钟推算；
//! - 暂停中：`remaining` 是权威值。
//!
//! 时钟用 `instant::Instant` 而不是 `std::time::Instant`：后者在
//! `wasm32-unknown-unknown` 上会 panic（"time not implemented on this
//! platform"），`instant` 在浏览器里走 `performance.now()`，桌面端仍是系统时钟，
//! 于是同一份计时代码两边都能用。
//!
//! 这么做的直接原因是：任何依赖「每一拍都必须成功执行」的递减写法，一旦有一拍
//! 没跑到（窗口重建、任务被取消、更新失败），界面就永久冻结在没有减过的数字上，
//! 而且不会有任何报错。按截止时刻推算的话，丢拍只影响刷新频率，不影响正确性：
//! 下一次 render 读到的仍是真的剩余时间。
//!
//! ticker 只做两件事：推进阶段边界、`cx.notify()`。它不碰窗口，因此不会因为
//! 「实体没有关联窗口」这类失败而中断。

pub mod api;
pub mod model;
pub mod store;

use instant::{Duration, Instant};
use std::rc::Rc;

use gpui_kit::component::{
    ActiveTheme, Selectable, h_flex,
    button::{Button, ButtonGroup, ButtonVariants},
    progress::ProgressCircle,
    tag::{Tag, TagVariant},
};
use gpui_kit::{
    Context, FocusHandle, InteractiveElement, IntoElement, KeyBinding, ParentElement as _, Render,
    Styled as _, Task, Window, actions, div, rems,
};

/// 专注阶段的时长。
pub const FOCUS_SECONDS: u64 = 25 * 60;
/// 休息阶段的时长。
pub const BREAK_SECONDS: u64 = 5 * 60;

/// ticker 的刷新间隔。比 1 秒密，是为了让数字在整秒边界及时翻转，
/// 而不是最多慢一秒才变。
const TICK: Duration = Duration::from_millis(200);

/// 一次推进最多补算多少个阶段，防御异常时钟跳变。
const MAX_CATCH_UP_STEPS: usize = 64;

actions!(pomodoro, [ToggleTimer, ResetTimer]);

/// 番茄钟当前所处的阶段。
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    /// 专注工作。
    Focus,
    /// 短暂休息。
    Break,
}

impl Phase {
    /// 阶段名称。
    pub fn label(self) -> &'static str {
        match self {
            Phase::Focus => "专注",
            Phase::Break => "休息",
        }
    }

    /// 该阶段的完整时长。
    pub fn duration(self) -> Duration {
        Duration::from_secs(match self {
            Phase::Focus => FOCUS_SECONDS,
            Phase::Break => BREAK_SECONDS,
        })
    }

    /// 一个阶段结束后接哪个阶段。
    pub fn next(self) -> Self {
        match self {
            Phase::Focus => Phase::Break,
            Phase::Break => Phase::Focus,
        }
    }
}

/// 计时器对外报告的事件。
///
/// 界面本身不关心通知怎么发：桌面端可能只写日志，移动端壳需要在系统里预约一条
/// 本地通知（应用在后台时 JS 暂停，不能指望完成那一刻还在运行）。因此事件里带
/// 上阶段时长，让接收方能**提前预约**。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerEvent {
    /// 开始计时，`seconds` 后本阶段结束。
    Started { phase: Phase, seconds: u32 },
    /// 暂停，`remaining_seconds` 是冻住的剩余值。
    Paused { phase: Phase, remaining_seconds: u32 },
    /// 回到阶段起点。
    Reset { phase: Phase, seconds: u32 },
    /// 一个阶段自然结束，`next` 阶段已经开始计时。
    PhaseCompleted {
        finished: Phase,
        next: Phase,
        seconds: u32,
    },
}

/// 事件回调。
type EventHandler = Rc<dyn Fn(TimerEvent)>;

/// 番茄钟界面。
pub struct Pomodoro {
    focus_handle: FocusHandle,
    phase: Phase,
    /// 暂停时的剩余时长。运行中由 `deadline` 推算，这个值只作为恢复点。
    remaining: Duration,
    /// 运行中的截止时刻；`None` 表示暂停。
    deadline: Option<Instant>,
    completed: u32,
    ticker: Option<Task<()>>,
    on_event: Option<EventHandler>,
}

impl Pomodoro {
    /// 创建界面并注册快捷键。
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new("space", ToggleTimer, None),
            KeyBinding::new("secondary-r", ResetTimer, None),
        ]);

        let focus_handle = cx.focus_handle();
        // 初始焦点交给计时界面，空格键可以直接控制计时。
        focus_handle.focus(window, cx);

        Self {
            focus_handle,
            phase: Phase::Focus,
            remaining: Phase::Focus.duration(),
            deadline: None,
            completed: 0,
            ticker: None,
            on_event: None,
        }
    }

    /// 注册计时事件回调。
    ///
    /// 入口用它把「阶段即将结束」预约成系统通知。不设置时完全无副作用，
    /// 界面和测试都不依赖它。
    pub fn set_on_event(&mut self, handler: impl Fn(TimerEvent) + 'static) {
        self.on_event = Some(Rc::new(handler));
    }

    fn emit(&self, event: TimerEvent) {
        if let Some(handler) = &self.on_event {
            handler(event);
        }
    }

    /// 当前阶段是否正在计时。
    pub fn is_running(&self) -> bool {
        self.deadline.is_some()
    }

    /// 界面的键盘焦点句柄。
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// 当前阶段的阶段。
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// 已完成的专注次数。
    pub fn completed(&self) -> u32 {
        self.completed
    }

    /// 把剩余时间压到指定值，仅用于测试阶段边界。
    #[doc(hidden)]
    pub fn set_remaining_for_test(&mut self, remaining: Duration) {
        self.remaining = remaining;
    }

    /// 把运行中的截止时刻往前推，模拟应用在后台待了很久。仅用于测试。
    #[doc(hidden)]
    pub fn rewind_deadline_for_test(&mut self, elapsed: Duration) {
        if let Some(deadline) = self.deadline {
            self.deadline = Some(deadline - elapsed);
        }
    }

    /// 此刻的剩余时长。运行中由墙上时钟推算，所以丢拍不会让数字停住。
    pub fn remaining(&self) -> Duration {
        match self.deadline {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => self.remaining,
        }
    }

    /// 剩余秒数，向上取整：开始那一刻显示完整时长，走到终点才显示 0。
    pub fn remaining_seconds(&self) -> u64 {
        let millis = self.remaining().as_millis() as u64;
        millis.div_ceil(1000)
    }

    /// 开始或暂停。
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.is_running() {
            self.pause(cx);
        } else {
            self.start(cx);
        }
    }

    /// 开始计时。已经在运行时不重复启动。
    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.is_running() {
            return;
        }
        // 剩余为 0 时从整段重新开始，否则「开始」会立刻结束。
        if self.remaining.is_zero() {
            self.remaining = self.phase.duration();
        }
        self.deadline = Some(Instant::now() + self.remaining);
        self.start_ticker(cx);
        self.emit(TimerEvent::Started {
            phase: self.phase,
            seconds: self.remaining_seconds() as u32,
        });
        cx.notify();
    }

    /// 暂停计时，把剩余时长固化下来。
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if let Some(deadline) = self.deadline.take() {
            self.remaining = deadline.saturating_duration_since(Instant::now());
        }
        // 丢弃任务句柄即取消 ticker；下次开始时重新创建。
        self.ticker = None;
        self.emit(TimerEvent::Paused {
            phase: self.phase,
            remaining_seconds: self.remaining_seconds() as u32,
        });
        cx.notify();
    }

    /// 回到当前阶段的起点并暂停。
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.deadline = None;
        self.ticker = None;
        self.remaining = self.phase.duration();
        self.emit(TimerEvent::Reset {
            phase: self.phase,
            seconds: self.remaining_seconds() as u32,
        });
        cx.notify();
    }

    /// 切换阶段并暂停。
    pub fn set_phase(&mut self, phase: Phase, cx: &mut Context<Self>) {
        if self.phase == phase {
            return;
        }
        self.phase = phase;
        self.deadline = None;
        self.ticker = None;
        self.remaining = phase.duration();
        self.emit(TimerEvent::Reset {
            phase: self.phase,
            seconds: self.remaining_seconds() as u32,
        });
        cx.notify();
    }

    /// 启动 ticker。
    ///
    /// 用 `cx.spawn`（不带窗口）而不是 `spawn_in`：ticker 不需要窗口，也就不受
    /// 「实体是否关联窗口」影响。`update` 的失败被显式忽略而不是当成停止信号，
    /// 因为实体被释放时循环本来就该结束。
    fn start_ticker(&mut self, cx: &mut Context<Self>) {
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;

                let keep_running = this
                    .update(cx, |this, cx| {
                        // 实体还在，但可能已经被暂停。
                        if !this.is_running() {
                            return false;
                        }
                        this.advance_phase_if_due(cx);
                        // 每一拍都重绘，界面时钟跟着墙上时钟走。
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);

                if !keep_running {
                    break;
                }
            }
        }));
    }

    /// 若到达阶段终点则切到下一阶段并继续计时。
    ///
    /// 一次推进可以跨过多个阶段：应用在后台或页面被冻结时 ticker 不会跑，
    /// 回到前台时可能已经过了好几个完整的专注/休息周期。这里把超出的时间
    /// 结转到下一阶段，而不是把当前阶段重计，否则手机锁屏一小时后回来会凭空
    /// 多出好几轮。
    pub fn advance_phase_if_due(&mut self, _cx: &mut Context<Self>) {
        let Some(deadline) = self.deadline else {
            return;
        };
        let now = Instant::now();
        if now < deadline {
            return;
        }

        // 超出截止时刻的时长，会逐次从未结束的下一阶段里扣。
        let mut overshoot = now.saturating_duration_since(deadline);

        // 上限只是防御异常时钟跳变，正常路径两三次就结束了。
        for _ in 0..MAX_CATCH_UP_STEPS {
            let finished = self.phase;
            if finished == Phase::Focus {
                self.completed += 1;
            }

            self.phase = finished.next();
            let next_duration = self.phase.duration();

            self.emit(TimerEvent::PhaseCompleted {
                finished,
                next: self.phase,
                seconds: next_duration.as_secs() as u32,
            });

            if overshoot < next_duration {
                self.remaining = next_duration - overshoot;
                self.deadline = Some(now + self.remaining);
                return;
            }

            overshoot -= next_duration;
        }

        // 时钟跳变过大：停在最后一个阶段的起点，不再继续透支。
        self.remaining = self.phase.duration();
        self.deadline = Some(now + self.remaining);
    }

    /// 快捷键处理：空格开始/暂停。
    fn on_toggle_timer(
        &mut self,
        _action: &ToggleTimer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 焦点在按钮上时，空格属于那个按钮，这里不重复响应。
        if !self.focus_handle.is_focused(window) {
            return;
        }
        self.toggle(cx);
    }

    /// 快捷键处理：重置当前阶段。
    fn on_reset_timer(
        &mut self,
        _action: &ResetTimer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            return;
        }
        self.reset(cx);
    }
}

impl Render for Pomodoro {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        let total = self.phase.duration().as_secs_f32();
        let remaining_seconds = self.remaining_seconds();
        let progress = if total <= 0.0 {
            0.0
        } else {
            (1.0 - remaining_seconds as f32 / total).clamp(0.0, 1.0) * 100.0
        };

        let time_label = format!(
            "{:02}:{:02}",
            remaining_seconds / 60,
            remaining_seconds % 60
        );

        let running = self.is_running();
        let shortcut_hint = if self.focus_handle.is_focused(window) {
            "空格 开始/暂停 · ⌘R 重置"
        } else {
            ""
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_toggle_timer))
            .on_action(cx.listener(Self::on_reset_timer))
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                // 阶段切换：分段控件，选中项保持可见的选中态。
                ButtonGroup::new("phase")
                    .child(
                        Button::new("phase-focus")
                            .label(Phase::Focus.label())
                            .selected(self.phase == Phase::Focus),
                    )
                    .child(
                        Button::new("phase-break")
                            .label(Phase::Break.label())
                            .selected(self.phase == Phase::Break),
                    )
                    .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
                        if clicks.contains(&0) {
                            this.set_phase(Phase::Focus, cx);
                        } else if clicks.contains(&1) {
                            this.set_phase(Phase::Break, cx);
                        }
                    })),
            )
            .child(
                // 计时环：整个界面的唯一焦点。
                ProgressCircle::new("timer-ring")
                    .value(progress)
                    .color(if self.phase == Phase::Focus {
                        theme.primary
                    } else {
                        theme.success
                    })
                    .size_48()
                    .accessibility_label(format!("{} 剩余 {time_label}", self.phase.label()))
                    .child(
                        div()
                            .font_family(theme.mono_font_family.clone())
                            .text_size(rems(2.5))
                            .text_color(theme.foreground)
                            .child(time_label),
                    ),
            )
            .child(
                // 次要信息：阶段与今日进度。
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Tag::new()
                            .with_variant(if self.phase == Phase::Focus {
                                TagVariant::Primary
                            } else {
                                TagVariant::Success
                            })
                            .child(self.phase.label()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(format!("已完成 {} 个番茄", self.completed)),
                    ),
            )
            .child(
                // 主动作与次动作。
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("toggle")
                            .primary()
                            .label(if running { "暂停" } else { "开始" })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx))),
                    )
                    .child(
                        Button::new("reset")
                            .ghost()
                            .label("重置")
                            .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                    ),
            )
            .child(
                // 快捷键提示。高度固定，焦点变化时布局不跳动。
                div()
                    .h_5()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(shortcut_hint),
            )
    }
}

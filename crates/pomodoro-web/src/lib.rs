//! 浏览器（WASM）入口。
//!
//! 与桌面端的差别只有三处：平台对象、字体和资源加载。业务代码完全相同。
//!
//! 关键点：
//! - Web 平台没有系统字体库，必须用 `add_fonts` 注册内嵌字体；
//! - `run_embedded` 返回不代表已经开窗，图形初始化是异步的，
//!   `ApplicationHandle` 必须留在 `thread_local!` 里；
//! - Web 只支持一个顶层窗口，弹窗要用 Kit 的 Root 层。

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::theme::Theme;
use gpui_kit::web::{CanvasFontFallback, WebBackendPreference, WebPlatform};
use gpui_kit::{AppContext as _, Application, ApplicationHandle, WindowOptions, open_window};
use pomodoro::{Phase, Pomodoro, TimerEvent};
use wasm_bindgen::prelude::*;

thread_local! {
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
}

// 通知桥接：JS 侧在 `www/src/notifications.js` 里实现到
// `globalThis.__pomodoroNotify`。页面里没有它时（比如纯浏览器又不支持通知），
// 调用变成 no-op，不影响计时。
//
// 必须在**阶段开始**时就预约，而不是等阶段结束再发：应用退到后台后
// WebView 会暂停 JS，靠完成那一刻的回调是发不出去的。
#[wasm_bindgen(inline_js = r#"
export function pomodoro_notify_schedule(seconds, title, body) {
  return globalThis.__pomodoroNotify?.schedule(seconds, title, body) ?? null;
}
export function pomodoro_notify_cancel() {
  return globalThis.__pomodoroNotify?.cancel() ?? null;
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = pomodoro_notify_schedule)]
    fn notify_schedule(seconds: u32, title: &str, body: &str);
    #[wasm_bindgen(js_name = pomodoro_notify_cancel)]
    fn notify_cancel();
}

/// 阶段名称的中文标题。
fn phase_title(phase: Phase) -> &'static str {
    match phase {
        Phase::Focus => "专注结束",
        Phase::Break => "休息结束",
    }
}

/// 把计时事件翻译成通知预约。
fn install_notifications(view: &gpui_kit::Entity<Pomodoro>, cx: &mut gpui_kit::App) {
    view.update(cx, |this, _| {
        this.set_on_event(|event| match event {
            TimerEvent::Started { phase, seconds } => {
                notify_schedule(seconds, phase_title(phase), "计时结束，切换阶段。");
            }
            TimerEvent::Paused { .. } | TimerEvent::Reset { .. } => {
                // 暂停或重置后旧预约不再成立，否则会提前提醒。
                notify_cancel();
            }
            TimerEvent::PhaseCompleted { next, seconds, .. } => {
                // 上一阶段的预约已经触发，接着为新阶段预约。
                notify_schedule(seconds, phase_title(next), "计时结束，切换阶段。");
            }
        });
    });
}

/// 启动浏览器里的应用。
///
/// `asset_endpoint` 指向提供 `assets/icons/*.svg` 的地址；开发时由 Vite 提供，
/// 生产环境换成 CDN 即可。
///
/// `backend` 是 `"webgpu"`、`"webgl"` 或 `"auto"`（默认）。原生壳（Capacitor）
/// 应该传 `"webgl"`：WebView 的 WebGPU 探测会在 `is_browser_webgpu_supported()`
/// 上挂住，导致画面永远停在启动页。
#[wasm_bindgen]
pub fn run(asset_endpoint: Option<String>, backend: Option<String>) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("Failed to initialize logger");
    tracing_wasm::set_as_global_default();

    gpui_kit::platform::web_init();

    let backend_preference = match backend.as_deref() {
        Some("webgpu") => WebBackendPreference::WebGpu,
        Some("webgl") => WebBackendPreference::WebGl,
        _ => WebBackendPreference::Auto,
    };

    let platform = Rc::new(WebPlatform::new_with_backend_and_font_fallback(
        // 单线程 wasm：GPUI 的所有状态都在主线程。
        false,
        backend_preference,
        // 内嵌字体缺字时（emoji、访客输入的中文），用浏览器本地字体兜底。
        CanvasFontFallback::EmojiAndCjk,
    ));
    let http_client = Arc::new(platform.fetch_http_client());

    let app = Application::with_platform(platform)
        .with_http_client(http_client)
        .with_assets(gpui_kit::assets::Assets::new(
            asset_endpoint.unwrap_or_else(|| "/".to_string()),
        ));

    let launch = move |cx: &mut gpui_kit::App| {
        gpui_kit::init(cx);

        // 浏览器没有系统字体，必须自己注册内嵌字体。
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../fonts/Inter-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/JetBrainsMono-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/NotoSansSC-Regular.ttf").as_slice()),
                // Web 平台把 `.SystemUIFont` 解析成 IBM Plex Sans，
                // 这个族必须在第一个窗口之前存在，否则首帧文本测量就会失败。
                Cow::Borrowed(include_bytes!("../fonts/IBMPlexSans-Regular.ttf").as_slice()),
            ])
            .expect("Failed to load fonts");

        // 主题里的字体族在桌面端指向系统字体，Web 端要改回内嵌字体。
        Theme::update(cx, |theme| {
            theme.font_family = "Inter".into();
            theme.mono_font_family = "JetBrains Mono".into();
        });

        open_window(WindowOptions::default(), cx, |window, cx| {
            let view = cx.new(|cx| Pomodoro::new(window, cx));
            install_notifications(&view, cx);
            view
        })
        .expect("Failed to open window");

        cx.activate(true);
    };

    // `run_embedded` 会立刻返回，图形初始化在后台继续。
    // 返回的句柄必须保留，否则应用会被回收。
    APPLICATION.with(|application| {
        *application.borrow_mut() = Some(app.run_embedded(launch));
    });

    Ok(())
}

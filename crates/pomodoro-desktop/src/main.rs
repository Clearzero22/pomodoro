//! 桌面端入口。
//!
//! 桌面端和浏览器端共用 `pomodoro` 这个业务 crate，入口只负责三件事：
//! 准备平台、初始化 Kit、打开窗口。

use gpui_kit::{AppContext as _, WindowOptions, open_window};
use pomodoro::Pomodoro;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            // 必须在创建窗口之前初始化。
            gpui_kit::init(cx);

            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Pomodoro::new(window, cx))
            })
            .expect("Failed to open window");
        });
}

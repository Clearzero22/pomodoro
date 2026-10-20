//! 桌面端入口。
//!
//! 桌面端和浏览器端共用 `pomodoro` 这个业务 crate，入口只负责三件事：
//! 准备平台、初始化 Kit、打开窗口。
//!
//! 这里额外注入本地持久化：记录写到 `%APPDATA%\pomodoro\records.json`
//! （macOS / Linux 见 `persistence.rs` 的说明），重启后历史记录仍在。

use gpui_kit::{AppContext as _, WindowOptions, open_window};
use pomodoro::Pomodoro;
use pomodoro::store::Store;

use pomodoro_desktop::api::HttpApi;
use pomodoro_desktop::persistence::{self, FilePersistence};

/// 后端地址。默认指向本机开发服务器；发布时用 `POMODORO_API_URL` 覆盖。
fn api_url() -> String {
    std::env::var("POMODORO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string())
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            // 必须在创建窗口之前初始化。
            gpui_kit::init(cx);

            // 有后端就联网，没有就纯本地。
            // 令牌存在 `~/.config/pomodoro/token`（由登录流程写入）。
            let token = persistence::load_token();
            let store = match token {
                Some(token) => Store::new(Some(HttpApi::new(api_url(), Some(token))), FilePersistence::new()),
                None => Store::local(FilePersistence::new()),
            };

            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Pomodoro::new_with_store(window, cx, store))
            })
            .expect("Failed to open window");
        });
}

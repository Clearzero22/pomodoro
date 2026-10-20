//! 浏览器端的本地持久化：用 `localStorage` 存记录和待同步队列。
//!
//! key 固定为 `pomodoro:records` / `pomodoro:pending`，内容是 JSON。
//! 无痕模式或 `localStorage` 被禁用时，`get`/`set` 会抛异常，这里全部
//! 吞掉并退化成「内存有效」，不因为存储不可用就让应用起不来。
//!
//! 存储失败只影响跨会话保留，不影响计时和增删改查本身。

use pomodoro::model::PomodoroRecord;
use pomodoro::store::{PendingOp, Persistence};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function pomodoro_storage_get(key) {
  try {
    return globalThis.localStorage?.getItem(key) ?? null;
  } catch (error) {
    console.warn('localStorage.get failed:', error);
    return null;
  }
}
export function pomodoro_storage_set(key, value) {
  try {
    globalThis.localStorage?.setItem(key, value);
    return true;
  } catch (error) {
    // 配额满或无痕模式：忽略，不影响本次会话。
    console.warn('localStorage.set failed:', error);
    return false;
  }
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = pomodoro_storage_get)]
    fn storage_get(key: &str) -> Option<String>;
    #[wasm_bindgen(js_name = pomodoro_storage_set)]
    fn storage_set(key: &str, value: &str) -> bool;
}

const RECORDS_KEY: &str = "pomodoro:records";
const PENDING_KEY: &str = "pomodoro:pending";
const TOKEN_KEY: &str = "pomodoro:token";

/// 读取保存的访问令牌。登录页写入 `localStorage`。
pub fn load_token() -> Option<String> {
    let token = storage_get(TOKEN_KEY)?.trim().to_string();
    if token.is_empty() { None } else { Some(token) }
}

/// 保存访问令牌。
pub fn save_token(token: &str) {
    storage_set(TOKEN_KEY, token);
}

/// 基于 `localStorage` 的持久化。
#[derive(Clone, Default)]
pub struct LocalStoragePersistence;

impl LocalStoragePersistence {
    pub fn new() -> Self {
        Self
    }
}

impl Persistence for LocalStoragePersistence {
    fn load_records(&self) -> Vec<PomodoroRecord> {
        let Some(text) = storage_get(RECORDS_KEY) else {
            return Vec::new();
        };
        // 坏数据不能让应用起不来：解析失败当空的。
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn save_records(&self, records: &[PomodoroRecord]) {
        if let Ok(text) = serde_json::to_string(records) {
            storage_set(RECORDS_KEY, &text);
        }
    }

    fn load_pending(&self) -> Vec<PendingOp> {
        let Some(text) = storage_get(PENDING_KEY) else {
            return Vec::new();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn save_pending(&self, ops: &[PendingOp]) {
        if let Ok(text) = serde_json::to_string(ops) {
            storage_set(PENDING_KEY, &text);
        }
    }
}

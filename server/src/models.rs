//! 数据模型：与客户端 `crates/pomodoro/src/model.rs` 字段一一对应。
//!
//! 客户端的 `PomodoroRecord` 用 `Phase` 枚举，这里用字符串 + 解析，
//! 这样服务端不依赖客户端 crate（避免循环依赖），但字段名完全一致。

use serde::{Deserialize, Serialize};

/// 阶段。字符串形式是 `focus` / `break`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Focus,
    Break,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Focus => "focus",
            Phase::Break => "break",
        }
    }
}

impl std::str::FromStr for Phase {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "focus" | "Focus" => Ok(Phase::Focus),
            "break" | "Break" => Ok(Phase::Break),
            _ => Err(()),
        }
    }
}

/// 一条番茄记录。字段名与客户端完全一致。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroRecord {
    pub id: String,
    pub user_id: String,
    pub title: Option<String>,
    pub phase: Phase,
    pub duration_secs: u32,
    pub completed_at: i64,
    pub created_at: i64,
}

/// 按天统计。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyStat {
    pub date: String,
    pub completed: u32,
    pub focus_minutes: u32,
}

/// 创建请求。
///
/// `id` 和 `completed_at` 由客户端给出，服务端原样保存：
/// 这样本地记录和云端记录主键一致，同步时才能对上。
#[derive(Clone, Debug, Deserialize)]
pub struct CreateRecordRequest {
    pub id: String,
    pub title: Option<String>,
    pub phase: Phase,
    pub duration_secs: u32,
    pub completed_at: i64,
}

/// 更新请求（目前只改标题）。
#[derive(Clone, Debug, Deserialize)]
pub struct UpdateRecordRequest {
    pub title: Option<String>,
}

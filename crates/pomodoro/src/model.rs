//! 数据模型：前后端共享的 serde 结构
//! 零 `#[cfg]`，四端 + server 同构

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Phase;

/// 一条番茄记录（CRUD 核心实体）
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroRecord {
    /// 主键，客户端生成 uuid v4
    pub id: String,
    /// 归属用户，单用户阶段固定 "local"
    pub user_id: String,
    /// 本次专注做什么，可为空
    #[serde(default)]
    pub title: Option<String>,
    /// 所属阶段
    pub phase: Phase,
    /// 实际用时（秒）
    pub duration_secs: u32,
    /// 完成时刻（unix 秒）
    pub completed_at: i64,
    /// 创建时刻（unix 秒）
    pub created_at: i64,
}

impl PomodoroRecord {
    /// 新建一条专注记录，id 自动生成，时间为 now
    pub fn new_focus(title: Option<String>, duration_secs: u32) -> Self {
        let now = Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: "local".to_string(),
            title,
            phase: Phase::Focus,
            duration_secs,
            completed_at: now,
            created_at: now,
        }
    }

    /// 新建一条指定 phase 的记录（用于测试/休息记录）
    pub fn new_with_phase(phase: Phase, title: Option<String>, duration_secs: u32) -> Self {
        let mut r = Self::new_focus(title, duration_secs);
        r.phase = phase;
        r
    }

    /// 完成时刻的日期字符串 YYYY-MM-DD，用于按天聚合
    pub fn date_str(&self) -> String {
        DateTime::<Utc>::from_timestamp(self.completed_at, 0)
            .unwrap_or_else(Utc::now)
            .format("%Y-%m-%d")
            .to_string()
    }
}

/// 按天聚合的统计
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyStat {
    pub date: String, // YYYY-MM-DD
    pub completed: u32,
    pub focus_minutes: u32,
}

impl DailyStat {
    pub fn new(date: String) -> Self {
        Self {
            date,
            completed: 0,
            focus_minutes: 0,
        }
    }
}

/// 创建/更新时的 DTO。
///
/// `id` 由**客户端**生成并带上，服务端原样保存。这样本地记录和云端记录
/// 的主键一致，同步时才能把两边对上（否则两边各生成一个 id，合并时
/// 会变成两条重复记录）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateRecordRequest {
    /// 客户端生成的 uuid v4。
    pub id: String,
    pub title: Option<String>,
    pub phase: Phase,
    pub duration_secs: u32,
    /// 完成时刻（unix 秒）。客户端本地记录的时间。
    pub completed_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateRecordRequest {
    pub title: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_date_str_is_yyyy_mm_dd() {
        let mut r = PomodoroRecord::new_focus(Some("test".into()), 1500);
        r.completed_at = 1_700_000_000; // 2023-11-14
        assert_eq!(r.date_str().len(), 10);
        assert!(r.date_str().contains('-'));
    }

    #[test]
    fn record_serde_roundtrip() {
        let r = PomodoroRecord::new_focus(Some("hello".into()), 25 * 60);
        let json = serde_json::to_string(&r).unwrap();
        let back: PomodoroRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}

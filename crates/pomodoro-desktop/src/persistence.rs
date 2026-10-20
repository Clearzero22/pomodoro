//! 桌面端的本地持久化：把记录和待同步队列写进 JSON 文件。
//!
//! 位置遵循平台惯例：
//! - Windows: `%APPDATA%\pomodoro\records.json`
//! - macOS:   `~/Library/Application Support/pomodoro/records.json`
//! - Linux:   `$XDG_CONFIG_HOME/pomodoro/records.json`（默认 `~/.config/pomodoro/records.json`）
//!
//! 写入是「先写临时文件再改名」：断电或崩溃时不会留下半个坏文件。

use std::path::PathBuf;

use pomodoro::model::PomodoroRecord;
use pomodoro::store::{PendingOp, Persistence};
use serde::{Deserialize, Serialize};

/// 落盘的数据结构。
#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    records: Vec<PomodoroRecord>,
    #[serde(default)]
    pending: Vec<PendingOp>,
}

/// 基于文件的持久化。
#[derive(Clone)]
pub struct FilePersistence {
    path: PathBuf,
}

impl FilePersistence {
    /// 用默认位置创建。
    pub fn new() -> Self {
        Self::with_path(default_path())
    }

    /// 指定文件位置（测试用）。
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> PersistedState {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            // 文件不存在或读不了（权限），当作空状态。
            return PersistedState::default();
        };
        // 坏文件不能让应用起不来：解析失败就当空的。
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn write(&self, state: &PersistedState) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let Ok(text) = serde_json::to_string_pretty(state) else {
            return;
        };
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

impl Default for FilePersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl Persistence for FilePersistence {
    fn load_records(&self) -> Vec<PomodoroRecord> {
        self.read().records
    }

    fn save_records(&self, records: &[PomodoroRecord]) {
        let mut state = self.read();
        state.records = records.to_vec();
        self.write(&state);
    }

    fn load_pending(&self) -> Vec<PendingOp> {
        self.read().pending
    }

    fn save_pending(&self, ops: &[PendingOp]) {
        let mut state = self.read();
        state.pending = ops.to_vec();
        self.write(&state);
    }
}

/// 读取保存的访问令牌。
///
/// 令牌是登录后写到 `~/.config/pomodoro/token` 的纯文本。
/// 文件不存在就是没登录过，按本地模式跑。
pub fn load_token() -> Option<String> {
    let path = config_dir().join("pomodoro").join("token");
    let text = std::fs::read_to_string(path).ok()?;
    let token = text.trim().to_string();
    if token.is_empty() { None } else { Some(token) }
}

/// 保存访问令牌。
pub fn save_token(token: &str) -> std::io::Result<()> {
    let dir = config_dir().join("pomodoro");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("token"), token)
}

/// 默认数据目录。
fn default_path() -> PathBuf {
    config_dir().join("pomodoro").join("records.json")
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("POMODORO_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata);
        }
    }
    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library").join("Application Support");
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config");
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pomodoro::model::CreateRecordRequest;
    use pomodoro::Phase;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pomodoro-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn records_survive_a_new_instance() {
        let dir = tmp_dir("records");
        let path = dir.join("records.json");

        let p = FilePersistence::with_path(path.clone());
        let rec = PomodoroRecord::new_with_phase(Phase::Focus, Some("写文档".into()), 1500);
        p.save_records(std::slice::from_ref(&rec));

        // 重新打开同一个文件，数据还在
        let p2 = FilePersistence::with_path(path);
        let loaded = p2.load_records();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title.as_deref(), Some("写文档"));
        assert_eq!(loaded[0].id, rec.id);
    }

    #[test]
    fn pending_queue_survives_a_new_instance() {
        let dir = tmp_dir("pending");
        let path = dir.join("records.json");

        let p = FilePersistence::with_path(path.clone());
        let rec = PomodoroRecord::new_with_phase(Phase::Focus, None, 1500);
        p.save_pending(&[PendingOp::Create(rec.clone())]);

        let p2 = FilePersistence::with_path(path);
        let loaded = p2.load_pending();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], PendingOp::Create(rec));
    }

    #[test]
    fn missing_file_yields_empty_state() {
        let dir = tmp_dir("missing");
        let p = FilePersistence::with_path(dir.join("nope.json"));
        assert!(p.load_records().is_empty());
        assert!(p.load_pending().is_empty());
    }

    #[test]
    fn corrupt_file_does_not_panic() {
        let dir = tmp_dir("corrupt");
        let path = dir.join("records.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{ this is not json").unwrap();

        let p = FilePersistence::with_path(path);
        assert!(p.load_records().is_empty());
    }

    #[test]
    fn create_request_roundtrips_through_json() {
        let req = CreateRecordRequest {
            id: "11111111-2222-3333-4444-555555555555".into(),
            title: Some("标题".into()),
            phase: Phase::Break,
            duration_secs: 300,
            completed_at: 1_700_000_000,
        };
        let text = serde_json::to_string(&req).unwrap();
        let back: CreateRecordRequest = serde_json::from_str(&text).unwrap();
        assert_eq!(back.title, Some("标题".to_string()));
        assert_eq!(back.phase, Phase::Break);
        assert_eq!(back.duration_secs, 300);
        assert_eq!(back.id, "11111111-2222-3333-4444-555555555555");
        assert_eq!(back.completed_at, 1_700_000_000);
    }
}

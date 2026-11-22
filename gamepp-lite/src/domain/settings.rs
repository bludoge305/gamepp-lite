//! 本地设置，落盘为 `%LOCALAPPDATA%\GamePP-Lite\config.json`。

use crate::opt::OptPlan;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// 识别轮询间隔（毫秒）。对应 GPPPlatform 的前台窗口轮询节拍。
    pub poll_ms: u64,
    /// 连续命中多少次才判为"进入游戏"，用于过滤 alt-tab 抖动。
    pub stable_polls: u32,
    /// 关闭窗口时收进托盘（而非退出）。
    pub close_to_tray: bool,
    /// 托盘气泡通知开关（对应 `displayBalloon`）。
    pub balloon: bool,
    /// 开机自启。
    pub autostart: bool,
    /// 会话历史保留条数。
    pub history_limit: usize,
    /// 日志保留条数。
    pub log_limit: usize,
    /// Windows 优化清单（含"识别到游戏自动执行 / 退出自动还原"两个开关）。
    pub opt: OptPlan,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_ms: 1000,
            stable_polls: 2,
            close_to_tray: true,
            balloon: true,
            autostart: false,
            history_limit: 30,
            log_limit: 300,
            opt: OptPlan::default(),
        }
    }
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(base).join("GamePP-Lite");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(config_path(), text)
    }
}

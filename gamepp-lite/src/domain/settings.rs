//! 本地设置，落盘为 `%LOCALAPPDATA%\GamePP-Lite\config.json`。

use crate::opt::OptPlan;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

/// 原子落盘：先写同目录临时文件，再整体替换目标文件。
///
/// 直接 `fs::write` 在进程被强杀/断电时会留下半截 JSON，下次启动解析失败
/// 就静默回落到默认值，用户设置（含优化勾选）等于被清空。`rename` 在
/// Windows 上走 `MoveFileEx(REPLACE_EXISTING)`，替换是原子的。
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // 临时文件留着无意义，失败时顺手清掉（清不掉也不影响返回值）。
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                // 解析失败说明文件已损坏（旧版本非原子写、用户手改出错等）。
                // 先把它另存一份再回落默认值，避免下一次保存把用户原配置直接盖掉。
                let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
                crate::log_line("config", &format!("config.json 解析失败，已另存为 .corrupt：{e}"));
                Self::default()
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        write_atomic(&config_path(), &text)
    }
}

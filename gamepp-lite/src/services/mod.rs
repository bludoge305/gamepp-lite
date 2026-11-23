//! 服务层：识别服务与托盘服务。全部跑在独立线程，UI 只读快照。
//!
//! 逆向对照的进程/线程模型（原程序）：
//! ```text
//! GamePP.exe (Electron 主进程)
//!   ├── core services ── GPPPlatform.exe（前台窗口识别，独立进程）
//!   ├── gameppoptimizesdk ── 触发后执行 Windows 优化
//!   └── wapTray features ── 原生托盘 + 气泡
//! ```
//! 本工程把"独立进程"压成"独立线程"，但保留同样的边界与数据流方向。

pub mod tray;
pub mod watcher;

use crate::domain::games::{Confidence, GameLibrary};
use crate::win::foreground::Foreground;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// 识别状态机。
#[derive(Clone, Debug, PartialEq, Default)]
pub enum DetectState {
    /// 前台不是游戏。
    #[default]
    Idle,
    /// 连续命中但还没到稳定阈值。
    Candidate { pid: u32, exe: String, ticks: u32 },
    /// 已确认进入游戏。
    InGame {
        pid: u32,
        exe: String,
        name: String,
        confidence: Confidence,
        since: u64,
        /// 游戏库里的标题 id（`beginGameOptimization(gameId)` 的那个整数）。
        /// 决定「特殊标题」优先级规则；库里没有这一条时为 None。
        game_id: Option<u32>,
    },
}

/// 一次游戏会话（只保留时长，不再采集逐秒遥测）。
#[derive(Clone, Debug)]
pub struct Session {
    pub exe: String,
    pub name: String,
    pub started: u64,
    pub ended: Option<u64>,
}

impl Session {
    pub fn new(exe: String, name: String, started: u64) -> Self {
        Self {
            exe,
            name,
            started,
            ended: None,
        }
    }

    pub fn duration_secs(&self) -> u64 {
        self.ended.unwrap_or_else(now_epoch).saturating_sub(self.started)
    }
}

/// 会话结束后的摘要（① 页"最近识别记录"）。
#[derive(Clone, Debug)]
pub struct SessionSummary {
    pub exe: String,
    pub name: String,
    pub started: u64,
    pub duration_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Info,
    Warn,
    Action,
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub epoch: u64,
    pub level: Level,
    pub text: String,
}

/// 服务 → UI 的通知。
#[derive(Clone, Debug)]
pub enum UiNotice {
    /// 识别到游戏（`pid` 用于 Windows 优化的特殊标题判定）。
    GameStarted { pid: u32, exe: String },
    /// 游戏退出。
    GameEnded,
}

/// 服务线程与 UI 共享的可热更新配置。
#[derive(Clone)]
pub struct Config {
    pub poll_ms: u64,
    pub stable_polls: u32,
    pub history_limit: usize,
    pub log_limit: usize,
    pub library: GameLibrary,
}

/// 全部共享状态；服务线程写，UI 每帧读。
#[derive(Default)]
pub struct Shared {
    pub foreground: Option<Foreground>,
    /// 当前前台窗口命中受保护白名单时，记下原因（识别页展示「已跳过」）。
    pub protected: Option<&'static str>,
    pub detect: DetectState,
    pub session: Option<Session>,
    pub history: VecDeque<SessionSummary>,
    pub log: VecDeque<LogLine>,
    pub polls: u64,
    pub running: bool,
}

impl Shared {
    pub fn push_log(&mut self, level: Level, text: impl Into<String>, cap: usize) {
        self.log.push_back(LogLine {
            epoch: now_epoch(),
            level,
            text: text.into(),
        });
        while self.log.len() > cap {
            self.log.pop_front();
        }
    }
}

pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn fmt_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// 服务句柄：UI 通过它改写配置并接收通知。
pub struct Services {
    pub shared: Arc<Mutex<Shared>>,
    pub cfg: Arc<Mutex<Config>>,
    pub notices: std::sync::mpsc::Receiver<UiNotice>,
}

impl Services {
    pub fn start(cfg: Config) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let cfg = Arc::new(Mutex::new(cfg));
        let (tx, rx) = std::sync::mpsc::channel::<UiNotice>();

        {
            let shared = shared.clone();
            let cfg = cfg.clone();
            std::thread::Builder::new()
                .name("gpp-watcher".into())
                .spawn(move || watcher::run(shared, cfg, tx))
                .ok();
        }

        if let Ok(mut s) = shared.lock() {
            s.running = true;
        }
        Self {
            shared,
            cfg,
            notices: rx,
        }
    }

    pub fn update_cfg(&self, f: impl FnOnce(&mut Config)) {
        if let Ok(mut c) = self.cfg.lock() {
            f(&mut c);
        }
    }

    pub fn snapshot_log(&self, level: Level, text: impl Into<String>) {
        // 上限跟着用户设置走，别写死 300（否则改小/改大都不生效）。
        let cap = self
            .cfg
            .lock()
            .map(|c| c.log_limit.max(20))
            .unwrap_or(300);
        if let Ok(mut s) = self.shared.lock() {
            s.push_log(level, text, cap);
        }
    }
}

//! Windows 优化项集合（用户可勾选的那份清单）。
//!
//! 对应原版 `GamePPOpt.ini [SYS]` 的 22 个单字节开关；这里只保留界面上真正有动作的项，
//! 并按"常用 / 高级"分组，另外加两个本工程自己的自动化开关（原版由主进程的状态机隐式完成）。

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct OptPlan {
    // ---------------- 常用 ----------------
    /// SystemGameProPriority：命中的游戏进程提到 ABOVE_NORMAL。
    pub game_priority: bool,
    /// SystemKillBackPro：结束后台黑名单进程（不可恢复）。
    pub kill_background: bool,
    /// SystemAIPower：切到游戏加加的高性能电源方案。
    pub power_high_perf: bool,
    /// SystemAIMemory：对全部进程 EmptyWorkingSet。
    pub free_memory: bool,
    /// SystemWallPaperEngnine：结束名字含 wallpaper 的进程（还原时会重新启动）。
    pub wallpaper_engine: bool,
    /// SystemXblAuthManager：停止 Xbox Live 身份验证管理器。
    pub xbl_auth: bool,
    /// SystemWindwosSearch：停止 Windows Search。
    pub windows_search: bool,
    /// SystemFlushDns：ipconfig /flushdns。
    pub flush_dns: bool,
    /// SystemCleanClipBoard：清空剪贴板（不可恢复）。
    pub clean_clipboard: bool,

    // ---------------- 高级 ----------------
    /// SystemWindowsUpdate：停止 wuauserv。
    pub windows_update: bool,
    /// SystemDiagtrack：停止 DiagTrack（客户体验改善）。
    pub diag_track: bool,
    /// SystemDiagsvc：停止 Diagsvc。
    pub diag_svc: bool,
    /// SystemHomeGroupProvider/Listener：停止家庭组两个服务。
    pub homegroup: bool,

    // ---------------- 自动化（原版由主进程状态机隐式完成） ----------------
    /// 识别到游戏时自动执行本清单。
    pub auto_on_game_start: bool,
    /// 游戏退出后自动还原。
    pub auto_restore_on_exit: bool,
}

impl Default for OptPlan {
    fn default() -> Self {
        // 默认全关：这些动作会停服务、杀进程、清剪贴板，必须先由用户明确勾选。
        Self {
            game_priority: false,
            kill_background: false,
            power_high_perf: false,
            free_memory: false,
            wallpaper_engine: false,
            xbl_auth: false,
            windows_search: false,
            flush_dns: false,
            clean_clipboard: false,
            windows_update: false,
            diag_track: false,
            diag_svc: false,
            homegroup: false,
            auto_on_game_start: false,
            auto_restore_on_exit: false,
        }
    }
}

impl OptPlan {
    pub fn enabled_count(&self) -> usize {
        [
            self.game_priority,
            self.kill_background,
            self.power_high_perf,
            self.free_memory,
            self.wallpaper_engine,
            self.xbl_auth,
            self.windows_search,
            self.flush_dns,
            self.clean_clipboard,
            self.windows_update,
            self.diag_track,
            self.diag_svc,
            self.homegroup,
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }

    /// 是否包含"不可撤销"的动作。Wallpaper Engine 已可在还原时重新启动，不算。
    pub fn has_destructive(&self) -> bool {
        self.kill_background || self.clean_clipboard
    }

    /// 服务名 + 是否处理。顺序与原始实现 `sub_100141e0` 里 8 条字符串的顺序一致。
    ///
    /// `SysMain` 恒为 false 是**照抄原版**：原版那个配置字节从不被写入，所以走 ini 路径
    /// 永远不会停 SysMain。这里保留这个行为并在界面上标注，避免"看起来能停却停不掉"。
    pub fn services(&self) -> [(&'static str, bool); 8] {
        [
            ("SysMain", false),
            ("HomeGroupListener", self.homegroup),
            ("HomeGroupProvider", self.homegroup),
            ("wuauserv", self.windows_update),
            ("WSearch", self.windows_search),
            ("XblAuthManager", self.xbl_auth),
            ("DiagTrack", self.diag_track),
            ("Diagsvc", self.diag_svc),
        ]
    }
}

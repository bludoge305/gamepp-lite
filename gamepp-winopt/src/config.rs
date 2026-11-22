//! GamePPOpt.ini 读取器。
//!
//! 原始实现（GamePPOptimize.node!sub_10013bd0）在每次 begin/restore 时都会用
//! GetPrivateProfileIntW 把整个 [SYS] 节一次性读进结构体，缺省值即第三个参数。
//! 这里保持完全相同的键名、缺省值与语义：值 == 0 视为关闭，非 0 视为开启。

use crate::ffi;

/// 与 sub_10013bd0 写入的结构体字段一一对应（注释里是结构体字节偏移）。
#[derive(Debug, Clone)]
#[allow(dead_code)] // 部分键（锁定帧数/NVIDIA 等）原模块也未在 Windows 优化路径使用
pub struct Config {
    // ---- 单字节开关（偏移 +0x00 .. +0x15） ----
    pub ai_power: bool,                  // +0x00 SystemAIPower           默认 0
    pub lock_fps: bool,                  // +0x01 SystemLockFPS           默认 0
    pub clean_clipboard: bool,           // +0x02 SystemCleanClipBoard    默认 0
    pub limit_core: bool,                // +0x03 SystemLimitCore         默认 0
    pub flush_dns: bool,                 // +0x04 SystemFlushDns          默认 0
    pub wallpaper_engine: bool,          // +0x05 SystemWallPaperEngnine  默认 0
    pub diag_svc: bool,                  // +0x06 SystemDiagsvc           默认 0
    pub diag_track: bool,                // +0x07 SystemDiagtrack         默认 0
    /// +0x08 对应 SysMain，但 sub_10013bd0 从不写这个字节，所以恒为 0。
    pub sysmain: bool,                   // +0x08 （常量 0，永不生效）
    pub windows_update: bool,            // +0x09 SystemWindowsUpdate     默认 0
    pub xbl_auth_manager: bool,          // +0x0A SystemXblAuthManager    默认 0
    pub windows_search: bool,            // +0x0B SystemWindwosSearch     默认 0
    pub homegroup_provider: bool,        // +0x0C SystemHomeGroupProvider 默认 0
    pub homegroup_listener: bool,        // +0x0D SystemHomeGroupListener 默认 0
    pub windows_aero: bool,              // +0x0E SystemWindowsAero       默认 0
    pub power_on_off: bool,              // +0x0F SystemPowerOnOff        默认 0
    pub nvidia_gpus: bool,               // +0x10 SystemNVIDIAGPUs        默认 0
    pub game_priority: bool,             // +0x11 SystemGameProPriority   默认 0
    pub kill_background: bool,           // +0x12 SystemKillBackPro       默认 0
    pub ai_res_geli: bool,               // +0x13 SystemAIResGeLi         默认 0
    pub ai_memory: bool,                 // +0x14 SystemAIMemory          默认 0
    pub ai_infra_res: bool,              // +0x15 SystemAIInfrastructureRes 默认 0

    // ---- DWORD 字段 ----
    pub game_close_power: u32,           // +0x18 SystemGameClosePower    默认 0
    pub gameing_power: u32,              // +0x1C SystemGameingPower      默认 0
    pub nvidia_sets: u32,                // +0x20 SystemNVIDIASets        默认 0
    pub lock_fps_value: u32,             // +0x24 SystemLockFPSValue      默认 0
    pub amd_ccd0: u32,                   // +0x28 SystemAMDOnlyMoveGameToCCD0 默认 0

    pub ini_path: String,
}

const SECTION: &str = "SYS";

impl Config {
    /// 读取 ini；文件不存在时所有键取缺省值（GetPrivateProfileIntW 的行为）。
    pub fn load(ini_path: &str) -> Self {
        let file = ffi::wide(ini_path);
        let sec = ffi::wide(SECTION);

        let int = |key: &str, default: i32| -> u32 {
            let k = ffi::wide(key);
            unsafe { ffi::GetPrivateProfileIntW(sec.as_ptr(), k.as_ptr(), default, file.as_ptr()) }
        };
        let flag = |key: &str| -> bool { int(key, 0) != 0 };

        Config {
            ai_power: flag("SystemAIPower"),
            lock_fps: flag("SystemLockFPS"),
            clean_clipboard: flag("SystemCleanClipBoard"),
            limit_core: flag("SystemLimitCore"),
            flush_dns: flag("SystemFlushDns"),
            wallpaper_engine: flag("SystemWallPaperEngnine"),
            diag_svc: flag("SystemDiagsvc"),
            diag_track: flag("SystemDiagtrack"),
            sysmain: false, // 见字段注释
            windows_update: flag("SystemWindowsUpdate"),
            xbl_auth_manager: flag("SystemXblAuthManager"),
            windows_search: flag("SystemWindwosSearch"),
            homegroup_provider: flag("SystemHomeGroupProvider"),
            homegroup_listener: flag("SystemHomeGroupListener"),
            windows_aero: flag("SystemWindowsAero"),
            power_on_off: flag("SystemPowerOnOff"),
            nvidia_gpus: flag("SystemNVIDIAGPUs"),
            game_priority: flag("SystemGameProPriority"),
            kill_background: flag("SystemKillBackPro"),
            ai_res_geli: flag("SystemAIResGeLi"),
            ai_memory: flag("SystemAIMemory"),
            ai_infra_res: flag("SystemAIInfrastructureRes"),

            game_close_power: int("SystemGameClosePower", 0),
            gameing_power: int("SystemGameingPower", 0),
            nvidia_sets: int("SystemNVIDIASets", 0),
            lock_fps_value: int("SystemLockFPSValue", 0),
            amd_ccd0: int("SystemAMDOnlyMoveGameToCCD0", 0),

            ini_path: ini_path.to_string(),
        }
    }

    /// 与 UI 上 9 个勾选项直接对应的开关（含电源方案）。
    pub fn ui_items(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("调整会影响游戏玩进程优先级", self.game_priority),
            ("关闭会影响游戏流畅性的后台进程", self.kill_background),
            ("电源方案改为游戏加加方案（高性能）", self.ai_power),
            ("释放电脑内存", self.ai_memory),
            ("关闭 Wallpaper Engine", self.wallpaper_engine),
            ("停止 Xbox Live 身份验证管理器服务", self.xbl_auth_manager),
            ("停止 Windows Search 服务", self.windows_search),
            ("清理 DNS", self.flush_dns),
            ("清理粘贴板内容", self.clean_clipboard),
        ]
    }
}

/// 默认 ini 路径：%LOCALAPPDATA%\GamePP\GamePPOpt.ini
pub fn default_ini_path() -> String {
    match std::env::var("LOCALAPPDATA") {
        Ok(p) => format!("{}\\GamePP\\GamePPOpt.ini", p),
        Err(_) => "GamePPOpt.ini".to_string(),
    }
}

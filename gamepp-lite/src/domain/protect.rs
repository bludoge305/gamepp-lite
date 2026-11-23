//! 受保护白名单（系统组件保护区）。
//!
//! 目的：把"哪些东西绝对不能碰"从各个执行环节里抽出来，做成**唯一**的判定入口。
//! 扫描（识别）、优化（杀进程 / 改优先级 / 释放工作集 / 停服务）、清理（还原时复位优先级）
//! 全部先过 `check` / `service_protected`，命中即跳过，并且在日志里写明跳过了什么、为什么。
//!
//! 为什么需要这一层：
//! - 原版 `GamePPOptimize.node` 的"特征表命中即处理"是无差别匹配，表里有大量通用短子串，
//!   实测会命中 `svchost.exe` / `explorer.exe` 这类系统进程；
//! - 「释放内存」和「还原优先级」是**全进程**遍历，天然会扫到系统组件；
//! - 「关闭 Wallpaper Engine」按子串 "wallpaper" 匹配，属于模糊匹配，更需要兜底。
//!
//! 四条规则（任一命中即受保护）：
//!   1. PID 守卫：PID 0（Idle）/ 4（System）永远受保护；
//!   2. 映像名白名单：`PROCESS_GROUPS`，覆盖内核与会话、系统管理工具、外壳与输入、
//!      安全与防护、安装与更新、虚拟化，以及本程序自身；
//!   3. 目录白名单：可执行文件位于 `%SystemRoot%`、`%ProgramFiles%\WindowsApps`、
//!      Windows Defender、IE、Windows 附件等系统目录下（`path_roots`）；
//!   4. 服务白名单：`SERVICE_GROUPS`，停这些服务会直接破坏系统稳定性。
//!
//! 匹配口径：映像名与服务的**大小写不敏感精确匹配**（去掉 `.exe` 后缀再比），
//! 不做子串匹配 —— 子串会误伤（`game.exe` 之类）；目录走**路径前缀**匹配，
//! 且要求前缀后紧跟分隔符，避免 `C:\Windows10Games` 被当成 `C:\Windows`。

/// 一组同类的受保护对象。
pub struct Group {
    pub label: &'static str,
    pub items: &'static [&'static str],
}

/// 受保护的映像名（白名单）。命名以 Windows 实际进程名为准，大小写不敏感。
pub const PROCESS_GROUPS: &[Group] = &[
    Group {
        label: "内核与会话",
        items: &[
            "system",
            "registry",
            "idle",
            "memcompression",
            "secure system",
            "smss.exe",
            "csrss.exe",
            "wininit.exe",
            "winlogon.exe",
            "services.exe",
            "lsass.exe",
            "svchost.exe",
            "fontdrvhost.exe",
            "dwm.exe",
            "logonui.exe",
            "userinit.exe",
            "audiodg.exe",
            "spoolsv.exe",
            "printisolationhost.exe",
            "wudfhost.exe",
            "dllhost.exe",
            "wmiprvse.exe",
            "taskhostw.exe",
            "conhost.exe",
            "openconsole.exe",
            "werfault.exe",
            "wermgr.exe",
            "consent.exe",
            "sppsvc.exe",
            "lsm.exe",
            "winlogon",
            // WMI / 索引 / 卷影 / 事务 / COM+ 宿主：都是 SYSTEM 身份运行的组件进程，
            // 非提权时读不到它们的映像路径，只能靠名字兜住。
            "unsecapp.exe",
            "searchindexer.exe",
            "searchprotocolhost.exe",
            "searchfilterhost.exe",
            "msdtc.exe",
            "vssvc.exe",
            "dllhst3g.exe",
            "wlanext.exe",
        ],
    },
    Group {
        label: "系统管理工具",
        items: &[
            "taskmgr.exe",       // 任务管理器
            "mmc.exe",           // 管理控制台：服务/设备/磁盘/组策略/任务计划管理器宿主
            "regedit.exe",       // 注册表编辑器
            "msconfig.exe",      // 系统配置
            "resmon.exe",        // 资源监视器
            "perfmon.exe",       // 性能监视器
            "eventvwr.exe",      // 事件查看器
            "control.exe",       // 控制面板
            "systemsettings.exe",// 设置
            "systemsettingsbroker.exe",
            "cleanmgr.exe",      // 磁盘清理
            "dfrgui.exe",        // 驱动器优化
            "sdclt.exe",         // 备份与还原
            "recdisc.exe",       // 恢复驱动器
            "msdt.exe",          // 疑难解答
            "wscui.cpl",
            "windowssecurity.exe",
        ],
    },
    Group {
        label: "外壳与输入",
        items: &[
            "explorer.exe", // 资源管理器（桌面/任务栏宿主）
            "sihost.exe",
            "shellexperiencehost.exe",
            "startmenuexperiencehost.exe",
            "searchhost.exe",
            "searchapp.exe",
            "searchui.exe",
            "textinputhost.exe",
            "ctfmon.exe",
            "chsime.exe",
            "tabtip.exe",
            "lockapp.exe",
            "applicationframehost.exe",
            "runtimebroker.exe",
            "backgroundtaskhost.exe",
            "useroobebroker.exe",
            "useroobe.exe",
            "windowsinternal.composableshell.experiences.textinput.inputapp.exe",
            "shellhost.exe",
        ],
    },
    Group {
        label: "安全与防护",
        items: &[
            "msmpeng.exe",            // Microsoft Defender 反恶意软件服务
            "nissrv.exe",             // Defender 网络检查
            "mpcmdrun.exe",           // Defender 命令行
            "mpsigstub.exe",          // 病毒库更新
            "securityhealthservice.exe", // Windows 安全中心
            "securityhealthsystray.exe",
            "sgrmbroker.exe",         // 系统保护运行时监视
            "smartscreen.exe",        // SmartScreen
            "mpdefendercoreservice.exe",
            "antimalware service executable",
        ],
    },
    Group {
        label: "安装与更新",
        items: &[
            "trustedinstaller.exe",
            "tiworker.exe",
            "wusa.exe",       // Windows 更新独立安装程序
            "dismhost.exe",   // DISM 宿主
            "msiexec.exe",    // Windows Installer
            "usoclient.exe",
            "usocoreworker.exe",
            "mousocoreworker.exe",
            "drvinst.exe",
            "devicecensus.exe",
            "compattelrunner.exe",
        ],
    },
    Group {
        label: "虚拟化与容器",
        items: &[
            "vmmem",
            "vmmemwsl",
            "vmcompute.exe",
            "vmms.exe",
            "vmwp.exe",
            "wslhost.exe",
            "wslservice.exe",
        ],
    },
    Group {
        label: "本程序",
        items: &[
            "gamepp-lite.exe",
            "gamepp-winopt.exe",
            "gamepp-lite-setup.exe",
            "gamepp-winopt-setup.exe",
        ],
    },
];

/// 受保护的服务（白名单）。停这些服务会直接影响系统稳定 / 安全 / 登录会话。
///
/// 注意：`OptPlan` 里可停的服务（`wuauserv` / `WSearch` / `XblAuthManager` /
/// `DiagTrack` / `Diagsvc` / `HomeGroup*`）**不在**本表内，否则那些选项会静默失效。
pub const SERVICE_GROUPS: &[Group] = &[
    Group {
        label: "核心子系统",
        items: &[
            "RpcSs",
            "RpcEptMapper",
            "DcomLaunch",
            "Winmgmt",
            "EventLog",
            "PlugPlay",
            "Power",
            "ProfSvc",
            "Schedule",
            "SENS",
            "gpsvc",
            "LSM",
            "SamSs",
            "CryptSvc",
            "SystemEventsBroker",
            "BrokerInfrastructure",
            "UserManager",
            "StateRepository",
            "CoreMessagingRegistrar",
            "Themes",
            "SysMain",
            "TimeBrokerSvc",
            "KeyIso",
        ],
    },
    Group {
        label: "网络与设备",
        items: &[
            "BFE",
            "MpsSvc",
            "nsi",
            "Dnscache",
            "Dhcp",
            "NlaSvc",
            "netprofm",
            "LanmanWorkstation",
            "LanmanServer",
            "WlanSvc",
            "ShellHWDetection",
            "DeviceInstall",
            "DeviceAssociationService",
            "AudioSrv",
            "AudioEndpointBuilder",
        ],
    },
    Group {
        label: "安全与更新",
        items: &[
            "WinDefend",
            "WdNisSvc",
            "SecurityHealthService",
            "wscsvc",
            "TrustedInstaller",
        ],
    },
    Group {
        label: "远程与终端",
        items: &["TermService", "UmRdpService", "SessionEnv"],
    },
];

/// 目录白名单里始终生效的固定片段（即使环境变量取不到也能兜住）。
const HARD_PATH_RULES: &[(&str, &str)] = &[
    ("\\windows\\system32\\", "Windows 系统目录"),
    ("\\windows\\syswow64\\", "Windows 系统目录"),
    ("\\windows\\winsxs\\", "Windows 系统目录"),
    ("\\windows\\servicing\\", "Windows 系统目录"),
    ("\\windows\\systemapps\\", "Windows 系统目录"),
    ("\\windows\\systemresources\\", "Windows 系统目录"),
    ("\\windowsapps\\", "Microsoft Store 应用"),
    ("\\windows defender\\", "Windows Defender"),
];

fn norm_name(s: &str) -> String {
    let lower = s.trim().to_ascii_lowercase();
    match lower.strip_suffix(".exe") {
        Some(rest) => rest.to_string(),
        None => lower,
    }
}

fn norm_path(p: &str) -> String {
    let mut s = p.trim().trim_matches('"').replace('/', "\\").to_ascii_lowercase();
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// `path` 是否落在 `root` 之下（前缀匹配且要求分隔符边界）。
fn under(path: &str, root: &str) -> bool {
    if root.is_empty() || path.is_empty() {
        return false;
    }
    if path == root {
        return true;
    }
    path.as_bytes().starts_with(root.as_bytes())
        && path.as_bytes().get(root.len()) == Some(&b'\\')
}

/// 目录白名单的根（路径按本机环境变量解析，结果做进程内缓存）。
pub fn path_roots() -> &'static [(String, &'static str)] {
    use std::sync::OnceLock;
    static ROOTS: OnceLock<Vec<(String, &'static str)>> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let sysroot = env("SystemRoot").unwrap_or_else(|| "C:\\Windows".to_string());
        let pf = env("ProgramW6432").or_else(|| env("ProgramFiles"));
        let pf86 = env("ProgramFiles(x86)");
        let pd = env("ProgramData").unwrap_or_else(|| "C:\\ProgramData".to_string());

        let mut roots: Vec<(String, &'static str)> = vec![
            (sysroot.clone(), "Windows 系统目录"),
            (format!("{pd}\\Microsoft\\Windows Defender"), "Windows Defender"),
        ];
        for base in [pf, pf86].into_iter().flatten() {
            roots.push((format!("{base}\\WindowsApps"), "Microsoft Store 应用"));
            roots.push((format!("{base}\\Windows Defender"), "Windows Defender"));
            roots.push((format!("{base}\\Windows NT"), "Windows 附件"));
            roots.push((format!("{base}\\Internet Explorer"), "Internet Explorer"));
            roots.push((
                format!("{base}\\Common Files\\Microsoft Shared"),
                "Microsoft 共享组件",
            ));
        }
        roots
    })
}

fn path_reason_with(path: &str, roots: &[(String, &'static str)]) -> Option<&'static str> {
    if path.trim().is_empty() {
        return None;
    }
    let p = norm_path(path);
    for (root, label) in roots {
        if under(&p, &norm_path(root)) {
            return Some(label);
        }
    }
    HARD_PATH_RULES
        .iter()
        .find(|(needle, _)| p.contains(needle))
        .map(|(_, label)| *label)
}

/// 映像名命中的受保护分组。
pub fn name_group(exe: &str) -> Option<&'static Group> {
    let n = norm_name(exe);
    if n.is_empty() {
        return None;
    }
    PROCESS_GROUPS
        .iter()
        .find(|g| g.items.iter().any(|i| norm_name(i) == n))
}

/// 路径命中的受保护目录类别。
pub fn path_reason(path: &str) -> Option<&'static str> {
    path_reason_with(path, path_roots())
}

/// 综合判定：返回"为什么受保护"的人类可读标签，`None` 表示可安全操作。
pub fn check(pid: u32, exe: &str, path: &str) -> Option<&'static str> {
    if pid <= 4 {
        // 0 = System Idle Process，4 = System；两者不属于任何普通进程。
        return Some("内核进程（PID 0/4）");
    }
    if let Some(g) = name_group(exe) {
        return Some(g.label);
    }
    path_reason(path)
}

/// 服务是否受保护。
pub fn service_protected(name: &str) -> bool {
    let n = norm_name(name);
    SERVICE_GROUPS
        .iter()
        .any(|g| g.items.iter().any(|i| norm_name(i) == n))
}

/// 白名单规模（设置页展示用）：(映像名条数, 服务条数, 目录规则条数)。
pub fn counts() -> (usize, usize, usize) {
    let names = PROCESS_GROUPS.iter().map(|g| g.items.len()).sum();
    let services = SERVICE_GROUPS.iter().map(|g| g.items.len()).sum();
    (names, services, path_roots().len() + HARD_PATH_RULES.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matching_is_exact_and_case_insensitive() {
        assert!(check(1234, "SVCHOST.EXE", "").is_some());
        assert!(check(1234, "svchost", "").is_some());
        assert!(check(1234, "explorer.exe", "").is_some());
        assert!(check(1234, "taskmgr.exe", "").is_some());
        assert!(check(1234, "System", "").is_some());
        // 精确匹配：相似名不算（子串匹配会误伤）。
        assert!(check(1234, "svchostx.exe", "").is_none());
        assert!(check(1234, "MyExplorer.exe", "").is_none());
        // 普通游戏/工具不受影响。
        assert!(check(1234, "cs2.exe", "").is_none());
        assert!(check(1234, "wallpaper64.exe", "").is_none());
    }

    #[test]
    fn pid_guard_covers_kernel_pids() {
        assert_eq!(check(0, "whatever.exe", ""), Some("内核进程（PID 0/4）"));
        assert_eq!(check(4, "whatever.exe", ""), Some("内核进程（PID 0/4）"));
    }

    #[test]
    fn path_prefix_needs_separator_boundary() {
        let roots = vec![("c:\\windows".to_string(), "Windows 系统目录")];
        assert_eq!(
            path_reason_with("C:\\Windows\\System32\\foo.exe", &roots),
            Some("Windows 系统目录")
        );
        assert_eq!(
            path_reason_with("C:/Windows/System32/foo.exe", &roots),
            Some("Windows 系统目录")
        );
        // 只是前缀相同、并不在目录内：不能误判。
        assert_eq!(path_reason_with("C:\\Windows10Games\\foo.exe", &roots), None);
        assert_eq!(path_reason_with("D:\\Games\\foo.exe", &roots), None);
        assert_eq!(path_reason_with("", &roots), None);
    }

    #[test]
    fn system_dir_catches_processes_not_in_name_list() {
        // 名字不在白名单，但路径在 System32 → 仍受保护。
        assert!(check(999, "someinbox.dll.exe", "C:\\Windows\\System32\\someinbox.dll.exe").is_some());
        // 游戏目录不受影响。
        assert!(check(999, "cs2.exe", "D:\\Steam\\steamapps\\common\\Counter-Strike Global Offensive\\game\\bin\\win64\\cs2.exe").is_none());
    }

    #[test]
    fn services_whitelist_keeps_opt_plan_services_usable() {
        for critical in ["RpcSs", "Winmgmt", "EventLog", "DcomLaunch", "Power", "BFE"] {
            assert!(service_protected(critical), "{critical} 应受保护");
        }
        // 优化清单里的服务必须仍然可停，否则选项会静默失效。
        for planned in [
            "wuauserv",
            "WSearch",
            "XblAuthManager",
            "DiagTrack",
            "Diagsvc",
            "HomeGroupListener",
            "HomeGroupProvider",
        ] {
            assert!(!service_protected(planned), "{planned} 不应受保护");
        }
    }

    #[test]
    fn counts_are_non_trivial() {
        let (names, services, paths) = counts();
        assert!(names >= 80, "映像名白名单过少：{names}");
        assert!(services >= 30, "服务白名单过少：{services}");
        assert!(paths >= 5, "目录规则过少：{paths}");
    }
}

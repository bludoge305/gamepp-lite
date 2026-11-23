//! 进程枚举、名字匹配与终止。
//!
//! 对应原始实现：
//!   sub_10016720  `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` 建两份链表：全系统、当前会话
//!                 （用 `ProcessIdToSessionId` 过滤）。
//!   sub_10016b20  进程名与黑名单**逐条小写后做子串**查找（所以 "wallpaper" 能命中
//!                 wallpaper32.exe / wallpaper64.exe，不是文件名全等）。
//!   sub_10013f70  命中项 `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess(0)`。

use super::ffi;
use crate::domain::protect;

/// 游戏/启动器进程名特征表（371 条）。
///
/// 证据：`GamePPOptimize.node` 的 .data 0x10056fa8 起是 372 条 `std::wstring`，
/// 由 `sub_10001c00` 用 .rdata 0x1004e48e 起始的字面量逐条构造；下面是该段的转储
/// （缺 1 条，见逆向报告 U1）。其中 3 条与别的条目重复（Squirrel/squirrel、
/// BugReport/bugreport、VALORANT ×2），原表如此，不影响子串匹配的结果。
pub const GAME_PATTERNS: &str = include_str!("../../opt-data/game_patterns.txt");

/// 后台进程黑名单（SystemKillBackPro）：.data 0x10056af8 起 48 条。
pub const KILL_LIST: &str = include_str!("../../opt-data/kill_list.txt");

#[derive(Debug, Clone)]
pub struct ProcEntry {
    pub pid: u32,
    pub name: String,
}

fn parse_list(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_lowercase())
        .collect()
}

pub fn game_patterns() -> Vec<String> {
    parse_list(GAME_PATTERNS)
}

pub fn kill_list() -> Vec<String> {
    parse_list(KILL_LIST)
}

/// 枚举进程；`only_current_session` 对应原版的会话过滤。
pub fn list_processes(only_current_session: bool) -> Vec<ProcEntry> {
    let mut out = Vec::new();
    let my_session = current_session_id();
    unsafe {
        let snap = ffi::CreateToolhelp32Snapshot(ffi::TH32CS_SNAPPROCESS, 0);
        if snap == ffi::INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry = ffi::PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of::<ffi::PROCESSENTRY32W>() as u32;
        if ffi::Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let pid = entry.th32ProcessID;
                let name = ffi::from_wide(&entry.szExeFile);
                let keep = if only_current_session {
                    pid != 0 && session_of(pid) == my_session
                } else {
                    pid != 0
                };
                if keep {
                    out.push(ProcEntry { pid, name });
                }
                if ffi::Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        ffi::CloseHandle(snap);
    }
    out
}

fn current_session_id() -> u32 {
    unsafe {
        let mut s: u32 = 0;
        if ffi::ProcessIdToSessionId(ffi::GetCurrentProcessId(), &mut s) != 0 {
            s
        } else {
            u32::MAX
        }
    }
}

fn session_of(pid: u32) -> u32 {
    unsafe {
        let mut s: u32 = 0;
        if ffi::ProcessIdToSessionId(pid, &mut s) != 0 {
            s
        } else {
            u32::MAX
        }
    }
}

pub fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return false;
    }
    haystack.to_lowercase().contains(needle_lower)
}

/// 进程名命中任意特征则返回该特征。
pub fn match_any(name: &str, patterns: &[String]) -> Option<String> {
    let lower = name.to_lowercase();
    patterns.iter().find(|p| lower.contains(p.as_str())).cloned()
}

pub fn terminate(pid: u32) -> bool {
    unsafe {
        let h = ffi::OpenProcess(ffi::PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = ffi::TerminateProcess(h, 0);
        ffi::CloseHandle(h);
        ok != 0
    }
}

/// 用已有句柄取完整映像路径（供"已经开了句柄"的调用点复用，避免二次 OpenProcess）。
pub fn exe_path_of_handle(h: ffi::HANDLE) -> Option<String> {
    if h.is_null() {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut size = buf.len() as u32;
    let ok = unsafe { ffi::QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size) };
    if ok == 0 {
        None
    } else {
        Some(ffi::from_wide(&buf[..size as usize]))
    }
}

/// 进程完整映像路径（终止前先取，供还原时重新启动；也用于预演统计）。
pub fn exe_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let h = ffi::OpenProcess(ffi::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let path = exe_path_of_handle(h);
        ffi::CloseHandle(h);
        path
    }
}

/// 一次批量终止的结果。
///
/// `skipped` 记录被受保护白名单挡下的进程（名字 + 原因），调用方会把它写进日志——
/// 「跳过了什么、为什么跳过」必须对用户可见，否则用户会以为动作真的生效了。
pub struct KillOutcome {
    /// (进程名, pid, 完整路径)；路径取不到时为空串。
    pub killed: Vec<(String, u32, String)>,
    /// (进程名, 受保护原因)。
    pub skipped: Vec<(String, &'static str)>,
}

/// 受保护判定（PID 守卫 + 映像名 + 完整路径），供优化流程各动作统一调用。
///
/// 路径要通过 `OpenProcess` 取一次；取不到（系统/受保护进程常见）时退化为
/// 按名字判定，而系统核心进程的名字都在白名单里，所以不会漏。
pub fn protected_reason(pid: u32, name: &str) -> Option<&'static str> {
    let self_pid = unsafe { ffi::GetCurrentProcessId() };
    if pid == self_pid {
        return Some("本程序自身");
    }
    let path = exe_path(pid).unwrap_or_default();
    protect::check(pid, name, &path)
}

/// 按特征表终止进程，返回 (进程名, pid, 完整路径)。
///
/// 「关闭 Wallpaper Engine」用这一版：终止前先把 exe 路径记下来，
/// 还原时才能重新启动（记录 pid 没有意义，届时多半已被系统复用）。
/// 取不到路径的进程照杀但不记录 —— 与原版行为一致。
///
/// 白名单内的进程**不终止、也不记录**，只在 `skipped` 里留一条原因。
pub fn kill_matching_recording(
    patterns: &[String],
    only_current_session: bool,
) -> KillOutcome {
    let mut out = KillOutcome {
        killed: Vec::new(),
        skipped: Vec::new(),
    };
    for p in list_processes(only_current_session) {
        if match_any(&p.name, patterns).is_none() {
            continue;
        }
        if let Some(reason) = protected_reason(p.pid, &p.name) {
            out.skipped.push((p.name, reason));
            continue;
        }
        let path = exe_path(p.pid);
        if terminate(p.pid) {
            out.killed.push((p.name, p.pid, path.unwrap_or_default()));
        }
    }
    out
}

/// 某个映像名（如 `wallpaper64.exe`）当前是否在运行。
pub fn is_running(exe_name: &str) -> bool {
    let lower = exe_name.to_lowercase();
    list_processes(false)
        .iter()
        .any(|p| p.name.to_lowercase() == lower)
}

/// 以默认方式启动一个 exe（不等待、不提权）。用于还原被结束的 Wallpaper Engine。
pub fn launch_detached(path: &str) -> bool {
    unsafe {
        let verb = ffi::wide("open");
        let file = ffi::wide(path);
        // 返回值 > 32 表示成功（SE_ERR 常规约定）。
        ffi::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            ffi::SW_SHOWNORMAL,
        ) > 32
    }
}

/// 按特征表终止进程。
///
/// 与 `kill_matching_recording` 同一套白名单过滤，只是调用方不需要路径
/// （后台黑名单杀完即止，还原不重建它们）。
pub fn kill_matching(patterns: &[String], only_current_session: bool) -> KillOutcome {
    let mut out = KillOutcome {
        killed: Vec::new(),
        skipped: Vec::new(),
    };
    for p in list_processes(only_current_session) {
        if match_any(&p.name, patterns).is_none() {
            continue;
        }
        if let Some(reason) = protected_reason(p.pid, &p.name) {
            out.skipped.push((p.name, reason));
            continue;
        }
        if terminate(p.pid) {
            out.killed.push((p.name, p.pid, String::new()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PID 0/4 不经过任何名字/路径判断，直接受保护。
    #[test]
    fn kernel_pids_are_guarded() {
        assert_eq!(protected_reason(4, "System"), Some("内核进程（PID 0/4）"));
        assert_eq!(protected_reason(0, "whatever.exe"), Some("内核进程（PID 0/4）"));
    }

    /// 在本机真实进程表上验证：外壳/服务宿主一定被挡下（名字或路径命中）。
    /// 这两类进程正是原版"名字命中即提优先级 / 全进程释放内存"最容易误伤的。
    #[test]
    fn live_system_components_are_guarded() {
        let procs = list_processes(false);
        for want in ["explorer.exe", "svchost.exe", "dwm.exe"] {
            if let Some(p) = procs.iter().find(|p| p.name.eq_ignore_ascii_case(want)) {
                assert!(
                    protected_reason(p.pid, &p.name).is_some(),
                    "{want} 应命中受保护白名单"
                );
            }
        }
    }

    /// 反例：普通游戏进程名不应被当成受保护组件（白名单不能误伤目标）。
    #[test]
    fn game_names_are_not_guarded() {
        for name in ["cs2.exe", "DeadCells.exe", "GenshinImpact.exe", "wallpaper64.exe"] {
            assert!(
                protected_reason(0xFFFF_FF00, name).is_none(),
                "{name} 不应受保护"
            );
        }
    }
}

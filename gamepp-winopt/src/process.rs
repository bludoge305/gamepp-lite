//! 进程枚举、名字匹配与终止。
//!
//! 对应原始实现：
//!   sub_10016720  用 CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS) 建立两份链表：
//!                 全系统进程，以及用 ProcessIdToSessionId 过滤出的“当前会话”进程。
//!   sub_10016b20  把进程名与黑名单逐条转小写后做**子串**查找。
//!   sub_10013f70  里对命中项 OpenProcess(PROCESS_TERMINATE, 0, pid) + TerminateProcess(0)。
//!
//! 注意：所有匹配都是 case-insensitive 的子串匹配（原始实现用 towlower 逐字符比较），
//! 不是文件名全等 —— 例如 "wallpaper" 能命中 wallpaper32.exe / wallpaper64.exe。

use crate::ffi;

/// 内置的游戏/启动器进程名特征表。
///
/// 证据：GamePPOptimize.node 的 .data 0x10056fa8 起是 std::wstring 数组（每条 0x18 字节，
/// 共 372 条），由 sub_10001c00 用 .rdata 里 0x1004e48e 起始的那段字面量逐条构造；
/// 下面的清单即该段字面量的地址序转储（371 条，缺 1 条见报告 U1）。
/// 其中 3 条与别的条目重复（Squirrel/squirrel、BugReport/bugreport、VALORANT ×2），
/// 原表如此，不影响子串匹配的结果。
pub const GAME_PATTERNS: &str = include_str!("../data/game_patterns.txt");

/// 后台进程黑名单（SystemKillBackPro）。
///
/// 证据：.data 0x10056af8 起 48 条 std::wstring，由 sub_100010a0 初始化。
pub const KILL_LIST: &str = include_str!("../data/kill_list.txt");

/// 当前会话里的一个进程。
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

/// 枚举进程；only_current_session = true 时只保留与自身同会话的进程
/// （对应 sub_10016720 里的 ProcessIdToSessionId 过滤）。
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
                    if pid == 0 {
                        false
                    } else {
                        session_of(pid) == my_session
                    }
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

/// 小写子串匹配（等价于 sub_1000b520 的“否含”判定）。
pub fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return false;
    }
    haystack.to_lowercase().contains(needle_lower)
}

/// 进程名是否命中任意一条特征（任一命中即返回 Some(命中项)）。
pub fn match_any(name: &str, patterns: &[String]) -> Option<String> {
    let lower = name.to_lowercase();
    patterns
        .iter()
        .find(|p| lower.contains(p.as_str()))
        .cloned()
}

/// 终止进程：与原实现一致，只取 PROCESS_TERMINATE 权限，退出码 0。
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

/// 按特征表终止进程，返回 (进程名, pid) 列表。调用方负责先决定是否刷新列表。
pub fn kill_matching(patterns: &[String], only_current_session: bool) -> Vec<(String, u32)> {
    let procs = list_processes(only_current_session);
    let mut killed = Vec::new();
    for p in procs {
        if match_any(&p.name, patterns).is_some() && terminate(p.pid) {
            killed.push((p.name, p.pid));
        }
    }
    killed
}

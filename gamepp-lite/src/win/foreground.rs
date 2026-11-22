//! 前台窗口 → 进程映射。
//!
//! 逆向对照：`GPPPlatform.exe` 的日志格式是
//! `GetForegroundWindow hWnd = {} processId = {} process = {}`，
//! 说明它的识别入口就是这三个 API 的组合；`CProcessNotifyWrapper` 另外用
//! `SetWinEventHook` 做事件驱动补充。本工程先用轮询版（更可控、可测）。
//!
//! 这里的"采样"按识别节拍（默认 1 秒）发生一次，UI 只读快照 —— 不要把
//! `current()` 塞进每帧绘制路径：它每次都要 OpenProcess + 枚举模块。

use super::ffi::*;
use super::process;
use std::sync::Mutex;

/// 模块枚举不便宜（一次 OpenProcess + 最多 512 次 GetModuleBaseNameW），
/// 而同一个前台进程的模块列表在一次会话里基本不变：按 (pid, exe) 缓存。
static RUNTIME_CACHE: Mutex<Option<(u32, String, Vec<String>)>> = Mutex::new(None);

#[derive(Clone, Debug, Default)]
pub struct Foreground {
    pub hwnd: usize,
    pub pid: u32,
    pub exe: String,
    pub path: String,
    pub title: String,
    pub class: String,
    /// 窗口是否铺满主显示器（全屏/无边框全屏的廉价近似）。
    pub fullscreen: bool,
    /// 当前优先级类别（`GetPriorityClass`），打不开进程时为 None。
    pub priority: Option<u32>,
    /// 已加载的图形运行时（d3d9/11/12、vulkan-1），用于启发式判定与界面展示。
    pub runtimes: Vec<String>,
}

/// 读一次当前前台窗口（含进程优先级与图形运行时）。
pub fn current() -> Option<Foreground> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }

        let mut title = [0u16; 512];
        let tn = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let mut class = [0u16; 256];
        let cn = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);

        let mut rect = RECT::default();
        let has_rect = GetWindowRect(hwnd, &mut rect) != 0;
        let sw = GetSystemMetrics(0);
        let sh = GetSystemMetrics(1);
        let fullscreen = has_rect
            && rect.left <= 0
            && rect.top <= 0
            && rect.right >= sw
            && rect.bottom >= sh;

        let exe = process::exe_name(pid).unwrap_or_default();
        Some(Foreground {
            hwnd: hwnd as usize,
            pid,
            path: process::exe_path(pid).unwrap_or_default(),
            title: from_wide(&title[..tn.max(0) as usize]),
            class: from_wide(&class[..cn.max(0) as usize]),
            fullscreen,
            // 优先级会随优化动作变化，每次采样都重新读（两个廉价调用）。
            priority: process::priority(pid),
            runtimes: runtimes_cached(pid, &exe),
            exe,
        })
    }
}

/// 取缓存里的图形运行时；同一 (pid, exe) 只在首次采样时枚举一次模块。
fn runtimes_cached(pid: u32, exe: &str) -> Vec<String> {
    let mut cache = match RUNTIME_CACHE.lock() {
        Ok(c) => c,
        // 缓存中毒不影响正确性，退回直接枚举。
        Err(_) => return process::graphics_runtimes(pid),
    };
    if let Some((p, e, v)) = cache.as_ref() {
        if *p == pid && e == exe {
            return v.clone();
        }
    }
    let v = process::graphics_runtimes(pid);
    *cache = Some((pid, exe.to_string(), v.clone()));
    v
}

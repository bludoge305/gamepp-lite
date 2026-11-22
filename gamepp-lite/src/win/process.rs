//! 进程信息（对应逆向里的 `processhelper` / `WebApiImpl_GetForegroundProcess`、
//! `K32EnumProcessModules`、`SetPriorityClass` 一族）。

use super::ffi::*;

/// 打开一个"够用就好"的句柄：查询 + 设置优先级。
fn open(pid: u32, access: u32) -> Option<HANDLE> {
    if pid == 0 {
        return None;
    }
    let h = unsafe { OpenProcess(access, 0, pid) };
    if h.is_null() { None } else { Some(h) }
}

/// 进程完整映像路径。对应 `K32GetProcessImageFileNameW` / `QueryFullProcessImageNameW`。
pub fn exe_path(pid: u32) -> Option<String> {
    let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut buf = [0u16; MAX_PATH * 2];
    let mut size = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(h) };
    if ok == 0 {
        None
    } else {
        Some(from_wide(&buf[..size as usize]))
    }
}

/// 映像文件名（不含路径），例如 `Client-Win64-Shipping.exe`。
pub fn exe_name(pid: u32) -> Option<String> {
    let path = exe_path(pid)?;
    Some(
        path.rsplit(['\\', '/'])
            .next()
            .unwrap_or(path.as_str())
            .to_string(),
    )
}

pub fn is_alive(pid: u32) -> bool {
    match open(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
        Some(h) => {
            let mut code = 0u32;
            let ok = unsafe { GetExitCodeProcess(h, &mut code) };
            unsafe { CloseHandle(h) };
            // STILL_ACTIVE = 259
            ok != 0 && code == 259
        }
        None => false,
    }
}

/// 当前优先级类别（用于 ① 页展示前台进程的优先级）。
pub fn priority(pid: u32) -> Option<u32> {
    let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let class = unsafe { GetPriorityClass(h) };
    unsafe { CloseHandle(h) };
    if class == 0 {
        None
    } else {
        Some(class)
    }
}

pub fn priority_name(class: u32) -> &'static str {
    match class {
        HIGH_PRIORITY_CLASS => "高",
        ABOVE_NORMAL_PRIORITY_CLASS => "高于正常",
        NORMAL_PRIORITY_CLASS => "正常",
        BELOW_NORMAL_PRIORITY_CLASS => "低于正常",
        0x00000040 => "空闲",
        0x000000100 => "实时",
        _ => "未知",
    }
}

/// 目标进程是否加载了图形运行时。
///
/// 用途：游戏库里没有登记的 exe，若加载了 d3d11/d3d12/vulkan-1，仍可判为"疑似游戏"。
/// 这就是逆向里 `GetGameProcessType` 的启发式部分（本工程只保留"看模块"这一层）。
pub fn graphics_runtimes(pid: u32) -> Vec<String> {
    let mut hits = Vec::new();
    let h = match open(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ) {
        Some(h) => h,
        None => return hits,
    };
    let mut mods = [std::ptr::null_mut::<std::ffi::c_void>(); 512];
    let mut needed = 0u32;
    let ok = unsafe {
        K32EnumProcessModules(
            h,
            mods.as_mut_ptr(),
            (mods.len() * std::mem::size_of::<HMODULE>()) as u32,
            &mut needed,
        )
    };
    if ok != 0 {
        let count = (needed as usize / std::mem::size_of::<HMODULE>()).min(mods.len());
        for m in mods.iter().take(count) {
            let mut buf = [0u16; 128];
            let n = unsafe { K32GetModuleBaseNameW(h, *m, buf.as_mut_ptr(), buf.len() as u32) };
            if n == 0 {
                continue;
            }
            let name = from_wide(&buf[..n as usize]).to_ascii_lowercase();
            for want in ["d3d11.dll", "d3d12.dll", "vulkan-1.dll", "d3d9.dll"] {
                if name == want && !hits.iter().any(|h: &String| h == want) {
                    hits.push(want.to_string());
                }
            }
        }
    }
    unsafe { CloseHandle(h) };
    hits
}

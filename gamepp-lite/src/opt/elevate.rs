//! 管理员权限检测与自动提权。
//!
//! GamePP 的 Windows 优化要停服务、改其他进程的优先级/工作集，**必须**是管理员。
//! 原版由 Electron 主程序以管理员身份启动来保证；独立 exe 则在需要时用
//! `ShellExecuteExW(..., "runas", ...)` 拉起自身副本。
//!
//! 注意这里**不等子进程**：GUI 拉起提权副本后自己就该退出，把窗口交给副本接管。
//! （命令行版 `gamepp-winopt` 反过来要等，因为它要把退出码透传给调用方。）

use super::ffi;

/// 当前进程是否已提权（TokenElevation）。
pub fn is_elevated() -> bool {
    unsafe {
        let mut token: ffi::HANDLE = std::ptr::null_mut();
        if ffi::OpenProcessToken(ffi::GetCurrentProcess(), ffi::TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevated: u32 = 0;
        let mut ret: ffi::DWORD = 0;
        let ok = ffi::GetTokenInformation(
            token,
            ffi::TOKEN_ELEVATION,
            &mut elevated as *mut u32 as *mut std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
            &mut ret,
        );
        ffi::CloseHandle(token);
        ok != 0 && elevated != 0
    }
}

#[derive(Debug)]
pub enum Relaunch {
    /// 提权副本已拉起（本进程应随即退出）
    Launched,
    /// 用户在 UAC 弹窗点了“否”
    Cancelled,
    Failed(String),
}

/// 以管理员身份重新运行自己；不等待子进程。
pub fn relaunch(params: &str) -> Relaunch {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return Relaunch::Failed(format!("取自身路径失败: {}", e)),
    };
    launch("runas", &exe, params)
}

/// 用指定谓词启动一个程序（不等待）。
pub fn launch(verb_str: &str, exe: &std::path::Path, params: &str) -> Relaunch {
    unsafe {
        let verb = ffi::wide(verb_str);
        let file = ffi::wide(&exe.to_string_lossy());
        let args = ffi::wide(params);

        let mut info: ffi::SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<ffi::SHELLEXECUTEINFOW>() as u32;
        info.fMask = ffi::SEE_MASK_NOCLOSEPROCESS | ffi::SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = args.as_ptr();
        info.nShow = ffi::SW_SHOWNORMAL;

        if ffi::ShellExecuteExW(&mut info) == 0 {
            let err = ffi::GetLastError();
            return if err == ffi::ERROR_CANCELLED {
                Relaunch::Cancelled
            } else {
                Relaunch::Failed(format!("ShellExecuteExW 失败，Win32 错误码 {}", err))
            };
        }

        // 只要句柄：不等待，也不关心退出码；句柄必须自己关。
        if !info.hProcess.is_null() {
            ffi::CloseHandle(info.hProcess);
        }
        Relaunch::Launched
    }
}

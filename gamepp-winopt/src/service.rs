//! 服务控制（SystemXblAuthManager / SystemWindwosSearch / SystemDiagtrack / ...）。
//!
//! 对应原始实现：
//!   sub_100141e0  固定顺序遍历 8 个服务名，逐个用配置字节判定是否处理；
//!   sub_10016d40  打开 SCM(0xF003F) → OpenService(0xF01FF) → QueryServiceStatusEx，
//!                 RUNNING 时 ControlService(SERVICE_CONTROL_STOP) 并在标记表里记 "1"；
//!                 STOPPED 且标记为 "1" 且 enable==0（还原）时 StartServiceW。
//!                 —— 也就是说：只有自己停掉的服务才会被自己重新拉起。
//!
//! SysMain 对应的配置字节是 +0x08，而 sub_10013bd0 从不写这个字节，所以恒为 0：
//! 原版只要走 ini 配置就永远不会停 SysMain（这是原版的行为，本实现照样保留）。

use crate::config::Config;
use crate::ffi;

/// 服务名 + 对应的配置开关，顺序与 sub_100141e0 中 8 条字符串的顺序完全一致。
pub fn services(cfg: &Config) -> [(&'static str, bool); 8] {
    [
        ("SysMain", cfg.sysmain),
        ("HomeGroupListener", cfg.homegroup_listener),
        ("HomeGroupProvider", cfg.homegroup_provider),
        ("wuauserv", cfg.windows_update),
        ("WSearch", cfg.windows_search),
        ("XblAuthManager", cfg.xbl_auth_manager),
        ("DiagTrack", cfg.diag_track),
        ("Diagsvc", cfg.diag_svc),
    ]
}

#[derive(Debug, PartialEq, Eq)]
pub enum StopOutcome {
    Stopped,      // 原本在运行，已下发 STOP
    NotRunning,   // 本来就没运行
    Missing,      // 服务不存在 / 打不开
    Failed,
}

/// 尝试停止一个服务（enable = true 的路径）。
pub fn stop_service(name: &str) -> StopOutcome {
    unsafe {
        let scm = ffi::OpenSCManagerW(std::ptr::null(), std::ptr::null(), ffi::SC_MANAGER_ALL_ACCESS);
        if scm.is_null() {
            return StopOutcome::Failed;
        }
        let wname = ffi::wide(name);
        let svc = ffi::OpenServiceW(scm, wname.as_ptr(), ffi::SERVICE_ALL_ACCESS);
        if svc.is_null() {
            ffi::CloseServiceHandle(scm);
            return StopOutcome::Missing;
        }

        let mut needed: u32 = 0;
        let mut status = ffi::SERVICE_STATUS_PROCESS {
            dwServiceType: 0,
            dwCurrentState: 0,
            dwControlsAccepted: 0,
            dwWin32ExitCode: 0,
            dwServiceSpecificExitCode: 0,
            dwCheckPoint: 0,
            dwWaitHint: 0,
            dwProcessId: 0,
            dwServiceFlags: 0,
        };
        let ok = ffi::QueryServiceStatusEx(
            svc,
            ffi::SC_STATUS_PROCESS_INFO,
            &mut status as *mut _ as *mut u8,
            std::mem::size_of::<ffi::SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        );
        if ok == 0 {
            ffi::CloseServiceHandle(svc);
            ffi::CloseServiceHandle(scm);
            return StopOutcome::Failed;
        }

        let outcome = if status.dwCurrentState == ffi::SERVICE_RUNNING {
            let mut st = ffi::SERVICE_STATUS {
                dwServiceType: 0,
                dwCurrentState: 0,
                dwControlsAccepted: 0,
                dwWin32ExitCode: 0,
                dwServiceSpecificExitCode: 0,
                dwCheckPoint: 0,
                dwWaitHint: 0,
            };
            if ffi::ControlService(svc, ffi::SERVICE_CONTROL_STOP, &mut st) != 0 {
                StopOutcome::Stopped
            } else {
                StopOutcome::Failed
            }
        } else if status.dwCurrentState == ffi::SERVICE_STOPPED {
            StopOutcome::NotRunning
        } else {
            // 处于 START_PENDING / STOP_PENDING 等中间态：原实现同样是"什么都不做"
            StopOutcome::NotRunning
        };

        ffi::CloseServiceHandle(svc);
        ffi::CloseServiceHandle(scm);
        outcome
    }
}

/// 还原：只对"当前 STOPPED 且我们自己停过"的服务调用 StartServiceW。
pub fn start_service_if_stopped(name: &str, marked_by_us: bool) -> bool {
    if !marked_by_us {
        return false;
    }
    unsafe {
        let scm = ffi::OpenSCManagerW(std::ptr::null(), std::ptr::null(), ffi::SC_MANAGER_ALL_ACCESS);
        if scm.is_null() {
            return false;
        }
        let wname = ffi::wide(name);
        let svc = ffi::OpenServiceW(scm, wname.as_ptr(), ffi::SERVICE_ALL_ACCESS);
        if svc.is_null() {
            ffi::CloseServiceHandle(scm);
            return false;
        }

        let mut needed: u32 = 0;
        let mut status = ffi::SERVICE_STATUS_PROCESS {
            dwServiceType: 0,
            dwCurrentState: 0,
            dwControlsAccepted: 0,
            dwWin32ExitCode: 0,
            dwServiceSpecificExitCode: 0,
            dwCheckPoint: 0,
            dwWaitHint: 0,
            dwProcessId: 0,
            dwServiceFlags: 0,
        };
        let ok = ffi::QueryServiceStatusEx(
            svc,
            ffi::SC_STATUS_PROCESS_INFO,
            &mut status as *mut _ as *mut u8,
            std::mem::size_of::<ffi::SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        );

        let mut started = false;
        if ok != 0 && status.dwCurrentState == ffi::SERVICE_STOPPED {
            started = ffi::StartServiceW(svc, 0, std::ptr::null()) != 0;
        }

        ffi::CloseServiceHandle(svc);
        ffi::CloseServiceHandle(scm);
        started
    }
}

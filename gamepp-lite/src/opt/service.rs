//! 服务控制。
//!
//! 对应原始实现：
//!   sub_100141e0  固定顺序遍历 8 个服务，按配置逐项处理；
//!   sub_10016d40  OpenSCManager(0xF003F) → OpenService(0xF01FF) → QueryServiceStatusEx，
//!                 RUNNING 时 `ControlService(SERVICE_CONTROL_STOP)` 并打标记；
//!                 还原时只对自己停过（有标记）且当前 STOPPED 的服务 `StartServiceW`。

use super::ffi;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StopOutcome {
    Stopped,
    NotRunning,
    Missing,
    Failed,
}

pub fn stop_service(name: &str) -> StopOutcome {
    unsafe {
        let scm = ffi::OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            ffi::SC_MANAGER_ALL_ACCESS,
        );
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
        let mut status = ffi::SERVICE_STATUS_PROCESS::default();
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
            let mut st: ffi::SERVICE_STATUS = std::mem::zeroed();
            if ffi::ControlService(svc, ffi::SERVICE_CONTROL_STOP, &mut st) != 0 {
                StopOutcome::Stopped
            } else {
                StopOutcome::Failed
            }
        } else {
            // STOPPED 或 START_PENDING / STOP_PENDING：原实现同样是"什么都不做"
            StopOutcome::NotRunning
        };

        ffi::CloseServiceHandle(svc);
        ffi::CloseServiceHandle(scm);
        outcome
    }
}

/// 还原：确保「我们停过」的服务重新运行。
///
/// 返回 `true` 表示服务当前处于运行状态 —— 本次拉起成功，或它本来就在跑
/// （例如用户在游戏期间手动启动了）。只有"该跑却没能跑起来"才算失败。
pub fn ensure_running(name: &str) -> bool {
    unsafe {
        let scm = ffi::OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            ffi::SC_MANAGER_ALL_ACCESS,
        );
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
        let mut status = ffi::SERVICE_STATUS_PROCESS::default();
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
            return false;
        }

        // 已在运行：不算失败（可能是用户自己拉起来的）。
        if status.dwCurrentState == ffi::SERVICE_RUNNING {
            ffi::CloseServiceHandle(svc);
            ffi::CloseServiceHandle(scm);
            return true;
        }
        let started = status.dwCurrentState == ffi::SERVICE_STOPPED
            && ffi::StartServiceW(svc, 0, std::ptr::null()) != 0;

        ffi::CloseServiceHandle(svc);
        ffi::CloseServiceHandle(scm);
        started
    }
}

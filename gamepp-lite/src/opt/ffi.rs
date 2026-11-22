//! Windows 优化模块用到的 Win32 声明。
//!
//! 与 `crate::win::ffi` 有少量同名符号（`OpenProcess` 等），但两者刻意分开：
//! `win::ffi` 服务"识别游戏"，这一份服务"Windows 优化"，各自只声明自己用到的部分，
//! 免得一个模块的改动波及另一个。链接期符号相同，不冲突。

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type BOOL = i32;
pub type DWORD = u32;
pub type LONG = i32;
pub type LPCWSTR = *const u16;
pub type LPCSTR = *const u8;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const WAIT_OBJECT_0: DWORD = 0;

pub const PROCESS_TERMINATE: DWORD = 0x0001;
pub const PROCESS_ALL_ACCESS: DWORD = 0x001F_FFFF;
pub const TH32CS_SNAPPROCESS: DWORD = 0x0000_0002;

pub const IDLE_PRIORITY_CLASS: DWORD = 0x0000_0040;
pub const BELOW_NORMAL_PRIORITY_CLASS: DWORD = 0x0000_4000;
pub const NORMAL_PRIORITY_CLASS: DWORD = 0x0000_0020;
pub const ABOVE_NORMAL_PRIORITY_CLASS: DWORD = 0x0000_8000;
pub const HIGH_PRIORITY_CLASS: DWORD = 0x0000_0080;
pub const REALTIME_PRIORITY_CLASS: DWORD = 0x0000_0100;

pub const SC_MANAGER_ALL_ACCESS: DWORD = 0x000F_003F;
pub const SERVICE_ALL_ACCESS: DWORD = 0x000F_01FF;
pub const SERVICE_CONTROL_STOP: DWORD = 0x0000_0001;
pub const SERVICE_STOPPED: DWORD = 0x0000_0001;
pub const SERVICE_RUNNING: DWORD = 0x0000_0004;
pub const SC_STATUS_PROCESS_INFO: i32 = 0;

pub const TOKEN_QUERY: DWORD = 0x0008;
pub const TOKEN_ELEVATION: i32 = 20;
pub const SW_SHOWNORMAL: i32 = 1;
pub const ERROR_CANCELLED: DWORD = 1223;

#[repr(C)]
pub struct PROCESSENTRY32W {
    pub dwSize: DWORD,
    pub cntUsage: DWORD,
    pub th32ProcessID: DWORD,
    pub th32DefaultHeapID: usize,
    pub th32ModuleID: DWORD,
    pub cntThreads: DWORD,
    pub th32ParentProcessID: DWORD,
    pub pcPriClassBase: LONG,
    pub dwFlags: DWORD,
    pub szExeFile: [u16; 260],
}

impl Default for PROCESSENTRY32W {
    fn default() -> Self {
        // SAFETY: 全零 POD
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
pub struct SERVICE_STATUS {
    pub dwServiceType: DWORD,
    pub dwCurrentState: DWORD,
    pub dwControlsAccepted: DWORD,
    pub dwWin32ExitCode: DWORD,
    pub dwServiceSpecificExitCode: DWORD,
    pub dwCheckPoint: DWORD,
    pub dwWaitHint: DWORD,
}

#[repr(C)]
pub struct SERVICE_STATUS_PROCESS {
    pub dwServiceType: DWORD,
    pub dwCurrentState: DWORD,
    pub dwControlsAccepted: DWORD,
    pub dwWin32ExitCode: DWORD,
    pub dwServiceSpecificExitCode: DWORD,
    pub dwCheckPoint: DWORD,
    pub dwWaitHint: DWORD,
    pub dwProcessId: DWORD,
    pub dwServiceFlags: DWORD,
}

impl Default for SERVICE_STATUS_PROCESS {
    fn default() -> Self {
        // SAFETY: 全零 POD
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl GUID {
    pub fn zeroed() -> Self {
        GUID {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        }
    }
}

/// x64 下 sizeof(SHELLEXECUTEINFOW) = 112；布局错了 ShellExecuteExW 会直接失败。
#[repr(C)]
pub struct SHELLEXECUTEINFOW {
    pub cbSize: u32,
    pub fMask: u32,
    pub hwnd: HANDLE,
    pub lpVerb: LPCWSTR,
    pub lpFile: LPCWSTR,
    pub lpParameters: LPCWSTR,
    pub lpDirectory: LPCWSTR,
    pub nShow: i32,
    pub hInstApp: isize,
    pub lpIDList: *mut c_void,
    pub lpClass: LPCWSTR,
    pub hkeyClass: isize,
    pub dwHotKey: u32,
    pub hIconOrMonitor: HANDLE,
    pub hProcess: HANDLE,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<SHELLEXECUTEINFOW>() == 112);

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn OpenProcess(access: DWORD, inherit: BOOL, pid: DWORD) -> HANDLE;
    pub fn TerminateProcess(h: HANDLE, code: u32) -> BOOL;
    pub fn SetPriorityClass(h: HANDLE, class: DWORD) -> BOOL;
    pub fn GetPriorityClass(h: HANDLE) -> DWORD;
    pub fn SetProcessAffinityMask(h: HANDLE, mask: usize) -> BOOL;
    pub fn SetProcessWorkingSetSize(h: HANDLE, min: usize, max: usize) -> BOOL;
    pub fn K32EmptyWorkingSet(h: HANDLE) -> BOOL;
    pub fn CreateToolhelp32Snapshot(flags: DWORD, pid: DWORD) -> HANDLE;
    pub fn Process32FirstW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    pub fn Process32NextW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    pub fn ProcessIdToSessionId(pid: DWORD, session: *mut DWORD) -> BOOL;
    pub fn GetCurrentProcessId() -> DWORD;
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn WinExec(cmd: LPCSTR, show: u32) -> u32;
    pub fn GetPrivateProfileIntW(
        section: LPCWSTR,
        key: LPCWSTR,
        default: i32,
        file: LPCWSTR,
    ) -> u32;
    pub fn GetActiveProcessorCount(group: u16) -> DWORD;
    pub fn LocalFree(h: HANDLE) -> HANDLE;
    pub fn GetLastError() -> DWORD;
    pub fn LoadLibraryW(name: LPCWSTR) -> HANDLE;
    pub fn GetProcAddress(module: HANDLE, name: LPCSTR) -> *mut c_void;
    pub fn FreeLibrary(module: HANDLE) -> BOOL;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    pub fn OpenSCManagerW(machine: LPCWSTR, database: LPCWSTR, access: DWORD) -> HANDLE;
    pub fn OpenServiceW(scm: HANDLE, name: LPCWSTR, access: DWORD) -> HANDLE;
    pub fn QueryServiceStatusEx(
        svc: HANDLE,
        level: i32,
        buf: *mut u8,
        len: DWORD,
        needed: *mut DWORD,
    ) -> BOOL;
    pub fn ControlService(svc: HANDLE, control: DWORD, status: *mut SERVICE_STATUS) -> BOOL;
    pub fn StartServiceW(svc: HANDLE, argc: DWORD, argv: *const LPCWSTR) -> BOOL;
    pub fn CloseServiceHandle(h: HANDLE) -> BOOL;
    pub fn OpenProcessToken(process: HANDLE, access: DWORD, token: *mut HANDLE) -> BOOL;
    pub fn GetTokenInformation(
        token: HANDLE,
        class: i32,
        info: *mut c_void,
        len: DWORD,
        ret_len: *mut DWORD,
    ) -> BOOL;
}

#[link(name = "user32")]
unsafe extern "system" {
    pub fn OpenClipboard(hwnd_owner: HANDLE) -> BOOL;
    pub fn EmptyClipboard() -> BOOL;
    pub fn CloseClipboard() -> BOOL;
}

#[link(name = "shell32")]
unsafe extern "system" {
    pub fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> BOOL;
}

// ---- 提权相关常量 ------------------------------------------------------------
pub const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;
pub const SEE_MASK_NOASYNC: u32 = 0x0000_0100;

pub fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

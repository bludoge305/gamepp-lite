//! 最小 Win32 FFI 声明。
//!
//! 只声明本模块实际用到的那部分 API —— 与 GamePPOptimize.node 的导入表一致：
//!   KERNEL32: CreateMutexW / WaitForSingleObject / OpenProcess / TerminateProcess /
//!             SetPriorityClass / GetPriorityClass / SetProcessWorkingSetSize /
//!             K32EmptyWorkingSet / CreateToolhelp32Snapshot / Process32FirstW /
//!             Process32NextW / ProcessIdToSessionId / WinExec / Sleep
//!   ADVAPI32: OpenSCManagerW / OpenServiceW / QueryServiceStatusEx / ControlService /
//!             StartServiceW / CloseServiceHandle
//!   USER32:   OpenClipboard / EmptyClipboard / CloseClipboard
//!
//! 电源方案在原始产品里由同目录 Power.dll 完成（它运行时从 POWRPROF.dll 动态
//! 取 PowerImportPowerScheme / PowerSetActiveScheme），这里用同样的动态加载方式。

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type BOOL = i32;
pub type DWORD = u32;
pub type LONG = i32;
pub type LPCWSTR = *const u16;
pub type LPCSTR = *const u8;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const WAIT_TIMEOUT: DWORD = 0x0000_0102;
pub const WAIT_OBJECT_0: DWORD = 0;

// ---- 由反汇编确认的常量 ------------------------------------------------------
pub const PROCESS_TERMINATE: DWORD = 0x0001;
pub const PROCESS_QUERY_INFORMATION: DWORD = 0x0400;
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
        // SAFETY: 全零 POD 结构体
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
        GUID { data1: 0, data2: 0, data3: 0, data4: [0; 8] }
    }
}

#[link(name = "kernel32")]
extern "system" {
    pub fn CreateMutexW(attrs: *const c_void, initial_owner: BOOL, name: LPCWSTR) -> HANDLE;
    pub fn WaitForSingleObject(h: HANDLE, ms: DWORD) -> DWORD;
    pub fn ReleaseMutex(h: HANDLE) -> BOOL;
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn OpenProcess(access: DWORD, inherit: BOOL, pid: DWORD) -> HANDLE;
    pub fn TerminateProcess(h: HANDLE, code: u32) -> BOOL;
    pub fn SetPriorityClass(h: HANDLE, class: DWORD) -> BOOL;
    pub fn GetPriorityClass(h: HANDLE) -> DWORD;
    pub fn SetProcessAffinityMask(h: HANDLE, mask: usize) -> BOOL;
    pub fn GetProcessAffinityMask(h: HANDLE, proc_mask: *mut usize, sys_mask: *mut usize) -> BOOL;
    pub fn SetProcessWorkingSetSize(h: HANDLE, min: usize, max: usize) -> BOOL;
    pub fn K32EmptyWorkingSet(h: HANDLE) -> BOOL;
    pub fn CreateToolhelp32Snapshot(flags: DWORD, pid: DWORD) -> HANDLE;
    pub fn Process32FirstW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    pub fn Process32NextW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    pub fn ProcessIdToSessionId(pid: DWORD, session: *mut DWORD) -> BOOL;
    pub fn GetCurrentProcessId() -> DWORD;
    pub fn Sleep(ms: DWORD);
    pub fn WinExec(cmd: LPCSTR, show: u32) -> u32;
    pub fn GetPrivateProfileIntW(section: LPCWSTR, key: LPCWSTR, default: i32, file: LPCWSTR) -> u32;
    pub fn GetActiveProcessorCount(group: u16) -> DWORD;
    pub fn LocalFree(h: HANDLE) -> HANDLE;
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn GetExitCodeProcess(h: HANDLE, code: *mut DWORD) -> BOOL;
    pub fn GetLastError() -> DWORD;
    pub fn LoadLibraryW(name: LPCWSTR) -> HANDLE;
    pub fn GetProcAddress(module: HANDLE, name: LPCSTR) -> *mut c_void;
    pub fn FreeLibrary(module: HANDLE) -> BOOL;
}

#[link(name = "advapi32")]
extern "system" {
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

    // 提权检测
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
extern "system" {
    pub fn OpenClipboard(hwnd_owner: HANDLE) -> BOOL;
    pub fn EmptyClipboard() -> BOOL;
    pub fn CloseClipboard() -> BOOL;
}

#[link(name = "shell32")]
extern "system" {
    pub fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> BOOL;
}

// ---- 提权相关 ----------------------------------------------------------------

pub const TOKEN_QUERY: DWORD = 0x0008;
pub const TOKEN_ELEVATION: i32 = 20;
pub const INFINITE: DWORD = 0xFFFF_FFFF;
pub const SW_SHOWNORMAL: i32 = 1;
pub const ERROR_CANCELLED: DWORD = 1223;

pub const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;
pub const SEE_MASK_NOASYNC: u32 = 0x0000_0100;

/// x64 下 sizeof(SHELLEXECUTEINFOW) = 112。
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

// 布局自检：x64 下这个结构体必须是 112 字节，否则 ShellExecuteExW 会直接失败
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<SHELLEXECUTEINFOW>() == 112);

/// 把 Rust 字符串转成以 NUL 结尾的 UTF-16 缓冲区。
pub fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

/// 把 C 侧宽字符串（可能有 260 个 wchar 的定长缓冲）转成 String，遇到 NUL 停止。
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

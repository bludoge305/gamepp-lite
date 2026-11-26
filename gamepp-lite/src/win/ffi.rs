#![allow(clippy::upper_case_acronyms)] // Win32 类型名按官方写法保留（HANDLE / DWORD..）
//! 平台 FFI 层：只声明本项目用到的 Win32 子集。
//!
//! 为什么手写而不是引第三方 crate：本工程要求"工程精简"，且逆向结果显示
//! GamePP 自身就是直接调 Win32（`GetForegroundWindow` / `K32GetProcessImageFileNameW`
//! / `CreateToolhelp32Snapshot` / `CreateFileMappingW`），没有中间框架。
//! 这里保持同样的"薄"。

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

pub type HWND = *mut c_void;
pub type HANDLE = *mut c_void;
pub type HMODULE = *mut c_void;
pub type HICON = *mut c_void;
pub type HMENU = *mut c_void;
pub type HKEY = *mut c_void;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type DWORD = u32;
pub type BOOL = i32;
pub type UINT = u32;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;
pub type LPCWSTR = *const u16;
pub type LPWSTR = *mut u16;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

// ---- 窗口消息 ----
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_APP: u32 = 0x8000;

// ---- 托盘 ----
pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIF_MESSAGE: u32 = 0x01;
pub const NIF_ICON: u32 = 0x02;
pub const NIF_TIP: u32 = 0x04;
pub const NIF_INFO: u32 = 0x10;
pub const NIIF_INFO: u32 = 0x01;

// ---- 菜单 ----
pub const MF_STRING: u32 = 0x0000;
pub const MF_SEPARATOR: u32 = 0x0800;
pub const MF_CHECKED: u32 = 0x0008;
pub const TPM_RETURNCMD: u32 = 0x0100;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;
pub const MB_OK: u32 = 0x0000;
pub const MB_ICONINFORMATION: u32 = 0x0040;
pub const MB_TOPMOST: u32 = 0x00040000;

// ---- 进程 ----
pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const PROCESS_TERMINATE: u32 = 0x0001;
pub const PROCESS_VM_READ: u32 = 0x0010;
pub const MAX_PATH: usize = 260;

pub const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x00008000;
pub const NORMAL_PRIORITY_CLASS: u32 = 0x00000020;
pub const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
pub const HIGH_PRIORITY_CLASS: u32 = 0x00000080;

pub const ERROR_ALREADY_EXISTS: u32 = 183;
pub const EVENT_MODIFY_STATE: u32 = 0x0002;
pub const WAIT_OBJECT_0: u32 = 0;
pub const KEY_WRITE: u32 = 0x20006;
/// 只要写值权限：建键时用它代替 KEY_WRITE，权限面更小。
pub const KEY_SET_VALUE: u32 = 0x0002;
pub const REG_SZ: u32 = 1;
pub const REG_BINARY: u32 = 3;
pub const RRF_RT_REG_SZ: u32 = 0x0002;
pub const RRF_RT_REG_EXPAND_SZ: u32 = 0x0004;
pub const RRF_RT_REG_BINARY: u32 = 0x0008;
pub const HKEY_CURRENT_USER: HKEY = 0x80000001u64 as HKEY;
pub const HKEY_LOCAL_MACHINE: HKEY = 0x80000002u64 as HKEY;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SYSTEMTIME {
    pub wYear: u16,
    pub wMonth: u16,
    pub wDayOfWeek: u16,
    pub wDay: u16,
    pub wHour: u16,
    pub wMinute: u16,
    pub wSecond: u16,
    pub wMilliseconds: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
    pub lPrivate: u32,
}

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HMODULE,
    pub hIcon: HICON,
    pub hCursor: HANDLE,
    pub hbrBackground: HANDLE,
    pub lpszMenuName: LPCWSTR,
    pub lpszClassName: LPCWSTR,
    pub hIconSm: HICON,
}

/// NOTIFYICONDATAW 的 v3 布局（szTip 128 / szInfo 256 / szInfoTitle 64）。
#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: u32,
    pub hWnd: HWND,
    pub uID: u32,
    pub uFlags: u32,
    pub uCallbackMessage: u32,
    pub hIcon: HICON,
    pub szTip: [u16; 128],
    pub dwState: u32,
    pub dwStateMask: u32,
    pub szInfo: [u16; 256],
    pub uVersion: u32,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: u32,
    pub guidItem: [u8; 16],
    pub hBalloonIcon: HICON,
}

impl Default for NOTIFYICONDATAW {
    fn default() -> Self {
        // 全零初始化即可：字段都是整型/数组。
        unsafe { std::mem::zeroed() }
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetLastError() -> u32;
    pub fn SetLastError(code: u32);
    pub fn OpenProcess(access: u32, inherit: BOOL, pid: u32) -> HANDLE;
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn QueryFullProcessImageNameW(h: HANDLE, flags: u32, buf: LPWSTR, size: *mut u32) -> BOOL;
    pub fn GetCurrentProcessId() -> u32;
    pub fn FileTimeToLocalFileTime(src: *const FILETIME, out: *mut FILETIME) -> BOOL;
    pub fn FileTimeToSystemTime(src: *const FILETIME, out: *mut SYSTEMTIME) -> BOOL;
    pub fn SetPriorityClass(h: HANDLE, class: u32) -> BOOL;
    pub fn GetPriorityClass(h: HANDLE) -> u32;
    pub fn TerminateProcess(h: HANDLE, code: u32) -> BOOL;
    pub fn K32EnumProcessModules(
        h: HANDLE,
        modules: *mut HMODULE,
        cb: u32,
        needed: *mut u32,
    ) -> BOOL;
    pub fn K32GetModuleBaseNameW(h: HANDLE, module: HMODULE, base: LPWSTR, size: u32) -> u32;
    pub fn CreateMutexW(attrs: *mut c_void, owner: BOOL, name: LPCWSTR) -> HANDLE;
    pub fn CreateEventW(attrs: *mut c_void, manual: BOOL, initial: BOOL, name: LPCWSTR) -> HANDLE;
    pub fn OpenEventW(access: u32, inherit: BOOL, name: LPCWSTR) -> HANDLE;
    pub fn SetEvent(h: HANDLE) -> BOOL;
    pub fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32;
    pub fn GetModuleHandleW(name: LPCWSTR) -> HMODULE;
    pub fn GetExitCodeProcess(h: HANDLE, code: *mut u32) -> BOOL;
    pub fn GetLocalTime(st: *mut SYSTEMTIME);
    /// 系统启动以来的毫秒数（不会回绕）。用于给"开机标记"取兜底值。
    pub fn GetTickCount64() -> u64;
}

#[link(name = "user32")]
unsafe extern "system" {
    pub fn GetForegroundWindow() -> HWND;
    pub fn GetWindowThreadProcessId(hwnd: HWND, pid: *mut u32) -> u32;
    pub fn GetWindowRect(hwnd: HWND, rect: *mut RECT) -> BOOL;
    pub fn GetWindowTextW(hwnd: HWND, buf: LPWSTR, max: i32) -> i32;
    pub fn GetClassNameW(hwnd: HWND, buf: LPWSTR, max: i32) -> i32;
    pub fn GetSystemMetrics(index: i32) -> i32;
    pub fn RegisterClassExW(class: *const WNDCLASSEXW) -> u16;
    pub fn CreateWindowExW(
        ex_style: u32,
        class: LPCWSTR,
        title: LPCWSTR,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HMENU,
        instance: HMODULE,
        param: *mut c_void,
    ) -> HWND;
    pub fn DefWindowProcW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT;
    pub fn DestroyWindow(hwnd: HWND) -> BOOL;
    pub fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> BOOL;
    pub fn TranslateMessage(msg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    pub fn PostMessageW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> BOOL;
    pub fn PostQuitMessage(code: i32);
    pub fn CreateIconFromResourceEx(
        bits: *const u8,
        size: u32,
        is_icon: BOOL,
        version: u32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> HICON;
    pub fn DestroyIcon(icon: HICON) -> BOOL;
    pub fn SetForegroundWindow(hwnd: HWND) -> BOOL;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(menu: HMENU, flags: u32, id: usize, item: LPCWSTR) -> BOOL;
    pub fn TrackPopupMenu(
        menu: HMENU,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: HWND,
        rect: *const RECT,
    ) -> i32;
    pub fn DestroyMenu(menu: HMENU) -> BOOL;
    pub fn GetCursorPos(pt: *mut POINT) -> BOOL;
    pub fn MessageBoxW(hwnd: HWND, text: LPCWSTR, caption: LPCWSTR, ty: u32) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    pub fn Shell_NotifyIconW(message: u32, data: *mut NOTIFYICONDATAW) -> BOOL;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    pub fn NtQuerySystemInformation(
        class: u32,
        info: *mut c_void,
        len: u32,
        ret_len: *mut u32,
    ) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    pub fn RegCreateKeyExW(
        root: HKEY,
        sub: LPCWSTR,
        reserved: u32,
        class: LPCWSTR,
        options: u32,
        sam: u32,
        attrs: *mut c_void,
        out: *mut HKEY,
        disposition: *mut u32,
    ) -> i32;
    pub fn RegSetValueExW(
        key: HKEY,
        name: LPCWSTR,
        reserved: u32,
        ty: u32,
        data: *const u8,
        len: u32,
    ) -> i32;
    pub fn RegDeleteValueW(key: HKEY, name: LPCWSTR) -> i32;
    pub fn RegCloseKey(key: HKEY) -> i32;
    pub fn RegGetValueW(
        key: HKEY,
        sub: LPCWSTR,
        value: LPCWSTR,
        flags: u32,
        ty: *mut u32,
        data: *mut c_void,
        len: *mut u32,
    ) -> i32;
}

// ---------------------------------------------------------------- 字符串工具

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn last_error() -> u32 {
    unsafe { GetLastError() }
}

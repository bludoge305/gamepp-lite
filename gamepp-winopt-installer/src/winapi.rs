//! 安装器用到的最小 Win32 / COM 声明（零外部 crate）。

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type HKEY = isize;
pub type LONG = i32;
pub type DWORD = u32;
pub type BOOL = i32;
pub type LPCWSTR = *const u16;
pub type LPCSTR = *const u8;
pub type HRESULT = i32;

pub const HKEY_LOCAL_MACHINE: HKEY = 0x8000_0002u32 as i32 as isize;
pub const KEY_ALL_ACCESS: DWORD = 0x000F_003F;
pub const KEY_READ: DWORD = 0x0002_0019;
pub const KEY_WOW64_64KEY: DWORD = 0x0000_0100;
pub const REG_OPTION_NON_VOLATILE: DWORD = 0;
pub const REG_SZ: DWORD = 1;
pub const REG_DWORD: DWORD = 4;
pub const ERROR_SUCCESS: LONG = 0;
pub const ERROR_FILE_NOT_FOUND: LONG = 2;
pub const ERROR_PATH_NOT_FOUND: LONG = 3;

pub const TOKEN_QUERY: DWORD = 0x0008;
pub const TOKEN_ELEVATION: i32 = 20;

pub const SW_SHOWNORMAL: i32 = 1;
pub const MB_ICONERROR: u32 = 0x0000_0010;
pub const MB_ICONINFORMATION: u32 = 0x0000_0040;
pub const MB_OK: u32 = 0x0000_0000;
pub const MB_YESNO: u32 = 0x0000_0004;
pub const IDYES: i32 = 6;

pub const MOVEFILE_DELAY_UNTIL_REBOOT: DWORD = 0x0000_0004;

pub const CLSCTX_INPROC_SERVER: DWORD = 1;
pub const COINIT_APARTMENTTHREADED: DWORD = 0x2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

pub const CLSID_SHELLLINK: GUID = GUID {
    data1: 0x0002_1401,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

pub const IID_ISHELLLINKW: GUID = GUID {
    data1: 0x0002_14F9,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

pub const IID_IPERSISTFILE: GUID = GUID {
    data1: 0x0000_010B,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

#[link(name = "kernel32")]
extern "system" {
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn GetLastError() -> DWORD;
    pub fn MoveFileExW(from: LPCWSTR, to: LPCWSTR, flags: DWORD) -> BOOL;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegCreateKeyExW(
        key: HKEY,
        sub: LPCWSTR,
        reserved: DWORD,
        class: LPCWSTR,
        options: DWORD,
        sam: DWORD,
        sa: *const c_void,
        out: *mut HKEY,
        disposition: *mut DWORD,
    ) -> LONG;
    pub fn RegSetValueExW(
        key: HKEY,
        name: LPCWSTR,
        reserved: DWORD,
        ty: DWORD,
        data: *const u8,
        len: DWORD,
    ) -> LONG;
    pub fn RegOpenKeyExW(key: HKEY, sub: LPCWSTR, options: DWORD, sam: DWORD, out: *mut HKEY) -> LONG;
    pub fn RegQueryValueExW(
        key: HKEY,
        name: LPCWSTR,
        reserved: *mut DWORD,
        ty: *mut DWORD,
        data: *mut u8,
        len: *mut DWORD,
    ) -> LONG;
    pub fn RegDeleteTreeW(key: HKEY, sub: LPCWSTR) -> LONG;
    pub fn RegCloseKey(key: HKEY) -> LONG;

    pub fn OpenProcessToken(process: HANDLE, access: DWORD, token: *mut HANDLE) -> BOOL;
    pub fn GetTokenInformation(
        token: HANDLE,
        class: i32,
        info: *mut c_void,
        len: DWORD,
        ret_len: *mut DWORD,
    ) -> BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetCurrentProcess() -> HANDLE;
}

#[link(name = "shell32")]
extern "system" {
    pub fn ShellExecuteW(
        hwnd: HANDLE,
        op: LPCWSTR,
        file: LPCWSTR,
        params: LPCWSTR,
        dir: LPCWSTR,
        show: i32,
    ) -> isize;
}

#[link(name = "user32")]
extern "system" {
    pub fn MessageBoxW(hwnd: HANDLE, text: LPCWSTR, caption: LPCWSTR, ty: u32) -> i32;
}

#[link(name = "ole32")]
extern "system" {
    pub fn CoInitializeEx(reserved: *mut c_void, coinit: DWORD) -> HRESULT;
    pub fn CoUninitialize();
    pub fn CoCreateInstance(
        clsid: *const GUID,
        outer: *mut c_void,
        ctx: DWORD,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> HRESULT;
}

pub fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevated: u32 = 0;
        let mut ret: DWORD = 0;
        let ok = GetTokenInformation(
            token,
            TOKEN_ELEVATION,
            &mut elevated as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
            &mut ret,
        );
        CloseHandle(token);
        ok != 0 && elevated != 0
    }
}

/// 以管理员身份重新启动自己（返回 true 表示已成功拉起子进程）。
pub fn relaunch_elevated(exe: &std::path::Path, params: &str) -> bool {
    unsafe {
        let op = wide("runas");
        let file = wide(&exe.to_string_lossy());
        let args = wide(params);
        let r = ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            args.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
        r > 32
    }
}

pub fn message_box(text: &str, caption: &str, ty: u32) -> i32 {
    unsafe {
        let t = wide(text);
        let c = wide(caption);
        MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), ty)
    }
}

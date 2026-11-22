//! 开机自启（对应 `WebApiImpl_setStartWithWindowState` / `isStartedWithOs`）。
//!
//! GamePP 把自启写在 `HKEY_CURRENT_USER\Software\GamePP` 之下；这里写到标准的
//! `...\CurrentVersion\Run`，语义一致但更符合系统约定。

use super::ffi::*;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "GamePPLite";

pub fn enabled() -> bool {
    let sub = to_wide(RUN_KEY);
    let name = to_wide(VALUE_NAME);
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut len,
        )
    };
    r == 0
}

pub fn set(enable: bool) -> bool {
    let sub = to_wide(RUN_KEY);
    let name = to_wide(VALUE_NAME);
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let mut disp = 0u32;
        let r = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE,
            std::ptr::null_mut(),
            &mut key,
            &mut disp,
        );
        if r != 0 {
            return false;
        }
        let ok = if enable {
            let exe = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let cmd = format!("\"{exe}\"");
            let data = to_wide(&cmd);
            let bytes = (data.len() * 2) as u32;
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                bytes,
            ) == 0
        } else {
            let r = RegDeleteValueW(key, name.as_ptr());
            r == 0 || r == 2 // 2 = ERROR_FILE_NOT_FOUND，视为已关闭
        };
        RegCloseKey(key);
        ok
    }
}

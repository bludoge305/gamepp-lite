//! 卸载项注册表读写（控制面板「应用和功能」里显示的那一条）。

use crate::winapi as w;

pub const UNINSTALL_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\GamePP-Lite";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    Str(&'a str),
    Dword(u32),
}

pub fn write_values(path: &str, values: &[(&str, Value)]) -> Result<(), String> {
    unsafe {
        let sub = w::wide(path);
        let mut key: w::HKEY = 0;
        let mut disp: w::DWORD = 0;
        let rc = w::RegCreateKeyExW(
            w::HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            0,
            std::ptr::null(),
            w::REG_OPTION_NON_VOLATILE,
            w::KEY_ALL_ACCESS | w::KEY_WOW64_64KEY,
            std::ptr::null(),
            &mut key,
            &mut disp,
        );
        if rc != w::ERROR_SUCCESS {
            return Err(format!("RegCreateKeyExW 失败，Win32 错误码 {}", rc));
        }

        let mut err = None;
        for (name, value) in values {
            let n = w::wide(name);
            let rc = match value {
                Value::Str(s) => {
                    let bytes = w::wide(s); // 已含结尾 NUL
                    w::RegSetValueExW(
                        key,
                        n.as_ptr(),
                        0,
                        w::REG_SZ,
                        bytes.as_ptr() as *const u8,
                        (bytes.len() * 2) as u32,
                    )
                }
                Value::Dword(d) => {
                    let bytes = d.to_le_bytes();
                    w::RegSetValueExW(
                        key,
                        n.as_ptr(),
                        0,
                        w::REG_DWORD,
                        bytes.as_ptr(),
                        4,
                    )
                }
            };
            if rc != w::ERROR_SUCCESS && err.is_none() {
                err = Some(format!("写注册表值 {} 失败，错误码 {}", name, rc));
            }
        }
        w::RegCloseKey(key);
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

pub fn delete_key(path: &str) -> Result<(), String> {
    unsafe {
        let sub = w::wide(path);
        let rc = w::RegDeleteTreeW(w::HKEY_LOCAL_MACHINE, sub.as_ptr());
        if rc == w::ERROR_SUCCESS || rc == w::ERROR_FILE_NOT_FOUND || rc == w::ERROR_PATH_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("RegDeleteTreeW 失败，错误码 {}", rc))
        }
    }
}

pub fn read_string(path: &str, name: &str) -> Option<String> {
    unsafe {
        let sub = w::wide(path);
        let mut key: w::HKEY = 0;
        if w::RegOpenKeyExW(
            w::HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            0,
            w::KEY_READ | w::KEY_WOW64_64KEY,
            &mut key,
        ) != w::ERROR_SUCCESS
        {
            return None;
        }
        let n = w::wide(name);
        let mut ty: w::DWORD = 0;
        let mut len: w::DWORD = 0;
        if w::RegQueryValueExW(key, n.as_ptr(), std::ptr::null_mut(), &mut ty, std::ptr::null_mut(), &mut len)
            != w::ERROR_SUCCESS
            || len == 0
        {
            w::RegCloseKey(key);
            return None;
        }
        let mut buf = vec![0u16; (len as usize) / 2 + 1];
        let ok = w::RegQueryValueExW(
            key,
            n.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            buf.as_mut_ptr() as *mut u8,
            &mut len,
        );
        w::RegCloseKey(key);
        if ok != w::ERROR_SUCCESS {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

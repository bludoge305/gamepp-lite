//! 原生提示框。
//!
//! 用途只有一个：第二个实例被单实例锁拦下时，必须让用户看得见"没有闪退，
//! 是已经有一个在跑"。只是 `MessageBoxW`，不引任何 UI 依赖。

use super::ffi::*;

pub fn info(title: &str, text: &str) {
    let t = to_wide(text);
    let c = to_wide(title);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            t.as_ptr(),
            c.as_ptr(),
            MB_OK | MB_ICONINFORMATION | MB_TOPMOST,
        );
    }
}

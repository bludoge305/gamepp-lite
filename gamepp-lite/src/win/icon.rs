//! 从 .ico 字节里取出 HICON。
//!
//! Windows 侧要的是"单个图标的位图资源"，不是整个 .ico 文件：
//! 所以先按 ICO 目录找到最接近目标尺寸的那一项，把它指向的
//! BITMAPINFOHEADER+像素 交给 `CreateIconFromResourceEx`（dwVer=0x00030000）。

use super::ffi::*;

/// ICO 目录项：宽、字节长度、数据偏移。
fn best_entry(data: &[u8], want: u32) -> Option<(u32, usize, usize)> {
    if data.len() < 6 {
        return None;
    }
    let kind = u16::from_le_bytes([data[2], data[3]]);
    if kind != 1 {
        return None; // 不是图标
    }
    let count = u16::from_le_bytes([data[4], data[5]]) as usize;
    let mut best: Option<(u32, usize, usize, u32)> = None; // +score
    for i in 0..count {
        let e = 6 + i * 16;
        if e + 16 > data.len() {
            break;
        }
        let w = if data[e] == 0 { 256 } else { data[e] as u32 };
        let len = u32::from_le_bytes([data[e + 8], data[e + 9], data[e + 10], data[e + 11]]) as usize;
        let off = u32::from_le_bytes([data[e + 12], data[e + 13], data[e + 14], data[e + 15]]) as usize;
        if off + len > data.len() || len == 0 {
            continue;
        }
        let score = (w as i64 - want as i64).unsigned_abs() as u32;
        if best.map(|b| score < b.3).unwrap_or(true) {
            best = Some((w, off, len, score));
        }
    }
    best.map(|(w, off, len, _)| (w, off, len))
}

/// 生成 HICON；失败返回 None（调用方回退到系统默认图标）。
pub fn from_ico(data: &[u8], want: u32) -> Option<HICON> {
    let (w, off, len) = best_entry(data, want)?;
    let icon = unsafe {
        CreateIconFromResourceEx(
            data[off..].as_ptr(),
            len as u32,
            1,          // fIcon
            0x0003_0000, // dwVer
            w as i32,
            w as i32,
            0, // LR_DEFAULTCOLOR
        )
    };
    if icon.is_null() { None } else { Some(icon) }
}

pub fn destroy(icon: HICON) {
    if !icon.is_null() {
        unsafe {
            DestroyIcon(icon);
        }
    }
}

//! 时间格式化。
//!
//! 界面上"日志时间""会话开始时间"都必须是**本地时间**。`epoch % 86400` 只能得到
//! UTC 的时分秒，在东八区会整整差 8 小时 —— 所以这里走 Win32 的时区转换
//! （`FileTimeToLocalFileTime` 会带上夏令时规则），不引第三方日期库。

use super::ffi::*;

/// FILETIME 起点（1601-01-01）到 Unix 起点（1970-01-01）的秒数。
const EPOCH_DIFF_SECS: u64 = 11_644_473_600;

/// epoch 秒 → 本地 `HH:MM:SS`；转换失败时退回 UTC，至少不会 panic。
pub fn hms(epoch: u64) -> String {
    let ticks = match epoch.checked_add(EPOCH_DIFF_SECS) {
        Some(v) => v.saturating_mul(10_000_000),
        None => 0,
    };
    let utc = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };

    unsafe {
        let mut local = FILETIME::default();
        if FileTimeToLocalFileTime(&utc, &mut local) == 0 {
            return utc_hms(epoch);
        }
        let mut st = SYSTEMTIME::default();
        if FileTimeToSystemTime(&local, &mut st) == 0 {
            return utc_hms(epoch);
        }
        format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond)
    }
}

fn utc_hms(epoch: u64) -> String {
    let s = epoch % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

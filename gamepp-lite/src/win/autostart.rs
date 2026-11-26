//! 开机自启（对应 `WebApiImpl_setStartWithWindowState` / `isStartedWithOs`）。
//!
//! GamePP 把自启写在 `HKEY_CURRENT_USER\Software\GamePP` 之下；这里写到标准的
//! `...\CurrentVersion\Run`，语义一致但更符合系统约定。
//!
//! 注意「Run 值存在」不等于「系统会执行」：任务管理器 → 启动应用、以及各类
//! 优化/安全工具在禁用某项时**不删 Run 值**，只在 `Explorer\StartupApproved\Run`
//! 下写一条 12 字节的 REG_BINARY 标记。只读 Run 会得到"界面显示已开启、登录却
//! 不启动"的假阳性，所以判定与写入两侧都要带上这个标记。

use super::ffi::*;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "GamePPLite";
/// 「启动应用」页面的启用/禁用标记。首字节 0x02/0x06 为启用，0x03 为禁用，
/// 其余 8 字节是最后一次改动时间（本模块不用）。
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
/// 写回"启用"用的标记体：首 DWORD 0x02，其余归零。
const APPROVED_ENABLED: [u8; 12] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// 登录自启时追加到命令行的标记。
///
/// 对应原版 `isStartedWithOs` / `setStartWithWindowState`：进程要知道自己是被
/// 系统拉起的，才可能选择"不弹窗口、直接进托盘"。以前只写裸路径，应用无法区分
/// 来源，登录时必然弹一个完整主窗口。
pub const AUTOSTART_FLAG: &str = "--autostart";

/// 真实生效状态：值写过 **且** 没被「启动应用」页面禁用。
pub fn enabled() -> bool {
    run_value_present() && !blocked_by_os()
}

/// Run 值是否写过（只回答"有没有这一项"，不回答"系统会不会执行"）。
fn run_value_present() -> bool {
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

/// 是否被任务管理器/优化工具禁用过。
///
/// 没有该标记 = 从未被禁用 = 启用；读不到同样按启用处理 —— 宁可少报"已关闭"，
/// 也不要把用户明确打开过的项误判成关闭。首字节只有 0x03 表示禁用，
/// 0x02/0x06 都算启用。
fn blocked_by_os() -> bool {
    let sub = to_wide(APPROVED_KEY);
    let name = to_wide(VALUE_NAME);
    let mut buf = [0u8; 12];
    let mut len = buf.len() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_BINARY,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut len,
        )
    };
    r == 0 && len >= 4 && buf[0] == 0x03
}

/// 把禁用标记改回启用。
///
/// 只在`blocked_by_os()` 为真时调用：那说明 Explorer\StartupApproved 键已经存在，
/// 不需要（也不应该）为这件事凭空创建它。
fn clear_block() -> bool {
    let sub = to_wide(APPROVED_KEY);
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
        let ok = RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_BINARY,
            APPROVED_ENABLED.as_ptr(),
            APPROVED_ENABLED.len() as u32,
        ) == 0;
        RegCloseKey(key);
        ok
    }
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
            let cmd = format!("\"{exe}\" {AUTOSTART_FLAG}");
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
        // 写值本身成功不代表会生效：用户在任务管理器里禁用过的话，系统仍不执行。
        // 此时把标记改回"启用"，等于替用户把这项从"已禁用"里捞回来。
        if ok && enable && blocked_by_os() {
            clear_block()
        } else {
            ok
        }
    }
}

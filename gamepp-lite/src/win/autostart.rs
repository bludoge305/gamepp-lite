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
///
/// 掩码同时接受 REG_SZ 与 REG_EXPAND_SZ：手工改过、或被别的安装器/工具写过的项
/// 常见是 EXPAND_SZ（里面放 `%LOCALAPPDATA%\..`），只认 REG_SZ 会把它读成"没开"。
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
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
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

/// 设置开机自启。
///
/// `Ok(())` 表示**写完并回读校验通过**，而不只是"API 没报错"——被安全软件拦下、
/// 写到只读键、写进去又被改回来的情况，靠返回值是看不出来的。
/// `Err` 带阶段描述与 Win32 错误码，调用方原样展示，同时由本函数落进 runtime.log。
pub fn set(enable: bool) -> Result<(), String> {
    let r = apply(enable);
    if let Err(e) = &r {
        crate::log_line(
            "autostart",
            &format!(
                "{}开机自启失败：{e}",
                if enable { "开启" } else { "关闭" }
            ),
        );
    }
    r
}

fn apply(enable: bool) -> Result<(), String> {
    // 自身路径先取好再开键：取不到就直接早退，不会留下没关闭的注册表句柄。
    let cmd = if enable {
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .map_err(|e| format!("取自身路径失败：{e}"))?;
        Some(format!("\"{exe}\" {AUTOSTART_FLAG}"))
    } else {
        None
    };

    let sub = to_wide(RUN_KEY);
    let name = to_wide(VALUE_NAME);
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let mut disp = 0u32;
        // 只要写值权限：建键时用 KEY_SET_VALUE 而不是 KEY_WRITE，权限面更小，
        // 在受控环境（组策略收紧注册表权限）里更不容易被直接拒掉。
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE,
            std::ptr::null_mut(),
            &mut key,
            &mut disp,
        );
        if rc != 0 {
            return Err(format!("打开 Run 键失败（Win32 错误码 {rc}）"));
        }
        let rc = match &cmd {
            Some(cmd) => {
                let data = to_wide(cmd);
                RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    (data.len() * 2) as u32,
                )
            }
            None => RegDeleteValueW(key, name.as_ptr()),
        };
        RegCloseKey(key);
        // 2 = ERROR_FILE_NOT_FOUND：关闭时本来就没有这一项，视同成功。
        if rc != 0 && !(cmd.is_none() && rc == 2) {
            let stage = if cmd.is_some() { "写入" } else { "删除" };
            return Err(format!("{stage} Run 值失败（Win32 错误码 {rc}）"));
        }
    }

    // 写值本身成功不代表会生效：用户在任务管理器里禁用过的话，系统仍不执行。
    // 此时把标记改回"启用"，等于替用户把这项从"已禁用"里捞回来。
    if enable && blocked_by_os() && !clear_block() {
        return Err("清除「启动应用」禁用标记失败".into());
    }

    // 回读校验：以真实生效状态为准，而不是以刚才那次写入的返回值为准。
    if enabled() != enable {
        return Err("写入后回读不一致（可能被安全软件拦截或改回）".into());
    }
    Ok(())
}

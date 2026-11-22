//! 单实例与"唤醒已有实例"。
//!
//! 逆向对照：GamePP 用命名互斥体 `GAMEPP-INGAME-MUTEX-{}-{}` 与命名事件
//! `GamePP-Product-{}-{}-Running` 判定"另一个实例是否在跑"，重复启动时通过
//! `second-instance` 事件把参数投递给常驻实例。这里用同一套办法：
//! 一个命名 Mutex 抢所有权，一个命名 Event 用于唤醒。

use super::ffi::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 单实例互斥体句柄。0 表示"本进程没有持有"。
static MUTEX: AtomicUsize = AtomicUsize::new(0);

/// 抢占单实例互斥体；返回 true 表示本进程成为唯一实例。
pub fn acquire(key: &str) -> bool {
    let name = to_wide(&format!("Local\\{key}-mutex"));
    unsafe {
        // GetLastError 不会因为"调用成功"而清零，必须先显式置 0，
        // 否则前面任何一次失败的文件/注册表调用留下的 183 会让这里误判"已有实例"。
        SetLastError(0);
        let h = CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr());
        if h.is_null() {
            // 建不出来就当失败放行，不影响主功能。
            return true;
        }
        let already = last_error() == ERROR_ALREADY_EXISTS;
        if already {
            // 别人的互斥体：立刻关掉自己的句柄，别让内核对象被我们拖着。
            CloseHandle(h);
            return false;
        }
        // 句柄故意不关闭：进程存活期间互斥体必须一直持有。
        MUTEX.store(h as usize, Ordering::SeqCst);
        true
    }
}

/// 释放单实例互斥体。
///
/// 只在"拉起提权副本"之前调用：提权副本是同一会话里的新进程，会看到我们持有的
/// 同名互斥体，从而把自己判成"第二个实例"、唤醒我们后直接退出 —— 那样等于点了
/// 「以管理员身份重启」却什么也没启动。所以必须先让出，再由副本接手。
pub fn release() {
    let h = MUTEX.swap(0, Ordering::SeqCst);
    if h != 0 {
        unsafe {
            CloseHandle(h as HANDLE);
        }
    }
}

/// 唤醒已在运行的实例（第二个实例调用）。
///
/// 返回 false 表示找不到监听者（第一个实例可能还没建好事件，或已异常退出）。
pub fn awake_existing(key: &str) -> bool {
    let name = to_wide(&format!("Local\\{key}-awake"));
    unsafe {
        let h = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if h.is_null() {
            return false;
        }
        let ok = SetEvent(h) != 0;
        CloseHandle(h);
        ok
    }
}

/// 常驻实例侧：建唤醒事件并起线程等待；收到信号即通过 channel 通知 UI。
pub fn spawn_awake_listener(key: &str) -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let name = to_wide(&format!("Local\\{key}-awake"));

    // 事件必须在**本线程**建好再交给等待线程：否则第二个实例可能抢在
    // CreateEventW 之前 OpenEventW，拿到空句柄后误报"没能唤醒"。
    let handle = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, name.as_ptr()) };
    if handle.is_null() {
        crate::log_line("single", "唤醒事件创建失败，重复启动将无法切回窗口");
        return rx;
    }
    // 裸指针不是 Send，转成整数带进线程。
    let handle_addr = handle as usize;

    std::thread::Builder::new()
        .name("gpp-awake".into())
        .spawn(move || unsafe {
            loop {
                let r = WaitForSingleObject(handle_addr as HANDLE, 1000);
                if r == WAIT_OBJECT_0 && tx.send(()).is_err() {
                    break;
                }
            }
        })
        .ok();
    rx
}

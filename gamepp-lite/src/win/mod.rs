//! Win32 封装层。对应逆向到的"原生侧"：
//! `GPPPlatform.exe`（前台窗口监视）、`processhelper`（进程信息）、
//! `GAMEPP-INGAME-MUTEX-*`（命名对象互斥）。

pub mod autostart;
pub mod ffi;
pub mod foreground;
pub mod icon;
pub mod msgbox;
pub mod process;
pub mod single;
pub mod time;

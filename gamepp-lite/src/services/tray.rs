//! 托盘常驻：自建消息窗口 + `Shell_NotifyIcon` + 右键菜单。
//!
//! 逆向对照：
//! - Electron 的 `Tray` 在 Windows 上最终也是 `Shell_NotifyIcon`；GamePP 的托盘文案
//!   （`TrayTitle` / `TrayToolTip` / `Game_Traymenu` / `displayBalloon`）说明它把
//!   "图标 + 原生弹出菜单 + 气泡"这一整层放在主进程里；
//! - 单实例由 `requestSingleInstanceLock` 保证，重复启动走 `second-instance`。
//!
//! 图标有两套（`assets/app.ico` / `assets/app-opt.ico`）：
//! 未优化 = 石板灰 + 闪电，已优化 = 绿色 + 对勾。状态变化时换图标并改提示文本，
//! 这样用户不用打开窗口就能看到 Windows 优化的当前状态。

use crate::win::ffi::*;
use crate::win::icon;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

/// 未优化态图标：石板灰 + 白闪电。
const ICO_IDLE: &[u8] = include_bytes!("../../assets/app.ico");
/// 已优化态图标：绿色 + 白对勾。
const ICO_OPTIMIZED: &[u8] = include_bytes!("../../assets/app-opt.ico");

/// 托盘图标的目标尺寸（Windows 会自己再缩放）。
const ICON_PX: u32 = 32;

/// 托盘菜单/事件命令。
#[derive(Clone, Debug, PartialEq)]
pub enum TrayCmd {
    ShowMain,
    ShowWinOpt,
    ShowSettings,
    ToggleAutostart,
    TestBalloon,
    Exit,
}

const MSG_CALLBACK: u32 = WM_APP + 1;
const MSG_REFRESH: u32 = WM_APP + 2;
const MSG_BALLOON: u32 = WM_APP + 3;
const MSG_QUIT: u32 = WM_APP + 4;
const WM_NULL: u32 = 0;

const ID_SHOW: usize = 1;
const ID_WINOPT: usize = 2;
const ID_SETTINGS: usize = 3;
const ID_AUTOSTART: usize = 4;
const ID_BALLOON: usize = 5;
const ID_EXIT: usize = 6;

struct Ctx {
    tx: Sender<TrayCmd>,
    /// 提示文本的"尾巴"（当前游戏等），状态前缀由 `compose_tip` 统一拼。
    tail: Mutex<String>,
    balloon: Mutex<String>,
    autostart: AtomicBool,
    optimized: AtomicBool,
    hwnd: Mutex<usize>,
}

static CTX: OnceLock<Arc<Ctx>> = OnceLock::new();

/// 取托盘消息窗口句柄（0 = 还没建好）。锁中毒也照常返回，托盘线程不该因此 panic。
fn hwnd_of(ctx: &Ctx) -> usize {
    *ctx.hwnd.lock().unwrap_or_else(|e| e.into_inner())
}

fn compose_tip(ctx: &Ctx) -> String {
    let state = if ctx.optimized.load(Ordering::Relaxed) {
        "已优化"
    } else {
        "未优化"
    };
    let tail = ctx.tail.lock().map(|t| t.clone()).unwrap_or_default();
    if tail.is_empty() {
        format!("GamePP-Lite · {state}")
    } else {
        format!("GamePP-Lite · {state} · {tail}")
    }
}

fn icon_handle(ctx: &Ctx) -> HICON {
    let data = if ctx.optimized.load(Ordering::Relaxed) {
        ICO_OPTIMIZED
    } else {
        ICO_IDLE
    };
    icon::from_ico(data, ICON_PX).unwrap_or(std::ptr::null_mut())
}

/// UI 侧句柄。
#[derive(Clone)]
pub struct TrayHandle {
    ctx: Arc<Ctx>,
}

impl TrayHandle {
    pub fn set_autostart(&self, on: bool) {
        self.ctx.autostart.store(on, Ordering::Relaxed);
    }

    /// 切换"已优化/未优化"图标与提示文本。
    pub fn set_optimized(&self, optimized: bool) {
        if self.ctx.optimized.swap(optimized, Ordering::Relaxed) == optimized {
            return;
        }
        self.refresh();
    }

    /// 提示文本的尾巴（例如"游戏中：xxx"）；传空串则只显示状态。
    pub fn set_tooltip(&self, text: &str) {
        if let Ok(mut t) = self.ctx.tail.lock() {
            *t = text.to_string();
        }
        self.refresh();
    }

    pub fn balloon(&self, text: &str) {
        if let Ok(mut b) = self.ctx.balloon.lock() {
            *b = text.to_string();
        }
        let h = hwnd_of(&self.ctx);
        if h != 0 {
            unsafe { PostMessageW(h as HWND, MSG_BALLOON, 0, 0) };
        }
    }

    pub fn exit(&self) {
        let h = hwnd_of(&self.ctx);
        if h != 0 {
            unsafe { PostMessageW(h as HWND, MSG_QUIT, 0, 0) };
        }
    }

    fn refresh(&self) {
        let h = hwnd_of(&self.ctx);
        if h != 0 {
            unsafe { PostMessageW(h as HWND, MSG_REFRESH, 0, 0) };
        }
    }
}

fn set_tip(target: &mut [u16], text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let n = wide.len().min(target.len() - 1);
    target[..n].copy_from_slice(&wide[..n]);
    target[n] = 0;
}

fn show_menu(hwnd: HWND) {
    let Some(ctx) = CTX.get() else { return };
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        let item = |s: &str| to_wide(s);
        let (i_show, i_opt, i_set) = (item("打开主界面"), item("Windows 优化"), item("设置"));
        let (i_boot, i_balloon, i_exit) = (item("开机自启"), item("测试气泡"), item("退出"));
        AppendMenuW(menu, MF_STRING, ID_SHOW, i_show.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_WINOPT, i_opt.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_SETTINGS, i_set.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        let boot_flags = if ctx.autostart.load(Ordering::Relaxed) {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        AppendMenuW(menu, boot_flags, ID_AUTOSTART, i_boot.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_BALLOON, i_balloon.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, ID_EXIT, i_exit.as_ptr());

        let mut pt = POINT::default();
        GetCursorPos(&mut pt);
        // 经典坑：TrackPopupMenu 前必须把窗口设成前台，否则点菜单外面不收菜单。
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);

        if cmd > 0 {
            let mapped = match cmd as usize {
                ID_SHOW => Some(TrayCmd::ShowMain),
                ID_WINOPT => Some(TrayCmd::ShowWinOpt),
                ID_SETTINGS => Some(TrayCmd::ShowSettings),
                ID_AUTOSTART => Some(TrayCmd::ToggleAutostart),
                ID_BALLOON => Some(TrayCmd::TestBalloon),
                ID_EXIT => Some(TrayCmd::Exit),
                _ => None,
            };
            if let Some(c) = mapped {
                let _ = ctx.tx.send(c);
            }
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(ctx) = CTX.get() else {
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    };
    match msg {
        MSG_CALLBACK => {
            match lp as u32 {
                WM_LBUTTONDBLCLK | WM_LBUTTONUP => {
                    let _ = ctx.tx.send(TrayCmd::ShowMain);
                }
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            0
        }
        MSG_REFRESH => {
            // 换图标 + 换提示文本，一次 NIM_MODIFY 完成。
            let tip = compose_tip(ctx);
            unsafe {
                let mut nid = NOTIFYICONDATAW::default();
                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = hwnd;
                nid.uID = 1;
                nid.uFlags = NIF_ICON | NIF_TIP;
                nid.hIcon = icon_handle(ctx);
                set_tip(&mut nid.szTip, &tip);
                let ok = Shell_NotifyIconW(NIM_MODIFY, &mut nid);
                crate::log_line(
                    "tray",
                    &format!(
                        "托盘状态刷新（{}）：{}",
                        if ctx.optimized.load(Ordering::Relaxed) {
                            "已优化"
                        } else {
                            "未优化"
                        },
                        if ok != 0 { "成功" } else { "失败" }
                    ),
                );
                // NOTIFYICONDATA 只是传值，图标句柄要自己留着并在替换后销毁旧的。
                icon::destroy(nid.hIcon);
            }
            0
        }
        MSG_BALLOON => {
            let text = ctx.balloon.lock().map(|t| t.clone()).unwrap_or_default();
            unsafe {
                let mut nid = NOTIFYICONDATAW::default();
                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = hwnd;
                nid.uID = 1;
                nid.uFlags = NIF_INFO;
                nid.dwInfoFlags = NIIF_INFO;
                set_tip(&mut nid.szInfo, &text);
                set_tip(&mut nid.szInfoTitle, "GamePP-Lite");
                Shell_NotifyIconW(NIM_MODIFY, &mut nid);
            }
            0
        }
        MSG_QUIT => {
            unsafe {
                let mut nid = NOTIFYICONDATAW::default();
                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = hwnd;
                nid.uID = 1;
                Shell_NotifyIconW(NIM_DELETE, &mut nid);
                DestroyWindow(hwnd);
                PostQuitMessage(0);
            }
            0
        }
        WM_CLOSE | WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// 起托盘线程。`optimized` 是启动时的初始状态（用于选图标）。
pub fn spawn(app_name: &str, optimized: bool) -> (TrayHandle, Receiver<TrayCmd>) {
    let (tx, rx) = std::sync::mpsc::channel::<TrayCmd>();
    let ctx = Arc::new(Ctx {
        tx,
        tail: Mutex::new(String::new()),
        balloon: Mutex::new(String::new()),
        autostart: AtomicBool::new(false),
        optimized: AtomicBool::new(optimized),
        hwnd: Mutex::new(0),
    });
    let _ = CTX.set(ctx.clone());
    let handle = TrayHandle { ctx: ctx.clone() };
    let app_name = app_name.to_string();

    std::thread::Builder::new()
        .name("gpp-tray".into())
        .spawn(move || unsafe {
            let class_name = to_wide("GppLiteTrayHost");
            let instance = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            RegisterClassExW(&wc);

            // HWND_MESSAGE = -3：只在消息里存在的窗口，不占任务栏。
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                -3isize as HWND,
                std::ptr::null_mut(),
                instance,
                std::ptr::null_mut(),
            );
            if hwnd.is_null() {
                crate::log_line("tray", "托盘消息窗口创建失败（CreateWindowExW 返回空）");
                return;
            }
            *ctx.hwnd.lock().unwrap_or_else(|e| e.into_inner()) = hwnd as usize;

            let tip = compose_tip(&ctx);
            let mut nid = NOTIFYICONDATAW::default();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = MSG_CALLBACK;
            nid.hIcon = icon_handle(&ctx);
            set_tip(&mut nid.szTip, &tip);
            let ok = Shell_NotifyIconW(NIM_ADD, &mut nid);
            crate::log_line(
                "tray",
                &format!(
                    "{}（{app_name}，{}, 状态={}）",
                    if ok != 0 {
                        "托盘图标已创建"
                    } else {
                        "托盘图标创建失败（NIM_ADD 返回 0）"
                    },
                    if nid.hIcon.is_null() {
                        "内置空图标"
                    } else {
                        "自绘 ICO"
                    },
                    if optimized { "已优化" } else { "未优化" }
                ),
            );

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .ok();

    (handle, rx)
}

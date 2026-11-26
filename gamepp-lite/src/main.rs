//! 进程引导层。
//!
//! 对应逆向结论：Electron 主进程启动时先做单实例判定
//! （`requestSingleInstanceLock` / 命名互斥体 `GAMEPP-INGAME-MUTEX-{}-{}`），
//! 再创建托盘（`createAppTray`），最后启动 core services。这里保持同样顺序。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod domain;
mod opt;
mod services;
mod ui;
mod win;

use app::App;
use std::io::Write;
use win::{ffi, single};

const APP_NAME: &str = "GamePP-Lite";
/// 单实例互斥体键。开发期可用 `GPP_INSTANCE_SUFFIX` 追加后缀，让测试实例
/// 与正在运行的正式实例并存（否则新实例只会唤醒旧的然后自己退出）。
///
/// 只在 debug 构建生效：release 里放开它等于允许双实例，两个 watcher 会并发写
/// `opt-state.ini`、开两个托盘、自动优化/还原互相踩，是正确性问题而非调试便利。
fn instance_key() -> String {
    let suffix = if cfg!(debug_assertions) {
        std::env::var("GPP_INSTANCE_SUFFIX").unwrap_or_default()
    } else {
        String::new()
    };
    format!("GamePP-Lite-Core{suffix}")
}

/// 本地时间戳，形如 `2026-11-26 18:30:04.123`。
///
/// 走 `GetLocalTime` 而不是把 `SystemTime` 自己换算成本地时区：换算要处理时区
/// 与夏令时，标准库给不出，为一行日志引 chrono 又不划算。
fn stamp() -> String {
    let mut st = ffi::SYSTEMTIME::default();
    unsafe { ffi::GetLocalTime(&mut st) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, st.wMilliseconds
    )
}

/// 启动上下文：谁把本进程拉起来的、从哪个目录、什么权限。
///
/// 自启排查时这三项是分水岭：参数里有没有 `--autostart` 决定来源；工作目录能
/// 区分"快捷方式/资源管理器/命令行"拉起；提权状态决定注册表落到哪个账户的 HKCU。
fn boot_context() -> String {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = if args.is_empty() {
        "无".to_string()
    } else {
        args.join(" ")
    };
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<取不到：{e}>"));
    format!("参数={args}；cwd={cwd}；已提权={}", opt::is_elevated())
}

/// 追加一行运行日志（闪退时用户只需把这个文件发来）。
pub fn log_line(tag: &str, text: &str) {
    let path = domain::settings::data_dir().join("runtime.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "[{}][{tag}] {text}", stamp());
    }
}

/// 安装 panic 钩子：任何线程 panic 都写进 crash.log（含 backtrace）。
fn install_crash_log() {
    let path = domain::settings::data_dir().join("crash.log");
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "==== panic @ {} ====", stamp());
            let _ = writeln!(f, "{info}");
            let _ = writeln!(f, "{}", std::backtrace::Backtrace::force_capture());
        }
    }));
}

fn main() -> eframe::Result<()> {
    install_crash_log();
    // 是否由开机自启拉起：写 Run 值时带了该标记（见 win::autostart::AUTOSTART_FLAG）。
    // 据此登录时不弹主窗口，只留托盘与识别服务 —— 对应原版 isStartedWithOs。
    let launched_by_os = std::env::args().any(|a| a == win::autostart::AUTOSTART_FLAG);
    log_line(
        "boot",
        &format!(
            "启动（来源={}；{}）",
            if launched_by_os { "开机自启" } else { "用户" },
            boot_context()
        ),
    );

    // ① 单实例：先抢占命名内核对象，抢不到说明已有常驻实例。
    if !single::acquire(&instance_key()) {
        // 已有实例在跑：先投递唤醒信号（等价 Electron 的 second-instance），
        // 再给用户一个明确反馈 —— 否则窗口一闪而过，看起来就像"闪退"。
        let woke = single::awake_existing(&instance_key());
        log_line(
            "boot",
            &format!("已有实例在运行，唤醒信号投递{}", if woke { "成功" } else { "失败" }),
        );
        // 唤醒成功时由常驻实例自己把窗口拉到前台，无需打扰用户；
        // 只有连监听者都找不到（对方已卡死/异常）才弹提示，避免"窗口一闪而过"被当成闪退。
        if !woke {
            win::msgbox::info(
                "GamePP-Lite",
                "已经有一个 GamePP-Lite 在运行，但没能唤醒它。\n请到任务管理器结束残留的 gamepp-lite.exe 再重新打开。",
            );
        }
        return Ok(());
    }

    // ② 托盘：在独立线程里建消息窗口并跑 GetMessage 循环，先于主窗口就绪，
    //    避免最小化到托盘时段出现"无图标"的空窗。
    let optimized_at_boot = opt::State::load().optimized;
    let (tray, tray_rx) = services::tray::spawn(APP_NAME, optimized_at_boot);

    // ③ 唤醒监听：别的实例启动时会把我们拉到前台。
    let awake_rx = single::spawn_awake_listener(&instance_key());

    // ④ UI + 服务
    // 开发期可用 GPP_SIZE=864x608 指定逻辑窗口大小，便于在小窗口下核验排版。
    let (w, h) = std::env::var("GPP_SIZE")
        .ok()
        .and_then(|v| {
            let mut it = v.split('x');
            Some((it.next()?.trim().parse::<f32>().ok()?, it.next()?.trim().parse::<f32>().ok()?))
        })
        .unwrap_or((1040.0, 720.0));

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([w, h])
            .with_min_inner_size([820.0, 560.0])
            // 自启拉起时不显示窗口：托盘图标与识别服务照常起来，
            // 用户点托盘"打开主界面"再由 App::show_window 显形。
            .with_visible(!launched_by_os)
            .with_title(format!("{APP_NAME} · 游戏识别与 Windows 优化")),
        ..Default::default()
    };

    log_line(
        "boot",
        &format!(
            "创建主窗口 {w}x{h}（逻辑像素，{}）",
            if launched_by_os { "自启隐藏" } else { "直接显示" }
        ),
    );
    let result = eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                tray,
                tray_rx,
                awake_rx,
                launched_by_os,
            )))
        }),
    );
    log_line("exit", &format!("事件循环结束：{result:?}"));
    result
}

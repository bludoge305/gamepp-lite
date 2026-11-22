//! 应用外壳：状态、导航、事件分发。
//!
//! 三个入口对应逆向出的三类职责：
//! - ① 游戏识别：`game listener` / `GPPPlatform.exe`
//! - ② Windows 优化：`gameppoptimizesdk` + `GamePPOpt.ini [SYS]`
//! - ③ 设置：`setting system` + `wapTray`（托盘配置收进设置页）

use crate::domain::games::GameLibrary;
use crate::domain::settings::Settings;
use crate::opt;
use crate::services::tray::{TrayCmd, TrayHandle};
use crate::services::{Config, DetectState, Level, Services, UiNotice};
use crate::ui;
use crate::win::autostart;
use eframe::egui;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Detect,
    WinOpt,
    Settings,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Detect => "游戏识别",
            Tab::WinOpt => "Windows 优化",
            Tab::Settings => "设置",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tab::Detect => "前台窗口 → 进程 → 游戏库匹配",
            Tab::WinOpt => "进程优先级 / 服务 / 电源方案 / 后台进程",
            Tab::Settings => "识别节拍、托盘常驻、数据目录",
        }
    }
}

/// 游戏库行内编辑的草稿。
#[derive(Clone)]
pub struct EditDraft {
    pub orig: String,
    pub exe: String,
    pub name: String,
}

/// 后台优化线程回传的消息。
enum OptMsg {
    Done {
        lines: Vec<String>,
        restored: bool,
        dry_run: bool,
    },
}

pub struct App {
    pub settings: Settings,
    pub library: GameLibrary,
    pub services: Services,
    pub tray: TrayHandle,
    pub tray_rx: Receiver<TrayCmd>,
    pub awake_rx: Receiver<()>,
    pub tab: Tab,
    pub dirty: bool,
    pub autostart_on: bool,
    pub status: Option<(String, f64)>,
    pub new_custom_exe: String,
    pub new_custom_name: String,
    pub editing: Option<EditDraft>,
    /// 最近一次优化的输出（预演/执行/还原共用）。
    pub opt_log: Vec<String>,
    pub opt_running: bool,
    pub opt_restored: bool,
    pub elevated: bool,
    opt_rx: Option<Receiver<OptMsg>>,
    /// 仅开发期使用的自截图帧计数（GPP_SHOT 环境变量触发）。
    shot_frame: u32,
    /// 仅开发期的探针计数（GPP_PROBE 环境变量触发）。
    probe_frame: u32,
    /// 真正退出中：此时不再把"关闭窗口"拦成"收进托盘"。
    quitting: bool,
    /// 「恢复默认设置」确认弹窗（D：危险操作二次确认）。
    pub confirm_reset: bool,
    /// 「退出程序」确认弹窗。
    pub confirm_quit: bool,
    ctx: egui::Context,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        tray: TrayHandle,
        tray_rx: Receiver<TrayCmd>,
        awake_rx: Receiver<()>,
    ) -> Self {
        // CJK 字体作为打底（base），主题字体在它之上安装；
        // 再应用 shadcn vega 主题（含深色样式）并锁定深色模式。
        let fonts = install_cjk_font(&cc.egui_ctx);
        let theme = shadcn_egui::Theme::vega()
            .color(
                ui::GOOD_ROLE,
                egui::Color32::from_rgb(0x1E, 0x8E, 0x5A),
                egui::Color32::from_rgb(0x4A, 0xDE, 0x80),
            )
            .color(
                ui::WARN_ROLE,
                egui::Color32::from_rgb(0xC9, 0x7A, 0x11),
                egui::Color32::from_rgb(0xFA, 0xC7, 0x75),
            );
        theme.apply_with_fonts(&cc.egui_ctx, fonts);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx.style_mut_of(egui::Theme::Dark, |s| {
            s.spacing.item_spacing = egui::vec2(10.0, 8.0);
            s.spacing.button_padding = egui::vec2(10.0, 5.0);
        });

        let mut settings = Settings::load();
        let library = GameLibrary::load();

        // 开机自启以注册表为准（用户可能在系统里手工改过）。
        let autostart_on = autostart::enabled();
        settings.autostart = autostart_on;

        tray.set_autostart(autostart_on);
        // 状态前缀（已优化/未优化）由托盘自己拼，这里只给"尾巴"。
        tray.set_tooltip("");

        let cfg = Config {
            poll_ms: settings.poll_ms,
            stable_polls: settings.stable_polls,
            history_limit: settings.history_limit,
            log_limit: settings.log_limit,
            library: library.clone(),
        };
        let services = Services::start(cfg);
        services.snapshot_log(Level::Info, "界面就绪，识别服务已启动");

        // 常驻刷新：即使窗口被最小化也保证事件循环被唤醒，
        // 这样托盘菜单/唤醒信号仍能即时响应。
        {
            let ctx = cc.egui_ctx.clone();
            std::thread::Builder::new()
                .name("gpp-ui-tick".into())
                .spawn(move || loop {
                    std::thread::sleep(Duration::from_millis(400));
                    ctx.request_repaint();
                })
                .ok();
        }

        let opt_restored = !opt::State::load().optimized;

        let mut app = Self {
            settings,
            library,
            services,
            tray,
            tray_rx,
            awake_rx,
            tab: initial_tab(),
            dirty: false,
            autostart_on,
            status: None,
            new_custom_exe: String::new(),
            new_custom_name: String::new(),
            editing: None,
            opt_log: Vec::new(),
            opt_running: false,
            opt_restored,
            elevated: opt::is_elevated(),
            opt_rx: None,
            shot_frame: 0,
            probe_frame: 0,
            quitting: false,
            confirm_reset: false,
            confirm_quit: false,
            ctx: cc.egui_ctx.clone(),
        };
        if std::env::var("GPP_OPT_DRY").is_ok() {
            app.start_opt(true, false);
        }
        app
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        self.status = Some((text.into(), secs + 4.0));
    }

    /// 把配置推给服务线程（用户改动后调用），并落盘。
    pub fn sync_cfg(&mut self) {
        let cfg = Config {
            poll_ms: self.settings.poll_ms,
            stable_polls: self.settings.stable_polls,
            history_limit: self.settings.history_limit,
            log_limit: self.settings.log_limit,
            library: self.library.clone(),
        };
        self.services.update_cfg(|c| *c = cfg);
        let _ = self.settings.save();
        self.library.save();
        self.dirty = false;
    }

    /// 从磁盘重新加载配置与游戏库。
    pub fn reload_all(&mut self) {
        self.settings = Settings::load();
        self.library = GameLibrary::load();
        self.autostart_on = autostart::enabled();
        self.settings.autostart = self.autostart_on;
        self.tray.set_autostart(self.autostart_on);
        self.dirty = true;
        self.toast("已从磁盘重新加载");
    }

    // ---------------------------------------------------------------- 游戏库编辑

    pub fn begin_edit(&mut self, exe: &str, name: &str) {
        self.editing = Some(EditDraft {
            orig: exe.to_string(),
            exe: exe.to_string(),
            name: name.to_string(),
        });
    }

    pub fn commit_edit(&mut self) {
        let Some(draft) = self.editing.clone() else {
            return;
        };
        if draft.exe.trim().is_empty() {
            self.toast("exe 名不能为空");
            return;
        }
        if self.library.update(&draft.orig, &draft.exe, &draft.name) {
            self.editing = None;
            self.dirty = true;
            self.toast("已保存该条目");
        } else {
            self.toast("保存失败：新 exe 名与既有条目重名");
        }
    }

    pub fn delete_entry(&mut self, exe: &str) {
        self.library.remove(exe);
        if self.editing.as_ref().map(|d| d.orig == exe).unwrap_or(false) {
            self.editing = None;
        }
        self.dirty = true;
        self.toast("已删除该条目");
    }

    pub fn restore_library(&mut self) {
        self.library.restore_default();
        self.editing = None;
        self.dirty = true;
        self.toast("游戏库已恢复为内置列表");
    }

    // ---------------------------------------------------------------- Windows 优化

    /// 后台执行一次优化/还原；`dry_run` 只产出计划文本。
    pub fn start_opt(&mut self, dry_run: bool, restore: bool) {
        if self.opt_running {
            self.toast("上一次优化还在执行中");
            return;
        }
        if !dry_run && !self.elevated {
            self.services.snapshot_log(
                Level::Warn,
                "Windows 优化需要管理员权限，本次已跳过（可在 ② 页点「以管理员身份重启」）",
            );
            self.toast("需要管理员权限：请先点「以管理员身份重启」");
            return;
        }

        let plan = self.settings.opt.clone();
        let game_pid = self
            .services
            .shared
            .lock()
            .ok()
            .and_then(|s| match &s.detect {
                DetectState::InGame { pid, .. } => Some(*pid),
                _ => None,
            })
            .unwrap_or(0);

        let (tx, rx) = std::sync::mpsc::channel::<OptMsg>();
        self.opt_rx = Some(rx);
        self.opt_running = true;
        self.opt_log.clear();
        self.opt_log.push(if restore {
            "正在还原…".to_string()
        } else if dry_run {
            "正在生成预演计划…".to_string()
        } else {
            "正在执行优化…".to_string()
        });

        std::thread::Builder::new()
            .name("gpp-winopt".into())
            .spawn(move || {
                let report = if restore {
                    opt::restore_plan(&plan, dry_run)
                } else {
                    opt::run_plan(&plan, game_pid, dry_run)
                };
                // 只有真正动过系统才落盘/清状态
                if !dry_run {
                    if restore {
                        opt::State::clear();
                    } else {
                        report.state.save();
                    }
                }
                let _ = tx.send(OptMsg::Done {
                    lines: report.lines,
                    restored: restore,
                    dry_run,
                });
            })
            .ok();
    }

    /// 以管理员身份重启自身；成功后当前实例退出。
    pub fn relaunch_elevated(&mut self) {
        if self.elevated {
            self.toast("当前已经是管理员权限");
            return;
        }
        // 先让出单实例互斥体：提权副本是同一会话里的新进程，会看到我们持有的同名互斥体，
        // 把自己判成"第二个实例"→ 唤醒我们 → 直接退出。那样点了按钮却什么也没启动。
        crate::win::single::release();
        match opt::relaunch_as_admin() {
            opt::elevate::Relaunch::Launched => {
                crate::log_line("elevate", "已拉起提权副本，本实例退出");
                self.quit();
            }
            opt::elevate::Relaunch::Cancelled => {
                self.reacquire_instance();
                self.toast("已取消 UAC 提权");
            }
            opt::elevate::Relaunch::Failed(e) => {
                self.reacquire_instance();
                self.toast(format!("提权失败：{e}"));
            }
        }
    }

    /// 提权被取消时把单实例锁抢回来（否则可以再开第二个实例）。
    fn reacquire_instance(&self) {
        if !crate::win::single::acquire(crate::INSTANCE_KEY) {
            crate::log_line("elevate", "提权取消后重新抢占单实例锁失败");
        }
    }

    fn poll_opt(&mut self) {
        let Some(rx) = self.opt_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(OptMsg::Done {
                lines,
                restored,
                dry_run,
            }) => {
                self.opt_running = false;
                for line in &lines {
                    crate::log_line("opt", line);
                }
                self.opt_log = lines;
                if !dry_run {
                    self.opt_restored = restored;
                    self.tray.set_optimized(!restored);
                    let summary = self
                        .opt_log
                        .last()
                        .cloned()
                        .unwrap_or_else(|| "完成".to_string());
                    if let Ok(mut s) = self.services.shared.lock() {
                        s.push_log(
                            Level::Action,
                            format!(
                                "Windows 优化{}：{summary}",
                                if restored { "还原" } else { "执行" }
                            ),
                            self.settings.log_limit,
                        );
                    }
                    if self.settings.balloon {
                        self.tray.balloon(&format!(
                            "Windows 优化{}：{summary}",
                            if restored { "还原" } else { "完成" }
                        ));
                    }
                    self.toast(summary);
                } else {
                    self.toast("预演完成（未修改系统）");
                }
            }
            Err(TryRecvError::Empty) => {
                // 还在跑，把接收端放回去
                self.opt_rx = Some(rx);
            }
            Err(TryRecvError::Disconnected) => {
                self.opt_running = false;
                self.opt_log.push("优化线程意外结束".to_string());
            }
        }
    }

    // ---------------------------------------------------------------- 托盘

    /// 收进托盘（窗口最小化，事件循环继续跑）。
    pub fn hide_to_tray(&mut self) {
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        self.toast("已最小化；程序在托盘里继续运行，双击图标恢复");
    }

    /// 真正退出：先落盘，再让托盘线程删图标并退出消息循环。
    pub fn quit(&mut self) {
        self.quitting = true;
        self.sync_cfg();
        self.tray.exit();
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub fn set_autostart(&mut self, on: bool) {
        let ok = autostart::set(on);
        self.autostart_on = on && ok;
        self.settings.autostart = self.autostart_on;
        self.tray.set_autostart(self.autostart_on);
        self.toast(if ok {
            format!("开机自启已{}", if on { "开启" } else { "关闭" })
        } else {
            "写入注册表失败（可能被安全软件拦截）".into()
        });
        self.dirty = true;
    }

    fn show_window(&self) {
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn handle_tray(&mut self) {
        while let Ok(cmd) = self.tray_rx.try_recv() {
            match cmd {
                TrayCmd::ShowMain => {
                    self.tab = Tab::Detect;
                    self.show_window();
                }
                TrayCmd::ShowWinOpt => {
                    self.tab = Tab::WinOpt;
                    self.show_window();
                }
                TrayCmd::ShowSettings => {
                    self.tab = Tab::Settings;
                    self.show_window();
                }
                TrayCmd::ToggleAutostart => {
                    let next = !self.autostart_on;
                    self.set_autostart(next);
                }
                TrayCmd::TestBalloon => {
                    self.tray.balloon("这是一条来自托盘的气泡通知");
                    self.toast("已发送测试气泡");
                }
                TrayCmd::Exit => self.quit(),
            }
        }
    }

    fn handle_notices(&mut self) {
        while let Ok(n) = self.services.notices.try_recv() {
            match n {
                UiNotice::GameStarted { pid, exe } => {
                    self.tray.set_tooltip(&format!("游戏中：{exe}"));
                    if self.settings.opt.auto_on_game_start {
                        self.services.snapshot_log(
                            Level::Action,
                            format!("识别到 {exe}（PID {pid}），自动执行 Windows 优化"),
                        );
                        self.start_opt(false, false);
                    }
                }
                UiNotice::GameEnded => {
                    self.tray.set_tooltip("");
                    if self.settings.opt.auto_restore_on_exit && !self.opt_restored {
                        self.start_opt(false, true);
                    }
                }
            }
        }
        while self.awake_rx.try_recv().is_ok() {
            self.show_window();
            self.toast("已有实例在运行，已为你切回窗口");
        }
    }

    // ---------------------------------------------------------------- 布局

    fn header(&mut self, ui: &mut egui::Ui) {
        use shadcn_egui::components::badge::BadgeVariant;
        egui::Panel::top("hdr")
            .frame(
                egui::Frame::NONE
                    .fill(ui::role_of(ui, shadcn_egui::style::Role::Card))
                    .inner_margin(egui::Margin::symmetric(14, 10)),
            )
            .show(ui, |ui| {
                shadcn_egui::layout::space_between(
                    ui,
                    |ui| {
                        ui.label(
                            egui::RichText::new("GamePP-Lite")
                                .size(18.0)
                                .strong()
                                .color(ui::role_of(ui, shadcn_egui::style::Role::Foreground)),
                        );
                        ui.label(
                            egui::RichText::new("游戏识别 · Windows 优化")
                                .size(11.0)
                                .color(ui::muted(ui)),
                        );
                    },
                    |ui| {
                        let (state_text, variant) = {
                            let s =
                                self.services.shared.lock().unwrap_or_else(|e| e.into_inner());
                            match &s.detect {
                                DetectState::InGame { name, .. } => {
                                    (format!("游戏中：{name}"), BadgeVariant::Default)
                                }
                                DetectState::Candidate { exe, .. } => {
                                    (format!("候选中：{exe}"), BadgeVariant::Secondary)
                                }
                                DetectState::Idle => ("未识别到游戏".to_string(), BadgeVariant::Outline),
                            }
                        };
                        ui::chip(ui, &state_text, variant);
                        if self.elevated {
                            ui::chip(ui, "管理员", BadgeVariant::Secondary);
                        }
                        if self.services.shared.lock().map(|s| s.running).unwrap_or(false) {
                            ui::chip(ui, "服务运行中", BadgeVariant::Secondary);
                        }
                    },
                );
            });
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        use shadcn_egui::components::button::{Button, ButtonVariant};
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(180.0)
            .frame(
                egui::Frame::NONE
                    .fill(ui::role_of(ui, shadcn_egui::style::Role::Card))
                    .inner_margin(egui::Margin::symmetric(10, 12)),
            )
            .show(ui, |ui| {
                for tab in [Tab::Detect, Tab::WinOpt, Tab::Settings] {
                    let selected = self.tab == tab;
                    let btn = Button::new(tab.label())
                        .variant(if selected {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        })
                        .expanded(true);
                    if ui.add(btn).on_hover_text(tab.hint()).clicked() {
                        self.tab = tab;
                    }
                    ui.add_space(2.0);
                }
            });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::NONE
                    .fill(ui::role_of(ui, shadcn_egui::style::Role::Card))
                    .inner_margin(egui::Margin::symmetric(14, 6)),
            )
            .show(ui, |ui| {
                shadcn_egui::layout::space_between(
                    ui,
                    |ui| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64())
                            .unwrap_or(0.0);
                        let (text, color) = match &self.status {
                            Some((t, until)) if *until > now => {
                                (t.clone(), ui::muted(ui))
                            }
                            _ => {
                                let s = self
                                    .services
                                    .shared
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                match s.log.back() {
                                    Some(l) => (
                                        format!("[{}] {}", ui::clock(l.epoch), l.text),
                                        match l.level {
                                            Level::Warn => ui::warn(ui),
                                            Level::Action => ui::good(ui),
                                            Level::Info => ui::muted(ui),
                                        },
                                    ),
                                    None => ("就绪".to_string(), ui::muted(ui)),
                                }
                            }
                        };
                        ui.label(egui::RichText::new(text).size(11.0).color(color));
                    },
                    |ui| {
                        let s = self.services.shared.lock().unwrap_or_else(|e| e.into_inner());
                        ui.label(
                            egui::RichText::new(format!("轮询 {} 次", s.polls))
                                .size(11.0)
                                .color(ui::muted(ui)),
                        );
                    },
                );
            });
    }

    /// 把 ColorImage 写成 BMP、以及最小化存活探针都在这里（开发期手段）。
    fn dev_hooks(&mut self, ctx: &egui::Context) {
        if std::env::var("GPP_TRAY_TOGGLE").is_ok() {
            self.probe_frame += 1;
            if self.probe_frame == 8 {
                self.tray.set_optimized(true);
            }
            if self.probe_frame == 16 {
                self.tray.set_optimized(false);
            }
        }

        if std::env::var("GPP_PROBE").is_ok() {
            self.probe_frame += 1;
            if self.probe_frame == 5 {
                self.hide_to_tray();
                crate::log_line("probe", "第 5 帧：已最小化到托盘态");
            }
            if self.probe_frame == 40 {
                crate::log_line("probe", "第 40 帧：最小化后事件循环仍在运行");
            }
        }

        let Ok(path) = std::env::var("GPP_SHOT") else {
            return;
        };
        self.shot_frame += 1;
        if self.shot_frame == 6 {
            // egui 0.37：截图改为 ScreenshotCallback（回调直接拿到 ColorImage）。
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                egui::ScreenshotCallback::new(move |img| save_bmp(&path, &img)),
            ));
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 窗口隐藏时（收进托盘）eframe 不跑 UI pass，但 logic 仍会被调用：
        // 托盘命令、后台消息、配置同步都放在这里。
        self.handle_tray();
        self.handle_notices();
        self.poll_opt();
        if self.dirty {
            self.sync_cfg();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 关闭按钮 → 收进托盘（等价 Electron 的 close to tray）。
        if ctx.input(|i| i.viewport().close_requested())
            && self.settings.close_to_tray
            && !self.quitting
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.toast("已最小化；程序仍在托盘里运行，双击图标可恢复");
        }

        self.header(ui);
        self.nav(ui);
        self.footer(ui);

        egui::CentralPanel::default_margins()
            .frame(
                egui::Frame::NONE
                    .fill(ui::role_of(ui, shadcn_egui::style::Role::Background))
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                // 滚动条常驻：否则它会晚于布局出现，页面宽度先宽后窄导致卡片溢出窗口。
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| match self.tab {
                        Tab::Detect => ui::detect::show(ui, self),
                        Tab::WinOpt => ui::winopt::show(ui, self),
                        Tab::Settings => ui::settings::show(ui, self),
                    });
            });

        self.dev_hooks(&ctx);
        ctx.request_repaint_after(Duration::from_millis(600));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.quitting = true;
        let _ = self.settings.save();
        self.library.save();
        self.tray.exit();
    }
}

/// 初始页签（可用 GPP_TAB 指定，便于开发期逐页核验排版）。
fn initial_tab() -> Tab {
    match std::env::var("GPP_TAB").ok().and_then(|v| v.parse::<u8>().ok()) {
        Some(2) => Tab::WinOpt,
        Some(3) => Tab::Settings,
        _ => Tab::Detect,
    }
}

/// 安装中文字体，返回以 CJK 打底的字体定义（供 `Theme::apply_with_fonts` 用）。
///
/// egui 自带字体只有拉丁字形，中文会渲染成「□□□」。这里直接读 Windows 系统字体
/// （不打包字体文件，避免仓库体积与授权问题），按可用性依次尝试微软雅黑 / 等线 /
/// 黑体 / 宋体，并把它放在字体表首位，让中英文使用同一套字形。
fn install_cjk_font(_ctx: &egui::Context) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    for name in ["msyh.ttc", "Deng.ttf", "simhei.ttf", "simsun.ttc", "msyhl.ttc"] {
        let path = std::path::Path::new(&windir).join("Fonts").join(name);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let mut data = egui::FontData::from_owned(bytes);
        // .ttc 是字体集合，取第 0 个字面（Regular）。
        data.index = 0;
        fonts
            .font_data
            .insert("cjk".to_owned(), std::sync::Arc::new(data));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "cjk".to_owned());
        }
        break;
    }
    fonts
}

/// 把 ColorImage 写成 24 位 BMP（不引图片编码库）。
fn save_bmp(path: &str, img: &egui::ColorImage) {
    let (w, h) = (img.size[0], img.size[1]);
    if w == 0 || h == 0 {
        return;
    }
    let row = (w * 3).div_ceil(4) * 4;
    let data_size = row * h;
    let mut out = Vec::with_capacity(54 + data_size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + data_size) as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(data_size as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..h).rev() {
        let mut line = Vec::with_capacity(row);
        for x in 0..w {
            let c = img.pixels[y * w + x];
            line.extend_from_slice(&[c.b(), c.g(), c.r()]);
        }
        line.resize(row, 0);
        out.extend_from_slice(&line);
    }
    let _ = std::fs::write(path, out);
}

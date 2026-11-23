//! ③ 设置：识别节拍、数据与保留、托盘常驻、关于（原独立页已并入这里）。
//!
//! 定稿布局（草图 ③ A+B+D+E 单栏）：
//! - 五张卡整宽纵向堆叠（E：窄窗口下更稳）；
//! - 滑杆字段标签在左、当前值贴右缘（A）；
//! - 「恢复默认设置」「退出程序」点击后弹确认 Dialog（D），防误触。

use crate::app::App;
use crate::domain::protect;
use crate::domain::settings;
use crate::ui;
use eframe::egui;
use eframe::egui::RichText;
use shadcn_egui::components::button::{Button, ButtonVariant};
use shadcn_egui::components::dialog::{self, Dialog};

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_title(ui, "设置", crate::app::Tab::Settings.hint());
    // 页面可用宽度（Card 体内的 Ui 宽度无界，滑杆宽度从这里推）。
    let page_w = ui.available_width();

    let mut changed = false;
    let mut req_reload = false;
    let mut req_save = false;
    let mut req_hide = false;

    // ---------------- 识别节拍（A：标签左、值右对齐） ----------------
    ui::card(ui, "识别节拍", |ui| {
        let mut poll = app.settings.poll_ms as f32;
        if ui::slider_field(
            ui,
            "前台窗口轮询间隔 (ms)",
            &mut poll,
            200.0..=5000.0,
            100.0,
            |v| format!("{v:.0} ms"),
            page_w,
        ) {
            app.settings.poll_ms = poll as u64;
            changed = true;
        }
        let mut stable = app.settings.stable_polls as f32;
        if ui::slider_field(ui, "判定稳定轮数", &mut stable, 1.0..=6.0, 1.0, |v| format!("{v:.0}"), page_w) {
            app.settings.stable_polls = stable as u32;
            changed = true;
        }
        ui::hint(
            ui,
            "轮询越密越及时，但会多占一点 CPU；稳定轮数是防 alt-tab 抖动的关键。",
        );
    });

    ui.add_space(10.0);

    // ---------------- 数据与保留 ----------------
    ui::card(ui, "数据与保留", |ui| {
        let mut hist = app.settings.history_limit as f32;
        if ui::slider_field(ui, "会话历史条数", &mut hist, 5.0..=200.0, 5.0, |v| format!("{v:.0}"), page_w) {
            app.settings.history_limit = hist as usize;
            changed = true;
        }
        let mut log = app.settings.log_limit as f32;
        if ui::slider_field(ui, "日志条数", &mut log, 50.0..=2000.0, 50.0, |v| format!("{v:.0}"), page_w) {
            app.settings.log_limit = log as usize;
            changed = true;
        }
        ui.add_space(4.0);
        ui::kv(ui, "配置目录", &settings::data_dir().display().to_string());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add(Button::new("重新加载").variant(ButtonVariant::Outline))
                .clicked()
            {
                req_reload = true;
            }
            // D：点击后弹确认 Dialog。
            if ui
                .add(Button::new("恢复默认设置").variant(ButtonVariant::Destructive))
                .clicked()
            {
                app.confirm_reset = true;
            }
            if ui.add(Button::new("保存")).clicked() {
                req_save = true;
            }
        });
    });

    ui.add_space(10.0);

    // ---------------- 受保护白名单（只读） ----------------
    ui::card(ui, "受保护白名单", |ui| {
        let (names, services, paths) = protect::counts();
        ui::hint(
            ui,
            "扫描、优化与清理全部先过这张表：命中的进程/服务直接跳过，不改优先级、不结束、不裁剪工作集、不停止。",
        );
        ui.add_space(4.0);
        ui::kv(
            ui,
            "覆盖范围",
            &format!("系统组件 {names} 条 · 受保护服务 {services} 个 · 目录规则 {paths} 条"),
        );
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("protect_list")
            .max_height(170.0)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                for g in protect::PROCESS_GROUPS {
                    ui.label(
                        RichText::new(format!("{} · {}", g.label, g.items.len()))
                            .size(11.0)
                            .color(ui::role_of(ui, shadcn_egui::style::Role::CardForeground)),
                    );
                    ui.label(
                        RichText::new(g.items.join("、"))
                            .size(10.0)
                            .color(ui::muted(ui)),
                    );
                    ui.add_space(4.0);
                }
                ui.label(
                    RichText::new("受保护服务")
                        .size(11.0)
                        .color(ui::role_of(ui, shadcn_egui::style::Role::CardForeground)),
                );
                for g in protect::SERVICE_GROUPS {
                    ui.label(
                        RichText::new(format!("{}：{}", g.label, g.items.join("、")))
                            .size(10.0)
                            .color(ui::muted(ui)),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new("目录规则（路径前缀）")
                        .size(11.0)
                        .color(ui::role_of(ui, shadcn_egui::style::Role::CardForeground)),
                );
                for (root, label) in protect::path_roots() {
                    ui.label(
                        RichText::new(format!("{label}：{root}"))
                            .size(10.0)
                            .color(ui::muted(ui)),
                    );
                }
            });
    });

    ui.add_space(10.0);

    // ---------------- 托盘与常驻 ----------------
    ui::card(ui, "托盘与常驻", |ui| {
        let mut close_tray = app.settings.close_to_tray;
        if ui::check_row(ui, &mut close_tray, "关闭窗口时收进托盘（不退出）") {
            app.settings.close_to_tray = close_tray;
            changed = true;
        }
        let mut balloon = app.settings.balloon;
        if ui::check_row(ui, &mut balloon, "允许托盘气泡通知") {
            app.settings.balloon = balloon;
            changed = true;
        }
        let mut boot = app.autostart_on;
        if ui::check_row(ui, &mut boot, "开机自启（HKCU\\...\\Run）") {
            app.set_autostart(boot);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(Button::new("立即最小化").variant(ButtonVariant::Outline))
                .clicked()
            {
                req_hide = true;
            }
            // D：点击后弹确认 Dialog。
            if ui
                .add(Button::new("退出程序").variant(ButtonVariant::Destructive))
                .clicked()
            {
                app.confirm_quit = true;
            }
        });
    });

    ui.add_space(10.0);

    // ---------------- 关于本程序 ----------------
    ui::card(ui, "关于本程序", |ui| {
        ui::kv(ui, "版本", env!("CARGO_PKG_VERSION"));
        ui::kv(ui, "识别模块", "GPPPlatform.exe");
        ui::kv(ui, "优化模块", "gameppoptimizesdk");
        ui::kv(ui, "管理员权限", if app.elevated { "已获得" } else { "未获得" });
    });

    // ---------------- 危险操作确认弹窗（D） ----------------
    let ctx = ui.ctx().clone();

    if app.confirm_reset {
        let mut do_reset = false;
        Dialog::new("confirm_reset", "恢复默认设置")
            .description("将把识别节拍、保留策略、托盘与自启等全部设置重置为默认值。识别历史与游戏库不受影响。此操作不可撤销。")
            .show(&ctx, &mut app.confirm_reset, |ui| {
                dialog::footer(ui, |ui| {
                    if ui
                        .add(Button::new("取消").variant(ButtonVariant::Ghost))
                        .clicked()
                    {
                        // 仅关闭弹窗。
                    }
                    if ui.add(Button::new("确认恢复")).clicked() {
                        do_reset = true;
                    }
                });
            });
        if do_reset {
            app.settings = settings::Settings::default();
            // 开机自启的真实状态在注册表里，恢复默认设置不会（也不该）去改它；
            // 把内存中的字段对齐到注册表，否则下次保存会落一份与实际不符的配置。
            app.settings.autostart = app.autostart_on;
            changed = true;
            app.toast("已恢复默认设置");
        }
    }

    if app.confirm_quit {
        let mut do_quit = false;
        Dialog::new("confirm_quit", "退出程序")
            .description("确定退出 GamePP-Lite？托盘里的监控也会停止。退出前会保存当前设置。")
            .show(&ctx, &mut app.confirm_quit, |ui| {
                dialog::footer(ui, |ui| {
                    if ui
                        .add(Button::new("取消").variant(ButtonVariant::Ghost))
                        .clicked()
                    {
                        // 仅关闭弹窗。
                    }
                    if ui
                        .add(Button::new("退出程序").variant(ButtonVariant::Destructive))
                        .clicked()
                    {
                        do_quit = true;
                    }
                });
            });
        if do_quit {
            app.quit();
        }
    }

    if changed {
        app.dirty = true;
    }
    if req_reload {
        app.reload_all();
    }
    if req_save {
        app.sync_cfg();
        app.toast("设置已保存");
    }
    if req_hide {
        app.hide_to_tray();
    }
}

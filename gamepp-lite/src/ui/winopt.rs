//! ② Windows 优化（由 `gamepp-winopt` 合并进来）。
//!
//! 左侧是可勾选的优化项，右侧是高级项，底部是执行日志。
//! 所有动作都需要管理员权限；没有提权时页面顶部给出一键"以管理员身份重启"。

use crate::app::App;
use crate::ui;
use eframe::egui;
use eframe::egui::RichText;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_title(ui, "Windows 优化", "");

    // ---------------- 权限 & 状态 ----------------
    // 状态取自 app（改动时同步），不要每帧读 opt-state.ini。
    let restored = app.opt_restored;
    ui::card(ui, "运行状态", |ui| {
        ui.horizontal_wrapped(|ui| {
            if app.elevated {
                ui::chip(ui, "已获得管理员权限", ui::GOOD);
            } else {
                ui::chip(ui, "未提权", ui::BAD);
            }
            ui::chip(
                ui,
                if restored {
                    "当前处于「已还原」状态"
                } else {
                    "当前处于「已优化」状态"
                },
                if restored { ui::MUTED } else { ui::WARN },
            );
        });
        if !app.elevated {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("停服务 / 改别人的进程优先级必须提权，否则只会记录失败原因。")
                        .size(11.0)
                        .color(ui::WARN),
                );
                if ui.button("以管理员身份重启").clicked() {
                    app.relaunch_elevated();
                }
            });
        }
    });

    // ---------------- 操作按钮 ----------------
    ui.add_space(8.0);
    ui::card(ui, "操作", |ui| {
        ui.horizontal(|ui| {
            let enabled = !app.opt_running;
            if ui
                .add_enabled(enabled, egui::Button::new("预演（不改系统）"))
                .on_hover_text("逐条打印将要执行的动作，不做任何修改")
                .clicked()
            {
                app.start_opt(true, false);
            }
            if ui
                .add_enabled(enabled, egui::Button::new("执行优化"))
                .clicked()
            {
                app.start_opt(false, false);
            }
            // 只有处于「已优化」状态才有东西可还原；`restored` 表示"当前已还原"。
            if ui
                .add_enabled(enabled && !restored, egui::Button::new("还原"))
                .on_hover_text("重启自己停过的服务、重启被结束的 Wallpaper Engine、优先级复位、电源方案还原")
                .clicked()
            {
                app.start_opt(false, true);
            }
            if app.opt_running {
                ui.spinner();
                ui.label(RichText::new("执行中…").size(11.0).color(ui::MUTED));
            }
        });
        if app.settings.opt.has_destructive() {
            ui.add_space(4.0);
            ui.label(
                RichText::new("注意：勾选项里包含「结束进程 / 清空剪贴板」，这些动作不可撤销（Wallpaper Engine 会在还原时重新启动）。")
                    .size(11.0)
                    .color(ui::BAD),
            );
        }
    });

    // ---------------- 优化项 ----------------
    ui.add_space(8.0);
    let mut changed = false;
    ui.columns(2, |cols| {
        ui::card(&mut cols[0], "常用项", |ui| {
            let p = &mut app.settings.opt;
            changed |= ui
                .checkbox(&mut p.game_priority, "调整游戏进程优先级（ABOVE_NORMAL）")
                .changed();
            changed |= ui
                .checkbox(&mut p.kill_background, "关闭影响流畅性的后台进程（48 条黑名单）")
                .changed();
            changed |= ui
                .checkbox(&mut p.power_high_perf, "电源方案改为高性能")
                .changed();
            changed |= ui
                .checkbox(&mut p.free_memory, "释放电脑内存（EmptyWorkingSet）")
                .changed();
            changed |= ui
                .checkbox(&mut p.wallpaper_engine, "关闭 Wallpaper Engine")
                .changed();
            changed |= ui
                .checkbox(&mut p.xbl_auth, "停止 Xbox Live 身份验证管理器")
                .changed();
            changed |= ui
                .checkbox(&mut p.windows_search, "停止 Windows Search 服务")
                .changed();
            changed |= ui.checkbox(&mut p.flush_dns, "清理 DNS 缓存").changed();
            changed |= ui
                .checkbox(&mut p.clean_clipboard, "清空剪贴板内容")
                .changed();
        });

        ui::card(&mut cols[1], "高级项", |ui| {
            let p = &mut app.settings.opt;
            changed |= ui
                .checkbox(&mut p.windows_update, "停止 Windows Update（wuauserv）")
                .changed();
            changed |= ui
                .checkbox(&mut p.diag_track, "停止 DiagTrack（客户体验改善）")
                .changed();
            changed |= ui
                .checkbox(&mut p.diag_svc, "停止 Diagsvc")
                .changed();
            changed |= ui
                .checkbox(&mut p.homegroup, "停止家庭组服务（HomeGroup ×2）")
                .changed();
        });
    });

    // ---------------- 自动化 ----------------
    ui.add_space(8.0);
    ui::card(ui, "自动化", |ui| {
        let p = &mut app.settings.opt;
        changed |= ui
            .checkbox(&mut p.auto_on_game_start, "识别到游戏后自动执行上面的优化")
            .changed();
        changed |= ui
            .checkbox(&mut p.auto_restore_on_exit, "游戏退出后自动还原")
            .changed();
        ui::hint(
            ui,
            "自动执行同样要求管理员权限；未提权时只会在日志里记一条「已跳过」，不会静默失败。",
        );
    });
    if changed {
        app.dirty = true;
    }

    // ---------------- 日志 ----------------
    ui.add_space(8.0);
    ui::card(ui, "执行日志", |ui| {
        if app.opt_log.is_empty() {
            ui.label(
                RichText::new("还没有执行记录。建议先点「预演」看一眼要做什么。")
                    .size(11.0)
                    .color(ui::MUTED),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("optlog")
            .max_height(220.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for line in &app.opt_log {
                    let color = if line.contains("失败") || line.contains("权限") {
                        ui::BAD
                    } else if line.starts_with("完成") || line.starts_with("还原完成") {
                        ui::GOOD
                    } else {
                        ui::MUTED
                    };
                    ui.label(RichText::new(line).size(11.0).color(color));
                }
            });
    });
}

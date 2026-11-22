//! ② Windows 优化（由 `gamepp-winopt` 合并进来）。
//!
//! 定稿布局（草图 ② B+C+D+E + 黄金分割）：
//! - 「运行状态」整宽卡：徽章在左、操作按钮靠右，未提权时给一键提权；
//! - 破坏性提醒用 shadcn Alert（E：仅勾选不可逆项时渲染）；
//! - 第二行黄金分割：左「优化项」61.8%（B：常用项/高级项/自动化合并一卡，
//!   D：sm 紧凑行距 + 全选/清空），右「执行日志」38.2%（C：与优化项同行等高，
//!   执行中自动滚到底）。
//! 所有动作都需要管理员权限；没有提权时页面顶部给出一键"以管理员身份重启"。

use crate::app::App;
use crate::ui;
use eframe::egui;
use eframe::egui::RichText;
use shadcn_egui::components::alert::{Alert, AlertVariant};
use shadcn_egui::components::badge::BadgeVariant;
use shadcn_egui::components::button::{Button, ButtonVariant};
use shadcn_egui::components::scroll_area::ScrollArea;
use shadcn_egui::components::separator::Separator;
use shadcn_egui::icons::lucide;
use shadcn_egui::style::Role;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_title(ui, "Windows 优化", "");
    // 页面可用宽度（Card 体内的 Ui 宽度无界，卡内分栏一律从这里推）。
    let page_w = ui.available_width();

    let restored = app.opt_restored;
    let enabled = !app.opt_running;
    let mut req_dry = false;
    let mut req_exec = false;
    let mut req_restore = false;
    let mut req_relaunch = false;
    let mut changed = false;

    // ---------------- 运行状态（整宽卡：徽章左、操作按钮右） ----------------
    ui::card(ui, "运行状态", |ui| {
        ui.horizontal(|ui| {
            if app.elevated {
                ui::chip(ui, "已获得管理员权限", BadgeVariant::Secondary);
            } else {
                ui::chip(ui, "未提权", BadgeVariant::Destructive);
            }
            ui::chip(
                ui,
                if restored {
                    "当前处于「已还原」状态"
                } else {
                    "当前处于「已优化」状态"
                },
                if restored {
                    BadgeVariant::Outline
                } else {
                    BadgeVariant::Secondary
                },
            );
            // 操作：按钮固定靠卡片右缘。
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if app.opt_running {
                    ui.spinner();
                    ui.label(RichText::new("执行中…").size(11.0).color(ui::muted(ui)));
                }
                // 只有处于「已优化」状态才有东西可还原；`restored` 表示"当前已还原"。
                if ui
                    .add_enabled(
                        enabled && !restored,
                        Button::new("还原").variant(ButtonVariant::Outline),
                    )
                    .on_hover_text("重启自己停过的服务、重启被结束的 Wallpaper Engine、优先级复位、电源方案还原")
                    .clicked()
                {
                    req_restore = true;
                }
                if ui
                    .add_enabled(enabled, Button::new("执行优化"))
                    .clicked()
                {
                    req_exec = true;
                }
                if ui
                    .add_enabled(
                        enabled,
                        Button::new("预演（不改系统）").variant(ButtonVariant::Ghost),
                    )
                    .on_hover_text("逐条打印将要执行的动作，不做任何修改")
                    .clicked()
                {
                    req_dry = true;
                }
            });
        });
        if !app.elevated {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("停服务 / 改别人的进程优先级必须提权，否则只会记录失败原因。")
                        .size(11.0)
                        .color(ui::warn(ui)),
                );
                if ui
                    .add(Button::new("以管理员身份重启").variant(ButtonVariant::Outline))
                    .clicked()
                {
                    req_relaunch = true;
                }
            });
        }
    });

    // ---------------- 破坏性提醒（E：仅勾选不可逆项时渲染） ----------------
    if app.settings.opt.has_destructive() {
        ui.add_space(8.0);
        ui.add(
            Alert::new()
                .variant(AlertVariant::Destructive)
                .icon(lucide::CIRCLE_ALERT)
                .title("注意：不可撤销操作")
                .description("勾选项里包含「结束进程 / 清空剪贴板」，这些动作不可撤销（Wallpaper Engine 会在还原时重新启动）。"),
        );
    }

    // ---------------- 第二行：优化项（61.8%）｜ 执行日志（38.2%） ----------------
    ui.add_space(8.0);
    ui::split_row(ui, "winopt_row", 0.618, 10.0, app, |ui, h, app| {
        ui::card(ui, "优化项", |ui| {
            ui::stretch_body(ui, h, 240.0);
            // B：常用项与高级项/自动化合并一卡；卡内左右两栏，中间分隔线。
            ui.horizontal(|ui| {
                let total = page_w - 30.0;
                let col_w = ((total - 30.0) * 0.5).floor().max(160.0);
                ui::column(ui, col_w, |ui| {
                    ui.label(
                        RichText::new("常用项").size(11.0).color(ui::muted(ui)),
                    );
                    ui.add_space(2.0);
                    let p = &mut app.settings.opt;
                    changed |= ui::check_row(ui, &mut p.game_priority, "调整游戏进程优先级（ABOVE_NORMAL）");
                    changed |= ui::check_row(ui, &mut p.kill_background, "关闭影响流畅性的后台进程（48 条黑名单）");
                    changed |= ui::check_row(ui, &mut p.power_high_perf, "电源方案改为高性能");
                    changed |= ui::check_row(ui, &mut p.free_memory, "释放电脑内存（EmptyWorkingSet）");
                    changed |= ui::check_row(ui, &mut p.wallpaper_engine, "关闭 Wallpaper Engine");
                    changed |= ui::check_row(ui, &mut p.xbl_auth, "停止 Xbox Live 身份验证管理器");
                    changed |= ui::check_row(ui, &mut p.windows_search, "停止 Windows Search 服务");
                    changed |= ui::check_row(ui, &mut p.flush_dns, "清理 DNS 缓存");
                    changed |= ui::check_row(ui, &mut p.clean_clipboard, "清空剪贴板内容");
                    ui.add_space(6.0);
                    // D：每组末尾「全选 / 清空」Ghost 小按钮。
                    ui.horizontal(|ui| {
                        let on = ui::ghost_sm(ui, "全选").clicked();
                        let off = ui::ghost_sm(ui, "清空").clicked();
                        if on || off {
                            let p = &mut app.settings.opt;
                            p.game_priority = on;
                            p.kill_background = on;
                            p.power_high_perf = on;
                            p.free_memory = on;
                            p.wallpaper_engine = on;
                            p.xbl_auth = on;
                            p.windows_search = on;
                            p.flush_dns = on;
                            p.clean_clipboard = on;
                            changed = true;
                        }
                    });
                });
                ui.add(Separator::new().vertical());
                ui.add_space(10.0);
                ui::column(ui, (total - col_w - 30.0).max(160.0), |ui| {
                    ui.label(
                        RichText::new("高级项").size(11.0).color(ui::muted(ui)),
                    );
                    ui.add_space(2.0);
                    let p = &mut app.settings.opt;
                    changed |= ui::check_row(ui, &mut p.windows_update, "停止 Windows Update（wuauserv）");
                    changed |= ui::check_row(ui, &mut p.diag_track, "停止 DiagTrack（客户体验改善）");
                    changed |= ui::check_row(ui, &mut p.diag_svc, "停止 Diagsvc");
                    changed |= ui::check_row(ui, &mut p.homegroup, "停止家庭组服务（HomeGroup ×2）");
                    ui.add_space(4.0);
                    ui::hint(
                        ui,
                        "自动执行同样要求管理员权限；未提权时只会在日志里记一条「已跳过」，不会静默失败。",
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("自动化").size(11.0).color(ui::muted(ui)),
                    );
                    let p = &mut app.settings.opt;
                    changed |= ui::check_row(ui, &mut p.auto_on_game_start, "识别到游戏后自动执行上面的优化");
                    changed |= ui::check_row(ui, &mut p.auto_restore_on_exit, "游戏退出后自动还原");
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let on = ui::ghost_sm(ui, "全选").clicked();
                        let off = ui::ghost_sm(ui, "清空").clicked();
                        if on || off {
                            let p = &mut app.settings.opt;
                            p.windows_update = on;
                            p.diag_track = on;
                            p.diag_svc = on;
                            p.homegroup = on;
                            p.auto_on_game_start = on;
                            p.auto_restore_on_exit = on;
                            changed = true;
                        }
                    });
                });
            });
        })
    }, |ui, h, app| {
        ui::card(ui, "执行日志", |ui| {
            ui::stretch_body(ui, h, 240.0);
            let max_h = ui::scroll_max(ui, h, 200.0);
            if app.opt_log.is_empty() {
                ui::hint(ui, "还没有执行记录。建议先点「预演」看一眼要做什么。");
                return;
            }
            // C：执行中 stick_to_bottom 自动滚到底。
            ScrollArea::vertical()
                .id_salt("optlog")
                .max_height(max_h)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &app.opt_log {
                        let color = if line.contains("失败") || line.contains("权限") {
                            ui::bad(ui)
                        } else if line.starts_with("完成") || line.starts_with("还原完成") {
                            ui::good(ui)
                        } else {
                            ui::muted(ui)
                        };
                        ui.label(RichText::new(line).size(11.0).color(color));
                    }
                });
            ui.add_space(2.0);
            ui.label(
                RichText::new(format!(
                    "{} 行 · 执行中自动滚到底",
                    app.opt_log.len()
                ))
                .size(10.0)
                .color(ui::role_of(ui, Role::MutedForeground)),
            );
        })
    });

    if changed {
        app.dirty = true;
    }
    if req_dry {
        app.start_opt(true, false);
    }
    if req_exec {
        app.start_opt(false, false);
    }
    if req_restore {
        app.start_opt(false, true);
    }
    if req_relaunch {
        app.relaunch_elevated();
    }
}

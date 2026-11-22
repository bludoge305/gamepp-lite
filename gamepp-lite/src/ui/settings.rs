//! ③ 设置：识别节拍、数据与保留、托盘常驻（原独立页已并入这里）。

use crate::app::App;
use crate::domain::settings;
use crate::ui::{self, kv};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui::page_title(ui, "设置", "");

    let mut changed = false;
    ui.columns(2, |cols| {
        // ---------------- 左栏 ----------------
        ui::card(&mut cols[0], "识别节拍", |ui| {
            let mut poll = app.settings.poll_ms as f64;
            if ui
                .add(
                    egui::Slider::new(&mut poll, 200.0..=5000.0)
                        .text("前台窗口轮询间隔 (ms)")
                        .step_by(100.0),
                )
                .changed()
            {
                app.settings.poll_ms = poll as u64;
                changed = true;
            }
            let mut stable = app.settings.stable_polls as f64;
            if ui
                .add(egui::Slider::new(&mut stable, 1.0..=6.0).text("判定稳定轮数"))
                .changed()
            {
                app.settings.stable_polls = stable as u32;
                changed = true;
            }
            ui::hint(
                ui,
                "轮询越密越及时，但会多占一点 CPU；稳定轮数是防 alt-tab 抖动的关键。",
            );
        });

        cols[0].add_space(8.0);
        ui::card(&mut cols[0], "数据与保留", |ui| {
            let mut hist = app.settings.history_limit as f64;
            if ui
                .add(egui::Slider::new(&mut hist, 5.0..=200.0).text("会话历史条数"))
                .changed()
            {
                app.settings.history_limit = hist as usize;
                changed = true;
            }
            let mut log = app.settings.log_limit as f64;
            if ui
                .add(egui::Slider::new(&mut log, 50.0..=2000.0).text("日志条数"))
                .changed()
            {
                app.settings.log_limit = log as usize;
                changed = true;
            }
            ui.add_space(4.0);
            kv(ui, "配置目录", &settings::data_dir().display().to_string());
            ui.horizontal(|ui| {
                if ui.button("重新加载").clicked() {
                    app.reload_all();
                }
                if ui.button("恢复默认设置").clicked() {
                    app.settings = settings::Settings::default();
                    changed = true;
                    app.toast("已恢复默认设置");
                }
                if ui.button("保存").clicked() {
                    app.sync_cfg();
                    app.toast("设置已保存");
                }
            });
        });

        // ---------------- 右栏 ----------------
        ui::card(&mut cols[1], "托盘与常驻", |ui| {
            let mut close_tray = app.settings.close_to_tray;
            if ui
                .checkbox(&mut close_tray, "关闭窗口时收进托盘（不退出）")
                .changed()
            {
                app.settings.close_to_tray = close_tray;
                changed = true;
            }
            let mut balloon = app.settings.balloon;
            if ui.checkbox(&mut balloon, "允许托盘气泡通知").changed() {
                app.settings.balloon = balloon;
                changed = true;
            }
            let mut boot = app.autostart_on;
            if ui
                .checkbox(&mut boot, "开机自启（HKCU\\...\\Run）")
                .changed()
            {
                app.set_autostart(boot);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("发送测试气泡").clicked() {
                    app.tray.balloon("这是一条来自托盘的气泡通知");
                    app.toast("已发送测试气泡");
                }
                if ui.button("立即最小化").clicked() {
                    app.hide_to_tray();
                }
                if ui.button("退出程序").clicked() {
                    app.quit();
                }
            });

        });

        cols[1].add_space(8.0);
        ui::card(&mut cols[1], "关于本程序", |ui| {
            kv(ui, "版本", env!("CARGO_PKG_VERSION"));
            kv(ui, "识别模块", "GPPPlatform.exe");
            kv(ui, "优化模块", "gameppoptimizesdk");
            kv(ui, "管理员权限", if app.elevated { "已获得" } else { "未获得" });

        });
    });

    if changed {
        app.dirty = true;
    }
}

//! ① 游戏识别：前台窗口 → 进程 → 游戏库/启发式 → 状态。
//!
//! 界面字段刻意对齐 `GPPPlatform.exe` 的日志行
//! `GetForegroundWindow hWnd = … processId = … process = …`：
//! 用户看到的应该和原生侧打的日志一致，出问题时才能对上。
//!
//! 定稿布局（草图 ① A+B+D+E + 黄金分割）：
//! - 「当前前台窗口」整宽卡，判定依据并入卡内右侧（D）；
//! - 第二行黄金分割：左「游戏库」61.8%，右「手工登记」38.2%，行内等高；
//!   命中行整行 Secondary 高亮（A，行内不再放「命中」Badge）；
//! - 第三行整宽「最近识别记录」，Grid 斑马纹 + 时长右对齐（E）。

use crate::app::App;
use crate::domain::games::Confidence;
use crate::services::{fmt_duration, now_epoch, DetectState};
use crate::ui;
use eframe::egui;
use eframe::egui::RichText;
use shadcn_egui::components::badge::BadgeVariant;
use shadcn_egui::components::button::{Button, ButtonVariant};
use shadcn_egui::components::separator::Separator;
use shadcn_egui::style::Role;

/// 游戏库列表的行间距（示例草图 ~30px 行距：行体 26 + 行间 4）。
const LIB_ROW_GAP: f32 = 4.0;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui.set_max_width(ui.available_width());
    ui::page_title(ui, "游戏识别", crate::app::Tab::Detect.hint());

    let (state_text, state_hit, fg, library_hit, heuristics, game_name, since) = {
        let s = app.services.shared.lock().unwrap_or_else(|e| e.into_inner());
        let (state_text, state_hit, name, since) = match &s.detect {
            DetectState::Idle => ("空闲（前台不是游戏）".to_string(), false, None, 0),
            DetectState::Candidate { exe, ticks, .. } => {
                (format!("候选中 · {exe} · 命中 {ticks} 次"), false, None, 0)
            }
            DetectState::InGame {
                name,
                since,
                confidence,
                ..
            } => (
                format!(
                    "已识别 · {}",
                    match confidence {
                        Confidence::Matched => "游戏库命中",
                        Confidence::Heuristic => "启发式命中",
                    }
                ),
                true,
                Some(name.clone()),
                *since,
            ),
        };
        let fg = s.foreground.clone();
        let hit = fg
            .as_ref()
            .and_then(|f| app.library.lookup(&f.exe))
            .map(|e| (e.name.clone(), e.tags.join(" / ")));
        // 优先级与图形运行时都是采样时取好的（见 win::foreground），这里只读快照。
        let heur = fg.as_ref().map(|f| f.runtimes.clone()).unwrap_or_default();
        (state_text, state_hit, fg, hit, heur, name, since)
    };

    // 游戏库快照：编辑/删除在遍历结束后统一落地，避免同时借用 app 的两块。
    let entries: Vec<(String, String, bool)> = app
        .library
        .entries
        .iter()
        .map(|e| (e.exe.clone(), e.name.clone(), e.custom))
        .collect();
    let mut add_req: Option<(String, String)> = None;
    let mut del_req: Option<String> = None;
    let mut edit_req: Option<(String, String)> = None;
    let mut do_commit = false;
    let mut do_cancel = false;
    let mut do_restore = false;

    // ---------- 第一行：当前前台窗口（D：判定依据并入卡内右侧） ----------
    {
        let title_badge = if state_hit {
            BadgeVariant::Default
        } else {
            BadgeVariant::Outline
        };
        ui::card_action(ui, "当前前台窗口", |ui| {
            ui::chip(
                ui,
                if state_hit { "已识别" } else { "空闲" },
                title_badge,
            );
        }, |ui| {
            // 卡体 Ui 有界，以实际可用宽度为准。行内横向间距清零，
            // 右栏（判定依据）吃剩余宽度：与卡右内缘精确对齐，不再溢出裁切。
            let total = ui.available_width();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let left_w = (total * 0.62).floor().max(200.0);
                ui::column(ui, left_w, |ui| {
                    match &fg {
                        Some(f) => {
                            // 键值分两列排（对齐示例），行高减半、卡体更紧凑。
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let half = (ui.available_width() * 0.5).floor().max(150.0);
                                ui::column(ui, half, |ui| {
                                    ui::kv(ui, "窗口句柄", &format!("0x{:X}", f.hwnd));
                                    ui::kv(ui, "映像名", &f.exe);
                                    ui::kv(ui, "全屏", if f.fullscreen { "是" } else { "否" });
                                    ui::kv(ui, "映像路径", &f.path);
                                });
                                ui.add_space(14.0);
                                ui::column(ui, ui.available_width().max(120.0), |ui| {
                                    ui::kv(ui, "进程 ID", &f.pid.to_string());
                                    ui::kv(
                                        ui,
                                        "窗口标题",
                                        if f.title.is_empty() { "—" } else { &f.title },
                                    );
                                    ui::kv(ui, "窗口类", if f.class.is_empty() { "—" } else { &f.class });
                                    let prio = f
                                        .priority
                                        .map(crate::win::process::priority_name)
                                        .unwrap_or("—");
                                    ui::kv(ui, "进程优先级", prio);
                                });
                            });
                        }
                        None => {
                            ui.label(
                                RichText::new("没有前台窗口").size(12.0).color(ui::muted(ui)),
                            );
                        }
                    }
                });
                ui.add(Separator::new().vertical());
                ui.add_space(14.0);
                ui::column(ui, ui.available_width().max(160.0), |ui| {
                    ui::hint(ui, "判定依据");
                    ui.horizontal_wrapped(|ui| {
                        ui::chip(ui, &state_text, if state_hit {
                            BadgeVariant::Default
                        } else {
                            BadgeVariant::Outline
                        });
                        if game_name.is_some() {
                            ui::chip(
                                ui,
                                &format!("已持续 {}", fmt_duration(now_epoch() - since)),
                                BadgeVariant::Secondary,
                            );
                        }
                    });
                    ui.add_space(4.0);
                    match &library_hit {
                        Some((name, tags)) => {
                            ui::kv(ui, "游戏库", &format!("{name}（{tags}）"))
                        }
                        None => ui::kv(ui, "游戏库", "未命中（exe 不在库中）"),
                    }
                    let runtime_text = if heuristics.is_empty() {
                        "未检测到 d3d9/11/12、vulkan".to_string()
                    } else {
                        heuristics.join("、")
                    };
                    ui::kv(ui, "图形运行时", &runtime_text);
                });
            });
        });
    }

    // ---------- 第二行：游戏库（61.8%）｜ 手工登记（38.2%） ----------
    ui.add_space(4.0);
    ui::split_row(ui, "detect_row", 0.618, 10.0, app, |ui, h, app| {
        ui::card_action(ui, "游戏库（本地）", |ui| {
            if ui::ghost_sm(ui, "恢复默认库")
                .on_hover_text("丢弃所有手工修改，恢复内置列表")
                .clicked()
            {
                do_restore = true;
            }
        }, |ui| {
            ui::stretch_body(ui, h, 170.0);
            ui.label(
                RichText::new(format!(
                    "共 {} 条 · 命中判据为映像名 · A：命中行整行高亮",
                    entries.len()
                ))
                .size(11.0)
                .color(ui::muted(ui)),
            );
            ui.add_space(4.0);
            let list_h = ui::scroll_max(ui, h, 130.0);
            egui::ScrollArea::vertical()
                .id_salt("lib")
                .max_height(list_h)
                .auto_shrink([false, false])
                // 悬浮式滚动条覆盖在内容上、不占宽度，按需显示即可。
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    // 紧凑行距（对齐示例 ~30px 行距）：行内上下留白 3、行间 4。
                    ui.spacing_mut().item_spacing.y = LIB_ROW_GAP;
                    for (exe, name, _custom) in &entries {
                        let hit = fg
                            .as_ref()
                            .map(|f| f.exe.eq_ignore_ascii_case(exe))
                            .unwrap_or(false);
                        let editing_this = app
                            .editing
                            .as_ref()
                            .map(|d| d.orig.eq_ignore_ascii_case(exe))
                            .unwrap_or(false);

                        if editing_this {
                            // ---- 行内编辑 ----
                            if let Some(draft) = app.editing.as_mut() {
                                ui.horizontal(|ui| {
                                    ui::input(ui, &mut draft.exe, "exe 名", 130.0);
                                    ui::input(ui, &mut draft.name, "显示名", 100.0);
                                });
                                ui.horizontal(|ui| {
                                    if ui
                                        .add(Button::new("保存").size(shadcn_egui::components::button::ButtonSize::Sm))
                                        .clicked()
                                    {
                                        do_commit = true;
                                    }
                                    if ui::ghost_sm(ui, "取消").clicked() {
                                        do_cancel = true;
                                    }
                                    ui::hint(ui, "改 exe 名可修正误添加的条目");
                                });
                            }
                        } else {
                            // 命中行整行 Secondary 底色（A），无行内 Badge。
                            let bg = if hit {
                                ui::role_of(ui, Role::Secondary)
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            egui::Frame::NONE
                                .fill(bg)
                                .corner_radius(6.0)
                                .inner_margin(egui::Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let btn_w = 104.0;
                                        let text_w =
                                            (ui.available_width() - btn_w - 6.0).max(56.0);
                                        let text = format!("{exe}  ·  {name}");
                                        let full = text.clone();
                                        ui::text_cell(
                                            ui,
                                            &text,
                                            text_w,
                                            11.0,
                                            ui::role_of(ui, Role::CardForeground),
                                        )
                                        .on_hover_text(full);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui::ghost_sm(ui, "删除").clicked() {
                                                    del_req = Some(exe.clone());
                                                }
                                                if ui::ghost_sm(ui, "编辑").clicked() {
                                                    edit_req = Some((exe.clone(), name.clone()));
                                                }
                                            },
                                        );
                                    });
                                });
                        }
                    }
                });
        })
    }, |ui, h, app| {
        ui::card(ui, "手工登记", |ui| {
            ui::stretch_body(ui, h, 150.0);
            ui::hint(ui, "启发式识别到但未登记的游戏，可在这里补一条；误加的条目可在上表编辑或删除。");
            ui.add_space(4.0);
            let w = ui.available_width().max(120.0);
            ui::input(ui, &mut app.new_custom_exe, "exe 名，如 MyGame.exe", w);
            ui::input(ui, &mut app.new_custom_name, "显示名", w);
            ui.horizontal_wrapped(|ui| {
                if ui.add(Button::new("加入库")).clicked() {
                    let exe = app.new_custom_exe.trim().to_string();
                    let name = app.new_custom_name.trim().to_string();
                    if !exe.is_empty() {
                        let name = if name.is_empty() { exe.clone() } else { name };
                        add_req = Some((exe, name));
                    }
                }
                if let Some(f) = &fg {
                    if ui
                        .add(Button::new("用当前前台进程填充").variant(ButtonVariant::Outline))
                        .clicked()
                    {
                        app.new_custom_exe = f.exe.clone();
                        app.new_custom_name = f.title.chars().take(20).collect();
                    }
                }
            });
        })
    });

    // ---------- 落地游戏库操作 ----------
    if let Some((exe, name)) = add_req {
        add_custom(&mut app.library, &exe, &name);
        app.new_custom_exe.clear();
        app.new_custom_name.clear();
        app.dirty = true;
        app.toast("已加入游戏库");
    }
    if let Some((exe, name)) = edit_req {
        app.begin_edit(&exe, &name);
    }
    if do_commit {
        app.commit_edit();
    }
    if do_cancel {
        app.editing = None;
    }
    if let Some(exe) = del_req {
        app.delete_entry(&exe);
    }
    if do_restore {
        app.restore_library();
    }

    // ---------- 第三行：历史（E：斑马纹 + 时长右对齐） ----------
    ui.add_space(4.0);
    ui::card(ui, "最近识别记录", |ui| {
        let history = app
            .services
            .shared
            .lock()
            .map(|s| s.history.iter().take(8).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if history.is_empty() {
            ui::hint(ui, "暂无历史会话");
            return;
        }
        egui::Grid::new("hist")
            .num_columns(4)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for h in ["游戏", "进程", "开始", "时长"] {
                    ui.label(RichText::new(h).size(10.0).color(ui::muted(ui)));
                }
                ui.end_row();
                for it in &history {
                    ui.add(egui::Label::new(RichText::new(&it.name).size(11.0)).truncate());
                    ui.add(
                        egui::Label::new(RichText::new(&it.exe).size(11.0).color(ui::muted(ui)))
                            .truncate(),
                    );
                    ui.label(RichText::new(ui::clock(it.started)).size(11.0));
                    // 时长列固定宽度、右对齐（E）。
                    ui::text_cell_align(
                        ui,
                        &fmt_duration(it.duration_secs),
                        70.0,
                        11.0,
                        ui::role_of(ui, Role::Foreground),
                        egui::Align::RIGHT,
                    );
                    ui.end_row();
                }
            });
    });
}

fn add_custom(lib: &mut crate::domain::games::GameLibrary, exe: &str, name: &str) {
    lib.add_custom(exe, name);
}

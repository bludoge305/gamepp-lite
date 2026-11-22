//! ① 游戏识别：前台窗口 → 进程 → 游戏库/启发式 → 状态。
//!
//! 界面字段刻意对齐 `GPPPlatform.exe` 的日志行
//! `GetForegroundWindow hWnd = … processId = … process = …`：
//! 用户看到的应该和原生侧打的日志一致，出问题时才能对上。
//!
//! 布局上"当前前台窗口"与"游戏库"放在**同一行**，两张卡片的顶边对齐；
//! 第二行放"判定依据"与"手工登记"，第三行是完整宽度的历史记录。

use crate::app::App;
use crate::domain::games::Confidence;
use crate::services::{fmt_duration, now_epoch, DetectState};
use crate::ui::{self, kv};
use eframe::egui;
use eframe::egui::RichText;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    ui.set_max_width(ui.available_width());
    ui::page_title(ui, "游戏识别", "");

    let (state, fg, library_hit, heuristics, game_name, since) = {
        let s = app.services.shared.lock().unwrap_or_else(|e| e.into_inner());
        let (state, name, since) = match &s.detect {
            DetectState::Idle => ("空闲（前台不是游戏）".to_string(), None, 0),
            DetectState::Candidate { exe, ticks, .. } => {
                (format!("候选中 · {exe} · 命中 {ticks} 次"), None, 0)
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
        (state, fg, hit, heur, name, since)
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

    // ---------- 第一行：当前前台窗口 | 游戏库 ----------
    // 两张卡片要求底边对齐：左卡先画，量出内容高度；右卡的列表吃掉剩余高度。
    // 用 min_rect/cursor 量的是 Frame 内部内容高度（标题+行），两卡的 Frame 边距一致，
    // 内容等高即卡片等高。
    let mut left_card_h = 0.0f32;
    ui.columns(2, |cols| {
        ui::card(&mut cols[0], "当前前台窗口", |ui| {
            let top = ui.min_rect().top();
            match &fg {
                Some(f) => {
                    kv(ui, "窗口句柄", &format!("0x{:X}", f.hwnd));
                    kv(ui, "进程 ID", &f.pid.to_string());
                    kv(ui, "映像名", &f.exe);
                    kv(ui, "窗口标题", if f.title.is_empty() { "—" } else { &f.title });
                    kv(ui, "窗口类", if f.class.is_empty() { "—" } else { &f.class });
                    kv(ui, "全屏", if f.fullscreen { "是" } else { "否" });
                    let prio = f
                        .priority
                        .map(crate::win::process::priority_name)
                        .unwrap_or("—");
                    kv(ui, "进程优先级", prio);
                    kv(ui, "映像路径", &f.path);
                }
                None => {
                    ui.label(RichText::new("没有前台窗口").size(12.0).color(ui::MUTED));
                }
            }
            left_card_h = ui.min_rect().bottom() - top;
        });

        ui::card(&mut cols[1], "游戏库（本地）", |ui| {
            let top = ui.min_rect().top();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("共 {} 条 · 命中判据为映像名", entries.len()))
                        .size(11.0)
                        .color(ui::MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("恢复默认库")
                        .on_hover_text("丢弃所有手工修改，恢复内置列表")
                        .clicked()
                    {
                        do_restore = true;
                    }
                });
            });
            ui.add_space(4.0);
            // 列表高度 = 左卡内容高度 −（标题+统计行已占用）。80px 下限兜底：
            // 左卡没有前台窗口时只剩一行，此时列表保持可用的小高度。
            let list_h = (left_card_h - (ui.cursor().top() - top)).max(80.0);

            egui::ScrollArea::vertical()
                .id_salt("lib")
                .max_height(list_h)
                .auto_shrink([false, false])
                // 滚动条常驻：否则它会晚于布局出现，available_width 少扣 10 多像素，
                // 行内按钮就会顶出卡片、把整页撑宽。
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
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
                                    ui.add(
                                        egui::TextEdit::singleline(&mut draft.exe)
                                            .hint_text("exe 名")
                                            .desired_width(140.0),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut draft.name)
                                            .hint_text("显示名")
                                            .desired_width(110.0),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    if ui.button("保存").clicked() {
                                        do_commit = true;
                                    }
                                    if ui.button("取消").clicked() {
                                        do_cancel = true;
                                    }
                                    ui.label(
                                        RichText::new("改 exe 名可修正误添加的条目")
                                            .size(10.0)
                                            .color(ui::MUTED),
                                    );
                                });
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(if hit { "●" } else { "○" })
                                        .size(9.0)
                                        .color(if hit { ui::GOOD } else { ui::LINE }),
                                );
                                // 先给右侧两个按钮留出固定宽度，再让文本按剩余宽度截断，
                                // 否则 truncate 会吃掉全部可用宽度、把按钮挤到文字上。
                                // 右侧按钮区固定 104px 且右对齐：按钮再宽也只在自己这块里排，
                                // 不会把整行顶宽（之前正是因为按钮撑宽，页面出现横向溢出）。
                                let btn_w = 92.0;
                                let text_w = (ui.available_width() - btn_w - 6.0).max(56.0);
                                let text = format!("{exe}  ·  {name}");
                                let full = text.clone();
                                // 固定宽度 + 左对齐：所有条目从同一列开始，长名截断、悬停看全文。
                                ui::text_cell(ui, &text, text_w, 11.0, egui::Color32::from_rgb(0x2A, 0x2F, 0x38))
                                    .on_hover_text(full);
                                // 在"剩余宽度"里右对齐放按钮：with_layout 会占满剩余空间，
                                // 两个按钮因此永远贴在卡片右边缘（不用子 Ui，裁剪行为正常）。
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("删除").clicked() {
                                            del_req = Some(exe.clone());
                                        }
                                        if ui.small_button("编辑").clicked() {
                                            edit_req = Some((exe.clone(), name.clone()));
                                        }
                                    },
                                );
                            });
                        }
                    }
                });
        });
    });

    // ---------- 第二行：判定依据 | 手工登记 ----------
    ui.add_space(8.0);
    ui.columns(2, |cols| {
        ui::card(&mut cols[0], "判定依据", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui::chip(
                    ui,
                    &state,
                    if game_name.is_some() { ui::GOOD } else { ui::MUTED },
                );
                if game_name.is_some() {
                    ui::chip(
                        ui,
                        &format!("已持续 {}", fmt_duration(now_epoch() - since)),
                        ui::ACCENT,
                    );
                }
            });
            ui.add_space(6.0);
            match &library_hit {
                Some((name, tags)) => kv(ui, "游戏库", &format!("{name}（{tags}）")),
                None => kv(ui, "游戏库", "未命中（exe 不在库中）"),
            }
            let runtime_text = if heuristics.is_empty() {
                "未检测到 d3d9/11/12、vulkan".to_string()
            } else {
                heuristics.join("、")
            };
            kv(ui, "图形运行时", &runtime_text);
        });

        ui::card(&mut cols[1], "手工登记", |ui| {
            ui::hint(ui, "启发式识别到但未登记的游戏，可在这里补一条；误加的条目可在上表编辑或删除。");
            ui.add_space(4.0);
            let field_w = ui.available_width().max(120.0);
            ui.add(
                egui::TextEdit::singleline(&mut app.new_custom_exe)
                    .hint_text("exe 名，如 MyGame.exe")
                    .desired_width(field_w),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.new_custom_name)
                    .hint_text("显示名")
                    .desired_width(field_w),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button("加入库").clicked() {
                    let exe = app.new_custom_exe.trim().to_string();
                    let name = app.new_custom_name.trim().to_string();
                    if !exe.is_empty() {
                        let name = if name.is_empty() { exe.clone() } else { name };
                        add_req = Some((exe, name));
                    }
                }
                if let Some(f) = &fg {
                    if ui.button("用当前前台进程填充").clicked() {
                        app.new_custom_exe = f.exe.clone();
                        app.new_custom_name = f.title.chars().take(20).collect();
                    }
                }
            });
        });
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

    // ---------- 第三行：历史 ----------
    ui.add_space(8.0);
    ui::card(ui, "最近识别记录", |ui| {
        let history = app
            .services
            .shared
            .lock()
            .map(|s| s.history.iter().take(8).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if history.is_empty() {
            ui.label(RichText::new("暂无历史会话").size(11.0).color(ui::MUTED));
            return;
        }
        egui::Grid::new("hist")
            .num_columns(4)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for h in ["游戏", "进程", "开始", "时长"] {
                    ui.label(RichText::new(h).size(10.0).color(ui::MUTED));
                }
                ui.end_row();
                for it in &history {
                    ui.add(egui::Label::new(RichText::new(&it.name).size(11.0)).truncate());
                    ui.add(
                        egui::Label::new(RichText::new(&it.exe).size(11.0).color(ui::MUTED))
                            .truncate(),
                    );
                    ui.label(RichText::new(ui::clock(it.started)).size(11.0));
                    ui.label(RichText::new(fmt_duration(it.duration_secs)).size(11.0));
                    ui.end_row();
                }
            });
    });
}

fn add_custom(lib: &mut crate::domain::games::GameLibrary, exe: &str, name: &str) {
    lib.add_custom(exe, name);
}

//! 界面层：三个入口（游戏识别 / Windows 优化 / 设置）。

pub mod detect;
pub mod settings;
pub mod winopt;

use eframe::egui::{self, Color32, FontId, RichText};

// 浅色主题下的一组语义色。
pub const ACCENT: Color32 = Color32::from_rgb(0x2E, 0x63, 0xC8);
pub const GOOD: Color32 = Color32::from_rgb(0x1E, 0x8E, 0x5A);
pub const WARN: Color32 = Color32::from_rgb(0xC9, 0x7A, 0x11);
pub const BAD: Color32 = Color32::from_rgb(0xC0, 0x39, 0x2B);
pub const MUTED: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
pub const LINE: Color32 = Color32::from_rgb(0xD8, 0xDD, 0xE4);
pub const PANEL: Color32 = Color32::from_rgb(0xF5, 0xF7, 0xFA);

/// 卡片容器：标题 + 内容。
pub fn card<R>(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, LINE))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(13.0).strong().color(MUTED));
            ui.add_space(6.0);
            body(ui)
        })
        .inner
}

/// 固定宽度、**左对齐**、超长截断的文本单元。
///
/// 为什么不用 `add_sized`：它把整个控件矩形在分配到的空间里居中，
/// `Label::halign(LEFT)` 也救不回来 —— 表格里会看到长短不一的条目"参差"。
/// 这里改成在一个固定宽度的子 Ui 里按 left_to_right 排布，文本自然贴左。
pub fn text_cell(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    size: f32,
    color: Color32,
) -> egui::Response {
    // 三个坑一次避开：
    // ① `add_sized` 会把控件居中 → 文本参差；
    // ② `allocate_ui_with_layout` 会缩到内容宽度 → 后续控件"贴着文字"跑；
    // ③ `new_child` 建子 Ui 画内容 → 子 Ui 不继承 ScrollArea 的裁剪区，行会画到卡片外。
    // 所以这里自己排版：先按精确尺寸占位，再用 painter 在 Ui 的裁剪矩形内画左对齐 galley。
    let galley = egui::WidgetText::from(RichText::new(text).size(size).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        width,
        egui::TextStyle::Body,
    );
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, size + 6.0), egui::Sense::hover());
    let pos = egui::pos2(
        rect.left(),
        rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(pos, galley, color);
    resp
}

/// 键值行；值过长时截断，悬停看全文。
pub fn kv(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        text_cell(ui, k, 92.0, 12.0, MUTED);
        ui.add(egui::Label::new(RichText::new(v).size(12.0)).truncate())
            .on_hover_text(v);
    });
}

/// 状态小标签。
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(11.0), Color32::WHITE);
    let pad = egui::vec2(8.0, 3.0);
    let size = galley.size() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, 8, color);
    ui.painter().galley(rect.min + pad, galley, Color32::WHITE);
}

/// 把 epoch 秒格式化成**本地**时间 HH:MM:SS。
pub fn clock(epoch: u64) -> String {
    crate::win::time::hms(epoch)
}

/// 页面大标题。
pub fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(20.0).strong());
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(12.0).color(MUTED));
    }
    ui.add_space(10.0);
}

/// 行内提示文字。
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(MUTED));
}

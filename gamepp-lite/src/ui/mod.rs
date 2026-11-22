//! 界面层：主题助手、共享组件与三个入口（游戏识别 / Windows 优化 / 设置）。
//!
//! 视觉全部走 shadcn-egui（vega 风格 + 深色模式）：
//! - 主题在 `app.rs` 安装（CJK 字体打底 → `Theme::apply_with_fonts`）；
//! - 颜色不再手写常量，统一从 `Theme::role` 取角色色；
//! - GOOD / WARN 注册为主题自定义角色（`Role::custom`），深浅模式各有一份；
//! - 卡片 = shadcn `Card`，状态标签 = `Badge`，输入 = `Input`，勾选 = `Checkbox`，
//!   滑杆 = `Slider`，双栏等高行由 [`split_row`] 做（黄金分割 + 上一帧高度回填）。

use eframe::egui::{self, Color32, RichText, Ui};
use shadcn_egui::components::badge::{Badge, BadgeVariant};
use shadcn_egui::components::button::{Button, ButtonSize, ButtonVariant};
use shadcn_egui::components::card::{card_header, card_header_with_action, Card};
use shadcn_egui::components::checkbox::Checkbox;
use shadcn_egui::components::input::Input;
use shadcn_egui::components::slider::Slider;
use shadcn_egui::style::Role;
use shadcn_egui::Theme;

pub mod detect;
pub mod settings;
pub mod winopt;

// ---------------------------------------------------------------- 主题角色

/// 语义色注册为主题自定义角色（app.rs 里 `.color(..)` 注册深浅两份）。
pub const GOOD_ROLE: Role = Role::custom("good");
pub const WARN_ROLE: Role = Role::custom("warn");

pub fn role_ctx(ctx: &egui::Context, r: Role) -> Color32 {
    Theme::current(ctx).role(r)
}

pub fn role_of(ui: &Ui, r: Role) -> Color32 {
    role_ctx(ui.ctx(), r)
}

pub fn muted(ui: &Ui) -> Color32 {
    role_of(ui, Role::MutedForeground)
}

pub fn good(ui: &Ui) -> Color32 {
    role_of(ui, GOOD_ROLE)
}

pub fn warn(ui: &Ui) -> Color32 {
    role_of(ui, WARN_ROLE)
}

pub fn bad(ui: &Ui) -> Color32 {
    role_of(ui, Role::Destructive)
}

// ---------------------------------------------------------------- 容器与文本

/// 卡片容器：shadcn Card + 标题头 + 内容。
pub fn card<R>(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Card::new().show(ui, |ui| {
        card_header(ui, title, "");
        body(ui)
    })
}

/// 带头部动作（标题行右侧）的卡片。
pub fn card_action<R>(
    ui: &mut egui::Ui,
    title: &str,
    action: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    Card::new().show(ui, |ui| {
        card_header_with_action(ui, title, "", action);
        body(ui)
    })
}

/// 固定宽度、左对齐、超长截断的文本单元。
///
/// 不用 `add_sized`（会把控件矩形居中导致文本参差），而是在固定宽度的子空间里
/// 自排版：先按精确尺寸占位，再用 painter 在裁剪矩形内画 galley。
pub fn text_cell(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    size: f32,
    color: Color32,
) -> egui::Response {
    text_cell_align(ui, text, width, size, color, egui::Align::LEFT)
}

/// 同 [`text_cell`]，但对齐方向可选（表格右对齐列用）。
pub fn text_cell_align(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    size: f32,
    color: Color32,
    align: egui::Align,
) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), egui::FontId::proportional(size), color);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, size + 6.0), egui::Sense::hover());
    let x = match align {
        egui::Align::RIGHT => rect.right() - galley.size().x,
        egui::Align::Center => rect.center().x - galley.size().x * 0.5,
        _ => rect.left(),
    };
    let pos = egui::pos2(x, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(pos, galley, color);
    resp
}

/// 键值行；值过长时截断，悬停看全文。
pub fn kv(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        text_cell(ui, k, 92.0, 12.0, muted(ui));
        ui.add(egui::Label::new(RichText::new(v).size(12.0)).truncate())
            .on_hover_text(v);
    });
}

/// 状态小标签（shadcn Badge）。
pub fn chip(ui: &mut egui::Ui, text: &str, variant: BadgeVariant) {
    ui.add(Badge::new(text).variant(variant));
}

/// 把 epoch 秒格式化成**本地**时间 HH:MM:SS。
pub fn clock(epoch: u64) -> String {
    crate::win::time::hms(epoch)
}

/// 页面大标题 + 副标题。
pub fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(20.0).strong());
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(12.0).color(muted(ui)));
    }
    ui.add_space(10.0);
}

/// 行内提示文字。
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(muted(ui)));
}

// ---------------------------------------------------------------- 常用控件行

/// shadcn Checkbox 行：勾选框 + 文本（超长截断、悬停看全文）。返回是否变化。
pub fn check_row(ui: &mut egui::Ui, checked: &mut bool, text: &str) -> bool {
    let mut c = *checked;
    ui.horizontal(|ui| {
        ui.add(Checkbox::new(&mut c));
        ui.add(egui::Label::new(RichText::new(text).size(12.0)).truncate())
            .on_hover_text(text);
    });
    let changed = c != *checked;
    *checked = c;
    changed
}

/// 滑杆字段（A 方案：标签在左、shadcn Slider 居中、当前值贴右缘右对齐）。
/// `total_w` 传页面可用宽度（Card 体内 Ui 宽度无界，不能用 available_width）。
/// 返回是否变化。
pub fn slider_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    display: impl Fn(f32) -> String,
    total_w: f32,
) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        text_cell(ui, label, 150.0, 12.0, muted(ui));
        let slider_w = (total_w - 150.0 - 56.0 - 28.0).max(120.0);
        ui.allocate_ui(egui::vec2(slider_w, 0.0), |ui| {
            ui.add(Slider::new(value, range).step(step));
        });
        let text = display(*value);
        text_cell_align(ui, &text, 56.0, 12.0, role_of(ui, Role::Foreground), egui::Align::RIGHT);
    });
    *value != before
}

/// 小号 Ghost 按钮（行内编辑/全选清空用）。
pub fn ghost_sm(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(Button::new(text).variant(ButtonVariant::Ghost).size(ButtonSize::Sm))
}

// ---------------------------------------------------------------- 黄金分割行

/// 在横向一行里画一个**精确宽度、纵向排版**的栏。
///
/// 不能用 `allocate_ui`：它只按内容尺寸推进光标（申请的宽度不会被保留），
/// 且继承父级的横向布局。这里手工开一个定宽纵向子 Ui，再按精确尺寸占位推进光标。
/// 返回内容与内容高度（供等高行回填）。
pub fn column<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> (R, f32) {
    let start = ui.cursor().min;
    let rect = egui::Rect::from_min_size(start, egui::vec2(width.max(0.0), 0.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ret = add(&mut child);
    let h = child.min_rect().height();
    ui.allocate_exact_size(egui::vec2(width.max(0.0), h), egui::Sense::hover());
    (ret, h)
}

/// 黄金分割双卡行：左卡占 `ratio`（0.618 ≈ 1:1.618 的"大部分"），行内等高。
///
/// 等高用"上一帧高度回填"实现：首帧各自自适应；之后每帧把上一帧两卡实测高度的
/// 较大值传回给 `h`，卡片内部用它 `set_min_height` / 撑 ScrollArea，两卡便稳定等高。
/// 内容高度变化时一帧内收敛，无副作用（两闭包共享同一个 `state`，顺序借用）。
pub fn split_row<S, R1, R2>(
    ui: &mut egui::Ui,
    id: &str,
    ratio: f32,
    gap: f32,
    state: &mut S,
    left: impl FnOnce(&mut egui::Ui, f32, &mut S) -> R1,
    right: impl FnOnce(&mut egui::Ui, f32, &mut S) -> R2,
) {
    let ctx = ui.ctx().clone();
    let key = ui.make_persistent_id(id);
    let lh = ctx.data(|d| d.get_temp::<f32>(key.with("lh"))).unwrap_or(0.0);
    let rh = ctx.data(|d| d.get_temp::<f32>(key.with("rh"))).unwrap_or(0.0);
    let h = lh.max(rh);

    let total = ui.available_width();
    let left_w = ((total - gap) * ratio).floor().max(120.0);
    let right_w = (total - left_w - gap * 2.0).max(120.0);

    let mut mlh = 0.0f32;
    let mut mrh = 0.0f32;
    ui.horizontal(|ui| {
        let (_l, hl) = column(ui, left_w, |ui| left(ui, h, state));
        mlh = hl;
        ui.allocate_exact_size(egui::vec2(gap, 0.0), egui::Sense::hover());
        let (_r, hr) = column(ui, right_w, |ui| right(ui, h, state));
        mrh = hr;
    });
    ctx.data_mut(|d| {
        d.insert_temp(key.with("lh"), mlh);
        d.insert_temp(key.with("rh"), mrh);
    });
}

/// 卡片体内使用的"目标高度"：`h > 0` 时把身体最小高度撑到减去头部/边距后的值，
/// 让卡片与另一侧等高；首帧 `h == 0` 时退回 `fallback`。
pub fn stretch_body(ui: &mut egui::Ui, h: f32, fallback: f32) {
    let target = if h > 20.0 { (h - 30.0).max(0.0) } else { fallback };
    ui.set_min_height(target);
}

/// 滚动区的最大高度：按等高目标 `h` 扣掉头部/边距估算（Card 体内高度无界，
/// 不能用 available_height），首帧给兜底值。
pub fn scroll_max(_ui: &egui::Ui, h: f32, fallback: f32) -> f32 {
    if h > 20.0 {
        (h - 110.0).max(fallback)
    } else {
        fallback
    }
}

// ---------------------------------------------------------------- 输入

/// shadcn Input 单行输入框。
pub fn input(ui: &mut egui::Ui, buf: &mut String, hint: &str, width: f32) -> egui::Response {
    ui.add(Input::new(buf).hint(hint).max_width(width))
}

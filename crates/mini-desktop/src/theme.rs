//! Shared visual language: an X-inspired black palette, an initials avatar,
//! card frames, pill badges and borderless icon actions.
//!
//! Every glyph painted anywhere in this crate is checked against the three
//! fonts `eframe`'s `default_fonts` feature actually bundles (Ubuntu-Light,
//! NotoEmoji-Regular, emoji-icon-font). A glyph outside that set renders as
//! a silent tofu box with no compile-time signal, so `glyph_coverage` below
//! pins the set with a real font-shaping test instead of a visual read.

use eframe::egui::{self, Color32};

pub const BG: Color32 = Color32::from_rgb(0, 0, 0);
pub const CARD: Color32 = Color32::from_rgb(16, 18, 22);
pub const CARD_HOVER: Color32 = Color32::from_rgb(24, 27, 32);
pub const BORDER: Color32 = Color32::from_rgb(47, 51, 54);
pub const ACCENT: Color32 = Color32::from_rgb(29, 155, 240);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(231, 233, 234);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(113, 118, 123);
pub const ONLINE_GREEN: Color32 = Color32::from_rgb(0, 186, 124);
pub const LIKE_PINK: Color32 = Color32::from_rgb(249, 24, 128);
pub const WARN_AMBER: Color32 = Color32::from_rgb(255, 173, 31);

const AVATAR_PALETTE: [Color32; 8] = [
    Color32::from_rgb(29, 155, 240),
    Color32::from_rgb(0, 186, 124),
    Color32::from_rgb(249, 24, 128),
    Color32::from_rgb(255, 173, 31),
    Color32::from_rgb(148, 116, 255),
    Color32::from_rgb(255, 122, 89),
    Color32::from_rgb(23, 191, 195),
    Color32::from_rgb(230, 89, 143),
];

/// A deterministic color from a small curated palette, keyed by a stable
/// string (the DID when available) so one person always paints the same.
fn avatar_color(seed: &str) -> Color32 {
    let hash: u32 = seed.bytes().fold(5381u32, |h, b| {
        h.wrapping_mul(33).wrapping_add(u32::from(b))
    });
    AVATAR_PALETTE[(hash as usize) % AVATAR_PALETTE.len()]
}

/// A filled circle with the name's first letter. No image loading, no
/// network fetch, no cache: identical inputs always paint identically.
pub fn avatar(ui: &mut egui::Ui, name: &str, seed: &str, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .circle_filled(rect.center(), size / 2.0, avatar_color(seed));
        let initial = name
            .chars()
            .find(|c| c.is_alphanumeric())
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            initial,
            egui::FontId::proportional(size * 0.42),
            Color32::WHITE,
        );
    }
    response
}

/// The rounded, bordered surface every post/community/profile card uses.
pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(16))
}

/// A flat divider-separated row, the X timeline look for posts.
pub fn row_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(BG)
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::symmetric(16, 12))
}

/// A small rounded status/category label, e.g. "HOSTING" or "You follow".
pub fn pill_badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.linear_multiply(0.16))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().color(color).strong());
        });
}

/// A borderless, pill-hover glyph+label action (reply, like, …).
pub fn icon_action(ui: &mut egui::Ui, glyph: &str, label: &str, hover_color: Color32) -> bool {
    let response = ui.add(
        egui::Button::new(
            egui::RichText::new(format!("{glyph}  {label}"))
                .small()
                .color(TEXT_SECONDARY),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(255)),
    );
    if response.hovered() {
        ui.painter().rect_filled(
            response.rect.expand(4.0),
            egui::CornerRadius::same(255),
            hover_color.linear_multiply(0.15),
        );
    }
    response.clicked()
}

/// The filled, rounded primary action ("Post", "Start session").
pub fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text.to_owned())
            .strong()
            .color(Color32::WHITE),
    )
    .fill(ACCENT)
    .corner_radius(egui::CornerRadius::same(255))
}

/// The outlined secondary action.
pub fn secondary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_owned()).color(TEXT_PRIMARY))
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(255))
}

pub fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .strong()
            .size(17.0)
            .color(TEXT_PRIMARY),
    );
}

pub fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().color(TEXT_SECONDARY));
}

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.faint_bg_color = CARD;
    visuals.extreme_bg_color = Color32::from_rgb(8, 9, 11);
    visuals.override_text_color = Some(TEXT_PRIMARY);

    visuals.widgets.noninteractive.bg_fill = BG;
    visuals.widgets.noninteractive.weak_bg_fill = BG;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);

    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(255);

    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(255);

    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(255);

    visuals.selection.bg_fill = ACCENT.linear_multiply(0.35);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARN_AMBER;

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 9.0);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.5));
    ctx.set_style(style);
}

/// Every glyph the desktop UI paints. Keep this list in step with the
/// navigation rail and card actions; the test below shapes each one with the
/// bundled fonts.
#[cfg(test)]
const USED_GLYPHS: &[&str] = &[
    "🏠", "🔍", "🎬", "✉", "👥", "🏢", "✏", "🔗", "🖥", "🔒", "🔓", "⬆", "💬", "♥", "🔄", "🌐", "•",
    "🖼", "⚠", "ℹ", "📋", "▶", "■", "✔", "✖",
];

#[cfg(test)]
mod glyph_coverage {
    use super::USED_GLYPHS;
    use eframe::egui;

    #[test]
    fn every_glyph_this_ui_paints_is_in_the_bundled_fonts() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            let font_id = egui::FontId::proportional(15.0);
            for glyph in USED_GLYPHS {
                assert!(
                    ctx.fonts(|f| f.has_glyphs(&font_id, glyph)),
                    "glyph {glyph:?} is not covered by eframe's bundled fonts -- it will \
                     render as a tofu box"
                );
            }
        });
    }

    #[test]
    fn avatar_color_is_stable_for_the_same_seed() {
        assert_eq!(
            super::avatar_color("did:mini:abc"),
            super::avatar_color("did:mini:abc")
        );
    }
}

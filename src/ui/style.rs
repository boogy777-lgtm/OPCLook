//! UI8: the one place that sets the application look. The type scale, the
//! spacing scale and the soft table stripes are applied to both the dark and
//! the light egui style, once at start-up and again when the theme setting
//! changes (see [`sync_style`]).

use crate::constants::{
    BUTTON_PADDING, ITEM_SPACING, TEXT_SIZE_BODY, TEXT_SIZE_HEADING, TEXT_SIZE_SMALL, WINDOW_MARGIN,
};
use egui::{Color32, FontFamily, FontId, TextStyle};

/// Striped-row colour: barely lighter than the dark `panel_fill` (gray 27).
const DARK_STRIPE: Color32 = Color32::from_gray(31);
/// Striped-row colour: barely darker than the light `panel_fill` (gray 248).
const LIGHT_STRIPE: Color32 = Color32::from_gray(243);

/// Type scale (Small 12, Body/Button/Monospace 13, Heading 15) and spacing
/// (item 8x6, button padding 8x4, window margin 12).
fn apply_scale(style: &mut egui::Style) {
    let font = |size: f32, family: FontFamily| FontId::new(size, family);
    style.text_styles = [
        (
            TextStyle::Small,
            font(TEXT_SIZE_SMALL, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            font(TEXT_SIZE_BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            font(TEXT_SIZE_BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            font(TEXT_SIZE_BODY, FontFamily::Monospace),
        ),
        (
            TextStyle::Heading,
            font(TEXT_SIZE_HEADING, FontFamily::Proportional),
        ),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(ITEM_SPACING.0, ITEM_SPACING.1);
    style.spacing.button_padding = egui::vec2(BUTTON_PADDING.0, BUTTON_PADDING.1);
    style.spacing.window_margin = egui::Margin::same(WINDOW_MARGIN);
}

/// Apply the application style to the context (both themes).
pub fn apply_style(ctx: &egui::Context) {
    // Only the dark theme is selectable until UI9 adds the setting.
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, egui::Visuals::dark());
    ctx.set_visuals_of(egui::Theme::Light, egui::Visuals::light());
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        apply_scale(style);
        style.visuals.faint_bg_color = DARK_STRIPE;
    });
    ctx.style_mut_of(egui::Theme::Light, |style| {
        apply_scale(style);
        style.visuals.faint_bg_color = LIGHT_STRIPE;
    });
    ctx.data_mut(|d| d.insert_temp(applied_marker(), StyleApplied));
}

/// Marker stored in the egui context: the style has been applied.
#[derive(Clone, Copy, PartialEq, Eq)]
struct StyleApplied;

fn applied_marker() -> egui::Id {
    egui::Id::new("opclook_style_applied")
}

/// Apply the style once per context; called at the start of every frame.
pub(super) fn sync_style(ctx: &egui::Context) {
    let applied = ctx
        .data(|d| d.get_temp::<StyleApplied>(applied_marker()))
        .is_some();
    if !applied {
        apply_style(ctx);
        ctx.request_repaint();
    }
}

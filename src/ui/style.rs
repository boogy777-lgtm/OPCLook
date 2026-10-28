//! UI8: the one place that sets the application look. The type scale, the
//! spacing scale and the soft table stripes are applied to both the dark and
//! the light egui style, once at start-up and again when the theme setting
//! changes (see [`sync_style`]).

use crate::constants::{
    BUTTON_PADDING, ITEM_SPACING, TEXT_SIZE_BODY, TEXT_SIZE_HEADING, TEXT_SIZE_SMALL, WINDOW_MARGIN,
};
use crate::settings::Theme;
use egui::{FontFamily, FontId, TextStyle};

use super::colors::palette;

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

/// Apply a complete palette to surfaces and interaction states.
fn apply_palette(visuals: &mut egui::Visuals) {
    let p = palette(visuals.dark_mode);
    visuals.disabled_alpha = 0.65;
    visuals.panel_fill = p.panel;
    visuals.window_fill = p.window;
    visuals.extreme_bg_color = p.input;
    visuals.text_edit_bg_color = Some(p.input);
    visuals.code_bg_color = p.stripe;
    visuals.faint_bg_color = p.stripe;
    visuals.weak_text_color = Some(p.weak);
    visuals.hyperlink_color = p.accent;
    visuals.warn_fg_color = p.warning;
    visuals.error_fg_color = p.error;
    visuals.selection.bg_fill = p.selection;
    visuals.selection.stroke.color = p.accent;
    visuals.window_stroke.color = p.border;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.fg_stroke.color = p.text;
        widget.bg_stroke.color = p.border;
    }
    visuals.widgets.noninteractive.bg_fill = p.window;
    visuals.widgets.noninteractive.weak_bg_fill = p.stripe;
    visuals.widgets.inactive.bg_fill = p.button;
    visuals.widgets.inactive.weak_bg_fill = p.button;
    visuals.widgets.hovered.bg_fill = p.hover;
    visuals.widgets.hovered.weak_bg_fill = p.hover;
    visuals.widgets.hovered.bg_stroke.color = p.accent;
    visuals.widgets.active.bg_fill = p.selection;
    visuals.widgets.active.weak_bg_fill = p.selection;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, p.accent);
    visuals.widgets.open.bg_fill = p.selection;
    visuals.widgets.open.weak_bg_fill = p.selection;
}

/// Apply the application style (both palettes) and the theme preference:
/// `System` follows the OS (`ctx.system_theme()`; dark when the OS says
/// nothing), `Light`/`Dark` force the palette.
pub fn apply_style(ctx: &egui::Context, theme: Theme) {
    ctx.set_theme(match theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    });
    ctx.set_visuals_of(egui::Theme::Dark, egui::Visuals::dark());
    ctx.set_visuals_of(egui::Theme::Light, egui::Visuals::light());
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        apply_scale(style);
        apply_palette(&mut style.visuals);
    });
    ctx.style_mut_of(egui::Theme::Light, |style| {
        apply_scale(style);
        apply_palette(&mut style.visuals);
    });
    ctx.data_mut(|d| d.insert_temp(applied_marker(), AppliedTheme(theme)));
}

/// Marker stored in the egui context: the style was applied for this theme.
#[derive(Clone, Copy, PartialEq, Eq)]
struct AppliedTheme(Theme);

fn applied_marker() -> egui::Id {
    egui::Id::NULL
}

/// Apply the style on the first frame and whenever the theme setting changes;
/// called at the start of every frame.
pub(super) fn sync_style(ctx: &egui::Context, theme: Theme) {
    let applied = ctx.data(|d| d.get_temp::<AppliedTheme>(applied_marker()));
    if applied != Some(AppliedTheme(theme)) {
        apply_style(ctx, theme);
        // The frame in progress still uses the old style.
        ctx.request_repaint();
    }
}

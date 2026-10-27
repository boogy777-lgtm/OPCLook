//! UI8: the one place that sets the application look. The type scale, the
//! spacing scale and the soft table stripes are applied to both the dark and
//! the light egui style, once at start-up and again when the theme setting
//! changes (see [`sync_style`]).

use crate::constants::{
    BUTTON_PADDING, ITEM_SPACING, TEXT_SIZE_BODY, TEXT_SIZE_HEADING, TEXT_SIZE_SMALL, WINDOW_MARGIN,
};
use crate::settings::Theme;
use egui::{Color32, FontFamily, FontId, TextStyle};

use super::colors::accent_for;

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

/// Accent-coloured selection and links for one palette (UI9).
fn apply_accent(visuals: &mut egui::Visuals) {
    let dark = visuals.dark_mode;
    let accent = accent_for(dark);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = if dark {
        Color32::from_rgb(36, 76, 128)
    } else {
        Color32::from_rgb(190, 214, 245)
    };
    visuals.selection.stroke.color = if dark {
        Color32::from_rgb(200, 222, 255)
    } else {
        Color32::from_rgb(16, 60, 130)
    };
    visuals.widgets.hovered.bg_stroke.color = accent;
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
        style.visuals.faint_bg_color = DARK_STRIPE;
        apply_accent(&mut style.visuals);
    });
    ctx.style_mut_of(egui::Theme::Light, |style| {
        apply_scale(style);
        style.visuals.faint_bg_color = LIGHT_STRIPE;
        apply_accent(&mut style.visuals);
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

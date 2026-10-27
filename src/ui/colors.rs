use crate::bff::model::QualityKind;
use egui::Color32;
use std::cell::Cell;

thread_local! {
    /// Whether the dark palette is in use. `ui::draw` refreshes it at the
    /// start of every frame from `ui.visuals().dark_mode`; egui draws on one
    /// thread, so the colour helpers below keep their argument-free shape.
    static DARK: Cell<bool> = const { Cell::new(true) };
}

/// Select the palette for the current frame (UI9).
pub(super) fn set_dark(dark: bool) {
    DARK.with(|d| d.set(dark));
}

fn is_dark() -> bool {
    DARK.with(Cell::get)
}

pub(super) fn quality_color(kind: QualityKind) -> Color32 {
    match kind {
        QualityKind::Good => green(),
        QualityKind::Uncertain => yellow(),
        QualityKind::Bad => red(),
    }
}

pub(super) fn green() -> Color32 {
    if is_dark() {
        Color32::from_rgb(64, 200, 96)
    } else {
        Color32::from_rgb(20, 130, 56)
    }
}

pub(super) fn yellow() -> Color32 {
    if is_dark() {
        Color32::from_rgb(230, 180, 40)
    } else {
        Color32::from_rgb(158, 104, 0)
    }
}

pub(super) fn red() -> Color32 {
    if is_dark() {
        Color32::from_rgb(235, 90, 80)
    } else {
        Color32::from_rgb(200, 40, 34)
    }
}

/// The single accent colour (a calm blue) of the given theme; used for
/// selection, links and the active-tab marker.
pub(super) fn accent_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(86, 156, 235)
    } else {
        Color32::from_rgb(28, 100, 200)
    }
}

/// The accent colour of the theme drawn this frame.
pub(super) fn accent() -> Color32 {
    accent_for(is_dark())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(c: Color32) -> u32 {
        u32::from(c.r()) * 299 + u32::from(c.g()) * 587 + u32::from(c.b()) * 114
    }

    #[test]
    fn quality_colors_differ_between_light_and_dark_and_stay_distinct() {
        for kind in [QualityKind::Good, QualityKind::Uncertain, QualityKind::Bad] {
            set_dark(true);
            let dark = quality_color(kind);
            set_dark(false);
            let light = quality_color(kind);
            assert_ne!(dark, light, "{kind:?}");
            // Darker on the white background of the light theme.
            assert!(luma(light) < luma(dark), "{kind:?}");
        }
        set_dark(false);
        let light = [green(), yellow(), red()];
        assert!(light[0] != light[1] && light[1] != light[2] && light[0] != light[2]);
        set_dark(true);
    }

    #[test]
    fn light_quality_colors_have_readable_contrast_on_white() {
        set_dark(false);
        for c in [green(), yellow(), red()] {
            // WCAG-style luminance ratio against white must exceed 3:1 (large
            // bold table text) - white is 1.0.
            let lin = |v: u8| {
                let v = f32::from(v) / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            let l = 0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b());
            let ratio = 1.05 / (l + 0.05);
            assert!(ratio >= 3.0, "{c:?} ratio {ratio}");
        }
        set_dark(true);
    }

    #[test]
    fn accent_follows_the_palette() {
        set_dark(true);
        let dark = accent();
        set_dark(false);
        assert_ne!(dark, accent());
        assert_eq!(accent(), accent_for(false));
        set_dark(true);
    }
}

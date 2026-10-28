use crate::bff::model::QualityKind;
use egui::Color32;
use std::cell::Cell;

/// Opaque palette tokens shared by egui visuals and application-specific painting.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub panel: Color32,
    pub window: Color32,
    pub stripe: Color32,
    pub input: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub button: Color32,
    pub hover: Color32,
    pub selection: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub good: Color32,
    pub warning: Color32,
    pub error: Color32,
}

pub(super) fn palette(dark: bool) -> Palette {
    let rgb = Color32::from_rgb;
    if dark {
        Palette {
            panel: rgb(30, 32, 35),
            window: rgb(37, 39, 43),
            stripe: rgb(36, 38, 41),
            input: rgb(24, 26, 29),
            text: rgb(235, 237, 240),
            weak: rgb(190, 194, 200),
            button: rgb(48, 51, 56),
            hover: rgb(57, 61, 68),
            selection: rgb(42, 57, 72),
            border: rgb(155, 160, 168),
            accent: rgb(125, 185, 255),
            good: rgb(99, 218, 130),
            warning: rgb(245, 194, 75),
            error: rgb(255, 170, 165),
        }
    } else {
        Palette {
            panel: rgb(238, 239, 241),
            window: rgb(245, 246, 247),
            stripe: rgb(232, 234, 237),
            input: rgb(255, 255, 255),
            text: rgb(30, 33, 38),
            weak: rgb(75, 79, 86),
            button: rgb(229, 231, 234),
            hover: rgb(218, 222, 227),
            selection: rgb(212, 224, 237),
            border: rgb(91, 96, 104),
            accent: rgb(22, 79, 160),
            good: rgb(0, 105, 39),
            warning: rgb(121, 76, 0),
            error: rgb(164, 30, 26),
        }
    }
}

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

pub(super) fn muted() -> Color32 {
    palette(is_dark()).weak
}

pub(super) fn green() -> Color32 {
    palette(is_dark()).good
}
pub(super) fn yellow() -> Color32 {
    palette(is_dark()).warning
}
pub(super) fn red() -> Color32 {
    palette(is_dark()).error
}
pub(super) fn accent_for(dark: bool) -> Color32 {
    palette(dark).accent
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

    fn composite(fg: Color32, alpha: f64, bg: Color32) -> Color32 {
        let channel =
            |a: u8, b: u8| (f64::from(a) * alpha + f64::from(b) * (1.0 - alpha)).round() as u8;
        Color32::from_rgb(
            channel(fg.r(), bg.r()),
            channel(fg.g(), bg.g()),
            channel(fg.b(), bg.b()),
        )
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let luminance = |c: Color32| {
            let linear = |v: u8| {
                let x = f64::from(v) / 255.0;
                if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn p1_actual_palette_pairs_and_composited_states_meet_targets() {
        for dark in [false, true] {
            let p = palette(dark);
            let surfaces = [
                p.panel,
                p.window,
                p.stripe,
                p.input,
                p.button,
                p.hover,
                p.selection,
            ];
            for bg in surfaces {
                for fg in [p.text, p.weak, p.accent, p.good, p.warning, p.error] {
                    let ratio = contrast(fg, bg);
                    assert!(ratio >= 4.5, "dark={dark} {fg:?} on {bg:?}: {ratio}");
                }
                for fg in [p.good, p.warning, p.error] {
                    // The quality pill is drawn with 8% of its tint over the row.
                    let pill = composite(fg, 0.08, bg);
                    let ratio = contrast(fg, pill);
                    assert!(ratio >= 4.5, "dark={dark} pill {fg:?} on {pill:?}: {ratio}");
                }
            }
            // Row selection (45%), change flash (80%) and the quality pill
            // (8%) can be painted on top of one another.
            for base in [
                p.panel,
                p.stripe,
                composite(p.selection, 0.45, p.panel),
                p.selection,
            ] {
                let flashed = composite(p.selection, 0.8, base);
                for fg in [p.good, p.warning, p.error] {
                    let pill = composite(fg, 0.08, flashed);
                    let ratio = contrast(fg, pill);
                    assert!(
                        ratio >= 4.5,
                        "dark={dark} layered pill {fg:?} on {pill:?}: {ratio}"
                    );
                }
            }
            for bg in [p.panel, p.window, p.button, p.hover, p.selection] {
                assert!(contrast(p.border, bg) >= 3.0, "border on {bg:?}");
                assert!(contrast(p.accent, bg) >= 3.0, "focus on {bg:?}");
            }
            let disabled_text = composite(p.text, 0.65, p.button);
            assert!(contrast(disabled_text, p.button) >= 3.0, "disabled text");
        }
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

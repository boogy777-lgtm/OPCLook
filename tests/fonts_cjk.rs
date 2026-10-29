//! A1: the Chinese UI language needs a CJK fallback font. Without it every
//! hanzi (and the «中文» menu entry) renders as a box.
use egui::{FontFamily, FontId};

/// Face to check: three characters of the language menu entry plus a
/// character that only a CJK font has.
const SAMPLE: &str = "中文界面";

fn glyphs_present(ctx: &egui::Context, family: FontFamily) -> bool {
    let mut present = false;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        present = ui
            .ctx()
            .fonts_mut(|fonts| fonts.has_glyphs(&FontId::new(14.0, family.clone()), SAMPLE));
    });
    // epaint asserts that texture deltas are not silently dropped.
    output.textures_delta.clear();
    present
}

#[test]
fn default_egui_fonts_cannot_draw_chinese() {
    // The premise of the bug: without a CJK face the glyphs are missing.
    let ctx = egui::Context::default();
    assert!(!glyphs_present(&ctx, FontFamily::Proportional));
}

#[test]
fn install_fonts_adds_a_cjk_fallback_when_the_host_has_one() {
    let ctx = egui::Context::default();
    let Some(path) = opclook::app::install_fonts(&ctx) else {
        // Honest skip: this host has none of the candidate CJK fonts (the
        // Windows ones are msyh.ttc / simsun.ttc; Linux hosts use Noto CJK or
        // WenQuanYi, or OPCLOOK_CJK_FONT).
        eprintln!("SKIP a1: no CJK font file found on this host");
        return;
    };
    eprintln!("CJK fallback font: {path}");
    assert!(glyphs_present(&ctx, FontFamily::Proportional));
    assert!(glyphs_present(&ctx, FontFamily::Monospace));
}

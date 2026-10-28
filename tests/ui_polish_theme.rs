//! UI polish, stream `theme` (UI7-UI9): bottom panel layout, type/spacing
//! scale and the light/dark/system theme. Headless egui_kittest tests.
use egui_kittest::{Harness, kittest::Queryable};
use opcda_egui::app::{App, BottomTab};
use opcda_egui::i18n::{Lang, Tr, tr};

fn harness(mut app: App) -> Harness<'static, App> {
    app.settings.set_lang(Lang::Ru);
    Harness::builder()
        .with_size([1200.0, 700.0])
        .with_max_steps(60)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app| opcda_egui::ui::draw(app, ui), app)
}

fn fresh_app() -> App {
    let ctx = egui::Context::default();
    App::new_test(&ctx)
}

#[test]
fn ui7_fresh_app_shows_journal_tab_first_and_script_second() {
    let mut h = harness(fresh_app());
    h.run_steps(3);
    assert_eq!(h.state().bottom_tab, BottomTab::Journal);
    // The journal toolbar is visible, the script toolbar is not.
    assert!(h.query_by_label(tr(Lang::Ru, Tr::SaveJournal)).is_some());
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveScriptLabel))
            .is_none()
    );
    // Journal tab sits to the left of the script tab (the lower copy of each
    // label is the tab; "Журнал" also exists in the menu bar).
    let tab_x = |label: &str| {
        h.get_all_by_label(label)
            .max_by(|a, b| a.rect().center().y.total_cmp(&b.rect().center().y))
            .map(|n| n.rect().center().x)
            .expect("tab node")
    };
    assert!(tab_x(tr(Lang::Ru, Tr::JournalTab)) < tab_x(tr(Lang::Ru, Tr::ScriptTab)));
}

#[test]
fn ui7_script_note_is_only_shown_on_the_script_tab() {
    let mut h = harness(fresh_app());
    h.run_steps(3);
    assert!(h.query_by_label(tr(Lang::Ru, Tr::ScriptsNotImpl)).is_none());
    h.state_mut().bottom_tab = BottomTab::Script;
    h.run_steps(3);
    assert!(h.query_by_label(tr(Lang::Ru, Tr::ScriptsNotImpl)).is_some());
}

#[test]
fn ui7_script_tab_hint_says_not_implemented_in_every_language() {
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        let hint = tr(lang, Tr::HintScriptTabNotImpl);
        assert!(!hint.is_empty());
        assert_ne!(hint, tr(lang, Tr::HintScriptTab));
    }
    assert!(tr(Lang::Ru, Tr::HintScriptTabNotImpl).contains("не реализовано"));
    assert!(tr(Lang::En, Tr::HintScriptTabNotImpl).contains("not implemented"));
}

#[test]
fn ui7_bottom_panel_default_is_compact_and_flags_keep_their_logic() {
    assert!(opcda_egui::constants::BOTTOM_PANEL_DEFAULT_HEIGHT <= 180.0);
    // Journal-only: no tab bar, script toolbar absent.
    let mut app = fresh_app();
    app.settings.view.options.show_script = false;
    app.settings.view.options.show_log = true;
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveScriptLabel))
            .is_none()
    );
    assert!(h.query_by_label(tr(Lang::Ru, Tr::SaveJournal)).is_some());
    // Script-only: script toolbar shown, no journal toolbar.
    let mut app = fresh_app();
    app.settings.view.options.show_script = true;
    app.settings.view.options.show_log = false;
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveScriptLabel))
            .is_some()
    );
    assert!(h.query_by_label(tr(Lang::Ru, Tr::SaveJournal)).is_none());
}

// ---------------------------------------------------------------------------
// UI8: one type / spacing scale
// ---------------------------------------------------------------------------

fn font_size(style: &egui::Style, ts: egui::TextStyle) -> f32 {
    style.text_styles.get(&ts).map(|f| f.size).unwrap_or(0.0)
}

#[test]
fn ui8_text_scale_spacing_and_row_height_are_applied_to_both_themes() {
    let mut h = harness(fresh_app());
    h.run_steps(3);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let style = h.ctx.style_of(theme);
        assert_eq!(font_size(&style, egui::TextStyle::Small), 12.0);
        assert_eq!(font_size(&style, egui::TextStyle::Body), 13.0);
        assert_eq!(font_size(&style, egui::TextStyle::Button), 13.0);
        assert_eq!(font_size(&style, egui::TextStyle::Monospace), 13.0);
        assert_eq!(font_size(&style, egui::TextStyle::Heading), 15.0);
        assert_eq!(style.spacing.item_spacing, egui::vec2(8.0, 6.0));
        assert_eq!(style.spacing.button_padding, egui::vec2(8.0, 4.0));
        assert_eq!(style.spacing.window_margin, egui::Margin::same(12));
    }
    assert_eq!(opcda_egui::constants::TABLE_ROW_HEIGHT, 24.0);
    assert_eq!(opcda_egui::constants::TABLE_HEADER_HEIGHT, 22.0);
}

#[test]
fn ui8_table_stripes_stay_close_to_the_panel_fill() {
    let mut h = harness(fresh_app());
    h.run_steps(3);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let v = h.ctx.style_of(theme).visuals.clone();
        let (a, b) = (v.faint_bg_color, v.panel_fill);
        let diff = a.r().abs_diff(b.r());
        assert!(
            diff > 0 && diff <= 8,
            "{theme:?}: stripe vs panel diff {diff}"
        );
    }
}

// ---------------------------------------------------------------------------
// UI9: light / dark / system theme
// ---------------------------------------------------------------------------

use opcda_egui::settings::{Options, Settings, Theme};

fn dark_mode(h: &Harness<'_, App>) -> bool {
    h.ctx.global_style().visuals.dark_mode
}

fn app_with_theme(theme: Theme) -> App {
    let mut app = fresh_app();
    app.settings.view.options.theme = theme;
    app
}

#[test]
fn ui9_theme_defaults_to_system_and_old_settings_files_load() {
    assert_eq!(Options::default().theme, Theme::System);
    let mut value = serde_json::to_value(Options::default()).expect("serialize");
    value
        .as_object_mut()
        .expect("options object")
        .remove("theme");
    let old: Options = serde_json::from_value(value).expect("old file loads");
    assert_eq!(old.theme, Theme::System);
}

#[test]
fn ui9_theme_setting_roundtrips_through_the_settings_file() {
    let dir = std::env::temp_dir().join(format!("opclook-ui9-theme-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    {
        let _redirect = opcda_egui::settings::thread_dir::redirect(&dir);
        for theme in [Theme::Light, Theme::Dark, Theme::System] {
            let mut s = Settings::default();
            s.view.options.theme = theme;
            s.save().unwrap();
            assert_eq!(Settings::load().settings.view.options.theme, theme);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ui9_changing_the_theme_setting_switches_the_visuals() {
    let mut h = harness(app_with_theme(Theme::Dark));
    h.run_steps(4);
    assert!(dark_mode(&h));

    h.state_mut().settings.view.options.theme = Theme::Light;
    h.run_steps(4);
    assert!(!dark_mode(&h));

    h.state_mut().settings.view.options.theme = Theme::Dark;
    h.run_steps(4);
    assert!(dark_mode(&h));
}

#[test]
fn ui9_system_theme_follows_the_operating_system() {
    let mut h = harness(app_with_theme(Theme::System));
    h.input_mut().system_theme = Some(egui::Theme::Light);
    h.run_steps(4);
    assert!(!dark_mode(&h), "system light");
    h.input_mut().system_theme = Some(egui::Theme::Dark);
    h.run_steps(4);
    assert!(dark_mode(&h), "system dark");
}

#[test]
fn ui9_forced_theme_ignores_the_operating_system() {
    let mut h = harness(app_with_theme(Theme::Light));
    h.input_mut().system_theme = Some(egui::Theme::Dark);
    h.run_steps(4);
    assert!(!dark_mode(&h));
}

#[test]
fn ui9_options_dialog_picks_the_theme_and_save_applies_it() {
    let mut app = app_with_theme(Theme::Dark);
    app.open_options_dialog();
    let mut h = harness(app);
    h.run_steps(4);
    assert!(dark_mode(&h));
    h.get_by_label(tr(Lang::Ru, Tr::ThemeLight)).click();
    h.run_steps(4);
    // Only the draft changes until Save.
    assert_eq!(h.state().settings.view.options.theme, Theme::Dark);
    assert_eq!(h.state().dialogs.options.draft.theme, Theme::Light);
    h.get_by_label(tr(Lang::Ru, Tr::Save)).click();
    h.run_steps(6);
    assert_eq!(h.state().settings.view.options.theme, Theme::Light);
    assert!(!dark_mode(&h));
}

#[test]
fn ui9_theme_labels_exist_in_every_language() {
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        for key in [
            Tr::ThemeTitle,
            Tr::ThemeSystem,
            Tr::ThemeLight,
            Tr::ThemeDark,
            Tr::ThemeHint,
        ] {
            assert!(!tr(lang, key).is_empty());
        }
    }
}

#[test]
fn ui9_options_dialog_keeps_save_reachable_in_a_short_window() {
    let mut app = fresh_app();
    app.open_options_dialog();
    app.settings.set_lang(Lang::Ru);
    let mut h = Harness::builder()
        .with_size([1000.0, 520.0])
        .with_max_steps(60)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app| opcda_egui::ui::draw(app, ui), app);
    h.run_steps(5);
    let save = h.get_by_label(tr(Lang::Ru, Tr::Save)).rect();
    assert!(
        save.bottom() <= 520.0 && save.top() >= 0.0,
        "Save button {save:?} must stay inside the 520 px window"
    );
}

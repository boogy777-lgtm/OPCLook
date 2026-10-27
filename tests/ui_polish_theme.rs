//! UI polish, stream `theme` (UI7-UI9): bottom panel layout, type/spacing
//! scale and the light/dark/system theme. Headless egui_kittest tests.
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
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
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveJournalLabel))
            .is_some()
    );
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
    app.settings.options.show_script = false;
    app.settings.options.show_log = true;
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveScriptLabel))
            .is_none()
    );
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveJournalLabel))
            .is_some()
    );
    // Script-only: script toolbar shown, no journal toolbar.
    let mut app = fresh_app();
    app.settings.options.show_script = true;
    app.settings.options.show_log = false;
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveScriptLabel))
            .is_some()
    );
    assert!(
        h.query_by_label(tr(Lang::Ru, Tr::SaveJournalLabel))
            .is_none()
    );
}

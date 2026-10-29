use egui::Key;
use egui_kittest::{Harness, kittest::Queryable};
use opcda_egui::{
    app::{App, BottomTab},
    i18n::Lang,
};

fn harness(width: f32) -> Harness<'static, App> {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.set_lang(Lang::Ru);
    Harness::builder()
        .with_size([width, 900.0])
        .with_max_steps(60)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app| opcda_egui::ui::draw(app, ui), app)
}

#[test]
fn header_keyboard_opens_real_menu_and_runs_existing_action() {
    for width in [480.0, 1200.0] {
        let mut h = harness(width);
        h.run_steps(3);
        h.key_press(Key::Tab);
        h.run_steps(2);
        h.key_press(Key::ArrowRight);
        h.run_steps(2);
        h.key_press(Key::ArrowDown);
        h.run_steps(3);
        assert!(h.query_by_label("Палитра команд").is_some());
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(h.state().dialogs.palette.open);
    }
}

#[test]
fn tab_leaves_header_and_closes_open_popup_without_action() {
    let mut h = harness(1200.0);
    h.run_steps(3);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    assert!(h.query_by_label("Палитра команд").is_some());
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert!(h.query_by_label("Палитра команд").is_none());
    assert!(!h.state().dialogs.palette.open);
}

#[test]
fn footer_tabs_and_script_editor_work_with_keyboard() {
    let mut h = harness(1200.0);
    h.run_steps(3);
    for _ in 0..4 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    assert_eq!(h.state().bottom_tab, BottomTab::Journal);
    h.key_press(Key::ArrowRight);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().bottom_tab, BottomTab::Script);
    for _ in 0..3 {
        h.key_press(Key::ArrowDown);
        h.run_steps(2);
    }
    assert!(
        h.ctx.text_edit_focused(),
        "script editor must be reachable with arrows"
    );
    h.event(egui::Event::Text("keyboard-test".into()));
    h.run_steps(2);
    assert!(h.state().script_text.contains("keyboard-test"));
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.state().script_text.contains("keyboard-test"));
    assert!(!h.ctx.text_edit_focused());
    h.key_press(Key::ArrowDown);
    h.run_steps(3);
    assert!(h.query_by_label("Как в системе").is_some());
}

#[test]
fn footer_arrows_do_not_escape_into_adjacent_regions() {
    let mut h = harness(1200.0);
    h.run_steps(3);
    for _ in 0..4 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    for _ in 0..10 {
        h.key_press(Key::ArrowUp);
        h.run_steps(2);
    }
    let boundary = h.ctx.memory(|memory| memory.focused());
    assert!(boundary.is_some());
    h.key_press(Key::ArrowUp);
    h.run_steps(2);
    assert_eq!(h.ctx.memory(|memory| memory.focused()), boundary);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(!h.state().dialogs.connect.open);
    assert!(!h.state().dialogs.palette.open);
    assert!(!h.state().dialogs.write.open);
}

#[test]
fn clicking_footer_does_not_leave_a_background_button_armed() {
    let mut h = harness(1200.0);
    h.run_steps(3);
    h.get_by_label("Добавить сервер").focus();
    h.run_steps(2);
    let pos = h.get_by_label("Готово").rect().center();
    h.event(egui::Event::PointerMoved(pos));
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(!h.state().dialogs.connect.open);
}

#[test]
fn footer_is_focusable_when_bottom_panels_are_hidden() {
    let mut h = harness(1200.0);
    h.state_mut().settings.view.options.show_log = false;
    h.state_mut().settings.view.options.show_script = false;
    h.run_steps(3);
    for _ in 0..4 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    assert!(h.ctx.memory(|memory| memory.focused().is_some()));
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press(Key::ArrowDown);
    h.run_steps(3);
    assert!(h.query_by_label("Как в системе").is_some());
}

#[test]
fn header_arrows_choose_existing_menu_items_before_enter() {
    for width in [480.0, 1200.0] {
        let mut h = harness(width);
        h.run_steps(3);
        h.key_press(Key::Tab);
        h.run_steps(2);
        h.key_press(Key::ArrowRight);
        h.run_steps(2);
        h.key_press(Key::ArrowDown);
        h.run_steps(3);
        h.key_press(Key::ArrowDown);
        h.run_steps(2);
        h.key_press(Key::ArrowUp);
        h.run_steps(2);
        h.key_press(Key::ArrowDown);
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(4);
        assert!(
            h.state().dialogs.connect.open,
            "second Client menu item on width {width}"
        );
        assert!(!h.state().dialogs.palette.open);
    }
}

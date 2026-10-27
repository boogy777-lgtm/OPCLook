//! UI polish of the signal table (UI1, UI2, UI4, UI5, UI10), headless via
//! egui_kittest: accessibility labels, clicks and the painted shapes.
use egui::{Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use opcda_egui::app::App;
use opcda_egui::bff::model::{
    DataValue, EndpointSpec, EventKind, ItemKey, Quality, QualityKind, Value,
};

fn harness(app: App) -> Harness<'static, App> {
    let mut app = app;
    app.settings.set_lang(opcda_egui::i18n::Lang::Ru);
    Harness::builder()
        .with_size([1200.0, 900.0])
        .with_max_steps(60)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, app| {
                opcda_egui::ui::draw(app, ui);
            },
            app,
        )
}

fn pointer_click(h: &Harness<'_, App>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

fn pump_value() -> Value {
    Value::Struct {
        type_name: "PumpType".into(),
        fields: vec![
            ("Speed".into(), Value::F64(12.5)),
            (
                "Motor".into(),
                Value::Struct {
                    type_name: "MotorType".into(),
                    fields: vec![("Current".into(), Value::F64(3.25))],
                },
            ),
        ],
    }
}

/// UI1: the expander is a painted triangle with a localized accessible name
/// that says what a click does; the name flips with the state and a click on
/// the triangle still toggles (the row owns the pointer).
#[test]
fn ui1_value_expander_is_labelled_and_toggles_by_click() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let key = app.seed_row(1, "ns=2;s=Pump", "Pump", pump_value());
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label("Свернуть значение").is_none());

    let at = h.get_by_label("Развернуть значение").rect().center();
    pointer_click(&h, at);
    h.run_steps(5);
    assert!(h.state().expanded_values.contains(&(key, vec![])));
    // The row's own expander now reads "collapse"; the nested one is closed.
    assert_eq!(h.get_all_by_label("Свернуть значение").count(), 1);
    assert_eq!(h.get_all_by_label("Развернуть значение").count(), 1);

    let at = h.get_by_label("Свернуть значение").rect().center();
    pointer_click(&h, at);
    h.run_steps(5);
    assert!(h.state().expanded_values.is_empty());
    assert!(h.query_by_label("Свернуть значение").is_none());
}

/// UI1: the expander is drawn with the painter (a polygon), never as a `>` / `v`
/// text glyph.
#[test]
fn ui1_value_expander_is_a_painted_triangle_not_text() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_row(1, "ns=2;s=Pump", "Pump", pump_value());
    let mut h = harness(app);
    h.run_steps(3);
    let rect = h.get_by_label("Развернуть значение").rect();
    let in_expander = |s: &egui::Shape| match s {
        egui::Shape::Mesh(m) => m.vertices.iter().any(|v| rect.contains(v.pos)),
        egui::Shape::Path(p) => p.points.iter().any(|pt| rect.contains(*pt)),
        egui::Shape::Text(t) => rect.contains(t.pos),
        _ => false,
    };
    let shapes = flat_shapes(&h);
    assert!(
        shapes
            .iter()
            .any(|s| matches!(s, egui::Shape::Path(_) | egui::Shape::Mesh(_)) && in_expander(s)),
        "a painted polygon sits in the expander rect"
    );
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(s, egui::Shape::Text(_)) && in_expander(s)),
        "no text glyph inside the expander rect"
    );
}

fn flat_shapes(h: &Harness<'_, App>) -> Vec<egui::Shape> {
    fn walk(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(items) => items.iter().for_each(|s| walk(s, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

/// Colour a text shape is drawn in (explicit section colour, else the fallback).
fn text_colors(shapes: &[egui::Shape], text: &str) -> Vec<egui::Color32> {
    shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(t) if t.galley.text() == text => {
                let section = t
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|sec| sec.format.color)
                    .filter(|c| *c != egui::Color32::PLACEHOLDER);
                Some(
                    t.override_text_color
                        .or(section)
                        .unwrap_or(t.fallback_color),
                )
            }
            _ => None,
        })
        .collect()
}

fn rect_fills(shapes: &[egui::Shape]) -> Vec<egui::Color32> {
    shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) => Some(r.fill),
            _ => None,
        })
        .collect()
}

fn quality(kind: QualityKind) -> Quality {
    let (code, text) = match kind {
        QualityKind::Good => (0xC0, "GOOD"),
        QualityKind::Uncertain => (0x40, "UNCERTAIN"),
        QualityKind::Bad => (0x00, "BAD"),
    };
    Quality {
        code,
        kind,
        text: text.to_string(),
    }
}

/// Deliver a value update for a row of session 1 (as the backend would).
fn push_value(app: &mut App, key: ItemKey, value: Value, kind: QualityKind) {
    app.on_event_for_test_at(
        1,
        EventKind::Values {
            values: vec![(
                key,
                DataValue {
                    value,
                    quality: quality(kind),
                    timestamp: None,
                    reason: None,
                },
            )],
        },
    );
}

fn two_quality_rows(show_colors: bool) -> Harness<'static, App> {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.options.show_colors = show_colors;
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let good = app.seed_row(1, "ns=2;s=A", "Level", Value::F64(21.5));
    let bad = app.seed_row(1, "ns=2;s=B", "Pressure", Value::F64(7.25));
    push_value(&mut app, good, Value::F64(21.5), QualityKind::Good);
    push_value(&mut app, bad, Value::F64(7.25), QualityKind::Bad);
    let mut h = harness(app);
    h.run_steps(3);
    h
}

/// UI2: the quality is a pill (its colour on a soft background of the same
/// colour); the value stays in the neutral text colour.
#[test]
fn ui2_quality_is_a_pill_and_the_value_is_neutral() {
    let h = two_quality_rows(true);
    let shapes = flat_shapes(&h);
    let neutral = h.ctx.global_style().visuals.text_color();

    for value in ["21.5", "7.25"] {
        let colors = text_colors(&shapes, value);
        assert!(!colors.is_empty(), "value {value} is drawn");
        assert!(colors.iter().all(|c| *c == neutral), "{value} is neutral");
    }
    let good = text_colors(&shapes, "GOOD");
    let bad = text_colors(&shapes, "BAD");
    assert_eq!((good.len(), bad.len()), (1, 1));
    assert_ne!(good[0], neutral);
    assert_ne!(bad[0], neutral);
    assert_ne!(good[0], bad[0], "good and bad differ in colour");
    let fills = rect_fills(&shapes);
    assert!(
        fills.contains(&good[0].gamma_multiply(0.18)),
        "a soft pill behind GOOD"
    );
    assert!(
        fills.contains(&bad[0].gamma_multiply(0.18)),
        "a soft pill behind BAD"
    );
}

/// UI2: «show colors» off: the pill and the dot are neutral, no tint at all.
#[test]
fn ui2_show_colors_off_makes_the_pill_neutral() {
    let on = flat_shapes(&two_quality_rows(true));
    let good_color = text_colors(&on, "GOOD")[0];

    let h = two_quality_rows(false);
    let shapes = flat_shapes(&h);
    let neutral = h.ctx.global_style().visuals.text_color();
    assert_eq!(text_colors(&shapes, "GOOD"), vec![neutral]);
    assert_eq!(text_colors(&shapes, "BAD"), vec![neutral]);
    assert!(
        !rect_fills(&shapes).contains(&good_color.gamma_multiply(0.18)),
        "no coloured pill when colours are off"
    );
}

/// (left x, right x, monospace) of the single text shape drawn as `text`.
fn text_extent(shapes: &[egui::Shape], text: &str) -> (f32, f32, bool) {
    let found: Vec<_> = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "exactly one {text:?} drawn");
    let t = found[0];
    let mono = t
        .galley
        .job
        .sections
        .first()
        .is_some_and(|sec| sec.format.font_id.family == egui::FontFamily::Monospace);
    // A right-aligned galley spans negative x around its anchor `pos`.
    (
        t.pos.x + t.galley.rect.min.x,
        t.pos.x + t.galley.rect.max.x,
        mono,
    )
}

/// UI5: numbers are monospace and right-aligned in the Value column; strings
/// and dates keep the left edge (proportional text).
#[test]
fn ui5_numbers_are_right_aligned_and_monospace() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_row(1, "ns=2;s=A", "Count", Value::I64(7));
    app.seed_row(1, "ns=2;s=B", "Ratio", Value::F64(0.5));
    app.seed_row(1, "ns=2;s=C", "Big", Value::U64(9));
    app.seed_row(1, "ns=2;s=D", "Name", Value::Str("Pump".into()));
    app.seed_row(1, "ns=2;s=E", "Flag", Value::Bool(true));
    let mut h = harness(app);
    h.run_steps(3);
    let shapes = flat_shapes(&h);
    let (name_left, _, name_mono) = text_extent(&shapes, "Pump");
    let (flag_left, _, _) = text_extent(&shapes, "true");
    assert!(!name_mono, "a string is proportional");
    assert!((flag_left - name_left).abs() < 0.5, "left edge shared");
    // The Value column is 170 wide: a one-glyph number sits at its right edge.
    for number in ["7", "0.5", "9"] {
        let (left, right, mono) = text_extent(&shapes, number);
        assert!(mono, "{number} is monospace");
        assert!(
            left > name_left + 100.0,
            "{number} is far from the left edge"
        );
        let width = right - name_left;
        assert!(
            (152.0..=167.0).contains(&width),
            "{number} ends at the cell's right edge, got {width}"
        );
    }
}

/// UI5: the same rule for the field lines of an expanded structure.
#[test]
fn ui5_field_lines_follow_the_same_alignment() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let key = app.seed_row(
        1,
        "ns=2;s=Pump",
        "Pump",
        Value::Struct {
            type_name: "PumpType".into(),
            fields: vec![
                ("Speed".into(), Value::F64(12.5)),
                ("Label".into(), Value::Str("Pump one".into())),
            ],
        },
    );
    app.toggle_value(key, vec![]);
    let mut h = harness(app);
    h.run_steps(3);
    let shapes = flat_shapes(&h);
    let (label_left, _, label_mono) = text_extent(&shapes, "Pump one");
    let (speed_left, speed_right, speed_mono) = text_extent(&shapes, "12.5");
    assert!(!label_mono && speed_mono);
    assert!(speed_left > label_left + 100.0);
    assert!((152.0..=167.0).contains(&(speed_right - label_left)));
}

fn changed_at(app: &App, key: ItemKey) -> Option<std::time::Instant> {
    app.rows
        .iter()
        .find(|r| r.key == key)
        .and_then(|r| r.changed_at)
}

/// UI4: a new value stamps `changed_at`; the same value arriving again does not.
#[test]
fn ui4_changed_value_is_stamped_and_the_same_value_is_not() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let key = app.seed_row(1, "ns=2;s=A", "Level", Value::F64(1.0));
    assert!(changed_at(&app, key).is_none());

    push_value(&mut app, key, Value::F64(1.0), QualityKind::Good);
    assert!(changed_at(&app, key).is_none(), "same value: no flash");

    push_value(&mut app, key, Value::F64(2.0), QualityKind::Good);
    assert!(changed_at(&app, key).is_some(), "new value: flash");
}

/// UI4: the first value of a freshly added row (pending / waiting / empty) is
/// not a change; the next different one is. NaN delivered again is not either.
#[test]
fn ui4_first_value_after_adding_a_row_is_not_highlighted() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let key = app.seed_row(1, "ns=2;s=A", "Level", Value::Empty);
    // Still pending: the first value arrives.
    app.rows[0].pending = true;
    push_value(&mut app, key, Value::F64(5.0), QualityKind::Good);
    assert!(changed_at(&app, key).is_none(), "pending → value: no flash");
    // Registered but waiting for the first value (placeholder quality).
    app.rows[0].value = Value::Empty;
    app.rows[0].quality = Quality {
        code: Quality::CODE_PLACEHOLDER_WAITING,
        kind: QualityKind::Good,
        text: "waiting".into(),
    };
    push_value(&mut app, key, Value::F64(6.0), QualityKind::Good);
    assert!(changed_at(&app, key).is_none(), "waiting → value: no flash");

    push_value(&mut app, key, Value::F64(7.0), QualityKind::Good);
    assert!(changed_at(&app, key).is_some(), "later change: flash");

    app.rows[0].changed_at = None;
    push_value(&mut app, key, Value::F64(f64::NAN), QualityKind::Good);
    app.rows[0].changed_at = None;
    push_value(&mut app, key, Value::F64(f64::NAN), QualityKind::Good);
    assert!(
        changed_at(&app, key).is_none(),
        "NaN again is the same value"
    );
}

/// UI4: the glow fades out over ~1 s.
#[test]
fn ui4_flash_strength_fades_over_a_second() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let key = app.seed_row(1, "ns=2;s=A", "Level", Value::F64(1.0));
    let now = std::time::Instant::now();
    assert_eq!(app.rows[0].change_flash(now), 0.0, "never changed");
    app.rows[0].changed_at = Some(now);
    assert!(app.rows[0].change_flash(now) > 0.99);
    let later = |ms: u64| now + std::time::Duration::from_millis(ms);
    let (a, b) = (
        app.rows[0].change_flash(later(250)),
        app.rows[0].change_flash(later(600)),
    );
    assert!(a > b && b > 0.0 && a < 1.0, "monotonic fade: {a} > {b}");
    assert_eq!(app.rows[0].change_flash(later(1000)), 0.0);
    assert_eq!(app.rows[0].change_flash(later(5000)), 0.0);
    let _ = key;
}

fn repaint_delay(h: &Harness<'_, App>) -> std::time::Duration {
    h.output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|v| v.repaint_delay)
        .expect("root viewport output")
}

/// UI4: a just-changed value is painted with an accent glow behind it and the
/// table keeps requesting frames while it fades; afterwards both are gone.
#[test]
fn ui4_changed_value_cell_glows_and_requests_repaints_until_it_fades() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let key = app.seed_row(1, "ns=2;s=A", "Level", Value::F64(1.0));
    let mut h = harness(app);
    h.run_steps(3);
    let calm = rect_fills(&flat_shapes(&h));

    push_value(h.state_mut(), key, Value::F64(2.0), QualityKind::Good);
    h.step();
    let glow = rect_fills(&flat_shapes(&h));
    let accent = h.ctx.global_style().visuals.selection.bg_fill;
    let new_fills: Vec<_> = glow.iter().filter(|c| !calm.contains(c)).collect();
    assert!(
        new_fills
            .iter()
            .any(|c| c.a() > 0 && c.r() <= accent.r() && c.b() > c.r()),
        "an accent-tinted rect appears: {new_fills:?}"
    );
    assert_eq!(
        repaint_delay(&h),
        std::time::Duration::ZERO,
        "keeps repainting"
    );

    // Pretend the change happened two seconds ago: no glow, no repaint request.
    h.state_mut().rows[0].changed_at =
        std::time::Instant::now().checked_sub(std::time::Duration::from_secs(2));
    h.step();
    h.step();
    assert_eq!(rect_fills(&flat_shapes(&h)), calm, "glow is gone");
    assert_ne!(repaint_delay(&h), std::time::Duration::ZERO, "idle again");
}

const CONNECT_BUTTON: &str = "Подключиться (Ctrl+N)";
const DRAG_HINT: &str = "Перетащите теги из дерева или нажмите Enter на узле";

fn empty_harness(with_session: bool) -> Harness<'static, App> {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    if with_session {
        app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
        app.activate_session(1);
    }
    let mut h = harness(app);
    h.run_steps(3);
    h
}

/// UI10: no session: the hint, the keys line and a connect button that opens
/// the connect dialog.
#[test]
fn ui10_empty_table_without_session_offers_connect() {
    let mut h = empty_harness(false);
    assert!(h.query_by_label_contains("Нет сигналов").is_some());
    assert!(
        h.query_by_label_contains("Ctrl+N — подключиться к серверу")
            .is_some(),
        "keys line"
    );
    assert!(h.query_by_label(DRAG_HINT).is_none());
    assert!(!h.state().dialogs.connect.open);
    let at = h.get_by_label(CONNECT_BUTTON).rect().center();
    pointer_click(&h, at);
    h.run_steps(3);
    assert!(
        h.state().dialogs.connect.open,
        "the button opens the dialog"
    );
}

/// UI10: with a session the table says how to fill it and has no button.
#[test]
fn ui10_empty_table_with_session_explains_how_to_fill_it() {
    let h = empty_harness(true);
    assert!(h.query_by_label_contains("Нет сигналов").is_some());
    assert!(h.query_by_label(DRAG_HINT).is_some());
    assert!(h.query_by_label(CONNECT_BUTTON).is_none());
}

/// UI10: the hint is bigger than the old 15 pt painted text.
#[test]
fn ui10_empty_hint_is_larger() {
    let h = empty_harness(true);
    let size = flat_shapes(&h)
        .iter()
        .find_map(|s| match s {
            egui::Shape::Text(t) if t.galley.text().starts_with("Нет сигналов") => t
                .galley
                .job
                .sections
                .first()
                .map(|sec| sec.format.font_id.size),
            _ => None,
        })
        .expect("hint is drawn");
    assert!(size >= 17.0, "hint size {size}");
}

/// UI10: the button never keeps egui's focus after a press, so Enter does not
/// press it again (Enter belongs to the table), and a click beside it on the
/// hint text does nothing.
#[test]
fn ui10_connect_button_does_not_keep_the_keyboard() {
    let mut h = empty_harness(false);
    // A click on the hint text opens nothing.
    let hint = h.get_by_label_contains("Нет сигналов").rect();
    pointer_click(&h, hint.center());
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open, "text is not a button");

    let at = h.get_by_label(CONNECT_BUTTON).rect().center();
    pointer_click(&h, at);
    h.run_steps(3);
    assert!(h.state().dialogs.connect.open);
    h.state_mut().dialogs.connect.open = false;
    h.run_steps(3);
    assert!(h.ctx.memory(|m| m.focused()).is_none(), "no focused widget");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open, "Enter did not re-press it");
}

/// UI10: even when egui focuses the button (an accessibility focus request),
/// it gives the focus back at once, so Enter cannot press it.
#[test]
fn ui10_a_focused_connect_button_gives_the_focus_back() {
    let mut h = empty_harness(false);
    h.get_by_label(CONNECT_BUTTON).focus();
    h.run_steps(3);
    assert!(h.ctx.memory(|m| m.focused()).is_none(), "focus surrendered");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open, "Enter did not press it");
}

/// UI10: Enter in the table area with no rows still does nothing harmful.
#[test]
fn ui10_enter_with_an_empty_table_opens_nothing() {
    let mut h = empty_harness(true);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open);
    assert!(!h.state().dialogs.any_open());
}

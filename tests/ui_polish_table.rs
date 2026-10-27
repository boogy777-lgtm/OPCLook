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

//! UI polish of the signal table (UI1, UI2, UI4, UI5, UI10), headless via
//! egui_kittest: accessibility labels, clicks and the painted shapes.
use egui::{Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use opcda_egui::app::App;
use opcda_egui::bff::model::{EndpointSpec, Value};

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

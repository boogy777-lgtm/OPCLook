#![cfg(feature = "ui-snapshots")]
//! Reproducible README image: the app is rendered with synthetic fixture data.
use egui_kittest::Harness;
use opcda_egui::app::App;
use opcda_egui::bff::model::{BrowseNode, EndpointSpec, NodeKind, SessionId, Value};
use opcda_egui::i18n::Lang;
use opcda_egui::settings::Theme;

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn render_readme_main_window() {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.set_lang(Lang::Ru);
    app.settings.view.options.theme = Theme::Light;
    app.settings.view.options.update_interval_secs = 86_400;
    app.settings.view.options.test_interval_secs = 0;
    app.settings.view.options.auto_reconnect = false;
    let session = SessionId::new(1).unwrap();
    app.seed_session(session, EndpointSpec::da("", "Demo.OPC"));
    for (id, name, value) in [
        ("Plant.Tank.Level", "Plant.Tank.Level", Value::F64(67.4)),
        (
            "Plant.Pump.Pressure",
            "Plant.Pump.Pressure",
            Value::F64(4.2),
        ),
        ("Plant.Pump.Flow", "Plant.Pump.Flow", Value::F64(125.0)),
        (
            "Plant.Pump.Running",
            "Plant.Pump.Running",
            Value::Bool(true),
        ),
        (
            "Plant.Valve.Position",
            "Plant.Valve.Position",
            Value::F64(52.0),
        ),
        (
            "Plant.Tank.Temperature",
            "Plant.Tank.Temperature",
            Value::F64(24.8),
        ),
    ] {
        app.seed_writable_row(session, id, name, value);
        app.seed_tree_item(
            session,
            None,
            0,
            BrowseNode {
                node_id: id.to_owned(),
                name: name.to_owned(),
                kind: NodeKind::Item,
                has_children: false,
            },
        );
    }
    app.journal
        .info("Демонстрационные данные: реального подключения нет");
    let mut h = Harness::builder()
        .with_size([1500.0, 900.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opcda_egui::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), app);
    h.run_steps(8);
    h.render()
        .unwrap()
        .save("docs/images/main-window.png")
        .unwrap();
}

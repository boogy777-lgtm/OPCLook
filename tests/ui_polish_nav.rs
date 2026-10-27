//! UI polish of the navigation area (UI1 tree expander, UI3 session tabs,
//! UI6 left panel, UI10 empty states), headless through egui_kittest.
use egui_kittest::{Harness, kittest::Queryable};
use opcda_egui::app::App;
use opcda_egui::bff::model::{BrowseNode, EndpointSpec, NodeKind};

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

fn node(id: &str, name: &str, kind: NodeKind, has_children: bool) -> BrowseNode {
    BrowseNode {
        node_id: id.to_string(),
        name: name.to_string(),
        kind,
        has_children,
    }
}

// ---------------------------------------------------------------- UI1

/// UI1: the tree expander is a painted triangle with its own accessible name
/// (no «>» / «v» glyph), and a click on it toggles the node.
#[test]
fn ui1_tree_expander_is_painted_and_toggles() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(1, None, 0, node("b", "Ветка", NodeKind::Branch, true));
    app.seed_tree_item(
        1,
        Some(branch),
        1,
        node("c", "Сигнал", NodeKind::Item, false),
    );
    let mut h = harness(app);
    h.run_steps(3);

    assert_eq!(h.query_all_by_label(">").count(), 0, "no text arrow");
    assert_eq!(h.query_all_by_label("v").count(), 0, "no text arrow");
    assert!(h.query_by_label("Сигнал").is_none(), "collapsed at first");

    h.get_by_label("Развернуть").click();
    h.run_steps(3);
    let expanded = |h: &Harness<'_, App>| {
        h.state()
            .sessions
            .iter()
            .find(|s| s.id == 1)
            .and_then(|s| s.tree.iter().find(|t| t.id == branch))
            .map(|t| t.expanded)
    };
    assert_eq!(expanded(&h), Some(true));
    assert!(h.query_by_label("Сигнал").is_some(), "child shown");

    h.get_by_label("Свернуть").click();
    h.run_steps(3);
    assert_eq!(expanded(&h), Some(false));
    assert!(h.query_by_label("Сигнал").is_none());
}

/// UI1: a click on the node row selects it (the row is still one widget), a
/// double click on an Item adds it; the expander does not select.
#[test]
fn ui1_tree_row_click_selects_and_expander_does_not() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(1, None, 0, node("b", "Ветка", NodeKind::Branch, true));
    let mut h = harness(app);
    h.run_steps(3);

    h.get_by_label("Развернуть").click();
    h.run_steps(3);
    assert_eq!(h.state().selected_tree, None, "the expander only toggles");

    h.get_by_label("Ветка").click();
    h.run_steps(3);
    assert_eq!(h.state().selected_tree, Some((1, branch)));
}

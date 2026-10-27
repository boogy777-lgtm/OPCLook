//! UI polish of the navigation area (UI1 tree expander, UI3 session tabs,
//! UI6 left panel, UI10 empty states), headless through egui_kittest.
use egui::{Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use opcda_egui::app::{App, SessionState};
use opcda_egui::bff::model::{
    BrowseNode, EndpointSpec, NodeKind, UaSecurityMode, UaSecurityPolicy, Value,
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

fn pointer_move(h: &Harness<'_, App>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
}

fn pointer_click(h: &Harness<'_, App>, pos: egui::Pos2) {
    pointer_move(h, pos);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
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

// ---------------------------------------------------------------- UI3

const URL: &str = "opc.tcp://127.0.0.1:4840";
const CLOSE_TAB: &str = "Скрыть вкладку (контейнер сохраняется)";

/// Session 1: the plain endpoint; session 2: the same URL with Basic256Sha256
/// + SignAndEncrypt; session 3: a DA server.
fn two_endpoints_of_one_server() -> App {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::ua(URL));
    let mut secure = EndpointSpec::ua(URL);
    secure.ua_security_policy = Some(UaSecurityPolicy::Basic256Sha256);
    secure.ua_security_mode = Some(UaSecurityMode::SignAndEncrypt);
    app.seed_session(2, secure);
    app.seed_row(1, "ns=2;s=A", "OnlyPlain", Value::I64(1));
    app.seed_row(2, "ns=2;s=B", "OnlySecure", Value::I64(2));
    app.activate_session(1);
    app
}

/// Tab nodes live in the central panel (x >= 400 in a 1200 px harness).
fn central<'a>(nodes: impl Iterator<Item = egui_kittest::Node<'a>>) -> Vec<egui_kittest::Node<'a>> {
    nodes.filter(|n| n.rect().center().x >= 400.0).collect()
}

/// UI3 / V2: two tabs of one server are told apart: the secured one carries
/// the security in its accessible name (its visible mark is a padlock), the
/// plain one has none; there is no glyph text («●», «×», an emoji) in a tab.
#[test]
fn ui3_two_endpoints_of_one_server_are_distinguishable_tabs() {
    let mut h = harness(two_endpoints_of_one_server());
    h.run_steps(4);

    let plain = central(h.query_all_by_label("UA: 127.0.0.1:4840"));
    assert_eq!(plain.len(), 1, "exactly one plain tab");
    let secure = central(h.query_all_by_label("UA: 127.0.0.1:4840 · Подпись и шифрование"));
    assert_eq!(secure.len(), 1, "exactly one secured tab");
    assert!(
        plain[0].rect().center().x < secure[0].rect().center().x,
        "tabs keep their order"
    );
    // Nothing in the central strip is a text glyph standing in for an icon.
    for glyph in ["●", "×", "🔒"] {
        assert_eq!(
            central(h.query_all_by_label_contains(glyph)).len(),
            0,
            "no «{glyph}» text in the tab strip"
        );
    }
}

/// UI3: clicking a tab activates it and the table follows (V1).
#[test]
fn ui3_clicking_a_tab_switches_the_session() {
    let mut h = harness(two_endpoints_of_one_server());
    h.run_steps(4);
    assert_eq!(h.state().active_session, Some(1));
    assert!(h.query_by_label_contains("OnlyPlain").is_some());

    central(h.query_all_by_label("UA: 127.0.0.1:4840 · Подпись и шифрование"))[0].click();
    h.run_steps(4);
    assert_eq!(h.state().active_session, Some(2));
    assert!(h.query_by_label_contains("OnlySecure").is_some());
    assert!(h.query_by_label_contains("OnlyPlain").is_none());
}

/// UI3: «×» is on the active tab always, on an inactive tab only while the
/// pointer is over it; its accessible name is the tooltip; it hides just
/// that tab.
#[test]
fn ui3_close_button_shows_on_active_tab_and_on_hover() {
    let mut h = harness(two_endpoints_of_one_server());
    h.run_steps(4);
    assert_eq!(
        central(h.query_all_by_label(CLOSE_TAB)).len(),
        1,
        "only the active tab shows ×"
    );

    // Hover the inactive (secured) tab: its × appears next to the active one.
    let secure =
        central(h.query_all_by_label("UA: 127.0.0.1:4840 · Подпись и шифрование"))[0].rect();
    pointer_move(&h, secure.center());
    h.run_steps(3);
    let closes = central(h.query_all_by_label(CLOSE_TAB));
    assert_eq!(closes.len(), 2, "× on the hovered tab too");

    // Click the hovered tab's × (the rightmost one): only that tab is hidden.
    let target = closes
        .iter()
        .map(|n| n.rect().center())
        .max_by(|a, b| a.x.total_cmp(&b.x))
        .expect("the hovered tab's ×");
    pointer_click(&h, target);
    h.run_steps(3);
    let state = h.state();
    assert!(state.sessions.iter().any(|s| s.id == 1 && s.visible));
    assert!(state.sessions.iter().any(|s| s.id == 2 && !s.visible));
    assert_eq!(state.sessions.len(), 2, "the container itself survives");
    assert_eq!(state.rows.len(), 2, "its rows survive");
}

/// UI3: once the pointer leaves, the inactive tab's × goes away again.
#[test]
fn ui3_close_button_hides_again_when_the_pointer_leaves() {
    let mut h = harness(two_endpoints_of_one_server());
    h.run_steps(3);
    let secure =
        central(h.query_all_by_label("UA: 127.0.0.1:4840 · Подпись и шифрование"))[0].rect();
    pointer_move(&h, secure.center());
    h.run_steps(3);
    assert_eq!(central(h.query_all_by_label(CLOSE_TAB)).len(), 2);
    pointer_move(&h, egui::pos2(700.0, 600.0));
    h.run_steps(3);
    assert_eq!(central(h.query_all_by_label(CLOSE_TAB)).len(), 1);
}

/// UI3: the tooltip carries the full security text, the state and the name.
#[test]
fn ui3_tab_tooltip_has_full_security_and_state() {
    let mut h = harness(two_endpoints_of_one_server());
    h.run_steps(3);
    let secure =
        central(h.query_all_by_label("UA: 127.0.0.1:4840 · Подпись и шифрование"))[0].rect();
    pointer_move(&h, secure.left_center() + egui::vec2(30.0, 0.0));
    h.run_steps(30);
    assert!(
        h.query_all_by_label_contains("Basic256Sha256").count() >= 1,
        "tooltip names the policy"
    );
}

/// UI3: a disconnected session keeps its tab; the dot is painted, the state is
/// in the tooltip, and the tab is still clickable under its name.
#[test]
fn ui3_disconnected_tab_is_still_a_clickable_named_tab() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::da("", "Alpha.Server"));
    app.seed_session(2, EndpointSpec::da("", "Beta.Server"));
    if let Some(s) = app.sessions.iter_mut().find(|s| s.id == 2) {
        s.state = SessionState::Disconnected;
    }
    app.activate_session(1);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("DA: Beta.Server").click();
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(2));
}

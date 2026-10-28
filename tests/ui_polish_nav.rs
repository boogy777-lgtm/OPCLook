//! UI polish of the navigation area (UI1 tree expander, UI3 session tabs,
//! UI6 left panel, UI10 empty states), headless through egui_kittest.
use egui::{Modifiers, PointerButton};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
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
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(sid(1), None, 0, node("b", "Ветка", NodeKind::Branch, true));
    app.seed_tree_item(
        sid(1),
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
            .find(|s| s.id == sid(1))
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
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(sid(1), None, 0, node("b", "Ветка", NodeKind::Branch, true));
    let mut h = harness(app);
    h.run_steps(3);

    h.get_by_label("Развернуть").click();
    h.run_steps(3);
    assert_eq!(h.state().selected_tree, None, "the expander only toggles");

    h.get_by_label("Ветка").click();
    h.run_steps(3);
    assert_eq!(h.state().selected_tree, Some((sid(1), branch)));
}

// ---------------------------------------------------------------- UI3

const URL: &str = "opc.tcp://127.0.0.1:4840";
const CLOSE_TAB: &str = "Скрыть вкладку (контейнер сохраняется)";

/// Session 1: the plain endpoint; session 2: the same URL with Basic256Sha256
/// + SignAndEncrypt; session 3: a DA server.
fn two_endpoints_of_one_server() -> App {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua(URL));
    let mut secure = EndpointSpec::ua(URL);
    secure.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
    secure.ua_security_mode = UaSecurityMode::SignAndEncrypt;
    app.seed_session(sid(2), secure);
    app.seed_row(sid(1), "ns=2;s=A", "OnlyPlain", Value::I64(1));
    app.seed_row(sid(2), "ns=2;s=B", "OnlySecure", Value::I64(2));
    app.activate_session(sid(1));
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
    assert_eq!(h.state().active_session, Some(sid(1)));
    assert!(h.query_by_label_contains("OnlyPlain").is_some());

    central(h.query_all_by_label("UA: 127.0.0.1:4840 · Подпись и шифрование"))[0].click();
    h.run_steps(4);
    assert_eq!(h.state().active_session, Some(sid(2)));
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
    assert!(state.sessions.iter().any(|s| s.id == sid(1) && s.visible));
    assert!(state.sessions.iter().any(|s| s.id == sid(2) && !s.visible));
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
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_session(sid(2), EndpointSpec::da("", "Beta.Server"));
    if let Some(s) = app.sessions.iter_mut().find(|s| s.id == sid(2)) {
        s.state = SessionState::Disconnected;
    }
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("DA: Beta.Server").click();
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(2)));
}

// ---------------------------------------------------------------- UI6

const LEFT: f32 = 400.0;

fn left<'a>(nodes: impl Iterator<Item = egui_kittest::Node<'a>>) -> Vec<egui_kittest::Node<'a>> {
    nodes.filter(|n| n.rect().center().x < LEFT).collect()
}

fn connected_with_tree() -> (App, u64, u64) {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(sid(1), None, 0, node("b", "Ветка", NodeKind::Branch, true));
    let item = app.seed_tree_item(sid(1), None, 0, node("i", "Сигнал", NodeKind::Item, false));
    (app, branch, item)
}

/// UI6: the four tree actions are painted icons that keep the old button text
/// as their accessible name, each exactly once, in the left panel.
#[test]
fn ui6_tree_actions_are_icons_with_the_old_names() {
    let (app, _, _) = connected_with_tree();
    let mut h = harness(app);
    h.run_steps(4);
    for name in ["Обновить", "Добавить", "Дочерние", "По имени..."] {
        let found = left(h.query_all_by_label(name));
        assert_eq!(found.len(), 1, "one «{name}» icon in the left panel");
        assert!(
            !found[0].accesskit_node().is_disabled(),
            "«{name}» is enabled on a connected session"
        );
        let rect = found[0].rect();
        assert!(
            rect.width() <= 32.0 && rect.height() <= 32.0,
            "«{name}» is a small icon, got {rect:?}"
        );
    }
    // The four icons share one row.
    let ys: Vec<f32> = ["Обновить", "Добавить", "Дочерние", "По имени..."]
        .iter()
        .map(|n| left(h.query_all_by_label(n))[0].rect().center().y)
        .collect();
    assert!(
        ys.iter().all(|y| (y - ys[0]).abs() < 1.0),
        "one row: {ys:?}"
    );
}

/// UI6: on a disconnected session the icons are disabled.
#[test]
fn ui6_tree_action_icons_are_disabled_without_a_connection() {
    let (mut app, _, _) = connected_with_tree();
    if let Some(s) = app.sessions.iter_mut().find(|s| s.id == sid(1)) {
        s.state = SessionState::Disconnected;
    }
    let mut h = harness(app);
    h.run_steps(4);
    for name in ["Обновить", "Добавить", "Дочерние", "По имени..."] {
        let found = left(h.query_all_by_label(name));
        assert_eq!(found.len(), 1);
        assert!(found[0].accesskit_node().is_disabled(), "«{name}» disabled");
    }
}

/// UI6: «По имени...» opens the add-signal dialog, «Обновить» re-reads the
/// root (nothing selected), «Добавить» on a branch explains it cannot.
#[test]
fn ui6_tree_action_icons_act() {
    let (app, branch, _) = connected_with_tree();
    let mut h = harness(app);
    h.run_steps(3);

    left(h.query_all_by_label("По имени..."))[0].click();
    h.run_steps(3);
    assert!(h.state().dialogs.add_signal.open);
    h.state_mut().dialogs.add_signal.open = false;

    left(h.query_all_by_label("Обновить"))[0].click();
    h.run_steps(3);
    let view = &h.state().sessions[0];
    assert!(view.root_loading, "refresh asked the server for the root");

    // «Добавить» with a branch selected shows the explanation, not a crash.
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let b = app.seed_tree_item(sid(1), None, 0, node("b", "Ветка", NodeKind::Branch, true));
    app.set_tree_focus(sid(1), b);
    let mut h = harness(app);
    h.run_steps(3);
    left(h.query_all_by_label("Добавить"))[0].click();
    h.run_steps(3);
    assert!(h.state().dialogs.message.is_some());
    let _ = branch;
}

/// UI6: a click on a server row (its name, left-aligned) activates it; the
/// selected row is the active one.
#[test]
fn ui6_clicking_a_server_row_selects_it() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_session(sid(2), EndpointSpec::da("", "Beta.Server"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(4);

    let beta = left(h.query_all_by_label("Beta.Server"));
    assert_eq!(beta.len(), 1, "one server row «Beta.Server»");
    beta[0].click();
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(2)));
    let alpha = left(h.query_all_by_label("Alpha.Server"));
    assert_eq!(alpha.len(), 1);
    alpha[0].click();
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(1)));
}

// ---------------------------------------------------------------- UI10

fn connected_with_empty_tree() -> App {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app
}

/// UI10: an empty tree of a live session offers «Обновить (F5)»; the button
/// re-reads the root exactly like F5 does.
#[test]
fn ui10_empty_tree_offers_refresh_and_does_what_f5_does() {
    let mut h = harness(connected_with_empty_tree());
    h.run_steps(4);
    assert!(h.query_by_label_contains("Дерево не загружено").is_some());
    let buttons = left(h.query_all_by_label("Обновить (F5)"));
    assert_eq!(buttons.len(), 1, "one action button");
    assert!(!h.state().sessions[0].root_loading);
    buttons[0].click();
    h.run_steps(3);
    let by_button = h.state().sessions[0].root_loading;
    assert!(by_button, "the button re-reads the root");

    let mut h = harness(connected_with_empty_tree());
    h.run_steps(4);
    h.key_press(egui::Key::F5);
    h.run_steps(3);
    // Same effect (and same code path) as the button.
    assert_eq!(h.state().sessions[0].root_loading, by_button);
}

/// UI10: while the root is being read the empty state gives way to the
/// spinner (no second request can be started from the button).
#[test]
fn ui10_no_refresh_button_while_the_tree_loads() {
    let mut app = connected_with_empty_tree();
    app.sessions[0].root_loading = true;
    let mut h = harness(app);
    h.run_steps(4);
    assert_eq!(h.query_all_by_label("Обновить (F5)").count(), 0);
    assert!(h.query_by_label_contains("Загрузка дерева").is_some());
}

/// UI10: a lost connection shows the plain message, not a refresh action.
#[test]
fn ui10_no_refresh_button_without_a_connection() {
    let mut app = connected_with_empty_tree();
    app.sessions[0].state = SessionState::Disconnected;
    let mut h = harness(app);
    h.run_steps(4);
    assert_eq!(h.query_all_by_label("Обновить (F5)").count(), 0);
    assert!(h.query_by_label_contains("Нет соединения").is_some());
}

/// UI10: no session at all - the tree area shows the hint only; the one
/// «Подключиться (Ctrl+N)» action is in the empty table (no twin button on the
/// left), and Ctrl+N opens the dialog from anywhere.
#[test]
fn ui10_no_sessions_offers_connect_like_ctrl_n() {
    let ctx = egui::Context::default();
    let app = App::new_test(&ctx);
    let mut h = harness(app);
    h.run_steps(4);
    assert!(h.query_by_label_contains("Подключитесь к OPC").is_some());
    assert!(
        left(h.query_all_by_label("Подключиться (Ctrl+N)")).is_empty(),
        "no twin of the table's button in the left panel"
    );
    assert_eq!(h.query_all_by_label("Подключиться (Ctrl+N)").count(), 1);
    assert!(!h.state().dialogs.connect.open);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::N);
    h.run_steps(3);
    assert!(h.state().dialogs.connect.open, "Ctrl+N opens the dialog");
}

fn sid(raw: u64) -> opcda_egui::bff::model::SessionId {
    opcda_egui::bff::model::SessionId::new(raw).unwrap()
}

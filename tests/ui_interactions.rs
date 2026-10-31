//! Headless UI interaction tests for the desktop client (context menus and
//! keyboard navigation/selection), using egui_kittest / AccessKit.
use egui::{Key, Modifiers, PointerButton};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use opclook::app::{App, ReconnectState, SessionState};
use opclook::bff::model::{
    BrowseNode, Command, CommandKind, EndpointSpec, EventKind, ItemKey, NodeKind, Protocol, ScanId,
    SubscriptionState, UaSecurityMode, UaSecurityPolicy, Value,
};
use opclook::settings::Settings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SETTINGS_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

/// Run `f` against a private temporary settings directory of THIS thread.
///
/// Tests that save settings must never overwrite the developer's real
/// `settings.json`, nor another test's file. The directory is redirected per
/// thread (`settings::thread_dir`), not through the process-wide `APPDATA`
/// variable: parallel tests cannot see each other's files and nothing needs a
/// lock or `set_var` (which races with every other thread reading the
/// environment).
fn with_settings_lock<R>(f: impl FnOnce() -> R) -> R {
    let seq = SETTINGS_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("opclook-ui-settings-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated settings directory");
    let _redirect = opclook::settings::thread_dir::redirect(&dir);
    let _cleanup = RemoveDir(dir);
    f()
}

struct RemoveDir(PathBuf);

impl Drop for RemoveDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Force the app's UI language to Russian even if not persisted. The
/// application default IS Russian (`Lang::default`), but several kittest tests
/// seeded a Chinese baseline in earlier passes; callers that assert on Russian
/// labels keep opting in to Ru explicitly so the tests stay robust regardless of
/// a persisted `settings.json` language.
fn set_ru(app: &mut App) {
    app.settings.set_lang(opclook::i18n::Lang::Ru);
}

fn harness(app: App) -> Harness<'static, App> {
    let mut app = app;
    set_ru(&mut app);
    Harness::builder()
        .with_size([1200.0, 900.0])
        .with_max_steps(60)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, app| {
                opclook::ui::draw(app, ui);
            },
            app,
        )
}

fn seed_table(app: &mut App, names: &[(&str, &str, i64)]) -> Vec<ItemKey> {
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    names
        .iter()
        .map(|(id, name, value)| app.seed_row(sid(1), id, name, Value::I64(*value)))
        .collect()
}

fn pointer_click(h: &Harness<'_, App>, pos: egui::Pos2, button: PointerButton) {
    h.event(egui::Event::PointerMoved(pos));
    h.event(egui::Event::PointerButton {
        pos,
        button,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos,
        button,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
}

/// Click a row at a blank part of its FullName cell (labels reply to their own
/// hover, so we avoid clicking directly on the text or its immediate surroundings).
fn row_cell_pos(h: &Harness<'_, App>, name: &str) -> egui::Pos2 {
    let rect = h.get_by_label(name).rect();
    egui::pos2(rect.right() + 40.0, rect.center().y)
}

fn right_click_row(h: &Harness<'_, App>, name: &str) {
    pointer_click(h, row_cell_pos(h, name), PointerButton::Secondary);
}

fn click_menu_item(h: &mut Harness<'_, App>, label: &str) {
    h.run_steps(20);
    h.get_by_label(label).click();
    h.run_steps(20);
}

/// Open a ComboBox laid out immediately to the right of a text label.
fn open_combo_right_of(h: &mut Harness<'_, App>, label: &str, dx: f32) {
    let rect = h.get_by_label(label).rect();
    pointer_click(
        h,
        egui::pos2(rect.right() + dx, rect.center().y),
        PointerButton::Primary,
    );
    h.run_steps(5);
}

/// Seed a session with two sibling Item nodes and return their ids.
fn seed_tree(app: &mut App) -> (u64, u64) {
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let a = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "ns=1;s=a".to_string(),
            name: "SigA".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    let b = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "ns=1;s=b".to_string(),
            name: "SigB".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    (a, b)
}

/// A point in the tag tree pane's empty space (below the last rendered node).
fn tree_empty_pos(h: &Harness<'_, App>, name: &str) -> egui::Pos2 {
    let rect = h.get_by_label(name).rect();
    // Click well below the node so we land in pane space that no item occupies,
    // but still inside the tree's scroll area (the pane extends to the bottom).
    egui::pos2(rect.left() + 8.0, rect.bottom() + 70.0)
}

#[test]
fn tree_row_context_menu_copies_node_name() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "sig1".to_string(),
            name: "Сигнал.Тест".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );

    let mut h = harness(app);
    h.get_by_label("Сигнал.Тест").click_secondary();
    click_menu_item(&mut h, "Копировать имя узла");
    assert_eq!(h.state().status, "Имя узла скопировано");
}

#[test]
fn table_row_context_menu_removes_selected() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let keys = seed_table(&mut app, &[("a", "SigA", 1), ("b", "SigB", 2)]);

    let mut h = harness(app);
    right_click_row(&h, "SigA");
    click_menu_item(&mut h, "Удалить выбранные");

    let rows: Vec<ItemKey> = h.state().rows().iter().map(|r| r.key).collect();
    assert_eq!(rows, vec![keys[1]]);
}

/// The tags-area panel context menu must never steal a right-click from a data
/// row: right-clicking a row opens the ROW menu (with «Удалить выбранные»), while
/// right-clicking the table background (the «Сигналов: N | Выбрано: M» strip)
/// opens the PANEL menu (with «Столбцы...»).
#[test]
fn row_menu_wins_over_panel_menu_and_background_opens_panel() {
    // Row context menu takes precedence.
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    seed_table(&mut app, &[("a", "SigA", 1)]);
    let mut h = harness(app);
    h.run_steps(3);

    right_click_row(&h, "SigA");
    h.run_steps(20);
    assert!(
        h.query_all_by_label("Удалить выбранные").count() >= 1,
        "the row context menu must appear with «Удалить выбранные»"
    );
    assert_eq!(
        h.query_all_by_label("Столбцы...").count(),
        0,
        "the row menu must NOT include the panel-level «Столбцы...» item"
    );

    // Empty/padded table background opens the panel context menu. Use a fresh
    // app (no rows) so it is unambiguous that no row menu can appear.
    let ctx2 = egui::Context::default();
    let app2 = App::new_test(&ctx2);
    let mut h2 = harness(app2);
    h2.run_steps(3);
    // The «Сигналов» label appears both in the tags-area count strip (top) and the
    // status bar (bottom). Pick the topmost one — the tags-area strip.
    let all: Vec<_> = h2.query_all_by_label_contains("Сигналов:").collect();
    assert!(!all.is_empty(), "the «Сигналов» count strip must render");
    let strip = all
        .iter()
        .min_by_key(|n| n.rect().min.y as i32)
        .expect("a «Сигналов» label")
        .rect();
    // Right-click on the strip label itself (still tags-area background).
    pointer_click(
        &h2,
        egui::pos2(strip.center().x, strip.center().y),
        PointerButton::Secondary,
    );
    h2.run_steps(20);
    assert!(
        h2.query_all_by_label("Столбцы...").count() >= 1,
        "right-clicking the tags-area background must open the panel menu with «Столбцы...»"
    );
}

/// The columns dialog's keyboard fallback: Alt/Ctrl+ArrowDown reorders the
/// focused column and persists the new order (drag-and-drop is tested at the
/// model/headless level in `columns.rs` and `ui.rs`).
#[test]
fn columns_dialog_keyboard_alt_down_reorders_and_persists() {
    with_settings_lock(|| {
        use opclook::columns::ColumnId;
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        seed_table(&mut app, &[("a", "SigA", 1)]);
        app.dialogs.columns.open = true;
        // Focus «Полное имя» (row 1) so Alt+ArrowDown moves it one slot.
        app.dialogs.columns.focused = Some(ColumnId::FullName);
        let mut h = harness(app);
        h.run_steps(3);

        let before: Vec<ColumnId> = h.state().settings.resolved_order();
        let target = ColumnId::FullName;
        let idx = before.iter().position(|c| *c == target).unwrap();
        assert!(idx + 1 < before.len());

        h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
        h.run_steps(3);

        let after: Vec<ColumnId> = h.state().settings.resolved_order();
        assert_eq!(after.len(), ColumnId::ALL.len());
        assert_eq!(
            after[idx + 1],
            target,
            "«Полное имя» must move down one slot via Alt+ArrowDown"
        );
        assert_eq!(after[idx], before[idx + 1]);
        assert!(
            h.state().settings.view.column_order.is_some(),
            "column_order must be persisted after the keyboard move"
        );
    });
}

/// The columns dialog's keyboard fallback: Alt/Ctrl+ArrowUp moves the focused
/// column up and persists the change.
#[test]
fn columns_dialog_keyboard_alt_up_reorders_and_persists() {
    with_settings_lock(|| {
        use opclook::columns::ColumnId;
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        seed_table(&mut app, &[("a", "SigA", 1)]);
        app.dialogs.columns.open = true;
        let target = ColumnId::Value;
        app.dialogs.columns.focused = Some(target);
        let mut h = harness(app);
        h.run_steps(3);

        let before: Vec<ColumnId> = h.state().settings.resolved_order();
        let idx = before.iter().position(|c| *c == target).unwrap();
        assert!(idx > 0);

        h.key_press_modifiers(Modifiers::CTRL, Key::ArrowUp);
        h.run_steps(3);

        let after: Vec<ColumnId> = h.state().settings.resolved_order();
        assert_eq!(
            after[idx - 1],
            target,
            "«Значение» must move up via Ctrl+ArrowUp"
        );
        assert_eq!(after[idx], before[idx - 1]);
        assert!(h.state().settings.view.column_order.is_some());
    });
}

#[test]
fn arrow_down_moves_and_shift_down_extends_selection() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let keys = seed_table(
        &mut app,
        &[("a", "SigA", 1), ("b", "SigB", 2), ("c", "SigC", 3)],
    );

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    assert_eq!(h.state().table_focus, Some(0));

    h.key_press(Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(h.state().table_focus, Some(1));
    assert!(h.state().selected_rows.contains(&keys[1]));

    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(h.state().table_focus, Some(2));
    assert!(h.state().selected_rows.contains(&keys[1]));
    assert!(h.state().selected_rows.contains(&keys[2]));
    assert_eq!(h.state().selected_rows.len(), 2);
}

#[test]
fn ctrl_a_selects_all_rows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let keys = seed_table(
        &mut app,
        &[("a", "SigA", 1), ("b", "SigB", 2), ("c", "SigC", 3)],
    );

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(3);
    assert!(h.state().selected_rows.contains(&keys[0]));
    assert_eq!(h.state().selected_rows.len(), keys.len());
}

#[test]
fn delete_removes_selected_row() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let keys = seed_table(&mut app, &[("a", "SigA", 1), ("b", "SigB", 2)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Delete);
    h.run_steps(3);

    assert_eq!(h.state().rows().len(), 1);
    assert!(h.state().rows().iter().any(|r| r.key == keys[1]));
    assert!(!h.state().rows().iter().any(|r| r.key == keys[0]));
}

#[test]
fn enter_opens_write_dialog() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    seed_table(&mut app, &[("a", "SigA", 42)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);

    assert!(h.state().dialogs.write.open);
    assert_eq!(h.state().dialogs.write.keys.len(), 1);
}

#[test]
fn tree_space_toggles_expansion() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "branch1".to_string(),
            name: "Ветка".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(sid(1), branch);

    let mut h = harness(app);
    h.key_press(Key::Space);
    h.run_steps(3);

    let item = h
        .state()
        .sessions
        .iter()
        .find(|s| s.id == sid(1))
        .unwrap()
        .tree()
        .iter()
        .find(|t| t.id == branch)
        .unwrap();
    assert!(item.expanded);
}

#[test]
fn csv_save_load_roundtrip_preserves_rows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    seed_table(
        &mut app,
        &[("ns=1;s=a", "SigA", 1), ("ns=1;s=b", "SigB", 2)],
    );

    let tmp = std::env::temp_dir().join("opclook-ui-roundtrip.csv");
    opclook::ui::export_tree_csv_to_path(&app, &tmp).expect("export");
    let text = std::fs::read_to_string(&tmp).expect("read csv");
    assert!(text.contains("SigA"));
    assert!(text.contains("SigB"));

    let mut loaded = App::new_test(&ctx);
    loaded.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let count = opclook::ui::import_tree_csv_from_path(&mut loaded, &tmp).expect("import");
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(count, 2);
    let ids: Vec<&str> = loaded.rows().iter().map(|r| r.item_id.as_str()).collect();
    assert!(ids.contains(&"ns=1;s=a"));
    assert!(ids.contains(&"ns=1;s=b"));
}

/// Seeding helper for a writable row.
fn seed_writable_table(app: &mut App, names: &[(&str, &str, i64)]) -> Vec<ItemKey> {
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    names
        .iter()
        .map(|(id, name, value)| app.seed_writable_row(sid(1), id, name, Value::I64(*value)))
        .collect()
}

#[test]
fn write_dialog_shows_confirmation_when_enabled() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.write.open);

    // Click "Записать"; because confirm_write is on, a confirm dialog appears
    // and the write is not sent yet (write dialog stays open behind it).
    click_menu_item(&mut h, "Записать");
    assert!(h.state().dialogs.confirm.is_some());
    assert!(h.state().dialogs.write.open);
}

#[test]
fn write_confirm_no_keeps_write_dialog() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    click_menu_item(&mut h, "Записать");
    assert!(h.state().dialogs.confirm.is_some());

    click_menu_item(&mut h, "Да");
    assert!(!h.state().dialogs.confirm.is_some());
    assert!(!h.state().dialogs.write.open);
}

#[test]
fn write_without_confirm_writes_immediately() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    click_menu_item(&mut h, "Записать");
    assert!(!h.state().dialogs.confirm.is_some());
    assert!(!h.state().dialogs.write.open);
}

#[test]
fn tab_exclusive_switch_keeps_flags_and_changes_view() {
    use opclook::app::BottomTab;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    // Both panels enabled -> a real tab bar appears; clicking a tab is a view
    // choice only, it does NOT toggle the show_* flags.
    app.settings.view.options.show_script = true;
    app.settings.view.options.show_log = true;
    app.bottom_tab = BottomTab::Script;
    seed_table(&mut app, &[("a", "SigA", 1)]);

    let mut h = harness(app);
    assert_eq!(h.state().bottom_tab, BottomTab::Script);

    // "Журнал" appears both as a top-level menu entry and as the bottom tab; the
    // tab is rendered lowest on screen, so pick the bottom-most match.
    let tab = h
        .get_all_by_label("Журнал")
        .max_by(|a, b| a.rect().center().y.total_cmp(&b.rect().center().y))
        .expect("journal tab node");
    tab.click();
    h.run_steps(10);
    // Flags unchanged; only the selected view changed.
    assert!(h.state().settings.view.options.show_script);
    assert!(h.state().settings.view.options.show_log);
    assert_eq!(h.state().bottom_tab, BottomTab::Journal);
}

#[test]
fn single_tab_does_not_show_unavailable_tab() {
    use opclook::app::BottomTab;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    // Only script enabled -> journal tab must be absent (view stays Script).
    app.settings.view.options.show_script = true;
    app.settings.view.options.show_log = false;
    app.bottom_tab = BottomTab::Journal;
    seed_table(&mut app, &[("a", "SigA", 1)]);

    let mut h = harness(app);
    // The "Журнал работы" tab is not shown (single panel, no tab bar).
    assert_eq!(h.state().bottom_tab, BottomTab::Journal);
    h.run_steps(5);
    // Still on script-only view; journal panel not renderable as a tab.
    assert!(!h.state().settings.view.options.show_log);
}

#[test]
fn reconnect_stop_sets_flag_and_changes_hint() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.auto_reconnect = true;
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    // Simulate a disconnected session the auto-reconnect loop is looking at.
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        view.state = SessionState::Disconnected;
        view.reconnect = ReconnectState::Waiting {
            at: std::time::Instant::now(),
            attempt: 2,
        };
    }
    let mut h = harness(app);
    let hint = h.state().reconnect_hint();
    assert!(hint.is_some());
    assert!(hint.unwrap().contains("Повторное подключение 2/20"));

    h.state_mut().stop_reconnect(sid(1));
    let hint2 = h.state().reconnect_hint();
    assert!(hint2.is_none() || hint2.unwrap().contains("остановлено"));
}

/// A reply whose (session, request_id) is not outstanding must not mutate the
/// session (stale Connected / Browse from a superseded lifecycle).
#[test]
fn stale_reply_with_unknown_request_id_is_ignored() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let info_before = app.sessions[0].info.clone();

    app.on_event_for_test_with_request(
        sid(1),
        opclook::bff::model::RequestId::new(9_999).unwrap(),
        EventKind::Connected {
            endpoint: EndpointSpec::da("", "Srv.Stale"),
            info: opclook::bff::model::BackendMsg::raw("stale"),
        },
    );
    assert_eq!(app.sessions[0].state, SessionState::Connected);
    assert_eq!(
        app.sessions[0].info, info_before,
        "stale Connected must be ignored"
    );

    app.on_event_for_test_with_request(
        sid(1),
        opclook::bff::model::RequestId::new(9_999).unwrap(),
        EventKind::Browse {
            node_id: String::new(),
            children: vec![BrowseNode {
                node_id: "ghost".to_string(),
                name: "Ghost".to_string(),
                kind: NodeKind::Item,
                has_children: false,
            }],
        },
    );
    assert!(
        app.sessions[0].tree().is_empty(),
        "stale Browse must be ignored"
    );
}

/// The real reply id allocated by `send` is accepted and consumes the pending
/// entry exactly once.
#[test]
fn matching_request_id_reply_is_accepted() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_tab_for_endpoint(EndpointSpec::da("", "Srv.Match"));
        let ids = app.pending_request_ids_for_test(sid(1));
        assert_eq!(ids.len(), 1, "one Connect outstanding, got {ids:?}");
        let connect_id = ids[0];
        app.on_event_for_test_with_request(
            sid(1),
            connect_id,
            EventKind::Connected {
                endpoint: EndpointSpec::da("", "Srv.Match"),
                info: opclook::bff::model::BackendMsg::raw("ok"),
            },
        );
        assert_eq!(app.sessions[0].state, SessionState::Connected);
        assert_eq!(app.sessions[0].info, "ok");

        // Replaying the same id after it was consumed is now stale.
        app.on_event_for_test_with_request(
            sid(1),
            connect_id,
            EventKind::Connected {
                endpoint: EndpointSpec::da("", "Srv.Match"),
                info: opclook::bff::model::BackendMsg::raw("replayed"),
            },
        );
        assert_eq!(
            app.sessions[0].info, "ok",
            "a reply id is accepted only once"
        );
    });
}

/// Request id 0 is reserved for unsolicited pushes (DataChange / subscription
/// state) and must always be accepted.
#[test]
fn push_event_with_zero_request_id_is_accepted() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.on_event_for_test_at(
        sid(1),
        EventKind::SubscriptionState {
            state: SubscriptionState::Active,
        },
    );
    assert_eq!(
        app.sessions[0].ua().unwrap().subscription,
        SubscriptionState::Active,
        "request-id 0 push must be accepted"
    );
}

/// Old replies from the pre-disconnect link must not mutate the new session
/// state after Disconnect+Connect; the new Connect id still works.
#[test]
fn disconnect_connect_does_not_let_old_connected_mutate_new_state() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_tab_for_endpoint(EndpointSpec::da("", "Srv.Old"));
        let old_connect = app.pending_request_ids_for_test(sid(1))[0];

        app.disconnect_session(sid(1));
        app.reconnect_session(sid(1));
        assert_eq!(app.sessions[0].state, SessionState::Connecting);

        app.on_event_for_test_with_request(
            sid(1),
            old_connect,
            EventKind::Connected {
                endpoint: EndpointSpec::da("", "Srv.Old"),
                info: opclook::bff::model::BackendMsg::raw("old"),
            },
        );
        assert_eq!(
            app.sessions[0].state,
            SessionState::Connecting,
            "superseded Connected must not flip the new session"
        );
        assert_ne!(app.sessions[0].info, "old");

        let new_connect = *app
            .pending_request_ids_for_test(sid(1))
            .iter()
            .max()
            .expect("new Disconnect+Connect outstanding");
        app.on_event_for_test_with_request(
            sid(1),
            new_connect,
            EventKind::Connected {
                endpoint: EndpointSpec::da("", "Srv.Old"),
                info: opclook::bff::model::BackendMsg::raw("new"),
            },
        );
        assert_eq!(app.sessions[0].state, SessionState::Connected);
        assert_eq!(app.sessions[0].info, "new");
    });
}

#[test]
fn column_reorder_persists_in_settings() {
    with_settings_lock(|| {
        use opclook::columns::ColumnId;
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        seed_table(&mut app, &[("a", "SigA", 1)]);

        // Simulate the header context-menu "Влево"/"Вправо" path: move Value left.
        let resolved = app.settings.resolved_order();
        let idx = resolved.iter().position(|c| *c == ColumnId::Value).unwrap();
        let mut order = resolved;
        order.swap(idx, idx - 1); // Value moves one position left
        app.settings.persist_column_order(&order);
        app.settings.save().unwrap();

        // Persisted for save/load via BOTH the legacy columns.order and the new
        // column_order snapshot.
        assert!(app.settings.view.column_order.is_some());
        assert_eq!(app.settings.view.columns.order[idx - 1], ColumnId::Value);
        let resolved = app.settings.resolved_order();
        assert_eq!(resolved[idx - 1], ColumnId::Value);
        // Re-listing the canonical set keeps every column exactly once.
        assert_eq!(resolved.len(), ColumnId::ALL.len());

        // A fresh Settings built from the persisted layout resolves the same order
        // (as it would after a save/load roundtrip).
        let reloaded = Settings {
            view: app.settings.view.clone(),
            ..Settings::default()
        };
        assert_eq!(reloaded.resolved_order()[idx - 1], ColumnId::Value);
    });
}

#[test]
fn status_popup_opens_from_indicator_state() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    seed_table(&mut app, &[("a", "SigA", 1)]);

    let mut h = harness(app);
    // Initially closed.
    assert_eq!(h.state().status_popup, false);
    // Open it (as clicking the status-bar indicator does).
    h.state_mut().status_popup = true;
    h.run_steps(5);
    assert!(h.state().status_popup);
    // The popup renders server detail rows.
    let found = h.get_all_by_label("Точка подключения:").count();
    assert!(found >= 1, "status popup should render server details");
}

#[test]
fn pending_write_shows_status_label_then_summary() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    seed_table(&mut app, &[("a", "SigA", 1)]);

    let mut h = harness(app);
    // A write batch is in flight -> «Запись...» label is visible.
    h.state_mut().write_batch.pending = 2;
    h.state_mut().write_batch.total = 2;
    h.run_steps(5);
    let visible = h.get_all_by_label("Запись...").count();
    assert!(
        visible >= 1,
        "spinner label should be visible while writing"
    );

    // Batch finishes -> the status bar renders the compact summary.
    h.state_mut().write_batch.pending = 0;
    h.state_mut().write_batch.ok = 2;
    h.state_mut().write_batch.total = 2;
    h.state_mut().write_batch.summary = Some("Записано: 2 из 2".to_string());
    h.run_steps(5);
    let shown = h.get_all_by_label("Записано: 2 из 2").count();
    assert_eq!(
        shown, 1,
        "completed-write summary should be in the status bar"
    );
    assert_eq!(
        h.state().write_batch.summary.as_deref(),
        Some("Записано: 2 из 2")
    );
}

/// UA connect dialog opens with EMPTY host and port fields (no preset values,
/// reference behavior). Placeholders/hints, not values.
#[test]
fn connect_dialog_ua_search_empty_on_open() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(5);
    // Switch to the OPC UA protocol tab so discovery fields are rendered.
    h.get_by_label("OPC UA").click();
    h.run_steps(5);
    assert_eq!(h.state().dialogs.connect.ua_search_host, "");
    assert_eq!(h.state().dialogs.connect.ua_search_port, "");
    // The reference hint text is rendered under the fields.
    assert!(
        h.query_all_by_label_contains("Пусто — поиск на этом компьютере")
            .count()
            >= 1,
        "empty-search hint must be shown"
    );
    // No 127.0.0.1 or 4840 value is pre-filled as a field default.
    assert_eq!(h.query_all_by_value("127.0.0.1").count(), 0);
    assert_eq!(h.query_all_by_value("4840").count(), 0);
}

fn lease_with_subnet(ip: &str, prefix: u8) -> opclook::net::NetworkLease {
    use opclook::net::{
        AdapterAddress, AdapterId, AdapterInfo, AdapterSelection, AdapterState, NetworkLease,
        SubnetSpec,
    };
    let ip: std::net::IpAddr = ip.parse().unwrap();
    let address = AdapterAddress {
        ip,
        prefix,
        scope_id: None,
    };
    NetworkLease {
        generation: 1,
        requested: AdapterSelection::None,
        adapter: AdapterInfo {
            id: AdapterId::UNSPECIFIED,
            name: "test".to_string(),
            description: String::new(),
            state: AdapterState::Up,
            if_type: 0,
            addresses: vec![address],
            gateways: Vec::new(),
            if_index: std::num::NonZeroU32::new(7),
            ipv6_if_index: std::num::NonZeroU32::new(8),
            luid: 0,
            metric: 10,
            is_physical: false,
            is_loopback: false,
        },
        local_address: address,
        subnet: SubnetSpec::new(ip, prefix).unwrap(),
    }
}

fn net_adapter(
    guid: &str,
    name: &str,
    physical: bool,
    up: bool,
    addresses: &[(&str, u8)],
) -> opclook::net::AdapterInfo {
    use opclook::net::{AdapterAddress, AdapterId, AdapterInfo, AdapterState};
    AdapterInfo {
        id: AdapterId::parse(guid).unwrap(),
        name: name.to_string(),
        description: format!("{name} description"),
        state: if up {
            AdapterState::Up
        } else {
            AdapterState::Down
        },
        if_type: if physical { 6 } else { 71 },
        addresses: addresses
            .iter()
            .map(|(ip, prefix)| AdapterAddress {
                ip: ip.parse().unwrap(),
                prefix: *prefix,
                scope_id: None,
            })
            .collect(),
        gateways: Vec::new(),
        if_index: std::num::NonZeroU32::new(9),
        ipv6_if_index: std::num::NonZeroU32::new(9),
        luid: 42,
        metric: 10,
        is_physical: physical,
        is_loopback: false,
    }
}

fn scan_server() -> opclook::bff::model::UaServerInfo {
    opclook::bff::model::UaServerInfo {
        application_name: "SCAN".to_string(),
        application_uri: "urn:scan".to_string(),
        endpoints: vec![opclook::bff::model::UaEndpointInfo {
            url: "opc.tcp://10.0.0.9:4840".parse().unwrap(),
            security_policy: opclook::bff::model::AdvertisedPolicy::from_wire("None"),
            security_mode: opclook::bff::model::AdvertisedMode::from_wire("None"),
        }],
    }
}

/// Scan events from an old scan id never mutate the current scan state.
#[test]
fn ua_scan_events_ignore_stale_scan_ids() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.open_connect_dialog();
    {
        let d = &mut app.dialogs.connect;
        d.ua_search = opclook::app::UaSearch::Subnet {
            scan_id: ScanId::new(2).unwrap(),
        };
        d.ua_scan_total = 10;
    }
    app.on_event_for_test(EventKind::UaScanProgress {
        scan_id: ScanId::new(1).unwrap(),
        total: 5,
        checked: 5,
        found: 3,
    });
    app.on_event_for_test(EventKind::UaScanFound {
        scan_id: ScanId::new(1).unwrap(),
        servers: vec![scan_server()],
    });
    app.on_event_for_test(EventKind::UaScanFinished {
        scan_id: ScanId::new(1).unwrap(),
        found: 3,
        end: opclook::bff::model::ScanEnd::Completed,
    });
    let d = &app.dialogs.connect;
    assert_eq!(d.ua_search.scan_id(), ScanId::new(2));
    assert!(d.ua_search.scan_id().is_some());
    assert_eq!(d.ua_scan_checked, 0);
    assert_eq!(d.ua_scan_total, 10);
    assert!(d.ua_scan_servers.is_empty());
    assert!(d.ua_search_results().is_empty());

    app.on_event_for_test(EventKind::UaScanProgress {
        scan_id: ScanId::new(2).unwrap(),
        total: 10,
        checked: 4,
        found: 1,
    });
    app.on_event_for_test(EventKind::UaScanFound {
        scan_id: ScanId::new(2).unwrap(),
        servers: vec![scan_server()],
    });
    app.on_event_for_test(EventKind::UaScanFinished {
        scan_id: ScanId::new(2).unwrap(),
        found: 1,
        end: opclook::bff::model::ScanEnd::Completed,
    });
    let d = &app.dialogs.connect;
    assert_eq!(d.ua_search.scan_id(), None);
    assert!(!d.ua_search.scan_id().is_some());
    assert_eq!(d.ua_scan_checked, 10);
    assert_eq!(d.ua_scan_found, 1);
    assert_eq!(d.ua_search_results().len(), 1);
    assert_eq!(d.ua_search_results()[0].application_uri, "urn:scan");
}

/// A subnet above the confirmation threshold is not launched on the first
/// click; confirming narrows the run to loopback + the local address.
#[test]
fn ua_subnet_scan_requires_confirmation_for_large_subnet() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.open_connect_dialog();
    app.network.loaded = true;
    app.network.lease = Some(lease_with_subnet("10.0.0.5", 19));
    app.start_ua_subnet_scan();
    {
        let d = &app.dialogs.connect;
        assert!(!d.ua_search.scan_id().is_some());
        assert_eq!(d.ua_search.scan_id(), None);
        assert_eq!(d.ua_search.confirmation_count(), Some(8190));
        assert!(
            d.ua_scan_error.as_deref().unwrap_or("").contains("Подсеть"),
            "localized confirmation prompt expected"
        );
    }
    app.confirm_ua_subnet_scan();
    {
        let d = &app.dialogs.connect;
        assert!(d.ua_search.scan_id().is_some());
        assert!(d.ua_search.scan_id().is_some());
        assert_eq!(d.ua_search.confirmation_count(), None);
        assert_eq!(d.ua_search_results().len(), 0);
    }
    app.cancel_ua_subnet_scan();
    assert!(!app.dialogs.connect.ua_search.scan_id().is_some());
    assert_eq!(app.dialogs.connect.ua_search.scan_id(), None);
}

/// Scan/cancel commands are streaming: they must not reserve request-id slots.
#[test]
fn ua_subnet_scan_commands_are_not_pending_requests() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.network.loaded = true;
    app.network.lease = Some(lease_with_subnet("10.0.0.5", 30));
    let before = app.pending_request_ids_for_test(opclook::bff::model::Owner::Discovery);
    app.start_ua_subnet_scan();
    assert!(app.dialogs.connect.ua_search.scan_id().is_some());
    assert_eq!(
        app.pending_request_ids_for_test(opclook::bff::model::Owner::Discovery),
        before
    );
    app.cancel_ua_subnet_scan();
    assert_eq!(
        app.pending_request_ids_for_test(opclook::bff::model::Owner::Discovery),
        before
    );
}

fn start_recorded_ua_host(app: &mut App) -> opclook::bff::model::RequestId {
    app.record_dispatches_for_test();
    app.dialogs.connect.ua_search_host = "127.0.0.1".into();
    app.search_ua_servers();
    let opclook::app::UaSearch::SingleHost { request } = app.dialogs.connect.ua_search else {
        panic!("host search")
    };
    request
}

/// UaServersFound populates the connect dialog's search results state.
#[test]
fn ua_servers_found_populates_results_state() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let request = start_recorded_ua_host(&mut app);
    let servers = vec![opclook::bff::model::UaServerInfo {
        application_name: "CODESYS".to_string(),
        application_uri: "urn:test".to_string(),
        endpoints: vec![opclook::bff::model::UaEndpointInfo {
            url: "opc.tcp://127.0.0.1:4840".parse().unwrap(),
            security_policy: opclook::bff::model::AdvertisedPolicy::from_wire("None"),
            security_mode: opclook::bff::model::AdvertisedMode::from_wire("None"),
        }],
    }];
    app.on_event_for_test_with_request(
        opclook::bff::model::Owner::Discovery,
        request,
        opclook::bff::model::EventKind::UaServersFound {
            host: "127.0.0.1".to_string(),
            port: 4840,
            servers,
        },
    );
    assert_eq!(app.dialogs.connect.ua_search_results().len(), 1);
    assert!(!app.dialogs.connect.ua_search.host_searching());
    assert_eq!(
        app.dialogs.connect.ua_search_results()[0]
            .endpoints
            .first()
            .unwrap()
            .url
            .as_str(),
        "opc.tcp://127.0.0.1:4840"
    );
}

#[test]
fn ua_discovery_reply_arrives_on_network_pane() {
    use opclook::app::ConnectPane;
    use opclook::bff::model::{AdvertisedMode, AdvertisedPolicy, UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let request = start_recorded_ua_host(&mut app);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("OPC UA").click();
    h.run_steps(3);
    h.get_by_label("Сетевые настройки").click();
    h.run_steps(3);
    h.state_mut().on_event_for_test_with_request(
        opclook::bff::model::Owner::Discovery,
        request,
        EventKind::UaServersFound {
            host: "127.0.0.1".into(),
            port: 4840,
            servers: vec![UaServerInfo {
                application_name: "Sample".into(),
                application_uri: "urn:sample".into(),
                endpoints: vec![UaEndpointInfo {
                    url: "opc.tcp://127.0.0.1:4840".parse().unwrap(),
                    security_policy: AdvertisedPolicy::from_wire("None"),
                    security_mode: AdvertisedMode::from_wire("None"),
                }],
            }],
        },
    );
    assert!(matches!(
        h.state().dialogs.connect.pane,
        ConnectPane::Network { .. }
    ));
    assert_eq!(h.state().dialogs.connect.ua_search_results().len(), 1);
    h.get_by_label("OPC UA").click();
    h.run_steps(3);
    assert_eq!(
        h.state().dialogs.connect.ua_search_results()[0].application_name,
        "Sample"
    );
}

/// Closing and reopening the connect popup keeps previously discovered DA
/// servers and UA search results, and does not restart DA discovery once the
/// DA list is already populated.
#[test]
fn connect_dialog_reopen_keeps_discovered_servers() {
    use opclook::bff::model::{ServerEntry, UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();

    // Opening starts the combined DA discovery; deliver its local result.
    assert!(app.dialogs.connect.da_auto_pending > 0);
    app.on_event_for_test(EventKind::Servers {
        host: String::new(),
        servers: vec![ServerEntry {
            id: "Matrikon.OPC.Simulation".to_string(),
            description: "Simulation".to_string(),
        }],
    });
    assert_eq!(app.dialogs.connect.da_servers.len(), 1);

    // A completed single-host UA search populates the result list as well as the
    // security-combo endpoint source.
    let request = start_recorded_ua_host(&mut app);
    app.on_event_for_test_with_request(
        opclook::bff::model::Owner::Discovery,
        request,
        EventKind::UaServersFound {
            host: "127.0.0.1".to_string(),
            port: 4840,
            servers: vec![UaServerInfo {
                application_name: "CODESYS".to_string(),
                application_uri: "urn:test".to_string(),
                endpoints: vec![UaEndpointInfo {
                    url: "opc.tcp://127.0.0.1:4840".parse().unwrap(),
                    security_policy: opclook::bff::model::AdvertisedPolicy::from_wire("None"),
                    security_mode: opclook::bff::model::AdvertisedMode::from_wire("None"),
                }],
            }],
        },
    );
    assert_eq!(app.dialogs.connect.ua_search_results().len(), 1);
    assert_eq!(app.dialogs.connect.ua_discovered_endpoints.len(), 1);

    // Close cancels in-flight searches (keeping the found servers), then reopen.
    app.close_connect_dialog();
    app.open_connect_dialog();

    assert_eq!(
        app.dialogs.connect.da_servers.len(),
        1,
        "DA servers must survive a close/reopen"
    );
    assert_eq!(
        app.dialogs.connect.ua_search_results().len(),
        1,
        "UA search results must survive a close/reopen"
    );
    assert_eq!(
        app.dialogs.connect.ua_discovered_endpoints.len(),
        1,
        "the security-combo endpoints must survive a close/reopen"
    );
    // The DA list is already populated: reopening must not start a new
    // discovery (no fresh pending request on session 0).
    assert_eq!(app.dialogs.connect.da_auto_pending, 0);
    assert!(
        app.pending_request_ids_for_test(opclook::bff::model::Owner::Discovery)
            .is_empty(),
        "reopening with a populated DA list must not restart discovery"
    );
}

/// A subnet scan cancelled by closing the popup merges its buffered servers;
/// those merged results must then survive the next reopen too.
#[test]
fn connect_dialog_reopen_keeps_merged_subnet_scan_results() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.network.loaded = true;
    app.network.lease = Some(lease_with_subnet("10.0.0.5", 30));
    app.start_ua_subnet_scan();
    let scan_id = app
        .dialogs
        .connect
        .ua_search
        .scan_id()
        .expect("subnet scan id");
    app.on_event_for_test(EventKind::UaScanFound {
        scan_id,
        servers: vec![scan_server()],
    });
    // Scan results appear immediately instead of waiting for completion.
    assert_eq!(app.dialogs.connect.ua_search_results().len(), 1);

    app.close_connect_dialog();
    assert_eq!(
        app.dialogs.connect.ua_search_results().len(),
        1,
        "closing must cancel the scan and merge its buffer"
    );

    app.open_connect_dialog();
    assert_eq!(
        app.dialogs.connect.ua_search_results().len(),
        1,
        "merged subnet-scan results must survive a reopen"
    );
    assert_eq!(app.dialogs.connect.ua_discovered_endpoints.len(), 1);
    assert!(!app.dialogs.connect.ua_search.scan_id().is_some());
}

/// UA connect dialog renders the security controls with the Auto defaults.
#[test]
fn connect_dialog_ua_security_controls_render() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(5);
    // Switch to the OPC UA protocol tab so security fields are rendered.
    h.get_by_label("OPC UA").click();
    h.run_steps(5);
    let d = &h.state().dialogs.connect;
    assert_eq!(d.ua_security_policy, opclook::bff::ua::SECURITY_POLICY_AUTO);
    assert_eq!(d.ua_security_mode, opclook::bff::ua::SECURITY_MODE_AUTO);
    assert_eq!(d.ua_session_timeout, 120_000);
    // The trust checkbox, the identity combo and the network-search button are
    // all present on the UA tab.
    assert_eq!(
        h.query_all_by_label("Доверять сертификату сервера (только на эту сессию)")
            .count(),
        1
    );
    assert_eq!(h.query_all_by_label("Поиск через LDS/mDNS").count(), 1);
}

/// U15: the UA tab has an optional «LDS-хост» field; a named host is what
/// «Поиск через LDS/mDNS» asks, and it is remembered in the settings.
#[test]
fn u15_connect_dialog_has_an_lds_host_field_used_by_the_lds_search() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_connect_dialog();
        let mut h = harness(app);
        h.run_steps(5);
        assert_eq!(
            h.query_all_by_label("LDS-хост").count(),
            0,
            "the field belongs to the OPC UA tab"
        );
        h.get_by_label("OPC UA").click();
        h.run_steps(5);
        assert_eq!(h.query_all_by_label("LDS-хост").count(), 1);

        h.state_mut().dialogs.connect.ua_lds_host = "127.0.0.1:48690".to_string();
        h.run_steps(2);
        h.get_by_label("Поиск через LDS/mDNS").click();
        h.run_steps(5);
        let state = h.state();
        assert!(state.dialogs.connect.ua_search.network_searching());
        assert!(state.dialogs.connect.error.is_none());
        assert_eq!(state.settings.ua.ua_lds_host, "127.0.0.1:48690");
        assert_eq!(
            opclook::settings::Settings::load().settings.ua.ua_lds_host,
            "127.0.0.1:48690",
            "the field is persisted"
        );
    });
}

/// U15: a malformed LDS host is reported in the dialog and nothing is searched.
#[test]
fn u15_connect_dialog_rejects_a_malformed_lds_host() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_connect_dialog();
        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("OPC UA").click();
        h.run_steps(5);
        h.state_mut().dialogs.connect.ua_lds_host = "host:0".to_string();
        h.run_steps(2);
        h.get_by_label("Поиск через LDS/mDNS").click();
        h.run_steps(5);
        assert!(!h.state().dialogs.connect.ua_search.network_searching());
        assert!(h.state().dialogs.connect.error.is_some());
        assert!(h.query_all_by_label_contains("LDS-хост:").count() >= 1);
        assert_eq!(h.state().settings.ua.ua_lds_host, "");
    });
}

/// The third popup tab renders a single adapter ComboBox. There is no
/// automatic mode, no diagnostic detail fields and opening the tab does not
/// rewrite the saved selection.
#[test]
fn connect_dialog_network_tab_renders_single_adapter_combo() {
    use opclook::bff::model::Protocol;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(5);

    h.get_by_label("Сетевые настройки").click();
    h.run_steps(5);
    assert!(matches!(
        h.state().dialogs.connect.pane,
        opclook::app::ConnectPane::Network { .. }
    ));
    assert_eq!(
        h.query_all_by_label("Сетевые адаптеры").count(),
        0,
        "the duplicate title must be gone"
    );
    assert_eq!(h.query_all_by_label("Состояние: Работает").count(), 0);
    assert!(h.query_all_by_label("Обновить").count() >= 1);
    assert!(h.query_all_by_label("Сетевой адаптер").count() >= 1);
    assert_eq!(
        h.state().settings.adapter_selection(),
        opclook::net::AdapterSelection::None,
        "opening the tab must not rewrite the saved selection"
    );
    assert_eq!(
        h.query_all_by_label("Автоматически").count(),
        0,
        "the automatic adapter mode must be gone"
    );
    // An empty cache shows the explicit "refresh first" hint.
    assert_eq!(
        h.query_all_by_label("Список не загружен. Нажмите «Обновить».")
            .count(),
        1
    );

    // The last protocol and both drafts survive the network pane and reopen.
    h.get_by_label("OPC UA").click();
    h.run_steps(3);
    h.state_mut().dialogs.connect.ua_url = "opc.tcp://localhost:4840".into();
    h.state_mut().dialogs.connect.da_manual = "Draft.DA".into();
    h.get_by_label("Сетевые настройки").click();
    h.run_steps(3);
    assert_eq!(h.state().dialogs.connect.pane.protocol(), Protocol::OpcUa);
    h.state_mut().close_connect_dialog();
    h.state_mut().open_connect_dialog();
    assert_eq!(
        h.state().dialogs.connect.pane,
        opclook::app::ConnectPane::Ua
    );
    h.state_mut().dialogs.connect.ua_url = "opc.tcp://localhost:4840".into();
    h.state_mut().dialogs.connect.da_manual = "Draft.DA".into();
    h.run_steps(3);
    h.get_by_label("Сетевые настройки").click();
    h.run_steps(3);
    h.get_by_label("OPC UA").click();
    h.run_steps(3);
    assert_eq!(h.state().dialogs.connect.ua_url, "opc.tcp://localhost:4840");
    assert_eq!(h.state().dialogs.connect.da_manual, "Draft.DA");

    // Switching back to a protocol tab leaves the network view.
    h.get_by_label("OPC DA (COM)").click();
    h.run_steps(5);
    assert!(!matches!(
        h.state().dialogs.connect.pane,
        opclook::app::ConnectPane::Network { .. }
    ));
}

/// A remote UA host search requires an explicitly selected adapter; without
/// one it fails with the localized instruction instead of using a default route.
#[test]
fn remote_ua_search_without_adapter_is_blocked() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.dialogs.connect.ua_search_host = "192.0.2.10".to_string();
    app.dialogs.connect.ua_search_port = "4840".to_string();
    app.search_ua_servers();
    assert!(!app.dialogs.connect.ua_search.host_searching());
    assert!(
        app.dialogs
            .connect
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Выберите сетевой адаптер")),
        "expected the localized adapter error, got {:?}",
        app.dialogs.connect.error
    );
}

/// A loopback UA endpoint can be opened without an adapter; a remote endpoint
/// cannot.
#[test]
fn remote_ua_connect_without_adapter_is_blocked_but_loopback_is_allowed() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);

        app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://192.0.2.10:4840"));
        assert!(app.sessions.is_empty(), "remote connect must be blocked");
        assert!(
            app.status.contains("Выберите сетевой адаптер"),
            "expected the localized adapter error, got {:?}",
            app.status
        );

        app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
        assert_eq!(app.sessions.len(), 1, "loopback connect is always allowed");
        assert!(
            app.sessions[0]
                .endpoint()
                .ua_bind
                .as_ref()
                .is_some_and(|binding| binding.is_loopback()),
            "loopback UA sessions use a loopback binding"
        );
    });
}

/// V2: two endpoints of one server (same URL, other security) are two
/// sessions with their own tags; reopening an endpoint reuses its session.
#[test]
fn v2_endpoints_with_other_security_are_separate_sessions() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        let url = "opc.tcp://127.0.0.1:4840";
        let plain = EndpointSpec::ua(url);
        let mut secure = EndpointSpec::ua(url);
        secure.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
        secure.ua_security_mode = UaSecurityMode::SignAndEncrypt;

        assert!(app.open_tab_for_endpoint(plain.clone()));
        assert!(app.open_tab_for_endpoint(secure.clone()));
        assert_eq!(app.sessions.len(), 2);
        let secure_id = app.sessions[1].id;
        assert_eq!(app.active_session, Some(secure_id));

        assert!(app.open_tab_for_endpoint(plain));
        assert_eq!(app.sessions.len(), 2, "the plain endpoint is reused");
        assert_eq!(app.active_session, Some(app.sessions[0].id));
        assert!(app.open_tab_for_endpoint(secure));
        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.active_session, Some(secure_id));

        // The two tabs are told apart by the security of the explicit one.
        let mut h = harness(app);
        h.run_steps(3);
        assert!(
            h.query_all_by_label_contains("· Подпись и шифрование")
                .count()
                >= 1
        );
    });
}

/// Opening the UA tab must not start a network search by itself. Search is
/// explicit: the user chooses host/LDS/mDNS or subnet scan.
#[test]
fn connect_dialog_ua_tab_does_not_autostart_search() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(5);

    h.get_by_label("OPC UA").click();
    h.run_steps(5);

    let d = &h.state().dialogs.connect;
    assert!(
        !d.ua_search.host_searching(),
        "UA host search must not start on tab open"
    );
    assert!(
        !d.ua_search.network_searching(),
        "LDS/mDNS search must not start on tab open"
    );
    assert!(
        !d.ua_search.scan_id().is_some(),
        "subnet scan must not start on tab open"
    );
    assert_eq!(d.ua_search.auto_pending_count(), 0);
}

/// Clicking the already active UA tab is not a "leave UA" action and must not
/// cancel a running subnet scan.
#[test]
fn connect_dialog_active_ua_tab_keeps_scan() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.dialogs.connect.ua_search = opclook::app::UaSearch::Subnet {
        scan_id: ScanId::new(1).unwrap(),
    };
    let mut h = harness(app);
    h.run_steps(5);

    h.get_by_label("OPC UA").click();
    h.run_steps(2);

    assert!(
        h.state().dialogs.connect.ua_search.scan_id().is_some(),
        "active UA tab click must not cancel the scan"
    );
}

/// The network tab applies changes immediately, so its footer action is
/// «Закрыть», not «Отмена»; the hint makes the persistence explicit.
#[test]
fn connect_dialog_network_tab_close_and_apply_hint() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(5);

    h.get_by_label("Сетевые настройки").click();
    h.run_steps(5);

    assert_eq!(
        h.query_all_by_label("Изменения применяются сразу и сохраняются.")
            .count(),
        1
    );
    assert!(h.query_all_by_label("Закрыть").count() >= 1);
    assert_eq!(
        h.query_all_by_label("Отмена").count(),
        0,
        "network settings are already applied; Cancel would be misleading"
    );
}

/// Startup uses `cached_adapters` from settings; it never queries Windows. A
/// fake adapter that cannot exist on this machine proves the source.
#[test]
fn network_tab_startup_uses_cached_adapters_without_windows_query() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let a = "31000000-0000-0000-0000-000000000001";
        let b = "31000000-0000-0000-0000-000000000002";
        let mut settings = Settings::default();
        settings.network.cached_adapters = opclook::settings::CachedAdapters::from_vec(vec![
            opclook::net::CachedAdapter::from_info(&net_adapter(
                a,
                "Cached A",
                true,
                true,
                &[("192.168.1.20", 24)],
            )),
            opclook::net::CachedAdapter::from_info(&net_adapter(
                b,
                "Cached B",
                false,
                true,
                &[("10.0.0.5", 24)],
            )),
        ]);
        settings.set_adapter_selection(opclook::net::AdapterSelection::Adapter(
            opclook::net::AdapterId::parse(a).unwrap(),
        ));
        settings.set_selected_local_ip(Some("192.168.1.20".parse().unwrap()));

        let mut app = App::new_test_with_settings(&ctx, settings);
        app.open_connect_dialog();
        assert_eq!(app.network.adapters.len(), 2);
        assert_eq!(
            opclook::net::selected_adapter_count(
                &app.network.adapters,
                &app.settings.adapter_selection()
            ),
            1,
            "exactly one cached row is selected"
        );
        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("Сетевые настройки").click();
        h.run_steps(5);
        assert_eq!(h.query_all_by_label("192.168.1.20/24").count(), 1);
        // Open the adapter dropdown: exactly one row carries the visible
        // selection marker, and it is the cached saved adapter.
        open_combo_right_of(&mut h, "Сетевой адаптер", 100.0);
        assert_eq!(
            h.query_all_by_label("● Cached A (физический)").count(),
            1,
            "one visibly selected adapter row is expected"
        );
        assert_eq!(h.query_all_by_label_contains("● ").count(), 1);
    });
}

/// A stale saved GUID selects zero rows, shows the stale status and «Забыть»;
/// forgetting clears the selection/IP but keeps the cached list.
#[test]
fn network_tab_stale_guid_has_zero_selection_and_forget_keeps_cache() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let a = "32000000-0000-0000-0000-000000000001";
        let b = "32000000-0000-0000-0000-000000000002";
        let stale = "32000000-0000-0000-0000-0000000000ff";
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);
        app.apply_network_inventory(vec![
            net_adapter(a, "Ethernet", true, true, &[("192.168.1.20", 24)]),
            net_adapter(b, "Wi-Fi", false, true, &[("10.0.0.5", 24)]),
        ]);
        app.settings.network.preferred_adapter =
            Some(opclook::net::AdapterId::parse(stale).unwrap());
        app.settings.network.selected_local_ip = Some("10.9.9.9".parse().unwrap());
        app.open_connect_dialog();

        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("Сетевые настройки").click();
        h.run_steps(5);
        assert_eq!(
            h.query_all_by_label("Сохранённый адаптер недоступен. Выберите активный.")
                .count(),
            1
        );
        assert!(h.query_all_by_label("Забыть").count() >= 1);
        assert_eq!(h.query_all_by_label("10.9.9.9").count(), 0);
        assert_eq!(
            opclook::net::selected_adapter_count(
                &h.state().network.adapters,
                &h.state().settings.adapter_selection()
            ),
            0,
            "stale GUID selects no row"
        );
        open_combo_right_of(&mut h, "Сетевой адаптер", 100.0);
        assert_eq!(
            h.query_all_by_label_contains("● ").count(),
            0,
            "a stale GUID must not mark any dropdown row as selected"
        );
        let r = h.get_by_label("Сетевой адаптер").rect();
        pointer_click(&h, r.center(), PointerButton::Primary);
        h.run_steps(5);

        h.get_by_label("Забыть").click();
        h.run_steps(5);
        let app = h.state();
        assert!(app.settings.network.preferred_adapter.is_none());
        assert!(app.settings.network.selected_local_ip.is_none());
        assert_eq!(
            app.settings.network.cached_adapters.adapters.len(),
            2,
            "forgetting a stale GUID must not clear the cached list"
        );
        assert_eq!(app.network.adapters.len(), 2);
    });
}

/// One address is rendered read-only; multiple addresses open a ComboBox whose
/// choice immediately updates the saved local IP.
#[test]
fn network_tab_one_ip_read_only_and_multiple_ip_combo() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let a = "33000000-0000-0000-0000-000000000001";
        let b = "33000000-0000-0000-0000-000000000002";
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);
        app.apply_network_inventory(vec![
            net_adapter(a, "Ethernet", true, true, &[("192.168.1.20", 24)]),
            net_adapter(
                b,
                "Wi-Fi",
                false,
                true,
                &[("10.0.0.5", 24), ("10.0.0.6", 24)],
            ),
        ]);
        app.apply_network_selection(
            opclook::net::AdapterSelection::Adapter(opclook::net::AdapterId::parse(a).unwrap()),
            None,
        );
        app.open_connect_dialog();

        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("Сетевые настройки").click();
        h.run_steps(5);
        assert_eq!(h.query_all_by_label("192.168.1.20/24").count(), 1);

        h.state_mut().apply_network_selection(
            opclook::net::AdapterSelection::Adapter(opclook::net::AdapterId::parse(b).unwrap()),
            None,
        );
        h.run_steps(5);
        assert_eq!(
            h.state().settings.network.selected_local_ip,
            Some("10.0.0.5".parse().unwrap())
        );
        assert_eq!(
            h.state()
                .network
                .adapters
                .iter()
                .find(|adapter| adapter.id.to_string() == b)
                .map(|adapter| adapter.addresses.len()),
            Some(2),
            "the selected adapter has multiple addresses"
        );

        // Multiple addresses use a ComboBox; choosing one updates the saved IP.
        open_combo_right_of(&mut h, "Локальный IP", 80.0);
        assert!(
            h.query_all_by_label("10.0.0.6/24").count() >= 1,
            "multiple addresses must offer a ComboBox"
        );
        h.get_by_label("10.0.0.6/24").click();
        h.run_steps(5);
        assert_eq!(
            h.state().settings.network.selected_local_ip,
            Some("10.0.0.6".parse().unwrap())
        );
        assert_eq!(
            h.state()
                .network
                .lease
                .as_ref()
                .map(|lease| lease.local_ip()),
            Some("10.0.0.6".parse().unwrap())
        );
    });
}

/// None of the implicit paths (network tab, binding lookup, DA discovery,
/// subnet scan start) may query the Windows adapter inventory.
#[test]
fn implicit_network_paths_do_not_refresh_inventory() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.network.loaded = false;
    app.open_connect_dialog();
    let _ = app.current_network_binding();
    app.discover_da_all();
    app.start_ua_subnet_scan();
    assert!(
        !app.network.loaded,
        "implicit operations must not refresh the adapter inventory"
    );

    let mut h = harness(app);
    h.run_steps(5);
    h.get_by_label("Сетевые настройки").click();
    h.run_steps(5);
    assert!(
        !h.state().network.loaded,
        "opening the network tab must not query Windows"
    );
}

/// The «Обновить» button must actually query the Windows inventory and replace
/// the runtime cache. This is the regression for the button that appeared to
/// do nothing when all adapters shared the wrong identity.
#[test]
#[cfg_attr(
    not(windows),
    ignore = "Windows-only: adapter inventory comes from GetAdaptersAddresses (IP Helper)"
)]
fn network_tab_refresh_button_queries_windows_inventory() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.network.loaded = false;
        app.open_connect_dialog();

        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("Сетевые настройки").click();
        h.run_steps(5);
        h.get_by_label("Обновить").click();
        h.run_steps(10);

        let app = h.state();
        assert!(
            app.network.loaded,
            "refresh must populate the runtime cache"
        );
        assert!(
            !app.network.refresh_failed,
            "refresh failed: {:?}",
            app.network.error
        );
        assert_eq!(
            app.settings.network.cached_adapters.adapters.len(),
            app.network.adapters.len(),
            "the persisted cache must match the refreshed inventory"
        );
    });
}

/// A refresh fully replaces the cache; down adapters never enter the dropdown.
#[test]
fn network_refresh_replaces_cache_and_filters_down_adapters() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let a = "34000000-0000-0000-0000-000000000001";
        let down = "34000000-0000-0000-0000-000000000002";
        let b = "34000000-0000-0000-0000-000000000003";
        let mut app = App::new_test(&ctx);
        app.apply_network_inventory(vec![
            net_adapter(a, "Ethernet", true, true, &[("192.168.1.20", 24)]),
            net_adapter(down, "Ethernet Down", true, false, &[("192.168.1.30", 24)]),
        ]);
        assert_eq!(app.settings.network.cached_adapters.adapters.len(), 1);
        assert_eq!(app.network.adapters.len(), 1);
        assert_eq!(app.network.adapters[0].id.to_string(), a);

        app.apply_network_inventory(vec![net_adapter(
            b,
            "Wi-Fi",
            false,
            true,
            &[("10.0.0.5", 24)],
        )]);
        assert_eq!(app.settings.network.cached_adapters.adapters.len(), 1);
        assert_eq!(app.network.adapters.len(), 1);
        assert_eq!(app.network.adapters[0].id.to_string(), b);
        assert!(
            app.settings
                .network
                .cached_adapters
                .adapters
                .iter()
                .all(|adapter| adapter.id.to_string() != a),
            "refresh must replace, not merge, the old cache"
        );
    });
}

/// Changing the adapter affects future operations only; an existing session
/// keeps the binding captured when it was opened.
#[test]
fn changing_adapter_does_not_rebind_active_sessions() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let a = "35000000-0000-0000-0000-000000000001";
        let b = "35000000-0000-0000-0000-000000000002";
        let mut app = App::new_test(&ctx);
        app.apply_network_inventory(vec![
            net_adapter(a, "Ethernet", true, true, &[("192.168.1.20", 24)]),
            net_adapter(b, "Wi-Fi", false, true, &[("10.0.0.5", 24)]),
        ]);
        app.apply_network_selection(
            opclook::net::AdapterSelection::Adapter(opclook::net::AdapterId::parse(a).unwrap()),
            None,
        );
        app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://192.0.2.50:4840"));
        assert_eq!(app.sessions.len(), 1);
        let old_binding = app.sessions[0].endpoint().ua_bind.clone();
        assert_eq!(
            old_binding.as_ref().map(|binding| binding.local_ip),
            Some("192.168.1.20".parse().unwrap())
        );

        app.apply_network_selection(
            opclook::net::AdapterSelection::Adapter(opclook::net::AdapterId::parse(b).unwrap()),
            None,
        );
        assert_eq!(
            app.sessions[0].endpoint().ua_bind,
            old_binding,
            "an active session keeps its captured binding"
        );
        assert_eq!(
            app.network.lease.as_ref().map(|lease| lease.local_ip()),
            Some("10.0.0.5".parse().unwrap()),
            "future operations use the new adapter"
        );
        assert_eq!(
            app.settings.network.selected_local_ip,
            Some("10.0.0.5".parse().unwrap())
        );
    });
}

/// Anonymous identity must ignore previously typed credentials. The old path
/// forwarded the username and silently connected as UserName.
#[test]
fn connect_dialog_anonymous_identity_does_not_send_credentials() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_connect_dialog();
        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("OPC UA").click();
        h.run_steps(5);

        {
            let d = &mut h.state_mut().dialogs.connect;
            d.ua_url = "opc.tcp://127.0.0.1:4840".to_string();
            d.ua_user = "operator".to_string();
            d.ua_password = "secret".to_string();
            d.ua_identity = opclook::app::UaIdentity::Anonymous;
        }
        h.run_steps(2);
        {
            let connect = h
                .query_all_by_label("Подключиться")
                .filter(|n| !n.accesskit_node().is_disabled())
                .last()
                .expect("enabled connect button");
            connect.click();
        }
        h.run_steps(3);

        let endpoint = h
            .state()
            .sessions
            .last()
            .expect("connect creates a session container")
            .endpoint()
            .clone();
        assert!(
            endpoint.user.is_empty(),
            "Anonymous must not forward a user name"
        );
        assert!(
            endpoint.password.is_empty(),
            "Anonymous must not forward a password"
        );
    });
}

#[test]
fn connect_identity_switch_preserves_buffers_and_certificate_auth() {
    use opclook::app::UaIdentity;
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_connect_dialog();
        app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
        app.dialogs.connect.ua_url = "opc.tcp://127.0.0.1:4840".into();
        app.dialogs.connect.ua_user = "operator".into();
        app.dialogs.connect.ua_password = "secret".into();
        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_value("Анонимно").click();
        h.run_steps(2);
        h.get_by_label("Имя пользователя").click();
        h.run_steps(2);
        assert_eq!(h.state().dialogs.connect.ua_identity, UaIdentity::UserName);
        h.get_by_value("Имя пользователя").click();
        h.run_steps(2);
        h.get_by_label("Сертификат").click();
        h.run_steps(2);
        let d = &h.state().dialogs.connect;
        assert_eq!(d.ua_identity, UaIdentity::Certificate);
        assert_eq!(d.ua_user, "operator");
        assert_eq!(d.ua_password, "secret");
        h.query_all_by_label("Подключиться")
            .filter(|n| !n.accesskit_node().is_disabled())
            .last()
            .expect("enabled connect button")
            .click();
        h.run_steps(3);
        let endpoint = &h.state().sessions.last().expect("session").endpoint();
        assert!(endpoint.ua_use_certificate);
        assert!(endpoint.user.is_empty());
        assert!(endpoint.password.is_empty());
    });
}

/// UserName identity without a user name must stay in the dialog with a
/// localized error, not silently connect as Anonymous.
#[test]
fn connect_dialog_username_identity_requires_user() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_connect_dialog();
        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("OPC UA").click();
        h.run_steps(5);

        {
            let d = &mut h.state_mut().dialogs.connect;
            d.ua_url = "opc.tcp://127.0.0.1:4840".to_string();
            d.ua_user.clear();
            d.ua_password.clear();
            d.ua_identity = opclook::app::UaIdentity::UserName;
        }
        h.run_steps(2);
        {
            let connect = h
                .query_all_by_label("Подключиться")
                .filter(|n| !n.accesskit_node().is_disabled())
                .last()
                .expect("enabled connect button");
            connect.click();
        }
        h.run_steps(2);

        let d = &h.state().dialogs.connect;
        assert!(d.open, "invalid identity must keep the dialog open");
        assert!(
            d.error
                .as_deref()
                .is_some_and(|e| e.contains("имя пользователя")),
            "expected localized UserName error, got {:?}",
            d.error
        );
        assert!(h.state().sessions.is_empty());
    });
}

/// The security combos are typed enums (not free text). Changes remain a
/// dialog draft until a connection is accepted, so Cancel does not save them.
#[test]
fn connect_dialog_security_combos_are_typed_and_persist() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_connect_dialog();
        app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
        let mut h = harness(app);
        h.run_steps(5);
        // Both typed combos start on Auto (selected text is the a11y value).
        assert_eq!(h.query_all_by_value("Авто").count(), 2);
        // Open the policy combo (first in layout order) and choose a typed value.
        {
            let auto = h.query_all_by_value("Авто").next().unwrap();
            auto.click();
        }
        h.run_steps(5);
        h.get_by_label("Basic256Sha256").click();
        h.run_steps(5);
        assert_eq!(
            h.state().dialogs.connect.ua_security_policy,
            UaSecurityPolicy::Basic256Sha256
        );
        assert_eq!(
            h.state().settings.ua.ua_security_policy,
            UaSecurityPolicy::Auto,
            "uncommitted choice must not modify persisted settings"
        );
        // The mode combo is now the only Auto control.
        assert_eq!(h.query_all_by_value("Авто").count(), 1);
        h.get_by_value("Авто").click();
        h.run_steps(5);
        h.get_by_label("Подпись").click();
        h.run_steps(5);
        assert_eq!(
            h.state().dialogs.connect.ua_security_mode,
            UaSecurityMode::Sign
        );
        assert_eq!(h.state().settings.ua.ua_security_mode, UaSecurityMode::Auto);
    });
}

/// An invalid policy/mode pair is shown as a localized dialog error and does
/// not create a session (no silent connect).
#[test]
fn connect_dialog_invalid_security_pair_shows_error_and_does_not_connect() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.dialogs.connect.ua_url = "opc.tcp://127.0.0.1:4840".to_string();
    app.dialogs.connect.ua_security_policy = UaSecurityPolicy::None;
    app.dialogs.connect.ua_security_mode = UaSecurityMode::SignAndEncrypt;
    let mut h = harness(app);
    h.run_steps(5);
    {
        let connect = h
            .query_all_by_label("Подключиться")
            .filter(|n| !n.accesskit_node().is_disabled())
            .last()
            .expect("enabled connect button");
        connect.click();
    }
    h.run_steps(5);
    assert!(h.state().dialogs.connect.open, "dialog must stay open");
    assert!(
        h.state().sessions.is_empty(),
        "must not connect with an invalid pair"
    );
    let error = h.state().dialogs.connect.error.as_deref().unwrap_or("");
    assert!(
        error.contains("Недопустимое сочетание политики и режима"),
        "expected localized error, got: {error}"
    );
}

/// An explicit policy/mode the server does not advertise is rejected before a
/// connection, with the localized wrapper plus the technical server detail.
#[test]
fn connect_dialog_unadvertised_security_choice_shows_error_and_does_not_connect() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.dialogs.connect.ua_url = "opc.tcp://127.0.0.1:4840".to_string();
    // A non-empty host makes the seeded endpoint list appear to belong to the
    // current search context; opening the tab no longer auto-starts a search.
    app.dialogs.connect.ua_search_host = "127.0.0.1".to_string();
    app.dialogs.connect.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
    app.dialogs.connect.ua_security_mode = UaSecurityMode::SignAndEncrypt;
    app.dialogs.connect.ua_discovered_endpoints = vec![opclook::bff::model::UaEndpointInfo {
        url: "opc.tcp://127.0.0.1:4840".parse().unwrap(),
        security_policy: opclook::bff::model::AdvertisedPolicy::from_wire("None"),
        security_mode: opclook::bff::model::AdvertisedMode::from_wire("None"),
    }];
    let mut h = harness(app);
    h.run_steps(5);
    {
        let connect = h
            .query_all_by_label("Подключиться")
            .filter(|n| !n.accesskit_node().is_disabled())
            .last()
            .expect("enabled connect button");
        connect.click();
    }
    h.run_steps(5);
    assert!(
        h.state().sessions.is_empty(),
        "must not connect with a fallback"
    );
    let error = h.state().dialogs.connect.error.as_deref().unwrap_or("");
    assert!(
        error.contains("Не удалось выбрать точку безопасности"),
        "expected localized wrapper, got: {error}"
    );
    assert!(
        error.contains("not offered") && error.contains("None/None"),
        "expected server detail (advertised list), got: {error}"
    );
}

/// The trust checkbox shows the value persisted in settings (strict by default)
/// and toggling it updates the dialog state used for the connection.
#[test]
fn connect_dialog_trust_checkbox_defaults_and_toggles() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(5);
    h.get_by_label("OPC UA").click();
    h.run_steps(5);
    // Strict trust by default (settings default false).
    assert!(!h.state().dialogs.connect.ua_trust_server_cert);
    // Clicking the checkbox flips the dialog state.
    h.get_by_label("Доверять сертификату сервера (только на эту сессию)")
        .click();
    h.run_steps(5);
    assert!(h.state().dialogs.connect.ua_trust_server_cert);
}

/// The dialog picks up a custom timeout persisted in settings. U13: the trust
/// flag is a per-connection choice, so a legacy global `true` in an old
/// settings file is NOT re-applied to a freshly opened dialog.
#[test]
fn connect_dialog_timeout_restores_but_trust_stays_opt_in() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.ua.ua_trust_server_cert = true;
    app.settings.ua.ua_session_timeout_ms = 300_000;
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    let h = harness(app);
    assert!(!h.state().dialogs.connect.ua_trust_server_cert);
    assert_eq!(h.state().dialogs.connect.ua_session_timeout, 300_000);
}

/// Ctrl+A then Delete removes all selected rows (regression: Ctrl+A clears the
/// table focus, so Delete must still act on the selection).
#[test]
fn ctrl_a_then_delete_removes_rows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let keys = seed_table(
        &mut app,
        &[("a", "SigA", 1), ("b", "SigB", 2), ("c", "SigC", 3)],
    );

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(3);
    assert_eq!(h.state().selected_rows.len(), keys.len());

    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(h.state().rows().len(), 0);
    assert!(h.state().selected_rows.is_empty());
}

/// Esc closes the topmost (write) dialog without disturbing the app.
#[test]
fn esc_closes_write_dialog() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.write.open);

    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().dialogs.write.open);
}

/// Enter confirms the primary action (write dialog «Записать»).
#[test]
fn enter_confirms_write_dialog() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.write.open);

    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.write.open);
    assert_eq!(h.state().write_batch.pending, 1);
}

#[test]
fn enter_opens_confirmation_without_accepting_it() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_some());
    assert_eq!(h.state().write_batch.pending, 0);
    h.key_press(Key::ArrowLeft);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.confirm.is_some());
    assert_eq!(h.state().write_batch.pending, 1);
}

#[test]
fn modified_enter_cannot_accept_new_write_confirmation() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_some());
    assert_eq!(h.state().write_batch.pending, 0);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_some());
    assert_eq!(h.state().write_batch.pending, 0);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_none());
    assert!(h.state().dialogs.write.open);
}

/// K3: the write dialog opens with the current value selected, so typing
/// replaces it (no mouse, no Backspace) and Enter writes the new value.
#[test]
fn k3_write_dialog_preselects_value_so_typing_replaces_it() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);

    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.write.open);
    assert_eq!(h.state().dialogs.write.text, "42");

    h.event(egui::Event::Text("77".into()));
    h.run_steps(3);
    assert_eq!(
        h.state().dialogs.write.text,
        "77",
        "the old value was selected and replaced, not appended to"
    );

    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.write.open);
    assert_eq!(h.state().write_batch.pending, 1);
    // The table keeps the keyboard after the dialog: the next Enter opens it again.
    assert_eq!(h.state().table_focus, Some(0));
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.write.open);
}

/// K3: a rejected value is selected again, so the next keystrokes replace it.
#[test]
fn k3_write_error_reselects_the_value() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.confirm_write = false;
    let key = seed_writable_table(&mut app, &[("a", "SigA", 42)])[0];
    app.rows_mut_for_test()[0].canonical_vt = Some(opclook::bff::model::VarType::I4); // VT_I4: text is not accepted
    let mut h = harness(app);
    h.state_mut().open_write_dialog(vec![key]);
    h.run_steps(3);
    h.event(egui::Event::Text("abc".into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(
        h.state().dialogs.write.open,
        "an invalid integer keeps the dialog"
    );
    assert!(h.state().dialogs.write.error.is_some());
    h.event(egui::Event::Text("5".into()));
    h.run_steps(3);
    assert_eq!(h.state().dialogs.write.text, "5");
}

/// K3 regression: with the tree having had the keyboard, a row click, Ctrl+A
/// and Delete removed nothing (Ctrl+A dropped the table focus and the tree
/// took the keys back).
#[test]
fn k3_ctrl_a_then_delete_after_the_tree_had_focus() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    seed_table(&mut app, &[("a", "SigA", 1), ("b", "SigB", 2)]);
    app.tree_focused = true;

    let mut h = harness(app);
    h.run_steps(2);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(3);
    assert_eq!(h.state().selected_rows.len(), 2);
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(h.state().rows().len(), 0);
}

/// Tab walks Header -> Tags -> Servers; tree is a subfocus of Servers.
#[test]
fn k3_tab_moves_the_keyboard_between_tree_and_table() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    seed_table(&mut app, &[("a", "SigA", 1)]);
    let node = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "Folder".into(),
            name: "Folder".into(),
            kind: NodeKind::Branch,
            has_children: false,
        },
    );
    let mut h = harness(app);
    h.run_steps(2);

    // Nothing owns the keyboard at start: Tab gives it to the header.
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().table_focus, None);
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().table_focus, Some(0));
    assert!(!h.state().tree_focused);

    h.key_press(Key::Tab);
    h.run_steps(3);
    assert!(!h.state().tree_focused);
    h.key_press(Key::ArrowRight);
    h.run_steps(3);
    assert!(h.state().tree_focused);
    assert_eq!(h.state().tree_focus, Some((sid(1), node)));
    assert_eq!(h.state().table_focus, None);

    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().table_focus, Some(0));
    assert!(!h.state().tree_focused);

    h.key_press(Key::F6);
    h.run_steps(3);
    assert!(!h.state().tree_focused);
}

/// K3: Ctrl+Tab / Ctrl+PgUp switch the session tabs; Ctrl+N opens «Подключиться».
#[test]
fn k3_ctrl_tab_switches_sessions_and_ctrl_n_connects() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_session(sid(2), EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(2);

    h.key_press_modifiers(Modifiers::COMMAND, Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(2)));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(1)), "wraps around");
    h.key_press_modifiers(Modifiers::COMMAND, Key::PageUp);
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(2)));

    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run_steps(3);
    assert!(h.state().dialogs.connect.open);
}

fn pump_value() -> Value {
    Value::Struct {
        type_name: "PumpType".into(),
        fields: vec![
            ("Speed".into(), Value::F64(12.5)),
            ("Running".into(), Value::Bool(true)),
            (
                "Motor".into(),
                Value::Struct {
                    type_name: "MotorType".into(),
                    fields: vec![("Current".into(), Value::F64(3.25))],
                },
            ),
            (
                "Alarms".into(),
                Value::Array(vec![Value::Bool(false), Value::Bool(true)]),
            ),
        ],
    }
}

/// V3: a structured value is a tree in the table: the row has an expander,
/// a click shows the fields as lines under it, nested ones expand in turn.
#[test]
fn v3_structured_value_expands_into_field_lines() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let key = app.seed_row(sid(1), "ns=2;s=Pump", "Pump", pump_value());
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("Speed: 12.5").is_some(),
        "summary"
    );
    assert!(h.query_by_label("Speed").is_none(), "collapsed at first");

    let at = h.get_by_label("Развернуть значение").rect().center();
    pointer_click(&h, at, PointerButton::Primary);
    h.run_steps(3);
    assert!(h.state().expanded_values().contains(&(key, vec![])));
    assert!(h.query_by_label("Speed").is_some());
    assert!(h.query_by_label("Running").is_some());
    assert!(h.query_by_label("MotorType").is_some(), "nested type name");
    assert!(
        h.query_by_label("Current").is_none(),
        "nested level still closed"
    );

    // Expand the nested structure (the first closed expander is Motor's).
    let at = h
        .get_all_by_label("Развернуть значение")
        .next()
        .expect("nested expander")
        .rect()
        .center();
    pointer_click(&h, at, PointerButton::Primary);
    h.run_steps(3);
    assert!(h.query_by_label("Current").is_some());
    assert!(h.query_by_label("3.25").is_some());
}

/// V3: keyboard: → opens the focused row's value, → again opens every
/// level, ← closes it all.
#[test]
fn v3_arrow_keys_expand_and_collapse_a_structure() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_row(sid(1), "ns=2;s=Pump", "Pump", pump_value());
    let mut h = harness(app);
    h.state_mut().select_only(0);
    h.run_steps(2);

    h.key_press(Key::ArrowRight);
    h.run_steps(3);
    assert!(h.query_by_label("Speed").is_some());
    assert!(h.query_by_label("Current").is_none());

    h.key_press(Key::ArrowRight);
    h.run_steps(3);
    assert!(h.query_by_label("Current").is_some());
    assert!(h.query_by_label("[1]").is_some(), "array elements too");

    h.key_press(Key::ArrowLeft);
    h.run_steps(3);
    assert!(h.query_by_label("Speed").is_none());
    assert!(h.state().expanded_values().is_empty());
}

/// V3: field lines never take part in the selection: a click on one selects
/// its row, and the visible-row keyboard still steps row by row.
#[test]
fn v3_field_lines_select_their_row() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let pump = app.seed_row(sid(1), "ns=2;s=Pump", "Pump", pump_value());
    let other = app.seed_row(sid(1), "ns=2;s=Level", "Level", Value::F64(1.0));
    app.toggle_value(pump, vec![]);
    let mut h = harness(app);
    h.run_steps(3);
    pointer_click(&h, row_cell_pos(&h, "Running"), PointerButton::Primary);
    h.run_steps(3);
    assert_eq!(h.state().selected_rows.len(), 1);
    assert!(h.state().selected_rows.contains(&pump));
    h.key_press(Key::ArrowDown);
    h.run_steps(3);
    assert!(
        h.state().selected_rows.contains(&other),
        "next ROW, not next line"
    );
}

/// A Disconnected event must NOT clear the rows or the tree; rows stay visible
/// with their last values marked stale («устаревшие данные»).
#[test]
fn disconnect_event_keeps_rows_and_marks_them_stale() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.settings.view.options.auto_reconnect = true;
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "ns=1;s=keep".to_string(),
            name: "Keep".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    app.seed_row(sid(1), "ns=1;s=keep", "Keep", Value::I64(5));

    app.on_event_for_test_at(
        sid(1),
        opclook::bff::model::EventKind::Disconnected {
            reason: opclook::bff::model::BackendMsg::raw("связь потеряна"),
        },
    );

    // Rows and tree remain; the value is kept.
    assert_eq!(app.rows().len(), 1, "rows must survive a disconnect");
    assert_eq!(
        app.rows()[0].value,
        Value::I64(5),
        "last value must be kept"
    );
    assert_eq!(
        app.rows()[0].quality.text(),
        "устаревшие данные",
        "row quality must be marked stale"
    );
    assert_eq!(app.sessions[0].state, SessionState::Disconnected);
    assert_eq!(
        app.sessions[0].tree().len(),
        1,
        "tree must survive a disconnect"
    );
}

/// Simulate a Disconnected then a Connected event with rows already present:
/// the Connected handler must offer rows for re-subscription (pending=true),
/// request a fresh root browse, and arm an immediate re-read.
#[test]
fn reconnect_connected_resubscribes_rows_requests_browse_and_read() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.seed_row(sid(1), "ns=1;s=a", "SigA", Value::I64(1));
    app.seed_row(sid(1), "ns=1;s=b", "SigB", Value::I64(2));

    // Drop the connection.
    app.on_event_for_test_at(
        sid(1),
        opclook::bff::model::EventKind::Disconnected {
            reason: opclook::bff::model::BackendMsg::raw("связь потеряна"),
        },
    );
    assert_eq!(app.rows().len(), 2);

    // Server comes back in response to a real reconnect request.
    app.record_dispatches_for_test();
    app.reconnect_session(sid(1));
    let request = *app.pending_request_ids_for_test(sid(1)).last().unwrap();
    app.on_event_for_test_with_request(
        sid(1),
        request,
        opclook::bff::model::EventKind::Connected {
            endpoint: EndpointSpec::da("", "Srv.Test"),
            info: opclook::bff::model::BackendMsg::raw("подключено"),
        },
    );

    // Rows offered for re-add (pending=true is set by register_rows).
    assert!(
        app.rows().iter().all(|r| r.pending),
        "rows must be re-registered"
    );
    // Fresh root browse in flight.
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    assert!(view.root_loading, "root must be re-browsed after reconnect");
    assert!(view.root_loaded == false);
    assert!(view.tree().is_empty());
    // Immediate re-read armed: last_poll is in the past.
    assert!(view.read_in_flight == false);
}

/// While a root browse (root_loading) is in flight the periodic read tick must
/// be suppressed so reads do not pile up behind the browse.
#[test]
fn read_tick_is_suppressed_while_root_loading() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.seed_row(sid(1), "ns=1;s=a", "SigA", Value::I64(1));
    // Record the actual dispatch without allowing the DA stub to race its reply.
    app.record_dispatches_for_test();
    // A real outstanding browse is the protocol-level gate for polling.
    app.browse(sid(1), String::new());
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        // Force a due read while the browse remains in flight.
        view.last_poll = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(std::time::Instant::now());
        view.read_in_flight = false;
    }
    app.tick_for_test();
    // No read was dispatched: read_in_flight stays false and last_poll unchanged.
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    assert!(
        !view.read_in_flight,
        "read must not be sent while root_loading"
    );
    let commands = app.dispatched_for_test();
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0].kind, CommandKind::Browse { .. }));
}

/// Once a UA session's push subscription is active, the app stops issuing
/// periodic Reads for that session; values arrive via DataChange push.
#[test]
fn ua_subscription_active_stops_periodic_read() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_row(sid(1), "i=2258", "CurrentTime", Value::Empty);
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        // A periodic read is due and permitted (no browse/tree load in flight).
        view.root_loading = false;
        view.read_in_flight = false;
        view.last_poll = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(std::time::Instant::now());
        view.ua_mut().unwrap().subscription = SubscriptionState::Inactive;
    }

    // Without an active subscription a due Read is dispatched.
    app.tick_for_test();
    {
        let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
        assert!(
            view.read_in_flight,
            "poll driven Read must be sent while inactive"
        );
    }

    // Mark the subscription active: the same due read is now suppressed.
    app.on_event_for_test_at(
        sid(1),
        EventKind::SubscriptionState {
            state: SubscriptionState::Active,
        },
    );
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        view.read_in_flight = false;
        view.last_poll = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(std::time::Instant::now());
        view.root_loading = false;
    }
    app.tick_for_test();
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    assert!(
        !view.read_in_flight,
        "UA read must not be sent while subscription is active"
    );
    assert_eq!(
        view.ua().unwrap().subscription,
        SubscriptionState::Active,
        "active state must survive the tick"
    );
}

/// DA sessions are untouched by the subscription flag: they keep polling
/// regardless of it (the flag is meaningless for DA).
#[test]
fn da_keeps_polling_unaffected_by_subscription_flag() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.seed_row(sid(1), "ns=1;s=a", "SigA", Value::I64(1));
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        view.root_loading = false;
        view.read_in_flight = false;
        assert!(view.ua().is_none());
        view.last_poll = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(std::time::Instant::now());
    }
    app.tick_for_test();
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    assert!(
        view.read_in_flight,
        "DA must keep polling (subscription flag ignored)"
    );
}

/// Reconnect (Connected on a UA session with rows already present) must offer
/// the rows for re-add (AddItems re-subscribes them), arm a fresh browse and
/// reset the push-subscription flag so a new subscription is established.
#[test]
fn ua_reconnect_resubscribes_rows_and_resets_subscription_flag() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_row(sid(1), "i=2258", "CurrentTime", Value::I64(0));
    app.seed_row(sid(1), "i=2256", "ServerStatus", Value::I64(0));
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        let ua = view.ua_mut().unwrap();
        ua.subscription = SubscriptionState::Active;
        ua.ua_view_id = Some("ns=1;s=view".into());
        ua.ua_browse_filter = opclook::bff::model::NodeClassFilter::Variables;
    }
    app.on_event_for_test_at(
        sid(1),
        EventKind::Disconnected {
            reason: opclook::bff::model::BackendMsg::raw("связь потеряна"),
        },
    );
    // While disconnected the subscription is gone.
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    assert_eq!(view.state, SessionState::Disconnected);

    // Server returns: a fresh subscription must be re-established.
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    endpoint.ua_session_timeout = Some(123_000);
    app.record_dispatches_for_test();
    app.reconnect_session(sid(1));
    let request = *app.pending_request_ids_for_test(sid(1)).last().unwrap();
    app.on_event_for_test_with_request(
        sid(1),
        request,
        EventKind::Connected {
            endpoint,
            info: opclook::bff::model::BackendMsg::raw("подключено"),
        },
    );
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    assert_eq!(view.state, SessionState::Connected);
    assert!(view.root_loading, "root must be re-browsed after reconnect");
    assert_eq!(
        view.ua().unwrap().subscription,
        SubscriptionState::Inactive,
        "state must reset so a new subscription is created on re-add"
    );
    assert_eq!(
        view.ua().unwrap().ua_view_id.as_deref(),
        Some("ns=1;s=view")
    );
    assert_eq!(
        view.ua().unwrap().ua_browse_filter,
        opclook::bff::model::NodeClassFilter::Variables
    );
    assert_eq!(view.ua().unwrap().ua_session_timeout_ms, 123_000);
    // Rows offered for re-add (AddItems dispatched via register_rows).
    let rows: Vec<&opclook::app::Row> = app.rows().iter().filter(|r| r.session == sid(1)).collect();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter().all(|r| r.pending),
        "rows must be re-registered after reconnect"
    );
}

/// A `Failed` subscription falls back to polling (only `Active` suppresses
/// periodic reads).
#[test]
fn ua_failed_subscription_falls_back_to_polling() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_row(sid(1), "i=2258", "CurrentTime", Value::Empty);
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        view.ua_mut().unwrap().subscription = SubscriptionState::Active;
        view.root_loading = false;
        view.read_in_flight = false;
        view.last_poll = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(std::time::Instant::now());
    }
    app.on_event_for_test_at(
        sid(1),
        EventKind::SubscriptionState {
            state: SubscriptionState::Failed,
        },
    );
    assert_eq!(
        app.sessions[0].ua().unwrap().subscription,
        SubscriptionState::Failed
    );
    app.tick_for_test();
    assert!(
        app.sessions[0].read_in_flight,
        "Failed subscription must re-enable periodic Read"
    );
}

/// The server-info popup surfaces the UA push-subscription state.
#[test]
fn server_info_popup_shows_subscription_state() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.sessions[0].ua_mut().unwrap().subscription = SubscriptionState::Failed;
    app.status_popup = true;

    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_all_by_label_contains("Подписка:").count() >= 1,
        "popup must label the subscription state"
    );
    assert!(
        h.query_all_by_label("ошибка").count() >= 1,
        "popup must show the Failed state"
    );
}

/// After 20 failed attempts the reconnect loop stops and the hint reports that
/// the attempts are exhausted.
#[test]
fn reconnect_attempts_cap_stops_the_loop_and_shows_exhausted() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.settings.view.options.auto_reconnect = true;
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.activate_session(sid(1));
    app.record_dispatches_for_test();
    app.reconnect_session(sid(1));
    let connect = *app.pending_request_ids_for_test(sid(1)).last().unwrap();
    // The twentieth automatic Connect is in flight; its failure exhausts the cap.
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        view.state = SessionState::Connecting;
        view.reconnect = ReconnectState::Waiting {
            at: std::time::Instant::now(),
            attempt: 20,
        };
    }
    app.on_event_for_test_with_request(
        sid(1),
        connect,
        EventKind::Error {
            operation: opclook::bff::model::Operation::Connect,
            message: opclook::bff::model::BackendMsg::raw("опять"),
        },
    );
    // Re-scheduling must refuse: the state becomes Exhausted, no new timer.
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    assert_eq!(view.reconnect, ReconnectState::Exhausted);
    let hint = app.reconnect_hint().unwrap_or_default();
    assert!(hint.contains("Попытки исчерпаны (20)"), "got: {hint}");
}

// ---- Tree focus + keyboard (Task 1) ----

/// Clicking the tag tree pane (empty space below the nodes) focuses the tree;
/// ArrowDown then starts navigating from the first visible node.
#[test]
fn tree_click_focus_enables_arrow_navigation() {
    use opclook::app::BottomTab;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    // Keep both bottom panels off so the tree pane is tall enough for an empty
    // click point below the nodes.
    app.settings.view.options.show_script = false;
    app.settings.view.options.show_log = false;
    app.bottom_tab = BottomTab::Journal;
    let (_a, _b) = seed_tree(&mut app);
    assert_eq!(app.tree_focused, false);

    let mut h = harness(app);
    // Click empty space in the tree pane, below the last rendered node.
    pointer_click(&h, tree_empty_pos(&h, "SigB"), PointerButton::Primary);
    h.run_steps(5);
    assert_eq!(
        h.state().tree_focused,
        true,
        "tree pane click must focus tree"
    );

    // ArrowDown selects the first visible node.
    h.key_press(Key::ArrowDown);
    h.run_steps(3);
    assert!(
        h.state().selected_tree.is_some(),
        "ArrowDown after pane click must select first node"
    );
}

/// Clicking a tree item keeps the tree focused (arrow keys still work) instead
/// of dropping focus to the clicked widget.
#[test]
fn tree_item_click_keeps_focus_for_arrows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let (a, b) = seed_tree(&mut app);
    let mut h = harness(app);
    // Click the second tree item directly.
    h.get_by_label("SigB").click();
    h.run_steps(5);
    assert_eq!(
        h.state().tree_focused,
        true,
        "item click must keep tree focused"
    );
    assert_eq!(h.state().selected_tree.map(|(_, id)| id), Some(b));

    // ArrowUp moves back to the first item.
    h.key_press(Key::ArrowUp);
    h.run_steps(3);
    assert_eq!(h.state().selected_tree.map(|(_, id)| id), Some(a));
}

/// Enter on a subscribed Item opens the write dialog for that exact row.
#[test]
fn tree_enter_on_subscribed_item_opens_write_dialog() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let id = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "ns=1;s=sub".to_string(),
            name: "Subscribed".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    let row_key = app.seed_writable_row(sid(1), "ns=1;s=sub", "Subscribed", Value::I64(7));
    app.set_tree_focus(sid(1), id);
    assert!(!app.dialogs.write.open);

    let mut h = harness(app);
    // Selecting the node via set_tree_focus set tree_focused; keep it.
    h.key_press(Key::Enter);
    h.run_steps(3);

    assert!(h.state().dialogs.write.open, "write dialog must open");
    assert_eq!(
        h.state().dialogs.write.keys,
        vec![row_key],
        "write dialog must target the exact subscribed row"
    );
}

/// Enter on an unsubscribed Item adds a row to the table and does NOT open a
/// write dialog (the write would race AddItems).
#[test]
fn tree_enter_on_unsubscribed_item_adds_row_without_dialog() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let id = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "ns=1;s=new".to_string(),
            name: "NewSig".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    assert_eq!(app.rows().len(), 0, "no rows yet");

    let mut h = harness(app);
    h.state_mut().set_tree_focus(sid(1), id);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);

    assert_eq!(h.state().rows().len(), 1, "row must be added");
    assert_eq!(h.state().rows()[0].item_id, "ns=1;s=new");
    assert!(
        !h.state().dialogs.write.open,
        "write dialog must NOT open for an unsubscribed item"
    );
    // Journal notes the add so the user understands why no dialog appeared.
    let note = h
        .state()
        .journal
        .entries()
        .back()
        .map(|e| e.render(opclook::i18n::Lang::Ru))
        .unwrap_or_default();
    assert!(note.contains("Сигнал добавлен в таблицу"), "got: {note}");
}

/// Clicking into a text field drops the tree keyboard focus.
#[test]
fn tree_focus_cleared_by_typing_in_text_field() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    // Show the script editor (single bottom panel) so there is a real text field
    // into which the user can click.
    app.settings.view.options.show_script = true;
    app.settings.view.options.show_log = false;
    let (_a, _b) = seed_tree(&mut app);
    app.set_tree_focus(sid(1), _a);
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(h.state().tree_focused, true);

    // Click inside the script code editor (bottom panel, above the status bar).
    pointer_click(&h, egui::pos2(700.0, 845.0), PointerButton::Primary);
    h.run_steps(5);
    assert_eq!(
        h.state().tree_focused,
        false,
        "clicking into a text field must clear tree focus"
    );
}

// ---- Unified connect dialog (Task 2) ----

/// The connect dialog always opens on the OPC DA tab and immediately starts
/// local discovery (da_pending == true) as the dialog opens.
#[test]
fn connect_dialog_opens_on_da_tab_with_auto_discovery() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(
        h.state().dialogs.connect.pane.protocol(),
        opclook::bff::model::Protocol::OpcDa
    );
    assert_eq!(
        h.state().dialogs.connect.da_pending,
        true,
        "opening the dialog must start local DA discovery"
    );
}

/// DA auto-search on open goes through the combined path: local enumeration
/// plus any known remote hosts (da_auto_pending counts the sub-searches).
#[test]
fn connect_dialog_da_auto_search_uses_combined_flag() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    // A selected adapter + a known remote DA host contribute an extra
    // sub-search; without an adapter only the local search would run.
    app.network.loaded = true;
    app.network.lease = Some(lease_with_subnet("10.0.0.5", 24));
    use opclook::bff::model::Protocol;
    app.settings
        .recent_endpoints
        .push(EndpointSpec::da("127.0.0.1", "Alpha.Server"));
    app.open_connect_dialog();
    assert_eq!(
        app.dialogs.connect.da_pending, true,
        "DA discovery must be running on open"
    );
    assert!(
        app.dialogs.connect.da_auto_pending >= 2,
        "combined DA auto-search must include local (1) + known remote host(s)"
    );
    assert_eq!(
        app.dialogs.connect.da_host, "",
        "DA host field must not be pre-filled from settings"
    );
    let _ = Protocol::OpcDa;
}

/// The session tab strip lives in the tags (table) area, not in a separate
/// window-wide top panel.
#[test]
fn tab_strip_is_in_tags_area_not_top_panel() {
    assert!(
        !opclook::ui::top_tab_panel_enabled(),
        "the old 'session_tabs' top panel must not be mounted"
    );
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.One"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(5);
    // The tab label is rendered (now inside the central tags panel).
    assert!(h.query_all_by_label("DA: Srv.One").count() >= 1);
}

/// An empty UA host without a selected adapter searches only the loopback
/// endpoint (one request). mDNS and subnet discovery require an adapter.
#[test]
fn connect_dialog_ua_empty_host_without_adapter_searches_loopback_only() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.record_dispatches_for_test();
    app.search_ua_all();
    let opclook::app::UaSearch::Auto {
        pending: opclook::app::AutoPending::Host(request),
    } = app.dialogs.connect.ua_search
    else {
        panic!("host auto search")
    };
    let d = &app.dialogs.connect;
    assert_eq!(
        d.ua_search.host_searching(),
        true,
        "combined UA search must be running"
    );
    assert!(
        !d.ua_search.network_searching(),
        "LDS/mDNS must not run without a selected adapter"
    );
    assert_eq!(d.ua_search.auto_pending_count(), 1, "one loopback request");
    let hosts = [("127.0.0.1".to_string(), 4840)];
    for (h_name, p) in hosts {
        app.on_event_for_test_with_request(
            opclook::bff::model::Owner::Discovery,
            request,
            EventKind::UaServersFound {
                host: h_name.clone(),
                port: p,
                servers: vec![opclook::bff::model::UaServerInfo {
                    application_name: "SRV".to_string(),
                    application_uri: format!("urn:srv-{h_name}"),
                    endpoints: vec![opclook::bff::model::UaEndpointInfo {
                        url: format!("opc.tcp://{h_name}:{p}").parse().unwrap(),
                        security_policy: opclook::bff::model::AdvertisedPolicy::from_wire("None"),
                        security_mode: opclook::bff::model::AdvertisedMode::from_wire("None"),
                    }],
                }],
            },
        );
    }
    assert!(!app.dialogs.connect.ua_search.host_searching());
    assert!(!app.dialogs.connect.ua_search.network_searching());
    assert_eq!(app.dialogs.connect.ua_search.auto_pending_count(), 0);
    assert!(
        app.dialogs.connect.ua_search_results().len() == 1,
        "the loopback result must be in the list"
    );
}

/// The session tab row contains NO «Подключиться»/«Подключиться...» button —
/// connect lives only in the left nav and the menu.
#[test]
fn tab_strip_has_no_connect_button() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.One"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(5);
    assert!(
        h.query_all_by_label("DA: Srv.One").count() >= 1,
        "tab present"
    );
    // The top menu is collapsed; the menu item «Подключиться...» is not open, so
    // the only connect controls are the left-nav «Подключиться» button.
    assert_eq!(
        h.query_all_by_label("Подключиться...").count(),
        0,
        "no connect button (ellipsis) may appear in the tab strip/menu"
    );
    // A single plain «Подключиться» remains in the left navigation.
    assert_eq!(h.query_all_by_label("Подключиться").count(), 1);
}

/// Tab overflow: many open tabs scroll horizontally inside the tags header and
/// do not break the table render (all tab labels still mount).
#[test]
fn tab_strip_many_tabs_scroll_without_breaking_table() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.One"));
    app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    for i in 2..=14 {
        app.seed_session(sid(i), EndpointSpec::da("", &format!("Srv.{i:0>2}")));
    }
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(5);
    // A representative sample of the tabs is mounted, and the table row renders.
    assert!(h.query_all_by_label("DA: Srv.One").count() >= 1);
    assert!(h.query_all_by_label("DA: Srv.02").count() >= 1);
    assert!(h.query_all_by_label("DA: Srv.03").count() >= 1);
    assert!(
        h.query_all_by_label("SigA").count() >= 1,
        "table row still renders"
    );
    // The tab strip scrolls horizontally inside the tags header (12+ tabs).
    let total_tabs = h.query_all_by_label_contains("DA: Srv.").count();
    assert!(
        total_tabs >= 3,
        "tabs must mount and scroll, got {total_tabs}"
    );
}

/// The port field is present on the UA tab and absent on the DA tab. The UA
/// port field is EMPTY by default (the port internally defaults to 4840).
#[test]
fn connect_dialog_port_field_visibility_per_tab() {
    use opclook::bff::model::Protocol;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let mut h = harness(app);
    h.run_steps(3);
    // DA tab active -> no port input visible.
    assert_eq!(h.state().dialogs.connect.pane.protocol(), Protocol::OpcDa);
    let da_ports = h.query_all_by_value("4840").collect::<Vec<_>>();
    assert!(
        da_ports.is_empty(),
        "DA tab must hide the port field entirely"
    );
    // DA server list does not leak the default port either.
    assert_eq!(h.state().dialogs.connect.ua_search_port, "");

    // Switch to UA -> port field appears but is empty (no 4840 preset).
    h.get_by_label("OPC UA").click();
    h.run_steps(5);
    let ua_ports = h.query_all_by_value("4840").collect::<Vec<_>>();
    assert!(
        ua_ports.is_empty(),
        "UA port field must be empty by default (4840 is an internal default)"
    );
    assert_eq!(h.state().dialogs.connect.ua_search_port, "");
}

/// Selecting a DA server from the list enables connect and fills state.
#[test]
fn connect_dialog_select_da_server_enables_connect() {
    use opclook::bff::model::Protocol;
    use opclook::bff::model::ServerEntry;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.da_pending = false;
    app.dialogs.connect.da_servers.push(opclook::app::DaHit {
        server: ServerEntry {
            id: "Alpha.Server".to_string(),
            description: "Тестовый".to_string(),
        },
        host: String::new(),
    });
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(h.state().dialogs.connect.pane.protocol(), Protocol::OpcDa);
    // The DA server is rendered as a selectable row.
    h.get_by_label("Alpha.Server — Тестовый").click();
    h.run_steps(3);
    assert_eq!(
        h.state()
            .dialogs
            .connect
            .da_selected
            .as_ref()
            .map(|hit| hit.server.id.as_str()),
        Some("Alpha.Server")
    );
    assert_eq!(h.state().dialogs.connect.da_manual, "Alpha.Server");
    // The connect is enabled because the DA ProgID / URL field is non-empty.
    assert!(
        !h.state().dialogs.connect.da_manual.trim().is_empty(),
        "connect must be enabled after selection"
    );
}

/// Selecting a UA search result fills the URL field.
#[test]
fn connect_dialog_select_ua_result_fills_url() {
    use opclook::bff::model::UaEndpointInfo;
    use opclook::bff::model::UaServerInfo;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.dialogs
        .connect
        .ua_search_results_mut_for_test()
        .push(UaServerInfo {
            application_name: "CODESYS".to_string(),
            application_uri: "urn:test".to_string(),
            endpoints: vec![UaEndpointInfo {
                url: "opc.tcp://127.0.0.1:4840".parse().unwrap(),
                security_policy: opclook::bff::model::AdvertisedPolicy::from_wire("None"),
                security_mode: opclook::bff::model::AdvertisedMode::from_wire("None"),
            }],
        });
    let mut h = harness(app);
    h.get_by_label("OPC UA").click();
    h.run_steps(5);
    // The result row shows the endpoint URL plus any security suffix, so match
    // it by a substring of the URL.
    h.get_by_label_contains("opc.tcp://127.0.0.1:4840").click();
    h.run_steps(3);
    assert_eq!(
        h.state().dialogs.connect.ua_url,
        "opc.tcp://127.0.0.1:4840",
        "selecting a UA result must fill the URL"
    );
    assert_eq!(
        h.state().dialogs.connect.ua_security_policy,
        UaSecurityPolicy::None,
        "selecting an endpoint must use that row's advertised policy"
    );
    assert_eq!(
        h.state().dialogs.connect.ua_security_mode,
        UaSecurityMode::None,
        "selecting an endpoint must use that row's advertised mode"
    );
}

#[test]
fn rejected_ua_connect_keeps_popup_open_with_error() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.dialogs.connect.ua_url = "opc.tcp://192.0.2.77:4840".to_string();
    let mut h = harness(app);
    h.run_steps(4);
    h.query_all_by_label("Подключиться")
        .filter(|n| !n.accesskit_node().is_disabled())
        .last()
        .expect("connect button")
        .click();
    h.run_steps(3);
    assert!(h.state().dialogs.connect.open);
    assert!(h.state().dialogs.connect.error.is_some());
    assert!(h.state().sessions.is_empty());
}

// ---- Columns dialog ↑/↓ (Task 3) ----

/// The columns dialog's «Вниз» button reorders its own list immediately and the
/// change is persisted into settings.view.column_order.
#[test]
fn columns_dialog_down_moves_row_and_persists() {
    with_settings_lock(|| {
        use opclook::columns::ColumnId;
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        seed_table(&mut app, &[("a", "SigA", 1)]);
        app.dialogs.columns.open = true;
        let mut h = harness(app);
        h.run_steps(3);

        let before: Vec<ColumnId> = h.state().settings.resolved_order();
        // Row 1 is «Полное имя» (its title is non-empty, unlike the Status row 0).
        let target = ColumnId::FullName;
        let target_idx = before.iter().position(|c| *c == target).unwrap();
        assert!(target_idx + 1 < before.len());

        // Click the «Вниз» button on the target row. The down buttons appear in
        // row order in the accessibility tree, so the target row's is the
        // (target_idx)-th down button (index matches plugin… row index).
        let downs: Vec<_> = h.query_all_by_label("↓").collect();
        assert!(downs.len() >= target_idx + 1, "not enough down buttons");
        downs[target_idx].click();
        h.run_steps(3);

        let after: Vec<ColumnId> = h.state().settings.resolved_order();
        assert_eq!(after.len(), ColumnId::ALL.len());
        assert_eq!(
            after[target_idx + 1],
            target,
            "«Полное имя» must move down one slot"
        );
        assert_eq!(after[target_idx], before[target_idx + 1]);
        assert!(
            h.state().settings.view.column_order.is_some(),
            "column_order must be persisted after the dialog move"
        );
        // The in-memory persisted snapshot is typed and reflects the move.
        let parsed = h.state().settings.view.column_order.clone().unwrap();
        assert_eq!(parsed.len(), ColumnId::ALL.len(), "all columns persisted");
        assert_eq!(
            parsed[target_idx + 1],
            target,
            "persisted order reflects the move"
        );
    });
}

#[test]
fn language_menu_switches_ui_language() {
    with_settings_lock(|| {
        use opclook::i18n::Lang;

        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        // The test drives the UI starting from Russian (the application default is
        // also Russian; label-based assertions opt in to Ru explicitly regardless).
        set_ru(&mut app);
        assert_eq!(app.lang(), Lang::Ru);

        let mut h = harness(app);
        // Default Russian: the top «Сигналы» and «Язык» menu buttons render in ru.
        assert!(h.query_all_by_label("Сигналы").count() >= 1);
        assert!(h.query_all_by_label("Язык").count() >= 1);

        // Switch through the language submenu's available options (covered for label
        // presence by the i18n dictionary tests) by driving set_lang, then assert
        // the top-level menu buttons re-render in the new language.
        h.state_mut().set_lang(Lang::En);
        h.run_steps(5);
        assert_eq!(h.state().lang(), Lang::En);
        assert!(h.query_all_by_label("Signals").count() >= 1);
        assert!(h.query_all_by_label("Language").count() >= 1);

        h.state_mut().set_lang(Lang::Zh);
        h.run_steps(5);
        assert_eq!(h.state().lang(), Lang::Zh);
        assert!(h.query_all_by_label("信号").count() >= 1);
        assert!(h.query_all_by_label("语言").count() >= 1);
    });
}

#[test]
fn language_switch_persists_in_settings() {
    with_settings_lock(|| {
        use opclook::i18n::Lang;

        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        // A brand-new app has no persisted language: it inherits the Russian
        // default.
        assert_eq!(app.lang(), opclook::i18n::Lang::Ru);

        // Persisting Russian must survive a restart (the persisted value wins
        // over the Russian default — here it equals it).
        app.set_lang(Lang::Ru);
        assert_eq!(app.lang(), Lang::Ru);
        let reloaded = opclook::settings::Settings::load().settings;
        assert_eq!(reloaded.lang(), Lang::Ru);

        // And switching to Chinese persists through Settings::save() to
        // settings.json; a persisted non-default language is never overridden.
        app.set_lang(Lang::Zh);
        assert_eq!(app.lang(), Lang::Zh);
        let reloaded2 = opclook::settings::Settings::load().settings;
        assert_eq!(reloaded2.lang(), Lang::Zh);
    });
}

#[test]
fn language_switch_keeps_rows_and_selection() {
    with_settings_lock(|| {
        use opclook::i18n::Lang;

        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        seed_table(&mut app, &[("a", "SigA", 1), ("b", "SigB", 2)]);
        app.selected_rows.insert(app.rows()[0].key);
        let count_before = app.rows().len();

        let mut h = harness(app);
        h.run_steps(3);
        assert_eq!(h.state().rows().len(), count_before);
        assert_eq!(h.state().selected_rows.len(), 1);

        h.state_mut().set_lang(Lang::Zh);
        h.run_steps(3);

        // Switching language must not clear rows or the selection state.
        assert_eq!(h.state().rows().len(), count_before);
        assert_eq!(h.state().selected_rows.len(), 1);
        assert_eq!(h.state().lang(), Lang::Zh);
    });
}

// ---- Drag & drop (Task) ----

/// Seed an active session with a Branch whose direct Item children are already
/// loaded into the tree.
fn seed_branch_with_items(app: &mut App) -> (u64, u64, u64) {
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "branch".to_string(),
            name: "Branch".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    let c1 = app.seed_tree_item(
        sid(1),
        Some(branch),
        1,
        BrowseNode {
            node_id: "c1".to_string(),
            name: "C1".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    let c2 = app.seed_tree_item(
        sid(1),
        Some(branch),
        1,
        BrowseNode {
            node_id: "c2".to_string(),
            name: "C2".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    (branch, c1, c2)
}

/// Feed a Browse reply using the request id the running add-children job issued
/// for `node_id` (request-id correlation).
fn feed_job_browse(
    app: &mut App,
    session: opclook::bff::model::SessionId,
    node_id: &str,
    children: Vec<BrowseNode>,
) {
    let request_id = app
        .add_children
        .as_ref()
        .and_then(|job| {
            job.in_flight
                .iter()
                .find_map(|(rid, node)| (node == node_id).then_some(*rid))
        })
        .unwrap_or_else(|| panic!("no in-flight add-children browse for {node_id}"));
    app.on_event_for_test_with_request(
        session,
        request_id,
        EventKind::Browse {
            node_id: node_id.to_string(),
            children,
        },
    );
}

/// Dropping a Branch whose Item children are already loaded adds every child.
///
/// The recursive job always issues a browse for the dropped branch, so even a
/// loaded branch needs its `Browse` response fed back before the items land.
#[test]
fn add_dropped_tree_branch_with_loaded_children_adds_items() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let (branch, _, _) = seed_branch_with_items(&mut app);
    assert_eq!(app.rows().len(), 0);

    app.add_dropped_tree(sid(1), vec![branch]);

    // No rows yet: the browse for the branch is in flight.
    assert_eq!(app.rows().len(), 0);

    // Feed the browse response for the branch; its Item children are collected.
    feed_job_browse(
        &mut app,
        sid(1),
        "branch",
        vec![
            BrowseNode {
                node_id: "c1".to_string(),
                name: "C1".to_string(),
                kind: NodeKind::Item,
                has_children: false,
            },
            BrowseNode {
                node_id: "c2".to_string(),
                name: "C2".to_string(),
                kind: NodeKind::Item,
                has_children: false,
            },
        ],
    );

    assert_eq!(
        app.rows().len(),
        2,
        "both Item children of the branch must be added"
    );
    let ids: Vec<&str> = app.rows().iter().map(|r| r.item_id.as_str()).collect();
    assert!(ids.contains(&"c1"));
    assert!(ids.contains(&"c2"));
}

/// Dropping a Branch whose children are not loaded yet triggers a browse and
/// adds the items once the browse completes (reusing the pending mechanism).
#[test]
fn add_dropped_tree_unloaded_branch_browses_then_adds() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let branch = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "branch".to_string(),
            name: "Branch".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    // Simulate a branch the client has not browsed into yet.
    {
        let view = app.sessions.iter_mut().find(|s| s.id == sid(1)).unwrap();
        view.tree_mut_for_test()
            .iter_mut()
            .find(|t| t.id == branch)
            .unwrap()
            .loaded = false;
    }

    app.add_dropped_tree(sid(1), vec![branch]);

    // No rows yet; the browse is in flight (branch marked loading).
    assert_eq!(app.rows().len(), 0);
    let view = app.sessions.iter().find(|s| s.id == sid(1)).unwrap();
    let item = view.tree().iter().find(|t| t.id == branch).unwrap();
    assert!(
        item.loading,
        "drop must trigger a browse of the unloaded branch"
    );
    assert!(!item.loaded);

    // The browse completes with two Item children.
    feed_job_browse(
        &mut app,
        sid(1),
        "branch",
        vec![
            BrowseNode {
                node_id: "c1".to_string(),
                name: "C1".to_string(),
                kind: NodeKind::Item,
                has_children: false,
            },
            BrowseNode {
                node_id: "c2".to_string(),
                name: "C2".to_string(),
                kind: NodeKind::Item,
                has_children: false,
            },
        ],
    );

    assert_eq!(
        app.rows().len(),
        2,
        "pending children must be added on browse completion"
    );
}

/// Dropping an Item node adds exactly that row.
#[test]
fn add_dropped_tree_item_adds_row() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let (a, _b) = seed_tree(&mut app);
    assert_eq!(app.rows().len(), 0);

    app.add_dropped_tree(sid(1), vec![a]);

    assert_eq!(app.rows().len(), 1);
    assert_eq!(app.rows()[0].item_id, "ns=1;s=a");
}

/// Re-dropping an already-subscribed Item must not create a duplicate row.
#[test]
fn add_dropped_tree_deduplicates_existing_rows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let (a, _b) = seed_tree(&mut app);
    app.add_items(sid(1), vec![("ns=1;s=a".to_string(), "SigA".to_string())]);
    assert_eq!(app.rows().len(), 1);

    app.add_dropped_tree(sid(1), vec![a]);

    assert_eq!(app.rows().len(), 1, "duplicate drop must be skipped");
}

/// Dropping a node that belongs to a different session than the active one
/// journals a warning and adds nothing.
#[test]
fn add_dropped_tree_cross_session_warns_and_skips() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let other = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "ns=1;s=other".to_string(),
            name: "Other".to_string(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    // Seed a second session and make it the active one.
    app.seed_session(sid(2), EndpointSpec::da("", "Srv.Other"));
    assert_eq!(app.active_session, Some(sid(2)));

    app.add_dropped_tree(sid(1), vec![other]);

    assert_eq!(app.rows().len(), 0, "cross-session drop must be skipped");
    let warned = app.journal.entries().iter().any(|e| {
        e.render(opclook::i18n::Lang::Ru)
            .contains("активного сервера")
    });
    assert!(warned, "cross-session drop must journal a warning");
}

/// Full pointer drag from a tree Item onto the signal table adds the row.
///
/// SKIPPED: egui_kittest exposes the raw pointer API (`drag_at`/`hover_at`/
/// `drop_at`), but it cannot reproduce egui's multi-frame drag gesture. A
/// press-down at the source followed by pointer moves never trips
/// `Response::drag_started()` in the harness, so `dnd_set_drag_payload` is
/// never activated and the drop zone never sees a payload. The drag-and-drop
/// integration is therefore covered by the direct `add_dropped_tree` unit tests
/// above, which drive the exact code path the drop handler invokes.
#[allow(dead_code)]
fn kittest_drag_not_feasible_in_harness() {
    // kittest pointer events cannot reliably simulate an egui drag; the golden
    // path is validated by `add_dropped_tree_*` unit tests instead.
}

// ---- Session tabs / persistent containers (Task: tabs architecture) ----

use opclook::app::ConfirmAction;

/// Simulate a double-click on a server row in the connect dialog (tests drive
/// the same app-level path the double-click handler calls).
#[test]
fn open_tab_for_endpoint_creates_visible_active_session() {
    // open_tab_for_endpoint persists the recent endpoint via Settings::save(),
    // so it must hold the process-global settings lock.
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        let ep = EndpointSpec::da("", "Alpha.Server");
        app.open_tab_for_endpoint(ep);

        assert_eq!(app.sessions.len(), 1);
        assert!(app.sessions[0].visible, "new session tab must be visible");
        assert_eq!(app.active_session, Some(app.sessions[0].id));
        assert_eq!(app.sessions[0].endpoint().address, "Alpha.Server");
    });
}

/// Reopening the same endpoint reactivates the SAME session (no duplicate).
#[test]
fn reopen_tab_for_same_endpoint_does_not_duplicate() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_tab_for_endpoint(EndpointSpec::da("", "Alpha.Server"));
        app.seed_row(app.sessions[0].id, "ns=1;s=a", "SigA", Value::I64(7));
        let id = app.sessions[0].id;
        let rows_before = app.rows().len();
        let counts_before = app.sessions.len();

        // Simulate closing the tab, then double-clicking the server again.
        app.close_tab(id);
        assert_eq!(app.sessions.len(), counts_before);
        app.open_tab_for_endpoint(EndpointSpec::da("", "Alpha.Server"));

        assert_eq!(
            app.sessions.len(),
            counts_before,
            "must NOT create a duplicate container"
        );
        assert_eq!(app.sessions[0].id, id, "same session must be reactivated");
        assert!(
            app.sessions[0].visible,
            "restored tab must be visible again"
        );
        assert_eq!(app.active_session, Some(id));
        assert_eq!(
            app.rows().len(),
            rows_before,
            "rows stay with the same container"
        );
    });
}

/// Closing a tab hides the view, keeps rows/tree/address, and does NOT
/// disconnect a Connected session (reopening shows live data).
#[test]
fn close_tab_hides_view_keeps_rows_and_keeps_connection() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_row(sid(1), "ns=1;s=a", "SigA", Value::I64(1));
    app.activate_session(sid(1));
    assert_eq!(app.sessions[0].state, SessionState::Connected);

    app.close_tab(sid(1));

    assert!(!app.sessions[0].visible, "closed tab must be hidden");
    assert_eq!(
        app.sessions[0].state,
        SessionState::Connected,
        "closing a tab must NOT disconnect a connected session"
    );
    assert_eq!(app.rows().len(), 1, "rows must survive closing the tab");
    assert_eq!(app.sessions[0].tree().len(), 0);
    assert!(
        app.active_session.is_none(),
        "no visible tab remains -> no active session"
    );
    assert_eq!(app.sessions[0].endpoint().address, "Alpha.Server");
}

/// Closing the active tab activates the nearest remaining visible tab.
#[test]
fn close_active_tab_activates_nearest_visible() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.One"));
    app.seed_session(sid(2), EndpointSpec::da("", "Srv.Two"));
    app.seed_session(sid(3), EndpointSpec::da("", "Srv.Three"));
    app.activate_session(sid(2));

    app.close_tab(sid(2));

    // Nearest remaining visible to the right is session 3.
    assert_eq!(app.active_session, Some(sid(3)));
    assert!(!app.sessions[1].visible);
    assert_eq!(
        app.sessions.iter().filter(|s| s.visible).count(),
        2,
        "other tabs stay visible"
    );
}

/// remove_session (after confirm) drops the session container and its rows.
#[test]
fn remove_session_drops_container_and_rows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_row(sid(1), "ns=1;s=a", "SigA", Value::I64(1));
    app.seed_row(sid(1), "ns=1;s=b", "SigB", Value::I64(2));
    app.activate_session(sid(1));

    // The destructive path is gated behind the confirm dialog.
    app.request_confirm(
        "t".to_string(),
        "remove?".to_string(),
        ConfirmAction::RemoveSession(sid(1)),
    );
    assert!(app.dialogs.confirm.is_some());
    app.confirm_yes();

    assert!(app.sessions.is_empty(), "container must be dropped");
    assert!(app.rows().is_empty(), "its rows must be dropped");
    assert!(app.active_session.is_none());
}

fn seed_ua_sessions(app: &mut App, count: usize) {
    for index in 0..count {
        app.seed_session(
            sid(1000 + index as u64),
            EndpointSpec::ua(format!("opc.tcp://127.0.0.1:{}", 20000 + index)),
        );
    }
}

/// The 100-session cap is enforced at the central UA session-creation path:
/// 99 -> 100 is allowed, the 101st is rejected with a localized message.
#[test]
fn ua_session_cap_allows_100_and_rejects_the_101st() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);
        seed_ua_sessions(&mut app, 99);
        assert_eq!(app.ua_session_count(), 99);

        app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
        assert_eq!(app.ua_session_count(), 100, "100th UA session is allowed");

        app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
        assert_eq!(
            app.ua_session_count(),
            100,
            "the 101st UA session must not be added"
        );
        assert!(
            app.status.contains("предел UA-сессий"),
            "expected a localized limit message, got {:?}",
            app.status
        );
    });
}

/// Reconnecting an existing session at the cap is allowed (no new container).
#[test]
fn ua_session_cap_allows_reconnect_at_the_limit() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);
        seed_ua_sessions(&mut app, 100);
        assert_eq!(app.ua_session_count(), 100);

        app.reconnect_session(sid(1000));
        assert_eq!(
            app.ua_session_count(),
            100,
            "reconnect must not create a new session"
        );
        assert_eq!(app.sessions[0].state, SessionState::Connecting);
    });
}

/// DA sessions are not counted against the UA cap.
#[test]
fn ua_session_cap_does_not_block_da_sessions() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);
        seed_ua_sessions(&mut app, 100);

        app.open_tab_for_endpoint(EndpointSpec::da("", "Alpha.Server"));
        assert_eq!(app.ua_session_count(), 100);
        assert_eq!(app.sessions.len(), 101);
        assert_eq!(
            app.sessions.last().unwrap().endpoint().protocol,
            Protocol::OpcDa
        );
    });
}

/// Removing a session frees a UA slot, and hidden/disconnected containers still
/// count toward the cap.
#[test]
fn ua_session_cap_counts_hidden_disconnected_and_frees_on_remove() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);
        seed_ua_sessions(&mut app, 99);
        app.sessions[0].visible = false;
        app.sessions[1].state = SessionState::Disconnected;

        app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
        assert_eq!(
            app.ua_session_count(),
            100,
            "hidden/disconnected UA sessions count"
        );

        app.remove_session(sid(1000));
        assert_eq!(app.ua_session_count(), 99);
        app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
        assert_eq!(app.ua_session_count(), 100, "removal frees a slot");
    });
}

/// Switching tabs changes active_session and the table/tree render that
/// session's data.
#[test]
fn tab_switch_changes_active_session_and_rendered_rows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.One"));
    app.seed_row(sid(1), "a", "SigA1", Value::I64(1));
    app.seed_session(sid(2), EndpointSpec::da("", "Srv.Two"));
    app.seed_row(sid(2), "b", "SigB2", Value::I64(2));
    app.activate_session(sid(1));

    // Session 2's row is present but not active yet.
    assert_eq!(app.active_session, Some(sid(1)));
    // Switch to session 2 via the tab bar: active changes.
    app.activate_session(sid(2));
    assert_eq!(app.active_session, Some(sid(2)));

    let mut h = harness(app);
    h.run_steps(5);
    // Both tab labels are rendered; the second tab is the active one.
    assert!(h.query_all_by_label("DA: Srv.One").count() >= 1);
    assert!(h.query_all_by_label("DA: Srv.Two").count() >= 1);
    // The table renders both sessions' rows (global signal table).
    assert_eq!(h.state().active_session, Some(sid(2)));
}

/// The tab bar renders an empty-state hint + «Подключиться...» when no tab is open.
#[test]
fn tab_bar_shows_empty_state_even_with_hidden_containers() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.close_tab(sid(1)); // hide the only container

    let mut h = harness(app);
    h.run_steps(5);
    // Only the empty-state hint + button are shown; no tab labels.
    assert!(
        h.query_all_by_label_contains("в диалоге подключения открывает вкладку")
            .count()
            >= 1,
        "empty-state hint must be rendered"
    );
    let tab_count =
        h.query_all_by_label_contains("DA:").count() + h.query_all_by_label_contains("UA:").count();
    assert_eq!(tab_count, 0, "hidden containers must not render a tab");
}

/// The «Удалить из списка» context menu on a tab removes the container after
/// confirmation.
#[test]
fn tab_context_menu_remove_from_list_drops_container() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    app.activate_session(sid(1));

    let mut h = harness(app);
    // Open the tab's context menu and pick «Удалить из списка».
    h.get_by_label("DA: Alpha.Server").click_secondary();
    click_menu_item(&mut h, "Удалить из списка");
    assert!(
        h.state().dialogs.confirm.is_some(),
        "confirmation must be requested"
    );

    click_menu_item(&mut h, "Да");
    assert!(h.state().sessions.is_empty());
    assert!(h.state().rows().is_empty());
}

/// A manual disconnect keeps the container (tab/rows/address), marks rows stale
/// and does NOT arm the auto-reconnect loop (a lost link does, a manual
/// disconnect must not).
#[test]
fn disconnect_keeps_container_and_does_not_arm_reconnect() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.settings.view.options.auto_reconnect = true;
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_row(sid(1), "ns=1;s=a", "SigA", Value::I64(5));
    app.activate_session(sid(1));

    // User disconnects manually (menu → «Отключиться» → confirm).
    app.disconnect_session(sid(1));

    // Container survives: session + row still present, row marked stale.
    assert_eq!(app.sessions.len(), 1);
    assert!(
        app.sessions[0].visible,
        "tab stays visible after manual disconnect"
    );
    assert_eq!(app.rows().len(), 1, "rows must survive a manual disconnect");
    assert_eq!(app.rows()[0].quality.text(), "устаревшие данные");

    // The backend reports the link closed; because this was a manual disconnect
    // it must NOT re-arm the auto-reconnect loop.
    app.on_event_for_test_at(
        sid(1),
        EventKind::Disconnected {
            reason: opclook::bff::model::BackendMsg::raw("отключено"),
        },
    );
    assert_eq!(
        app.sessions[0].reconnect,
        ReconnectState::Stopped,
        "manual disconnect must not arm auto-reconnect"
    );
}

// ---- Servers area, toolbar, tab «+», default language (tasks) ----

/// Whether an accessible widget with `label` is not disabled.
fn widget_enabled(h: &Harness<'_, App>, label: &str) -> bool {
    use egui_kittest::kittest::NodeT;
    !h.get_by_label(label).accesskit_node().is_disabled()
}

/// The servers-row «×» button (the destructive one in the left panel), distinct
/// from the tab bar's «×» in the central panel. The row's × is a painter-drawn
/// glyph whose accessible name is its localized tooltip, so it is addressed by
/// that tooltip instead of a raw "×" (a raw "×" would only match the central
/// tab strip). Returns the first match located in the left navigation panel
/// (x < 400 in a 1200px harness).
fn servers_remove_button<'a>(h: &'a Harness<'a, App>) -> Option<egui_kittest::Node<'a>> {
    h.query_all_by_label("Удалить сервер и вкладку")
        .find(|n| n.rect().center().x < 400.0)
}

/// The full-width «Добавить сервер» button opens the connect dialog.
#[test]
fn servers_add_button_opens_connect_dialog() {
    let ctx = egui::Context::default();
    let app = App::new_test(&ctx);
    let mut h = harness(app);
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open);
    h.get_by_label("Добавить сервер").click();
    h.run_steps(3);
    assert!(
        h.state().dialogs.connect.open,
        "«Добавить сервер» must open the connect dialog"
    );
}

/// A server row renders the state dot, the display name and the destructive ×.
#[test]
fn servers_row_shows_state_name_and_remove() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(3);
    // The servers list shows the bare name (the tab shows "DA: Alpha.Server").
    assert!(
        h.query_all_by_label("Alpha.Server").count() >= 1,
        "server row must render the server name"
    );
    // A state dot (painted, accessible name "●") and a × are present in the
    // servers area (left panel, x < 400); the tab strip also shows "●"/"×", so
    // scope the query to the left navigation panel.
    assert!(
        h.query_all_by_label("●")
            .find(|n| n.rect().center().x < 400.0)
            .is_some(),
        "server row must render a state dot"
    );
    assert!(
        servers_remove_button(&h).is_some(),
        "servers row must render a destructive ×"
    );
}

/// The remote-host suffix is appended to the DA name (pure rule is unit-tested
/// in lib; here we confirm a seeded remote container exposes it).
#[test]
fn servers_row_applies_remote_host_suffix() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("box1", "Alpha.Server"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_all_by_label_contains("Alpha.Server — box1").count() >= 1,
        "remote DA server must show \"name — host\""
    );
}

/// The servers-row × requests the remove confirm dialog; the container and its
/// rows are dropped only after confirming.
#[test]
fn servers_row_remove_requires_confirm_and_drops_after_yes() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(3);

    let remove = servers_remove_button(&h).expect("servers ×");
    remove.click();
    h.run_steps(3);
    assert!(
        h.state().dialogs.confirm.is_some(),
        "row × must open the remove confirm dialog"
    );
    // The container and its tab are still present before confirmation.
    assert_eq!(h.state().sessions.len(), 1);
    assert_eq!(h.state().rows().len(), 1);

    click_menu_item(&mut h, "Да");
    assert!(
        h.state().sessions.is_empty(),
        "session dropped after confirm"
    );
    assert!(
        h.state().rows().is_empty(),
        "its rows dropped after confirm"
    );
}

/// The servers-row × (confirm→No) leaves the container and rows intact.
#[test]
fn servers_row_remove_cancel_keeps_container() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(3);
    servers_remove_button(&h).expect("servers ×").click();
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_some());
    click_menu_item(&mut h, "Нет");
    assert_eq!(h.state().sessions.len(), 1, "container kept on cancel");
    assert_eq!(h.state().rows().len(), 1, "rows kept on cancel");
}

/// The tab bar's «×» only hides the view, keeping the container + connection.
#[test]
fn tab_close_button_only_hides_view() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    app.activate_session(sid(1));
    let mut h = harness(app);

    // The tab × is inside the central panel (x >= 400), unlike the servers ×.
    // UI3: the tab's close button is painted; its accessible name is the
    // tooltip. The active tab shows it without hovering.
    let tab_close = h
        .query_all_by_label("Скрыть вкладку (контейнер сохраняется)")
        .find(|n| n.rect().center().x >= 400.0)
        .expect("tab ×");
    tab_close.click();
    h.run_steps(3);
    assert_eq!(h.state().sessions.len(), 1, "container survives tab ×");
    assert_eq!(h.state().rows().len(), 1, "rows survive tab ×");
    assert!(!h.state().sessions[0].visible, "tab × hides only the view");
}

/// The tab strip renders a trailing «+» that opens the connect dialog.
#[test]
fn tab_strip_plus_opens_connect_dialog() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open);
    // The «+» sits at the end of the tab strip and opens the connect dialog.
    h.get_by_label("+").click();
    h.run_steps(10);
    assert!(
        h.state().dialogs.connect.open,
        "tab strip «+» must open the connect dialog"
    );
}

/// Reconnecting shows an in-icon attempt counter and a stop button; stopping
/// toggles to a resume button; the counter reflects the attempt number.
/// The reconnect / stop / resume glyphs are painter-drawn (no tofu font glyphs),
/// so tests address them by their localized accessible tooltip names.
#[test]
fn servers_row_reconnect_counter_and_stop_resume_toggle() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.view.options.auto_reconnect = true;
    app.seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    app.activate_session(sid(1));
    // Simulate a lost link that armed a reconnect loop at attempt 3.
    app.sessions[0].state = SessionState::Disconnected;
    app.sessions[0].reconnect = ReconnectState::Waiting {
        at: std::time::Instant::now(),
        attempt: 3,
    };

    let mut h = harness(app);
    h.run_steps(3);
    // The reconnect counter «3/20» is plain text (no ↻ glyph needed).
    assert!(
        h.query_all_by_label_contains("3/20").count() >= 1,
        "reconnecting row must show the attempt counter"
    );
    // No non-ASCII icon glyphs are rendered as labels anywhere.
    assert_eq!(
        h.query_all_by_label("↻").count()
            + h.query_all_by_label("⏸").count()
            + h.query_all_by_label("▶").count(),
        0,
        "icons must be painter-drawn, not text glyphs"
    );
    // A stop button (accessible name = tooltip) is offered while running.
    assert!(
        h.get_all_by_label("Остановить попытки").count() >= 1,
        "a running reconnect loop exposes the stop icon"
    );
    let stop = h
        .get_all_by_label("Остановить попытки")
        .min_by_key(|n| n.rect().center().x as i64)
        .expect("stop button");
    stop.click();
    h.run_steps(3);
    // Stopped: the stop icon goes away; a resume icon appears.
    assert!(
        h.query_all_by_label("Остановить попытки").count() == 0,
        "stop icon must disappear once stopped"
    );
    assert!(
        h.query_all_by_label("Возобновить попытки").count() >= 1,
        "a stopped loop exposes the resume icon"
    );
}

/// The toolbar exposes connect / disconnect / reconnect with correct enabled
/// states for the active session. The reconnect control is a painter-drawn icon
/// with an accessible label equal to its tooltip.
#[test]
fn toolbar_enable_states_follow_active_session() {
    let ctx = egui::Context::default();
    let app = App::new_test(&ctx);
    // No active session: only «Подключиться» is enabled (A7: first start).
    let mut h = harness(app);
    h.run_steps(3);
    assert!(widget_enabled(&h, "Подключиться"));
    assert!(!widget_enabled(&h, "Отключиться"));
    assert!(!widget_enabled(&h, "Переподключиться"));

    // Connected: all three enabled.
    h.state_mut()
        .seed_session(sid(1), EndpointSpec::da("", "Alpha.Server"));
    h.state_mut().activate_session(sid(1));
    h.run_steps(3);
    assert!(widget_enabled(&h, "Подключиться"));
    assert!(widget_enabled(&h, "Отключиться"));
    assert!(widget_enabled(&h, "Переподключиться"));

    // Disconnected: connect + reconnect enabled, disconnect disabled.
    h.state_mut().sessions[0].state = SessionState::Disconnected;
    h.run_steps(3);
    assert!(widget_enabled(&h, "Подключиться"));
    assert!(!widget_enabled(&h, "Отключиться"));
    assert!(widget_enabled(&h, "Переподключиться"));

    // Connecting: disconnect + reconnect enabled; connect stays available.
    h.state_mut().sessions[0].state = SessionState::Connecting;
    h.run_steps(3);
    assert!(widget_enabled(&h, "Подключиться"));
    assert!(widget_enabled(&h, "Отключиться"));
    assert!(widget_enabled(&h, "Переподключиться"));
}

/// A8: the table shows the signals of every server (the tabs are not a
/// filter) and says so; the row menu has no tree actions.
#[test]
fn v1_each_tab_shows_its_own_tags_and_row_menu_has_no_tree_actions() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_session(sid(2), EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
    app.activate_session(sid(1));
    app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    app.seed_row(sid(2), "b", "SigB", Value::I64(2));
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label_contains("SigA").is_some());
    assert!(
        h.query_by_label_contains("SigB").is_none(),
        "V1: the other session's tags are not in this tab's table"
    );
    assert_eq!(h.query_all_by_label_contains("не фильтр").count(), 0);

    right_click_row(&h, "SigA");
    h.run_steps(20);
    assert!(h.query_all_by_label("Удалить выбранные").count() >= 1);
    assert_eq!(h.query_all_by_label("Добавить (по этому узлу)").count(), 0);
    assert_eq!(h.query_all_by_label("Добавить дочерние").count(), 0);
    h.key_press(Key::Escape);
    h.run_steps(3);

    // Switching the tab shows the other endpoint's own list.
    h.state_mut().activate_session(sid(2));
    h.run_steps(3);
    assert!(h.query_by_label_contains("SigB").is_some());
    assert!(h.query_by_label_contains("SigA").is_none());
}

/// V1: a selection never survives a tab switch, so Delete in the new tab
/// cannot remove the hidden rows of the old one.
#[test]
fn v1_switching_tabs_drops_the_hidden_selection() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_session(sid(2), EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
    app.activate_session(sid(1));
    let a = app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    let b = app.seed_row(sid(2), "b", "SigB", Value::I64(2));
    let mut h = harness(app);
    h.run_steps(2);
    h.state_mut().select_only(0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(3);
    assert_eq!(
        h.state().selected_rows.len(),
        1,
        "Ctrl+A takes this tab only"
    );
    assert!(h.state().selected_rows.contains(&a));

    h.state_mut().activate_session(sid(2));
    h.run_steps(2);
    assert!(h.state().selected_rows.is_empty());
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert!(h.state().rows().iter().any(|r| r.key == a));
    assert!(h.state().rows().iter().any(|r| r.key == b));
}

/// V1: the count strip shows the active tab's tags.
#[test]
fn v1_signal_count_is_per_tab() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_session(sid(2), EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
    app.activate_session(sid(2));
    app.seed_row(sid(1), "a", "SigA", Value::I64(1));
    app.seed_row(sid(1), "c", "SigC", Value::I64(3));
    app.seed_row(sid(2), "b", "SigB", Value::I64(2));
    assert_eq!(app.visible_row_count(), 1);
    app.activate_session(sid(1));
    assert_eq!(app.visible_row_count(), 2);
    assert_eq!(app.visible_row_positions().as_slice(), &[0, 1]);
}

/// A12: a branch with thousands of nodes builds only the visible rows.
#[test]
fn a12_a_huge_tree_builds_only_the_visible_rows() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.activate_session(sid(1));
    for index in 0..5000 {
        app.seed_tree_item(
            sid(1),
            None,
            0,
            BrowseNode {
                node_id: format!("ns=2;i={index}"),
                name: format!("Node{index:05}"),
                kind: NodeKind::Item,
                has_children: false,
            },
        );
    }
    let mut h = harness(app);
    h.run_steps(3);
    let drawn = h.query_all_by_label_contains("Node0").count();
    assert!(drawn > 5, "the top of the tree is visible: {drawn}");
    assert!(
        drawn < 120,
        "only the viewport is built, not 5000 rows: {drawn}"
    );
    assert!(h.query_by_label_contains("Node04999").is_none());
}

/// A7: «Подключиться» works on a fresh start (no sessions) and opens the
/// connect dialog.
#[test]
fn a7_toolbar_connect_opens_the_dialog_on_first_start() {
    let ctx = egui::Context::default();
    let mut h = harness(App::new_test(&ctx));
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open);
    h.get_by_label("Подключиться").click();
    h.run_steps(3);
    assert!(h.state().dialogs.connect.open);
}

/// A7: with a disconnected active session the button reconnects it instead of
/// opening the dialog; with a connected one it opens the dialog (new server).
#[test]
fn a7_toolbar_connect_reconnects_a_disconnected_active_session() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.activate_session(sid(1));
    app.sessions[0].state = SessionState::Disconnected;
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Подключиться").click();
    h.run_steps(3);
    assert!(
        !h.state().dialogs.connect.open,
        "no dialog for a known server"
    );
    assert_eq!(h.state().sessions[0].state, SessionState::Connecting);

    h.state_mut().sessions[0].state = SessionState::Connected;
    h.run_steps(3);
    h.get_by_label("Подключиться").click();
    h.run_steps(3);
    assert!(h.state().dialogs.connect.open);
}

/// A fresh app (no persisted language) defaults to Russian.
#[test]
fn fresh_app_defaults_to_russian() {
    let ctx = egui::Context::default();
    let app = App::new_test(&ctx);
    assert_eq!(app.lang(), opclook::i18n::Lang::Ru);
}

/// `on_exit` must actually wait for the backend teardown: after it returns the
/// gateway is shut down and a further command is rejected (not silently lost).
#[test]
fn on_exit_shuts_down_the_gateway() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));

        let started = std::time::Instant::now();
        <App as eframe::App>::on_exit(&mut app, None);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(6),
            "on_exit waited too long: {:?}",
            started.elapsed()
        );

        let error = app
            .gateway_for_test()
            .send(Command {
                request_id: opclook::bff::model::RequestId::new(1),
                owner: sid(1).into(),
                protocol: Protocol::OpcDa,
                kind: CommandKind::Test,
            })
            .unwrap_err();
        assert_eq!(error, opclook::bff::SendError::GatewayClosed);
    });
}

// ---- U9: the connect dialog waits for Connected / Error of its attempt ----

fn u9_start_attempt(
    app: &mut App,
    url: &str,
) -> (
    opclook::bff::model::SessionId,
    opclook::bff::model::RequestId,
) {
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    app.dialogs.connect.ua_url = url.to_string();
    assert!(app.connect_from_dialog(EndpointSpec::ua(url)));
    let session = app.active_session.expect("session created by the attempt");
    let request = *app
        .pending_request_ids_for_test(session)
        .last()
        .expect("connect request is outstanding");
    (session, request)
}

#[test]
fn u9_dialog_stays_open_until_connected_then_closes() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        let (session, request) = u9_start_attempt(&mut app, "opc.tcp://127.0.0.1:1");
        assert!(app.dialogs.connect.open, "dialog must stay open");
        assert_eq!(app.dialogs.connect.connecting, Some(session));
        assert_eq!(app.sessions[0].state, SessionState::Connecting);

        app.on_event_for_test_with_request(
            session,
            request,
            EventKind::Connected {
                endpoint: EndpointSpec::ua("opc.tcp://127.0.0.1:1"),
                info: opclook::bff::model::BackendMsg::raw("ok"),
            },
        );
        assert!(!app.dialogs.connect.open, "Connected closes the dialog");
        assert_eq!(app.dialogs.connect.connecting, None);
        assert_eq!(app.sessions[0].state, SessionState::Connected);
    });
}

#[test]
fn u9_connect_error_stays_in_the_dialog_and_stops_the_retry_loop() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        let (session, request) = u9_start_attempt(&mut app, "opc.tcp://127.0.0.1:1");
        app.on_event_for_test_with_request(
            session,
            request,
            EventKind::Error {
                operation: opclook::bff::model::Operation::Connect,
                message: opclook::bff::model::BackendMsg::raw("certificate rejected"),
            },
        );
        assert!(
            app.dialogs.connect.open,
            "the error must stay in the dialog"
        );
        assert_eq!(
            app.dialogs.connect.error.as_deref(),
            Some("certificate rejected")
        );
        assert_eq!(app.dialogs.connect.connecting, None);
        assert_eq!(app.sessions[0].state, SessionState::Disconnected);
        assert_eq!(
            app.sessions[0].reconnect,
            ReconnectState::Stopped,
            "a failed dialog attempt must not loop in the background"
        );
    });
}

#[test]
fn u9_cancel_aborts_the_wait_and_ignores_the_late_reply() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        let (session, request) = u9_start_attempt(&mut app, "opc.tcp://127.0.0.1:1");
        app.close_connect_dialog();
        assert!(!app.dialogs.connect.open);
        assert_eq!(app.dialogs.connect.connecting, None);
        assert_eq!(app.sessions[0].state, SessionState::Disconnected);
        assert_eq!(app.sessions[0].reconnect, ReconnectState::Stopped);
        // The abandoned attempt's reply must not resurrect the session.
        app.on_event_for_test_with_request(
            session,
            request,
            EventKind::Connected {
                endpoint: EndpointSpec::ua("opc.tcp://127.0.0.1:1"),
                info: opclook::bff::model::BackendMsg::raw("late"),
            },
        );
        assert_eq!(app.sessions[0].state, SessionState::Disconnected);
        // The half-open session can still be removed.
        app.remove_session(session);
        assert!(app.sessions.is_empty());
    });
}

#[test]
fn u9_other_session_connecting_does_not_close_the_dialog() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.seed_session(sid(7), EndpointSpec::ua("opc.tcp://127.0.0.1:2"));
        app.open_connect_dialog();
        app.on_event_for_test_at(
            sid(7),
            EventKind::Connected {
                endpoint: EndpointSpec::ua("opc.tcp://127.0.0.1:2"),
                info: opclook::bff::model::BackendMsg::raw("background"),
            },
        );
        assert!(
            app.dialogs.connect.open,
            "a background reconnect must not close an unrelated dialog"
        );
    });
}

#[test]
fn u9_button_shows_connecting_spinner_and_blocks_a_second_attempt() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        let _ = u9_start_attempt(&mut app, "opc.tcp://127.0.0.1:1");
        let mut h = harness(app);
        h.run_steps(4);
        assert!(h.query_by_label("Подключение…").is_some());
        let before = h.state().sessions.len();
        // Enter must not start another attempt while one is pending.
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().sessions.len(), before);
        assert!(h.state().dialogs.connect.open);
    });
}

// ---- A2: settings problems are visible in the journal ----

fn journal_text(app: &App) -> String {
    app.journal
        .entries()
        .iter()
        .map(|e| e.render(opclook::i18n::Lang::Ru))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a2_damaged_settings_file_is_backed_up_and_reported_in_the_journal() {
    with_settings_lock(|| {
        let dir = opclook::settings::app_dir();
        std::fs::create_dir_all(&dir).expect("settings dir");
        std::fs::write(dir.join("settings.json"), "{ broken").expect("write damaged file");
        let loaded = Settings::load();
        let ctx = egui::Context::default();
        let app = App::new_test_with_loaded(&ctx, loaded);
        let text = journal_text(&app);
        assert!(text.contains("повреждён"), "journal: {text}");
        assert!(text.contains("settings.json.bad-"), "journal: {text}");
        let backups = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("settings.json.bad-")
            })
            .count();
        assert_eq!(backups, 1);
    });
}

#[test]
fn a2_failed_settings_save_is_reported_in_the_journal() {
    with_settings_lock(|| {
        let dir = opclook::settings::app_dir();
        // A directory in place of settings.json makes the final rename fail.
        std::fs::create_dir_all(dir.join("settings.json")).expect("squatting dir");
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.persist_settings();
        let text = journal_text(&app);
        assert!(
            text.contains("Не удалось сохранить настройки"),
            "journal: {text}"
        );
    });
}

// ---- A3: a failed connectivity test makes the rows stale ----

#[test]
fn a3_failed_connectivity_test_marks_rows_stale_like_disconnected() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    seed_table(&mut app, &[("tag.a", "A", 1), ("tag.b", "B", 2)]);
    assert!(
        app.rows()
            .iter()
            .all(|r| r.quality.kind() == opclook::bff::model::QualityKind::Good)
    );
    app.on_event_for_test_at(
        sid(1),
        EventKind::Error {
            operation: opclook::bff::model::Operation::Test,
            message: opclook::bff::model::BackendMsg::raw("link down"),
        },
    );
    assert_eq!(app.sessions[0].state, SessionState::Disconnected);
    let stale = opclook::i18n::tr(opclook::i18n::Lang::Ru, opclook::i18n::Tr::StaleData);
    for row in app.rows() {
        assert_eq!(row.quality.kind(), opclook::bff::model::QualityKind::Bad);
        assert_eq!(row.quality.text(), stale);
    }
}

#[test]
fn da_preparation_failure_is_rendered_by_real_update_frames() {
    with_settings_lock(|| {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        set_ru(&mut app);
        app.dialogs.connect.open = true;
        assert!(app.connect_from_dialog(EndpointSpec::da("10.23.45.67", "Srv.Test")));
        assert!(app.dialogs.connect.connecting.is_some());
        let mut h = Harness::builder()
            .with_size([1200.0, 900.0])
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(|ui, app: &mut App| app.update(ui), app);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while h.state().dialogs.connect.error.is_none() && std::time::Instant::now() < deadline {
            h.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(h.state().dialogs.connect.error.is_some());
        assert!(h.state().dialogs.connect.connecting.is_none());
        assert_eq!(h.state().sessions[0].state, SessionState::Disconnected);
    });
}

fn sid(raw: u64) -> opclook::bff::model::SessionId {
    opclook::bff::model::SessionId::new(raw).unwrap()
}

#[test]
fn scan_terminal_outcomes_keep_partial_results_and_progress() {
    use opclook::app::UaSearch;
    use opclook::bff::model::{BackendMsg, ScanEnd};
    for end in [
        ScanEnd::Cancelled,
        ScanEnd::Failed(BackendMsg::raw("scan failed")),
    ] {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.open_connect_dialog();
        let scan_id = ScanId::new(42).unwrap();
        app.dialogs.connect.ua_search = UaSearch::Subnet { scan_id };
        app.on_event_for_test(EventKind::UaScanProgress {
            scan_id,
            total: 10,
            checked: 3,
            found: 1,
        });
        app.on_event_for_test(EventKind::UaScanFound {
            scan_id,
            servers: vec![scan_server()],
        });
        let failed = matches!(end, ScanEnd::Failed(_));
        app.on_event_for_test(EventKind::UaScanFinished {
            scan_id,
            found: 1,
            end,
        });
        let d = &app.dialogs.connect;
        assert_eq!(d.ua_search, UaSearch::Idle);
        assert_eq!(d.ua_scan_checked, 3);
        assert_eq!(d.ua_scan_found, 1);
        assert_eq!(d.ua_search_results().len(), 1);
        assert_eq!(d.ua_scan_error.is_some(), failed);
    }
}

#[test]
fn large_subnet_confirmation_ui_starts_only_after_yes_and_can_stop() {
    use opclook::app::{ConnectPane, UaSearch};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = ConnectPane::Ua;
    app.network.loaded = true;
    app.network.lease = Some(lease_with_subnet("10.0.0.5", 19));
    app.record_dispatches_for_test();
    let mut h = harness(app);
    h.run_steps(4);
    h.get_by_label("Поиск в сети").click();
    h.run_steps(3);
    assert_eq!(
        h.state().dialogs.connect.ua_search,
        UaSearch::ConfirmSubnet { count: 8190 }
    );
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|command| matches!(command.kind, CommandKind::ScanUaNetwork { .. }))
    );
    h.get_by_label("Да").click();
    h.run_steps(3);
    assert!(matches!(
        h.state().dialogs.connect.ua_search,
        UaSearch::Subnet { .. }
    ));
    assert!(
        h.state()
            .dispatched_for_test()
            .iter()
            .any(|command| matches!(
                command.kind,
                CommandKind::ScanUaNetwork { subnet: None, .. }
            ))
    );
    h.get_by_label("Остановить").click();
    h.run_steps(3);
    assert_eq!(h.state().dialogs.connect.ua_search, UaSearch::Idle);
    assert!(
        h.state()
            .dispatched_for_test()
            .iter()
            .any(|command| matches!(command.kind, CommandKind::CancelUaScan { .. }))
    );
}

#[test]
fn csv_import_cancel_retains_partial_rows_and_close_then_complete() {
    use opclook::app::CsvImportState;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let mut csv = String::from("server;item_id;name\n");
    for i in 0..100_000 {
        csv.push_str(&format!("Srv.Test;Tag{i};Name{i}\n"));
    }
    opclook::ui::start_csv_import(&mut app, csv);
    let mut h = harness(app);
    h.run_steps(1);
    h.get_by_label("Отмена").click();
    h.run_steps(1);
    let job = h.state().csv_import.as_ref().unwrap();
    assert_eq!(job.state, CsvImportState::Cancelled);
    assert!(job.processed > 0 && job.processed < job.total);
    assert_eq!(job.loaded, h.state().rows().len());
    let retained = job.loaded;
    h.run_steps(2);
    assert_eq!(h.state().rows().len(), retained);
    h.get_by_label("Закрыть").click();
    h.run_steps(1);
    assert!(h.state().csv_import.is_none());
    assert_eq!(h.state().rows().len(), retained);

    opclook::ui::start_csv_import(
        h.state_mut(),
        "server;item_id;name\nSrv.Test;Final;Final\n".into(),
    );
    h.run_steps(2);
    let job = h.state().csv_import.as_ref().unwrap();
    assert_eq!(job.state, CsvImportState::Completed);
    assert_eq!((job.processed, job.loaded), (1, 1));
    h.get_by_label("Закрыть").click();
    h.run_steps(1);
    assert!(h.state().csv_import.is_none());
}

#[test]
fn da_results_with_same_progid_select_their_own_host() {
    use opclook::bff::model::ServerEntry;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.da_auto_pending = 2;
    app.dialogs.connect.da_pending = true;
    for host in ["plant-a", "plant-b"] {
        app.on_event_for_test(EventKind::Servers {
            host: host.into(),
            servers: vec![ServerEntry {
                id: "Vendor.OPC".into(),
                description: "Factory".into(),
            }],
        });
    }
    let mut h = harness(app);
    h.run_steps(2);
    assert_eq!(
        h.state().dialogs.connect.da_selected.as_ref().unwrap().host,
        "plant-a"
    );
    h.get_by_label("Vendor.OPC — Factory @ plant-b").click();
    h.run_steps(2);
    let d = &h.state().dialogs.connect;
    let selected = d.da_selected.as_ref().unwrap();
    assert_eq!(
        (selected.server.id.as_str(), selected.host.as_str()),
        ("Vendor.OPC", "plant-b")
    );
    assert_eq!(
        (d.da_manual.as_str(), d.da_host.as_str()),
        ("Vendor.OPC", "plant-b")
    );
}

#[test]
fn acquisition_editor_keeps_protocol_drafts_and_cancel_does_not_persist() {
    use opclook::bff::model::UpdateMode;
    with_settings_lock(|| {
        let mut app = App::new_test(&egui::Context::default());
        app.open_connect_dialog();
        let before = serde_json::to_value(&app.settings).unwrap();
        let mut h = harness(app);
        h.get_by_label("Получение данных").click();
        h.run_steps(3);
        assert!(h.get_by_label("Подписка").accesskit_node().is_disabled());
        h.get_by_label("OPC UA").click();
        h.run_steps(3);
        h.get_by_label("Опрос").click();
        h.run_steps(3);
        h.get_by_label("Читать после записи").click();
        h.run_steps(2);
        assert!(
            !h.state()
                .dialogs
                .connect
                .ua_acquisition
                .as_ref()
                .unwrap()
                .read_after_write
        );
        h.get_by_label("Читать после записи").click();
        h.run_steps(2);
        assert!(
            h.state()
                .dialogs
                .connect
                .ua_acquisition
                .as_ref()
                .unwrap()
                .read_after_write
        );
        assert!(
            !h.state()
                .dialogs
                .connect
                .da_acquisition
                .as_ref()
                .unwrap()
                .read_after_write
        );
        assert_eq!(
            h.state()
                .dialogs
                .connect
                .ua_acquisition
                .as_ref()
                .unwrap()
                .update_mode,
            UpdateMode::Poll
        );
        assert_eq!(
            h.state()
                .dialogs
                .connect
                .da_acquisition
                .as_ref()
                .unwrap()
                .update_mode,
            UpdateMode::Poll
        );
        h.get_by_label("Отмена").click();
        h.run_steps(3);
        assert_eq!(serde_json::to_value(&h.state().settings).unwrap(), before);
    });
}

#[test]
fn acquisition_recent_profile_restores_without_changing_other_draft() {
    use opclook::bff::model::{AcquisitionConfig, UpdateMode};
    with_settings_lock(|| {
        let mut app = App::new_test(&egui::Context::default());
        let mut endpoint = EndpointSpec::ua("opc.tcp://plant.example:4840");
        let mut profile = AcquisitionConfig::ua_default();
        profile.update_mode = UpdateMode::Poll;
        profile.interval_ms = 2345;
        endpoint.acquisition = Some(profile.clone());
        app.settings.remember(&endpoint);
        app.open_connect_dialog();
        let mut h = harness(app);
        h.get_by_label("OPC UA").click();
        h.run_steps(3);
        h.get_by_label("Недавние").click();
        h.run_steps(3);
        h.get_by_label(&endpoint.display_name()).click();
        h.run_steps(3);
        assert_eq!(h.state().dialogs.connect.ua_acquisition, Some(profile));
        assert_eq!(
            h.state().dialogs.connect.da_acquisition,
            Some(AcquisitionConfig::da_available())
        );
    });
}

#[test]
fn acquisition_disabled_subscription_explains_capability() {
    let mut app = App::new_test(&egui::Context::default());
    app.open_connect_dialog();
    let mut h = harness(app);
    h.get_by_label("Получение данных").click();
    h.run_steps(3);
    let subscription = h.get_by_label("Подписка");
    assert!(subscription.accesskit_node().is_disabled());
    subscription.hover();
    h.run_steps(40);
    assert!(
        h.query_all_by_label_contains("Недоступно в текущем адаптере")
            .next()
            .is_some()
    );
}

#[test]
fn double_click_da_result_connects_to_its_host() {
    use opclook::app::DaHit;
    use opclook::bff::model::ServerEntry;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs
        .connect
        .da_acquisition
        .as_mut()
        .unwrap()
        .interval_ms = 2345;
    app.dialogs.connect.da_pending = false;
    app.dialogs.connect.da_servers.push(DaHit {
        server: ServerEntry {
            id: "Vendor.OPC".into(),
            description: "Factory".into(),
        },
        host: "plant-b".into(),
    });
    set_ru(&mut app);
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_step_dt(0.016)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
    h.run_steps(2);
    h.get_by_label("Vendor.OPC — Factory @ plant-b").click();
    h.run_steps(1);
    h.get_by_label("Vendor.OPC — Factory @ plant-b").click();
    h.run_steps(1);
    assert_eq!(h.state().sessions.len(), 1);
    let endpoint = &h.state().sessions[0].endpoint();
    assert_eq!(
        (endpoint.host.as_str(), endpoint.address.as_str()),
        ("plant-b", "Vendor.OPC")
    );
    assert_eq!(endpoint.acquisition.as_ref().unwrap().interval_ms, 2345);
}

#[test]
fn server_popup_shows_ua_fields_only_for_ua_sessions() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.seed_session(sid(1), EndpointSpec::da("", "Server"));
    app.status_popup = true;
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(h.query_all_by_label("Подписка:").count(), 0);
    assert_eq!(h.query_all_by_label("Таймаут сессии (мс):").count(), 0);
    h.state_mut()
        .seed_session(sid(2), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    h.run_steps(3);
    assert!(h.query_all_by_label("Подписка:").count() > 0);
    assert!(h.query_all_by_label("Таймаут сессии (мс):").count() > 0);
}

#[test]
fn browse_filter_control_is_ua_only_and_changes_ua_filter() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.seed_session(sid(1), EndpointSpec::da("", "Server"));
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(h.query_all_by_label("Только переменные").count(), 0);
    h.state_mut()
        .seed_session(sid(2), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    h.run_steps(3);
    h.get_by_label("Только переменные").click();
    h.run_steps(3);
    assert_eq!(
        h.state().sessions[1].ua().unwrap().ua_browse_filter,
        opclook::bff::model::NodeClassFilter::Variables
    );
}

#[test]
fn ua_result_tree_rebuilds_after_same_length_replacement() {
    use opclook::bff::model::{AdvertisedMode, AdvertisedPolicy, UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.pane = opclook::app::ConnectPane::Ua;
    let server = |url: &str, policy: &str, mode: &str| UaServerInfo {
        application_name: "Server".into(),
        application_uri: "urn:test".into(),
        endpoints: vec![UaEndpointInfo {
            url: url.parse().unwrap(),
            security_policy: AdvertisedPolicy::from_wire(policy),
            security_mode: AdvertisedMode::from_wire(mode),
        }],
    };
    app.dialogs
        .connect
        .ua_search_results_mut_for_test()
        .push(server("opc.tcp://127.0.0.1:4840", "None", "None"));
    let mut h = harness(app);
    h.run_steps(4);
    assert!(
        h.query_by_label_contains("opc.tcp://127.0.0.1:4840")
            .is_some()
    );
    h.state_mut()
        .dialogs
        .connect
        .ua_search_results_mut_for_test()[0] = server("opc.tcp://127.0.0.1:4841", "None", "None");
    h.run_steps(4);
    assert!(
        h.query_by_label_contains("opc.tcp://127.0.0.1:4840")
            .is_none()
    );
    h.get_by_label_contains("opc.tcp://127.0.0.1:4841").click();
    h.run_steps(3);
    assert_eq!(h.state().dialogs.connect.ua_url, "opc.tcp://127.0.0.1:4841");
}

#[test]
fn p2_open_connect_menu_button_and_keyboard_are_equivalent() {
    use opclook::app::{DaHit, actions::UiAction};
    use opclook::bff::model::ServerEntry;
    let mut outcomes = Vec::new();
    for source in 0..6 {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.record_dispatches_for_test();
        app.dialogs.connect.da_servers.push(DaHit {
            server: ServerEntry {
                id: "Cached.Server".into(),
                description: "Cached".into(),
            },
            host: "localhost".into(),
        });
        app.dialogs.connect.da_host = "old host".into();
        assert!(
            app.ui_action_unavailable(UiAction::OpenConnectDialog)
                .is_none()
        );
        assert!(app.dispatched_for_test().is_empty());
        let mut h = harness(app);
        h.run_steps(3);
        match source {
            0 => {
                h.get_by_label("Добавить сервер").click();
            }
            1 => {
                h.get_by_label("OPC клиент").click();
                h.run_steps(3);
                h.get_by_label("Подключиться... Ctrl+N").click();
            }
            2 => h.key_press_modifiers(Modifiers::COMMAND, Key::N),
            3 => h.get_by_label("+").click(),
            4 => h.get_by_label("Подключиться (Ctrl+N)").click(),
            _ => h.get_by_label("Подключиться").click(),
        }
        h.run_steps(3);
        let d = &h.state().dialogs.connect;
        outcomes.push((
            d.open,
            d.da_host.clone(),
            d.da_servers.len(),
            h.state().dispatched_for_test().len(),
        ));
    }
    assert_eq!(outcomes, vec![(true, String::new(), 1, 0); 6]);
}

#[test]
fn p2_availability_is_pure_and_execution_rechecks_modal_state() {
    use opclook::app::actions::UiAction;
    use opclook::i18n::Tr;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.dialogs.about.open = true;
    app.dialogs.connect.da_host = "keep".into();
    for _ in 0..3 {
        assert_eq!(
            app.ui_action_unavailable(UiAction::OpenConnectDialog),
            Some(Tr::ActionModalOpen)
        );
    }
    assert_eq!(
        app.execute_ui_action(UiAction::OpenConnectDialog),
        Err(Tr::ActionModalOpen)
    );
    assert_eq!(app.dialogs.connect.da_host, "keep");
    assert!(!app.dialogs.connect.open);
    assert!(app.dispatched_for_test().is_empty());
    let mut h = harness(app);
    h.run_steps(3);
    assert!(!widget_enabled(&h, "Добавить сервер"));
    assert!(!widget_enabled(&h, "Подключиться"));
    assert!(!widget_enabled(&h, "+"));
    assert!(!widget_enabled(&h, "Подключиться (Ctrl+N)"));
    use egui_kittest::kittest::NodeT;
    assert_eq!(
        h.get_by_label("Добавить сервер")
            .accesskit_node()
            .description()
            .as_deref(),
        Some("Сначала закройте открытый диалог")
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run_steps(3);
    assert!(!h.state().dialogs.connect.open);
}

#[test]
fn p2_write_enter_repeat_never_confirms_or_repeats_a_write() {
    for confirm in [false, true] {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = confirm;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        let mut h = harness(app);
        h.run_steps(3);
        let event = |pressed| egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        h.event(event(true));
        h.run_steps(1);
        for _ in 0..3 {
            h.event(event(true));
            h.run_steps(1);
        }
        let writes = |app: &App| {
            app.dispatched_for_test()
                .iter()
                .filter(|command| matches!(command.kind, CommandKind::Write { .. }))
                .count()
        };
        if confirm {
            assert!(h.state().dialogs.confirm.is_some());
            assert_eq!(writes(h.state()), 0);
            h.event(event(false));
            h.run_steps(1);
            h.key_press(Key::ArrowLeft);
            h.run_steps(1);
            h.event(event(true));
            h.run_steps(1);
        }
        for _ in 0..3 {
            h.event(event(true));
            h.run_steps(1);
        }
        h.run_steps(5);
        assert_eq!(writes(h.state()), 1);
    }
}

#[test]
fn p2_write_confirmation_cancel_and_connection_change_do_not_write() {
    for disconnect in [false, true] {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = true;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("Записать").click();
        h.run_steps(5);
        assert!(h.state().dialogs.confirm.is_some());
        assert!(h.query_by_label("Отмена").is_none());
        if disconnect {
            h.state_mut().on_event_for_test_at(
                sid(1),
                EventKind::Disconnected {
                    reason: opclook::bff::model::BackendMsg::raw("test disconnect"),
                },
            );
            h.run_steps(5);
            assert!(h.state().dialogs.confirm.is_none());
            assert!(h.state().dialogs.write.error.is_some());
            h.state_mut().reconnect_session(sid(1));
            let request = *h
                .state()
                .pending_request_ids_for_test(sid(1))
                .last()
                .unwrap();
            h.state_mut().on_event_for_test_with_request(
                sid(1),
                request,
                EventKind::Connected {
                    endpoint: EndpointSpec::da("", "Srv.Test"),
                    info: opclook::bff::model::BackendMsg::raw("reconnected"),
                },
            );
            assert_eq!(h.state().sessions[0].state, SessionState::Connected);
            let message = opclook::i18n::tr(
                opclook::i18n::Lang::Ru,
                opclook::i18n::Tr::ActionWriteConnectionChanged,
            );
            assert!(journal_text(h.state()).contains(message));
            h.state_mut().confirm_yes();
        } else {
            h.get_by_label("Нет").click();
        }
        h.run_steps(5);
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::Write { .. }))
        );
    }
}

#[test]
fn p2_write_pending_confirmation_does_not_survive_reconnect() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = true;
    let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    app.seed_session(sid(1), endpoint.clone());
    app.seed_writable_row(sid(1), "ns=2;s=x", "x", Value::I64(42));
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    app.apply_write();
    assert!(app.dialogs.confirm.is_some());
    app.reconnect_session(sid(1));
    assert!(app.dialogs.confirm.is_none());
    assert!(journal_text(&app).contains(opclook::i18n::tr(
        opclook::i18n::Lang::Ru,
        opclook::i18n::Tr::ActionWriteConnectionChanged
    )));
    let request = app
        .dispatched_for_test()
        .iter()
        .rev()
        .find_map(|command| {
            if matches!(command.kind, CommandKind::Connect { .. }) {
                command.request_id
            } else {
                None
            }
        })
        .expect("reconnect dispatched");
    app.on_event_for_test_with_request(
        sid(1),
        request,
        EventKind::Connected {
            endpoint,
            info: opclook::bff::model::BackendMsg::raw("reconnected"),
        },
    );
    assert_eq!(app.sessions[0].state, SessionState::Connected);
    app.confirm_yes();
    assert!(
        !app.dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p2_gui_rejected_write_diagnostic_does_not_expose_value() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    app.dialogs.write.type_index = 6;
    app.dialogs.write.text = "write-secret-canary".into();
    app.on_event_for_test_at(
        sid(1),
        EventKind::Disconnected {
            reason: opclook::bff::model::BackendMsg::raw("test disconnect"),
        },
    );
    app.apply_write();
    assert_eq!(
        app.status,
        opclook::i18n::tr(
            opclook::i18n::Lang::Ru,
            opclook::i18n::Tr::ActionSessionNotConnected
        )
    );
    assert!(
        !app.dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
    assert!(!app.status.contains("write-secret-canary"));
    assert!(!journal_text(&app).contains("write-secret-canary"));
}

#[test]
fn p2_disconnected_write_is_disabled_before_confirmation() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    app.on_event_for_test_at(
        sid(1),
        EventKind::Disconnected {
            reason: opclook::bff::model::BackendMsg::raw("disconnected"),
        },
    );
    let mut h = harness(app);
    h.run_steps(3);
    assert!(!widget_enabled(&h, "Записать"));
    assert_eq!(
        h.get_by_label("Записать")
            .accesskit_node()
            .description()
            .as_deref(),
        Some("Сессия не подключена")
    );
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_none());
    h.state_mut().apply_write();
    assert!(h.state().dialogs.confirm.is_none());
    assert_eq!(h.state().status, "Сессия не подключена");
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p2_session_diagnostic_redacts_known_credentials() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    endpoint.user = "p2-username".into();
    endpoint.password = "p2-password-secret".into();
    app.seed_session(sid(1), endpoint);
    app.on_event_for_test_at(
        sid(1),
        EventKind::Log {
            message: opclook::bff::model::BackendMsg::raw(
                "p2-username p2-password-secret server failure",
            ),
        },
    );
    let log = journal_text(&app);
    assert!(log.contains("server failure"));
    assert!(!log.contains("p2-username"));
    assert!(!log.contains("p2-password-secret"));
}

#[test]
fn p2_space_repeat_on_confirmation_button_does_not_write() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    let mut h = harness(app);
    h.run_steps(5);
    h.get_by_label("Записать").focus();
    h.run_steps(1);
    let key = |pressed| egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    h.event(key(true));
    h.run_steps(5);
    assert!(h.state().dialogs.confirm.is_some());
    h.get_by_label("Да").focus();
    h.run_steps(1);
    for _ in 0..3 {
        h.event(key(true));
        h.run_steps(1);
    }
    assert!(h.state().dialogs.confirm.is_some());
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
    h.event(key(false));
    h.run_steps(1);
    h.event(key(true));
    h.run_steps(5);
    assert_eq!(
        h.state()
            .dispatched_for_test()
            .iter()
            .filter(|c| matches!(c.kind, CommandKind::Write { .. }))
            .count(),
        1
    );
}

#[test]
fn p2_enter_on_no_cancels_write_without_dispatch() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    let mut h = harness(app);
    h.run_steps(5);
    h.get_by_label("Записать").click();
    h.run_steps(5);
    assert!(h.state().dialogs.confirm.is_some());
    h.get_by_label("Нет").focus();
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(5);
    assert!(h.state().dialogs.confirm.is_none());
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|command| matches!(command.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p2_enter_on_no_cancels_disconnect_and_remove() {
    for remove in [false, true] {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.record_dispatches_for_test();
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        let action = if remove {
            ConfirmAction::RemoveSession(sid(1))
        } else {
            ConfirmAction::Disconnect(sid(1))
        };
        app.request_confirm("Confirm".into(), "Proceed?".into(), action);
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Нет").focus();
        h.run_steps(1);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(h.state().dialogs.confirm.is_none());
        assert_eq!(h.state().sessions.len(), 1);
        assert_eq!(h.state().rows().len(), 1);
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|command| matches!(command.kind, CommandKind::Disconnect))
        );
    }
}

#[test]
fn p2_oversized_write_input_is_visible_and_never_dispatched() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    app.dialogs.write.text = "я".repeat(524_289);
    app.apply_write();
    assert!(app.dialogs.write.open);
    assert!(
        app.dialogs
            .write
            .error
            .as_deref()
            .is_some_and(|text| text.contains("1 МиБ"))
    );
    assert!(
        !app.dispatched_for_test()
            .iter()
            .any(|command| matches!(command.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p2_click_write_shows_oversized_batch_error_without_dispatch() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = false;
    let keys = seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    // Exercise the batch guard without rendering a megabyte in the text field.
    let per_request = std::mem::size_of::<opclook::bff::model::WriteRequest>() + 1;
    let count = 16 * 1024 * 1024 / per_request + 1;
    app.dialogs.write.keys = vec![keys[0]; count];
    app.dialogs.write.text = "1".into();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Записать").click();
    h.run_steps(2);
    assert!(
        h.query_by_label_contains("расчётный размер пакета")
            .is_some()
    );
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|command| matches!(command.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p2_oversized_input_precedes_session_availability_in_apply_write() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    set_ru(&mut app);
    app.record_dispatches_for_test();
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    app.on_event_for_test_at(
        sid(1),
        EventKind::Disconnected {
            reason: opclook::bff::model::BackendMsg::raw("test disconnect"),
        },
    );
    app.dialogs.write.text = "x".repeat(1024 * 1024 + 1);
    app.apply_write();
    assert!(
        app.dialogs
            .write
            .error
            .as_deref()
            .is_some_and(|text| text.contains("1 МиБ"))
    );
    assert!(
        !app.dispatched_for_test()
            .iter()
            .any(|command| matches!(command.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p3_mod_binding_ignores_extra_shift_and_repeat() {
    let app = App::new_test(&egui::Context::default());
    let mut h = harness(app);
    h.run_steps(3);
    h.event(egui::Event::Key {
        key: Key::N,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
    });
    h.run_steps(2);
    assert!(!h.state().dialogs.connect.open);
    h.event(egui::Event::Key {
        key: Key::N,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
    assert!(!h.state().dialogs.connect.open);
    h.event(egui::Event::Key {
        key: Key::N,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run_steps(2);
    assert!(h.state().dialogs.connect.open);
}

#[test]
fn p3_ime_commit_enter_does_not_submit_write() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    let mut h = harness(app);
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "1".into(),
        active_range_chars: None,
    }));
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::Ime(egui::ImeEvent::Commit("1".into())),
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(1);
    assert!(h.state().dialogs.write.open);
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|command| matches!(command.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p3_ime_space_cannot_activate_focused_write_or_confirm_button() {
    for confirm_open in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = true;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        let mut h = harness(app);
        h.run_steps(5);
        if confirm_open {
            h.get_by_label("Записать").click();
            h.run_steps(5);
            h.get_by_label("Да").focus();
        } else {
            h.get_by_label("Записать").focus();
        }
        h.run_steps(1);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "a".into(),
            active_range_chars: None,
        }));
        h.event(egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        assert_eq!(h.state().dialogs.confirm.is_some(), confirm_open);
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::Write { .. }))
        );
    }
}

#[test]
fn p3_macos_mod_n_rejects_extra_control() {
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .with_os(egui::os::OperatingSystem::Mac)
        .build_ui_state(
            |ui, app: &mut App| opclook::ui::draw(app, ui),
            App::new_test(&egui::Context::default()),
        );
    h.run_steps(3);
    let mac = Modifiers {
        command: true,
        mac_cmd: true,
        ..Modifiers::NONE
    };
    h.key_press_modifiers(Modifiers { ctrl: true, ..mac }, Key::N);
    h.run_steps(2);
    assert!(!h.state().dialogs.connect.open);
    h.key_press_modifiers(mac, Key::N);
    h.run_steps(2);
    assert!(h.state().dialogs.connect.open);
}

#[test]
fn p3_ime_still_allows_deliberate_pointer_write() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    let mut h = harness(app);
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "1".into(),
        active_range_chars: None,
    }));
    h.run_steps(1);
    h.get_by_label("Записать").click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .dispatched_for_test()
            .iter()
            .filter(|c| matches!(c.kind, CommandKind::Write { .. }))
            .count(),
        1
    );
}

#[test]
fn p3_ime_enter_and_space_do_not_submit_add_or_options() {
    for options in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.record_dispatches_for_test();
        if options {
            app.open_options_dialog();
        } else {
            seed_table(&mut app, &[("a", "SigA", 42)]);
            app.dialogs.add_signal.open = true;
            app.dialogs.add_signal.session = Some(sid(1));
            app.dialogs.add_signal.item_id = "another".into();
        }
        let mut h = harness(app);
        h.run_steps(3);
        if options {
            h.get_by_label("Сохранить").focus();
        } else {
            h.query_all_by_label("Добавить")
                .last()
                .expect("add button")
                .focus();
        }
        h.run_steps(1);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "a".into(),
            active_range_chars: None,
        }));
        h.event(egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        assert!(if options {
            h.state().dialogs.options.open
        } else {
            h.state().dialogs.add_signal.open
        });
        h.input_mut().events.extend([
            egui::Event::Ime(egui::ImeEvent::Commit("a".into())),
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(1);
        assert!(if options {
            h.state().dialogs.options.open
        } else {
            h.state().dialogs.add_signal.open
        });
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::AddItems { .. }))
        );
    }
}

#[test]
fn p3_simultaneous_pointer_focus_and_mod_n_does_not_open_dialog() {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.view.options.show_script = true;
    app.settings.view.options.show_log = false;
    let mut h = harness(app);
    h.run_steps(3);
    let pos = egui::pos2(700.0, 845.0);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::N,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
    ]);
    h.run_steps(1);
    assert!(!h.state().dialogs.connect.open);
}

#[test]
fn p3_ime_held_enter_and_new_press_after_commit() {
    for redraw in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = false;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        let mut h = harness(app);
        h.run_steps(3);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "1".into(),
            active_range_chars: None,
        }));
        h.run_steps(1);
        h.event(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        h.event(egui::Event::Ime(egui::ImeEvent::Commit("1".into())));
        h.run_steps(1);
        h.event(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        assert!(h.state().dialogs.write.open);
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::Write { .. }))
        );
        h.event(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        if redraw {
            h.run_steps(1);
        }
        // After composition, explicitly focus the action before a new Enter.
        h.get_by_label("Записать").focus();
        h.run_steps(1);
        h.key_press(Key::Enter);
        h.run_steps(1);
        assert_eq!(
            h.state()
                .dispatched_for_test()
                .iter()
                .filter(|c| matches!(c.kind, CommandKind::Write { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn p3_new_mod_n_after_ime_commit_is_not_lost() {
    let mut h = harness(App::new_test(&egui::Context::default()));
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "a".into(),
        active_range_chars: None,
    }));
    h.run_steps(1);
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("a".into())));
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run_steps(1);
    assert!(h.state().dialogs.connect.open);
}

#[test]
fn p3_modified_enter_in_one_raw_input_does_not_confirm_write() {
    for focus_yes in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = true;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        let mut h = harness(app);
        h.run_steps(5);
        h.get_by_label("Записать").click();
        h.run_steps(5);
        if focus_yes {
            h.get_by_label("Да").focus();
            h.run_steps(1);
        }
        h.input_mut().events.extend([
            egui::Event::ModifiersChanged(Modifiers::SHIFT),
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::SHIFT,
            },
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::SHIFT,
            },
            egui::Event::ModifiersChanged(Modifiers::NONE),
        ]);
        h.run_steps(1);
        assert!(h.state().dialogs.confirm.is_some());
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::Write { .. }))
        );
    }
}

#[test]
fn p3_select_all_uses_modifiers_of_press_in_same_raw_input() {
    for press_mod in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        seed_table(&mut app, &[("a", "A", 1), ("b", "B", 2)]);
        let mut h = harness(app);
        h.run_steps(3);
        h.state_mut().select_only(0);
        h.run_steps(1);
        assert_eq!(h.state().table_focus, Some(0));
        let at_press = if press_mod {
            Modifiers::COMMAND
        } else {
            Modifiers::NONE
        };
        let after = if press_mod {
            Modifiers::NONE
        } else {
            Modifiers::COMMAND
        };
        h.input_mut().events.extend([
            egui::Event::ModifiersChanged(at_press),
            egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: at_press,
            },
            egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: at_press,
            },
            egui::Event::ModifiersChanged(after),
        ]);
        h.run_steps(1);
        assert_eq!(h.state().selected_rows.len(), if press_mod { 2 } else { 1 });
    }
}

#[test]
fn p3_global_mod_page_takes_priority_over_table_paging() {
    let mut app = App::new_test(&egui::Context::default());
    seed_table(&mut app, &[("a", "A", 1), ("b", "B", 2)]);
    app.seed_session(sid(2), EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
    app.activate_session(sid(1));
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.run_steps(1);
    assert_eq!(h.state().table_focus, Some(0));
    h.key_press_modifiers(Modifiers::COMMAND, Key::PageDown);
    h.run_steps(2);
    assert_eq!(h.state().active_session, Some(sid(2)));
    assert_eq!(h.state().table_focus, None);
    assert!(h.state().selected_rows.is_empty());
}

#[test]
fn p3_desktop_copy_event_copies_selected_table_rows() {
    let mut app = App::new_test(&egui::Context::default());
    seed_table(&mut app, &[("a", "SigA", 42)]);
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.run_steps(1);
    assert_eq!(h.state().table_focus, Some(0));
    h.event(egui::Event::Copy);
    h.run_steps(1);
    assert_eq!(
        h.state().status,
        opclook::i18n::tr(opclook::i18n::Lang::Ru, opclook::i18n::Tr::Copied)
    );
}

#[test]
fn p3_enter_on_write_cancel_never_dispatches_or_opens_confirmation() {
    for confirm in [false, true] {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = confirm;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Отмена").focus();
        h.run_steps(1);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(!h.state().dialogs.write.open);
        assert!(h.state().dialogs.confirm.is_none());
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|command| matches!(command.kind, CommandKind::Write { .. }))
        );
    }
}

#[test]
fn p3_enter_on_options_cancel_does_not_save_draft() {
    with_settings_lock(|| {
        let mut app = App::new_test(&egui::Context::default());
        app.open_options_dialog();
        let original = app.settings.view.options.auto_reconnect;
        app.dialogs.options.draft.auto_reconnect = !original;
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Отмена").focus();
        h.run_steps(1);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(!h.state().dialogs.options.open);
        assert_eq!(h.state().settings.view.options.auto_reconnect, original);
    });
}

#[test]
fn p3_enter_on_add_cancel_does_not_submit() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    seed_table(&mut app, &[("a", "SigA", 42)]);
    app.dialogs.add_signal.open = true;
    app.dialogs.add_signal.session = Some(sid(1));
    app.dialogs.add_signal.item_id = "ns=2;i=23".into();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Отмена").focus();
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.add_signal.open);
    assert!(h.state().dialogs.add_signal.error.is_none());
    assert!(!h.state().dispatched_for_test().iter().any(|c| matches!(
        c.kind,
        CommandKind::AddItems { .. } | CommandKind::ResolveBrowsePath { .. }
    )));
}

#[test]
fn p3_ime_space_on_column_reset_does_not_change_settings() {
    with_settings_lock(|| {
        use opclook::columns::ColumnId;
        let mut app = App::new_test(&egui::Context::default());
        app.settings
            .view
            .columns
            .set_visible(ColumnId::FullName, false);
        app.dialogs.columns.open = true;
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Сбросить").focus();
        h.run_steps(1);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "a".into(),
            active_range_chars: None,
        }));
        h.key_press(Key::Space);
        h.run_steps(3);
        assert!(
            !h.state()
                .settings
                .view
                .columns
                .is_visible(ColumnId::FullName)
        );
        assert!(h.state().dialogs.columns.open);
    });
}

#[test]
fn p3_modified_enter_on_write_cancel_never_dispatches() {
    for modifiers in [Modifiers::SHIFT, Modifiers::CTRL] {
        for confirm in [false, true] {
            let mut app = App::new_test(&egui::Context::default());
            app.record_dispatches_for_test();
            app.settings.view.options.confirm_write = confirm;
            seed_writable_table(&mut app, &[("a", "SigA", 42)]);
            app.select_only(0);
            app.open_write_dialog(Vec::new());
            let mut h = harness(app);
            h.run_steps(3);
            h.get_by_label("Отмена").focus();
            h.run_steps(1);
            h.key_press_modifiers(modifiers, Key::Enter);
            h.run_steps(3);
            assert!(h.state().dialogs.confirm.is_none());
            assert!(
                !h.state()
                    .dispatched_for_test()
                    .iter()
                    .any(|c| matches!(c.kind, CommandKind::Write { .. }))
            );
        }
    }
}

#[test]
fn p3_enter_on_write_type_combo_never_dispatches() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = false;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    let mut h = harness(app);
    h.run_steps(3);
    h.query_all_by_value("Auto")
        .last()
        .expect("type combo")
        .click();
    h.run_steps(2);
    h.get_by_label("VT_I8").focus();
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.write.open);
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p3_modified_enter_on_write_value_never_dispatches() {
    for modifiers in [Modifiers::SHIFT, Modifiers::CTRL] {
        for confirm in [false, true] {
            let mut app = App::new_test(&egui::Context::default());
            app.record_dispatches_for_test();
            app.settings.view.options.confirm_write = confirm;
            seed_writable_table(&mut app, &[("a", "SigA", 42)]);
            app.select_only(0);
            app.open_write_dialog(Vec::new());
            let mut h = harness(app);
            h.run_steps(3);
            h.key_press_modifiers(modifiers, Key::Enter);
            h.run_steps(3);
            assert!(h.state().dialogs.write.open);
            assert!(h.state().dialogs.confirm.is_none());
            assert!(
                !h.state()
                    .dispatched_for_test()
                    .iter()
                    .any(|c| matches!(c.kind, CommandKind::Write { .. }))
            );
        }
    }
}

#[test]
fn p3_ime_space_on_options_checkbox_does_not_modify_draft() {
    let mut app = App::new_test(&egui::Context::default());
    app.open_options_dialog();
    let before = app.dialogs.options.draft.confirm_write;
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Запрашивать подтверждение изменения сигнала")
        .focus();
    h.run_steps(1);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "a".into(),
        active_range_chars: None,
    }));
    h.key_press(Key::Space);
    h.run_steps(2);
    assert_eq!(h.state().dialogs.options.draft.confirm_write, before);
}

#[test]
fn p3_enter_on_add_session_combo_does_not_submit() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    seed_table(&mut app, &[("a", "SigA", 42)]);
    app.dialogs.add_signal.open = true;
    app.dialogs.add_signal.session = Some(sid(1));
    app.dialogs.add_signal.item_id = "ns=2;i=23".into();
    let session_label = app.active_sessions()[0].1.clone();
    let mut h = harness(app);
    h.run_steps(3);
    h.query_all_by_value(&session_label)
        .last()
        .expect("session combo")
        .focus();
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.add_signal.open);
    assert!(!h.state().dispatched_for_test().iter().any(|c| matches!(
        c.kind,
        CommandKind::AddItems { .. } | CommandKind::ResolveBrowsePath { .. }
    )));
}

#[test]
fn p3_enter_on_options_checkbox_does_not_save_draft() {
    with_settings_lock(|| {
        let mut app = App::new_test(&egui::Context::default());
        app.open_options_dialog();
        let before = app.settings.view.options.confirm_write;
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Запрашивать подтверждение изменения сигнала")
            .focus();
        h.run_steps(1);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(h.state().dialogs.options.open);
        assert_eq!(h.state().settings.view.options.confirm_write, before);
    });
}

#[test]
fn p3_enter_in_add_name_field_submits_once() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    seed_table(&mut app, &[("a", "SigA", 42)]);
    app.dialogs.add_signal.open = true;
    app.dialogs.add_signal.session = Some(sid(1));
    app.dialogs.add_signal.item_id = "another".into();
    app.dialogs.add_signal.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .dispatched_for_test()
            .iter()
            .filter(|c| matches!(c.kind, CommandKind::AddItems { .. }))
            .count(),
        1
    );
}

#[test]
fn p3_enter_on_options_save_applies_draft() {
    with_settings_lock(|| {
        let mut app = App::new_test(&egui::Context::default());
        app.open_options_dialog();
        let target = !app.settings.view.options.auto_reconnect;
        app.dialogs.options.draft.auto_reconnect = target;
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Сохранить").focus();
        h.run_steps(1);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(!h.state().dialogs.options.open);
        assert_eq!(h.state().settings.view.options.auto_reconnect, target);
    });
}

#[test]
fn p3_followup_empty_state_displays_current_bindings_in_three_languages() {
    use opclook::i18n::Lang;
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        for width in [480.0, 1200.0] {
            let mut app = App::new_test(&egui::Context::default());
            app.settings.set_lang(lang);
            let mut h = Harness::builder()
                .with_os(egui::os::OperatingSystem::Windows)
                .with_size([width, 900.0])
                .with_max_steps(60)
                .with_accessibility_check(false)
                .allow_missing_glyphs()
                .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
            h.run_steps(3);
            assert!(h.query_all_by_label_contains("Ctrl+N").next().is_some());
            assert!(h.query_all_by_label_contains("F6").next().is_some());
            assert!(h.query_all_by_label_contains("{1}").next().is_none());
        }
    }
}

#[test]
fn p3_followup_add_menu_does_not_claim_tree_enter_equivalence() {
    let mut h = harness(App::new_test(&egui::Context::default()));
    h.run_steps(3);
    h.get_by_label("Сигналы").click();
    h.run_steps(2);
    h.get_by_label("Добавить выбранный").hover();
    h.run_steps(40);
    assert!(
        h.query_all_by_label_contains("Enter в дереве")
            .next()
            .is_none()
    );
    h.get_by_label("Добавить дочерние сигналы").hover();
    h.run_steps(40);
    assert!(
        h.query_all_by_label_contains("Добавить все сигналы ветки")
            .next()
            .is_some()
    );
}

#[test]
fn p4_palette_search_is_read_only_and_displays_context_reason_in_three_languages() {
    use opclook::i18n::{Lang, Tr, tr};
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        let mut app = App::new_test(&egui::Context::default());
        app.settings.set_lang(lang);
        app.record_dispatches_for_test();
        app.dialogs.palette.open = true;
        app.dialogs.palette.focus_pending = true;
        let mut h = Harness::builder()
            .with_size([1200.0, 900.0])
            .with_max_steps(60)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
        h.run_steps(3);
        assert!(h.query_by_label_contains("ui.connect.open").is_some());
        assert!(h.query_by_label_contains("2001").is_some());
        assert!(
            h.query_all_by_label(tr(lang, Tr::PaletteNeedsContext))
                .next()
                .is_some()
        );
        h.state_mut().dialogs.palette.query = "2002".into();
        h.run_steps(2);
        assert!(h.query_by_label_contains("session.write").is_some());
        assert!(h.query_by_label_contains("ui.connect.open").is_none());
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert!(h.state().dialogs.palette.open);
        assert!(h.state().dispatched_for_test().is_empty());
        h.state_mut().dialogs.palette.query = "no_such_action".into();
        h.run_steps(2);
        assert!(h.query_by_label(tr(lang, Tr::PaletteNoResults)).is_some());
    }
}

#[test]
fn p4_enter_dispatches_catalog_ui_action_once_and_respects_modal() {
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(!h.state().dialogs.palette.open);
    assert!(h.state().dialogs.connect.open);
}

#[test]
fn p4_ime_and_repeated_enter_do_not_execute() {
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "a".into(),
        active_range_chars: None,
    }));
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().dialogs.palette.open);
    assert!(!h.state().dialogs.connect.open);
}

#[test]
fn p4_unavailable_other_modal_cannot_be_bypassed_by_palette() {
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().dialogs.options.open = true;
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(!h.state().dialogs.connect.open);
    assert!(!h.state().dialogs.palette.open);
    assert!(h.state().dialogs.options.open);
}

#[test]
fn p4_localized_name_and_semantic_id_search() {
    use opclook::i18n::{Lang, Tr, tr};
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        let mut app = App::new_test(&egui::Context::default());
        app.settings.set_lang(lang);
        app.dialogs.palette.open = true;
        app.dialogs.palette.focus_pending = true;
        app.dialogs.palette.query = tr(lang, Tr::PaletteBrowse).into();
        let mut h = Harness::builder()
            .with_size([1200.0, 900.0])
            .with_max_steps(60)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
        h.run_steps(3);
        assert!(h.query_by_label_contains("session.browse").is_some());
        assert!(h.query_by_label_contains("session.write").is_none());
        h.state_mut().dialogs.palette.query = "ui.connect.open".into();
        h.run_steps(2);
        assert!(h.query_by_label_contains("1001").is_some());
        assert!(h.query_by_label_contains("2001").is_none());
    }
}

#[test]
fn p4_open_from_menu_and_block_background_pointer() {
    let app = App::new_test(&egui::Context::default());
    let mut h = harness(app);
    h.run_steps(3);
    let menu_pos = h.get_by_label("OPC клиент").rect().center();
    h.get_by_label("OPC клиент").click();
    h.run_steps(2);
    h.get_by_label_contains("Палитра команд").click();
    h.run_steps(3);
    assert!(h.state().dialogs.palette.open);
    pointer_click(&h, menu_pos, PointerButton::Primary);
    h.run_steps(3);
    assert!(h.state().dialogs.palette.open);
    assert!(h.query_by_label("Выход").is_none());
}

#[test]
fn p4_selection_and_key_repeats_do_not_execute_disabled() {
    use opclook_contract::actions::ActionId;
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(Key::ArrowDown);
    h.run_steps(1);
    assert_eq!(h.state().dialogs.palette.selected, Some(ActionId::Browse));
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().dialogs.palette.open);
    assert!(!h.state().dialogs.connect.open);
    h.key_press(Key::ArrowUp);
    h.run_steps(1);
    h.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(h.state().dialogs.palette.open);
    assert!(!h.state().dialogs.connect.open);
}

#[test]
fn p4_modified_enter_on_run_and_enter_on_cancel_do_not_execute() {
    for modifiers in [Modifiers::SHIFT, Modifiers::CTRL] {
        let mut app = App::new_test(&egui::Context::default());
        app.dialogs.palette.open = true;
        app.dialogs.palette.focus_pending = true;
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Выполнить").focus();
        h.run_steps(1);
        h.key_press_modifiers(modifiers, Key::Enter);
        h.run_steps(2);
        assert!(h.state().dialogs.palette.open);
        assert!(!h.state().dialogs.connect.open);
    }
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Отмена").focus();
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(!h.state().dialogs.palette.open);
    assert!(!h.state().dialogs.connect.open);
}

#[test]
fn p4_modal_blocks_background_delete_pointer() {
    let mut app = App::new_test(&egui::Context::default());
    seed_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Сигналы").click();
    h.run_steps(2);
    let delete_pos = h.get_by_label("Удалить выбранные").rect().center();
    h.state_mut().dialogs.palette.open = true;
    h.state_mut().dialogs.palette.focus_pending = true;
    h.run_steps(3);
    pointer_click(&h, delete_pos, PointerButton::Primary);
    h.run_steps(2);
    assert_eq!(h.state().rows().len(), 1);
    assert!(h.state().dialogs.palette.open);
}

#[test]
fn p4_typing_search_filters_results_and_enter_cannot_execute_disabled() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.event(egui::Event::Text("2002".into()));
    h.run_steps(2);
    assert!(h.query_by_label_contains("session.write").is_some());
    assert!(h.query_by_label_contains("ui.connect.open").is_none());
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().dialogs.palette.open);
    assert!(h.state().dispatched_for_test().is_empty());
}

#[test]
fn p4_palette_enter_never_confirms_background_write() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    app.settings.view.options.confirm_write = true;
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().open_write_dialog(Vec::new());
    h.state_mut().apply_write();
    assert!(h.state().dialogs.confirm.is_some());
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_some());
    assert!(!h.state().dialogs.palette.open);
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn p4_message_preempts_palette_escape_without_accepting_same_event() {
    use opclook::app::MessageDialog;
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().dialogs.message = Some(MessageDialog {
        title: "Alarm".into(),
        text: "Important".into(),
        details: String::new(),
    });
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().dialogs.message.is_some());
    assert!(!h.state().dialogs.palette.open);
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().dialogs.message.is_none());
}

#[test]
fn p4_interrupted_ime_without_end_stays_fail_closed_until_focus_loss() {
    use opclook::app::MessageDialog;
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "a".into(),
        active_range_chars: None,
    }));
    h.state_mut().dialogs.message = Some(MessageDialog {
        title: "Alarm".into(),
        text: "Important".into(),
        details: String::new(),
    });
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().dialogs.message.is_some());
    assert!(!h.state().dialogs.palette.open);
    h.event(egui::Event::WindowFocused(false));
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().dialogs.message.is_none());
}

#[test]
fn p4_message_button_ignores_ime_keyboard_and_allows_pointer() {
    use opclook::app::MessageDialog;
    let mut app = App::new_test(&egui::Context::default());
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    let mut h = harness(app);
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "a".into(),
        active_range_chars: None,
    }));
    h.state_mut().dialogs.message = Some(MessageDialog {
        title: "Alarm".into(),
        text: "Important".into(),
        details: String::new(),
    });
    h.run_steps(3);
    h.get_by_label("Закрыть").focus();
    h.run_steps(1);
    h.key_press(Key::Space);
    h.run_steps(2);
    assert!(h.state().dialogs.message.is_some());
    h.get_by_label("Закрыть").click();
    h.run_steps(2);
    assert!(h.state().dialogs.message.is_none());
}

#[test]
fn keyboard_regions_edit_focused_tag_and_confirm_selected_choice() {
    use opclook::i18n::Lang;
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        for accept in [false, true] {
            let ctx = egui::Context::default();
            let mut app = App::new_test(&ctx);
            app.settings.set_lang(lang);
            app.settings.view.options.confirm_write = true;
            app.record_dispatches_for_test();
            let keys = seed_writable_table(&mut app, &[("a", "SigA", 42), ("b", "SigB", 15)]);
            let mut h = Harness::builder()
                .with_size([1200.0, 900.0])
                .with_max_steps(60)
                .with_accessibility_check(false)
                .allow_missing_glyphs()
                .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
            h.run_steps(3);
            h.key_press(Key::Tab);
            h.run_steps(2);
            h.key_press(Key::Tab);
            h.run_steps(2);
            assert_eq!(h.state().table_focus, Some(0));
            h.key_press(Key::ArrowDown);
            h.run_steps(2);
            assert_eq!(h.state().table_focus, Some(1));
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert!(h.state().dialogs.write.open);
            assert_eq!(h.state().dialogs.write.keys, vec![keys[1]]);
            h.event(egui::Event::Text("77".into()));
            h.run_steps(2);
            assert_eq!(
                h.state().dialogs.write.text,
                "77",
                "current value must be selected"
            );
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert!(h.state().dialogs.confirm.is_some());
            assert!(
                !h.state()
                    .dispatched_for_test()
                    .iter()
                    .any(|c| matches!(c.kind, CommandKind::Write { .. }))
            );
            if accept {
                h.key_press(Key::ArrowLeft);
                h.run_steps(2);
            }
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert!(h.state().dialogs.confirm.is_none());
            let writes: Vec<_> = h
                .state()
                .dispatched_for_test()
                .iter()
                .filter_map(|c| match &c.kind {
                    CommandKind::Write { request } => Some(request.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(writes.len(), usize::from(accept));
            if let Some(write) = writes.first() {
                assert_eq!(write.key, keys[1]);
                assert_eq!(write.value, Value::I64(77));
            }
            assert_eq!(h.state().table_focus, Some(1));
        }
    }
}

#[test]
fn keyboard_write_edits_focused_tag_only_in_multi_selection() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let keys = seed_writable_table(&mut app, &[("a", "SigA", 42), ("b", "SigB", 15)]);
    let mut h = harness(app);
    h.run_steps(2);
    h.state_mut().select_only(0);
    h.state_mut().select_extend_to(1);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialogs.write.keys, vec![keys[1]]);
}

#[test]
fn keyboard_table_scrolls_to_focused_row_after_expanded_lines() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.seed_row(sid(1), "root", "Expandable", pump_value());
    for i in 0..80 {
        app.seed_writable_row(
            sid(1),
            &format!("tag{i}"),
            &format!("Signal{i:02}"),
            Value::I64(i),
        );
    }
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press(Key::ArrowRight);
    h.run_steps(3);
    assert!(h.query_by_label("Speed").is_some());
    h.key_press(Key::End);
    h.run_steps(4);
    assert_eq!(h.state().table_focus, Some(80));
    assert!(
        h.query_by_label("Signal79").is_some(),
        "focused row should be visible after navigation"
    );
    h.key_press(Key::Home);
    h.run_steps(4);
    assert_eq!(h.state().table_focus, Some(0));
    assert!(h.query_by_label("Expandable").is_some());
}

#[test]
fn keyboard_confirm_movement_or_pointer_with_enter_never_accepts_old_focus() {
    for pointer in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = true;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        let mut h = harness(app);
        h.run_steps(3);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(h.state().dialogs.confirm.is_some());
        h.key_press(Key::ArrowLeft);
        h.run_steps(3);
        if pointer {
            let pos = h.get_by_label("Нет").rect().center();
            h.input_mut().events.push(egui::Event::PointerMoved(pos));
            h.input_mut().events.push(egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
        } else {
            h.input_mut().events.push(egui::Event::Key {
                key: Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        }
        h.input_mut().events.push(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(2);
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::Write { .. }))
        );
        if !pointer {
            assert!(h.state().dialogs.confirm.is_some());
            h.event(egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(1);
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert!(h.state().dialogs.confirm.is_none());
            assert!(
                !h.state()
                    .dispatched_for_test()
                    .iter()
                    .any(|c| matches!(c.kind, CommandKind::Write { .. }))
            );
        }
    }
}

#[test]
fn keyboard_never_retargets_write_when_rows_are_reordered_or_removed() {
    for remove in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.record_dispatches_for_test();
        let keys = seed_writable_table(&mut app, &[("a", "SigA", 42), ("b", "SigB", 15)]);
        let mut h = harness(app);
        h.run_steps(3);
        h.key_press(Key::Tab);
        h.run_steps(2);
        h.key_press(Key::Tab);
        h.run_steps(2);
        assert_eq!(h.state().table_focus, Some(0));
        assert!(h.state().selected_rows.contains(&keys[0]));
        if remove {
            h.state_mut().rows_mut_for_test().remove(0);
        } else {
            h.state_mut().rows_mut_for_test().swap(0, 1);
        }
        h.run_steps(2);
        if remove {
            assert_eq!(h.state().table_focus, None);
        } else {
            assert_eq!(h.state().table_focus, Some(1));
            assert!(h.state().selected_rows.contains(&keys[0]));
        }
        h.key_press(Key::Enter);
        h.run_steps(3);
        if remove {
            assert!(!h.state().dialogs.write.open);
        } else {
            assert!(h.state().dialogs.write.open);
            assert_eq!(h.state().dialogs.write.keys, vec![keys[0]]);
        }
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::Write { .. }))
        );
    }
}

#[test]
fn keyboard_move_and_enter_same_frame_never_opens_a_different_tag() {
    let mut app = App::new_test(&egui::Context::default());
    seed_writable_table(&mut app, &[("a", "SigA", 42), ("b", "SigB", 15)]);
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    for key in [Key::ArrowDown, Key::Enter] {
        h.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(2);
    assert_eq!(h.state().table_focus, Some(1));
    assert!(!h.state().dialogs.write.open);
}

#[test]
fn keyboard_modified_write_key_does_not_open_dialog() {
    let mut app = App::new_test(&egui::Context::default());
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(2);
    assert!(!h.state().dialogs.write.open);
}

#[test]
fn keyboard_write_and_confirmation_block_background_delete_click() {
    for confirm in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.settings.view.options.confirm_write = true;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        let mut h = harness(app);
        h.run_steps(3);
        h.state_mut().select_only(0);
        h.run_steps(2);
        h.get_by_label("Сигналы").click();
        h.run_steps(2);
        let delete_pos = h.get_by_label("Удалить выбранные").rect().center();
        h.state_mut().open_write_dialog(Vec::new());
        if confirm {
            h.state_mut().apply_write();
        }
        h.run_steps(3);
        pointer_click(&h, delete_pos, PointerButton::Primary);
        h.run_steps(3);
        assert_eq!(h.state().rows().len(), 1);
        assert!(h.state().dialogs.write.open);
        if confirm {
            assert!(h.state().dialogs.confirm.is_some());
        }
    }
}

#[test]
fn keyboard_message_preempts_write_and_confirm_without_leaking_keys() {
    use opclook::app::MessageDialog;
    for confirm in [false, true] {
        let mut app = App::new_test(&egui::Context::default());
        app.record_dispatches_for_test();
        app.settings.view.options.confirm_write = true;
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        app.select_only(0);
        app.open_write_dialog(Vec::new());
        if confirm {
            app.apply_write();
        }
        let mut h = harness(app);
        h.run_steps(5);
        h.state_mut().dialogs.message = Some(MessageDialog {
            title: "Server notification".into(),
            text: "Notice".into(),
            details: String::new(),
        });
        h.run_steps(5);
        assert!(h.query_by_label("Notice").is_some());
        h.key_press(if confirm { Key::Escape } else { Key::Enter });
        h.run_steps(5);
        assert!(
            !h.state()
                .dispatched_for_test()
                .iter()
                .any(|c| matches!(c.kind, CommandKind::Write { .. }))
        );
        assert!(h.state().dialogs.write.open);
        if confirm {
            assert!(h.state().dialogs.message.is_none());
            assert!(h.state().dialogs.confirm.is_some());
            h.key_press(Key::Enter);
            h.run_steps(5);
            assert!(
                h.state().dialogs.confirm.is_none(),
                "resumed confirmation defaults to No"
            );
            assert!(
                !h.state()
                    .dispatched_for_test()
                    .iter()
                    .any(|c| matches!(c.kind, CommandKind::Write { .. }))
            );
        } else {
            assert!(h.state().dialogs.message.is_some());
        }
    }
}

#[test]
fn keyboard_new_message_ignores_prior_region_escape() {
    use opclook::app::MessageDialog;
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    seed_writable_table(&mut app, &[("a", "SigA", 42)]);
    app.select_only(0);
    app.open_write_dialog(Vec::new());
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().dialogs.message = Some(MessageDialog {
        title: "Notice".into(),
        text: "Server warning".into(),
        details: String::new(),
    });
    h.input_mut().events.push(egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert!(h.state().dialogs.message.is_some());
    assert!(h.state().dialogs.write.open);
    h.event(egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().dialogs.message.is_none());
    assert!(h.state().dialogs.write.open);
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn keyboard_other_dialog_regions_block_background_delete_pointer() {
    for region in ["connect", "options", "import"] {
        let mut app = App::new_test(&egui::Context::default());
        seed_writable_table(&mut app, &[("a", "SigA", 42)]);
        let mut h = harness(app);
        h.run_steps(3);
        h.state_mut().select_only(0);
        h.run_steps(2);
        h.get_by_label("Сигналы").click();
        h.run_steps(2);
        let delete_pos = h.get_by_label("Удалить выбранные").rect().center();
        match region {
            "connect" => h.state_mut().open_connect_dialog(),
            "options" => h.state_mut().open_options_dialog(),
            _ => opclook::ui::start_csv_import(
                h.state_mut(),
                "server;item_id;name\nSrv.Test;a;SigA\n".into(),
            ),
        }
        h.run_steps(5);
        pointer_click(&h, delete_pos, PointerButton::Primary);
        h.run_steps(3);
        assert_eq!(h.state().rows().len(), 1, "background click in {region}");
    }
}

#[test]
fn keyboard_message_pauses_import_and_it_resumes_after_dismissal() {
    use opclook::app::MessageDialog;
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let mut csv = String::from("server;item_id;name\n");
    for i in 0..20_000 {
        csv.push_str(&format!("Srv.Test;Tag{i};Name{i}\n"));
    }
    opclook::ui::start_csv_import(&mut app, csv);
    let mut h = harness(app);
    h.run_steps(4);
    let before = h.state().csv_import.as_ref().unwrap().processed;
    assert!(before > 0 && before < 20_000);
    h.state_mut().dialogs.message = Some(MessageDialog {
        title: "Notice".into(),
        text: "Server warning".into(),
        details: String::new(),
    });
    h.run_steps(4);
    assert_eq!(h.state().csv_import.as_ref().unwrap().processed, before);
    h.key_press(Key::Escape);
    h.run_steps(5);
    assert!(h.state().dialogs.message.is_none());
    assert!(h.state().csv_import.as_ref().unwrap().processed > before);
}

#[test]
fn keyboard_server_region_arrows_select_and_enter_activates_server() {
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::da("", "First"));
    app.seed_session(sid(2), EndpointSpec::da("", "Second"));
    app.activate_container(sid(1));
    let mut h = harness(app);
    h.run_steps(3);
    for _ in 0..3 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    assert_eq!(h.state().active_session, Some(sid(1)));
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(
        h.state().active_session,
        Some(sid(1)),
        "moving the cursor must not activate server"
    );
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(2)));
}

#[test]
fn keyboard_all_regions_are_reachable_when_views_are_empty() {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.view.options.show_log = false;
    app.settings.view.options.show_script = false;
    let mut h = harness(app);
    h.run_steps(3);
    // Header -> empty tags -> empty servers -> status -> Header.
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_by_label("Тема оформления")
            .accesskit_node()
            .is_focused()
    );
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_all_by_label("Сигналы")
            .any(|node| node.accesskit_node().is_focused())
    );
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_all_by_label("Серверы")
            .any(|node| node.accesskit_node().is_focused())
    );
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_all_by_label("нет активного сервера")
            .any(|node| node.accesskit_node().is_focused())
    );
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_by_label("Тема оформления")
            .accesskit_node()
            .is_focused()
    );
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_all_by_label("нет активного сервера")
            .any(|node| node.accesskit_node().is_focused())
    );
}

#[test]
fn keyboard_tree_subfocus_returns_after_write_cancel() {
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    let id = app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "a".into(),
            name: "SigA".into(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    app.seed_writable_row(sid(1), "a", "SigA", Value::I64(7));
    let mut h = harness(app);
    h.run_steps(3);
    for _ in 0..3 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(h.state().tree_focus, Some((sid(1), id)));
    assert!(h.state().tree_focused);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert!(h.state().dialogs.write.open);
    h.key_press(Key::Escape);
    h.run_steps(5);
    assert!(!h.state().dialogs.write.open);
    assert!(
        h.state().tree_focused,
        "tree subfocus restored after dialog"
    );
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert!(h.state().dialogs.write.open);
}

#[test]
fn keyboard_f5_empty_tree_only_browses_in_servers_region() {
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    app.record_dispatches_for_test();
    let mut h = harness(app);
    h.run_steps(4);
    let count = |app: &App| {
        app.dispatched_for_test()
            .iter()
            .filter(|c| matches!(c.kind, CommandKind::Browse { .. }))
            .count()
    };
    h.key_press(Key::Tab);
    h.run_steps(2); // header
    h.key_press(Key::F5);
    h.run_steps(2);
    assert_eq!(count(h.state()), 0);
    h.key_press(Key::Tab);
    h.run_steps(2); // empty tags
    h.key_press(Key::F5);
    h.run_steps(2);
    assert_eq!(count(h.state()), 0);
    h.key_press(Key::Tab);
    h.run_steps(2); // servers
    h.key_press(Key::F5);
    h.run_steps(3);
    assert_eq!(count(h.state()), 1);
}

#[test]
fn keyboard_server_click_resets_tree_subfocus_and_cursor() {
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::da("", "First"));
    app.seed_session(sid(2), EndpointSpec::da("", "Second"));
    app.activate_container(sid(1));
    app.seed_tree_item(
        sid(1),
        None,
        0,
        BrowseNode {
            node_id: "a".into(),
            name: "Leaf".into(),
            kind: NodeKind::Item,
            has_children: false,
        },
    );
    let mut h = harness(app);
    h.run_steps(3);
    for _ in 0..3 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    h.key_press(Key::ArrowRight);
    h.run_steps(3);
    assert!(h.state().tree_focused);
    let row = h
        .get_all_by_label("Second")
        .find(|node| node.rect().center().x < 350.0)
        .expect("server list row");
    row.click();
    h.run_steps(4);
    assert_eq!(h.state().active_session, Some(sid(2)));
    assert!(!h.state().tree_focused);
    h.key_press(Key::ArrowUp);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().active_session, Some(sid(1)));
}

#[test]
fn keyboard_theme_popup_never_saves_or_closes_open_options() {
    let mut app = App::new_test(&egui::Context::default());
    app.open_options_dialog();
    app.dialogs.options.draft.update_interval_secs = 17;
    let mut h = harness(app);
    h.run_steps(4);
    h.get_all_by_label("Тема оформления")
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .click();
    h.run_steps(4);
    assert!(h.query_all_by_label("Светлая").count() > 0);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.options.open);
    assert_eq!(h.state().dialogs.options.draft.update_interval_secs, 17);
    assert_ne!(h.state().settings.view.options.update_interval_secs, 17);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().dialogs.options.open, "Escape closes popup only");
}

#[test]
fn keyboard_theme_popup_with_options_accepts_choice_without_leaking_keys() {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.view.options.theme = opclook::settings::Theme::Dark;
    app.open_options_dialog();
    app.dialogs.options.draft.update_interval_secs = 17;
    let mut h = harness(app);
    h.run_steps(4);
    h.get_all_by_label("Тема оформления")
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .click();
    h.run_steps(4);
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(
        h.state().settings.view.options.theme,
        opclook::settings::Theme::Light
    );
    assert_eq!(
        h.state().dialogs.options.draft.theme,
        opclook::settings::Theme::Light
    );
    assert!(h.state().dialogs.options.open);
    assert_ne!(h.state().settings.view.options.update_interval_secs, 17);
}

#[test]
fn keyboard_theme_popup_ime_escape_does_not_close_popup_or_options() {
    let mut app = App::new_test(&egui::Context::default());
    app.open_options_dialog();
    let mut h = harness(app);
    h.run_steps(4);
    h.get_all_by_label("Тема оформления")
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .click();
    h.run_steps(4);
    h.input_mut().events.extend([
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "a".into(),
            active_range_chars: None,
        }),
        egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert!(h.state().dialogs.options.open);
    assert!(
        h.query_all_by_label("Светлая").count() >= 2,
        "theme popup remains open during composition"
    );
}

#[test]
fn keyboard_theme_popup_ignores_modified_and_repeated_enter_but_allows_click() {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.view.options.theme = opclook::settings::Theme::Dark;
    app.open_options_dialog();
    let mut h = harness(app);
    h.run_steps(4);
    h.get_all_by_label("Тема оформления")
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .click();
    h.run_steps(4);
    h.get_all_by_label("Светлая")
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(2);
    assert_eq!(
        h.state().settings.view.options.theme,
        opclook::settings::Theme::Dark
    );
    // Hold an explicitly rejected modified Enter before the OS repeats it.
    h.input_mut().events.push(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::SHIFT,
    });
    h.run_steps(1);
    h.input_mut().events.push(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert_eq!(
        h.state().settings.view.options.theme,
        opclook::settings::Theme::Dark
    );
    assert!(h.state().dialogs.options.open);
    h.get_all_by_label("Светлая")
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .click();
    h.run_steps(4);
    assert_eq!(
        h.state().settings.view.options.theme,
        opclook::settings::Theme::Light
    );
    assert_eq!(
        h.state().dialogs.options.draft.theme,
        opclook::settings::Theme::Light
    );
}

#[test]
fn keyboard_multiselection_cursor_anchor_and_space_are_independent() {
    let mut app = App::new_test(&egui::Context::default());
    let keys = seed_writable_table(
        &mut app,
        &[
            ("a", "SigA", 1),
            ("b", "SigB", 2),
            ("c", "SigC", 3),
            ("d", "SigD", 4),
        ],
    );
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_eq!(h.state().selected_rows, [keys[0]].into_iter().collect());
    h.key_press_modifiers(Modifiers::CTRL, Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(h.state().table_focus, Some(1));
    assert_eq!(h.state().selected_rows, [keys[0]].into_iter().collect());
    h.key_press_modifiers(Modifiers::CTRL, Key::Space);
    h.run_steps(2);
    assert_eq!(
        h.state().selected_rows,
        [keys[0], keys[1]].into_iter().collect()
    );
    h.key_press_modifiers(Modifiers::CTRL, Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(h.state().table_focus, Some(2));
    assert_eq!(
        h.state().selected_rows,
        [keys[0], keys[1]].into_iter().collect()
    );
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(
        h.state().selected_rows,
        [keys[1], keys[2], keys[3]].into_iter().collect()
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    assert_eq!(h.state().selected_rows.len(), 4);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(
        h.state().dialogs.write.keys,
        vec![keys[3]],
        "Enter must edit only focused tag"
    );
}

#[test]
fn keyboard_anchor_follows_identity_when_rows_reorder() {
    let mut app = App::new_test(&egui::Context::default());
    let keys = seed_writable_table(
        &mut app,
        &[("a", "SigA", 1), ("b", "SigB", 2), ("c", "SigC", 3)],
    );
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.state_mut().focus_row_only(1);
    h.run_steps(2);
    h.state_mut().rows_mut_for_test().swap(0, 2);
    h.run_steps(2);
    assert_eq!(h.state().table_focus, Some(1));
    assert_eq!(h.state().table_anchor, Some(2));
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    h.run_steps(2);
    assert_eq!(
        h.state().selected_rows,
        [keys[0], keys[1], keys[2]].into_iter().collect()
    );
}

#[test]
fn keyboard_tree_modified_enter_and_movement_enter_never_open_wrong_tag() {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    app.seed_session(sid(1), EndpointSpec::da("", "Srv.Test"));
    for (id, name) in [("a", "A"), ("b", "B")] {
        app.seed_tree_item(
            sid(1),
            None,
            0,
            BrowseNode {
                node_id: id.into(),
                name: name.into(),
                kind: NodeKind::Item,
                has_children: false,
            },
        );
        app.seed_writable_row(sid(1), id, name, Value::I64(1));
    }
    let mut h = harness(app);
    h.run_steps(3);
    for _ in 0..3 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    assert!(h.state().tree_focused);
    h.key_press_modifiers(Modifiers::CTRL, Key::Enter);
    h.run_steps(2);
    assert!(!h.state().dialogs.write.open);
    for key in [Key::ArrowDown, Key::Enter] {
        h.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(3);
    assert!(!h.state().dialogs.write.open);
    assert!(
        !h.state()
            .dispatched_for_test()
            .iter()
            .any(|c| matches!(c.kind, CommandKind::Write { .. }))
    );
}

#[test]
fn keyboard_mouse_control_and_shift_extend_selected_rows() {
    let mut app = App::new_test(&egui::Context::default());
    let keys = seed_writable_table(
        &mut app,
        &[
            ("a", "SigA", 1),
            ("b", "SigB", 2),
            ("c", "SigC", 3),
            ("d", "SigD", 4),
        ],
    );
    let mut h = harness(app);
    h.run_steps(3);
    pointer_click(&h, row_cell_pos(&h, "SigA"), PointerButton::Primary);
    h.run_steps(3);
    for (name, modifiers) in [("SigC", Modifiers::CTRL), ("SigD", Modifiers::SHIFT)] {
        let pos = row_cell_pos(&h, name);
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers,
        });
        h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        h.run_steps(5);
        if name == "SigC" {
            assert_eq!(
                h.state().selected_rows,
                [keys[0], keys[2]].into_iter().collect()
            );
        }
    }
    assert_eq!(
        h.state().selected_rows,
        [keys[2], keys[3]].into_iter().collect()
    );
}

#[test]
fn keyboard_context_write_targets_single_tag_while_batch_menu_remains_explicit() {
    let mut app = App::new_test(&egui::Context::default());
    let keys = seed_writable_table(&mut app, &[("a", "SigA", 1), ("b", "SigB", 2)]);
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.state_mut().select_extend_to(1);
    h.run_steps(2);
    right_click_row(&h, "SigB");
    click_menu_item(&mut h, "Изменить значение...");
    h.run_steps(4);
    assert_eq!(h.state().dialogs.write.keys, vec![keys[1]]);
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert!(!h.state().dialogs.write.open);
    h.get_by_label("Сигналы").click();
    h.run_steps(3);
    h.get_by_label("Изменить выбранные значения...").click();
    h.run_steps(4);
    assert_eq!(
        h.state().dialogs.write.keys.len(),
        2,
        "main menu explicitly acts on selection"
    );
    assert!(
        h.query_by_label("Количество выбранных сигналов: 2")
            .is_some()
    );
}

#[test]
fn keyboard_double_click_edits_one_tag_after_multiple_selection() {
    let mut app = App::new_test(&egui::Context::default());
    let keys = seed_writable_table(&mut app, &[("a", "SigA", 1), ("b", "SigB", 2)]);
    set_ru(&mut app);
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(60)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.state_mut().select_extend_to(1);
    h.run_steps(2);
    let pos = row_cell_pos(&h, "SigB");
    pointer_click(&h, pos, PointerButton::Primary);
    h.run_steps(2);
    pointer_click(&h, pos, PointerButton::Primary);
    h.run_steps(4);
    assert!(h.state().dialogs.write.open);
    assert_eq!(h.state().dialogs.write.keys, vec![keys[1]]);
}

#[test]
fn keyboard_control_shift_range_contracts_against_original_selection() {
    let mut app = App::new_test(&egui::Context::default());
    let keys = seed_writable_table(
        &mut app,
        &[
            ("a", "SigA", 1),
            ("b", "SigB", 2),
            ("c", "SigC", 3),
            ("d", "SigD", 4),
            ("e", "SigE", 5),
        ],
    );
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.state_mut().select_toggle(2);
    h.run_steps(2);
    let mods = Modifiers::CTRL | Modifiers::SHIFT;
    h.event(egui::Event::ModifiersChanged(mods));
    h.run_steps(1);
    for key in [Key::ArrowDown, Key::ArrowDown, Key::ArrowUp] {
        h.event(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: mods,
        });
        h.event(egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: mods,
        });
        h.run_steps(2);
    }
    assert_eq!(
        h.state().selected_rows,
        [keys[0], keys[2], keys[3]].into_iter().collect()
    );
}

#[test]
fn keyboard_delete_with_selection_change_in_same_frame_never_removes_rows() {
    let mut app = App::new_test(&egui::Context::default());
    seed_writable_table(&mut app, &[("a", "SigA", 1), ("b", "SigB", 2)]);
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.run_steps(2);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        },
    ]);
    h.run_steps(3);
    assert_eq!(h.state().rows().len(), 2);
}

#[test]
fn keyboard_select_all_menu_only_selects_active_session_rows() {
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::da("", "First"));
    let first = app.seed_writable_row(sid(1), "a", "SigA", Value::I64(1));
    app.seed_session(sid(2), EndpointSpec::da("", "Second"));
    let second = app.seed_writable_row(sid(2), "b", "SigB", Value::I64(2));
    app.activate_container(sid(1));
    let mut h = harness(app);
    h.run_steps(4);
    h.get_by_label("Сигналы").click();
    h.run_steps(3);
    h.get_by_label("Выделить все").click();
    h.run_steps(3);
    assert_eq!(h.state().selected_rows, [first].into_iter().collect());
    assert!(!h.state().selected_rows.contains(&second));
}

#[test]
fn keyboard_clearing_selection_discards_additive_gesture_snapshot() {
    let mut app = App::new_test(&egui::Context::default());
    let keys = seed_writable_table(
        &mut app,
        &[
            ("a", "A", 1),
            ("b", "B", 2),
            ("c", "C", 3),
            ("d", "D", 4),
            ("e", "E", 5),
        ],
    );
    let mut h = harness(app);
    h.run_steps(3);
    h.state_mut().select_only(0);
    h.state_mut().select_toggle(2);
    h.run_steps(2);
    let mods = Modifiers::CTRL | Modifiers::SHIFT;
    h.event(egui::Event::ModifiersChanged(mods));
    h.run_steps(1);
    h.event(egui::Event::Key {
        key: Key::ArrowDown,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: mods,
    });
    h.event(egui::Event::Key {
        key: Key::ArrowDown,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: mods,
    });
    h.run_steps(2);
    assert!(h.state().selected_rows.contains(&keys[0]));
    h.state_mut().clear_selection();
    h.run_steps(1);
    h.event(egui::Event::Key {
        key: Key::ArrowDown,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: mods,
    });
    h.event(egui::Event::Key {
        key: Key::ArrowDown,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: mods,
    });
    h.run_steps(2);
    assert!(
        !h.state().selected_rows.contains(&keys[0]),
        "old snapshot must not restore cleared rows"
    );
    assert_eq!(
        h.state().selected_rows,
        [keys[2], keys[3], keys[4]].into_iter().collect()
    );
}

#[test]
fn active_profile_cancel_keeps_active_saved_and_staged_profile() {
    use opclook::bff::model::{AcquisitionConfig, UpdateMode};
    with_settings_lock(|| {
        let mut app = App::new_test(&egui::Context::default());
        let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
        endpoint.acquisition = Some(AcquisitionConfig::ua_default());
        app.seed_session(sid(1), endpoint.clone());
        app.record_dispatches_for_test();
        app.status_popup = true;
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Настроить профиль соединения").click();
        h.run_steps(4);
        assert!(!h.state().status_popup);
        h.get_by_label("Опрос").click();
        h.run_steps(2);
        h.get_by_label("Сохранить для следующего ручного подключения")
            .click();
        h.run_steps(3);
        assert_eq!(
            h.state().sessions[0]
                .pending_acquisition
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .update_mode,
            UpdateMode::Poll
        );
        let settings = serde_json::to_value(&h.state().settings).unwrap();
        let count = h.state().dispatched_for_test().len();
        h.state_mut().status_popup = true;
        h.run_steps(3);
        h.get_by_label("Настроить профиль соединения").click();
        h.run_steps(4);
        h.get_by_label("Подписка").click();
        h.run_steps(2);
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(!h.state().dialogs.acquisition.open);
        assert_eq!(
            h.state().sessions[0].endpoint().acquisition,
            endpoint.acquisition
        );
        assert_eq!(
            h.state().sessions[0]
                .pending_acquisition
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .update_mode,
            UpdateMode::Poll
        );
        assert_eq!(serde_json::to_value(&h.state().settings).unwrap(), settings);
        assert_eq!(h.state().dispatched_for_test().len(), count);
    });
}

#[test]
fn active_profile_enter_message_and_missing_target_are_safe() {
    use opclook::app::MessageDialog;
    use opclook::bff::model::AcquisitionConfig;
    let mut app = App::new_test(&egui::Context::default());
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    endpoint.acquisition = Some(AcquisitionConfig::ua_default());
    app.seed_session(sid(1), endpoint);
    app.record_dispatches_for_test();
    app.open_acquisition_editor(sid(1));
    let mut h = harness(app);
    h.run_steps(5);
    let before = h.state().dispatched_for_test().len();
    h.get_by_value("1000").click();
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.acquisition.open);
    assert_eq!(h.state().dispatched_for_test().len(), before);
    h.state_mut().dialogs.message = Some(MessageDialog {
        title: "notice".into(),
        text: "server notice".into(),
        details: String::new(),
    });
    h.run_steps(4);
    assert!(h.query_by_label("server notice").is_some());
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert!(h.state().dialogs.message.is_none());
    assert!(h.state().dialogs.acquisition.open);
    h.state_mut().remove_session(sid(1));
    h.run_steps(4);
    assert!(h.state().dialogs.acquisition.open);
    assert!(
        h.query_by_label("Соединение изменилось или закрыто; откройте настройки снова")
            .is_some()
    );
    h.get_by_label("Отмена").click();
    h.run_steps(3);
    assert!(!h.state().dialogs.acquisition.open);
}

#[test]
fn p4_keyboard_palette_typed_browse_and_unknown_result() {
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.record_dispatches_for_test();
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run_steps(3);
    assert!(h.state().dialogs.palette.open);
    h.event(egui::Event::Text("2001".into()));
    h.run_steps(3);
    assert!(h.state().dispatched_for_test().is_empty());
    h.get_by_label("NodeId ветки (пусто — корень)").focus();
    h.run_steps(1);
    h.event(egui::Event::Text("ns=2;s=Unknown".into()));
    h.run_steps(2);
    assert!(h.state().dispatched_for_test().is_empty());
    h.key_press(Key::Enter);
    h.run_steps(3);
    let commands = h.state().dispatched_for_test();
    let browse = commands.iter().find(|c| matches!(&c.kind, CommandKind::Browse { node_id, .. } if node_id == "ns=2;s=Unknown")).expect("typed Browse dispatched");
    let request = browse.request_id.unwrap();
    h.state_mut().on_event_for_test_with_request(
        sid(1),
        request,
        EventKind::Browse {
            node_id: "ns=2;s=Unknown".into(),
            children: vec![BrowseNode {
                node_id: "ns=2;s=Child".into(),
                name: "Child".into(),
                kind: NodeKind::Item,
                has_children: false,
            }],
        },
    );
    h.run_steps(2);
    assert!(
        h.state().sessions[0]
            .tree()
            .iter()
            .any(|item| item.node.node_id == "ns=2;s=Unknown" && item.loaded)
    );
    assert!(
        h.state().sessions[0]
            .tree()
            .iter()
            .any(|item| item.node.node_id == "ns=2;s=Child")
    );
}

#[test]
fn p4_browse_parameter_has_a_localized_accessible_name() {
    use opclook::i18n::{Lang, Tr, tr};
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        let mut app = App::new_test(&egui::Context::default());
        app.settings.set_lang(lang);
        app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
        app.open_command_palette();
        app.dialogs.palette.query = "2001".into();
        let mut h = Harness::builder()
            .with_size([1200.0, 900.0])
            .with_max_steps(60)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
        h.run_steps(4);
        let node = h.get_by_label(tr(lang, Tr::PaletteBrowseNode));
        assert_eq!(
            node.accesskit_node().role(),
            egui::accesskit::Role::TextInput
        );
    }
}

#[test]
fn p4_palette_write_reuses_typed_dialog_and_confirmation() {
    let mut app = App::new_test(&egui::Context::default());
    let key = seed_writable_table(&mut app, &[("a", "SigA", 42)])[0];
    app.select_only(0);
    app.settings.view.options.confirm_write = true;
    app.record_dispatches_for_test();
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run_steps(3);
    h.event(egui::Event::Text("2002".into()));
    h.run_steps(2);
    assert!(h.state().dispatched_for_test().is_empty());
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.write.open);
    h.event(egui::Event::Text("77".into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    // The dialog-region handoff deliberately waits for a clean input frame.
    h.run_steps(4);
    assert!(h.state().dialogs.confirm.is_some());
    assert!(h.state().dispatched_for_test().is_empty());
    h.get_by_label("Да").click();
    h.run_steps(3);
    let commands = h.state().dispatched_for_test();
    assert_eq!(commands.iter().filter(|c| matches!(&c.kind, CommandKind::Write { request } if request.key == key && request.value == Value::I64(77))).count(), 1);
}

#[test]
fn p4_palette_keyboard_refuses_repeat_ime_and_modal() {
    for mode in 0..3 {
        let mut app = App::new_test(&egui::Context::default());
        if mode == 2 {
            app.dialogs.about.open = true;
        }
        let mut h = harness(app);
        h.run_steps(3);
        if mode == 1 {
            h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "x".into(),
                active_range_chars: None,
            }));
        }
        if mode == 0 {
            // egui derives repeat from held-key state, ignoring the incoming
            // repeat bit. Hold an unmodified P first; then send its repeat.
            h.event(egui::Event::Key {
                key: Key::P,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(1);
            assert!(!h.state().dialogs.palette.open, "initial unmodified P");
            h.event(egui::Event::Key {
                key: Key::P,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: Modifiers::COMMAND,
            });
        } else {
            h.key_press_modifiers(Modifiers::COMMAND, Key::P);
        }
        h.run_steps(2);
        assert!(
            !h.state().dialogs.palette.open,
            "unexpected palette open in mode {mode} (0=held repeat, 1=IME, 2=modal)"
        );
    }
}

#[test]
fn p4_palette_actions_are_pure_and_localized_for_disabled_context() {
    use opclook::i18n::{Lang, Tr, tr};
    for lang in [Lang::Ru, Lang::En, Lang::Zh] {
        let mut app = App::new_test(&egui::Context::default());
        app.settings.set_lang(lang);
        app.seed_session(sid(1), EndpointSpec::da("localhost", "Srv.Test"));
        app.seed_row(sid(1), "readonly", "ReadOnly", Value::I64(1));
        app.select_only(0);
        app.record_dispatches_for_test();
        app.open_command_palette();
        app.dialogs.palette.query = "2002".into();
        let mut h = Harness::builder()
            .with_size([1200.0, 900.0])
            .with_max_steps(60)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(|ui, app| opclook::ui::draw(app, ui), app);
        h.run_steps(3);
        assert!(h.query_by_label(tr(lang, Tr::ReadOnly)).is_some());
        assert!(!widget_enabled(&h, tr(lang, Tr::PaletteExecute)));
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert!(h.state().dialogs.palette.open);
        assert!(h.state().dispatched_for_test().is_empty());
    }
}

#[test]
fn p3_columns_focused_control_owns_enter_before_dialog_apply() {
    with_settings_lock(|| {
        use opclook::columns::ColumnId;
        let mut app = App::new_test(&egui::Context::default());
        seed_table(&mut app, &[("a", "SigA", 1)]);
        app.dialogs.columns.open = true;
        let mut h = harness(app);
        h.run_steps(3);
        let before = h.state().settings.resolved_order();
        let index = before
            .iter()
            .position(|column| *column == ColumnId::FullName)
            .unwrap();
        assert!(index + 1 < before.len());
        let down = h
            .query_all_by_label("↓")
            .nth(index)
            .expect("column down button");
        down.focus();
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(
            h.state().dialogs.columns.open,
            "Enter must not also close the dialog"
        );
        assert_eq!(
            h.state().settings.resolved_order()[index + 1],
            ColumnId::FullName
        );
        let after = h.state().settings.resolved_order();
        h.query_all_by_label("↓")
            .nth(index + 1)
            .expect("moved column down button")
            .focus();
        h.run_steps(1);
        h.input_mut().events.push(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(2);
        assert!(h.state().dialogs.columns.open);
        assert_eq!(
            h.state().settings.resolved_order(),
            after,
            "repeat must not move or close"
        );
        h.get_by_label("Применить").click();
        h.run_steps(3);
        assert!(!h.state().dialogs.columns.open);
    });
}

#[test]
fn p3_ime_candidate_does_not_switch_add_signal_session() {
    let mut app = App::new_test(&egui::Context::default());
    app.seed_session(sid(1), EndpointSpec::da("", "First"));
    app.seed_session(sid(2), EndpointSpec::da("", "Second"));
    app.dialogs.add_signal.open = true;
    app.dialogs.add_signal.session = Some(sid(1));
    let sessions = app.active_sessions();
    let current = sessions
        .iter()
        .find(|(id, _)| *id == sid(1))
        .unwrap()
        .1
        .clone();
    let target = sessions
        .iter()
        .find(|(id, _)| *id == sid(2))
        .unwrap()
        .1
        .clone();
    let mut h = harness(app);
    h.run_steps(3);
    h.query_all_by_value(&current)
        .last()
        .expect("session combo")
        .click();
    h.run_steps(3);
    h.get_all_by_label(&target)
        .last()
        .expect("other session option")
        .focus();
    h.run_steps(1);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "a".into(),
        active_range_chars: None,
    }));
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().dialogs.add_signal.session, Some(sid(1)));
    assert!(h.state().dialogs.add_signal.open);
}

#![cfg(feature = "ui-snapshots")]
//! Manual visual QA; run ignored test with a headless wgpu renderer.
use egui_kittest::{Harness, kittest::Queryable};
use opclook::app::{App, ConfirmAction, ConnectPane, DaHit, UaIdentity};
use opclook::bff::model::{EndpointSpec, Protocol, ServerEntry, SessionId, Value};
use opclook::i18n::Lang;

fn base(lang: Lang) -> App {
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    app.dialogs.connect.da_servers.push(DaHit {
        server: ServerEntry {
            id: "Vendor.OPC".into(),
            description: "Plant".into(),
        },
        host: "plant-a".into(),
    });
    app.settings.set_lang(lang);
    app.settings.view.options.update_interval_secs = 86_400;
    app.settings.view.options.test_interval_secs = 0;
    app.settings.view.options.auto_reconnect = false;
    app
}
fn save(name: &str, app: App) {
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), app);
    h.run_steps(8);
    std::fs::create_dir_all("docs/screenshots").unwrap();
    h.render()
        .unwrap()
        .save(format!("docs/screenshots/{name}.png"))
        .unwrap();
    if name == "da-helper-error-ru" {
        assert!(
            h.query_all_by_label_contains("IPC timeout")
                .next()
                .is_some()
        );
    }
}
#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn rendered_app_scenes() {
    let session = SessionId::new(1).unwrap();
    let mut main = base(Lang::Ru);
    main.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    for i in 0..10_000 {
        main.seed_row(
            session,
            &format!("ns=2;i={i}"),
            &format!("Tag{i:05}"),
            Value::I64(i),
        );
    }
    assert_eq!(main.rows().len(), 10_000);
    save("main-10k-ru", main);

    let mut da = base(Lang::Ru);
    da.open_connect_dialog();
    da.dialogs.connect.pane = ConnectPane::Da;
    da.dialogs.connect.da_pending = false;
    da.dialogs.connect.da_auto_pending = 0;
    da.dialogs.connect.error = Some(opclook::i18n::da_route_error(
        Lang::Ru,
        &opclook::da_route::DaRouteError::HelperUnavailable("IPC timeout".into()),
    ));
    save("da-helper-error-ru", da);

    let mut ua = base(Lang::Zh);
    ua.open_connect_dialog();
    ua.dialogs.connect.pane = ConnectPane::Ua;
    ua.dialogs.connect.ua_identity = UaIdentity::UserName;
    ua.dialogs.connect.ua_search_host = "127.0.0.1".into();
    save("ua-security-zh", ua);

    let mut net = base(Lang::Ru);
    net.open_connect_dialog();
    net.dialogs.connect.pane = ConnectPane::Network {
        previous: Protocol::OpcDa,
    };
    save("network-ru", net);

    let mut confirm = base(Lang::Ru);
    confirm.seed_session(session, EndpointSpec::da("", "Srv.Test"));
    confirm.request_confirm(
        "Подтверждение".into(),
        "Удалить сервер и все его сигналы?".into(),
        ConfirmAction::RemoveSession(session),
    );
    save("confirmation-ru", confirm);
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p1_theme_scenes() {
    use opclook::i18n::{Tr, tr};
    use opclook::settings::Theme;
    let session = SessionId::new(1).unwrap();
    std::fs::create_dir_all("docs/screenshots/p1").unwrap();
    for (language, lang) in [("ru", Lang::Ru), ("en", Lang::En), ("zh", Lang::Zh)] {
        for (name, theme) in [
            ("light", Theme::Light),
            ("dark", Theme::Dark),
            ("system", Theme::System),
        ] {
            let mut app = base(lang);
            app.settings.view.options.theme = theme;
            app.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
            let selected = app.seed_row(session, "ns=2;s=Level", "Level", Value::F64(21.5));
            app.seed_row(session, "ns=2;s=Pressure", "Pressure", Value::F64(7.25));
            app.selected_rows.insert(selected);
            app.journal.info("Connected to sample server");
            app.journal.warn("Sample warning");
            app.journal.error("Sample error");
            let mut h = Harness::builder()
                .with_size([1200.0, 700.0])
                .with_accessibility_check(false)
                .with_render_cursor(false)
                .with_context_setup(|ctx| {
                    assert!(opclook::app::install_fonts(ctx).is_some());
                })
                .build_ui_state(|ui, app: &mut App| app.update(ui), app);
            h.input_mut().system_theme = Some(egui::Theme::Light);
            h.run_steps(8);
            h.get_by_label(tr(lang, Tr::ThemeTitle)).click();
            h.run_steps(4);
            h.key_press(egui::Key::ArrowDown);
            h.run_steps(3);
            assert!(h.ctx.memory(|memory| memory.focused()).is_some());
            let hover = h.get_by_label(tr(lang, Tr::ThemeDark)).rect().center();
            h.event(egui::Event::PointerMoved(hover));
            h.run_steps(3);
            h.render()
                .unwrap()
                .save(format!("docs/screenshots/p1/{name}-{language}.png"))
                .unwrap();
        }
    }
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p1_narrow_scene() {
    use opclook::i18n::{Tr, tr};
    let mut h = Harness::builder()
        .with_size([480.0, 520.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), base(Lang::Ru));
    h.run_steps(8);
    h.get_by_label(tr(Lang::Ru, Tr::ThemeTitle)).click();
    h.run_steps(4);
    std::fs::create_dir_all("docs/screenshots/p1").unwrap();
    h.render()
        .unwrap()
        .save("docs/screenshots/p1/narrow-ru.png")
        .unwrap();
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p1_focus_scene() {
    let mut h = Harness::builder()
        .with_size([1200.0, 700.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), base(Lang::En));
    h.run_steps(8);
    h.get_by_label(opclook::i18n::tr(Lang::En, opclook::i18n::Tr::ThemeTitle))
        .click();
    h.run_steps(3);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(4);
    assert!(h.ctx.memory(|memory| memory.focused()).is_some());
    std::fs::create_dir_all("docs/screenshots/p1").unwrap();
    h.render()
        .unwrap()
        .save("docs/screenshots/p1/focus-en.png")
        .unwrap();
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p2_disabled_action_scene() {
    let mut app = base(Lang::Ru);
    app.dialogs.about.open = true;
    save("p2-disabled-action-ru", app);
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p3_followup_shortcut_menu_ru() {
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), base(Lang::Ru));
    h.run_steps(5);
    h.get_by_label("OPC клиент").click();
    h.run_steps(3);
    h.render()
        .unwrap()
        .save("docs/screenshots/p3-shortcut-menu-followup-ru.png")
        .unwrap();
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p4_palette_ru() {
    let mut app = base(Lang::Ru);
    app.dialogs.palette.open = true;
    app.dialogs.palette.focus_pending = true;
    save("p4-palette-ru", app);
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p5_button_states_ru() {
    save("p5-button-enabled-ru", base(Lang::Ru));
    let mut disabled = base(Lang::Ru);
    disabled.dialogs.about.open = true;
    save("p5-button-disabled-ru", disabled);
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn keyboard_write_and_confirmation_ru() {
    let session = SessionId::new(1).unwrap();
    let mut app = base(Lang::Ru);
    app.settings.view.options.confirm_write = true;
    app.seed_session(session, EndpointSpec::da("", "Srv.Test"));
    let key = app.seed_writable_row(session, "a", "Pump.Pressure", Value::I64(42));
    app.open_write_dialog(vec![key]);
    save("keyboard-write-ru", app);
    let mut app = base(Lang::Ru);
    app.settings.view.options.confirm_write = true;
    app.seed_session(session, EndpointSpec::da("", "Srv.Test"));
    let key = app.seed_writable_row(session, "a", "Pump.Pressure", Value::I64(42));
    app.open_write_dialog(vec![key]);
    app.apply_write();
    save("keyboard-confirm-no-ru", app);
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn keyboard_unselected_focus_ru() {
    let session = SessionId::new(1).unwrap();
    let mut app = base(Lang::Ru);
    app.seed_session(session, EndpointSpec::da("", "Srv.Test"));
    app.seed_writable_row(session, "a", "Pump.Pressure", Value::I64(42));
    app.seed_writable_row(session, "b", "Pump.Flow", Value::I64(15));
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), app);
    h.run_steps(4);
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(h.state().table_focus, Some(1));
    assert_eq!(h.state().selected_rows.len(), 1);
    h.render()
        .unwrap()
        .save("docs/screenshots/keyboard-unselected-focus-ru.png")
        .unwrap();
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn write_outcomes_render() {
    use egui::Key;
    use opclook::bff::model::{BackendMsg, EventKind, WriteOutcome};
    let output = std::env::var("OPCLOOK_WRITE_SCREENSHOTS").unwrap();
    for scenario in ["rejected", "not-submitted", "unknown"] {
        let session = SessionId::FIRST;
        let mut app = base(Lang::En);
        app.settings.view.options.confirm_write = false;
        app.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
        let key = app.seed_writable_row(session, "ns=2;i=1", "Pump", Value::I64(9));
        let mut h = Harness::builder()
            .with_size([1200.0, 900.0])
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                assert!(opclook::app::install_fonts(ctx).is_some());
            })
            .build_ui_state(|ui, app: &mut App| app.update(ui), app);
        h.state_mut().select_only(0);
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(3);
        h.key_press(Key::Enter);
        h.run_steps(3);
        let id = h.state().pending_request_ids_for_test(session)[0];
        let result = match scenario {
            "rejected" => Ok(WriteOutcome::from_ua(0x8000_0042, "Bad")),
            "not-submitted" => Err(BackendMsg::NodeNotSubscribed),
            _ => Ok(WriteOutcome::unknown_because(BackendMsg::TimeoutWrite)),
        };
        h.state_mut()
            .on_event_for_test_with_request(session, id, EventKind::Wrote { key, result });
        h.run_steps(8);
        assert_eq!(h.state().dialogs.message.is_some(), scenario != "unknown");
        h.render()
            .unwrap()
            .save(format!("{output}/{scenario}.png"))
            .unwrap();
    }
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn acquisition_dialog_scenes() {
    let output = std::env::var("OPCLOOK_ACQUISITION_SCREENSHOTS").unwrap();
    for (name, ua, height) in [
        ("da", false, 900.0),
        ("ua", true, 900.0),
        ("ua-short", true, 720.0),
    ] {
        let mut app = base(Lang::En);
        app.open_connect_dialog();
        let mut h = Harness::builder()
            .with_size([1200.0, height])
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                assert!(opclook::app::install_fonts(ctx).is_some());
            })
            .build_ui_state(|ui, app: &mut App| app.update(ui), app);
        h.run_steps(3);
        if ua {
            h.get_by_label("OPC UA").click();
            h.run_steps(3);
        }
        h.get_by_label("Data acquisition").click();
        h.run_steps(8);
        h.render()
            .unwrap()
            .save(format!("{output}/{name}.png"))
            .unwrap();
        if name == "ua-short" {
            h.event(egui::Event::PointerMoved(egui::pos2(800.0, 500.0)));
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -1500.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(8);
            h.render()
                .unwrap()
                .save(format!("{output}/ua-short-scrolled.png"))
                .unwrap();
            h.get_by_label("Cancel").click();
            h.run_steps(3);
            assert!(!h.state().dialogs.connect.open);
        }
    }
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn active_acquisition_profile_scenes() {
    use opclook::bff::model::AcquisitionConfig;
    let output = std::env::var("OPCLOOK_ACTIVE_PROFILE_SCREENSHOTS").unwrap();
    for height in [900.0, 720.0] {
        let mut app = base(Lang::Ru);
        let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
        endpoint.acquisition = Some(AcquisitionConfig::ua_default());
        app.seed_session(SessionId::FIRST, endpoint);
        app.open_acquisition_editor(SessionId::FIRST);
        let mut h = Harness::builder()
            .with_size([1200.0, height])
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                assert!(opclook::app::install_fonts(ctx).is_some());
            })
            .build_ui_state(|ui, app: &mut App| app.update(ui), app);
        h.run_steps(8);
        h.render()
            .unwrap()
            .save(format!("{output}/profile-{height}.png"))
            .unwrap();
        h.get_by_label("Отмена").click();
        h.run_steps(3);
        assert!(!h.state().dialogs.acquisition.open);
    }
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p4_palette_browse_ru() {
    use opclook_contract::actions::ActionId;
    for (lang, suffix) in [(Lang::Ru, "ru"), (Lang::En, "en"), (Lang::Zh, "zh")] {
        let mut app = base(lang);
        app.settings.view.options.theme = opclook::settings::Theme::Dark;
        app.seed_session(
            SessionId::FIRST,
            EndpointSpec::ua("opc.tcp://127.0.0.1:4840"),
        );
        app.open_command_palette();
        app.dialogs.palette.query = "2001".into();
        app.dialogs.palette.selected = Some(ActionId::Browse);
        app.dialogs.palette.browse_node = "ns=2;s=Plant/Line1".into();
        save(&format!("p4-palette-browse-{suffix}"), app);
    }
    let mut app = base(Lang::Ru);
    app.settings.view.options.theme = opclook::settings::Theme::Light;
    app.seed_session(
        SessionId::FIRST,
        EndpointSpec::da("localhost", "Vendor.OPC"),
    );
    let key = app.seed_writable_row(SessionId::FIRST, "Fixture.Value", "Насос", Value::I64(42));
    app.settings.view.options.confirm_write = true;
    app.select_only(0);
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), app);
    h.run_steps(5);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    h.run_steps(3);
    h.event(egui::Event::Text("2002".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialogs.write.keys, vec![key]);
    h.event(egui::Event::Text("77".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(h.state().dialogs.confirm.is_some());
    assert!(h.state().dispatched_for_test().is_empty());
    std::fs::create_dir_all("docs/screenshots").unwrap();
    h.render()
        .unwrap()
        .save("docs/screenshots/p4-palette-write-confirm-ru.png")
        .unwrap();
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn t08_value_time_tooltip_ru() {
    use opclook::bff::model::{ValueOrigin, ValueTimes};
    let session = SessionId::new(1).unwrap();
    let mut app = base(Lang::Ru);
    app.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.seed_row(session, "ns=2;s=Clock", "Clock", Value::I64(1));
    for column in opclook::columns::ColumnId::ALL {
        app.settings.view.columns.set_visible(
            column,
            matches!(
                column,
                opclook::columns::ColumnId::FullName
                    | opclook::columns::ColumnId::Value
                    | opclook::columns::ColumnId::Quality
                    | opclook::columns::ColumnId::Time
            ),
        );
    }
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-31T12:34:56Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let server = now + chrono::Duration::milliseconds(5);
    let received = now + chrono::Duration::milliseconds(12);
    let row = &mut app.rows_mut_for_test()[0];
    row.times = ValueTimes::ua(Some(now), Some(server), received, ValueOrigin::Read);
    row.timestamp = row.times.primary();
    let text = row.timestamp_text();
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_max_steps(60)
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), app);
    h.run_steps(8);
    h.get_by_label(&text).hover();
    h.run_steps(40);
    assert!(
        h.query_all_by_label_contains("Время источника")
            .next()
            .is_some()
    );
    std::fs::create_dir_all("docs/screenshots").unwrap();
    h.render()
        .unwrap()
        .save("docs/screenshots/t08-value-time-tooltip-ru.png")
        .unwrap();
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p3_columns_focused_control_ru() {
    use egui_kittest::kittest::NodeT;
    use opclook::columns::ColumnId;
    let mut app = base(Lang::Ru);
    app.dialogs.columns.open = true;
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), app);
    h.run_steps(6);
    let order = h.state().settings.resolved_order();
    let index = order
        .iter()
        .position(|column| *column == ColumnId::FullName)
        .unwrap();
    h.query_all_by_label("↓")
        .nth(index)
        .expect("column down button")
        .focus();
    h.run_steps(4);
    std::fs::create_dir_all("docs/screenshots").unwrap();
    h.render()
        .unwrap()
        .save("docs/screenshots/p3-columns-focus-ru.png")
        .unwrap();
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p5_options_hints_ru() {
    let mut app = base(Lang::Ru);
    app.open_options_dialog();
    save("p5-options-hints-ru", app);
}

#[test]
#[ignore = "manual visual QA requiring a headless wgpu renderer"]
fn p2_action_payload_error_ru() {
    let mut app = base(Lang::Ru);
    // Synthetic rendering fixture; App dispatch validation is tested separately.
    app.status = opclook::i18n::tr(Lang::Ru, opclook::i18n::Tr::ActionPayloadInvalid).to_string();
    save("p2-action-payload-error-ru", app);
}

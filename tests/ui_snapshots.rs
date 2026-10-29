#![cfg(feature = "ui-snapshots")]
//! Manual visual QA; run ignored test with a headless wgpu renderer.
use egui_kittest::{Harness, kittest::Queryable};
use opcda_egui::app::{App, ConfirmAction, ConnectPane, DaHit, UaIdentity};
use opcda_egui::bff::model::{EndpointSpec, Protocol, ServerEntry, SessionId, Value};
use opcda_egui::i18n::Lang;

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
            assert!(opcda_egui::app::install_fonts(ctx).is_some());
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
    da.dialogs.connect.error = Some(opcda_egui::i18n::da_route_error(
        Lang::Ru,
        &opcda_egui::da_route::DaRouteError::HelperUnavailable("IPC timeout".into()),
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
    use opcda_egui::i18n::{Tr, tr};
    use opcda_egui::settings::Theme;
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
                    assert!(opcda_egui::app::install_fonts(ctx).is_some());
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
    use opcda_egui::i18n::{Tr, tr};
    let mut h = Harness::builder()
        .with_size([480.0, 520.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opcda_egui::app::install_fonts(ctx).is_some());
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
            assert!(opcda_egui::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), base(Lang::En));
    h.run_steps(8);
    h.get_by_label(opcda_egui::i18n::tr(
        Lang::En,
        opcda_egui::i18n::Tr::ThemeTitle,
    ))
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
            assert!(opcda_egui::app::install_fonts(ctx).is_some());
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

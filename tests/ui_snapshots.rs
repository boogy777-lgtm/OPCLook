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

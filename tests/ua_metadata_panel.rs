//! Requested and SDK-revised facts remain read-only in the acquisition dialog.
use egui_kittest::{Harness, kittest::Queryable};
use opclook::app::App;
use opclook::bff::model::{
    AcquisitionConfig, EndpointSpec, EventKind, ItemKey, SessionId, UaMonitoredParameters,
    UaRevisedSubscription, UaSubscriptionParameters,
};

fn fixture() -> App {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.set_lang(opclook::i18n::Lang::En);
    app.record_dispatches_for_test();
    let session = SessionId::FIRST;
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    endpoint.acquisition = Some(AcquisitionConfig::ua_default());
    app.seed_session(session, endpoint);
    app.on_event_for_test_at(
        session,
        EventKind::UaSubscriptionParameters {
            subscriptions: vec![UaSubscriptionParameters {
                subscription_id: 17,
                requested_publishing_ms: 1000,
                requested_lifetime: 30,
                requested_keepalive: 10,
                revised: Some(UaRevisedSubscription::from_sdk(1250, 30, 10)),
                items: vec![UaMonitoredParameters {
                    key: ItemKey::FIRST,
                    requested_sampling_ms: -1.0,
                    requested_queue_size: 2,
                    revised_sampling_ms: 125.5,
                    revised_queue_size: 3,
                }],
            }],
        },
    );
    app.open_acquisition_editor(session);
    app
}

#[test]
fn requested_and_revised_values_are_visible_without_dispatch() {
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut App| opclook::ui::draw(app, ui), fixture());
    h.run_steps(8);
    assert!(
        h.query_by_label("Publishing: requested 1000, revised 1250")
            .is_some()
    );
    assert!(
        h.query_by_label(
            "Key 1: sampling requested -1, revised 125.5; queue requested 2, revised 3"
        )
        .is_some()
    );
    assert!(h.state().dispatched_for_test().is_empty());
}

#[cfg(feature = "ui-snapshots")]
#[test]
#[ignore = "manual visual QA requiring a headless renderer"]
fn ua_acquisition_revised_facts_snapshot() {
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .with_render_cursor(false)
        .with_context_setup(|ctx| {
            assert!(opclook::app::install_fonts(ctx).is_some());
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), fixture());
    h.run_steps(8);
    std::fs::create_dir_all("docs/screenshots").unwrap();
    h.render()
        .unwrap()
        .save("docs/screenshots/ua-acquisition-revised-en.png")
        .unwrap();
}

#[test]
fn unknown_access_is_distinct_from_read_only_in_write_dialog() {
    let mut app = App::new_test(&egui::Context::default());
    app.settings.set_lang(opclook::i18n::Lang::En);
    app.seed_session(
        SessionId::FIRST,
        EndpointSpec::ua("opc.tcp://127.0.0.1:4840"),
    );
    let key = app.seed_writable_row(
        SessionId::FIRST,
        "ns=2;s=tag",
        "tag",
        opclook::bff::model::Value::I64(1),
    );
    app.rows_mut_for_test()[0].writable = false;
    app.rows_mut_for_test()[0].ua_access_known = false;
    app.open_write_dialog(vec![key]);
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut App| opclook::ui::draw(app, ui), app);
    h.run_steps(8);
    assert!(h.query_by_label("Write permission is unknown").is_some());
    assert!(
        h.query_by_label("Signal is read-only (writing is unavailable)")
            .is_none()
    );
    h.state_mut().rows_mut_for_test()[0].ua_access_known = true;
    h.run_steps(4);
    assert!(
        h.query_by_label("Signal is read-only (writing is unavailable)")
            .is_some()
    );
}

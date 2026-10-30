use super::present::operation_label;
use super::sessions::reconnect_delay;
use super::table::infer_kind;
use super::*;
use crate::bff::model::{
    BackendMsg, BrowseNode, EndpointSpec, Event, EventKind, NodeKind, Operation, Origin,
    WriteRequest,
};
use crate::constants::{RECONNECT_MAX_ATTEMPTS, RECONNECT_MAX_DELAY};
use crate::i18n::Lang;
use crate::settings::Options;

#[test]
fn reconnect_delay_doubles_exponentially() {
    // Attempts 1..=7 double: 1, 2, 4, ... 64 would be next but the ceiling
    // (RECONNECT_MAX_DELAY = 60) applies from attempt 7 on.
    let expected = [1u64, 2, 4, 8, 16, 32, 60, 60, 60];
    for (attempt, secs) in expected.iter().enumerate() {
        assert_eq!(
            reconnect_delay(attempt as u32 + 1).as_secs(),
            *secs,
            "attempt {} should be {}s",
            attempt + 1,
            secs
        );
    }
}

#[test]
fn reconnect_delay_has_a_ceiling_for_every_attempt() {
    // U10: the old loop reached 2^19 s (about 6 days) at attempt 20.
    for attempt in 1..=RECONNECT_MAX_ATTEMPTS {
        assert!(
            reconnect_delay(attempt).as_secs() <= RECONNECT_MAX_DELAY.as_secs(),
            "attempt {attempt}"
        );
    }
    assert_eq!(
        reconnect_delay(RECONNECT_MAX_ATTEMPTS).as_secs(),
        RECONNECT_MAX_DELAY.as_secs()
    );
}

#[test]
fn reconnect_delay_never_overflows() {
    // Beyond the 64-bit shift width the helper pins to the ceiling instead of
    // panicking.
    assert_eq!(reconnect_delay(65).as_secs(), RECONNECT_MAX_DELAY.as_secs());
    assert_eq!(
        reconnect_delay(u32::MAX).as_secs(),
        RECONNECT_MAX_DELAY.as_secs()
    );
}

#[test]
fn clamp_options_enforces_interval_and_timeout_ranges() {
    // Update interval is capped at 1..=60 (reconciled with the TrackBar);
    // the advanced timeouts clamp to 1..=1800.
    let mut draft = Options::default();
    draft.update_interval_secs = 10_000; // far above the 60 s TrackBar cap
    draft.operation_timeout_secs = 1_000_000;
    draft.test_interval_secs = 0;
    let clamped = clamp_options(draft);
    assert_eq!(clamped.update_interval_secs, 60);
    assert_eq!(clamped.operation_timeout_secs, 1800);
    assert_eq!(clamped.test_interval_secs, 1);
}

#[test]
fn every_operation_has_a_localized_label() {
    for operation in Operation::ALL {
        for lang in Lang::ALL {
            let label = operation_label(lang, operation);
            assert!(
                !label.trim().is_empty(),
                "empty label for {operation:?} in {lang:?}"
            );
            assert_ne!(
                label,
                operation.as_str(),
                "operation label leaked the raw token for {operation:?} in {lang:?}"
            );
        }
    }
    // The three formerly-raw tokens now render localized text.
    assert_eq!(
        operation_label(Lang::Ru, Operation::Subscription),
        "Подписка"
    );
    assert_eq!(
        operation_label(Lang::Ru, Operation::Add),
        "Добавление сигналов"
    );
    assert_eq!(
        operation_label(Lang::Ru, Operation::Remove),
        "Удаление сигналов"
    );
    assert_eq!(
        operation_label(Lang::Ru, Operation::SearchNetwork),
        "поиск через LDS/mDNS"
    );
}

#[test]
fn compose_name_for_local_da_uses_progid() {
    // Local DA (empty host): the name is just the ProgID, no host suffix.
    let ep = EndpointSpec::da("", "Alpha.Server");
    assert_eq!(compose_server_name(&ep), "Alpha.Server");
}

#[test]
fn compose_name_for_remote_da_appends_host() {
    // Remote DA appends « — host» so two servers on different PCs differ.
    let ep = EndpointSpec::da("box1", "Alpha.Server");
    assert_eq!(compose_server_name(&ep), "Alpha.Server — box1");
}

#[test]
fn compose_name_for_ua_uses_host_port() {
    let ep = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    assert_eq!(compose_server_name(&ep), "127.0.0.1:4840");
    // The localhost host must not produce a remote suffix.
    let local = EndpointSpec::ua("opc.tcp://localhost:4840");
    assert_eq!(compose_server_name(&local), "localhost:4840");
}

#[test]
fn compose_name_for_remote_ua_appends_host() {
    let ep = EndpointSpec::ua("opc.tcp://192.168.1.50:4840");
    assert_eq!(compose_server_name(&ep), "192.168.1.50:4840");
    // A named remote host (no port) is reported without a redundant suffix.
    let ep2 = EndpointSpec::ua("opc.tcp://plc-east:4840");
    assert_eq!(compose_server_name(&ep2), "plc-east:4840");
}

#[test]
fn compose_name_prefers_advertised_ua_name_without_remote_suffix() {
    // Advertised application name replaces the host:port on a local server.
    let ep = EndpointSpec::ua("opc.tcp://127.0.0.1:4840")
        .with_advertised_name(Some("Prosys OPC UA".to_string()));
    assert_eq!(compose_server_name(&ep), "Prosys OPC UA");
}

#[test]
fn compose_name_appends_host_suffix_to_advertised_remote_names() {
    // Both a remote UA and a remote DA advertised name get « — host».
    let ua = EndpointSpec::ua("opc.tcp://plc-east:4840")
        .with_advertised_name(Some("Prosys OPC UA".to_string()));
    assert_eq!(compose_server_name(&ua), "Prosys OPC UA — plc-east");
    let da = EndpointSpec::da("box1", "Alpha.Server")
        .with_advertised_name(Some("Alpha.Server — Some Vendor".to_string()));
    assert_eq!(
        compose_server_name(&da),
        "Alpha.Server — Some Vendor — box1"
    );
    // A local DA advertised name stays as-is (no host suffix).
    let da_local = EndpointSpec::da("", "Alpha.Server")
        .with_advertised_name(Some("Alpha.Server — Some Vendor".to_string()));
    assert_eq!(compose_server_name(&da_local), "Alpha.Server — Some Vendor");
}

#[test]
fn empty_advertised_name_falls_back_to_address() {
    let ep =
        EndpointSpec::ua("opc.tcp://10.0.0.9:4840").with_advertised_name(Some("   ".to_string()));
    assert_eq!(compose_server_name(&ep), "10.0.0.9:4840");
}

// ---- Recursive «Добавить дочерние» job ----

/// Feed the Browse response for `node_id` using the request id the
/// add-children job actually issued (request-id correlation).
#[track_caller]
fn feed_browse(app: &mut App, session: SessionId, node_id: &str, children: Vec<BrowseNode>) {
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

fn item_node(node_id: &str, name: &str) -> BrowseNode {
    BrowseNode {
        node_id: node_id.to_string(),
        name: name.to_string(),
        kind: NodeKind::Item,
        has_children: false,
    }
}

fn branch_node(node_id: &str, name: &str, has_children: bool) -> BrowseNode {
    BrowseNode {
        node_id: node_id.to_string(),
        name: name.to_string(),
        kind: NodeKind::Branch,
        has_children,
    }
}

/// A branch with NO direct Item children but nested Items one level deeper:
/// the reported failure case for a naive "add children" that only looks at
/// the immediate children.
#[test]
fn add_children_handles_branch_with_only_nested_items() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::da("", "Srv.Test"),
    );
    app.seed_tree_item(
        crate::bff::model::SessionId::new(1).unwrap(),
        None,
        0,
        BrowseNode {
            node_id: "top".to_string(),
            name: "Top".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(crate::bff::model::SessionId::new(1).unwrap(), 1);

    app.add_selected_children();

    // Browse the start branch: it contains only a nested Branch (no Items).
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "top",
        vec![branch_node("mid", "Mid", true)],
    );
    // Browse the nested branch: it contains one Item.
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "mid",
        vec![item_node("i1", "I1")],
    );

    let ids: Vec<&str> = app.rows.iter().map(|r| r.item_id.as_str()).collect();
    assert!(ids.contains(&"i1"), "nested item must be added: {ids:?}");
    assert_eq!(app.rows.len(), 1);
    assert!(app.add_children.is_none(), "job must self-cancel when done");
}

#[test]
fn add_children_walks_three_levels_and_deduplicates() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::da("", "Srv.Test"),
    );
    app.seed_tree_item(
        crate::bff::model::SessionId::new(1).unwrap(),
        None,
        0,
        BrowseNode {
            node_id: "l0".to_string(),
            name: "L0".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(crate::bff::model::SessionId::new(1).unwrap(), 1);
    app.add_selected_children();

    // L0 -> L1 (with direct item) and L2.
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "l0",
        vec![branch_node("l1", "L1", true), branch_node("l2", "L2", true)],
    );
    // L1 -> item + L3.
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "l1",
        vec![item_node("d1", "D1"), branch_node("l3", "L3", true)],
    );
    // L2 -> duplicate d1 + new d2.
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "l2",
        vec![item_node("d1", "D1dup"), item_node("d2", "D2")],
    );
    // L3 -> deep item (3 levels below the start).
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "l3",
        vec![item_node("deep", "Deep")],
    );

    let mut ids: Vec<&str> = app.rows.iter().map(|r| r.item_id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, vec!["d1", "d2", "deep"]);
    assert!(app.add_children.is_none());
}

#[test]
fn add_children_cycles_are_guarded() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::da("", "Srv.Test"),
    );
    app.seed_tree_item(
        crate::bff::model::SessionId::new(1).unwrap(),
        None,
        0,
        BrowseNode {
            node_id: "a".to_string(),
            name: "A".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(crate::bff::model::SessionId::new(1).unwrap(), 1);
    app.add_selected_children();

    // A -> B ; B -> A (cycle) + item.
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "a",
        vec![branch_node("b", "B", true)],
    );
    feed_browse(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "b",
        vec![branch_node("a", "A", true), item_node("x", "X")],
    );
    // The cycle node 'a' must not be re-browsed; the job completes.
    assert!(app.add_children.is_none());
    let ids: Vec<&str> = app.rows.iter().map(|r| r.item_id.as_str()).collect();
    assert_eq!(ids, vec!["x"]);
}

/// A Browse reply for the same node but a different (manual) request id
/// must not be consumed as a page of the add-children job.
#[test]
fn add_children_ignores_unrelated_browse_request_id() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::da("", "Srv.Test"),
    );
    app.seed_tree_item(
        crate::bff::model::SessionId::new(1).unwrap(),
        None,
        0,
        BrowseNode {
            node_id: "a".to_string(),
            name: "A".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(crate::bff::model::SessionId::new(1).unwrap(), 1);
    app.add_selected_children();

    let job_request = app
        .add_children
        .as_ref()
        .and_then(|job| {
            job.in_flight
                .iter()
                .find_map(|(rid, node)| (node == "a").then_some(*rid))
        })
        .expect("job browse in flight");
    // A manual browse of the same node gets its own request id.
    let manual_request = app
        .browse(
            crate::bff::model::SessionId::new(1).unwrap(),
            "a".to_string(),
        )
        .expect("manual browse sent");
    assert_ne!(job_request, manual_request);

    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        manual_request,
        EventKind::Browse {
            node_id: "a".to_string(),
            children: vec![item_node("x", "X")],
        },
    );
    assert!(
        app.rows.is_empty(),
        "manual browse must not advance the job"
    );
    assert!(
        app.add_children.is_some(),
        "job must still await its own reply"
    );

    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        job_request,
        EventKind::Browse {
            node_id: "a".to_string(),
            children: vec![item_node("x", "X")],
        },
    );
    assert_eq!(app.rows.len(), 1, "job reply must add the item");
    assert!(app.add_children.is_none(), "job completes after its reply");
}

#[test]
fn add_children_disconnect_cancels_job() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::da("", "Srv.Test"),
    );
    app.seed_tree_item(
        crate::bff::model::SessionId::new(1).unwrap(),
        None,
        0,
        BrowseNode {
            node_id: "a".to_string(),
            name: "A".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(crate::bff::model::SessionId::new(1).unwrap(), 1);
    app.add_selected_children();
    assert!(app.add_children.is_some());

    // A disconnect during the job cancels it.
    app.disconnect_session(crate::bff::model::SessionId::new(1).unwrap());
    assert!(app.add_children.is_none(), "disconnect must cancel the job");
    // Reconnect through a real correlated Connect before simulating link loss.
    let session = SessionId::new(1).unwrap();
    let endpoint = EndpointSpec::da("", "Srv.Test");
    let connect = app
        .send(
            session,
            Protocol::OpcDa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        connect,
        EventKind::Connected {
            endpoint,
            info: BackendMsg::raw("connected"),
        },
    );
    app.seed_tree_item(
        session,
        None,
        0,
        BrowseNode {
            node_id: "a".to_string(),
            name: "A".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(session, 1);
    app.add_selected_children();
    assert!(app.add_children.is_some());
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Disconnected {
            reason: BackendMsg::raw("lost"),
        },
    );
    assert!(app.add_children.is_none());
}

/// OPC UA Part 4 requires the initial value notification for a new monitored
/// item, but some stacks report only on change and a slow first publish can
/// delay the snapshot. With an active push subscription the periodic Read is
/// suppressed, so Engine schedules a one-shot read for confirmed keys.
#[test]
fn add_items_reply_reads_confirmed_keys_once() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let session = crate::bff::model::SessionId::new(1).unwrap();
    app.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    app.record_dispatches_for_test();
    assert!(app.add_items(
        session,
        vec![
            ("ns=2;s=Ok".to_string(), "Ok".to_string()),
            ("ns=2;s=Bad".to_string(), "Bad".to_string()),
        ],
    ));
    let keys: Vec<ItemKey> = app.rows.iter().map(|row| row.key).collect();
    assert_eq!(keys.len(), 2);
    let add_request = app
        .dispatched_for_test()
        .into_iter()
        .find_map(|command| {
            matches!(command.kind, CommandKind::AddItems { .. })
                .then_some(command.request_id)
                .flatten()
        })
        .unwrap();
    app.on_event(Event {
        owner: session.into(),
        origin: Origin::Reply(add_request),
        kind: EventKind::AddItems {
            results: vec![
                (
                    keys[0],
                    Ok(crate::bff::model::ItemInfo {
                        data_type: None,
                        type_id: None,
                        writable: false,
                    }),
                ),
                (keys[1], Err(BackendMsg::NoConnection)),
            ],
        },
    });
    app.tick_at(Instant::now() + Duration::from_secs(2));
    let reads: Vec<Vec<ItemKey>> = app
        .dispatched_for_test()
        .into_iter()
        .filter_map(|command| match command.kind {
            CommandKind::Read { keys } => Some(keys),
            _ => None,
        })
        .collect();
    assert_eq!(
        reads,
        vec![vec![keys[0]]],
        "only the confirmed key is read once after AddItems"
    );
}

#[test]
fn starting_new_job_replaces_previous_for_same_session() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::da("", "Srv.Test"),
    );
    for node_id in ["a", "b"] {
        app.seed_tree_item(
            crate::bff::model::SessionId::new(1).unwrap(),
            None,
            0,
            BrowseNode {
                node_id: node_id.to_string(),
                name: node_id.to_string(),
                kind: NodeKind::Branch,
                has_children: true,
            },
        );
    }
    app.set_tree_focus(crate::bff::model::SessionId::new(1).unwrap(), 1);
    app.add_selected_children();
    assert_eq!(
        app.add_children.as_ref().map(|j| j.session),
        Some(crate::bff::model::SessionId::new(1).unwrap())
    );
    // Starting another job for the same session replaces the in-flight one.
    app.set_tree_focus(crate::bff::model::SessionId::new(1).unwrap(), 2);
    app.add_selected_children();
    assert!(app.add_children.is_some());
}

#[test]
fn da_search_preserves_host_for_duplicate_progid() {
    use crate::bff::model::ServerEntry;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.da_auto_pending = 2;
    app.dialogs.connect.da_pending = true;
    for host in ["plant-a", "plant-b"] {
        app.on_event_for_test(EventKind::Servers {
            host: host.to_string(),
            servers: vec![ServerEntry {
                id: "Vendor.OPC".to_string(),
                description: "Factory".to_string(),
            }],
        });
    }
    assert_eq!(app.dialogs.connect.da_servers.len(), 2);
    assert_eq!(
        app.dialogs
            .connect
            .da_servers
            .iter()
            .map(|hit| hit.host.as_str())
            .collect::<Vec<_>>(),
        vec!["plant-a", "plant-b"]
    );
    assert!(!app.dialogs.connect.da_pending);
}

#[test]
fn da_search_error_clears_pending_state() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.da_auto_pending = 1;
    app.dialogs.connect.da_pending = true;
    app.on_event_for_test(EventKind::Error {
        operation: Operation::Discover,
        message: BackendMsg::raw("OPCEnum unavailable"),
    });
    assert_eq!(app.dialogs.connect.da_auto_pending, 0);
    assert!(!app.dialogs.connect.da_pending);
    assert!(app.dialogs.connect.error.is_some());
}

#[test]
fn standalone_lds_results_become_visible() {
    use crate::bff::model::UaNetworkInfo;
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.record_dispatches_for_test();
    app.dialogs.connect.ua_lds_host = "127.0.0.1".into();
    app.search_ua_network();
    let UaSearch::Network { request } = app.dialogs.connect.ua_search else {
        panic!("network search")
    };
    app.on_event_for_test_with_request(
        Owner::Discovery,
        request,
        EventKind::UaNetworkFound {
            servers: vec![UaNetworkInfo {
                application_name: "Simulator".to_string(),
                application_uri: "urn:sim".to_string(),
                discovery_urls: vec!["opc.tcp://127.0.0.1:4840".parse().unwrap()],
            }],
        },
    );
    assert_eq!(app.dialogs.connect.ua_search_results.len(), 1);
    assert_eq!(
        app.dialogs.connect.ua_search_results[0].application_uri,
        "urn:sim"
    );
}

#[test]
fn security_validation_applies_to_selected_url_only() {
    use crate::bff::model::{UaEndpointInfo, UaSecurityMode, UaSecurityPolicy};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.ua_url = "opc.tcp://server-a:4840".to_string();
    app.dialogs.connect.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
    app.dialogs.connect.ua_security_mode = UaSecurityMode::SignAndEncrypt;
    app.dialogs.connect.ua_discovered_endpoints = vec![
        UaEndpointInfo {
            url: "opc.tcp://server-a:4840".parse().unwrap(),
            security_policy: crate::bff::model::AdvertisedPolicy::from_wire("None"),
            security_mode: crate::bff::model::AdvertisedMode::from_wire("None"),
        },
        UaEndpointInfo {
            url: "opc.tcp://server-b:4840".parse().unwrap(),
            security_policy: crate::bff::model::AdvertisedPolicy::from_wire("Basic256Sha256"),
            security_mode: crate::bff::model::AdvertisedMode::from_wire("SignAndEncrypt"),
        },
    ];
    assert!(!app.validate_ua_security());
    app.dialogs.connect.ua_url = "opc.tcp://server-b:4840".to_string();
    assert!(app.validate_ua_security());
}

fn start_recorded_auto(app: &mut App) -> (RequestId, RequestId) {
    app.record_dispatches_for_test();
    app.dialogs.connect.ua_lds_host = "127.0.0.1:4850".into();
    app.search_ua_all();
    match app.dialogs.connect.ua_search {
        UaSearch::Auto {
            pending: AutoPending::Both { host, network },
        } => (host, network),
        ref other => panic!("expected two requests, got {other:?}"),
    }
}

#[test]
fn combined_ua_search_preserves_security_variants_at_same_url() {
    use crate::bff::model::{UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let (host, network) = start_recorded_auto(&mut app);
    let servers = [("None", "None"), ("Basic256Sha256", "SignAndEncrypt")]
        .into_iter()
        .map(|(policy, mode)| UaServerInfo {
            application_name: "Simulator".into(),
            application_uri: "urn:sim".into(),
            endpoints: vec![UaEndpointInfo {
                url: "opc.tcp://localhost:4840".parse().unwrap(),
                security_policy: crate::bff::model::AdvertisedPolicy::from_wire(policy),
                security_mode: crate::bff::model::AdvertisedMode::from_wire(mode),
            }],
        })
        .collect();
    app.on_event_for_test_with_request(
        Owner::Discovery,
        host,
        EventKind::UaServersFound {
            host: "localhost".into(),
            port: 4840,
            servers,
        },
    );
    assert_eq!(app.dialogs.connect.ua_search_results.len(), 1);
    assert_eq!(app.dialogs.connect.ua_search_results[0].endpoints.len(), 2);
    app.on_event_for_test_with_request(
        Owner::Discovery,
        network,
        EventKind::UaNetworkFound { servers: vec![] },
    );
    assert_eq!(app.dialogs.connect.ua_search_results[0].endpoints.len(), 2);
}

#[test]
fn combined_ua_search_shows_a_loopback_server_once_while_running() {
    use crate::bff::model::{UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let (host, _) = start_recorded_auto(&mut app);
    let server = UaServerInfo {
        application_name: "Simulator".into(),
        application_uri: "urn:sim".into(),
        endpoints: vec![UaEndpointInfo {
            url: "opc.tcp://127.0.0.1:4840".parse().unwrap(),
            security_policy: crate::bff::model::AdvertisedPolicy::from_wire("None"),
            security_mode: crate::bff::model::AdvertisedMode::from_wire("None"),
        }],
    };
    app.on_event_for_test_with_request(
        Owner::Discovery,
        host,
        EventKind::UaServersFound {
            host: "127.0.0.1".into(),
            port: 4840,
            servers: vec![server.clone(), server],
        },
    );
    assert_eq!(app.dialogs.connect.ua_search.auto_pending_count(), 1);
    assert_eq!(app.dialogs.connect.ua_search_results.len(), 1);
    assert_eq!(app.dialogs.connect.ua_search_results[0].endpoints.len(), 1);
}

#[test]
fn lds_urls_merge_into_host_result_instead_of_being_dropped() {
    use crate::bff::model::{UaEndpointInfo, UaNetworkInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let (host, network) = start_recorded_auto(&mut app);
    app.on_event_for_test_with_request(
        Owner::Discovery,
        host,
        EventKind::UaServersFound {
            host: "127.0.0.1".into(),
            port: 4840,
            servers: vec![UaServerInfo {
                application_name: "Simulator".into(),
                application_uri: "urn:sim".into(),
                endpoints: vec![UaEndpointInfo {
                    url: "opc.tcp://127.0.0.1:4840".parse().unwrap(),
                    security_policy: crate::bff::model::AdvertisedPolicy::from_wire("None"),
                    security_mode: crate::bff::model::AdvertisedMode::from_wire("None"),
                }],
            }],
        },
    );
    app.on_event_for_test_with_request(
        Owner::Discovery,
        network,
        EventKind::UaNetworkFound {
            servers: vec![
                UaNetworkInfo {
                    application_name: "Simulator".into(),
                    application_uri: "urn:sim".into(),
                    discovery_urls: vec!["opc.tcp://10.0.0.5:4840".parse().unwrap()],
                },
                UaNetworkInfo {
                    application_name: "Simulator (LDS)".into(),
                    application_uri: String::new(),
                    discovery_urls: vec!["opc.tcp://127.0.0.1:4840/".parse().unwrap()],
                },
            ],
        },
    );
    let results = &app.dialogs.connect.ua_search_results;
    assert_eq!(results.len(), 1, "one server, not three rows: {results:?}");
    assert_eq!(results[0].application_uri, "urn:sim");
    assert!(
        results[0]
            .endpoints
            .iter()
            .any(|e| e.url.as_str() == "opc.tcp://10.0.0.5:4840")
    );
}

#[test]
fn different_application_uris_are_never_merged_by_url() {
    use crate::bff::model::{UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    for uri in ["urn:a", "urn:b"] {
        super::search::merge_ua_servers(
            &mut app.dialogs.connect,
            vec![UaServerInfo {
                application_name: uri.to_string(),
                application_uri: uri.to_string(),
                endpoints: vec![UaEndpointInfo {
                    url: "opc.tcp://shared:4840".parse().unwrap(),
                    security_policy: crate::bff::model::AdvertisedPolicy::Unspecified,
                    security_mode: crate::bff::model::AdvertisedMode::Unspecified,
                }],
            }],
        );
    }
    assert_eq!(app.dialogs.connect.ua_search_results.len(), 2);
}

// ---- Tree helpers shared by the U6 / U7 / A4 tests ----

fn tree_app() -> (egui::Context, App) {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::ua("opc.tcp://127.0.0.1:4840"),
    );
    (ctx, app)
}

fn tree_names(app: &App) -> Vec<(usize, String)> {
    app.sessions[0]
        .tree
        .iter()
        .map(|t| (t.depth, t.node.name.clone()))
        .collect()
}

fn item_id_named(app: &App, name: &str) -> u64 {
    app.sessions[0]
        .tree
        .iter()
        .find(|t| t.node.name == name)
        .unwrap_or_else(|| panic!("no tree item named {name}"))
        .id
}

// ---- U6: «Представления» replaces only its own children ----

#[test]
fn u6_views_event_keeps_the_rest_of_the_tree() {
    let (_ctx, mut app) = tree_app();
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Browse {
            node_id: String::new(),
            children: vec![
                branch_node("ns=1;s=R1", "R1", true),
                branch_node("ns=1;s=R2", "R2", true),
            ],
        },
    );
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Browse {
            node_id: "ns=1;s=R1".to_string(),
            children: vec![item_node("ns=1;s=C1", "C1")],
        },
    );
    let r1 = item_id_named(&app, "R1");

    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Views {
            children: vec![
                branch_node("ns=0;i=1", "View1", false),
                branch_node("ns=0;i=2", "View2", false),
            ],
        },
    );
    let names: Vec<String> = tree_names(&app).into_iter().map(|(_, n)| n).collect();
    assert_eq!(
        names,
        ["Представления", "View1", "View2", "R1", "C1", "R2"],
        "R1's expanded child must survive the Views event"
    );
    let r1_item = app
        .tree_item(crate::bff::model::SessionId::new(1).unwrap(), r1)
        .expect("R1 still present");
    assert!(r1_item.expanded && r1_item.loaded, "R1 keeps its state");

    // A second Views event replaces only the previous views.
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Views {
            children: vec![branch_node("ns=0;i=3", "View3", false)],
        },
    );
    let names: Vec<String> = tree_names(&app).into_iter().map(|(_, n)| n).collect();
    assert_eq!(names, ["Представления", "View3", "R1", "C1", "R2"]);
}

// ---- U7: replies land under the item that was expanded ----

#[test]
fn u7_browse_reply_goes_to_the_expanded_instance_of_a_duplicated_node_id() {
    let (_ctx, mut app) = tree_app();
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Browse {
            node_id: String::new(),
            children: vec![
                branch_node("ns=1;s=P", "P", true),
                branch_node("ns=1;s=Q", "Q", true),
            ],
        },
    );
    for parent in ["ns=1;s=P", "ns=1;s=Q"] {
        app.on_event_for_test_at(
            crate::bff::model::SessionId::new(1).unwrap(),
            EventKind::Browse {
                node_id: parent.to_string(),
                children: vec![branch_node("ns=1;s=SHARED", "Shared", true)],
            },
        );
    }
    let shared: Vec<u64> = app.sessions[0]
        .tree
        .iter()
        .filter(|t| t.node.name == "Shared")
        .map(|t| t.id)
        .collect();
    assert_eq!(shared.len(), 2, "same NodeId appears under P and Q");

    // The user expands the SECOND instance (under Q).
    let request = app
        .browse_item(crate::bff::model::SessionId::new(1).unwrap(), shared[1])
        .expect("dispatched");
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::Browse {
            node_id: "ns=1;s=SHARED".to_string(),
            children: vec![item_node("ns=1;s=LEAF", "Leaf")],
        },
    );
    let names = tree_names(&app);
    let leaf_pos = names.iter().position(|(_, n)| n == "Leaf").expect("leaf");
    let second_pos = app.sessions[0]
        .tree
        .iter()
        .position(|t| t.id == shared[1])
        .expect("second instance");
    assert_eq!(leaf_pos, second_pos + 1, "leaf is under the 2nd instance");
    let first = app
        .tree_item(crate::bff::model::SessionId::new(1).unwrap(), shared[0])
        .expect("first instance");
    assert!(!first.loaded && !first.expanded, "1st instance untouched");
    let second = app
        .tree_item(crate::bff::model::SessionId::new(1).unwrap(), shared[1])
        .expect("second instance");
    assert!(second.loaded && second.expanded && !second.loading);
}

#[test]
fn u7_reply_for_a_removed_item_is_dropped() {
    let (_ctx, mut app) = tree_app();
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Browse {
            node_id: String::new(),
            children: vec![branch_node("ns=1;s=P", "P", true)],
        },
    );
    let p = item_id_named(&app, "P");
    let request = app
        .browse_item(crate::bff::model::SessionId::new(1).unwrap(), p)
        .expect("dispatched");
    // The tree is re-read meanwhile: the old item ids are gone.
    app.reread_tree(crate::bff::model::SessionId::new(1).unwrap());
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::Browse {
            node_id: "ns=1;s=P".to_string(),
            children: vec![item_node("ns=1;s=LEAF", "Leaf")],
        },
    );
    assert!(
        app.sessions[0].tree.iter().all(|t| t.node.name != "Leaf"),
        "a reply for a dead item must not attach anywhere"
    );
}

// ---- U5 (app side): toggling «Только переменные» re-reads the tree ----

#[test]
fn u5_toggling_vars_only_rereads_the_tree_from_the_root() {
    let (_ctx, mut app) = tree_app();
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Browse {
            node_id: String::new(),
            children: vec![branch_node("ns=1;s=P", "P", true)],
        },
    );
    assert_eq!(app.sessions[0].tree.len(), 1);
    app.set_ua_browse_vars_only(crate::bff::model::SessionId::new(1).unwrap(), true);
    let view = &app.sessions[0];
    assert_eq!(
        view.ua().unwrap().ua_browse_filter,
        crate::bff::model::NodeClassFilter::Variables
    );
    assert!(view.tree.is_empty(), "stale tree dropped");
    assert!(view.root_loading, "root browse issued");
    assert_eq!(
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())
            .len(),
        1
    );
    // Setting the same value again must not browse again.
    app.set_ua_browse_vars_only(crate::bff::model::SessionId::new(1).unwrap(), true);
    assert_eq!(
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())
            .len(),
        1
    );
}

// ---- U11: URL host parsing ----

#[test]
fn ua_url_userinfo_is_rejected_before_journal_or_session() {
    let (_ctx, mut app) = tree_app();
    let before = app.sessions.len();
    assert!(!app.connect_endpoint(EndpointSpec::ua("opc.tcp://user:secret@host:4840")));
    assert_eq!(app.sessions.len(), before);
    assert!(
        app.journal
            .entries()
            .iter()
            .all(|entry| !entry.render(Lang::Ru).contains("secret"))
    );
}

#[test]
fn u11_ipv6_url_host_and_port_are_parsed() {
    use super::present::ua_url_host;
    assert_eq!(ua_url_host("opc.tcp://[::1]:4840").as_deref(), Some("::1"));
    assert_eq!(
        ua_url_host("opc.tcp://[2001:db8::7]:4840/path").as_deref(),
        Some("2001:db8::7")
    );
    assert_eq!(ua_url_host("opc.tcp://[::1]").as_deref(), Some("::1"));
    assert_eq!(ua_url_host("opc.tcp://user@plant:4840/x").as_deref(), None);
    assert_eq!(ua_url_host("opc.tcp://:4840"), None);
}

#[test]
fn u11_server_name_brackets_ipv6_and_treats_loopback_as_local() {
    let local = EndpointSpec::ua("opc.tcp://[::1]:4840");
    assert_eq!(compose_server_name(&local), "[::1]:4840");
    let advertised_local =
        EndpointSpec::ua("opc.tcp://[::1]:4840").with_advertised_name(Some("Sim".into()));
    assert_eq!(compose_server_name(&advertised_local), "Sim");
    let remote =
        EndpointSpec::ua("opc.tcp://[2001:db8::7]:4840").with_advertised_name(Some("Sim".into()));
    assert_eq!(compose_server_name(&remote), "Sim — 2001:db8::7");
    // Any 127.x.x.x address is this machine.
    let loop2 =
        EndpointSpec::ua("opc.tcp://127.0.0.2:4840").with_advertised_name(Some("Sim".into()));
    assert_eq!(compose_server_name(&loop2), "Sim");
}

#[test]
fn u11_loopback_detection_covers_the_whole_127_block_and_v6() {
    use super::present::is_loopback_host;
    for host in [
        "localhost",
        "LOCALHOST",
        "127.0.0.1",
        "127.0.0.2",
        "127.255.255.254",
        "::1",
        "[::1]",
        "::ffff:127.0.0.1",
    ] {
        assert!(is_loopback_host(host), "{host} is loopback");
    }
    for host in [
        "128.0.0.1",
        "10.0.0.1",
        "127.0.0.1.evil.example",
        "plant",
        "",
    ] {
        assert!(!is_loopback_host(host), "{host} is not loopback");
    }
}

// ---- D4 / D5: DA discovery routes ----

fn pend_discover(app: &mut App, request_id: RequestId, host: &str) {
    app.engine.insert_pending_for_test(
        crate::bff::model::Owner::Discovery,
        request_id,
        PendingRequest::new(
            Operation::Discover,
            Instant::now(),
            PendingCtx::Host(host.to_string()),
        ),
    );
}

#[test]
fn d4_discover_error_releases_only_the_failing_hosts_route() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.engine
        .seed_da_route_for_test(
            crate::bff::model::Owner::Discovery,
            RequestId::new(501).unwrap(),
        )
        .unwrap();
    app.engine
        .seed_da_route_for_test(
            crate::bff::model::Owner::Discovery,
            RequestId::new(502).unwrap(),
        )
        .unwrap();
    pend_discover(&mut app, RequestId::new(501).unwrap(), "pc-a");
    pend_discover(&mut app, RequestId::new(502).unwrap(), "pc-b");
    app.on_event_for_test_with_request(
        crate::bff::model::Owner::Discovery,
        RequestId::new(501).unwrap(),
        EventKind::Error {
            operation: Operation::Discover,
            message: BackendMsg::raw("RPC unavailable"),
        },
    );
    assert!(
        !app.engine
            .has_discovery_route_for_test(RequestId::new(501).unwrap())
    );
    assert!(
        app.engine
            .has_discovery_route_for_test(RequestId::new(502).unwrap()),
        "the other search keeps its route"
    );
}

#[test]
fn d5_closing_the_connect_dialog_releases_discovery_routes() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.engine
        .seed_da_route_for_test(
            crate::bff::model::Owner::Discovery,
            RequestId::new(501).unwrap(),
        )
        .unwrap();
    app.engine
        .seed_da_route_for_test(
            crate::bff::model::Owner::Discovery,
            RequestId::new(502).unwrap(),
        )
        .unwrap();
    app.engine
        .seed_da_route_for_test(
            crate::bff::model::SessionId::new(9).unwrap(),
            RequestId::FIRST,
        )
        .unwrap();
    app.close_connect_dialog();
    assert!(
        !app.engine
            .has_discovery_route_for_test(RequestId::new(501).unwrap())
    );
    assert!(
        !app.engine
            .has_discovery_route_for_test(RequestId::new(502).unwrap())
    );
    assert!(
        app.engine
            .has_da_route(crate::bff::model::SessionId::new(9).unwrap())
    );
}

// ---- U14: «Добавить по имени» waits for the resolution ----

fn open_add_by_name(app: &mut App, session: SessionId, path: &str) {
    app.dialogs.add_signal.open = true;
    app.dialogs.add_signal.session = Some(session);
    app.dialogs.add_signal.item_id = path.to_string();
    app.add_by_name();
}

#[test]
fn u14_dialog_stays_open_until_the_path_is_resolved() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "0:Objects/2:Sim/2:Tag1",
    );
    let (session, request) = app.dialogs.add_signal.resolving.expect("waiting");
    assert_eq!(session, crate::bff::model::SessionId::new(1).unwrap());
    assert!(app.dialogs.add_signal.open, "dialog must stay open");
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::PathResolved {
            targets: vec!["ns=2;s=Tag1".to_string()],
        },
    );
    assert!(!app.dialogs.add_signal.open, "closed after the answer");
    assert_eq!(app.dialogs.add_signal.resolving, None);
    assert!(app.rows.iter().any(|r| r.item_id == "ns=2;s=Tag1"));
}

#[test]
fn u14_resolve_error_is_shown_in_the_dialog_and_frees_pending() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "0:Objects/2:Nope",
    );
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::Error {
            operation: Operation::ResolveBrowsePath,
            message: BackendMsg::raw("BadNoMatch"),
        },
    );
    assert!(app.dialogs.add_signal.open);
    assert_eq!(app.dialogs.add_signal.resolving, None);
    assert_eq!(app.dialogs.add_signal.error.as_deref(), Some("BadNoMatch"));
    assert!(app.rows.is_empty());
}

#[test]
fn u14_empty_target_list_reports_name_not_resolved_in_the_dialog() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "0:Objects/2:Nope",
    );
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::PathResolved {
            targets: Vec::new(),
        },
    );
    assert!(app.dialogs.add_signal.open);
    assert!(app.dialogs.add_signal.error.is_some());
    assert_eq!(app.dialogs.add_signal.resolving, None);
}

#[test]
fn u14_another_sessions_answer_is_not_consumed() {
    let (_ctx, mut app) = tree_app();
    app.seed_session(
        crate::bff::model::SessionId::new(2).unwrap(),
        EndpointSpec::ua("opc.tcp://127.0.0.1:4841"),
    );
    open_add_by_name(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "0:Objects/2:Tag1",
    );
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    // A reply with the same request id but from session 2 is unsolicited for
    // this dialog: it is dropped and the wait continues.
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(2).unwrap(),
        request,
        EventKind::PathResolved {
            targets: vec!["ns=2;s=Other".to_string()],
        },
    );
    assert!(app.rows.is_empty());
    assert_eq!(
        app.dialogs.add_signal.resolving,
        Some((crate::bff::model::SessionId::new(1).unwrap(), request))
    );
}

#[test]
fn u14_cancelled_dialog_ignores_the_late_answer() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "0:Objects/2:Tag1",
    );
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    app.close_add_signal_dialog();
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::PathResolved {
            targets: vec!["ns=2;s=Tag1".to_string()],
        },
    );
    assert!(app.rows.is_empty(), "a cancelled dialog adds nothing");
}

#[test]
fn u14_disconnect_frees_a_waiting_dialog_with_an_error() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "0:Objects/2:Tag1",
    );
    app.disconnect_session(crate::bff::model::SessionId::new(1).unwrap());
    assert_eq!(app.dialogs.add_signal.resolving, None);
    assert!(app.dialogs.add_signal.error.is_some());
}

// ---- A1: CJK font candidates ----

#[test]
fn a1_cjk_candidates_cover_windows_fonts_and_honor_the_override() {
    use super::present::cjk_candidates_with;
    let names: Vec<String> = cjk_candidates_with(None)
        .iter()
        .map(|f| f.path.display().to_string())
        .collect();
    assert!(names.iter().any(|p| p.ends_with("msyh.ttc")), "{names:?}");
    assert!(names.iter().any(|p| p.ends_with("simsun.ttc")), "{names:?}");

    let list = cjk_candidates_with(Some("/fonts/My.ttc#3".to_string()));
    assert_eq!(list[0].path.display().to_string(), "/fonts/My.ttc");
    assert_eq!(list[0].index, 3);
    let plain = cjk_candidates_with(Some("/fonts/Plain.ttf".to_string()));
    assert_eq!(
        (plain[0].index, plain[0].path.display().to_string()),
        (0, "/fonts/Plain.ttf".to_string())
    );
    let bad_face = cjk_candidates_with(Some("/fonts/X.ttc#abc".to_string()));
    assert_eq!(
        bad_face[0].index, 0,
        "an unparsable face index falls back to 0"
    );
}

#[test]
fn a1_build_fonts_appends_the_cjk_face_as_a_fallback_only() {
    use super::present::{FontFile, build_fonts};
    // A readable file stands in for the font (the bytes are not parsed here).
    let dir = std::env::temp_dir().join(format!("opclook-fonts-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let fake = dir.join("fake.ttc");
    std::fs::write(&fake, b"not a real font").expect("write fake font");
    let missing = dir.join("missing.ttc");
    let (fonts, loaded) = build_fonts(
        &[],
        &[],
        &[
            FontFile {
                path: missing,
                index: 0,
            },
            FontFile {
                path: fake.clone(),
                index: 2,
            },
        ],
    );
    assert_eq!(loaded.as_deref(), Some(fake.display().to_string().as_str()));
    let data = fonts.font_data.get("cjk").expect("cjk font registered");
    assert_eq!(data.index, 2);
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let names = fonts.families.get(&family).expect("family");
        assert_eq!(
            names.last().map(String::as_str),
            Some("cjk"),
            "fallback is last"
        );
    }
    // No candidate readable: nothing is registered.
    let (fonts, loaded) = build_fonts(&[], &[], &[]);
    assert!(loaded.is_none() && !fonts.font_data.contains_key("cjk"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn journal_lines(app: &App) -> Vec<String> {
    app.journal
        .entries()
        .iter()
        .map(|e| e.render(Lang::Ru))
        .collect()
}

fn no_reply_lines(app: &App) -> usize {
    journal_lines(app)
        .iter()
        .filter(|l| l.contains("Нет ответа от сервера"))
        .count()
}

#[test]
fn k1_request_timeout_honours_the_setting_with_floors() {
    use super::runtime::request_timeout;
    // Plain operations follow the setting but never go below 5 s.
    assert_eq!(request_timeout(Operation::Read, 60).as_secs(), 60);
    assert_eq!(request_timeout(Operation::Read, 1).as_secs(), 5);
    // Slow operations get at least 45 s.
    assert_eq!(request_timeout(Operation::Connect, 10).as_secs(), 45);
    assert_eq!(request_timeout(Operation::Discover, 10).as_secs(), 45);
    assert_eq!(request_timeout(Operation::Connect, 300).as_secs(), 300);
}

#[test]
fn k1_fresh_requests_do_not_expire() {
    let (_ctx, mut app) = tree_app();
    app.engine.insert_pending_for_test(
        crate::bff::model::SessionId::new(1).unwrap(),
        RequestId::new(501).unwrap(),
        PendingRequest::new(Operation::Browse, Instant::now(), PendingCtx::None),
    );
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 20);
    app.tick_for_test();
    assert_eq!(
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())
            .len(),
        1
    );
    assert_eq!(no_reply_lines(&app), 0);
}

#[test]
fn k1_unanswered_root_browse_stops_the_spinner_and_reports() {
    let (_ctx, mut app) = tree_app();
    app.browse(crate::bff::model::SessionId::new(1).unwrap(), String::new());
    assert!(app.sessions[0].root_loading);
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    assert!(!app.sessions[0].root_loading, "spinner must stop");
    assert!(
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())
            .is_empty()
    );
    assert_eq!(no_reply_lines(&app), 1, "{:?}", journal_lines(&app));
}

#[test]
fn k1_unanswered_node_expand_resets_only_the_loading_flags() {
    let (_ctx, mut app) = tree_app();
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::Browse {
            node_id: String::new(),
            children: vec![branch_node("ns=1;s=P", "P", true)],
        },
    );
    let item = item_id_named(&app, "P");
    app.browse_item(crate::bff::model::SessionId::new(1).unwrap(), item);
    assert!(app.sessions[0].tree[0].loading);
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    assert!(!app.sessions[0].tree[0].loading);
    assert!(!app.sessions[0].tree[0].loaded, "can be expanded again");
}

#[test]
fn k1_unanswered_read_frees_the_poll_slot_once() {
    let (_ctx, mut app) = tree_app();
    app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "Tag1",
        "Tag1",
        Value::I64(1),
    );
    app.sessions[0].read_in_flight = true;
    app.send(
        crate::bff::model::SessionId::new(1).unwrap(),
        Protocol::OpcUa,
        CommandKind::Read {
            keys: vec![app.rows[0].key],
        },
    );
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    assert!(
        !app.sessions[0].read_in_flight
            || app
                .pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())
                .len()
                == 1,
        "the slot is free (a new poll may already be in flight)"
    );
    assert_eq!(no_reply_lines(&app), 1, "one message, not two");
}

#[test]
fn k1_late_reply_after_expiry_is_dropped() {
    let (_ctx, mut app) = tree_app();
    app.browse(crate::bff::model::SessionId::new(1).unwrap(), String::new());
    let request =
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())[0];
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::Browse {
            node_id: String::new(),
            children: vec![branch_node("ns=1;s=P", "P", true)],
        },
    );
    assert!(app.sessions[0].tree.is_empty(), "late answer ignored");
}

#[test]
fn k1_unanswered_connect_from_the_dialog_shows_the_error_there() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let _ = app.connect_from_dialog(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let session = app.sessions.last().expect("session").id;
    assert_eq!(app.dialogs.connect.connecting, Some(session));
    app.age_pending_requests_for_test(session, 3600);
    app.tick_for_test();
    assert_eq!(app.dialogs.connect.connecting, None);
    assert!(
        app.dialogs
            .connect
            .error
            .as_deref()
            .is_some_and(|e| e.contains("Нет ответа"))
    );
    assert_eq!(
        app.sessions.last().unwrap().state,
        SessionState::Disconnected
    );
}

#[test]
fn k1_unanswered_connect_tells_the_worker_to_drop_the_session() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.engine.limit_dispatch_for_test(2);
    assert!(app.connect_from_dialog(EndpointSpec::ua("opc.tcp://127.0.0.1:4840")));
    let session = app.sessions.last().expect("session").id;
    app.age_pending_requests_for_test(session, 3600);
    app.tick_for_test();
    app.tick_for_test();
    let commands = app.engine.dispatched_for_test();
    let disconnects: Vec<_> = commands
        .iter()
        .filter(|command| matches!(command.kind, CommandKind::Disconnect))
        .collect();
    assert_eq!(disconnects.len(), 1);
    assert_eq!(disconnects[0].owner, Owner::Session(session));
    assert_eq!(disconnects[0].request_id, None);
    assert!(app.pending_request_ids_for_test(session).is_empty());
}

#[test]
fn k1_unanswered_resolve_name_unblocks_the_add_signal_dialog() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(
        &mut app,
        crate::bff::model::SessionId::new(1).unwrap(),
        "0:Objects/2:Tag1",
    );
    assert!(app.dialogs.add_signal.resolving.is_some());
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    assert_eq!(app.dialogs.add_signal.resolving, None);
    assert!(app.dialogs.add_signal.open);
    assert!(app.dialogs.add_signal.error.is_some());
}

#[test]
fn k1_unanswered_add_items_leaves_the_waiting_state() {
    let (_ctx, mut app) = tree_app();
    app.add_items(
        crate::bff::model::SessionId::new(1).unwrap(),
        vec![("ns=2;s=Tag1".to_string(), "Tag1".to_string())],
    );
    assert!(app.rows[0].pending);
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    assert!(!app.rows[0].pending, "row must stop waiting");
    assert_eq!(app.rows[0].quality.kind(), QualityKind::Bad);
}

#[test]
fn k1_unanswered_write_completes_the_batch_as_failed() {
    let (_ctx, mut app) = tree_app();
    app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "Tag1",
        "Tag1",
        Value::I64(1),
    );
    let key = app.rows[0].key;
    app.write_batch.pending = 1;
    app.write_batch.total = 1;
    app.send_with(
        crate::bff::model::SessionId::new(1).unwrap(),
        Protocol::OpcUa,
        CommandKind::Write {
            request: WriteRequest {
                key,
                value: Value::I64(2),
                type_hint: None,
            },
        },
        PendingCtx::WriteKey(key),
    );
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    assert_eq!(app.write_batch.pending, 0);
    assert_eq!(app.write_batch.errors, 1);
    assert!(app.write_batch.summary.is_some());
}

#[test]
fn k1_disconnect_and_subscription_expire_silently() {
    let (_ctx, mut app) = tree_app();
    app.send(
        crate::bff::model::SessionId::new(1).unwrap(),
        Protocol::OpcUa,
        CommandKind::Disconnect,
    );
    app.send(
        crate::bff::model::SessionId::new(1).unwrap(),
        Protocol::OpcUa,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 1000,
            deadband: 0.0,
        },
    );
    assert_eq!(
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())
            .len(),
        2
    );
    app.age_pending_requests_for_test(crate::bff::model::SessionId::new(1).unwrap(), 3600);
    app.tick_for_test();
    assert!(
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())
            .is_empty()
    );
    assert_eq!(no_reply_lines(&app), 0);
    assert_eq!(app.sessions[0].state, SessionState::Connected);
}

fn trace_lines(app: &App) -> Vec<String> {
    journal_lines(app)
        .into_iter()
        .filter(|l| l.contains('#'))
        .collect()
}

#[test]
fn k2_trace_is_off_by_default() {
    let (_ctx, mut app) = tree_app();
    app.browse(crate::bff::model::SessionId::new(1).unwrap(), String::new());
    let request =
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())[0];
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::Browse {
            node_id: String::new(),
            children: Vec::new(),
        },
    );
    assert!(trace_lines(&app).is_empty(), "{:?}", journal_lines(&app));
}

#[test]
fn k2_trace_logs_request_reply_latency_and_dropped_replies() {
    let (_ctx, mut app) = tree_app();
    app.settings.view.options.diagnostic_trace = true;
    app.browse(crate::bff::model::SessionId::new(1).unwrap(), String::new());
    let request =
        app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())[0];
    let lines = trace_lines(&app);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(&format!("#{request}")), "{lines:?}");
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::Browse {
            node_id: String::new(),
            children: Vec::new(),
        },
    );
    let lines = trace_lines(&app);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[1].contains("мс"), "latency is shown: {lines:?}");
    // The same reply again has no outstanding request: reported as dropped.
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        request,
        EventKind::Browse {
            node_id: String::new(),
            children: Vec::new(),
        },
    );
    let lines = trace_lines(&app);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[2].contains("отброшен"), "{lines:?}");
}

#[test]
fn a10_decimal_comma_is_a_float_when_the_type_is_unknown() {
    for text in ["3,14", "-0,5", "1.5", "2e3", "1,5e-3"] {
        assert_eq!(
            infer_kind(text, None, None, Protocol::OpcUa),
            WriteKind::F64,
            "{text}"
        );
    }
    for text in ["42", "-7", "inf", "nan", "NaN", "abc", "1,2,3", ","] {
        assert_ne!(
            infer_kind(text, None, None, Protocol::OpcUa),
            WriteKind::F64,
            "{text}"
        );
    }
    assert_eq!(
        infer_kind("42", None, None, Protocol::OpcUa),
        WriteKind::I64
    );
}

fn write_request(key: ItemKey) -> WriteRequest {
    WriteRequest {
        key,
        value: Value::I64(1),
        type_hint: None,
    }
}

fn start_batch(app: &mut App, keys: &[ItemKey]) {
    app.perform_write(PendingWrites {
        session: crate::bff::model::SessionId::new(1).unwrap(),
        epoch: app.engine.connection_id(SessionId::FIRST),
        protocol: Protocol::OpcUa,
        requests: keys.iter().copied().map(write_request).collect(),
        type_label: "Int".to_string(),
    });
}

#[test]
fn write_outcomes_keep_status_and_never_replace_observed_value() {
    use crate::bff::model::WriteOutcome;
    for (code, accepted) in [
        (0x002e_0000, true),
        (0x4000_1234, false),
        (0x8000_0042, false),
    ] {
        let (_ctx, mut app) = tree_app();
        let session = SessionId::FIRST;
        let key = app.seed_row(session, "A", "A", Value::I64(9));
        start_batch(&mut app, &[key]);
        let id = app.pending_request_ids_for_test(session)[0];
        app.on_event_for_test_with_request(
            session,
            id,
            EventKind::Wrote {
                key,
                result: Ok(WriteOutcome::from_ua(code, "status")),
            },
        );
        assert_eq!(app.rows[0].value, Value::I64(9));
        assert_eq!(app.write_batch.ok, usize::from(accepted));
        assert_eq!(app.dialogs.message.is_some(), code == 0x8000_0042);
        assert!(
            app.journal
                .entries()
                .iter()
                .any(|entry| entry.render(app.lang()).contains(&format!("0x{code:08X}")))
        );
    }
}

#[test]
fn write_timeout_is_reported_as_unknown_and_not_retried() {
    let (_ctx, mut app) = tree_app();
    let session = SessionId::FIRST;
    let key = app.seed_row(session, "A", "A", Value::I64(9));
    app.record_dispatches_for_test();
    start_batch(&mut app, &[key]);
    let id = app.pending_request_ids_for_test(session)[0];
    app.on_event_for_test_with_request(
        session,
        id,
        EventKind::Error {
            operation: Operation::Write,
            message: BackendMsg::RequestTimeout { seconds: 5 },
        },
    );
    assert_eq!(app.rows[0].value, Value::I64(9));
    assert_eq!(app.write_batch.ok, 0);
    assert_eq!(
        app.dispatched_for_test()
            .into_iter()
            .filter(|cmd| matches!(cmd.kind, CommandKind::Write { .. }))
            .count(),
        1
    );
    assert!(app.journal.entries().iter().any(|entry| {
        entry
            .render(app.lang())
            .contains(tr(app.lang(), Tr::WriteUnknownStatus))
    }));
    assert!(app.dialogs.message.is_none());
}

#[test]
fn preflight_write_error_is_not_submitted() {
    let (_ctx, mut app) = tree_app();
    let session = SessionId::FIRST;
    let key = app.seed_row(session, "A", "A", Value::I64(9));
    start_batch(&mut app, &[key]);
    let id = app.pending_request_ids_for_test(session)[0];
    app.on_event_for_test_with_request(
        session,
        id,
        EventKind::Wrote {
            key,
            result: Err(BackendMsg::NodeNotSubscribed),
        },
    );
    let dialog = app.dialogs.message.as_ref().unwrap();
    assert!(
        dialog
            .details
            .contains(tr(app.lang(), Tr::WriteNotSubmitted))
    );
    assert_eq!(app.rows[0].value, Value::I64(9));
}

#[test]
fn a11_overlapping_write_batches_add_up() {
    let (_ctx, mut app) = tree_app();
    let a = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "A",
        "A",
        Value::I64(0),
    );
    let b = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "B",
        "B",
        Value::I64(0),
    );
    let c = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "C",
        "C",
        Value::I64(0),
    );
    start_batch(&mut app, &[a, b]);
    start_batch(&mut app, &[c]); // starts while the first batch is running
    assert_eq!((app.write_batch.pending, app.write_batch.total), (3, 3));
    let ids = app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap());
    assert_eq!(ids.len(), 3);
    for (index, id) in ids.iter().enumerate() {
        assert!(app.write_batch.summary.is_none(), "not done after {index}");
        let key = [a, b, c][index];
        app.on_event_for_test_with_request(
            crate::bff::model::SessionId::new(1).unwrap(),
            *id,
            EventKind::Wrote {
                key,
                result: Ok(crate::bff::model::WriteOutcome::from_ua(0, "Good")),
            },
        );
    }
    assert_eq!(app.write_batch.pending, 0);
    assert_eq!(
        (
            app.write_batch.ok,
            app.write_batch.errors,
            app.write_batch.total
        ),
        (3, 0, 3)
    );
    assert!(
        app.write_batch
            .summary
            .as_deref()
            .is_some_and(|s| s.contains('3'))
    );
}

#[test]
fn a11_counters_restart_after_a_finished_batch() {
    let (_ctx, mut app) = tree_app();
    let a = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "A",
        "A",
        Value::I64(0),
    );
    start_batch(&mut app, &[a]);
    let id = app.pending_request_ids_for_test(crate::bff::model::SessionId::new(1).unwrap())[0];
    app.on_event_for_test_with_request(
        crate::bff::model::SessionId::new(1).unwrap(),
        id,
        EventKind::Wrote {
            key: a,
            result: Ok(crate::bff::model::WriteOutcome::from_ua(0, "Good")),
        },
    );
    start_batch(&mut app, &[a]);
    assert_eq!(
        (
            app.write_batch.pending,
            app.write_batch.total,
            app.write_batch.ok
        ),
        (1, 1, 0)
    );
}

#[test]
fn a11_writes_lost_with_the_session_complete_the_batch_as_failed() {
    let (_ctx, mut app) = tree_app();
    let a = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "A",
        "A",
        Value::I64(0),
    );
    let b = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "B",
        "B",
        Value::I64(0),
    );
    start_batch(&mut app, &[a, b]);
    app.disconnect_session(crate::bff::model::SessionId::new(1).unwrap());
    assert_eq!(app.write_batch.pending, 0, "no reply can arrive any more");
    assert_eq!((app.write_batch.ok, app.write_batch.errors), (0, 2));
    assert!(app.write_batch.summary.is_some());
}

// ---------------------------------------------------------------------------
// A9: backend messages follow the interface language
// ---------------------------------------------------------------------------

fn has_cyrillic(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0x0400..=0x052F))
}

/// A real connect to a port nobody listens on: before A9 the backend's
/// hardcoded Russian error reached the English and Chinese interface.
#[test]
fn a9_connect_failure_to_an_unreachable_ua_server_is_in_the_interface_language() {
    for lang in [Lang::En, Lang::Zh] {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.set_lang(lang);
        let mut app = App::new_test_with_settings(&ctx, settings);
        let journal_before = app.journal.entries().len();
        assert!(app.connect_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:1")));
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            app.poll_events();
            let settled = app
                .sessions
                .first()
                .is_some_and(|view| view.state != SessionState::Connecting);
            if settled {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{lang:?}: the connect attempt did not finish"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let view = app.sessions.first().expect("the session tab");
        assert_eq!(view.state, SessionState::Disconnected);
        assert!(!view.info.trim().is_empty(), "{lang:?}: empty error text");
        assert!(
            !has_cyrillic(&view.info),
            "{lang:?}: Russian text in the session info: {}",
            view.info
        );
        let lines: Vec<String> = app
            .journal
            .entries()
            .iter()
            .skip(journal_before)
            .map(|entry| entry.render(lang))
            .collect();
        assert!(
            lines.iter().any(|line| line.contains(&view.info)),
            "{lang:?}: the error is in the journal: {lines:?}"
        );
        for line in &lines {
            assert!(
                !has_cyrillic(line),
                "{lang:?}: Russian text in the journal line: {line}"
            );
        }
    }
}

/// Every backend message kind is shown in the interface language when it
/// arrives as an event (the application, not the worker, picks the wording).
#[test]
fn a9_backend_events_are_rendered_in_the_interface_language() {
    let ctx = egui::Context::default();
    let mut settings = Settings::default();
    settings.set_lang(Lang::En);
    let mut app = App::new_test_with_settings(&ctx, settings);
    let before = app.journal.entries().len();
    app.on_event_for_test(EventKind::Log {
        message: BackendMsg::MdnsNotSupported,
    });
    app.on_event_for_test(EventKind::Error {
        operation: Operation::Test,
        message: BackendMsg::NoConnection,
    });
    let lines: Vec<String> = app
        .journal
        .entries()
        .iter()
        .skip(before)
        .map(|entry| entry.render(Lang::En))
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("mDNS: the search is not supported")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("no connection")),
        "{lines:?}"
    );
    assert!(lines.iter().all(|line| !has_cyrillic(line)), "{lines:?}");
}

// ---- U15: one loopback request, optional LDS host ----

#[test]
fn u15_lds_host_text_is_parsed_with_the_default_port() {
    use super::search::parse_lds_host;
    assert_eq!(
        parse_lds_host("aaaaaaaaaé"),
        Ok(Some(("aaaaaaaaaé".to_string(), 4840)))
    );
    assert_eq!(parse_lds_host(""), Ok(None));
    assert_eq!(parse_lds_host("   "), Ok(None));
    assert_eq!(
        parse_lds_host("lds.plant.local"),
        Ok(Some(("lds.plant.local".to_string(), 4840)))
    );
    assert_eq!(
        parse_lds_host(" 10.0.0.5:4850 "),
        Ok(Some(("10.0.0.5".to_string(), 4850)))
    );
    assert_eq!(
        parse_lds_host("opc.tcp://lds:4841/"),
        Ok(Some(("lds".to_string(), 4841)))
    );
    assert_eq!(
        parse_lds_host("[fe80::1]:4850"),
        Ok(Some(("fe80::1".to_string(), 4850)))
    );
    assert_eq!(parse_lds_host("::1"), Ok(Some(("::1".to_string(), 4840))));
    for bad in [
        "host:",
        "host:0",
        "host:99999",
        "host:abc",
        "a b",
        "[::1",
        "ho/st",
        ":4840",
    ] {
        assert_eq!(
            parse_lds_host(bad),
            Err(super::search::LdsInputError::InvalidHost),
            "{bad:?}"
        );
    }
}

#[test]
fn u15_the_lds_target_builds_its_url() {
    use crate::bff::model::LdsTarget;
    use crate::bff::net::NetworkBinding;
    let target = |host: &str| LdsTarget {
        host: host.to_string(),
        port: 4850,
        bind: NetworkBinding::loopback(),
    };
    assert_eq!(target("lds").url(), "opc.tcp://lds:4850");
    assert_eq!(target("fe80::1").url(), "opc.tcp://[fe80::1]:4850");
}

#[test]
fn u15_the_combined_search_sends_exactly_one_loopback_request() {
    use super::search::ua_all_commands;
    use crate::bff::model::{CommandKind, LdsTarget};
    use crate::bff::net::NetworkBinding;
    let adapter = NetworkBinding::new("192.168.1.21".parse().unwrap(), None, None, None);
    let lds = LdsTarget {
        host: "lds".to_string(),
        port: 4840,
        bind: adapter.clone(),
    };
    let loopback_requests = |commands: &[CommandKind]| {
        commands
            .iter()
            .filter(|c| matches!(c, CommandKind::SearchUaServers { .. }))
            .count()
    };
    for (bind, lds, expected) in [
        (None, None, 1),
        (Some(adapter.clone()), None, 2),
        (None, Some(lds.clone()), 2),
        (Some(adapter.clone()), Some(lds.clone()), 2),
    ] {
        let commands = ua_all_commands(bind, lds);
        assert_eq!(commands.len(), expected, "{commands:?}");
        assert_eq!(loopback_requests(&commands), 1, "{commands:?}");
        for command in &commands {
            if let CommandKind::SearchUaServers { host, port, bind } = command {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(*port, 4840);
                assert!(bind.as_ref().is_some_and(NetworkBinding::is_loopback));
            }
        }
    }
}

#[test]
fn u15_the_combined_search_counts_one_pending_step_per_request() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.search_ua_all();
    let d = &app.dialogs.connect;
    assert!(d.ua_search.host_searching());
    assert!(!d.ua_search.network_searching(), "no adapter, no LDS host");
    assert_eq!(d.ua_search.auto_pending_count(), 1);

    app.dialogs.connect.ua_lds_host = "127.0.0.1:4850".to_string();
    app.search_ua_all();
    let d = &app.dialogs.connect;
    assert!(
        d.ua_search.network_searching(),
        "a named LDS is asked without an adapter"
    );
    assert_eq!(d.ua_search.auto_pending_count(), 2);
}

#[test]
fn u15_the_lds_host_is_remembered_and_restored() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.ua_lds_host = " localhost:4850 ".to_string();
    app.search_ua_network();
    assert!(app.dialogs.connect.ua_search.network_searching());
    assert!(app.dialogs.connect.error.is_none());
    assert_eq!(app.settings.ua.ua_lds_host, "localhost:4850");

    app.close_connect_dialog();
    app.open_connect_dialog();
    assert_eq!(app.dialogs.connect.ua_lds_host, "localhost:4850");

    // Clearing the field forgets it.
    app.dialogs.connect.ua_lds_host.clear();
    app.search_ua_network();
    assert_eq!(app.settings.ua.ua_lds_host, "");
}

#[test]
fn u15_a_remote_lds_host_needs_an_adapter_and_a_bad_text_is_reported() {
    let ctx = egui::Context::default();
    let mut settings = Settings::default();
    settings.set_lang(Lang::En);
    let mut app = App::new_test_with_settings(&ctx, settings);
    app.open_connect_dialog();

    app.dialogs.connect.ua_lds_host = "lds.plant.local".to_string();
    app.search_ua_network();
    assert!(!app.dialogs.connect.ua_search.network_searching());
    assert_eq!(
        app.dialogs.connect.error.as_deref(),
        Some("Select a network adapter")
    );

    app.dialogs.connect.ua_lds_host = "host:0".to_string();
    app.search_ua_network();
    assert!(!app.dialogs.connect.ua_search.network_searching());
    let error = app.dialogs.connect.error.clone().unwrap_or_default();
    assert!(error.contains("LDS host"), "{error}");
    assert_eq!(
        app.settings.ua.ua_lds_host, "",
        "an unusable text is not saved"
    );

    // Empty field and no adapter: the old requirement stays.
    app.dialogs.connect.ua_lds_host.clear();
    app.search_ua_network();
    assert_eq!(
        app.dialogs.connect.error.as_deref(),
        Some("Select a network adapter")
    );
}

#[test]
fn u15_lds_texts_exist_in_every_language() {
    use crate::i18n::{Tr, tr};
    for key in [
        Tr::LdsHostLabel,
        Tr::LdsHostHint,
        Tr::LdsHostTip,
        Tr::LdsHostInvalid,
        Tr::SearchingUaLds,
    ] {
        let ru = tr(Lang::Ru, key);
        let en = tr(Lang::En, key);
        let zh = tr(Lang::Zh, key);
        assert!(!ru.is_empty() && !en.is_empty() && !zh.is_empty());
        assert!(ru != en && en != zh);
    }
}

/// P1: with an active UA push subscription the app does not poll, except the
/// items the server would not take on any subscription (poll-only).
#[test]
fn p1_poll_only_rows_are_polled_while_push_is_active() {
    let (_ctx, mut app) = tree_app();
    app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "ns=2;s=A",
        "A",
        Value::I64(1),
    );
    let b = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "ns=2;s=B",
        "B",
        Value::I64(2),
    );
    app.sessions[0].root_loading = false;
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::SubscriptionState {
            state: SubscriptionState::Active,
        },
    );
    app.sessions[0].last_poll = Instant::now() - Duration::from_secs(3600);
    app.tick_for_test();
    assert!(
        !app.sessions[0].read_in_flight,
        "everything is pushed: no Read"
    );

    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::PollOnly { keys: vec![b] },
    );
    assert!(app.rows.iter().any(|r| r.key == b && r.poll_only));
    app.tick_for_test();
    assert!(app.sessions[0].read_in_flight, "the poll-only row is read");
}

/// P1: a reconnect lets the new session decide again what it can push.
#[test]
fn p1_reconnect_clears_poll_only() {
    let (_ctx, mut app) = tree_app();
    let b = app.seed_row(
        crate::bff::model::SessionId::new(1).unwrap(),
        "ns=2;s=B",
        "B",
        Value::I64(2),
    );
    app.on_event_for_test_at(
        crate::bff::model::SessionId::new(1).unwrap(),
        EventKind::PollOnly { keys: vec![b] },
    );
    app.register_rows_for_test(crate::bff::model::SessionId::new(1).unwrap());
    assert!(app.rows.iter().all(|r| !r.poll_only));
}

#[test]
fn remove_active_session_activates_nearest_right_neighbor() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    for id in 1..=4 {
        app.seed_session(
            crate::bff::model::SessionId::new(id).unwrap(),
            EndpointSpec::da("", &format!("Srv{id}")),
        );
    }
    app.active_session = Some(crate::bff::model::SessionId::new(2).unwrap());
    app.remove_session(crate::bff::model::SessionId::new(2).unwrap());
    assert_eq!(
        app.active_session,
        Some(crate::bff::model::SessionId::new(3).unwrap())
    );
}

#[test]
fn session_ids_stop_after_maximum_and_report_exhaustion() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.next_session = Some(crate::bff::model::SessionId::new(u64::MAX).unwrap());
    assert!(app.connect_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4840")));
    assert_eq!(
        app.sessions.last().unwrap().id,
        crate::bff::model::SessionId::new(u64::MAX).unwrap()
    );
    assert!(app.next_session.is_none());
    let count = app.sessions.len();
    assert!(!app.connect_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4841")));
    assert_eq!(app.sessions.len(), count);
    assert_eq!(app.status, tr(app.lang(), Tr::SessionIdsExhausted));
}

#[test]
fn item_ids_stop_after_maximum_and_keep_earlier_items() {
    let (_ctx, mut app) = tree_app();
    assert!(ItemKey::new(0).is_none());
    app.next_key = ItemKey::new(u32::MAX);
    app.add_items(
        crate::bff::model::SessionId::new(1).unwrap(),
        vec![("one".into(), "one".into()), ("two".into(), "two".into())],
    );
    assert_eq!(app.rows.len(), 1);
    assert_eq!(app.rows[0].key.get(), u32::MAX);
    assert!(app.next_key.is_none());
    assert_eq!(app.status, tr(app.lang(), Tr::ItemIdsExhausted));
    app.add_items(
        crate::bff::model::SessionId::new(1).unwrap(),
        vec![("three".into(), "three".into())],
    );
    assert_eq!(app.rows.len(), 1);
}

#[test]
fn partial_write_then_closed_worker_finishes_batch() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(
        crate::bff::model::SessionId::new(1).unwrap(),
        EndpointSpec::ua("opc.tcp://127.0.0.1:4840"),
    );
    app.engine.limit_dispatch_for_test(1);
    app.perform_write(PendingWrites {
        session: crate::bff::model::SessionId::new(1).unwrap(),
        epoch: app.engine.connection_id(SessionId::FIRST),
        protocol: Protocol::OpcUa,
        requests: vec![
            WriteRequest {
                key: ItemKey::new(1).unwrap(),
                value: Value::Bool(true),
                type_hint: None,
            },
            WriteRequest {
                key: ItemKey::new(2).unwrap(),
                value: Value::Bool(true),
                type_hint: None,
            },
        ],
        type_label: String::new(),
    });
    assert_eq!(app.write_batch.pending, 0);
    assert_eq!(app.write_batch.total, 1);
    assert_eq!(app.write_batch.errors, 1);
    assert!(app.write_batch.summary.is_some());
}

#[test]
fn closed_worker_marks_link_lost_and_finishes_pending_write() {
    let (_ctx, mut app) = tree_app();
    let session = SessionId::new(1).unwrap();
    app.seed_row(session, "Tag1", "Tag1", Value::I64(1));
    let key = app.rows[0].key;
    app.seed_tree_item(session, None, 0, branch_node("node", "node", true));
    app.sessions[0].tree[0].loading = true;
    app.sessions[0].root_loading = true;
    app.sessions[0].read_in_flight = true;
    app.sessions[0].ua_mut().unwrap().subscription = SubscriptionState::Active;
    app.engine.limit_dispatch_for_test(1);
    app.write_batch.pending = 1;
    app.write_batch.total = 1;
    let request = app.send_with(
        session,
        Protocol::OpcUa,
        CommandKind::Write {
            request: WriteRequest {
                key,
                value: Value::I64(2),
                type_hint: None,
            },
        },
        PendingCtx::WriteKey(key),
    );
    assert!(request.is_some());
    assert!(
        app.send(
            session,
            Protocol::OpcUa,
            CommandKind::Read { keys: vec![key] }
        )
        .is_none()
    );
    let view = &app.sessions[0];
    assert_eq!(view.state, SessionState::Disconnected);
    assert_eq!(view.reconnect, ReconnectState::Stopped);
    assert!(!view.read_in_flight && !view.root_loading && !view.tree[0].loading);
    assert_eq!(view.ua().unwrap().subscription, SubscriptionState::Inactive);
    assert_eq!(app.rows[0].quality.kind(), QualityKind::Bad);
    assert!(app.pending_request_ids_for_test(session).is_empty());
    assert_eq!((app.write_batch.pending, app.write_batch.errors), (0, 1));
    assert!(app.write_batch.summary.is_some());
}

#[test]
fn old_disconnect_preserves_new_connection_write_confirmation() {
    let (_ctx, mut app) = tree_app();
    let session = SessionId::FIRST;
    let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    app.engine.limit_dispatch_for_test(usize::MAX);
    let a = app
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        a,
        EventKind::Connected {
            endpoint: endpoint.clone(),
            info: BackendMsg::raw("A"),
        },
    );
    let disconnect = app
        .send(session, Protocol::OpcUa, CommandKind::Disconnect)
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        disconnect,
        EventKind::Disconnected {
            reason: BackendMsg::ConnectionClosed,
        },
    );
    let b = app
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        b,
        EventKind::Connected {
            endpoint,
            info: BackendMsg::raw("B"),
        },
    );
    app.dialogs.confirm = Some(ConfirmRequest {
        title: String::new(),
        text: String::new(),
        focus_pending: true,
        action: ConfirmAction::Write(PendingWrites {
            session,
            epoch: Some(b),
            protocol: Protocol::OpcUa,
            requests: vec![],
            type_label: String::new(),
        }),
    });
    app.sync_fixture_sessions();
    assert_eq!(app.engine.connection_id_for_test(session), Some(b));
    let lost = |id| crate::bff::model::Event {
        owner: session.into(),
        origin: crate::bff::model::Origin::Session(id),
        kind: EventKind::Disconnected {
            reason: BackendMsg::ConnectionClosed,
        },
    };
    app.on_event(lost(a));
    assert!(app.dialogs.confirm.is_some());
    assert!(app.dialogs.write.error.is_none());
    app.on_event(lost(b));
    assert!(app.dialogs.confirm.is_none());
    assert_eq!(
        app.dialogs.write.error.as_deref(),
        Some(tr(app.lang(), Tr::ActionWriteConnectionChanged))
    );
}

#[test]
fn stale_connection_push_does_not_change_current_row() {
    let (_ctx, mut app) = tree_app();
    let session = SessionId::new(1).unwrap();
    let key = app.seed_row(session, "tag", "tag", Value::I64(1));
    app.sync_fixture_sessions();
    let event = |origin, value| crate::bff::model::Event {
        owner: session.into(),
        origin,
        kind: EventKind::Values {
            values: vec![(
                key,
                crate::bff::model::DataValue {
                    value: Value::I64(value),
                    quality: crate::bff::model::Quality::from_da(0xc0),
                    timestamp: None,
                    reason: None,
                },
            )],
        },
    };
    app.on_event(event(
        crate::bff::model::Origin::Session(RequestId::new(2).unwrap()),
        99,
    ));
    assert_eq!(app.rows[0].value, Value::I64(1));
    app.on_event(event(
        crate::bff::model::Origin::Session(RequestId::FIRST),
        2,
    ));
    assert_eq!(app.rows[0].value, Value::I64(2));
}

#[test]
fn late_unsolicited_data_cannot_revive_a_lost_or_connecting_link() {
    let (_ctx, mut app) = tree_app();
    let session = SessionId::new(1).unwrap();
    let key = app.seed_row(session, "tag", "tag", Value::I64(1));
    app.disconnect_session(session);
    for state in [SessionState::Disconnected, SessionState::Connecting] {
        app.sessions[0].state = state;
        app.on_event_for_test_at(
            session,
            EventKind::Values {
                values: vec![(
                    key,
                    crate::bff::model::DataValue {
                        value: Value::I64(99),
                        quality: crate::bff::model::Quality::from_da(0xc0),
                        timestamp: None,
                        reason: None,
                    },
                )],
            },
        );
        app.on_event_for_test_at(
            session,
            EventKind::SubscriptionState {
                state: SubscriptionState::Active,
            },
        );
        app.on_event_for_test_at(session, EventKind::PollOnly { keys: vec![key] });
        assert_eq!(app.rows[0].value, Value::I64(1));
        assert_eq!(app.rows[0].quality.kind(), QualityKind::Bad);
        assert!(!app.rows[0].poll_only);
        assert_eq!(
            app.sessions[0].ua().unwrap().subscription,
            SubscriptionState::Inactive
        );
    }
}

#[test]
fn exhausted_request_does_not_leave_browse_add_or_search_waiting() {
    let (_ctx, mut app) = tree_app();
    app.engine.exhaust_requests_for_test();
    assert!(
        app.browse(crate::bff::model::SessionId::new(1).unwrap(), String::new())
            .is_none()
    );
    assert!(!app.sessions[0].root_loading);
    assert!(!app.add_items(
        crate::bff::model::SessionId::new(1).unwrap(),
        vec![("ns=2;s=Rejected".into(), "Rejected".into())]
    ));
    assert!(!app.rows.iter().any(|row| row.item_id == "ns=2;s=Rejected"));
    app.dialogs.connect.ua_search_host = "localhost".into();
    app.search_ua_servers();
    assert!(!app.dialogs.connect.ua_search.host_searching());
    assert!(app.dialogs.connect.error.is_some());
}

#[test]
fn exhausted_reconnect_preserves_pending_and_existing_state() {
    let (_ctx, mut app) = tree_app();
    app.engine.limit_dispatch_for_test(1);
    let request = app
        .send(
            crate::bff::model::SessionId::new(1).unwrap(),
            Protocol::OpcUa,
            CommandKind::Read {
                keys: vec![ItemKey::FIRST],
            },
        )
        .unwrap();
    app.engine.exhaust_requests_for_test();
    let before = app.sessions[0].state;
    app.reconnect_session(crate::bff::model::SessionId::new(1).unwrap());
    assert_eq!(app.sessions[0].state, before);
    assert!(
        app.engine
            .has_pending(crate::bff::model::SessionId::new(1).unwrap(), request)
    );
}

#[test]
fn unsolicited_values_and_read_error_do_not_clear_pending_read_in_ui() {
    let (_ctx, mut app) = tree_app();
    let session = crate::bff::model::SessionId::FIRST;
    app.sync_fixture_sessions();
    app.engine.limit_dispatch_for_test(1);
    let request = app
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Read {
                keys: vec![ItemKey::FIRST],
            },
        )
        .unwrap();
    app.project_session(session);
    app.on_event_for_test_at(session, EventKind::Values { values: vec![] });
    assert!(app.sessions[0].read_in_flight);
    app.on_event_for_test_at(
        session,
        EventKind::Error {
            operation: Operation::Read,
            message: BackendMsg::raw("push error"),
        },
    );
    assert!(app.sessions[0].read_in_flight);
    assert!(app.engine.has_pending(session, request));
    app.on_event_for_test_with_request(session, request, EventKind::Values { values: vec![] });
    assert!(!app.sessions[0].read_in_flight);
}

#[test]
fn ua_search_replacement_preserves_independent_da_pending_and_rejects_stale_reply() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.record_dispatches_for_test();
    let da = app
        .send(
            Owner::Discovery,
            Protocol::OpcDa,
            CommandKind::Discover {
                host: String::new(),
            },
        )
        .unwrap();
    app.dialogs.connect.ua_search_host = "localhost".into();
    app.search_ua_servers();
    let UaSearch::SingleHost { request: old } = app.dialogs.connect.ua_search else {
        panic!("single search")
    };
    app.search_ua_servers();
    let UaSearch::SingleHost { request: current } = app.dialogs.connect.ua_search else {
        panic!("replacement search")
    };
    let pending = app.pending_request_ids_for_test(Owner::Discovery);
    assert!(pending.contains(&da));
    assert!(pending.contains(&current));
    assert!(!pending.contains(&old));
    app.on_event_for_test_with_request(
        Owner::Discovery,
        old,
        EventKind::UaServersFound {
            host: "localhost".into(),
            port: 4840,
            servers: vec![crate::bff::model::UaServerInfo {
                application_name: "stale".into(),
                application_uri: "urn:stale".into(),
                endpoints: vec![],
            }],
        },
    );
    assert!(app.dialogs.connect.ua_search_results.is_empty());
    assert_eq!(
        app.dialogs.connect.ua_search,
        UaSearch::SingleHost { request: current }
    );
    app.on_event_for_test_with_request(
        Owner::Discovery,
        current,
        EventKind::UaServersFound {
            host: "localhost".into(),
            port: 4840,
            servers: vec![],
        },
    );
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
    assert!(
        app.pending_request_ids_for_test(Owner::Discovery)
            .contains(&da)
    );
}

#[test]
fn ua_auto_partial_dispatch_failure_finishes_after_accepted_reply() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.engine.limit_dispatch_for_test(1);
    app.dialogs.connect.ua_lds_host = "localhost".into();
    app.search_ua_all();
    let UaSearch::Auto {
        pending: AutoPending::Network(request),
    } = app.dialogs.connect.ua_search
    else {
        panic!("accepted network request")
    };
    assert!(app.dialogs.connect.error.is_some());
    app.on_event_for_test_with_request(
        Owner::Discovery,
        request,
        EventKind::UaNetworkFound { servers: vec![] },
    );
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
}

#[test]
fn ua_auto_timeout_and_single_network_error_clear_search() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.record_dispatches_for_test();
    app.search_ua_all();
    app.age_pending_requests_for_test(Owner::Discovery, 3600);
    app.tick_for_test();
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
    assert!(app.dialogs.connect.error.is_some());
    app.dialogs.connect.ua_lds_host = "localhost".into();
    app.search_ua_network();
    let UaSearch::Network { request } = app.dialogs.connect.ua_search else {
        panic!("network request")
    };
    app.on_event_for_test_with_request(
        Owner::Discovery,
        request,
        EventKind::Error {
            operation: Operation::SearchNetwork,
            message: BackendMsg::raw("failed"),
        },
    );
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
    assert!(app.dialogs.connect.error.is_some());
}

#[test]
fn ua_auto_failure_keeps_partial_results_and_single_error_clears_spinner() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    let (host, network) = start_recorded_auto(&mut app);
    app.on_event_for_test_with_request(
        Owner::Discovery,
        host,
        EventKind::UaServersFound {
            host: "localhost".into(),
            port: 4840,
            servers: vec![crate::bff::model::UaServerInfo {
                application_name: "partial".into(),
                application_uri: "urn:partial".into(),
                endpoints: vec![],
            }],
        },
    );
    app.on_event_for_test_with_request(
        Owner::Discovery,
        network,
        EventKind::Error {
            operation: Operation::SearchNetwork,
            message: BackendMsg::raw("network failed"),
        },
    );
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
    assert_eq!(app.dialogs.connect.ua_search_results.len(), 1);
    assert!(app.dialogs.connect.error.is_some());
    app.dialogs.connect.ua_search_host = "localhost".into();
    app.search_ua_servers();
    let UaSearch::SingleHost { request } = app.dialogs.connect.ua_search else {
        panic!("host request")
    };
    app.on_event_for_test_with_request(
        Owner::Discovery,
        request,
        EventKind::Error {
            operation: Operation::SearchServers,
            message: BackendMsg::raw("host failed"),
        },
    );
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
    assert!(app.dialogs.connect.error.is_some());
}

#[test]
fn new_da_discovery_keeps_queued_ua_timeout() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.record_dispatches_for_test();
    app.dialogs.connect.ua_search_host = "localhost".into();
    app.search_ua_servers();
    app.age_pending_requests_for_test(Owner::Discovery, 3600);
    app.engine.expire_requests(
        Instant::now(),
        app.settings.view.options.operation_timeout_secs,
    );
    app.restart_da_discovery(String::new());
    app.on_event_for_test(EventKind::Error {
        operation: Operation::Discover,
        message: BackendMsg::raw("DA failed"),
    });
    app.dialogs.connect.error = None;
    app.tick_for_test();
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
    assert!(app.dialogs.connect.error.is_some());
}

#[test]
fn new_da_discovery_keeps_independent_ua_search_pending() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.record_dispatches_for_test();
    app.dialogs.connect.ua_search_host = "localhost".into();
    app.search_ua_servers();
    let UaSearch::SingleHost { request } = app.dialogs.connect.ua_search else {
        panic!("UA host request")
    };
    app.restart_da_discovery(String::new());
    assert!(
        app.pending_request_ids_for_test(Owner::Discovery)
            .contains(&request)
    );
    assert_eq!(
        app.dialogs.connect.ua_search,
        UaSearch::SingleHost { request }
    );
    app.on_event_for_test_with_request(
        Owner::Discovery,
        request,
        EventKind::UaServersFound {
            host: "localhost".into(),
            port: 4840,
            servers: vec![],
        },
    );
    assert_eq!(app.dialogs.connect.ua_search, UaSearch::Idle);
}

#[test]
fn session_kind_tracks_endpoint_and_preserves_ua_selection_on_metadata_update() {
    let id = crate::bff::model::SessionId::new(1).unwrap();
    let mut view = SessionView::new(id, EndpointSpec::ua("opc.tcp://localhost:4840"), Lang::Ru);
    let ua = view.ua_mut().unwrap();
    ua.ua_view_id = Some("ns=1;s=view".into());
    ua.ua_browse_filter = crate::bff::model::NodeClassFilter::Variables;
    let mut updated = view.endpoint().clone();
    updated.user = "user".into();
    view.update_endpoint(updated);
    assert_eq!(
        view.ua().unwrap().ua_view_id.as_deref(),
        Some("ns=1;s=view")
    );
    assert_eq!(
        view.ua().unwrap().ua_browse_filter,
        crate::bff::model::NodeClassFilter::Variables
    );
    view.update_endpoint(EndpointSpec::da("", "Server"));
    assert_eq!(view.protocol(), Protocol::OpcDa);
    assert_eq!(view.endpoint().protocol, view.protocol());
    assert!(view.ua().is_none());
    view.update_endpoint(EndpointSpec::ua("opc.tcp://localhost:4840"));
    assert_eq!(view.endpoint().protocol, view.protocol());
    let ua = view.ua().unwrap();
    assert_eq!(ua.ua_view_id, None);
    assert_eq!(ua.ua_browse_filter, crate::bff::model::NodeClassFilter::All);
    assert_eq!(ua.subscription, SubscriptionState::Inactive);
}

#[test]
fn failed_script_save_reports_error_without_success() {
    let (_ctx, mut app) = tree_app();
    app.settings.save_blocked = true;
    app.journal.clear();
    app.save_script();
    let messages: Vec<_> = app
        .journal
        .entries()
        .iter()
        .map(|entry| entry.render(app.lang()))
        .collect();
    assert!(messages.iter().any(|message| {
        message.contains(
            tr(app.lang(), Tr::SettingsSaveBlocked)
                .split("{0}")
                .next()
                .unwrap(),
        )
    }));
    assert!(
        !messages
            .iter()
            .any(|message| message == tr(app.lang(), Tr::ScriptSaved))
    );
}

#[test]
fn startup_network_messages_follow_the_selected_language() {
    let (_ctx, app) = tree_app();
    for lang in Lang::ALL {
        let rendered: Vec<_> = app
            .journal
            .entries()
            .iter()
            .map(|entry| entry.render(lang))
            .collect();
        assert!(rendered.contains(&opclook_locale::fmt(lang, Tr::NetworkCacheLoaded, &[&0])));
        assert!(rendered.contains(&tr(lang, Tr::NetworkSelectionNone).to_string()));
    }
}

#[test]
fn exit_messages_include_localized_save_failure_and_shutdown_failure() {
    use opclook_contract::model::{GatewayShutdown, ShutdownOutcome};
    let (_ctx, mut app) = tree_app();
    app.settings.save_blocked = true;
    let save_error = app.save_settings();
    assert!(save_error.is_some());
    let normal = GatewayShutdown {
        da: ShutdownOutcome::Joined,
        ua: ShutdownOutcome::Joined,
    };
    assert!(super::exit_messages(None, Lang::En, normal).is_empty());
    let failed = GatewayShutdown {
        da: ShutdownOutcome::Panicked,
        ua: ShutdownOutcome::Joined,
    };
    let messages = super::exit_messages(save_error, app.lang(), failed);
    assert_eq!(messages.len(), 2);
    assert!(
        messages[0].contains(
            tr(app.lang(), Tr::SettingsSaveBlocked)
                .split("{0}")
                .next()
                .unwrap()
        )
    );
}

#[test]
fn abnormal_shutdown_messages_are_localized_and_normal_shutdown_is_silent() {
    use opclook_contract::model::{GatewayShutdown, ShutdownOutcome};
    use opclook_locale::Lang;
    let normal = GatewayShutdown {
        da: ShutdownOutcome::Joined,
        ua: ShutdownOutcome::Joined,
    };
    assert!(super::shutdown_messages(Lang::En, normal).is_empty());
    let failed = GatewayShutdown {
        da: ShutdownOutcome::Panicked,
        ua: ShutdownOutcome::TimedOut,
    };
    for lang in Lang::ALL {
        let messages = super::shutdown_messages(lang, failed);
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("DA"));
        assert!(messages[1].contains("UA"));
    }
    assert_ne!(
        super::shutdown_messages(Lang::Ru, failed),
        super::shutdown_messages(Lang::En, failed)
    );
    assert_ne!(
        super::shutdown_messages(Lang::Zh, failed),
        super::shutdown_messages(Lang::En, failed)
    );
}

#[test]
fn row_index_tracks_same_length_replacement() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let session = SessionId::new(1).unwrap();
    app.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let old = app.seed_row(session, "old", "old", Value::Empty);
    assert!(app.has_row_for_item(session, "old"));
    let mut other = App::new_test(&ctx);
    other.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    other.seed_row(session, "new", "new", Value::Empty);
    let mut replacement = other.rows.pop().unwrap();
    replacement.key = ItemKey::new(100).unwrap();
    app.rows_mut_for_test()[0] = replacement;
    assert!(!app.has_row_for_item(session, "old"));
    assert!(app.has_row_for_item(session, "new"));
    assert_eq!(app.row_position(old), None);
    assert_eq!(app.row_position(ItemKey::new(100).unwrap()), Some(0));
}

#[test]
fn visible_tree_cache_updates_on_toggle() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let session = SessionId::new(1).unwrap();
    app.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let node = |name: &str| BrowseNode {
        node_id: name.into(),
        name: name.into(),
        kind: NodeKind::Branch,
        has_children: true,
    };
    let root = app.seed_tree_item(session, None, 0, node("root"));
    app.seed_tree_item(session, Some(root), 1, node("child"));
    let view = &mut app.sessions[0];
    let first = view.visible_tree_indices();
    assert_eq!(first.as_slice(), &[0]);
    assert!(std::rc::Rc::ptr_eq(&first, &view.visible_tree_indices()));
    view.toggle_tree(root);
    assert_eq!(view.visible_tree_indices().as_slice(), &[0, 1]);
    view.toggle_tree(root);
    assert_eq!(view.visible_tree_indices().as_slice(), &[0]);
}

#[test]
fn explicit_profiles_do_not_reconfigure_when_global_options_change() {
    use crate::bff::model::AcquisitionConfig;
    let mut app = App::new_test(&egui::Context::default());
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    endpoint.acquisition = Some(AcquisitionConfig::ua_default());
    app.seed_session(SessionId::FIRST, endpoint.clone());
    app.record_dispatches_for_test();
    app.dialogs.options.draft.update_interval_secs = 9;
    app.apply_options();
    assert_eq!(app.sessions[0].endpoint().acquisition, endpoint.acquisition);
    assert!(
        app.dispatched_for_test()
            .iter()
            .all(|cmd| !matches!(cmd.kind, CommandKind::ConfigureUaSubscription { .. }))
    );
}

#[test]
fn same_endpoint_with_different_profiles_creates_distinct_sessions() {
    use crate::bff::model::{AcquisitionConfig, UpdateMode};
    let mut app = App::new_test(&egui::Context::default());
    app.record_dispatches_for_test();
    let mut subscription = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    subscription.acquisition = Some(AcquisitionConfig::ua_default());
    let mut poll = subscription.clone();
    poll.acquisition.as_mut().unwrap().update_mode = UpdateMode::Poll;
    assert!(app.open_tab_for_endpoint(subscription.clone()));
    assert!(app.open_tab_for_endpoint(poll.clone()));
    assert_eq!(app.sessions.len(), 2);
    assert_eq!(
        app.sessions[0].endpoint().acquisition,
        subscription.acquisition
    );
    assert_eq!(app.sessions[1].endpoint().acquisition, poll.acquisition);
}

fn profile_fixture() -> (egui::Context, App, EndpointSpec) {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    endpoint.acquisition = Some(crate::bff::model::AcquisitionConfig::ua_default());
    app.seed_session(SessionId::FIRST, endpoint.clone());
    app.record_dispatches_for_test();
    let connect = app
        .send(
            SessionId::FIRST,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event(Event {
        owner: SessionId::FIRST.into(),
        origin: Origin::Reply(connect),
        kind: EventKind::Connected {
            endpoint: endpoint.clone(),
            info: BackendMsg::raw("connected"),
        },
    });
    (ctx, app, endpoint)
}

#[test]
fn profile_save_auto_and_manual_reconnect_keep_distinct_snapshots() {
    use crate::bff::model::UpdateMode;
    let dir = std::env::temp_dir().join(format!("opclook-profile-{}-auto", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _redirect = crate::settings::thread_dir::redirect(&dir);
    let (_ctx, mut app, active) = profile_fixture();
    let session = SessionId::FIRST;
    app.open_acquisition_editor(session);
    app.dialogs.acquisition.draft.as_mut().unwrap().update_mode = UpdateMode::Poll;
    assert!(app.apply_acquisition_editor(false));
    assert_eq!(
        app.session(session).unwrap().endpoint().acquisition,
        active.acquisition
    );
    assert_eq!(
        app.session(session)
            .unwrap()
            .pending_acquisition
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .update_mode,
        UpdateMode::Poll
    );
    app.session_mut(session).unwrap().reconnect = ReconnectState::Waiting {
        at: Instant::now(),
        attempt: 1,
    };
    app.attempt_auto_reconnect(session);
    let auto = app
        .dispatched_for_test()
        .into_iter()
        .rev()
        .find_map(|cmd| match cmd.kind {
            CommandKind::Connect { endpoint } => Some((cmd.request_id.unwrap(), endpoint)),
            _ => None,
        })
        .unwrap();
    assert_eq!(auto.1.acquisition, active.acquisition);
    assert!(app.session(session).unwrap().pending_acquisition.is_some());
    app.on_event(Event {
        owner: session.into(),
        origin: Origin::Reply(auto.0),
        kind: EventKind::Connected {
            endpoint: auto.1,
            info: BackendMsg::raw("auto"),
        },
    });
    app.reconnect_session(session);
    let manual = app
        .dispatched_for_test()
        .into_iter()
        .rev()
        .find_map(|cmd| match cmd.kind {
            CommandKind::Connect { endpoint } => Some(endpoint),
            _ => None,
        })
        .unwrap();
    assert_eq!(manual.acquisition.unwrap().update_mode, UpdateMode::Poll);
    assert!(app.session(session).unwrap().pending_acquisition.is_none());
    drop(_redirect);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn profile_rejects_invalid_save_collision_and_stale_epoch() {
    let (_ctx, mut app, active) = profile_fixture();
    let session = SessionId::FIRST;
    let epoch = app.engine.connection_id(session);
    let dispatch_count = app.dispatched_for_test().len();
    app.open_acquisition_editor(session);
    app.dialogs.acquisition.draft.as_mut().unwrap().interval_ms = 0;
    assert!(!app.apply_acquisition_editor(true));
    assert_eq!(app.engine.connection_id(session), epoch);
    assert_eq!(app.dispatched_for_test().len(), dispatch_count);
    assert_eq!(
        app.session(session).unwrap().endpoint().acquisition,
        active.acquisition
    );
    app.dialogs.acquisition.draft = active.acquisition.clone();
    app.settings.save_blocked = true;
    assert!(!app.apply_acquisition_editor(true));
    assert_eq!(app.engine.connection_id(session), epoch);
    assert_eq!(app.dispatched_for_test().len(), dispatch_count);
    app.settings.save_blocked = false;
    app.seed_session(SessionId::new(2).unwrap(), active.clone());
    assert!(!app.apply_acquisition_editor(true));
    assert_eq!(app.engine.connection_id(session), epoch);
    assert_eq!(app.dispatched_for_test().len(), dispatch_count);
    app.sessions.retain(|v| v.id == session);
    app.reconnect_session(session);
    let new_connect = app
        .dispatched_for_test()
        .into_iter()
        .rev()
        .find_map(|cmd| match cmd.kind {
            CommandKind::Connect { endpoint } => Some((cmd.request_id.unwrap(), endpoint)),
            _ => None,
        })
        .unwrap();
    app.on_event(Event {
        owner: session.into(),
        origin: Origin::Reply(new_connect.0),
        kind: EventKind::Connected {
            endpoint: new_connect.1,
            info: BackendMsg::raw("new"),
        },
    });
    assert!(!app.apply_acquisition_editor(true));
    assert_eq!(
        app.dialogs.acquisition.error.as_deref(),
        Some(tr(app.lang(), Tr::AcquisitionEpochChanged))
    );
    assert_eq!(app.engine.connection_id(session), Some(new_connect.0));
}

#[test]
fn profile_reconnect_rejects_old_read_write_and_push_and_reregisters_rows() {
    use crate::bff::model::{DataValue, ItemInfo, Quality, UpdateMode, WriteOutcome};
    for initial_mode in [UpdateMode::Subscription, UpdateMode::Poll] {
        let dir = std::env::temp_dir().join(format!(
            "opclook-profile-{}-{initial_mode:?}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let redirect = crate::settings::thread_dir::redirect(&dir);
        let (_ctx, mut app, mut endpoint) = profile_fixture();
        // Establish the initial profile using a real new Connect if Poll was requested.
        if initial_mode == UpdateMode::Poll {
            endpoint.acquisition.as_mut().unwrap().update_mode = initial_mode;
            app.sessions[0].update_endpoint(endpoint.clone());
            app.reconnect_session(SessionId::FIRST);
            let connect = app
                .dispatched_for_test()
                .into_iter()
                .rev()
                .find_map(|cmd| match cmd.kind {
                    CommandKind::Connect { endpoint } => Some((cmd.request_id.unwrap(), endpoint)),
                    _ => None,
                })
                .unwrap();
            app.on_event(Event {
                owner: SessionId::FIRST.into(),
                origin: Origin::Reply(connect.0),
                kind: EventKind::Connected {
                    endpoint: connect.1,
                    info: BackendMsg::raw("poll"),
                },
            });
        }
        let session = SessionId::FIRST;
        let old_epoch = app.engine.connection_id(session).unwrap();
        assert!(app.add_items(session, vec![("ns=2;s=A".into(), "A".into())]));
        let key = app.rows[0].key;
        let add = app
            .dispatched_for_test()
            .into_iter()
            .rev()
            .find(|cmd| matches!(cmd.kind, CommandKind::AddItems { .. }))
            .unwrap()
            .request_id
            .unwrap();
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Reply(add),
            kind: EventKind::AddItems {
                results: vec![(
                    key,
                    Ok(ItemInfo {
                        data_type: None,
                        type_id: None,
                        writable: true,
                    }),
                )],
            },
        });
        let read = app
            .send(
                session,
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key] },
            )
            .unwrap();
        let write = app
            .send_with(
                session,
                Protocol::OpcUa,
                CommandKind::Write {
                    request: WriteRequest {
                        key,
                        value: Value::I64(9),
                        type_hint: None,
                    },
                },
                PendingCtx::WriteKey(key),
            )
            .unwrap();
        app.open_acquisition_editor(session);
        app.dialogs.acquisition.draft.as_mut().unwrap().update_mode =
            if initial_mode == UpdateMode::Poll {
                UpdateMode::Subscription
            } else {
                UpdateMode::Poll
            };
        assert!(app.apply_acquisition_editor(true));
        let new_connect = app
            .dispatched_for_test()
            .into_iter()
            .rev()
            .find_map(|cmd| match cmd.kind {
                CommandKind::Connect { endpoint } => Some((cmd.request_id.unwrap(), endpoint)),
                _ => None,
            })
            .unwrap();
        assert_ne!(app.engine.connection_id(session), Some(old_epoch));
        assert_eq!(
            new_connect.1.acquisition.as_ref().unwrap().update_mode,
            if initial_mode == UpdateMode::Poll {
                UpdateMode::Subscription
            } else {
                UpdateMode::Poll
            }
        );
        let value_before = app.rows[0].value.clone();
        let values = || EventKind::Values {
            values: vec![(
                key,
                DataValue {
                    value: Value::I64(99),
                    quality: Quality::from_da(0xc0),
                    timestamp: None,
                    reason: None,
                },
            )],
        };
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Reply(read),
            kind: values(),
        });
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Reply(write),
            kind: EventKind::Wrote {
                key,
                result: Ok(WriteOutcome::from_ua(0, "Good")),
            },
        });
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Session(old_epoch),
            kind: values(),
        });
        assert_eq!(app.rows[0].value, value_before);
        let before_connected = app.dispatched_for_test().len();
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Reply(new_connect.0),
            kind: EventKind::Connected {
                endpoint: new_connect.1,
                info: BackendMsg::raw("new"),
            },
        });
        let after_connected = app.dispatched_for_test().len();
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Reply(read),
            kind: values(),
        });
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Reply(write),
            kind: EventKind::Wrote {
                key,
                result: Ok(WriteOutcome::from_ua(0, "Good")),
            },
        });
        app.on_event(Event {
            owner: session.into(),
            origin: Origin::Session(old_epoch),
            kind: values(),
        });
        assert_eq!(app.rows[0].value, value_before);
        assert_eq!(app.dispatched_for_test().len(), after_connected);
        assert!(
            app.dispatched_for_test()[before_connected..]
                .iter()
                .any(|cmd| match &cmd.kind {
                    CommandKind::AddItems { items } => items.iter().any(|item| item.key == key),
                    _ => false,
                })
        );
        assert_eq!(
            app.dispatched_for_test()
                .iter()
                .filter(|cmd| matches!(cmd.kind, CommandKind::Write { .. }))
                .count(),
            1
        );
        assert!(app.journal.entries().iter().any(|entry| {
            entry
                .render(app.lang())
                .contains(tr(app.lang(), Tr::WriteUnknownStatus))
        }));
        drop(redirect);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn profile_late_collision_and_capacity_preserve_staged_choice() {
    use crate::bff::model::UpdateMode;
    let dir =
        std::env::temp_dir().join(format!("opclook-profile-{}-collision", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let redirect = crate::settings::thread_dir::redirect(&dir);
    let (_ctx, mut app, mut candidate) = profile_fixture();
    app.open_acquisition_editor(SessionId::FIRST);
    app.dialogs.acquisition.draft.as_mut().unwrap().update_mode = UpdateMode::Poll;
    assert!(app.apply_acquisition_editor(false));
    candidate.acquisition.as_mut().unwrap().update_mode = UpdateMode::Poll;
    app.seed_session(SessionId::new(2).unwrap(), candidate);
    let before = app.dispatched_for_test().len();
    let epoch = app.engine.connection_id(SessionId::FIRST);
    app.reconnect_session(SessionId::FIRST);
    assert_eq!(app.dispatched_for_test().len(), before);
    assert_eq!(app.engine.connection_id(SessionId::FIRST), epoch);
    assert!(
        app.session(SessionId::FIRST)
            .unwrap()
            .pending_acquisition
            .is_some()
    );
    assert_eq!(app.status, tr(app.lang(), Tr::AcquisitionCollision));
    app.sessions.retain(|view| view.id == SessionId::FIRST);
    app.open_acquisition_editor(SessionId::FIRST);
    app.dialogs.acquisition.draft = None;
    app.engine.exhaust_requests_for_test();
    assert!(!app.apply_acquisition_editor(true));
    assert_eq!(app.dispatched_for_test().len(), before);
    assert!(app.apply_acquisition_editor(false));
    assert_eq!(
        app.session(SessionId::FIRST).unwrap().pending_acquisition,
        Some(None)
    );
    app.reconnect_session(SessionId::FIRST);
    assert_eq!(app.dispatched_for_test().len(), before);
    assert_eq!(app.engine.connection_id(SessionId::FIRST), epoch);
    assert_eq!(
        app.session(SessionId::FIRST).unwrap().pending_acquisition,
        Some(None)
    );
    drop(redirect);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn profile_connect_dispatch_failure_keeps_saved_candidate() {
    use crate::bff::model::UpdateMode;
    let dir = std::env::temp_dir().join(format!("opclook-profile-{}-dispatch", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let redirect = crate::settings::thread_dir::redirect(&dir);
    let (_ctx, mut app, active) = profile_fixture();
    app.open_acquisition_editor(SessionId::FIRST);
    app.dialogs.acquisition.draft.as_mut().unwrap().update_mode = UpdateMode::Poll;
    app.engine.limit_dispatch_for_test(1); // Disconnect succeeds; Connect fails.
    assert!(app.apply_acquisition_editor(true));
    let view = app.session(SessionId::FIRST).unwrap();
    assert_eq!(view.endpoint().acquisition, active.acquisition);
    assert_eq!(
        view.pending_acquisition
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .update_mode,
        UpdateMode::Poll
    );
    assert_eq!(view.state, SessionState::Disconnected);
    assert!(!app.status.is_empty());
    drop(redirect);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn migrated_profiles_are_reported_as_localized_warning_and_status() {
    use crate::journal::{JournalMessage, Level};
    use crate::settings::SettingsNotice;
    let mut app = App::new_inner(Settings::default());
    let message = app.report_settings_notice(SettingsNotice::AcquisitionMigrated {
        ua: 1,
        da: 1,
        interval_ms: 5000,
    });
    assert_eq!(app.status, message);
    assert!(
        app.journal
            .entries()
            .iter()
            .any(|entry| matches!(entry, crate::journal::Entry {
        level: Level::Warning,
        message: JournalMessage::Localized { key: Tr::SettingsAcquisitionMigrated, args }, ..
    } if args == &vec!["1", "1", "5000"]))
    );
    for lang in Lang::ALL {
        let text = crate::i18n::fmt(lang, Tr::SettingsAcquisitionMigrated, &[&1, &1, &5000]);
        assert!(text.contains("5000"));
        assert!(text.contains("poll_only"));
    }
}

#[test]
fn failed_migration_cannot_open_legacy_profile() {
    let settings = Settings {
        save_blocked: true,
        legacy_migration_failed: true,
        ..Settings::default()
    };
    let mut app = App::new_inner(settings);
    assert!(!app.open_tab_for_endpoint(EndpointSpec::ua("opc.tcp://127.0.0.1:4840")));
    assert!(app.sessions.is_empty());
    assert_eq!(
        app.status,
        tr(app.lang(), Tr::SettingsAcquisitionMigrationBlocked)
    );
}

#[test]
fn p4_stale_write_form_cannot_cross_connection_epoch() {
    let (_ctx, mut app) = tree_app();
    let session = SessionId::FIRST;
    app.record_dispatches_for_test();
    let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    let a = app
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        a,
        EventKind::Connected {
            endpoint: endpoint.clone(),
            info: BackendMsg::raw("A"),
        },
    );
    let key = app.seed_writable_row(session, "a", "A", Value::I64(1));
    app.settings.view.options.confirm_write = false;
    app.open_write_dialog(vec![key]);
    app.dialogs.write.text = "7".into();
    let b = app
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        b,
        EventKind::Connected {
            endpoint,
            info: BackendMsg::raw("B"),
        },
    );
    assert_ne!(a, b);
    app.record_dispatches_for_test();
    app.apply_write();
    assert!(
        app.dispatched_for_test()
            .iter()
            .all(|command| !matches!(command.kind, CommandKind::Write { .. }))
    );
    assert_eq!(
        app.dialogs.write.error.as_deref(),
        Some(tr(app.lang(), Tr::PaletteWriteStale))
    );
}

#[test]
fn p4_stale_write_confirmation_cannot_cross_connection_epoch() {
    let (_ctx, mut app) = tree_app();
    app.record_dispatches_for_test();
    let session = SessionId::FIRST;
    let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    let a = app
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        a,
        EventKind::Connected {
            endpoint: endpoint.clone(),
            info: BackendMsg::raw("A"),
        },
    );
    let key = app.seed_writable_row(session, "a", "A", Value::I64(1));
    app.settings.view.options.confirm_write = true;
    app.open_write_dialog(vec![key]);
    app.dialogs.write.text = "7".into();
    app.apply_write();
    // Simulate a previously queued confirmation being delivered after reconnect.
    let old_confirmation = app.dialogs.confirm.take().expect("write confirmation");
    let b = app
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        session,
        b,
        EventKind::Connected {
            endpoint,
            info: BackendMsg::raw("B"),
        },
    );
    assert_ne!(a, b);
    app.dialogs.confirm = Some(old_confirmation);
    app.confirm_yes();
    assert!(
        app.dispatched_for_test()
            .iter()
            .all(|command| !matches!(command.kind, CommandKind::Write { .. }))
    );
    assert_eq!(app.status, tr(app.lang(), Tr::PaletteWriteStale));
}

#[test]
fn p4_palette_captures_connection_when_opened() {
    use opclook_contract::actions::ActionId;
    let (_ctx, mut app) = tree_app();
    app.record_dispatches_for_test();
    app.open_command_palette();
    let old = app.dialogs.palette.context_epoch;
    let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    app.send(SessionId::FIRST, Protocol::OpcUa, CommandKind::Test)
        .unwrap();
    let id = app
        .send(
            SessionId::FIRST,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
        )
        .unwrap();
    app.on_event_for_test_with_request(
        SessionId::FIRST,
        id,
        EventKind::Connected {
            endpoint,
            info: BackendMsg::raw("new"),
        },
    );
    assert_ne!(old, app.engine.connection_id(SessionId::FIRST));
    app.prepare_palette_context();
    assert_eq!(app.dialogs.palette.context_epoch, old);
    assert_eq!(
        app.execute_palette_action(ActionId::Browse),
        Err(Tr::PaletteConnectionChanged)
    );
}

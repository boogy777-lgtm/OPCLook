use super::present::operation_label;
use super::sessions::reconnect_delay;
use super::*;
use crate::bff::model::{BrowseNode, EndpointSpec, EventKind, NodeKind, Operation};
use crate::i18n::Lang;
use crate::settings::Options;

#[test]
fn reconnect_delay_doubles_exponentially() {
    // Attempts 1..=21 give 1, 2, 4, ..., 2^20 (1048576) seconds.
    let expected: Vec<u64> = (0..21).map(|shift| 1u64 << shift).collect();
    for (attempt, secs) in expected.iter().enumerate() {
        assert_eq!(
            reconnect_delay(attempt as u32 + 1).as_secs(),
            *secs,
            "attempt {} should be {}s",
            attempt + 1,
            secs
        );
    }
    assert_eq!(reconnect_delay(21).as_secs(), 1 << 20);
}

#[test]
fn reconnect_delay_never_overflows() {
    // Beyond the 64-bit shift width the helper pins to u64::MAX instead of
    // panicking; the retry loop never gets this far (cap is 20).
    assert_eq!(reconnect_delay(65).as_secs(), u64::MAX);
    assert_eq!(reconnect_delay(u32::MAX).as_secs(), u64::MAX);
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
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    app.seed_tree_item(
        1,
        None,
        0,
        BrowseNode {
            node_id: "top".to_string(),
            name: "Top".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(1, 1);

    app.add_selected_children();

    // Browse the start branch: it contains only a nested Branch (no Items).
    feed_browse(&mut app, 1, "top", vec![branch_node("mid", "Mid", true)]);
    // Browse the nested branch: it contains one Item.
    feed_browse(&mut app, 1, "mid", vec![item_node("i1", "I1")]);

    let ids: Vec<&str> = app.rows.iter().map(|r| r.item_id.as_str()).collect();
    assert!(ids.contains(&"i1"), "nested item must be added: {ids:?}");
    assert_eq!(app.rows.len(), 1);
    assert!(app.add_children.is_none(), "job must self-cancel when done");
}

#[test]
fn add_children_walks_three_levels_and_deduplicates() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    app.seed_tree_item(
        1,
        None,
        0,
        BrowseNode {
            node_id: "l0".to_string(),
            name: "L0".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(1, 1);
    app.add_selected_children();

    // L0 -> L1 (with direct item) and L2.
    feed_browse(
        &mut app,
        1,
        "l0",
        vec![branch_node("l1", "L1", true), branch_node("l2", "L2", true)],
    );
    // L1 -> item + L3.
    feed_browse(
        &mut app,
        1,
        "l1",
        vec![item_node("d1", "D1"), branch_node("l3", "L3", true)],
    );
    // L2 -> duplicate d1 + new d2.
    feed_browse(
        &mut app,
        1,
        "l2",
        vec![item_node("d1", "D1dup"), item_node("d2", "D2")],
    );
    // L3 -> deep item (3 levels below the start).
    feed_browse(&mut app, 1, "l3", vec![item_node("deep", "Deep")]);

    let mut ids: Vec<&str> = app.rows.iter().map(|r| r.item_id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, vec!["d1", "d2", "deep"]);
    assert!(app.add_children.is_none());
}

#[test]
fn add_children_cycles_are_guarded() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    app.seed_tree_item(
        1,
        None,
        0,
        BrowseNode {
            node_id: "a".to_string(),
            name: "A".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(1, 1);
    app.add_selected_children();

    // A -> B ; B -> A (cycle) + item.
    feed_browse(&mut app, 1, "a", vec![branch_node("b", "B", true)]);
    feed_browse(
        &mut app,
        1,
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
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    app.seed_tree_item(
        1,
        None,
        0,
        BrowseNode {
            node_id: "a".to_string(),
            name: "A".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(1, 1);
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
    let manual_request = app.browse(1, "a".to_string()).expect("manual browse sent");
    assert_ne!(job_request, manual_request);

    app.on_event_for_test_with_request(
        1,
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
        1,
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
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    app.seed_tree_item(
        1,
        None,
        0,
        BrowseNode {
            node_id: "a".to_string(),
            name: "A".to_string(),
            kind: NodeKind::Branch,
            has_children: true,
        },
    );
    app.set_tree_focus(1, 1);
    app.add_selected_children();
    assert!(app.add_children.is_some());

    // A disconnect during the job cancels it.
    app.disconnect_session(1);
    assert!(app.add_children.is_none(), "disconnect must cancel the job");
    // A Disconnected event also cancels (non-triggered link loss path).
    app.add_selected_children();
    assert!(app.add_children.is_some());
    app.on_event_for_test_at(
        1,
        EventKind::Disconnected {
            reason: "lost".to_string(),
        },
    );
    assert!(app.add_children.is_none());
}

#[test]
fn starting_new_job_replaces_previous_for_same_session() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(1, EndpointSpec::da("", "Srv.Test"));
    for node_id in ["a", "b"] {
        app.seed_tree_item(
            1,
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
    app.set_tree_focus(1, 1);
    app.add_selected_children();
    assert_eq!(app.add_children.as_ref().map(|j| j.session), Some(1));
    // Starting another job for the same session replaces the in-flight one.
    app.set_tree_focus(1, 2);
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
        app.dialogs.connect.da_server_hosts,
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
        message: "OPCEnum unavailable".to_string(),
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
    app.on_event_for_test(EventKind::UaNetworkFound {
        servers: vec![UaNetworkInfo {
            application_name: "Simulator".to_string(),
            application_uri: "urn:sim".to_string(),
            discovery_urls: vec!["opc.tcp://127.0.0.1:4840".to_string()],
        }],
    });
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
            url: "opc.tcp://server-a:4840".to_string(),
            security_policy: "None".to_string(),
            security_mode: "None".to_string(),
        },
        UaEndpointInfo {
            url: "opc.tcp://server-b:4840".to_string(),
            security_policy: "Basic256Sha256".to_string(),
            security_mode: "SignAndEncrypt".to_string(),
        },
    ];
    assert!(!app.validate_ua_security());
    app.dialogs.connect.ua_url = "opc.tcp://server-b:4840".to_string();
    assert!(app.validate_ua_security());
}

#[test]
fn combined_ua_search_preserves_security_variants_at_same_url() {
    use crate::bff::model::{UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.ua_auto_pending = 2;
    for (policy, mode) in [("None", "None"), ("Basic256Sha256", "SignAndEncrypt")] {
        app.on_event_for_test(EventKind::UaServersFound {
            host: "localhost".to_string(),
            port: 4840,
            servers: vec![UaServerInfo {
                application_name: "Simulator".to_string(),
                application_uri: "urn:sim".to_string(),
                endpoints: vec![UaEndpointInfo {
                    url: "opc.tcp://localhost:4840".to_string(),
                    security_policy: policy.to_string(),
                    security_mode: mode.to_string(),
                }],
            }],
        });
    }
    assert_eq!(app.dialogs.connect.ua_search_results.len(), 1);
    assert_eq!(app.dialogs.connect.ua_search_results[0].endpoints.len(), 2);
}

use super::present::operation_label;
use super::sessions::reconnect_delay;
use super::table::infer_kind;
use super::*;
use crate::bff::model::{
    BackendMsg, BrowseNode, EndpointSpec, EventKind, NodeKind, Operation, WriteRequest,
};
use crate::constants::{RECONNECT_MAX_ATTEMPTS, RECONNECT_MAX_DELAY_SECS};
use crate::i18n::Lang;
use crate::settings::Options;

#[test]
fn reconnect_delay_doubles_exponentially() {
    // Attempts 1..=7 double: 1, 2, 4, ... 64 would be next but the ceiling
    // (RECONNECT_MAX_DELAY_SECS = 60) applies from attempt 7 on.
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
            reconnect_delay(attempt).as_secs() <= RECONNECT_MAX_DELAY_SECS,
            "attempt {attempt}"
        );
    }
    assert_eq!(
        reconnect_delay(RECONNECT_MAX_ATTEMPTS).as_secs(),
        RECONNECT_MAX_DELAY_SECS
    );
}

#[test]
fn reconnect_delay_never_overflows() {
    // Beyond the 64-bit shift width the helper pins to the ceiling instead of
    // panicking.
    assert_eq!(reconnect_delay(65).as_secs(), RECONNECT_MAX_DELAY_SECS);
    assert_eq!(
        reconnect_delay(u32::MAX).as_secs(),
        RECONNECT_MAX_DELAY_SECS
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
            reason: BackendMsg::raw("lost"),
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

#[test]
fn combined_ua_search_shows_a_loopback_server_once_while_running() {
    use crate::bff::model::{UaEndpointInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    // LDS/mDNS + 127.0.0.1 + localhost; LDS has not answered yet.
    app.dialogs.connect.ua_auto_pending = 3;
    app.dialogs.connect.ua_searching = true;
    for host in ["127.0.0.1", "localhost"] {
        app.on_event_for_test(EventKind::UaServersFound {
            host: host.to_string(),
            port: 4840,
            servers: vec![UaServerInfo {
                application_name: "Simulator".to_string(),
                application_uri: "urn:sim".to_string(),
                endpoints: vec![UaEndpointInfo {
                    url: "opc.tcp://127.0.0.1:4840".to_string(),
                    security_policy: "None".to_string(),
                    security_mode: "None".to_string(),
                }],
            }],
        });
    }
    // Still searching (LDS pending), yet the visible list is already merged.
    assert_eq!(app.dialogs.connect.ua_auto_pending, 1);
    assert_eq!(app.dialogs.connect.ua_search_results.len(), 1);
    assert_eq!(app.dialogs.connect.ua_search_results[0].endpoints.len(), 1);
}

#[test]
fn lds_urls_merge_into_host_result_instead_of_being_dropped() {
    use crate::bff::model::{UaEndpointInfo, UaNetworkInfo, UaServerInfo};
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    app.dialogs.connect.ua_auto_pending = 2;
    app.on_event_for_test(EventKind::UaServersFound {
        host: "127.0.0.1".to_string(),
        port: 4840,
        servers: vec![UaServerInfo {
            application_name: "Simulator".to_string(),
            application_uri: "urn:sim".to_string(),
            endpoints: vec![UaEndpointInfo {
                url: "opc.tcp://127.0.0.1:4840".to_string(),
                security_policy: "None".to_string(),
                security_mode: "None".to_string(),
            }],
        }],
    });
    app.on_event_for_test(EventKind::UaNetworkFound {
        servers: vec![
            // Same server (same URI) advertised on the LAN address.
            UaNetworkInfo {
                application_name: "Simulator".to_string(),
                application_uri: "urn:sim".to_string(),
                discovery_urls: vec!["opc.tcp://10.0.0.5:4840".to_string()],
            },
            // LDS record without URI for the loopback URL: merged by URL,
            // its name must not become a fake application URI.
            UaNetworkInfo {
                application_name: "Simulator (LDS)".to_string(),
                application_uri: String::new(),
                discovery_urls: vec!["opc.tcp://127.0.0.1:4840/".to_string()],
            },
        ],
    });
    let results = &app.dialogs.connect.ua_search_results;
    assert_eq!(results.len(), 1, "one server, not three rows: {results:?}");
    assert_eq!(results[0].application_uri, "urn:sim");
    assert!(
        results[0]
            .endpoints
            .iter()
            .any(|e| e.url == "opc.tcp://10.0.0.5:4840"),
        "the LAN URL reported only by LDS/mDNS must be kept"
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
                    url: "opc.tcp://shared:4840".to_string(),
                    security_policy: String::new(),
                    security_mode: String::new(),
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
    app.seed_session(1, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
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
        1,
        EventKind::Browse {
            node_id: String::new(),
            children: vec![
                branch_node("ns=1;s=R1", "R1", true),
                branch_node("ns=1;s=R2", "R2", true),
            ],
        },
    );
    app.on_event_for_test_at(
        1,
        EventKind::Browse {
            node_id: "ns=1;s=R1".to_string(),
            children: vec![item_node("ns=1;s=C1", "C1")],
        },
    );
    let r1 = item_id_named(&app, "R1");

    app.on_event_for_test_at(
        1,
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
    let r1_item = app.tree_item(1, r1).expect("R1 still present");
    assert!(r1_item.expanded && r1_item.loaded, "R1 keeps its state");

    // A second Views event replaces only the previous views.
    app.on_event_for_test_at(
        1,
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
        1,
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
            1,
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
    let request = app.browse_item(1, shared[1]).expect("dispatched");
    app.on_event_for_test_with_request(
        1,
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
    let first = app.tree_item(1, shared[0]).expect("first instance");
    assert!(!first.loaded && !first.expanded, "1st instance untouched");
    let second = app.tree_item(1, shared[1]).expect("second instance");
    assert!(second.loaded && second.expanded && !second.loading);
}

#[test]
fn u7_reply_for_a_removed_item_is_dropped() {
    let (_ctx, mut app) = tree_app();
    app.on_event_for_test_at(
        1,
        EventKind::Browse {
            node_id: String::new(),
            children: vec![branch_node("ns=1;s=P", "P", true)],
        },
    );
    let p = item_id_named(&app, "P");
    let request = app.browse_item(1, p).expect("dispatched");
    // The tree is re-read meanwhile: the old item ids are gone.
    app.reread_tree(1);
    app.on_event_for_test_with_request(
        1,
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
        1,
        EventKind::Browse {
            node_id: String::new(),
            children: vec![branch_node("ns=1;s=P", "P", true)],
        },
    );
    assert_eq!(app.sessions[0].tree.len(), 1);
    app.set_ua_browse_vars_only(1, true);
    let view = &app.sessions[0];
    assert!(view.ua_browse_vars_only);
    assert!(view.tree.is_empty(), "stale tree dropped");
    assert!(view.root_loading, "root browse issued");
    assert_eq!(app.pending_request_ids_for_test(1).len(), 1);
    // Setting the same value again must not browse again.
    app.set_ua_browse_vars_only(1, true);
    assert_eq!(app.pending_request_ids_for_test(1).len(), 1);
}

// ---- U11: URL host parsing ----

#[test]
fn u11_ipv6_url_host_and_port_are_parsed() {
    use super::present::ua_url_host;
    assert_eq!(ua_url_host("opc.tcp://[::1]:4840").as_deref(), Some("::1"));
    assert_eq!(
        ua_url_host("opc.tcp://[2001:db8::7]:4840/path").as_deref(),
        Some("2001:db8::7")
    );
    assert_eq!(ua_url_host("opc.tcp://[::1]").as_deref(), Some("::1"));
    assert_eq!(
        ua_url_host("opc.tcp://user@plant:4840/x").as_deref(),
        Some("plant")
    );
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

fn hold_discovery(app: &mut App, host: &str) {
    app.da_route_holds.push(DaRouteHold {
        session: 0,
        host: host.to_string(),
        prepared: PreparedRoutes::default(),
    });
}

fn pend_discover(app: &mut App, request_id: RequestId, host: &str) {
    app.pending_requests.insert(
        (0, request_id),
        PendingRequest {
            operation: Operation::Discover,
            sent: Instant::now(),
            ctx: PendingCtx::Host(host.to_string()),
        },
    );
}

#[test]
fn d4_discover_error_releases_only_the_failing_hosts_route() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    hold_discovery(&mut app, "pc-a");
    hold_discovery(&mut app, "pc-b");
    pend_discover(&mut app, 501, "pc-a");
    pend_discover(&mut app, 502, "pc-b");
    app.on_event_for_test_with_request(
        0,
        501,
        EventKind::Error {
            operation: Operation::Discover,
            message: BackendMsg::raw("RPC unavailable"),
        },
    );
    let hosts: Vec<&str> = app.da_route_holds.iter().map(|h| h.host.as_str()).collect();
    assert_eq!(hosts, ["pc-b"], "the other search keeps its route");
}

#[test]
fn d5_closing_the_connect_dialog_releases_discovery_routes() {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.open_connect_dialog();
    hold_discovery(&mut app, "pc-a");
    hold_discovery(&mut app, "pc-b");
    // A session route (not a discovery one) must survive.
    app.da_route_holds.push(DaRouteHold {
        session: 9,
        host: "pc-c".to_string(),
        prepared: PreparedRoutes::default(),
    });
    app.close_connect_dialog();
    let left: Vec<(SessionId, &str)> = app
        .da_route_holds
        .iter()
        .map(|h| (h.session, h.host.as_str()))
        .collect();
    assert_eq!(left, [(9, "pc-c")]);
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
    open_add_by_name(&mut app, 1, "0:Objects/2:Sim/2:Tag1");
    let (session, request) = app.dialogs.add_signal.resolving.expect("waiting");
    assert_eq!(session, 1);
    assert!(app.dialogs.add_signal.open, "dialog must stay open");
    app.on_event_for_test_with_request(
        1,
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
    open_add_by_name(&mut app, 1, "0:Objects/2:Nope");
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    app.on_event_for_test_with_request(
        1,
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
    open_add_by_name(&mut app, 1, "0:Objects/2:Nope");
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    app.on_event_for_test_with_request(
        1,
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
    app.seed_session(2, EndpointSpec::ua("opc.tcp://127.0.0.1:4841"));
    open_add_by_name(&mut app, 1, "0:Objects/2:Tag1");
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    // A reply with the same request id but from session 2 is unsolicited for
    // this dialog: it is dropped and the wait continues.
    app.on_event_for_test_with_request(
        2,
        request,
        EventKind::PathResolved {
            targets: vec!["ns=2;s=Other".to_string()],
        },
    );
    assert!(app.rows.is_empty());
    assert_eq!(app.dialogs.add_signal.resolving, Some((1, request)));
}

#[test]
fn u14_cancelled_dialog_ignores_the_late_answer() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(&mut app, 1, "0:Objects/2:Tag1");
    let (_, request) = app.dialogs.add_signal.resolving.expect("waiting");
    app.close_add_signal_dialog();
    app.on_event_for_test_with_request(
        1,
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
    open_add_by_name(&mut app, 1, "0:Objects/2:Tag1");
    app.disconnect_session(1);
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
    let dir = std::env::temp_dir().join(format!("opcda-egui-fonts-{}", std::process::id()));
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
    app.browse(1, String::new());
    app.age_pending_requests_for_test(1, 20);
    app.tick_for_test();
    assert_eq!(app.pending_request_ids_for_test(1).len(), 1);
    assert_eq!(no_reply_lines(&app), 0);
}

#[test]
fn k1_unanswered_root_browse_stops_the_spinner_and_reports() {
    let (_ctx, mut app) = tree_app();
    app.browse(1, String::new());
    assert!(app.sessions[0].root_loading);
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    assert!(!app.sessions[0].root_loading, "spinner must stop");
    assert!(app.pending_request_ids_for_test(1).is_empty());
    assert_eq!(no_reply_lines(&app), 1, "{:?}", journal_lines(&app));
}

#[test]
fn k1_unanswered_node_expand_resets_only_the_loading_flags() {
    let (_ctx, mut app) = tree_app();
    app.on_event_for_test_at(
        1,
        EventKind::Browse {
            node_id: String::new(),
            children: vec![branch_node("ns=1;s=P", "P", true)],
        },
    );
    let item = item_id_named(&app, "P");
    app.browse_item(1, item);
    assert!(app.sessions[0].tree[0].loading);
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    assert!(!app.sessions[0].tree[0].loading);
    assert!(!app.sessions[0].tree[0].loaded, "can be expanded again");
}

#[test]
fn k1_unanswered_read_frees_the_poll_slot_once() {
    let (_ctx, mut app) = tree_app();
    app.seed_row(1, "Tag1", "Tag1", Value::I64(1));
    app.sessions[0].read_in_flight = true;
    app.send(
        1,
        Protocol::OpcUa,
        CommandKind::Read {
            keys: vec![app.rows[0].key],
        },
    );
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    assert!(
        !app.sessions[0].read_in_flight || app.pending_request_ids_for_test(1).len() == 1,
        "the slot is free (a new poll may already be in flight)"
    );
    assert_eq!(no_reply_lines(&app), 1, "one message, not two");
}

#[test]
fn k1_late_reply_after_expiry_is_dropped() {
    let (_ctx, mut app) = tree_app();
    app.browse(1, String::new());
    let request = app.pending_request_ids_for_test(1)[0];
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    app.on_event_for_test_with_request(
        1,
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
    app.settings.options.diagnostic_trace = true;
    let _ = app.connect_from_dialog(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    let session = app.sessions.last().expect("session").id;
    let before = journal_lines(&app).len();
    app.age_pending_requests_for_test(session, 3600);
    app.tick_for_test();
    let disconnect = operation_label(Lang::Ru, Operation::Disconnect);
    let sent_disconnect = journal_lines(&app)[before..]
        .iter()
        .any(|line| line.contains("→ #") && line.contains(&disconnect));
    assert!(
        sent_disconnect,
        "an abandoned Connect must be followed by Disconnect: {:?}",
        journal_lines(&app)
    );
    // Its reply is not awaited: nothing stays pending for the session.
    assert!(app.pending_request_ids_for_test(session).is_empty());
}

#[test]
fn k1_unanswered_resolve_name_unblocks_the_add_signal_dialog() {
    let (_ctx, mut app) = tree_app();
    open_add_by_name(&mut app, 1, "0:Objects/2:Tag1");
    assert!(app.dialogs.add_signal.resolving.is_some());
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    assert_eq!(app.dialogs.add_signal.resolving, None);
    assert!(app.dialogs.add_signal.open);
    assert!(app.dialogs.add_signal.error.is_some());
}

#[test]
fn k1_unanswered_add_items_leaves_the_waiting_state() {
    let (_ctx, mut app) = tree_app();
    app.add_items(1, vec![("ns=2;s=Tag1".to_string(), "Tag1".to_string())]);
    assert!(app.rows[0].pending);
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    assert!(!app.rows[0].pending, "row must stop waiting");
    assert_eq!(app.rows[0].quality.kind, QualityKind::Bad);
}

#[test]
fn k1_unanswered_write_completes_the_batch_as_failed() {
    let (_ctx, mut app) = tree_app();
    app.seed_row(1, "Tag1", "Tag1", Value::I64(1));
    let key = app.rows[0].key;
    app.pending_writes = 1;
    app.write_total = 1;
    app.send_with(
        1,
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
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    assert_eq!(app.pending_writes, 0);
    assert_eq!(app.write_err, 1);
    assert!(app.last_write_summary.is_some());
}

#[test]
fn k1_disconnect_and_subscription_expire_silently() {
    let (_ctx, mut app) = tree_app();
    app.send(1, Protocol::OpcUa, CommandKind::Disconnect);
    app.send(
        1,
        Protocol::OpcUa,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 1000,
            deadband: 0.0,
        },
    );
    assert_eq!(app.pending_request_ids_for_test(1).len(), 2);
    app.age_pending_requests_for_test(1, 3600);
    app.tick_for_test();
    assert!(app.pending_request_ids_for_test(1).is_empty());
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
    app.browse(1, String::new());
    let request = app.pending_request_ids_for_test(1)[0];
    app.on_event_for_test_with_request(
        1,
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
    app.settings.options.diagnostic_trace = true;
    app.browse(1, String::new());
    let request = app.pending_request_ids_for_test(1)[0];
    let lines = trace_lines(&app);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(&format!("#{request}")), "{lines:?}");
    app.on_event_for_test_with_request(
        1,
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
        1,
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
        session: 1,
        protocol: Protocol::OpcUa,
        requests: keys.iter().copied().map(write_request).collect(),
        type_label: "Int".to_string(),
    });
}

#[test]
fn a11_overlapping_write_batches_add_up() {
    let (_ctx, mut app) = tree_app();
    let a = app.seed_row(1, "A", "A", Value::I64(0));
    let b = app.seed_row(1, "B", "B", Value::I64(0));
    let c = app.seed_row(1, "C", "C", Value::I64(0));
    start_batch(&mut app, &[a, b]);
    start_batch(&mut app, &[c]); // starts while the first batch is running
    assert_eq!((app.pending_writes, app.write_total), (3, 3));
    let ids = app.pending_request_ids_for_test(1);
    assert_eq!(ids.len(), 3);
    for (index, id) in ids.iter().enumerate() {
        assert!(app.last_write_summary.is_none(), "not done after {index}");
        let key = [a, b, c][index];
        app.on_event_for_test_with_request(
            1,
            *id,
            EventKind::Wrote {
                key,
                result: Ok(()),
            },
        );
    }
    assert_eq!(app.pending_writes, 0);
    assert_eq!((app.write_ok, app.write_err, app.write_total), (3, 0, 3));
    assert!(
        app.last_write_summary
            .as_deref()
            .is_some_and(|s| s.contains('3'))
    );
}

#[test]
fn a11_counters_restart_after_a_finished_batch() {
    let (_ctx, mut app) = tree_app();
    let a = app.seed_row(1, "A", "A", Value::I64(0));
    start_batch(&mut app, &[a]);
    let id = app.pending_request_ids_for_test(1)[0];
    app.on_event_for_test_with_request(
        1,
        id,
        EventKind::Wrote {
            key: a,
            result: Ok(()),
        },
    );
    start_batch(&mut app, &[a]);
    assert_eq!(
        (app.pending_writes, app.write_total, app.write_ok),
        (1, 1, 0)
    );
}

#[test]
fn a11_writes_lost_with_the_session_complete_the_batch_as_failed() {
    let (_ctx, mut app) = tree_app();
    let a = app.seed_row(1, "A", "A", Value::I64(0));
    let b = app.seed_row(1, "B", "B", Value::I64(0));
    start_batch(&mut app, &[a, b]);
    app.disconnect_session(1);
    assert_eq!(app.pending_writes, 0, "no reply can arrive any more");
    assert_eq!((app.write_ok, app.write_err), (0, 2));
    assert!(app.last_write_summary.is_some());
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

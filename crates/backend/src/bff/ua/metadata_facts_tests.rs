//! Live SDK attributes through the real Gateway; no mock UA data plane.
use super::live_tests::{Harness, PreparedMetadataServer, TestServer};
use crate::bff::{Gateway, model::*};
use opcua::server::address_space::{AccessLevel, VariableBuilder};
use opcua::server::node_manager::memory::SimpleNodeManager;
use opcua::types::{DataTypeId, NodeId, ObjectId, Variant};
use std::time::{Duration, Instant};

fn reply(gateway: &Gateway, request: RequestId) -> EventKind {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some(event) = gateway.try_recv() {
            if event.origin == Origin::Reply(request) {
                return event.kind;
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    panic!("timed out waiting for correlated Gateway reply")
}

#[test]
fn gateway_uses_user_access_level_not_generic_access_level() {
    let server = TestServer::start(48692);
    let manager = server
        .handle
        .node_managers()
        .get_of_type::<SimpleNodeManager>()
        .unwrap();
    let variable = VariableBuilder::new(
        &NodeId::new(server.ns, "Restricted"),
        "Restricted",
        "Restricted",
    )
    .data_type(DataTypeId::Int32)
    .value(Variant::Int32(7))
    .access_level(AccessLevel::CURRENT_READ | AccessLevel::CURRENT_WRITE)
    .user_access_level(AccessLevel::CURRENT_READ)
    .build();
    manager
        .address_space()
        .write()
        .add_variables(vec![variable], &ObjectId::ObjectsFolder.into());
    let session = SessionId::FIRST;
    let mut gateway = Gateway::new();
    let connect = RequestId::new(1).unwrap();
    gateway
        .send(Command {
            request_id: Some(connect),
            owner: session.into(),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: Harness::endpoint(&server.url),
            },
        })
        .unwrap();
    assert!(matches!(
        reply(&gateway, connect),
        EventKind::Connected { .. }
    ));
    let add = RequestId::new(2).unwrap();
    gateway
        .send(Command {
            request_id: Some(add),
            owner: session.into(),
            protocol: Protocol::OpcUa,
            kind: CommandKind::AddItems {
                items: vec![ItemSpec {
                    key: ItemKey::FIRST,
                    item_id: format!("ns={};s=Restricted", server.ns),
                }],
            },
        })
        .unwrap();
    let EventKind::AddItems { results } = reply(&gateway, add) else {
        panic!("expected AddItems")
    };
    let info = results[0].1.as_ref().unwrap();
    assert!(
        !info.writable,
        "generic AccessLevel permits writing, but UserAccessLevel does not"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut observed = None;
    while Instant::now() < deadline {
        if let Some(event) = gateway.try_recv() {
            if let EventKind::UaItemMetadata { facts } = event.kind {
                observed = facts.into_iter().find(|fact| fact.key == ItemKey::FIRST);
                if observed.is_some() {
                    break;
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let fact = observed.expect("scoped SDK UserAccessLevel fact after AddItems acknowledgement");
    assert_eq!(fact.user_access_level, Some(1));
    assert_eq!(fact.type_id, Some(6));
    let _ = gateway.shutdown();
}

#[test]
fn gateway_recovery_reloads_metadata_in_new_sdk_revision() {
    let server = TestServer::start(48693);
    let port = server.port;
    let endpoint = Harness::endpoint(&server.url);
    // Generate replacement keys and namespace while the old server remains
    // online. Starting its listener still waits for the real Lost barrier.
    let prepared = PreparedMetadataServer::prepare(port);
    let session = SessionId::FIRST;
    let mut gateway = Gateway::new();
    let connect = RequestId::new(1).unwrap();
    gateway
        .send(Command {
            request_id: Some(connect),
            owner: session.into(),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect { endpoint },
        })
        .unwrap();
    assert!(matches!(
        reply(&gateway, connect),
        EventKind::Connected { .. }
    ));
    let add = RequestId::new(2).unwrap();
    gateway
        .send(Command {
            request_id: Some(add),
            owner: session.into(),
            protocol: Protocol::OpcUa,
            kind: CommandKind::AddItems {
                items: vec![ItemSpec {
                    key: ItemKey::FIRST,
                    item_id: server.node("Level"),
                }],
            },
        })
        .unwrap();
    assert!(
        matches!(reply(&gateway, add), EventKind::AddItems { results } if results[0].1.is_ok())
    );
    drop(server);
    // A fast replacement can bind before the SDK observes the disconnect.
    // Wait for the real Lost event before starting it; no timing guess proves it.
    let lost_deadline = Instant::now() + Duration::from_secs(45);
    let mut lost_revision = None;
    let mut transitions = Vec::new();
    while Instant::now() < lost_deadline {
        if let Some(event) = gateway.try_recv() {
            if let (
                Origin::UaSession {
                    connection,
                    revision,
                },
                EventKind::UaRecovery { state },
            ) = (&event.origin, &event.kind)
                && *connection == connect
            {
                if transitions.len() < 16 {
                    transitions.push((*revision, format!("{state:?}")));
                }
                if matches!(state, UaRecoveryState::ConnectionLost { .. }) && *revision > 0 {
                    lost_revision = Some(*revision);
                    break;
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let lost_revision = lost_revision.expect("SDK did not observe the stopped sample server");
    let replacement = prepared.start();
    assert_eq!(
        replacement.port, port,
        "recovery fixture must retain endpoint"
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut reconnect_revision = None;
    let mut recovered = None;
    while Instant::now() < deadline {
        if let Some(event) = gateway.try_recv() {
            if let (
                Origin::UaSession {
                    connection,
                    revision,
                },
                EventKind::UaRecovery { state },
            ) = (&event.origin, &event.kind)
                && *connection == connect
                && transitions.len() < 16
            {
                transitions.push((*revision, format!("{state:?}")));
            }
            match (event.origin, event.kind) {
                (
                    Origin::UaSession {
                        connection,
                        revision,
                    },
                    EventKind::UaRecovery {
                        state: UaRecoveryState::Reconnected { .. },
                    },
                ) if connection == connect && revision > lost_revision => {
                    reconnect_revision = Some(revision)
                }
                (
                    Origin::UaSession {
                        connection,
                        revision,
                    },
                    EventKind::UaItemMetadata { facts },
                ) if connection == connect && Some(revision) == reconnect_revision => {
                    if let Some(fact) = facts.into_iter().find(|fact| fact.key == ItemKey::FIRST) {
                        recovered = Some(fact);
                        break;
                    }
                }
                (
                    Origin::UaSession { connection, .. },
                    EventKind::UaRecovery {
                        state: UaRecoveryState::Stopped { .. },
                    },
                ) if connection == connect => break,
                _ => {}
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(
        reconnect_revision.is_some(),
        "missing actual SDK recovery transition; observed {transitions:?}"
    );
    let fact = recovered.unwrap_or_else(|| {
        panic!("missing scoped post-recovery metadata; observed {transitions:?}")
    });
    assert_eq!(fact.user_access_level, Some(3));
    assert_eq!(fact.data_type.as_deref(), Some("Double"));
    let _ = gateway.shutdown();
    drop(replacement);
}

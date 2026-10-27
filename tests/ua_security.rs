//! Live (ignored) tests covering the UA security features: endpoint discovery
//! before connect, explicit security policy/mode, session timeout and mDNS/LDS
//! network search. Run with an OPC UA server at OPCUA_TEST_URL:
//!
//!   cargo test --test ua_security -- --ignored
use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{Command, CommandKind, EndpointSpec, Event, EventKind, Protocol};
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "opc.tcp://127.0.0.1:4840";

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn test_url() -> String {
    std::env::var("OPCUA_TEST_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn wait_for(
    gateway: &Gateway,
    command: Command,
    timeout: Duration,
    predicate: impl Fn(&EventKind) -> bool,
) -> Option<Event> {
    let request_id = command.request_id;
    let session = command.session;
    gateway.send(command).expect("gateway send");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.request_id != request_id || event.session != session {
                continue;
            }
            if predicate(&event.kind) {
                return Some(event);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

/// Connect with the given endpoint, returning the `Connected` event.
fn connect_endpoint(gateway: &Gateway, session: u64, endpoint: EndpointSpec) -> Event {
    wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect { endpoint },
        },
        Duration::from_secs(40),
        |kind| matches!(kind, EventKind::Connected { .. } | EventKind::Error { .. }),
    )
    .expect("connect timeout")
}

fn disconnect(gateway: &Gateway, session: u64) {
    let _ = wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        },
        Duration::from_secs(5),
        |kind| matches!(kind, EventKind::Disconnected { .. }),
    );
}

/// Endpoint discovery returns the server's advertised endpoints, including a
/// usable default (the exact set depends on the server).
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn server_search_enumerates_endpoints() {
    let gateway = Gateway::new();
    let session = next_id();
    let events = wait_for(
        &gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::SearchUaServers {
                host: "127.0.0.1".to_string(),
                port: 4840,
                bind: None,
            },
        },
        Duration::from_secs(60),
        |kind| {
            matches!(
                kind,
                EventKind::UaServersFound { .. } | EventKind::Error { .. }
            )
        },
    )
    .expect("discovery timeout");
    match events.kind {
        EventKind::UaServersFound { servers, .. } => {
            assert!(!servers.is_empty(), "no servers discovered");
            let endpoints_ok = servers.iter().all(|s| !s.endpoints.is_empty());
            assert!(endpoints_ok, "discovered server(s) expose no endpoints");
        }
        EventKind::Error { message, .. } => panic!("discovery failed: {message}"),
        other => panic!("unexpected event: {other:?}"),
    }
}

/// Explicitly requesting security policy None + mode None connects (matches the
/// default CODESYS configuration and proves the explicit policy path works).
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn server_connect_explicit_none_policy() {
    let gateway = Gateway::new();
    let session = next_id();
    let mut endpoint = EndpointSpec::ua(test_url());
    endpoint.ua_security_policy = Some(opcda_egui::bff::ua::SECURITY_POLICY_NONE);
    endpoint.ua_security_mode = Some(opcda_egui::bff::ua::SECURITY_MODE_NONE);
    endpoint.ua_trust_server_cert = true;
    endpoint.ua_session_timeout = Some(90_000);
    let connected = connect_endpoint(&gateway, session, endpoint);
    match connected.kind {
        EventKind::Connected { endpoint, info } => {
            assert_eq!(endpoint.ua_session_timeout, Some(90_000));
            let info = info.to_string();
            assert!(
                info.contains("None"),
                "session info should include the resolved policy: {info}"
            );
        }
        EventKind::Error { message, .. } => panic!("connect failed: {message}"),
        other => panic!("unexpected event: {other:?}"),
    }
    disconnect(&gateway, session);
}

/// The default (Auto/Auto) connect resolves against the server and succeeds,
/// and the session info embeds the resolved security policy/mode.
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn server_connect_auto_resolves_security() {
    let gateway = Gateway::new();
    let session = next_id();
    let connected = connect_endpoint(&gateway, session, EndpointSpec::ua(test_url()));
    match connected.kind {
        EventKind::Connected { info, .. } => {
            let info = info.to_string();
            assert!(
                info.contains('[') && info.contains('/'),
                "session info should describe policy/mode: {info}"
            );
        }
        EventKind::Error { message, .. } => panic!("connect failed: {message}"),
        other => panic!("unexpected event: {other:?}"),
    }
    disconnect(&gateway, session);
}

/// mDNS/LDS network search finds the server (or at least reaches the discovery
/// endpoint without a hard error) via `FindServersOnNetwork`.
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn server_network_search_reaches_server() {
    let gateway = Gateway::new();
    let session = next_id();
    let events = wait_for(
        &gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::SearchUaNetwork {
                bind: None,
                lds: None,
            },
        },
        Duration::from_secs(60),
        |kind| matches!(kind, EventKind::UaNetworkFound { .. }),
    )
    .expect("network search timeout");
    match events.kind {
        EventKind::UaNetworkFound { servers } => {
            let _ = servers;
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

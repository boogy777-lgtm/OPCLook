//! Live (ignored) tests for the typed UA security selection.
//!
//! Run with an OPC UA server at OPCUA_TEST_URL:
//!
//!   cargo test --test security_live -- --ignored
use opclook::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, Protocol, UaSecurityMode,
    UaSecurityPolicy,
};
use opclook_backend::bff::Gateway;
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
    let owner = command.owner;
    gateway.send(command).expect("gateway send");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.origin != request_id.into() || event.owner != owner {
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

fn connect_endpoint(gateway: &Gateway, session: u64, endpoint: EndpointSpec) -> Event {
    wait_for(
        gateway,
        Command {
            request_id: opclook::bff::model::RequestId::new(next_id()),
            owner: opclook::bff::model::Owner::Session(
                opclook::bff::model::SessionId::new(session).unwrap(),
            ),
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
            request_id: opclook::bff::model::RequestId::new(next_id()),
            owner: opclook::bff::model::Owner::Session(
                opclook::bff::model::SessionId::new(session).unwrap(),
            ),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        },
        Duration::from_secs(5),
        |kind| matches!(kind, EventKind::Disconnected { .. }),
    );
}

/// Auto/Auto still resolves against the live server and the info line names
/// the chosen policy (the default CODESYS advertises None, so Auto picks it).
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn live_auto_security_still_connects() {
    let gateway = Gateway::new();
    let session = next_id();
    let mut endpoint = EndpointSpec::ua(test_url());
    endpoint.ua_security_policy = UaSecurityPolicy::Auto;
    endpoint.ua_security_mode = UaSecurityMode::Auto;
    endpoint.ua_trust_server_cert = true;
    let event = connect_endpoint(&gateway, session, endpoint);
    match event.kind {
        EventKind::Connected { info, .. } => {
            let info = opclook::i18n::backend_text(opclook::i18n::Lang::En, &info);
            assert!(
                info.contains("None") || info.contains("Basic256"),
                "info should name the resolved policy: {info}"
            );
        }
        EventKind::Error { message, .. } => panic!("auto connect failed: {message:?}"),
        other => panic!("unexpected event: {other:?}"),
    }
    disconnect(&gateway, session);
}

/// Explicit Basic256Sha256 + SignAndEncrypt is NOT silently downgraded when the
/// server (CODESYS default) does not advertise it: the connect returns a clear
/// security error naming the advertised endpoints.
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn live_explicit_unadvertised_security_is_an_error() {
    let gateway = Gateway::new();
    let session = next_id();
    let mut endpoint = EndpointSpec::ua(test_url());
    endpoint.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
    endpoint.ua_security_mode = UaSecurityMode::SignAndEncrypt;
    endpoint.ua_trust_server_cert = true;
    let event = connect_endpoint(&gateway, session, endpoint);
    match event.kind {
        EventKind::Error { message, .. } => {
            let opclook::bff::model::BackendMsg::SecurityResolveFailed { detail } = &message else {
                panic!("expected the security-resolution error, got: {message:?}");
            };
            assert!(
                detail.contains("not offered"),
                "expected a clear 'not offered' detail, got: {message:?}"
            );
            assert!(
                detail.contains("advertised:"),
                "error must list the advertised endpoints, got: {message:?}"
            );
        }
        EventKind::Connected { info, .. } => {
            panic!(
                "server actually advertises Basic256Sha256/SignAndEncrypt; \
                 this test targets a None-only CODESYS endpoint (connected: {info:?})"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

//! Live integration tests for OPC UA server discovery (`SearchUaServers`).
//! These require a running OPC UA server (the test rig starts CODESYS at
//! 127.0.0.1:4840). Run with:
//!   cargo test --test ua_search -- --ignored --nocapture
use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{Command, CommandKind, Event, EventKind, Protocol};
use std::time::{Duration, Instant};

const DEFAULT_PORT: u16 = 4840;
const DEFAULT_CLIENT_URL: &str = "https://www.codesys.com/";

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(100);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn client_url() -> String {
    std::env::var("OPCUA_CLIENT_URL").unwrap_or_else(|_| DEFAULT_CLIENT_URL.to_string())
}

fn port() -> u16 {
    std::env::var("OPCUA_TEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn wait_for_servers(gateway: &Gateway, host: &str, port: u16, timeout: Duration) -> Option<Event> {
    let request_id = next_id();
    gateway
        .send(Command {
            request_id: opcda_egui::bff::model::RequestId::new(request_id),
            session: 0,
            protocol: Protocol::OpcUa,
            kind: CommandKind::SearchUaServers {
                host: host.to_string(),
                port,
                bind: None,
            },
        })
        .expect("gateway send");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.origin
                == opcda_egui::bff::model::Origin::Reply(
                    opcda_egui::bff::model::RequestId::new(request_id).unwrap(),
                )
            {
                return Some(event);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// Find OPC UA servers on 127.0.0.1:4840.
#[test]
#[ignore = "requires OPC UA server at 127.0.0.1:4840"]
fn search_ua_servers_on_loopback() {
    let gateway = Gateway::new();
    let c_url = client_url();
    let event = wait_for_servers(&gateway, "127.0.0.1", port(), Duration::from_secs(25))
        .expect("search timeout (is the UA server running?)");
    match event.kind {
        EventKind::UaServersFound { servers, .. } => {
            assert!(!servers.is_empty(), "expected at least one OPC UA server");
            for server in &servers {
                println!(
                    "server: application_name={:?} application_uri={:?} endpoints={}",
                    server.application_name,
                    server.application_uri,
                    server.endpoints.len()
                );
                for ep in &server.endpoints {
                    println!(
                        "    endpoint: url={} policy={} mode={}",
                        ep.url, ep.security_policy, ep.security_mode
                    );
                }
            }
            // The rig's CODESYS server describes itself (application_uri is the
            // CODESYS Control URI; the check is case-insensitive).
            let known = servers.iter().any(|s| {
                s.application_uri.to_ascii_lowercase().contains("codesys")
                    || s.application_name.to_ascii_lowercase().contains("codesys")
                    || s.application_uri == c_url
            });
            assert!(known, "expected a CODESYS server, got: {servers:#?}");
            let has_tcp = servers
                .iter()
                .flat_map(|s| s.endpoints.iter())
                .any(|ep| ep.url.starts_with("opc.tcp://"));
            assert!(has_tcp, "expected at least one opc.tcp:// endpoint");
        }
        other => panic!("expected UaServersFound, got {other:?}"),
    }
}

/// "localhost" must resolve with IPv4 first; the IPv4-first ordering fix in the
/// worker lets this find a server that listens on 127.0.0.1.
#[test]
#[ignore = "requires OPC UA server at 127.0.0.1:4840"]
fn search_ua_servers_via_localhost_resolves_ipv4_first() {
    let gateway = Gateway::new();
    let event = wait_for_servers(&gateway, "localhost", port(), Duration::from_secs(25))
        .expect("search via localhost timeout");
    match event.kind {
        EventKind::UaServersFound { servers, .. } => {
            assert!(
                !servers.is_empty(),
                "expected servers via 'localhost' (IPv4-first ordering)"
            );
            println!("via localhost found {} server(s)", servers.len());
        }
        other => panic!("expected UaServersFound via localhost, got {other:?}"),
    }
}

//! End-to-end CLI regression against a real local OPC UA sample server.
#![cfg(unix)]
use opcua::server::ServerBuilder;
use opcua::server::address_space::{AccessLevel, VariableBuilder};
use opcua::server::diagnostics::NamespaceMetadata;
use opcua::server::node_manager::memory::{SimpleNodeManager, simple_node_manager};
use opcua::types::{DataTypeId, DataValue, NodeId, ObjectId, Variant};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const NS: &str = "urn:opclook:cli-live";

// Watch emits scoped SDK facts alongside values. They do not consume --count.
fn assert_ua_watch_fact(record: &serde_json::Value) {
    if let Some(metadata) = record.get("ua_item_metadata") {
        assert_eq!(metadata["key"], 1, "{record}");
        assert!(metadata["data_type"].is_string(), "{record}");
        assert_eq!(metadata["user_access_level"], 3, "{record}");
        assert_eq!(metadata["writable"], true, "{record}");
        assert!(metadata["diagnostic"].is_null(), "{record}");
    } else if let Some(parameters) = record.get("ua_subscription_parameters") {
        assert!(parameters.is_array(), "{record}");
    } else {
        panic!("unexpected watch record: {record}");
    }
}
struct Server {
    runtime: tokio::runtime::Runtime,
    handle: opcua::server::ServerHandle,
    nm: std::sync::Arc<SimpleNodeManager>,
    url: String,
    node: String,
    namespace: u16,
    dir: std::path::PathBuf,
}
impl Server {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let dir =
            std::env::temp_dir().join(format!("opclook-cli-live-{}-{port}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let url = format!("opc.tcp://127.0.0.1:{port}/");
        let guard = runtime.enter();
        let (server, handle) = ServerBuilder::new_sample()
            .host("127.0.0.1")
            .port(port)
            .pki_dir(dir.join("server"))
            .trust_client_certs(true)
            .discovery_urls(vec![url.clone()])
            .with_node_manager(simple_node_manager(
                NamespaceMetadata {
                    namespace_uri: NS.into(),
                    ..Default::default()
                },
                "cli-live",
            ))
            .build()
            .unwrap();
        drop(guard);
        let running = runtime.spawn(server.run());
        let namespace = handle.get_namespace_index(NS).unwrap();
        let nm = handle
            .node_managers()
            .get_of_type::<SimpleNodeManager>()
            .unwrap();
        let node_id = NodeId::new(namespace, "Level");
        {
            let space = nm.address_space();
            let mut space = space.write();
            let rw = AccessLevel::CURRENT_READ | AccessLevel::CURRENT_WRITE;
            let var = VariableBuilder::new(&node_id, "Level", "Level")
                .data_type(DataTypeId::Double)
                .value(Variant::Double(1.0))
                .access_level(rw)
                .user_access_level(rw)
                .build();
            space.add_variables(vec![var], &ObjectId::ObjectsFolder.into());
        }
        let until = Instant::now() + Duration::from_secs(90);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                !running.is_finished(),
                "sample server exited during startup"
            );
            assert!(Instant::now() < until, "sample server did not listen");
            std::thread::sleep(Duration::from_millis(50));
        }
        Self {
            runtime,
            handle,
            nm,
            url,
            node: format!("ns={namespace};s=Level"),
            namespace,
            dir,
        }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_opclook-cli"));
        command
            .env("APPDATA", self.dir.join("client"))
            .args([
                "--ua",
                &self.url,
                "--policy",
                "None",
                "--mode",
                "None",
                "--timeout",
                "60",
            ])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn execute(&self, args: &[&str]) -> serde_json::Value {
        let mut child = self.command(args).spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("CLI timed out: {args:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "CLI {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn set(&self, value: f64) {
        self.nm
            .set_value(
                self.handle.subscriptions(),
                &NodeId::new(self.namespace, "Level"),
                None,
                DataValue::new_now(Variant::Double(value)),
            )
            .unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.handle.cancel();
        let _ = &self.runtime;
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn browse_read_write_push_forced_poll_and_idle_interrupt() {
    let server = Server::start();
    let browse = server.execute(&["browse", "i=85"]);
    assert!(
        browse["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["node_id"] == server.node)
    );
    let first = server.execute(&["read", &server.node]);
    assert_eq!(first["data"]["value"]["value"], 1.0);
    assert!(first["data"]["quality"]["kind"].is_string());
    assert!(first["data"]["timestamp"].is_string());
    assert_eq!(
        server.execute(&["write", &server.node, "f64", "2.5"])["accepted"],
        true
    );
    assert_eq!(
        server.execute(&["read", &server.node])["data"]["value"]["value"],
        2.5
    );
    let observed = server.execute(&["--read-after-write", "write", &server.node, "f64", "2.5"]);
    assert_eq!(observed["accepted"], true);
    assert_eq!(observed["readback"]["status"], "observed");
    assert_eq!(observed["readback"]["data"]["value"]["value"], 2.5);

    let poll = server
        .command(&["--poll", "--count", "3", "watch", &server.node])
        .spawn()
        .unwrap();
    let mut poll = Running(poll);
    let until = Instant::now() + Duration::from_secs(20);
    while poll.0.try_wait().unwrap().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        poll.0.try_wait().unwrap().is_some(),
        "forced polls did not finish"
    );
    let stdout = poll.0.stdout.take().unwrap();
    let records: Vec<serde_json::Value> = BufReader::new(stdout)
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    let values: Vec<_> = records
        .iter()
        .filter(|record| record.get("data").is_some())
        .collect();
    assert_eq!(values.len(), 3, "{records:?}");
    assert!(
        values
            .iter()
            .all(|record| record["data"]["value"]["value"] == 2.5)
    );
    let facts: Vec<_> = records
        .iter()
        .filter(|record| record.get("data").is_none())
        .collect();
    assert!(
        facts
            .iter()
            .any(|record| record.get("ua_item_metadata").is_some()),
        "{records:?}"
    );
    for fact in facts {
        assert_ua_watch_fact(fact);
    }
    assert!(poll.0.wait().unwrap().success());

    // Warm the client PKI above; now a constant push watch must survive well
    // beyond its one-second per-operation timeout without inventing a timeout.
    let mut watch = server
        .command(&["--timeout", "1", "watch", &server.node])
        .spawn()
        .unwrap();
    let stdout = watch.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut watch = Running(watch);
    let first_deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = first_deadline.saturating_duration_since(Instant::now());
        let first: serde_json::Value =
            serde_json::from_str(&rx.recv_timeout(remaining).unwrap().unwrap()).unwrap();
        if first.get("data").is_some() {
            assert_eq!(first["data"]["value"]["value"], 2.5);
            break;
        }
        assert_ua_watch_fact(&first);
    }
    std::thread::sleep(Duration::from_secs(3));
    assert!(watch.0.try_wait().unwrap().is_none(), "idle watch exited");
    server.set(3.5);
    // A one-shot initial snapshot can race with the first subscription push.
    // Both are legitimate observations; continue until the change arrives.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let observed: serde_json::Value =
            serde_json::from_str(&rx.recv_timeout(remaining).unwrap().unwrap()).unwrap();
        if observed.get("data").is_none() {
            assert_ua_watch_fact(&observed);
            continue;
        }
        let value = &observed["data"]["value"]["value"];
        if *value == 3.5 {
            break;
        }
        assert_eq!(*value, 2.5);
    }
    assert!(
        Command::new("kill")
            .args(["-INT", &watch.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let until = Instant::now() + Duration::from_secs(15);
    while watch.0.try_wait().unwrap().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        watch
            .0
            .try_wait()
            .unwrap()
            .expect("Ctrl+C did not stop watch")
            .success()
    );
}

#[test]
fn credential_errors_are_redacted_and_closed_pipe_is_clean() {
    let dir = std::env::temp_dir().join(format!("opclook-cli-redact-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let user = "redaction-user-4937";
    let password = "redaction-password-5291";
    let mut child = Command::new(env!("CARGO_BIN_EXE_opclook-cli"))
        .env("APPDATA", &dir)
        .env("CLI_TEST_USER", user)
        .env("CLI_TEST_PASSWORD", password)
        .args([
            "--ua",
            "opc.tcp://127.0.0.1:1/",
            "--timeout",
            "2",
            "--user-env",
            "CLI_TEST_USER",
            "--password-env",
            "CLI_TEST_PASSWORD",
            "browse",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    if child.try_wait().unwrap().is_none() {
        let _ = child.kill();
        panic!("credential failure did not finish");
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(user));
        assert!(!text.contains(password));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_opclook-cli"))
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

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
                "15",
            ])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn execute(&self, args: &[&str]) -> serde_json::Value {
        let mut child = self.command(args).spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(40);
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
        server.execute(&["write", &server.node, "f64", "2.5"])["written"],
        true
    );
    assert_eq!(
        server.execute(&["read", &server.node])["data"]["value"]["value"],
        2.5
    );

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
    assert_eq!(records.len(), 3);
    assert!(records.iter().all(|r| r["data"]["value"]["value"] == 2.5));
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
    let first: serde_json::Value =
        serde_json::from_str(&rx.recv_timeout(Duration::from_secs(20)).unwrap().unwrap()).unwrap();
    assert_eq!(first["data"]["value"]["value"], 2.5);
    std::thread::sleep(Duration::from_secs(3));
    assert!(watch.0.try_wait().unwrap().is_none(), "idle watch exited");
    server.set(3.5);
    let pushed: serde_json::Value =
        serde_json::from_str(&rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap()).unwrap();
    assert_eq!(pushed["data"]["value"]["value"], 3.5);
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

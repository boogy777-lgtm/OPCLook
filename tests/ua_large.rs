//! Live (ignored) stress test for OPC UA request sizing.
//!
//! Regression for the CODESYS UA-stack errors seen on 2026-10-25:
//!   `OpcUa_TcpStream_Flush: ... Too many chunks!` / `0x80800000`
//! A single Browse/Read response was larger than the server's
//! `MaxChunkCount = 5`. This test drives the real worker against a live server:
//!
//! 1. browse the reachable Objects tree (many branches, capped page size),
//! 2. pick the branch with the most direct variables,
//! 3. add 100+ items in one AddItems command,
//! 4. read all of them in ONE Read command (the worker splits it into
//!    `READ_MAX_ITEMS_PER_CALL` service calls internally),
//! 5. run 5 connect/browse/read/disconnect cycles.
//!
//! Run:  cargo test --test ua_large -- --ignored --nocapture
//!
//! After the process exits there must be no ESTABLISHED client connection to
//! port 4840 (`netstat -ano | findstr 4840`).
use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    BrowseNode, Command, CommandKind, DataValue, EndpointSpec, Event, EventKind, ItemSpec,
    NodeKind, Protocol, QualityKind,
};
use opcda_egui::constants::{BROWSE_MAX_REFERENCES_PER_NODE, READ_MAX_ITEMS_PER_CALL};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "opc.tcp://127.0.0.1:4840";
const CYCLES: u32 = 5;
const ITEMS_TARGET: usize = 120;
const MAX_BRANCHES_BROWSED: usize = 120;
const MAX_BRANCH_DEPTH: u8 = 6;

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
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn connect(gateway: &Gateway, session: u64) -> Event {
    wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::ua(test_url()),
            },
        },
        Duration::from_secs(40),
        |kind| matches!(kind, EventKind::Connected { .. } | EventKind::Error { .. }),
    )
    .expect("connect timeout (server up on 127.0.0.1:4840?)")
}

fn disconnect(gateway: &Gateway, session: u64) {
    let event = wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        },
        Duration::from_secs(15),
        |kind| {
            matches!(
                kind,
                EventKind::Disconnected { .. } | EventKind::Error { .. }
            )
        },
    )
    .expect("disconnect timeout");
    assert!(
        matches!(event.kind, EventKind::Disconnected { .. }),
        "disconnect failed: {event:?}"
    );
}

/// Browse one node; panics on timeout/Error so a live failure is loud.
fn browse(gateway: &Gateway, session: u64, node_id: &str) -> Vec<BrowseNode> {
    let event = wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Browse {
                node_id: node_id.to_string(),
                filter: opcda_egui::bff::model::NodeClassFilter::All,
                view_id: None,
            },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. }),
    )
    .unwrap_or_else(|| panic!("browse timeout for {node_id}"));
    match event.kind {
        EventKind::Browse { children, .. } => children,
        EventKind::Error { message, .. } => {
            panic!("browse failed for {node_id}: {message:?}")
        }
        other => panic!("browse {node_id}: unexpected event {other:?}"),
    }
}

#[derive(Debug)]
struct TreeScan {
    items: Vec<(String, String)>,
    largest_branch: (String, usize),
    largest_items: Vec<String>,
    branches_browsed: usize,
    browse_calls: usize,
    max_children: usize,
    max_children_branch: String,
    top_branches: Vec<(usize, String)>,
}

/// Breadth-first scan of the reachable Objects tree. Stops after
/// `MAX_BRANCHES_BROWSED` non-empty branch pages or when the queue is empty.
fn scan_tree(gateway: &Gateway, session: u64) -> TreeScan {
    let mut queue: VecDeque<(String, String, u8)> = VecDeque::new();
    let mut seen_branches: HashSet<String> = HashSet::new();
    let mut seen_items: HashSet<String> = HashSet::new();
    let mut items: Vec<(String, String)> = Vec::new();
    let mut largest_branch = (String::new(), 0usize);
    let mut largest_items: Vec<String> = Vec::new();
    let mut branches_browsed = 0usize;
    let mut browse_calls = 0usize;
    let mut max_children = 0usize;
    let mut max_children_branch = String::new();
    let mut top_branches: Vec<(usize, String)> = Vec::new();

    queue.push_back(("Objects".to_string(), "i=85".to_string(), 0));
    seen_branches.insert("i=85".to_string());

    while let Some((label, node_id, depth)) = queue.pop_front() {
        if branches_browsed >= MAX_BRANCHES_BROWSED {
            break;
        }
        branches_browsed += 1;
        browse_calls += 1;
        let children = browse(gateway, session, &node_id);
        if children.len() > max_children {
            max_children = children.len();
            max_children_branch = label.clone();
        }

        let mut direct_items = 0usize;
        let mut direct_ids: Vec<String> = Vec::new();
        for child in children {
            match child.kind {
                NodeKind::Item => {
                    direct_items += 1;
                    direct_ids.push(child.node_id.clone());
                    if seen_items.insert(child.node_id.clone()) {
                        items.push((child.node_id, child.name));
                    }
                }
                NodeKind::Branch => {
                    if depth < MAX_BRANCH_DEPTH && seen_branches.insert(child.node_id.clone()) {
                        let child_label = format!("{label}/{}", child.name);
                        queue.push_back((child_label, child.node_id, depth + 1));
                    }
                }
            }
        }
        if direct_items > largest_branch.1 {
            largest_branch = (label.clone(), direct_items);
            largest_items = direct_ids;
        }
        top_branches.push((direct_items, label));
    }

    top_branches.sort_by(|a, b| b.0.cmp(&a.0));
    top_branches.truncate(5);

    TreeScan {
        items,
        largest_branch,
        largest_items,
        branches_browsed,
        browse_calls,
        max_children,
        max_children_branch,
        top_branches,
    }
}

fn add_items(gateway: &Gateway, session: u64, items: &[ItemSpec]) {
    let event = wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::AddItems {
                items: items.to_vec(),
            },
        },
        Duration::from_secs(40),
        |kind| matches!(kind, EventKind::AddItems { .. } | EventKind::Error { .. }),
    )
    .expect("add items timeout");
    match event.kind {
        EventKind::AddItems { results } => {
            assert_eq!(
                results.len(),
                items.len(),
                "AddItems returned the wrong count"
            );
            for (key, result) in &results {
                assert!(result.is_ok(), "add item key {key} failed: {result:?}");
            }
        }
        EventKind::Error { message, .. } => panic!("add items failed: {message:?}"),
        other => panic!("add items: unexpected event {other:?}"),
    }
}

fn read_all(gateway: &Gateway, session: u64, keys: &[u32]) -> (Vec<(u32, DataValue)>, Duration) {
    let started = Instant::now();
    let event = wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Read {
                keys: keys.to_vec(),
            },
        },
        Duration::from_secs(40),
        |kind| matches!(kind, EventKind::Values { .. } | EventKind::Error { .. }),
    )
    .expect("read timeout");
    let elapsed = started.elapsed();
    match event.kind {
        EventKind::Values { values } => (values, elapsed),
        EventKind::Error { message, .. } => panic!("read failed: {message:?}"),
        other => panic!("read: unexpected event {other:?}"),
    }
}

/// Sum of the debug-formatted values is a stable lower-bound proxy for the
/// response payload (the client model is what the UI receives).
fn approx_payload_bytes(values: &[(u32, DataValue)]) -> usize {
    values
        .iter()
        .map(|(_, dv)| format!("{:?}", dv.value).len())
        .sum()
}

#[test]
#[ignore = "requires OPC UA server at 127.0.0.1:4840"]
fn ua_large_browse_read_and_cycles() {
    let gateway = Gateway::new();
    let mut session = next_id();

    let connected = connect(&gateway, session);
    assert!(
        matches!(connected.kind, EventKind::Connected { .. }),
        "connection failed: {connected:?}"
    );

    let scan_started = Instant::now();
    let scan = scan_tree(&gateway, session);
    let scan_elapsed = scan_started.elapsed();
    println!(
        "[ua_large] scan: {} browse calls / {} branches in {:?}; {} unique items; \
         max nodes returned by one Browse command = {} at {}; largest branch = {} ({} direct items)",
        scan.browse_calls,
        scan.branches_browsed,
        scan_elapsed,
        scan.items.len(),
        scan.max_children,
        scan.max_children_branch,
        scan.largest_branch.0,
        scan.largest_branch.1,
    );
    println!(
        "[ua_large] top branches by direct items: {:?}",
        scan.top_branches
    );
    let page_cap = BROWSE_MAX_REFERENCES_PER_NODE as usize;
    if scan.largest_branch.1 > page_cap {
        println!(
            "[ua_large] continuation verified: largest branch returned {} direct refs \
             with page cap {page_cap}",
            scan.largest_branch.1
        );
    } else {
        println!("[ua_large] note: no scanned branch exceeded the {page_cap}-reference page cap");
    }
    assert!(
        scan.items.len() >= ITEMS_TARGET,
        "server exposed fewer than {ITEMS_TARGET} readable variables ({} found); \
         cannot exercise the large-read path",
        scan.items.len()
    );

    // Prefer variables from the branch with the most direct children, then fill
    // from the rest of the scan so we always exceed 100 items.
    let take = scan.items.len().min(ITEMS_TARGET.max(100));
    let mut chosen: Vec<String> = Vec::with_capacity(take);
    let mut chosen_set: HashSet<String> = HashSet::new();
    for node_id in &scan.largest_items {
        if chosen.len() >= take {
            break;
        }
        if chosen_set.insert(node_id.clone()) {
            chosen.push(node_id.clone());
        }
    }
    for (node_id, _) in &scan.items {
        if chosen.len() >= take {
            break;
        }
        if chosen_set.insert(node_id.clone()) {
            chosen.push(node_id.clone());
        }
    }
    let specs: Vec<ItemSpec> = chosen
        .iter()
        .enumerate()
        .map(|(index, node_id)| ItemSpec {
            key: (index + 1) as u32,
            item_id: node_id.clone(),
        })
        .collect();
    let keys: Vec<u32> = specs.iter().map(|s| s.key).collect();

    let add_started = Instant::now();
    add_items(&gateway, session, &specs);
    println!(
        "[ua_large] AddItems: {} items in one command in {:?}",
        specs.len(),
        add_started.elapsed()
    );

    let (values, read_elapsed) = read_all(&gateway, session, &keys);
    let good = values
        .iter()
        .filter(|(_, dv)| dv.quality.kind == QualityKind::Good)
        .count();
    let missing: Vec<u32> = keys
        .iter()
        .copied()
        .filter(|key| !values.iter().any(|(k, _)| k == key))
        .collect();
    println!(
        "[ua_large] Read: {} keys in ONE command -> {} values in {:?} \
         (chunks of {READ_MAX_ITEMS_PER_CALL}); Good = {}; approx payload = {} bytes",
        keys.len(),
        values.len(),
        read_elapsed,
        good,
        approx_payload_bytes(&values),
    );
    assert_eq!(
        values.len(),
        keys.len(),
        "Read returned {} values for {} keys",
        values.len(),
        keys.len()
    );
    assert!(missing.is_empty(), "Read dropped keys: {missing:?}");
    assert!(
        good > 0,
        "no Good values in a {} item read: {:?}",
        values.len(),
        values
            .iter()
            .filter(|(_, dv)| dv.quality.kind != QualityKind::Good)
            .take(5)
            .map(|(k, dv)| (*k, dv.quality.text.clone()))
            .collect::<Vec<_>>()
    );

    disconnect(&gateway, session);

    // Five full connect/browse/read/disconnect cycles using the same nodes.
    for cycle in 1..=CYCLES {
        session = next_id();
        let connected = connect(&gateway, session);
        assert!(
            matches!(connected.kind, EventKind::Connected { .. }),
            "cycle {cycle}: connection failed: {connected:?}"
        );

        let children = browse(&gateway, session, "i=85");
        assert!(
            !children.is_empty(),
            "cycle {cycle}: Objects (i=85) browse returned no children"
        );

        let cycle_specs: Vec<ItemSpec> = specs
            .iter()
            .map(|spec| ItemSpec {
                key: spec.key,
                item_id: spec.item_id.clone(),
            })
            .collect();
        add_items(&gateway, session, &cycle_specs);

        let (values, elapsed) = read_all(&gateway, session, &keys);
        let good = values
            .iter()
            .filter(|(_, dv)| dv.quality.kind == QualityKind::Good)
            .count();
        println!(
            "[ua_large] cycle {cycle}: browse {} children, read {} values in {:?} (Good = {})",
            children.len(),
            values.len(),
            elapsed,
            good
        );
        assert_eq!(
            values.len(),
            keys.len(),
            "cycle {cycle}: Read returned {} values for {} keys",
            values.len(),
            keys.len()
        );
        assert!(good > 0, "cycle {cycle}: no Good values");

        disconnect(&gateway, session);
    }

    // Drain any trailing push/subscription events so nothing is left queued.
    while gateway.try_recv().is_some() {}
}

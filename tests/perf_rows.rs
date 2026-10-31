//! A4: timing of the row/tree bookkeeping with thousands of tags.
//!
//! Not part of the regular run (`#[ignore]`): wall-clock numbers depend on the
//! machine. Run with
//! `cargo test --release --test perf_rows -- --ignored --nocapture`
//! and compare before/after a change. Only public APIs are used, so the same
//! file runs unchanged against older revisions.
use opclook::app::App;
use opclook::bff::model::{
    BrowseNode, DataValue, EndpointSpec, EventKind, NodeKind, Quality, QualityKind, Value,
};
use std::time::Instant;

const N: usize = 10_000;

fn app() -> (egui::Context, App) {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.seed_session(sid(1), EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    (ctx, app)
}

fn good() -> Quality {
    Quality {
        code: 0,
        kind: QualityKind::Good,
        text: "Good".into(),
    }
}

#[test]
#[ignore = "timing measurement: cargo test --release --test perf_rows -- --ignored --nocapture"]
fn a4_ten_thousand_rows() {
    let (_ctx, mut app) = app();

    // 1. add_items: duplicate check per item.
    let items: Vec<(String, String)> = (0..N)
        .map(|i| (format!("ns=2;i={i}"), format!("Tag{i}")))
        .collect();
    let started = Instant::now();
    app.add_items(sid(1), items);
    let add_items = started.elapsed();
    assert_eq!(app.rows().len(), N);

    // 2. one value per event (what an OPC UA subscription delivers).
    let keys: Vec<_> = app.rows().iter().map(|r| r.key).collect();
    let started = Instant::now();
    for (i, key) in keys.iter().enumerate() {
        app.on_event_for_test_at(
            sid(1),
            EventKind::Values {
                values: vec![(
                    *key,
                    DataValue {
                        value: Value::I64(i as i64),
                        quality: good(),
                        timestamp: None,
                        reason: None,
                        times: Default::default(),
                    },
                )],
            },
        );
    }
    let values = started.elapsed();

    // 3. a branch with N children expanded.
    let (_ctx2, mut tree) = app_with_branch();
    let children: Vec<BrowseNode> = (0..N)
        .map(|i| BrowseNode {
            node_id: format!("ns=2;i={i}"),
            name: format!("Child{i}"),
            kind: NodeKind::Item,
            has_children: false,
        })
        .collect();
    let started = Instant::now();
    tree.on_event_for_test_at(
        sid(1),
        EventKind::Browse {
            node_id: "ns=1;s=Branch".to_string(),
            children,
        },
    );
    let browse = started.elapsed();

    eprintln!("A4 n={N}");
    eprintln!("A4 add_items      {:>10.2?}", add_items);
    eprintln!("A4 {N} value events {:>10.2?}", values);
    eprintln!("A4 browse {N} children {:>7.2?}", browse);
}

fn app_with_branch() -> (egui::Context, App) {
    let (ctx, mut app) = app();
    app.on_event_for_test_at(
        sid(1),
        EventKind::Browse {
            node_id: String::new(),
            children: vec![BrowseNode {
                node_id: "ns=1;s=Branch".to_string(),
                name: "Branch".to_string(),
                kind: NodeKind::Branch,
                has_children: true,
            }],
        },
    );
    (ctx, app)
}

fn sid(raw: u64) -> opclook::bff::model::SessionId {
    opclook::bff::model::SessionId::new(raw).unwrap()
}

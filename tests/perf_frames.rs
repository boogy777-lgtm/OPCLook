//! Reproducible CPU frame baseline: see docs/reports/refactor-frame-baseline.md.
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use opcda_egui::app::App;
use opcda_egui::bff::model::{
    DataValue, EndpointSpec, EventKind, Quality, QualityKind, SessionId, Value, VarType,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use std::{alloc::System, time::Instant};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const ROWS: usize = 10_000;
const FRAMES: usize = 120;

fn fixture(journal: bool) -> App {
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.record_dispatches_for_test();
    app.settings.view.options.update_interval_secs = 86_400;
    app.settings.view.options.test_interval_secs = 0;
    app.settings.view.options.auto_reconnect = false;
    let session = SessionId::new(1).unwrap();
    app.seed_session(session, EndpointSpec::ua("opc.tcp://127.0.0.1:4840"));
    for i in 0..if journal { 0 } else { ROWS } {
        app.seed_row(
            session,
            &format!("ns=2;i={i}"),
            &format!("Tag{i:05}"),
            Value::I64(i as i64),
        );
    }
    for row in app.rows_mut_for_test() {
        row.data_type = Some("Int64".into());
        row.canonical_vt = Some(VarType(20));
        row.timestamp = chrono::DateTime::from_timestamp(1_700_000_000, 0);
    }
    if journal {
        for i in 0..5000 {
            app.journal.info(format!("Journal entry {i:05}"));
        }
        assert_eq!(app.journal.entries().len(), 5000);
        app.settings.view.options.show_log = true;
    } else {
        assert_eq!(app.rows().len(), ROWS);
    }
    app.tick_for_test();
    app
}

fn scene(changed: usize, journal: bool) {
    let mut h = Harness::builder()
        .with_size([1600.0, 900.0])
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            opcda_egui::app::install_fonts(ctx);
        })
        .build_ui_state(|ui, app: &mut App| app.update(ui), fixture(journal));
    h.run_steps(20);
    let mut times = Vec::with_capacity(FRAMES);
    let mut allocations = Vec::with_capacity(FRAMES);
    let mut reallocations = Vec::with_capacity(FRAMES);
    let mut bytes = Vec::with_capacity(FRAMES);
    let keys: Vec<_> = h
        .state()
        .rows()
        .iter()
        .take(changed)
        .map(|row| row.key)
        .collect();
    for frame in 0..FRAMES {
        let event = (changed > 0).then(|| EventKind::Values {
            values: keys
                .iter()
                .map(|key| {
                    (
                        *key,
                        DataValue {
                            value: Value::I64(100_000 + frame as i64),
                            quality: Quality {
                                code: 0,
                                kind: QualityKind::Good,
                                text: "Good".into(),
                            },
                            timestamp: chrono::DateTime::from_timestamp(
                                1_700_000_000 + frame as i64,
                                0,
                            ),
                            reason: None,
                        },
                    )
                })
                .collect(),
        });
        let region = Region::new(GLOBAL);
        let started = Instant::now();
        if let Some(event) = event {
            h.state_mut()
                .on_synced_event_for_test_at(SessionId::new(1).unwrap(), event);
        }
        h.step();
        let micros = started.elapsed().as_secs_f64() * 1_000_000.0;
        let stats = region.change();
        times.push(micros);
        allocations.push(stats.allocations);
        reallocations.push(stats.reallocations);
        bytes.push(stats.bytes_allocated);
    }
    if changed > 0 {
        assert!(matches!(h.state().rows()[0].value, Value::I64(100_119)));
        let timestamp = h.state().rows()[0].timestamp_text();
        assert!(
            h.query_all_by_label(&timestamp).next().is_some(),
            "updated timestamp must be visible"
        );
        assert!(
            h.query_all_by_label("100119").next().is_some(),
            "updated value must be visible"
        );
    }
    bytes.sort_unstable();
    times.sort_by(f64::total_cmp);
    allocations.sort_unstable();
    reallocations.sort_unstable();
    println!(
        "journal={journal} changed={changed} frames={FRAMES} median_us={:.1} p95_us={:.1} median_alloc={} p95_alloc={} median_realloc={} p95_realloc={} median_bytes={} p95_bytes={}",
        times[FRAMES / 2],
        times[FRAMES * 95 / 100 - 1],
        allocations[FRAMES / 2],
        allocations[FRAMES * 95 / 100 - 1],
        reallocations[FRAMES / 2],
        reallocations[FRAMES * 95 / 100 - 1],
        bytes[FRAMES / 2],
        bytes[FRAMES * 95 / 100 - 1]
    );
}

#[test]
#[ignore = "machine-dependent CPU and allocation measurements"]
fn actual_app_update_frames() {
    scene(0, false);
    scene(100, false);
    scene(0, true);
}

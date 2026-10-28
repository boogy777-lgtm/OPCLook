# F: CPU frame and allocation baseline

These measurements precede the F/G rendering optimizations. They are headless
CPU wall times, including egui_kittest frame bookkeeping and accessibility, not
physical display or GPU FPS. The `stats_alloc` MIT dev dependency instruments
the process-wide system allocator: counts and bytes include any concurrent
background thread work and its atomic instrumentation adds overhead. They are
allocation requests and requested bytes, not live memory. Compare only on the
same machine, toolchain, flags, viewport and scene.

The 120 measured frames follow 20 warmup frames with the host's CJK fallback
font installed before the first pass. Each scene uses a fresh App and a 1600 ×
900 logical-point viewport. A disconnected-from-network test fixture seeds one
connected UA session, with dispatches intercepted by the test recorder,
reconnect off, update interval 86400 seconds and test interval zero. The table
scenes assert 10,000 actual rows. The stream scene constructs one batch of 100
Values before measurement; within the timed region it invokes the real App
handler and then executes the full App::update frame. It checks the changed
value and timestamp in state and accessibility output. Rows carry an Int64 type
and fixed initial UTC timestamp; each batch advances the timestamp by one second. Transport queueing and payload
construction are excluded. The journal scene has 5,000 actual entries and no
table rows. The plain table and journal scenes contain no synthetic events.
All figures are per frame; p95 is the 114th ordered sample, median is the 61st.

The source harness is `tests/perf_frames.rs`. Run alone, serially, without GPU
rendering:

```sh
PATH=/root/.cargo/bin:$PATH CARGO_TARGET_DIR=target/perf \
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_OPT_LEVEL=3 \
cargo test --offline --config scripts/perf-codegen.toml \
  --target x86_64-unknown-linux-gnu --test perf_frames -- --ignored --nocapture --test-threads=1
```

The checked-in benchmark-only config matches the config used in the measured run.
It overrides dev codegen-units to 1 for `async-opcua-core-namespace`,
`read-fonts`, `vello_cpu`, `opclook-locale`, `opclook-engine`, and dev build
scripts. Test profile uses opt-level 3 and debug info 0; release is a different
profile (project release opt-level z, LTO, one codegen unit), so these are not
release or production FPS figures. The 1.99.1 toolchain and config are part of
the reproduction conditions.

## Measured results

Measured 2026-10-28 on Linux x86_64, Intel Xeon Platinum 8573C,
9 visible CPUs with cgroup quota of 8 CPUs, rustc 1.99.1
(58cf8f9c3 2026-10-13), NotoSansCJK-Regular.ttc. The application source was
at `6abe5f1432edaf88d5d4de4d54038e12e02fd99b` with the final fixture
`tests/perf_frames.rs` SHA-256
`c8b6bcc824821e592fe1f2b3f17aabf625ff00ff18dbf6d5b1a742eba59137d1`;
no F/G optimization was applied. Actual run used the byte-identical external
config and `/workspace/scratch/358410ef0195/target-opclook` as target directory.
The optimized run passed 1/1, including value and timestamp visibility checks.

| Scene | Median µs | p95 µs | Median allocs | p95 allocs | Median reallocs | p95 reallocs | Median bytes | p95 bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 10k static | 2918.9 | 4472.3 | 4472 | 4472 | 378 | 378 | 2,589,180 | 2,589,180 |
| 10k + batch of 100 Values | 3312.2 | 6089.1 | 4530 | 4558 | 393 | 406 | 2,609,300 | 2,616,865 |
| 5k journal, no rows | 75120.0 | 101021.1 | 90968 | 90968 | 5075 | 5075 | 22,653,898 | 22,653,898 |

The reciprocal of the median CPU wall time is approximately 343, 302, and 13
frames/second respectively. These are derived CPU throughput figures for this
instrumented headless test, not measured display or GPU FPS. The journal
scenario demonstrates a separate performance cost and must not be combined
with the table scenes when evaluating changes.

## UI image QA

The ignored `tests/ui_snapshots.rs` test renders actual `App::update` frames with
`egui_kittest`/wgpu using Mesa lavapipe (Vulkan, no display). The harness
installs the host Noto CJK fallback and saves 1200 × 900 PNGs. Run with:

```sh
PATH=/root/.cargo/bin:$PATH CARGO_TARGET_DIR=target/perf \
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 \
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json XDG_RUNTIME_DIR=/tmp \
cargo test --offline --config scripts/perf-codegen.toml \
  --target x86_64-unknown-linux-gnu --features ui-snapshots --test ui_snapshots \
  -- --ignored --nocapture --test-threads=1
```

Reviewed images: `docs/screenshots/main-10k-ru.png`,
`da-helper-error-ru.png`, `ua-security-zh.png`, `network-ru.png`, and
`confirmation-ru.png`. Russian and Chinese labels are legible and no missing
glyph boxes were seen. The main table's far-right columns extend beyond the
1200-point viewport. DA uses a seeded discovery entry and a typed,
localized HelperUnavailable error containing `IPC timeout`; the test asserts
the detail appears in accessibility output. This is a visual fixture, not a
real Windows helper, UAC, network adapter, or DA server runtime test. The
screenshots feature is opt-in and does not add wgpu to the normal CLI graph.

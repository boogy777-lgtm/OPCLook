# Native DA acceptance ledger: N2/N3/N4

Evidence updated 2026-10-31. This is an engineering map, not a
production-readiness declaration. Public exact head
`6c5349c87334860c52a8317f8fdc5e34a9568dd1` passed nine jobs across
[COM 41176472844](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472844),
[SDK 41176472728](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472728),
[App 41176472851](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472851)
and [package 41176472772](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472772).
Both COM jobs (sdk124464956471 and official-mit124464956639) passed the new
generic test-only Async value fixture and strict cleanup, as well as all
existing gates. This does not enable production Async. The following five
earlier gates also passed on exact `81feb138`:
The COM matrix jobs [official MIT inputs 124449766321](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41171347707/job/124449766321)
and [SDK control inputs 124449766459](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41171347707/job/124449766459)
independently passed the following local fixture checks with server/proxy cleanup.
Linux source/mock and extracted C++ stub tests are not Windows COM results.

| Measured Windows fixture gate | Observed scope |
| --- | --- |
| A09 `sdk_partial_read_live` | One SDK `syncRead(2)` returned mixed `S_FALSE` with per-item availability. A separate Gateway batch issued individual SDK reads and preserved Good, denied, Uncertain and Bad facts. |
| `sdk_async_isolation_live` | While A's test-only async SDK task was held and its caller had timed out, B progressed; manager finish was Busy until A retired and final cleanup succeeded. The barrier is before SDK async submit, not delayed device I/O. |
| A08 `sdk_late_callback_live` | A real callback carrying 88 was held while the registration was removed and re-added; after release the retired callback was rejected and a new 99 push arrived. Unadvise/disconnect during that hold is not covered. |
| O23 `sdk_subscription_one_shot_live` | Active SDK subscription remained usable across a correlated one-shot Fresh read and subsequent unsolicited push. SDK fixture request counters showed no extra client Sync poll during the bounded 1.5-second observation, after initial activity settled; SDK server cyclic work is not called client polling. |
| O24/O25 `softing_core_probe --live-da1-inactive` | The SDK reported forced and selected version 257; inactive Cache quality was `0x001c`, Device `0x00c0`. Fixture request counters finished at Cache 2 / Device 1, including baseline Cache 1. This is forced SDK DA1 against the local newer fixture, not a physical DA1-only or remote server. |

## What is enabled

| Route | Actual code and evidence | Scope and remaining condition |
| --- | --- | --- |
| Production/default DA | `crates/backend/src/bff/mod.rs` selects legacy DA on Windows when `native-da-preview` is absent; backend has no default feature enabling preview. | N4 switch/removal of the legacy OPC data plane has **not** happened. Keep legacy until its replacement gates are accepted. |
| Native DA preview | `native-da-preview` selects `softing_da`; `acquisition_capabilities` exposes subscription, FreshSource and readback only on Windows with that feature. `CommandKind::ReadFresh` uses SDK max-age 0; `Read`/`ReadCached` use `u32::MAX` (`softing_da.rs`). | These are adapter abilities, not per-server negotiated guarantees. DA CacheAllowed is permission to use cache, not a promise of the same freshness on every SDK/server/version. |
| Native Async | `validate_acquisition` rejects `DaReadMethod::Async`; router fails closed. SDK I4 async read, `TestAsyncSessionLease` and barrier hooks exist only in the test bridge (`OPCL_DA_TESTING`/`native-da-test-hooks`). | No production Async, no Async Write claim. The test lease may quarantine a session/host indefinitely if a callback never arrives. Queue admission does not establish an absolute OS scheduling deadline. |
| Package profile | `core-probe/CMakeLists.txt` requires pinned official OPCF inputs by default; legacy SDK OPC inputs are an explicit probe-only control. `package-preview.py` requires official inputs, `OPCL_BUILD_PROBES=OFF`, and rejects `opcl_da_test_` exports. | This is a private preview artifact, not a release or production flip. The SDK fixture and hook-enabled bridge are test-only. |

## Truth-table coverage and actual gaps

| IDs | Existing entry point or evidence | Open acceptance work |
| --- | --- | --- |
| A04, A05 | `sdk_device_read_live`: SDK fixture distinguishes cached and device reads with a server-side device counter; readback follows a correlated accepted write. `sdk_device_profile_live` covers the Engine profile path. | Verify server/version negotiation and actual Cache/Device outcomes beyond the selected local SDK fixture. Do not claim a strict cache hit for all CacheAllowed servers. |
| A06, A10 | `validate_acquisition` and router reject Async. Test-only `softing_core_probe --live-async`, `sdk_async_lease_live` and `sdk_async_isolation_live` passed in both 81f COM jobs; their I4 callback, SDK worker and host lifetime evidence is scoped to the fixture. | Production RequestId/epoch correlation, supported shapes, accepted-vs-terminal errors, callback negotiation, cancellation, timeouts and bounded teardown remain open. DA1 needs its own transaction design. |
| A07, O23 | `sdk_gateway_live` exercises subscriptions; `sdk_subscription_one_shot_live` passed the active subscription + one-shot Fresh + no hidden Sync poll gate in both 81f COM jobs. | The no-poll observation is bounded and local, not a guarantee for all negotiated servers and errors. |
| A08 | `sdk_late_callback_live` passed held real callback 88 / remove-readd / new 99 and cleanup in both 81f COM jobs; sync blocked-write isolation and shutdown are separate gates. | Held callback across Unadvise/disconnect, other epochs, timeout/failure and remote teardown remain open. |
| A09 | `sdk_partial_read_live` passed actual SDK group mixed `S_FALSE` and separate per-key Gateway outcomes in both 81f COM jobs. `sdk_read_failure_facts_live` separately covers codec failure after an attested SDK sample. | Other versions and remote servers remain untested; unavailable quality/time must stay absent and operation failure must not become Good. |
| O24, O25 | `--live-da1-inactive` passed forced and selected SDK DA1 against the local fixture in both 81f COM jobs, with separately counted SDK request kinds. | A physically DA1-only server and remote interoperability remain open. Do not generalize this inactive-quality observation to other methods or versions. |
| O26 | `onReadAdvise` classifies data change vs read completion; test-only I4 matches transaction/handle, ignores stale tokens and preserves the empty failed-master envelope. | This is not integrated production Async/stream coexistence. DA1, error and queue-overflow paths, generation changes and remote servers require separate evidence. |

## Value shapes, ownership and distribution

`copy_sdk_value`/`build_variant_wire` and the Rust mapper already support
bounded fixed-width primitive arrays, plus existing BSTR and VARIANT array
paths. The recent change validates canonical `VARIANT_BOOL` bytes before a
write reaches the SDK; it did not introduce all primitive widths. One-dimensional
arrays are limited to 256 elements and payloads to 64 KiB; nested conversion
has a depth limit, and BYREF/multidimensional input is rejected. Existing
coverage is not a claim that every VARIANT/SAFEARRAY shape or server conversion
is supported. `sdk_primitive_arrays_live` prepares direct R8/BOOL/I8/UI8 exact
read/write/readback and read-only/negative cases and passed in both 81f COM
jobs. `test-primitive-array-source.py`
checks actual validation code with SDK stubs, not COM.

SDK owns server/group/item objects, worker, callbacks, COM and wire transport;
the adapter owns only the bounded copied facts and session/registration scope.
The test-only async gate blocks before the SDK `asyncRead` call, so it does
not prove a physically delayed server I/O or production Async cleanup. The
manager's final response follows `Host::close`, but the manager retains no
`JoinHandle` and this is not a separate thread-join observation. See
[async lease](async-session-lease.md) and [async isolation](async-isolation.md).

[License closure](license-closure.md) and
[Cargo notice inventory](cargo-notice-inventory.md) record the pinned Softing
SDK, official OPCF input, Expat and Cargo source/notice evidence. The measured
Cargo graph is a resolved normal/build dependency **superset**, not the exact
PE link set; the link map is separate evidence. A no-hook artifact and an empty
missing-text list do not establish legal release clearance. Windows system,
MSVC/CRT/toolchain redistribution and applicable rights/release review remain
manual gates; do not accept new terms or infer a grant for historical SDK OPC
copies from the official replacement inputs.

## Reproducing the scoped gates

From a configured Windows x86 environment with the pinned SDK, official MIT
inputs and test fixture/bridge builds described in `async-session-lease.md`:

```powershell
# Precompile outside the fixture, then launch each target in a fresh fixture.
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_isolation_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_isolation_live -- --nocapture
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_partial_read_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_partial_read_live -- --nocapture
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_late_callback_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_late_callback_live -- --nocapture
cargo test --locked -p opclook-engine --target i686-pc-windows-msvc --features native-da-preview --test sdk_subscription_one_shot_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-engine --target i686-pc-windows-msvc --features native-da-preview --test sdk_subscription_one_shot_live -- --ignored --nocapture
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- ../softing-build/Release/softing_core_probe.exe --live-da1-inactive
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-preview --test sdk_primitive_arrays_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-preview --test sdk_primitive_arrays_live -- --nocapture
```

Set `SOFTING_RUNTIME_LIB_DIR` to the hook-enabled bridge Release directory.
Other strict entries and matrix settings are in
`.github/workflows/softing-da-interop.yml`; source/stub checks are
`test-async-empty-callback.py` and `test-primitive-array-source.py`. The SDK
source, test fixture, CLI package, and Windows system/toolchain dependencies
have distinct receipts and scopes.

## Production Async: source facts and remaining engineering route

The current production selector deliberately rejects `DaReadMethod::Async`; no
capability is enabled by these tests. The SDK owner remains the existing
server/group worker, callback and COM transport. Async fields within
`PushState`, `TestAsyncReadTask` and `TestAsyncSessionLease` are gated by
`OPCL_DA_TESTING`/`native-da-test-hooks`; they cover one pending bounded single-item read,
non-reused transaction tokens and retained host/session ownership. This is a
prototype, not a production asynchronous Gateway route. The separately armed
test-only Gateway correlation case has bounded live acceptance below. Source and test details
are in [DA Async audit](da-async-audit.md) and [session lease](async-session-lease.md).

The next implementation should remain bounded and preserve SDK ownership:

1. Define a production outcome/ownership contract for `RequestId`, Connect
   epoch, registration and SDK transaction, with a bounded pending budget,
   no token reuse within a group and an explicit terminal error on admission,
   queue overflow or unsupported callback. Do not infer successful item I/O
   from an accepted SDK submit; service, item, quality and timestamp are facts
   only when provided.
2. Reuse the existing bounded Sync scalar/array value codec for copied callback
   results, including its byte, element and depth limits. Cover scalar, text,
   arrays, per-item failures and unavailable facts before advertising those
   shapes. Async Write is a separate outcome/readback scope.
3. Negotiate a working SDK advise connection before accepting requests; classify
   unsolicited subscription data separately from read completion, and prove
   early completion, stale registration, timeout and removal under the actual
   SDK. Preserve current fail-closed behavior when callback support is absent.
4. Keep accepted task, item/group, session and manager host alive until the SDK
   has retired both task and completion. Test queued, executing, timed-out,
   disconnected and never-callback cases. The current test lease intentionally
   quarantines an unretired owner indefinitely; it does **not** prove bounded
   resource reclamation. Define an honest resource limit and visible outcome
   rather than freeing live SDK state or adding another worker/retry loop.
5. Cancellation requires a separate library-owner review and real valid,
   non-null interface coverage for the SDK's `Cancel2` path. The existing
   pinned-source candidate and synthetic test are not a production SDK patch
   or real Win32 cancellation proof; do not repeat the old null-pointer crash
   diagnostic. Cancellation must not be presented as guaranteed completion.
6. DA2 device semantics and DA3 max-age need explicit negotiated-path tests.
   DA1's server-assigned cancel ID and early-completion ordering need a
   separate correlation design; do not silently route it through a DA2/3
   transaction assumption. The local forced DA1 **Sync** result above is not
   Async coverage.

These gates do not close remote interoperability, release/redistribution rights
or N4. No current N4 production flip, default DA change, full-history claim or
new protocol owner follows from the 81f fixture results.


### Accepted generic test-only Async probe on6c

[Generic single-value probe](async-generic-value-probe.md) extends the shared
test-only I4 slot with the existing Sync codec/ReadOutcome decoder. Root mock
stand53/53 and actual-source stub checks passed. Its actual Win32 shape/ABI
fixture passed 1/1 in both COM variants on6c, with strict cleanup. Production
Async remains Unsupported. Unknown
SDK per-item output does not permit early retirement; entering SDK and throwing
without a return is unknown admission, not proof that nothing was accepted.
Caller gets one adapter error, keeps the lease and consumes later callback only
as a cleanup acknowledgement. This fault was modeled against actual task code,
not reproduced as a Windows SDK crash. Generic batching, cancellation negotiation,
version/capability/cleanup policy and production facade are still separate gates.


### Accepted scoped Gateway correlation fixture onf787

[Gateway probe](async-gateway-correlation.md) routes one explicitly armed
ReadFresh through the existing SDK slot, scoped by owner, Connect epoch,
RequestId and current registration. One timeout terminal is followed only by
cleanup retirement. Pending Disconnect/channel close exits the logical worker
while the SDK lease is quarantined; no bounded reclamation claim is made.
A later Connect is tested after full retirement, not with a late old callback
crossing that Connect. The allocated token alone is not SDK/server acceptance.
Root source/mock54/54, getter source/stub and Linux Rust-only typecheck passed;
agent no-default-features49/49 and three C++ guards passed.

Exact `f787f43842756c58ab57beeb689e09ede9e7c920` passed all9 jobs:
[SDK4](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41178553269),
[COM2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41178553338),
[App2](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41178553274),
[package1](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41178553268).
The new live fixture passed1/1 on SDK inputs (job124471049602,0.32s) and
1/1 on official-MIT inputs (job124471049388,0.33s); both strict server/proxy
cleanup checks passed. Other native gates also passed. These outcomes cover
the stated local test-only scope, not remote DA, old callbacks crossing Connect,
all async failures/versions, cancellation or production Async adoption.
Production Async remains Unsupported.


### Prepared forced DA2/DA3 synchronous routes (pair29)

[Version probes](version-routes.md) generalize the existing test-only early-force
open; production default and the DA1 wrapper remain unchanged. Pinned SDK source
fragments compiled with COM stubs verify DA2 interface filtering and Cache versus
Device selection, plus DA3 ReadMaxAge forwarding. The fixture driver checks
per-read target counter deltas: DA2 UINT32_MAX→Cache,0/1234→Device; DA3 all three
ages→syncMaxAge. That measures request kinds, not numeric age arriving at the
server, cache freshness or a physical DA2/DA3-only server.

Root runtime/mock54/54 and actual-source/stub guards passed, including a
wrong-kind counterfactual. Two independent live entries are wired into each COM
input. Actual Win32 compilation, execution and cleanup are pending the new head.
This is Sync route evidence, not Async version/cancellation admission or N4.

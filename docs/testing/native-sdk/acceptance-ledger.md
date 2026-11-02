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
6. Forced DA2/DA3 Sync request routes are measured below. Async negotiation,
   numeric max-age at the server and cache-freshness/other-server behavior remain
   separate gates.
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


### Accepted forced DA2/DA3 synchronous routes (pair29)

[Version probes](version-routes.md) generalize the existing test-only early-force
open; production default and the DA1 wrapper remain unchanged. Pinned SDK source
fragments compiled with COM stubs verify DA2 interface filtering and Cache versus
Device selection, plus DA3 ReadMaxAge forwarding. The fixture driver checks
per-read target counter deltas: DA2 UINT32_MAX→Cache,0/1234→Device; DA3 all three
ages→syncMaxAge. That measures request kinds, not numeric age arriving at the
server, cache freshness or a physical DA2/DA3-only server.

Root runtime/mock54/54 and actual-source/stub guards passed, including a
wrong-kind counterfactual. Two independent live entries ran in each COM
input on exact22a. Both Windows x86 jobs passed compilation, all route assertions
and strict fixture cleanup: [SDK inputs124547050998](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41204614566/job/124547050998)
and [official-MIT124547051147](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41204614566/job/124547051147).
Forced/selected versions512/768 and inactive group were observed; for ages
MAX/0/1234 the total/cache/device deltas were DA2:1/1/0,1/0/1,1/0/1 and
DA3:1/0/0 each. All9 jobs for22a passed.
This is Sync route evidence, not Async version/cancellation admission or N4.


### Measured Async error/quality facts (pair30)

[Async facts probe](async-facts.md) reuses the SDK task/slot/lease and decoder.
Ordinary fixture Good/Uncertain/Bad samples preserve value, raw quality and time;
ReadDenied must retain available service/item failure without inventing sample
quality/FILETIME/VT. A read-only same-mutex snapshot separates call attempt,
SDK return, known submit-item status and matched callback. It is test-only and
not a production outcome contract. Root runtime/mock54/54 and source guards,
including the task/destructor guard, passed. On exact656, i686 ABI/execution
and strict cleanup passed1/1 in both COM inputs (SDK0.07s, official-MIT0.08s).
All cases observed flags0xf, submitS_OK/known itemS_OK; denial was a matched
callback failure, not observed initial rejection. Full656 was8/1 due to the
separate Windows App deadline test; repaired-head acceptance is pending. No Async method,
cancellation, network failure, DA1 or remote acceptance follows from it.

## Pair31 — per-item Async admission (Windows pending)

[SDK access-rights proof](async-admission.md) uses an ordinary write-only tag.
Pinned SDK source excludes it from the transaction with S_FALSE/OPC_E_BADRIGHTS;
a dedicated test-only live gate checks actual submit facts, no invented sample
and cleanup in both inputs. Local runtime54/54 and selector/source guard passed.
This is distinct from pair30 accepted-then-callback failure; no new Win claim yet.

Receipt 2026-11-01 Asia/Yekaterinburg: exact repaired code
`266c683d303f396b67a9e69e054bb27f62c8248d` passed9/9 jobs
(SDK41231426756, COM41231426901, App41231426880, package41231426739).
The new pair31 admission/cardinality packet needs its own exact-head CI.

## Pair31 — actual local admission accepted (2026-11-01 Asia/Yekaterinburg)

Exact5fcaab98 passed1/1 in both COM inputs,0.03s each, strict cleanup PASS.
Both measured flags0x7/submit1/item0xc0040006, no invented sample fields and no
matching callback observed at retirement. [Proof](async-admission.md).
This closes the dedicated local per-item admission case, not production Async
or a general promise that no later callback can ever arrive. Full-head CI
acceptance is recorded separately in status/packet.

Final receipt2026-11-01 Asia/Yekaterinburg: exact code
`5fcaab98df0183ad61f7034e9f9198f8131db954` passed9/9:
SDK41232803418, COM41232803478, App41232803411, package41232803483.
Actual-source excess regression and ordinary O18/O19 passed Windows. Both
local SDK admission tests and strict cleanup passed; this does not add a
malformed wire/metadata-cap/recovery or production Async acceptance.

## Pair32 — forced DA2/DA3 Async source scope (pre-CI)

[Dedicated proof](async-version-routes.md) uses started/advise group and SDK
request-creation counters on one target tag. DA2 expects asyncDevice delta1/0,
DA3 asyncMaxAge0/1, with matching callback/submit facts, sample42 and strict
cleanup. Both CI inputs wired. Source/stub review passes are not actual COM
or physical older-server/numeric freshness/production Async acceptance.

Pair32 source receipt:551e8769 failed the old counter extractor's incomplete
SDK stubs. The repaired actual-creator stand passed increments, exclusions,
saturation and old Sync checks. New-head CI pending; no native source change
or retrospective acceptance of551. [Details](async-version-routes.md).

## Pair32 — actual forced Async routes accepted locally

Exact84396da7 passed1/1 in both COM inputs, SDK0.08s/MIT0.07s, strict cleanup
PASS. Forced=selected512/768 and started groups were observed; dedicated deltas
DA2(1,0),DA3(0,1), flags0xf, submit/itemS_OK and sample42 passed. Source uses
max_age0; numeric wire age/physical freshness, other age policies and production
Async remain unaccepted. [Exact proof](async-version-routes.md).

Pair32 full acceptance: exact84396da7 passed9/9, SDK41237089913/
COM41237090071/App41237090037/package41237090066. Windows317/0/1ignored/
1isolatedfiltered passed. The final Markdown receipt preserves this code.
Production Async/N4/remote/physical/license gates remain open.

## Pair33 — held subscription callback source scope (pre-CI)

[Gate scope](late-reconnect.md): new Connect/registration must finish while old88
is held, then release and new77→99/FreshRead99 with new epoch and cleanup.
Test-only held helper before/fail and after/pass on extracted C++ stubs; root
real-file runtime stand54/54 passed. Actual i686/COM gate pending both inputs.
Post-retirement reconnect is not a substitute for overlap.

## Pair33 — actual local reconnect gate accepted

Exactd401c27d passed9/9. New reconnect test1/1 passed in both COM inputs,
SDK0.33s/MIT0.34s, strict cleanup PASS. Assertions require same-ID new Connect
and registration before old88 release, new77→99/FreshRead99 with epoch8 and
joined Gateway/SDK finish. [Proof](late-reconnect.md). This is separate from
old same-epoch remove/readd and Async post-retirement reconnect, and remains
local fixture evidence, not production Async or remote/vendor acceptance.

## Pair37: SDK inventory index flags

Code `7512ede914548b3a1c57cb892ccf2ee4014bfc2d` rejects selected SDK tracked
inputs hidden by assume-unchanged or skip-worktree. Both real temporary Git
fixture counterfactuals fail against baseline and pass after fix; guard 5/5,
package 8/8, notices 14/14 and pinned SDK inventory 271 entries pass locally.
This strengthens source identity, not redistribution/legal clearance. No pin,
production DA default or Async change; exact-head CI 9/9.

## Pair38: Cargo registry manifest and license metadata integrity

Code `2c52d547e29194119c81e125878ff5e76eb9845e`: registry package metadata
license checked against installed Cargo.toml, which is checked against the
checksum-verified locked crate archive. Both tamper fixtures fail baseline
and pass fix; notices 17/17, SDK inventory 5/5, package 8/8. Linux-calculated
i686 metadata/archive graph 183 packages / 94 notices / 0 missing; Windows private preview package subsequently passed; no legal clearance claim. Only collector profile hash updated;
SDK pins and production defaults unchanged. Exact-head CI 9/9.

## Pair39: final patched SDK index visibility

Code `476fcec0338a8fd67b20e2c1b3fa74e449dd66b9`: final preview packaging
and clean SDK verifier share `verify_visible_index` for selected paths. Git
fixture baseline fails four flag/clean-or-modified cases, corrected package
8/8, verifier 5/5, notices 17/17; pinned SDK with two standard patches passes.
The shared helper is included in package evidence; only package-preview profile
hash changed, SDK pins unchanged. No legal clearance or production flip.
CI exact head 9/9; tests passed Linux and Windows i686.

## Pair40: scoped fixture evidence

Code `5ade14eec04618d4a34b5ed814f7269da5a9bf23`: test UA server hands
OS-bound listener to SDK run_with; controlled old free_port race reproduces,
real fallback/OPC Connect and real-file tests passed. Other free_port uses and
readiness heuristic remain. DA Router test with mock host shows healthy B
teardown while blocked A is detached, then A teardown on release; no actual
COM or OS deadline claim. CI 9/9; no production switch or pin change.

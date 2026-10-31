# Native DA acceptance ledger: N2/N3/N4

Source snapshot: `ffe236c`, 2026-10-31. This is an engineering map of the
current implementation and available tests, not a declaration of production
readiness. The last fully accepted public baseline recorded in `docs/status.md`
is `94773fac64bb925a32cae0d21ca914279f111002`: nine jobs, including both COM
input variants and the no-hook packaged CLI fixture. The newer lease, direct
primitive-array and async-isolation tests below are present in this snapshot;
primitive arrays and manager lease passed in both94773fa COM inputs. Async
isolation, SDK base-destruction correction, late callback and subscription
one-shot gates require their own later exact-head CI.
A Linux source/mock or extracted C++ stub test is not a Windows COM result.

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
| A06, A10 | A10 currently fails closed in `validate_acquisition` and router tests. `softing_core_probe --live-async` exercises test-only SDK I4 callback; checked lease and manager tests add timeout/retirement coverage when their exact-head Windows CI passes. | A06 product RequestId/epoch correlation, all supported shapes, submission/retirement/error paths, callback-unavailable negotiation, and exactly-once bounded teardown remain open. DA1 transaction behavior needs a separate design. No sync facade may be called Async. |
| A07, O23 | `sdk_gateway_live` exercises real subscription activation, initial SDK DataChange, write-driven update and remove/re-add. `RuntimeGroup::onReadAdvise` separates unsolicited mode 1 from read completion. | A single test that combines active Subscription with a correlated one-shot Sync read and verifies no hidden persistent Poll remains a live gap. Other server negotiation and errors remain open. |
| A08 | SDK callback state, bounded push queue, item registration and retired SDK item retention exist in `runtime.cpp`/`softing_da.rs`; Gateway fixture covers ordinary remove/re-add. `sdk_pending_shutdown_live`, `sdk_host_reuse_live` and `sdk_session_isolation_live` cover blocked **sync device write**, host Busy/cleanup and independent sessions. | A deliberately delayed callback released after remove/re-add or Unadvise/disconnect must not mutate a later epoch; ordinary re-add and mocked stale checks do not establish that race. Full failure/timeout and remote teardown remain open. `sdk_async_isolation_live` is a separate prepared test; do not call it passed without exact-head CI. |
| A09 | `sdk_read_failure_facts_live` checks an actual successful SDK read followed by an adapter codec failure and retains attested service/item HRESULT, quality and FILETIME while displaying Bad. SDK source/offline callback probes check mixed item results and non-S_OK values. | That fixture is not a live mixed-item `S_FALSE` server response. Real per-item partial failures, bad/uncertain quality and missing facts need server-backed negative cases across negotiated versions. |
| O24, O25 | SDK Sync path accepts max-age/source and preserves supplied quality/error facts. | Actual DA 1.0 inactive group/item Cache and Device cases have not been measured; do not generalize DA 1.0 inactive-quality behavior to all methods and versions. |
| O26 | `onReadAdvise` classifies data change vs read completion; private I4 probe matches transaction and item handle, ignores late/stale tokens, and preserves a matching empty failed-master envelope. | Test-only I4 correlation and synthetic callback checks are not an integrated production Async/stream coexistence proof. DA1, late callbacks, queue overflow and generation changes require explicit real-SDK tests. |

## Value shapes, ownership and distribution

`copy_sdk_value`/`build_variant_wire` and the Rust mapper already support
bounded fixed-width primitive arrays, plus existing BSTR and VARIANT array
paths. The recent change validates canonical `VARIANT_BOOL` bytes before a
write reaches the SDK; it did not introduce all primitive widths. One-dimensional
arrays are limited to 256 elements and payloads to 64 KiB; nested conversion
has a depth limit, and BYREF/multidimensional input is rejected. Existing
coverage is not a claim that every VARIANT/SAFEARRAY shape or server conversion
is supported. `sdk_primitive_arrays_live` prepares direct R8/BOOL/I8/UI8 exact
read/write/readback and read-only/negative cases; its new Windows result is
pending unless confirmed on the tested head. `test-primitive-array-source.py`
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
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-preview --test sdk_primitive_arrays_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-preview --test sdk_primitive_arrays_live -- --nocapture
```

Set `SOFTING_RUNTIME_LIB_DIR` to the hook-enabled bridge Release directory.
Other strict entries and matrix settings are in
`.github/workflows/softing-da-interop.yml`; source/stub checks are
`test-async-empty-callback.py` and `test-primitive-array-source.py`. The SDK
source, test fixture, CLI package, and Windows system/toolchain dependencies
have distinct receipts and scopes.

## Next bounded engineering slice

Prioritize A08: build a deterministic test around the existing SDK fixture and
callback extension point that proves a real callback is held, performs
remove/re-add or disconnect/reconnect, then releases the old callback and
checks the current epoch/registration and cleanup. First verify the SDK can
safely hold and release that callback without adding a COM owner, second
scheduler or a fabricated protocol response. If the existing extension point
cannot produce the ordering, document exact source and a minimal SDK-owned
seam. Follow with the O23 active-subscription one-shot read/no-hidden-poll
case. Neither step enables native Async or substitutes for the separate
remote/other-server and release-rights gates.


### Prepared follow-ups after the original source snapshot

[Late callback](late-callback-proof.md) prepares a genuine SDK callback held
through Remove/re-add, not a synthetic onReadAdvise invocation; Unadvise during
the server advise lock is not claimed. [One-shot subscription](subscription-one-shot.md)
uses actual Engine scheduling and public SDK request-factory counters, including
cache-served client Sync reads while excluding server cyclic work. Their Windows
results are pending. The SDK base-destructor ordering defect is separately fixed
and source-tested in [lease](async-session-lease.md). A09 real item/quality failure fixtures are prepared in
[partial read facts](partial-read-facts.md): one SDK syncRead(2), separate Gateway
per-key calls, and strict availability/quality assertions. Their Windows result
is pending; none is replaced by the earlier codec-failure test.

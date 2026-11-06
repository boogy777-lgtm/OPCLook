# N4 readiness and switch plan — CLOSED, production switch (2026-11-05)

Update: N1.LICENSE was closed by owner decision, not by legal analysis; grant
and release notices remain unresolved. The Windows selector is native-only and
the legacy OPC COM data-plane is removed. The vendored no-hook x86 bridge uses
official OPCF inputs and the pinned SDK with reviewed patches; a normal
`cargo build --release` works without `SOFTING_RUNTIME_LIB_DIR`. Local gates
passed: gateway 1/1, reconnect 1/1, remove/re-add 1/1, isolation 1/1, pending
shutdown 1/1, strict cleanup PASS. Remote OPCEnum/DCOM/auth is wired but not
accepted; independent vendor/physical DA and production Async remain external
residuals. The inventory below is historical.

Discovery regression: a stale 32-bit registry entry
`opcdaclientopcda.OPC.1` with a missing CLSID made the SDK browser return
`REGDB_E_CLASSNOTREG` and hide every healthy server. The bridge now skips
unresolvable ProgID entries, sets flag bit 1, and the Rust adapter logs the
skip. Before the fix `sdk_gateway_live` failed 0/1 with
`DaServerListHostUnavailable { detail: "Native(-2147221164)" }`; after the fix
it passed 1/1 in 2.16 s with strict cleanup.

Source snapshot: `main@b4bc724` plus the uncommitted N3/N4/N5/N6 changes.
Local fixture results do not establish remote/vendor or legal acceptance.

## Selection and call sites

`crates/backend/src/bff/mod.rs`: Windows without the feature aliases `da` to
legacy `da.rs`; `native-da-preview` aliases it to `softing_da.rs`; non-Windows
default uses `da_unsupported.rs`. `Gateway::{new,send,shutdown}` dispatches the
common Command/Event contract; Engine, UI and CLI do not call `DaSession`.
Feature selection is build-time, not a fallback between transports. Backend
Cargo.toml has no native default, and `build.rs` requires
`SOFTING_RUNTIME_LIB_DIR` on Windows with native preview. Enabling it by default
breaks normal SDK-less builds. `src/app/state.rs` and `src/ui/dialogs/write.rs`
use `da_variant::vt_name` via `src/lib.rs`; preserve or migrate that API.

## Operation / legacy symbols / native coverage

Paths below are under `crates/backend/src/` unless otherwise stated. Named tests
are evidence pointers, not tests rerun by this documentation-only change; their
actual scope and historical receipts are in [acceptance ledger](acceptance-ledger.md).

| Operation, current owner and symbols | Native preview, test and gap |
| --- | --- |
| COM thread/security: `bff/da.rs` `ComGuard::{new,drop}`, `WinDaHost::{run_session,run_discovery}`, `spawn`, `init_dcom_security{,_once}` | `softing_da::{manager,acquire_host,finish_native_sdk,spawn}` and SDK own native lifecycle. `sdk_session_isolation_live`, `sdk_pending_shutdown_live`. Legacy uses process-wide NONE/IDENTIFY; remote/security interactions and equivalence remain open. |
| Connect/activation/group: `DaSession::connect`, `create_instance`, `create_group`, `DaSession`, `ItemSlot`, `ServerGroup::{remove,drop}`, `ServerStatus::{from_raw,get,drop}` | `NativeHost::run_session` Connect, SDK `open_config` / `opcl_da_session_open*`; `sdk_gateway_live`, `sdk_late_reconnect_live`. Remote and independent vendor server not accepted. |
| Dispatch/shutdown: `owned_session_mut`, `handle_command`, `send`, `send_error`, `DaSession::shutdown` | Shared `da_router.rs` `Route`, `route_of`, `no_connection_reply`, `DaHost`, `SessionHandle`, `Router`, `RouterShutdown`, `run_router`, `reap_finished`, `join_until`; native `run_session`/`drain_push`. Router mock `disconnect_removes_the_group_and_allows_reconnect_of_the_same_id`; separate live reconnect/cleanup test. Keep shared router. |
| Add/Remove: `DaSession::{add_items,remove_items}`, `clear_item_result`, `ItemSlot` | Native `Session::{add,remove}` / SDK items; `sdk_gateway_live`, `sdk_late_callback_live`. Compare remove/error semantics before deletion. |
| Sync Read/quality/time: `DaSession::read`, `clear_item_state`, `read_hresult` | Native `Session::read_scalar`, Read/ReadCached max-age `u32::MAX`, ReadFresh 0; `sdk_device_read_live`, `sdk_partial_read_live`, `sdk_read_failure_facts_live`. CacheAllowed does not guarantee a cache hit; physical freshness and other servers open. Legacy explicit Fresh/Cached are rejected. |
| Sync Write: `DaSession::{write,error_text}`, `OwnedVariant` | Native `write_outcome`, `softing_scalar::encode*`, `Session::write_{scalar,array,text}`; `sdk_gateway_live`, `sdk_primitive_arrays_live`, `sdk_write_exception_live` (hook). Full type/vendor fidelity open; unknown remains unknown. |
| Browse: `DaSession::browse`, `browse_enum`, `wide` | `Session::browse`, SDK `opcl_da_browse`; local `sdk_gateway_live`. Compare flat/hierarchical, limits, errors and node identity against independent servers. |
| Test/status: `DaSession::test`, `ServerStatus`, `error_text`, `da_status::{server_state_name,check_server_state,ServerStatusInfo,format_connect_info}` | Native `Session::test_status`, shared `check_server_state`; `test_command_reports_sdk_state_and_native_error_through_router` is mock. Legacy Connect vendor/version/group information differs from native revised-rate message; parity decision/test open. |
| Subscription/Advise, Async: legacy active group but no callback/Advise or production async transaction engine; router and `validate_acquisition` reject Async | SDK subscription + `drain_push`, epoch/registration mapping; `sdk_gateway_live`, `sdk_subscription_one_shot_live`, `sdk_late_callback_live`, `sdk_late_reconnect_live`. `sdk_async_*` probes are test-only; production Async Unsupported, Async Write not established. No polling emulation. |
| OPCEnum/discovery/remote: `enumerate_servers`, `class_details`, `clsid_from_remote_opcenum`, `clsid_from_local_registry`, `resolve_server_clsid`, `WinClsidLookup`, `discovered_clsids`, `remember_discovered` | Native `NativeHost::run_discovery`, `NativeClsidLookup::{remote_enum,discovered,local_registry}` call SDK discover/resolve; local DA1 registry fallback logged. `sdk_gateway_live` covers local discovery; `da_clsid` tests are mocks. Remote OPCEnum/resolve/DCOM/auth require real remote tests. Discovery is not wholly absent. |
| VARIANT: `da_variant.rs` `filetime_to_utc`, `ole_date_to_utc`, `utc_to_ole_date`, `variant_to_value{,_inner}`, `scalar_to_value`, `bounded_array_len`, `ArrayAccess`, `expected_array_element_size`, `array_to_value`, `set_vt`, `variant_inner`, `variant_from_value`, `write_scalar`, `SafeArrayBuilder`, `build_array_variant`, `default_vt`, `clear_variant`; fault helpers are test-only | Native `softing_scalar.rs`, `softing_facts.rs`, `softing_runtime.rs`, `native/softing-da/{adapter,runtime}.cpp`; `sdk_primitive_arrays_live`, `sdk_gateway_live`. Ledger describes bounded/unsupported shapes. Legacy `roundtrip_*` tests do not prove native fidelity. |
| COM allocations: `com.rs` `CoString`, `CoAllocation`, `ComArray`, `OwnedVariant` constructors/accessors/Drop | Imported by legacy `da.rs`; `com::tests::array_clears_each_element_on_error_and_unwind` is a unit test. Recheck all references before deletion; system COM support is not automatically OPC. |
| ABI declarations: `opc/bindings/{da,comn}.rs`, `opc/mod.rs` | Generated bindings are not an SDK; legacy `da.rs` is the direct caller. Recheck public exports/tests and references before narrowing/removing. |

`da_clsid.rs` (`is_local_host`, `ClsidLookup`, `resolve_clsid`,
`DiscoveredClsids::{remember,get}`), `da_router.rs`, `da_status.rs`,
`da_convert.rs` (`da_range`, `convert_int`, `convert_uint`, `convert_float`),
`vt.rs`, `filetime.rs`, `ole_time.rs` and `da_unsupported.rs` are shared or
platform/policy code, not automatically removable. Native uses `da_convert`,
`da_clsid`, `da_router`, and `da_status`; UI uses the VT name API.

## Preserve non-OPC system code

`nethelper/`, `crates/backend/src/net/windows.rs`, `bff/net.rs`, `bff/mdns.rs`,
`winutil.rs`, `src/da_route.rs` and callers provide OS network, routing,
elevation/UAC or discovery support. Do not remove them as OPC data-plane.
Cargo Windows features also serve other code: prune only after independent
call-site/build review. Process-wide DCOM initialization requires its own review.

## Blockers and ownership

1. **N1.LICENSE OPEN:** project owner/legal must resolve the applicable OPC
   Foundation IDL/generated-source grant and actual release notices, link and
   distribution dependencies; [license closure](license-closure.md). Official
   MIT-labelled inputs do not establish rights for historical SDK copies.
2. **SDK packaging OPEN:** release/build owner must provide reproducible pinned
   x86 SDK/no-hook bridge and a tested SDK-less build/install story. Do not
   silently fall back to legacy or change default while either gate is missing.
3. **Production Async Unsupported:** SDK/adapter owner must prove production
   transaction and request/epoch/registration mapping, admission/completion,
   cancellation, failures and bounded timeout/never-return teardown, DA1 and
   version routes. Hooks/fixtures are not production. No second transport.
4. **Remote/interop and lifetime OPEN:** test owner needs remote OPCEnum,
   DCOM/auth, independent vendor and physical DA1 servers, failures/timeouts,
   types/status matrix. Remote is wired but not accepted; it is not necessarily
   disabled today. Gate the switch, or explicitly disable unsupported scope;
   do not claim remote works from a local fixture. Explicit alternative
   credentials/security configuration has not been established.
5. **Parity OPEN:** adapter/product owner compares Browse identities, values,
   status, Connect metadata, UI/CLI, errors and profiles using contract tables.
   Unsupported shapes must be explicit. If a required capability is missing,
   fix the SDK or change candidate; owner may decide which optional features
   remain visibly disabled, never emulate a subscription using Read.

## Switch order and checks

After gates and N3 acceptance, compare old/new contract baselines in separate
processes, design packaging and switch actual GUI/CLI entry points, then remove
only proven legacy callers and COM implementation. Keep shared/system code.
Exact-head Windows x86 CI, official-input fixture and independent remote/vendor
acceptance are required. Roll back by reverting the release, not by silently
running two transports. Verify commands and retain exit codes/logs:

```powershell
cargo fmt --all -- --check
cargo test -j 2
cargo test -j 2 -p opclook-backend
cargo test --test ua_smoke -- --ignored --test-threads=1
rg 'bff::da|da_router|da_variant|da_convert|DaSession|OPC' crates src tests
$env:SOFTING_RUNTIME_LIB_DIR='D:\opclook-native\softing-build\Release'
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_late_reconnect_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server D:/opclook-native/fixture-build/Release/opcl_da_server.exe --proxy D:/opclook-native/fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_late_reconnect_live -- --nocapture
```

Check local fixture paths; COM registration may require elevation and strict
unregister/cleanup must pass. Use `--isolation` for isolation and pending
shutdown as in `.github/workflows/softing-da-interop.yml`. Run other live gates
in fresh fixtures and follow that workflow and [license closure](license-closure.md)
for official inputs, no-hook package and traces. Local pass is not remote/legal.

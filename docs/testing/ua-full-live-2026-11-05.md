# Full permitted UA live run and cleanup — 2026-11-05

Dirty working tree based on `b4bc724`, Windows i686 client / x64 CODESYS console
runtime. No commit/push or exact-head CI claim. All live binaries serial with
`--test-threads=1`; endpoint supplied only as `opc.tcp://127.0.0.1:4840`.
Demo expiry was not observed during the run; future availability not inferred.

## Measured CODESYS results

| Binary | Passed | Failed | Test time |
| --- | ---: | ---: | ---: |
| ua_smoke | 2 | 0 | 0.35 s |
| ua_search | 1 | 0 | 0.06 s |
| ua_security | 3 | 1 | 2.27 s |
| security_live | 2 | 0 | 0.14 s |
| ua_types | 5 | 0 | 2.86 s |
| ua_write | 1 | 0 | 0.34 s |
| ua_subscription | 5 | 0 | 7.68 s |
| ua_large | 1 | 0 | 1.67 s |
| ua_lifecycle | 1 | 0 | 0.69 s |
| ua_refresh_speed | 1 | 0 | 0.17 s |
| **Total attempted** | **22** | **1** | |

One additional hostname-localhost search case was explicitly excluded to obey
the owner endpoint restriction, not rewritten/weakened/counted as PASS. Three
offline guards filtered by `--ignored` are not live passes. Each binary ran
separately, so LDS failure did not prevent remaining cases from executing.

Only failure: `server_network_search_reaches_server`, preflight
`MISSING_TEST_STAND`, local LDS127.0.0.1:4841 refused10061. CODESYS4840 is not
an LDS/registration fixture. This is an external stand blocker, not a product
defect or a passing skip. No broad-interface mDNS was sent or mDNS acceptance
claimed. Thus **not a fully green unfiltered suite**. No new product defect was
observed in the executed cases; this does not prove none exist.

Separately S03 local sample/proxy: **2/2, 49.84 s**, also serial, not CODESYS
passes. Large:46 browses/482 unique items, Read120/120 Good in20.5638ms;
refresh Browse10.3386ms with10 monitored items. Views absence/graceful does not
mean actual View execution. Raw: `%LOCALAPPDATA%\Temp\opencode\sol-full-live.log`.

## Write safety

Only authorized CMD env override:
`ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.Cmd_ChokePanelActive`.
Read-only state:
`ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.ChokePanelActiveSwitch`.

Baseline0/Good; ua_write confirmed0→1→0. Minimal subscription pulse accepted1
at10.494ms and restore0 at20.7775ms from client start, Good readback0. Original
write-push got1/Good, source/server17:31:27.962UTC, then restore0/Good. State
observed0/Good, no asserted CMD→state semantics or proof of no short transition.

In addition to subscription cleanup scope, the serial runner had independent
process-level `finally` for both write-capable binaries: restore captured CMD
original and require separate Good CLI readback before continuing. Both printed
`PROCESS_GUARD_RESTORE_VERIFIED`; final CMD/state reads0/0 Good. ua_write's
historical arbitrary-tag default path was not exercised: env-selected CMD only.
No other tag or PLC setting written. Network/runtime loss could prevent physical
restoration and would abort/fail, not count as PASS.

## Cleanup design and verification

The six named I4 APIs are not dead across supported fixtures: ReadI4 is used by
hook AsyncLease; open/read_i4/write_i4/C imports are used by unit tests and
source-included `rust-stand/src/sdk_live.rs`, hook counters use read_i4, and the
C++ core probe uses that C ABI. **Zero such APIs deleted.** Six item-specific
non-unit-test dead-code annotations document compatibility purpose; no blanket
suppression, changed SDK I/O, or ABI removal. The five native warning diagnostics
are classified, not misrepresented as deleted code.

UI Binding::scope is read only by the collision oracle: field now cfg(test),
existing key/modifier/focus behavior unchanged, unit scope table retained.
build.rs now rejects hooks without a nonempty explicit SOFTING_RUNTIME_LIB_DIR,
explaining vendored libs lack hooks. No-hook default/explicit override semantics
unchanged. Actual-build-script regression: before2 failed subcases (missing and
empty hook env); after **5 test methods PASS**. Empty library placeholders test
selection/diagnostics only, not SDK ABI.

First post-cleanup Cargo checks could not complete: C: exhausted, LNK1180/
ENOSPC112 (including the negative hook check failing before the intended guard).
These are NOT PASS. Only this invocation's failed opclook/opclook_cli linker PDB
outputs were removed; no source/history/other cache deleted. C: then91MB free,
D:253.7GB free. Retry uses owned D:\opclook-native\sol-cleanup-target and temp,
no incremental/debug symbols to avoid re-filling C:, with normal dev assertions.
Retry completed successfully; the failed C: attempts above remain recorded.

| Fresh isolated D: gate | Measured result |
| --- | --- |
| Default no-hook production dev build | PASS185.06s, zero Rust warnings |
| Backend default | 292 passed/0 failed/1 ignored; unit97.97s, Cargo173.00s |
| Root default offline | PASS164.80s: root269, UI262+16+21+19, CLI21, S03 sample2/2, restore guards3/3 |
| Hooks without env, actual Cargo negative | Expected101, exact explanatory build error;36.15s |
| All backend hook test binaries, real SDK link | 19 executables built, no-run, PASS27.50s; not19 executed tests |
| Source-included sdk-live/sdk-lease-live stand build | PASS9.13s with actual hook SDK library |
| Stand default mock/fact tests | 5/5 PASS; not COM interop |
| fmt / diff-check | PASS |

The default build/backend/root and hook-target build emit none of the five
native or UI-scope warnings. Standalone source-included probe builds still emit
their pre-existing unused-API warnings for APIs those probes do not call; they
were not globally suppressed. No fresh COM registration/execution/cleanup gate
was claimed: the fixture/hook evidence here is actual x86 compilation/linking.
Log: `%LOCALAPPDATA%\Temp\opencode\sol-cleanup-checks-d-drive.log`.

## Residual boundary

[#12 CLOSED completed](https://github.com/boogy777-lgtm/OPCLook/issues/12) for repaired owners/oracle/restore/warning, explicitly retaining
LDS/mDNS as external stand gate. Closure must not mean23/23, hostname coverage,
full matrix or DA acceptance. Remote/vendor DA has no stand and remains OPEN;
production Async, owner-closed N1 legal residuals and exact-head CI not accepted.
Existing issue comment updated in place without duplicates. C: remains near
full; successful D: validation is not disk cleanup or an accepted C: rebuild.

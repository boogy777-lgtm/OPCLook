# CODESYS CMD write/push/restore — 2026-11-05

Measured on the dirty working tree based on `b4bc724`, Windows i686 client,
x64 CODESYS console runtime with user profile. The owner reports the earlier
LocalSystem service instance was the wrong runtime mode; that explanation was
not independently reproduced. Actual Session/Read/Write/Push success below
establishes that the earlier #21 blocker is absent in this current local stand.
No security check disabled, PLC certificate store/configuration/logic/startup
modified by this continuation. No commit/push or exact-head CI receipt.

## Exact authorized pair

- CMD: `ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.Cmd_ChokePanelActive`
- state: `ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.ChokePanelActiveSwitch`
- Endpoint: `opc.tcp://127.0.0.1:4840`; no hostname-localhost endpoint.

Owner explicitly authorized CMD mutation and read-only observation of this
state. No other tag was written. No active-project transition semantics are
assumed merely from the names or older source declarations.

## Baselines and first minimal pulse

Independent CLI baselines at 17:23:24 UTC: **CMD I64(0), Good; state I64(0), Good**.
`ua_cmd_pair_minimal_write_restore_and_observe` independently registered exactly
these two nodes, required writable CMD, and read both Good baselines again.

One CMD target Write **0→1** was accepted at elapsed **10.5219 ms**. The cleanup
scope immediately sent the restore Write **1→0**, with no intervening Read,
sleep or push wait; restore acceptance was observed at elapsed **20.9832 ms**.
These are client observations, not wire/device execution timings. Correlated
Read confirmed CMD **0, Good**. Test: **1 passed / 0 failed, 1.29 s**.

State was monitored read-only during this test. A queued subscription value was
**0, Good** (source time 17:23:24.729 UTC; possibly the initial snapshot), and
after the 1.1 s observation window the state Read was still **0, Good**.
This does not establish that no shorter state transition occurred.

Separate CLI connections after this test confirmed **CMD=0 Good / state=0 Good**
at approximately 17:23:26 UTC, before the second test was allowed to start.

## Exact original write-push test, only after verified restore

`OPCUA_TEST_WRITE_NODE` was set to the full CMD NodeId above. Then:

```text
cargo test -j 2 --test ua_subscription ua_subscription_pushes_values_without_reads_and_writes -- --ignored --exact --test-threads=1 --nocapture
```

The existing test independently read CMD baseline **0, Good**, waited for its
baseline subscription push, wrote **1**, and received a current-connection
subscription push **I64(1), Good**, source/server timestamp
**2026-11-05T17:23:28.696Z**, received **17:23:28.707934Z**. No explicit Read
was issued between the target Write and this push. It then restored **0** and
confirmed it through correlated Good readback: `RESTORE_OK`.

Result: **1 passed / 0 failed, 2.16 s**. Separate CLI connections afterwards
again confirmed **CMD=0 Good / state=0 Good**, at 17:23:28.867 / 17:23:28.999 UTC.
State was not independently subscribed during this second test; before/after
reads alone do not prove its behavior while CMD held 1. No state was written.

Total: **two target Writes + two restoring Writes**, all to this CMD only.

## Safety and acceptance boundaries

Both tests use the existing `with_restore` unwind-safe cleanup scope: a caught
body panic still runs acknowledged restore and Good readback before rethrowing;
cleanup failure fails loudly rather than PASS. Fresh offline panic/restore-error/
bounded-target guards **3/3 passed**, five live cases ignored in that command.
No deliberate live fault/panic was injected into the PLC, so physical restore
under a lost runtime/network is not claimed. The guard cannot guarantee remote
state when communications disappear.

Only a new ignored exact-pair test was added to `tests/ua_subscription.rs`;
production code unchanged. Fmt and diff-check PASS. Earlier full offline
root/backend runs remain historical, not rerun for this addition.

Raw log: `%LOCALAPPDATA%\Temp\opencode\sol-cmd-live.log`.
#12 write-push/normal restore is now verified in this narrow authorized CMD
scope; LDS/mDNS and unrelated warning remain separate. #21 local Session blocker
resolved and issue closed as completed on this stand, not a general
policy/certificate root-cause proof. Existing #12 comment updated in place;
no duplicate comment/issue created for this successful run.
Remote/vendor DA remains OPEN without a stand; production Async and exact-head
CI are not accepted by these UA tests.

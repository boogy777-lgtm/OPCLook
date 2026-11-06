# CODESYS Session security blocker — 2026-11-05

**Local blocker resolved on the console/user-profile stand:** [fresh actual
CMD write/push/restore proof](codesys-cmd-write-proof.md), two live cases 1/1
each, fresh Good baselines and independent restore readbacks. Owner reports the
earlier LocalSystem service mode was wrong; not independently reproduced.
No application security fix/bypass or general certificate-policy proof claimed.
Issue #21 closed as completed for this local blocker. The original failure
evidence below is retained, not the current runtime verdict.

[Issue #21](https://github.com/boogy777-lgtm/OPCLook/issues/21) records the new
live failure outside the original five test-side/environment cases in #12.
Source tree: dirty `b4bc724` plus existing changes; Windows i686 client / x64
CODESYS runtime. No code fix, commit/push, or exact-head CI claim.

## Requested pair and non-mutation boundary

- CMD: `ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.Cmd_ChokePanelActive`
- state: `ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.ChokePanelActiveSwitch`

User authorized CMD mutation and neighboring-state observation. Both baselines
remain **unknown**, not zero. Four fresh CLI Read attempts failed before values:
Auto/default: `OPC UA session activation timed out` for each node; explicit
None/None, `--trust-server-cert --timeout 10`: `operation timed out` for each.
Client-only arguments did not change PLC configuration or certificate stores.
**0 PLC Writes, 0 write-push mutation executions**, no target selected, no state
transition measured, no restore needed. Existing unwind-safe restore scope and
its earlier 3/3 offline guards remain unchanged, not freshly revalidated here.

## Fresh protocol evidence

Numeric-loopback discovery passed **1/1, 0.11 s**, reporting three endpoints
including numeric `127.0.0.1:4840`, policy None/mode None:

```text
cargo test -j 2 --test ua_search search_ua_servers_on_loopback -- --ignored --exact --test-threads=1 --nocapture
```

Current read-only subscription check failed **0 passed / 1 failed, 25.05 s**:

```text
cargo test -j 2 --test ua_subscription ua_subscription_pushes_current_time_without_reads -- --ignored --exact --test-threads=1 --nocapture
```

Before Connected, the helper timed out and retained this diagnostic:

```text
UaSession { connection: RequestId(1), revision: 1 }
UaRecovery { state: Stopped { status: 2153054208 } }
```

Locked `async-opcua-types` 0.19.0 `src/status_code.rs:660` identifies decimal
2153054208 as **0x80550000 BadSecurityPolicyRejected**: “The security policy
does not meet the requirements set by the server.” This identifies the SDK
status, not the failing handshake phase or root cause. The helper's timeout
could precede a later terminal Connect error; no stronger timing claim made.

Open: deployed server/certificate/access policy versus SDK/application behavior,
why advertised None is not producing a usable session, and terminal-error
propagation. No server defect or production regression diagnosed. No security
checks bypassed and no PLC policy/configuration/logic/runtime startup modified.

Earlier 19 live passes predate this runtime restart. They do not supersede this
fresh failure or prove current Session readiness. Actual CMD write-push/restore
can proceed only after usable UA Session and Good CMD/state baselines. Remote
DA remains OPEN, an external blocker without a stand; no acceptance.

# O11: bounded SDK-owned Republish prototype

**Research artifact only. Do not ship this patch or enable a production capability.** The SDK client patch and its wire test are isolated diffs. The pinned client is async-opcua 0.19 at VCS `9ad28fc011002398f2e8a95696a50408a14531d9`; the stand verifies the exact blobs it changes. The Cargo cache and production dependency pins are not modified. The separate sample-server copy receives only the previously documented stale Publish queue fixture patch, so it cannot suppress the later-value observation; this is not a client fix.

## Measured counterfactual

The same SDK-level live TCP test discards one actual nonempty Publish response and cuts the connection. An unmodified SDK failed the targeted assertion `SDK did not automatically Republish` (0/1). With the isolated SDK patch, the same test passed 1/1 in 9.23 seconds: the actual reactivated Session used SDK Republish to recover subscription 1 sequence 2, the existing SDK data-change callback received 52.5, then a later normal callback received 53.5, and no extra callback arrived during a one-second observation. This exercises real SDK transport, decoder and callback. It does **not** prove global exactly-once behavior or complete history.

The experimental SDK change records a sequence cursor per existing subscription, tries the next sequence using the SDK's existing `Republish` service during reactivation, and passes returned messages to the existing subscription notification handler. It stops on `BadMessageNotAvailable`, another error, a sequence mismatch, a global budget of 256 requests, or a five-second overall deadline. Transport closure is handled by the SDK's existing connection driver. Subscription deletion clears its cursor. No application Publish/Republish loop is added. These bounds and failure paths are source-reviewed, not all separately fault-injected or proven to have product-ready cancellation semantics.

## Required work before product use

* This experiment invokes callbacks before the SDK reports `Reconnected`. OPCLook intentionally closes its identity gate on Lost until revision and namespace reconciliation; those callbacks are discarded by that gate. **The sample proves SDK direct-callback behavior only, not OPCLook recovery or history completeness.** A safe integration needs an SDK-owned bounded readiness phase after application identity reconciliation, without blocking the SDK network driver or creating a second replay owner. The application must keep history unknown until evidence supports a stronger statement.
* Cursor and delivery semantics need a proper SDK design: concurrent in-flight Publish responses, gaps, duplicates, sequence wrap, reordering, subscription replacement, callback failures, and lifetime/transfer races require explicit proof and bounded handling. The prototype can replay an already-delivered out-of-order message; it must not be shipped as an exactly-once implementation. Its warning-only uncertain outcomes are not product-visible status.
* TransferSubscriptions/NewSession, secure endpoints, exhausted budget, deadline, loss while a Republish is pending, and `BadMessageNotAvailable` were not separately measured here. `BadMessageNotAvailable` is a stopping condition, not evidence that history is complete. No automatic production fork, library upgrade, or new license is introduced.

## Diagnostic runner

In a clean linked disposable worktree at this artifact's base revision, with the pinned SDK crates already cached, run:

```sh
python3 scripts/ua-sdk-republish-library-stand.py \
  --worktree /absolute/disposable-worktree \
  --client-source /absolute/async-opcua-client-0.19.0 \
  --client-dest /absolute/new-isolated-client-copy \
  --server-source /absolute/async-opcua-server-0.19.0 \
  --server-dest /absolute/new-isolated-server-copy \
  --target-dir /absolute/disposable-cargo-target \
  --baseline-log /absolute/before.log --patched-log /absolute/after.log
```

The stand checks the baseline source identity, makes verified SDK copies, applies the test and SDK patch only there, and uses invocation-local Cargo overrides. It requires the specific unpatched failure before the patched success, and refuses to label unrelated compile failures as a successful counterfactual. The network scenario is diagnostic and can be timing-sensitive; an unreproduced baseline is reported as such, never retried into a pass or made a required CI gate.

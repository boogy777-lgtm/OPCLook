# R07: SDK-owned bounded identity-readiness prototype

Historical baseline: this initial candidate has independently reproduced
readiness races. Use the [race-fix overlay and verified runner](ua-sdk-readiness-racefix-proof.md)
for the corrected experimental candidate; the initial eight-test stand alone
is not acceptance of those races.

**Research candidate only; not a production SDK, adapter or application change.** This stand builds an isolated patch on the pinned async-opcua 0.19 SDK v2 + sticky-unknown experiment. The existing SDK event loop remains the only owner of connection, Publish, Republish and callbacks. No application replay loop or client payload queue is added. The test-only sample server copy has the separately verified stale-Publish queue fixture correction.

## Design under test

The prior v2 prototype performed Republish inside `ensure_and_activate_session`, before its event loop yielded `Reconnected`. The real Engine/Gateway boundary test showed the resulting callback met OPCLook's closed identity gate ([previous proof](ua-sdk-replay-engine-proof.md)). The research candidate moves that replay into the existing SDK Connected state. After `Reconnected`, it emits `RecoveryReadyRequired(generation)` and a typed readiness watch request. While waiting, the same SDK select loop keeps driving its secure channel and service traffic, but does not poll the existing subscription Publish stream. It accepts exactly one matching, unexpired confirmation. After confirmation it runs bounded SDK Republish before polling normal Publish. Server-side queues remain server-owned; no notification payloads are held in a new client queue.

A five-second readiness deadline and checked SDK-local generation bound the phase. Wrong, duplicate, stale and late confirmations are rejected. Deadline or generation exhaustion leaves subscription polling fail-closed while the network driver continues; loss while waiting reports cancellation, and the existing sticky unknown remains true. Replay has its own five-second / 256-request bound. A previously delivered out-of-order gap makes replay `SequenceUncertain` rather than replaying an older missing callback behind a newer one. Server-queue overflow or expiry while waiting can still lose data; unknown is not cleared. The readiness generation is SDK-Session-local and must be bound to OPCLook's Connect nonce and revision by any eventual adapter integration.

## Measured

* A live same-Session SDK/proxy counterfactual reinstating only the prior pre-Reconnected replay failed the targeted assertion **`callback before identity ready`** (0/1, 12.52 seconds); the fresh runner reproduced that exact failure in 4.55 seconds. With the readiness candidate, the same real wire scenario passed: after Lost → Reconnected, wrong generation was rejected, a fresh SDK Read of 53.5 completed while unready, no callback was delivered before confirmation, then the existing SDK callback delivered replay 52.5 followed by normal Publish 53.5. History stayed unknown. The initial candidate test passed 1/1 in 7.41 seconds; subsequent checks below test the final candidate.
* A second real SDK subscription/transport test expired readiness without confirmation: a typed `ReadinessDeadline` was observed, late confirmation was rejected, a fresh Read of 65.5 still worked and no callback reached the unready consumer during one second. A later transport loss during another pending generation produced typed `Cancelled`; both the old and cancelled generation acknowledgements were rejected. It passed 1/1 in 15.27 seconds. The two live cases passed together 2/2 in 26.57 seconds before the final ordering guard; the fresh source-verified runner passed the final candidate 2/2 in 22.94 seconds.
* SDK source unit tests passed 8/8: bounded sequence window, wrap and duplicate handling, a prior out-of-order delivery blocking replay until its gap closes, deletion cleanup, readiness generation/deadline/duplicate fences, generation overflow and cancellation guard. This does not prove every cancellation interleaving or server-queue overflow behavior.

The Read of 53.5 while unready demonstrates that the network driver remains available; the later callback of 52.5 is **older than that Read**. This SDK proof is not a latest-row ordering proof. OPCLook still needs a reviewed readiness handoff and an appropriately ordered fresh snapshot after replay/stream reconciliation; without trusted clock/order evidence, do not let the early Read and replay imply a physical ordering. Current adapter code ignores the new SDK readiness event and does not acknowledge it; if these patches were enabled unchanged, the SDK phase would time out fail-closed. GUI behavior, complete history, automatic NewSession transfer/recreation, secure endpoints and all races remain open. The latest-value SDK watch is not durable event history.

## Reproduce

With offline dependencies and a clean disposable linked worktree at the source blob verified by the runner:

```sh
python3 scripts/ua-sdk-readiness-stand.py \
  --worktree /absolute/disposable-worktree \
  --client-source /absolute/async-opcua-client-0.19.0 \
  --client-dest /absolute/new-readiness-client-copy \
  --noguard-dest /absolute/new-early-replay-counterfactual-copy \
  --server-source /absolute/async-opcua-server-0.19.0 \
  --server-dest /absolute/new-fixture-server-copy \
  --target-dir /absolute/disposable-cargo-target \
  --bad-log /absolute/early-replay.log --good-log /absolute/readiness.log
```

The stand checks the SDK VCS receipt and source blobs, applies v2 + sticky + readiness patches to a client copy and the proxy/tests to a disposable worktree, then builds a second client copy with only the early replay counterfactual. It requires the specific early-callback failure, both real live cases and eight SDK unit tests. A compile failure, unrelated timeout or missing marker is not a pass. This timing-sensitive diagnostic is not a required CI gate. SDK sources, production dependency pin and application files are not modified in the real repository.

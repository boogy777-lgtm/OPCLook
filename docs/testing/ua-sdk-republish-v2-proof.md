# R07: SDK replay cursor and typed outcomes — research v2

**Not a production patch.** This follows the independently measured v1 in `ua-sdk-republish-library-proof.md`. Apply `ua-sdk-republish-v2-client.patch` with `git apply --unidiff-zero` to a verified, disposable copy of the same pinned SDK client (VCS `9ad28fc011002398f2e8a95696a50408a14531d9`). The patch includes the actual SDK unit regressions. `ua-sdk-republish-v2-live.patch` is test-only and applies to a disposable OPCLook worktree with live-test blob `7eee090282aea6f74ff14ddb81d59b658630a562`. It requires the separately verified sample-server fixture copy from the previous stand. Neither production pins nor the registry were changed.

## Measured

* The SDK's real `SubscriptionState::handle_notification`, with v1 plus `ua-sdk-republish-v1-regression.patch`, **failed**: input sequence 1,3,2,3 emitted callbacks 1,3,2,3 instead of 1,3,2. With v2, the same regression passed and the contiguous cursor advanced to 3.
* Four SDK unit tests passed: ordering/dedup and gap closure; wrap, duplicate and bounded window; cursor removal on DeleteSubscription; and typed completion/cancellation guard state. The last test simulates guard drop; it is not a measured network cancellation.
* Real transport test passed 1/1 in 12.01 seconds: the server retained dropped sequence 2, the SDK delivered 52.5 and the next 53.5 via its existing callback, and no duplicate arrived during one second. A live SDK watch subscriber observed typed `MessageNotAvailable`. This status means the next sequence was unavailable; it is not proof that history is complete.

The SDK-only prototype stores one cursor per live SDK subscription and at most 64 future sequence numbers per cursor. It recognizes wrap 0xffffffff→1, suppresses duplicates in its bounded window and clears cursor state when its subscription is deleted. A further sequence beyond that window marks the cursor uncertain; recovery returns `SequenceUncertain` instead of claiming replay success. Such far-out sequence arrivals are not guaranteed to be deduplicated. The prototype uses the SDK's existing event stream (`RepublishRecovery`) plus an SDK watch accessor for the latest typed result; it adds no App replay loop or broker. Recovery reports `MessageNotAvailable`, `Exhausted`, `Deadline`, `Cancelled`, `Failed(status)`, `SequenceMismatch`, `SequenceUncertain`, or `NoCursor`. A five-second deadline and 256-request budget remain. Actual deadline, exhausted budget, and loss while request is in flight have not been fault-injected; only guard cancellation and a real unavailable-message result have been observed.

## Open correctness gates

Callbacks are still delivered before the SDK reports `Reconnected`. OPCLook's existing identity gate therefore drops these early callbacks; v2 does **not** make application recovery ready or imply complete history. A bounded SDK readiness phase after identity reconciliation remains necessary before product integration. The watch is latest-value state, not durable per-attempt history, and terminal stream shutdown may prevent observing a cancellation event. Sequence window overflow becomes visible on the next recovery attempt, not immediately. Transfer/NewSession, secure endpoints, subscription deletion/recreation during an in-flight replay, runtime reset, and long-lived wrap/reordering remain unmeasured. Do not enable the patch or rely on exactly-once or continuous history from these tests.

## Reproduction

Use the first stand's pin/blob validation and isolated sample-server preparation. For the v1 counterfactual, apply the existing `ua-sdk-republish-library-client.patch` followed by `ua-sdk-republish-v1-regression.patch` to a verified client copy (`git apply --unidiff-zero`). For v2, instead apply `ua-sdk-republish-v2-client.patch` to a fresh verified client copy and `ua-sdk-republish-v2-live.patch` to a clean disposable linked worktree. With offline dependencies present, run from the worktree, setting an isolated `CARGO_TARGET_DIR`, debug info to 0 and incremental compilation off:

```sh
cargo test --offline --target x86_64-unknown-linux-gnu \
  --config 'patch.crates-io.async-opcua-client.path="/absolute/client-copy"' \
  -p async-opcua-client --lib recovery_ -- --nocapture
cargo test --offline --target x86_64-unknown-linux-gnu \
  --config 'patch.crates-io.async-opcua-client.path="/absolute/client-copy"' \
  --config 'patch.crates-io.async-opcua-server.path="/absolute/fixture-server-copy"' \
  -p opclook-backend --lib sdk_republish_wire_loss_observation -- --nocapture
```

The first test name filter runs four tests on v2; on v1, the named ordering regression is expected to fail. Cargo overrides and generated lock changes belong only to the disposable worktree. This timing-sensitive live test is diagnostic, not a required CI gate.

# R07: bounded SDK replay failure visibility on the wire

This diagnostic concerns the **research-only SDK v2 + sticky-uncertainty patches**, not the production dependency. It runs the real pinned async-opcua 0.19 SDK client against the sample server, with a test-only transparent None-security TCP proxy. The proxy decodes one actual nonempty Publish response and changes its sequence number from 2 to 72 using SDK codecs. This intentionally synthetic fault tests the bounded cursor's failure branch; it is not evidence that the server naturally generated that sequence. The server copy also has the separately reviewed stale-Publish fixture correction.

## Measured before/after

After the SDK delivered the modified notification, the proxy cut the real connection and the same remote Session was reactivated. With the prototype's 64-entry uncertainty guard intact, the SDK watch reported `SequenceUncertain`, the proxy observed **zero Republish requests**, and a later 68.25 reached the SDK's normal data-change callback. The sticky session history-unknown fact remained true. The guarded test passed **1/1 (9.34 seconds)** on Linux.

For a counterfactual, an isolated copy of the *same* SDK patch was changed only to omit the `recovery_uncertain` guard. The same test failed **0/1 (10.48 seconds)**: it observed `MessageNotAvailable` where `SequenceUncertain` was required. This verifies that the assertion distinguishes the guard from an unavailable next message. The fresh disposable runner repeated the specific counterfactual failure (17.90 seconds) and guarded success (13.90 seconds). This does not establish history completeness, exactly-once delivery beyond the bounded window, or the application's treatment of every error.

The typed result was observed through the SDK's watch accessor. The current production adapter does not export this research-only SDK result as a dedicated product event. Existing adapter loss/gap facts and sticky Engine uncertainty are distinct evidence, not a claim that this outcome reaches the UI. No new replay loop, production pin, GUI test or runtime SDK fork is introduced.

## Other gates from the source audit

* Secure-channel replay is open: this test's fault injector intentionally parses None-security frames and makes no claim about encrypted or signed channels.
* TransferSubscriptions replay is open: the existing sample server restart creates a new server and destroys its old subscriptions; it is not a valid proof that another live Session transferred an existing subscription. A server with retained subscriptions and controlled session takeover is needed.
* NewSession recreation, slow-consumer loss of individual watch outcomes, SDK readiness after application identity reconciliation, and duplicate handling outside the 64-entry window remain open. The sticky unknown fact does not retain every cancellation reason.

## Reproduce

Use a clean linked worktree whose `crates/backend/src/bff/ua/live_tests.rs` Git blob is `ca034b47365a1027a7b705ec105b7a519c3f235e`, offline Cargo dependencies and disposable paths:

```sh
python3 scripts/ua-sdk-republish-wide-gap-stand.py \
  --worktree /absolute/disposable-worktree \
  --client-source /absolute/async-opcua-client-0.19.0 \
  --client-dest /absolute/new-guarded-client-copy \
  --noguard-dest /absolute/new-counterfactual-client-copy \
  --server-source /absolute/async-opcua-server-0.19.0 \
  --server-dest /absolute/new-fixture-server-copy \
  --target-dir /absolute/disposable-cargo-target \
  --bad-log /absolute/counterfactual.log --good-log /absolute/guarded.log
```

The runner checks the pinned client VCS receipt and six changed source blobs, verifies and makes its separate server fixture copy, applies the existing SDK patches and a test-only probe patch, and removes one exact guard in the counterfactual copy. It requires the specific expected failure before accepting the guarded success; compilation failures are never counted as proof. Cargo overrides and lock changes remain confined to the disposable worktree. This is a timing-sensitive diagnostic, not a required CI gate.

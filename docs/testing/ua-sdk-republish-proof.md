# R07/O11–O12: SDK Republish ownership probe

**Scope:** engineering investigation, not a production capability or a claim of complete history. The locked `async-opcua-client` and sample-server crates identify VCS revision `9ad28fc011002398f2e8a95696a50408a14531d9`. The probe validates the client's VCS metadata and the Git blob identities of `connect.rs` and `services/subscriptions/service.rs`. The existing first-notification stand verifies the sample-server provenance and applies its isolated one-line stale Publish request fix to a copy only. No Cargo registry, production dependency, or application source is modified by the stand.

## Actual observation

A real SDK client and sample server communicate through a test-only TCP proxy. With SecurityPolicy None, the proxy decodes the actual SDK Publish response, discards one nonempty response, and closes the connection. The test asserts that the existing remote Session is reactivated and normal Publish resumes. In one measured run, the lost response was subscription 1, sequence 2, value 52.5:

* No Republish request was observed before normal Publish resumed and the ordinary callback did not receive 52.5.
* As a test control, one call to the existing public SDK `Session::republish(1, 2)` returned the retained message containing 52.5. It did not invoke the SDK data-change callback.
* A subsequent server value 53.5 arrived via the ordinary SDK callback. The test passed 1/1 (7.36 seconds; Linux, 2026-10-31).

The test checks an actual available retransmission and callback, not a synthetic sequence formula. Its proxy and manual Republish call are **test instrumentation**, not application recovery. It covers one ReactivatedSession case; it does not measure TransferSubscriptions, unavailable sequences, secure channels, every server, or history completeness.

With the unmodified sample server, the same experiment reached the manual Republish assertions but the later-value assertion failed. This is consistent with, but by itself does not prove, the separately investigated sample-server stale Publish queue defect. The successful experiment uses the independently verified fixture-only fix described in `ua-sdk-first-notification-proof.md`; it does not change the client SDK. The original strict first-notification test remains ignored for the original server. The server issue is not presented as the cause of this client SDK's missing automatic Republish.

## Source findings and blocker

At the pinned client source, `ensure_and_activate_session` can request TransferSubscriptions or SDK recreation for a new Session; it does not initiate a Republish recovery. The ordinary SDK `publish` path calls private `SubscriptionState::handle_notification`. The public `Session::republish` instead adds the returned sequence to the SDK acknowledgement queue and returns the raw `NotificationMessage` without invoking that callback. Existing acknowledgement state is drained for Publish and is not a retained contiguous-delivery cursor. Therefore a caller cannot obtain ordinary callback delivery simply by configuring this public method; acknowledging before the application can accept the message also risks misrepresenting delivery.

The existing OPCLook identity gate closes on Lost and reopens only after recovery reconciliation. The SDK event loop yields `Reconnected` after `ensure_and_activate_session` returns. Consequently, a proposed SDK patch that simply invokes subscription callbacks during that earlier method would encounter the closed gate and could discard replayed values while acknowledging them. This source-based integration constraint has not itself been measured by a new end-to-end replay implementation. A safe solution needs a reviewed SDK-owned recovery phase and a bounded identity-ready handoff to the existing consumer, or another SDK API with equivalent ordering; it must retain sequence/delivery state, handle cancellation and shutdown, and expose unavailable/uncertain history rather than fabricate completeness. It must reuse the SDK's existing Publish, decoding and callback ownership. This task does not implement or assert such a fix, and the scope excludes changing the application integration here.

## Reproduce

Create a **clean disposable linked worktree** at a revision whose `crates/backend/src/bff/ua/live_tests.rs` blob is `7eee090282aea6f74ff14ddb81d59b658630a562` (the stand fails closed otherwise). With the pinned SDK crates already in Cargo's cache and a disposable output directory, run:

```sh
python3 scripts/ua-sdk-republish-stand.py \
  --worktree /absolute/path/to/disposable-worktree \
  --client-source /absolute/path/to/async-opcua-client-0.19.0 \
  --server-source /absolute/path/to/async-opcua-server-0.19.0 \
  --server-dest /absolute/path/to/new-isolated-server-copy \
  --target-dir /absolute/path/to/isolated-cargo-target \
  --log /absolute/path/to/probe.log
```

The stand requires an offline-resolvable Cargo cache, uses Cargo configuration overrides for the verified unmodified client source and the fixture server copy only in that test invocation, writes its test-only patch and Cargo lock changes into the disposable worktree, and refuses to overwrite an existing server copy. It requires all three observation markers and a successful test; a compile failure or missing observation is not a pass. The stand is a diagnostic, not a required CI gate. Server timing and other environmental failures may prevent reproduction.

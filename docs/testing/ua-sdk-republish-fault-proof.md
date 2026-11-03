# R07: SDK Republish fault probes and sticky uncertainty

**Experimental SDK-only artifacts; not production integration or proof of complete history.** The tests use a disposable worktree, the client SDK v2 patch and a separate sample-server copy with the previously verified stale Publish queue fixture correction. The proxy operates on actual None-security UA TCP frames and decodes the SDK's message type. No production dependency pin, registry source, or application recovery loop is changed.

## Measured on Linux, 2026-10-31

* **Delayed response:** the proxy drops an actual initial Publish response, then holds the real `RepublishResponse`. The SDK emitted `Deadline`; after a newer scoped `Reconnected` event the test released the held response and confirmed the proxy had sent it. No stale callback arrived during two seconds, and a later server value 53.5 arrived through the ordinary SDK callback. The test passed 1/1 in 17.02 seconds. This is one observed interleaving, not a general proof of all late-response races.
* **Transport cut:** the proxy closed the actual socket on an SDK `RepublishRequest`. A live watch consumer observed `Cancelled`, the SDK subsequently recovered 52.5 once, observed no duplicate for two seconds, and delivered later 53.5. A newly subscribed watch saw the later `MessageNotAvailable`, not the earlier cancellation. The test passed 1/1 in 9.08 seconds.
* An additional SDK unit test sent `Cancelled` then `MessageNotAvailable` before reading the watch; only the latter was observable. This confirms the watch is latest-value state, not a lossless event history. It passed 1/1. A separately observed real consumer happened to see cancellation in the cut test; that does not make it reliable for slow consumers.

The small SDK-only follow-up patch sets a sticky `republish_history_uncertain` flag when the existing connected transport reports closure. It remains true for that SDK Session object's lifetime, including after a later successful value and after the latest watch outcome changes; a new SDK Session object starts false. Both live fault tests observed false before the initial cut and true after recovery and the later 53.5. This conservative fact says history is unknown, not that messages were definitely lost, and it does not preserve a detailed sequence of outcomes. A terminal stream shutdown can still prevent a consumer from seeing `Cancelled` itself.

## Reproduce

Verify and prepare the pinned SDK client and server copies using `ua-sdk-republish-v2-proof.md` and the prior pinned stand; do not alter Cargo's registry. Apply `ua-sdk-republish-v2-client.patch`, then `ua-sdk-republish-fault-sticky.patch` to the isolated client copy with `git apply --unidiff-zero`. Apply `ua-sdk-republish-fault-live.patch` the same way to a clean linked OPCLook worktree whose live-test blob is `7eee090282aea6f74ff14ddb81d59b658630a562`. Both patches are checked against those sources; the resulting client source was byte-for-byte compared with the copy actually used in the tests.

With cached offline dependencies, isolated `CARGO_TARGET_DIR`, debug information disabled and incremental compilation disabled, run separately:

```sh
cargo test --offline --target x86_64-unknown-linux-gnu \
  --config 'patch.crates-io.async-opcua-client.path="/absolute/client-copy"' \
  --config 'patch.crates-io.async-opcua-server.path="/absolute/fixture-server-copy"' \
  -p opclook-backend --lib sdk_republish_delayed_reply_after_deadline -- --nocapture
cargo test --offline --target x86_64-unknown-linux-gnu \
  --config 'patch.crates-io.async-opcua-client.path="/absolute/client-copy"' \
  --config 'patch.crates-io.async-opcua-server.path="/absolute/fixture-server-copy"' \
  -p opclook-backend --lib sdk_republish_transport_cut_during_request -- --nocapture
cargo test --offline --target x86_64-unknown-linux-gnu \
  --config 'patch.crates-io.async-opcua-client.path="/absolute/client-copy"' \
  -p async-opcua-client --lib latest_watch_does_not_retain_cancelled_history -- --nocapture
```

These diagnostic tests are not required CI gates and may fail to reproduce under different timing. TransferSubscriptions, secure channels, budget exhaustion, lifetime races and application readiness after identity reconciliation remain open. In particular OPCLook's gate deliberately rejects the pre-Reconnected SDK callback; the direct callback evidence here is not a claim of an application-level replay fix.

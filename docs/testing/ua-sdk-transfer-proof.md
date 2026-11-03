# R07: secured TransferSubscriptions across two SDK Sessions

This is a diagnostic of the **unmodified pinned** async-opcua 0.19 client and sample server at VCS `9ad28fc011002398f2e8a95696a50408a14531d9`. Both sessions use the same anonymous identity and **Basic256Sha256 / SignAndEncrypt**. The fixture's own self-signed server certificate is placed in the test PKI's trusted store, while `ua_trust_server_cert=false` leaves the SDK's certificate verification enabled. No production trust option, SDK dependency, registry source, or application recovery loop is changed.

## Measured

A live test established distinct remote SDK Sessions A and B. A created one server subscription with one monitored item (client handle 1), then received value 45.5 through the normal SDK data-change callback. B invoked the public SDK `Session::transfer_subscriptions` with A's existing subscription ID. The service returned one per-subscription **Good** result. No B-side CreateSubscription or CreateMonitoredItems call was made.

Immediately after the Good response, B's public SDK `subscription_state.get(id)` was empty. For observation only, the test added a **local** SDK `Subscription` descriptor with that same ID and a raw SDK notification callback; this does not send a server Create or recreate any monitored item. B then received 46.5 in an SDK-decoded notification carrying the original client handle 1. Its callback's SDK monitored-item map did not contain that handle. A did not receive 46.5 during a subsequent one-second observation. The stricter test, requiring raw `StatusCode::Good`, passed **1/1 in 19.69 seconds** on Linux; the verified fresh-worktree runner passed **1/1 in 8.14 seconds**.

This establishes successful transfer of a real existing server subscription and a later notification, under these fixture conditions. The pinned server source transfers the existing subscription and notification state between the sessions, which supports the identity interpretation; the test records the original monitored-item ID but does not independently query that server ID after transfer. It does not establish all-history continuity, every race, or a fully usable mapped data-change callback in B.

## Important SDK boundary

The public client `transfer_subscriptions` method sends the SDK service, returns the per-subscription results and triggers the SDK Publish owner, but it does not migrate a different Session's local `Subscription` object, callback or monitored-item map into the destination. The test's raw local observer makes this limitation visible; it must not be mistaken for a production mapping solution. The SDK's automatic NewSession recovery operates on the **same SDK Session object's existing state** and calls its separate internal transfer/recreate logic. This two-object experiment does not prove or disprove that automatic path, retransmission replay, or loss-free history.

In the pinned sample-server source, anonymous transfer requires a signed or encrypted destination and matching application URI. This test exercises that supported case; it is not an authorization or certificate-interoperability survey. Usernames, different identities, remote commercial servers, SecureChannel token renewal, full GUI behavior and NewSession replay remain open.

## Reproduce

With cached offline Cargo dependencies, use a clean disposable linked worktree whose `crates/backend/src/bff/ua/live_tests.rs` Git blob is `ca034b47365a1027a7b705ec105b7a519c3f235e`:

```sh
python3 scripts/ua-sdk-transfer-stand.py \
  --worktree /absolute/disposable-worktree \
  --client-source /absolute/async-opcua-client-0.19.0 \
  --client-dest /absolute/new-isolated-client-copy \
  --server-source /absolute/async-opcua-server-0.19.0 \
  --server-dest /absolute/new-isolated-server-copy \
  --target-dir /absolute/disposable-cargo-target \
  --log /absolute/secure-transfer.log
```

The runner checks both SDK VCS receipts, the relevant client and server source blobs, and the worktree source blob; it makes independent unmodified SDK copies, applies the test-only patch to the disposable worktree, and uses invocation-local Cargo overrides. It requires the transfer, mapped-readiness diagnostic and both value observations. Compilation errors or missing observations are failures. This local, timing-sensitive diagnostic is not a required CI gate.

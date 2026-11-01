# R07/O11–O12: SDK readiness-drain scope audit

This is a source-only audit of a disposable research prototype, not a production capability, SDK pin change, accepted fix, or claim of a vulnerability. The dynamic subscription-mutation/drain diagnostic was stopped after an automatic safety check identified a possible cybersecurity risk. **Zero subscription-mutation tests were executed for this audit.** No dynamic result or fix was accepted.

## Evidence reviewed

- The client source inspected was the published `async-opcua-client` 0.19.0 archive, SHA-256 `df6b6057fb54758328c3f069c39d55920470247e2cf183bbf9ab8a293d6604b4`. The repository's existing research patches were applied to a disposable copy for reading. The original source is associated with VCS revision `9ad28fc011002398f2e8a95696a50408a14531d9`; the established runner also verifies specific source blobs.
- In `ua-sdk-readiness-drain-client.patch`, `start_drain` checks `subscription_ids().len() == 1` once. A different count returns typed `UnsupportedSubscriptions`; otherwise it awaits up to 64 sequential SDK Publish calls inside a five-second timeout. `Ok(false)` yields `Complete`. The event-loop announcement changes the phase to `Normal` on `Complete`, and to `Failed` on other outcomes. These are source observations, not new runtime measurements.
- In the pinned SDK `src/session/services/subscriptions/service.rs`, `create_subscription_inner` awaits the Create response before locking state and adding the subscription. `delete_subscriptions` awaits the Delete response before locking state and removing IDs. `publish` takes acknowledgements under a short state lock, awaits the response without that lock, and subsequently locks state to handle the notification. The inspected code does not hold the subscription-state lock across these service awaits.
- The prototype's acknowledgement guard uses incarnation tokens for requeue after cancellation, including subscription-ID reuse. That guard addresses acknowledgement ownership; its existence does not establish that drain completion is scoped to an unchanged subscription incarnation.
- The existing `ua-sdk-readiness-drain-proof.md` records prior single-subscription, cancellation and acknowledgement measurements and explicitly excludes multi-subscription drain. Those prior receipts are not new measurements for this audit and do not establish the result of a changed subscription set.

## Unverified questions and limits

The source permits consideration of a Create or Delete completion while a Publish is awaited. It does not by itself prove that a particular interleaving occurs, that the server returns a response in that interleaving, or that a stale `Complete` or `Normal` is observed. A count returning to one after replacement would not by itself establish the original subscription's identity. Initial two-subscription behavior is inferred from the branch in the prototype and has not been newly tested here. No defect or fix is claimed.

The historical Engine proof remains tied to its frozen `9ba4db1cfab32b1c6648c9392ef9b4e07af63997` application baseline and existing guarded receipts. Those hashes and the Engine overlay were not changed or ported. This audit makes no App adoption, transport, complete-history, or SDK pin claim; the prototype's sticky history uncertainty remains part of its documented scope.

# R2/N5: SDK replay boundary through real Engine and Gateway

**Diagnostic integration evidence, not a production SDK change.** This test runs actual `Engine`, `Gateway`, the pinned `async-opcua` 0.19 client at VCS `9ad28fc011002398f2e8a95696a50408a14531d9` with the isolated research v2 + sticky-uncertainty patches, and the SDK sample server with the separately verified stale-Publish fixture patch. A test-only TCP proxy drops one real Publish response and holds reconnection. A test-only marker in the disposable backend copy records when the existing SDK data-change callback encounters the replayed 52.5 with its existing identity gate closed; it does not alter the gate or route a value.

The test uses a small consumer of Engine-accepted events; **it does not exercise the GUI or claim GUI row rendering**. A dev-only dependency in the disposable test worktree allows the existing backend fixture to instantiate the real Engine. Production manifests, SDK pin, registry and application source are unchanged.

## Measured

On Linux the targeted end-to-end test passed 1/1 (12.06 seconds); a fresh disposable-worktree runner also passed 1/1 (18.24 seconds):

1. Engine sent Connect and AddItems for one real SDK subscription. The proxy dropped the actual Publish carrying 52.5 and held reconnect while the test observed the scoped Lost and updated the server to 53.5.
2. After reconnect was released, the proxy observed SDK Republish. The SDK callback reached the existing closed identity gate with 52.5; no Engine-accepted value was 52.5. The existing Connect nonce and a newer SDK revision were checked. Exactly the original item remained registered.
3. After actual `Reconciled`, Engine produced exactly one due Read for that item. Its correlated terminal identified `RecoverySnapshot`, and the returned value was 53.5. No second Read was due at the checked instant.
4. A later SDK subscription push of 54.5 was accepted by Engine. The consumer's latest accepted value was 54.5 and `ua_history_uncertain` remained true after both the fresh Read and push.

These assertions establish one concrete gate/snapshot boundary. They do not establish complete history, absence of all timing races or GUI behavior. Old in-flight Read correlation cancellation was not separately exercised. SDK replay itself is delivered before Reconnected in this prototype and remains unsuitable for production until its separately documented readiness/ordering issues are resolved. The source of the dropped message is the test proxy; it is not an application recovery mechanism.

## Reproduce

With cached offline dependencies, create a **clean disposable linked Git worktree** with exactly the source blobs checked by the stand. Run from this artifact's repository:

```sh
python3 scripts/ua-sdk-replay-engine-stand.py \
  --worktree /absolute/disposable-worktree \
  --client-source /absolute/async-opcua-client-0.19.0 \
  --client-dest /absolute/new-isolated-client-copy \
  --server-source /absolute/async-opcua-server-0.19.0 \
  --server-dest /absolute/new-isolated-server-copy \
  --target-dir /absolute/disposable-cargo-target \
  --log /absolute/engine-replay.log
```

The runner verifies the SDK VCS receipt and six source Git blobs plus three application source blobs, makes independent client/server copies, applies the existing research patches and `ua-sdk-replay-engine-probe.patch` only to those copies and the disposable worktree, and uses invocation-local Cargo overrides. It requires the targeted test, diagnostic marker and observation line all to succeed. No cache source is modified. This is a timing-sensitive diagnostic, not a required CI gate; a failure is retained as such, not retried into a pass.

# R07: SecureChannel token renewal without Session loss

This is a local diagnostic of the **unmodified pinned** async-opcua 0.19 client, server and core at VCS `9ad28fc011002398f2e8a95696a50408a14531d9`. The test configures only the sample server's public `ServerBuilder::max_secure_channel_token_lifetime_ms(5000)` fixture option. Pinned server source computes the revised lifetime as the lesser of its cap and the client's requested lifetime; client/core source owns renewal after the configured threshold. The test does not inspect the encrypted OpenSecureChannel body and does not independently measure the returned lifetime.

The proxy forwards the real TCP stream and observes only the clear, standard SecureChannel ID and symmetric token ID in outgoing MSG headers. It does not decrypt or change payloads, renew a token itself, or introduce a second UA protocol owner. The fixture's own self-signed certificate is placed in the isolated test PKI trusted store; `ua_trust_server_cert=false` keeps existing SDK verification enabled. Production trust settings and dependency pins are unchanged.

## Measured

The live targeted test established Basic256Sha256 / SignAndEncrypt with verification enabled and read 1.0. With ongoing SDK traffic, the proxy observed token ID **1 → 2 on the same TCP socket and SecureChannel ID 2**. The SDK-reported remote Session ID was unchanged. The same locally registered subscription ID and monitored-item ID/client handle remained present; a subsequent unsolicited SDK callback delivered 54.5 and a fresh secured Read returned 54.5. No scoped ConnectionLost event was observed during the test. It passed **1/1 in 14.61 seconds** on Linux; a fresh source-verified runner passed **1/1 in 15.74 seconds**.

This demonstrates a real token change and continued operation in one local fixture scenario. It is not a proof of all server lifetime policies, long-running repeated renewals, reconnection or replay during renewal, exact missed history, GUI behavior or commercial-server interoperability. It does not claim that production has changed to a research SDK patch; there is none in this test.

## Reproduce

With cached offline dependencies, use a clean disposable linked worktree whose `crates/backend/src/bff/ua/live_tests.rs` Git blob is `ca034b47365a1027a7b705ec105b7a519c3f235e`:

```sh
python3 scripts/ua-sdk-token-renewal-stand.py \
  --worktree /absolute/disposable-worktree \
  --client-source /absolute/async-opcua-client-0.19.0 --client-dest /absolute/new-client-copy \
  --server-source /absolute/async-opcua-server-0.19.0 --server-dest /absolute/new-server-copy \
  --core-source /absolute/async-opcua-core-0.19.0 --core-dest /absolute/new-core-copy \
  --target-dir /absolute/disposable-cargo-target --log /absolute/token-renewal.log
```

The runner verifies all three SDK VCS receipts and relevant source blobs, makes independent unmodified SDK copies, applies the test-only fixture/proxy patch in the disposable worktree and uses invocation-local Cargo overrides. It requires the secured mode, a changed token assertion, same Session, fresh Read and push. Failure or missing observations is not accepted. The diagnostic is timing-sensitive and is not a required CI gate.

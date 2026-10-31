# R07: secured SDK connection after transport loss

This is a diagnostic acceptance test for the **research-only SDK v2 + sticky-uncertainty patches**. It does not change the production client pin. A test-only loopback proxy forwards opaque TCP bytes and closes the socket on a barrier; it does not decode, change or synthesize encrypted messages. The client and server SDK copies are isolated from Cargo's registry. The sample server copy retains the separately verified stale-Publish fixture correction, although this test's final check is a fresh Read.

## Measured on Linux

The SDK rejected the fixture's previously unknown self-signed server certificate and placed its exact certificate in the test PKI `rejected` directory. The test removed that **fixture-specific** rejection and placed that same certificate in the test PKI `trusted` directory. The endpoint used `ua_trust_server_cert=false`, so the existing client code kept SDK certificate verification enabled. No production trust setting was weakened. The actual connection summary reported policy **Basic256Sha256**, mode **SignAndEncrypt**, and protection **SignAndEncrypt**; the test also completed a secured Read of 1.0 and an initial subscription callback.

The test then cut the opaque transport. Actual scoped recovery events were `Lost` at revision 1 followed by `Reconnected(NewSession)` at revision 2 with the same local Connect request identity. A fresh SDK Read returned 53.5, and the SDK session's sticky history-unknown fact remained true after that Read. The final targeted test passed **1/1 in 38.06 seconds**; a fresh disposable-worktree runner passed **1/1 in 42.45 seconds** with the same observed `NewSession` and absent replay outcome. An earlier narrower expectation of `ReactivatedSession` failed twice; the diagnostic trace showed `NewSession`, so the final test accepts either SDK mode while reporting the mode actually observed. It does not turn an absent transition into success.

For this observed NewSession, the v2 SDK replay-result watch remained `None`: its prototype only starts its automatic Republish phase for `ReactivatedSession`. The typed `NewSession` event and sticky unknown fact were observed; **a typed replay outcome for this NewSession was not**. That is an explicit remaining gap. If a future run genuinely reactivates the same remote Session, the test instead requires the actual typed `MessageNotAvailable` replay outcome. Neither branch asserts complete history.

This test does not prove TransferSubscriptions of a retained subscription, encrypted-message gap recovery, SecureChannel token renewal, application GUI behavior, or every server's certificate policy. The self-signed trust preparation applies only to this test's isolated PKI. Secure mode and certificate acceptance are established for this connection; no claim is made that production has adopted the research patch.

## Reproduce

With cached offline Cargo dependencies, use a clean disposable linked worktree whose `crates/backend/src/bff/ua/live_tests.rs` Git blob is `ca034b47365a1027a7b705ec105b7a519c3f235e`:

```sh
python3 scripts/ua-sdk-secure-reconnect-stand.py \
  --worktree /absolute/disposable-worktree \
  --client-source /absolute/async-opcua-client-0.19.0 \
  --client-dest /absolute/new-isolated-client-copy \
  --server-source /absolute/async-opcua-server-0.19.0 \
  --server-dest /absolute/new-isolated-server-copy \
  --target-dir /absolute/disposable-cargo-target \
  --log /absolute/secure-reconnect.log
```

The runner checks the pinned client VCS receipt and six source blobs, verifies the server fixture and creates independent copies, then applies the research SDK and test-only patches. Cargo overrides and lock changes are confined to the disposable worktree. It requires both the untrusted-certificate rejection and the secured reconnect/fresh-Read observation; a compile failure or missing marker is not accepted. The test is diagnostic and timing-sensitive, not a required CI gate.

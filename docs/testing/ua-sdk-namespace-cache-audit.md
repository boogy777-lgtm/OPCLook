# R07: namespace-cache Session scope — source audit only

Inspected on 2026-10-31 against locked async-opcua-client 0.19.0, VCS
`9ad28fc011002398f2e8a95696a50408a14531d9`. The installed receipt identifies
`async-opcua-client`; exact Git blob identities are:

| SDK file | Blob |
| --- | --- |
| `src/session/mod.rs` | `1031d64b943900fd0e192c8f9375063b52a8ce89` |
| `src/session/connect.rs` | `6482789c73e6223fb290ca8871567c28d2ef9e20` |

Class: SDK Session identity/cache lifetime. Invariant: a NamespaceArray response
started under an old remote Session must not replace the namespace map belonging
to a replacement Session. A consumer's post-check cannot undo an SDK cache write.
N+1 cases include overlapping old/new replies, repeated server SessionIds and
concurrent reset/commit. This is a source-level risk, not a reproduced runtime
failure or a claim that an arbitrary server can trigger the interleaving.

## Inspected path

`Session::read_namespace_array` awaits the ordinary SDK Read, builds a
NamespaceMap, then unconditionally calls `set_namespaces`. That setter replaces
the namespaces in the shared encoding context. The reconnect state machine can
call `reset()` followed by `create_session()` when reactivation fails; stock
`reset` clears the SessionId and replaces the internal session counter. No
remote-Session guard spans this namespace Read and its cache commit. OPCLook's
metadata/revision post-check runs after that internal SDK write.

An external revision check can reject the stale consumer result. It does not
establish cache safety inside the SDK. The accepted same-Session
[readiness/drain experiment](ua-sdk-readiness-drain-proof.md) does not test this
NewSession boundary and must not be used to close it.

## Unverified candidate and remaining proof

A local, unpublished SDK copy sketches a separate incarnation token and one
mutex shared by reset and namespace commit. Reset advances the token and clears
the map; commit compares the captured token under that lock and rejects a
mismatch. This is not a reviewed or compiled fix. Lock ordering, other cache
writers, type-loader state, concurrent Read dispatch around reset, error mapping
and SDK reset/create coverage still require review and tests. A token captured
before Read dispatch may conservatively reject a result dispatched after reset;
that does not by itself establish the whole Session transition contract.

No successful before/after test, NewSession observation, unit result or candidate
runtime result is available for this slice. The unfinished diagnostic test and
SDK copies are excluded from the PR. Production dependencies and App remain
unchanged. Keep this gate open; a later library-owner fix needs an exact-source
regression and its own acceptance evidence before adoption.

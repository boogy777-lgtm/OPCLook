# R07 SDK readiness: race and cancellation fixes (research only)

**Historical fix-only layer:** the later [shared-guard drain prototype](ua-sdk-readiness-drain-proof.md)
supersedes this phase layout. This layer alone leaves the Reconnected-to-Announce
Drop gap; its Replay guard does not prove cancellation across every phase.
Apply the subsequent drain delta for the reviewed shared RecoveryControl guard
and bounded Engine/Gateway handoff. Results below describe this intermediate layer.

This is a delta over `ua-sdk-readiness-client.patch`, itself applied to the
pinned async-opcua client 0.19.0 VCS `9ad28fc011002398f2e8a95696a50408a14531d9`
and the existing v2/sticky patches. It does not change OPCLook's production
Cargo pin or enable replay in the application. The stand validates VCS receipt
and exact source blobs, patches disposable copies only, and never edits the
Cargo registry. The server fixture uses the separately documented sample-server
closed-Publish fix; it is not a production server change.

The candidate checks the confirmation clock under the watch write lock and
atomically expires only the matching still-Waiting generation under that same
lock. An accepted timely ACK cannot be overwritten by a stale timeout. Replay
cancellation is guarded from future construction, including drop before first
poll, and has exactly one cancellation owner. SDK-detected Closed is preferred
over a simultaneously ready ACK or replay. A non-Closed transport event also
advances a confirmed generation and polls the bounded replay once; an active
transport cannot indefinitely starve readiness or the replay deadline while
its event stream is being polled. An expired waiting state can be reported
before a physical close that has not yet been observed by the SDK: these are
not the same event.

The controlled wire test pauses only the *test copy* immediately before select,
cuts the reactivated socket, and confirms the generation before release. The
old ACK-first selection produced `NoCursor` instead of cancellation; the fixed
selection returned `Cancelled` and rejected the stale ACK. It does not claim to
order a physical cut before the SDK observes Closed. The other live test held
the handoff while repeated real SDK Reads made progress, observed 53.5 through
a Read, then observed replay callback 52.5 and subsequent push 53.5; history
remained unknown. This demonstrates SDK callback-stream ordering, not a global
ordering relative to that Read. A post-drain application snapshot and its
identity/correlation fencing remain future work. The timeout/loss wire test
checks typed deadline/cancellation, rejected late ACK, and normal network Read
while unready.

An independent source-function stand ran 2,000 concurrent confirmation/expiry
races: exactly one transition won and the final state agreed with that winner.
The SDK unit tests exercise expiry/ACK fencing, cancellation before first poll,
exactly one cancellation, and existing cursor/sequence cases. The runner creates
explicit old-behavior counterfactuals for deadline and unpolled cancellation;
it requires those regressions to fail and the patched source tests to pass.
The wire test's barrier patch is a diagnostic instrument only and is not part
of the candidate SDK delta.

On 2026-10-31 the fresh stand passed all 14 SDK recovery unit tests and
all three wire tests: busy Read/replay 13.04 s, deadline/loss 20.74 s, and
simultaneous ACK/Closed 14.65 s. Both counterfactual unit tests failed with
the expected assertions before the fixes. The old ACK-first wire result was
measured separately, not rerun by this stand.

Run from this repository with Rust 1.99.1 and a clean disposable linked
worktree whose UA live-test blob matches the receipt:

```text
python3 scripts/ua-sdk-readiness-racefix-stand.py \
  --worktree /path/to/clean-disposable-worktree \
  --client-source /path/to/async-opcua-client-0.19.0 \
  --server-source /path/to/async-opcua-server-0.19.0 \
  --scratch /path/to/new-nonexistent-scratch-directory \
  --target-dir /path/to/separate-cargo-target
```

Results are SDK-research evidence, not an application acceptance or a full
history guarantee. SDK callbacks run while its subscription-state lock is held;
future adapter integration must not query that state or hold the backend
identity lock across ACK/drain. Transfer, secure sessions, and destination
mapping are outside this fix-only regression. The candidate also does not
provide a drain-complete handoff or prove an Engine/UI latest-value fence.

# R07: experimental SDK drain and Engine boundary

This is a disposable research prototype on top of the separately reviewed
readiness racefix, not an enabled production capability or an SDK pin change.
The client uses the locked 0.19 VCS `9ad28fc011002398f2e8a95696a50408a14531d9`
with the v2, sticky-uncertainty, readiness and racefix patches. The separate
sample-server patch fixes the previously documented closed-Publish fixture bug.
The runner verifies source receipts and copies both SDKs. It applies the Engine
and adapter changes only to a clean disposable linked worktree.

**Application source baseline:** use a clean linked worktree from
`9ba4db1cfab32b1c6648c9392ef9b4e07af63997`; parent verified that all WORK_BLOBS
match it. The later O18/O19 production Read-loop change modifies four of these
receipts. This historical Engine proof has not been ported to that change.
Keep the guarded hashes; do not replace them without applying/reviewing the
adapter overlay on the new source and rerunning its acceptance.

The SDK prototype pauses its existing subscription stream while it runs bounded
replay followed by at most 64 sequential SDK Publish calls with a five-second
deadline. It accepts this drain only with exactly one SDK subscription. The
first response reporting `more_notifications=false` ends this controlled drain;
this is not a promise that future server changes cannot arrive, nor proof of
complete history. A cancelled SDK Publish returns its taken acknowledgements to
the SDK queue; duplicate acknowledgements may be rejected by the server. An
SDK-owned phase/generation event reports the result. One RecoveryControl guard
covers Announce, Waiting, Replaying, Draining, DrainAnnounce and a failure
awaiting delivery; it is disarmed at the terminal event. SDK-observed Closed
wins over a simultaneously ready DrainComplete. A bounded non-Closed event
budget fails closed if it prevents announcing completion. The guard emits at
most one cancellation before terminal *stream* delivery. A separate watch
receiver may observe an earlier failure followed by cancellation if the stream
is dropped before delivering that failure; no global exactly-once terminal
claim is made across watch and stream. The experiment does not
add a second transport, Publish loop in the adapter, or a recovery loop in App.

The disposable adapter holds the push identity gate closed while it collects
canonical registrations and item metadata. Metadata-only SDK Reads use the
Connected revision as a pre/post guard while the ordinary Read and push gate
remain closed. Only after metadata is ready does it acknowledge the SDK
generation tied to that remote revision and the owning Connect session. It
awaits matching drain completion, then publishes Reconciled and opens the gate
under the existing revision mutex. SDK subscription locks are never acquired
while holding that mutex. SDK callback values during replay and drain are
dropped at the existing closed gate; they are not buffered ahead of the later
snapshot. Typed internal failure and Changed/Lost abort the candidate handoff.

## Measurements and scope

On 2026-10-31 the original adapter candidate timed out in Reconciling (54.06 s):
its metadata Read required the closed push gate. After adding the narrower
revision-scoped metadata Read, real Engine + Gateway + SDK + sample server with
the test proxy passed (10.09 s). After typed failure and revision mutex changes,
the targeted test passed again (13.74 s). The final shared-guard candidate
passed directly (17.63 s) and, after the generation-overflow correction, in a
fresh VCS/blob-verified stand (15.45 s): actual dropped Publish 52.5,
reactivation, metadata-ready ACK for revision 2/generation 1, drain Complete,
existing closed gate discarded 52.5, exactly one Engine RecoverySnapshot returned
53.5, and a later push returned 54.5. The original Connect nonce and one item
remained; sticky history uncertainty remained true. A single SDK-only direct
callback drain test passed in 8.40 s. The final shared guard passed 16 SDK recovery unit tests, including a
compiler-exhaustive phase match, all nonterminal drops, the same guard across
phase transitions, pending failure including generation exhaustion, and terminal disarm. Two ACK guard units
passed, including Delete followed by reuse of the same server subscription ID.
The prior per-phase variant's instrumented SDK-observed Closed/Complete test
passed (29.90 s); reversed priority failed (29.72 s). On the final shared-guard
variant, the instrumented test passed directly (29.00 s); after the
final generation-overflow correction its fresh source-verified stand passed
(30.93 s). In the first fixture
attempt proxy close alone failed to establish that SDK Closed was ready; it is
not counted as an SDK ordering observation. A unit test of the adapter's scope predicate
accepted the matching revision/generation and rejected stale revisions and
generations; this is not a wire-level stale Connect injection. An SDK guard test
with an actually polled pending future and real SubscriptionState passed; a
counterfactual omitting ACK requeue failed (0 instead of 1 ACK).

This proves the controlled Engine boundary only. It does not exercise GUI,
old in-flight Read cancellation, multi-subscription drain, NewSession transfer,
secure transport or complete history. It does not order future Push and a Read
snapshot when their clock relationship is unknown. A physically closed socket
may not yet have been reported as Closed by the SDK; no claim is made about
ordering before that observation. ACK requeue protects only cancellation and subscription-incarnation reuse; it
does not establish per-ACK status correctness or revoke an ACK already sent.
The SDK namespace-array helper updates its own cache before the adapter's
post-revision check, so this same-session result does not prove NewSession
namespace-cache safety. Production integration requires independent
review, tests for stale Connect/late ACK and loss while draining, and a decision
about supported subscription count and the upstream SDK owner.

Run with a clean disposable linked worktree and cached offline dependencies:

```text
python3 scripts/ua-sdk-readiness-drain-stand.py \
  --worktree /path/to/clean-disposable-worktree \
  --client-source /path/to/async-opcua-client-0.19.0 \
  --client-dest /path/to/new-client-copy \
  --server-source /path/to/async-opcua-server-0.19.0 \
  --server-dest /path/to/new-server-copy \
  --target-dir /path/to/separate-cargo-target \
  --log /path/to/new-log
```

For the distinct Closed/Complete interleaving, use a second clean disposable
worktree and new client/server/barrier/log paths:

```text
python3 scripts/ua-sdk-readiness-drain-closed-stand.py \
  --worktree /path/to/clean-second-worktree \
  --client-source /path/to/async-opcua-client-0.19.0 \
  --client-dest /path/to/new-instrumented-client-copy \
  --server-source /path/to/async-opcua-server-0.19.0 \
  --server-dest /path/to/new-server-copy \
  --barrier /path/to/new-barrier-directory \
  --target-dir /path/to/separate-cargo-target \
  --log /path/to/new-log
```

The test-only barrier waits until the SDK has actually observed `Closed`, then
supplies that result alongside the ready completion to the select. It is not
part of the SDK candidate. The script runs only the matching targeted wire
test; SDK unit and per-incarnation ACK tests are separate measurements above.

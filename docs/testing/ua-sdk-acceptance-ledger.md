# R2.6 / R07 UA recovery: acceptance ledger

**Scope and source:** this ledger separates current application behavior from experiments updated on 2026-10-31, including the reviewed readiness/drain research packet. It is not a claim that the whole R2.6/R07 plan is complete. The application uses the locked crates.io `async-opcua` client 0.19; the installed client VCS receipt identifies `9ad28fc011002398f2e8a95696a50408a14531d9` ([source audit](ua-sdk-0.19-recovery-audit.md)). The SDK owns UA connection, Publish, callback and subscription transfer/recreation. OPCLook does not add an independent Publish or replay owner.

**Evidence labels:** “source” is inspected code; “unit” is an actual code-path test with a controlled boundary; “live” is a measured local SDK/sample-server run; “research” means an isolated, unshipped patch or test stand. Live results are individual measured scenarios, not guarantees for all servers or platforms. None of the research SDK or server patches changes the production pin.

| Route | Current application and evidence | Remaining acceptance |
| --- | --- | --- |
| O10: transport loss, local Connect epoch retained | **Source/unit/live:** SDK event stream drives scoped Lost and Reconnected; Engine checks Connect nonce plus SDK revision. A local None-security reactivation was exercised; the separate secure test measured verified Basic256Sha256/SignAndEncrypt Lost → **NewSession**, not reactivation ([secure proof](ua-sdk-secure-reconnect-proof.md)). | Stock SDK [token renewal](ua-sdk-token-renewal-proof.md) measured one same-channel token1→2 with the same Session/subscription/item and fresh Read/push. Broader secure endpoints, repeated loss/renewals and loss during renewal are not proven. Do not infer remote Session identity from a transport reconnect or local epoch. |
| O11: available missed sequence | **Stock SDK live gap:** a dropped retained value was returned by manual public Republish but was not automatically replayed to its callback ([proof](ua-sdk-republish-proof.md)). **Research:** a bounded SDK-owner prototype replayed 52.5 in a same-Session case ([prototype](ua-sdk-republish-library-proof.md), [v2](ua-sdk-republish-v2-proof.md)). | Production automatic recovery is not established. Earlier v2 callback replay occurred before Reconnected and met the closed identity gate. The later [readiness/drain prototype](ua-sdk-readiness-drain-proof.md) deliberately suppresses replay behind that gate and accepts one post-drain Engine snapshot; it does not provide application history replay. |
| O12: unavailable/uncertain gap | **Source/unit:** Engine marks history uncertain on Lost and incomplete on observed gap/overflow; App surfaces recovery/facts and CLI emits recovery/notification records. A successful fresh Read does not clear those facts within the current Connect epoch. **Research live:** a synthetic out-of-window sequence led to SDK `SequenceUncertain`, no Republish and later value ([wide-gap proof](ua-sdk-republish-wide-gap-proof.md)); timeout/cut tests and sticky unknown are [separate](ua-sdk-republish-fault-proof.md). | Research typed replay outcomes are not a production adapter event. A bounded outcome and snapshot can support a current view; neither proves all missed history. The latest-value watch may overwrite a detailed cancellation. |
| O13: new remote Session/subscription | **Source/unit/live:** revision-scoped reconciliation distinguishes NewSession from ReactivatedSession, clears stale SDK type/negotiation state, checks namespace identity, and reports missing/ambiguous registrations. The sample restart and metadata-reload tests exercise selected NewSession paths. Secure research observed NewSession and a subsequent fresh Read. | Research SDK v2 emits no typed replay result for its NewSession branch. Reconciliation and transfer/recreation are not evidence of a continuous stream or complete history for arbitrary servers. |
| O27: stale callback, registration or alias | **Source/unit/live:** captured callback scope, current revision, gate, registration and namespace checks reject stale/unknown identity. Engine rejects old scoped values/facts and current missing keys; NewSession invalidates UI tree context. A deliberately changed namespace and delayed operation were tested in local fixture scenarios. OPCLook does not use RegisterNodes aliases in this path. | Same-epoch lifetime races and all namespace/server combinations are not exhaustively proven. If aliases are added later, they must be scoped and reconciled; current absence is not a test of alias transfer. |
| O28 / R07: one library owner | **Source:** current SDK owns lifecycle; adapter observes and reconciles, Engine schedules logical Reads only. **Research live:** [secure transfer](ua-sdk-transfer-proof.md) used the public SDK service between two Sessions, obtained exact `Good`, and received a later value on the retained server subscription. | Cross-Session public transfer did not populate the destination SDK local item map. Its test-only raw observer is not a mapped application callback. Automatic NewSession transfer on one SDK Session object remains a separate gate. No research fork or SDK patch is production-enabled. |

## What the current latest-value path actually provides

On scoped Lost the Engine cancels covered Reads and marks history unknown; it does not label a cancelled or missing result as a successful value. After accepted Reconciled, it creates one coalesced `RecoverySnapshot` demand per currently registered, non-blocked key through the existing Read scheduler. Values, quality and Read terminal outcomes remain distinct. Reconciliation missing keys are blocked rather than revived. App recovery events clear stale tree context and expose missing registrations; the CLI has structured recovery and notification facts. These are current-source behaviors, not a claim that every UI state or commercial server has been live-tested.

The [Engine/Gateway research stand](ua-sdk-replay-engine-proof.md) measured the narrow boundary: SDK replay 52.5 met the closed gate, one Engine RecoverySnapshot returned latest 53.5, a later push 54.5 arrived, one item remained registered, and history stayed unknown. The stand uses a model consumer, not GUI rendering, and did not measure old in-flight Read cancellation. A snapshot is a current-value repair, **not historical replay**. The requirement here is an honest latest view with unknown/incomplete history when warranted; complete historical reconstruction is not assumed to be the product requirement.

## Decisions and gates before a production SDK candidate

1. **Identity-ready replay:** design and test an SDK-owned bounded recovery phase/readiness handoff so SDK replay cannot be acknowledged and delivered behind the adapter's closed identity gate. The existing network driver must remain able to process Read/Browse, loss, timeout and shutdown. No second App/Engine replay loop, silent queue, or dropped outcome.
2. **Observable outcomes:** make NewSession, unavailable sequence, deadline, cancellation, budget exhaustion, uncertain ordering and later recovery distinguishable without a latest-only channel silently erasing material evidence. Preserve an explicit unknown/incomplete state; the research sticky boolean is a conservative fact, not durable event history.
3. **Transfer and identity:** establish mapped callback/item ownership for the supported automatic NewSession path, including missing/recreated IDs, namespace changes and deletion/re-add races. The two-Session raw observer test is not a replacement for this.
4. **Boundary tests:** run secure-channel recovery with retained notifications and token renewal, transfer with retained subscriptions, bounded out-of-order/duplicate/wrap cases, outage/cancellation and realistic server variants. Verify current-value/readback behavior separately from loss/history. Cross-platform and Windows results must be reported on their exact code heads.
5. **Candidate adoption:** choose a reviewable SDK library patch or replacement and its source/distribution and license evidence, then change the production dependency only after the above behavior is testable and reviewed. The owner has authorized library-candidate research; this ledger does not assume an external hosting or approval blocker. It does not authorize an upstream message or creating a remote repository.

The earlier next-step proposal was a **disposable SDK-only readiness-phase prototype**: expose a bounded post-Reconnected handoff, keep the SDK transport driver running, and test delayed acceptance, disconnect/cancellation and no premature acknowledgement/callback using the existing SDK transport and identity gate in an isolated stand. Follow it with the narrow adapter handoff only after its contract is reviewed; do not introduce an application replay owner. Transfer map migration remains open; one stock-SDK secure token renewal is independently measured, without implying repeated renewals or replay during renewal.


## Reviewed bounded readiness/drain research

The [original readiness layer](ua-sdk-readiness-proof.md),
[racefix layer](ua-sdk-readiness-racefix-proof.md), and final
[shared-guard drain proof](ua-sdk-readiness-drain-proof.md) form ordered,
disposable SDK overlays. The original and fix-only layers are historical
baselines, not the final cancellation design. VCS/blob-verified runners apply
v2 → sticky → readiness → racefix → drain; diagnostic Closed barriers are separate.
Production Cargo dependencies and App sources are unchanged.

Measured final candidate: SDK recovery16/16, ACK Drop/incarnation2/2,
revision/generation predicate1/1; fresh Engine/Gateway15.45s and instrumented
SDK-observed Closed/Complete30.93s. One guard remains through nonterminal phases;
metadata-only Reads run under a narrower revision scope while the value gate
is closed. Matching metadata-ready ACK enables bounded SDK-owned replay then
single-flight Publish drain (one subscription, five seconds/64 calls).
DrainComplete → Reconciled → one RecoverySnapshot53.5 → later push54.5 was
observed; replay52.5 was discarded by the closed gate and history stayed unknown.
The SDK network driver remains the transport owner. No adapter Publish loop or
payload queue is added.

This accepts a same-Session ReactivatedSession research scenario only. The first
`more_notifications=false` is not full history, no future server-change promise,
and no general Read/Push ordering proof. Guard cancellation is once before
terminal stream delivery; a watch receiver may separately observe failure then
Cancelled. ACK requeue covers local cancellation/incarnation reuse, not all
per-ACK statuses or requests already sent. Namespace-array cache mutation occurs
inside the SDK before the adapter's post-revision check; NewSession safety needs
its own scoped-commit proof. Stale Connect/late ACK/loss during drain wire tests,
multi-subscription, mapped transfer, secure interleavings, GUI and candidate
adoption remain open. Next independent slice: namespace-cache stale commit;
manual/secure fixtures already measured are not rerun as substitutes for it.


The subsequent [namespace-cache source audit](ua-sdk-namespace-cache-audit.md)
records exact locked SDK blob identities and the unguarded post-Read commit
path. Independent static review accepted the source-only description. There is
no reproduced NewSession cache failure or verified fix for that slice; its
unfinished SDK/test copies are excluded. This remains an adoption gate.


## Bounded SDK Publish ACK-result candidate (pair29)

The [ACK-result overlay and runner](ua-sdk-ack-results-audit.md) extend the existing
SDK service owner. Per-position exact Good is distinct from non-exact raw status,
missing and excess results. A fixed-size latest summary and Session-object-lifetime
ACK-specific uncertainty are observable; this is not a durable event journal or
a replacement for republish history/App state. Callback notification delivery and
more_notifications remain independent. Local guard commit does not assert server
ACK acceptance, and no status retry is introduced. Historical request IDs do not
control a current subscription incarnation. The getter takes the SDK state mutex:
query outside callbacks and backend identity locks.

Both owner and independent root fresh runs authenticated the published client
archive against Cargo.lock and applied ordered overlays. Counterfactual summary
omission failed as expected; fixed3/3, cancellation/incarnation2/2 and recovery16/16
passed. All40 package files and isolated paths were checked; corruption, source
change, extras, symlinks, path overlap and log preservation guards passed. These
are in-process SDK response tests with synthetic opaque callback data, not wire
decoder/live server/Windows/GUI or production adoption. Namespace, multiple
subscriptions, transfer/secure interleavings and final candidate adoption remain open.


## Production adapter Read operation limits (pair30)

[O18/O19 proof](ua-read-limits-proof.md) records the original failing normal
Gateway Read and the bounded fix using stock SDK Read calls. Positive advertised
limit is capped locally; absent/zero metadata has a nonzero fallback, and only
whole-service size rejection halves the actual failed slice. The ordinary live
server tests cover max3/five keys and deliberately stale cached5→2 with a
per-item error in position. Parent Linux backend268/0/1knownignored passed.
This is not a dedicated cap-timeout/cardinality/reconnect or SDK type-loader
proof. Scoped fences are retained; existing unscoped metadata fallback is not
promoted to an identity guarantee. Windows new-head acceptance is pending.

The earlier readiness/drain Engine overlays still require historical source
receipts. Parent static verification found all final drain WORK_BLOBS match
9ba4db1, while the new Read implementation changes core, connection, session
state and live-test blobs. Keep those historical proofs reproducible on frozen
9ba. Combining this production adapter change with that experimental handoff
requires an actual port and fresh SDK/Engine acceptance; no hash-only update.


The656 Windows backend passed the new Read-limit cases, but its existing
late-result timeout regression failed. A [controlled deadline repair](ua-deadline-proof.md)
now rejects results first observed at/after budget, without repolling completed
futures and retaining bounded drain/quarantine. Root backend268/0/1knownignored
passed again. This does not determine a remote completion instant or remove
frozen-source research limitations; repaired Windows CI is pending.

## Pair31 — malformed data Read cardinality

[Actual-source proof](ua-read-limits-proof.md) rejects excess results with an
explicit Read error; short prefix/missing-key policy is retained. Helper1/1 and
ordinary O18/O19 live2/2 passed. No malformed wire, cap-query cardinality or new
recovery acceptance claimed; frozen SDK research receipts remain9ba.

Receipt 2026-11-01 Asia/Yekaterinburg: exact repaired code
`266c683d303f396b67a9e69e054bb27f62c8248d` passed9/9 jobs
(SDK41231426756, COM41231426901, App41231426880, package41231426739).
The new pair31 admission/cardinality packet needs its own exact-head CI.

Final receipt2026-11-01 Asia/Yekaterinburg: exact code
`5fcaab98df0183ad61f7034e9f9198f8131db954` passed9/9:
SDK41232803418, COM41232803478, App41232803411, package41232803483.
Actual-source excess regression and ordinary O18/O19 passed Windows. Both
local SDK admission tests and strict cleanup passed; this does not add a
malformed wire/metadata-cap/recovery or production Async acceptance.

## Pair32 — metadata cap cardinality source scope (pre-CI)

[Read proof](ua-read-limits-proof.md) requires exactly one cap metadata result.
Source before/fail and after2/2, ordinary live O18/O19 2/2 passed; malformed
wire, cap-query timeout/revision and SDK recovery remain separate. No SDK pin
or frozen9ba receipt changes.

Final receipt2026-11-01 Asia/Yekaterinburg: exact84396da7 passed9/9
(SDK41237089913, COM41237090071, App41237090037, package41237090066).
Windows metadata cardinality regression and O18/O19 passed; backend317/0/1
known ignored/1 isolated filtered249.56s. No malformed wire, new cap-query
timeout/revision race or SDK recovery adoption claim. Root270/0/1 in74.38s.

## Pair33 — SDK drain source audit only; dynamic gate remains open

[Source audit](ua-sdk-drain-scope-audit.md) records the one-subscription pre-await
check and SDK service lock boundaries as observations, not measured interleavings.
Automatic safety check stopped the dynamic mutation diagnostic for possible
cybersecurity risk. No retry; mutation tests0, verified defects/fixes0. The
original gate remains open; no new patch, pin or frozen9ba receipt change.

Final pair33 receipt: exactd401c27d passed9/9 CI for the delivered native
gate/source audit. This does not execute or accept the SDK mutation gate.
Automatic safety stop (possible cybersecurity risk), mutation tests0, no
verified defect/fix remain recorded in [audit](ua-sdk-drain-scope-audit.md).
No retry, SDK patch/pin adoption or frozen9ba receipt change.

## M09/M03–M04: migrated profile under limited capacity, pair34

On code `45e730269cf4de67018274c58f49faa97ff2cc0a`, local live sample
server / actual Settings loader / Engine / Gateway test passed 1/1 in 13.99 s.
Five-second legacy settings migrate and persist as explicit subscription profile.
With capacity one, initial read terminates for the unmonitored item and monitored
push continues; explicit-profile warning says polling is disabled. Controlled
Engine time yields no periodic fallback due for explicit profile; legacy None
produces a real fallback Read and Values. This is not a wire-level count of all
requests, external server proof or Windows proof. PollOnly means no monitored
subscription; no defect established. Test fixture port selection has bind/drop
race. Exact code head CI passed 9/9; Windows i686 library 269/269, including this test. SDK research pins and drain gate unchanged.

## Pair37: O29 worker Read timeout and reconnect

Code `7512ede914548b3a1c57cb892ccf2ee4014bfc2d`: local sample server +
real UA worker and loopback proxy holding a ReadRequest for a specific NodeId.
Observed TimeoutRead, SessionClosedAfterTimeout, queued NoConnection and
successful new Connect/Add/Read. Root 1/1 37.55s, real-file stand 1/1 38.18s.
No independent wire counter, bounded proxy-thread join or complete Engine S03
proof. SDK pins unchanged; stopped mutation/drain diagnostic not retried.
Exact-head CI 9/9; live test passed Linux and Windows i686.

## Pair38: Engine S03 timeout close window

Code `2c52d547e29194119c81e125878ff5e76eb9845e`: baseline live
Engine/Gateway/sample fixture failed by scheduling R2 after correlated
TimeoutRead and before Disconnected; fixed test passed 1/1 43.71s. Terminal
for R1, cancellation of pending readback, reconnect initial Received and
Gateway Joined observed. Original Read keys survive Remove for correlation;
unrelated keys/epochs/DA/late negatives tested. Engine 103/103, real-file
stand 10/10. Loopback None-security, bind/drop port race, no independent wire
no-repeat count or bounded proxy join; not full S03 matrix. Exact-head CI 9/9; live S03 passed Linux 38.13s and Windows i686 50.07s.

## Pair39: S03 test proxy cleanup observation

Code `476fcec0338a8fd67b20e2c1b3fa74e449dd66b9`: local live S03 and
controlled timeout/panic/unwind fixture 2/2; observed proxy join 5.110325ms;
real-file stand 2/2, 5.103038ms. Old Drop blocked held thread in a child
terminated by 2s external timeout. Polling uses a deadline and timeout keeps
the handle; zero budget rejects even completed thread. This is not an absolute
OS deadline, production cleanup or wire no-repeat counter. CI exact head 9/9; tests passed Linux and Windows i686.

## Pair40: scoped fixture evidence

Code `5ade14eec04618d4a34b5ed814f7269da5a9bf23`: test UA server hands
OS-bound listener to SDK run_with; controlled old free_port race reproduces,
real fallback/OPC Connect and real-file tests passed. Other free_port uses and
readiness heuristic remain. DA Router test with mock host shows healthy B
teardown while blocked A is detached, then A teardown on release; no actual
COM or OS deadline claim. CI 9/9; no production switch or pin change.

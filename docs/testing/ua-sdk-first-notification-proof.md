# R07: first notification after UA reactivation — fixture diagnosis

## Scope and source identity

The OPCLook strict test remains `#[ignore]` with the original, unpatched sample
server. No production client dependency, application Publish loop or recovery
policy is changed. Both installed `async-opcua-client` and sample-server 0.19.0
report VCS commit `9ad28fc011002398f2e8a95696a50408a14531d9`. The exact
server source file `async-opcua-server/src/subscriptions/session_subscriptions.rs`
has Git blob `41ad79de03c1607a45db763e91c346cb14add7e6`, matching that commit's
upstream file. The stand verifies this file and `.cargo_vcs_info.json`; it does
not assert that every byte in the published SDK package matches the repository.

## Measured causal observation (Linux, local sample server)

At OPCLook base `353050d`, the untouched locked server and client reproduced
strict failure: the SDK reported `ReactivatedSession`, the adapter reported
Reconciled and TypeMetadata, but no `43.5` value arrived during the strict
15-second wait. A second instrumented baseline run passed: new Publish requests
arrived and the old entries were displaced before the value was paired. The
failure therefore depends on timing; neither result is a universal guarantee.

A separately copied, pinned server was instrumented only for diagnosis. Its
trace showed old Publish request handle `21` still queued after transport loss;
after reactivation, a new request handle `26` entered with one queued request.
The server selected handle `21` with `response.is_closed() == true`, assigned
notification sequence `2` containing `43.5`, and discarded the failed send;
handle `26` subsequently received an empty keepalive advertising sequence `3`.
The adapter and SDK client were not modified for that observation. This is
concrete evidence of a stale pending Publish request in this sample-server
implementation, not evidence that all real servers behave this way.

The library-only counterfactual in
[ua-sdk-first-notification-server.patch](ua-sdk-first-notification-server.patch)
prunes already-closed response receivers before pairing queued requests with
notifications. On the same strict scenario, the instrumented server selected
new handle `26` with `closed=false`, sequence `2` and value `43.5`; the normal
SDK callback delivered it and the test passed. A clean server copy prepared by
the stand also passed an isolated strict test extended to require `43.5` and
then a distinct `44.5`, both with UA time facts. These are local sampled passes.

The one-line patch handles receivers already closed at the start of the server
tick. A receiver can still close between the check and sending; this experiment
is not proof of race-free server delivery or general OPC UA conformance. It is a
candidate SDK-server fix for review, not a production client patch. In
particular, Part 4 §6.7 requires a reconnect Republish procedure; pinned client
0.19 exposes a public Republish call but the traced automatic SDK event loop
does not establish the required replay behavior. That is a separate open
client SDK owner gap, not the demonstrated cause of this 43.5 sample-server
failure. No later read or value establishes complete intervening history.

## Reproduce without touching shared Cargo sources

Fetch/build the locked Cargo dependencies first using the normal environment.
Create a **clean disposable linked worktree** at this same code head, and use
new scratch paths for the patched server, the two separate Cargo targets and
logs. The worktree's lockfile is allowed to change for the path override; the
original checkout and shared registry are not modified. The runner is Linux
and requires Rust 1.99, an available loopback network and Python 3.

```bash
python3 scripts/ua-sdk-first-notification-stand.py \
  --source /path/to/registry/async-opcua-server-0.19.0 \
  --dest /scratch/new-patched-server \
  --run --worktree /scratch/clean-linked-worktree \
  --baseline-target /scratch/new-baseline-target \
  --patched-target /scratch/new-patched-target \
  --logs /scratch/new-first-notification-logs
```

A local run of the runner reproduced baseline FAIL and patched PASS with
separate target directories. It checks the pin before copying, executes the unmodified strict test
with `--ignored`, requires the actual strict assertion failure (a build failure
is not accepted), then executes the same test with the copied server path
patch and requires PASS. It writes separate complete logs and `run.json` with the exact worktree head,
test-file blob and patched source blob, and never overwrites
an existing server destination or log directory. A changed baseline, network
failure or non-reproduction stops the run and is not called a pass. This
manual diagnostic is timing-sensitive and must not be a required CI gate.
The live experiment does not include a deterministic SDK-private `tick()` unit
regression with a preconstructed closed/live pair; that remains an upstream-test
follow-up.

Local `--self-test`: 4/4 passed, covering exact anchor/one-time transformation,
Git blob hashing, provenance, no-overwrite behavior and rejection of build or
unrelated failures as a strict reproduction. Full Windows CI,
production client recovery, retransmission and long-running sample-server
correctness remain unproved by this experiment.

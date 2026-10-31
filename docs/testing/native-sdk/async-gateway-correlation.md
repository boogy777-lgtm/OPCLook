# Test-only SDK Async correlation through Gateway

This probe adds no `CommandKind` and does not enable production Async. Before a
subscription Connect, a test-only one-shot arm specifies exactly one session
owner, Connect request ID (epoch), ReadFresh request ID, ItemKey and bounded
caller wait. Only that request uses the existing SDK generic async slot and
its callback transaction; other reads retain the ordinary Sync path when no probe lease is pending. The
server/group worker, SDK registration, callback and process host manager are
unchanged. The diagnostic observes the last allocated test transaction token and
current Rust registration; it does not expose a pointer or assert global token
uniqueness. Tokens may restart with a new SDK group, so identity is scoped by
owner, epoch and registration. Slot allocation precedes task queue admission
and the SDK asyncRead call; the diagnostic dispatched flag proves allocation,
not server acceptance. The live fixture separately checks gate_entered to
observe the SDK worker task before release.

The live test asks for a real ReadFresh through Gateway while the existing SDK
worker barrier is held before submission. It checks the exact correlated Read
`TimeoutRead` terminal and no fabricated sample. The pending lease owns the
actual session and shared host; another Gateway session connects and completes
a real fresh read while the barrier remains held. After explicit release, an
ordinary Gateway Test request waits at most one second for retirement and then
checks server status. Retirement is cleanup only and cannot send another
reply for the timed-out read. Disconnect followed by a fresh Connect epoch,
new registration and a successful second scoped Async read checks a new
correlation context. The first SDK operation has already retired before that
reconnect: this is **not** a late-old-callback-across-Connect test. No release
or remote server behavior is inferred.

During a pending probe, other commands are rejected rather than manipulating
item maps under a pinned SDK task. A Disconnect ends the logical Gateway
session and Drop quarantines the SDK lease; shutdown/channel close also exits
the worker with the same quarantine. If completion never arrives, that
quarantine intentionally keeps the host alive indefinitely. The deterministic
Linux mock child tests check wrong request, wrong Connect epoch, one-slot arm,
terminal correlation, retirement, and pending Disconnect/shutdown separately;
they do not execute Win32 SDK, COM, or actual Gateway. The native token source
stand compiles the observation function with a stub slot only.

From a configured Windows x86 test bridge and fixture:

```powershell
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_gateway_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_gateway_live -- --nocapture
```

Exact `f787f43842756c58ab57beeb689e09ede9e7c920` compiled and ran this
fixture on i686 Windows in both COM variants:
[SDK inputs](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41178553338/job/124471049602)
passed1/1 in0.32s;
[official-MIT inputs](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41178553338/job/124471049388)
passed1/1 in0.33s. Both strict server/proxy cleanups passed. All nine jobs for
that code head passed. This research seam does not establish production
Async lifecycle, cancellation, DA1 behavior, remote interoperability, or
bounded recovery of an SDK task that never completes.

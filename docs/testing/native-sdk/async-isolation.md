# Native DA async session isolation gate

The private test-only I4 async probe uses the SDK's own per-server worker and
one process-wide SDK host manager. The pinned Softing SDK's
[`SOCltServer`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/include/SOCltServer.h)
contains a worker per server. Its [addServer](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCEntry.cpp) path creates a new server for each call, and
[`getWorkerThread`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SOClt/SOCltServer.cpp)
returns the server's worker member. This source fact does not by itself prove independent
progress through the SDK, COM and the server fixture.

`sdk_async_isolation_live` starts a manager-owned A session, makes its SDK
worker enter an event barrier and observes a bounded caller timeout. It then
connects a separate Gateway B and checks both acquisitions share the manager
host. With A's release event explicitly still unset, B adds an item, completes
a correlated fresh device read with Good quality and timestamp, disconnects,
and shuts down normally. The test requires `finish_native_sdk` to report Busy
while A is still held. It then releases A, waits for SDK task and callback
retirement without a second read outcome, closes A, and requires a successful
final finish. The fixture launcher owns registration and cleanup. The test-only
held-event hook only observes the event; it does not reset or release it.

The live test requires a Windows x86 test bridge and a fresh fixture process:

```powershell
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_isolation_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_isolation_live -- --nocapture
```

Local Linux source-stand coverage exercises the actual Rust SDK manager, lease
and separate router worker with mock FFI; it does not exercise SDK threads,
COM or the fixture, and does not prove the held event. Actual Windows execution
and cleanup remain CI gates. Production Async remains disabled, and the test
does not claim remote-server behavior or an absolute OS scheduling bound. The
existing manager has no JoinHandle: final finish confirms host cleanup before
the manager replies, not independent observation of its thread joining.

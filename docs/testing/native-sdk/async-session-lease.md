# Test-only SDK async session lease

This I4 probe is not a production acquisition mode. Its checked admission uses
the connected SDK server worker's `addTask` only after rejecting absent,
stopped and own-thread workers. The SDK queue owns a task reference; the task
owns the SDK item and the SDK task base retains the group. The one-slot state
belongs to the live session's callback object. Task destruction marks either
completion or queued-task abortion, including SDK queue removal. The callback
checks the nonreused transaction and item handle. A completed callback cannot
publish a successful read until submission succeeds and the SDK task has been
released. Session close and item removal return `E_PENDING` while that slot is
occupied.

`TestAsyncSessionLease`, available only with `native-da-test-hooks`, owns the
actual Rust `Session` and its `SharedHost` Arc. On timeout, the caller receives its sole terminal outcome; later retirement returns
a cleanup acknowledgement without presenting a second read value. The lease keeps both until
the serialized owner observes task and callback retirement. A failed retire
wait leaves the lease held. If this test-only lease is dropped while pending,
it deliberately leaks the session and Arc rather than terminate the SDK under
an accepted task; a callback that never arrives can therefore prevent cleanup
indefinitely. There is no new worker, COM implementation, retry, polling loop,
or production Async capability. An SDK queue lock or OS scheduler has no hard
wall-clock deadline; the test timeout covers the wait after enqueue.

Two Windows fixture entries are prepared, both require the test bridge built
with `OPCL_BUILD_PROBES=ON` and a clean owned SDK fixture:

```powershell
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- ../core-build/Release/softing_core_probe.exe --live-async
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- cargo run --locked --manifest-path docs/testing/native-sdk/rust-stand/Cargo.toml --target i686-pc-windows-msvc --features native-da-test-hooks --bin sdk-lease-live
```

The first entry holds the actual SDK worker at an event before its asyncRead,
checks caller timeout plus native session/host busy, releases it, observes the
real fixture completion, then closes. The second checks the Rust Session and
SharedHost owner with the same barrier and `Host::is_idle`, the predicate used
by the host manager before finish. It does not itself call the manager's
`finish_native_sdk`. Launcher cleanup is checked separately by the fixture.
No artificial delay inside the SDK server is claimed by these entries.

Locally measured on Linux: runtime-stand with test hooks 48/48 and rust-stand
5/5; the lease test uses the real Rust Session/SharedHost code with mock FFI.
Windows compile, live SDK task, callbacks, ABI, cleanup and multi-session
isolation are pending CI. Real SDK feature negotiation, DA1 correlation and
remote servers remain open. No wall-time or production readiness claim follows
from the stub tests.

The pinned SDK [`SODaCDataCallback::handleReadAdvise`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCCallback.cpp) forwards
`rellevantItemCount` after resolving and compacting handles, including zero.
For a matching, nonreused transaction, an empty completion with a failed
master result terminates the read with that service error; its item error is
only an `E_FAIL` sentinel, and no item value, quality, type or timestamp is
claimed. Empty success, stale transactions and a one-item envelope with another
handle are ignored. `python docs/testing/native-sdk/test-async-empty-callback.py`
compiles the actual callback and retirement branches against SDK stubs to check
this behavior; the native SDK callback test remains part of Windows CI.

The separate `sdk_async_lease_live` integration test obtains its session from
`acquire_host`, the same process-global manager as Gateway. In a fresh process
it checks manager acquisition count 0 -> 1, holds the SDK worker at its event,
requires `finish_native_sdk` to return `SDK sessions still running`, releases
and retires the task, closes the session, and requires a successful final
`finish_native_sdk`. This final response follows `Host::close` on the SDK owner
thread; the current manager stores no JoinHandle, so it does not independently
observe the manager thread join. The ready-to-run Windows entry is:

```powershell
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_lease_live -- --nocapture
```

A separate-process Linux runtime-stand test exercises the same manager and
lease code using mock native FFI, including the explicit Busy response and
final successful finish. It does not replace the Windows SDK fixture.

### SDK task destruction ordering

The pinned SDK [`SOCltTask` destructor](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SOClt/SOCltTask.cpp#L30)
releases its retained element/group in the **base** destructor. The previous
probe signalled `async_task_done` in the derived destructor, before that SDK
release. Normal session close generally queues further work on the SDK worker,
but that does not make the earlier retirement notification a safe invariant
for failure or queue-removal paths. The derived task now releases the item and
uses a private first-declared retirement base, whose destructor runs after the
`SOCltTask` base and its SDK group release. SDK refcount and COM ownership are
unchanged; [`SOCmnObject`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/include/SOCmnObject.h)
has a virtual destructor and its pinned `release` deletes through that virtual
base. The SDK worker's normal drain releases a task after dropping the queue
lock; SDK shutdown may also discard queued tasks while manipulating the queue.
The completion signal occurs after the SDK base release in both cases, but this
does not establish a bound on SDK queue locking or worker shutdown.

`test-async-task-destruction.py --task-source <pinned-sdk>/development/core/src/SOClt/SOCltTask.cpp`
verifies the source Git blob, compiles the actual adapter class and SDK base
destructor with SDK stubs, and holds group release at a deterministic barrier.
For both executed and queued-then-discarded tasks, completion must remain unset
until item and SDK group references have been released. The former task failed
this assertion; the reordered task passes on Linux. This is a controlled source
stand, not execution of the Windows SDK or an observation of a live worker join.

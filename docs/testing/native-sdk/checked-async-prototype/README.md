# Checked SDK async submission: review prototype

This is a private, unintegrated SDK extension-point prototype. Production Async
remains disabled. The executable tests below use C++ stubs and a deterministic
barrier, not Softing's actual worker, a Windows COM call, or an OPC server.

The Rust `Session` is `!Send`, owns a `SharedHost` Arc and is used by a
serialized session worker. Its native `OpclDaSession` holds the server and
item objects and increments `host->sessions`; host destruction refuses while
that count is nonzero. That arrangement protects an ordinary synchronous call,
but `session_close` presently disconnects and releases the session. It does
**not** provide a detached lease after an asynchronous timeout. A future bridge
must keep the full native session, item registration and Rust `SharedHost` live
until task and callback retirement, and reject or quarantine session close
while they remain live; simply holding the SDK group is insufficient.

Pinned SDK `SOCltServer::getWorkerThread` returns its member worker. After a
connected server and started group, the SDK group normally obtains that worker
through its parent chain; nonetheless `admission.h` checks for null, own
worker and a stopped worker and fails closed. It calls public `addTask` rather
than `executeTask` (which can execute inline). Source `SOCmnList.cpp` adds an
object reference on successful insertion; `SOCltThread.cpp` configures a
synchronized object queue. `addTask` does not itself call OPC or callback,
but its lock and operating-system scheduling have no absolute deadline.
Admission must be serialized with close/shutdown by the same session owner;
checking `isRunning` alone cannot resolve a shutdown race. Involuntary worker
failure after the check remains an SDK/lifetime integration gate.

`state.h` holds one monotonic token and terminal result. No server response is
stored in the caller's stack. The terminal is recorded once; timeout does not
retire the task or release its lease. A callback can precede the async submit
return, and is only published as a value when submission also succeeds. Late,
retired and duplicate tokens are ignored. If a callback never arrives, the
lease remains quarantined: this is intentionally not a bounded cleanup claim.
`sdk-task.h` is a concrete SDK `SOCltTask` subclass proposal. Its base retains
the group; it explicitly retains the item. Destructor notification handles a
queued task removed by SDK shutdown when the queue releases its final task
reference. An accepted callback is not represented in this header: real
`RuntimeGroup::onReadAdvise` must validate the current session, item handle,
transaction and registration before publishing to the matching heap state.
The stand's shared pointer is only a stand-in, not the required native lease.

Build and run both portable tests:

```sh
c++ -std=c++17 -Wall -Wextra -Werror -pthread docs/testing/native-sdk/checked-async-prototype/test.cpp -o /tmp/opcl-checked-async-stand
/tmp/opcl-checked-async-stand
c++ -std=c++17 -Wall -Wextra -Werror -pthread docs/testing/native-sdk/checked-async-prototype/sdk-task-test.cpp -o /tmp/opcl-checked-sdk-task-stand
/tmp/opcl-checked-sdk-task-stand
```

Measured locally: both tests pass. They cover admission rejection, queued-task
destruction, a held worker beyond the caller's deadline, host lease retention,
release and late callback, stale/duplicate callbacks, early completion and
submit failure. The SDK task test exercises the proposal against stubs only.
The existing pinned-source test separately checks the actual SDK branch and
shutdown source. Windows i686 compile, a real SDK fixture barrier, session
close/host finish, multi-session isolation and actual late callback remain
unmeasured. The SDK task proposal does not replace a product SDK-owner review.

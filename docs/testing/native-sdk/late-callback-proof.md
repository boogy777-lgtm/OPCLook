# SDK delayed callback and registration gate (A08)

This prepares a strict local Windows fixture test; it does not itself claim a
passing Windows result, remote-server coverage, or a production capability.
The probe uses the SDK callback and existing SDK server/client lifecycles. It
does not issue a synthetic `onReadAdvise` call or create another COM owner.

## Source and bounded interleaving

At Softing SDK pin `5130e763998189c822882434c43e6be2eb0174cf`,
[`SODaCDataCallback::handleReadAdvise`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCCallback.cpp)
resolves an SDK item by client handle, adds a reference, invokes `setRead`,
then calls the virtual group `onReadAdvise` and releases the item after that
returns. The SDK's `SOCltComGuard` only AddRefs/Releases its COM object and
`SODaCComGroupImpl::onDataChangeBegin/End` are empty at this pin. The new
`OPCL_DA_TESTING` one-shot event is reached at the existing `RuntimeGroup`
override before it locks the adapter's registration map; callback arrays and
SDK item references remain owned by the SDK call while it is held. The hook
claims only the configured I4 value, waits at most 30 seconds, and reports
entered, release-unset and processed/timeout; normal builds exclude it.

The fixture SDK server's `SODaSTransaction::sendReadData` releases its request
list before calling the client, but the transaction holds an advise-list lock
across that call. `SODaSComGroup::RemoveItems` takes a server work lock and
`SODaSGroup::removeItem` skips cyclic transactions while traversing remaining
requests. These source facts make a Remove/re-add candidate possible, not a
proof that all SDK/COM locking permits the interleaving. In particular, this
test does **not** hold a callback while calling Unadvise/disconnect. If the
actual fixture cannot complete the required Remove/re-add before release, the
bounded test fails and the gap remains open; it must not be treated as a pass.

## Prepared live gate

`sdk_late_callback_live` creates subscribed session A and normal session B via
one Gateway. After A's initial value 42, B writes 88 and the test waits until
the real SDK callback carrying 88 has entered the barrier. While it remains
held, B writes 99 and A removes and re-adds the same ItemKey. Only after both
correlated replies and an explicit release-unset observation does the test
release the callback. It requires adapter callback processing to finish,
checks A receives a new 99 push with the same session epoch, rejects any old 88
push, and issues a fresh correlated read as a session-worker drain fence. Both
sessions disconnect, the Gateway joins and the host finishes; the fixture
launcher verifies server/proxy cleanup. The release guard unblocks the SDK
callback even if a test assertion fails. The expected 88 is a test-supplied
value, not a product item-ID special case.

With a test bridge (`OPCL_BUILD_PROBES=ON`) and the owned SDK fixture:

```powershell
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_late_callback_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_late_callback_live -- --nocapture
```

Set `SOFTING_RUNTIME_LIB_DIR` to the hook-enabled bridge Release directory.
`test-late-callback-gate.py` compiles the actual gate code with local event/SDK
stubs to check its state transitions; it is not a COM or SDK test. Local Rust
runtime stand and compile-only checks similarly do not prove live ordering.
Actual i686 fixture, cleanup, and the independent Unadvise/epoch race remain
open until measured. The test does not enable native Async.

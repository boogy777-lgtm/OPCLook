# Test-only single-slot SDK Async value probe

This extends the existing SDK-owned async I4 probe for one value per request.
It does not enable `DaReadMethod::Async`, change the legacy default, or provide
a production Async Write. `test_async_submit_wait` is shared by the existing
I4 ABI and the new test-only generic ABI: there is one transaction slot, one
SDK task, the same non-reused token and item-handle correlation, worker
admission, retirement sentinel and `TestAsyncSessionLease`. There is no extra
worker, retry or COM owner. The lease still quarantines its SDK session and
host indefinitely if an accepted task never retires.

`RuntimeGroup::onReadAdvise` copies the SDK-owned value while its callback
arguments are alive using the same `copy_sdk_value` as Sync. Only a successful
master/item with sample pointers makes quality, FILETIME and VT available;
item errors and a failed empty master have their own availability masks.
The payload is limited to 64 KiB, and the existing codec imposes 256 elements,
one-dimensional arrays and its depth/type/BYREF bounds. Codec failure retains
attested sample facts but never becomes a Good operation. The generic result
is decoded by the existing `softing_scalar::decode_read_outcome`. A timeout
retains the same pending lease and uses existing retirement as a cleanup ACK,
not a second read. The SDK writes per-item submit status only when its own
returned error array exists; the probe tracks an untouched sentinel as unknown
and waits for callback rather than retiring on an initialized error. An SDK
status coincidentally equal to that sentinel is also treated as unknown.
A queued task removed before the SDK call is a safe local abort. If the SDK
call is entered but throws before returning, admission is unknown: the caller
receives one adapter error with no invented service/item facts, and the lease
stays pending until a correlated callback allows a cleanup-only retirement.
A never-arriving callback leaves the owner quarantined. The pinned SDK public
`asyncRead` declaration does not provide a no-throw or pre-acceptance-only
exception guarantee; the accepted-then-throw exercise is a deterministic SDK
stub, not an observed Windows SDK crash. SDK initial reads are disabled only for this test fixture,
and an actual correlated callback connection is required before submit.

`sdk_async_values_live` checks I4, BOOL, Unicode BSTR, R8/BOOL/I8 primitive
arrays and BSTR arrays, plus an oversized valid SDK array that fails closed
with raw sample facts. This test does not claim coverage of every shape, DA1,
remote servers or an actual timeout; the existing lease/isolation tests cover
their own separately bounded timeout scenarios. Build the hook-enabled bridge
and fixture and run from the repository root:

```powershell
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_values_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_values_live -- --nocapture
```

Linux `test-async-empty-callback.py` and `test-async-generic-source.py` compile
actual callback/handoff branches against SDK stubs; Rust runtime stand uses a
mock FFI. Neither is an i686 SDK or COM execution. Exact `6c5349c87334860c52a8317f8fdc5e34a9568dd1` compiled and ran this fixture
on i686 Windows with both SDK and official-MIT COM inputs: 1/1 per variant,
strict server/proxy cleanup passed ([COM run](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41176472844)).
No production capability follows from a passing test-only probe.

`test-async-task-destruction.py --task-source <pinned-SOCltTask.cpp>` uses the
actual task, retirement predicate and pinned SDK base destructor with stubs.
Its counterfactual changes only the terminal predicate back to the previous
one and must fail the accepted-then-throw assertion; the current predicate
must retain the pending slot until a matching callback and cleanup ACK. It also
checks a queued task removed before SDK entry remains immediately retirable.

# Test-only SDK Async failure and quality facts

`sdk_async_facts_live` uses the existing one-slot `TestAsyncSessionLease`, SDK
worker, correlated callback, `copy_sdk_value`, and shared `decode_read_outcome`.
It reads four ordinary fixture items separately. Good, uncertain and bad quality
are successful I4 samples with their actual timestamp; bad quality does not
turn a successful sample into a failed operation. ReadDenied is E_ACCESSDENIED
with no sample quality, FILETIME or VT. A generic Bad display quality on failure
is synthetic; it is not a server quality fact.

A read-only test-only snapshot of the same slot records whether the SDK call
was attempted and returned, whether the SDK supplied its submit item HRESULT,
and whether a matching callback was observed. Submit HRESULT and item HRESULT
are interpreted only when their flags say they exist. The test distinguishes an
immediate per-item rejection from a callback completion and prints observed
facts; it does not assume an unknown submit item is S_OK or equate a submission
HRESULT with a callback master HRESULT. The decoded failure preserves whichever
service/item facts the existing test bridge actually exposed; the test requires
a nonfailed service status and E_ACCESSDENIED, but does not invent S_OK for it.
These snapshot flags are not production API, do not retain SDK pointers, and
add no new worker, transport or session owner. A timed-out lease retains its
existing quarantine/retirement behavior.

With the pinned fixture and hook-enabled bridge, the Win32 gate is:

```text
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_facts_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_facts_live -- --nocapture
```

The Win32 execution and strict fixture cleanup are pending CI. Local
`test-async-facts-source.py` exercises the actual snapshot with deterministic
stubs; the existing generic handoff and empty callback source checks also pass.
The repository Rust runtime stand passes 54/54 with test hooks. These local
checks do not measure COM or the live SDK. This probe
is not a production Async implementation, cancellation/timeout proof, DA1
coverage or remote-server interoperability evidence. Production Async stays
unsupported and the legacy default is unchanged.

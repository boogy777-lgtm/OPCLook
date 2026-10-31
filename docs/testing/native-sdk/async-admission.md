# Async per-item admission: write-only fixture

This test targets a different event from the accepted pair30 `ReadDenied`
fixture. In pair30 Win32 execution, the SDK admitted the read and then delivered
`E_ACCESSDENIED` in the completion callback. Here `Fixture.WriteOnly` is an
ordinary SDK server tag configured with `OPC_WRITEABLE` only, using
`SODaSTag::setAccessRights`. It is registered with the existing SDK client and
read through the existing single-slot test lease. The fixture does not implement
or inject an Async request result, worker, callback, or COM transport.

Pinned Softing SDK source establishes the expected path: server
[`SODaSComGroup::ReadMaxAge`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaS/SODaSComGroup.cpp)
creates a transaction and starts it only if it has requests;
[`SODaSGroup::fillRequestList`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaS/SODaSGroup.cpp)
checks `OPC_READABLE`, excludes an item lacking that right and supplies
`OPC_E_BADRIGHTS` with `S_FALSE`. The client
[`asyncReadImpl`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCComGroup.cpp)
copies the returned per-item HRESULT when available. These are source
expectations; the strict Win32 test is the runtime check.

`sdk_async_admission_live` requires SDK call attempted and returned, known
per-item submit error `0xC0040006`, overall submit `S_FALSE`, and no matching
callback observed at slot retirement. The decoder must preserve those facts
without claiming a server quality, timestamp or VT. The displayed Bad quality
is synthetic. A successful close and manager finish check the existing SDK
ownership cleanup. The callback flag is a snapshot at retirement, not a proof
that no late callback can ever arrive; this fixture and source path do not
establish a general server rule. Production Async remains unsupported.

After building the fixture and test-enabled bridge:

```text
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_admission_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_async_admission_live -- --nocapture
python docs/testing/native-sdk/test-async-admission-source.py --sdk <pinned-sdk-root>
```

Win32 SDK execution and strict fixture cleanup are pending CI. The local source
guard compiles the actual fixture access selector and checks the pinned SDK
source path; it is not a COM test. The case does not cover other servers,
Async Write, cancellation, remote interoperability or production lifecycle.

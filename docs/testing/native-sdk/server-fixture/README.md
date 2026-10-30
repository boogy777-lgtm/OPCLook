# N3 local OPC DA interop fixture

Class: actual local COM transport between independent SDK server/client processes;
offline callback injection is outside this class. Invariant: the pinned Softing
SODaS/SOSrv implementation owns OPC server interfaces and request processing;
the pinned SODaC implementation owns client calls. Failure to build, register,
connect, read or write fails the gate. No live test silently skips.

N+1: add a test tag through the SDK namespace/cache API, and extend the client
expectations. Do not implement OPC interfaces in the fixture. Defect guard:
the launcher bounds startup/client/cleanup, stops its own process and unregisters
its test-only COM class and generated OPC RPC proxy in `finally`.

The fixture uses the same pinned SDK revision as N1. No OTB, demo UI, private
activation component or service installation is used. The public demo allocation
helpers are compiled unchanged because the server core uses their public types.
Microsoft MIDL generates the RPC proxy from the SDK's original OPCDA/OPCCOMN
IDL files; this is test infrastructure, not a client protocol implementation.

CLSID: `{7D4BD1B1-7422-48C0-AD58-601B701449C6}`.
ProgID: `OPCLook.NativeDaFixture.1`. Item: `Fixture.Value`, writable `VT_I4`,
initial value 42, Good quality and a real server timestamp. The server is an
independent console process with a finite lease and explicit stop file.

Additional writable tags use the SDK's own `SOCmnVariant` setters and cache:
`Fixture.Bool=true`, `Fixture.Int64=-9007199254740993`,
`Fixture.UInt64=18446744073709551600`, `Fixture.Float64=12.5`,
`Fixture.Text="initial text"`, `Fixture.Unicode="РУГ 中文"`, and
`Fixture.Тег=314` (I4). The server core is built with Unicode SDK strings.
Integers deliberately exceed the exact integer range of IEEE double.

This fixture can prove local Windows COM interop. It cannot prove remote DCOM,
Kepware-specific interoperability, physical I/O or every native value type.
The production adapter must additionally be tested through its real Gateway.

Array/date fixtures use checked SDK safeArrayCreateVector and SDK-owned SOCmnVariant,
with temporary borrowed access only. Bytes: UI1 [0,127,255]; Ints: I4 [-2,0,42]
with lower bound 5; Strings: BSTR ["РУГ 中文", ""]; Mixed: VARIANT
[I4(-42), BSTR("РУГ 中文"), UI8-array([2,255])]. DateBeforeEpoch is DATE -1.25,
which denotes 1899-12-29 06:00 under OPCLook's existing UTC interpretation.
The unsafe SDK initArray helper is not used. New fixtures need exact-head Windows proof.

The fixture models whole-array replacement explicitly through SDK `setValue`;
failure is returned as an error and automatic `updateValue` is disabled for that
request. `setValue` supplies the timestamp; the initially good quality remains.
The SDK's default array `updateValue` merges by index and may report success
even when a disjoint range was not applied, so it is not a replacement oracle.
OPCLook's `Value::Array` contains elements, not SAFEARRAY lower bounds; writes
currently start at zero. Real DA servers may apply different array semantics;
an accepted HRESULT is not proof that a value was applied. This fixture tests
read of a nonzero lower bound and replacement by this particular server, not
preservation of the lower bound on write.

## Session isolation and server state controls

`--isolation` on `run-interop.py` enables `Fixture.Delayed` (I4, 42) and creates
unique per-process Windows events. Its direct SDK device-write handler signals
that the request arrived, then waits at most 60 seconds for release; a timeout
completes the request with failure. The Rust live test sends the write through
Gateway session A, waits for the server event, and checks session B read, write,
and read-back before releasing A. It then requires both disconnects, Gateway and
SDK shutdown; the launcher still checks server exit and unregisters server/proxy.
A failure guard releases the event on test assertion failure. Events are test
control only and never provide OPC data. This is a local, specific interleaving;
it is not evidence of remote-server behavior or general deadlock freedom.

Run on Windows after building the fixture and RPC proxy:

```
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll --isolation -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-preview --test sdk_session_isolation_live -- --nocapture
```

`Fixture.ServerState` is an I4 control tag with initial value 1. Writes 1 and 2
call SDK `SODaSEntry::setServerState` for Running and Failed respectively;
other values fail. Its initial value leaves the normal SDK state unchanged.
This fixture control is not a claim about other servers' response to Failed.

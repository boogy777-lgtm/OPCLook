# Native DA partial read facts (A09)

The strict fixture test uses the pinned Softing SDK, not another server or COM implementation. `Fixture.ReadDenied` completes a device read with `E_ACCESSDENIED` through the SDK request extension; `Fixture.ReadUncertain` and `Fixture.ReadBad` retain an I4 cache sample with SDK quality 0x40 and 0x00. The regular `Fixture.Value` supplies the good sample. Failure does not make an initialized quality or timestamp a server fact.

There are two distinct observations in `sdk_mixed_group_sfalse_and_gateway_per_item_facts`:

* A test-only task calls the SDK group `syncRead(2)` once, with the good and denied items. The pinned SDK server COM group source (`development/core/src/SODaS/SODaSComGroup.cpp`, `Read`/`ReadMaxAge`) returns `S_FALSE` for an item failure. Its client (`development/core/src/SODaC/SODaCComGroup.cpp`, `syncReadImpl`) copies item errors when the service succeeds and copies sample fields only for successful items. The test checks the service and item HRESULTs, sample-availability mask, good quality and timestamp, and the denied item's absence of sample facts.
* A Gateway `ReadFresh` batch verifies all four keys and their independently correlated values/failures. The adapter currently invokes SDK `syncRead(1)` for each key; this is not evidence that the Gateway batch issued a single native group call. A successful sample with uncertain or bad quality remains a sample, while the denied item is a bad operation with no invented server quality/time.

The fixture, group task and hook are test-only. They do not enable DA Async or change its lifecycle. Normal SDK worker, session and host cleanup remain in use. Reproducible Windows gate, after building the fixture and test-enabled bridge, using the same launcher and environment as the other COM tests:

```text
cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_partial_read_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-backend --target i686-pc-windows-msvc --features native-da-test-hooks --test sdk_partial_read_live -- --nocapture
```

Use `scripts/ci-cargo-live.py` in the launcher as in the other strict native tests to reject zero executed tests. The exact launcher arguments are defined by the CI job. Linux `python docs/testing/native-sdk/test-partial-read-source.py` compiles the actual test task and fixture branch against a deterministic SDK stub; it is not evidence of a Win32/COM execution. Windows execution, fixture registration and cleanup remain pending CI.

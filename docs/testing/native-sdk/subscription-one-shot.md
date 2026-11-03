# O23: SDK subscription with one-shot Sync Fresh read

This is a prepared local Windows SDK fixture gate, not a result for a real
server or a production Async mode. The test uses the actual Engine scheduling
loop, Gateway, selected SDK client, SDK server application and callback. The
SDK remains the owner of device requests, callback and COM lifecycle.

The existing `Fixture.DeviceReadCount` counts **every** device callback for
`Fixture.DeviceValue`, including SDK-server cyclic subscription refreshes.
The pinned SDK's `SODaSRequest::start` may request device data for cyclic
subscriptions according to cache age. Therefore a flat assertion on that total
would confuse legitimate server-side cyclic work with an extra client Poll.

The fixture adds two bounded SDK-observed counters for that tag:

* `Fixture.SyncReadRequestCount` increments in the SDK's public
  `SODaSCreator::createRequest` override for read requests of types
  `syncCache`, `syncDevice` and `syncMaxAge`. The override delegates object
  creation to the SDK. It sees Sync reads served from cache as well as device
  reads, while excluding SDK cyclic, async initial-notify, writes, other tags
  and the counter's own reads.
* `Fixture.SyncDeviceReadCount` increments only when those synchronous device
  or max-age requests actually reach the fixture tag's SDK `handleRequest`
  device read callback. It excludes cyclic refreshes. Existing
  `DeviceReadCount` is unchanged.

The request categories and virtual factory signature are from the pinned
[SDK request header](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/include/SODaSRequest.h),
[creator header](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/include/SODaSEntry.h)
and [group request construction](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaS/SODaSGroup.cpp).
Both counters are SDK fixture observations, not counts of Engine commands;
their cache references are released after `entry->stop` before SDK termination.

`sdk_subscription_one_shot_live` waits for an Active subscription and an
unsolicited initial 42. It runs the Engine due loop while allowing initial
work to settle, then establishes a stable counter baseline. An explicit
Engine refresh must produce one correlated Read terminal and increment both
counters exactly once. A second Gateway writes 77; the same subscription must
then deliver an unsolicited 77. During a further 1.5-second window (15
configured 100 ms intervals), the test continues dispatching any real Engine
due reads and accepting events. Both counters must stay unchanged: a hidden
cache-only or device Sync poll of the target would increment the first, while
a hidden device read would also increment the second. An observer Gateway reads
the counters; it does not read the target item. Initial-notify and cyclic
server requests are deliberately excluded from the Sync request count, and
`SubscriptionState::Failed`/callback queue failure aborts the test.

```powershell
cargo test --locked -p opclook-engine --target i686-pc-windows-msvc --features native-da-preview --test sdk_subscription_one_shot_live --no-run
python docs/testing/native-sdk/server-fixture/run-interop.py --server ../fixture-build/Release/opcl_da_server.exe --proxy ../fixture-build/Release/opcl_opcproxy.dll -- python scripts/ci-cargo-live.py -- cargo test --locked -p opclook-engine --target i686-pc-windows-msvc --features native-da-preview --test sdk_subscription_one_shot_live -- --ignored --nocapture
```

Use a hook-enabled test bridge and set `SOFTING_RUNTIME_LIB_DIR`. The launcher
requires a fresh server process and checks server/proxy cleanup. Local
`python docs/testing/native-sdk/test-subscription-counters.py` compiles the
actual fixture factory and device-count code with SDK stubs; its observations
are not Windows COM evidence. Linux compile-only checks of the gated Engine
test likewise do not execute this fixture. Actual i686 SDK execution and
cleanup must be measured in CI; remote/other-server and other DA versions
remain separate gates.

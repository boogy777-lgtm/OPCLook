# Test-only forced DA2/DA3 Async Read routes

O26 adds a separate forced-before-connect, started and advised SDK session. The
existing forced-version session remains inactive and continues to test Sync.
The new route uses the existing manager, SDK worker, one-slot lease, disabled
client initial read and transaction-correlated callback. Open checks both forced
and selected SDK versions. The live test reads a dedicated fixture item once per
version, checks successful submission and admission facts, matching callback,
value, request-kind counter deltas, session close and manager cleanup.

The fixture counts SDK-created read requests for `Fixture.AsyncRoute` only when
the exact kind is `asyncDevice` or `asyncMaxAge`. The pinned SDK uses separate
`asyncInitNotify` and `cyclic` kinds, so those do not count. DA2 expects delta
`(1,0)`, DA3 `(0,1)`; counters are sampled immediately around the explicit
read. The pinned client filters AsyncIO3 for forced DA2, then its async read
chooses AsyncIO2 Read; DA3 selects AsyncIO3 ReadMaxAge. The pinned server maps
those calls to the respective request kinds. `test-async-versions-source.py`
checks these facts against the actual pinned SDK tree and the local fixture.
`test-version-sdk-source.py` executes the actual interface filter with stubs.
Neither source check runs COM.

Local Linux runtime stand: 54/54 hook-enabled and 48/48 without hooks; source
and existing slot-facts guards pass. Windows i686 compilation and live COM
results are pending. Run the new `sdk_async_versions_live` test under
`server-fixture/run-interop.py` and `scripts/ci-cargo-live.py` for each fixture
COM input, requiring strict server and proxy cleanup. This is a local forced
client route, not a physical DA2-only server, numeric freshness or max-age
measurement, or production Async capability. Production Async remains unsupported.

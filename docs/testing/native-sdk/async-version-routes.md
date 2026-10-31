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
and existing slot-facts guards pass. Exact code
`84396da7c815f3c1d2b911120eb5eaac58011724` passed the new live test1/1 in both
i686 COM inputs (SDK0.08s, official-MIT0.07s), strict cleanup PASS. Both logs
confirmed forced=selected512/768, group connected1/started1, DA2 counters
(0,0)→(1,0) and DA3 (1,0)→(1,1), flags0xf and submit/itemS_OK. The value42
and no operation error were asserted. The source passes max_age0 with a 10000ms
budget; no numeric wire age, physical freshness or other age policy is measured.
Full-head CI acceptance is recorded separately in status/packet.
The `sdk_async_versions_live` test runs under
`server-fixture/run-interop.py` and `scripts/ci-cargo-live.py` for each fixture
COM input, requiring strict server and proxy cleanup. This is a local forced
client route, not a physical DA2-only server, numeric freshness or max-age
measurement, or production Async capability. Production Async remains unsupported.

## Source stand repair receipt

Head551e8769 failed the existing Linux fixture counter stand: its extracted
FixtureCreator used the new asyncMaxAge/counter fields absent from its SDK
stubs. This is a stand compile failure; no full-head acceptance is claimed.
The repaired stand preserves the actual creator, adds the missing stub
declarations and executes both Async counters, initial/cyclic/sync/write/other
exclusions, saturation and cache release order along with previous Sync checks.
Root reproduced after/pass; native/runtime source is unchanged. The actual COM receipt appears above; full-head acceptance is recorded separately.

Final exact84396da7 passed all9 jobs: SDK41237089913, COM41237090071,
App41237090037 and package41237090066. This receipt changes Markdown only.
Source/runtime code is unchanged from the accepted head. Old551 was not
accepted (6success/1failure/2cancelled), despite its separate native successes.

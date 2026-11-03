# Forced SDK DA2 and DA3 synchronous routes (O24/O25)

The `--live-da2-routes` and `--live-da3-routes` fixtures force a version before connection and require the SDK's selected version to match. They create an inactive group and use the ordinary SDK sync read entry point. For each read they baseline the fixture's SDK request factory counters for the one target tag. DA2 expects cache/device counts for `UINT32_MAX`/0 and a device count for finite 1234. DA3 expects the total request count to rise without either Cache or Device kind rising for all three ages: the remaining counted kind is `syncMaxAge`. Results, timestamps, and fixture value 42 are checked; no assumption is made about cache freshness or quality for these routes. Counter reads concern other tags and do not alter target counts. This is a local forced route against a newer fixture, not evidence of a physical DA2-only or DA3-only server.

Pinned source was retrieved by GitHub's file API at commit `5130e763998189c822882434c43e6be2eb0174cf`: [version constants](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/include/SODaCServer.h), [client interface selection and sync read](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCComGroup.cpp), and [server transaction creation](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaS/SODaSGroup.cpp). Forcing DA2 releases SyncIO2; the fallback maps only the cache sentinel to cache and other ages to device. With SyncIO2 the client forwards age into ReadMaxAge, and the server creates a syncMaxAge request. The counter proves the request kind, not the numeric age arriving at the server; the latter is source evidence only. A separate clean SDK checkout was verified at that exact commit with `verify-softing-sdk.py --core-clean`.

Linux `test-version-sdk-source.py` compiles and executes the pinned SDK interface-filter and read-dispatch source fragments against COM stubs, including forwarding 0, 1234, and UINT32_MAX. Linux `test-version-routes-source.py` executes the actual driver against deterministic ABI stubs, including a wrong-kind counterfactual; `test-subscription-counters.py` executes the actual fixture counter branch against stub SDK classes. Neither is real COM. Windows x86 live tests passed in both COM inputs on exact22a; run each separately:

```sh
python opclook/docs/testing/native-sdk/server-fixture/run-interop.py --server fixture-build/Release/opcl_da_server.exe --proxy fixture-build/Release/opcl_opcproxy.dll -- ./softing-build/Release/softing_core_probe.exe --live-da2-routes
python opclook/docs/testing/native-sdk/server-fixture/run-interop.py --server fixture-build/Release/opcl_da_server.exe --proxy fixture-build/Release/opcl_opcproxy.dll -- ./softing-build/Release/softing_core_probe.exe --live-da3-routes
```

The legacy production version selection and DA1 test ABI are retained; production Async remains unsupported. No new Softing SDK patch is introduced.


## Measured Windows receipt

Exact `22a134866bdad3dfdc487156cb9613ba74c2238f` passed all9 CI jobs.
[SDK-input job124547050998](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41204614566/job/124547050998)
and [official-MIT job124547051147](https://github.com/boogy777-lgtm/OPCLook/actions/runs/41204614566/job/124547051147)
both passed the DA2 and DA3 invocations and their strict server/proxy cleanup.
Both showed forced=selected512/768 and connected1/started0. Each target sample
was42, I4, with nonzero timestamp; service/item success was checked.

| Forced route | Client age argument | Observed total/cache/device delta | Observed quality |
| --- | --- | --- | --- |
| DA2 | UINT32_MAX | 1/1/0 | 0x001c |
| DA2 | 0 or1234 | 1/0/1 | 0x00c0 |
| DA3 | UINT32_MAX,0 or1234 | 1/0/0 | 0x00c0 |

Quality is a fixture observation, not a guarantee for every server. Counter
classification and the separate pinned-source forwarding test do not measure
the numeric age arriving at the server or prove real cache age/freshness.

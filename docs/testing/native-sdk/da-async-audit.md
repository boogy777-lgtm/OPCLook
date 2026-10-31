# Native DA async source and cancellation audit

This is a source-level feasibility and controlled test of the pinned Softing
SDK `5130e763998189c822882434c43e6be2eb0174cf`, not proof of a Windows server,
COM interoperability, timeout behavior, production readiness or legal release.
DA Sync remains the default; an async facade over synchronous calls is not used.

## Source evidence

- [`SODaCGroup.h`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/include/SODaCGroup.h#L380) exposes `asyncRead`, `asyncWrite`, `asyncCancel` and virtual `onReadAdvise`/`onWriteAdvise`. A successful submit and per-item initial HRESULTs are not final completions. The SDK requires an established read/write advise connection before async submission.
- [`SODaCCallback.cpp`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCCallback.cpp#L818) forwards `OnDataChange` with mode 1 and `OnReadComplete` with mode 0 to the same group callback, with transaction ID and raw master/item results. `OnWriteComplete` supplies its own transaction and per-item errors. Callback arrays are temporary; copy any data needed after return. `OnCancelComplete` forwards to `SODaCComGroupImpl::onCancelAdvise`.
- [`asyncReadImpl`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCComGroup.cpp#L2841) passes `maxAge` to DA3 `ReadMaxAge`; the DA2 `Read` signature has no source/maxAge argument. The SDK's own [DA2 server implementation](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaS/SODaSComGroup.cpp) creates this read as `OPC_DS_DEVICE`. That observation is limited to this implementation; `CacheAllowed` permits a fresher value and does not promise a cache hit. The [official MIT-labelled OPCF IDL](https://github.com/OPCF-Members/OPC-Classic-CoreComponents/blob/efe0d1d1ea86a8a727bf26a501a261765e836766/Source/DataAccess/ProxyStub/opcda.idl) separately documents the DA2 and DA3 interface signatures.
- DA1 async read supplies source but the SDK replaces the client transaction ID with the server-supplied cancel ID **after** the call. It cannot safely use the proposed pre-registered DA2/3 correlation unchanged; DA1 must fail closed until its ID and early-completion behavior are designed and measured.
- The SDK read callback compacts surviving item/value/quality arrays after an unrecognized client handle, but forwards the original error array. Per-item async requests avoid claiming correct error alignment for a multi-item callback with a removed registration. Registration retirement and close/late callbacks still need real-server proof.

## Cancellation defect and bounded patch

[`asyncCancelImpl`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCComGroup.cpp#L3600) first asks for AsyncIO3 and only asks for AsyncIO2 if that fails. In the advised branch it then unconditionally dereferences `asyncIO2` for `Cancel2`. When AsyncIO3 succeeds, the local AsyncIO2 smart pointer was not populated. The candidate patch chooses AsyncIO3's inherited `Cancel2` in that branch, preserving AsyncIO2 and DA1 fallbacks. It is not yet part of the normal SDK patch/profile or package.

`test-async-cancel-source.py` verifies the complete original source's Git blob
`b6add51b268f0dc7a77a4873aa300ebed8c62d9e`, extracts the exact function body,
applies the zero-context `softing-async-cancel.patch` with `git apply
--unidiff-zero` to a disposable copy after exact blob verification, verifies that only
the intended call changed, and compiles both function bodies against fake
interface access. In the controlled AsyncIO3-success branch the original hits
the fake null-pointer trap (42); the patched version returns success and calls
the AsyncIO3 object. The DA2 fallback is checked in both cases and the patched
DA1 fallback is checked as well. This does not reproduce an actual CComPtr
crash or prove Windows COM behavior.

```sh
python docs/testing/native-sdk/test-async-cancel-source.py --source /path/to/clean-sdk/development/core/src/SODaC/SODaCComGroup.cpp
```

Local measurement: exact upstream blob fetched at the pin, Linux C++ controlled
harness original 42 / patched 0. Actual Win32 SDK build, real cancellation,
late callback and teardown remain open. Before using the patch, integrate it
into exact-blob patch application, license/profile guard and preview evidence;
then test real fixture and cleanup in both official-input and legacy controls.

## Proposed bounded interface and remaining gates

A private prototype may expose a nonzero transaction ID, one registration per
request, immediate service/item submit results, and a separate bounded copied
completion queue; stream change facts remain separate. Register pending work
before submit to tolerate callbacks before return. Do not reuse a transaction
within a group lifetime; reject exhaustion, overflow and unmatched/retired
callbacks explicitly. Associate pending work with current session epoch and
request ID; every accepted request must obtain exactly one terminal result,
including disconnect, error and timeout. An SDK worker must never wait on its
own callback; any waiting belongs to the session owner with bounded lifetime.
Do not classify submit success as confirmed Write and do not add retries.

Polling sessions currently use `drPolledRefresh` and only subscription sessions
create the bounded push queue. Async read therefore needs an advise-capable
path independent of `UpdateMode`, a completion queue, and deterministic real
fixture coverage. The current `AcquisitionConfig` has no DA method selector;
any new Sync/Async selector needs backwards-compatible default Sync, validation,
UI and caller integration. Until this is implemented and measured, do not
advertise Async capability. Verify early/late callback, immediate failure,
multiple sessions, queue overflow, timeout/quarantine, release and final SDK
cleanup with the actual selected SDK. Remote servers and DA1/DA2/DA3 negotiation
remain separate gates.


## Private SDK callback probe (not an acquisition mode)

The test bridge has an `OPCL_DA_TESTING`-only, single-pending I4 probe for
real SDK `asyncRead`; its caller waits on a condition variable, while the
SDK server worker only submits the request and the SDK callback publishes the
result. It registers a nonzero transaction before submission, checks both
transaction and SDK item handle, copies value/quality/time before return,
ignores unmatched, duplicate and late completions, bounds the callback wait
after the SDK submission returns, and
refuses DA1 callback mode. It is not a Gateway request or product selector,
does not prove Async Write, and does not call SDK cancellation.

`SODaCComGroup.cpp` can issue its own initial async read during item startup,
using SDK-generated transaction IDs; `SOCltElement` defaults the corresponding
`performStateTransitionAction` to true. The probe opts out through that SDK
virtual extension **before** its own item registration, without changing other
group transitions. This is required for its simple token correlation; it is
not yet a design for arbitrary production subscription lifecycles. A synthetic
callback-state probe exercises stale, duplicate and retired transaction IDs.
The production no-hook bridge excludes all of these functions.

With a test bridge (`OPCL_BUILD_PROBES=ON`) and the actual fixture, the future
Windows CI command is:

```sh
python docs/testing/native-sdk/server-fixture/run-interop.py --server <fixture-build>/Release/opcl_da_server.exe --proxy <fixture-build>/Release/opcl_opcproxy.dll -- <core-build>/Release/softing_core_probe.exe --live-async
```

SDK `executeTask` submission can itself block; the callback timeout does not
bound that call or establish cleanup while submission is stuck. The probe and
C++ callback-state check are **not yet compiled or run on
Windows**. They are preparatory evidence only. Per-request terminal behavior
through Gateway, pending writes, timeout during blocked SDK submit, two-session
isolation, remote negotiation and final cleanup still require real tests before
async can be advertised.

## Bounded submission: pinned SDK source gate

The public [`SOCltElement::executeTask`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SOClt/SOCltElement.cpp#L353) has `SOCLT_EXEC_ASYNC`. When the caller is outside an existing SDK worker it calls that worker's `addTask` and returns `S_OK` for a successful enqueue, not the task's I/O result. However, when there is no worker or the caller is on the worker itself, the same method directly calls `task->execute()` before returning, irrespective of that flag. The worker's [`addTask`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SOClt/SOCltThread.cpp#L98) adds to a synchronized SDK list; the exposed call has no deadline or try-lock contract. Thus checking `SOCLT_EXEC_ASYNC` alone does not establish a bounded submission. This is a source-level API limitation, not a measured queue-lock hang.

[`SOCltTask` retains the element](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SOClt/SOCltTask.cpp#L16), and the worker normally takes a task reference and calls `reportCompletion` after execution. But [`shutdown`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SOClt/SOCltThread.cpp#L38) removes the task queue before stopping the worker, without reporting completion for those removed tasks. A separate `cleanUp` reports `E_ABORT` for tasks still present, but cannot report tasks already discarded by `shutdown`. The group reference is not proof that the host wrapper or SDK entry remains alive, nor that each accepted task has a terminal outcome.

`test-async-submit-source.py` verifies the exact Git blobs of these three SDK files and compiles the **unaltered** `executeTask` body against explicitly synthetic SDK stubs. A deterministic event barrier shows the external-worker async branch returns after enqueue without executing the task, while both own-worker and absent-worker branches remain in the task until the barrier is released. It also source-checks the shutdown ordering and task element reference. Measured on Linux with C++17: all three branches passed. The test does not execute the real SDK queue, Windows COM, or an OPC server, and does not test the latency of a real lock.

```sh
python docs/testing/native-sdk/test-async-submit-source.py \
  --element-source <pinned-sdk>/development/core/src/SOClt/SOCltElement.cpp \
  --thread-source <pinned-sdk>/development/core/src/SOClt/SOCltThread.cpp \
  --task-source <pinned-sdk>/development/core/src/SOClt/SOCltTask.cpp
```

A bounded prototype still needs a library-owned admission primitive that rejects missing/own worker and can fail within a defined deadline while synchronizing against shutdown, plus a single completion-or-abort notification for each accepted task, including shutdown queue retirement. A candidate SDK patch should expose a checked, bounded/try-enqueue operation on the existing worker and drain accepted tasks with `E_ABORT` exactly once before releasing them, notifying outside the queue lock; it must not execute a user task inline. The wrapper must separately retain the host/entry, group, item registration and heap result state until the SDK task and any callback are retired, enforce one pending request and non-reused transaction IDs per group, and ignore late callbacks after timeout. Such a patch needs SDK-owner review, pinned-source/closure update, a controlled source regression and real i686 SDK fixture barriers for queued, executing, timed-out and shutdown paths before enabling production Async. No implementation or runtime bound is claimed by this source gate.

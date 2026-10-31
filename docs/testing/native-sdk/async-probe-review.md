# N3 async read: SDK prototype review

Scope: test-only `OPCL_DA_TESTING` C ABI and core probe on SDK commit
[`5130e763998189c822882434c43e6be2eb0174cf`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/tree/5130e763998189c822882434c43e6be2eb0174cf).
No production async capability is enabled. The SDK owns COM, subscription
advise and the worker; this prototype introduces neither a COM client nor a
second transport, retry loop or cancellation implementation.

## Source review

- `SODaCGroup.h:228–238` exposes `asyncRead(count, items, maxAge,
  transactionID, cancelID, pErrors)`. `SODaCComGroup.cpp:2841–3065` selects
  AsyncIO3 or AsyncIO2 with the data callback, otherwise legacy AsyncIO whose
  transaction identity is server-assigned. The prototype requires the SDK
  data callback and uses a nonzero, nonreused transaction ID, one item and
  the SDK worker. AsyncIO2 does not implement arbitrary maxAge; the SDK's
  fallback must not be represented as equivalent to AsyncIO3 ReadMaxAge.
- `SODaCCallback.cpp:877–915` passes mode 0 for ReadComplete; DataChange uses
  mode 1. The slot accepts only the pending transaction and SDK client handle,
  ignores late/duplicate callbacks, and copies I4/quality/FILETIME rather than
  retaining SDK-owned callback buffers. SDK handle-compaction/error alignment
  uses the separately pinned patch; the single-item probe does not establish
  correctness for a multi-item request.
- `SODaCComGroup.cpp:1063–1074` checks virtual
  `performStateTransitionAction(SODAC_ITEM_ACTION_INITIAL_READ)` before the
  SDK initial read. The probe disables this via the SDK extension point before
  Add; this avoids collision with SDK-generated transaction IDs. It does not
  disable server-originated DataChange or establish stream continuity.
- `SOCltElement.cpp:353–405` executes the submit task synchronously by default;
  the SDK worker signals completion in `SOCltThread.cpp:333–335`. Therefore
  `timeout_ms` bounds **only the subsequent callback wait**, not submission,
  COM call duration, or total caller/shutdown time. A timed-out accepted read
  is not cancelled or proved complete; the slot is retired and late callback
  ignored. Session close and SDK cleanup remain separate lifecycle obligations.
- Callback master and per-item HRESULTs now survive negative completion; a
  successful callback with a non-I4 variant returns `DISP_E_TYPEMISMATCH`.
  Submit HRESULT and immediate per-item HRESULT are also copied to the output.
  The existing DTO has no field for callback master-quality HRESULT; per-item
  quality and source FILETIME are preserved. Neither timestamp nor GOOD quality
  proves physical freshness. Synthetic SDK callback regression covers distinct
  item/master errors, wrong type, stale token and duplicate callback.

## Evidence and next integration boundary

A local C++17 stand extracts the actual mode-0 callback branch from
`runtime.cpp` and substitutes small SDK/Win32 types. The pre-fix branch fails
the per-item `E_INVALIDARG` assertion; the fixed branch passes stale token,
success and quality/time copy, duplicate, item error, master error and wrong
type checks. This is a stubbed branch test, not an SDK integration test.

This source review and local stand do **not** compile or execute the
Win32 C++ probe. The existing synthetic callback test and live fixture remain
pending Windows CI on the integrated head; a successful fixture would prove
only the sampled server/read path, not timeout, isolation or cancellation.

Before an adapter offers Async Read, use adapter-assigned transaction IDs and SDK/server cancel IDs with
per-session/epoch and per-registration identity, preserve submit versus final
per-item/service outcomes and all value/quality/time facts, bound outstanding
requests, handle retirement and late callbacks exactly once, and reconcile
session removal/disconnect with accepted operations and SDK cleanup. Define
and test the actual timeout/cancel and shutdown bounds against delayed device
I/O without claiming cancellation before SDK evidence. Exercise real I4 and
other supported shapes, immediate and callback failures, multiple concurrent
sessions, late completion and resource cleanup on Win32. Keep the production
capability unavailable until those gates pass; no history completeness follows
from one callback.

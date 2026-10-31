# U2: late-observed completion is still a timeout

Exact6564157 Windows App failed one existing backend test:
`timeout_drains_same_future_and_discards_late_result` observed `Ok(42)` instead
of `Err(RequestTimeout)`. Other314 backend tests passed,1 was ignored and1
platform-isolated test was filtered. Both COM jobs and all other jobs eventually
passed:656 is8success/1failure, not an accepted full head.

Pinned Tokio1.54.1 timeout polls the request before its timer. A task first
repolled when both are ready can therefore receive a value after the budget.
The available CI log does not establish why that scheduling occurred; Windows
timer granularity or delayed scheduling is only a plausible explanation.
The application helper already promised to discard late completions and close
that session, so relying on value-first polling did not enforce its contract.

A controlled actual-helper regression first polls one request pending, leaves
the operation unpolled until after the deadline, then marks the same request
ready. The original helper returns Ready(Ok42), reproducing the mismatch.
The repair records a monotonic start and accepts a ready result only before
the budget. A completed late value is discarded without polling that completed
future again. When the timer is observed while the request remains pending,
the existing bounded grace still polls that same pinned request. After grace
expiry it is dropped and caller quarantine/close remains necessary. The helper
does not preempt a blocking poll or prove the physical remote completion time;
it classifies when the caller can first observe the result.

The regression also checks the pending-then-drained path, early success,
Duration::ZERO timeout and Duration::MAX success. Both controlled request
futures assert against polling after Ready. This does not merely extend a sleep
or turn the timeout assertion into success. The owner reproduced the original
failure; the corrected focused test passed1/1. Parent integrated full Linux
backend repeated:268passed,0failed,1knownignored in77.54s (parallel), including
O18/O19 and existing lifecycle regressions. Windows repaired-head acceptance
is pending. No SDK dependency, transport or recovery owner changed.

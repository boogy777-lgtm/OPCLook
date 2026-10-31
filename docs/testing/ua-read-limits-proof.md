# O18/O19: bounded UA Read operation limits

This changes the production adapter's existing Read chunk loop, using locked
async-opcua0.19 public `Session::read`. The SDK constructs one service request;
it has no general Read OperationLimits batching API. SDK custom-type loading
has its own internal calls and is outside this change. No transport, scheduler,
Publish or recovery owner is added and the SDK dependency is unchanged.

The adapter lazily reads standard MaxNodesPerRead through one SDK Read when a
ready identity revision is available. A positive UInt32 bounds the local chunk;
zero, absent, unreadable or unsupported metadata uses the existing nonzero local
bound. Learned limits are per revision and separate from monitoring limits.
Only a whole-service BadTooManyOperations rejection halves the actual rejected
slice and retries it; a singleton rejection terminates with the error. DataValue
per-item statuses are returned in order and never cause service retries.
The caller's max_age and timestamp mode are forwarded unchanged. Scoped calls
check revision before/after every awaited operation. Wall-clock or SDK BadTimeout
marks the session timed out; that call returns without sending a user Read after
the timeout. A short or excess response ends concatenation so later chunk values
cannot shift onto missing keys. Existing unscoped metadata calls without a ready
identity retain their former fallback; this is not a new identity-quarantine proof.

## Measured regression

The original fixed-size loop failed the new normal Gateway test: a sample server
advertising/enforcing max_nodes_per_read=3 returned BadTooManyOperations for a
five-key Read. The fixed `o18_read_respects_server_operation_limit` passed.
`o19_read_retries_only_whole_service_size_rejection` first observed the advertised
3, then deliberately seeded cached size5 while keeping the ordinary server
unchanged. It observed learned size2 and preserved a BadNodeIdUnknown result in
position between successful results. This is a stale-cache test, not a wire test
of omitted OperationLimits metadata. Zero/absent/bad/positive bounds have a unit
test. The owner measured both live tests passing together in15.96s.

The parent reran the integrated backend suite on Linux:268 passed,0 failed,
1 previously known ignored SDK first-notification diagnostic,394.32s with serial
test execution. Native SDK source/mock stand54/54 and focused native source guards
also passed. These checks do not establish Windows behavior, remote servers,
wire numeric max_age, malformed result cardinality, or dedicated cap-query
timeout/reconnect races. Existing broader tests remain regression coverage,
not proof of every transition in the new metadata lookup.

```text
cargo test --locked -p opclook-backend --target x86_64-unknown-linux-gnu --lib o1 -- --test-threads=1
cargo test --locked -p opclook-backend --target x86_64-unknown-linux-gnu --lib read_limit_absent_zero_bad_and_positive_are_bounded
```

The final SDK readiness/drain Engine research runner remains tied to historical
application source receipts. Commit9ba4db1cfab32b1c6648c9392ef9b4e07af63997 matches
all its WORK_BLOBS; this Read-loop change does not. Use a separate clean linked
worktree from that commit to reproduce its historical proof. Do not merely
rewrite its hashes to accept the new adapter: an actual port and fresh acceptance
are needed before combining the research handoff with this production change.

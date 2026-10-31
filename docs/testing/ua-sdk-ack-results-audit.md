# R07: experimental SDK Publish acknowledgement results

This is an isolated SDK research overlay, not a production dependency or App
change. It applies after v2 → sticky uncertainty → readiness → racefix → drain
to `async-opcua-client` 0.19.0. The published package checksum was verified
against `Cargo.lock`, its VCS receipt is
`9ad28fc011002398f2e8a95696a50408a14531d9`. The stand now authenticates
the complete published archive against the repository
`Cargo.lock` SHA-256 and checks all 40 published files against the source and
fresh copy. It rejects unexpected files other than Cargo's root metadata and
rejects symlinks. It also retains the reviewed drain runner's six blob checks.

## Contract and evidence

- **Class:** ordinary Publish response per-ACK result; the SDK subscription
  service remains the sole owner.
- **Invariant:** distinguish a positional exact Good result from another
  status or a missing result. Notification/callback delivery and ACK processing
  are independent facts; a good value does not imply ACK acceptance. A bounded
  observation cannot claim complete event history.
- **N+1:** mixed statuses; missing, short and excess arrays; zero ACKs;
  `GoodRetransmissionQueueNotSupported`; duplicate ACKs after cancellation;
  subscription deletion/reuse; repeated responses and slow observers.
- **Gap:** the original `publish` zips ACKs with returned results, logs certain
  failures, and explicitly suppresses the warning for
  `BadSequenceNumberUnknown`. Missing/short entries have no typed outcome. The
  drain overlay protects pending local ACK ownership on cancellation but commits
  the batch on any successful service response. That commit ends local guard
  ownership; it does not mean the server accepted the ACKs. Neither mechanism
  proves per-ACK success.

[OPC UA Part 4 §5.14.5](https://reference.opcfoundation.org/specs/OPC-10000-4/5.14.5)
requires result size and order to match the request. It defines
`Bad_SequenceNumberUnknown` and `Good_RetransmissionQueueNotSupported`; the
latter means the server does not support the retransmission queue and
acknowledgement is unavailable. A bad ACK status alone does not invalidate a
notification in the response.

## Candidate behavior

The [client patch](ua-sdk-ack-results-client.patch) processes a successful
response in the existing SDK subscription service. It retains only the latest
batch summary: number sent, exact Good statuses, other matched statuses,
missing and excess counts, and the first non-exact/missing ACK with its raw
status or `None`. The Session getter returns that summary plus a sticky
uncertainty flag for the lifetime of this SDK Session object. Other Good
subcodes, including unsupported retransmission, are non-exact; a raw status is
available for the first such issue. This is deliberately lossy, bounded
observation, not a complete per-ACK record. A later summary can overwrite the
first issue, but it cannot erase the sticky flag. IDs are historical request
facts, not authority over a current subscription incarnation. Concurrent
responses are summarized in SDK processing order, not necessarily send order.
The ACK-specific sticky flag is separate from `republish_history_uncertain`
and is not connected to App state. The getter locks the SDK subscription state;
query it outside SDK notification callbacks (which hold that mutex) and outside
the backend identity lock to avoid reentrant or lock-order deadlock.

The candidate preserves the existing logging policy, response notification
callback and `more_notifications` result. It does not retry failed or missing
ACK statuses: the server might have processed the request, and retries can be
ambiguous. Existing cancellation requeue remains incarnation scoped. An exact
Good result is an observed per-position status; neither it nor drain completion
proves complete notification history. No application or transport owner is
added.

## Verification and limits

On 2026-10-31 the fresh archive-, full-source-, receipt- and blob-verified
offline stand measured the
no-recording counterfactual failing at the expected assertion, fixed ACK-result
tests 3/3, existing ACK guard tests 2/2 and recovery tests 16/16 passing.

The deterministic test calls the actual extracted SDK response-processing path
with a real `PendingPublishAcks` and `SubscriptionState`. The no-recording
counterfactual fails at the expected missing-summary assertion. The fixed tests
exercise exact Good, `BadSequenceNumberUnknown`, bad subscription ID, mixed,
missing/short/excess/zero responses, an Uncertain status and the special Good
unsupported code;
they observe the callback sequence and synthetic opaque payload,
`more_notifications`, and the new notification's ACK. The synthetic payload is
not a decoded data-change value. A subsequent Good result leaves sticky uncertainty set. Existing ACK
cancellation/incarnation and recovery unit filters are rerun by the stand. This
is a controlled in-process response boundary, not a wire decoder, live server,
network fault injection, Windows, multi-server, or GUI test. The summary's
first-issue bound and Session-lifetime stickiness require review before any
production adoption. Namespace cache and the broader recovery gates remain open.

Run with a clean published client source and a populated Cargo cache (omit
`--offline` if dependencies must first be fetched):

```text
python3 scripts/ua-sdk-ack-results-stand.py \
  --client-archive /path/to/async-opcua-client-0.19.0.crate \
  --client-source /path/to/async-opcua-client-0.19.0 \
  --client-dest /path/to/new-client-copy \
  --target-dir /path/to/separate-target \
  --log /path/to/new-log --offline
```

The runner verifies archive checksum, complete source contents, VCS receipt and
existing blob receipts, creates and re-verifies a fresh copy, applies the ordered
patches, seeds the repository lockfile, checks that disabling latest-summary
recording fails at the expected assertion, restores it, then runs the fixed,
ACK guard and recovery filters. Cargo adjusts the lock root for the standalone
published client package; this is not a production lockfile update. All resolved
archive, source, destination, target and log paths must be disjoint, and the
destination and log must not already exist. The fast offline guard test is:

```text
python3 scripts/ua-sdk-ack-results-guards.py \
  --client-archive /path/to/async-opcua-client-0.19.0.crate \
  --client-source /path/to/async-opcua-client-0.19.0
```

This guard test accepted the authenticated 40-file package and rejected a
modified subscription module, unexpected file, symlink, corrupt archive,
equal/nested and symlink-overlapping paths, and an existing log.

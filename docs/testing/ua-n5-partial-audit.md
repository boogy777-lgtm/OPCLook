# N5: UA SDK ownership and O/OF audit — CLOSED (audit scope)

Scope: working tree based on `b4bc724`, locked crates.io `async-opcua-client`
0.19.0, registry source `index.crates.io-1949cf8c6b5b557f`. This is a
navigation, crosswalk and bounded source review, not acceptance of all R00/R04–R12,
O-tables or OF01–OF12. Historical live evidence linked below is not a new-head
test. Source paths in the SDK column are relative to that package's `src`.

**2026-11-05 continuation:** `tests/ua_subscription.rs::ua_subscription_reports_codesys_negotiated_parameters` passed 1/1 against CODESYS `127.0.0.1:4840` (1.18 s).
Direct Gateway metadata: requested/revised publish 1000/1000 ms, lifetime 60/60, keepalive 20/20, sampling -1/1000 ms, queue 1/1, valid=true; not inferred from a push rate.
This narrows CODESYS O01 evidence only, not changed counts or the sample 1000→2000 grace case. [#12 continuation](issue-12-update.md) records scope, write/LDS blockers and the published issue update.

**Closure decision (parent):** N5 is closed for its SDK-ownership/duplicate
scope. The locked SDK was crosswalked against O01–O28/OF01–OF12, no second
Publish/ACK/Republish/Transfer/RegisterNodes owner was found, the dead
`report_negotiated` wrapper was removed, and requested/revised lifetime/keepalive
are now observable in CLI/UI with unknown and invalid kept distinct. The
`gap` rows below are explicit residual protocol/recovery items for R2.6 and the
recovery ledger; they are not evidence of a duplicated SDK implementation.

| Ability | SDK API/source → adapter | Test or evidence; remaining scope |
| --- | --- | --- |
| SecureChannel, Session, reconnect | `transport/channel.rs`, `session/connect.rs`, `builder.rs::session_retry_limit` → `ua/session/connection.rs` uses retry limit 3 and observes SDK events | [recovery audit](ua-sdk-0.19-recovery-audit.md), [acceptance ledger](ua-sdk-acceptance-ledger.md); different secure servers and loss interleavings open |
| Publish, ACK, notifications | `session/services/subscriptions/service.rs` owns Publish; `trigger_publish_now` requests SDK action → `ua/session/connection.rs`, `session/notification.rs` | [notification facts](ua-notification-facts.md), [drain scope](ua-sdk-drain-scope-audit.md); trigger is not a second Publish owner; ACK status gaps remain |
| Monitored items and initial result | `service.rs::create_subscription`, `create_monitored_items` → `session/subscription.rs` | `ua/live_tests.rs` and [acquisition truth tables](acquisition-truth-tables.md); full O-table acceptance not re-established here |
| Queue, filter, deadband | SDK monitored-item service parameters → `session/mod.rs::monitoring_parameters`, `values.rs::data_change_filter`, `subscription.rs` | `ua/tests.rs::deadband_filter_builds_absolute`, `ua/live_tests.rs::u4_deadband_change_is_applied_to_a_live_subscription`; adapter's unsupported-filter retry is policy, not proof of duplicate filtering |
| Limits | SDK Read `session/services/attributes.rs::read` → `session/core.rs::read_values_chunked_scoped`; subscription splitting in `subscription.rs` | [Read limits](ua-read-limits-proof.md); not every limit or malformed response newly exercised |
| Aliases | SDK `session/services/view.rs::register_nodes` → current adapter does not use RegisterNodes | O27 in [acceptance ledger](ua-sdk-acceptance-ledger.md); identity/key maps are not RegisterNodes aliases |
| Discovery/LDS | SDK `session/client.rs::{get_endpoints,find_servers,find_servers_on_network}` → `ua/discovery.rs`, `scanner.rs` | `tests/ua_search.rs`; LDS and all endpoint variants not newly accepted |
| CacheAllowed/maxAge | SDK `attributes.rs::read` passes `max_age` into Read builder → `worker.rs::ReadCached`, `io.rs::read`, `session/core.rs` | `ua_live_read_sends_requested_max_age_and_direct_read_sends_zero` checks SDK sample-server wire request; maxAge is a request, not a measured value age |
| Requested/revised | SDK subscription/item state getters → `session/subscription.rs::report_negotiated_at`, `UaSubscriptionParameters` | `ua_live_reports_requested_and_server_revised_subscription_parameters`, `tests/ua_metadata_panel.rs`; queue/lifetime/keepalive revision differences and consumer paths still need independent end-to-end audit |
| Republish, Transfer, ResendData | SDK `service.rs::{republish,transfer_subscriptions}` and internal recovery → adapter reconciliation; see linked audits for exact limitations | [Republish proof](ua-sdk-republish-proof.md), [transfer proof](ua-sdk-transfer-proof.md), [namespace cache audit](ua-sdk-namespace-cache-audit.md). Stock automatic Republish not established; ResendData coverage not verified in this slice |
| Write/readback | SDK `attributes.rs::write`, `read` → `ua/io.rs`; readback is separate Engine logical demand | `tests/ua_write.rs`; outcome/readback truth-table coverage requires separate complete review |

The negotiated snapshot in `crates/contract/src/model/event.rs` has separate
requested/revised subscription and item fields, but no effective maxAge field.
UA Read's maxAge is a client request, not a server-revised setting; the SDK
Read response shown in `attributes.rs::read` returns DataValues, not a revised
maxAge or evidence of actual cache age. Whether and where each dispatched
Read's requested maxAge is externally observable remains to be audited; it must
not be represented as a server-attested actual age.
OF02's explicit trigger/effective-filter visibility remains unestablished.

## N5 continuation: O and OF crosswalk (2026-11-05)

`covered` below means the stated narrow evidence, not general conformance;
`gap` means no sufficient evidence or feature, not necessarily a defect.
`historical` receipts are on the head named in their linked documents, not this
working tree. SDK paths are in client 0.19. DA-only rows are outside UA N5.

| ID | SDK owner/API; OPCLook adapter/consumer; evidence | Status |
| --- | --- | --- |
| O01 | `subscriptions/service.rs::create_subscription`; `subscription.rs::report_negotiated_at`, `engine/lib.rs::revised_interval_for`; `ua_live_reports_requested_and_server_revised_subscription_parameters`, `revised_subscription_grace_is_per_item_and_cached_batch_uses_minimum` | covered, local sample server |
| O02 | SDK item/subscription state; `subscription.rs::report_negotiated_at`; same live test checks requested/revised sampling/queue but not changed counts | gap: independently revised queue/keepalive/lifetime wire cases |
| O03 | server DataChangeFilter; `values.rs::data_change_filter`, `session/mod.rs::monitoring_parameters`; `ua/tests.rs::parsed_absolute_deadband_matches` | gap: ServerTimestamp-only live case |
| O04 | server filter; adapter `values.rs`; no selectable timestamp trigger evidenced | gap: trigger unsupported/unchecked |
| O05 | server filter; adapter `values.rs`; live `u4_deadband_change_is_applied_to_a_live_subscription` | gap: timestamp+deadband/quality live semantics |
| O06 | SDK monitored items and server queue; `session/mod.rs::monitoring_parameters`; `subscription_monitoring_parameters_shape` | gap: multi-sample queue=1 wire proof |
| O07 | SDK callback/status; `session/notification.rs`; `sdk_notification_overflow_is_separate_from_good_latest_value` | covered narrow synthetic overflow, queue>1 wire gap |
| O08 | SDK subscription state; contract `UaRevisedSubscription::from_sdk`, adapter report; `sdk_revised_lifetime_ratio_checked_without_overflow` | covered unit; invalid server revision live gap |
| O09 | SDK Publish keepalive; adapter `notification.rs`; [notification facts](ua-notification-facts.md) | gap: explicit liveness-versus-tag test not established |
| O10 | SDK `session/connect.rs` and event loop; adapter `session/connection.rs`, `recovery.rs`; [recovery ledger](ua-sdk-acceptance-ledger.md) | covered selected historical live reactivation, not all secure variants |
| O11 | SDK `subscriptions/service.rs::republish`; adapter recovery gate; [republish proof](ua-sdk-republish-proof.md) | gap: stock automatic replay not established |
| O12 | SDK recovery; `session/recovery.rs`, Engine recovery snapshot; [ledger](ua-sdk-acceptance-ledger.md) | gap: production bounded replay outcomes/ResendData not established; history remains unknown |
| O13 | SDK transfer/recreation; `session/connection.rs`, `subscription.rs`; `sdk_recovery_observer_reports_new_session_after_server_restart` | covered selected sample scenario; mapped transfer/adoption gaps in ledger |
| O14 | SDK `attributes.rs::write`; adapter `io.rs` write outcomes; `adapter_keeps_good_async_uncertain_bad_and_missing` | covered controlled outcome mapping; not physical device confirmation |
| O15 | same SDK/write adapter and controlled test as O14 | covered controlled mapping, raw uncertainty retained |
| O16 | SDK Write; adapter `io.rs`, Engine terminal handling; `write_timeout_is_reported_as_unknown_and_not_retried` | covered unit; remote execution remains unknown |
| O17 | SDK Write; `io.rs`; no ordering claim for duplicate target | gap: explicit repeated-node ordering boundary test not established |
| O18 | SDK `attributes.rs::read`; adapter `session/core.rs` bounded chunk; `o18_read_respects_server_operation_limit`, [receipt](ua-read-limits-proof.md) | covered local sample, historical Windows receipt |
| O19 | SDK Read; adapter `session/core.rs`; `o19_read_retries_only_whole_service_size_rejection`, [receipt](ua-read-limits-proof.md) | covered local sample; other failure paths in receipt |
| O20 | SDK `view.rs::register_nodes`; UA adapter does not call it; O27 ledger | out of scope: alias optimization not used; no alias transfer proof |
| O21 | same SDK and adapter as O20; no RegisterNodes validity inference | out of scope: optimization not used |
| O22 | SDK `create_monitored_items` and callback; `subscription.rs`, `notification.rs`; `create_results_are_paired_with_their_items_and_bad_statuses_are_failures` | gap: exact successful registration then Bad_NotReadable initial sample live test |
| O23 | DA SDK/router and Engine capability, not UA; [native ledger](native-sdk/acceptance-ledger.md) | out of scope UA N5; see DA gates |
| O24 | DA Cache/active policy, not UA; native/legacy DA adapters; Foundation OF11 | out of scope UA N5; DA version evidence separate |
| O25 | DA Device/active policy, not UA; native/legacy DA adapters; Foundation OF11 | out of scope UA N5; DA version evidence separate |
| O26 | DA callback/transaction routing; native SDK facade and DA adapter; [native ledger](native-sdk/acceptance-ledger.md) | out of scope UA N5; production Async gate separate |
| O27 | SDK subscription lifecycle; `session/notification.rs`, `session/recovery.rs`; `old_notification_cannot_be_relabelled_after_recovery_before_delegate`, [ledger](ua-sdk-acceptance-ledger.md) | covered narrow unit/live registration identity; alias not used, race coverage incomplete |
| O28 | SDK event loop/recovery and DA facade; `session/connection.rs` observer, Engine scheduler; [recovery ledger](ua-sdk-acceptance-ledger.md) | covered ownership source and selected live, not full recovery correctness |

| Finding | Owner, adapter, evidence and status |
| --- | --- |
| OF01 | SDK subscription/item state → adapter `report_negotiated_at`, contract/Engine/UI/CLI; O01 covered narrow, O02 gap. |
| OF02 | Server filter → `values.rs`, subscription requests; O03–O05 explicit trigger/effective-filter and live gaps. |
| OF03 | Server queue, SDK notification → `notification.rs`; O06 queue live gap, O07 synthetic overflow covered. |
| OF04 | SDK revised counters → `UaRevisedSubscription::from_sdk`, Engine; O08 controlled, O09 live gap. |
| OF05 | SDK recovery → adapter recovery and Engine snapshot; O10–O13 historical selected live, production automatic replay/transfer gaps. |
| OF06 | SDK Write → `io.rs`, Engine/CLI outcomes; O14–O16 controlled, O17 explicit order test gap. |
| OF07 | SDK Read → `core.rs` splitter; O18/O19 local sample and historical receipt, other bounds in linked proof. |
| OF08 | SDK RegisterNodes → no adapter alias use; O20/O21 out of UA implementation scope, no alias acceptance claim. |
| OF09 | SDK create/callback → adapter subscription/notification; O22 exact Bad-initial live gap. |
| OF10 | DA SDK/router and Engine capability; O23 out of UA N5, [native ledger](native-sdk/acceptance-ledger.md). |
| OF11 | DA SDK/router; O24–O26 out of UA N5, version/Async gates remain in native ledger. |
| OF12 | SDK Session and callback lifecycle → connection/recovery guards; O12/O13/O27 selected tests, remaining replay/identity/namespace gaps in recovery ledger. |

### Consumer and duplicate review

`contract/src/model/event.rs` preserves separate requested/revised values and
optional subscription revision; `engine/src/lib.rs::revised_interval_for`
only accepts valid revised state, never relabels a fallback as revised, and
tests invalid/stale facts. App `state.rs`/`ui/dialogs/acquisition.rs` now show
requested/revised publishing, sampling, queue, lifetime and keepalive, with
missing values shown as unknown and invalid state explicit. The CLI event
previously omitted lifetime/keepalive counts and validity; `ua_subscription_json` now emits
both requested counts, optional revised counts and optional validity (unknown is
JSON null), with a targeted regression. Read maxAge is a request, not a revised
fact; per-dispatch external reporting remains a gap, not an invented value.

Candidate source review (no blanket claim about unreviewed paths): SDK
`subscriptions/service.rs` owns Publish/ACK; `connection.rs::trigger_publish_now`
signals that owner, not a second Publish. No adapter Republish/Transfer or
RegisterNodes invocation was found in `crates/backend/src/bff/ua`; recovery
observers and `recovery.rs` identity maps are product policy, not aliases.
`connection.rs` configures SDK retry; `subscription.rs` retries rejected
filters without filter and `core.rs` boundedly splits rejected Read batches:
application fallback, not a transport/reconnect loop. `io.rs` deduplicates
Browse references and discovery/scanner deduplicate endpoints/targets, not
SDK notification values. `notification.rs` guards epoch/registration and
preserves sequence/overflow facts; Engine fairness, logical RequestId and
recovery snapshot are product policy, not replay. Negotiated report reads SDK
state; removed unused `report_negotiated` was a dead wrapper, not a second SDK
implementation. These source observations do not prove all dynamic interleavings.

Fresh runs on this working tree: backend sample-server `ua_live_` 2/2 (wire
maxAge and negotiated state), `u4_` 3/3 (filter/interval), `o1` 2/2 (Read
limits). These tests start their own async-opcua sample servers: they are **not
CODESYS tests**. Alias is not enabled, so no alias live proof is claimed. The
CODESYS smoke/search/subscription/types/write results and known #12 failures
remain as recorded in status; the earlier search included a forbidden localhost
case. CODESYS-specific negotiated/filter/limits/initial-result evidence remains
open. `cargo test -j 2 -p opclook-backend` 322/0/1 is supplied by the parent,
not a run independently witnessed in this continuation.

`report_negotiated` was an unused wrapper around the active, revision-scoped
`report_negotiated_at`; the wrapper was removed without changing the active
reporting path. No behavior-changing duplicate has been established in this
review. Logical RequestId, per-session scheduling/fairness, stale-epoch
and namespace guards, and a recovery snapshot are application policies, not
evidence of a second SDK transport or Publish implementation. They must not be
removed on the basis of name similarity. Unknown/incomplete history is not
converted to success.

N5 audit scope is closed on the evidence above. Residual gaps stay visible in
the O/OF table and in the linked recovery/republish documents: O02–O09
(revised counters, timestamp trigger, queue/keepalive), O11–O12 (automatic
replay/history), O17 (duplicate-target ordering), O22 (exact Bad-initial
sample), CODESYS-specific negotiated/filter/limits/initial-result cases, and
per-dispatch maxAge observability. These are R2.6/recovery/N6 work, not a
reimplementation of the SDK in OPCLook.

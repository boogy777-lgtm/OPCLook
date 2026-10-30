# R2.REUSE: UA 0.19 source ownership checkpoint

Source review, 2026-10-31. This is a pinned-source audit, not a runtime recovery
acceptance report. Cargo.lock selects async-opcua-client 0.19.0. The reviewed
[client manifest](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/Cargo.toml)
at commit `9ad28fc011002398f2e8a95696a50408a14531d9` declares that version and
MPL-2.0. This is the existing dependency; no SDK change or new license terms
are introduced. A release dependency/notice audit remains separate.

`ua-sdk-source.py` checks the installed registry crate's `.cargo_vcs_info.json`
against this commit using locked Cargo metadata. Its temporary-file regression
passed locally; actual installed-crate verification is pending the new CI.
A matching manifest version alone is insufficient proof of published bytes.
No moving `latest` documentation is used as the API oracle.

## What the traced source owns

All paths below are under `async-opcua-client/src` at the commit above.

| Path and method | Observed implementation | OPCLook consequence / remaining gate |
| --- | --- | --- |
| [session/event_loop.rs](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/src/session/event_loop.rs), enter/run/spawn | SDK stream manages connection/retry/keepalive/subscription activity. run drains its result stream and spawn wraps run. The stream exposes BeginConnect, Reconnected(mode), reconnect failures and subscription activity. | Keep SDK as sole owner. Current OPCLook activate_ua_session calls spawn and observes final completion, not the detailed stream. A thin observer of the existing SDK stream is the next seam; no second reconnect timer. |
| [session/connect.rs](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/src/session/connect.rs), ensure_and_activate_session | Attempts ActivateSession; failed reactivation resets and creates a remote Session. Returns NewSession or ReactivatedSession. Calls subscription transfer/recreation when configured. | A new remote Session is distinct from local Connect epoch. Adapter registrations, aliases and negotiated snapshots require explicit reconciliation; source existence does not prove current application mapping. |
| [config.rs](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/src/config.rs), defaults | recreate_subscriptions defaults true; monitored-item recreation chunk defaults1000. | Use configuration and SDK implementation, not an independent reconstruction loop in Engine. Verify actual configured behavior in live interruption tests. |
| [session/services/subscriptions/service.rs](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/src/session/services/subscriptions/service.rs), transfer_subscriptions_from_old_session | Tries TransferSubscriptions, then recreates failed subscriptions and monitored items through existing SDK service paths. | OPCLook's stored server subscription/item IDs may become stale even without a new local Connect. It must observe/reconcile SDK facts; this audit does not close O13/O27. |
| Same service file, publish; [state.rs](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/src/session/services/subscriptions/state.rs), handle_notification | Publish sends acknowledgements, requeues acknowledgements after service failure and forwards notifications to stored subscription callbacks. | SDK owns acknowledgement delivery; automatic missing-sequence replay is not established by this trace. |
| [event_loop_state.rs](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/src/session/services/subscriptions/event_loop_state.rs), tick | Maintains pending Publish requests and handles TooManyPublishRequests/NoSubscription/fatal session errors. | No Engine Publish loop. Keepalive/Publish activity must not manufacture a user-tag value or received timestamp. |
| service.rs, republish | Public Republish service API exists. The traced automatic Publish path above calls publish, acknowledgement and notification processing; existence of the API/comment does not establish automatic missing-sequence replay. | O11/O12 sequence-gap detection, retransmission behavior and visible unrecoverable gap remain open. Do not create a parallel replay owner without a confirmed library gap and regression. |
| [subscriptions/mod.rs](https://github.com/FreeOpcUa/async-opcua/blob/9ad28fc011002398f2e8a95696a50408a14531d9/async-opcua-client/src/session/services/subscriptions/mod.rs) | Public state exposes SDK-normalized revised publishing/lifetime/keepalive and item sampling/queue. | Pair04 maps these facts separately from requested values. SDK normalization can lose original wire f64; sampling-1 remains a sentinel. New sample-server test is pending CI. |

The acknowledgement/notification row describes SDK source, not delivery of all
historical values. A successful snapshot Read never closes a lost-history gap.
The relevant SDK subscription sources were read in full; this is a scoped trace,
not a claim that every platform/configuration path has been exercised.

## Next acceptance slice

1. Verify the installed Cargo source identity in Linux and Windows CI.
2. Observe the SDK's existing event stream without a second protocol owner.
3. Test short transport interruption with preserved remote Session separately
   from server restart/new remote Session and subscription recreation. Preserve
   local Connect epoch unless the application actually reconnects.
4. Reconcile SDK subscription/item facts and alias scope; reject retired adapter
   registrations before accepting values. Refresh negotiated facts after recovery.
5. Establish sequence-gap/replay behavior and the library's supported callbacks
   before selecting ResendData/snapshot behavior. Keep unrecovered history visible.
6. Execute O09–O13/O27–O29 through real sample-server/adapter entry points.

This does not remove the existing UA I/O timeout quarantine, claim recovered
history, close remote DA acceptance, or authorize production DA switching.

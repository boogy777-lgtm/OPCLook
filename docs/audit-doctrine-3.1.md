# Doctrine v3.1 READ-ONLY Audit — opcda-egui first-party Rust

Scope: `src/app.rs`, `src/ui.rs`, `src/bff/{model,mod,da,ua,da_variant}.rs`, `src/settings.rs`,
`src/columns.rs`, `src/journal.rs`, `src/i18n.rs`, `src/constants.rs`, `src/lib.rs`, `src/main.rs`,
`tests/*`. Third-party/generated code (`async-opcua`, `egui`/`eframe`, `tokio`, `windows`,
`src/opc/bindings/*`) excluded except where first-party code mishandles their API.

Non-goal: GoF patterns and rewrites. All remediation below is minimal, doctrine-shaped
(prefer ΔR<0 or ΔI>0).

---

## 1. Verdict Summary

The codebase is unusually disciplined for a re-implementation of a legacy Delphi app. Classes are
largely defined by *membership via exhaustive enums / matches* rather than by name-checking, and
each mechanism family carries explicit checked invariants (unit tests, integration tests, or a
self-checking dictionary). The two strongest exemplars are the **operation-code module** (`op`) with an
ASCII/completeness test, and the **i18n Tr dictionary** with a three-way exhaustive match plus a
compile-time-injected test that asserts no Cyrillic UI literal leaks into production code.

However, there is corner erosion: several mechanisms carry **string-typed special cases that are R>0
and carry no matching I**, and a few are genuine Patch / near-!OK. Most are small and confirmed.

Counts:

| Verdict | Count |
|---|---|
| OK | 10 |
| Patch | 4 |
| !OK | 1 |

> Status after the 2026-10-25 pass: 3.1 (!OK), 3.2, 3.3 and the command-dispatch
> part of 3.6 are fixed — see §6 for file:line evidence and the remaining list.
> A later 2026-10-26 security pass fixed 3.5 (typed `UaSecurityPolicy` /
> `UaSecurityMode`, strict `resolve_security`) — see §7. The counts above
> describe the original audit snapshot.

Top 10 issues (severity descending):

1. **!OK — `add` / `remove` / `subscription` operation strings are raw literals.** Not in `op`;
   `operation_label()` (app.rs:2947) leaves them untranslated, so `EventKind::Error` for these ops
   render the raw English token in the Russian journal. Confirmed. (ua.rs:438,467,481; da.rs:236;
   app.rs:2947-2966)
2. **Patch — operation-string dispatch is a string `if/else` chain** (app.rs:1475-1521). String
   compare is itself an operation rule; a misspelled/uncaptured op silently falls through to a generic
   journal line instead of the typed handling. No exhaustive match / completeness guard over the *dispatch*.
3. **Patch — duplicated discovery-client builder.** `search_servers` inlines a client build
   (ua.rs:1018-1027) identical to `build_discovery_client()` (ua.rs:1188-1199), except one sets
   `session_retry_limit(0)` and the other also does; full duplication. Next "one more one-shot client"
   (GetEndpoints on another host, FindServers variant) has two edit points.
4. **Patch — `subscribe`/`polling` bimodal decision is a boolean flag + distance check.**
    *(FIXED 2026-10-25: replaced by the `SubscriptionState` enum; see 3.4 and the
    remediation section.)* Historically the tick used
    `!(protocol == OpcUa && subscription_active)`, where `subscription_active`
    was a plain `bool` on `SessionView` that nothing forced consistent with
    what the worker actually created.
5. **Patch — security policy/mode carried as free strings.** *(FIXED 2026-10-26: typed
   `UaSecurityPolicy` / `UaSecurityMode` at every first-party boundary, strict resolution; see §7.)*
   Was: `Option<String>` in model.rs, dialog in ui.rs, combos of short names. `resolve_security`
   string-parsed them (`parse_mode`, `policy_uri_short`, `policy_uri`). Auto ("Авто") was a string
   sentinel, and a server-advertised unknown policy collapsed via `.unwrap_or(SECURITY_POLICY_NONE)`.
6. **Patch — suppressed send errors everywhere.** `Gateway::send` swallows the channel result
   (`let _ = tx.send`, mod.rs:40) and every BFF worker `send`/`send_error` does the same
   (ua.rs:541-563, da.rs:299-321). A dead/slow worker silently drops commands/events.
7. **Patch — `<...>` no-op `ConfigureUaSubscription` for DA** (da.rs:295). Empty match arm in
   `handle_command`: UA-only config silently discarded for DA. Confirmed; low harm (DA polls), but it
   is a "mode with no behavior" rule.
8. **Patch — CSV header skip / session resolve are string heuristics.** `parse_csv_rows` skips a header
   by `line_index == 0 && starts_with("server")` (ui.rs:3789); `resolve_csv_session` matches by
   `display_name().contains(server)` substring (ui.rs:3851-3862). Fuzzy matching on user data; a false
   positive silently imports into a wrong session.
9. **Patch — `GROUP_UPDATE_RATE_MS` duplicates the update-interval constant.** Local literal 1000 in
   da.rs:29 (and `AddGroup` uses it) not centralized with `UPDATE_INTERVAL_DEFAULT_SECS` in
   constants.rs. Two sources of truth for "1000 ms / 1 s".
10. **Patch — settings uses two parallel column-order stores.** `columns.order` and `column_order`
    (Vec<String>) must be kept in sync by `persist_column_order`/`resolved_order` (settings.rs:178-211).
    A legacy/foreign JSON with only one present silently re-derives the other; every future
    column-order state (e.g. per-session order) must touch both.

No confirmed instance violates a *type* guarantee or an exhaustive match in a way that panics at
runtime; the `unsafe` COM/UA conversions are well-fenced and test-backed (da_variant.rs), which I do
not flag as a doctrine violation.

---

## 2. Mechanism Table

Mechanism | Requirement class + near non-member | Invariant + guard | ΔR | ΔI | Verdict | Evidence
---|---|---|---|---|---|---
BFF Command/Event gateway | class: commands routed by `Protocol` (OpcDa/OpcUa); near non-member: a command with unknown protocol or dead worker | `Protocol` is a 2-variant enum, matched exhaustively in `send`; `send` now returns `Result`; `shutdown()` joins workers with a bounded wait; tests `control_command_classification`, `send_after_shutdown_returns_error`, `shutdown_joins_workers_without_hanging_and_is_idempotent` | 0 | 4 (enum; Result; bounded join; 2 tests) | OK | mod.rs:87-113, ua.rs:2354, tests/gateway_shutdown.rs:17,29
Sessions/tabs lifecycle | class: session container (`visible`, kept in memory); non-member: unhidden session removed from `sessions` | `SessionView` struct + `activate_nearest_visible_around` tests; `disconnect`∉`remove` separation | 0 | 2 (types; tests) | OK | app.rs:91-163, 839-916
Reconnect schedule | class: backoff by `attempt`; non-member: attempt 0 | `reconnect_delay` overflow-pinned; single `ReconnectState` enum; cap/exhausted/resume/stop tests | 0 (was 2 boolean flags) | 5 (reconnect_delay tests; cap/stop-row/disconnect tests) | OK (fixed) | app.rs `ReconnectState`, tests/ui_interactions
Subscription vs polling | class: UA push; non-member: DA poll | `SubscriptionState { Inactive, Active, Failed }`; only `Active` suppresses `read`; Active/Failed/DA + popup tests | 1 (enum) | 4 (active, failed, DA, popup tests) | OK (fixed) | app.rs `subscription`, bff/ua.rs, tests/ui_interactions
Security policy selection | class: policy+mode combo; near non-member: unknown server policy | typed `UaSecurityPolicy`/`UaSecurityMode` + exhaustive label matches; serde legacy aliases (unknown = error); `resolve_security -> Result` (no fallback); unit + kittest + live tests | 0 free strings | 6 (enums; serde migration; URI mapping; pair validator; resolve tests; kittest/live) | OK (fixed) | model.rs, ua.rs, settings.rs, ui.rs, tests/security_live.rs
DnD | class: tree→tags drop; non-member: plain click | `TreeDragPayload` type + `dnd_proof` raw-input test | 2 (payload type, drop-zone) | 3 (dnd_proof 3 tests) | OK | ui.rs:57,4045
Recursive add-children job | class: depth-first walk; non-member: cancel on disconnect / manual browse | `AddChildrenJob` with `visited` + `in_flight: HashMap<RequestId, String>` + unit tests incl. cycle, cap and request-id attribution | 2 (`in_flight`,`capped`) | 6 (tests) | OK (fixed attribution) | app.rs `AddChildrenJob`
i18n dictionary | class: every `Tr` key; non-member: a literal outside `Tr` | 3 exhaustive `ru/en/zh`; `Tr::COUNT/all()` + `every_key_is_nonempty` + `no_hardcoded_cyrillic_ui_strings` | 1 (fmt `{n}` placeholder) | 3 (enum completeness; dictionary test; no-hardcoded test) | OK | i18n.rs:514-596, tests/no_hardcoded
Column order persistence | class: canonical `ColumnId` set; near non-member: unknown persisted name | `ColumnId::ALL` + `resolved_order` fill-missing + 2 storage tests | 2 (legacy double-quote parse, dual-store sync) | 2 (dual-store tests) | Patch | settings.rs:167-211, columns.rs:218
Quality placeholders | class: sentinel code; non-member: any real OPC status | sentinel `code` range + `is_placeholder_*` methods; avoid text scan | 0 | 3 (sentinel methods, `from_da`/`from_ua`, no-text-scan) | OK | model.rs:183-230
Operation-string matching | class: app reacts to typed ops; non-member: unrecognized op | `Operation` enum + `as_str()` + exhaustive dispatch/label matches; `Operation::ALL` ASCII/uniqueness test; `command_kind_maps_to_its_operation`; static guard `no_raw_operation_strings_or_dispatch` | 0 raw strings | 4 (typed enum; exhaustive matches; 2 unit tests; 1 guard test) | OK | model.rs:15-81,532-548,581; app.rs:1500-1567,2993-3011; tests/operation_guard.rs:38
Hardcoded constants | class: named in `constants.rs`; near non-member: literal in bff | constants tests (ranges/values) | 3 (GROUP_UPDATE_RATE_MS, MAX_ARRAY_ITEMS, 0x2000/0x0FFF repeated) | 1 (constants tests) | Patch | constants.rs:116-178, da.rs:29, da_variant.rs:13,52
Error propagation | class: error surfaced to journal/dialog; non-member: swallowed | command path: `Gateway::send -> Result`, App journals + marks sessions dead; worker join bounded; dead `let _ = last_error` removed. Event-channel sends remain best-effort (no consumer after app exit) | 0 | 2 (App send-failure path; shutdown test) | Patch -> command path OK | app.rs:724-753, mod.rs:87-113, ua.rs:541-563, tests/gateway_shutdown.rs
Retries/timeouts | class: timeout+retry per op budget; non-member: no budget | `tokio::time::timeout` everywhere; `reconnect_delay` overflow test | 2 (retry_limit 0 vs 3; fallback_items) | 2 (timeout constant tests) | OK | ua.rs:1018,1246, reads
Tests as invariants | class: checked invariant; near non-member: live server only | unit tests + `#![ignore]` live tests; DnD raw-input proof | 0 | extensive | OK | tests/*

---

## 3. Detailed Findings

### 3.1 !OK — unwired operation strings render untranslated — **FIXED (typed `Operation`)**

- Severity: high
- Where: `src/bff/ua.rs:434-441 (op "subscription")`, `ua.rs:467 ("add")`, `ua.rs:481 ("remove")`,
  `src/bff/da.rs:236 ("remove")`; `src/app.rs:2947-2966` (`operation_label`)
- Class/Inv/N+1: `EventKind::Error { operation }` is the transport of error taxonomy. The intended
  class is "canonical op code", with `op` (model.rs:12-27) as the membership table. The near
  non-member is the string "add"/"remove"/"subscription" which bypasses the table. Invariant missing:
  *every* `operation` string passed to `send_error` must be a member of `op` (a type/`&'static` list or
  a test that greps `send_error` args). Because `operation_label` has no arm for these, the journal
  shows raw "subscription"/"add"/"remove". Defect recurrence: three independent emitters already
  diverged; any future op has two edit points (emit + label).
- Minimal remediation (ΔI>0): add `op::SUBSCRIBE`, `op::ADD_ITEMS`, `op::REMOVE_ITEMS` usage at the
  three literal sites and a label arm; tighten the existing `operation_codes_are_ascii_and_cover_emitters`
  test to assert every `send_error` op token is in `op::*` (value check is enough; a full exhaustive
  list is the ideal ΔI). Recurrence guard: extend the model.rs test that already pins the emitter set.

### 3.2 Patch — string if/else dispatch on `operation` — **FIXED (exhaustive `match Operation`)**

- Severity: medium
- Where: `src/app.rs:1475-1521` (`if operation == op::CONNECT { } else if op::BROWSE …`)
- Class/Inv/N+1: `CONNECT`/`VIEWS`/`READ`/`TEST`/`SEARCH_*` are the only ops with special handling;
  every other op falls through to the generic `journal.error_l` line — a valid near non-member. The
  string-chain itself is an untyped rule duplicated across `da`/`ua` emitters; a new op with special
  handling has two edit points (this chain + `operation_label`). R = N_ops_handled (>0), I = no
  exhaustive check on the *dispatch* (only the emitter ASCII test).
- Minimal remediation (prefer ΔR<0): make the dispatch match on `&op::*` **constants** (it already
  does) and add a fallthrough guard that warns to the journal when an op has no handler — turning the
  "silent generic line" into a checked invariant (ΔI>0). Recurrence guard: unit test that feeds an
  unknown `operation` and asserts the generic journal path is taken (documents the non-member).

### 3.3 Patch — duplicated discovery client builder — **FIXED (single factory)**

- Severity: medium
- Where: `src/bff/ua.rs:1018-1027` (inside `search_servers`) vs `src/bff/ua.rs:1188-1199`
  (`build_discovery_client`)
- Class/Inv/N+1: class = "one-shot UA discovery client (FindServers/GetEndpoints/FindServersOnNetwork)".
  Membership is identical builder args (application_name/uri, sample keypair, trust_all, pki,
  retry_limit 0). The two sites are the classic copied-function-with-edits. Next same-class case
  (e.g. a `FindServers` against a non-default port, or a per-host client in `search_network_servers`)
  touches both. Invariant: none ties the two to the same params; drift risk.
- Minimal remediation (ΔR<0): `search_servers` should call `build_discovery_client()` (it already
  exists precisely for this). Recurrence guard is the shared function; optionally a test that
  `build_discovery_client` is the single client factory (grep `ClientBuilder::new` count == 1 +
  the connect builder).

### 3.4 Patch — subscription vs polling decided by a detached bool — FIXED

- Severity: medium
- **Status (2026-10-25): fixed.** The detached `subscription_active: bool` is
  replaced by `SubscriptionState { Inactive, Active, Failed }` in
  `SessionView.subscription` and in `EventKind::SubscriptionState { state }`.
  Only `Active` suppresses the periodic `Read`; `Inactive` and `Failed` both
  poll. The UA worker emits `Active` on successful monitoring, `Failed` on a
  monitor failure and `Inactive` on teardown/no items. The server-info popup
  shows the state (localized ru/en/zh).
- Where it was: `src/app.rs` (`subscription_active`, read-suppress branch),
  `Connected` reset, `SubscriptionState` handler; now the enum fields above.
- Class/Inv/N+1: class = "session that receives pushes and therefore must not
  poll". Near non-members were DA and a UA session whose worker reported
  `active:false`; the old bool could not distinguish "never created/torn down"
  from "creation/monitoring failed". The enum makes both explicit while keeping
  polling behavior identical for both.
- Recurrence guards: `ua_subscription_active_stops_periodic_read`,
  `ua_failed_subscription_falls_back_to_polling`,
  `da_keeps_polling_unaffected_by_subscription_flag`,
  `server_info_popup_shows_subscription_state`
  (`tests/ui_interactions.rs`).

### 3.5 Patch — security policy/mode carried as free strings — **FIXED (typed enums)**

- Severity: medium (security-relevant)
- Status (2026-10-26): fixed. `UaSecurityPolicy { Auto, None, Basic128Rsa15, Basic256,
  Basic256Sha256 }` and `UaSecurityMode { Auto, None, Sign, SignAndEncrypt }` live in
  `src/bff/model.rs` with `as_str()`/`Display`/`as_uri()`/`from_advertised()` and a manual
  serde pair. `EndpointSpec`, `Settings` and `ConnectDialog` carry the enums, not strings.
- Legacy migration: the deserializer accepts `"Авто"`/`"Auto"`, `"None"`, the short policy
  names and full `http://opcfoundation.org/UA/SecurityPolicy#...` URIs. An unknown value is a
  serde error (never a silent `Auto`). Missing fields default to `Auto`.
- Resolution: `resolve_security(policy, mode, endpoints) -> Result<(String,
  MessageSecurityMode), String>`. `Auto` keeps the old preference order (None → Basic256Sha256
  Sign → Basic256Sha256 SignAndEncrypt → first recognized). An explicit choice must match an
  advertised endpoint exactly; otherwise the call returns a detailed error listing what the
  server offers. Invalid combinations (`None` policy + Sign/SignAndEncrypt) are errors. The
  worker prefixes resolution failures with `SECURITY_ERROR_PREFIX`; the app wraps that detail
  in a localized message and keeps the dialog open.
- Recurrence guards: `security_enums_serde_roundtrip_and_stable_tokens`,
  `security_legacy_strings_deserialize`, `security_unknown_strings_are_errors_not_auto`,
  `security_policy_uri_mapping`, `security_pair_compatibility`,
  `endpoint_security_legacy_json_migrates_to_enums` (`model.rs`); settings migration tests;
  `explicit_*_is_an_error*`, `unknown_advertised_policy_is_an_error_not_none`
  (`ua.rs`); kittest typed/persist and invalid/unadvertised tests; live
  `tests/security_live.rs`.

### 3.6 Patch — suppressed send failures — **FIXED for command dispatch + worker join**

- Severity: medium
- Where: `src/bff/mod.rs:40` (`let _ = tx.send(command)`); `src/bff/ua.rs:541-543` and `da.rs:299-301`;
  `src/bff/ua.rs:1182` (`let _ = last_error;`)
- Class/Inv/N+1: error-propagation class: a dropped command (worker thread dead) or dropped event
  (queue full/closed) is invisible. `let _` is the doctrine's suppressed-error signal. I = 0 for the
  send path (no test asserts a send failure is surfaced). The `let _ = last_error` in
  `search_network_servers` is dead code that *silently discards a diagnostic* — a confirmed bug in the
  error-propagation mechanism.
- Minimal remediation (both ΔR<0 / ΔI>0): in `Gateway::send`, on `Err(_)` push a synthetic
  `EventKind::Error { operation: op::UA_RUNTIME, … }` so a dead UA/DA worker is visible to the UI
  instead of silently dropped (ΔI>0). Remove the dead `let _ = last_error` in `ua.rs:1182` (ΔR<0) or
  route it into the `"mDNS/LDS не отвечает"` message. Recurrence guard: a unit test on `Gateway::send`
  to a closed channel asserts an `Error` event is produced.

### 3.7 Patch — no-op `ConfigureUaSubscription` for DA

- Severity: low
- Where: `src/bff/da.rs:295` (`CommandKind::ConfigureUaSubscription { .. } => {}`)
- Class/Inv/N+1: UA-only mechanism reaching the DA adapter. The empty arm is a "mode with no
  behavior"; harmless because DA always polls, but it is an R-counted special case with no I: a future
  UA-only command could be silently discarded here by omission (no `#[allow(unreachable_code)]`, no
  `unreachable!()`). Better to `=> unreachable!()` or `Err("unsupported for DA")` so the protocol
  boundary is explicit.
- Minimal remediation (ΔI>0): replace `{}` with an explicit `send_error(…, "UA subscription не
  поддерживается для OPC DA")` mirroring the `ResolveBrowsePath`/`BrowseViews` arms already in this
  file. Recurrence guard: the exhaustive `match command.kind` in both adapters is already the guard —
  make DA's extra arms loud.

### 3.8 Patch — CSV header/session-matching are fuzzy string heuristics

- Severity: low-medium
- Where: `src/ui.rs:3789` (`line_index == 0 && line.to_ascii_lowercase().starts_with("server")`),
  `src/ui.rs:3851-3862` (`resolve_csv_session` `display_name().contains(server)`), `src/ui.rs:3799`
- Class/Inv/N+1: class = "CSV import row identity → session". A header row is skipped by value-guess;
  a session is matched by substring `contains`. A value-collision (two endpoints where one name is a
  substring of the other, or a server label that begins with "server") changes import behavior without
  any code change. R = the `contains` + `== 0` rule; I = only the roundtrip export/import tests
  (ui_interactions:387), which use distinct names, so the collision case is unguarded.
- Minimal remediation (ΔI>0): (a) match the header by exact column-name equality per `ColumnId`, not
  `starts_with("server")`; (b) prefer longest/exact match in `resolve_csv_session` and journal a
  warning when the match is only a substring. Add a unit test with colliding names asserting the exact
  session is picked. Recurrence guard: extend the roundtrip test.

### 3.9 Patch — `GROUP_UPDATE_RATE_MS` duplicates update-interval constant

- Severity: low
- Where: `src/bff/da.rs:29` (`const GROUP_UPDATE_RATE_MS: u32 = 1000`) and `da.rs:345` (used in
  `AddGroup`), vs `src/constants.rs:54` `UPDATE_INTERVAL_DEFAULT_SECS = 1`
- Class/Inv/N+1: "update cadence" value has two declarations; the DA group's 1000 ms is decoupled
  from the UI's update interval and from the UA publishing interval, yet they are conceptually the
  same knob. The array/truncation literals (`da_variant.rs:13 MAX_ARRAY_ITEMS`, `0x2000`/`0x0FFF`
  repeated) are the same family (documented inline, not centralized).
- Minimal remediation (ΔR<0): move `GROUP_UPDATE_RATE_MS` into `constants.rs` (referencing it from
  both UA `publishing_interval_ms` and DA `AddGroup`), or derive it from the option. Recurrence guard:
  the constants.rs test suite already pins values; extend it to assert the DA group rate equals the
  default update interval.

### 3.10 Patch — dual column-order store must be kept in sync manually

- Severity: low-medium
- Where: `src/settings.rs:178-211` (`resolved_order`/`persist_column_order`), `Settings` fields
  `column_order: Option<Vec<String>>` + `columns: ColumnSet` (settings.rs:63,73)
- Class/Inv/N+1: class = "persistent column order". Two representations (typed `Vec<ColumnId>`
  legacy and string snapshot) with hand-maintained sync. Near non-member: a settings.json written by a
  foreign/older build with only `column_order`. The sync is done in two functions (edit points = 2);
  a new kind of order state (per-session, per-load) must touch both. I = the two roundtrip/legacy
  tests, which cover the *known* shapes but not a new field.
- Minimal remediation (prefer ΔR<0): drop the string `column_order` snapshot and keep only
  `columns: ColumnSet` (already the canonical typed struct), simplifying `resolved_order` to plain
  `columns.order` with legacy deserialization handled by serde default. If the string variant must
  stay for backward compat, derive it (ΔI>0) with a test that both are always equal after
  `persist_column_order`. Recurrence guard: `persist_column_order_updates_resolved_and_columns`.

### 3.11 OK notes (doctrine applied well)

These are mostly OK and are the reference behavior to preserve:

- **Operation-code module + completeness test**: `op` is ASCII, listed exhaustively in a unit test
  (model.rs:509-536). This is the discipline the raw `add/remove` literals violated.
- **i18n Tr dictionary**: 3 exhaustive dictionaries + `Tr::COUNT/deck` + `every_key_is_nonempty` +
  the no-hardcoded-Cyrillic scanner (tests/no_hardcoded_ui_strings.rs). Excellent ΔI for the
  "all UI copy in one place" class.
- **DnD**: a typed payload (`TreeDragPayload`) plus a real raw-input drag test
  (`drag_item_onto_tags_table_adds_row`) — a confirmed invariant over an egui gesture that kittest
  cannot express.
- **Recursive add-children**: the `visited` + `in_flight: HashMap<RequestId,
  String>` job guards cycles and correlates each page by the exact request id
  it issued (manual/stale browses cannot be consumed), with unit tests for
  cycles, cap and request-id attribution. Model.
- **Add-items dedup + `register_rows` on reconnect**: rows are re-registered after reconnect
  (`ua_reconnect_resubscribes_rows…` test).
- **Quality placeholders via sentinel codes** rather than text-scanning (`is_placeholder_*`).

---

## 4. Where the Doctrine Was Applied Well

- Exhaustive enum + match membership for `Protocol`, `CommandKind`, `EventKind`, `QualityKind`,
  `WriteKind`, `ColumnId`, `Lang`, `BottomTab`, `SessionState`. Adding a variant forces all matches
  (type-level I).
- The `Tr` dictionary with a compile-time `all()` deck and the no-hardcoded-Cyrillic scanner test is
  the single strongest ΔI in the codebase.
- The operation-code module `op` (with ASCII test) is a near-correct N+1/Contract mechanism; only the
  three literal sites (3.1/3.2) let it decay.
- `AddChildrenJob` state machine and its DnD-proof tests are recurring-guard exemplars.
- Reconnect backoff (overflow-pinned), the single `ReconnectState` enum and
  their unit/interaction tests.

---

## 5. Prioritized Follow-Up

Scope estimates: S (≤0.5 day), M (≤1 day), L (>1 day). "Confirmed" = directly observed in
code/test; "Hypothesis" = inferred risk without a failing reproduction.

P0
1. (M, Confirmed) Wire `op::SUBSCRIBE/ADD_ITEMS/REMOVE_ITEMS` at `ua.rs` 438/467/481 and `da.rs` 236;
   add a `operation_label` test + extend the emitter-completeness test (3.1).
2. (S, Confirmed) Remove the dead `let _ = last_error;` in `ua.rs:1182` or surface it (3.6).

P1
3. (M, Confirmed) Make `Gateway::send` surface a dropped command as an `Error` event and add the test (3.6).
4. (S, Confirmed) Collapse `search_servers`' inline client build onto `build_discovery_client` (3.3).
5. (S, Confirmed) Replace the empty DA `ConfigureUaSubscription` arm with an explicit
   "unsupported for DA" `send_error` (3.7).
6. (S, Hypothesis) Tighten `resolve_csv_session`/header detection to exact match; add collision test (3.8).
7. (S, Confirmed) Centralize `GROUP_UPDATE_RATE_MS` into `constants.rs` (3.9).

P2
8. ~~(M, Hypothesis) Validate/reject unknown security policy in `resolve_security` instead of silently
   forcing None; add unknown-policy unit test (3.5).~~ **Done 2026-10-26:** typed enums + strict
   `Result` resolution (see §7).
9. (M, Hypothesis) Reduce dual column-order store to one canonical `ColumnSet` (3.10).
10. ~~(M, Hypothesis) Make subscription/polling transition a small owned state and add a
    active→inactive re-poll test (3.4).~~ **Done 2026-10-25:** `SubscriptionState`
    enum + `Failed`→polling test.

Note on live `#![ignore]` tests (ua_subscription/ua_security/ua_lifecycle/ua_refresh_speed etc.):
they are genuine invariants (I) but only run against an external CODESYS server; they are not a
fallback the CI enforces, so confirm before relying on them as the sole guard for security/subscription
behavior.

---

## 6. Remediation pass — typed operations + gateway lifecycle (2026-10-25)

Fixed in this pass (file:line as of this change):

1. **3.1 / 3.2 — typed operations (was !OK/Patch).**
   - `Operation` enum (17 variants) with `as_str()`: `src/bff/model.rs:15-81`.
   - `EventKind::Error { operation: Operation, .. }`: `src/bff/model.rs:581`.
   - `CommandKind::operation()` exhaustive command→operation mapping:
     `src/bff/model.rs:532-548`.
   - Producers: `src/bff/ua.rs:211,314,331,350,357,373,414,416,429,431,444,446,459,488,502,543,555,557`;
     `src/bff/da.rs:111,121,128,133,155,185,187,193,200,242,284,296,298`.
   - Exhaustive dispatch (no wildcard/fallback): `src/app.rs:1501-1568`.
   - Exhaustive label mapping: `src/app.rs:2994-3011`; new `Tr::OpBrowseViews/OpResolvePath/OpSearchNetwork/OpSubscription/OpUaRuntime`
     in `src/i18n.rs:444-448` with ru/en/zh entries (`src/i18n.rs:943-947,1354-1358,1765-1769`).
   - Guard test rejects `operation == "`, `operation: "`, `operation.to_string()` and `op::` in `src/bff/*` + `src/app.rs`:
     `tests/operation_guard.rs:38`.
   - Unit tests: `operation_codes_are_ascii_and_cover_the_emitters`, `command_kind_maps_to_its_operation` (`src/bff/model.rs:589,610`); label completeness `every_operation_has_a_localized_label` (`src/app.rs:3194`).

2. **3.3 — duplicated discovery builder (was Patch).**
   - `search_servers` now calls the single factory: `src/bff/ua.rs:1038`, factory at `src/bff/ua.rs:1197`.
   - `discover_endpoints` already used the same factory (`src/bff/ua.rs:1391`).
   - The connect builder remains separate (`src/bff/ua.rs:1245`) by design: it differs semantically
     (user `trust_server_certs`, inverse `verify_server_certs`, `session_retry_limit(3)`, optional
     `session_timeout`) and is not a discovery client. No bool flag added to the discovery helper.

3. **3.6 — suppressed sends / worker death (was Patch).**
   - `Gateway::send -> Result<(), String>`: `src/bff/mod.rs:87-97`; call sites updated in tests/bins.
   - App surfaces send failure to the journal and marks every session on that protocol Disconnected
     (`src/app.rs:720-753`), without arming a reconnect that could never reach a dead worker.
   - `Gateway::shutdown()` closes both command channels and joins workers with a bounded 10 s budget:
     `src/bff/mod.rs:107-113`, `WORKER_SHUTDOWN_TIMEOUT` at `src/bff/mod.rs:15`.
   - Worker teardown on channel close: UA closes subscription + CloseSession/CloseSecureChannel
     (`src/bff/ua.rs:243-258`); DA drops each RemoveGroup (`src/bff/da.rs:78-80`).
   - `App::on_exit` sends Disconnect per session then calls `gateway.shutdown()`:
     `src/app.rs:3137-3148`.
   - Dead `let _ = last_error` removed with the unused variable/assignments in `search_network_servers`
     (`src/bff/ua.rs:1147-1191`); behavior (graceful empty result) unchanged.
   - Dead test-only `let _ = id_salt` removed (`src/app.rs` add-children test).
   - Tests: `tests/gateway_shutdown.rs:17,29`; `tests/ui_interactions.rs:2340` (`on_exit` path).

Remaining findings (unchanged; outside this pass):

- 3.5 security policy/mode free strings — **fixed in the later 2026-10-26 pass (§7).**
- 3.7 DA `ConfigureUaSubscription` empty arm — still silently discards a UA-only command.
- 3.8 CSV header/session substring heuristics — untouched (`src/ui.rs` outside this pass).
- 3.9 `GROUP_UPDATE_RATE_MS` duplicate constant — untouched (`src/bff/da.rs`, `constants.rs`).
- 3.10 dual column-order store — untouched (`settings.rs`).
- Event-channel `let _ = tx.send(event)` and best-effort cleanup timeouts remain intentional:
  once the UI receiver is gone there is no journal/dialog to report to; they are not dead code.

### Remediation pass 2026-10-25 — request correlation / reconnect / subscription

Fixed:

- **`Event.request_id` correlation.** `App::pending_requests:
  HashSet<(SessionId, RequestId)>`; insert on `send`, remove on receipt, reject
  unknown/replayed non-zero replies before any state mutation, accept `0`
  pushes/runtime events. Clear per session on Disconnect/Connect/reconnect and
  on send failure. Producers emit one correlated reply per command; ancillary
  timeout/diagnostic `Log`/`Disconnected` use `0`. Tests:
  `stale_reply_with_unknown_request_id_is_ignored`,
  `matching_request_id_reply_is_accepted`,
  `push_event_with_zero_request_id_is_accepted`,
  `disconnect_connect_does_not_let_old_connected_mutate_new_state`; add-children
  attribution test `add_children_ignores_unrelated_browse_request_id`.
- **Reconnect state machine.** `ReconnectState { Idle, Waiting { at, attempt },
  Stopped, Exhausted }` replaces `reconnect_at/attempts/cap/stopped/exhausted`;
  cap stays `RECONNECT_MAX_ATTEMPTS` (20). Link-loss → `Waiting(1s)`; failure →
  2/4/8…; attempt 20 failure → `Exhausted`; manual stop → `Stopped`;
  resume/manual reconnect → `Idle` (resume reschedules attempt 1); success →
  `Idle`; manual disconnect → `Stopped`. Server-row icons/hint/popup derive
  from the enum.
- **Subscription state.** `SubscriptionState { Inactive, Active, Failed }`
  replaces the detached bool in `SessionView` and the event; only `Active`
  suppresses polling; the popup surfaces the state.

Verification for this pass: `cargo test --lib` 148 passed;
`cargo test --test ui_interactions` 84 passed; all non-ignored integration
tests green; live UA smoke/write/search/types/security/subscription/
refresh_speed/lifecycle/large all green;
`cargo build --bin opcda-egui` clean without warnings.

## 7. Remediation pass — typed UA security (2026-10-26)

Fixed (3.5, was Patch):

- **Enums** — `UaSecurityPolicy` / `UaSecurityMode` (`src/bff/model.rs`) derive
  Clone/Copy/Debug/PartialEq/Eq/Hash; `as_str()`/`Display` are the stable ASCII
  tokens, `as_uri()` is the single policy→URI mapping (no prefix hacks).
  `validate_security_pair` encodes the invariant "None policy ⇒ None mode".
- **Serde/migration** — manual `Serialize`/`Deserialize`; legacy `"Авто"`,
  `"Auto"`, `"None"`, short names and full policy URIs are accepted, unknown
  values are errors. `Settings` gained `ua_security_policy`/`ua_security_mode`
  (default `Auto`); `EndpointSpec` and recent endpoints store the enums.
- **Resolution** — `resolve_security(...) -> Result<_, String>`; Auto preference
  order preserved; explicit policy/mode must be advertised exactly; invalid
  combinations and unknown advertised values are errors. The connect error is
  tagged with `SECURITY_ERROR_PREFIX`; `App::on_event` renders it through
  `Tr::SecurityResolveFailed` (localized wrapper + technical detail).
- **UI** — combos iterate `ALL` and render localized labels via exhaustive
  matches; selecting a value persists it to settings and immediately validates
  it; `App::validate_ua_security` blocks an invalid/unadvertised choice before a
  connect (dialog stays open with the error). New `Tr` keys ru/en/zh.
- **Tests** — enum serde roundtrip + legacy/unknown, URI mapping, pair
  compatibility, resolve errors (not offered / unknown advertised / empty),
  settings migration, kittest typed persistence + invalid/unadvertised
  no-connect, live `tests/security_live.rs` (Auto still connects; explicit
  Basic256Sha256/SignAndEncrypt against None-only CODESYS is a clear error).

Verification: `cargo test --lib` 162 passed; `cargo test --test ui_interactions`
87 passed; all non-ignored integration tests passed; live `ua_*` +
`security_live` passed against 127.0.0.1:4840; `cargo build --bin opcda-egui`
clean without warnings.

Remaining audit backlog (not part of this pass):

1. **`BffError` / newtype IDs** — workers still return `Result<T, String>`;
   `SessionId`/`ItemKey`/`RequestId` are bare `u64`/`u32` aliases.
2. **Settings path injection** — `Settings::load/save` read `APPDATA` at call
   time; add `load_from/save_to` and remove the test-global APPDATA lock.
3. **Test-hook gating** — `seed_*`, `on_event_for_test_*`, `tick_for_test` are
   production-public; gate behind `#[cfg(test)]`/a feature.
4. **Dialog split** — `ConnectDialog` and the other dialog structs still live
   in `app.rs`; move them to a view module.
5. **Duplicate column-order store** — `columns.order` + `column_order`
   (`Vec<String>`) remain hand-synchronized.
6. **CSV heuristics** — header skip by `starts_with("server")` and session
   matching by `display_name().contains(...)` still fuzzy.
7. Other unchanged items from §5: DA `ConfigureUaSubscription` empty arm,
   `GROUP_UPDATE_RATE_MS` duplication.

## Parts Not Verified

- I did not run `cargo test`/`cargo build` (READ-ONLY audit; no build configured). All file:line
  references are from static reading.
- `src/ui.rs` was read selectively (core draw path, dialogs, CSV, DnD columns/tree, tests). Some
  deeply nested rendering helpers (status_popup, about, options layouts) were skimmed, not line-audited.
- The generated bindings (`src/opc/bindings/*`) were excluded per instructions; correct
  `unsafe` usage there is not claimed.
- Live-server integration tests were reviewed for mechanism/invariant intent but not executed
  (they require a running OPC UA server).
# Architecture Audit — opcda-egui (skeleton, Rust idioms vs GoF)

Scope: original read-only review of the program skeleton. The 2026-10-25 pass
marked fixed items inline (`FIXED`) and added a status section (§9); unmarked
items remain open. Files read:
`src/{lib,main,app,ui,settings,columns,journal,i18n,constants}.rs`,
`src/bff/{mod,model,da,ua,da_variant}.rs`, `tests/*`, skimmed
`docs/{ui-parity,test-matrix}.md`.

Audience: the team that maintains this egui rewrite of the EleSy OPC DA/UA test
client. Verdict style: per-area findings first, then a prioritized plan with
minimal-diff sketches. Nothing here requires a behavior change.

---

## 1. Module / dependency map

```
main.rs ──> app.rs ──> ui.rs
               │          │
               │        columns.rs  (ColumnId + ColumnSet; pure model + localize keys)
               │        i18n.rs     (Lang + Tr + tr()/fmt(); pure dictionary)
               │        journal.rs  (Journal + Entry; pure log buffer)
               │
               ├──> settings.rs   (Options, Settings, serde persistence; path from APPDATA)
               ├──> constants.rs  (shared named literals; imported by app/ui/bff)
               ├──> bff/mod.rs    (Gateway: two senders + one event Receiver)
               │       │
               │       ├──> bff/model.rs  (Command/Event, Protocol, Value, Quality, ...)
               │       ├──> bff/da.rs     (single MTA COM worker thread; String errors)
               │       ├──> bff/ua.rs     (single worker thread + current_thread tokio)
               │       └──> bff/da_variant.rs (COM VARIANT <-> Value)
               └──> opc/bindings (raw `windows`-crate COM bindings; leaf, no deps)
```

Flow: `ui::draw(app, ui)` -> `app` mutates model -> `Gateway::send(Command)` ->
one of the two `unbounded_channel` tx -> worker thread -> `event_tx` (single
`std::sync::mpsc::channel`) -> `app.poll_events()` drains via `try_recv`.
`app.update()` = `poll_events` + `tick` + `draw` (`app.rs:712-717`).

### Cycles
None at the crate/module level. `app` <-> `ui` is one-way (`ui` imports
`app` types; `app` never imports `ui`). `settings` imports `columns` + `bff::model`;
`columns` imports only `i18n::Tr`. Dependencies are clean and acyclic. The only
"retro" edges are `app.rs` importing `settings::default_*()` and `bff::da_variant::vt_name`
directly (`app.rs:297`, `app.rs:160`) — benign, but they pull `da_variant`'s
COM-free pure helpers into view code (see boundary leak below).

### Boundary leaks — domain knowledge in the wrong layer
1. `Value: Display` (`bff/model.rs:303-358`) formats bytes/arrays/datetime for
   *display*. That is UI formatting living on the protocol-neutral model, in
   Russian ("…N эл."), hardcoded.
2. `Row::write_type_text` / `Row::timestamp_text` (`app.rs:295-314`) put
   presentation strings and `Local` timezone + format on a domain row.
3. `compose_server_name` and URL helpers `ua_host_port/ua_url_host/ua_url_port`
   (`app.rs:182-275`) — name composition is presentation; URL parsing is
   protocol detail. Both live in `app`.
4. `App` carries UI-only counters for the write progress indicator
   (`pending_writes/write_total/write_ok/write_err/last_write_summary`,
   `app.rs:615-621`) — mirror state that belongs to a view, not the model.
5. `Operation -> label` (`operation_label`, `app.rs:2947`) maps BFF op-codes to
   localized text in `app`; harmless but contributes to `app` being a god-object.
6. `Dialogs` structs carry `focus_pending` flags and `focused` cursor state
   (`app.rs:437-489`) — view/IM state owned by the model layer. Accepted tradeoff
   for immediate-mode, but it is exactly the "UI state mirror" the audit asks about.

### Test seams
- Unit: `#[cfg(test)]` modules in `settings/columns/journal/bff-model/bff-ua`, plus
  `src/app.rs` tests that drive `on_event_for_test`/`tick_for_test` and the pure
  helpers (`reconnect_delay`, `clamp_options`, `compose_server_name`).
- Headless UI (kittest): `tests/ui_interactions.rs` drives `ui::draw(app, ui)` via
  `Harness` and injects `RawInput`, aided by `ui::dnd_test_hook` /
  `ui_test_hook` / `columns_hook` global Mutexes that record the last drawn rects.
- Live integration (`--ignored`): `tests/ua_*.rs`, `tests/da_write.rs` spin real
  workers via `Gateway`; gated on an OPC server being up.

---

## 2. State ownership & data flow

### Single source of truth?
`App` is the single mutable root. It owns: `sessions: Vec<SessionView>`,
`rows: Vec<Row>`, `selected_rows`, `tree` per session, `dialogs`, `journal`,
`add_children`, `csv_import`, plus a separate *back-end* truth inside each
worker thread (`HashMap<SessionId, DaSession|UaSession>`).

Two truths are not reconciled by an ordering contract:
- **Rows are inserted optimistically** by `add_items`/`register_rows`
  (`app.rs:1836-1912`) with sentinel `Quality::CODE_PLACEHOLDER_*` codes, then
  *corrected* when `AddItems`/`Values` events arrive (`apply_item_info`,
  `app.rs:1527`). The "pending" placeholder semantics is a distributed protocol:
  app-side sentinel codes + worker `ItemInfo`/`DataValue` results. Clean enough,
  but the invariant "every `key` in the model corresponds to one live monitored
  item" is only enforced by event ordering, not by type.
- **Parameter state is duplicated**: `EndpointSpec` fields are echoed into
  `ConnectDialog` (`ua_security_policy`, `ua_session_timeout`, ...) and into
  `Settings` and into `SessionView::ua_*`. There are three copies of "requested
  UA security/timeout". The dialog is the draft; `open_tab_for_endpoint` copies
  into the session; settings persists separately. Acceptable, but is duplicated
  state with no invariant test. **Partially fixed 2026-10-26:** the security
  selection is now a typed `UaSecurityPolicy`/`UaSecurityMode` in all three
  copies, with serde migration and kittest persistence coverage.

### Mirrored view state
`selected_tree: Option<(SessionId,u64)>`, `selected_rows: HashSet<ItemKey>`,
`table_focus`, `table_anchor`, `tree_focus`, `tree_focused`, `bottom_tab`,
`status_popup` (`app.rs:596-622`) are pure view selections. They live in `App`,
which is fine for immediate-mode, but they are exactly the "UI state mirrors"
section. The issue isn't that they exist; it's that they are not separated from
model invariants, so any event handler (`on_event`) can and does touch both.

### Lifecycle
- Session container lifecycle is two-axis: `visible` (tab open) vs `state`
  (Connecting/Connected/Disconnected). Closing a tab never disconnects
  (`app.rs:876`); disconnecting keeps rows/tree marked stale (`app.rs:803`);
  only `remove_session` fully drops (`app.rs:839`). This is a subtle but
  deliberate model. **FIXED:** the five `reconnect_*` fields are replaced by
  one `ReconnectState { Idle, Waiting { at, attempt }, Stopped, Exhausted }`
  enum (`app.rs`), with the attempt cap kept as the shared
  `RECONNECT_MAX_ATTEMPTS` constant. Transitions live only in
  `schedule_reconnect` / `attempt_auto_reconnect` / `resume_reconnect` /
  `stop_reconnect` / the `Connected` handler.
- Reconnect ordering assumption: `attempt_auto_reconnect` sends `Disconnect`
  then `Connect` back-to-back. FIFO ordering still makes this safe, and the
  reply correlation below now also rejects a stale `Connected` from the
  superseded link even if it arrives after the new `Connect`.
- `add_children` job (`app.rs`) is driven by
  `in_flight: HashMap<RequestId, String>` (request id → node id). A `Browse`
  reply advances the job only on an exact request-id match, so a manual browse
  or a stale reply is never consumed as a job page.
- Graceful shutdown — **FIXED**: `on_exit` (`app.rs:3137`) sends `Disconnect`
  per session, then `Gateway::shutdown()` (`bff/mod.rs:107`) closes both command
  channels and joins the workers with a bounded 10 s budget (`bff/mod.rs:15`).
  The DA worker exits its `blocking_recv` loop and calls `RemoveGroup`; the UA
  worker performs subscription teardown + CloseSession/CloseSecureChannel
  (`bff/ua.rs:243`). Both workers signal completion over a `std::sync::mpsc`
  channel so the join is bounded rather than indefinite.

### Race / ordering assumptions (worker side)
- Both workers are single-threaded, so no data race inside a worker. The UA
  worker adds a priority re-ordering (`is_control_command`/`partition_commands`,
  `ua.rs:248-279`): control commands jump queued reads. This is a hand-rolled
  priority queue on an unbounded channel.
- Push vs poll: the UA `DataChange` callback (registered in `ensure_subscription`,
  `ua.rs`) sends `EventKind::Values` directly into the shared event channel.
  It runs on the tokio runtime's thread (inside the worker). The same key can
  therefore be updated by (a) periodic `Read` from the worker loop and (b) the
  DataChange callback. **FIXED (guard):** `EventKind::SubscriptionState {
  state }` is a three-state enum (`Inactive`/`Active`/`Failed`); only `Active`
  suppresses polling, and non-zero `Read`/`Browse`/`Connected` replies are
  accepted only for an outstanding `request_id`, so a late poll cannot mutate a
  newer lifecycle. DataChange pushes (`request_id == 0`) remain last-writer-wins
  by row order — no per-value sequence/timestamp dedup (still a residual,
  benign UI race).

---

## 3. Rust idioms vs GoF, pattern by pattern

### Command (+ CommandHandler) — present, earns its keep
`Command`/`Event` (`bff/model.rs:419-501`) is a straightforward Message/Command
object. For a cross-thread, two-backend boundary it is the right call and
idiomatic. Do not discard it for an actor framework. Refinements that fit Rust
idiom rather than GoF:
- **FIXED:** `Event.request_id` is meaningful. `App::send` records
  `(SessionId, RequestId)` in `pending_requests`; `on_event` drops non-zero
  replies that are not outstanding and removes the pair on receipt. `0` stays
  reserved for pushes/runtime events. See §10.
- Errors are `Result<T, String>` / `Err(String)` everywhere in the worker
  (`da.rs`, `ua.rs`). Prefer a typed `BffError` (thiserror is already a
  dependency, `Cargo.toml:27`) so the UI can branch on error *kind* instead of
  string-matching `op::`. The `operation: String` in `EventKind::Error`
  (`model.rs:500`) is a mini-type: replace with a `Operation` enum or tag the
  variant structurally. The op-code constants (`model.rs:12-27`) are a workable
  middle ground and are already tested for ASCII/completeness — acceptable, but
  a real enum removes the `&str` risk.

### Observer — present, and the interface is right-sized
DataChange (UA push) is a subscription callback into a channel — this is
idiomatic Rust (callback, not Observer with attach/detach). The `SubscriptionState`
event (`model.rs:496`) is the protocol-level "subscription became active" signal.
Keep it. Do not wrap the event channel in an Observable/Listener framework;
the single `mpsc` receiver is already the right primitive.

### State — present in two places
- `SessionState` (`app.rs:20`) is a clean 3-state enum with an exhaustive match
  in `tick`. Good.
- **FIXED:** reconnect is `ReconnectState { Idle, Waiting { at, attempt },
  Stopped, Exhausted }`. The old boolean cluster
  (`reconnect_stopped/exhausted/at/attempts/cap`) and its impossible states are
  gone; `Stopped` carries the manual-stop decision inside the enum. The
  transitions are centralized in
  `schedule_reconnect`/`attempt_auto_reconnect`/`resume_reconnect`/
  `stop_reconnect`/`Connected`.

### Strategy — absent, and keep it absent
The two backends differ by `Protocol`; the code handles the difference by
`match protocol { ... }` both in `write_type_choices` (`app.rs:334`) and in
many `if view.protocol() == OpcUa` branches. That is idiomatic enum dispatch
and should stay. Do *not* introduce `trait OpcBackend` + `Box<dyn>`; the
variant count is 2, the branches are exhaustively checked by the compiler, and a
trait would buy less than it costs. This is a case where GoF Strategy would
violate doctrine 3.1 (minimal mechanisms).

### Adapter — present, minimal
`da_variant.rs` adapts COM VARIANT <-> `Value`; the BFF `Value` is the
protocol-neutral vocabulary. This is justified. Keep it as pure functions
(`safe` wrappers around `unsafe`), no `trait`.

### Facade — the Gateway slightly over-reaches
`Gateway::new()` seeds two workers and merges their events into one receiver
(`mod.rs`). Good facade for "UI never talks COM/async". It is not an actor — it
is just two senders + a receiver, which is *more* idiomatic than an actor for
this shape. **FIXED:** `send` now returns `Result<(), String>` (`mod.rs:87`) and
`App` journals a failure + marks that protocol's sessions disconnected
(`app.rs:720`); `Gateway::shutdown()` (`mod.rs:107`) joins both workers with a
bounded wait. No actor framework was added.

### Over-engineering to avoid in Rust
- Full **actor** (tokio actor with request/response from each worker): the
  current two unbounded channels already provide ordered delivery; adding
  request-id correlation and typed replies is the *only* missing piece and can be
  added to the existing envelope. Do not rebuild as `Actor<Msg>`.
- **Builder** for `EndpointSpec`/`Command`: constructors `EndpointSpec::da/ua`
  (`model.rs:84-114`) cover the two main shapes; a builder buys nothing here.
- **Interpreter/CLI-parser** abstractions; **DI container**; **EventBus**;
  **Repository** for settings. Each would be net-negative.
- **Implementing `Deref`/`DerefMut`** to hide `SessionView` — avoid.

### Typestate / newtypes
IDs are already `type SessionId = u64`, `ItemKey = u32`, `RequestId = u64`
(`model.rs:4-6`). Good hygiene for *intent* but not *type safety* — they are
aliases, so `SessionId` and `ItemKey` are interchangeable to the compiler and
could be swapped (the `add_items(session, items)` calls with `(String,String)`
vs key mixes are the risk). A strict newtype would catch a class of
session-vs-key bugs; it does not just fix the type but also forces every
`HashMap<SessionId,_>`/`VecDeque` to be typed. That's a real correctness win
worth a P1, but be aware it touches every BFF signature.

### Result/error typing
Already noted: `thiserror` is in deps but unused for BFF errors; all worker
errors are `String`. P1.

### Iterators vs manual loops
`ui.rs` and `app.rs` mostly use `iter/filter/find/map` correctly
(`app.rs:850`, `app.rs:920`). The `find().map().unwrap_or` chains that repeat
`self.sessions.iter().find(|s| s.id == session)` several times per function
(`app.rs:925,1187,1192,...`) are the real smell — extract `session_view(&self,
id) -> Option<&SessionView>` / `_mut`, and a guard like `let Some(view) =
self.session_view_mut(session) else { return; }`. This is an ergonomics refactor
repeated ~25 times.

---

## 4. UI skeleton

### Structure
`ui.rs` is immediate-mode and conventional: `draw` (`ui.rs:430`) composes
`menu/status_bar/bottom_panels/navigation/table_panel/handle_shortcuts/
status_popup/dialogs`. Interaction is captured via small local action enums
(`TreeAction`, `TableAction`, `ColumnDir`, `ui.rs:20-51`) then applied to `App`.
That split (defer mutation through a local enum) is already a light
command/reducer and is the *right* instinct. It is not carried all the way:
only tree/table use it; dialogs mutate `App` directly.

- Rendering vs mutation: `draw(app, ui)` takes `&mut App` for everything, so
  render and mutation are fused by signature. The action-enum pattern proves the
  seam exists but is inconsistently used.
- i18n access: heavy but consistent `app.lang()` + `tr(lang, Tr::X)` everywhere;
  the `Tr` enum is exhaustive per language (`i18n.rs:71,585,991,1397`), which is
  excellent — a compile-checked dictionary. Recommend *entry points take `lang`
  already* (they do) and avoid `app.lang()` inside deep draw helpers to keep
  them unit-testable.
- Hardcoded sizes: centralized in `constants.rs` (good), but `columns.rs:95-107`
  hardcodes width per `ColumnId` outside `constants.rs`, and `app.rs:1234`,
  `ui.rs` have a few stray `20.0`/`140.0` (e.g. `app.rs:1234` `"conn lost"` is
  not a size; the columns widths are the notable exception).
- DnD/context-menu: the tree->table drag uses a `TreeDragPayload`
  (`ui.rs:57`) and the test hooks (`ui.rs:72-249`) are global `OnceLock<Mutex<..>>`
  rect recorders. Layering is fine; the globals are only `#[cfg(test)]` and scoped
  to ui, so no leak into production.

### The big UI question: `draw(app)` vs view-model/reducer
Current: one god-draw with `&mut App`, ~2000 of the 4538 lines are draw+dialog
code, ~500 are the test hooks + helpers, ~1500 are dialogs with inline mutation.
Testability today is achieved through the kittest harness + seeded `App`
(hence public `seed_*` + `on_event_for_test` methods, `app.rs:2519-2615,1057-1078`).

Smallest high-value change (do NOT introduce a full view-model):
1. **Extract pure render helpers** that take (`&App` or `&SessionView`, `&ui`)
   and return data, leaving mutation to `&mut App` command methods. The action
   enums already point the way — finish the job: `table_panel` collects
   `Vec<TableAction>` and a single `apply_table_actions(app, actions)` at the
   end of the frame. This makes the render *idempotent* and testable without
   kittest for the pure parts.
2. **Split `app.rs` by concern** (see section 8): move dialog structs into a
   `view/` module, keep `App` as model+commands. This is the single biggest
   readability win and is a pure file move.
3. Keep immediate-mode. A separate view-model struct + intermediate tree would
   fight egui and be net-negative.

---

## 5. Concurrency skeleton

- UI thread: `eframe` main thread; `update` = poll + tick + draw, repaints every
  200 ms via `request_repaint_after` (`app.rs:716`). No blocking work on UI
  thread — good.
- DA worker: one thread, `CoInitializeEx(MULTITHREADED)` (`da.rs:37`),
  `blocking_recv` loop (`da.rs:74`), unbounded channel. COM objects are confined
  to that thread. Correct confinement (single MTA thread), correct because OPC
  DA 2.0 servers are free-threaded. No backpressure — an unbounded queue can
  grow if the app issues reads faster than the server answers. In practice the
  app polls at >=1 s and the queue drains; acceptable, but see P1.
- UA worker: one thread, own `current_thread` tokio runtime (`ua.rs:200`),
  `block_on` loop (`ua.rs:217`). Command priority re-order (`ua.rs:248`). The
  tokio runtime is not shared with anything else, so `current_thread` is right.
- Events: single `std::sync::mpsc::channel` from *both* workers. Both workers
  may `send` concurrently; `mpsc` is multi-sender/single-consumer, so this is
  correct. No capacity bound (unbounded) and `try_recv` drains per frame.
- Request IDs: allocated in `App::send` (`app.rs:720`), carried in `Command`,
  carried back in `Event` but **not correlated** (`model.rs:474` dead). This is
  the key unused affordance.
- Backpressure: absent at both the command channel and the event channel. For a
  low-rate HMI client this is acceptable; the risk is a loud server spamming
  DataChange events into the unbounded event queue faster than the 200 ms frame
  drain.
- Graceful shutdown — **FIXED:** UI thread sends `Disconnect` on exit and then
  `Gateway::shutdown()` joins both workers (`app.rs:3137`, `bff/mod.rs:107`).
  UA keeps `GRACEFUL_CLOSE_TIMEOUT`; DA now also tears down its groups when the
  command channel closes. Worker handles are stored in the gateway (not detached
  anonymously).
- UA request sizing / timeout policy (2026-10-25 note): a single CODESYS Browse
  or Read response can exceed the server's `MaxChunkCount = 5` (`0x80800000`),
  and a timed-out request must never be abandoned on a channel that stays open
  (`0x80310000`). First-party code now uses named caps
  `BROWSE_MAX_REFERENCES_PER_NODE = 100` (Browse/BrowseViews; continuation loop
  still yields all nodes) and `READ_MAX_ITEMS_PER_CALL = 100` (chunked
  `session.read` for Read, AddItems attribute probing, Test and RemoveItems
  batching), plus `DISCOVERY_GUARD_TIMEOUT = 20 s` behind the discovery client's
  own `UA_SEARCH_TIMEOUT` request deadline. Session requests run through
  `timeout_drain`: after `IO_TIMEOUT` the same request future keeps being polled
  for up to `GRACEFUL_CLOSE_TIMEOUT` so an in-flight response is drained, then
  the worker marks the session poisoned, removes it, performs the graceful
  `Session::disconnect()` (CloseSession/CloseSecureChannel) and only then aborts
  as a last resort; it emits `Error` + `Disconnected` instead of reusing the
  session. `request_id` correlation is now implemented (§10); the timeout path
  emits exactly one correlated reply per command and unsolicited follow-ups
  (`Disconnected`/diagnostic `Log`) with id 0.

### Idiomatic target (minimal diff)
Keep the two-worker + one event-receiver shape. Improve with:
- **DONE:** `Gateway::send` returns a typed error and `App` logs it.
- **DONE:** `Event.request_id` is matched against `pending_requests` so
  `on_event` rejects stale/late non-zero replies.
- Add a bounded capacity to the *per-session command* channel only if profiling
  shows it grows; otherwise document the unbounded choice. Do not invent
  backpressure machinery for a rate that is bounded by the 1..60 s poll tick.
- Make worker handles joinable (keep `JoinHandle`s in `Gateway`) and have
  `on_exit` wait (bounded) — closes the drop-ordering gap for DA (P1).

---

## 6. Testing skeleton

Three tiers are present and healthy:
1. Pure unit (`#[cfg(test)]` in `settings/columns/journal/model/ua`) — no I/O for
   the pure functions, with the notable exception of settings which needs an
   `APPDATA` redirect. The current workaround (`with_appdata`, `settings.rs:236`,
   and the `SETTINGS_LOCK` in `tests/ui_interactions.rs:13`) is the least-idiomatic
   seam: it mutates process-global env vars and requires an unsafe block and a
   global mutex to serialize. Better: inject a `settings_path` via the `Settings`
   constructor (add `Settings::load_from(path)`/`save_to(path)`) and keep the
   global `APPDATA` behavior as the default. That removes the unsafe + the lock,
   and matches "temporary settings dirs" requirement without a trait.
2. kittest headless UI (`tests/ui_interactions.rs`, `ui.rs` hooks) — excellent
   approach for egui. The global rect hooks are the one oddity; they are
   `#[cfg(test)]`-scoped, so leakage is limited, but a better seam is to thread a
   `&mut UiTestProbe` out of `draw` (returned via `&App.test_out`) instead of
   global Mutexes. That is a larger change; the globals are an acceptable
   intermediate. Recommend keeping the globals but centralizing them (they
   already are, under `ui::*_hook`).
3. Live integration (`tests/ua_*.rs`, `--ignored`) — correct to gate behind an
   actual server.

Test-hook leakage assessment:
- `App` exposes `new_test`, `on_event_for_test(_at)`, `tick_for_test`,
  `activate_session`, `seed_*` (`app.rs:645,1057-1078,2519-2615`). These are
  production `pub fn`, not `#[cfg(test)]`, so they ship. The seed methods
  especially are test-only but compiled into the binary. Recommend moving them
  behind `#[cfg(test)]` (they are unused in prod) or at minimum document them as
  debug/test surface. This is a concrete, cheap P1 cleanup.
- `ui::*_for_test` / hooks are correctly `#[cfg(test)]` already.

Recommended seam principles (no over-abstracting):
- Prefer *constructor-time injection* (paths, clocks) over env-var mutation and
  over traits. `UpdateInterval`/delay already take `Instant`/`Duration` from
  const sources; inject a `now: Instant` stub for `tick` tests instead of a clock
  trait (the code already takes `now` implicitly via `Instant::now()` — a small
  `fn tick_at(&mut self, now: Instant)` would centralize it).
- Keep tests at three levels and do not force traits across worker boundaries.

---

## 7. Findings / recommendations map

### P0 — correctness / structure
- **Dead `request_id` correlation — FIXED.** `Event.request_id` is no longer
  dead. `App::send` records `(SessionId, RequestId)` in `pending_requests`;
  `App::on_event` accepts a non-zero reply only while that pair is outstanding
  and removes it on receipt. Unknown/replayed ids are dropped before any state
  mutation; `0` is reserved for pushes/runtime events. Lifecycle resets
  (Disconnect/Connect/reconnect/send failure) clear the session's outstanding
  ids. Tests: `stale_reply_with_unknown_request_id_is_ignored`,
  `matching_request_id_reply_is_accepted`,
  `push_event_with_zero_request_id_is_accepted`,
  `disconnect_connect_does_not_let_old_connected_mutate_new_state`
  (`tests/ui_interactions.rs`).
- **Reconnect 5-boolean cluster — FIXED.** Replaced by
  `ReconnectState { Idle, Waiting { at, attempt }, Stopped, Exhausted }` plus
  the shared `RECONNECT_MAX_ATTEMPTS` (20). Semantics: link loss schedules
  attempt 1 (1 s), failures schedule 2/4/8…, attempt 20 failing → `Exhausted`,
  manual stop → `Stopped`, resume/manual reconnect → `Idle` (and, for resume,
  a fresh `Waiting`), successful `Connected` → `Idle`, manual disconnect →
  `Stopped`. UI hint, server-row counter/stop/resume and popup read the enum.
- **Worker lifecycle not joinable — FIXED.** `Gateway` stores `WorkerHandle`s
  (command sender + `JoinHandle` + done receiver); `Gateway::shutdown()`
  closes the channels and joins with a 10 s bound. `on_exit` calls it. DA
  performs `RemoveGroup` when its loop ends; UA keeps its graceful close.
  Tests: `tests/gateway_shutdown.rs` and `on_exit_shuts_down_the_gateway`.
- **Event attribution for `add_children` — FIXED.** `advance_add_children`
  now keys in-flight pages by `RequestId` → `node_id`
  (`in_flight: HashMap<RequestId, String>`); only the exact reply the job
  issued advances it. Test:
  `add_children_ignores_unrelated_browse_request_id` (`src/app.rs`).
- **Subscription/polling detached bool — FIXED.** `subscription_active: bool`
  became `SubscriptionState { Inactive, Active, Failed }` in `SessionView` and
  `EventKind::SubscriptionState { state }`. Only `Active` suppresses polling;
  `Failed` and `Inactive` poll. The server-info popup shows the state. UA emits
  `Active` on successful monitoring, `Failed` on monitor failure, `Inactive` on
  teardown/no items.

### P1 — elegance / testability
- **Typed errors — operation identity FIXED; full `BffError` remains.**
   `EventKind::Error` now carries `Operation` (`model.rs:15,581`), and the app
   dispatch/labels are exhaustive matches (`app.rs:1500,2993`). The worker
   internals still use `Result<T, String>` (a typed `BffError` is a separate
   refactor); `operation == op::CONNECT` string matching is gone, guarded by
   `tests/operation_guard.rs`. **Security selection is now typed (2026-10-26)**:
   `UaSecurityPolicy`/`UaSecurityMode` replace the free strings in the dialog,
   settings and `EndpointSpec`; `resolve_security` returns `Result` and errors
   instead of falling back. See §11.
- **`Operation` enum instead of `str` op-codes — FIXED.** `model.rs:15-81` has
  the enum, `as_str()` for logs, `Display`, and `Operation::ALL`; the old `op`
  constant module is removed. Unit tests pin ASCII/unique tokens and the
  command→operation mapping (`model.rs:589,610`).
- **Newtype IDs.** `SessionId`/`ItemKey`/`RequestId` (`model.rs:4-6`) →
  `#[repr(transparent)]` newtypes so `session` and `key` are not interchangeable.
  Highest-churn, highest-reward for a protocol-heavy codebase. Do only if you can
  absorb the signature sweep.
- **Seed/test hooks gating.** `seed_*`, `on_event_for_test_*`, `tick_for_test`
  (`app.rs:1057,2519-2615`) are prod-public. Move behind `#[cfg(test)]` (or an
  `#[cfg(feature = "test-hooks")]`).
- **Settings path injection.** `Settings::load/save` read `APPDATA` at call time
  (`settings.rs:110-137,214-223`). Add `load_from(path)/save_to(path)` and a
  `settings_path()` default overload; delete `with_appdata` + `SETTINGS_LOCK`.
- **`SessionView` accessors.** Add `session_view(&self, id)` /
  `session_view_mut` and use `let Some(view) = ... else { return; }`; removes the
  ~25 repeated `find().unwrap_or` chains (`app.rs:925,1187,1192,1375,...`).
- **Split dialogs out of `app.rs`.** Move `ConnectDialog/AddSignalDialog/WriteDialog/
  ColumnsDialog/OptionsDialog/AboutDialog/MessageDialog/ConfirmDialog/Dialogs`
  (`app.rs:360-590`) into `src/view/dialogs.rs`. Pure file move; `ui.rs` already
  namespaces them. This directly addresses "app.rs god-object" and mirrors the
  `ui`->`app` dependency.

### P2 — nice-to-have
- Extract `compose_server_name` + URL helpers into `view/names.rs` (`app.rs:182-275`),
  `Row::write_type_text/timestamp_text` render helpers into `view/row.rs`
  (`app.rs:294-314`), and remove the Russian string from `Value: Display`
  (`model.rs:337`) by pushing the rendered form into the view.
- Move the `pending_writes/write_total/.../last_write_summary` counter set
  (`app.rs:615-621`) into a small `WriteProgress` struct owned by `App` but
  updated only by `on_event`.
- **FIXED:** `Gateway::send` returns `Result` so a dead worker is not silent
  (`mod.rs:87`); `App::send` journals it and marks the protocol's sessions dead
  (`app.rs:720`).
- Move column widths into `constants.rs` (or keep in `columns.rs` but reference
  named constants); they are currently literal `f32` in `ColumnId::width`
  (`columns.rs:95-107`).
- **DONE:** request-id correlation exists, and `add_children` now has an
  attribution test (`add_children_ignores_unrelated_browse_request_id`).

### Where GoF is justified vs over-engineering
Justified (already present and right): Command (message envelope),
Facade (`Gateway`), Adapter (`da_variant`), light reducer (TreeAction/TableAction)
where used. **DONE:** State (reconnect enum).
Over-engineering in Rust (do NOT): Strategy/`Box<dyn Backend>` over the two
protocols, a full actor framework, a Builder for `EndpointSpec`, an EventBus,
Repository, DI container. These violate doctrine 3.1 (minimal mechanisms) — the
enum + exhaustive match already gives compiler-checked dispatch.

---

## 8. Proposed target skeleton (preserves behavior)

```
src/
  app.rs            model root: App + SessionView + Row + AddChildrenJob +
                    commands + on_event/tick (no dialog structs, no render helpers)
  view/
    mod.rs          pub use draw; draw(app, ui) keeps its current signature
    dialogs.rs      all *Dialog structs + Dialogs (moved from app.rs)
    names.rs        compose_server_name + URL/name helpers
    row.rs          Row render helpers (write_type_text/timestamp_text)
  ui.rs             immediate-mode draw: menu/panels/tree/table/dnd/shortcuts
  bff/
    mod.rs          Gateway (adds Option<JoinHandle>s + shutdown(); send->Result)
    model.rs        Operation enum; typed BffError; newtype IDs
    da.rs, ua.rs    keep single-worker + event emission; return typed errors
    da_variant.rs   unchanged (pure COM<->Value adapter)
  settings.rs       add load_from/save_to + default APPDATA overload
  columns.rs / journal.rs / i18n.rs / constants.rs   unchanged-module seams
```

Ownership unchanged: `App` remains the single owner of model + selection; workers
own their sessions; `Gateway` owns the channels and (new) join handles. The
changes are (a) move/grouping, (b) `request_id` correlation — **done**,
(c) reconnect enum — **done**, (d) typed errors + newtype IDs — `Operation`
done, `BffError`/newtypes open, (e) path-injectable settings, (f) gate test
hooks. No behavior change.

---

## 9. Remediation status — typed operations + worker lifecycle (2026-10-25)

Fixed:

- **Typed operations** (P1): `Operation` enum + `as_str` (`bff/model.rs:15-81`);
  `EventKind::Error { operation: Operation }` (`bff/model.rs:581`); exhaustive
  dispatch (`app.rs:1500-1567`) and labels (`app.rs:2993-3011`); new i18n labels
  (`i18n.rs:444-448` + ru/en/zh arms); static guard
  (`tests/operation_guard.rs:38`).
- **Worker lifecycle / graceful shutdown** (P0): `WorkerHandle` + bounded
  `Gateway::shutdown()` (`bff/mod.rs:15,42,107`), `send -> Result`
  (`bff/mod.rs:87`), worker-side teardown (`bff/ua.rs:243`, `bff/da.rs:78`),
  `App::on_exit` waits (`app.rs:3137`). Tests: `tests/gateway_shutdown.rs`,
  `on_exit_shuts_down_the_gateway`.
- **Duplicated discovery builder** (doctrine 3.3): `search_servers` and
  `discover_endpoints` both use `build_discovery_client()`
  (`bff/ua.rs:1038,1148,1197,1391`). The connect builder remains separate because
  it has different security/retry/session-timeout semantics.

Still open (not part of this pass): newtype IDs, typed `BffError`, settings
path injection, test-hook gating, session-view accessors, dialog extraction,
DA no-op `ConfigureUaSubscription`, CSV heuristics, `GROUP_UPDATE_RATE_MS`,
dual column-order store, per-value sequence/timestamp dedup for DataChange
pushes. **Security policy/mode strings are fixed (see §11).**

## 10. Remediation status — request correlation / reconnect / subscription (2026-10-25)

Fixed in this pass (behavior-preserving apart from the added guards):

- **Request-id correlation** (P0): `App::pending_requests:
  HashSet<(SessionId, RequestId)>`; insert in `send`, remove on receipt,
  reject unknown/replayed non-zero replies, accept `0` pushes/runtime events;
  clear per session on Disconnect/Connect/reconnect/send failure. Producers
  emit exactly one correlated reply per command; ancillary timeout/diagnostic
  events use `0`.
- **Reconnect state enum** (P0): `ReconnectState` replaces the five fields;
  cap stays `RECONNECT_MAX_ATTEMPTS`. Server-row icons, status hint and popup
  are derived from the enum.
- **Subscription state enum** (P0): `SubscriptionState` replaces the detached
  bool in `SessionView` and the event; `Active` is the only polling-suppressing
  state; `Failed`/`Inactive` poll. Popup surfaces the state with new
  ru/en/zh labels.
- **add-children attribution**: `in_flight: HashMap<RequestId, String>`; no
  `pending` counter guessing.

Verification: `cargo test --lib` 148 ✅, `cargo test --test ui_interactions`
84 ✅, all non-ignored integration tests ✅, live `ua_*` ✅
(`ua_smoke`/`ua_write`/`ua_search`/`ua_types`/`ua_security`/`ua_subscription`/
`ua_refresh_speed`/`ua_lifecycle`/`ua_large`), `cargo build --bin opcda-egui`
clean.

## 11. Remediation status — typed UA security (2026-10-26)

Fixed:

- **Typed security selection** (doctrine 3.5): `UaSecurityPolicy { Auto, None,
  Basic128Rsa15, Basic256, Basic256Sha256 }` and `UaSecurityMode { Auto, None,
  Sign, SignAndEncrypt }` in `bff/model.rs`; `EndpointSpec`, `Settings` and
  `ConnectDialog` carry the enums. Legacy `"Авто"`/`"Auto"`/`"None"`/short
  names/full URIs migrate through the enum deserializer; unknown values are
  serde errors, missing fields default to `Auto`.
- **Strict resolution**: `resolve_security(policy, mode, endpoints) ->
  Result<(String, MessageSecurityMode), String>` keeps the Auto preference
  order but requires explicit choices to be advertised; invalid combinations
  and unknown advertised values are errors. Worker failures carry
  `SECURITY_ERROR_PREFIX`; the app shows a localized wrapper plus the technical
  detail.
- **UI**: combos iterate the typed enums with exhaustive localized labels;
  a selection is persisted immediately and validated, and an invalid or
  unadvertised choice blocks the connect while the dialog stays open.

Verification: `cargo test --lib` 162 ✅, `cargo test --test ui_interactions`
87 ✅, all non-ignored integration tests ✅, live `ua_*` + `security_live` ✅,
`cargo build --bin opcda-egui` clean without warnings.

Remaining backlog (outside this pass): typed `BffError`, newtype IDs, settings
path injection, test-hook gating, dialog extraction, dual column-order store,
CSV heuristics (header/session matching), DA `ConfigureUaSubscription` no-op,
`GROUP_UPDATE_RATE_MS` duplication.

## Open questions

1. Is live concurrency (the push-vs-poll interleave on UA values) a real,
   observed bug or only a latent risk? **Answered:** latent risk; the reply
   correlation guard now rejects superseded command replies, and DataChange
   pushes remain last-writer-wins (benign, no observed corruption).
2. Which net-new sessions-per-host limit (if any) is desired? `sessions` is a
   flat `Vec` today; a `MAX_CONCURRENT_SESSIONS` cap would be a `connect_endpoint`
   guard, but I saw no requirement for one.
3. Do the DA COM workers ever need a real backoff on a stuck `blocking_recv`
   (e.g. an OPC server that stops answering)? Today a hung server stalls only
   that session's commands, not the UI. Confirm that's acceptable.
4. Are the `seed_*`/`on_event_for_test_*` methods relied upon by any non-test
   path (script host / probe bins)? `src/bin/probe_*` import `Gateway`, not
   `App::seed_*`, so gating them should be safe — but confirm against the script
   host in `src/opc`.
5. The `ui`/`opc`/empty-`src/ui/` directory and `src/opc/mod.rs` were skimmed,
   not audited in depth. Confirm whether `opc/bindings` is fully statically
   generated or hand-written (affects the "minimal mechanisms" verdict for COM).
6. `SessionView.visible` + `state` two-axis lifecycle: is a disconnected-start
   (visible but never connected, e.g. restored after `remove_session`? there is
   no path that restores an invisible container to connected) intentional, or is
   there a latent "tab open but nothing shown" state worth locking down?
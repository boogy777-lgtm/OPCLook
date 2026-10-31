# P3/P5 embedded controls and local component audit

Scope: local egui widgets and input routing. This is a bounded source review,
not a claim of complete keyboard, accessibility, or native Windows IME coverage.
The actual GUI tests and screenshots for this revision require CI execution and
visual inspection; local Cargo resolution lacks the cached GUI dependencies.

## Findings and changes

- `dialogs/columns.rs` previously closed the dialog on every Enter after
  rendering its controls. Enter could therefore activate a focused move or
  reset button and close the dialog in the same frame. The default Enter now
  applies only when none of the dialog's checkbox/buttons owns focus; the
  explicit Apply button remains authoritative. Enter and Escape use the
  one-press IME guard, and repeat key events cannot repeatedly activate the
  move, reset or apply buttons. The interaction regression focuses a real down
  control, presses Enter, checks its single reorder and that the dialog remains
  open, then applies explicitly. Baseline failure is a source inference until
  the before/after test runs on CI.
- Add Signal, Network and UA security ComboBox choices did not use the
  existing Write type choice's IME rollback. `components::selectable_value`
  shares that behavior: a keyboard-generated candidate cannot mutate the
  chosen value; an explicit pointer selection can. The enclosing egui popup
  retains ownership of open/close behavior. In particular, keeping the popup
  visibly open after a composition candidate has **not** been established.
- The Options and Connect trust checkboxes now share the existing Options
  IME-safe checkbox behavior. A subdued hint label is extracted where the same
  rendering occurred in Options and Network. These helpers render local UI and
  return `egui::Response`; they do not own application actions, persistence,
  session state, availability, or backend operations. Existing button-with-
  reason remains separate: hover/disabled reasons differ from hint labels.

## Reviewed boundaries and remaining work

| Surface | Evidence and boundary |
| --- | --- |
| Text entry | Global shortcuts are blocked by focused text fields, dialogs and IME; footer editor has intentional region-navigation handling. Tests cover representative write, add and P4 typed-palette fields, not every embedded field or platform input sequence. |
| ComboBox | Write type, Add Signal session, Network adapter/address and Connect security use the shared guarded selection; theme choices have their existing modal/menu input guard. Popup lifecycle and all mixed pointer/key frames need further end-to-end tests. |
| Checkbox and columns | Options and Connect trust use the shared guard. Columns already checks IME before committing checkbox changes; focused column controls now take precedence over default Enter. |
| Numeric controls | Slider/DragValue in Options, Connect and acquisition remain native egui controls; all keyboard, focus and IME combinations have not been individually tested. |
| Context/regions | Existing keymap overlap tests, modal priority, region navigation and P4 palette checks remain in place. Popup focus, shortcut repeat and control interaction across every region still need broader review. |
| P5 candidates | Menu/action button, painter icon button, table cell, row/tree and popup code have different accessibility, hover, geometry, or owner semantics. They were not merged solely for superficial similarity. A broader component inventory and visual/accessibility review remain open. |

Physical Windows IME, real layouts, screen readers, native Windows input and
external servers are outside these synthetic tests. No change here enables
production DA or changes Engine, command routing, localization, or SDK behavior.

## Exact CI checks and visual evidence

```sh
cargo test --locked -p opclook --lib ui::components::tests -- --nocapture
cargo test --locked -p opclook --test ui_interactions p3_columns_focused_control_owns_enter_before_dialog_apply -- --nocapture
cargo test --locked -p opclook --test ui_interactions p3_ime_candidate_does_not_switch_add_signal_session -- --nocapture
cargo test --locked --target x86_64-unknown-linux-gnu --features ui-snapshots --test ui_snapshots p3_columns_focused_control_ru -- --ignored --nocapture
cargo test --locked --target x86_64-unknown-linux-gnu --features ui-snapshots --test ui_snapshots p5_options_hints_ru -- --ignored --nocapture
```

The repository also has a synthetic P2 `p2_action_payload_error_ru` snapshot;
it renders the real localized `ActionPayloadInvalid` status, while actual
command rejection is covered by separate App tests. Run it with the same
`ui_snapshots` command and inspect `docs/screenshots/p2-action-payload-error-ru.png`.

The snapshot tests need the existing headless GPU/renderer setup; inspect
`docs/screenshots/p3-columns-focus-ru.png` and
`docs/screenshots/p5-options-hints-ru.png` after CI. Passing synthetic tests
and viewing these images would not close the native IME/layout gates.

## Numeric controls and remaining component inventory

The exact source review covers the local `src/ui` tree as follows. “Retain”
means the similar-looking code has a distinct owner or interaction contract;
it is not an assertion about platform accessibility.

| Paths | Repetition or responsibility | Owner and decision |
| --- | --- | --- |
| `components.rs`, `actions.rs`, `dialogs/write.rs` | Disabled button/reason, checkbox, selectable value, supporting hint | Keep the existing local `Response` helpers; action availability and translation remain with the caller. |
| `dialogs/options.rs`, `dialogs/connect/security.rs`, `dialogs/connect.rs`, `dialogs/acquisition.rs` | Slider and integer/float DragValue; explicit profile and readback checkboxes | Share the small `numeric` IME guard and guarded checkbox. The caller supplies range, speed, value and layout; the helper sends no actions. |
| `dialogs/add_signal.rs`, `dialogs/write.rs`, `dialogs/connect/security.rs`, `dialogs/network.rs` | ComboBox choices | Retain the shared selection primitive; each dialog owns the selected value and popup. Do not merge their validation or focus transitions. |
| `dialogs/confirm.rs`, `message.rs`, `csv_import.rs`, `mod.rs` | Modal choice and routing | Retain independently: confirm, message and import have different terminal actions and modality. |
| `dialogs/palette.rs`, `add_signal.rs`, `write.rs`, `connect.rs` | Search/form text fields and Enter | Retain their separate field and Enter logic: typed parameters, connect, Add and Write have different effects and confirmation rules. |
| `dialogs/columns.rs`, `ua_tree.rs`, `connect/results.rs` | Reorder, grouped endpoint rows and result lists | Retain: focus/drag/selection and row payload semantics differ. |
| `dialogs/about.rs`, `network.rs`, `options.rs`, `acquisition.rs` | Informational labels and status | Existing `hint` handles matching subdued text; warnings, semantic colors and state-dependent details remain local. |
| `navigation.rs`, `navigation/tree_rows.rs`, `tree.rs`, `servers.rs` | Server list, hierarchical tree and tabs | Retain: region focus, expansion, session identity and close/selection ownership differ; painter geometry is not a shared generic row contract. |
| `table.rs`, `table/cells.rs` | Table cells, multiselection, structured values and time | Retain existing local cell models; table focus, selection and value rendering have different response semantics from tree rows. |
| `paint.rs`, `colors.rs`, `style.rs` | Icons, token palette and theme | Retain existing painter/style helpers: icon accessibility, enabled state and painting callbacks differ from ordinary buttons. |
| `menu.rs`, `panels.rs`, `status.rs` | Menus, footer widgets, journal/script and status popup | Retain: region navigation, popup stacking and editor focus are distinct; shared button logic would own UI state. |
| `keymap.rs`, `shortcuts.rs`, `regions.rs`, `mod.rs` | Keyboard routing, IME state and region ownership | Keep these as routing infrastructure, not presentation widgets. |
| `csv.rs`, `files.rs`, `test_hooks.rs` | CSV/file operations and test hooks | Not render primitives; keep effects and test instrumentation separate. |

The `numeric` helper snapshots whether an IME candidate arrow is present
*before* egui consumes it and restores the prior value after rendering. It
leaves committed text and pointer interaction intact when no simultaneous IME
arrow is present; an ambiguous same-frame pointer plus candidate-arrow change
is conservatively discarded. The raw `Response::changed` still describes the
event observed by egui; callers compare the final value rather than treating
that flag as a domain action. Existing ranges and float speed are unchanged.
Integer acquisition interval, UA session timeout and integer Options
DragValues now use speed 1.0 instead of egui's default 0.25: a keyboard arrow
can change an integer by one, and pointer dragging correspondingly changes at
one unit per point rather than a quarter. Slider styling is unchanged.

Local Linux `egui_kittest` measurement on this revision: all 39 `p3_` UI
interaction tests and all 4 local component tests passed. The acquisition
interval normal arrow, text entry, synthetic IME committed text, held repeat,
pointer drag during IME, IME arrow, explicit-profile
checkbox, Connect timeout and Options slider/deadband scenarios are included. A targeted P4 IME/repeat
test also passed.
A controlled before/after of the explicit-profile checkbox ran the actual test:
raw checkbox failed by losing its draft, guarded checkbox passed. For the
integer IME arrow, a counterfactual with the approved speed 1.0 but without the
IME guard changed 1000 to 1001; the guard restored 1000. The default 0.25
integer control also left ordinary arrow changes rounded back to the original
integer in this test. These are synthetic input results, not physical Windows
IME evidence. The previous columns focus-guard counterfactual was also run:
the named assertion failed with the guard disabled and passed after restoration.

Remaining checks: integrate and rerun the full UI suite on the final combined
head, observe actual native Windows layout/IME and accessibility behavior,
and inspect the separately requested visual snapshots. No new drawing style or
component layout is introduced by this numeric change.

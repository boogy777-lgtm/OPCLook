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

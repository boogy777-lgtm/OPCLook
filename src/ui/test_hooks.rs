// Headless DnD proof: record the last drawn tree-row rect and the tags-area
// (drop zone) rect so a test can drive RawInput drag events precisely. Only
// compiled in test builds.
#[cfg(test)]
mod dnd_test_hook {
    use std::sync::Mutex;
    use std::sync::OnceLock;

    // Both rects live under one lock so a test can snapshot tree+tags together
    // (parallel tests never interleave between the two reads).
    static BOTH: OnceLock<Mutex<(Option<egui::Rect>, Option<egui::Rect>)>> = OnceLock::new();

    fn both_cell() -> &'static Mutex<(Option<egui::Rect>, Option<egui::Rect>)> {
        BOTH.get_or_init(|| Mutex::new((None, None)))
    }

    pub fn set_tree_row(rect: egui::Rect) {
        both_cell().lock().unwrap().0 = Some(rect);
    }
    pub fn set_tags(rect: egui::Rect) {
        both_cell().lock().unwrap().1 = Some(rect);
    }
    /// Snapshot (tree, tags) together; both reflect the same draw unless one of
    /// the setters hasn't run for the current frame.
    pub fn snapshot() -> (Option<egui::Rect>, Option<egui::Rect>) {
        *both_cell().lock().unwrap()
    }
    pub fn tree_row() -> Option<egui::Rect> {
        both_cell().lock().unwrap().0
    }
    pub fn tags() -> Option<egui::Rect> {
        both_cell().lock().unwrap().1
    }
}

#[cfg(test)]
pub fn set_tree_row_rect_for_test(rect: egui::Rect) {
    dnd_test_hook::set_tree_row(rect);
}

#[cfg(test)]
pub fn set_tags_rect_for_test(rect: egui::Rect) {
    dnd_test_hook::set_tags(rect);
}

/// The last drawn tree-row rect (test hook for the headless DnD proof).
#[cfg(test)]
pub fn tree_row_rect_for_test() -> Option<egui::Rect> {
    dnd_test_hook::tree_row()
}

/// The tags-area / drop-zone rect (test hook for the headless DnD proof).
#[cfg(test)]
pub fn tags_rect_for_test() -> Option<egui::Rect> {
    dnd_test_hook::tags()
}

/// Snapshot of the last drawn (tree-row, tags-area) rect pair, consistent with
/// a single frame even when parallel tests share the hooks.
#[cfg(test)]
pub fn dnd_rects_snapshot() -> (Option<egui::Rect>, Option<egui::Rect>) {
    dnd_test_hook::snapshot()
}

/// Last frame's table scroll info: (offset, content_size, inner_rect).
#[cfg(test)]
pub fn table_scroll_for_test() -> Option<(egui::Vec2, egui::Vec2, egui::Rect)> {
    ui_test_hook::table_scroll()
}

/// Last drawn per-session server-row rects, for the single-line-height assert.
#[cfg(test)]
pub fn server_rows_for_test() -> Vec<(crate::bff::model::SessionId, egui::Rect)> {
    ui_test_hook::server_rows()
}

/// Clear the per-frame UI test hooks so a fresh render starts clean.
#[cfg(test)]
pub fn reset_ui_test_hooks() {
    ui_test_hook::reset();
}

// Test hook for the columns-dialog DnD: records the grip rect for each column
// and the insertion-gap rect by index so the headless RawInput drag proof can
// drive a grip -> gap drag precisely.
#[cfg(test)]
mod columns_hook {
    use crate::columns::ColumnId;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    static GRIPS: OnceLock<Mutex<HashMap<ColumnId, egui::Rect>>> = OnceLock::new();
    static GAPS: OnceLock<Mutex<HashMap<usize, egui::Rect>>> = OnceLock::new();
    static ROWS: OnceLock<Mutex<HashMap<ColumnId, egui::Rect>>> = OnceLock::new();

    fn grips() -> &'static Mutex<HashMap<ColumnId, egui::Rect>> {
        GRIPS.get_or_init(|| Mutex::new(HashMap::new()))
    }
    fn gaps() -> &'static Mutex<HashMap<usize, egui::Rect>> {
        GAPS.get_or_init(|| Mutex::new(HashMap::new()))
    }
    fn rows() -> &'static Mutex<HashMap<ColumnId, egui::Rect>> {
        ROWS.get_or_init(|| Mutex::new(HashMap::new()))
    }
    pub fn set_row(column: ColumnId, rect: egui::Rect) {
        rows().lock().unwrap().insert(column, rect);
    }
    pub fn row(column: ColumnId) -> Option<egui::Rect> {
        rows().lock().unwrap().get(&column).copied()
    }
    pub fn set_grip(column: ColumnId, rect: egui::Rect) {
        grips().lock().unwrap().insert(column, rect);
    }
    pub fn set_gap(index: usize, rect: egui::Rect) {
        gaps().lock().unwrap().insert(index, rect);
    }
    pub fn grip(column: ColumnId) -> Option<egui::Rect> {
        grips().lock().unwrap().get(&column).copied()
    }
    pub fn gap(index: usize) -> Option<egui::Rect> {
        gaps().lock().unwrap().get(&index).copied()
    }
    pub fn clear() {
        grips().lock().unwrap().clear();
        gaps().lock().unwrap().clear();
        rows().lock().unwrap().clear();
    }
}

#[cfg(test)]
pub(super) fn record_column_grip(column: crate::columns::ColumnId, rect: egui::Rect) {
    columns_hook::set_grip(column, rect);
}

#[cfg(test)]
pub(super) fn record_column_gap(index: usize, rect: egui::Rect) {
    columns_hook::set_gap(index, rect);
}

#[cfg(test)]
pub(super) fn record_column_row(column: crate::columns::ColumnId, rect: egui::Rect) {
    columns_hook::set_row(column, rect);
}

#[cfg(test)]
pub mod columns_hook_api {
    pub fn grip(c: crate::columns::ColumnId) -> Option<egui::Rect> {
        super::columns_hook::grip(c)
    }
    pub fn gap(i: usize) -> Option<egui::Rect> {
        super::columns_hook::gap(i)
    }
    pub fn row(c: crate::columns::ColumnId) -> Option<egui::Rect> {
        super::columns_hook::row(c)
    }
    pub fn clear() {
        super::columns_hook::clear();
    }
}

// Header cells are captured independently of dialog rows for real-pointer
// reorder tests (the table and dialog may both be present in one frame).
#[cfg(test)]
mod header_hook {
    use crate::columns::ColumnId;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CELLS: OnceLock<Mutex<HashMap<ColumnId, egui::Rect>>> = OnceLock::new();

    fn cells() -> &'static Mutex<HashMap<ColumnId, egui::Rect>> {
        CELLS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn record(id: ColumnId, rect: egui::Rect) {
        cells().lock().unwrap().insert(id, rect);
    }

    pub fn get(id: ColumnId) -> Option<egui::Rect> {
        cells().lock().unwrap().get(&id).copied()
    }
}

#[cfg(test)]
pub(super) fn record_header_cell(column: crate::columns::ColumnId, rect: egui::Rect) {
    header_hook::record(column, rect);
}

#[cfg(test)]
pub fn header_cell_for_test(column: crate::columns::ColumnId) -> Option<egui::Rect> {
    header_hook::get(column)
}

#[cfg(test)]
mod ui_test_hook {
    use std::sync::Mutex;
    use std::sync::OnceLock;

    /// Last drawn per-container server-row rect (by session id), so headless
    /// tests can assert the row is a single line (height <= 28 px).
    static SERVER_ROWS: OnceLock<Mutex<Vec<(crate::bff::model::SessionId, egui::Rect)>>> =
        OnceLock::new();
    /// Last frame's table scroll info: (offset, content_size, inner_rect).
    static TABLE_SCROLL: OnceLock<Mutex<Option<(egui::Vec2, egui::Vec2, egui::Rect)>>> =
        OnceLock::new();

    fn server_rows_cell() -> &'static Mutex<Vec<(crate::bff::model::SessionId, egui::Rect)>> {
        SERVER_ROWS.get_or_init(|| Mutex::new(Vec::new()))
    }
    fn table_scroll_cell() -> &'static Mutex<Option<(egui::Vec2, egui::Vec2, egui::Rect)>> {
        TABLE_SCROLL.get_or_init(|| Mutex::new(None))
    }

    pub fn push_server_row(id: crate::bff::model::SessionId, rect: egui::Rect) {
        server_rows_cell().lock().unwrap().push((id, rect));
    }
    pub fn server_rows() -> Vec<(crate::bff::model::SessionId, egui::Rect)> {
        server_rows_cell().lock().unwrap().clone()
    }
    pub fn table_scroll() -> Option<(egui::Vec2, egui::Vec2, egui::Rect)> {
        *table_scroll_cell().lock().unwrap()
    }
    pub fn set_table_scroll(data: (egui::Vec2, egui::Vec2, egui::Rect)) {
        *table_scroll_cell().lock().unwrap() = Some(data);
    }

    /// Clear per-frame hooks so a fresh render starts clean.
    pub fn reset() {
        server_rows_cell().lock().unwrap().clear();
        *table_scroll_cell().lock().unwrap() = None;
    }
}

/// Record a server row rect for the headless single-line-height assertion.
/// Compiled in all builds so the call site counts as a real use (avoids the
/// unused-variable warning in release builds); it only records under `#[cfg(test)]`.
pub(super) fn record_server_row_rect(id: crate::bff::model::SessionId, rect: egui::Rect) {
    #[cfg(test)]
    ui_test_hook::push_server_row(id, rect);
    let _ = (id, rect);
}

/// Record the tags table's current scroll state for the offset-stability test.
#[cfg(test)]
pub(super) fn record_table_scroll(out: &egui::scroll_area::ScrollAreaOutput<()>) {
    ui_test_hook::set_table_scroll((out.state.clamped_offset(), out.content_size, out.inner_rect));
}

#[cfg(test)]
mod dnd_proof {
    //! Headless proof that an actual egui drag gesture wires the tree row into
    //! the tags table through the production drag-and-drop path.
    //!
    //! The kittest harness cannot express a real egui drag: its pointer events
    //! fire one per frame and never hold a button down across frames, so
    //! `Response::drag_started()` never trips. Here we drive the `Context`
    //! directly with `RawInput` over real passes, moving the pointer past the
    //! drag threshold while holding the button, so the drag payload activates,
    //! then releasing over the tags drop zone.

    use super::*;
    use crate::app::App;
    use crate::bff::model::{BrowseNode, NodeKind};
    use crate::ui::draw;
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput};

    const SIZE: egui::Vec2 = egui::Vec2::new(1200.0, 900.0);

    // The test-only hooks (`ui_test_hook`, `dnd_test_hook`) are process-global.
    // Serialize every test that renders through `draw` so concurrent tests cannot
    // clobber each other's recorded rects/offsets between a frame and its read.
    use std::sync::Mutex;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        // A panicking test poisons the mutex; recover so the rest of the suite
        // (which needs the shared hooks) keeps running rather than failing on a
        // `PoisonError`.
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn setup() -> (egui::Context, App) {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let mut app = App::new_test(&ctx);
        app.settings.set_lang(crate::i18n::Lang::Ru);
        app.seed_session(
            crate::bff::model::SessionId::new(1).unwrap(),
            crate::bff::model::EndpointSpec::da("", "Srv.Test"),
        );
        app.seed_tree_item(
            crate::bff::model::SessionId::new(1).unwrap(),
            None,
            0,
            BrowseNode {
                node_id: "ns=1;s=item".to_string(),
                name: "DragMe".to_string(),
                kind: NodeKind::Item,
                has_children: false,
            },
        );
        assert_eq!(app.rows().len(), 0);
        (ctx, app)
    }

    fn base_input() -> RawInput {
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, SIZE)),
            ..Default::default()
        }
    }

    fn frame(ctx: &egui::Context, app: &mut App, input: RawInput) {
        let mut output = ctx.run_ui(input, |ui| {
            draw(app, ui);
        });
        // Headless: there is no renderer to consume the font/texture deltas.
        // Dropping them would trip an epaint assertion, so mark them handled.
        output.textures_delta.clear();
    }

    /// Render once with no pointer to lay out the panels, then read the rects
    /// recorded by the test hooks in `draw_tree_item` / `table_panel`.
    fn capture_rects(ctx: &egui::Context, app: &mut App) -> (egui::Rect, egui::Rect) {
        frame(ctx, app, base_input());
        let (tree, tags) = dnd_test_hook::snapshot();
        let tree = tree.expect("tree-row rect hook must be populated");
        let tags = tags.expect("tags rect hook must be populated");
        assert!(!tree.is_negative());
        assert!(tags.contains_rect(tree) || !tags.intersects(tree));
        (tree, tags)
    }

    #[test]
    fn drag_item_onto_tags_table_adds_row() {
        let _guard = test_lock();
        let (ctx, mut app) = setup();
        let (tree, tags) = capture_rects(&ctx, &mut app);

        // Press on the tree row.
        let start = tree.center();
        let mut input = base_input();
        input.events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];
        frame(&ctx, &mut app, input);

        // Move far enough to activate a drag (past max_click_dist = 6 px).
        let drag = Pos2::new(start.x + 30.0, start.y + 40.0);
        frame(&ctx, &mut app, base_input_with_move(drag));

        // The payload must now be active so the drop zone will accept it.
        assert!(
            egui::DragAndDrop::has_any_payload(&ctx),
            "a real drag must set the DnD payload"
        );

        // Hover over the tags drop zone while holding the button.
        let over = Pos2::new(tags.center().x, tags.top() + 30.0);
        frame(&ctx, &mut app, base_input_with_move(over));

        // Release over the tags area; `dnd_drop_zone` yields the payload and the
        // Item is added synchronously by `add_dropped_tree`.
        let mut input = base_input();
        input.events = vec![Event::PointerButton {
            pos: over,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }];
        frame(&ctx, &mut app, input);

        assert_eq!(
            app.rows().len(),
            1,
            "dropping an Item must add exactly one row"
        );
        assert_eq!(app.rows()[0].item_id, "ns=1;s=item");
    }

    /// The drop target includes the unused central-panel space, even when
    /// the table has no rows. A user should not have to aim at its header.
    #[test]
    fn drag_item_to_empty_table_background_adds_row() {
        let _guard = test_lock();
        let (ctx, mut app) = setup();
        let (tree, tags) = capture_rects(&ctx, &mut app);
        assert!(tags.height() > 250.0, "drop zone must fill empty panel");

        let start = tree.center();
        let mut press = base_input();
        press.events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];
        frame(&ctx, &mut app, press);
        frame(
            &ctx,
            &mut app,
            base_input_with_move(Pos2::new(start.x + 30.0, start.y + 40.0)),
        );
        assert!(egui::DragAndDrop::has_any_payload(&ctx));
        let over = Pos2::new(tags.center().x, tags.bottom() - 20.0);
        frame(&ctx, &mut app, base_input_with_move(over));
        let mut release = base_input();
        release.events = vec![Event::PointerButton {
            pos: over,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }];
        frame(&ctx, &mut app, release);
        assert_eq!(app.rows().len(), 1, "blank background must accept a drop");
    }

    #[test]
    fn plain_click_on_tree_row_does_not_drop() {
        let _guard = test_lock();
        let (ctx, mut app) = setup();
        let (tree, _tags) = capture_rects(&ctx, &mut app);

        // A click (press + immediate release, no movement) must not start a drag
        // or drop anything; the row is selected and focus stays on the tree.
        let pos = tree.center();
        let mut input = base_input();
        input.events = vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ];
        frame(&ctx, &mut app, input);

        assert_eq!(app.rows().len(), 0, "a click must not add rows");
        assert_eq!(
            app.tree_focused, true,
            "a click on a tree row keeps tree focus"
        );
    }

    /// Render once at a custom screen size.
    fn frame_at(ctx: &egui::Context, app: &mut App, size: egui::Vec2) {
        frame(
            ctx,
            app,
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            },
        );
    }

    /// Seed a session with a branch of many item children so the table has rows
    /// to drag in / scroll, plus an Item sibling for direct drops.
    fn seed_wide_table(app: &mut App, rows: usize) -> u64 {
        app.seed_session(
            crate::bff::model::SessionId::new(1).unwrap(),
            crate::bff::model::EndpointSpec::da("", "Srv.Test"),
        );
        for i in 0..rows {
            app.seed_row(
                crate::bff::model::SessionId::new(1).unwrap(),
                &format!("ns=1;s=item{i}"),
                &format!("Sig{i:0>2}"),
                crate::bff::model::Value::I64(i as i64),
            );
        }
        app.activate_session(crate::bff::model::SessionId::new(1).unwrap());
        app.selected_tree = None;
        // A tree Item not yet subscribed, for the drop path.
        app.seed_tree_item(
            crate::bff::model::SessionId::new(1).unwrap(),
            None,
            0,
            BrowseNode {
                node_id: "ns=1;s=drop".to_string(),
                name: "DropMe".to_string(),
                kind: NodeKind::Item,
                has_children: false,
            },
        )
    }

    /// A narrow window with 9 columns and many rows must show a vertical
    /// scrollbar (the table's internal ScrollArea overflows) and must NOT shift
    /// the header or reset the scroll offset when rows are added.
    #[test]
    fn narrow_table_shows_scroll_and_keeps_offset_on_add() {
        let _guard = test_lock();
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let mut app = App::new_test(&ctx);
        app.settings.set_lang(crate::i18n::Lang::Ru);
        let _drop_id = seed_wide_table(&mut app, 100);
        let narrow = egui::Vec2::new(1100.0, 700.0);

        // Settle layout (first frame finalizes column widths / scrollbars).
        frame_at(&ctx, &mut app, narrow);
        for _ in 0..3 {
            frame_at(&ctx, &mut app, narrow);
        }
        let (offset_before, content, inner) =
            table_scroll_for_test().expect("table scroll hook must be populated");
        // Vertical overflow -> a vertical scrollbar is required.
        assert!(
            content.y > inner.height() + 1.0,
            "vertical scrollbar must be needed (content {content:?} vs inner {inner:?})"
        );
        // 9 columns at ~1248 px total exceed the ~760 px central panel, so the
        // content is wider than the viewport (clipped; egui_extras has no
        // native horizontal scrollbar, documented in ui-parity.md).
        let header_min_x = inner.min.x;
        let offset_y_before = offset_before.y;

        // Adding rows via «Добавить дочерние» style must not move the header or
        // reset the scroll offset.
        for i in 0..10 {
            app.seed_row(
                crate::bff::model::SessionId::new(1).unwrap(),
                &format!("ns=1;s=new{i}"),
                &format!("New{i}"),
                crate::bff::model::Value::I64(i as i64),
            );
        }
        frame_at(&ctx, &mut app, narrow);
        let (offset_after, _, inner_after) =
            table_scroll_for_test().expect("table scroll hook must be populated");
        eprintln!("DBG after offset={offset_after:?} inner_after={inner_after:?}");
        assert!(
            (offset_after.y - offset_y_before).abs() < 0.001,
            "vertical offset must be preserved on add: before {offset_y_before} after {}",
            offset_after.y
        );
        assert!(
            (inner_after.min.x - header_min_x).abs() < 0.001,
            "header x must not shift on add: before {header_min_x} after {}",
            inner_after.min.x
        );
        assert_eq!(
            app.rows().len(),
            110,
            "all seeded + added rows must be present"
        );
    }

    /// Drive a real drop gesture: the header x-position and scroll offset must
    /// stay put (the table must not jump when the drop adds rows).
    #[test]
    fn drop_keeps_scroll_and_header_position() {
        let _guard = test_lock();
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let mut app = App::new_test(&ctx);
        app.settings.set_lang(crate::i18n::Lang::Ru);
        let drop_id = seed_wide_table(&mut app, 6);
        let narrow = egui::Vec2::new(1200.0, 700.0);

        // Capture the tree row + tags rect on the first frame.
        frame_at(&ctx, &mut app, narrow);
        let (tree, tags) = dnd_test_hook::snapshot();
        let tree = tree.expect("tree hook");
        let tags = tags.expect("tags hook");
        let (offset_before, _, inner_before) = table_scroll_for_test().expect("table scroll hook");
        let header_x_before = inner_before.min.x;

        // Full drag gesture: press on the tree row, move past the threshold,
        // hover over the tags, release.
        let start = tree.center();
        let mut input = base_input();
        input.screen_rect = Some(egui::Rect::from_min_size(Pos2::ZERO, narrow));
        input.events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];
        frame(&ctx, &mut app, input);

        let drag = Pos2::new(start.x + 30.0, start.y + 40.0);
        let mut m = base_input();
        m.screen_rect = Some(egui::Rect::from_min_size(Pos2::ZERO, narrow));
        m.events = vec![Event::PointerMoved(drag)];
        frame(&ctx, &mut app, m);
        assert!(
            egui::DragAndDrop::has_any_payload(&ctx),
            "drag must be active"
        );

        let over = Pos2::new(tags.center().x, tags.top() + 30.0);
        let mut h2 = base_input();
        h2.screen_rect = Some(egui::Rect::from_min_size(Pos2::ZERO, narrow));
        h2.events = vec![Event::PointerMoved(over)];
        frame(&ctx, &mut app, h2);

        let mut rel = base_input();
        rel.screen_rect = Some(egui::Rect::from_min_size(Pos2::ZERO, narrow));
        rel.events = vec![Event::PointerButton {
            pos: over,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }];
        frame(&ctx, &mut app, rel);

        assert_eq!(app.rows().len(), 7, "drop must add the tree Item");
        let (offset_after, _, inner_after) =
            table_scroll_for_test().expect("table scroll hook after drop");
        assert!(
            (offset_after.y - offset_before.y).abs() < 0.001,
            "scroll offset must be preserved after drop: before {} after {}",
            offset_before.y,
            offset_after.y
        );
        assert!(
            (inner_after.min.x - header_x_before).abs() < 0.001,
            "header x must not shift after drop: before {header_x_before} after {}",
            inner_after.min.x
        );
        let _ = drop_id;
    }

    /// Every server row in the left panel renders as a single line: the name
    /// never wraps and the row height stays <= 28 px.
    #[test]
    fn server_rows_render_as_single_line() {
        let _guard = test_lock();
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let mut app = App::new_test(&ctx);
        app.settings.set_lang(crate::i18n::Lang::Ru);
        app.seed_session(
            crate::bff::model::SessionId::new(1).unwrap(),
            crate::bff::model::EndpointSpec::da("", "Alpha.Server"),
        );
        app.seed_session(
            crate::bff::model::SessionId::new(2).unwrap(),
            crate::bff::model::EndpointSpec::da("box2", "Beta.Long.Server.Name"),
        );
        app.activate_session(crate::bff::model::SessionId::new(1).unwrap());
        reset_ui_test_hooks();

        frame_at(&ctx, &mut app, SIZE);

        let rows = server_rows_for_test();
        assert!(!rows.is_empty(), "server rows must be recorded");
        for (_, rect) in &rows {
            assert!(
                rect.height() <= 28.0,
                "server row must be a single line (height {} > 28)",
                rect.height()
            );
            assert!(
                rect.width() >= 84.0,
                "server row name must keep readable width"
            );
        }
    }

    fn base_input_with_move(pos: Pos2) -> RawInput {
        let mut input = base_input();
        input.events = vec![Event::PointerMoved(pos)];
        input
    }

    /// The header is both a drag source and a drop target. Moving FullName
    /// before Type inserts it after WriteType and preserves persistence.
    #[test]
    fn header_drag_reorders_visible_columns() {
        use crate::columns::ColumnId;
        let _guard = test_lock();
        let (ctx, mut app) = setup();
        for _ in 0..3 {
            frame(&ctx, &mut app, base_input());
        }
        let source = header_cell_for_test(ColumnId::FullName).expect("source header");
        let target = header_cell_for_test(ColumnId::Type).expect("target header");
        let start = source.center();
        let mut press = base_input();
        press.events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];
        frame(&ctx, &mut app, press);
        frame(
            &ctx,
            &mut app,
            base_input_with_move(Pos2::new(start.x + 20.0, start.y)),
        );
        assert!(egui::DragAndDrop::has_any_payload(&ctx));
        let over = Pos2::new(target.left() + 5.0, target.center().y);
        frame(&ctx, &mut app, base_input_with_move(over));
        let mut release = base_input();
        release.events = vec![Event::PointerButton {
            pos: over,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }];
        frame(&ctx, &mut app, release);
        let order = app.settings.resolved_order();
        assert_eq!(order[2], ColumnId::FullName, "header drop must reorder");
        assert_eq!(order, app.settings.view.columns.order);
        assert!(app.settings.view.column_order.is_some());
    }

    /// Releasing the grip on the middle of a dialog row must work too:
    /// a user should not have to hit the 14px insertion gap exactly.
    #[test]
    fn dialog_drag_onto_row_reorders() {
        use crate::columns::ColumnId;
        let _guard = test_lock();
        columns_hook_api::clear();
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let mut app = App::new_test(&ctx);
        app.settings.set_lang(crate::i18n::Lang::Ru);
        app.seed_session(
            crate::bff::model::SessionId::new(1).unwrap(),
            crate::bff::model::EndpointSpec::da("", "Srv.Test"),
        );
        app.dialogs.columns.open = true;
        for _ in 0..3 {
            frame(&ctx, &mut app, base_input());
        }
        let grip = columns_hook_api::grip(ColumnId::FullName).expect("source grip");
        let target = columns_hook_api::row(ColumnId::Type).expect("destination row");
        let start = grip.center();
        let mut press = base_input();
        press.events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];
        frame(&ctx, &mut app, press);
        frame(
            &ctx,
            &mut app,
            base_input_with_move(Pos2::new(start.x + 30.0, start.y + 20.0)),
        );
        assert!(egui::DragAndDrop::has_any_payload(&ctx));
        let over = Pos2::new(target.center().x, target.top() + 2.0);
        frame(&ctx, &mut app, base_input_with_move(over));
        let mut release = base_input();
        release.events = vec![Event::PointerButton {
            pos: over,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }];
        frame(&ctx, &mut app, release);
        assert_eq!(app.settings.resolved_order()[2], ColumnId::FullName);
    }

    /// Headless proof that dragging a columns-dialog row by its grip onto an
    /// insertion gap reorders the column list and persists it (the same egui
    /// DnD path the user drives with the mouse).
    #[test]
    fn columns_dialog_drag_row_reorders_and_persists() {
        use crate::columns::ColumnId;
        // This test drives the production DnD path, which persists the column
        // order through `settings.save()`; redirect settings to a private temp
        // dir so it never overwrites the developer's real settings.json.
        let _settings = crate::settings::test_isolation::ScopedDir::temp("columns-dnd");
        let _guard = test_lock();
        columns_hook_api::clear();
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let mut app = App::new_test(&ctx);
        app.settings.set_lang(crate::i18n::Lang::Ru);
        app.seed_session(
            crate::bff::model::SessionId::new(1).unwrap(),
            crate::bff::model::EndpointSpec::da("", "Srv.Test"),
        );
        app.dialogs.columns.open = true;
        columns_hook_api::clear();

        // Settle the dialog layout (a centered Window may reposition on its
        // first passes), then capture the grip + gap rects from the settled frame.
        for _ in 0..3 {
            frame(&ctx, &mut app, base_input());
        }
        let grip =
            columns_hook_api::grip(ColumnId::FullName).expect("FullName grip must be recorded");
        // Gap before the column at absolute index 3 («Тип»): dropping FullName
        // here moves it AFTER «Записываемый тип» — a real reorder.
        let gap = columns_hook_api::gap(3).expect("gap before index 3 must be recorded");
        assert!(!grip.is_negative());
        assert!(!gap.is_negative());

        let before: Vec<ColumnId> = app.settings.view.columns.order.clone();
        assert_eq!(before[1], ColumnId::FullName);

        // Press on the FullName grip.
        let start = grip.center();
        let mut input = base_input();
        input.events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];
        frame(&ctx, &mut app, input);

        // Move past the drag threshold to activate the payload.
        let drag = Pos2::new(start.x + 30.0, start.y + 20.0);
        frame(&ctx, &mut app, base_input_with_move(drag));
        assert!(
            egui::DragAndDrop::has_any_payload(&ctx),
            "the grip drag must set a DnD payload"
        );

        // Hover over the gap before index 2 while holding the button.
        let over = gap.center();
        frame(&ctx, &mut app, base_input_with_move(over));

        // Release over the gap: `dnd_drop_zone` yields the payload and the dialog
        // reorders.
        let mut rel = base_input();
        rel.events = vec![Event::PointerButton {
            pos: over,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }];
        frame(&ctx, &mut app, rel);

        let after: Vec<ColumnId> = app.settings.view.columns.order.clone();
        assert_eq!(
            after.len(),
            ColumnId::ALL.len(),
            "reorder must keep all {} columns",
            ColumnId::ALL.len()
        );
        // «Полное имя» had to land right after «Записываемый тип» (index 2).
        let full_pos = after.iter().position(|c| *c == ColumnId::FullName).unwrap();
        assert_eq!(
            full_pos, 2,
            "«Полное имя» must move to the slot after «Записываемый тип», got position {full_pos}: {after:?}"
        );
        assert!(
            app.settings.view.column_order.is_some(),
            "column_order must be persisted after the dialog DnD"
        );
        // And persistence is consistent with the reordered legacy list.
        assert_eq!(
            app.settings.resolved_order(),
            app.settings.view.columns.order,
            "resolved order must match the persisted columns.order"
        );
    }
}

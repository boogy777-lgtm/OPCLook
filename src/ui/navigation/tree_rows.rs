use super::*;

pub(super) fn draw_tree_item(
    ui: &mut egui::Ui,
    item: &TreeItem,
    session: SessionId,
    selected: Option<(SessionId, u64)>,
    actions: &mut Vec<TreeAction>,
    lang: Lang,
) {
    ui.horizontal(|ui| {
        ui.add_space(item.depth as f32 * 14.0);
        if item.node.has_children {
            // UI1: a painted triangle that turns smoothly (no «>» / «v» glyph).
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(DOT_LEAF, DOT_LEAF), egui::Sense::click());
            let label = tr(
                lang,
                if item.expanded {
                    Tr::TreeCollapseLabel
                } else {
                    Tr::TreeExpandLabel
                },
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
            let openness = ui.ctx().animate_bool(response.id, item.expanded);
            let color = if response.hovered() {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            paint_expander(ui.painter(), rect, openness, color);
            if response.clicked() {
                actions.push(TreeAction::Toggle(item.id));
            }
        } else {
            ui.add_space(DOT_LEAF);
        }

        let is_selected = selected == Some((session, item.id));
        let text = match item.node.kind {
            NodeKind::Branch => {
                egui::RichText::new(&item.node.name).color(super::super::colors::accent())
            }
            NodeKind::Item => egui::RichText::new(&item.node.name),
        };
        // A full-width row is both clickable/queryable and a reliable drag
        // source: the whole row (not just the label glyphs) senses
        // click_and_drag, so a mouse grab anywhere on the row starts a drag.
        // This is what the headless RawInput DnD proof drives.
        let response = flat_row(
            ui,
            egui::vec2(ui.available_width(), TREE_ROW_HEIGHT),
            egui::Sense::click_and_drag(),
            is_selected,
            text,
            &item.node.name,
            RowBand::Fill,
        )
        .on_hover_text(&item.node.node_id);
        if item.loading {
            ui.spinner();
        }
        if response.clicked() {
            actions.push(TreeAction::Select(item.id));
        }
        if response.double_clicked() {
            if item.node.kind == NodeKind::Item {
                actions.push(TreeAction::Add(item.id));
            } else {
                actions.push(TreeAction::Toggle(item.id));
            }
        }
        // Announce this row as a drag source. egui only activates the payload
        // once the pointer moves past the drag threshold, so plain clicks keep
        // working. The tree has single selection, so the payload holds just
        // this node's id; `add_dropped_tree` accepts a list for multi-select.
        if response.drag_started() {
            response.dnd_set_drag_payload(TreeDragPayload {
                session,
                node_ids: vec![item.id],
            });
        }
        response.context_menu(|ui| {
            if ui.button(tr(lang, Tr::Add)).clicked() {
                actions.push(TreeAction::Select(item.id));
                actions.push(TreeAction::Add(item.id));
                ui.close();
            }
            if item.node.kind == NodeKind::Branch {
                if ui.button(tr(lang, Tr::AddChildrenSignals)).clicked() {
                    actions.push(TreeAction::Select(item.id));
                    actions.push(TreeAction::Children(item.id));
                    ui.close();
                }
            }
            if item.node.kind == NodeKind::Branch {
                if ui.button(tr(lang, Tr::RefreshBranch)).clicked() {
                    actions.push(TreeAction::Select(item.id));
                    actions.push(TreeAction::Refresh);
                    ui.close();
                }
            }
            if item.node.kind == NodeKind::Branch {
                if ui.button(tr(lang, Tr::ExpandCollapse)).clicked() {
                    actions.push(TreeAction::Toggle(item.id));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr(lang, Tr::CopyNodeName)).clicked() {
                actions.push(TreeAction::CopyName(item.id));
                ui.close();
            }
        });

        #[cfg(test)]
        set_tree_row_rect_for_test(response.rect);
    });
}

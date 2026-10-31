use crate::app::App;
use crate::i18n::{Tr, fmt, tr};
use crate::ui::ImeSafeClick;
use opclook_contract::actions::{ActionId, ActionSpec, CATALOG};

fn label(id: ActionId) -> Tr {
    match id {
        ActionId::OpenConnectDialog => Tr::ConnectEllipsis,
        ActionId::Browse => Tr::PaletteBrowse,
        ActionId::Write => Tr::PaletteWrite,
    }
}

fn unavailable(app: &App, id: ActionId) -> Option<Tr> {
    app.palette_unavailable(id)
}

fn matches_query(spec: &ActionSpec, title: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || title.to_lowercase().contains(&query)
        || spec.semantic_id.to_lowercase().contains(&query)
        || spec.code.to_string().contains(&query)
}

pub(super) fn palette_dialog(app: &mut App, ui: &mut egui::Ui) {
    if !app.dialogs.palette.open {
        return;
    }
    app.prepare_palette_context();
    let ctx = ui.ctx().clone();
    let lang = app.lang();
    let opening = app.dialogs.palette.focus_pending;
    let mut close = false;
    let mut execute = None;
    egui::Modal::new(egui::Id::unique("command_palette"))
        .show(&ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 40.0).clamp(120.0, 480.0));
            ui.heading(tr(lang, Tr::PaletteTitle));
            super::dialog_spacing(ui);
            let search = ui.add(egui::TextEdit::singleline(&mut app.dialogs.palette.query)
                .id_salt("palette_search").char_limit(256).desired_width(f32::INFINITY)
                .hint_text(tr(lang, Tr::PaletteSearch))).accessible_name(tr(lang, Tr::PaletteSearch));
            if opening { search.request_focus(); app.dialogs.palette.focus_pending = false; }
            let filtered: Vec<&ActionSpec> = CATALOG.iter().filter(|spec| matches_query(spec, tr(lang, label(spec.id)), &app.dialogs.palette.query)).collect();
            if !filtered.iter().any(|spec| Some(spec.id) == app.dialogs.palette.selected) {
                app.dialogs.palette.selected = filtered.first().map(|spec| spec.id);
            }
            if matches!(app.dialogs.palette.selected, Some(ActionId::Browse | ActionId::Write)) {
                if let Some(view) = app.dialogs.palette.context_session.and_then(|id| app.sessions.iter().find(|view| view.id == id)) {
                    ui.label(fmt(lang, Tr::PaletteTarget, &[&view.display_name]));
                }
            }
            let mut browse_focus = false;
            if app.dialogs.palette.selected == Some(ActionId::Browse) {
                ui.label(tr(lang, Tr::PaletteBrowseNode));
                let node = ui.add(egui::TextEdit::singleline(&mut app.dialogs.palette.browse_node)
                    .id_salt("palette_browse_node").hint_text(tr(lang, Tr::HintItemId)).char_limit(4096).desired_width(f32::INFINITY)).accessible_name(tr(lang, Tr::PaletteBrowseNode));
                browse_focus = node.has_focus() || (node.lost_focus() && ui.memory(|m| m.focused().is_none()));
            }
            if !crate::ui::ime_blocks_keys(&ctx) && !opening {
                for (key, delta) in [(egui::Key::ArrowDown, 1isize), (egui::Key::ArrowUp, -1isize)] {
                    if search.has_focus() && crate::ui::key_pressed_once(&ctx, key) && !filtered.is_empty() {
                        let index = filtered.iter().position(|spec| Some(spec.id) == app.dialogs.palette.selected).unwrap_or(0);
                        let next = index.saturating_add_signed(delta).min(filtered.len() - 1);
                        app.dialogs.palette.selected = filtered.get(next).map(|spec| spec.id);
                    }
                }
                if crate::ui::key_pressed_once(&ctx, egui::Key::Escape) { close = true; }
                if crate::ui::key_pressed_once(&ctx, egui::Key::Enter)
                    && ui.input(|input| input.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, repeat: false, modifiers, .. } if modifiers.is_none())))
                    && (browse_focus || search.has_focus() || (search.lost_focus() && ui.memory(|m| m.focused().is_none())))
                    && !ui.input(|input| input.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. }))) {
                    execute = app.dialogs.palette.selected;
                }
            }
            if filtered.is_empty() { ui.label(tr(lang, Tr::PaletteNoResults)); }
            for spec in &filtered {
                let reason = unavailable(app, spec.id);
                let selected = app.dialogs.palette.selected == Some(spec.id);
                let title = tr(lang, label(spec.id));
                let shortcut = crate::ui::shortcuts::action_shortcut(spec.id).map(|s| ctx.format_shortcut(&s)).unwrap_or_default();
                ui.push_id(spec.semantic_id, |ui| {
                    let response = ui.selectable_label(selected, format!("{title}  ·  {}  ·  {}  {shortcut}", spec.semantic_id, spec.code));
                    if response.ime_safe_clicked() { app.dialogs.palette.selected = Some(spec.id); }
                    if response.double_clicked() && reason.is_none() && !crate::ui::ime_blocks_keys(&ctx) { execute = Some(spec.id); }
                    if let Some(reason) = reason { ui.label(tr(lang, reason)); }
                });
            }
            let can_execute = app.dialogs.palette.selected.is_some_and(|id| unavailable(app, id).is_none());
            ui.horizontal(|ui| {
                let run = ui.add_enabled(can_execute, egui::Button::new(tr(lang, Tr::PaletteExecute)));
                let modified_enter = ui.input(|input| input.events.iter().any(|event| matches!(event,
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } if !modifiers.is_none()
                )));
                if (run.clicked_by(egui::PointerButton::Primary) || (run.ime_safe_clicked() && !modified_enter))
                    && !crate::ui::key_repeat(&ctx, egui::Key::Enter) && !crate::ui::key_repeat(&ctx, egui::Key::Space) {
                    execute = app.dialogs.palette.selected;
                }
                if ui.button(tr(lang, Tr::Cancel)).ime_safe_clicked() { close = true; }
            });
        });
    if close {
        app.dialogs.palette.open = false;
    } else if let Some(id) = execute {
        if let Err(reason) = app.execute_palette_action(id) {
            app.status = tr(lang, reason).to_string();
        }
    }
}

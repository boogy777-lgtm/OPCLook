use super::super::csv::{CsvServer, report_csv_server_problem, resolve_csv_session};
use super::dialog_spacing;
use crate::app::{App, CsvImportState};
use crate::i18n::{Tr, fmt, tr};

/// Modal progress window for a running CSV import; advances the job a chunk per
/// frame and allows cancel.
pub(super) fn csv_import_dialog(app: &mut App, ui: &mut egui::Ui) {
    let Some(job) = app.csv_import.as_ref() else {
        return;
    };
    let (total, processed, loaded, state) = (job.total, job.processed, job.loaded, job.state);
    let ctx = ui.ctx().clone();
    let lang = app.lang();
    let mut cancel = false;

    let done = state == CsvImportState::Completed;
    let fraction = if total > 0 {
        processed as f32 / total as f32
    } else {
        0.0
    };
    let job_done = done;
    let job_cancelled = state == CsvImportState::Cancelled;

    egui::Window::new(tr(lang, Tr::LoadTreeDialog))
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            dialog_spacing(ui);
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
            ui.label(if job_cancelled {
                tr(lang, Tr::ImportStopped).to_string()
            } else if job_done {
                fmt(lang, Tr::ImportDone, &[&loaded.to_string()])
            } else {
                fmt(
                    lang,
                    Tr::ImportProgress,
                    &[&processed.to_string(), &total.to_string()],
                )
            });
            ui.add(
                egui::ProgressBar::new(fraction.min(1.0))
                    .show_percentage()
                    .animate(job_cancelled),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !job_done && !job_cancelled,
                        egui::Button::new(tr(lang, Tr::Cancel)),
                    )
                    .clicked()
                {
                    cancel = true;
                }
                if (job_done || job_cancelled) && ui.button(tr(lang, Tr::Close)).clicked() {
                    cancel = true; // treated as close after completion
                }
            });
            if esc && (job_done || job_cancelled) {
                cancel = true;
            }
        });

    if cancel && (job_done || job_cancelled) {
        app.csv_import = None;
        return;
    }
    if cancel {
        if let Some(job) = app.csv_import.as_mut() {
            job.state = CsvImportState::Cancelled;
        }
        return;
    }

    // Advance a chunk of rows while the job runs.
    if !job_done && !job_cancelled {
        let batch = 300;
        let mut session_items: Vec<(crate::bff::model::SessionId, Vec<(String, String)>)> =
            Vec::new();
        for _ in 0..batch {
            let Some(row) = app.csv_import.as_mut().and_then(|j| j.remaining.pop()) else {
                break;
            };
            match resolve_csv_session(app, &row.server) {
                CsvServer::Session(session) => {
                    if let Some(entry) = session_items.iter_mut().find(|e| e.0 == session) {
                        entry.1.push((row.item_id, row.name));
                    } else {
                        session_items.push((session, vec![(row.item_id, row.name)]));
                    }
                }
                problem => report_csv_server_problem(app, &row, problem),
            }
            if let Some(job) = app.csv_import.as_mut() {
                job.processed += 1;
            }
        }
        for (session, items) in session_items {
            let before = app.rows().len();
            app.add_items(session, items);
            let loaded = app.rows().len().saturating_sub(before);
            if let Some(job) = app.csv_import.as_mut() {
                job.loaded += loaded;
            }
        }
        if let Some(job) = app.csv_import.as_mut() {
            if job.remaining.is_empty() {
                job.state = CsvImportState::Completed;
            }
        }
        // Keep repainting while the job is active.
        if !job_done {
            ctx.request_repaint();
        }
    }
}

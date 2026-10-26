use crate::app::{App, CsvImportJob, CsvImportRow};
use crate::i18n::{Tr, fmt, tr};

/// Serialize the signal table as CSV text.
pub fn export_tree_csv(app: &App) -> String {
    let mut text = String::from("server;item_id;name;value;quality;time\n");
    for row in &app.rows {
        text.push_str(&format!(
            "{};{};{};{};{};{}\n",
            csv_escape(&row.server_label),
            csv_escape(&row.item_id),
            csv_escape(&row.display_name),
            csv_escape(&row.value.to_string()),
            csv_escape(&row.quality.text),
            csv_escape(&row.timestamp_text()),
        ));
    }
    text
}

/// Serialize the signal table to a file.
pub fn export_tree_csv_to_path(app: &App, path: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(path, export_tree_csv(app))
}
/// Parse CSV text into import rows (server / item_id / name), skipping the
/// header and blank or empty-item lines.
pub(super) fn parse_csv_rows(text: &str) -> Vec<CsvImportRow> {
    let mut rows = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if line_index == 0 && line.to_ascii_lowercase().starts_with("server") {
            continue;
        }
        let mut parts = split_csv_line(line).into_iter();
        let Some(server) = parts.next() else { continue };
        let Some(item_id) = parts.next() else {
            continue;
        };
        let name = parts.next().unwrap_or(item_id.clone());
        let server = server.trim();
        let item_id = item_id.trim();
        if item_id.is_empty() {
            continue;
        }
        rows.push(CsvImportRow {
            server: server.to_string(),
            item_id: item_id.to_string(),
            name: name.to_string(),
        });
    }
    rows
}

/// Begin a modal CSV import (TFormLoad-style progress window). The job is
/// advanced a chunk at a time from the UI and supports cancel.
pub fn start_csv_import(app: &mut App, text: String) {
    let rows = parse_csv_rows(&text);
    let total = rows.len();
    if total == 0 {
        app.journal.warn(tr(app.lang(), Tr::CsvNoSignals));
        return;
    }
    app.csv_import = Some(CsvImportJob {
        open: true,
        cancelled: false,
        done: false,
        total,
        processed: 0,
        loaded: 0,
        remaining: rows,
    });
}

/// Load signal rows from CSV text synchronously. Returns the number of signals
/// added. Kept for tests and programmatic use; the interactive load uses the
/// modal progress job instead.
pub fn import_tree_csv(app: &mut App, text: &str) -> usize {
    let mut loaded = 0usize;
    for row in parse_csv_rows(text) {
        let session = resolve_csv_session(app, &row.server);
        let Some(session) = session else {
            app.journal
                .warn(fmt(app.lang(), Tr::CsvServerNotConnected, &[&row.server]));
            continue;
        };
        app.add_items(session, vec![(row.item_id, row.name)]);
        loaded += 1;
    }
    loaded
}

pub(super) fn resolve_csv_session(app: &App, server: &str) -> Option<crate::bff::model::SessionId> {
    app.sessions.iter().find_map(|s| {
        if s.endpoint.display_name() == server
            || s.endpoint.address == server
            || s.endpoint.display_name().contains(server)
        {
            Some(s.id)
        } else {
            None
        }
    })
}

/// Load signal rows from a CSV file. Returns the number of signals added.
pub fn import_tree_csv_from_path(app: &mut App, path: &std::path::Path) -> std::io::Result<usize> {
    let text = std::fs::read_to_string(path)?;
    Ok(import_tree_csv(app, &text))
}
pub(super) fn remaining_len(job: &CsvImportJob) -> usize {
    job.remaining.len()
}

pub(super) fn csv_escape(text: &str) -> String {
    if text.contains(';') || text.contains('"') || text.contains('\n') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// Split a `;`-separated line respecting `"..."` quoting (with `""` escapes).
pub(super) fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut in_quotes = true;
                while in_quotes {
                    match chars.next() {
                        Some('"') => {
                            if chars.peek() == Some(&'"') {
                                current.push('"');
                                chars.next();
                            } else {
                                in_quotes = false;
                            }
                        }
                        Some(other) => current.push(other),
                        None => in_quotes = false,
                    }
                }
            }
            ';' => {
                fields.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    fields.push(current);
    fields
}

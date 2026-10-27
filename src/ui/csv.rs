use crate::app::{App, CsvImportJob, CsvImportRow};
use crate::i18n::{Tr, fmt, tr};

/// UTF-8 byte order mark. Written first by the export (Excel only recognises a
/// UTF-8 CSV with it, otherwise Cyrillic and Chinese turn into mojibake) and
/// stripped by the import (a BOM in front of the header would hide it).
pub const CSV_BOM: char = '\u{feff}';

/// Serialize the signal table as CSV text (UTF-8 with BOM, `;` separated).
pub fn export_tree_csv(app: &App) -> String {
    let mut text = String::new();
    text.push(CSV_BOM);
    text.push_str("server;item_id;name;value;quality;time\n");
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
/// header and blank or empty-item lines. Quoted fields may contain `;`, `""`
/// and line breaks (what the export writes), `\r\n` / `\n` / `\r` end a record
/// and a leading BOM is ignored.
pub(super) fn parse_csv_rows(text: &str) -> Vec<CsvImportRow> {
    let mut rows = Vec::new();
    for (record_index, record) in parse_csv_records(text).into_iter().enumerate() {
        let mut fields = record.into_iter();
        let Some(server) = fields.next() else {
            continue;
        };
        if record_index == 0 && server.trim().to_ascii_lowercase().starts_with("server") {
            continue;
        }
        let Some(item_id) = fields.next() else {
            continue;
        };
        let item_id = item_id.trim().to_string();
        if item_id.is_empty() {
            continue;
        }
        let name = fields.next().unwrap_or_else(|| item_id.clone());
        rows.push(CsvImportRow {
            server: server.trim().to_string(),
            item_id,
            name,
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
    // The job pops rows from the end; store them reversed so the signals reach
    // the table in file order.
    let mut rows = rows;
    rows.reverse();
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
        match resolve_csv_session(app, &row.server) {
            CsvServer::Session(session) => {
                app.add_items(session, vec![(row.item_id, row.name)]);
                loaded += 1;
            }
            problem => report_csv_server_problem(app, &row, problem),
        }
    }
    loaded
}

/// How a CSV row's `server` column matched the open sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CsvServer {
    Session(crate::bff::model::SessionId),
    /// The column is empty: the row cannot be assigned to any server.
    Empty,
    /// Nothing matches.
    NotFound,
    /// Only a substring matches, and it matches several sessions.
    Ambiguous,
}

/// Match a CSV `server` value to a session (A5): an EXACT match of the display
/// name / address wins (case-sensitive first, then case-insensitive); only
/// when there is none, a substring is accepted, and only if it is unambiguous.
/// An empty value never matches (`"".contains("")` used to send such rows to
/// the first session).
pub(super) fn resolve_csv_session(app: &App, server: &str) -> CsvServer {
    let server = server.trim();
    if server.is_empty() {
        return CsvServer::Empty;
    }
    let names = |s: &crate::app::SessionView| {
        [
            s.endpoint.display_name(),
            s.endpoint.address.clone(),
            s.display_name.clone(),
        ]
    };
    let exact = |case_insensitive: bool| {
        app.sessions.iter().find_map(|s| {
            names(s)
                .iter()
                .any(|name| {
                    if case_insensitive {
                        name.eq_ignore_ascii_case(server)
                    } else {
                        name == server
                    }
                })
                .then_some(s.id)
        })
    };
    if let Some(id) = exact(false).or_else(|| exact(true)) {
        return CsvServer::Session(id);
    }
    let needle = server.to_lowercase();
    let mut partial = app.sessions.iter().filter(|s| {
        names(s)
            .iter()
            .any(|name| name.to_lowercase().contains(&needle))
    });
    match (partial.next(), partial.next()) {
        (Some(only), None) => CsvServer::Session(only.id),
        (Some(_), Some(_)) => CsvServer::Ambiguous,
        (None, _) => CsvServer::NotFound,
    }
}

/// Journal why a CSV row was skipped.
pub(super) fn report_csv_server_problem(app: &mut App, row: &CsvImportRow, problem: CsvServer) {
    let lang = app.lang();
    match problem {
        CsvServer::Session(_) => {}
        CsvServer::Empty => app
            .journal
            .warn(fmt(lang, Tr::CsvServerEmpty, &[&row.item_id])),
        CsvServer::NotFound => {
            app.journal
                .warn(fmt(lang, Tr::CsvServerNotConnected, &[&row.server]))
        }
        CsvServer::Ambiguous => app
            .journal
            .warn(fmt(lang, Tr::CsvServerAmbiguous, &[&row.server])),
    }
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
    if text.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// Split CSV text into records of `;`-separated fields (RFC 4180 quoting with
/// `;` as the separator). A quote opens a quoted field only at the start of a
/// field; inside quotes `""` is a quote and line breaks are data. An
/// unterminated quote swallows the rest of the text instead of failing.
pub(super) fn parse_csv_records(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix(CSV_BOM).unwrap_or(text);
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let end_record =
        |record: &mut Vec<String>, field: &mut String, records: &mut Vec<Vec<String>>| {
            record.push(std::mem::take(field));
            let blank = record.len() == 1 && record.first().is_some_and(|f| f.trim().is_empty());
            let finished = std::mem::take(record);
            if !blank {
                records.push(finished);
            }
        };
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            ';' => record.push(std::mem::take(&mut field)),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                end_record(&mut record, &mut field, &mut records);
            }
            '\n' => end_record(&mut record, &mut field, &mut records),
            other => field.push(other),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        end_record(&mut record, &mut field, &mut records);
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bff::model::{EndpointSpec, Value};

    fn app_with(servers: &[&str]) -> (egui::Context, App) {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        for (index, address) in servers.iter().enumerate() {
            app.seed_session(index as u64 + 1, EndpointSpec::da("", *address));
        }
        (ctx, app)
    }

    #[test]
    fn a5_exact_server_match_beats_an_earlier_substring_match() {
        // "A.OPC.2" comes first and contains "A.OPC": the old code picked it.
        let (_ctx, app) = app_with(&["A.OPC.2", "A.OPC"]);
        assert_eq!(resolve_csv_session(&app, "A.OPC"), CsvServer::Session(2));
        assert_eq!(resolve_csv_session(&app, "a.opc.2"), CsvServer::Session(1));
    }

    #[test]
    fn a5_empty_server_is_a_row_error_not_the_first_session() {
        let (_ctx, mut app) = app_with(&["A.OPC", "B.OPC"]);
        assert_eq!(resolve_csv_session(&app, ""), CsvServer::Empty);
        assert_eq!(resolve_csv_session(&app, "   "), CsvServer::Empty);
        let added = import_tree_csv(&mut app, "server;item_id;name\n;Tag1;One\n");
        assert_eq!(added, 0);
        assert!(app.rows.is_empty(), "nothing may land in the first session");
        let log: Vec<String> = app
            .journal
            .entries()
            .iter()
            .map(|e| e.render(crate::i18n::Lang::Ru))
            .collect();
        assert!(
            log.iter()
                .any(|l| l.contains("Tag1") && l.contains("не указан")),
            "{log:?}"
        );
    }

    #[test]
    fn a5_substring_is_only_a_fallback_and_must_be_unambiguous() {
        let (_ctx, app) = app_with(&["Vendor.Alpha.OPC", "Vendor.Beta.OPC"]);
        assert_eq!(resolve_csv_session(&app, "alpha"), CsvServer::Session(1));
        assert_eq!(resolve_csv_session(&app, "Vendor"), CsvServer::Ambiguous);
        assert_eq!(resolve_csv_session(&app, "Gamma"), CsvServer::NotFound);
    }

    #[test]
    fn a6_export_starts_with_a_bom_for_excel() {
        let (_ctx, app) = app_with(&["A.OPC"]);
        let text = export_tree_csv(&app);
        assert!(text.starts_with(CSV_BOM));
        assert!(text.trim_start_matches(CSV_BOM).starts_with("server;"));
    }

    #[test]
    fn a6_import_ignores_a_bom_in_front_of_the_header() {
        let rows = parse_csv_rows("\u{feff}server;item_id;name\r\nA.OPC;Tag1;One\r\n");
        assert_eq!(rows.len(), 1, "the header must not become a data row");
        assert_eq!(rows[0].server, "A.OPC");
        assert_eq!(rows[0].item_id, "Tag1");
        assert_eq!(rows[0].name, "One");
    }

    #[test]
    fn a6_quoted_fields_with_newlines_and_separators_roundtrip() {
        let (_ctx, mut app) = app_with(&["A.OPC"]);
        let nasty = "Line one\nLine \"two\"; and more\r\nthird";
        let key = app.seed_row(1, "Tag.1", nasty, Value::I64(5));
        let plain = app.seed_row(1, "Tag.2", "Plain", Value::I64(6));
        assert!(key != plain);
        let text = export_tree_csv(&app);
        let rows = parse_csv_rows(&text);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].name, nasty);
        assert_eq!(rows[1].name, "Plain");

        let (_ctx2, mut other) = app_with(&["A.OPC"]);
        assert_eq!(import_tree_csv(&mut other, &text), 2);
        assert_eq!(other.rows.len(), 2);
        assert_eq!(other.rows[0].display_name, nasty);
        assert_eq!(other.rows[0].item_id, "Tag.1");
    }

    #[test]
    fn a6_records_split_on_crlf_lf_and_cr_and_skip_blank_lines() {
        let records = parse_csv_records("a;b\r\nc;d\n\ne;f\rg;h");
        assert_eq!(
            records,
            vec![
                vec!["a", "b"],
                vec!["c", "d"],
                vec!["e", "f"],
                vec!["g", "h"],
            ]
        );
    }

    #[test]
    fn a6_unterminated_quote_does_not_panic_or_loop() {
        let records = parse_csv_records("server;item_id\nA;\"never closed\nB;C");
        assert_eq!(records.len(), 2);
        assert_eq!(records[1][1], "never closed\nB;C");
    }

    #[test]
    fn interactive_import_keeps_the_file_order() {
        let (_ctx, mut app) = app_with(&["A.OPC"]);
        start_csv_import(
            &mut app,
            "server;item_id;name\nA.OPC;T1;One\nA.OPC;T2;Two\nA.OPC;T3;Three\n".to_string(),
        );
        let job = app.csv_import.as_mut().expect("job");
        let popped: Vec<String> = std::iter::from_fn(|| job.remaining.pop())
            .map(|r| r.item_id)
            .collect();
        assert_eq!(popped, ["T1", "T2", "T3"]);
    }
}

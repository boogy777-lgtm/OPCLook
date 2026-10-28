use crate::app::{App, CsvImportJob, CsvImportRow, CsvImportState};
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
    for row in app.rows() {
        text.push_str(&format!(
            "{};{};{};{};{};{}\n",
            csv_escape(&row.server_label),
            csv_escape(&row.item_id),
            csv_escape(&row.display_name),
            csv_escape(&row.value.to_string()),
            csv_escape(row.quality.text()),
            csv_escape(&row.timestamp_text()),
        ));
    }
    text
}

/// Serialize the signal table to a file.
pub fn export_tree_csv_to_path(app: &App, path: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(path, export_tree_csv(app))
}

const MAX_CSV_ISSUES: usize = 100;

#[derive(Debug, PartialEq, Eq)]
enum CsvIssueKind {
    MalformedQuote,
    MissingFields,
    EmptyItem,
}
#[derive(Debug, PartialEq, Eq)]
struct CsvIssue {
    line: usize,
    kind: CsvIssueKind,
}
#[derive(Default, Debug)]
struct CsvParseResult {
    rows: Vec<CsvImportRow>,
    issues: Vec<CsvIssue>,
    omitted: usize,
}
impl CsvParseResult {
    fn issue(&mut self, line: usize, kind: CsvIssueKind) {
        if self.issues.len() < MAX_CSV_ISSUES {
            self.issues.push(CsvIssue { line, kind });
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }
}

fn parse_csv_rows(text: &str) -> CsvParseResult {
    let mut result = CsvParseResult::default();
    let text = text.strip_prefix(CSV_BOM).unwrap_or(text);
    let mut chars = text.chars().peekable();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut field_started = false;
    let mut line = 1usize;
    let mut start = 1usize;
    let mut quoted = false;
    let mut closed = false;
    let mut malformed = false;
    let mut first = true;
    loop {
        let c = chars.next();
        if quoted {
            match c {
                Some('"') if chars.peek() == Some(&'"') => {
                    chars.next();
                    if fields.len() < 3 {
                        field.push('"');
                    }
                }
                Some('"') => {
                    quoted = false;
                    closed = true;
                }
                Some('\r') => {
                    if fields.len() < 3 {
                        field.push('\r');
                    }
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                        if fields.len() < 3 {
                            field.push('\n');
                        }
                    }
                    line = line.saturating_add(1);
                }
                Some('\n') => {
                    if fields.len() < 3 {
                        field.push('\n');
                    }
                    line = line.saturating_add(1);
                }
                Some(ch) => {
                    if fields.len() < 3 {
                        field.push(ch);
                    }
                }
                None => {
                    malformed = true;
                    quoted = false;
                }
            }
            if c.is_some() {
                continue;
            }
        }
        match c {
            Some('"') if !field_started && !closed => {
                quoted = true;
                field_started = true;
            }
            Some('"') => {
                malformed = true;
                if fields.len() < 3 {
                    field.push('"');
                }
            }
            Some(';') => {
                if fields.len() < 3 {
                    fields.push(std::mem::take(&mut field));
                }
                field_started = false;
                closed = false;
            }
            Some('\r' | '\n') | None => {
                if c == Some('\r') && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                if fields.len() < 3 {
                    fields.push(std::mem::take(&mut field));
                }
                let blank = fields.len() == 1
                    && fields.first().is_some_and(|f| f.trim().is_empty())
                    && !malformed;
                if !blank {
                    let header = first
                        && fields
                            .first()
                            .is_some_and(|f| f.trim().eq_ignore_ascii_case("server"))
                        && fields
                            .get(1)
                            .is_some_and(|f| f.trim().eq_ignore_ascii_case("item_id"));
                    first = false;
                    if malformed {
                        result.issue(start, CsvIssueKind::MalformedQuote);
                    } else if !header {
                        match (fields.first(), fields.get(1)) {
                            (Some(server), Some(item)) if !item.trim().is_empty() => {
                                let item_id = item.trim().to_owned();
                                result.rows.push(CsvImportRow {
                                    line: start,
                                    server: server.trim().to_owned(),
                                    name: fields.get(2).cloned().unwrap_or_else(|| item_id.clone()),
                                    item_id,
                                });
                            }
                            (_, Some(_)) => result.issue(start, CsvIssueKind::EmptyItem),
                            _ => result.issue(start, CsvIssueKind::MissingFields),
                        }
                    }
                }
                fields.clear();
                field_started = false;
                malformed = false;
                closed = false;
                if c.is_none() {
                    break;
                }
                line = line.saturating_add(1);
                start = line;
            }
            Some(ch) => {
                if closed {
                    malformed = true;
                }
                field_started = true;
                if fields.len() < 3 {
                    field.push(ch);
                }
            }
        }
    }
    result
}

fn report_csv_issues(app: &mut App, parsed: &CsvParseResult) {
    let lang = app.lang();
    for issue in &parsed.issues {
        let kind = tr(
            lang,
            match issue.kind {
                CsvIssueKind::MalformedQuote => Tr::CsvMalformedQuote,
                CsvIssueKind::MissingFields => Tr::CsvMissingFields,
                CsvIssueKind::EmptyItem => Tr::CsvEmptyItem,
            },
        );
        app.journal.warn(fmt(
            lang,
            Tr::CsvIssueLine,
            &[&issue.line.to_string(), &kind],
        ));
    }
    if parsed.omitted > 0 {
        app.journal.warn(fmt(
            lang,
            Tr::CsvIssuesOmitted,
            &[&parsed.omitted.to_string()],
        ));
    }
}

/// Begin a modal CSV import (TFormLoad-style progress window). The job is
/// advanced a chunk at a time from the UI and supports cancel.
pub fn start_csv_import(app: &mut App, text: String) {
    let parsed = parse_csv_rows(&text);
    report_csv_issues(app, &parsed);
    let rows = parsed.rows;
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
        state: CsvImportState::Running,
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
    let parsed = parse_csv_rows(text);
    report_csv_issues(app, &parsed);
    for row in parsed.rows {
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
            s.endpoint().display_name(),
            s.endpoint().address.clone(),
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
    let message = match problem {
        CsvServer::Session(_) => return,
        CsvServer::Empty => fmt(lang, Tr::CsvServerEmpty, &[&row.item_id]),
        CsvServer::NotFound => fmt(lang, Tr::CsvServerNotConnected, &[&row.server]),
        CsvServer::Ambiguous => fmt(lang, Tr::CsvServerAmbiguous, &[&row.server]),
    };
    app.journal.warn(fmt(
        lang,
        Tr::CsvIssueLine,
        &[&row.line.to_string(), &message],
    ));
}

/// Load signal rows from a CSV file. Returns the number of signals added.
pub fn import_tree_csv_from_path(app: &mut App, path: &std::path::Path) -> std::io::Result<usize> {
    let text = std::fs::read_to_string(path)?;
    Ok(import_tree_csv(app, &text))
}
pub(super) fn csv_escape(text: &str) -> String {
    if text.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bff::model::{EndpointSpec, Value};

    fn app_with(servers: &[&str]) -> (egui::Context, App) {
        let ctx = egui::Context::default();
        let mut app = App::new_test(&ctx);
        for (index, address) in servers.iter().enumerate() {
            app.seed_session(
                crate::bff::model::SessionId::new(index as u64 + 1).unwrap(),
                EndpointSpec::da("", *address),
            );
        }
        (ctx, app)
    }

    #[test]
    fn a5_exact_server_match_beats_an_earlier_substring_match() {
        // "A.OPC.2" comes first and contains "A.OPC": the old code picked it.
        let (_ctx, app) = app_with(&["A.OPC.2", "A.OPC"]);
        assert_eq!(
            resolve_csv_session(&app, "A.OPC"),
            CsvServer::Session(crate::bff::model::SessionId::new(2).unwrap())
        );
        assert_eq!(
            resolve_csv_session(&app, "a.opc.2"),
            CsvServer::Session(crate::bff::model::SessionId::new(1).unwrap())
        );
    }

    #[test]
    fn a5_empty_server_is_a_row_error_not_the_first_session() {
        let (_ctx, mut app) = app_with(&["A.OPC", "B.OPC"]);
        assert_eq!(resolve_csv_session(&app, ""), CsvServer::Empty);
        assert_eq!(resolve_csv_session(&app, "   "), CsvServer::Empty);
        let added = import_tree_csv(&mut app, "server;item_id;name\n;Tag1;One\n");
        assert_eq!(added, 0);
        assert!(
            app.rows().is_empty(),
            "nothing may land in the first session"
        );
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
        assert_eq!(
            resolve_csv_session(&app, "alpha"),
            CsvServer::Session(crate::bff::model::SessionId::new(1).unwrap())
        );
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
        let rows = parse_csv_rows("\u{feff}server;item_id;name\r\nA.OPC;Tag1;One\r\n").rows;
        assert_eq!(rows.len(), 1, "the header must not become a data row");
        assert_eq!(rows[0].server, "A.OPC");
        assert_eq!(rows[0].item_id, "Tag1");
        assert_eq!(rows[0].name, "One");
    }

    #[test]
    fn a6_quoted_fields_with_newlines_and_separators_roundtrip() {
        let (_ctx, mut app) = app_with(&["A.OPC"]);
        let nasty = "Line one\nLine \"two\"; and more\r\nthird";
        let key = app.seed_row(
            crate::bff::model::SessionId::new(1).unwrap(),
            "Tag.1",
            nasty,
            Value::I64(5),
        );
        let plain = app.seed_row(
            crate::bff::model::SessionId::new(1).unwrap(),
            "Tag.2",
            "Plain",
            Value::I64(6),
        );
        assert!(key != plain);
        let text = export_tree_csv(&app);
        let rows = parse_csv_rows(&text).rows;
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].name, nasty);
        assert_eq!(rows[1].name, "Plain");

        let (_ctx2, mut other) = app_with(&["A.OPC"]);
        assert_eq!(import_tree_csv(&mut other, &text), 2);
        assert_eq!(other.rows().len(), 2);
        assert_eq!(other.rows()[0].display_name, nasty);
        assert_eq!(other.rows()[0].item_id, "Tag.1");
    }

    #[test]
    fn diagnostics_track_physical_lines_and_preserve_valid_records() {
        let parsed = parse_csv_rows(
            "serverName;id;\"a\r\nb;\"\"c\"\"\"\r\n\rmissing\nA;\rB;x;bad\"quote\rC;y;\"unterminated",
        );
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.rows[0].server, "serverName");
        assert_eq!(parsed.rows[0].name, "a\r\nb;\"c\"");
        assert_eq!(
            parsed.issues,
            vec![
                CsvIssue {
                    line: 4,
                    kind: CsvIssueKind::MissingFields
                },
                CsvIssue {
                    line: 5,
                    kind: CsvIssueKind::EmptyItem
                },
                CsvIssue {
                    line: 6,
                    kind: CsvIssueKind::MalformedQuote
                },
                CsvIssue {
                    line: 7,
                    kind: CsvIssueKind::MalformedQuote
                }
            ]
        );
    }

    #[test]
    fn issue_collection_is_bounded() {
        let parsed = parse_csv_rows(&"missing\n".repeat(MAX_CSV_ISSUES + 7));
        assert_eq!(parsed.issues.len(), MAX_CSV_ISSUES);
        assert_eq!(parsed.omitted, 7);
    }

    #[test]
    fn extra_columns_are_discarded_but_quotes_are_validated() {
        let input = format!("A;tag;name;{}\nB;id;ok;bad\"quote", ";".repeat(100_000));
        let parsed = parse_csv_rows(&input);
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.rows[0].name, "name");
        assert_eq!(
            parsed.issues,
            vec![CsvIssue {
                line: 2,
                kind: CsvIssueKind::MalformedQuote
            }]
        );
    }

    #[test]
    fn both_import_paths_report_parse_errors_and_source_lines() {
        for modal in [false, true] {
            let (_ctx, mut app) = app_with(&["A.OPC"]);
            app.journal.clear();
            let input = "server;item_id;name\nmissing\nA.OPC;T;ok";
            if modal {
                start_csv_import(&mut app, input.to_owned());
            } else {
                assert_eq!(import_tree_csv(&mut app, input), 1);
            }
            assert!(app.journal.entries().iter().any(|entry| matches!(&entry.message, crate::journal::JournalMessage::Plain(text) if text.contains('2'))));
        }
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

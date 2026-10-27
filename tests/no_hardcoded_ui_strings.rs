//! Guard against reintroducing hardcoded Russian (and other non-ASCII) UI text
//! into the first-party UI layer.
//!
//! User-visible strings belong in `src/i18n.rs` (the `Tr` dictionary). This test
//! reads `src/ui.rs` and `src/app.rs` via `include_str!`, strips `//` line and
//! `/* */` block comments plus every `#[cfg(test)]`-gated item, then scans the
//! remaining string literals for Cyrillic script and asserts none are left
//! (except an explicit whitelist of intentional data aliases / symbols).

/// String literals that legitimately contain non-ASCII characters. Kept short
/// and reviewed: language endonyms live in `i18n::Lang::endonym()` (not here),
/// symbols (● × ↑ ↓ ∞ …) are punctuation not Cyrillic, and the boolean spellings
/// are data-input aliases, not UI copy.
const WHITELIST: &[&str] = &["да", "нет", "是", "否"];

/// Return true if `text` (the inner content of a string literal) contains a
/// Cyrillic script character.
fn has_cyrillic(text: &str) -> bool {
    text.chars().any(|c| {
        // Cyrillic U+0400..=U+052F (incl. extended), plus combining marks.
        matches!(c as u32, 0x0400..=0x052F)
    })
}

/// A minimal scanner over a Rust source slice that skips `//` and `/*...*/`
/// comments, skips `#[cfg(test)]`-gated items (brace-matched), and yields every
/// double-quoted string literal that survives (escapes handled).
struct Scan {
    src: Vec<char>,
    i: usize,
}

impl Scan {
    fn new(src: &str) -> Self {
        Self {
            src: src.chars().collect(),
            i: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.i).copied()
    }

    fn skip_line_comment(&mut self) {
        while self.peek().is_some() && self.peek() != Some('\n') {
            self.i += 1;
        }
    }

    fn skip_block_comment(&mut self) {
        // assumes current chars are `/*`; advances past the closing `*/`.
        self.i += 2;
        while self.i + 1 < self.src.len()
            && !(self.src[self.i] == '*' && self.src[self.i + 1] == '/')
        {
            self.i += 1;
        }
        self.i = (self.i + 2).min(self.src.len());
    }

    /// If positioned at a `#[cfg(test)]` attribute, skip it and the brace body
    /// of the item it annotates. Returns true when a block was skipped.
    fn try_skip_cfg_test(&mut self) -> bool {
        const ATTR: [char; 12] = ['#', '[', 'c', 'f', 'g', '(', 't', 'e', 's', 't', ')', ']'];
        if self.src[self.i..].starts_with(&ATTR) {
            self.i += ATTR.len();
            // Skip the annotated item's keyword/name/signature up to its body.
            // An item without a body (`mod x;`, `use ...;`, `static ... = ...;`)
            // ends at its `;`.
            while self.peek().is_some() && self.peek() != Some('{') && self.peek() != Some(';') {
                self.i += 1;
            }
            if self.peek() == Some(';') {
                self.i += 1;
                return true;
            }
            if self.peek() == Some('{') {
                self.skip_balanced_braces();
                return true;
            }
        }
        false
    }

    /// Skip the `{ ... }` whose opening brace is at the current position
    /// (counting nested braces, treating string literals as single tokens),
    /// leaving `self.i` just past the matching `}`.
    fn skip_balanced_braces(&mut self) {
        debug_assert_eq!(self.peek(), Some('{'));
        let mut depth = 0usize;
        loop {
            match self.peek() {
                Some('{') => depth += 1,
                Some('}') => {
                    depth -= 1;
                    if depth == 0 {
                        self.i += 1;
                        break;
                    }
                }
                Some('"') => self.skip_string_literal(),
                Some('r') if self.skip_raw_string() => continue,
                Some('\'') if self.skip_char_literal() => continue,
                Some('\\') => {
                    // Skip an escaped char (e.g. inside nothing here, but safe).
                    self.i += 2;
                    continue;
                }
                Some(_) => {}
                None => break,
            }
            self.i += 1;
        }
    }

    /// Skip a raw string literal (`r"..."`, `r#"..."#`) at `self.i`; false (and
    /// no movement) when the position is not the start of one.
    fn skip_raw_string(&mut self) -> bool {
        if self.i > 0 {
            let before = self.src[self.i - 1];
            if before.is_alphanumeric() || before == '_' {
                return false;
            }
        }
        let mut j = self.i + 1;
        let mut hashes = 0usize;
        while self.src.get(j) == Some(&'#') {
            hashes += 1;
            j += 1;
        }
        if self.src.get(j) != Some(&'"') {
            return false;
        }
        j += 1;
        while j < self.src.len() {
            if self.src[j] == '"' && (1..=hashes).all(|k| self.src.get(j + k) == Some(&'#')) {
                self.i = j + 1 + hashes;
                return true;
            }
            j += 1;
        }
        self.i = self.src.len();
        true
    }

    /// Skip a character literal (`'{'`, `'"'`, `'\\n'`) at `self.i`; false for a
    /// lifetime or anything else.
    fn skip_char_literal(&mut self) -> bool {
        match (
            self.src.get(self.i + 1),
            self.src.get(self.i + 2),
            self.src.get(self.i + 3),
        ) {
            (Some('\\'), Some(_), Some('\'')) => {
                self.i += 4;
                true
            }
            (Some(c), Some('\''), _) if *c != '\\' => {
                self.i += 3;
                true
            }
            _ => false,
        }
    }

    /// Skip a double-quoted string literal at `self.i` (open quote inclusive),
    /// leaving `self.i` just past the closing quote. Handles backslash escapes.
    fn skip_string_literal(&mut self) {
        if self.peek() != Some('"') {
            return;
        }
        self.i += 1;
        loop {
            match self.peek() {
                Some('\\') => self.i += 2,
                Some('"') => {
                    self.i += 1;
                    break;
                }
                Some(_) => self.i += 1,
                None => break,
            }
        }
    }
}

/// Collect all surviving string-literal contents (with escapes unescaped) from
/// production code (comments and `#[cfg(test)]` items removed by the scanner).
fn production_string_literals(src: &str) -> Vec<String> {
    let mut scan = Scan::new(src);
    let mut out = Vec::new();
    while scan.i < scan.src.len() {
        match scan.peek() {
            Some('/') if scan.src.get(scan.i + 1).copied() == Some('/') => {
                scan.skip_line_comment();
            }
            Some('/') if scan.src.get(scan.i + 1).copied() == Some('*') => {
                scan.skip_block_comment();
            }
            Some('#') if scan.src[scan.i..].starts_with(&['#', '[', 'c', 'f', 'g'][..]) => {
                if !scan.try_skip_cfg_test() {
                    scan.i += 1;
                }
            }
            Some('"') => {
                // Parse a string literal.
                scan.i += 1; // opening quote
                let mut content = String::new();
                loop {
                    match scan.peek() {
                        Some('\\') => {
                            // Keep escapes verbatim (we only detect raw Cyrillic);
                            // push both chars so the literal is still well-formed.
                            content.push('\\');
                            scan.i += 1;
                            if let Some(c) = scan.peek() {
                                content.push(c);
                                scan.i += 1;
                            }
                        }
                        Some('"') => {
                            scan.i += 1; // closing quote
                            break;
                        }
                        Some(c) => {
                            content.push(c);
                            scan.i += 1;
                        }
                        None => break,
                    }
                }
                out.push(content);
            }
            Some(_) => scan.i += 1,
            None => break,
        }
    }
    out
}

fn assert_no_hardcoded_ui_strings(source: &str, path: &str) {
    let mut failures: Vec<String> = Vec::new();
    for literal in production_string_literals(source) {
        if !has_cyrillic(&literal) {
            continue;
        }
        if WHITELIST.contains(&literal.as_str()) {
            continue;
        }
        failures.push(literal);
    }
    assert!(
        failures.is_empty(),
        "{path}: production string literal(s) contain hardcoded Cyrillic UI text: {failures:?} \
         (route them through the `Tr` dictionary in src/i18n.rs)",
    );
}

#[test]
fn ui_rs_has_no_hardcoded_cyrillic_ui_strings() {
    for (name, source) in [
        ("src/ui/mod.rs", include_str!("../src/ui/mod.rs")),
        ("src/ui/actions.rs", include_str!("../src/ui/actions.rs")),
        ("src/ui/colors.rs", include_str!("../src/ui/colors.rs")),
        ("src/ui/csv.rs", include_str!("../src/ui/csv.rs")),
        ("src/ui/files.rs", include_str!("../src/ui/files.rs")),
        ("src/ui/menu.rs", include_str!("../src/ui/menu.rs")),
        (
            "src/ui/navigation.rs",
            include_str!("../src/ui/navigation.rs"),
        ),
        ("src/ui/paint.rs", include_str!("../src/ui/paint.rs")),
        ("src/ui/panels.rs", include_str!("../src/ui/panels.rs")),
        ("src/ui/servers.rs", include_str!("../src/ui/servers.rs")),
        (
            "src/ui/shortcuts.rs",
            include_str!("../src/ui/shortcuts.rs"),
        ),
        ("src/ui/status.rs", include_str!("../src/ui/status.rs")),
        ("src/ui/table.rs", include_str!("../src/ui/table.rs")),
        (
            "src/ui/test_hooks.rs",
            include_str!("../src/ui/test_hooks.rs"),
        ),
        ("src/ui/tree.rs", include_str!("../src/ui/tree.rs")),
        (
            "src/ui/dialogs/mod.rs",
            include_str!("../src/ui/dialogs/mod.rs"),
        ),
        (
            "src/ui/dialogs/about.rs",
            include_str!("../src/ui/dialogs/about.rs"),
        ),
        (
            "src/ui/dialogs/add_signal.rs",
            include_str!("../src/ui/dialogs/add_signal.rs"),
        ),
        (
            "src/ui/dialogs/columns.rs",
            include_str!("../src/ui/dialogs/columns.rs"),
        ),
        (
            "src/ui/dialogs/confirm.rs",
            include_str!("../src/ui/dialogs/confirm.rs"),
        ),
        (
            "src/ui/dialogs/connect.rs",
            include_str!("../src/ui/dialogs/connect.rs"),
        ),
        (
            "src/ui/dialogs/csv_import.rs",
            include_str!("../src/ui/dialogs/csv_import.rs"),
        ),
        (
            "src/ui/dialogs/message.rs",
            include_str!("../src/ui/dialogs/message.rs"),
        ),
        (
            "src/ui/dialogs/network.rs",
            include_str!("../src/ui/dialogs/network.rs"),
        ),
        (
            "src/ui/dialogs/options.rs",
            include_str!("../src/ui/dialogs/options.rs"),
        ),
        (
            "src/ui/dialogs/ua_tree.rs",
            include_str!("../src/ui/dialogs/ua_tree.rs"),
        ),
        (
            "src/ui/dialogs/write.rs",
            include_str!("../src/ui/dialogs/write.rs"),
        ),
    ] {
        assert_no_hardcoded_ui_strings(source, name);
    }
}

#[test]
fn app_modules_have_no_hardcoded_cyrillic_ui_strings() {
    for (name, source) in [
        ("src/app/mod.rs", include_str!("../src/app/mod.rs")),
        ("src/app/dialogs.rs", include_str!("../src/app/dialogs.rs")),
        ("src/app/events.rs", include_str!("../src/app/events.rs")),
        ("src/app/present.rs", include_str!("../src/app/present.rs")),
        ("src/app/runtime.rs", include_str!("../src/app/runtime.rs")),
        ("src/app/search.rs", include_str!("../src/app/search.rs")),
        (
            "src/app/sessions.rs",
            include_str!("../src/app/sessions.rs"),
        ),
        ("src/app/state.rs", include_str!("../src/app/state.rs")),
        ("src/app/table.rs", include_str!("../src/app/table.rs")),
        ("src/app/tree.rs", include_str!("../src/app/tree.rs")),
    ] {
        assert_no_hardcoded_ui_strings(source, name);
    }
}

/// Data-input aliases in the backend (boolean spellings, the legacy "Auto"
/// setting value) are not user-visible text.
const BACKEND_WHITELIST: &[&str] = &["да", "нет", "Авто", "авто"];

fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// A9: the backend (`src/bff/**`) sends typed `BackendMsg` variants and the
/// application localizes them; a Russian literal in production backend code
/// would show Russian text in the English and Chinese interface.
#[test]
fn backend_has_no_hardcoded_cyrillic_strings() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bff");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(
        files.len() >= 15,
        "the backend sources were not found under {root:?}: {files:?}"
    );
    let mut failures: Vec<String> = Vec::new();
    for path in files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Wholly test-only modules (`#[cfg(test)] mod tests;`).
        if name == "tests.rs" || name == "live_tests.rs" {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            failures.push(format!("{}: unreadable", path.display()));
            continue;
        };
        for literal in production_string_literals(&source) {
            if has_cyrillic(&literal) && !BACKEND_WHITELIST.contains(&literal.as_str()) {
                failures.push(format!("{}: {literal:?}", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "hardcoded Cyrillic in backend string literals (use a `BackendMsg` variant and \
         localize it in `i18n::backend_text`): {failures:#?}"
    );
}

/// The scanner itself: it must see production literals and skip test items.
#[test]
fn scanner_sees_production_literals_and_skips_test_items() {
    let src = r#"
        // "комментарий"
        const A: &str = "боевая строка";
        #[cfg(test)]
        mod tests { const B: &str = "тестовая строка"; }
        #[cfg(test)]
        mod live_tests;
        const C: &str = "after";
    "#;
    let literals = production_string_literals(src);
    assert_eq!(
        literals,
        vec!["боевая строка".to_string(), "after".to_string()]
    );
}

/// Braces and quotes inside raw strings and char literals of a test module
/// must not end the skipped block early.
#[test]
fn scanner_skips_test_modules_containing_raw_strings_and_char_literals() {
    let src = r##"
        #[cfg(test)]
        mod tests {
            const JSON: &str = r#"{ "k": "Авто" "#;
            fn f(c: char) -> bool { c == '{' || c == '"' || c == '}' }
            const D: &str = "тест";
        }
        const C: &str = "after";
    "##;
    assert_eq!(production_string_literals(src), vec!["after".to_string()]);
}

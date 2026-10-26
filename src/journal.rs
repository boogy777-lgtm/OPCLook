use crate::i18n::{Lang, Tr};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// An optionally-localized journal message. When `l10n` is present, `text`
/// holds the Russian rendering (kept for file export and `text` readers) and
/// `render()` formats the message for the current `Lang` on the fly.
#[derive(Clone, Debug)]
pub struct Entry {
    pub time: DateTime<Local>,
    pub level: Level,
    pub text: String,
    pub l10n: Option<(Tr, Vec<String>)>,
}

#[derive(Debug)]
pub struct Journal {
    entries: Vec<Entry>,
    max_entries: usize,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 5000,
        }
    }
}

impl Journal {
    pub fn add(&mut self, level: Level, text: impl Into<String>) {
        let text = text.into();
        self.entries.push(Entry {
            time: Local::now(),
            level,
            text,
            l10n: None,
        });
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(0..excess);
        }
    }

    /// Add a localized entry: stores the Russian rendering as fallback text and
    /// the key+args so it can be re-formatted for the current language at render
    /// time (on-the-fly language switching).
    pub fn add_l(&mut self, level: Level, key: Tr, args: &[&dyn std::fmt::Display]) {
        let rendered = crate::i18n::fmt(Lang::Ru, key, args);
        self.entries.push(Entry {
            time: Local::now(),
            level,
            text: rendered,
            l10n: Some((key, args.iter().map(|a| a.to_string()).collect())),
        });
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(0..excess);
        }
    }

    pub fn info_l(&mut self, key: Tr, args: &[&dyn std::fmt::Display]) {
        self.add_l(Level::Info, key, args);
    }

    pub fn warn_l(&mut self, key: Tr, args: &[&dyn std::fmt::Display]) {
        self.add_l(Level::Warning, key, args);
    }

    pub fn error_l(&mut self, key: Tr, args: &[&dyn std::fmt::Display]) {
        self.add_l(Level::Error, key, args);
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.add(Level::Info, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.add(Level::Warning, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.add(Level::Error, text);
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        self.save_localized(path, Lang::Ru)
    }

    /// Save the journal, rendering localized entries in the given language.
    pub fn save_localized(&self, path: &Path, lang: Lang) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        for entry in &self.entries {
            writeln!(
                file,
                "{} [{}] {}",
                entry.time.format("%Y-%m-%d %H:%M:%S%.3f"),
                level_name(entry.level),
                entry.render(lang)
            )?;
        }
        Ok(())
    }

    pub fn load(&mut self, path: &Path) -> std::io::Result<usize> {
        let text = std::fs::read_to_string(path)?;
        self.entries.clear();
        let mut count = 0;
        for line in text.lines() {
            let text = line.trim_end();
            if text.is_empty() {
                continue;
            }
            self.entries.push(Entry {
                time: Local::now(),
                level: Level::Info,
                text: text.to_string(),
                l10n: None,
            });
            count += 1;
        }
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(0..excess);
        }
        Ok(count)
    }
}

impl Entry {
    /// Render an entry for the given language (localized entries are
    /// re-formatted on the fly; raw entries return their stored text).
    pub fn render(&self, lang: Lang) -> String {
        match &self.l10n {
            Some((key, args)) => {
                let disp: Vec<&dyn std::fmt::Display> =
                    args.iter().map(|s| s as &dyn std::fmt::Display).collect();
                crate::i18n::fmt(lang, *key, &disp)
            }
            None => self.text.clone(),
        }
    }
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Info => "INFO",
        Level::Warning => "WARN",
        Level::Error => "ERROR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_records_level_and_text_in_order() {
        let mut j = Journal::default();
        j.info("first");
        j.warn("second");
        j.error("third");
        j.add(Level::Info, "fourth");
        assert_eq!(j.entries().len(), 4);
        assert_eq!(j.entries()[0].level, Level::Info);
        assert_eq!(j.entries()[0].text, "first");
        assert_eq!(j.entries()[1].level, Level::Warning);
        assert_eq!(j.entries()[1].text, "second");
        assert_eq!(j.entries()[2].level, Level::Error);
        assert_eq!(j.entries()[2].text, "third");
        assert_eq!(j.entries()[3].level, Level::Info);
    }

    #[test]
    fn distinct_level_helpers_map_correctly() {
        let mut j = Journal::default();
        j.info("i");
        j.warn("w");
        j.error("e");
        assert_eq!(j.entries()[0].level, Level::Info);
        assert_eq!(j.entries()[1].level, Level::Warning);
        assert_eq!(j.entries()[2].level, Level::Error);
    }

    #[test]
    fn entries_are_sorted_newest_last() {
        let mut j = Journal::default();
        j.info("a");
        j.info("b");
        j.info("c");
        let times: Vec<_> = j.entries().iter().map(|e| e.time).collect();
        assert!(times.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn append_cap_trims_oldest_when_over_capacity() {
        let mut j = Journal::default();
        let capacity = j.max_entries;
        // Fill past capacity.
        for i in 0..=(capacity + 1) {
            j.info(format!("entry {i}"));
        }
        assert!(j.entries().len() <= capacity);
        assert_eq!(j.entries().len(), capacity);
        // Oldest two entries dropped (0 and 1) -> first kept is "entry 2".
        assert_eq!(
            j.entries().first().unwrap().text.as_str(),
            &format!("entry {}", 2)
        );
        assert_eq!(
            j.entries().last().unwrap().text.as_str(),
            &format!("entry {}", capacity + 1)
        );
    }

    #[test]
    fn below_capacity_does_not_trim() {
        let mut j = Journal::default();
        for i in 0..3 {
            j.info(format!("e{i}"));
        }
        assert_eq!(j.entries().len(), 3);
    }

    #[test]
    fn clear_empties_journal() {
        let mut j = Journal::default();
        j.info("a");
        j.warn("b");
        j.clear();
        assert!(j.entries().is_empty());
        // Still usable after clear.
        j.error("c");
        assert_eq!(j.entries().len(), 1);
    }

    #[test]
    fn save_writes_expected_line_structure() {
        let dir = std::env::temp_dir().join("opcda-egui-journal-structure");
        let path = dir.join("j.log");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut j = Journal::default();
        j.warn("disk low");
        j.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        let line = lines.next().unwrap();
        // "2026-10-25 12:34:56.789 [WARN] disk low"
        assert!(line.contains("[WARN]"), "level marker missing: {line}");
        assert!(line.contains("disk low"), "text missing: {line}");
        assert!(line.ends_with("disk low"), "text must end line: {line}");
        assert!(lines.next().is_none(), "expected exactly one line");

        // INFO/ERROR markers too, for completeness of the format mapping.
        let mut j2 = Journal::default();
        j2.info("i");
        j2.error("e");
        let path2 = dir.join("j2.log");
        j2.save(&path2).unwrap();
        let text2 = std::fs::read_to_string(&path2).unwrap();
        let l2: Vec<&str> = text2.lines().collect();
        assert!(l2[0].contains("[INFO]"));
        assert!(l2[1].contains("[ERROR]"));
    }

    #[test]
    fn load_populates_entries_and_caps() {
        let dir = std::env::temp_dir().join("opcda-egui-journal-load");
        let path = dir.join("in.log");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "line one\nline two\n\n  \nline three\n").expect("write fixture");

        let mut j = Journal::default();
        let count = j.load(&path).unwrap();
        assert_eq!(count, 3, "blank lines are skipped");
        assert_eq!(j.entries().len(), 3);
        assert_eq!(j.entries()[0].text, "line one");
        assert_eq!(j.entries()[1].text, "line two");
        assert_eq!(j.entries()[2].text, "line three");
    }

    #[test]
    fn load_over_capacity_trims_to_max() {
        let dir = std::env::temp_dir().join("opcda-egui-journal-cap");
        let path = dir.join("big.log");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut fixture = String::new();
        for i in 0..(Journal::default().max_entries + 10) {
            fixture.push_str(&format!("e{i}\n"));
        }
        std::fs::write(&path, fixture).unwrap();

        let mut j = Journal::default();
        let count = j.load(&path).unwrap();
        assert_eq!(count, Journal::default().max_entries + 10);
        assert_eq!(j.entries().len(), j.max_entries);
        // Oldest 10 dropped, first kept matches the 11th line.
        assert_eq!(
            j.entries().first().unwrap().text.as_str(),
            &format!("e{}", 10)
        );
    }

    #[test]
    fn save_then_load_roundtrip_preserves_text() {
        let dir = std::env::temp_dir().join("opcda-egui-journal-roundtrip");
        let path = dir.join("rt.log");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut j = Journal::default();
        j.info("alpha");
        j.warn("beta");
        j.error("gamma");
        j.save(&path).unwrap();

        let mut back = Journal::default();
        let count = back.load(&path).unwrap();
        assert_eq!(count, 3);
        // Saved lines carry a timestamp prefix, so the original text is a suffix
        // of each loaded entry's text.
        let texts: Vec<&str> = back.entries().iter().map(|e| e.text.as_str()).collect();
        assert!(texts[0].ends_with("alpha"), "got {texts:?}");
        assert!(texts[1].ends_with("beta"), "got {texts:?}");
        assert!(texts[2].ends_with("gamma"), "got {texts:?}");
    }
}

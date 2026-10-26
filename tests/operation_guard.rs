//! Static guard: operation identity must stay typed. No BFF/app source may
//! regress to raw operation strings, string comparison or the old `op::`
//! constant module.

use std::fs;
use std::path::{Path, PathBuf};

fn first_party_sources(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs(&root.join("src").join("app"), &mut files);
    collect_rs(&root.join("src").join("bff"), &mut files);
    files
}

/// Collect every `.rs` file under `dir`. Both modules are directories since
/// `src/app.rs` / flat `src/bff` were split into `mod.rs` plus submodules.
fn collect_rs(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

/// True when `needle` occurs as an identifier token (so `ManuallyDrop::new`
/// does not match `op::`).
fn contains_ident_token(line: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = line[from..].find(needle) {
        let at = from + offset;
        let prev_is_ident = line[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if !prev_is_ident {
            return true;
        }
        from = at + needle.len();
    }
    false
}

#[test]
fn no_raw_operation_strings_or_dispatch() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forbidden = ["operation == \"", "operation: \"", "operation.to_string()"];
    let mut hits = Vec::new();

    for file in first_party_sources(&root) {
        let text = fs::read_to_string(&file).expect("read source");
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let mut matched = forbidden.iter().any(|pattern| line.contains(pattern));
            if contains_ident_token(line, "op::") {
                matched = true;
            }
            if matched {
                hits.push(format!("{}:{}: {}", file.display(), index + 1, line.trim()));
            }
        }
    }

    assert!(
        hits.is_empty(),
        "raw operation string/dispatch found (use the Operation enum):\n{}",
        hits.join("\n")
    );
}

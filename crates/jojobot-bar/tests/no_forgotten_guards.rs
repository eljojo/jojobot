//! **No source forgets a value, so no guard that owns a directory is kept
//! from dropping.** A scratch directory is removed by the guard that owns it
//! when the test returns, fails or panics. `std::mem::forget` on one of those
//! guards leaves the directory behind on every run, green ones included, which
//! is how tens of thousands of them piled up. A directory that has to outlive
//! its test is handed to a hook that removes it when the process exits.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name == "fixtures" {
            continue;
        }
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn no_source_forgets_a_value() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(
        files.len() > 100,
        "the scan found only {} files, so it is not looking where the sources are",
        files.len()
    );
    // Built in two pieces, so this file does not name what it forbids.
    let needle = format!("mem::{}(", "forget");
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains(&needle) {
                found.push(format!("{}:{}", file.display(), n + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "these forget a value, and a guard that is forgotten never removes its directory: {found:#?}"
    );
}

//! No served text carries a run of spaces.
//!
//! A string literal wrapped across source lines with a trailing backslash
//! drops the next line's indentation. A literal written on one line, or joined
//! from two lines without the backslash at the join, keeps the indentation as a
//! run of spaces in the middle of a sentence, and every caller reads it that
//! way. Asserting a field is non-empty cannot see this, so the shape is
//! asserted here, over every string literal in the non-test source of the
//! two crates that write what a caller reads: that is where every refusal, description and error text is
//! written, so one place reaches them all. The adapters crate is not read: it holds SQL.
//!
//! Raw strings are skipped: the orientation essay is markdown, and markdown
//! indents on purpose. A literal is read as the compiler reads it: after a
//! backslash and a newline, the whitespace that follows is not part of it.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The files whose literals reach a caller: every `.rs` file under a crate's
/// `src`, except the ones that exist only to support tests.
fn served_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable source dir") {
        let path = entry.expect("readable dir entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name == "target" || name == "tests" || name == "testing" {
                continue;
            }
            served_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !name.contains("test")
            // Declared `#[cfg(test)]` by the crate root, so no caller reads them.
            && name != "harness.rs"
            && name != "surface.rs"
        {
            out.push(path);
        }
    }
}

/// Every run of two or more spaces inside a non-raw string literal, as
/// `line: the literal's text around the run`. Code after the first
/// `#[cfg(test)]` at the start of a line is a test module and is not read.
fn space_runs(source: &str) -> Vec<String> {
    let end = source
        .match_indices("\n#[cfg(test)]")
        .next()
        .map_or(source.len(), |(at, _)| at);
    let chars: Vec<char> = source[..end].chars().collect();
    let mut found = Vec::new();
    let mut line = 1;
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        match c {
            '\n' => line += 1,
            '/' if chars.get(at + 1) == Some(&'/') => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                continue;
            }
            '/' if chars.get(at + 1) == Some(&'*') => {
                let mut depth = 0;
                while at < chars.len() {
                    if chars[at] == '\n' {
                        line += 1;
                    }
                    if chars[at] == '/' && chars.get(at + 1) == Some(&'*') {
                        depth += 1;
                        at += 2;
                    } else if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
                        depth -= 1;
                        at += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        at += 1;
                    }
                }
                continue;
            }
            // A character literal holding a quote is not the start of a string.
            '\'' if chars.get(at + 1) == Some(&'"') && chars.get(at + 2) == Some(&'\'') => {
                at += 3;
                continue;
            }
            '\'' if chars.get(at + 1) == Some(&'\\') && chars.get(at + 3) == Some(&'\'') => {
                at += 4;
                continue;
            }
            // A raw string: skipped whole.
            'r' if at == 0 || !(chars[at - 1].is_alphanumeric() || chars[at - 1] == '_') => {
                let mut hashes = 0;
                let mut probe = at + 1;
                while chars.get(probe) == Some(&'#') {
                    hashes += 1;
                    probe += 1;
                }
                if chars.get(probe) == Some(&'"') {
                    probe += 1;
                    loop {
                        match chars.get(probe) {
                            None => break,
                            Some('\n') => line += 1,
                            Some('"')
                                if (1..=hashes).all(|n| chars.get(probe + n) == Some(&'#')) =>
                            {
                                probe += hashes;
                                break;
                            }
                            _ => {}
                        }
                        probe += 1;
                    }
                    at = probe + 1;
                    continue;
                }
            }
            '"' => {
                at += 1;
                let mut literal = String::new();
                let start_line = line;
                while at < chars.len() && chars[at] != '"' {
                    match chars[at] {
                        '\\' if chars.get(at + 1) == Some(&'\n') => {
                            // A continuation: the newline and the whitespace
                            // after it are not part of the literal.
                            at += 1;
                            while at < chars.len() && chars[at].is_whitespace() {
                                if chars[at] == '\n' {
                                    line += 1;
                                }
                                at += 1;
                            }
                            continue;
                        }
                        '\\' => {
                            literal.push(chars[at]);
                            at += 1;
                            if let Some(escaped) = chars.get(at) {
                                literal.push(*escaped);
                            }
                        }
                        '\n' => {
                            line += 1;
                            literal.push('\n');
                        }
                        other => literal.push(other),
                    }
                    at += 1;
                }
                if let Some(run) = literal.find("  ") {
                    let from = literal[..run].chars().rev().take(30).collect::<String>();
                    let from: String = from.chars().rev().collect();
                    found.push(format!("line {start_line}: …{from}[  ]…"));
                }
            }
            _ => {}
        }
        at += 1;
    }
    found
}

#[test]
fn no_served_text_carries_a_run_of_spaces() {
    let mut files = Vec::new();
    // The two crates that write what a caller reads. The adapters hold SQL,
    // whose line breaks and indentation are the query's layout and reach no
    // caller.
    for krate in ["jojobot-domain", "jojobot-mcp"] {
        served_sources(
            &workspace_root().join("crates").join(krate).join("src"),
            &mut files,
        );
    }
    assert!(
        files.len() > 50,
        "the scan reached only {} files, so it is not reading the workspace",
        files.len()
    );
    let mut violations = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("readable source file");
        for run in space_runs(&text) {
            violations.push(format!("{}: {run}", file.display()));
        }
    }
    violations.sort();
    assert!(
        violations.is_empty(),
        "string literals with a run of two or more spaces — a wrapped literal must end its line \
         with a backslash so the indentation is dropped:\n{}",
        violations.join("\n")
    );
}

/// The scanner finds the defect it exists for and passes the ordinary shapes,
/// so a green `no_served_text_carries_a_run_of_spaces` means the corpus is
/// clean and not that the scanner reads nothing.
#[test]
fn the_scanner_flags_a_run_and_passes_a_continuation() {
    let wrapped = "fn f() -> String { \"one two \\\n        three\".into() }";
    assert!(space_runs(wrapped).is_empty(), "{:?}", space_runs(wrapped));
    let kept = "fn f() -> String { \"one two        three\".into() }";
    assert_eq!(space_runs(kept).len(), 1);
    let raw = "const E: &str = r#\"  indented on purpose\"#;";
    assert!(space_runs(raw).is_empty());
    let comment = "// a  comment\nfn f() {}";
    assert!(space_runs(comment).is_empty());
    let after_tests = "fn f() {}\n#[cfg(test)]\nmod tests { const S: &str = \"a  b\"; }";
    assert!(space_runs(after_tests).is_empty());
    let quote = "fn f() { let q = '\"'; let s = \"a  b\"; }";
    assert_eq!(space_runs(quote).len(), 1);
}

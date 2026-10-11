//! **The byte-exact round trip, over a corpus.**
//!
//! A document put in and read back must be the document that went in, character
//! for character. The corpus is synthetic files whose edges a trimming store would
//! lose (no final newline, CRLF, blank runs, indented and tab-trailed headings,
//! fences, frontmatter, Unicode) and the repository's own public documents, which
//! are long, real and already free of anything private.

use std::path::{Path, PathBuf};

use jojobot_domain::memory::document::{join, split};
use jojobot_domain::memory::validate_content;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    found.sort();
    found
}

fn corpus() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut files = files_in(&root.join("crates/jojobot-domain/tests/fixtures/documents"));
    files.extend(files_in(&root.join("pm/feature-briefs")));
    for name in ["README.md", "CONTRIBUTING.md"] {
        files.push(root.join(name));
    }
    files
}

/// 🚨 **Every file in the corpus splits and joins to itself, and every section's
/// words can be a claim's words.** The corpus is counted first: a sweep over
/// nothing reports success.
#[test]
fn every_file_in_the_corpus_joins_back_to_itself() {
    let files = corpus();
    assert!(
        files.len() >= 30,
        "the corpus holds {} files, which is too few to be the corpus",
        files.len()
    );
    let mut sections_seen = 0;
    for path in &files {
        let text =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let sections = split(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            join(&sections),
            text,
            "{} did not round-trip",
            path.display()
        );
        for section in &sections {
            validate_content(&section.first_line).unwrap_or_else(|e| {
                panic!(
                    "{}: {:?} is not a claim: {e}",
                    path.display(),
                    section.first_line
                )
            });
        }
        sections_seen += sections.len();
    }
    assert!(
        sections_seen >= 200,
        "only {sections_seen} sections in the whole corpus"
    );
}

/// **The synthetic files hold the shapes they are named for**, so the sweep above
/// is a sweep over those shapes and not over files that lost them.
#[test]
fn the_synthetic_files_hold_the_shapes_they_are_named_for() {
    let dir = workspace_root().join("crates/jojobot-domain/tests/fixtures/documents");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).expect("a fixture");
    assert!(!read("no_final_newline.md").ends_with('\n'));
    assert!(read("crlf.md").contains("\r\n"));
    assert!(read("edges.md").starts_with("\n\n"));
    assert!(read("frontmatter.md").ends_with("\n\n\n"));
    let fenced = split(&read("fenced.md")).expect("splits");
    let firsts: Vec<&str> = fenced.iter().map(|s| s.first_line.as_str()).collect();
    assert_eq!(
        firsts,
        ["# Code", "## Real heading"],
        "a fenced # is not a heading"
    );
    // **Where headings split, not only that the join restores them**: an indent
    // of two is a heading, an indent of four is code, and a bare `#` is a heading.
    let edges = split(&read("edges.md")).expect("splits");
    let firsts: Vec<&str> = edges.iter().map(|s| s.first_line.as_str()).collect();
    assert_eq!(
        firsts,
        [
            "# Indented heading",
            "#",
            "# Trailing spaces",
            "## Tab after"
        ],
        "{edges:?}"
    );
    assert!(edges[0].body.contains("    # four spaces is code"));
}

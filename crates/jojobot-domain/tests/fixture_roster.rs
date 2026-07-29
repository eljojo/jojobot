//! The fixture roster — the machine gate on life specifics.
//!
//! Every entity handle written anywhere in this workspace's sources must name
//! an openly fictional thing: the Simpsons universe, greek letters, or an
//! obviously synthetic token. The roster below is the complete allowlist.
//! Adding a name is a conscious, reviewed diff — and it is NEVER a person,
//! place, event, or organization from the operator's life, no matter which
//! branch or commit it would ride in on.
//!
//! No denylist exists anywhere: a list of forbidden names would re-embed the
//! very strings this gate exists to keep out.
//!
//! Scope: handle-shaped string literals (`kind:slug`) in every `.rs` file
//! under `crates/`, comments included — comments are where past leaks lived.
//! Bare slugs handed to constructors are out of reach for a text scan; the
//! handle form is where every leak so far has entered.

use std::fs;
use std::path::{Path, PathBuf};

const KINDS: [&str; 9] = [
    "person", "place", "event", "work", "thing", "org", "topic", "project", "bot",
];

/// The complete allowlist. Keep it sorted; keep it fictional.
const ROSTER: &[&str] = &[
    "bot:delta",
    "bot:epsilon",
    "bot:gamm",
    "bot:gamma",
    "bot:nobody",
    "bot:otto",
    "event:departure-flight",
    "event:winter-fest",
    "org:globex",
    "org:guild",
    "org:north-trail-club",
    "org:springfield-movers",
    "person:a",
    "person:alpha",
    "person:alpha-2",
    "person:alpha-a",
    "person:alpha-b",
    "person:alpha-one",
    "person:alpha-two",
    "person:alphaa",
    "person:alphonse",
    "person:barney-gumble",
    "person:bet",
    "person:beta",
    "person:bodoque",
    "person:contract-orient",
    "person:cosme-fulanito",
    "person:frontdoor-probe",
    "person:ghost",
    "person:ghostly",
    "person:hijacked",
    "person:homer",
    "person:homer-simpson",
    "person:kappa",
    "person:milhouse",
    "person:ned-flanders",
    "person:otto",
    "person:patana",
    "person:someone-else",
    "person:tulio",
    "person:x",
    "person:y",
    "person:zenit",
    "person:zenith",
    "person:zzz",
    "place:a",
    "place:atlas",
    "place:bet",
    "place:capital-city",
    "place:far-country",
    "place:leftorium",
    "place:north-haverbrook",
    "place:north-trail",
    "place:north-trail-2",
    "place:riverbend",
    "place:riverbnd",
    "place:shelbyville",
    "place:springfield",
    "place:trail-spot",
    "place:x",
    "project:atlas",
    "project:jojobot-server",
    "thing:red-bike",
    "thing:red-bikee",
    "topic:widgets",
    "work:first-mix",
];

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable source dir") {
        let path = entry.expect("readable dir entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `kind:slug` occurrence in the text, comments included.
fn handles_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for kind in KINDS {
        let needle = format!("{kind}:");
        for (idx, _) in text.match_indices(&needle) {
            // A word/URL character right before means this is the tail of
            // something longer (e.g. `.org:` in a URL), not a handle.
            if idx > 0 {
                let prev = text.as_bytes()[idx - 1];
                if prev.is_ascii_alphanumeric() || matches!(prev, b'_' | b'-' | b'.') {
                    continue;
                }
            }
            let slug: String = text[idx + needle.len()..]
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                .collect();
            if slug.is_empty() || !slug.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                continue;
            }
            found.push(format!("{kind}:{slug}"));
        }
    }
    found
}

/// **Every entry on the allowlist is one the workspace actually uses.**
///
/// The roster's value is that adding a name is a conscious, reviewed diff. An
/// entry nothing uses is permission granted to nothing: it widens the gate
/// without a caller, and it turns the list from a record of what is in the repo
/// into a pool of names anybody may reach for without review. Both of those
/// undo the reason the gate exists.
///
/// It also means the list stops being readable as evidence. "These are the
/// fictional names this repo contains" is a claim somebody can check; "these
/// are the fictional names this repo contains, plus some it used to" is not.
#[test]
fn the_roster_carries_no_name_the_workspace_has_stopped_using() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    let mut files = Vec::new();
    rust_sources(&crates, &mut files);
    let corpus: String = files
        .iter()
        .filter(|f| f.file_name().is_some_and(|n| n != "fixture_roster.rs"))
        .map(|f| fs::read_to_string(f).expect("readable source file"))
        .collect();

    let orphaned: Vec<&str> = ROSTER
        .iter()
        .copied()
        .filter(|handle| !corpus.contains(handle))
        .collect();
    assert!(
        orphaned.is_empty(),
        "roster entries nothing in the workspace uses — delete them, or the allowlist \
         stops being a record of what is here and becomes a pool of names nobody \
         reviewed the use of:\n{}",
        orphaned.join("\n")
    );
}

#[test]
fn every_handle_in_the_workspace_is_on_the_fictional_roster() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    let mut files = Vec::new();
    rust_sources(&crates, &mut files);
    assert!(files.len() > 10, "the scan must actually see the workspace");

    let mut violations = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("readable source file");
        for handle in handles_in(&text) {
            if !ROSTER.contains(&handle.as_str()) {
                violations.push(format!("{} in {}", handle, file.display()));
            }
        }
    }
    violations.sort();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "handles outside the fictional roster — if the name is openly fictional, \
         add it to ROSTER in a conscious diff; if it names anything real from the \
         operator's life, it must not enter this repo at all:\n{}",
        violations.join("\n")
    );
}

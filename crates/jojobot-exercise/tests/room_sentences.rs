//! **The sentences a room prints, held to reading as one line.**
//!
//! A lock's failure sentence is the whole of what a person gets when a run
//! goes wrong, and **nothing exercises it**: it is built only on the failing
//! branch of a check nobody wants to take. So a sentence can be broken for
//! months and the suite stays green — which is the definition of a string that
//! needs a mechanism rather than a rule.
//!
//! **The defect this exists for.** These sentences are written with `\`
//! continuations, and `rustfmt` may rejoin the lines while leaving the
//! indentation inside the literal. The result serves a run of spaces
//! mid-sentence. Three of them shipped into one commit, written by somebody
//! who could recite the hazard; one was found by accident and two by scanning.
//! Reading does not find these.
//!
//! ⛔️ **Shape only, never prose quality.** There is no predicate for a good
//! sentence. This asks two things a machine can answer: no run of two or more
//! spaces between non-space characters, and no stray control character.
//!
//! ⛔️ **Not a reason to stop using `\` continuations.** They are how a long
//! sentence stays readable in source, and this guard is what makes them safe
//! to keep using.

use std::fs;
use std::path::{Path, PathBuf};

/// The workspace root, from this crate's own manifest.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// **Why a piece of text does not read as one line**, or nothing when it does.
///
/// **An escaped newline or tab is a break the author asked for**, so the text
/// is split on those first and each segment is judged on its own. Without
/// that, deliberate indentation after a `\n` — which is how this crate lays
/// out its own report — reads as a collapse.
///
/// **A run of spaces is only a fault when something precedes it on its own
/// segment.** That is what separates the two cases: collapsed padding sits
/// mid-sentence with words either side, while a `\` continuation's indentation
/// is stripped by the compiler and never reaches this text at all.
fn unreadable(text: &str) -> Option<String> {
    for segment in text.split("\\n").flat_map(|part| part.split("\\t")) {
        let chars: Vec<char> = segment.chars().collect();
        for at in 1..chars.len().saturating_sub(1) {
            if chars[at] == ' ' && chars[at + 1] == ' ' && !chars[at - 1].is_whitespace() {
                let from = at.saturating_sub(30);
                let to = (at + 30).min(chars.len());
                let around: String = chars[from..to].iter().collect();
                return Some(format!("a run of spaces mid-sentence, around: …{around}…"));
            }
        }
        if let Some(stray) = segment.chars().find(|one| one.is_control()) {
            return Some(format!("a control character in the text: {stray:?}"));
        }
    }
    None
}

/// **Every string literal in a Rust source, with the line it opens on.**
///
/// The literal is kept as the compiler would build it in the one way that
/// matters here: **a `\` at end of line eats the newline and the indentation
/// after it**, which is exactly the continuation this guard must not flag.
/// Every other escape is carried through as its two characters, so `\n` stays
/// visible to [`unreadable`] as the break the author asked for.
fn literals_in(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let (mut at, mut line) = (0usize, 1usize);
    while at < chars.len() {
        match chars[at] {
            '\n' => {
                line += 1;
                at += 1;
            }
            // A comment is prose for a reader of the source, not a sentence
            // the room prints.
            '/' if chars.get(at + 1) == Some(&'/') => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
            }
            '"' => {
                let opened = line;
                let mut value = String::new();
                at += 1;
                while at < chars.len() && chars[at] != '"' {
                    if chars[at] == '\\' {
                        match chars.get(at + 1) {
                            // The continuation: the newline and the whitespace
                            // after it are not part of the string.
                            Some('\n') => {
                                line += 1;
                                at += 2;
                                while at < chars.len() && chars[at].is_whitespace() {
                                    if chars[at] == '\n' {
                                        line += 1;
                                    }
                                    at += 1;
                                }
                                continue;
                            }
                            Some(escaped) => {
                                value.push('\\');
                                value.push(*escaped);
                                at += 2;
                                continue;
                            }
                            None => break,
                        }
                    }
                    if chars[at] == '\n' {
                        line += 1;
                    }
                    value.push(chars[at]);
                    at += 1;
                }
                at += 1;
                out.push((opened, value));
            }
            _ => at += 1,
        }
    }
    out
}

/// **Every sentence a room document declares**, with the line it sits on. A
/// `say` line is what a person reads when the lock beside it does not hold.
fn said_in(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter_map(|(at, line)| {
            line.strip_prefix("say ")
                .map(|said| (at + 1, said.trim().to_string()))
        })
        .collect()
}

/// **The room-suite test files.** Every string literal in each of these is an
/// assert! sentence, the same shape as `src/checks.rs`: nothing exercises the
/// failing branch, so a collapsed one survives a green suite. This is where
/// the guard's own defect landed — `year_room.rs`, fixed at `91962c8` — and
/// the boundary covers exactly the five files that shape occurs in, no more.
///
/// ⛔️ **Deliberately not wider.** A workspace-wide scan of every string
/// literal finds SQL text, room-DSL fixture strings and raw multi-line
/// literals alongside real sentences, and the predicate cannot tell them
/// apart — only file selection can. Widening further would need a
/// per-string allow-list, which is the guard somebody eventually turns off.
const ROOM_SUITE: [&str; 5] = [
    "bike_room.rs",
    "handover_room.rs",
    "ledger_room.rs",
    "loop_room.rs",
    "year_room.rs",
];

/// Every string literal in the room-suite files, as `path, line, text`.
fn room_suite_sentences() -> Vec<(PathBuf, usize, String)> {
    let dir = workspace_root().join("crates/jojobot-exercise/tests");
    let mut out = Vec::new();
    for name in ROOM_SUITE {
        let path = dir.join(name);
        let text = fs::read_to_string(&path).expect("a readable room-suite test");
        for (line, said) in literals_in(&text) {
            out.push((path.clone(), line, said));
        }
    }
    out
}

/// The three corpora, as `path, line, text`.
fn every_sentence() -> Vec<(PathBuf, usize, String)> {
    let root = workspace_root();
    let mut out = Vec::new();

    let hatches = root.join("crates/jojobot-exercise/src/checks.rs");
    let text = fs::read_to_string(&hatches).expect("the hatches are readable");
    for (line, said) in literals_in(&text) {
        out.push((hatches.clone(), line, said));
    }

    let rooms = root.join("crates/jojobot-exercise/rooms");
    for entry in fs::read_dir(&rooms).expect("the rooms are readable") {
        let path = entry.expect("a readable room").path();
        if path.extension().is_some_and(|one| one == "md") {
            let text = fs::read_to_string(&path).expect("a readable room document");
            for (line, said) in said_in(&text) {
                out.push((path.clone(), line, said));
            }
        }
    }

    out.extend(room_suite_sentences());
    out
}

/// 🚨 **The predicate itself, asked both ways.**
///
/// A shape check that cannot fail is the defect it exists for, one level up.
/// **The samples are the real thing rather than invented**: the collapsed one
/// is the literal this guard was written after, padding and all, and the
/// continued one is the same sentence as the compiler actually builds it.
#[test]
fn the_predicate_tells_a_collapsed_sentence_from_a_continued_one() {
    let collapsed = "the trace of person:bart#f1 came back with no writes on it at all, so \
                     nothing              was measured";
    assert!(
        unreadable(collapsed).is_some(),
        "the padding rustfmt left inside a rejoined literal is what this exists to catch",
    );

    let continued = "the trace of person:bart#f1 came back with no writes on it at all, so \
                     nothing was measured";
    assert_eq!(
        unreadable(continued),
        None,
        "a sentence written with a continuation reads as one line and must not be flagged",
    );

    // **The break an author asked for is not a collapse.** This crate lays its
    // own report out this way, and a guard that flagged it would be turned off.
    assert_eq!(
        unreadable("\\n  runs offered: 3\\n  world: everything"),
        None,
        "indentation after an escaped newline is deliberate",
    );

    // And the second half of the shape question.
    assert!(
        unreadable("a sentence carrying \u{7} a bell").is_some(),
        "a stray control character is caught too",
    );
}

/// **The room-suite corpus alone reaches real content.**
///
/// The other floor, on `every_sentence()`, is already cleared by the hatches
/// and the room documents before the room suite adds a single literal — so it
/// cannot catch this corpus silently contributing nothing. This one asks the
/// room suite by itself, which is what makes a broken path or an empty scan
/// here visible.
#[test]
fn the_room_suite_scan_actually_reaches_its_five_files() {
    let sentences = room_suite_sentences();
    assert!(
        sentences.len() > 1000,
        "the room-suite scan found only {} literals, but the five files hold over 1300 between \
         them, so it is reading almost nothing and a broken predicate would pass unnoticed",
        sentences.len(),
    );
}

/// **Every sentence the room can print reads as one line.**
///
/// The floor is asserted with it: a scanner that silently found nothing would
/// pass this test on an empty corpus, which is the same failure as the strings
/// never being exercised.
#[test]
fn every_sentence_the_room_can_print_reads_as_one_line() {
    let sentences = every_sentence();
    assert!(
        sentences.len() > 1300,
        "the scan found only {} sentences, so it is reading almost nothing and would pass \
         whatever the rooms say",
        sentences.len(),
    );

    let broken: Vec<String> = sentences
        .iter()
        .filter_map(|(path, line, said)| {
            unreadable(said).map(|why| format!("{}:{line} — {why}", path.display()))
        })
        .collect();
    assert!(
        broken.is_empty(),
        "sentences a room prints that do not read as one line:\n{}",
        broken.join("\n"),
    );
}

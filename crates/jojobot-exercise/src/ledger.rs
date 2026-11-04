//! **The coverage ledger of the rooms: every shipped lock and every integration
//! case of this crate, as a row.**
//!
//! Rule 767 moves the rooms into fewer, faster lanes and loses no coverage. A
//! lane cannot be moved against a claim nobody wrote down, so this is the claim.
//!
//! **A `lock` row** is one lock of one shipped room: the input it is asked of (the
//! room's playbook up to the phase the lock reads, the window the answer is read
//! in, the query and the alternates tried when the first holds nothing), the
//! oracle it holds the answer to, what it says when it does not hold, the cases
//! that run the room and the proof that it can fail. **A `case` row** is one
//! integration test of this crate, so a regression kept as a test rather than as
//! a lock is in the ledger too, with the claim in the comment above it.
//!
//! The rows are read from the shipped rooms, the failability registry and the
//! test sources, never typed, so the file the repository keeps cannot drift from
//! them without a test going red. The last column is the case that will hold the
//! assertion after a move. It is empty until a move names one.

use std::path::{Path, PathBuf};

use crate::failability::{NEGATIVE_CONTROLS, PENDING};
use crate::lock::{Asks, Expect, When};
use crate::{expectations, lock};

/// The columns of a row, in order.
pub const HEADER: &str = "kind\troom\tlock\tphase\twindow\tasks\talso\toracle\tunkept\tfailure\thistory\theld_by\tproof\tmoves_to";

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// One text with the characters that would break a row folded to a space.
fn one_line(text: &str) -> String {
    text.split(['\t', '\n', '\r']).collect::<Vec<_>>().join(" ")
}

fn oracle(expects: &[Expect]) -> String {
    expects
        .iter()
        .map(|expect| match expect {
            Expect::Carries(text) => format!("carries {text:?}"),
            Expect::CarriesAnyCase(text) => format!("carries-any-case {text:?}"),
            Expect::CarriesEither(any) => format!("carries-either {any:?}"),
            Expect::Lacks(text) => format!("lacks {text:?}"),
            Expect::AtLeast(count, text) => format!("at-least {count} {text:?}"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn window(when: When) -> &'static str {
    match when {
        When::Live => "live",
        When::OwnPhase => "own-phase",
        When::PhaseEnd => "phase-end",
    }
}

/// The sorted names of the test files under `tests/`, with their text.
fn test_sources() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = std::fs::read_dir(tests_dir())
        .expect("the tests directory must read")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "rs" {
                return None;
            }
            let name = format!("tests/{}", path.file_name()?.to_str()?);
            Some((name, std::fs::read_to_string(&path).ok()?))
        })
        .collect();
    files.sort();
    files
}

/// **The test files that run a room**: those naming its constant, the room
/// document's stem in capitals with `_ROOM` after it.
fn runners_of(room: &str, sources: &[(String, String)]) -> String {
    let stem = Path::new(room)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_uppercase()
        .replace('-', "_");
    let token = format!("{stem}_ROOM");
    let named: Vec<&str> = sources
        .iter()
        .filter(|(_, text)| {
            text.match_indices(&token).any(|(at, _)| {
                let before = text[..at].chars().next_back();
                let after = text[at + token.len()..].chars().next();
                !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                    && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
            })
        })
        .map(|(name, _)| name.as_str())
        .collect();
    named.join(", ")
}

/// **Every integration test of this crate**, with the claim in the comment above
/// it: `(file, function, claim)`.
fn cases() -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    for (file, text) in test_sources() {
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            let attribute = line.trim_start();
            if !(attribute.starts_with("#[test]") || attribute.starts_with("#[tokio::test")) {
                continue;
            }
            let Some(name) = lines[at + 1..].iter().find_map(|next| {
                let next = next.trim_start();
                let rest = next
                    .strip_prefix("async fn ")
                    .or_else(|| next.strip_prefix("fn "))?;
                Some(rest.split('(').next()?.trim().to_string())
            }) else {
                continue;
            };
            let mut claim: Vec<&str> = Vec::new();
            for above in lines[..at].iter().rev() {
                let above = above.trim_start();
                if let Some(doc) = above
                    .strip_prefix("///")
                    .or_else(|| above.strip_prefix("//"))
                {
                    claim.push(doc.trim());
                } else if above.starts_with("#[") {
                    continue;
                } else {
                    break;
                }
            }
            claim.reverse();
            found.push((file.clone(), name, claim.join(" ")));
        }
    }
    found
}

/// **The ledger, whole**: the header, one row per lock of every shipped room in
/// the order the rooms are shipped, then one row per integration test.
pub fn render() -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    let sources = test_sources();
    for room in expectations::shipped_rooms() {
        let runners = runners_of(room, &sources);
        for lock in lock::locks_of(room) {
            let asks = match &lock.asks {
                Asks::Query { verb, args } => format!("{verb} {args}"),
                Asks::Check(name) => format!("check {name}"),
            };
            let also = lock
                .also
                .iter()
                .map(|(verb, args)| format!("{verb} {args}"))
                .collect::<Vec<_>>()
                .join(" | ");
            let phase = lock.phase_key.clone().unwrap_or_default();
            let history = format!(
                "playbook {room}, through {}",
                if phase.is_empty() {
                    "the end of the room"
                } else {
                    &phase
                }
            );
            let proof = match NEGATIVE_CONTROLS
                .iter()
                .find(|c| c.room == room && c.lock == lock.name)
            {
                Some(control) => format!("{}::{}", control.file, control.function),
                None if PENDING
                    .iter()
                    .any(|p| p.room == room && p.lock == lock.name) =>
                {
                    "pending".to_string()
                }
                None => "unnamed".to_string(),
            };
            let row = [
                "lock".to_string(),
                room.to_string(),
                lock.name.clone(),
                phase,
                window(lock.when).to_string(),
                asks,
                also,
                oracle(&lock.expects),
                lock.unkept.clone().unwrap_or_default(),
                lock.say.clone(),
                history,
                format!("lock of {room}, run by {runners}"),
                proof,
                String::new(),
            ];
            out.push_str(&row.map(|cell| one_line(&cell)).join("\t"));
            out.push('\n');
        }
    }
    for (file, name, claim) in cases() {
        let row = [
            "case".to_string(),
            String::new(),
            format!("{file}::{name}"),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            claim,
            String::new(),
            file,
            String::new(),
            String::new(),
        ];
        out.push_str(&row.map(|cell| one_line(&cell)).join("\t"));
        out.push('\n');
    }
    out
}

/// How many integration tests the test sources hold, counted apart from
/// [`cases`] so a case the reader above misses is found.
pub fn counted_cases() -> usize {
    test_sources()
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("#[test]") || line.starts_with("#[tokio::test")
        })
        .count()
}

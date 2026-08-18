//! The fixture roster — the machine gate on life specifics.
//!
//! Every entity handle written anywhere in this workspace's sources must name
//! an openly fictional thing: a character from the Simpsons, South Park,
//! Family Guy or Bob's Burgers, a greek letter, or an obviously synthetic
//! token. The roster below is the complete allowlist.
//! Adding a name is a conscious, reviewed diff — and it is NEVER a person,
//! place, event, or organization from the operator's life, no matter which
//! branch or commit it would ride in on.
//!
//! No denylist exists anywhere: a list of forbidden names would re-embed the
//! very strings this gate exists to keep out.
//!
//! Scope: handle-shaped text (`kind:slug`) in every `.rs`, `.md` and `.json`
//! file under `crates/`, and in the markdown at the workspace root, which is
//! where this repository's prose about itself lives. A comment is read on the
//! same terms as code and on no wider ones — the handle form is what the scan
//! looks for, wherever it sits, and comments are where past leaks lived. Bare
//! slugs handed to constructors are out of reach for a text scan; the handle
//! form is where every leak so far has entered.
//!
//! **A life specific written in ordinary words passes this gate, because
//! nothing here is looking for it.** A comment, a doc string or a fixture that
//! names a real person, place or event in prose carries nothing handle-shaped,
//! so this suite has no opinion on it. A green run says one thing: no unlisted
//! handle. It is not a clearance for the text around one. What holds that line
//! is somebody's attention, and there is no second gate behind it.

use jojobot_domain::memory::EntityKind;
use std::fs;
use std::path::{Path, PathBuf};

/// Every kind token a handle can carry, **derived from the enum rather than
/// copied out of it**. A hand-written copy is a list that drifts silently: this
/// one sat a kind behind for the whole life of `pet`, and a kind the scan does
/// not know is a kind whose handles are never compared against the roster at
/// all.
fn kinds() -> Vec<&'static str> {
    EntityKind::ALL.iter().map(|k| k.as_token()).collect()
}

/// The complete allowlist. Keep it sorted; keep it fictional.
///
/// **`bot:assistant` is the one entry that is not a fixture**, and it is here
/// deliberately rather than by drift. It is the default identity jojobot ships
/// with — engine material, naming a ROLE the way the orientation essay names
/// "the operator". It identifies nobody's instance and nobody's life, which is
/// the line this file exists to hold. Every other name here is fictional and
/// must stay that way.
const ROSTER: &[&str] = &[
    "bot:assistant",
    "bot:delta",
    "bot:epsilo",
    "bot:epsilon",
    "bot:gamm",
    "bot:gamma",
    "bot:nobody",
    "bot:otto",
    "bot:smoke-gamma",
    "bot:worker-1",
    "bot:worker-2",
    "event:birthday-party",
    "event:departure-flight",
    "event:the-booking",
    "event:the-jotting",
    "event:erosion-review",
    "event:moe-open-mic",
    "event:otto-benefit-show",
    "event:leaving-party",
    "event:lisa-quartet-night",
    "event:trail-survey",
    "event:winter-fest",
    "org:globex",
    "org:guild",
    "org:north-trail-club",
    "org:springfield-cyclery",
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
    "person:bart",
    "person:bet",
    "person:beta",
    "person:bodoque",
    "person:contract-derived",
    "person:contract-nobody",
    "person:contract-fields",
    "person:contract-orient",
    "person:cosme-fulanito",
    "person:frontdoor-probe",
    "person:ghost",
    "person:ghostly",
    "person:homer",
    "person:homer-simpson",
    "person:kappa",
    "person:maude",
    "person:milhouse",
    "person:ned-flander",
    "person:ned-flanders",
    "person:otto",
    "person:patana",
    "person:smoke-alfa",
    "person:smoke-alpha",
    "person:someone-else",
    "person:tulio",
    "person:x",
    "person:y",
    "person:zenit",
    "person:zenith",
    "person:zzz",
    "person:zzz-nobody",
    "pet:santas-little-helper",
    "pet:snowball",
    "place:a",
    "place:atlas",
    "place:bet",
    "place:capital-citty",
    "place:capital-city",
    "place:far-country",
    "place:golden-north-trail",
    "place:leftorium",
    "place:moes-tavern",
    "place:moes",
    "place:north-haverbook",
    "place:north-haverbrook",
    "place:north-trail",
    "place:north-trail-2",
    "place:riverbend",
    "place:riverbnd",
    "place:shelbyvile",
    "place:shelbyville",
    "place:springfeild",
    "place:springfield",
    "place:springfield-mall",
    "place:trail-spot",
    "place:x",
    "project:atlas",
    "project:atlas-two",
    "project:atlas-visa",
    "project:jojobot-server",
    "thing:jukebox",
    "thing:bike-chain",
    "thing:commit-omicron",
    "thing:floor-pump",
    "thing:folding-chairs",
    "thing:gravel-bike",
    "thing:leftorium-menu",
    "thing:phi",
    "thing:red-bike",
    "thing:red-bikee",
    "thing:road-bike",
    "thing:sigma",
    "thing:tau",
    "thing:that-search-summary",
    "thing:torque-wrench",
    "thing:trail-email",
    "thing:upsilon",
    "topic:widgets",
    "work:first-mix",
];

/// Every file in the workspace that can carry a handle.
///
/// **`.rs` is not the whole of it any more, and the day it stopped being was
/// the day a fixture was recorded from a live store into the repo.** Those
/// files are text, they carry handles by construction, and a gate that only
/// reads source would have watched the leak vector this project has been
/// burned by three times move into a file class it does not open. The
/// recorder points at a disposable collection and writes its own entities —
/// which is exactly the kind of reasoning that is true until somebody records
/// against something else.
fn scanned_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable source dir") {
        let path = entry.expect("readable dir entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            scanned_sources(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e == "rs" || e == "md" || e == "json")
        {
            out.push(path);
        }
    }
}

/// The workspace root the gate scans from.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// **Every file the gate reads, from one root** — so the two tests below
/// cannot disagree about what the corpus is. They ask opposite questions of
/// the same set: one that no handle in it is off the roster, one that no
/// roster entry is unused by it. Given two file lists those questions stop
/// being opposites, and a handle in a file only one of them reads is either
/// an unreviewed name or a roster entry reported as orphaned.
fn scanned_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scanned_sources(&root.join("crates"), &mut out);
    root_markdown(root, &mut out);
    out
}

/// **The markdown sitting at the root of the workspace**, which is where this
/// repository's prose about itself lives.
///
/// Prose about this software names handles — a document saying what a session
/// should write carries the handles it should write — so the root is the one
/// place the gate exists for and was not looking. Not recursive: everything
/// below the root is either `crates`, which [`scanned_sources`] already walks,
/// or build output.
fn root_markdown(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("readable workspace root") {
        let path = entry.expect("readable dir entry").path();
        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// Every off-roster handle in these files, as `handle in path`.
fn violations_in(files: &[PathBuf]) -> Vec<String> {
    let mut violations = Vec::new();
    for file in files {
        let text = fs::read_to_string(file).expect("readable source file");
        for handle in handles_in(&text) {
            if !ROSTER.contains(&handle.as_str()) {
                violations.push(format!("{} in {}", handle, file.display()));
            }
        }
    }
    violations.sort();
    violations.dedup();
    violations
}

/// Every `kind:slug` occurrence in the text, comments included.
fn handles_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for kind in kinds() {
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

/// A workspace of this test's own, removed when it is done — so a case about
/// what the gate scans can put a file where it wants one without writing into
/// the repository the real gate is reading in the same run.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("jojobot-roster-{}-{what}", std::process::id()));
        fs::create_dir_all(path.join("crates")).expect("a scratch workspace");
        Scratch(path)
    }

    /// Write one file under the scratch root and return nothing — the point is
    /// where it lands, and the test reads it back through the gate.
    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("a directory for the file");
        }
        fs::write(path, text).expect("a written file");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// **The gate reads markdown at the root of the workspace, not only under
/// `crates`.**
///
/// The root is where this repository's prose lives, and prose about this
/// software names handles: a document describing what a session should write
/// carries the handles it should write. Those files were outside the scan, so
/// an unlisted name in one was not a red run anywhere — the one file class the
/// gate exists for, in the one place it did not look.
///
/// Each half is paired, because a scan that reads nothing satisfies every
/// negative on its own: an off-roster handle at the root must be REPORTED, and
/// a roster handle in the same position must NOT be.
#[test]
fn the_gate_reads_markdown_at_the_workspace_root() {
    let unlisted = unlisted_handle("person");
    let scratch = Scratch::new("root-markdown");
    scratch.write("ROOT-DOC.md", &format!("the suite writes {unlisted} here"));
    scratch.write("crates/kept.rs", "// person:milhouse is listed");

    let violations = violations_in(&scanned_files(&scratch.0));

    assert!(
        violations.iter().any(|v| v.contains(&unlisted)),
        "an unlisted handle in a root markdown file has to be reported: {violations:?}"
    );
    assert!(
        !violations.iter().any(|v| v.contains("person:milhouse")),
        "…and a roster handle must still pass, or the report above is the scan \
         failing rather than the gate working: {violations:?}"
    );
}

/// A handle of that kind no roster entry matches, **assembled rather than
/// written**: the gate reads this file too, so spelling one out here would be
/// an unlisted handle in the workspace and the real scan would report it.
fn unlisted_handle(kind: &str) -> String {
    format!("{kind}:zzz-not-on-the-roster")
}

/// **The gate reads the newest kind the domain declares.**
///
/// The scan's kind list was hand-written, and it fell a kind behind the enum
/// the moment `pet` was added: no `pet:` handle was extracted, so none was ever
/// compared against the roster, and an off-roster name under that kind shipped
/// green. The list is derived now, and this is the case that fails if it ever
/// stops being.
///
/// Paired like the others, and for a sharper reason here: the half that says an
/// unlisted handle is reported is the only one a blind scan cannot satisfy.
#[test]
fn the_gate_reads_the_newest_kind_the_domain_declares() {
    let unlisted = unlisted_handle("pet");
    let scratch = Scratch::new("newest-kind");
    scratch.write("crates/kennel/src/lib.rs", &format!("// {unlisted}"));
    scratch.write("crates/kennel/fixture.md", "pet:snowball is listed");

    let violations = violations_in(&scanned_files(&scratch.0));

    assert!(
        violations.iter().any(|v| v.contains(&unlisted)),
        "an unlisted handle under the newest kind has to be reported: {violations:?}"
    );
    assert!(
        !violations.iter().any(|v| v.contains("pet:snowball")),
        "…and a roster handle of that kind must still pass: {violations:?}"
    );
}

/// **Widening the root did not stop the gate reading under `crates`.**
///
/// The behaviour that was already there is asserted rather than assumed: the
/// same call that now reaches the root still reports an unlisted handle in a
/// source file, which is the property every earlier version of this gate had.
#[test]
fn the_gate_still_reads_sources_under_crates() {
    let unlisted = unlisted_handle("person");
    let scratch = Scratch::new("under-crates");
    scratch.write(
        "crates/whatever/src/lib.rs",
        &format!("// {unlisted} in a comment"),
    );
    scratch.write("crates/whatever/fixture.md", "person:milhouse is listed");

    let violations = violations_in(&scanned_files(&scratch.0));

    assert!(
        violations.iter().any(|v| v.contains(&unlisted)),
        "an unlisted handle under crates has to be reported: {violations:?}"
    );
    assert!(
        !violations.iter().any(|v| v.contains("person:milhouse")),
        "…and a roster handle there must still pass: {violations:?}"
    );
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
    let files = scanned_files(&workspace_root());
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
    let files = scanned_files(&workspace_root());
    assert!(files.len() > 10, "the scan must actually see the workspace");

    let violations = violations_in(&files);
    assert!(
        violations.is_empty(),
        "handles outside the fictional roster — if the name is openly fictional, \
         add it to ROSTER in a conscious diff; if it names anything real from the \
         operator's life, it must not enter this repo at all:\n{}",
        violations.join("\n")
    );
}

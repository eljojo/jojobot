//! The fixture roster — the machine gate on life specifics.
//!
//! Every entity handle written anywhere in this workspace's sources must name
//! an openly fictional thing: a character from the Simpsons, South Park,
//! Family Guy or Bob's Burgers, a greek letter, or an obviously synthetic
//! token. The shared catalog in `jojobot-roster` is the complete allowlist.
//! Adding a name is a conscious, reviewed diff — and it is NEVER a person,
//! place, event, or organization from the operator's life, no matter which
//! branch or commit it would ride in on.
//!
//! No denylist exists anywhere: a list of forbidden names would re-embed the
//! very strings this gate exists to keep out.
//!
//! Scope: handle-shaped text (`kind:slug`) in every `.rs`, `.md`, `.json` and
//! `.jsonl` file under `crates/`, and in the markdown at the workspace root,
//! which is where this repository's prose about itself lives. A comment is read
//! on the same terms as code and on no wider ones — the handle form is what the
//! scan looks for, wherever it sits, and comments are where past leaks lived.
//!
//! **This file asks a second question of the same corpus, and it is the same
//! kind of question.** A recorded agent session lands here as `.jsonl`, and
//! what leaks in one is not a name but a FIELD: the CLI writes the machine it
//! ran on into the stream — connected servers, working directory, socket,
//! installed commands, billing — beside the session itself. So a capture is
//! measured against [`CAPTURE_KEYS`], an allowlist of the keys its readers
//! read, exactly as a handle is measured against [`ROSTER`]. **Two questions,
//! one mechanism, one corpus** — and neither is a list of forbidden values.
//!
//! **A slug handed to a handle constructor is in scope too**, and it is what
//! [`no_handle_in_the_workspace_is_built_from_a_bare_slug`] holds: a name given
//! to `EntityId::new` or `EntityId::person` in parts never appears as
//! `kind:slug`, so nothing here would compare it against the roster. That check
//! reads the constructor rather than the string — the signature declares the
//! argument is a slug — so it stays as crisp as this one. ⚠️ **It
//! demands a whole handle only where the call writes its KIND out.** A kind a
//! caller declares at runtime carries a token the domain's set does not hold, so
//! a handle under one is unreadable to this scan whichever way it is written. The
//! slug under such a kind is held to the two lists by
//! [`no_slug_under_a_declared_kind_is_off_the_lists`].
//!
//! **A life specific written in ordinary words passes this gate, because
//! nothing here is looking for it.** A comment, a doc string or a fixture that
//! names a real person, place or event in prose carries nothing handle-shaped,
//! so this suite has no opinion on it. A green run says two narrow things: no
//! unlisted handle, and no capture field outside [`CAPTURE_KEYS`]. It is not a
//! clearance for the text around either. What holds that line is somebody's
//! attention, and there is no third gate behind it.
//!
//! **A commit message in the unpushed range is text on its way in too**, and it
//! gets the same handle-shaped check every source file does — see
//! [`off_roster_handles_in`] — beside its own pronoun check. Ordinary prose in a
//! commit message is exactly as out of scope as ordinary prose anywhere else: no
//! machine reads it for a life specific, by the same standing rule.

use jojobot_domain::memory::EntityKind;
use jojobot_roster::{
    ROSTER, bare_slugs_in, handles_in, is_listed, listed, off_roster_handles_in, ordinary,
    two_part_handles_in, unlisted_slugs_under_a_declared_kind_in,
};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "fixture_roster/pronouns.rs"]
mod pronouns;
#[path = "fixture_roster/root_markdown.rs"]
mod root_markdown;
use pronouns::PRONOUNS;

/// The leaf kind list is guarded against the domain below.
fn kinds() -> Vec<&'static str> {
    jojobot_roster::KINDS.to_vec()
}

#[test]
fn the_shared_roster_kind_list_matches_the_domain() {
    let mut declared: Vec<_> = EntityKind::ALL.iter().map(|kind| kind.as_token()).collect();
    declared.sort();
    let mut scanned = kinds();
    scanned.sort();
    assert_eq!(scanned, declared);
}

/// **The kinds whose handle names somebody**: a person, a pet, a place or an
/// organisation. A handle of one of these kinds is a name, so its being fictional
/// rests on the show it comes from, and the cast list is where that is checked.
const SOMEBODY_KINDS: [&str; 4] = ["person", "pet", "place", "org"];

/// **The slug prefixes that say a handle names nobody.** A person, pet, place or
/// organisation on [`ORDINARY`] has to carry one. `contract-` is the label every
/// shared-contract fixture wears, and a reader seeing it knows the handle is a
/// label for a test and not a name.
const SYNTHETIC_PREFIXES: [&str; 1] = ["contract-"];

/// **Why a handle is allowed on ORDINARY without a synthetic prefix.** There are
/// three reasons, and each is a fact about the test or the fixture that cannot
/// be changed by renaming the handle.
const NEAR_MISS: &str = "a test measures how close this handle is to another, and a prefix would change \
                         the distance";
const ROOM_PLACEHOLDER: &str = "a room's furniture and its document name this handle, and a model \
                                creates it under that name";
const RECORDED: &str = "a recorded store holds this handle, and the store is frozen";

/// **The person, pet, place and organisation handles on ORDINARY that carry no
/// synthetic prefix, each with its reason.** An entry earns its place by being
/// a fact about a test and not a convenience; the list only shrinks. A handle
/// whose reason stops being true is renamed and leaves this list, and
/// [`every_exemption_is_on_ordinary_without_a_prefix_and_says_why`] holds the
/// list to what it claims.
const EXEMPT: &[(&str, &str)] = &[
    ("person:alpha-2", NEAR_MISS),
    ("person:alpha-a", NEAR_MISS),
    ("person:alpha-b", NEAR_MISS),
    ("person:alpha-one", NEAR_MISS),
    ("person:alpha-two", NEAR_MISS),
    ("person:alphaa", NEAR_MISS),
    ("person:bet", NEAR_MISS),
    ("person:gama", NEAR_MISS),
    ("person:zenit", NEAR_MISS),
    ("person:zenith", NEAR_MISS),
    ("person:milhouse-2", NEAR_MISS),
    ("person:nelson-2", NEAR_MISS),
    ("person:ned-flander", NEAR_MISS),
    ("person:flanders-original", NEAR_MISS),
    ("person:barn", NEAR_MISS),
    ("person:barney-gumbel", NEAR_MISS),
    ("place:capital-citty", NEAR_MISS),
    ("place:north-haverbook", NEAR_MISS),
    ("place:shelbyvile", NEAR_MISS),
    ("place:shelbyvlle", NEAR_MISS),
    ("place:springfeild", NEAR_MISS),
    ("place:far-country", NEAR_MISS),
    ("place:gorge-spot", NEAR_MISS),
    ("place:golden-daredevil-gorge", NEAR_MISS),
    ("place:daredevil-gorge-2", NEAR_MISS),
    ("person:plus-one", ROOM_PLACEHOLDER),
    ("place:bet", NEAR_MISS),
    ("person:upgrade-fixture-person", RECORDED),
    ("place:upgrade-fixture-place", RECORDED),
    ("person:pointer-migration-subject", RECORDED),
];

/// **The handles on `ordinary` that name somebody and say nothing that shows
/// they name nobody.** A handle of a somebody kind passes when its slug starts
/// with a synthetic prefix, when `roster` already lists it, or when `exempt`
/// lists it. Any other kind passes untouched: a thing, a work item or an event
/// is never a name.
///
/// All three lists are parameters so a case can hand it a made-up list and see
/// the check act, without the real lists having to be wrong first.
fn somebodies_without_a_synthetic_prefix<'a>(
    roster: &[&str],
    ordinary: &[&'a str],
    exempt: &[&str],
) -> Vec<&'a str> {
    ordinary
        .iter()
        .copied()
        .filter(|handle| {
            let Some((kind, slug)) = handle.split_once(':') else {
                return false;
            };
            SOMEBODY_KINDS.contains(&kind)
                && !SYNTHETIC_PREFIXES
                    .iter()
                    .any(|prefix| slug.starts_with(prefix))
                && !roster.contains(handle)
                && !exempt.contains(handle)
        })
        .collect()
}

/// Every file in the workspace that can carry a handle.
///
/// **`.rs` is not the whole of it: a fixture recorded from a live store lands
/// in the repository as data.** Those files are text and they carry handles by
/// construction, so a gate that reads source alone lets the leak vector this
/// project has been burned by three times move into a file class it never
/// opens. The recorder points at a disposable collection and writes its own
/// entities — which is exactly the kind of reasoning that is true until
/// somebody records against something else.
///
/// **`.jsonl` is here for the same reason and it is the newest of them**: a
/// recorded agent session is a stream of JSON events written straight to disk,
/// and the handles in it were said by a real model to a real surface.
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
            .is_some_and(|e| e == "rs" || e == "md" || e == "json" || e == "jsonl")
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
/// ordinary exception is unused by it. Given two file lists those questions stop
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

/// The same classifier the local static gate calls.
fn violations_in(files: &[PathBuf]) -> Vec<String> {
    jojobot_roster::violations_in(files).expect("readable source files")
}

/// **Every name that can stand behind a pronoun**, derived from the roster
/// rather than written out again.
///
/// Two spellings, because prose names a character both ways: the handle
/// (`pet:snowball`), and the name in words, which English capitalizes. **The
/// capital is the discriminator and it is doing real work**: several roster
/// slugs are ordinary words — a bot called nobody, a person called ghost — and
/// lower-case "nobody" in a sentence names no one at all. Parts shorter than
/// four characters are left out for the same reason; `person:alpha-one` would
/// otherwise make the word "One" a character, and "One reader" is not a
/// person.
fn character_names() -> Vec<String> {
    let mut names = Vec::new();
    for entry in listed() {
        let Some((kind, slug)) = entry.split_once(':') else {
            continue;
        };
        if !matches!(kind, "person" | "pet" | "bot") {
            continue;
        }
        for part in slug.split('-').filter(|p| p.len() >= 4) {
            let mut capitalized = part.to_string();
            capitalized[..1].make_ascii_uppercase();
            names.push(capitalized);
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Whether `text` uses `word` as a word rather than as a run of letters inside
/// a longer one.
///
/// **An apostrophe closes a word and does not open one.** Both halves were
/// wrong here once, and only one of them mattered: reading an apostrophe as
/// part of the word that precedes it let a contraction carry a pronoun past
/// this gate whole. It still does not OPEN one, so a quoted word — the way
/// this file has to write about the words it forbids — is not a use of it.
fn says_word(text: &str, word: &str, exact_case: bool) -> bool {
    let (haystack, needle) = if exact_case {
        (text.to_string(), word.to_string())
    } else {
        (text.to_lowercase(), word.to_lowercase())
    };
    haystack.match_indices(&needle).any(|(at, _)| {
        let opens = haystack[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '\'');
        // **An apostrophe CLOSES a word, and reading it as part of one is how a
        // contraction hid a pronoun from this gate.** A shortened form still
        // opens with the pronoun: the shortening changes what follows it and
        // not what it stands for.
        let closes = haystack[at + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        opens && closes
    })
}

/// **The prose of a file**: a comment in Rust, every line in markdown.
///
/// **Scoped to prose deliberately, and the reason is not tidiness.** Prose is
/// where this repository talks about a person; a string literal is data a test
/// compares against, and the gate below has to be able to write down the
/// sentence it forbids in order to test itself. That is the same problem
/// [`unlisted_handle`] solves by assembling a handle rather than spelling it,
/// answered here by saying which half of a file is prose.
fn prose_lines(path: &Path, text: &str) -> Vec<(usize, String)> {
    let markdown = path.extension().is_some_and(|e| e == "md");
    text.lines()
        .enumerate()
        .map(|(n, line)| {
            let trimmed = line.trim_start();
            let prose = markdown || trimmed.starts_with("//");
            (
                n + 1,
                if prose {
                    line.to_string()
                } else {
                    String::new()
                },
            )
        })
        .collect()
}

/// **Every pronoun in the prose with nobody to stand for**, as
/// `path:line: the line`.
///
/// **The rule this reads: a pronoun is licensed by a character somebody
/// named.** A comment about Milhouse may say his wrench, because Milhouse is
/// named right there; a comment about the operator may not say his anything,
/// because the operator is a ROLE and this repository never names the person
/// behind it. So the question the gate asks is not "is this pronoun about the
/// operator" — nothing can read that off a sentence — but "is there anybody
/// here for it to mean".
///
/// **The block is the unit, and it includes the code.** A story names its
/// characters in the calls it makes and talks about them in the comment above,
/// so a license that only read the comment would report correct English every
/// time the name sat one line below it.
///
/// ⚠️ **It reports less than its name says**: a block holding a roster name
/// that is also an ordinary word is licensed whole. [`names_somebody`] says
/// why and what that means for a green run.
fn pronouns_for_nobody(files: &[PathBuf]) -> Vec<String> {
    let names = character_names();
    let mut unattached = Vec::new();
    for file in files {
        let text = fs::read_to_string(file).expect("readable source file");
        let prose = prose_lines(file, &text);
        let lines: Vec<&str> = text.lines().collect();
        let mut at = 0;
        while at < lines.len() {
            if lines[at].trim().is_empty() {
                at += 1;
                continue;
            }
            let start = at;
            while at < lines.len() && !lines[at].trim().is_empty() {
                at += 1;
            }
            let block = lines[start..at].join(" ");
            if names_somebody(&block, &names) {
                continue;
            }
            for (line, text) in &prose[start..at] {
                if PRONOUNS.iter().any(|word| says_word(text, word, false)) {
                    unattached.push(format!("{}:{line}: {}", file.display(), text.trim()));
                }
            }
        }
    }
    unattached.sort();
    unattached
}

/// **Is there anybody in this block for a pronoun to mean?**
///
/// A handle, or a character's name in words. The two checks below ask the same
/// question of different text — a block of source, and a block of a commit
/// message — so they ask it through one function rather than two copies that
/// drift (rule 51).
///
/// ⚠️ **A KNOWN LIMIT: a block holding a roster name that is also an ordinary
/// word is licensed whole**, so a pronoun about nobody in that block passes.
/// Many roster names are words a sentence capitalizes — "Ghost" opening a
/// phrase, "Default", "Assistant", the Greek letters — and the capital cannot
/// tell the name from the word. A measurement over this workspace found 311
/// licensing names, 268 of them dictionary words. Both fixes tried failed: a
/// smaller block flagged 28 blameless lines at a window of one, and a
/// narrower name set needs a hand-picked exclusion list, which rule 106
/// forbids. **So this guard's green says "no pronoun stands alone in a block
/// with no name in it", and nothing more.** A better guard is new design.
/// [`a_block_naming_an_ordinary_word_roster_name_licenses_a_pronoun_about_nobody`]
/// pins the limit as known behaviour.
fn names_somebody(block: &str, names: &[String]) -> bool {
    block.contains("person:")
        || block.contains("pet:")
        || block.contains("bot:")
        || names.iter().any(|name| says_word(block, name, true))
}

/// Every commit in `range`, as its short name and its message.
///
/// **It reads the message and never the diff.** What a commit changed is not
/// this check's business; what it SAYS is.
///
/// ⚠️ **A range this cannot read is a failure rather than a pass.** No git, no
/// repository, no far end of the range to compare against — in each case the
/// check did not run, and reporting that as a clean range would be the exact
/// lie every other gate in this file is built to avoid.
fn commits_in_range(range: &str) -> Vec<(String, String)> {
    let out = std::process::Command::new("git")
        .args(["log", "--format=%h%x00%B%x01", range])
        .current_dir(workspace_root())
        .output()
        .expect("this check reads commit messages and needs git on the PATH. It did not run.");
    assert!(
        out.status.success(),
        "the range {range} could not be read, so this check did not run — one end of it is \
         missing here: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout)
        .split('\u{1}')
        .filter_map(|commit| commit.trim().split_once('\u{0}'))
        .map(|(name, message)| (name.to_string(), message.to_string()))
        .collect()
}

/// **Every commit that this checkout has not pushed**, as its short name and
/// its message. Feeds the pronoun check, which keeps this range.
///
/// The range is READ rather than configured: whatever is on `HEAD` and not on
/// `origin/main`. Nobody maintains a count, and a push shortens the range by
/// itself.
fn unpushed_commits() -> Vec<(String, String)> {
    commits_in_range("origin/main..HEAD")
}

/// **The commit that introduced the handle check on commit messages.**
/// [`commits_checked_for_handles`] reads from here, never from `origin/main`.
///
/// **A fixed ref, not the pushed boundary, and that is deliberate.** The
/// handle check is new; commits already in the unpushed range were written
/// before it existed and were swept by hand at this boundary instead — two
/// hits, a `thing` named `phone` and a `thing` named `already-here`, each
/// quoting a pre-rename fixture handle to explain the rename, read as
/// synthetic labels rather than life specifics. Judging them by a rule they
/// could not have known about would make the very first run of this check
/// red on `main`, which is the one outcome this whole file exists to
/// prevent. A message written AFTER this commit has no such excuse.
const COMMIT_HANDLE_CHECK_BOUNDARY: &str = "99681a26";

/// **Every commit this checkout has made since the handle check began**, as
/// its short name and its message. [`off_roster_handles_in`] is asked of
/// this range, never of [`unpushed_commits`]'s — see
/// [`COMMIT_HANDLE_CHECK_BOUNDARY`] for why the two checks read different
/// ranges.
fn commits_checked_for_handles() -> Vec<(String, String)> {
    commits_in_range(&format!("{COMMIT_HANDLE_CHECK_BOUNDARY}..HEAD"))
}

/// **Every pronoun in these messages with nobody to stand for**, as
/// `commit: the line`.
///
/// The same rule the source check uses, asked of a commit message: the block is
/// the unit, and a block that names nobody has nobody for a pronoun to mean.
fn operator_pronouns_in(commits: &[(String, String)]) -> Vec<String> {
    let names = character_names();
    let mut unattached = Vec::new();
    for (commit, message) in commits {
        for block in message.split("\n\n") {
            let joined = block.replace('\n', " ");
            if names_somebody(&joined, &names) {
                continue;
            }
            for line in block.lines().filter(|l| !l.trim().is_empty()) {
                if PRONOUNS.iter().any(|word| says_word(line, word, false)) {
                    unattached.push(format!("{commit}: {}", line.trim()));
                }
            }
        }
    }
    unattached.sort();
    unattached.dedup();
    unattached
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
/// green. The shared kind list is checked against the domain enum, and this
/// case also fails when a known kind stops being scanned.
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

/// **ORDINARY names nobody who could be somebody.** A handle of kind person, pet,
/// place or org on it has to carry a synthetic prefix, or be on the cast list.
/// Without the check, a real person's name could be filed on ORDINARY as "a
/// handle that names nobody", which is the one list a reviewer does not read.
#[test]
fn ordinary_holds_no_person_pet_place_or_org_without_a_synthetic_prefix() {
    let exempt: Vec<&str> = EXEMPT.iter().map(|(handle, _)| *handle).collect();
    let found = somebodies_without_a_synthetic_prefix(ROSTER, &ordinary(), &exempt);
    assert!(
        found.is_empty(),
        "these handles name a person, pet, place or organisation and are on ORDINARY without a \
         synthetic slug prefix ({SYNTHETIC_PREFIXES:?}). A character from the shows is a roster \
         name; a label for a test wears a prefix: {found:?}"
    );
}

/// **The check acts on all three shapes it has to tell apart.** A made-up person
/// handle on ORDINARY is refused; the same slug behind the synthetic prefix
/// passes; a roster person passes; and a made-up handle of a kind that names
/// nobody passes too, so the check is a check on the four kinds and not on every
/// handle.
#[test]
fn the_check_on_ordinary_refuses_a_made_up_person_and_passes_the_rest() {
    let roster_person = ROSTER
        .iter()
        .copied()
        .find(|handle| handle.starts_with("person:"))
        .expect("the cast has a person");
    // **Assembled rather than written**, because the gate reads this file too: a
    // literal made-up handle here would be a violation of the rule the case is
    // about, caught by the very scan it is testing.
    let made_up = format!("person:{}", "made-up-for-this-case");
    let labelled = format!(
        "person:{}{}",
        SYNTHETIC_PREFIXES[0], "made-up-for-this-case"
    );
    assert_eq!(
        somebodies_without_a_synthetic_prefix(&[], &[made_up.as_str()], &[]),
        [made_up.as_str()],
        "a made-up person handle on ORDINARY is refused"
    );
    assert!(
        somebodies_without_a_synthetic_prefix(&[], &[labelled.as_str()], &[]).is_empty(),
        "a person behind the synthetic prefix passes"
    );
    assert!(
        somebodies_without_a_synthetic_prefix(&[roster_person], &[roster_person], &[]).is_empty(),
        "a roster person passes"
    );
    let thing = format!("thing:{}", "made-up-for-this-case");
    assert!(
        somebodies_without_a_synthetic_prefix(&[], &[thing.as_str()], &[]).is_empty(),
        "a handle of a kind that names nobody passes without a prefix"
    );
    for kind in SOMEBODY_KINDS {
        assert_eq!(
            somebodies_without_a_synthetic_prefix(&[], &[&format!("{kind}:made-up")], &[]).len(),
            1,
            "{kind} is one of the kinds the check acts on"
        );
    }
}

/// **Every exemption is on ORDINARY, names a somebody, wears no prefix and says
/// why.** An entry that is not on ORDINARY exempts nothing; one that carries the
/// prefix needs no exemption; one that is not a somebody kind was never subject
/// to the check. Each of the three is a stale entry, and the list only shrinks.
#[test]
fn every_exemption_is_on_ordinary_without_a_prefix_and_says_why() {
    assert!(
        !EXEMPT.is_empty(),
        "an empty list would pass this for the wrong reason"
    );
    let mut stale = Vec::new();
    for (handle, reason) in EXEMPT {
        let (kind, slug) = handle.split_once(':').expect("a handle is kind:slug");
        let ok = ordinary().contains(handle)
            && SOMEBODY_KINDS.contains(&kind)
            && !SYNTHETIC_PREFIXES
                .iter()
                .any(|prefix| slug.starts_with(prefix))
            && !ROSTER.contains(handle)
            && !reason.trim().is_empty();
        if !ok {
            stale.push(*handle);
        }
    }
    assert!(
        stale.is_empty(),
        "these exemptions are not needed or not allowed any more; rename the handle or take \
         it off the list: {stale:?}"
    );
}

/// **No handle is on both lists.** The bar prints what arrives on [`ROSTER`], and
/// a handle listed on both would be read as ordinary by whoever looked at the
/// second list. A repeat inside one list is older than the split and is left
/// alone.
#[test]
fn no_handle_is_on_both_the_cast_and_the_ordinary_list() {
    let both: Vec<&str> = ROSTER
        .iter()
        .copied()
        .filter(|handle| ordinary().contains(handle))
        .collect();
    assert!(
        both.is_empty(),
        "these handles are on both lists; a name from the shows belongs on ROSTER only, \
         and a handle that names nobody on ORDINARY only: {both:?}"
    );
    assert!(
        !ROSTER.is_empty() && !ordinary().is_empty(),
        "an empty list would pass this for the wrong reason"
    );
}

/// Reviewed cast names may be unused; ordinary exceptions still describe actual use.
#[test]
fn ordinary_carries_no_name_the_workspace_has_stopped_using() {
    let files = scanned_files(&workspace_root());
    let corpus: String = files
        .iter()
        .filter(|f| {
            f.file_name().is_some_and(|n| n != "fixture_roster.rs")
                && !f.ends_with(jojobot_roster::ROSTER_FILE)
                && !f.ends_with(jojobot_roster::ORDINARY_FILE)
        })
        .map(|f| fs::read_to_string(f).expect("readable source file"))
        .collect();

    // **The same two ways of naming a handle the violation scan reads**, or the
    // two questions stop being opposites: a name built out of two literals
    // would be caught as off-roster by one test and reported as an unused
    // entry by the other, and adding it to the roster could not satisfy both.
    //
    // **`handles_in`, never a raw substring check.** A roster entry that is
    // the PREFIX of some other, longer handle (a `person` named "milhouse"
    // sitting inside one named "milhouse-extra") is a real substring of the
    // corpus even when nothing ever wrote the entry itself — a `contains`
    // check reads that as "used" forever, exactly the false negative
    // `violations_in`'s own near-miss handling exists to avoid on the other
    // side of this gate.
    let found = handles_in(&corpus);
    let built = two_part_handles_in(&corpus);
    let orphaned: Vec<&str> = ordinary()
        .into_iter()
        .filter(|handle| !found.iter().any(|f| f == handle) && !built.iter().any(|b| b == handle))
        .collect();
    assert!(
        orphaned.is_empty(),
        "ordinary exceptions nothing in the workspace uses — delete them; only reviewed \
         cast names may be kept ready for future fixtures:\n{}",
        orphaned.join("\n")
    );
}

/// 🚨 **A name built out of two literals is compared against the roster, and
/// one already on it is not.**
///
/// **Both halves in one case.** The refusal is the capability; the acceptance
/// is what says the gate discriminates rather than refusing everything it can
/// read. A gate that refused every two-part name would satisfy the first alone
/// and would be uninstallable.
///
/// ⚠️ **It states what it does NOT reach.** A kind a caller declares at
/// runtime is not in [`kinds`], so a handle under one is unreadable however it
/// is written — whole or in halves. **That gap is older than this gate and this
/// gate does not close it**, and a reader who took the roster for complete
/// coverage would be wrong in a way nothing on screen corrects.
#[test]
fn a_two_part_name_is_read_and_measured_against_the_roster() {
    let on_it = listed()
        .find(|h| h.starts_with("thing:"))
        .expect("the roster names a thing");
    let (_, slug) = on_it.split_once(':').expect("a handle is kind:slug");

    // **Assembled rather than written**, because the gate reads this file too:
    // a literal off-roster pair here would be a violation of the rule the case
    // is about, caught by the very scan it is testing.
    let nobody = "no-such-".to_string() + "fixture";
    let whole = format!("thing:{nobody}");
    let refused = format!("add_args(\"thing\", \"{nobody}\", \"Nope\")");
    assert_eq!(
        two_part_handles_in(&refused),
        vec![whole.clone()],
        "a name whose halves never meet as text was not read, so no allowlist sees it",
    );
    assert!(
        !is_listed(&whole),
        "the case rests on this name being off the roster",
    );

    let accepted = format!(r#"add_args("thing", "{slug}", "Fine")"#);
    assert_eq!(
        two_part_handles_in(&accepted),
        vec![(*on_it).to_string()],
        "the gate read a name already on the roster as something else",
    );

    // **The payload form, which is the other way a fixture names one.**
    let payload = format!("{{\"kind\": \"thing\", \"handle\": \"{nobody}\"}}");
    assert_eq!(two_part_handles_in(&payload), vec![whole]);

    // ⛔️ **And what it must NOT read, asserted at each of the two places that
    // refuse it**, because one string can pass for the wrong reason.
    //
    // No anchor: a migration's table and column sit beside each other as two
    // literals and are not a handle. **Nothing about the kind list is being
    // exercised here** — the constructor name is what excludes it.
    assert!(
        two_part_handles_in(r#"Leaves::NoRows("session", "timezone IS NULL")"#).is_empty(),
        "two literals beside each other are not a handle",
    );
    // An anchor, and a first literal that is no kind. **This is the half the
    // kind list decides**, and without it a sabotage of that list passes.
    assert!(
        two_part_handles_in(r#"add_args("notakind", "whatever", "X")"#).is_empty(),
        "a call whose first argument is no kind does not name an entity",
    );
}

#[test]
fn every_handle_in_the_workspace_is_on_the_fictional_roster() {
    let files = scanned_files(&workspace_root());
    assert!(files.len() > 10, "the scan must actually see the workspace");

    let violations = violations_in(&files);
    assert!(
        violations.is_empty(),
        "{}",
        jojobot_roster::refusal(&violations)
    );
}

/// **A pronoun in this repository stands for a character somebody named, or it
/// stands for the operator.**
///
/// The bright line says this repository names ROLES — the operator, a caller,
/// a reader — and never the person behind one. A pronoun breaks it quietly:
/// nothing is misspelled, no handle is off the roster, and the sentence reads
/// perfectly, which is why several review rounds passed over thirteen of them.
///
/// **What makes it checkable is that the operator is never named.** A
/// character has a name in the block — `pet:snowball`, or Milhouse in words —
/// and the operator has none, by rule. So a pronoun with nobody in its block
/// is a pronoun for the operator, and the gate needs no opinion about the
/// sentence.
///
/// It is not a denylist (rule 106): the pronouns are English's closed set and
/// the names come off the roster the gate already keeps.
#[test]
fn no_pronoun_in_the_workspace_stands_for_the_operator() {
    let files = scanned_files(&workspace_root());
    assert!(files.len() > 10, "the scan must actually see the workspace");

    let unattached = pronouns_for_nobody(&files);
    assert!(
        unattached.is_empty(),
        "these read as the operator's pronouns — this repository names the role rather than \
         the person, so write the operator, a reader or a caller. If the line really is about \
         a fictional character, name them in it:\n{}",
        unattached.join("\n")
    );
}

/// **Both halves, or the gate above proves nothing.**
///
/// A scan that reads nothing satisfies every negative on its own, and a gate
/// that flagged all prose would be deleted by the first author who hit it. So
/// one file says a character's name and keeps its pronoun, and one says only
/// a role and loses it.
#[test]
fn the_gate_reads_a_named_character_and_an_unnamed_role_apart() {
    let scratch = Scratch::new("pronouns");
    scratch.write(
        "crates/named.rs",
        "// Milhouse lent the wrench and never got it back.\n\
         // The walk finds his tools.\nfn a() {}\n",
    );
    scratch.write(
        "crates/role.rs",
        "// The page the operator opens is the one this test reads.\n\
         // It is the surface he reads himself.\nfn b() {}\n",
    );
    // **A contraction hides the pronoun from a boundary rule that lets an
    // apostrophe close a word.** It is the same breach with two characters
    // after it.
    scratch.write(
        "crates/shortened.rs",
        "// The listing is the one page nobody else opens.\n\
         // He's the one who reads it.\nfn c() {}\n",
    );

    let unattached = pronouns_for_nobody(&scanned_files(&scratch.0));

    assert!(
        unattached.iter().any(|line| line.contains("role.rs")),
        "a pronoun with only a role to stand for has to be reported: {unattached:?}"
    );
    assert!(
        unattached.iter().any(|line| line.contains("shortened.rs")),
        "…and shortening it to a contraction does not hide it: {unattached:?}"
    );
    assert!(
        !unattached.iter().any(|line| line.contains("named.rs")),
        "…and a pronoun for a character the line names must pass, or the report above is the \
         gate flagging everything rather than working: {unattached:?}"
    );
}

/// **A KNOWN LIMIT, pinned as behaviour and not as a pass of the rule.** A block
/// that holds a roster name which is also an ordinary word is licensed whole,
/// so a pronoun about nobody in that block is not reported.
///
/// The sentence below capitalizes "Ghost" as the start of a phrase, and the
/// roster holds a character of that name, so the block counts as naming
/// somebody. The control file has the same pronoun without that word and IS
/// reported, which shows the silence above comes from the licensing and not
/// from a scan that read nothing.
#[test]
fn a_block_naming_an_ordinary_word_roster_name_licenses_a_pronoun_about_nobody() {
    let scratch = Scratch::new("ordinary-word-license");
    scratch.write(
        "crates/licensed.rs",
        "// Ghost writing is not what this file does.\n\
         // It is the surface he reads himself.\nfn a() {}\n",
    );
    scratch.write(
        "crates/control.rs",
        "// Typing is what this file does.\n\
         // It is the surface he reads himself.\nfn b() {}\n",
    );

    let unattached = pronouns_for_nobody(&scanned_files(&scratch.0));

    assert!(
        unattached.iter().any(|line| line.contains("control.rs")),
        "the same pronoun without the ordinary word has to be reported, or the silence \
         below proves nothing: {unattached:?}"
    );
    assert!(
        !unattached.iter().any(|line| line.contains("licensed.rs")),
        "a block holding an ordinary-word roster name is licensed whole: {unattached:?}"
    );
}

/// **A handle built in this workspace names its kind where the scan can read
/// it.**
///
/// The gate above reads handle-shaped text, so a name that never appears as
/// `kind:slug` is a name it has never compared against the roster. Two
/// constructors take a handle apart: `EntityId::new`, which takes a kind and a
/// slug, and `EntityId::person`, which takes a slug alone. **A slug handed to
/// either of them is a name in this repository that no allowlist has seen.**
///
/// **This reads a DECLARED fact, not a guess.** The scan does not ask which
/// bare strings in the workspace look like slugs — that question has no crisp
/// answer and asking it would make the check above less trustworthy. It asks
/// what those two signatures already say: the argument in that position IS a
/// slug, because the constructor's own type says so. **A check keyed on the
/// constructor is exactly as crisp as one keyed on a `kind:` prefix.**
///
/// **It is keyed on the constructor rather than on the kind**, and one call
/// shows why: a kind can be a variable, so a scan reading the kind token would
/// not see that call at all. The second argument is what this reads, whatever
/// the first one is.
///
/// It holds no list of names and has no opinion about any string's content
/// (rules 45, 106). The way to satisfy it is to write the handle whole, which
/// is what puts the name in front of the roster.
#[test]
fn no_handle_in_the_workspace_is_built_from_a_bare_slug() {
    let bare = bare_slugs_in(&scanned_files(&workspace_root()));
    assert!(
        bare.is_empty(),
        "these hand a bare slug to a handle constructor, so the name never appears as \
         `kind:slug` and the roster gate above has never seen it — write the handle \
         whole:\n{}",
        bare.join("\n")
    );
}

/// **Both halves, or the case above proves nothing.**
///
/// A scan that read nothing would satisfy the assertion on its own, and a scan
/// that flagged every call would be deleted by the first author who met it. So
/// one file hands a bare slug to each constructor and one writes the handle
/// whole through the same calls.
#[test]
fn the_gate_reads_a_bare_slug_and_a_whole_handle_apart() {
    let scratch = Scratch::new("bare-slugs");
    scratch.write(
        "crates/bare.rs",
        concat!(
            "fn a() {\n",
            "    let x = EntityId::new(EntityKind::PERSON, \"someone\");\n",
            "    let y = EntityId::person(\"another\");\n",
            "}\n",
        ),
    );
    scratch.write(
        "crates/whole.rs",
        concat!(
            "fn b() {\n",
            "    let x = EntityId::new(EntityKind::PERSON, \"person:milhouse\");\n",
            "    let y = EntityId::person(\"person:ralph\");\n",
            "}\n",
        ),
    );

    let bare = bare_slugs_in(&scanned_files(&scratch.0));

    assert!(
        bare.iter().any(|line| line.contains("someone")),
        "a bare slug handed to EntityId::new has to be reported: {bare:?}"
    );
    assert!(
        bare.iter().any(|line| line.contains("another")),
        "…and one handed to EntityId::person: {bare:?}"
    );
    assert!(
        !bare.iter().any(|line| line.contains("whole.rs")),
        "…and a whole handle through the same calls must pass, or the report above is the \
         scan flagging everything rather than working: {bare:?}"
    );
}

/// **A slug handed to a constructor under a kind a caller declared is held to the
/// two lists, whatever the kind is.** One file builds a handle under a declared
/// kind with a slug on [`ROSTER`], one with a slug on [`ORDINARY`], and one with a
/// slug on neither. **The last is refused naming its file, its line and its slug**,
/// and the first two pass, so the scan is reading the slug rather than refusing
/// every call it can see.
///
/// Two things it must not read, each asserted: a call whose kind is written out
/// (the guard above holds those, and one slug is reported once), and a slug that
/// is not a literal.
#[test]
fn the_gate_reads_the_slug_under_a_declared_kind_against_the_lists() {
    let scratch = Scratch::new("declared-kind-slugs");
    scratch.write(
        "crates/declared.rs",
        concat!(
            "fn a(stall: EntityKind) {\n",
            "    let on_the_roster = EntityId::new(stall, \"marge\");\n",
            "    let on_ordinary = EntityId::new(stall, \"gravel-bike\");\n",
            "    let on_neither = EntityId::new(stall, \"zzz-unknown\");\n",
            "}\n",
        ),
    );
    scratch.write(
        "crates/not_this.rs",
        concat!(
            "fn b(stall: EntityKind, slug: &str) {\n",
            "    let written_out = EntityId::new(EntityKind::PERSON, \"zzz-written-out\");\n",
            "    let a_variable = EntityId::new(stall, slug);\n",
            "    let a_format = EntityId::new(stall, format!(\"zzz-{slug}\"));\n",
            "}\n",
        ),
    );

    let refused = unlisted_slugs_under_a_declared_kind_in(&scanned_files(&scratch.0));

    assert_eq!(
        refused.len(),
        1,
        "only the slug on neither list is refused: {refused:?}"
    );
    let said = &refused[0];
    for named in ["declared.rs", ":4", "zzz-unknown"] {
        assert!(said.contains(named), "{named} is missing from {said}");
    }
    assert!(
        !said.contains("marge") && !said.contains("gravel-bike"),
        "{said}"
    );
}

/// **A slug handed to a constructor under a kind the call does not write out is on
/// one of the two lists.** The scan above reads a call whose kind is written out,
/// and a kind a caller declared at runtime is invisible to it however a handle
/// under it is written. This reads the slug in the position the constructor's
/// signature gives it, as that scan does, and holds no list of names of its own
/// (rules 45, 106).
///
/// ⚠️ **A slug that is not a literal is out of reach of any text scan**, a variable
/// or a `format!` among them.
#[test]
fn no_slug_under_a_declared_kind_is_off_the_lists() {
    let refused = unlisted_slugs_under_a_declared_kind_in(&scanned_files(&workspace_root()));
    assert!(
        refused.is_empty(),
        "these hand a slug that is on neither ROSTER nor ORDINARY to a handle constructor \
         under a kind the call does not write out. If the name is openly fictional, add a \
         handle with it to ROSTER (a name from the shows) or ORDINARY (a handle that names \
         nobody) in a conscious diff; otherwise use a slug that is already there:\n{}",
        refused.join("\n")
    );
}

/// **A commit message names the role, never the person behind it.**
///
/// The bright line says this repository names ROLES — the operator, a caller, a
/// reader — and a commit message is text on its way in exactly as a comment is.
/// The message outlives the branch, and nothing rewrites it afterwards.
///
/// 🚨 **The convention is what mints this defect, which is why one careful pass
/// never catches it.** A commit body cites the authority behind a change, and
/// the shortest way to write that authority is a pronoun. So the rule that
/// makes this history auditable is the same rule that produces the breach, and
/// it produces it in almost every message that carries one.
///
/// **Forward-only, and that is the operator's ruling.** What is pushed stays:
/// rewriting published history costs more than the breach. This reads the range
/// that is still local, where a message can still be amended cheaply.
///
/// ⚠️ **An empty range passes, and that is not this check going blind.** A
/// range empties every time somebody pushes, so failing on it would fail the
/// normal state. What proves the reader is alive is
/// [`the_check_reads_a_commit_for_the_operator_and_one_for_a_character_apart`],
/// which runs in the same suite and does not depend on what the range holds.
#[test]
fn no_unpushed_commit_message_writes_a_pronoun_for_the_operator() {
    let unattached = operator_pronouns_in(&unpushed_commits());
    assert!(
        unattached.is_empty(),
        "these commit messages read as the operator's pronouns. This repository names the \
         ROLE rather than the person, so write the operator, a reader or a caller. These \
         commits are not pushed yet, so amending the message is still cheap. If a line \
         really is about a fictional character, name them in it:\n{}",
        unattached.join("\n")
    );
}

/// **Both halves, or the case above proves nothing.**
///
/// A reader that returned nothing would satisfy that assertion on its own, and
/// one that flagged every pronoun would be switched off by the first author who
/// met it. So one message cites an authority with nobody in the block, and one
/// talks about a character it names.
#[test]
fn the_check_reads_a_commit_for_the_operator_and_one_for_a_character_apart() {
    let commits = vec![
        (
            "aaaaaaa".to_string(),
            "boundary: a stranded write stops being told to retry\n\nThe change lands \
             under his ruling."
                .to_string(),
        ),
        (
            "bbbbbbb".to_string(),
            "tests: the walk finds the tools Milhouse lent\n\nMilhouse lent the wrench \
             and never got it back, so his tools are what the walk finds."
                .to_string(),
        ),
    ];

    let unattached = operator_pronouns_in(&commits);

    assert!(
        unattached.iter().any(|line| line.starts_with("aaaaaaa")),
        "an authority line with nobody in its block has to be reported: {unattached:?}"
    );
    assert!(
        !unattached.iter().any(|line| line.starts_with("bbbbbbb")),
        "…and a pronoun for a character the block names must pass, or the report above is \
         the check flagging everything rather than working: {unattached:?}"
    );
}

/// **A commit message is held to the roster too, from
/// [`COMMIT_HANDLE_CHECK_BOUNDARY`] forward.** Handle-shaped text is
/// handle-shaped text wherever it sits, and this repository is public the
/// moment a push makes it so — the same reasoning
/// [`no_unpushed_commit_message_writes_a_pronoun_for_the_operator`] states for
/// pronouns applies here, on the OTHER thing a message can leak: a real handle
/// quoted while describing a bug.
///
/// **Ordinary prose is out of scope, by a standing rule.** This checks
/// handle-shaped text only — `kind:slug`, or the two-literal constructor
/// shape — never a name written in words. A commit message otherwise stays
/// enforced by attention, exactly as every other comment and doc string in
/// this repository is; no machine reads prose for a life specific.
#[test]
fn no_commit_message_since_the_boundary_quotes_an_off_roster_handle() {
    let violations = off_roster_handles_in(&commits_checked_for_handles());
    assert!(
        violations.is_empty(),
        "these commit messages quote a handle this workspace's roster does not list. These \
         commits are not pushed yet, so fixing the message is still cheap — swap the real \
         handle for a fixture name, or add the fixture to the roster if it genuinely is one:\n{}",
        violations.join("\n")
    );
}

/// **Both halves, or the case above proves nothing.** One message quotes a
/// handle no roster entry matches; one quotes only a listed handle and
/// ordinary prose. A reader that flagged nothing would satisfy the first
/// case's assertion by being blind, and one that flagged everything would be
/// switched off by the first author it caught.
#[test]
fn the_check_reads_one_off_roster_handle_and_one_roster_handle_apart() {
    let unlisted = unlisted_handle("person");
    let commits = vec![
        (
            "aaaaaaa".to_string(),
            format!("mailbox: the read handed {unlisted} the wrong body"),
        ),
        (
            "bbbbbbb".to_string(),
            "mailbox: the read handed person:milhouse the wrong body, and ordinary prose \
             about the fix besides"
                .to_string(),
        ),
    ];

    let violations = off_roster_handles_in(&commits);

    assert!(
        violations
            .iter()
            .any(|line| line.starts_with("aaaaaaa") && line.contains(&unlisted)),
        "a commit quoting an off-roster handle has to be reported: {violations:?}"
    );
    assert!(
        !violations.iter().any(|line| line.starts_with("bbbbbbb")),
        "…and a commit quoting only a roster handle must pass, or the report above is the \
         check flagging everything rather than working: {violations:?}"
    );
}

/// This checkout's own current commit, short form — so a case can prove the
/// checked range reaches the present rather than something stale.
fn current_head() -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace_root())
        .output()
        .expect("this check reads HEAD and needs git on the PATH. It did not run.");
    assert!(
        out.status.success(),
        "HEAD could not be read: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// **The checked range reaches HEAD, not a boundary frozen in the past.**
///
/// [`COMMIT_HANDLE_CHECK_BOUNDARY`] is a fixed commit, unlike
/// [`unpushed_commits`]'s `origin/main`, which moves on its own with every
/// push. A fixed lower bound proves nothing about the upper one — this is
/// the case that does: a commit written today, quoting an off-roster handle,
/// would only be caught if `COMMIT_HANDLE_CHECK_BOUNDARY..HEAD` still ends at
/// HEAD. It is asserted directly, over the real range, because a boundary
/// that quietly stopped tracking HEAD would leave every later commit
/// unchecked with nothing here to notice.
#[test]
fn the_handle_check_range_reaches_head() {
    let commits = commits_checked_for_handles();
    let head = current_head();
    assert!(
        commits.iter().any(|(hash, _)| hash == &head),
        "the range COMMIT_HANDLE_CHECK_BOUNDARY..HEAD must include HEAD ({head}), or a commit \
         quoting an off-roster handle written today would not be caught: {commits:?}"
    );
}

/// **The keys a recorded agent session may carry into this repository.**
///
/// A capture is written by an agent CLI running on somebody's machine, and the
/// stream it writes describes that machine as much as the session: which
/// servers were connected, which directory the run sat in, which socket it
/// talked over, what the account was billed. **None of that is material — it is
/// the environment, riding along.** Two captures reached `main` carrying a list
/// of the operator's own connected products, and nothing here was looking.
///
/// So a committed capture is cut down to what the readers under test actually
/// read, and this is that list. It is an allowlist for the same reason
/// [`ROSTER`] is one: **a denylist of environment fields re-embeds the very
/// values it filters and knows nothing about the next field the CLI adds.**
/// Adding a key is a conscious, reviewed diff, and the question the review asks
/// is whether a reader reads it.
///
/// `model` is the one entry no reader reads. It stays because a recording that
/// does not say what produced it has lost its provenance, and the model driving
/// the exercise is this project's own dependency rather than anything about the
/// machine it ran on.
const CAPTURE_KEYS: &[&str] = &[
    "answers_type",
    "bot",
    "clear_fields",
    "content",
    "decide_by",
    "fields",
    "file_path",
    "guarantee_expires",
    "id",
    "input",
    "is_error",
    "message",
    "model",
    "name",
    "result",
    "role",
    "subtype",
    "text",
    "thinking",
    "tickets_needed_by",
    "today",
    "tool_use_id",
    "type",
    "uuid",
];

/// Every recorded stream committed under `crates/`.
fn capture_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scanned_sources(&root.join("crates"), &mut out);
    out.retain(|p| p.extension().is_some_and(|e| e == "jsonl"));
    out
}

/// Every key at every depth of one JSON value.
fn keys_in(value: &serde_json::Value, out: &mut std::collections::BTreeSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, held) in fields {
                out.insert(key.clone());
                keys_in(held, out);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|item| keys_in(item, out)),
        _ => {}
    }
}

/// 🚨 **A recorded session in this repository carries only the keys a reader
/// reads, and every key on that list is one some capture carries.**
///
/// **Both halves in one read, and the negative alone is worth nothing here.**
/// "No key outside the list" passes on an empty directory, on a deleted
/// fixture, and on a file scrubbed down to nothing — every one of which is a
/// gate that has stopped gating. The positive is what says the scan reached
/// real recordings and that the list describes them.
///
/// **It asks about FIELDS, never values.** A check that hunted for the product
/// names found in the two captures would be a denylist (rule 106), would
/// re-embed them, and would say nothing about the next connector the CLI
/// reports. A key the readers do not read is the environment whatever it holds.
#[test]
fn a_recorded_session_carries_only_the_keys_its_readers_read() {
    let captures = capture_fixtures(&workspace_root());
    assert!(
        !captures.is_empty(),
        "the scan found no recorded streams, so it is measuring nothing",
    );

    let mut found = std::collections::BTreeSet::new();
    let mut loose = Vec::new();
    for capture in &captures {
        let text = fs::read_to_string(capture).expect("readable capture");
        for (at, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let event: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!(
                    "a capture line nothing can parse: {}:{}: {e}",
                    capture.display(),
                    at + 1,
                )
            });
            let mut here = std::collections::BTreeSet::new();
            keys_in(&event, &mut here);
            for key in here.iter().filter(|k| !CAPTURE_KEYS.contains(&k.as_str())) {
                loose.push(format!("{key} in {}:{}", capture.display(), at + 1));
            }
            found.extend(here);
        }
    }
    assert!(
        loose.is_empty(),
        "keys no reader reads — a capture describes the machine it ran on, so a field \
         nothing under test consumes is that machine riding into the repo. Cut it, or add \
         it to CAPTURE_KEYS in a reviewed diff that says which reader wants it:\n{}",
        loose.join("\n"),
    );

    let orphaned: Vec<&str> = CAPTURE_KEYS
        .iter()
        .copied()
        .filter(|key| !found.contains(*key))
        .collect();
    assert!(
        orphaned.is_empty(),
        "keys permitted that no capture carries — the list stops being a description of what \
         is here and becomes permission granted to nothing:\n{orphaned:?}",
    );
}

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
//! argument is a slug — so it stays as crisp as this one. ⚠️ **It covers a call
//! whose KIND is written out.** A kind a caller declares at runtime carries a
//! token the domain's set does not hold, so a handle under one is unreadable to
//! this scan whichever way it is written.
//!
//! **A life specific written in ordinary words passes this gate, because
//! nothing here is looking for it.** A comment, a doc string or a fixture that
//! names a real person, place or event in prose carries nothing handle-shaped,
//! so this suite has no opinion on it. A green run says two narrow things: no
//! unlisted handle, and no capture field outside [`CAPTURE_KEYS`]. It is not a
//! clearance for the text around either. What holds that line is somebody's
//! attention, and there is no third gate behind it.

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
/// **The descriptive case labels are the synthetic branch of that rule**, not
/// an exception to it: `thing:contract-clear-off-address` says what a case is
/// about and names nobody. They are here because the fixture carrying one has
/// to be visible to the scan, never because the list is a pool of approved
/// words.
///
/// **Some entries are engine material rather than fixtures**, and they are
/// here deliberately rather than by drift: `bot:assistant`, the default
/// identity jojobot ships with, and the views the build supplies. Each names a
/// ROLE or a question the way the orientation essay names "the operator", and
/// each identifies nobody's instance and nobody's life, which is the line this
/// file exists to hold. **That is what a shipped name has to earn to sit
/// here**, and the review that adds one asks it. Every other name here is
/// fictional and must stay that way.
const ROSTER: &[&str] = &[
    "bot:assistant",
    // The views: the ones the software ships, and the ones a story or a page
    // case declares to stand beside them. **A shipped record names itself by
    // its handle**, so this scan reads it on the same terms as every other
    // name — a slug handed to a constructor on its own would carry no handle
    // for the scan to find.
    "view:colleagues",
    "view:contract-no-such-view",
    "view:contract-supplied-rename-target",
    "view:loop",
    "view:loops",
    "view:matches-nothing",
    "view:my-loops",
    "view:my-people",
    "view:names-no-kind",
    "view:nobody-has-this",
    "bot:contract-no-box-owner",
    "bot:contract-repointed",
    "bot:contract-still-no-box",
    "bot:delta",
    "bot:epsilo",
    "bot:epsilon",
    "bot:gamm",
    "bot:gamma",
    // The software's own name, and the one name this repository owns. A
    // recorded session asked the room to boot as it and the room refused —
    // which is the whole point of that capture. It names no instance and
    // nobody's life, so it sits here on the same terms as `bot:assistant`.
    "bot:jojobot",
    "bot:milhouse",
    "bot:nobody",
    "bot:otto",
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
    "event:trail-survey-2026",
    "event:winter-fest",
    "machine:phi",
    "machine:sigma",
    "machine:tau",
    "machine:upsilon",
    "org:globex",
    "org:guild",
    "org:north-trail-club",
    "org:springfield-cyclery",
    "org:springfield-movers",
    "person:a",
    "person:acting-beta",
    "person:alias-rename-was",
    "person:alias-rekey-alpha",
    "person:noted-alpha",
    "person:already-here",
    "person:alpha",
    "person:alpha-2",
    "person:alpha-a",
    "person:alpha-b",
    "person:alpha-one",
    "person:alpha-two",
    "person:alphaa",
    "person:alphonse",
    "person:backfill-alpha",
    "person:badge-alpha",
    "person:badge-beta",
    "person:badge-gamma",
    "person:barney-gumble",
    "person:bart",
    "person:bet",
    "person:beta",
    "person:bodoque",
    "person:contract-chainless",
    "person:contract-claim-histories",
    "person:contract-claim-histories-ghost",
    "person:contract-corrected",
    "person:contract-derived",
    "person:contract-no-such-chain",
    "person:contract-nobody",
    "person:contract-fields",
    "person:contract-no-facts-no-counts",
    "person:contract-orient",
    "person:contract-revision-count",
    "person:cosme-fulanito",
    "person:fill-alpha",
    "person:fill-beta",
    "person:frontdoor-probe",
    "person:ghost",
    "person:ghostly",
    "person:homer",
    "person:homer-simpson",
    "person:kept-alpha",
    "person:kept-beta",
    "person:kappa",
    "person:maude",
    "person:milhouse",
    "person:milhouse-2",
    "person:projected",
    "person:ralph",
    "person:rekey-alpha",
    "person:flanders-original",
    "person:ned-flander",
    "person:ned-flanders",
    "person:nelson",
    "person:nelson-2",
    "person:nobody-such-handle",
    "person:otto",
    "person:patana",
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
    "project:jojobot-server",
    "project:visa",
    "rhythm:chain-check",
    "rhythm:deep-clean",
    "rhythm:descale",
    "rhythm:half-made",
    "rhythm:orphan",
    "rhythm:read-meter",
    "rhythm:water-the-fern",
    "rhythm:pay-the-jukebox-lease",
    "rhythm:swap-the-air-filter",
    "rhythm:weekly-review",
    "rhythm:worming",
    "thing:jukebox",
    "thing:the-fern",
    "thing:the-air-filter",
    "thing:kettl",
    "thing:kettle",
    "thing:no-such-bike",
    "thing:bar-tape",
    "thing:bike-chain",
    "thing:blue-kite",
    "thing:commit-omicron",
    "thing:floor-pump",
    "thing:folding-chairs",
    "thing:gamma",
    "thing:gravel-bike",
    "thing:leftorium-menu",
    "thing:record-crate",
    "thing:red-bike",
    "thing:red-bikee",
    "thing:red-kite",
    "thing:road-bike",
    "thing:sigma",
    "thing:tau",
    "thing:that-search-summary",
    "thing:torque-wrench",
    "thing:trail-email",
    "topic:widgets",
    "work:first-mix",
    "work:phi",
    "work:sigma",
    // **The shared contract suite's own fixtures, and the case labels beside
    // them.** They were invisible until the fixtures named their handles whole:
    // a slug handed to a constructor carries no handle for this scan to read.
    // Most are labels saying what a case is about rather than names of anybody.
    "bot:sigma",
    "bot:contract-epsilon",
    "bot:contract-ghost-bot",
    "bot:contract-graph-one",
    "bot:contract-graph-two",
    "event:contract-connected-fest",
    "event:contract-connected-other",
    "event:contract-graph-gathering",
    "event:contract-leaving-party",
    "event:contract-lone-crossing",
    "event:contract-long-weekend",
    "event:contract-the-jotting",
    "event:contract-the-sketch",
    "event:contract-unlinked-fair",
    "event:contract-unheard-of-fest",
    "event:contract-winter-fest",
    "event:contract-withdrawn-gathering",
    "org:contract-fitting-thing",
    "org:contract-mention-guild",
    "org:contract-loose-record",
    "org:contract-orient-guild",
    "org:contract-pinnable-guild",
    "org:contract-riversid",
    "org:contract-riverside",
    "org:contract-self-labelled",
    "org:contract-unscreened",
    "org:contract-unscreened-twin",
    "person:comma-carrier",
    "person:contract-addressable",
    "person:contract-addresse",
    "person:contract-addressee",
    "person:contract-alias-borrower",
    "person:contract-alias-owner",
    "person:contract-alpha",
    "person:contract-pumpback",
    "person:contract-summertime",
    "person:contract-appended",
    "person:contract-away-talker",
    "person:contract-backing",
    "person:contract-backslash",
    "person:contract-brimful",
    "person:contract-cleared",
    "person:contract-clear-marker",
    "person:contract-clocks",
    "person:contract-confirmed-guess",
    "person:contract-conn-one",
    "person:contract-conn-two",
    "person:contract-counted",
    "person:contract-crate-partial",
    "person:contract-crate-unrelated",
    "person:contract-crate-whole",
    "person:contract-crosslink",
    "person:contract-demotable",
    "person:contract-duet",
    "person:contract-edged",
    "person:contract-edge-guarded",
    "person:contract-edge-later",
    "person:contract-edge-stranger",
    "person:contract-editable",
    "person:contract-evented",
    "person:contract-field-edit",
    "person:contract-folded-kept",
    "person:contract-folded-spare",
    "person:contract-fold-rename-before",
    "person:contract-fold-rename-after",
    "person:contract-fold-rename-survivor",
    "person:contract-fold-rename-onlooker",
    "thing:contract-fold-rename-child-former",
    "thing:contract-fold-rename-child-current",
    "person:contract-gene",
    "session:contract-gamma-run",
    "person:contract-filed",
    "person:contract-graph-coming",
    "person:contract-graph-outsider",
    "person:contract-graph-staying",
    "person:contract-hedged-word",
    "person:contract-inked",
    "person:contract-hijack-subject",
    "person:contract-injector",
    "person:contract-late-edge",
    "person:contract-lineage",
    "person:contract-many-labelled",
    "person:contract-many-named",
    "person:contract-mention-author",
    "person:contract-address-restart-was",
    "person:contract-edge-subject",
    "person:contract-former-handle-never",
    "person:contract-parent-was",
    "person:contract-parent-child",
    "person:contract-ref-subject",
    "person:contract-rename-author",
    "person:contract-rename-attempt-never-existed",
    "person:contract-double-rename-elsewhere",
    "person:contract-retype-parent",
    "person:contract-stale-edit-was",
    "person:contract-stale-retract-was",
    "person:contract-mention-bookkeeper",
    "person:contract-mention-broken",
    "person:contract-mention-gone",
    "person:contract-mention-kept",
    "person:contract-mention-moved",
    "person:contract-mention-nobody",
    "person:contract-mention-rider",
    "person:contract-mention-typist",
    "person:contract-milhouse",
    "person:contract-miskinded",
    "person:contract-missing-row",
    "person:contract-multi",
    "person:contract-nearslug",
    "person:contract-nearslugg",
    "person:contract-never-captured",
    "person:contract-nickname-only",
    "person:contract-nobody-created-this",
    "person:contract-no-such",
    "person:contract-observed",
    "person:contract-omicron",
    "person:contract-oneway",
    "person:contract-orienteer",
    "person:contract-orjent",
    "person:contract-otto",
    "person:contract-pallet-messy",
    "person:contract-nelson",
    "person:contract-pipe",
    "person:pointer-migration-blocked-subject",
    "person:pointer-migration-subject",
    "person:contract-promotable",
    "person:contract-provenance",
    "person:contract-quagmire",
    "person:contract-readback",
    "person:contract-redated",
    "person:contract-ref-guarded",
    "person:contract-refutable",
    "person:contract-relation-owner",
    "person:contract-relic-forwarded",
    "person:contract-relic-survivor",
    "person:contract-renamed-onto",
    "person:contract-renamer",
    "person:contract-reopening",
    "person:contract-reserved-key",
    "person:contract-retracted",
    "person:contract-retraction-hit",
    "person:contract-retract-miss",
    "person:contract-searchable",
    "person:contract-search-superseded",
    "person:contract-settle-gate",
    "person:contract-sigma",
    "person:contract-supplied-pointer",
    "person:contract-superseded-wording",
    "person:contract-silent-standing",
    "person:contract-solo",
    "person:contract-spoken",
    "person:contract-stamped",
    "person:contract-tallied",
    "person:contract-tau",
    "person:contract-totalled",
    "person:contract-towelie",
    "person:contract-unbacked-guess",
    "person:contract-unlinked-absent",
    "person:contract-unlinked-attended",
    "person:contract-unreasoned",
    "person:contract-unwritten",
    "person:contract-upsilon",
    "person:contract-whitespace",
    "person:contract-withdrawn-apart",
    "person:contract-withdrawn-pulled",
    "person:contract-withdrawn-stood",
    "person:contract-zenit",
    "person:contract-zenith",
    "person:kind-set-reader",
    "pet:contract-mention-dog",
    "pet:contract-pet-cat",
    "pet:contract-santas-little-helper",
    "pet:contract-the-heavy-one",
    "place:contract-faraway",
    "place:contract-mention-inn",
    "place:contract-mention-yard",
    "place:contract-far-country",
    "place:contract-fjord-town",
    "place:contract-harbour-end",
    "place:contract-kiln-yard",
    "place:contract-moes",
    "place:contract-north-trail",
    "place:contract-nowhere-in-particular",
    "place:contract-orient-hall",
    "place:contract-riverbend",
    "place:contract-riverbnd",
    "place:contract-tavern",
    "project:contract-atlas",
    "project:contract-away-project",
    "project:contract-bad-parent",
    "project:contract-ghost-parent",
    "project:contract-ghost-parnt",
    "project:contract-kwik-e",
    "project:contract-kwik-e-squishee",
    "project:contract-monorail",
    "project:contract-monorail-funding",
    "project:contract-ouroboros",
    "project:contract-plant",
    "project:contract-plnt",
    "project:contract-shift-rota",
    "project:contract-springfield",
    "project:contract-springfield-brakes",
    "project:contract-springfield-cars",
    "project:contract-springfield-track",
    "project:contract-the-sketchbook",
    "rhythm:contract-descale",
    "rhythm:corner-stall",
    "rhythm:holds-nothing",
    "thing:contract-cleared-key",
    "thing:contract-clear-off-address",
    "thing:contract-edited-older-record",
    "thing:contract-folded-thing",
    "thing:contract-kettle",
    "thing:contract-long-history",
    "thing:contract-marker-not-a-field",
    "thing:contract-pallet-half",
    "thing:contract-pallet-whole",
    "thing:contract-red-bike",
    "thing:contract-red-bikee",
    "thing:contract-edge-was",
    "thing:contract-hijack-elsewhere",
    "thing:contract-hijack-target",
    "thing:contract-parented-dependent",
    "thing:contract-parented-departed",
    "thing:contract-parented-vacancy",
    "thing:pointer-migration-child",
    "thing:pointer-migration-never-existed",
    "thing:pointer-migration-object",
    "thing:contract-former-handle-was",
    "thing:contract-ref-was",
    "thing:contract-rename-was",
    "thing:contract-retype-was",
    "thing:contract-retype-child",
    "thing:contract-rename-never-existed",
    "thing:contract-supplied-rename-stored",
    "thing:contract-supplied-rename-stored-now",
    "thing:contract-double-rename-was",
    "thing:contract-mention-cart",
    "thing:contract-mention-was",
    "thing:contract-migrate-mentions-was",
    "thing:contract-mailbox-mention-was",
    "thing:contract-mailbox-mention-now",
    "thing:contract-mailbox-mention-never-existed",
    "thing:contract-journal-mention-was",
    "thing:contract-journal-mention-now",
    "thing:contract-journal-mention-never-existed",
    "thing:contract-relation-held",
    "thing:contract-short-history",
    "thing:contract-the-ledger",
    "thing:contract-the-scrap",
    "thing:contract-vocabulary-answerer",
    "thing:green-kite",
    "thing:handcart",
    "thing:handcart-2",
    "thing:handcart-lost",
    "topic:contract-run-of-stalls",
    "topic:contract-widgets",
    "view:my-week",
    "work:contract-address-restart-now",
    "work:contract-conn-mix",
    "work:contract-stale-edit-now",
    "work:contract-stale-retract-now",
    "work:contract-edge-now",
    "work:contract-former-handle-now",
    "work:contract-mention-now",
    "work:contract-ref-now",
    "work:contract-rename-now",
    "work:contract-double-rename-now",
    "work:contract-retype-now",
    "work:contract-parent-now",
    "work:alias-rename-now",
    "work:handcart",
    "work:red-bike",
    "work:contract-first-mix",
    "work:contract-the-stay",
];

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
        for handle in handles_in(&text)
            .into_iter()
            .chain(two_part_handles_in(&text))
        {
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

/// **The constructors that build a handle out of two arguments**, and the
/// reason this is a list rather than a rule.
///
/// [`handles_in`] reads `kind:slug` as text, so it cannot see a name whose two
/// halves never meet: `add_args("thing", "red-kite", …)` and a payload carrying
/// `"kind"` and `"handle"` as separate keys both create an entity the scan
/// never reads. **That is the commonest way a fixture is named**, so the gate
/// that enforces the bright line was blind to the idiom most authors reach for.
///
/// ⛔️ **A rule instead of a list would have to guess which bare string is a
/// slug, and that guess does not work.** Measured over this workspace: flagging
/// any literal following any kind literal reports 38 off-roster names, of which
/// 5 are real — `"session", "timezone"` and `"pet", "owner"` are a migration's
/// table and column, not a handle. **A gate that is wrong six times out of
/// seven is one people learn to wave through**, which costs more than the hole.
///
/// So this names the constructors. **It can go stale, exactly as the roster
/// itself can**, and a new way of building a handle is invisible until somebody
/// adds it here. That is the trade: a list that is sometimes short, against a
/// rule that is usually wrong.
const TWO_PART: &[(&str, &str)] = &[
    // The test helper: `add_args("thing", "red-kite", "Red Kite")`.
    ("add_args(", ","),
    // A payload naming the halves as its own keys.
    ("\"kind\":", "\"handle\":"),
];

/// Every handle in the text built out of two separate literals.
///
/// **Deliberately tolerant of whitespace and strict about everything else**: it
/// reads the next quoted token after the anchor, requires it to be a kind, then
/// reads the next quoted token after the separator. A pair that is not a kind
/// followed by a slug is not a handle and is passed over.
fn two_part_handles_in(text: &str) -> Vec<String> {
    let known = kinds();
    let mut found = Vec::new();
    for (anchor, separator) in TWO_PART {
        for (at, _) in text.match_indices(anchor) {
            let after = at + anchor.len();
            let Some((kind, ended)) = quoted_from(&text[after..]) else {
                continue;
            };
            if !known.iter().any(|k| *k == kind) {
                continue;
            }
            let rest = &text[after + ended..];
            let Some(sep) = rest.find(separator) else {
                continue;
            };
            // Only across a short gap: the two halves of one call, never the
            // next one down the file.
            if sep > 40 {
                continue;
            }
            let Some((slug, _)) = quoted_from(&rest[sep + separator.len()..]) else {
                continue;
            };
            if slug.is_empty() || !slug.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                continue;
            }
            if slug
                .chars()
                .any(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-')
            {
                continue;
            }
            found.push(format!("{kind}:{slug}"));
        }
    }
    found
}

/// The next quoted token in `text`, and where its closing quote ends.
fn quoted_from(text: &str) -> Option<(String, usize)> {
    let opened = text.find('"')?;
    if text[..opened]
        .chars()
        .any(|c| !c.is_whitespace() && c != ',' && c != ':')
    {
        return None;
    }
    let rest = &text[opened + 1..];
    let closed = rest.find('"')?;
    Some((rest[..closed].to_string(), opened + 1 + closed + 1))
}

/// **The gendered pronouns of English.** A closed set of words rather than a
/// list of sentences somebody wrote: the words are the language's and cannot
/// go stale, where a catalogue of forbidden phrasings re-embeds what it
/// filters and misses the next author's wording (rule 106).
const PRONOUNS: &[&str] = &[
    "he", "him", "his", "she", "her", "hers", "himself", "herself",
];

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
    for entry in ROSTER {
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
fn names_somebody(block: &str, names: &[String]) -> bool {
    block.contains("person:")
        || block.contains("pet:")
        || block.contains("bot:")
        || names.iter().any(|name| says_word(block, name, true))
}

/// **Every commit that this checkout has not pushed**, as its short name and
/// its message.
///
/// The range is READ rather than configured: whatever is on `HEAD` and not on
/// `origin/main`. Nobody maintains a count, and a push shortens the range by
/// itself.
///
/// **It reads the message and never the diff.** What a commit changed is not
/// this check's business; what it SAYS is.
///
/// ⚠️ **A range this cannot read is a failure rather than a pass.** No git, no
/// repository, no `origin/main` to compare against — in each case the check did
/// not run, and reporting that as a clean range would be the exact lie every
/// other gate in this file is built to avoid.
fn unpushed_commits() -> Vec<(String, String)> {
    let out = std::process::Command::new("git")
        .args(["log", "--format=%h%x00%B%x01", "origin/main..HEAD"])
        .current_dir(workspace_root())
        .output()
        .expect(
            "this check reads the unpushed commit messages and needs git on the PATH. \
             It did not run.",
        );
    assert!(
        out.status.success(),
        "the unpushed range could not be read, so this check did not run — it compares \
         HEAD against origin/main, and one of them is missing here: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout)
        .split('\u{1}')
        .filter_map(|commit| commit.trim().split_once('\u{0}'))
        .map(|(name, message)| (name.to_string(), message.to_string()))
        .collect()
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
/// are the fictional names this repo contains, plus some that are free" is
/// not.
#[test]
fn the_roster_carries_no_name_the_workspace_has_stopped_using() {
    let files = scanned_files(&workspace_root());
    let corpus: String = files
        .iter()
        .filter(|f| f.file_name().is_some_and(|n| n != "fixture_roster.rs"))
        .map(|f| fs::read_to_string(f).expect("readable source file"))
        .collect();

    // **The same two ways of naming a handle the violation scan reads**, or the
    // two questions stop being opposites: a name built out of two literals
    // would be caught as off-roster by one test and reported as an unused
    // entry by the other, and adding it to the roster could not satisfy both.
    let built = two_part_handles_in(&corpus);
    let orphaned: Vec<&str> = ROSTER
        .iter()
        .copied()
        .filter(|handle| !corpus.contains(handle) && !built.iter().any(|b| b == handle))
        .collect();
    assert!(
        orphaned.is_empty(),
        "roster entries nothing in the workspace uses — delete them, or the allowlist \
         stops being a record of what is here and becomes a pool of names nobody \
         reviewed the use of:\n{}",
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
    let on_it = ROSTER
        .iter()
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
        !ROSTER.contains(&whole.as_str()),
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
        "handles outside the fictional roster — if the name is openly fictional, \
         add it to ROSTER in a conscious diff; if it names anything real from the \
         operator's life, it must not enter this repo at all:\n{}",
        violations.join("\n")
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

/// Every bare slug handed to a handle constructor, as `call in path`.
///
/// **A slug carries its kind or it does not.** The argument is read as written:
/// one that already holds a colon is a whole handle and the scan above reads
/// it; one that does not is a name nothing compares against the roster.
///
/// A non-literal argument — a variable, a `format!` — is out of reach here and
/// is out of reach of any text scan. What this catches is the literal, which is
/// where a name a person typed actually enters.
fn bare_slugs_in(files: &[PathBuf]) -> Vec<String> {
    let mut bare = Vec::new();
    for file in files {
        if file.file_name().is_some_and(|n| n == "fixture_roster.rs") {
            continue;
        }
        let text = fs::read_to_string(file).expect("readable source file");
        for (call, argument) in constructor_arguments(&text) {
            if argument.contains(':') {
                continue;
            }
            bare.push(format!("{call}(\"{argument}\") in {}", file.display()));
        }
    }
    bare.sort();
    bare.dedup();
    bare
}

/// **The two constructors that take a handle apart**, with the literal each
/// one is handed.
///
/// `EntityId::person` takes the slug first, so its argument is the literal that
/// opens the call. `EntityId::new` takes the kind first and the slug second, so
/// the scan walks past the kind and reads the literal after the comma.
///
/// ⚠️ **A call whose KIND is a variable is skipped, and that is a stated limit
/// rather than an oversight.** A kind can be declared by a caller at runtime,
/// and a handle under one carries a token that is not in the domain's set — so
/// the scan above would not read it as a handle even written whole, and
/// demanding the whole handle there would buy nothing. What this covers is a
/// handle whose kind the scan can name.
fn constructor_arguments(text: &str) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    for (call, takes_a_kind) in [("EntityId::person", false), ("EntityId::new", true)] {
        let needle = format!("{call}(");
        for (idx, _) in text.match_indices(&needle) {
            let mut rest = &text[idx + needle.len()..];
            if takes_a_kind {
                let kind = rest.trim_start();
                // The kind written out, rather than one a caller declared and
                // this call resolved: only the first is a token the scan knows.
                if !kind.starts_with("EntityKind::")
                    && !kind.starts_with("jojobot_domain::memory::EntityKind::")
                {
                    continue;
                }
                let Some(comma) = rest.find(',') else {
                    continue;
                };
                rest = &rest[comma + 1..];
            }
            let rest = rest.trim_start();
            let Some(literal) = rest.strip_prefix('"') else {
                continue;
            };
            let Some(end) = literal.find('"') else {
                continue;
            };
            found.push((call, literal[..end].to_string()));
        }
    }
    found
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
    "bot",
    "content",
    "file_path",
    "id",
    "input",
    "message",
    "model",
    "name",
    "result",
    "role",
    "subtype",
    "text",
    "thinking",
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

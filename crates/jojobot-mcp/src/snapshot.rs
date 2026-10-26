//! **Every text a model reads, in one checked-in file.**
//!
//! A change to a text a model reads changes what the model does, and a review
//! of the code that produced the text does not show the change: the words sit in
//! a string literal, a schema derived from a doc comment, a table of refusals
//! built far from the verb that raises them. This renders them all into
//! `served-texts.md` beside this crate's manifest and fails on any difference,
//! so a commit that changes a served text changes that file in the same commit
//! and the change is read, and accepted, by name.
//!
//! It does not judge whether a change is good. It makes the change visible.
//!
//! **One file per surface**, in `served-texts/` beside this crate's manifest, so
//! two commits that change two surfaces never touch one file: `instructions.md`,
//! `tools.md`, `orientation.md`, `skills.md`, `teachings.md`, `refusals.md`.
//!
//! **To accept a change**, run the test with `SNAPSHOT_UPDATE=1` (`make
//! snapshot` does) and commit the files it rewrote.
//!
//! # Rendered by the production code, and only by it
//!
//! Nothing here is a second copy of a text. Each text is read from the code
//! that serves it: the tool router for the tools, the constant the boot serves
//! for the essay, the skill table for the skills, the functions that answer for
//! the refusals, and a real write for the status refusal. A text this test
//! carried itself could drift from what is served, and the file would then agree
//! with the test and not with the server.
//!
//! # What is covered
//!
//! * the instructions the server sends at connection;
//! * every tool: its description, its argument structure, and the description
//!   of every argument, at every depth;
//! * the orientation essay (its core and each section of the rest), and the
//!   sentences of an anonymous boot and of a named boot of the shipped identity;
//! * every skill: its index line and its body;
//! * the teachings that ride on a write;
//! * every refusal the three error lanes (memory, mailbox, session) raise, one
//!   row per error kind, as the error reads and as the caller is answered;
//! * **the refusal of a status outside the vocabulary, rendered from a live
//!   write** and printed beside the vocabulary the domain holds, so a change to
//!   the statuses a piece of work may carry shows in the file.
//!
//! # What it cannot reach
//!
//! A refusal whose words are written at the verb that raises it, from data it
//! only has at that moment (the stranger gate on a role object, a limit of
//! zero, a body too large to deliver), is built inline and no table lists it.
//! Its words show in these files only if the same words are also in a
//! description or in an error kind's template. So a reworded inline refusal can
//! change what a model reads without changing a file here. Nor does it cover a
//! text built from stored data (a charter somebody wrote, a rule on a bot), or
//! the words a model reads in an answer's own fields. The replay guard and the
//! paid run are what see those.
//!
//! Test-only, and declared by `lib.rs`.

use super::*;
use crate::harness::*;
use rmcp::handler::server::wrapper::Parameters;
use std::path::{Path, PathBuf};

/// The directory of checked-in files, beside this crate's manifest.
const SNAPSHOT_DIRECTORY: &str = "served-texts";

/// **The variable that rewrites the file instead of comparing with it.**
const UPDATE_VARIABLE: &str = "SNAPSHOT_UPDATE";

fn snapshot_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT_DIRECTORY)
}

/// **The file being written**: headings that name where a text comes from, each
/// text in a block of its own, so a diff of the file reads as a diff of the
/// texts.
struct Doc {
    out: String,
}

impl Doc {
    fn new() -> Doc {
        Doc { out: String::new() }
    }

    fn heading(&mut self, level: usize, title: &str) {
        self.out
            .push_str(&format!("{} {title}\n\n", "#".repeat(level)));
    }

    fn prose(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push_str("\n\n");
    }

    /// One text under a heading of its own, in a fenced block. The fence is
    /// longer than any run of backticks in the text, so the text cannot end
    /// its own block.
    fn text(&mut self, level: usize, source: &str, language: &str, text: &str) {
        self.heading(level, source);
        let longest = text
            .split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or_default();
        let fence = "`".repeat(longest.max(2) + 1);
        self.out.push_str(&format!("{fence}{language}\n"));
        self.out.push_str(text.trim_end_matches('\n'));
        self.out.push_str(&format!("\n{fence}\n\n"));
    }
}

/// **A one-line text, one sentence to a line**, so a change to one sentence of
/// a long description is one changed line in the file. Text that already has
/// line breaks is left as it is.
fn sentence_per_line(text: &str) -> String {
    if text.contains('\n') {
        text.to_string()
    } else {
        text.replace(". ", ".\n")
    }
}

/// **Take every description out of a schema, and say where each was.** A
/// `description` that is a string is text a model reads; one under a
/// `properties` map is an argument that happens to be called `description`.
fn lift_descriptions(node: &mut serde_json::Value, path: &str, out: &mut Vec<(String, String)>) {
    match node {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(text)) = map.get("description") {
                out.push((path.to_string(), text.clone()));
                map.remove("description");
            }
            for (key, child) in map.iter_mut() {
                if key == "properties" {
                    if let serde_json::Value::Object(properties) = child {
                        for (name, property) in properties.iter_mut() {
                            lift_descriptions(property, &format!("{path}.{name}"), out);
                        }
                    }
                } else {
                    lift_descriptions(child, &format!("{path}/{key}"), out);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for (n, item) in items.iter_mut().enumerate() {
                lift_descriptions(item, &format!("{path}[{n}]"), out);
            }
        }
        _ => {}
    }
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("a value prints")
}

/// **Every string in an answer that is a sentence**, with the path it sits at:
/// long enough, and with a space in it. Ids, counts and handles are not texts.
fn sentences(node: &serde_json::Value, path: &str, out: &mut Vec<(String, String)>) {
    match node {
        serde_json::Value::String(text) if text.len() >= 40 && text.contains(' ') => {
            out.push((path.to_string(), text.clone()));
        }
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                sentences(child, &format!("{path}.{key}"), out);
            }
        }
        serde_json::Value::Array(items) => {
            for (n, item) in items.iter().enumerate() {
                sentences(item, &format!("{path}[{n}]"), out);
            }
        }
        _ => {}
    }
}

fn tools(doc: &mut Doc) {
    doc.heading(2, "Tools");
    doc.prose(
        "Each tool: its description, the structure of its arguments with every \
         description lifted out, then the description of each argument by the path it sits at.",
    );
    let mut all = Jojobot::tool_router().list_all();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    for tool in all {
        let name = tool.name.to_string();
        doc.heading(3, &format!("Tool `{name}`"));
        let mut whole = serde_json::to_value(&tool).expect("a tool serializes");
        let description = whole
            .as_object_mut()
            .and_then(|map| map.remove("description"))
            .and_then(|value| value.as_str().map(str::to_string));
        match description {
            Some(text) => doc.text(
                4,
                &format!("`{name}` — description"),
                "text",
                &sentence_per_line(&text),
            ),
            None => doc.prose("(no description)"),
        }
        let mut lifted = Vec::new();
        lift_descriptions(&mut whole, "", &mut lifted);
        doc.text(4, &format!("`{name}` — structure"), "json", &pretty(&whole));
        for (path, text) in lifted {
            let path = if path.is_empty() { "(tool)" } else { &path };
            doc.text(
                4,
                &format!("`{name}` — argument `{path}`"),
                "text",
                &sentence_per_line(&text),
            );
        }
    }
}

async fn boot(doc: &mut Doc, label: &str, bot: Option<&str>) {
    let jojobot = handler_shipped();
    // A named boot meets an identity that exists: the shipped one is made first,
    // as a fresh instance arrives holding it.
    if let Some(slug) = bot {
        make_bot(&jojobot, slug).await;
    }
    let result = jojobot
        .start_here(Parameters(crate::orientation::start_here::OrientArgs {
            request_id: None,
            compacted: None,
            claim: None,
            timezone: Some("UTC".into()),
            bot: bot.map(str::to_string),
            brief: None,
            skill: None,
            section: None,
            resume: None,
            sid: None,
            today: Some("2026-01-01".into()),
            ask_for_code: None,
            challenge: None,
            code: None,
            offer_code: None,
        }))
        .await
        .expect("start_here answers");
    let mut found = Vec::new();
    sentences(&json_of(&result), "", &mut found);
    for (path, text) in found {
        doc.text(
            4,
            &format!("{label} — `{path}`"),
            "text",
            &sentence_per_line(&text),
        );
    }
}

async fn orientation(doc: &mut Doc) {
    doc.heading(2, "Orientation");
    doc.text(
        3,
        "Essay — core (always served whole)",
        "markdown",
        crate::orientation::essay::ORIENTATION_CORE.as_str(),
    );
    let (opening, sections) = crate::orientation::essay::remainder();
    doc.text(
        3,
        "Essay — the rest, opening paragraphs",
        "markdown",
        opening,
    );
    for section in sections {
        doc.text(
            3,
            &format!("Essay — the rest, section `{}`", section.heading),
            "markdown",
            section.body,
        );
    }
    doc.heading(3, "Boot answers");
    doc.prose(
        "Every sentence in the answer to `start_here`, by the path it sits at. An id, a count and \
         a handle are not texts and are left out. Booted on the shipped build with its own \
         identity only, in UTC, on a fixed day.",
    );
    boot(doc, "Anonymous boot", None).await;
    boot(doc, "Named boot of the shipped identity", Some("assistant")).await;
    doc.text(
        3,
        "Unseated rule whose label exceeds one line",
        "text",
        crate::orientation::identity::UNSEATED_NEEDS_LABEL,
    );
}

fn skills(doc: &mut Doc) {
    doc.heading(2, "Skills");
    for skill in crate::orientation::skills::SKILLS {
        doc.text(
            3,
            &format!("Skill `{}` — index line (when to use)", skill.name),
            "text",
            &sentence_per_line(skill.when_to_use),
        );
        doc.text(
            3,
            &format!("Skill `{}` — body", skill.name),
            "markdown",
            skill.body,
        );
    }
}

fn teachings(doc: &mut Doc) {
    use crate::teaching::*;
    doc.heading(2, "Teachings");
    doc.prose("Each rides once per session, on the verb it is about.");
    let fixed: &[(&str, &str)] = &[
        ("CLAIMS_TEACHING", CLAIMS_TEACHING),
        ("CLAIM_SUBJECT_TEACHING", CLAIM_SUBJECT_TEACHING),
        ("RHYTHM_HISTORY_TEACHING", RHYTHM_HISTORY_TEACHING),
        ("CLAIM_DIRECTION_TEACHING", CLAIM_DIRECTION_TEACHING),
        ("PROJECTS_SKILL_TEACHING", PROJECTS_SKILL_TEACHING),
        ("RHYTHMS_SKILL_TEACHING", RHYTHMS_SKILL_TEACHING),
        ("DUPLICATE_REPAIR_TEACHING", DUPLICATE_REPAIR_TEACHING),
        ("RHYTHM_ARCHIVE_TEACHING", RHYTHM_ARCHIVE_TEACHING),
        ("CHECK_IN_DATE_TEACHING", CHECK_IN_DATE_TEACHING),
        ("OPERATOR_QUOTE_TEACHING", OPERATOR_QUOTE_TEACHING),
        ("DEPENDS_ON_READ_TEACHING", DEPENDS_ON_READ_TEACHING),
    ];
    for (name, text) in fixed {
        doc.text(
            3,
            &format!("Teaching `{name}`"),
            "text",
            &sentence_per_line(text),
        );
    }
    doc.text(
        3,
        "Teaching `session_id_teaching` (with a sample sid and session)",
        "text",
        &sentence_per_line(&session_id_teaching("a1b2", "c3d4")),
    );
    doc.text(
        3,
        "Teaching `where_on_a_broad_read_teaching` (with a sample kind, count and key)",
        "text",
        &sentence_per_line(&where_on_a_broad_read_teaching("work", 8, "status", "now")),
    );
    doc.text(
        3,
        "Teaching `field_shadows_argument_teaching` (with a sample key and verb)",
        "text",
        &sentence_per_line(&field_shadows_argument_teaching("note", "capture")),
    );
    doc.text(
        3,
        "Teaching `empty_shipped_type_teaching` (with a sample type)",
        "text",
        &sentence_per_line(&empty_shipped_type_teaching("trip")),
    );
    // **The search caveats**: the two standing notes a first search, and every thin
    // one, carries. They are built by the functions the verb's own answer uses, so
    // a changed word shows in this diff.
    use crate::memory::search::{corpus_note, matching_note};
    use jojobot_domain::memory::search::SearchQuery;
    for (title, note) in [
        (
            "Search caveat `matching` (a query of words)",
            matching_note(&SearchQuery::text("a few words"), true),
        ),
        (
            "Search caveat `matching` (a query that is a handle)",
            matching_note(&SearchQuery::text("thing:sigma"), true),
        ),
        (
            "Search caveat `corpus` (current wording only)",
            corpus_note(false),
        ),
        (
            "Search caveat `corpus` (include_history set)",
            corpus_note(true),
        ),
    ] {
        doc.text(
            3,
            title,
            "text",
            &sentence_per_line(note["note"].as_str().expect("a caveat carries a note")),
        );
    }
}

/// **A refusal as the caller is answered**: the body of a blocked answer, or
/// the message and data of a protocol error.
fn answered(result: Result<rmcp::model::CallToolResult, rmcp::ErrorData>) -> String {
    match result {
        Ok(result) => pretty(&json_of(&result)),
        Err(error) => format!(
            "protocol error: {}\n{}",
            error.message,
            error.data.as_ref().map_or(String::new(), pretty)
        ),
    }
}

/// **The note on a status outside the vocabulary, from a live write.** A piece
/// of work is given a status nobody declared. It lands, and the answer carries a
/// note the caller reads, so the words in it are the words the build composes
/// from the statuses it holds today. Only the note is printed: the rest of the
/// answer holds a timestamp. The vocabulary is printed first, from the same
/// constant, so a status added or dropped changes this file even where the
/// note's wording does not show it.
async fn status_note(doc: &mut Doc) {
    use jojobot_domain::memory::kinds::WORK_STATUSES;
    let jojobot = handler_shipped();
    crate::memory::testing::ensure(&jojobot, "work:atlas-root").await;
    let mut args = crate::memory::testing::capture_args("work:atlas-root", "a piece of work");
    args.fields = Some([("status".to_string(), "nonsense".to_string())].into());
    let result = jojobot
        .capture(Parameters(args))
        .await
        .expect("a status outside the vocabulary lands");
    let answer = json_of(&result);
    let note = answer["status_word_note"]
        .as_str()
        .expect("the answer carries the note");
    doc.text(
        3,
        "Note on a status outside the vocabulary, rendered from a live write",
        "text",
        &format!(
            "vocabulary (WORK_STATUSES): {}\nstatus_word_note: {}",
            WORK_STATUSES.join(", "),
            note
        ),
    );
}

async fn refusals(doc: &mut Doc) {
    doc.heading(2, "Refusals");
    doc.prose(
        "One row per error kind in each of the three lanes, built from the sample the lane's own \
         table of kinds holds: the error as it reads, then the answer the caller gets. The sample \
         data is placeholder text. **Not here:** a refusal written inline at the verb that raises \
         it, from data it only has at that moment. Its words appear in this file only where the \
         same words are in a description or an error kind's template.",
    );
    status_note(doc).await;
    let breaks_must = |error: &jojobot_domain::memory::MemoryError| {
        matches!(
            error,
            jojobot_domain::memory::MemoryError::BreaksMust { .. }
        )
    };
    for (label, error, _) in crate::memory::declined::tests::memory_refusal_rows() {
        // **A refusal that cannot be raised is not catalogued.** While a type's
        // rule is switched off no write meets `BreaksMust`, and a catalogue row for
        // it would teach a session to expect it.
        if breaks_must(&error) && jojobot_domain::memory::must::SWITCHED_OFF {
            continue;
        }
        let reads = error.to_string();
        // **A `must` refusal is answered differently by the verbs that take an
        // override and by those that do not.** `capture` is shown as it serves it,
        // with the token; the generic wording follows under a verb that serves it.
        // **A token is keyed by a secret the process makes at startup**, so it
        // differs on every run. The file shows where it stands, not its value.
        let token = match &error {
            jojobot_domain::memory::MemoryError::BreaksMust {
                subject,
                name,
                rule,
                ..
            } => Some(jojobot_domain::memory::must::override_token(
                subject, name, rule,
            )),
            _ => None,
        };
        let said = answered(if breaks_must(&error) {
            crate::memory::must_override::declined_or_lifted(
                "capture",
                error,
                &crate::memory::declined::TokenSlot::unsent(),
            )
        } else {
            crate::memory::declined::memory_declined("capture", error)
        });
        let said = match token {
            Some(token) => said.replace(&token, "<override_token>"),
            None => said,
        };
        doc.text(
            3,
            &format!("Memory refusal `{label}`"),
            "text",
            &format!("reads: {}\nanswer: {said}", sentence_per_line(&reads)),
        );
    }
    for (label, error, _) in crate::memory::declined::tests::memory_refusal_rows() {
        if !breaks_must(&error) || jojobot_domain::memory::must::SWITCHED_OFF {
            continue;
        }
        let reads = error.to_string();
        let said = answered(crate::memory::declined::memory_declined(
            "add_entity",
            error,
        ));
        doc.text(
            3,
            &format!("Memory refusal `{label}` from a verb that takes no override_token"),
            "text",
            &format!("reads: {}\nanswer: {said}", sentence_per_line(&reads)),
        );
    }
    for (label, error, _) in crate::mailboxes::declined::tests::mailbox_refusal_rows() {
        let reads = error.to_string();
        let said = answered(crate::mailboxes::declined::mailbox_declined(error));
        doc.text(
            3,
            &format!("Mailbox refusal `{label}`"),
            "text",
            &format!("reads: {}\nanswer: {said}", sentence_per_line(&reads)),
        );
    }
    for (label, error, _) in crate::session::declined::tests::session_refusal_rows() {
        let reads = error.to_string();
        let said = answered(
            crate::session::declined::session_declined(error, "a1b2")
                .map(crate::orientation::in_flight::unmarked),
        );
        doc.text(
            3,
            &format!("Session refusal `{label}`"),
            "text",
            &format!("reads: {}\nanswer: {said}", sentence_per_line(&reads)),
        );
    }
    doc.text(
        3,
        "Refusal `session_unbound` (a call with no sid)",
        "text",
        &answered(Ok(crate::caller::session_unbound())),
    );
}

/// **One file**: a heading, a statement of what it renders, then the texts.
fn file(title: &str, covers: &str) -> Doc {
    let mut doc = Doc::new();
    doc.heading(1, title);
    doc.prose(&format!(
        "{covers}\n\nWritten by `every_text_a_model_reads_matches_the_snapshot` in \
         `crates/jojobot-mcp/src/snapshot.rs` and compared with it at every `make check`. A commit \
         that changes one of these texts updates this file in the same commit: `make snapshot` \
         rewrites it. What the whole snapshot cannot reach is written at the top of that source \
         file."
    ));
    doc
}

/// **Every file of the snapshot**, by name. Rendered from the production code,
/// one surface to a file, so two commits changing two surfaces touch two files.
async fn rendered() -> Vec<(&'static str, String)> {
    let mut instructions_file = file(
        "Server instructions",
        "The instructions the server sends a client when it connects, with the keys that make a \
         thing fall due built from the shipped carriers.",
    );
    instructions_file.text(
        2,
        "Instructions sent at connection",
        "text",
        &sentence_per_line(&instructions()),
    );

    let mut tools_file = file(
        "Tools",
        "Every tool the router serves: its description, the structure of its arguments, and the \
         description of each argument at every depth.",
    );
    tools(&mut tools_file);

    let mut orientation_file = file(
        "Orientation",
        "The orientation essay as the boot serves it (the core, then each section of the rest) \
         and every sentence in the answer to an anonymous boot and to a named boot of the shipped \
         identity.",
    );
    orientation(&mut orientation_file).await;

    let mut skills_file = file(
        "Skills",
        "Every skill the build ships: the index line a boot lists and the body a call fetches.",
    );
    skills(&mut skills_file);

    let mut teachings_file = file(
        "Teachings",
        "Every teaching that rides once per session on the verb it is about.",
    );
    teachings(&mut teachings_file);

    let mut refusals_file = file(
        "Refusals",
        "The refusals the three error lanes raise, one row per error kind, and the refusal of a \
         status outside the vocabulary rendered from a live write. Refusals written inline at \
         the verb that raises them are not here.",
    );
    refusals(&mut refusals_file).await;

    vec![
        ("instructions.md", instructions_file.out),
        ("tools.md", tools_file.out),
        ("orientation.md", orientation_file.out),
        ("skills.md", skills_file.out),
        ("teachings.md", teachings_file.out),
        ("refusals.md", refusals_file.out),
    ]
}

/// **Where two texts first differ**, as a line number and the two lines, so a
/// failure names a place rather than printing two whole files.
fn first_difference(held: &str, now: &str) -> String {
    for (n, (a, b)) in held.lines().zip(now.lines()).enumerate() {
        if a != b {
            return format!("line {}:\n- checked in: {a}\n- rendered: {b}", n + 1);
        }
    }
    format!(
        "the files agree for {} lines, then one is longer ({} lines checked in, {} rendered)",
        held.lines().count().min(now.lines().count()),
        held.lines().count(),
        now.lines().count()
    )
}

/// **The snapshot holds every text a model reads, and a change to one fails
/// until the snapshot is updated with it.** The directory holds exactly the files
/// rendered: one left over from a surface that no longer exists is a difference
/// too.
#[tokio::test]
async fn every_text_a_model_reads_matches_the_snapshot() {
    let now = rendered().await;
    let directory = snapshot_directory();
    if std::env::var(UPDATE_VARIABLE).is_ok_and(|v| !v.is_empty() && v != "0") {
        std::fs::create_dir_all(&directory).expect("the snapshot directory is writable");
        for stale in held_files(&directory) {
            if !now.iter().any(|(name, _)| *name == stale) {
                std::fs::remove_file(directory.join(&stale)).expect("a stale file goes");
            }
        }
        for (name, text) in &now {
            std::fs::write(directory.join(name), text).expect("the snapshot is writable");
        }
        return;
    }
    let mut changed = Vec::new();
    for (name, text) in &now {
        match std::fs::read_to_string(directory.join(name)) {
            Ok(held) if held == *text => {}
            Ok(held) => changed.push(format!(
                "{name}: first difference at {}",
                first_difference(&held, text)
            )),
            Err(_) => changed.push(format!("{name}: there is no such file")),
        }
    }
    for stale in held_files(&directory) {
        if !now.iter().any(|(name, _)| *name == stale) {
            changed.push(format!("{stale}: no surface renders it any more"));
        }
    }
    assert!(
        changed.is_empty(),
        "a text a model reads changed.\n\n{}\n\nIf the change is meant, update the snapshot with \
         {UPDATE_VARIABLE}=1 (make snapshot) and commit the files with it.",
        changed.join("\n\n")
    );
}

/// The names of the files the snapshot directory holds now.
fn held_files(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// **The file is the same on every render**: no clock, no path, no drawn id and
/// no iteration order of a hash map reaches it.
#[tokio::test]
async fn the_snapshot_renders_the_same_every_time() {
    let (first, second) = (rendered().await, rendered().await);
    for ((name, a), (_, b)) in first.iter().zip(&second) {
        assert!(
            a == b,
            "{name} is not the same on two renders: {}",
            first_difference(a, b)
        );
    }
}

/// **The most characters of a tool description, or of the server instructions,
/// that reach a model.** Clients cut a description at about this length, so a
/// rule written past it is a rule no model reads. Whatever a call must be told
/// goes before this; detail goes in the argument descriptions, which arrive
/// whole, or in `start_here`.
const READ_IN_FULL: usize = 2_000;

/// **No tool description and no instructions text passes the length clients
/// cut at.** Every text over it is listed with its length, so one failure names
/// all of them rather than the first.
#[test]
fn no_description_passes_the_length_clients_cut_at() {
    let mut over: Vec<(String, usize)> = Jojobot::tool_router()
        .list_all()
        .into_iter()
        .filter_map(|tool| {
            let length = tool.description.as_deref().map_or(0, |d| d.chars().count());
            (length > READ_IN_FULL).then(|| (format!("tool `{}`", tool.name), length))
        })
        .collect();
    let length = instructions().chars().count();
    if length > READ_IN_FULL {
        over.push(("the server instructions".to_string(), length));
    }
    over.sort();
    assert!(
        over.is_empty(),
        "a text passes the {READ_IN_FULL} characters clients read before they cut it. Move its \
         detail into the argument descriptions or start_here, and keep the rules that govern \
         the call in the first {READ_IN_FULL}:\n{}",
        over.iter()
            .map(|(what, length)| format!("{what}: {length} characters"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// **Wherever a writer is told a handle in text becomes a link, the same
/// description says code is literal** (rule 450). A writer who is taught the
/// link and not its exception shows a handle in backticks and finds it resolved,
/// or refused when it names nothing. The verbs are listed, so a verb that loses
/// the link sentence is a failure here rather than a quiet pass.
#[test]
fn every_argument_that_teaches_a_link_in_text_says_code_is_literal() {
    fn descriptions(node: &serde_json::Value, found: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(map) => {
                if let Some(text) = map.get("description").and_then(|d| d.as_str()) {
                    found.push(text.to_string());
                }
                map.values().for_each(|child| descriptions(child, found));
            }
            serde_json::Value::Array(items) => {
                items.iter().for_each(|child| descriptions(child, found))
            }
            _ => {}
        }
    }
    let taught_on = [
        "capture",
        "update_fact",
        "set_charter",
        "journal",
        "amend_journal",
        "wrap_session",
        "post_message",
        "mark_processed",
    ];
    let tools = Jojobot::tool_router().list_all();
    for verb in taught_on {
        let tool = tools
            .iter()
            .find(|tool| tool.name == verb)
            .unwrap_or_else(|| panic!("the surface serves {verb}"));
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let mut all = Vec::new();
        descriptions(&schema, &mut all);
        let teaching: Vec<&String> = all
            .iter()
            .filter(|text| text.contains("to link to something that already exists"))
            .collect();
        assert!(
            !teaching.is_empty(),
            "{verb} no longer teaches that a handle in text is a link, so this pin has gone \
             blind: {all:?}"
        );
        for text in teaching {
            assert!(
                text.contains("literal"),
                "{verb} teaches the link and not that code is literal: {text}"
            );
        }
    }
}

/// **A teaching added to `teaching.rs` is in the snapshot.** The list above is
/// written by hand, so this reads the source for every teaching constant and
/// holds the list to it.
#[test]
fn every_teaching_constant_is_in_the_snapshot() {
    let source = include_str!("teaching.rs");
    let listed = include_str!("snapshot.rs");
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("pub(crate) const ")
            && let Some((name, _)) = rest.split_once(": &str")
            && name.ends_with("_TEACHING")
        {
            assert!(
                listed.contains(&format!("(\"{name}\", {name})")),
                "{name} is a teaching and is not in the snapshot's list"
            );
        }
    }
}

/// **The Makefile rewrites the file through the variable, by the test's own
/// name.** A rename of the test or of the variable breaks the target without a
/// word, so the target is held to both.
#[test]
fn the_makefile_updates_the_snapshot_through_the_variable() {
    let makefile =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile"))
            .expect("the Makefile");
    let recipe: Vec<&str> = makefile
        .lines()
        .skip_while(|line| !line.starts_with("snapshot:"))
        .skip(1)
        .take_while(|line| line.starts_with('\t'))
        .collect();
    assert!(!recipe.is_empty(), "the Makefile has no snapshot target");
    let recipe = recipe.join("\n");
    assert!(recipe.contains(&format!("{UPDATE_VARIABLE}=1")), "{recipe}");
    assert!(
        recipe.contains("every_text_a_model_reads_matches_the_snapshot"),
        "{recipe}"
    );
}

/// **The text of one tool argument, or of the tool itself when `argument` is
/// `None`**, as a model reads it.
fn served_text(verb: &str, argument: Option<&str>) -> String {
    let tools = Jojobot::tool_router().list_all();
    let tool = tools
        .iter()
        .find(|tool| tool.name == verb)
        .unwrap_or_else(|| panic!("the surface serves {verb}"));
    match argument {
        None => tool
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("{verb} has a description"))
            .to_string(),
        Some(name) => {
            let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
            schema["properties"][name]["description"]
                .as_str()
                .unwrap_or_else(|| panic!("{verb} serves a description for {name}"))
                .to_string()
        }
    }
}

/// **Every reason the guard asks a caller for a reason on is named in the
/// `override_reason` argument of each verb that screens a name** (rule 498).
/// The list a model reads is the one `needs_a_reason` holds, so a reason added
/// there without a word in the argument leaves a model unable to know the
/// refusal wants one. The match has no wildcard: a new variant stops compiling
/// here until it is placed.
#[test]
fn override_reason_names_every_reason_the_guard_asks_one_for() {
    use jojobot_domain::memory::guard::MatchReason as R;
    let every = [
        R::ExactHandle,
        R::SelfParent,
        R::SameName,
        R::SameNameOtherKind,
        R::NearSlug,
        R::Contains,
        R::NearName,
        R::HeldNumber,
        R::Nickname,
        R::SharedWord,
    ];
    // The word the argument says each reason with. `None` for a reason that
    // keeps the token alone.
    let said_as = |reason: R| match reason {
        R::SameNameOtherKind => Some("another kind"),
        R::Contains => Some("sits inside"),
        R::Nickname => Some("nickname"),
        R::SharedWord => Some("rare words"),
        R::ExactHandle
        | R::SelfParent
        | R::SameName
        | R::NearSlug
        | R::NearName
        | R::HeldNumber => None,
    };
    let mut asked = 0;
    for reason in every {
        assert_eq!(
            said_as(reason).is_some(),
            reason.needs_a_reason(),
            "{reason:?}: the words and the guard disagree about whether it takes a reason"
        );
        let Some(word) = said_as(reason) else {
            continue;
        };
        asked += 1;
        for verb in ["add_entity", "rename_entity", "update_entity"] {
            let text = served_text(verb, Some("override_reason"));
            assert!(
                text.contains(word),
                "{verb} override_reason does not name {reason:?} (\"{word}\"): {text}"
            );
        }
    }
    assert!(
        asked > 0,
        "no reason asks for a reason, so this pin is blind"
    );
}

/// **`merge_entities` lists the refusal for a thing folded into its own
/// descendant**, beside the other refusals a merge can meet.
#[test]
fn merge_entities_lists_the_refusal_for_a_fold_into_a_descendant() {
    let text = served_text("merge_entities", None);
    assert!(text.contains("descendant"), "{text}");
}

/// **`merge_entities` lists the refusal for a number the merge would lose**,
/// beside the other refusals a merge can meet. The word is the one the list
/// adds, so a description that loses the item fails here.
#[test]
fn merge_entities_lists_the_refusal_for_a_number_the_merge_would_lose() {
    let text = served_text("merge_entities", None);
    assert!(text.contains("lose"), "{text}");
}

/// **`start_here`'s `compacted` says a `skill` or a `section` beside it is
/// refused**, which is what the verb does.
#[test]
fn start_here_compacted_says_it_is_refused_beside_a_skill_or_a_section() {
    let text = served_text("start_here", Some("compacted"));
    for fetch in ["`skill`", "`section`"] {
        assert!(text.contains(fetch), "{fetch} is not named: {text}");
    }
}

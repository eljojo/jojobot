//! **The tests that watch the surface as a whole**, and the shipped source
//! behind it.
//!
//! They belong to the crate root because that is where the router is assembled:
//! each one reads the WHOLE surface — every tool a client can reach, every
//! description a caller reads — rather than any one verb's behaviour. A verb's
//! own tests live in the verb's own file.
//!
//! Test-only, and declared by `lib.rs`.

use super::*;

/// Every shipped `.rs` file in this crate, named, with its test half cut off.
///
/// The constraints below are about what SHIPS, and the way they are asserted
/// is by counting occurrences in the source — so what counts as "the source"
/// is load-bearing. This must count every shipped file, not just `lib.rs`:
/// counting one file only would silently stop covering new ones, letting the
/// test pass while watching a fraction of the crate.
///
/// Two halves are cut, and they are cut differently.
///
/// * A file's own `#[cfg(test)]` module: everything from the first one on is
///   scaffolding, and the tests below deliberately construct the things the
///   constraints forbid.
/// * **A file that is test-only IN ITS ENTIRETY.** `mailbox/testing.rs` carries
///   no marker of its own — the gate is the `#[cfg(test)] mod testing;` on its
///   PARENT — so nothing inside it says it is not shipped code. Those are found
///   by reading the declarations that gate them, never by a list somebody has to
///   remember to update: a list is how this goes stale, and going stale here is
///   invisible.
fn shipped_files() -> Vec<(String, String)> {
    fn walk(
        dir: &std::path::Path,
        gated: &mut Vec<std::path::PathBuf>,
        out: &mut Vec<(String, String)>,
    ) {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .expect("the crate's own src is readable")
            .map(|e| e.expect("a directory entry").path())
            .collect();
        // **Files before directories, and it is not cosmetic.** A parent module
        // living beside its directory (`mailbox.rs` next to `mailbox/`) is what
        // declares which of its children are test-only, so it has to be read
        // before they are walked or its gates are learned too late to apply.
        entries.sort_by_key(|p| (p.is_dir(), p.clone()));
        for path in entries {
            if path.is_dir() {
                walk(&path, gated, out);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            let here = path.parent().expect("a file has a parent");
            let stem = path.file_stem().expect("a .rs file has a stem");
            gated.extend(test_only_children(here, &text));
            gated.extend(test_only_children(&here.join(stem), &text));
            if gated.contains(&path) {
                continue;
            }
            out.push((
                path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
                shipped_half(&text),
            ));
        }
    }

    /// The files this one's `#[cfg(test)] mod NAME;` declarations gate, in both
    /// spellings Rust resolves a module to.
    fn test_only_children(dir: &std::path::Path, text: &str) -> Vec<std::path::PathBuf> {
        let mut gated = Vec::new();
        let mut lines = text.lines().peekable();
        while let Some(line) = lines.next() {
            if !line.trim_start().starts_with("#[cfg(test)]") {
                continue;
            }
            let Some(name) = lines.peek().and_then(|next| {
                next.trim()
                    .strip_suffix(';')
                    .and_then(|d| d.rsplit_once("mod "))
                    .map(|(_, name)| name.to_string())
            }) else {
                continue;
            };
            gated.push(dir.join(format!("{name}.rs")));
            gated.push(dir.join(&name).join("mod.rs"));
        }
        gated
    }

    let mut files = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut Vec::new(),
        &mut files,
    );
    assert!(
        !files.is_empty(),
        "the walk found no shipped source at all, which is a broken test rather than a clean crate"
    );
    files
}

/// A file's shipped half: the file with its test items taken out.
///
/// **It SKIPS a test item rather than cutting the file at one, and the
/// difference was 253 lines of `memory/search.rs`** — measured, not estimated:
/// that file went from 279 shipped lines to 532, and the sweeps built on this
/// walker gained six sentences they had never read. `#[cfg(test)]` wears three
/// meanings here and only one of them is a wall:
///
/// * `#[cfg(test)] mod tests {` opens scaffolding — skipped, with the module.
/// * `#[cfg(test)] fn helper()` is scaffolding in the MIDDLE of a file, with
///   shipped code after it. Cutting there took the rest of the file with it.
/// * `#[cfg(test)] mod surface;` merely gates a child file and can sit at the
///   very top, above everything a scan exists to count. It is kept: the walk
///   excludes the file it gates, and cutting at it once reduced `lib.rs` to its
///   imports.
///
/// **An item ends at the line that closes it at its own indentation**, which is
/// a fact about formatted Rust rather than a guess: `cargo fmt --check` is part
/// of this repository's bar, so an item that closed anywhere else would fail
/// the build before it reached here. A braceless item — `#[cfg(test)] use x;` —
/// ends at its own line.
fn shipped_half(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if !line.trim_start().starts_with("#[cfg(test)]") {
            kept.push(line);
            at += 1;
            continue;
        }
        let Some(item) = lines.get(at + 1) else {
            break;
        };
        // The child-file gate, kept: what it gates is dropped by the walk, and
        // the code around it ships.
        if item.trim().ends_with(';') && item.trim().contains("mod ") {
            kept.push(line);
            at += 1;
            continue;
        }
        // A braceless item is one line of scaffolding.
        if !item.contains('{') {
            at += 2;
            continue;
        }
        let indent = &line[..line.len() - line.trim_start().len()];
        let closes = format!("{indent}}}");
        at += 1;
        while at < lines.len() && lines[at] != closes {
            at += 1;
        }
        at += 1;
    }
    kept.join("\n")
}

/// **The walker reads to the end of a file, past a test item and out the other
/// side.**
///
/// A walker that CUT a file at the first `#[cfg(test)]` — counting an
/// attribute on a plain function as one — reads `memory/search.rs` to line 280
/// of 1767, and every sweep built on it is blind past that point. **Nothing is
/// wrong with those sweeps; they read a fraction of the crate and say so
/// nowhere.**
///
/// **The fixture is the shape that caused it**: a test-annotated function in
/// the middle of a file with shipped code after it. Both ends are asserted —
/// the prose past the test item is IN and the prose inside it is OUT — because
/// a walker returning the whole file, scaffolding included, would pass the
/// first half alone.
#[test]
fn the_walker_reads_past_a_test_item_and_not_into_one() {
    let source = concat!(
        "const A: &str = \"a sentence in the head of the file\";\n",
        "\n",
        "#[cfg(test)]\n",
        "fn helper() -> &'static str {\n",
        "    \"a sentence only the tests see\"\n",
        "}\n",
        "\n",
        "const B: &str = \"a sentence past the test helper\";\n",
        "\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    const C: &str = \"a sentence inside the test module\";\n",
        "}\n",
        "\n",
        "const D: &str = \"the far end of the file\";\n",
    );

    let shipped = shipped_half(source);

    assert!(
        shipped.contains("in the head of the file"),
        "the head of the file is shipped code: {shipped}"
    );
    assert!(
        shipped.contains("past the test helper"),
        "…and so is everything after a test item, which is the whole defect: {shipped}"
    );
    assert!(
        shipped.contains("the far end of the file"),
        "…including the far end, past an inline test module: {shipped}"
    );
    assert!(
        !shipped.contains("only the tests see"),
        "a test-annotated function is scaffolding and must not be counted: {shipped}"
    );
    assert!(
        !shipped.contains("inside the test module"),
        "…and neither is an inline test module: {shipped}"
    );
}

/// The same shipped source, as one text, for the checks that count occurrences
/// across the crate rather than asking which file carried one.
fn shipped_source() -> String {
    shipped_files()
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<String>>()
        .concat()
}

/// **Every kind the store accepts is a kind the surface lists.**
///
/// A kind the store accepts and the surface does not list is a kind a caller
/// reads as one an entity cannot be given. For `bot` that is the whole of how
/// an identity is made, because nothing about a bot is compiled in.
///
/// **The list lives in ONE place**, the `kind` argument, which is what makes
/// this checkable at all: a second copy in the verb description is how the
/// first one goes stale unnoticed. This pins the
/// surviving one against the enum, so a kind added to the store cannot be
/// missing from the only place a caller is told about it.
///
/// It pins tokens rather than phrasing — a rewrite of the sentence around them
/// is free, and dropping a kind out of it is not.
#[test]
fn every_kind_the_store_accepts_is_listed_where_a_caller_reads() {
    use jojobot_domain::memory::EntityKind;

    let tools = Jojobot::tool_router().list_all();
    let add_entity = tools
        .iter()
        .find(|t| t.name.as_ref() == "add_entity")
        .expect("the surface offers add_entity");
    // **The argument's own description, read out of the schema a client
    // receives** — not the whole schema as one string, where a token could be
    // satisfied by any other argument's prose.
    let schema = serde_json::to_value(&add_entity.input_schema).expect("the schema serializes");
    // **The opening paragraph, which is where the list is** — not the whole
    // description, where a token is satisfied by any sentence that happens to
    // mention it. That distinction is not academic: the prose below the list
    // says the word `bot` twice, so a check over the whole text passes with the
    // kind missing from the list a caller actually reads.
    let described = schema["properties"]["kind"]["description"]
        .as_str()
        .expect("the kind argument carries its own description")
        .to_string();
    let listed = described
        .split("\n\n")
        .next()
        .expect("a description has a first paragraph")
        .to_string();

    // ⛔️ **`session` is deliberately not offered.** A session is not created
    // through `add_entity` — it begins when a bot boots and its life is held by
    // the session verbs — so listing it would name a kind this argument cannot
    // make. It is addressable for READING, which is a different question from
    // what a caller may create.
    for kind in EntityKind::ALL
        .into_iter()
        .filter(|kind| *kind != EntityKind::SESSION)
    {
        let token = kind.as_token();
        assert!(
            listed.contains(&format!("`{token}`")),
            "the `kind` argument does not list `{token}`, which the store accepts: {listed}"
        );
    }
}

/// **`capture`'s `check_in` argument names the day a check-in is dated by, and
/// where a snooze names its own.** There is no `date` argument on capture: the
/// check-in's day is `recorded_at`, and an argument that named one that does not
/// exist sent a caller looking for it. The paragraph on the three outcomes says
/// what a snooze lasts until, in the key that carries it.
///
/// It pins identifiers rather than phrasing, and reads the argument's own
/// description out of the schema a client receives.
#[test]
fn the_check_in_argument_names_the_day_it_is_dated_by_and_the_snooze_day() {
    let tools = Jojobot::tool_router().list_all();
    let capture = tools
        .iter()
        .find(|t| t.name.as_ref() == "capture")
        .expect("the surface offers capture");
    let schema = serde_json::to_value(&capture.input_schema).expect("the schema serializes");
    assert!(
        schema["properties"]["recorded_at"].is_object(),
        "the day a check-in is dated by is a real argument of capture: {schema}"
    );
    assert!(
        schema["properties"]["date"].is_null(),
        "capture has no `date` argument, so the description must not name one"
    );
    let described = schema["properties"]["check_in"]["description"]
        .as_str()
        .expect("the check_in argument carries its own description");
    assert!(
        described.contains("`recorded_at`"),
        "the check-in's day is `recorded_at`, and the argument does not say so: {described}"
    );
    assert!(
        !described.contains("`date`"),
        "the argument names a `date` that capture does not take: {described}"
    );
    let on_the_outcomes = described
        .split("\n\n")
        .find(|paragraph| paragraph.contains("`snoozed`") && paragraph.contains("consume"))
        .expect("a paragraph describes the three outcomes");
    assert!(
        on_the_outcomes.contains("snoozed_until"),
        "the paragraph that says what a snooze does names the key carrying its day: \
         {on_the_outcomes}"
    );
}

/// **`recall`'s own description names the snooze answer and its `in_force`.** A
/// capability only a reader of the diff knows about has no path.
#[test]
fn the_recall_description_names_the_snooze_answer() {
    let tools = Jojobot::tool_router().list_all();
    let recall = tools
        .iter()
        .find(|t| t.name.as_ref() == "recall")
        .expect("the surface offers recall");
    let described = recall.description.as_deref().unwrap_or_default();
    assert!(
        described.contains("in_force") && described.contains("`snooze`"),
        "the recall description does not name the snooze answer: {described}"
    );
}

/// **The `overdue` argument tells a caller not to read a write's success off
/// this filter.** A model that just edited something and checked it against
/// this list, rather than reading the record back, took a coincidence as
/// confirmation — the failure this sentence exists to head off, at the one
/// place a caller stands right before making that mistake.
#[test]
fn the_overdue_argument_says_absence_is_not_a_writes_confirmation() {
    let tools = Jojobot::tool_router().list_all();
    let recall = tools
        .iter()
        .find(|t| t.name.as_ref() == "recall")
        .expect("the surface offers recall");
    let schema = serde_json::to_value(&recall.input_schema).expect("the schema serializes");
    let described = schema["properties"]["overdue"]["description"]
        .as_str()
        .expect("the overdue argument carries its own description");
    assert!(
        described.contains("not proof a write landed"),
        "the overdue argument does not say that leaving this list is not the same claim as a \
         write taking effect: {described}"
    );
}

/// **The `@kind:slug` mention spelling reaches a caller where they are
/// already standing, not only in the domain crate that resolves it.**
///
/// A year-long paid run wrote twenty-nine handle-shaped mentions and not one
/// carried the `@`: nothing an agent reads before writing named the syntax,
/// so a bare `person:milhouse` is a string that resembles a pointer and is
/// not one. This pins that every argument which actually resolves a mention
/// teaches the spelling, and that the door's own orientation shows it worked.
///
/// **Cites rather than restates.** The full mechanism — the badge, the
/// rename, what renders when a link is broken — is written once in
/// `jojobot_domain::memory::mention` (rule 51); a second explanation here
/// would be the second copy that drifts.
#[test]
fn the_mention_spelling_is_taught_on_every_argument_that_resolves_one() {
    let tools = Jojobot::tool_router().list_all();
    for (tool, field) in [
        ("capture", "content"),
        ("capture", "details"),
        ("update_fact", "content"),
        ("update_fact", "details"),
        ("set_charter", "prose"),
        ("journal", "entry"),
        ("journal", "focus"),
        ("amend_journal", "entry"),
        ("wrap_session", "story"),
        ("post_message", "body"),
        ("post_message", "subject"),
        ("mark_processed", "notes"),
    ] {
        let found = tools
            .iter()
            .find(|t| t.name.as_ref() == tool)
            .unwrap_or_else(|| panic!("the surface offers {tool}"));
        let schema = serde_json::to_value(&found.input_schema).expect("the schema serializes");
        let described = schema["properties"][field]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{tool}'s {field} argument carries its own description"));
        assert!(
            described.contains("@kind:slug"),
            "{tool}'s {field} argument does not teach the @kind:slug mention spelling: {described}",
        );
    }

    assert!(
        crate::orientation::essay::orientation().contains("@person:milhouse and @person:nelson"),
        "the orientation essay's worked example for writing a mention is missing",
    );
}

/// **`search`'s `clock` argument names the fallback its own answer reports.**
/// A caller choosing between `recorded_on` and `happened_at` needs to know
/// up front that a claim missing the one it asked for still ranks — by the
/// other — rather than discovering `rank_fallbacks` in the answer with
/// nothing in the argument's own text pointing at what it counts.
#[test]
fn the_search_clock_argument_names_the_ranking_fallback() {
    let tools = Jojobot::tool_router().list_all();
    let search = tools
        .iter()
        .find(|t| t.name.as_ref() == "search")
        .expect("the surface offers search");
    let schema = serde_json::to_value(&search.input_schema).expect("the schema serializes");
    let described = schema["properties"]["clock"]["description"]
        .as_str()
        .expect("the clock argument carries its own description");
    assert!(
        described.contains("rank_fallbacks"),
        "the clock argument does not name the field its own fallback is counted in: {described}"
    );
}

/// **The `claim` argument names all three outcomes a claim can come back
/// as** — `taken`, `refused` and `conflict` — so a caller reading the
/// argument before ever making the call already knows a collision is a
/// real, named outcome and not a surprise the answer alone explains.
#[test]
fn the_claim_argument_names_its_three_outcomes() {
    let tools = Jojobot::tool_router().list_all();
    let start_here = tools
        .iter()
        .find(|t| t.name.as_ref() == "start_here")
        .expect("the surface offers start_here");
    let schema = serde_json::to_value(&start_here.input_schema).expect("the schema serializes");
    let described = schema["properties"]["claim"]["description"]
        .as_str()
        .expect("the claim argument carries its own description");
    for outcome in ["`taken`", "`refused`", "`conflict`"] {
        assert!(
            described.contains(outcome),
            "the claim argument does not name the {outcome} outcome: {described}"
        );
    }
    assert!(
        described.to_lowercase().contains("retry"),
        "the claim argument does not say a conflict is answered by retrying: {described}"
    );
}

/// **The `claim` argument names how a lease is kept and given up**: every
/// write renews it, not only a journal beat, the lease is 45 minutes, and
/// wrapping releases it. A caller deciding whether it is safe to stay quiet
/// between beats, or whether wrapping costs it the role, needs this in the
/// argument it is reading before it acts — not only in this crate's own
/// engineering notes.
#[test]
fn the_claim_argument_names_how_the_lease_is_kept_and_given_up() {
    let tools = Jojobot::tool_router().list_all();
    let start_here = tools
        .iter()
        .find(|t| t.name.as_ref() == "start_here")
        .expect("the surface offers start_here");
    let schema = serde_json::to_value(&start_here.input_schema).expect("the schema serializes");
    let described = schema["properties"]["claim"]["description"]
        .as_str()
        .expect("the claim argument carries its own description");
    assert!(
        described.contains("Every write"),
        "the claim argument does not say every write renews the lease: {described}"
    );
    assert!(
        described.contains("45 minutes"),
        "the claim argument does not name the lease's own length: {described}"
    );
    assert!(
        described.to_lowercase().contains("wrapping releases"),
        "the claim argument does not say wrapping releases the claim: {described}"
    );
}

/// **The `claim` argument names the key a bot carries its role in** and says a
/// different identity writes it. A boot that names no claim claims that role
/// for a bot that carries one, so an argument that called naming none "the
/// ordinary boot, unchanged" would tell a caller the opposite of what the
/// door does.
#[test]
fn the_claim_argument_names_the_role_a_bot_carries() {
    let tools = Jojobot::tool_router().list_all();
    let start_here = tools
        .iter()
        .find(|t| t.name.as_ref() == "start_here")
        .expect("the surface offers start_here");
    let schema = serde_json::to_value(&start_here.input_schema).expect("the schema serializes");
    let described = schema["properties"]["claim"]["description"]
        .as_str()
        .expect("the claim argument carries its own description");
    assert!(
        described.contains("claims_role"),
        "the claim argument does not name the key a bot carries its role in: {described}"
    );
    assert!(
        described.contains("different identity"),
        "the claim argument does not say a different identity writes that key: {described}"
    );
}

/// **`journal`'s own description says a beat renews a held role claim** —
/// the one capability this verb has beside recording a beat, and the one a
/// caller relying on journalling alone to keep a lease fresh needs to read
/// here rather than infer.
#[test]
fn the_journal_description_states_it_renews_a_held_role_claim() {
    let tools = Jojobot::tool_router().list_all();
    let journal = tools
        .iter()
        .find(|t| t.name.as_ref() == "journal")
        .expect("the surface offers journal");
    let description = journal.description.as_deref().unwrap_or_default();
    assert!(
        description.to_lowercase().contains("renews"),
        "journal's description does not say a beat renews a held role claim: {description}"
    );
}

/// **The orientation essay explains claiming a role**, in the Sessions
/// section where it already explains what a session is — the same place a
/// caller reads to understand `start_here`'s `claim` argument, so the
/// explanation is not left to the argument's own text alone.
#[test]
fn the_orientation_essay_explains_claiming_a_role() {
    let essay = crate::orientation::essay::orientation();
    assert!(
        essay.contains("Claim a role"),
        "the orientation essay does not explain claiming a role"
    );
    assert!(
        essay.contains("45 minutes"),
        "the orientation essay does not name the lease's own length"
    );
}

/// **`capture`'s thought-room mechanics are documented on the arguments
/// that actually work it** — a connection edge on the caller's own handle,
/// `drop` paired with a required `drop_because`, `borrow` as the emergency
/// reserve, and `starred` as a boot's own seat — so a caller reads the
/// whole mechanism from the schema it already has, never only from this
/// crate's internal comments.
#[test]
fn capture_documents_its_thought_room_mechanics() {
    let tools = Jojobot::tool_router().list_all();
    let capture = tools
        .iter()
        .find(|t| t.name.as_ref() == "capture")
        .expect("the surface offers capture");
    let schema = serde_json::to_value(&capture.input_schema).expect("the schema serializes");
    let described = |field: &str| -> String {
        schema["properties"][field]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("capture's {field} argument carries its own description"))
            .to_string()
    };
    let shape = described("shape");
    assert!(
        shape.contains("thought_capacity") && shape.to_lowercase().contains("own bot handle"),
        "the shape argument does not name a connection edge on the caller's own handle as what \
         makes a thought: {shape}"
    );
    assert!(
        shape.contains("drop"),
        "the shape argument does not point a caller at drop when the room is full: {shape}"
    );
    let drop_because = described("drop_because");
    assert!(
        drop_because.contains("Required whenever"),
        "drop_because does not say it is required whenever drop is: {drop_because}"
    );
    let borrow = described("borrow");
    assert!(
        borrow.to_lowercase().contains("over capacity"),
        "borrow does not name the emergency-reserve shape: {borrow}"
    );
    let fields = described("fields");
    assert!(
        fields.contains("starred"),
        "fields does not name the starred convention: {fields}"
    );
}

/// The identifiers a caller must find in `verb`'s own description to know a
/// capability exists. An agent chooses a verb from that text before it reads
/// a schema, so a capability documented only on an argument is one it never
/// reaches for. The words around the identifiers are free to change.
fn assert_description_names(verb: &str, names: &[&str]) {
    let tools = Jojobot::tool_router().list_all();
    let description = tools
        .iter()
        .find(|t| t.name.as_ref() == verb)
        .unwrap_or_else(|| panic!("the surface offers {verb}"))
        .description
        .as_deref()
        .unwrap_or_default()
        .to_string();
    for name in names {
        assert!(
            description.contains(name),
            "{verb}'s description does not name `{name}`: {description}"
        );
    }
}

/// **`search`'s description names the `clock` choice** and the two days it
/// picks between.
#[test]
fn the_search_description_names_its_clock() {
    assert_description_names("search", &["`clock`", "recorded_on", "happened_at"]);
}

/// **`recall`'s description names the kind that reads a bot's own past runs**
/// and what `withheld` says about the runs it does not show.
#[test]
fn the_recall_description_names_session_runs_and_withheld() {
    assert_description_names("recall", &["`session`", "`withheld`"]);
}

/// **`recall`'s description names the `room` block and when it is sent.**
#[test]
fn the_recall_description_names_the_room_block_and_when_it_appears() {
    assert_description_names("recall", &["`room`", "aged_out"]);
}

/// **`capture`'s own description names `starred`** — the mark that spends one of
/// a bot's few boot seats on a rule — so a caller reads it before the schema.
#[test]
fn the_capture_description_names_starred() {
    assert_description_names("capture", &["`starred`"]);
}

/// **The verbs whose answers carry `written_by_other_run` name it**, since the
/// key is absent on a message the reader's own run posted and an agent that was
/// never told it exists reads the absence as nothing.
#[test]
fn the_mailbox_verbs_name_written_by_other_run() {
    for verb in ["read_mailbox", "read_message", "post_message"] {
        assert_description_names(verb, &["written_by_other_run"]);
    }
}

/// **`wrap_session` names `closing_focus`, and `recall` points at it.** A run's
/// closing focus rides the closing entry beside the story and never inside its
/// text, and a read of the run's prose does not carry it.
#[test]
fn wrap_session_names_closing_focus_and_recall_points_at_it() {
    assert_description_names("wrap_session", &["closing_focus"]);
    assert_description_names("recall", &["wrap_session", "closing_focus"]);
}

/// **`recall`'s description says an empty `answers_type` still names the
/// type's keys**, in the answer's own key for them.
#[test]
fn the_recall_description_names_type_keys() {
    assert_description_names("recall", &["type_keys"]);
}

/// **`merge_entities` names the refusal a merge into your own bot can meet**,
/// and the two ways past it.
#[test]
fn the_merge_description_names_its_ceiling_refusal() {
    assert_description_names(
        "merge_entities",
        &["thought_capacity", "thought_body_cap", "clear_fields"],
    );
}

/// **`archive_entity` says what it is not, and what it does to an owed read.**
///
/// A model that finds a duplicate reaches for the verb whose description says
/// "a mistaken write", and archiving leaves the duplicate's claims where they
/// are. The description names the verb that carries them, and says an archived
/// loop leaves the owed read, which `an_archived_loop_drops_out_of_the_owed_read_and_is_counted`
/// holds.
#[test]
fn the_archive_description_names_the_duplicate_repair_and_the_owed_read() {
    assert_description_names("archive_entity", &["merge_entities", "overdue"]);
}

/// **`merge_entities` says its names become aliases of the survivor.**
#[test]
fn the_merge_description_names_the_aliases_it_carries() {
    assert_description_names("merge_entities", &["aliases"]);
}

/// **`update_fact` says a claim cannot change subject, and how to move one,
/// and that moving a value to another key is one call.**
#[test]
fn the_update_fact_description_names_the_moves_it_cannot_make_in_place() {
    assert_description_names(
        "update_fact",
        &["`derived_from`", "`fields`", "`clear_fields`"],
    );
}

/// **`list_runs` points at the read that returns a run's chronology.**
#[test]
fn the_list_runs_description_points_at_recall_for_a_chronology() {
    assert_description_names("list_runs", &["recall", "kind: session"]);
}

/// **`start_here`'s description names `claim`** and the three outcomes it
/// answers with.
#[test]
fn the_start_here_description_names_claim_and_its_outcomes() {
    assert_description_names(
        "start_here",
        &[
            "`claim`",
            "`taken`",
            "`refused`",
            "`conflict`",
            "`unavailable`",
        ],
    );
}

/// **`capture`'s description names the thought room** — what makes a claim
/// a thought, the ceiling, and the two ways a full room takes one more.
#[test]
fn the_capture_description_names_the_thought_room() {
    assert_description_names(
        "capture",
        &[
            "`thought`",
            "`thought_capacity`",
            "`drop`",
            "`drop_because`",
            "`borrow`",
        ],
    );
}

/// **The whole tool surface, named.** Production jojobot never deletes
/// anything: the standing rule is structural at the store (the Mailboxes
/// port has no delete operation at all), and this pins the other end — that
/// nothing at all reaches a client except these.
///
/// **The exact list, not a filter and a list of forbidden words.** A
/// name-shape filter only sees the tools it thought to look for, and a
/// denylist only catches the wordings somebody guessed: `retire_message`,
/// `archive_box`, `clear_mailbox` all sail past both while doing the thing
/// the rule exists to forbid. Adding a tool here is a line in this list and
/// a reviewer reading it — which is the whole point.
#[test]
fn the_tool_surface_is_exactly_this_list() {
    let tools = Jojobot::tool_router().list_all();
    let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    names.sort_unstable();

    // Sorted, so the list is stable and a diff to it is legible — which
    // means it is NOT grouped by context, and any comment here claiming
    // otherwise would be describing a different list than the one below.
    // The five mailbox verbs in it are list_sent, post_message,
    // read_mailbox, read_message and mark_processed. There are two
    // deliberate absences and they are different kinds of absence: no
    // create_mailbox, because a box is not a thing you make — it opens with
    // the bot that owns it, in `add_entity`, and a bot is the only thing
    // that has one; and no list_mailboxes, RETIRED rather than never-built.
    // Its two surviving jobs are `read_mailbox` with counts_only (your own
    // box's counts and its unreadable report, taking delivery of nothing)
    // and `start_here`'s snapshot (every box on the board by name). The four
    // session verbs are journal, amend_journal, wrap_session and list_runs
    // (there is deliberately no start_session — booting an identity IS
    // starting its session); list_runs is the one read among them, and takes
    // no `bot` argument — the `sid` says whose runs it reads, exactly as it
    // says whose box `read_mailbox` opens; the rest are Memory's.
    assert_eq!(
        names,
        [
            "add_entity",
            "amend_journal",
            "archive_entity",
            "capture",
            "declare_type",
            "journal",
            "list_entities",
            "list_runs",
            "list_sent",
            "mark_processed",
            "merge_entities",
            "ping",
            "post_message",
            "read_mailbox",
            "read_message",
            "recall",
            "rename_entity",
            "retract",
            "search",
            "set_charter",
            "start_here",
            "update_entity",
            "update_fact",
            "wrap_session",
        ],
        "the tool surface changed — if that was deliberate, say so here"
    );
}

/// **Every key a shipped carrier reads is named where a write is described.**
///
/// At the moment a model writes, everything it reads says a key it invents is
/// kept as written. Nothing said which keys make a thing fall due, so a model
/// wrote a key no carrier reads and told the operator the thing would come up.
/// The two write verbs' `fields` descriptions and the served instructions name
/// the keys, and they are built from the shipped carriers, so a carrier added
/// later is named with no edit here.
///
/// ⚠️ **It fails when the corpus comes back empty**, because a sweep over
/// nothing reports success: the keys are counted before they are looked for.
#[test]
fn every_key_a_shipped_carrier_reads_is_named_where_a_write_is_described() {
    // **Read off each carrier's own interface, never through the function the
    // published text is built with**: a key that function dropped would then be
    // missing from the corpus too, and the sweep would agree with the bug.
    let keys: Vec<String> = jojobot_domain::attention::shipped()
        .iter()
        .flat_map(|carrier| carrier.interface().fields)
        .map(|field| field.key)
        .collect();
    assert!(
        keys.len() >= 7,
        "the shipped carriers name {} keys, which is too few to be the carriers — the corpus is \
         not being read: {keys:?}",
        keys.len(),
    );

    let tools = Jojobot::tool_router().list_all();
    let mut places: Vec<(String, String)> =
        vec![("the served instructions".into(), crate::instructions())];
    for verb in ["capture", "update_fact"] {
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == verb)
            .unwrap_or_else(|| panic!("the surface offers {verb}"));
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let described = schema["properties"]["fields"]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{verb}'s fields argument carries a description"));
        places.push((format!("{verb}'s fields argument"), described.to_string()));
    }
    assert_eq!(places.len(), 3, "three places describe a write");
    for (place, text) in &places {
        for key in &keys {
            assert!(
                text.contains(&format!("`{key}`")),
                "{place} does not name `{key}`, a key a shipped carrier reads: {text}",
            );
        }
    }
}

/// **The keys that describe their claim are taught where a write is described,
/// and so is what a handle in a field does.** A model writing a rule or a
/// check-in read that a key it invents is kept and folds onto the thing, and
/// nothing told it six keys stay on the claim, how to read them back, or that a
/// value that is a handle is a link refused when it names nothing.
///
/// The six names are read off the shipped `record-labels` type, never through
/// the function that builds the text, so a seventh is named with no edit here.
///
/// ⚠️ **It fails when the corpus comes back empty**: the keys are counted
/// before they are looked for.
#[test]
fn the_write_descriptions_teach_the_describing_keys_and_the_handle_link() {
    let labels: Vec<String> = crate::seed::shipped_types()
        .into_iter()
        .find(|declared| declared.name == "record-labels")
        .expect("the build ships the record-labels type")
        .fields
        .into_iter()
        .map(|field| field.key)
        .collect();
    assert!(
        labels.len() >= 6,
        "the shipped record-labels type names {} keys, too few to be the six: {labels:?}",
        labels.len(),
    );
    let tools = Jojobot::tool_router().list_all();
    let mut places: Vec<(String, String)> = Vec::new();
    for verb in ["capture", "update_fact"] {
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == verb)
            .unwrap_or_else(|| panic!("the surface offers {verb}"));
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let described = schema["properties"]["fields"]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{verb}'s fields argument carries a description"));
        places.push((format!("{verb}'s fields argument"), described.to_string()));
    }
    assert_eq!(places.len(), 2, "two verbs write fields");
    for (place, text) in &places {
        for key in &labels {
            assert!(
                text.contains(&format!("`{key}`")),
                "{place} does not name `{key}`, a key that describes its claim: {text}",
            );
        }
        for word in ["facts", "scope"] {
            assert!(text.contains(word), "{place} does not say `{word}`: {text}");
        }
    }
    assert!(
        places[0].1.contains("link"),
        "capture's fields argument does not say a handle is a link: {}",
        places[0].1
    );
    let instructions = crate::instructions();
    for word in ["link", "counter"] {
        assert!(
            instructions.contains(word),
            "the served instructions do not say `{word}` where fields are described: \
             {instructions}"
        );
    }
}

/// **A work item or project at its finishing status is not owed, and every place
/// a write is described says so.** The closing sentence of the description says
/// no other key acts on what is owed, and `status` on those two kinds does.
///
/// The kinds and the word are read off the shipped kinds here, not out of the
/// text, so a kind added to the rule is named with no edit.
///
/// ⚠️ **It fails when the corpus comes back empty**: the kinds are counted
/// before they are looked for.
#[test]
fn every_place_a_write_is_described_says_a_finished_piece_of_work_is_not_owed() {
    use jojobot_domain::memory::kinds;
    let finishing: Vec<&str> = kinds::SHIPPED
        .iter()
        .copied()
        .filter(|kind| kinds::holds_columns(kind))
        .collect();
    assert!(
        finishing.len() >= 2,
        "the shipped kinds that finish are {finishing:?}, too few to be work and project"
    );

    let tools = Jojobot::tool_router().list_all();
    let mut places: Vec<(String, String)> =
        vec![("the served instructions".into(), crate::instructions())];
    for verb in ["capture", "update_fact"] {
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == verb)
            .unwrap_or_else(|| panic!("the surface offers {verb}"));
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let described = schema["properties"]["fields"]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{verb}'s fields argument carries a description"));
        places.push((format!("{verb}'s fields argument"), described.to_string()));
    }
    for (place, text) in &places {
        for named in finishing
            .iter()
            .copied()
            .chain([kinds::STATUS, kinds::FINISHED])
        {
            assert!(
                text.contains(&format!("`{named}`")),
                "{place} does not name `{named}` where it says what is owed: {text}",
            );
        }
    }
}

/// **Every served place that teaches the provenance vocabulary teaches all
/// three values.**
///
/// A session acts on what it is told, so text that still says a claim is either
/// the operator's word or a guess does not merely go stale: it teaches a
/// session to file a claim it read out of a system of record as one of the two
/// wrong things.
///
/// **The corpus is the essay and every served description**, and a block
/// counts when it names both of the older values — the vocabulary is what is
/// being taught there, and a passage naming one of them in passing is not.
///
/// ⚠️ **It fails when the corpus comes back empty**, because a sweep over
/// nothing reports success: the count of teaching blocks is asserted before
/// they are read.
#[test]
fn every_served_place_that_teaches_provenance_names_all_three_values() {
    let tools = Jojobot::tool_router().list_all();
    let mut corpus: Vec<(String, String)> = vec![(
        "the orientation".into(),
        crate::orientation::essay::orientation(),
    )];
    for tool in &tools {
        if let Some(described) = tool.description.as_deref() {
            corpus.push((
                format!("{}'s description", tool.name),
                described.to_string(),
            ));
        }
        // The arguments' own descriptions, which is where a caller reads what a
        // value means before sending one.
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        if let Some(properties) = schema["properties"].as_object() {
            for (argument, described) in properties {
                if let Some(prose) = described["description"].as_str() {
                    corpus.push((
                        format!("{}'s '{argument}' argument", tool.name),
                        prose.to_string(),
                    ));
                }
            }
        }
    }

    let teaching: Vec<&(String, String)> = corpus
        .iter()
        .filter(|(_, text)| text.contains("testimony") && text.contains("inference"))
        .collect();
    assert!(
        teaching.len() >= 3,
        "the sweep found {} places teaching the provenance vocabulary, which is too few to be          the surface — the corpus is not being read",
        teaching.len(),
    );
    for (what, text) in teaching {
        assert!(
            text.contains("observation"),
            "{what} teaches that a claim is testimony or inference and no third thing, so a              session reading it files a claim it read out of a system of record as one of the              two wrong ones",
        );
    }
}

/// **There is exactly one orientation verb, and this is written so a second
/// one cannot satisfy it.**
///
/// "One door, never a second" was prose in the roadmap sitting beside a
/// claim about lineage, and the only test that watched the surface pinned a
/// LIST OF NAMES. So a second door was added, its name was added to the
/// list, the suite stayed green, and the diff read as a deliberate act
/// rather than as the drift it was. A list cannot express "one of these,
/// ever" — adding to it is how you satisfy it.
///
/// The property is asserted three ways in the code and once on the surface,
/// because a second door can be built four ways: by calling `orient` again,
/// by taking the door's arguments again, by reading the essay again, or by
/// telling a caller to start somewhere else.
#[test]
fn there_is_exactly_one_orientation_verb() {
    // The tests around this one construct doors on purpose; the constraint is
    // about the shipped surface, so it reads only what ships — across every
    // file, because a second door is added in a file, not in this one.
    let code = shipped_source();

    for (what, marker, expected) in [
        ("entry points into orientation", "self.orient(", 1),
        (
            "verbs taking the door's arguments",
            "Parameters<OrientArgs>",
            1,
        ),
        // Defined once, read at every legitimate site — a door that
        // reimplemented the answer rather than calling `orient` would still
        // have to reach for the essay, and this is where that shows. The
        // core has more readers than the remainder because a named boot
        // decides on it twice more (sized into the floor, then served) and
        // an anonymous boot joins it with the remainder for its own one
        // candidate; the remainder is read only there.
        ("readers of the essay's core", "ORIENTATION_CORE", 4),
        (
            "readers of the essay's remainder",
            "ORIENTATION_REMAINDER",
            2,
        ),
    ] {
        let found = code.matches(marker).count();
        assert_eq!(
            found, expected,
            "{found} {what} ({marker:?}) — there is one door, and a second is how this fails"
        );
    }

    // And on the surface a caller actually reads: exactly one verb claims
    // to be the one you call first. A door nobody is told to call is not a
    // door, so a second one has to say this somewhere.
    let tools = Jojobot::tool_router().list_all();
    let claiming: Vec<&str> = tools
        .iter()
        .filter(|t| {
            let description = t.description.as_deref().unwrap_or_default().to_lowercase();
            description.contains("call this first") || description.contains("call it first")
        })
        .map(|t| t.name.as_ref())
        .collect();
    assert_eq!(
        claiming,
        ["start_here"],
        "one verb tells a caller where to start, and it is the door"
    );
}

/// **Every verb whose miss is blocked says so where a caller reads it.**
///
/// A description that promises an error for a miss is worse than one that
/// says nothing: a client written against it branches on the wrong thing
/// and handles the answer exactly wrong. The whole class is pinned here
/// rather than one verb at a time, because a description drifts from the
/// answer it describes one verb at a time.
#[test]
fn the_verbs_whose_misses_are_blocked_all_say_so() {
    let tools = Jojobot::tool_router().list_all();
    for name in [
        "recall",
        "retract",
        "update_entity",
        "update_fact",
        "mark_processed",
        "journal",
        "amend_journal",
        "wrap_session",
        "list_runs",
        "read_message",
        "set_charter",
        "start_here",
    ] {
        let tool = tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is a tool"));
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("blocked"),
            "{name} must tell a caller its miss is a blocked result: {description}"
        );
        assert!(
            !description.contains("is an error"),
            "{name} promises an error for a miss it answers as blocked: {description}"
        );
    }
}

/// **The crash contract is in the tool description, not only in the docs.**
/// A consumer that marks first and then fails drops the message silently;
/// the model reading this surface has to be told which order is safe.
#[test]
fn the_mark_processed_description_states_the_crash_contract() {
    let tools = Jojobot::tool_router().list_all();
    let mark = tools
        .iter()
        .find(|t| t.name == "mark_processed")
        .expect("mark_processed is a tool");
    let description = mark.description.as_deref().unwrap_or_default();
    assert!(
        description.contains("ONLY AFTER"),
        "the crash contract must be stated where a consumer reads it: {description}"
    );
    // **…and it must not read as forbidding the ack.** "Act first" made a
    // real session hesitate over pure acknowledgements, where reading IS
    // the acting. The rule and its one boundary case travel together.
    assert!(
        description.contains("READING IT IS THE ACTING"),
        "the crash contract must say where reading is itself the acting: {description}"
    );
}

/// **`update_fact`'s own description sends every caller to the same rewrite,
/// even the one case the build warns against at runtime.** A rewrite that
/// turns a past-event claim into its negation is not an ordinary edit — the
/// receipt says so when it happens — but the door that hands out the
/// instruction in the first place never carves that case out.
#[test]
fn the_update_fact_description_carves_out_the_past_event_case() {
    let tools = Jojobot::tool_router().list_all();
    let update_fact = tools
        .iter()
        .find(|t| t.name == "update_fact")
        .expect("update_fact is a tool");
    let description = update_fact.description.as_deref().unwrap_or_default();
    assert!(
        description.contains("state the negative truth"),
        "the ordinary case is still the rewrite instruction: {description}"
    );
    assert!(
        description.contains("NOT FOR A PAST EVENT"),
        "a past-event negation is never this rewrite, and the door that hands the rewrite \
         instruction to every caller never carves that case out: {description}"
    );
}

/// 🚨 **The colleagues view's one-liner key names itself nowhere a bot would
/// meet it.** `view:colleagues` reads `fields.one_liner`, but nothing
/// agent-facing ever told a bot the key exists or how to write it — a caller
/// reading `view:colleagues` sees a gap where a colleague's summary should
/// be, with no path from there to filling it in. `set_charter` is the
/// surface a bot reaches for to describe itself, so this is where the
/// pointer belongs.
#[test]
fn the_colleagues_one_liner_key_is_named_on_a_surface_a_bot_would_meet() {
    let tools = Jojobot::tool_router().list_all();
    let set_charter = tools
        .iter()
        .find(|t| t.name == "set_charter")
        .expect("set_charter is a tool");
    let description = set_charter.description.as_deref().unwrap_or_default();
    assert!(
        description.contains(crate::orientation::charter::ONE_LINER_KEY),
        "the exact key view:colleagues reads must be named where a bot writing about itself \
         would read it: {description}"
    );
    assert!(
        description.contains("capture"),
        "and it must say which verb writes it, since set_charter itself does not: {description}"
    );
}

/// **The org chart's key is named beside the one-liner.** `view:colleagues`
/// shows who each bot reports to from `reports_to`, so the surface that points
/// a bot at `one_liner` for its self-description points at `reports_to` for
/// who it answers to, and says which verb writes both. The key is pinned as a
/// literal, since it is stored and nothing outside this process declares it.
#[test]
fn the_colleagues_reports_to_key_is_named_where_the_one_liner_is() {
    let tools = Jojobot::tool_router().list_all();
    let set_charter = tools
        .iter()
        .find(|t| t.name == "set_charter")
        .expect("set_charter is a tool");
    let description = set_charter.description.as_deref().unwrap_or_default();
    assert!(
        description.contains("reports_to"),
        "the key view:colleagues shows the chart from must be named where a bot describing \
         itself would read it: {description}"
    );
}

/// **capture's `provenance` says what a bot's instruction is, and what a bot's
/// report is not.** A claim that records what another bot instructed or decided
/// within its own remit is an observation read from that bot, and a world fact
/// the bot only reported is not covered by it. Pinned on the two words only
/// those points carry, read out of the schema a client receives, and on the key
/// that names the message the claim was read from.
#[test]
fn capture_provenance_says_what_a_bots_instruction_is_and_what_its_report_is_not() {
    let tools = Jojobot::tool_router().list_all();
    let capture = tools
        .iter()
        .find(|t| t.name.as_ref() == "capture")
        .expect("the surface offers capture");
    let schema = serde_json::to_value(&capture.input_schema).expect("the schema serializes");
    let described = schema["properties"]["provenance"]["description"]
        .as_str()
        .expect("the provenance argument carries its own description");
    for word in ["read_from", "read_ref", "remit", "reported"] {
        assert!(
            described.contains(word),
            "the provenance description does not say `{word}`: {described}"
        );
    }
}

/// **`add_entity`'s own description names the argument that sets a new thing.**
/// A new argument the verb's description does not mention is the commonest way a
/// capability goes unfound. The argument is `sets`: what this call sets on the
/// new thing, in the same word every write verb uses for it. It is read out of
/// the published schema, so a rename fails here, the old name `fields` is
/// asserted absent, and the description is pinned on the argument's name and on
/// the verb it follows the guards of.
#[test]
fn add_entity_names_the_argument_that_gives_a_new_thing_its_first_fields() {
    let tools = Jojobot::tool_router().list_all();
    let add_entity = tools
        .iter()
        .find(|t| t.name.as_ref() == "add_entity")
        .expect("the surface offers add_entity");
    let schema = serde_json::to_value(&add_entity.input_schema).expect("the schema serializes");
    assert!(
        schema["properties"]["sets"].is_object(),
        "add_entity publishes no `sets`: {schema}"
    );
    assert!(
        schema["properties"].get("fields").is_none(),
        "add_entity still publishes `fields`, which is the record's own word: {schema}"
    );
    let description = add_entity.description.as_deref().unwrap_or_default();
    // The argument counts only in backticks, as the identifier it is: the word
    // `sets` is also in the sentence that explains it.
    // The claim is inference, so a field that needs the operator's word is
    // written with `capture` and testimony: the description says so, or nobody
    // files testimony as inference by accident.
    for word in ["`sets`", "capture", "testimony"] {
        assert!(
            description.contains(word),
            "add_entity's description does not name {word}: {description}"
        );
    }
}

/// **`recall` says what its three newest behaviours are, where a caller reads
/// them.** A view's keys come back by asking without the view, a status filter
/// narrows the claims an answer lists, and a comma list of handles is a field
/// link beside a whole value.
///
/// Each is pinned on the identifier only that behaviour carries, within a short
/// window after the sentence it corrects, because the same words appear
/// elsewhere in the description and a search over the whole text would pass on
/// the wrong sentence. The `keys` argument and the `follow` argument are read
/// from the published schema, which is where a client reads them.
#[test]
fn the_recall_description_says_the_view_the_status_filter_and_the_handle_list() {
    let tools = Jojobot::tool_router().list_all();
    let recall = tools
        .iter()
        .find(|t| t.name.as_ref() == "recall")
        .expect("the surface offers recall");
    let description = recall.description.as_deref().unwrap_or_default();
    let schema = serde_json::to_string(&*recall.input_schema).expect("the schema serializes");
    let keys_argument = serde_json::to_value(&*recall.input_schema).expect("the schema serializes")
        ["properties"]["keys"]["description"]
        .as_str()
        .expect("recall's keys argument carries a description")
        .to_string();
    // The window starts at the anchor and is counted in characters, so a
    // multi-byte character cannot split it.
    let near = |text: &str, anchor: &str, width: usize, needle: &str| {
        let at = text
            .find(anchor)
            .unwrap_or_else(|| panic!("`{anchor}` is not in: {text}"));
        let window: String = text[at..].chars().take(width).collect();
        assert!(
            window.contains(needle),
            "`{needle}` is not within {width} characters after `{anchor}`: {window}"
        );
    };
    near(&keys_argument, "asking again without", 260, "its kind");
    near(description, "asking again without keys", 260, "its kind");
    near(description, "claims of EVERY status", 140, "`status`");
    near(description, "FIELD LINK", 200, "comma");
    near(&schema, "a field link (an entity", 200, "comma");
}

/// **capture's tool description says what becomes of a handle in a field.** It
/// is the text a model reads on every capture, and it said fields are stored
/// and never interpreted. A handle or a comma list of handles is a link and must
/// exist, and six keys stay on their claim.
///
/// Pinned on the words only that sentence carries, within a short window after
/// the sentence it corrects: `link` alone also sits in the next clause, which is
/// about `refs`.
#[test]
fn the_capture_description_says_a_handle_in_a_field_is_a_link() {
    let tools = Jojobot::tool_router().list_all();
    let capture = tools
        .iter()
        .find(|t| t.name.as_ref() == "capture")
        .expect("the surface offers capture");
    let description = capture.description.as_deref().unwrap_or_default();
    let at = description
        .find("GIVE IT FIELDS")
        .expect("capture's description gives fields their own sentence");
    let window: String = description[at..].chars().take(300).collect();
    for word in ["comma", "exist", "claim"] {
        assert!(
            window.contains(word),
            "`{word}` is not within 300 characters after `GIVE IT FIELDS`: {window}"
        );
    }
}

/// **The operator's box is taught where a caller meets it.** A bot reads
/// `post_message` to learn whom it may write to, `read_message` to learn what
/// it may open, and `list_sent` to learn what it may see of its own outbox. Each
/// says what is different about the one person who has a box: they are
/// addressed by their handle, no bot reads their mail back, and an outbox
/// listing shows only the id, time and subject.
///
/// Pinned on the words only those sentences carry, read from the published
/// descriptions and the `to` argument's schema, which is where a client reads
/// them.
#[test]
fn the_operators_box_is_taught_where_a_bot_posts_reads_and_lists() {
    let tools = Jojobot::tool_router().list_all();
    let tool = |name: &str| {
        tools
            .iter()
            .find(|t| t.name.as_ref() == name)
            .unwrap_or_else(|| panic!("the surface offers {name}"))
    };
    let described = |name: &str| {
        tool(name)
            .description
            .as_deref()
            .unwrap_or_default()
            .to_string()
    };

    let post = described("post_message");
    // `first post` is the clause that says the post is what opens their box;
    // `operator` alone also sits in the argument's own backticks.
    for word in ["operator", "person:", "handle", "first post"] {
        assert!(
            post.contains(word),
            "post_message does not say `{word}`: {post}"
        );
    }
    let schema = serde_json::to_value(&*tool("post_message").input_schema).expect("schema");
    let to = schema["properties"]["to"]["description"]
        .as_str()
        .expect("`to` carries a description");
    for word in ["operator", "person:"] {
        assert!(
            to.contains(word),
            "post_message's `to` does not say `{word}`: {to}"
        );
    }
    let read = described("read_message");
    assert!(
        read.to_lowercase().contains("operator"),
        "read_message does not say the operator's box is never opened: {read}"
    );
    let listed = described("list_sent");
    // The sentence about mail to the operator is the first place the word
    // appears, and `subject` must sit within it.
    let lower = listed.to_lowercase();
    let at = lower
        .find("operator")
        .expect("list_sent names the operator");
    let window: String = lower[at..].chars().take(120).collect();
    assert!(
        window.contains("subject"),
        "list_sent does not say mail to the operator is listed by subject: {listed}"
    );
}

/// **The second call for who reports to a bot is named where `reports_to` is
/// taught.** The colleagues view says only whom each bot reports to, so the
/// surface that tells a bot to write the key also says how to read the other
/// direction: `recall` of the manager with `reports_to` followed inward. The
/// argument names are pinned against the published schema, so a rename fails
/// here and not in a session's call.
#[test]
fn the_call_that_lists_who_reports_to_a_bot_is_named_where_reports_to_is_taught() {
    let tools = Jojobot::tool_router().list_all();
    let described = |name: &str| -> (String, serde_json::Value) {
        let tool = tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is a tool"));
        (
            tool.description.as_deref().unwrap_or_default().to_string(),
            serde_json::Value::Object((*tool.input_schema).clone()),
        )
    };
    let (recall_description, recall_schema) = described("recall");
    assert!(
        recall_schema["properties"].get("follow").is_some(),
        "recall publishes no `follow`: {recall_description}"
    );
    let (description, _) = described("set_charter");
    for word in ["recall", "follow", "relation", "direction"] {
        assert!(
            description.contains(word),
            "the description of set_charter does not name `{word}`, so the call that lists \
             who reports to a bot is not where reports_to is taught: {description}"
        );
    }
}

/// **`recall`'s own description names the argument that narrows each object's
/// keys, what an answer says when it left some out, and the view key that
/// carries it.** A new argument the verb's description does not mention is the
/// commonest way a capability goes unfound. The argument is pinned by the
/// published schema, so a rename fails here, and the two words only the answer
/// and a view record carry are pinned in the description.
#[test]
fn recall_names_the_keys_argument_and_what_an_answer_says_it_left_out() {
    let tools = Jojobot::tool_router().list_all();
    let recall = tools
        .iter()
        .find(|t| t.name == "recall")
        .expect("recall is a tool");
    let description = recall.description.as_deref().unwrap_or_default();
    let schema = serde_json::Value::Object((*recall.input_schema).clone());
    assert!(
        schema["properties"].get("keys").is_some(),
        "recall publishes no `keys`: {schema}"
    );
    for word in ["fields_left_out", "shows_keys"] {
        assert!(
            description.contains(word),
            "recall's description does not name `{word}`: {description}"
        );
    }
}

/// **`retract` and `update_fact`'s `clear_edge` used to both claim the same
/// case — a past event that turned out never to have happened.** `retract`
/// names itself the move for it; `clear_edge`'s own worked example, until
/// this fix, was exactly that case restated, with no mention of `retract` at
/// all — so a caller reading `update_fact` alone met an endorsement and no
/// fork.
///
/// **The deliverable is agreement, not either sentence alone**, so this reads
/// both: `retract` still claims the case (unchanged, and worth pinning so a
/// future edit there cannot silently drop it), and `update_fact` now names
/// `retract` as the verb for it rather than explaining how to do it with
/// `clear_edge`.
#[test]
fn retract_and_clear_edge_no_longer_claim_the_same_case() {
    let tools = Jojobot::tool_router().list_all();
    let described = |name: &str| -> String {
        tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is a tool"))
            .description
            .as_deref()
            .unwrap_or_default()
            .to_string()
    };
    let retract = described("retract");
    let update_fact = described("update_fact");
    assert!(
        retract.contains("SOMETHING THAT HAPPENED"),
        "retract must still claim a past event that turned out not to have happened: {retract}"
    );
    assert!(
        update_fact.contains("RETRACT'S CASE"),
        "update_fact's clear_edge advice must name retract as the verb for that case rather \
         than explaining how to do it here: {update_fact}"
    );
}

/// **Polling is a read, and the surface has to say so where the expensive
/// call is read.** A session whose standing loop was "check the box; if empty
/// do nothing" paid ~14 state-changing deliveries of an empty box, because
/// the only verb that visibly answered "is there anything waiting" was the
/// one that takes delivery.
///
/// A caller standing at `read_mailbox` must be told, in this description,
/// that there is a way to look without taking. Asserted on the ARGUMENT
/// rather than on a tool name, so it cannot be satisfied by pointing at a
/// different tool.
#[test]
fn the_read_mailbox_description_points_at_the_read_only_way_to_poll() {
    let tools = Jojobot::tool_router().list_all();
    let read = tools
        .iter()
        .find(|t| t.name == "read_mailbox")
        .expect("read_mailbox is a tool");
    let description = read.description.as_deref().unwrap_or_default();
    assert!(
        description.contains("counts_only"),
        "the cheap read must be named where the expensive one is read: {description}"
    );
    // …and what makes it cheap, since that is the part a caller acts on: a
    // poll that costs a delivery is the failure this exists to prevent.
    assert!(
        description.contains("nothing becomes yours to finish"),
        "…and must say that polling owes nothing, which is the whole reason to \
         reach for it: {description}"
    );
}

/// **A description may not name a parameter its verb does not take.**
///
/// `bot` and `session` are both gone from these verbs' schemas — one address
/// rides every call now, and it is the `sid`. The descriptions are the half
/// of the surface a model actually reads, so one still saying "pass `bot`,
/// the name you booted as" produces exactly the call the schema refuses,
/// from a caller who has no reason to doubt the sentence.
///
/// **Pinned per verb rather than swept over the whole surface**, because two
/// verbs keep a legitimate `bot` and neither is the caller's identity:
/// `start_here` takes the name to boot AS, and `set_charter`'s names the bot
/// its write is ABOUT, exactly as a capture names a subject.
#[test]
fn the_session_verbs_are_described_by_the_one_address_they_take() {
    let tools = Jojobot::tool_router().list_all();
    for name in [
        "journal",
        "amend_journal",
        "wrap_session",
        "list_runs",
        "read_mailbox",
        "post_message",
    ] {
        let tool = tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is a tool"));
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("`sid`"),
            "{name} must name the address it takes: {description}"
        );
        // `sender` joins the list for the same reason the other two are on
        // it: `post_message` derives it from the handle and takes no such
        // parameter, so a sentence describing one sends a caller to emit a
        // field that is silently dropped.
        for gone in ["`bot`", "`session`", "`sender`", "you booted as"] {
            assert!(
                !description.contains(gone),
                "{name} still describes {gone}, which is no parameter of it: {description}"
            );
        }
    }
}

/// **Nothing agent-facing tells a caller to declare who it is.**
///
/// `sender` is derived from the `sid`, so no verb takes it as an argument.
/// With the argument gate — see [`crate::arguments`] — a text naming it costs
/// the call rather than only the truth: a caller following the sentence sends
/// a top-level argument `post_message` does not implement, gets `blocked`
/// before dispatch, and the message it meant to leave is never written. The
/// text is what breaks the call, and the refusal contradicting the surface's
/// own words is the caller's only clue.
///
/// **Asserted as absence of the token, not as a list of today's
/// sentences.** The essay and `post_message` have no honest use for the
/// word: the caller does not supply one, so any sentence that reaches for
/// it is describing a parameter that is not there, whatever its wording.
/// `list_sent` is the one verb that still takes a `sender` — somebody
/// else's, to ask after their mail — so it is the one place the token
/// belongs.
#[test]
fn no_agent_facing_text_asks_a_caller_to_declare_a_sender() {
    assert!(
        !crate::orientation::essay::orientation().contains("`sender`"),
        "the essay still asks a caller for a sender it does not supply"
    );
    let tools = Jojobot::tool_router().list_all();
    let post = tools
        .iter()
        .find(|t| t.name == "post_message")
        .expect("post_message is a tool");
    let description = post.description.as_deref().unwrap_or_default();
    assert!(
        !description.contains("`sender`"),
        "post_message still describes a `sender` parameter it does not take: {description}"
    );
}

/// **The door says what to carry away from it, and how far it reaches.**
///
/// A boot that hands back an address and then tells the caller to identify
/// itself some other way has spent the answer it just gave. The reach is the
/// part a caller cannot infer: `sid` rides the reads too — they are
/// attributed, never journalled — and a caller who passes it only on the
/// session verbs is anonymous for every other call it makes.
#[test]
fn the_boot_door_says_the_sid_rides_every_call_including_the_reads() {
    let tools = Jojobot::tool_router().list_all();
    let door = tools
        .iter()
        .find(|t| t.name == "start_here")
        .expect("start_here is a tool");
    let description = door.description.as_deref().unwrap_or_default();
    assert!(
        !description.contains("you booted as"),
        "the door must not send a caller back to naming its bot: {description}"
    );
    assert!(
        description.contains("reads included"),
        "the door must say the sid rides the reads too: {description}"
    );
}

/// **The essay teaches the address, and what jojobot writes down about you.**
///
/// Two claims that moved with the model. What makes two connections one
/// session is the `sid` the caller carries, not an identity the connection
/// remembers — nothing remembers anything between calls. And jojobot's own
/// beats follow the WRITES: every call site of [`Jojobot::beat`] is a write
/// verb and [`BEAT_CLASSES`] holds no read, so an essay promising "one per
/// verb class you use" tells a session to expect a tally of its reads that
/// will never appear.
#[test]
fn the_orientation_teaches_the_sid_as_the_address_and_leaves_reads_untallied() {
    let essay = crate::orientation::essay::orientation();
    assert!(
        essay.contains("`sid` you carry"),
        "the essay must name what makes two connections one session"
    );
    assert!(
        !essay.contains("the identity that booted them"),
        "the essay still says a connection carries the identity, which nothing does"
    );
    assert!(
        essay.contains("Reads are not journalled"),
        "the essay must say which calls jojobot beats about"
    );
    assert!(
        !essay.contains("one per verb class you use"),
        "the essay still promises a beat per verb class, reads included"
    );
}

/// **The norms a session cannot derive from the tool list are taught.**
/// Each of these was a real session getting it wrong or having no way to
/// know: wrapping a session whose work continues (so the next run started
/// from nothing), treating `abandoned` as an ordinary ending, and reading a
/// flat box listing as an invitation to survey a shared namespace.
///
/// Deliberately **engine-generic**: how long a given role's session should
/// run, or which box a particular bot drains, is that bot's charter at
/// seeding — not prose compiled into a user-agnostic server.
#[test]
fn the_orientation_teaches_the_two_endings_and_the_own_box_norm() {
    let essay = crate::orientation::essay::orientation();
    // The two endings, and that they are a choice about the WORK.
    assert!(
        essay.contains("CLEAR AND RESUME"),
        "the continuing case is named"
    );
    assert!(
        essay.contains("do NOT wrap"),
        "…and says which verb NOT to reach for, since wrapping is the tempting default"
    );
    assert!(
        essay.contains("resume note"),
        "…and names the thing you leave for whoever picks it up"
    );
    assert!(
        essay.contains("exception to journal leanness"),
        "…and exempts it from the leanness rule, or the rule suppresses it"
    );
    // **`abandoned` is not a failure**, and the essay must not teach it as
    // one: it means the run was never wrapped up, and picking one back up
    // is ordinary rather than recovery. What the essay still has to draw is
    // the distinction that survives — a run that ENDED against one that
    // merely STOPPED.
    assert!(
        essay.contains("not a failure"),
        "abandoned is a run nobody wrapped up, not a run that broke"
    );
    assert!(
        !essay.contains("failure path"),
        "…so the old framing must be gone, not merely balanced by the new one"
    );
    assert!(
        essay.contains("merely stopped"),
        "…and the distinction that does survive is ended against stopped"
    );

    // The own-box norm, and the affordance that tempted otherwise. It is no
    // longer a norm a caller can decline — the read side takes no box name —
    // so what the essay owes is that the reader knows which box opens.
    assert!(essay.contains("read your OWN mailbox"));
    assert!(
        essay.contains("no name to pass"),
        "the essay has to say the choice is gone, not merely discouraged"
    );
    assert!(
        essay.contains("not an invitation"),
        "the flat listing is what posed the access question, so it is what gets answered"
    );
    assert!(
        essay.contains("post_message"),
        "…and there is a sanctioned way to reach another box: write to it"
    );
}

/// **A KIND holds a thing to its keys and a declared TYPE holds it to nothing,
/// and the essay is where a session learns which.**
///
/// What governs a write is the thing's own KIND. A text saying a declared type
/// blocks a write describes a guard that governs by resemblance: every thing
/// structurally completing any declaration anybody has made becomes subject to
/// it, with nothing offered and nothing switched on. Every session reads this
/// essay at boot and plans against it, so that sentence makes a session expect
/// a refusal that does not come and avoid a write nothing would have stopped.
///
/// **Both halves, or either is worthless**: the mechanism is exercised through
/// the guard every adapter runs, and the essay is read for what it says about
/// it. Prose pinned against no mechanism goes stale, and a mechanism half that
/// only asserted a refusal would pass on a build where a type still governs.
#[test]
fn the_orientation_says_a_kind_holds_a_thing_and_a_type_does_not() {
    use jojobot_domain::memory::guard_fit;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
    use std::collections::BTreeMap;

    let pet = DeclaredType::new("pet", vec![Field::required("species", ValueType::Text)]);
    let fitting: BTreeMap<String, String> = [("species".to_string(), "cat".to_string())]
        .into_iter()
        .collect();
    let refused = guard_fit(
        "pet",
        &fitting,
        &BTreeMap::new(),
        std::slice::from_ref(&pet),
    )
    .expect_err("a kind's key has to survive a write to a thing of that kind");
    assert!(
        refused.to_string().contains("species"),
        "…and the refusal names the key it would lose: {refused}"
    );
    // **The half that says the guard reads the KIND rather than the shape.**
    // The same keys and the same declaration, asked about a thing of another
    // kind: `pet` is a vocabulary this thing answers, and answering is not
    // being held.
    guard_fit(
        "thing",
        &fitting,
        &BTreeMap::new(),
        std::slice::from_ref(&pet),
    )
    .expect("a declaration that is not this thing's kind governs nothing");
    guard_fit("pet", &BTreeMap::new(), &BTreeMap::new(), &[pet])
        .expect("a thing below its kind's floor has nothing to protect, so nothing is refused");

    // Scoped to the paragraph that teaches declaring: "refus" is a word the
    // essay spends elsewhere, on the gates, and a needle over the whole text
    // would find one of those and call it this rule.
    let essay = crate::orientation::essay::orientation();
    let taught = essay
        .lines()
        .find(|line| line.contains("Declare a type"))
        .expect("the essay teaches declaring a type");
    assert!(
        taught.contains("refuses nothing"),
        "declaring a type gates no write, and a session planning around a refusal that never \
         comes avoids writes nothing would stop: {taught}"
    );
    assert!(
        taught.contains("KIND"),
        "…and it has to say what DOES hold a thing to its keys, since a session told only \
         that a type does not will conclude nothing does: {taught}"
    );
}

/// **Every sentence this crate compiles in**, with the file and line that
/// carries it.
///
/// **The second corpus, and it exists because the first cannot reach a
/// refusal.** [`agent_facing_text`] gathers what a caller reads BEFORE a call
/// — descriptions, schemas, the essay, the instructions — plus one module's
/// answer notes. The prose a caller reads AFTER a call is built where the
/// refusal is decided: some of it by a pure function of an error value, some
/// of it as a `format!` inside an async handler that only a rigged store
/// reaches. A gatherer that called the reachable ones would be a list of call
/// sites, and a list of call sites goes stale the moment somebody writes the
/// next refusal.
///
/// **So this reads the source instead, and takes every literal.** A sentence
/// jojobot compiles in is one it can serve; nothing in the source says which
/// ones do. That is deliberately wider than "agent-facing" — an `expect`
/// message is in here too — and the rule swept over it is the one worth
/// holding everywhere: the vocabulary. Holding an internal message to the
/// same words costs a word, and the alternative is a corpus that stops at
/// whichever sites somebody remembered.
fn shipped_prose() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (file, text) in shipped_files() {
        for (line, literal) in string_literals(&text) {
            // Prose, rather than every literal: a JSON key, a handle and a
            // token are not sentences, and the rules below are about
            // sentences.
            if literal.split_whitespace().count() >= 4 {
                found.push((format!("{file}:{line}"), literal));
            }
        }
    }
    found
}

/// The string literals of one Rust source, each with the line it opens on.
///
/// **Written out rather than approximated by a regular expression**, because
/// three things in this crate's own source defeat the approximation and each
/// of them changes the answer:
///
/// * A comment that quotes a sentence is not a literal. Half the prose in
///   these files sits in doc comments discussing the strings below them, so a
///   scan that counts those reports a sentence at a line nobody ships.
/// * The long descriptions are written as `\` continuations, and Rust drops
///   the newline AND the indentation that follows it. Rejoining them with a
///   space instead splits words that were written whole — `crm-\n card` is
///   how a hyphenated word becomes two, and one of them is a word this file
///   forbids.
/// * The skills and the essay are raw strings. A scanner that does not know
///   `r#"` reads no skill body at all and passes, having looked at nothing.
///
/// Escapes are resolved to what the string holds, not to what it looks like.
fn string_literals(text: &str) -> Vec<(usize, String)> {
    let src: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut at = 0;
    let mut line = 1;
    while at < src.len() {
        match src[at] {
            '\n' => {
                line += 1;
                at += 1;
            }
            '/' if src.get(at + 1) == Some(&'/') => {
                while at < src.len() && src[at] != '\n' {
                    at += 1;
                }
            }
            '/' if src.get(at + 1) == Some(&'*') => {
                let mut depth = 1;
                at += 2;
                while at < src.len() && depth > 0 {
                    match (src[at], src.get(at + 1)) {
                        ('/', Some('*')) => {
                            depth += 1;
                            at += 2;
                        }
                        ('*', Some('/')) => {
                            depth -= 1;
                            at += 2;
                        }
                        ('\n', _) => {
                            line += 1;
                            at += 1;
                        }
                        _ => at += 1,
                    }
                }
            }
            // A char literal or a lifetime. Only `'x'` and `'\x'` close; a
            // lifetime never does, and reading one as a literal swallows the
            // code after it.
            '\'' => {
                let escaped = src.get(at + 1) == Some(&'\\');
                let closes = src.get(at + if escaped { 3 } else { 2 }) == Some(&'\'');
                at += match (closes, escaped) {
                    (true, true) => 4,
                    (true, false) => 3,
                    (false, _) => 1,
                };
            }
            // A raw string, in any number of hashes. `r#type` is a raw
            // identifier and opens nothing.
            'r' if !src
                .get(at.wrapping_sub(1))
                .is_some_and(|c| c.is_alphanumeric() || *c == '_') =>
            {
                let mut hashes = 0;
                while src.get(at + 1 + hashes) == Some(&'#') {
                    hashes += 1;
                }
                if src.get(at + 1 + hashes) != Some(&'"') {
                    at += 1;
                    continue;
                }
                let opened = line;
                let mut end = at + 2 + hashes;
                while end < src.len() {
                    if src[end] == '"' && (1..=hashes).all(|h| src.get(end + h) == Some(&'#')) {
                        break;
                    }
                    if src[end] == '\n' {
                        line += 1;
                    }
                    end += 1;
                }
                found.push((
                    opened,
                    src[at + 2 + hashes..end.min(src.len())].iter().collect(),
                ));
                at = end + 1 + hashes;
            }
            '"' => {
                let opened = line;
                let mut literal = String::new();
                let mut end = at + 1;
                while end < src.len() && src[end] != '"' {
                    if src[end] != '\\' {
                        if src[end] == '\n' {
                            line += 1;
                        }
                        literal.push(src[end]);
                        end += 1;
                        continue;
                    }
                    match src.get(end + 1) {
                        // The continuation: the newline and every space after
                        // it are not in the string.
                        Some('\n') => {
                            line += 1;
                            end += 2;
                            while src.get(end).is_some_and(|c| c.is_whitespace()) {
                                if src[end] == '\n' {
                                    line += 1;
                                }
                                end += 1;
                            }
                        }
                        Some('n') | Some('t') => {
                            literal.push(' ');
                            end += 2;
                        }
                        Some(escaped) => {
                            literal.push(*escaped);
                            end += 2;
                        }
                        None => end += 1,
                    }
                }
                found.push((opened, literal));
                at = end + 1;
            }
            _ => at += 1,
        }
    }
    found
}

/// **The scanner reads what the compiler reads.**
///
/// Every case here is one this crate's own source contains, and each one
/// changes the answer rather than tidying it: a quoted sentence inside a
/// comment is a line nobody ships, a `\` continuation is where a hyphenated
/// word gets split in two, and a raw string is where every skill body lives.
/// The char literal is the one that does not look like prose at all: `'\"'`
/// is a quote the compiler does not open a string with, and a scanner that
/// misses that reads the CODE after it as a sentence — while a lifetime,
/// which never closes, must not be skipped as though it were a literal.
#[test]
fn the_scanner_reads_the_strings_and_not_the_source_around_them() {
    let source = concat!(
        "// a comment saying \"nothing shipped\"\n",
        "const A: &str = \"a plain shipped sentence\";\n",
        "/* a block\n saying \"nothing shipped\" */\n",
        "fn f<'a>(x: &'a str) -> char { '\\n' }\n",
        "fn g() -> char { '\"' }\n",
        "const B: &str = \"a crm-\\\n    card is one word\";\n",
        "const C: &str = r#\"a raw \"quoted\" body\"#;\n",
        "const D: &str = \"an escaped \\\" quote\";\n",
    );
    assert_eq!(
        string_literals(source),
        vec![
            (2, "a plain shipped sentence".to_string()),
            (7, "a crm-card is one word".to_string()),
            (9, "a raw \"quoted\" body".to_string()),
            (10, "an escaped \" quote".to_string()),
        ],
        "the scanner must read the literals, at the lines that carry them, and nothing else"
    );
}

/// **A file whose sentences went ungathered is the failure this gather
/// exists to end.**
///
/// A scanner that quietly read no raw string, or walked off a file, leaves
/// every sweep below green over a corpus with a hole in it — the same shape
/// as the refusals going unread in the first place, and invisible for the
/// same reason. So the scanner is checked against a SECOND, cruder reading:
/// one line at a time, quotes counted rather than parsed, comments skipped.
///
/// **One direction only, and that is the whole point.** The crude read misses
/// things the real one catches — a sentence written across a `\` continuation
/// is two short fragments to it — so it can never demand prose that is not
/// there. What it CAN do is name a file it can plainly see a sentence in and
/// the scanner returned nothing for, which is the only failure a scanner has.
///
/// It is derived rather than listed: no file is named here, so a file added
/// tomorrow is covered tomorrow.
#[test]
fn no_shipped_file_has_its_sentences_missed() {
    /// The crude reading: quoted runs on one line, outside a comment.
    fn plainly_carries_prose(text: &str) -> bool {
        text.lines()
            .filter(|line| {
                let start = line.trim_start();
                !start.starts_with("//") && !start.starts_with('*')
            })
            .flat_map(|line| line.split('"').skip(1).step_by(2))
            .any(|run| run.split_whitespace().count() >= 4)
    }

    let gathered = shipped_prose();
    assert!(
        !gathered.is_empty(),
        "the gather returned no sentence at all, so every sweep over it reads nothing and passes"
    );
    let plain: Vec<String> = shipped_files()
        .into_iter()
        .filter(|(_, text)| plainly_carries_prose(text))
        .map(|(file, _)| file)
        .collect();
    assert!(
        !plain.is_empty(),
        "the crude read found no prose anywhere, so it demands nothing of the scanner below"
    );
    let missed: Vec<&String> = plain
        .iter()
        .filter(|file| !gathered.iter().any(|(at, _)| at.starts_with(*file)))
        .collect();
    assert!(
        missed.is_empty(),
        "a sentence can be read off these files one line at a time and the scanner gathered \
         none of it, so no check on this text can fail for them: {missed:?}"
    );
}

/// **Every word an agent reads before a call, and `search`'s coverage notes**
/// — tool descriptions, the argument-schema field docs, the orientation essay,
/// the server instructions, and that one set of notes.
///
/// **The schemas are the half that gets forgotten.** A doc comment on a public
/// args field is not a comment: `schemars` renders it into the JSON schema, so
/// it reaches a caller exactly as a description does. That is where `boot`
/// spent a release describing the deleted `mailbox` parameter.
///
/// **A note in an answer is read the same way a description is.** A check that
/// reads every description and no answer reads the half a caller meets before
/// the call and skips the half it meets after.
///
/// **The answer half gathered HERE is still one module's**, and the rest of
/// it — the chronology note, the unreadable-items report, the notes on the
/// identity path, the orient notes, the mailbox note, the refusal texts —
/// comes in through [`shipped_prose`] instead, because it is written where it
/// is decided rather than returned by anything this function could call.
/// [`everything_served`] is the pair, and it is what the sweeps read.
fn agent_facing_text() -> Vec<(String, String)> {
    let mut found = vec![
        (
            "the orientation essay".to_string(),
            crate::orientation::essay::orientation(),
        ),
        (
            "the server instructions".to_string(),
            Jojobot::new(
                Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
                Arc::new(crate::memory::testing::SpySearch::default()),
                Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
                Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
                Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
                crate::harness::seeded_registry(),
            )
            .get_info()
            .instructions
            .unwrap_or_default(),
        ),
    ];
    for tool in Jojobot::tool_router().list_all() {
        found.push((
            format!("{}'s description", tool.name),
            tool.description.as_deref().unwrap_or_default().to_string(),
        ));
        found.push((
            format!("{}'s argument schema", tool.name),
            serde_json::to_string(&tool.input_schema).expect("a schema serializes"),
        ));
    }
    for (what, note) in crate::memory::search::coverage_notes() {
        found.push((what, note));
    }
    found
}

/// **The gathering really reaches the coverage notes.**
///
/// Delete that loop and every check over this text still passes, across a body
/// of text with a hole in it — the same shape as the notes going ungathered in
/// the first place. What is asserted is the relation, every note `search` can
/// serve being in the gathered text, rather than any sentence one of them
/// carries.
#[test]
fn the_gathered_text_holds_every_coverage_note() {
    let gathered = agent_facing_text();
    let notes = crate::memory::search::coverage_notes();
    assert!(
        !notes.is_empty(),
        "the gathering has notes to reach, or the loop below asserts nothing"
    );
    for (what, note) in notes {
        assert!(
            gathered.iter().any(|(w, t)| *w == what && *t == note),
            "{what} is served to an agent and no check on this text reads it"
        );
    }
}

/// **Everything jojobot can put in front of an agent**: what a caller reads
/// before a call, and every sentence the crate compiles in.
///
/// The two halves are gathered differently because they are reachable
/// differently — one by asking the router, one by reading the source — and
/// the sweeps below want them together, because a rule about the words
/// jojobot uses does not stop at the moment of the call. The retired-word
/// sweep found its last two offenders on the second half.
fn everything_served() -> Vec<(String, String)> {
    agent_facing_text()
        .into_iter()
        .chain(shipped_prose())
        .collect()
}

/// Whether this text uses `word` as a word, rather than as a run of letters
/// inside a longer one. The plural counts: a trailing `s` is the same word.
///
/// **A substring match cannot carry a rule about short words.** "row" sits
/// inside "grow", "narrow" and "browser"; "card" sits inside "discard". Every
/// one of those is ordinary English doing its job, so a substring test reports
/// sentences that are correct and the list below stops being usable for exactly
/// the words most likely to leak.
fn mentions(haystack: &str, word: &str) -> bool {
    haystack.match_indices(word).any(|(at, _)| {
        let opens = haystack[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let mut rest = haystack[at + word.len()..].chars();
        let closes = match rest.next() {
            None => true,
            Some('s') => rest.next().is_none_or(|c| !c.is_alphanumeric()),
            Some(c) => !c.is_alphanumeric(),
        };
        opens && closes
    })
}

/// **The boundary rule is what lets the list below carry a short word.**
///
/// Both directions matter and they fail differently: matching inside a longer
/// word reports correct sentences until somebody deletes the entry to get the
/// suite green, and matching nothing at all leaves a guard that passes because
/// it looks at nothing.
#[test]
fn a_word_is_only_found_where_it_is_a_word() {
    assert!(mentions("a fact is one row on the page", "row"));
    assert!(mentions("two rows", "row"), "the plural is the same word");
    assert!(mentions("row", "row"), "the whole text can be the word");
    assert!(mentions("a crm-card id", "card"), "a hyphen ends a word");

    assert!(!mentions("the graph is meant to grow", "row"));
    assert!(!mentions("narrowed to one kind", "row"));
    assert!(!mentions("a browser with no repository", "row"));
    assert!(!mentions("documented elsewhere", "document"));
}

/// **The prose an agent reads describes the system that exists.**
///
/// For an MCP server the prose IS the interface: a session boots, reads this
/// text, and forms its whole world model from it. So text teaching a retired
/// design is not a documentation lapse — it is a wrong interface, and an agent
/// that believes it acts on it. The deploy-boundary review found six of these
/// at once, all describing the pre-migration world, and one of them would have
/// made a bot refuse to open its own inbox.
///
/// An agent must never be taught the store's shape — not its business, and
/// it will be wrong again — and must never be sent to repair something in a
/// system that does not hold it. **That covers the store serving today as well
/// as the one that was retired** (rule 53): a word true only of the product
/// underneath is a word that stops being true when the product is swapped, and
/// an agent taught it has to unlearn a model rather than read a new sentence.
///
/// Six point-fixes would have left the seventh. This is the class.
#[test]
fn no_agent_facing_text_teaches_the_store() {
    // Each word, and what an agent wrongly concludes from meeting it.
    const RETIRED: &[(&str, &str)] = &[
        ("card", "a message is a record in a box, not a card"),
        ("kanban", "the board is gone"),
        ("funnel", "there are no columns to move between"),
        ("task board", "mail left the task layer entirely"),
        ("task system", "mail left the task layer entirely"),
        (
            "mailbox label",
            "a box is owned by its bot, not a label on something else",
        ),
        (
            "document",
            "an agent holds entities, facts and prose; a document is what the store keeps them in",
        ),
        (
            "row",
            "a fact is a claim, not a line in whatever table holds it today",
        ),
    ];
    // **The one legitimate use, allowlisted by name and by reason.** The word
    // survives in one place: `crm-card` as an example SOURCE value, which is a
    // label the operator's own records carry rather than a grammar jojobot
    // enforces. `crm` itself teaches no grammar — the task layer
    // decides how it addresses things
    // (`jojobot_domain::memory::validate_crm`). The rule is that no text
    // teaches JOJOBOT'S store; blanking the word here would turn a true
    // sentence false. Each entry must actually be hit — a stale exception
    // fails below, so this cannot quietly become a place to put new ones.
    const ALLOWED: &[(&str, &str)] = &[("add_entity's argument schema", "card")];

    let mut teaching: Vec<String> = Vec::new();
    let mut unused: Vec<&(&str, &str)> = ALLOWED.iter().collect();
    let served = everything_served();
    assert!(
        !served.is_empty(),
        "nothing was gathered, so the sweep below reads no text at all and passes on an empty \
         corpus"
    );
    for (what, text) in served {
        let haystack = text.to_lowercase();
        for (word, why) in RETIRED {
            if !mentions(&haystack, word) {
                continue;
            }
            if let Some(at) = unused.iter().position(|(w, x)| *w == what && x == word) {
                unused.remove(at);
                continue;
            }
            if ALLOWED.iter().any(|(w, x)| *w == what && x == word) {
                continue;
            }
            teaching.push(format!("{what} says {word:?} — {why}"));
        }
    }
    // **Every offender at once, not the first one.** This is a class, and a
    // test that stops at the first instance turns fixing a class into six
    // rounds of finding the next one.
    assert!(
        teaching.is_empty(),
        "agent-facing text teaches the store rather than the system. An agent reads this as the \
         truth about what it is calling, and acts on it:\n  {}",
        teaching.join("\n  ")
    );
    assert!(
        unused.is_empty(),
        "these exceptions no longer match anything — delete them, or the allowlist stops being \
         a record of what is here and becomes a hole nobody reviewed: {unused:?}"
    );
}

/// The sentences of one piece of served text, lowercased.
///
/// **A line break is not a sentence end here.** An argument doc reaches the
/// schema wrapped where its doc comment was wrapped — as the two characters
/// `\n`, since the schema arrives JSON-escaped — so a check that cut at those
/// would read the halves of one claim as two unrelated sentences and see
/// neither whole. Breaks become spaces; only the sentence marks divide.
fn sentences(text: &str) -> Vec<String> {
    text.replace("\\n", " ")
        .replace('\n', " ")
        .split(['.', '!', '?'])
        .map(str::to_lowercase)
        .collect()
}

/// Whether this sentence says `marker` — as a word, or verbatim when the marker
/// is a phrase.
fn says(sentence: &str, marker: &str) -> bool {
    if marker.contains(' ') {
        sentence.contains(marker)
    } else {
        mentions(sentence, marker)
    }
}

/// **What a kind requires is read off the kinds, and the served text has to
/// agree with it.**
///
/// A kind may name keys the things of that kind must keep, and the floor rule
/// refuses a write that would drop one. Whether any shipped kind does that is a
/// fact about the kind table, so this case reads the table rather than a
/// sentence somebody wrote about it: **a claim stating what is true today is a
/// claim nothing re-checks**, and this is the re-check.
///
/// **The failure it exists for is not a missing sentence, it is permission.** A
/// session reading that nothing is held to anything plans writes on that basis
/// and meets the refusal on the first loop it edits.
///
/// **Both halves, in one read.** The absence of the false claim passes
/// identically against a build serving no text at all, so the corpus is
/// asserted first and the required keys are asserted by name. The names are
/// derived from the kinds, so a kind that gains a required key fails this until
/// the text a session reads names it. **A key is looked for the way the essay
/// writes one, in backticks**, because a key called `name` is an ordinary
/// English word and a bare substring finds it in any sentence.
#[test]
fn no_agent_facing_text_says_a_kind_holds_a_thing_to_nothing() {
    use jojobot_domain::memory::kinds;

    // **Read off the kinds, never written down here.** Each shipped kind with
    // the keys its things have to keep.
    let holding: Vec<(&str, Vec<String>)> = kinds::SHIPPED
        .iter()
        .map(|token| {
            let required: Vec<String> = kinds::keys_of(token)
                .into_iter()
                .filter(|field| field.required)
                .map(|field| field.key)
                .collect();
            (*token, required)
        })
        .filter(|(_, required)| !required.is_empty())
        .collect();

    let served = everything_served();
    assert!(
        !served.is_empty(),
        "nothing was gathered, so the sweep below reads no text at all and passes on an empty          corpus"
    );

    // A sentence about kinds that says the floor is empty. The markers are the
    // claim rather than one wording of it: what makes such a sentence wrong is
    // that it tells a caller no kind holds anything.
    const EMPTY_FLOOR: &[&str] = &["names any key", "held to anything", "holds nothing to"];
    let mut claiming: Vec<String> = Vec::new();
    for (what, text) in &served {
        for sentence in sentences(text) {
            if !mentions(&sentence, "kind") {
                continue;
            }
            for marker in EMPTY_FLOOR {
                if says(&sentence, marker) {
                    claiming.push(format!("{what} says {marker:?} of a kind"));
                }
            }
        }
    }
    assert!(
        holding.is_empty() || claiming.is_empty(),
        "the kinds hold things to keys ({holding:?}) and agent-facing text says they hold them          to nothing. A session reads that as permission and plans a write the floor rule          refuses:
  {}",
        claiming.join("
  "),
    );

    // …and the true claim in its place: the essay a fresh session reads names
    // every key it can be refused for losing.
    let (_, essay) = served
        .iter()
        .find(|(what, _)| what == "the orientation essay")
        .expect("the essay a fresh session reads is in the corpus");
    let unsaid: Vec<String> = holding
        .iter()
        .flat_map(|(token, required)| {
            required
                .iter()
                .filter(|key| !essay.contains(&format!("`{key}`")))
                .map(move |key| format!("{token}.{key}"))
        })
        .collect();
    assert!(
        unsaid.is_empty(),
        "the essay teaches the floor rule and does not name the keys it holds a thing to, so a          session learns the rule and cannot learn what it applies to: {unsaid:?}"
    );
}

/// **A thing's fields are its WRITES, and the served text says so.**
///
/// The fold is over every write on a thing, the newest write of each key
/// winning, and a write that takes a key off takes it off the thing. A text
/// saying a thing's fields are the fields of every RECORD about it, folded
/// together — the union of its records' keys — describes a different store. An
/// agent reading that concludes that clearing a key on one record leaves the
/// key alive from another, and plans a write on that basis. The shipped contract says which system exists: two records
/// projecting `{chain_wear: 9}` and `{}`, and the thing does not hold the key
/// (`a_cleared_key_is_not_resurrected_by_an_older_record`).
///
/// `no_agent_facing_text_teaches_the_store` cannot reach this one. That guard
/// sweeps for retired WORDS, and every word in the false sentence is current —
/// what is wrong here is the claim.
#[test]
fn no_agent_facing_text_folds_the_records() {
    // A sentence that names a thing's records AND one of these is saying the
    // fold is taken over the records.
    const UNION: &[(&str, &str)] = &[
        (
            "folded",
            "the fold is over the writes, not over the records",
        ),
        (
            "together",
            "records do not combine — the newest write of a key is the whole answer for it",
        ),
        (
            "between them",
            "a key one record dropped is not held up by another record that carries it",
        ),
    ];
    // Where the model itself is taught, and where it therefore has to be right:
    // the essay a fresh session reads, the instructions a client is handed on
    // connect, and the contract of the verb that serves the fields.
    const TAUGHT_BY: &[&str] = &[
        "the orientation essay",
        "the server instructions",
        "recall's description",
    ];

    let served = everything_served();
    assert!(
        !served.is_empty(),
        "nothing was gathered, so the sweep below reads no text at all and passes on an empty \
         corpus"
    );

    let mut folding: Vec<String> = Vec::new();
    let mut taught: Vec<&str> = Vec::new();
    for (what, text) in &served {
        for sentence in sentences(text) {
            if mentions(&sentence, "record") {
                for (marker, why) in UNION {
                    if says(&sentence, marker) {
                        folding.push(format!("{what} says {marker:?} of its records — {why}"));
                    }
                }
            }
            // The true claim, in whatever words the site keeps: a fold, taken
            // over writes, the newest one winning.
            if (says(&sentence, "folded") || says(&sentence, "fold"))
                && says(&sentence, "write")
                && says(&sentence, "newest")
            {
                taught.push(what);
            }
        }
    }
    assert!(
        folding.is_empty(),
        "agent-facing text folds a thing's RECORDS. An agent reads this as the truth about what \
         it is calling, and plans a clear that the store will not honour:\n  {}",
        folding.join("\n  ")
    );
    let missing: Vec<&&str> = TAUGHT_BY.iter().filter(|w| !taught.contains(w)).collect();
    assert!(
        missing.is_empty(),
        "the false sentence is gone and nothing put the true one in its place — these teach a \
         session what a thing's fields are, so each must say the fold is over the writes with \
         the newest of each key winning: {missing:?}"
    );
}

/// What a sentence about a handle says when it promises rather than
/// describes, and what an agent does with it.
///
/// **No half of this retires once a rename verb ships.** A handle never
/// becomes permanent — that half of the claim is false on the day the verb
/// lands exactly as it is false today, so a sentence saying "permanent" or
/// "forever" of a handle is still wrong afterwards and stays checked here
/// unconditionally.
const FOREVER: &[(&str, &str)] = &[
    (
        "permanent",
        "a handle is what a thing is called, and nothing holds it still",
    ),
    (
        "permanently",
        "a handle is what a thing is called, and nothing holds it still",
    ),
    (
        "never change",
        "nothing about a handle is guaranteed to outlive the record it names",
    ),
    (
        "cannot change",
        "nothing about a handle is guaranteed to outlive the record it names",
    ),
    (
        "forever",
        "a caller told this writes handles into append-only text that nothing can repair",
    ),
    (
        "in a year",
        "advice to pick a handle that outlasts the year is the promise as an instruction, and \
         a caller acts on an instruction",
    ),
];

/// **What a rename verb's OWN description must not claim.**
///
/// Pairing "rename" with "handle" used to be forbidden outright, because no
/// verb existed and text that ran ahead of the code was the failure this
/// build had already paid for once. **That ban does not survive the verb
/// shipping**: the day a rename is real, "renaming changes the handle" is a
/// true sentence, and forbidding the pairing would forbid describing the
/// capability at all.
///
/// **What replaces it is narrower and does not expire.** A rename mechanism
/// this build actually ships follows a mention and an edge (rule 243) —
/// nothing else: free prose that never used `@kind:slug`, a merge's own
/// redirect, and every system outside jojobot are untouched by it. A
/// description claiming the move is free, automatic in full, or leaves
/// nothing for the caller to do is false regardless of which verb ships,
/// because *some* of what named the old handle is never rewritten — that is
/// the whole reason a rename needed this protection in the first place.
const OVERPROMISED: &[(&str, &str)] = &[
    (
        "at no cost",
        "a rename leaves every copy outside a mention or an edge unrepaired, which is a cost \
         a caller pays",
    ),
    (
        "no cost",
        "a rename leaves every copy outside a mention or an edge unrepaired, which is a cost \
         a caller pays",
    ),
    (
        "automatically update",
        "only a mention and an edge follow a rename; free prose, a merge's redirect, and every \
         system outside jojobot do not",
    ),
    (
        "updates everything",
        "only a mention and an edge follow a rename; free prose, a merge's redirect, and every \
         system outside jojobot do not",
    ),
    (
        "nothing to update",
        "a caller who wrote a handle outside a mention has something to update, and this \
         claims otherwise",
    ),
    (
        "nothing further",
        "the same overclaim as \"nothing to update\", in the words a checklist reads",
    ),
    (
        "no further action",
        "the same overclaim as \"nothing to update\", in the words a checklist reads",
    ),
    (
        "renaming is free",
        "a caller pays for it in the copies a rename cannot repair",
    ),
];

/// **A handle is described, never promised.**
///
/// The operator settled it: a thing is referenced by an opaque id underneath,
/// so the handle can move without breaking a link (rule 205), and that id
/// stays internal — the handle remains the only name a caller sees or sends
/// (rule 209). **Neither half is built.** What ships now is the surface
/// giving up the opposite claim (rule 155): three served texts told a caller
/// a handle is permanent, and one of them was advice rather than description
/// — "choose one that will still be right in a year" tells a caller to act on
/// the promise.
///
/// **The act it produces is the damage.** A caller that reads a handle as
/// safe forever writes it as free prose that never used `@kind:slug` — the
/// one shape a rename genuinely cannot reach, since nothing marks it as a
/// link to follow. A message body, a journal beat and a chronology entry are
/// not that shape any more (rule 260): each resolves a mention the same way
/// a claim's content does, and text written before that was true was
/// migrated once. Each day the promise stands, more of a handle is written
/// as bare prose instead of a mention, which is the copy that stays behind.
///
/// **The reversal is asserted too, and it is not the same shape as the
/// promise.** [`OVERPROMISED`] is what a rename verb's own description must
/// not claim — the promise it replaces once the verb ships, rather than a
/// ban on the verb existing at all. The sweep forbids [`FOREVER`] and
/// [`OVERPROMISED`], and then requires the description that replaces the
/// original promise — a caller who comes away thinking a handle is disposable
/// has been told a third wrong thing.
#[test]
fn no_agent_facing_text_promises_a_permanent_handle() {
    // Where a session learns what a handle IS: the essay a fresh one reads,
    // the instructions a client gets on connect, and the argument that asks a
    // caller to choose one.
    const TAUGHT_BY: &[&str] = &[
        "the orientation essay",
        "the server instructions",
        "add_entity's argument schema",
    ];

    let served = everything_served();
    assert!(
        !served.is_empty(),
        "nothing was gathered, so the sweep below reads no text at all and passes on an empty \
         corpus"
    );

    let mut promising: Vec<String> = Vec::new();
    let mut taught: Vec<&str> = Vec::new();
    for (what, text) in &served {
        for sentence in sentences(text) {
            if let Some(problem) = handle_promise_problem(&sentence) {
                promising.push(format!("{what} {problem}"));
            }
            // The description that replaces the promise: a handle is what the
            // thing is addressed by.
            if mentions(&sentence, "handle") && says(&sentence, "addressed") {
                taught.push(what);
            }
        }
    }
    // **Every offender at once**, because this is a class and stopping at the
    // first turns one fix into a round of finding the next.
    assert!(
        promising.is_empty(),
        "agent-facing text promises something about a handle that jojobot does not implement:\n  \
         {}",
        promising.join("\n  ")
    );
    let missing: Vec<&&str> = TAUGHT_BY.iter().filter(|w| !taught.contains(w)).collect();
    assert!(
        missing.is_empty(),
        "the promise is gone and nothing says what a handle IS in its place — these teach a \
         session the model, so each must say the handle is what an entity is addressed by: \
         {missing:?}"
    );
}

/// **The second, independent half: a description of the capability must name
/// a limit on it, rather than merely avoiding [`OVERPROMISED`]'s known lies.**
///
/// [`OVERPROMISED`] forbids specific ways to over-claim — the ones somebody
/// thought to write down. **A phrase list is beaten by a sixth phrasing
/// nobody listed**, the same objection this build already has about a
/// denylist elsewhere. This is the positive form instead: rather than
/// forbidding ways to overclaim, it requires the description to say
/// something true. There is one way to satisfy "name a limit" and many ways
/// to avoid saying one, so this does not expire the way a phrase list does —
/// it stays true as more things start following a rename, because the
/// sentence has to be re-earned each time the set of what follows changes.
///
/// 🚨 **Presence is not correctness, and this checks presence only.** A
/// description could pair "rename" with a sentence naming a limit that is
/// honest-sounding, incomplete, or stale, and this would still pass —
/// nothing here reads a sentence for whether the limit it names is the real
/// one. That is why [`OVERPROMISED`] is not retired in its favour: it
/// catches specific known lies this cannot see past, and this catches
/// silence that a phrase list cannot require against.
///
/// ⚠️ **Scoped to text that actually describes the capability.** Written
/// while nothing did — no served text paired "rename" with "handle" yet,
/// which is the pairing
/// [`no_agent_facing_text_promises_a_permanent_handle`] stopped forbidding —
/// checked once the scope became non-empty rather than skipped outright, so
/// it started asking a real question the day `rename_entity`'s own
/// description shipped without anybody having to remember to come back and
/// re-arm it. It now sweeps that description and its argument schema, and
/// stays armed for whatever served text describes the capability next.
#[test]
fn a_description_of_renaming_names_what_does_not_follow_it() {
    let served = everything_served();
    let unlimited = descriptions_missing_a_named_limit(&served);
    assert!(
        unlimited.is_empty(),
        "this describes renaming a handle and names no limit on what follows it — pair the \
         description with a sentence naming something that does not move: {unlimited:?}",
    );
}

/// **A sentence naming a limit**: something that stays where it was, or a
/// copy no mechanism reaches. Deliberately not the same list as
/// [`OVERPROMISED`] — that list names lies; this one names the true
/// sentence a description needs somewhere in it, and the two are checked
/// independently on purpose.
const NAMES_A_LIMIT: &[&str] = &[
    "does not",
    "is not",
    "are not",
    "elsewhere",
    "already written",
    "outside a mention",
];

/// The sources that describe renaming a handle but name no limit on it —
/// split out so [`a_description_of_renaming_names_what_does_not_follow_it`]
/// can ALSO be pinned against sources written for a test, for the reason
/// [`handle_promise_problem`] was split out: proving the logic against a
/// sentence written for the test is what makes the case mean something
/// beyond "the one real description I already wrote happens to pass."
fn descriptions_missing_a_named_limit(served: &[(String, String)]) -> Vec<&str> {
    let mut describing: Vec<&str> = Vec::new();
    let mut limited: Vec<&str> = Vec::new();
    for (what, text) in served {
        for sentence in sentences(text) {
            if mentions(&sentence, "handle") && says(&sentence, "rename") {
                describing.push(what);
            }
            if NAMES_A_LIMIT.iter().any(|marker| says(&sentence, marker)) {
                limited.push(what);
            }
        }
    }
    describing
        .into_iter()
        .filter(|what| !limited.contains(what))
        .collect()
}

/// 🚨 **A description that pairs "rename" with "handle" and says nothing
/// about a limit is exactly what this is supposed to catch.**
#[test]
fn an_unlimited_rename_description_is_caught() {
    let served = [(
        "a hypothetical rename tool".to_string(),
        "this tool lets you rename the handle an entity answers to.".to_string(),
    )];
    let unlimited = descriptions_missing_a_named_limit(&served);
    assert_eq!(
        unlimited,
        vec!["a hypothetical rename tool"],
        "a description with no named limit was not caught",
    );
}

/// 🚨 **The same description, with a limit named somewhere in it, must read
/// clean** — this is the positive the check exists to require, not merely
/// the negative it exists to catch.
#[test]
fn a_limited_rename_description_reads_clean() {
    let served = [(
        "a hypothetical rename tool".to_string(),
        "this tool lets you rename the handle an entity answers to. text written before the \
         rename, outside a mention, is not rewritten."
            .to_string(),
    )];
    let unlimited = descriptions_missing_a_named_limit(&served);
    assert!(
        unlimited.is_empty(),
        "a description naming a real limit was still flagged: {unlimited:?}",
    );
}

/// **One sentence's verdict, split out so it can be pinned against a sentence
/// written for a test rather than only against whatever the corpus holds
/// today.**
///
/// Written while the corpus held nothing about a rename — no verb existed —
/// so a case that only swept the real corpus would have proven this logic
/// works on an empty question. The two tests below, over sentences written
/// for this file rather than gathered from it, are what actually proved the
/// shape `rename_entity`'s own description needed to pass, and what keep
/// proving it as the description's wording changes.
fn handle_promise_problem(sentence: &str) -> Option<String> {
    if !mentions(sentence, "handle") {
        return None;
    }
    for (marker, why) in FOREVER {
        if says(sentence, marker) {
            return Some(format!("says {marker:?} of a handle — {why}"));
        }
    }
    for (marker, why) in OVERPROMISED {
        if says(sentence, marker) {
            return Some(format!("says {marker:?} — {why}"));
        }
    }
    None
}

/// 🚨 **The capability a rename verb ships is no longer forbidden to
/// describe.** An honest sentence pairing "rename" with "handle" — the
/// pairing the old, retired ban forbade outright — must read clean, or this
/// guard would refuse the very verb it exists to let ship truthfully.
#[test]
fn a_capable_descriptions_pairing_is_not_the_overpromise_it_replaced() {
    for honest in [
        "renaming a thing changes the handle it answers to.",
        "the handle can be renamed; a mention written as @kind:slug keeps finding it.",
        "this verb lets you rename the handle an entity answers to.",
    ] {
        assert_eq!(
            handle_promise_problem(honest),
            None,
            "an honest description of the rename capability was refused: {honest:?}",
        );
    }
}

/// 🚨 **What the honest description above must not slide into.** The verb is
/// real; what it does not do — repair a copy that sits outside a mention or
/// an edge — is exactly what these claim it does, and the claim is false
/// whichever verb ships it.
#[test]
fn an_overpromised_rename_is_still_forbidden() {
    for overpromised in [
        "renaming a handle updates everything at no cost.",
        "renaming the handle automatically updates every reference.",
        "once you rename a handle there is nothing further to do.",
        "a handle's renaming is free and needs no further action from the caller.",
    ] {
        assert!(
            handle_promise_problem(overpromised).is_some(),
            "an over-promised description of the rename capability was not refused: \
             {overpromised:?}",
        );
    }
}

/// **The fields are what a thing HOLDS. The kind is what it IS.**
///
/// A thing's fields and a thing's identity were the same sentence while a
/// schema and a kind were the same idea. They are not: a kind is the schema
/// that is also identity (rule 213), and a declared type is a shape that any
/// number of kinds can answer. **The gigs story is the proof and it is in this
/// repository**: a jukebox carries the key a gig carries, honestly, and only
/// the kind separates the two. If fields were identity that jukebox would be a
/// gig.
///
/// So text telling a caller that the fields are what the thing IS teaches an
/// identity the store does not keep — and a caller who believes it writes a
/// key to change what something is, which is a write that will never do that.
///
/// **Asserted as the claim rather than as a sentence**: a sentence that speaks
/// of fields and calls them what the thing is, in whatever words, and the
/// positive that has to replace it — the fields are what the thing holds.
#[test]
fn no_agent_facing_text_makes_the_fields_the_identity() {
    // A sentence that names the fields AND says one of these is saying the
    // fields are the thing.
    const IDENTITY: &[(&str, &str)] = &[
        (
            "what the thing is",
            "a kind is what a thing is; the fields are what it holds",
        ),
        (
            "what a thing is",
            "a kind is what a thing is; the fields are what it holds",
        ),
    ];
    // Where the model is taught, so where it has to be right.
    const TAUGHT_BY: &[&str] = &["recall's description"];

    let served = everything_served();
    assert!(
        !served.is_empty(),
        "nothing was gathered, so the sweep below reads no text at all and passes on an empty \
         corpus"
    );

    let mut identifying: Vec<String> = Vec::new();
    let mut taught: Vec<&str> = Vec::new();
    for (what, text) in &served {
        for sentence in sentences(text) {
            if !mentions(&sentence, "field") {
                continue;
            }
            for (marker, why) in IDENTITY {
                if says(&sentence, marker) {
                    identifying.push(format!("{what} calls the fields {marker:?} — {why}"));
                }
            }
            // **The whole claim, not the word.** `recall` says "the value it
            // holds" about a key several sentences away, so a check for the
            // verb alone passes on a description that never says what the
            // fields are — which is what the sabotage behind this case showed.
            if mentions(&sentence, "thing") && says(&sentence, "holds") {
                taught.push(what);
            }
        }
    }
    assert!(
        identifying.is_empty(),
        "agent-facing text makes a thing's fields its identity. A caller reads this as the truth \
         about what it is calling, and writes a key to change what something is:\n  {}",
        identifying.join("\n  ")
    );
    let missing: Vec<&&str> = TAUGHT_BY.iter().filter(|w| !taught.contains(w)).collect();
    assert!(
        missing.is_empty(),
        "the false claim is gone and nothing put the true one in its place — this teaches a \
         session what fields are, so it must say they are what the thing HOLDS: {missing:?}"
    );
}

/// **A type is matched against a THING, and `declare_type`'s text says so.**
///
/// A separate claim from the fold above, and it needed its own assertion: that
/// one catches text saying a thing's fields are its RECORDS' fields combined,
/// by the words of combining — `folded`, `together`, `between them`. This one
/// is the other half of the same fork, and shares none of those words: text
/// saying the unit a type is asked of is a RECORD, when it is the thing. An
/// agent reading it calls `search answers_type` expecting the records that
/// carry the keys, gets things, and reads a partial match as a missing record.
///
/// **Scoped to this verb's text, because `record` is the right word elsewhere
/// and a corpus-wide sweep for it would be unusable.** `recall` filters that
/// really are record-scoped say so on purpose — `fields` asks for objects
/// holding ONE record carrying these keys, an inbound key walk returns every
/// record using that key — and a guard that reported those would be deleted to
/// get the suite green. `declare_type` is the one verb whose whole subject is
/// type matching, and matching there is over the thing's folded fields
/// (`Graph::answers`), so no sentence of its served text has a record to name.
#[test]
fn declare_types_text_scopes_a_type_to_the_thing() {
    // Its description and its argument schema, addressed exactly as the
    // gathering names them — both, because the false sentence sat in both.
    const SITES: &[&str] = &[
        "declare_type's description",
        "declare_type's argument schema",
    ];

    let served = agent_facing_text();
    let mut scoped_to_a_record: Vec<String> = Vec::new();
    let mut read: Vec<&str> = Vec::new();
    for site in SITES {
        let (_, text) = served
            .iter()
            .find(|(what, _)| what == site)
            .unwrap_or_else(|| panic!("the gathering serves {site}, or this test reads nothing"));
        assert!(
            !text.is_empty(),
            "{site} is served empty, so the sweep below looks at no text at all"
        );
        read.push(site);
        for sentence in sentences(text) {
            if mentions(&sentence, "record") {
                scoped_to_a_record.push(format!("{site}: {}", sentence.trim()));
            }
        }
    }
    assert_eq!(read, SITES, "both sites are read, not one");
    assert!(
        scoped_to_a_record.is_empty(),
        "declare_type's text names a RECORD as what a type is matched against. A thing answers a \
         type over every write on it, folded — so the thing is the unit here, and a writer told \
         otherwise queries for the wrong shape:\n  {}",
        scoped_to_a_record.join("\n  ")
    );
    // The word is gone; the claim has to be there in its place, or deleting the
    // sentence passes as well as fixing it.
    let (_, description) = served
        .iter()
        .find(|(what, _)| what == SITES[0])
        .expect("the description was read above");
    assert!(
        sentences(description)
            .iter()
            .any(|s| mentions(s, "thing") && mentions(s, "key")),
        "nothing in declare_type's description says what carries a type's keys: {description}"
    );
}

/// **A shape the code can write is a shape the surface names.**
///
/// A session forms its world model from the served text alone. An edge shape
/// that exists in `EdgeShape::ALL` and appears in no description is a
/// capability nobody can reach on purpose: the session picks the nearest shape
/// it was told about, which for an unrecorded link is `about` — and that
/// launders an admission into a claim (rule 98).
///
/// Driven by `ALL`, so a sixth shape fails here the day it is added rather
/// than the day somebody notices it is missing from the essay.
#[test]
fn every_edge_shape_is_named_on_the_surface() {
    // **The needle is the token in its served register, `` `shape` ``, never
    // the bare word.** Three of the five tokens are ordinary English — the
    // essay calls a session "the unit of connection", and `about` and
    // `location` appear in running prose everywhere — so a bare-substring
    // needle passes on text that teaches the caller nothing about edges. The
    // backticked form is how every shape is offered to a caller, and it is
    // what a caller copies.
    let mut unnamed: Vec<String> = Vec::new();
    for shape in EdgeShape::ALL {
        let needle = format!("`{}`", shape.as_token());
        let named = agent_facing_text()
            .iter()
            .any(|(_, text)| text.to_lowercase().contains(&needle));
        if !named {
            unnamed.push(needle);
        }
    }
    assert!(
        unnamed.is_empty(),
        "these edge shapes are built and invisible — a caller cannot ask for what nothing names, \
         so the shape it reaches for instead says something the record does not: {unnamed:?}"
    );
}

/// **A memory write carries an identity, or it does not land.**
///
/// Every tool schema says the `sid` rides every call because it is what tells
/// jojobot which bot is asking, and rule 19 says the same. The memory writes
/// accepted `None` anyway and stored the write unattributed: `attributable`
/// resolves through `caller`, which answers Ok for a missing sid. A vanilla
/// session found this by trying, because nothing said it was allowed and
/// nothing said what it cost.
///
/// It sits under the trust machinery rather than beside it. `provenance`
/// answers who BACKS a claim; this is the separate question of who WROTE it,
/// and the answer could be nobody.
///
/// **There is no exemption, `add_entity` included.** The bootstrap loop that
/// once made one necessary is gone: every jojobot arrives with its default
/// identity, so the state where no bot exists to create the first bot is not
/// reachable.
///
/// **Reads are deliberately not here.** Rule 19 wants the sid on a read so
/// jojobot knows who is asking, and a read with no sid stays legal and
/// unattributed — it changes nothing, so there is nothing to attribute.
#[cfg(test)]
mod a_write_needs_an_identity {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::memory::{
        AddEntityArgs, CaptureArgs, ListEntitiesArgs, RetractArgs, SetCharterArgs, UpdateEntityArgs,
    };
    use rmcp::handler::server::wrapper::Parameters;

    /// What an anonymous write must come back with: a blocked answer naming the
    /// door, never a bare error and never a silent success (rule 68).
    fn refused(result: &CallToolResult, verb: &str) {
        let body = json_of(result);
        assert_eq!(
            body["status"], "blocked",
            "{verb} without a sid must be blocked, not an error and not a success: {body}"
        );
        assert_eq!(body["wrote"], false, "{verb} must not have written: {body}");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap_or_default()
                .contains("start_here"),
            "{verb} must name the door a caller who has never booted can walk through: {body}"
        );
    }

    #[tokio::test]
    async fn every_memory_write_refuses_an_anonymous_caller() {
        let jojobot = handler();

        // The positive first, and everything below leans on it: with an
        // identity these same writes land. Without it the refusals could be
        // any other failure wearing the same shape.
        let sid = writing_as(&jojobot);
        let made = jojobot
            .add_entity(Parameters(add_args("place", "springfield", "Springfield")))
            .await
            .expect("an identified add_entity answers");
        assert_ne!(json_of(&made)["status"], "blocked", "{:?}", json_of(&made));
        let captured = jojobot
            .capture(Parameters(capture_args(
                "place:springfield",
                "has a water tower",
            )))
            .await
            .expect("an identified capture answers");
        let address = address_of(&json_of(&captured));

        // **Retract needs an input it would otherwise accept**, so that the arm
        // below refuses for one reason and a deleted gate cannot hide behind a
        // second one. A record nothing has taken back is such an input, and
        // identity is then the only thing left to refuse it for.
        let second = jojobot
            .capture(Parameters(capture_args(
                "place:springfield",
                "the inspector came by",
            )))
            .await
            .expect("a second identified capture answers");
        let second_address = address_of(&json_of(&second));

        // …and now the same writes with nobody behind them. **Every one is
        // built explicitly rather than through a builder**, because the
        // builders carry the fixture identity — reaching for one here is how
        // the first version of this test came to pass while proving nothing.
        refused(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: None,
                    ..add_args("place", "shelbyville", "Shelbyville")
                }))
                .await
                .expect("add_entity answers"),
            "add_entity",
        );
        refused(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: None,
                    ..capture_args("place:springfield", "has a monorail")
                }))
                .await
                .expect("capture answers"),
            "capture",
        );
        refused(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: None,
                    ..update_args(&address)
                }))
                .await
                .expect("update_fact answers"),
            "update_fact",
        );
        refused(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    handle: "place:springfield".into(),
                    name: Some("Springfield Renamed".into()),
                    aliases: None,
                    source: None,
                    crm: None,
                    override_token: None,
                    sid: None,
                }))
                .await
                .expect("update_entity answers"),
            "update_entity",
        );
        refused(
            &jojobot
                .retract(Parameters(RetractArgs {
                    address: second_address.clone(),
                    reason: None,
                    sid: None,
                    recorded_at: None,
                }))
                .await
                .expect("retract answers"),
            "retract",
        );
        refused(
            &jojobot
                .set_charter(Parameters(SetCharterArgs {
                    bot: "bot:otto".into(),
                    prose: "a charter".into(),
                    sid: None,
                }))
                .await
                .expect("set_charter answers"),
            "set_charter",
        );

        // **The refusals wrote nothing**, which is the half a status check
        // cannot see — paired against the identified writes above, which are.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("place".into()),
                    parent: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("list_entities answers"),
        )
        .to_string();
        assert!(
            listed.contains("place:springfield"),
            "the identified write is on the board: {listed}"
        );
        assert!(
            !listed.contains("place:shelbyville"),
            "the anonymous write must have left nothing behind: {listed}"
        );
    }

    /// **Every unidentified write refuses through ONE constructor.**
    ///
    /// The refusal has to separate two problems a caller cannot tell apart —
    /// "you have not booted", which `start_here` fixes, and "the session world
    /// is down", which nothing the caller does fixes. That separation lives in
    /// the wording, so a test cannot hold it without pinning our own prose,
    /// and a test that pins prose goes red when somebody improves a sentence.
    ///
    /// What IS structural is that there is one source for it. Pin that: a
    /// verb's refusal is byte-identical to [`session_unbound`], so no verb can
    /// grow a refusal of its own that says less. Improving the sentence moves
    /// both sides at once, which is the point.
    #[tokio::test]
    async fn an_unidentified_write_refuses_through_the_one_constructor() {
        let jojobot = handler();
        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: None,
                    ..capture_args("place:springfield", "something")
                }))
                .await
                .expect("capture answers"),
        );
        assert_eq!(
            body,
            json_of(&crate::caller::session_unbound()),
            "a verb must refuse an unidentified write through the shared constructor: {body}"
        );
    }

    /// **Reads stay open, and this is the pair to the test above rather than an
    /// afterthought.** Rule 19 asks for the sid on a read so jojobot knows who
    /// is asking, not so it can refuse.
    #[tokio::test]
    async fn a_read_without_an_identity_is_still_answered() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("place", "springfield", "Springfield")))
            .await
            .expect("setup");

        let body = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("place".into()),
                    parent: None,
                    sid: None,
                }))
                .await
                .expect("an anonymous read answers"),
        );
        assert_ne!(
            body["status"], "blocked",
            "an anonymous read must still be served: {body}"
        );
        assert!(
            body.to_string().contains("place:springfield"),
            "…and must actually carry the answer: {body}"
        );
    }
}

/// **A description never offers a parameter its tool does not take.**
///
/// A caller that trusts the prose sends a call the schema rejects, and a
/// malformed call is the one shape that comes back as a raw protocol error
/// rather than as a blocked answer naming a way forward. So a description that
/// drifts away from its schema switches rule 68's guarantee off for that call,
/// silently and only for the callers who believed it.
///
/// **The form is what carries the promise, and it is measured rather than
/// assumed.** Parameter names here are ordinary English words — `name`,
/// `source`, `body`, `status`, `shape`, `story` — so looking for a bare word
/// flags forty sentences that promise nothing. What marks an identifier as a
/// field a caller passes is the backticks around it, which is the convention
/// every description on this surface already uses.
///
/// **A property, never a list**: the vocabulary is read off the schemas
/// themselves, so a parameter added or dropped tomorrow is covered without
/// anybody remembering this test exists.
///
/// **What it does NOT catch**, on the record before anybody treats it as
/// covering the class: a promise written as plain prose rather than as an
/// identifier, and the reverse direction — a restriction the schema enforces
/// and the description omits. That second one is a different check and cannot
/// be this one.
#[test]
fn no_description_offers_a_parameter_its_schema_does_not_have() {
    let tools = Jojobot::tool_router().list_all();
    let parameters = |tool: &rmcp::model::Tool| -> std::collections::BTreeSet<String> {
        tool.input_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|properties| properties.keys().cloned().collect())
            .unwrap_or_default()
    };
    // Every name that IS a parameter somewhere on this surface. A word that
    // names no parameter anywhere cannot be a promise of one.
    let vocabulary: std::collections::BTreeSet<String> =
        tools.iter().flat_map(&parameters).collect();
    assert!(
        vocabulary.contains("sid"),
        "the vocabulary is read off the schemas, and an empty one would pass this vacuously"
    );

    for tool in &tools {
        let mine = parameters(tool);
        let description = tool.description.as_deref().unwrap_or_default();
        let offered: Vec<&String> = vocabulary
            .iter()
            .filter(|name| !mine.contains(*name))
            .filter(|name| description.contains(&format!("`{name}`")))
            .collect();
        assert!(
            offered.is_empty(),
            "{}'s description offers {offered:?}, which its schema does not carry — a caller \
             that believes it sends a call that cannot be answered. Its parameters are {mine:?}",
            tool.name,
        );
    }
}

/// **A validated constraint is stated where a caller reads it, not only where
/// it is refused.**
///
/// `subject` rides in the record and in the listing a reader sees before
/// opening anything, so it takes one plain line and refuses markup. Nothing
/// said so until the call was refused, and the parameter's own prose read as
/// style guidance — so a caller naming a tool or a field reached for backticks,
/// which every other prose surface here accepts, and paid a round-trip carrying
/// the whole body to find out.
///
/// **Both halves, because either alone is a lie of a different kind**: the
/// refusal happens through the verb a caller uses, and the constraint is on the
/// parameter a caller reads. A description stating a rule nothing enforces is
/// as wrong as a rule nothing states.
#[tokio::test]
async fn the_subject_constraint_is_refused_by_the_verb_and_stated_on_the_parameter() {
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use rmcp::handler::server::wrapper::Parameters;

    let jojobot = handler();
    make_box(&jojobot, "dev").await;
    let sid = as_bot(&jojobot, "gamma");

    // **The refusal is a blocked answer, not a raw protocol error** (rule 68):
    // a caller that cannot branch on the answer cannot act on it, and the
    // model on the other end gets a failure where it should get a next move.
    let refused = blocked(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                to: "dev".into(),
                body: "the shipment landed".into(),
                subject: Some("what `post_message` does with a title".into()),
                in_reply_to: None,
                sid: sid.clone(),
            }))
            .await
            .expect("a subject carrying markup is an answer, not a protocol failure"),
    );
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a refusal says what to do instead");
    assert!(
        how.contains("subject"),
        "the refusal names the field it is about: {how}"
    );

    // **A second fault, through the same answer.** A subject can be refused
    // for more than one reason, and a refusal wired to one of them would send
    // the other back down the channel this card exists to close.
    let too_long = blocked(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                to: "dev".into(),
                body: "the shipment landed".into(),
                subject: Some("w".repeat(200)),
                in_reply_to: None,
                sid: sid.clone(),
            }))
            .await
            .expect("an over-long subject is an answer too"),
    );
    assert!(
        too_long["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("subject")),
        "{too_long}"
    );

    // …and the same message lands once the subject is one plain line, so the
    // refusal above is about the subject rather than about anything else in
    // the call.
    let posted = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                to: "dev".into(),
                body: "the shipment landed".into(),
                subject: Some("what post_message does with a title".into()),
                in_reply_to: None,
                sid,
            }))
            .await
            .expect("post ok"),
    );
    assert_eq!(posted["subject"], "what post_message does with a title");

    // The constraint is on the parameter, where it is read before the call.
    let tools = Jojobot::tool_router().list_all();
    let post = tools
        .iter()
        .find(|t| t.name == "post_message")
        .expect("post_message is a tool");
    let subject = post
        .input_schema
        .get("properties")
        .and_then(|p| p.get("subject"))
        .and_then(|s| s.get("description"))
        .and_then(|d| d.as_str())
        .expect("the subject parameter is described");
    assert!(
        subject.contains("backtick"),
        "the trap a caller falls into must be named where they read: {subject}"
    );
    assert!(
        subject.contains("nothing is written"),
        "…and it must read as a validated contract rather than as advice: {subject}"
    );
}

/// **Every address this build supplies is named by its handle, in the source.**
///
/// The bright-line gate is an allowlist over handle-shaped text — a kind, a
/// colon and a slug — in the workspace sources, and it reports every handle
/// that is not on the fictional roster. An address written as a kind and a
/// bare slug carries no handle anywhere, so the gate finds nothing to compare
/// and the slug reaches the repository unchecked.
///
/// **What changes is the shape the build writes, never the scanner** (rule
/// 45). A handle in the source is a handle the gate already reads, and a
/// scanner taught to guess which bare string is a slug would make the crisp
/// half of that check fuzzy.
///
/// **Asked of every address, never of the records alone.** A record and a
/// paragraph are supplied differently and addressed identically, and the blind
/// spot is the address rather than what sits at it — so a check scoped to one
/// of the two leaves the other open, which is what it did (rule 234).
///
/// **This is one half of the bright line and the roster suite is the other.**
/// This half says a supplied handle is written down;
/// `every_handle_in_the_workspace_is_on_the_fictional_roster` says a written
/// handle is on the roster. An off-roster shipped slug is caught by the pair
/// and by neither alone. The split is forced: this crate is the only one that
/// can see `provisions()`, and the roster suite is in a crate this one depends
/// on.
///
/// **The count is asserted before the loop**, because a build that supplies
/// nothing satisfies the loop without reading anything.
#[test]
fn every_address_the_build_supplies_is_named_by_its_handle_in_the_source() {
    let source = shipped_source();
    let supplied = provisions();
    let handles: Vec<String> = supplied.addresses().map(|at| at.to_string()).collect();

    assert!(
        !handles.is_empty(),
        "this build supplies nothing at all, so the check below reads nothing"
    );
    let unwritten: Vec<&String> = handles
        .iter()
        .filter(|handle| !source.contains(handle.as_str()))
        .collect();
    assert!(
        unwritten.is_empty(),
        "these addresses are written as a kind and a bare slug, so no handle exists for \
         the roster gate to read — name each one by its handle instead:\n{unwritten:?}"
    );
}

/// **Every text a caller is served that describes how jojobot works**: the
/// server instructions, every tool's description and input schema, the whole
/// orientation essay, and every skill. Counted by what is in it, so a case that
/// reads it fails when the corpus comes back empty.
fn every_served_text() -> Vec<(String, String)> {
    let mut texts = vec![
        ("the served instructions".to_string(), crate::instructions()),
        (
            "the orientation essay".to_string(),
            crate::orientation::essay::orientation(),
        ),
    ];
    for tool in Jojobot::tool_router().list_all() {
        let name = tool.name.to_string();
        texts.push((
            format!("{name}'s description"),
            tool.description.as_deref().unwrap_or_default().to_string(),
        ));
        texts.push((
            format!("{name}'s input schema"),
            serde_json::to_string(&tool.input_schema).expect("the schema serializes"),
        ));
    }
    for skill in crate::orientation::skills::SKILLS {
        texts.push((
            format!("the {} skill", skill.name),
            format!("{} {}", skill.when_to_use, skill.body),
        ));
    }
    texts
}

/// **No served text describes the provenance default as the weakest backing.**
/// A claim nobody confirmed is labelled a hypothesis, and "weakest backing" read
/// cold describes jojobot as a weak place to keep things: a model kept its
/// rulings in a markdown file instead and cited that sentence. The needle is the
/// word, over every text a caller is served, and the corpus is counted first so
/// the case cannot pass over nothing.
#[test]
fn no_served_text_calls_the_provenance_default_the_weakest() {
    let texts = every_served_text();
    assert!(
        texts.len() > 40,
        "the corpus of served texts is too small to be the served surface: {}",
        texts.len()
    );
    for (place, text) in &texts {
        assert!(
            !text.to_lowercase().contains("weakest"),
            "{place} describes the provenance default as the weakest backing"
        );
    }
}

/// **The hypothesis labelling is still taught, and so is where durable things
/// are kept.** The pair for the case above, which passes on an empty corpus of
/// teaching: the instructions say a claim nobody confirmed is labelled a
/// hypothesis until the user confirms it, the essay says the default reads back
/// as one, and the instructions say jojobot is where a fact a later session has
/// to find is kept.
#[test]
fn the_hypothesis_labelling_and_the_place_for_durable_things_are_still_taught() {
    let instructions = crate::instructions();
    assert!(
        instructions.contains("hypothesis") && instructions.contains("confirms"),
        "the instructions do not say what an unconfirmed claim is labelled: {instructions}"
    );
    assert!(
        crate::orientation::essay::orientation().contains("hypothesis"),
        "the essay does not teach the hypothesis labelling"
    );
    assert!(
        instructions.contains("durable"),
        "the instructions do not say jojobot is where durable things are kept"
    );
}

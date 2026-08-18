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
use crate::orientation::essay::ORIENTATION;

/// Every shipped `.rs` file in this crate, with its test half cut off.
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
fn shipped_source() -> String {
    fn walk(dir: &std::path::Path, gated: &mut Vec<std::path::PathBuf>, out: &mut Vec<String>) {
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
            out.push(shipped_half(&text));
        }
    }

    /// A file's shipped half: everything before it starts declaring tests
    /// INLINE.
    ///
    /// **Not simply "up to the first `#[cfg(test)]`".** That attribute wears two
    /// meanings here and only one of them ends the shipped code: `#[cfg(test)]
    /// mod tests {` opens the scaffolding, while `#[cfg(test)] mod surface;`
    /// merely gates a child module and can sit at the very top of a file, above
    /// everything this scan exists to count. Cutting at that one silently
    /// reduced `lib.rs` to its imports.
    fn shipped_half(text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let end = lines.iter().enumerate().position(|(n, line)| {
            line.trim_start().starts_with("#[cfg(test)]")
                && !lines
                    .get(n + 1)
                    .is_some_and(|next| next.trim().ends_with(';') && next.contains("mod "))
        });
        match end {
            Some(end) => lines[..end].join("\n"),
            None => text.to_string(),
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
    files.concat()
}

/// **Every kind the store accepts is a kind the surface lists.**
///
/// The surface listed eight and the enum has nine: `bot` was accepted and
/// undocumented, so a caller reading it believed an identity had to be made
/// some other way — and there is no other way, because nothing about a bot is
/// compiled in.
///
/// **The list lives in ONE place**, the `kind` argument, which is what makes
/// this checkable at all: the description used to carry a second copy, and two
/// copies of one list is how the first one goes stale unnoticed. This pins the
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

    for kind in EntityKind::ALL {
        let token = kind.as_token();
        assert!(
            listed.contains(&format!("`{token}`")),
            "the `kind` argument does not list `{token}`, which the store accepts: {listed}"
        );
    }
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
    // and `start_here`'s snapshot (every box on the board by name). The
    // three session verbs are journal, amend_journal and wrap_session (there
    // is deliberately no start_session — booting an identity IS starting its
    // session); the rest are Memory's.
    assert_eq!(
        names,
        [
            "add_entity",
            "amend_journal",
            "capture",
            "declare_type",
            "journal",
            "list_entities",
            "list_sent",
            "mark_processed",
            "ping",
            "post_message",
            "read_mailbox",
            "read_message",
            "recall",
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
        // Defined once, read once. A door that reimplemented the answer
        // rather than calling `orient` would still have to reach for the
        // essay, and this is where that shows.
        ("readers of the orientation essay", "ORIENTATION", 2),
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
/// and handles the answer exactly wrong. The unification rider fixed four
/// of these descriptions and missed `set_charter`, which went on promising
/// "an error naming the nearest handles" while the code returned blocked —
/// so the whole class is pinned here rather than one more instance of it.
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
            "{name} still promises an error for a miss it no longer errors on: {description}"
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
/// `sender` left `PostMessageArgs` when it became derived from the `sid`,
/// and three texts went on describing it. Since the argument gate — see
/// [`crate::arguments`] — such a sentence no longer costs a lie, it costs
/// the call: `sender` is a top-level argument `post_message` does not
/// implement, so a caller following the sentence gets `blocked` before
/// dispatch and the message it meant to leave is never written. The text is
/// what breaks the call, and the refusal contradicting the surface's own
/// words is the caller's only clue.
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
        !ORIENTATION.contains("`sender`"),
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
    assert!(
        ORIENTATION.contains("`sid` you carry"),
        "the essay must name what makes two connections one session"
    );
    assert!(
        !ORIENTATION.contains("the identity that booted them"),
        "the essay still says a connection carries the identity, which nothing does"
    );
    assert!(
        ORIENTATION.contains("Reads are not journalled"),
        "the essay must say which calls jojobot beats about"
    );
    assert!(
        !ORIENTATION.contains("one per verb class you use"),
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
    // The two endings, and that they are a choice about the WORK.
    assert!(
        ORIENTATION.contains("CLEAR AND RESUME"),
        "the continuing case is named"
    );
    assert!(
        ORIENTATION.contains("do NOT wrap"),
        "…and says which verb NOT to reach for, since wrapping is the tempting default"
    );
    assert!(
        ORIENTATION.contains("resume note"),
        "…and names the thing you leave for whoever picks it up"
    );
    assert!(
        ORIENTATION.contains("exception to journal leanness"),
        "…and exempts it from the leanness rule, or the rule suppresses it"
    );
    // **`abandoned` is not a failure**, and the essay must not teach it as
    // one: it means the run was never wrapped up, and picking one back up
    // is ordinary rather than recovery. What the essay still has to draw is
    // the distinction that survives — a run that ENDED against one that
    // merely STOPPED.
    assert!(
        ORIENTATION.contains("not a failure"),
        "abandoned is a run nobody wrapped up, not a run that broke"
    );
    assert!(
        !ORIENTATION.contains("failure path"),
        "…so the old framing must be gone, not merely balanced by the new one"
    );
    assert!(
        ORIENTATION.contains("merely stopped"),
        "…and the distinction that does survive is ended against stopped"
    );

    // The own-box norm, and the affordance that tempted otherwise. It is no
    // longer a norm a caller can decline — the read side takes no box name —
    // so what the essay owes is that the reader knows which box opens.
    assert!(ORIENTATION.contains("read your OWN mailbox"));
    assert!(
        ORIENTATION.contains("no name to pass"),
        "the essay has to say the choice is gone, not merely discouraged"
    );
    assert!(
        ORIENTATION.contains("not an invitation"),
        "the flat listing is what posed the access question, so it is what gets answered"
    );
    assert!(
        ORIENTATION.contains("post_message"),
        "…and there is a sanctioned way to reach another box: write to it"
    );
}

/// **Declaring a type refuses one move, and the essay is where a session
/// learns it.**
///
/// The essay taught that declaring "refuses nothing", which was true until the
/// floor rule shipped: a write that would drop a thing below a type it already
/// fits comes back blocked. Every session reads this text at boot and plans
/// against it, so a session holding that sentence meets a refusal it was told
/// could not happen — and the way out (put the key back, or write the value
/// where the type can still see it) is not something it can derive from a rule
/// it does not know exists.
///
/// **Both halves, or either is worthless**: the rule is exercised through the
/// guard every adapter runs, and the essay is read for what it says about it.
/// Prose pinned against no mechanism goes stale the same way this sentence
/// did.
#[test]
fn the_orientation_states_the_one_move_declaring_a_type_refuses() {
    use jojobot_domain::memory::guard_fit;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
    use std::collections::BTreeMap;

    let pet = DeclaredType::new("pet", vec![Field::new("species", ValueType::Text)]);
    let fitting: BTreeMap<String, String> = [("species".to_string(), "cat".to_string())]
        .into_iter()
        .collect();
    let refused = guard_fit(&fitting, &BTreeMap::new(), std::slice::from_ref(&pet))
        .expect_err("taking a type's key off a thing that fits it is the move that is refused");
    assert!(
        refused.to_string().contains("species"),
        "…and the refusal names the key it would lose: {refused}"
    );
    guard_fit(&BTreeMap::new(), &BTreeMap::new(), &[pet])
        .expect("a thing that fits nothing has nothing to protect, so nothing else is refused");

    // Scoped to the paragraph that teaches declaring: "refus" is a word the
    // essay spends elsewhere, on the gates, and a needle over the whole text
    // would find one of those and call it this rule.
    let taught = ORIENTATION
        .lines()
        .find(|line| line.contains("Declare a type"))
        .expect("the essay teaches declaring a type");
    assert!(
        !taught.contains("refuses nothing"),
        "the essay still says declaring refuses nothing, and a write below the floor is \
         refused: {taught}"
    );
    assert!(
        taught.contains("refus"),
        "…and it has to say so where declaring is taught, since silence reads as the old \
         claim: {taught}"
    );
    assert!(
        taught.contains("already fits"),
        "…naming the one move, which is what tells a session its own edit apart from an \
         ordinary write: {taught}"
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
/// **The answer half gathered here is one module's, and the principle above is
/// wider than the reach below.** `search`'s coverage notes are in; the
/// chronology note, the unreadable-items report, the notes on the identity
/// path, the orient notes, the mailbox note and the refusal texts are answer
/// prose that nothing here reads, so no check on this text can fail for any of
/// them. Gathering answer prose generically is its own piece of work. Until it
/// lands, read a pass here as covering the descriptions and one module.
fn agent_facing_text() -> Vec<(String, String)> {
    let mut found = vec![
        ("the orientation essay".to_string(), ORIENTATION.to_string()),
        (
            "the server instructions".to_string(),
            Jojobot::new(
                Arc::new(jojobot_domain::memory::testing::InMemoryMemory::new()),
                Arc::new(crate::memory::testing::SpySearch::default()),
                Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
                Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
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
    // enforces. `crm` itself no longer teaches a grammar — the task layer
    // decides how it addresses things
    // (`jojobot_domain::memory::validate_crm`). The rule is that no text
    // teaches JOJOBOT'S store; blanking the word here would turn a true
    // sentence false. Each entry must actually be hit — a stale exception
    // fails below, so this cannot quietly become a place to put new ones.
    const ALLOWED: &[(&str, &str)] = &[("add_entity's argument schema", "card")];

    let mut teaching: Vec<String> = Vec::new();
    let mut unused: Vec<&(&str, &str)> = ALLOWED.iter().collect();
    for (what, text) in agent_facing_text() {
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

/// **A thing's fields are its WRITES, and the served text says so.**
///
/// The fold is over every write on a thing, the newest write of each key
/// winning, and a write that takes a key off takes it off the thing. The text
/// used to say a thing's fields were the fields of every RECORD about it,
/// folded together — the union of its records' keys — and that sentence
/// describes a different store. An agent reading it concludes that clearing a
/// key on one record leaves the key alive from another, and plans a write on
/// that basis. The shipped contract says which system exists: two records
/// projecting `{chain_wear: 9}` and `{}`, and the thing does not hold the key
/// (`a_cleared_key_is_not_resurrected_by_an_older_record`).
///
/// `no_agent_facing_text_teaches_the_store` cannot reach this one. That guard
/// sweeps for retired WORDS, and every word in the false sentence is current —
/// what was retired here is the claim.
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

    let served = agent_facing_text();
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
/// safe forever writes it into places nothing can repair — a message body, a
/// journal beat, a chronology entry — every one of them append-only by
/// design. Each day the promise stands, more unrepairable copies of a handle
/// are made and the rename that is coming gets bigger.
///
/// **The reversal is asserted too, and the two are not symmetric.** No text
/// may say a handle CAN be renamed: no rename exists, and text that runs
/// ahead of the code is the failure this build has already paid for. So the
/// sweep forbids the promise AND its reversal, and then requires the
/// description that replaces them — a caller who comes away thinking a handle
/// is disposable has been told a third wrong thing.
#[test]
fn no_agent_facing_text_promises_a_permanent_handle() {
    // What a sentence about a handle says when it promises rather than
    // describes, and what an agent does with it.
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
            "advice to pick a handle that outlasts the year is the promise as an instruction, \
             and a caller acts on an instruction",
        ),
    ];
    // The reversal, checked as phrases rather than as the word "rename".
    // **Renaming is real on this surface** — `update_entity` edits what an
    // entity is CALLED, and the resemblance gate describes exactly that — so
    // forbidding the word would forbid true sentences. What is forbidden is
    // pairing the verb with the handle, which is the capability nothing
    // implements.
    const RENAMEABLE: &[&str] = &[
        "rename a handle",
        "rename the handle",
        "renaming a handle",
        "renaming the handle",
        "handle can be renamed",
        "handle can change",
        "renameable",
    ];
    // Where a session learns what a handle IS: the essay a fresh one reads,
    // the instructions a client gets on connect, and the argument that asks a
    // caller to choose one.
    const TAUGHT_BY: &[&str] = &[
        "the orientation essay",
        "the server instructions",
        "add_entity's argument schema",
    ];

    let served = agent_facing_text();
    assert!(
        !served.is_empty(),
        "nothing was gathered, so the sweep below reads no text at all and passes on an empty \
         corpus"
    );

    let mut promising: Vec<String> = Vec::new();
    let mut taught: Vec<&str> = Vec::new();
    for (what, text) in &served {
        for sentence in sentences(text) {
            if !mentions(&sentence, "handle") {
                continue;
            }
            for (marker, why) in FOREVER {
                if says(&sentence, marker) {
                    promising.push(format!("{what} says {marker:?} of a handle — {why}"));
                }
            }
            for marker in RENAMEABLE {
                if says(&sentence, marker) {
                    promising.push(format!(
                        "{what} says {marker:?} — no rename exists, and text that ships a \
                         capability before the code does sends a caller to call for it"
                    ));
                }
            }
            // The description that replaces the promise: a handle is what the
            // thing is addressed by.
            if says(&sentence, "addressed") {
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

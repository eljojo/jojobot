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
use crate::harness::*;
use crate::mailboxes::testing::*;
use crate::memory::testing::*;
use crate::orientation::essay::ORIENTATION;
use rmcp::handler::server::wrapper::Parameters;

/// **Every shipped `.rs` file in this crate, with its test half cut off.**
///
/// The constraints below are about what SHIPS, and the way they are asserted is
/// by counting occurrences in the source — so what counts as "the source" is
/// load-bearing. This used to be `include_str!("lib.rs")`, which was right while
/// the crate was one file and became a lie the moment it was not: a second door
/// added in `orientation/start_here.rs` would not have been counted, and the
/// test would have gone on passing while watching a fraction of the crate.
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
            "journal",
            "list_entities",
            "list_sent",
            "mark_processed",
            "ping",
            "post_message",
            "read_mailbox",
            "read_message",
            "recall",
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
/// **The cheap answer used to be a second verb and is now this verb's own
/// argument**, which does not retire the lesson: a caller standing at
/// `read_mailbox` still has to be told, in this description, that there is a
/// way to look without taking. It is asserted on the ARGUMENT rather than on
/// a tool name, so it cannot be satisfied by pointing somewhere else again.
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
/// and three texts went on describing it. `PostMessageArgs` does not deny
/// unknown fields, so a caller following those sentences emits a `sender`
/// that is silently dropped, then calls `list_sent` with the string it
/// invented, gets nothing, and concludes its report never arrived — which
/// is the exact failure `list_sent` exists to prevent.
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

/// **The engine names roles, never a particular working agreement.** A
/// cadence ("every 20 minutes"), a named protocol ("the round is closed"),
/// or one party's framing ("my report") is a charter's business — data in
/// the operator's own store — and compiling it in makes a user-agnostic
/// server carry one user's arrangements.
///
/// **Asserted as a property, not an enumerated denylist.** A list of
/// today's phrasings only fires on today's phrasings: "every 15 minutes"
/// and "each morning" would both sail past one. This matches the SHAPE — a
/// cadence is a count next to a unit of time — so a wording nobody
/// anticipated is caught too.
pub(crate) fn engine_generic(what: &str, prose: &str) {
    let lower = prose.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).collect();

    const UNITS: [&str; 12] = [
        "minute", "minutes", "hour", "hours", "day", "days", "week", "weeks", "morning", "evening",
        "night", "nights",
    ];
    const QUANTIFIERS: [&str; 6] = ["every", "each", "per", "twice", "once", "hourly"];

    for (i, word) in words.iter().enumerate() {
        // A cadence is a quantifier reaching a time unit within a couple of
        // words: "every 20 minutes", "each morning", "twice a day".
        if !QUANTIFIERS.contains(word) {
            continue;
        }
        if *word == "hourly" {
            panic!("{what} states a cadence ('hourly') — that belongs to a bot's charter");
        }
        let mut reach = words.iter().skip(i + 1).take(3);
        if let Some(unit) = reach.find(|w| UNITS.contains(w)) {
            panic!(
                "{what} states a cadence ('{word} … {unit}') — how often a role runs belongs \
                 to that bot's charter at seeding, not to a user-agnostic engine"
            );
        }
    }
}

/// The same property, over every tool description — which is where this
/// round's working-agreement prose actually landed. The orientation essay
/// had a gate; the descriptions had none, and they are read by exactly the
/// same audience for exactly the same purpose.
#[test]
fn no_tool_description_carries_a_working_agreement() {
    for tool in Jojobot::tool_router().list_all() {
        let description = tool.description.as_deref().unwrap_or_default();
        engine_generic(&format!("{}'s description", tool.name), description);

        // Named protocols and one party's framing: a verb's contract is
        // what it does and refuses, never who is arranged to call it.
        for borrowed in ["round-closed", "the round", "my report", "hand-off ↔"] {
            assert!(
                !description.to_lowercase().contains(borrowed),
                "{}'s description borrows a working agreement ({borrowed:?}): a description \
                 states the contract, and an arrangement between two bots is charter material",
                tool.name
            );
        }
    }
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

    engine_generic("ORIENTATION", ORIENTATION);
}

/// **Every parameter this tool takes**, nested ones included.
///
/// Not `schema["properties"]`, which is the top level only. `search`'s `edge`
/// is an object of its own carrying `shape` and `object`, and a reader that
/// stopped at the top level would call the description's perfectly correct
/// mention of `object` a defect — which is how a drift test earns its first
/// suppression and stops being believed.
fn parameters_of(tool: &rmcp::model::Tool) -> Vec<String> {
    fn walk(node: &serde_json::Value, out: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(fields) => {
                if let Some(serde_json::Value::Object(properties)) = fields.get("properties") {
                    out.extend(properties.keys().cloned());
                }
                for value in fields.values() {
                    walk(value, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let schema = serde_json::to_value(&tool.input_schema).expect("a schema serializes");
    let mut found = Vec::new();
    walk(&schema, &mut found);
    found.sort();
    found.dedup();
    found
}

/// **The tokens a text presents AS parameters**, in the two forms this prose
/// actually uses: backticked (``` `aliases` ```), and slash-enumerated
/// (`name/aliases/source/crm`).
///
/// **Both, because either alone misses real drift.** The instance this test was
/// written for — `update_entity` advertising a `mailbox` field — is in a slash
/// enumeration and wears no backticks at all, so a backtick scan passes it
/// clean. And a description that says "pass `mailbox`" wears no slashes.
///
/// It deliberately does NOT try to classify every word. A caller-facing text is
/// full of tokens that look like parameters and are not — `status: blocked` is
/// a response field, `new`/`read`/`processed` are states, `abandoned` and
/// `wrapped` are session endings — and every attempt to sort those by meaning
/// is a guess. The filter that makes this precise is applied by the caller: a
/// token only counts if it is a parameter name SOMEWHERE on this surface, which
/// is a derived set rather than a list somebody maintains.
fn named_as_a_parameter(text: &str) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    let mut inside: Option<String> = None;
    for c in text.chars() {
        match (c, &mut inside) {
            ('`', Some(_)) => named.push(inside.take().expect("inside a span")),
            ('`', None) => inside = Some(String::new()),
            (_, Some(open)) => open.push(c),
            (_, None) => {}
        }
    }
    for run in text.split(|c: char| !(c.is_ascii_lowercase() || c == '_' || c == '/')) {
        let parts: Vec<&str> = run.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() >= 2 {
            named.extend(parts.iter().map(|p| (*p).to_string()));
        }
    }
    named
}

/// **A description may not name a parameter its verb does not take — the whole
/// surface, not one verb at a time.**
///
/// The existing pins compare the SERIALIZED surface against itself; nothing
/// compared its two halves against each other, and they drift independently. A
/// caller reads the description, believes it, and emits a call the schema
/// refuses — and a malformed call is the one thing on this surface that comes
/// back as a bare protocol error instead of a blocked answer with a way
/// forward. The description is what manufactures the single shape the rule
/// against raw errors cannot cover.
///
/// **The schema's own field docs are swept too.** A doc comment on a public
/// args field is not a comment: `schemars` renders it into the JSON schema and
/// it reaches a caller exactly as a description does.
#[test]
fn no_description_names_a_parameter_its_schema_does_not_have() {
    let tools = Jojobot::tool_router().list_all();
    let mut parameter_shaped: Vec<String> = tools.iter().flat_map(parameters_of).collect();
    parameter_shaped.sort();
    parameter_shaped.dedup();

    // **One exception, and it is not a parameter mention at all.** The boot
    // door is where a `sid` comes FROM: its description has to name the thing
    // it hands back, and the token is parameter-shaped only because every other
    // verb takes one. Each entry must actually fire — a stale exception fails
    // below, so this cannot quietly become the place drift goes to be forgiven.
    const ALLOWED: &[(&str, &str)] = &[("start_here", "sid")];

    let mut drifting: Vec<String> = Vec::new();
    let mut unused: Vec<&(&str, &str)> = ALLOWED.iter().collect();
    for tool in &tools {
        let mine = parameters_of(tool);
        let mut text = tool.description.as_deref().unwrap_or_default().to_string();
        text.push(' ');
        text.push_str(&serde_json::to_string(&tool.input_schema).expect("a schema serializes"));

        let mut named = named_as_a_parameter(&text);
        named.sort();
        named.dedup();
        for token in named {
            if !parameter_shaped.contains(&token) || mine.contains(&token) {
                continue;
            }
            if let Some(at) = unused
                .iter()
                .position(|(t, x)| *t == tool.name && *x == token)
            {
                unused.remove(at);
                continue;
            }
            if ALLOWED.iter().any(|(t, x)| *t == tool.name && *x == token) {
                continue;
            }
            drifting.push(format!(
                "{} describes {token:?}, which is no parameter of it — it takes {mine:?}",
                tool.name
            ));
        }
    }
    assert!(
        drifting.is_empty(),
        "the two halves of this surface disagree. A caller reads the sentence, sends the call the \
         schema refuses, and gets the one answer this surface cannot make blocked:\n  {}",
        drifting.join("\n  ")
    );
    assert!(
        unused.is_empty(),
        "these exceptions no longer match anything — delete them, or the allowlist stops being a \
         record of what is here and becomes a hole nobody reviewed: {unused:?}"
    );
}

/// **Every word an agent reads, gathered in one place** — tool descriptions,
/// the argument-schema field docs, the orientation essay, and the server
/// instructions.
///
/// **The schemas are the half that gets forgotten.** A doc comment on a public
/// args field is not a comment: `schemars` renders it into the JSON schema, so
/// it reaches a caller exactly as a description does. That is where `boot`
/// spent a release describing the deleted `mailbox` parameter.
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
                Arc::new(sid::SessionRegistry::new()),
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
    found
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
/// **The vocabulary below is retired, not merely unfashionable.** Mail and
/// sessions used to live on a kanban board — a message was a card in a funnel
/// column wearing a mailbox label. They are pages now. An agent must never be
/// taught the store's shape (it is not its business and it will be wrong
/// again), and must never be sent to repair something in a system that no
/// longer holds it.
///
/// Six point-fixes would have left the seventh. This is the class.
#[test]
fn no_agent_facing_text_teaches_the_retired_store() {
    // Each word, and what an agent wrongly concludes from meeting it.
    const RETIRED: &[(&str, &str)] = &[
        ("card", "a message is a row on a page, not a card"),
        ("kanban", "the board is gone"),
        ("funnel", "there are no columns to move between"),
        ("task board", "mail left the task layer entirely"),
        ("task system", "mail left the task layer entirely"),
        (
            "mailbox label",
            "a box is a page owned by its bot, not a label",
        ),
    ];
    // **The one legitimate use, allowlisted by name and by reason.** `crm` is a
    // cross-link into the OPERATOR'S OWN task system — somebody else's store,
    // which a caller must name correctly, and whose grammar is literally
    // `card:N` (`jojobot_domain::memory::validate_crm`). The rule is that no
    // text teaches JOJOBOT'S store; blanking the word here would turn a true
    // sentence false. Each entry must actually be hit — a stale exception
    // fails below, so this cannot quietly become a place to put new ones.
    const ALLOWED: &[(&str, &str)] = &[
        ("add_entity's argument schema", "card"),
        ("update_entity's argument schema", "card"),
        ("add_entity's argument schema", "task system"),
        ("update_entity's argument schema", "task system"),
    ];

    let mut teaching: Vec<String> = Vec::new();
    let mut unused: Vec<&(&str, &str)> = ALLOWED.iter().collect();
    for (what, text) in agent_facing_text() {
        let haystack = text.to_lowercase();
        for (word, why) in RETIRED {
            if !haystack.contains(word) {
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
        "agent-facing text teaches a store that no longer exists. An agent reads this as the \
         truth about the system it is calling, and acts on it:\n  {}",
        teaching.join("\n  ")
    );
    assert!(
        unused.is_empty(),
        "these exceptions no longer match anything — delete them, or the allowlist stops being \
         a record of what is here and becomes a hole nobody reviewed: {unused:?}"
    );
}

/// **Every string literal in the shipped half of this crate**, comments cut.
///
/// A blunt instrument on purpose: what it is looking for is prose an agent will
/// read, and prose an agent will read is not only in the descriptions — a
/// refusal's `how_to_proceed` is assembled at runtime and reaches the same
/// reader with the same authority. Comments go first because they discuss the
/// very names being searched for ("there is deliberately no create_mailbox"),
/// and a doc comment saying a verb is gone is the opposite of the defect.
fn shipped_literals() -> Vec<String> {
    // **One pass over the whole text, not one per line.** Almost every
    // description in this crate is a `\`-continued literal spanning twenty
    // lines; a per-line scan reads the opening line, then meets the closing
    // quote of line two with no state and takes it for an opening one — so
    // every continuation is silently skipped and the scan quietly watches the
    // first line of each. That is how this test passed while three stale
    // mentions sat in exactly those continuations.
    let source: String = shipped_source()
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n");

    let mut found = Vec::new();
    let mut inside: Option<String> = None;
    let mut chars = source.chars();
    while let Some(c) = chars.next() {
        match (c, &mut inside) {
            ('\\', Some(open)) => {
                // An escape and whatever it escapes, neither of which can close
                // the literal. A `\` at end of line eats the newline, which is
                // exactly the continuation this has to walk through.
                open.push(c);
                if let Some(escaped) = chars.next() {
                    open.push(escaped);
                }
            }
            ('"', Some(_)) => found.push(inside.take().expect("inside a literal")),
            ('"', None) => inside = Some(String::new()),
            (_, Some(open)) => open.push(c),
            (_, None) => {}
        }
    }
    found
}

/// **A verb this surface has retired is named nowhere an agent reads.**
///
/// The same defect class as a description advertising a parameter its schema
/// does not have, and with a worse ending: a caller that believes the sentence
/// emits a call for a tool that is not there, and an absent tool is a protocol
/// error rather than a blocked answer with a way forward. So it escapes the one
/// rule that says a caller mistake comes back as an answer.
///
/// **Both halves, or this only catches half the mistake.** A name must be off
/// the surface AND out of the prose — a verb deleted while three descriptions
/// go on pointing at it is the exact failure being prevented, and a verb still
/// shipping while the prose calls it retired is the same lie the other way up.
#[test]
fn no_agent_facing_text_names_a_verb_this_surface_retired() {
    // Each shipped once, so each is a name a client, a habit, or a description
    // somebody forgot to edit may still reach for. `list_mailboxes`'s two
    // surviving jobs are `read_mailbox`'s counting mode; `create_mailbox` never
    // existed as a verb and is what a caller invents when told to make a box,
    // which a box's opening with its bot makes impossible; `start_session` is
    // what a caller invents when told to start one, and booting IS starting.
    const RETIRED: &[(&str, &str)] = &[
        (
            "list_mailboxes",
            "counting is read_mailbox with counts_only: true",
        ),
        (
            "create_mailbox",
            "a box opens with the bot that owns it, in add_entity",
        ),
        (
            "start_session",
            "booting an identity IS starting its session",
        ),
    ];

    let surface: Vec<String> = Jojobot::tool_router()
        .list_all()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    let mut naming: Vec<String> = Vec::new();
    for (verb, instead) in RETIRED {
        if surface.iter().any(|name| name == verb) {
            naming.push(format!("{verb} is still ON the surface — {instead}"));
        }
        for (what, text) in agent_facing_text() {
            if text.contains(verb) {
                naming.push(format!("{what} names {verb} — {instead}"));
            }
        }
        // **Descriptions and the essay are not all of it.** The advice inside a
        // REFUSAL is read by exactly the same caller for exactly the same
        // purpose, and `agent_facing_text` cannot reach one: it is built at
        // runtime, on a path a sweep only walks if it thought to provoke it.
        // Three of this round's stale mentions were there — a blocked post, a
        // degraded search's coverage note — and none of them is a description.
        //
        // So: every string literal that ships. **Literals only**, because these
        // names are live Rust identifiers too — `Mailboxes::list_mailboxes` is
        // the store port and stays, `create_mailbox` likewise — and a scan of
        // raw source could not tell the port from the prose.
        for literal in shipped_literals() {
            if literal.contains(verb) {
                naming.push(format!(
                    "a shipped string names {verb} — {instead}: {literal:?}"
                ));
            }
        }
    }
    assert!(
        naming.is_empty(),
        "agent-facing text sends a caller to a verb that is not there. An absent tool is a \
         protocol error, not a blocked answer — the one caller mistake this surface cannot \
         answer:\n  {}",
        naming.join("\n  ")
    );
}

/// Walk a whole answer — **keys as well as values**, since `card_ids` leaked
/// through its key alone and its values were opaque ids.
fn store_words_in(what: &str, body: &serde_json::Value, found: &mut Vec<String>) {
    use jojobot_domain::vocabulary::store_words;
    match body {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                for (_, why) in store_words(key) {
                    found.push(format!("{what} has a field {key:?} — {why}"));
                }
                // **`crm` is the one link out, and it is the OPERATOR'S store,
                // not jojobot's.** Its grammar is literally `card:N`
                // (`jojobot_domain::memory::validate_crm`), a caller must get it
                // right, and fronting the task layer is deliberate. The rule is
                // that no answer teaches JOJOBOT'S store.
                if key == "crm" {
                    continue;
                }
                store_words_in(what, value, found);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                store_words_in(what, item, found);
            }
        }
        serde_json::Value::String(text) => {
            for (word, why) in store_words(text) {
                found.push(format!("{what} says {word:?} — {why}"));
            }
        }
        _ => {}
    }
}

/// **No answer tells a caller where jojobot put anything.**
///
/// The sibling of `no_agent_facing_text_teaches_the_retired_store`, over the
/// other half of what an agent reads. That one watches the STATIC text —
/// descriptions, schemas, the essay. This one watches what comes BACK, which is
/// where the first production run found three leaks at once: an entity's page
/// narrating its own layout in a complete sentence, a doc id on every search
/// hit, and `card_ids` on every mailbox payload including a boot.
///
/// **One sweep over every verb, not three assertions at three call sites.**
/// Three point-fixes leave the fourth, and the coverage assertion below is what
/// makes a fourth verb arrive with this check already pointed at it: a tool on
/// the surface that this sweep does not call fails here, in the test that would
/// have caught its leak.
#[tokio::test]
async fn no_verb_answer_names_the_store() {
    let alpha = Entity {
        id: EntityId::person("alpha"),
        kind: EntityKind::Person,
        name: "Alpha".into(),
        aliases: Vec::new(),
        // **A real cross-link, on purpose.** The one legitimate `card:` on the
        // wire has to be in the fixture, or the exception above is untested and
        // the sweep passes for the wrong reason.
        crm: Some("card:554".into()),
        source: "user-named".into(),
        parent: None,
        boot: jojobot_domain::memory::Boot::OnDemand,
    };
    let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let jojobot = Jojobot::new(
        Arc::new(jojobot_domain::memory::testing::InMemoryMemory::new()),
        Arc::new(SpySearch::answering(vec![
            Hit::Entity {
                entity: alpha.clone(),
                doc_id: "doc-alpha".into(),
                edges: Vec::new(),
            },
            Hit::Prose {
                doc_id: "doc-alpha".into(),
                title: "Alpha".into(),
                entity: Some(alpha.clone()),
                edges: Vec::new(),
                snippet: "a paragraph somebody wrote".into(),
            },
        ])),
        boxes.clone(),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(sid::SessionRegistry::new()),
    );

    make_bot(&jojobot, "gamma").await;
    // **A card nobody can read, because that branch is the one that leaked.**
    // `list_sent` only renders its unreadable report when a box has one, so a
    // clean fixture would sweep past the very payload this test exists for.
    boxes.quarantine(
        &MailboxName("gamma".into()),
        &MessageId("4212".into()),
        "its row on the page cannot be read — a state or a sender has been edited past parsing",
    );
    let sid = booted(&jojobot, "gamma").await;
    let some = |s: &str| Some(s.to_string());

    let mut answers: Vec<(&str, serde_json::Value)> = Vec::new();
    let ask = |name: &'static str, result: CallToolResult, out: &mut Vec<_>| {
        let body = json_of(&result);
        out.push((name, body.clone()));
        body
    };

    ask("ping", jojobot.ping().await.expect("ping ok"), &mut answers);
    ask(
        "start_here",
        jojobot
            .start_here(Parameters(OrientArgs {
                bot: some("gamma"),
                brief: None,
                resume: None,
            }))
            .await
            .expect("start_here ok"),
        &mut answers,
    );
    ask(
        "add_entity",
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                crm: some("card:554"),
                sid: some(&sid),
                ..add_args("person", "alpha", "Alpha")
            }))
            .await
            .expect("add_entity ok"),
        &mut answers,
    );
    let captured = ask(
        "capture",
        jojobot
            .capture(Parameters(CaptureArgs {
                sid: some(&sid),
                ..capture_args("person:alpha", "moved to the coast")
            }))
            .await
            .expect("capture ok"),
        &mut answers,
    );
    ask(
        "update_fact",
        jojobot
            .update_fact(Parameters(UpdateFactArgs {
                content: some("moved to the coast in spring"),
                sid: some(&sid),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect("update_fact ok"),
        &mut answers,
    );
    ask(
        "update_entity",
        jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "person:alpha".into(),
                name: None,
                aliases: None,
                source: some("user-named"),
                crm: some("card:554"),
                create_new: None,
                sid: some(&sid),
            }))
            .await
            .expect("update_entity ok"),
        &mut answers,
    );
    ask(
        "recall",
        jojobot
            .recall(Parameters(RecallArgs {
                subject: "person:alpha".into(),
                sid: some(&sid),
            }))
            .await
            .expect("recall ok"),
        &mut answers,
    );
    ask(
        "list_entities",
        jojobot
            .list_entities(Parameters(ListEntitiesArgs {
                kind: None,
                sid: some(&sid),
            }))
            .await
            .expect("list_entities ok"),
        &mut answers,
    );
    ask(
        "search",
        jojobot
            .search(Parameters(SearchArgs {
                query: some("alpha"),
                sid: some(&sid),
                ..search_args()
            }))
            .await
            .expect("search ok"),
        &mut answers,
    );
    ask(
        "set_charter",
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "bot:gamma".into(),
                prose: "the implementer".into(),
                sid: some(&sid),
            }))
            .await
            .expect("set_charter ok"),
        &mut answers,
    );
    let posted = ask(
        "post_message",
        jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "gamma".into(),
                sid: sid.clone(),
                subject: some("the shipment landed"),
                body: "it landed on the coast".into(),
                in_reply_to: None,
            }))
            .await
            .expect("post_message ok"),
        &mut answers,
    );
    let message_id = posted["id"].as_str().expect("a posted message has an id");
    ask(
        "list_sent",
        jojobot
            .list_sent(Parameters(ListSentArgs {
                sender: None,
                mailbox: None,
                include_bodies: Some(true),
                sid: some(&sid),
            }))
            .await
            .expect("list_sent ok"),
        &mut answers,
    );
    ask(
        "read_message",
        jojobot
            .read_message(Parameters(ReadMessageArgs {
                message_id: message_id.to_string(),
                sid: some(&sid),
            }))
            .await
            .expect("read_message ok"),
        &mut answers,
    );
    ask(
        "read_mailbox",
        jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                counts_only: None,
                new_only: Some(false),
                sid: some(&sid),
            }))
            .await
            .expect("read_mailbox ok"),
        &mut answers,
    );
    ask(
        "mark_processed",
        jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: message_id.to_string(),
                notes: some("acted on"),
                sid: some(&sid),
            }))
            .await
            .expect("mark_processed ok"),
        &mut answers,
    );
    ask(
        "journal",
        jojobot
            .journal(Parameters(JournalArgs {
                entry: "set out to read the box".into(),
                focus: some("reading the box"),
                sid: sid.clone(),
            }))
            .await
            .expect("journal ok"),
        &mut answers,
    );
    ask(
        "amend_journal",
        jojobot
            .amend_journal(Parameters(AmendJournalArgs {
                entry: "set out to read the box, and did".into(),
                sid: sid.clone(),
            }))
            .await
            .expect("amend_journal ok"),
        &mut answers,
    );
    // Last, because it closes the run every verb above was addressed through.
    ask(
        "wrap_session",
        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "read the box and acted on what was in it".into(),
                sid: sid.clone(),
            }))
            .await
            .expect("wrap_session ok"),
        &mut answers,
    );

    // **Every verb, or this sweep is watching a fraction of the surface.** The
    // three leaks were on three different verbs and nobody went looking on the
    // fourth; a tool that ships without a call here fails at this line rather
    // than silently going unwatched.
    let mut swept: Vec<&str> = answers.iter().map(|(name, _)| *name).collect();
    swept.sort_unstable();
    let mut shipped: Vec<&str> = Jojobot::tool_router()
        .list_all()
        .iter()
        .map(|t| Box::leak(t.name.to_string().into_boxed_str()) as &str)
        .collect();
    shipped.sort_unstable();
    assert_eq!(
        swept, shipped,
        "every verb's answer is swept for the store's vocabulary — a new one is a new call above"
    );

    let mut leaking: Vec<String> = Vec::new();
    for (name, body) in &answers {
        store_words_in(&format!("{name}'s answer"), body, &mut leaking);
    }
    assert!(
        leaking.is_empty(),
        "answers tell a caller where jojobot keeps things. It is not their business and it will \
         be wrong again:\n  {}",
        leaking.join("\n  ")
    );
}

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
/// An agent must never be taught the store's shape — not its business, and
/// it will be wrong again — and must never be sent to repair something in a
/// system that does not hold it.
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

        // **Retract needs an input it would otherwise accept.** A fact is not
        // retractable on its own merits, so the arm below refused for two
        // reasons at once and only the `how_to_proceed` assertion could tell
        // them apart — an arm that survives a rename of a refusal message
        // rather than a deletion of the gate. An event is chronology, which
        // retract takes, so identity is the only thing left to refuse it for.
        let event = jojobot
            .capture(Parameters(CaptureArgs {
                event_type: Some("visit".into()),
                ..capture_args("place:springfield", "the inspector came by")
            }))
            .await
            .expect("an identified event capture answers");
        let event_address = address_of(&json_of(&event));

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
                    create_new: None,
                    sid: None,
                }))
                .await
                .expect("update_entity answers"),
            "update_entity",
        );
        refused(
            &jojobot
                .retract(Parameters(RetractArgs {
                    address: event_address.clone(),
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

    // **This refusal is a raw protocol error rather than a blocked answer**,
    // which is the shape rule 68 exists to remove from the surface. Pinned as
    // it behaves, not as it ought to: changing the channel is a decision of its
    // own and is not what states a constraint in a description.
    let refused = jojobot
        .post_message(Parameters(PostMessageArgs {
            mailbox: "dev".into(),
            body: "the shipment landed".into(),
            subject: Some("what `post_message` does with a title".into()),
            in_reply_to: None,
            sid: sid.clone(),
        }))
        .await
        .expect_err("a subject carrying markup is refused");
    assert_eq!(refused.code, ErrorCode::INVALID_PARAMS);
    assert!(
        refused.message.contains("subject"),
        "the refusal names the field it is about: {}",
        refused.message
    );

    // …and the same message lands once the subject is one plain line, so the
    // refusal above is about the subject rather than about anything else in
    // the call.
    let posted = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "dev".into(),
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

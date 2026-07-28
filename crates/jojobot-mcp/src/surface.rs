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
    // The six mailbox verbs in it are list_mailboxes, list_sent,
    // post_message, read_mailbox, read_message and mark_processed — there
    // is deliberately no create_mailbox, because a box is not a thing you
    // make: it opens with the bot that owns it, in `add_entity`, and a bot
    // is the only thing that has one. The three session verbs are journal,
    // amend_journal and wrap_session (there is deliberately no
    // start_session — booting an identity IS starting its session); the
    // rest are Memory's.
    assert_eq!(
        names,
        [
            "add_entity",
            "amend_journal",
            "capture",
            "journal",
            "list_entities",
            "list_mailboxes",
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

/// **Polling is a read, and the surface has to say which verb reads.** A
/// session whose standing loop was "check the box; if empty do nothing" paid
/// ~14 state-changing deliveries of an empty box, because the only verb that
/// visibly answers "is there anything waiting" is the one that takes
/// delivery. `list_mailboxes` was the answer the whole time and nothing
/// pointed at it from the place the caller was standing.
#[test]
fn the_read_mailbox_description_points_at_the_read_only_way_to_poll() {
    let tools = Jojobot::tool_router().list_all();
    let read = tools
        .iter()
        .find(|t| t.name == "read_mailbox")
        .expect("read_mailbox is a tool");
    let description = read.description.as_deref().unwrap_or_default();
    assert!(
        description.contains("list_mailboxes"),
        "the cheaper verb must be named where the expensive one is read: {description}"
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
        "list_mailboxes",
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

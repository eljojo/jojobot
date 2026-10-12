//! "New laptop, nothing on it, no notes, no history. Be useful on the first
//! turn."
//!
//! An inventory of what a boot reaches for. Whether jojobot's job is the method
//! only, or also who the operator is, or all of it, is not settled here.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_fresh_session_tries_to_be_useful_on_turn_one() {
    let story = Story::begin("bot:otto").await;

    // ── earlier · something true before this machine ever existed ───────────
    let s = story.session().await;
    s.add("person:ned-flanders", "Ned").await;
    s.fact("person:ned-flanders", "left-handed").await;
    s.wrap("recorded, then the session ended").await;

    // ── turn one · a session with nothing local, on its own connection ──────

    // First: is anything there, and which build is it? No identity is needed
    // to ask, and it starts nothing. A session whose verb list looks wrong for
    // what it was told this server does has no other way to tell one
    // deployment from another.
    let (pong, unbooted) = story.call("ping", json!({})).await;
    pong.says("\"status\":\"ok\"").says("\"build\"");
    // What is checkable from out here is the payload: the answer carries no
    // handle, so a caller cannot come away from a probe holding a session.
    // Whether the server opened one behind it is not visible — ping names no
    // bot, and the only wire-visible session state is a boot's offer, per-bot.
    assert!(
        unbooted.is_none(),
        "ping's answer carries no session handle"
    );

    let (booted, s) = story.full_boot().await;

    // The method is server-side and needs no relearning on a fresh machine:
    // the essay, the world snapshot and this bot's own rules arrive with the
    // boot, before anything local exists.
    assert!(
        booted["orientation"]
            .as_str()
            .is_some_and(|essay| !essay.is_empty()),
        "the essay must arrive whole on a fresh boot: {booted}"
    );
    assert!(
        booted["identity"]["rules"].is_array(),
        "a bot's rules travel with it, not with the machine: {booted}"
    );
    assert_eq!(
        booted["snapshot"]["entities"]["available"], true,
        "the world snapshot is served on the same call: {booted}"
    );

    // The procedures ship with the binary too, listed by name and when-to-use
    // and never by body, so a fresh session learns what exists without paying
    // for what it does not need.
    let skills = booted["skills"]
        .as_array()
        .expect("the boot names the skills that exist");
    assert!(
        !skills.is_empty(),
        "a boot with no skills leaves nothing to reach for: {booted}"
    );
    assert!(
        skills
            .iter()
            .all(|s| s["when_to_use"].as_str().is_some_and(|w| !w.is_empty())),
        "when-to-use is what decides whether to fetch one, so every skill carries it: {booted}"
    );

    // And one is fetched by name through the same door — a read, starting no
    // session of its own.
    let fetched = story.skill("rhythms").await;
    assert!(
        fetched["skill"]["body"]
            .as_str()
            .is_some_and(|b| !b.is_empty()),
        "a skill named in the index is fetchable by name: {fetched}"
    );

    // And again from inside the session, which is where the door sends a
    // booted caller when the index names something it needs. The handle rides
    // this call like every other one this session makes: fetching a procedure
    // mid-run is the ordinary path, and carrying the handle is not a reason to
    // be turned back at it.
    s.call("start_here", json!({"skill": "rhythms"}))
        .await
        .says("\"body\"");

    // What was already true stays reachable. The machine is new; jojobot's
    // memory is not.
    s.recall("person:ned-flanders").await.says("left-handed");
    s.find("left-handed").await.says("person:ned-flanders");

    // The boot also names every identity on the server, so a session that was
    // not told which one it is can see what there is to be.
    assert!(
        booted["snapshot"]["entities"]["bots"]
            .as_array()
            .is_some_and(|bots| bots.iter().any(|b| b["handle"] == "bot:otto")),
        "the snapshot names the bots, so an identity is choosable rather than guessed: {booted}"
    );

    // GAP — but nothing says who is on the OTHER end of the conversation.
    // Every read above needed a handle to ask for, and the boot hands back
    // none: a fresh session that has not been told a name has nothing yet to
    // recall or search for.
    //   s.whose_assistant_am_i().says("person:maude").await;
    s.has_no_verb("whose_assistant", &["start_here", "search"])
        .await;

    // And a read that only wants to know what somebody IS does not have to
    // take everything ever said about them: the answer is the fields folded
    // into one row, with the records counted behind it rather than shipped.
    let dense = s
        .shape(
            "what jojobot holds on him, without every claim",
            json!({"subject": "person:ned-flanders", "facts": false}),
        )
        .await;
    dense.says("person:ned-flanders");
    dense.says("records are behind these fields");
    dense.never_says("left-handed");

    // GAP — but no read composes "what MATTERS right now" for a person or a
    // topic once a name is in hand. Composing the picture is served: one recall
    // takes the subject, its fields, its prose and the objects it reaches to a
    // depth, and the answer nests. Size is served too, by the read above. What
    // no read does is WEIGH the picture — nothing ranks a claim, nothing
    // prefers the recent one, and nothing says which of forty facts is the one
    // that matters this morning.
    //   s.brief("person:ned-flanders").await;
    s.has_no_verb("brief", &["search", "recall"]).await;

    // GAP — and nothing reaches outside jojobot's own memory and mail. A
    // calendar, a task board, a link library, a message waiting in another
    // inbox: a fresh session can reach only what jojobot itself holds, which
    // is the half of the life layer that has been migrated so far.
    //   s.today().await;
    s.has_no_verb("today", &["search", "read_mailbox"]).await;

    s.wrap("oriented, memory intact, still waiting to be told who is asking")
        .await;

    // ── later · a run that stopped, and the boot that picks it up ───────────

    // A run that says what it is working on and then stops without telling its
    // story. Nothing auto-wraps it, and that is what leaves it to be found.
    let interrupted = story.session().await;
    interrupted
        .call(
            "journal",
            json!({
                "entry": "started going through what is already recorded, to work out what this \
                          machine does not need to be told",
                "focus": "what jojobot already holds about ned-flanders",
            }),
        )
        .await;
    let stopped = interrupted.run().to_string();

    // The next boot does not start a fresh run over the top of it. It offers
    // the one that stopped, says what that run was working on so two of them
    // could be told apart, and hands back no handle until the choice is
    // answered.
    let (offer, unanswered) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    offer
        .says(&stopped)
        .says("what jojobot already holds about ned-flanders");
    assert!(
        unanswered.is_none(),
        "an unanswered offer hands back no handle — a sid here means the boot chose for us"
    );

    // Answering it inherits that run rather than starting a second beside it,
    // so the chronology continues instead of beginning again.
    let (resumed, picked_up) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": &stopped}),
        )
        .await;
    resumed.says("\"resumed\":true");
    let s = picked_up.expect("answering the offer hands back the handle");
    assert_eq!(
        s.run(),
        stopped,
        "resuming continues that run rather than minting one beside it"
    );
    assert_ne!(
        s.sid(),
        interrupted.sid(),
        "…and hands the run a new key, closing the one it had"
    );
    s.wrap("finished what the run before it started").await;

    // 🚨 **The vocabulary the software arrived holding, named at the door.**
    //
    // A session that has just booted has to write something, and the kinds a
    // handle may carry are the first thing it needs. It used to find them out
    // by declaring one and reading a refusal, or by asking a question it had
    // no reason to ask — the distinction was invisible until it tripped.
    //
    // **Names only, never bodies.** What a kind means and which keys it asks
    // for is a deliberate second read; what a boot owes is the list. Origin
    // is left off too — an agent booting does not act differently for
    // knowing which name is closed to redeclaration, and that question is
    // answered where it is actually asked: declaring a kind or reading one
    // directly.
    let (vocabulary, _) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    vocabulary.says("\"kinds\"").says("\"person\"");

    // **The half that makes the three above mean anything.** A boot that named
    // an empty list would satisfy every `says` on a key that is present and
    // holds nothing, which is the shape this suite has paid for more than
    // once. The kinds are counted out of the answer instead.
    let named = vocabulary.json()["snapshot"]["vocabulary"]["kinds"]
        .as_array()
        .expect("the boot names the kinds it ships")
        .len();
    assert!(
        named > 5,
        "a boot naming {named} kind(s) is a build that ships almost none, or a list that came \
         back empty and read as an answer",
    );

    story.finish().await;
}

/// **A fresh instance's anonymous boot stays under the ceiling, and says so
/// when it left part of the essay out.** The ceiling bounds the answer a
/// caller receives, serialized: the boot sizes what it ships by what it costs
/// there, so a caller never gets more than the ceiling while the boot claims
/// nothing was cut. Whatever the answer left out is named, and the next call
/// that reaches it is named beside it.
#[tokio::test]
async fn a_fresh_anonymous_boot_is_under_the_ceiling_and_names_what_it_left_out() {
    let story = Story::begin_with_nothing_written().await;
    let (anon, _) = story.call("start_here", json!({})).await;
    let whole = anon.raw().chars().count();
    assert!(
        whole <= jojobot_domain::text::BOOT_ANSWER.budget,
        "the answer is {whole} characters, over the ceiling"
    );
    let booted = anon.json();
    if booted["orientation_elided"] == true {
        assert!(
            booted["orientation_note"].is_string(),
            "a boot that left something out says what: {booted}"
        );
    } else {
        assert!(booted["orientation_note"].is_null(), "{booted}");
    }
    // The core ships either way: a caller is always taught the surface.
    assert!(
        booted["orientation"]
            .as_str()
            .is_some_and(|essay| essay.starts_with("# jojobot")),
        "{booted}"
    );
}

/// **A caller meets `fix_by` in the boot's orientation, where it first learns
/// what a refusal looks like.** The essay's account of a blocked result names
/// the word, what each of its values asks of the caller, and when a refusal may
/// be sent again unchanged. A fresh instance's anonymous boot ships that
/// section (it may leave a later one out), so the paragraph is in the answer
/// rather than behind a section call.
#[tokio::test]
async fn a_new_callers_boot_names_fix_by_and_what_each_word_asks() {
    let story = Story::begin_with_nothing_written().await;
    let (anon, _) = story.call("start_here", json!({})).await;
    let booted = anon.json();
    let essay = booted["orientation"].as_str().expect("the essay");
    assert!(
        essay.contains("## The answers that are not errors"),
        "the section that explains a blocked result is in the boot, not behind a call: {booted}"
    );
    for word in ["fix_by", "retry", "change", "person"] {
        assert!(
            essay.contains(&format!("`{word}`")),
            "the boot's orientation does not name `{word}`: it is where a caller first meets a refusal"
        );
    }
    assert!(
        !essay.contains("Never retry one unchanged"),
        "the old rule is false now that a refusal can say retry"
    );
}

/// A slug of ten letters that depends on nothing but the index, spread far
/// enough apart that no two screen as near-misses of each other.
fn slug(i: usize) -> String {
    let mut state = (i as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..10)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (b'a' + ((state >> 33) % 26) as u8) as char
        })
        .collect()
}

/// **An instance as heavy as a real one gets**: many bots, and one thing of each
/// kind that can stand alone. The anonymous boot names every bot, so this is
/// what takes room from the essay's remainder.
/// How many bots the heavy instance has: enough that the anonymous boot, which
/// names every one, has to leave part of the essay out.
const HEAVY_BOTS: usize = 80;

async fn make_the_instance_heavy(story: &Story) -> super::dsl::Session {
    let s = story.session().await;
    for i in 0..HEAVY_BOTS {
        s.add(&format!("bot:{}", slug(i)), &slug(i + 10_000)).await;
    }
    for (n, kind) in [
        "person", "project", "place", "event", "work", "thing", "org", "topic", "pet", "machine",
        "thread",
    ]
    .iter()
    .enumerate()
    {
        s.add(&format!("{kind}:{}", slug(n + 20_000)), &slug(n + 30_000))
            .await;
    }
    s
}

/// The quoted names in a refusal that lists them: the remainder's sections, by
/// the exact heading each is fetched with, and `opening` for the paragraphs
/// before the first heading.
fn quoted_section_names(refusal: &serde_json::Value) -> Vec<String> {
    refusal["how_to_proceed"]
        .as_str()
        .unwrap_or_default()
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|name| *name == "opening" || name.starts_with("## "))
        .map(str::to_string)
        .collect()
}

/// **What the anonymous boot of this instance leaves out of the essay, checked
/// against everything it promises.** The core ships whole (it is what a named
/// boot is served as its orientation); whatever remainder follows is whole
/// sections in order; every section it did not ship is named in the note and
/// comes back whole from `start_here` with `section`, and the shipped part and
/// the fetched parts together are the remainder, each heading once and in order.
/// Returns how many sections it left out.
async fn the_anonymous_boot_names_everything_it_leaves_out(story: &Story) -> usize {
    let (named, _) = story
        .call("start_here", json!({"bot": "assistant", "brief": false}))
        .await;
    let core = named.json()["orientation"]
        .as_str()
        .expect("a named boot is served the core as its orientation")
        .to_string();
    let (anon, _) = story.call("start_here", json!({})).await;
    let booted = anon.json();
    let shipped = booted["orientation"]
        .as_str()
        .expect("the anonymous boot ships an orientation");
    assert!(
        shipped.starts_with(&core),
        "the anonymous boot does not open with the whole core: {booted}"
    );
    let rest = &shipped[core.len()..];

    let listing = story
        .refused_at_the_door("start_here", json!({"section": "## No such section"}))
        .await;
    let names = quoted_section_names(&listing.json());
    assert!(
        names.len() > 3,
        "the essay's sections could not be listed from the refusal: {}",
        listing.json()
    );
    let note = booted["orientation_note"].as_str().unwrap_or_default();
    let opening_shipped = !rest.trim().is_empty();
    let mut left_out = 0;
    let mut rebuilt = rest.to_string();
    for name in &names {
        let shipped_whole = if name == "opening" {
            opening_shipped
        } else {
            rest.lines().any(|line| line.trim() == name)
        };
        if shipped_whole {
            continue;
        }
        left_out += 1;
        assert!(
            note.contains(name.as_str()),
            "the boot left out {name:?} and did not name it: {note}"
        );
        let (fetched, _) = story.call("start_here", json!({"section": name})).await;
        let body = fetched.json()["section"]["body"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "{name:?} does not come back from section: {}",
                    fetched.json()
                )
            })
            .to_string();
        assert!(
            name == "opening" || body.trim_start().starts_with(name.as_str()),
            "{name:?} comes back without its own heading: {body}"
        );
        assert!(!body.trim().is_empty(), "{name:?} comes back empty");
        rebuilt.push_str(&body);
    }
    assert_eq!(
        booted["orientation_elided"],
        left_out > 0,
        "the boot's own flag disagrees with what it left out: {booted}"
    );
    // The shipped part and the fetched parts are the remainder: each heading
    // appears once, in the order the essay lists them.
    let mut at = 0;
    for name in names.iter().filter(|name| name.as_str() != "opening") {
        let found = rebuilt[at..]
            .find(&format!("\n{name}"))
            .or_else(|| rebuilt[at..].starts_with(name.as_str()).then_some(0))
            .unwrap_or_else(|| panic!("{name:?} is not in the remainder, in order: {rebuilt}"));
        at += found + name.len();
    }
    left_out
}

/// **Pin (a): the anonymous boot ships the core whole and names every section it
/// leaves out, on a fresh real instance and on a heavy one.** The essay is
/// ranked against the room the instance leaves, so the whole of it is not a
/// promise: what is promised is that nothing is dropped unnamed and that every
/// dropped section is one call away, whole. The heavy instance must actually
/// cut, or the pin would hold over a boot that never had to name anything.
#[tokio::test]
async fn the_anonymous_boot_names_every_section_it_leaves_out_on_a_fresh_and_a_heavy_instance() {
    let fresh = Story::begin_with_nothing_written().await;
    the_anonymous_boot_names_everything_it_leaves_out(&fresh).await;
    fresh.finish().await;

    let heavy = Story::begin("bot:otto").await;
    let _writer = make_the_instance_heavy(&heavy).await;
    let left_out = the_anonymous_boot_names_everything_it_leaves_out(&heavy).await;
    eprintln!("the heavy instance's anonymous boot left out {left_out} section(s)");
    assert!(
        left_out >= 1,
        "the heavy instance left nothing out, so this pin proved nothing about naming"
    );
    heavy.finish().await;
}

/// **Pin (b): a named boot of a heavy identity fits the core and its floor under
/// the ceiling, with the margin printed.** A named boot is served the core and
/// what cannot be cut. The identity here is a heavy one on a heavy instance:
/// the roster of a hundred and fifty bots in the snapshot, a long charter, and
/// starred rules until the guard on a star refuses one or the seats run out.
///
/// **`MARGIN` is the room the answer keeps under the ceiling** for the next
/// additions to the core and for the characters a serialized answer spends on
/// escapes. A change that eats it fails here before a boot is cut.
///
/// **The margin is 450, the PM's call, overturnable.** Each shipped type adds
/// its name to the vocabulary every boot carries, about 26 characters, and the
/// sixth type (`computed-dates`) took the room under 500. 450 leaves room for
/// one more shipped type of that size. The pressure underneath is that every
/// shipped type costs every boot, which is a design question and not this pin's.
#[tokio::test]
async fn a_named_boot_of_a_heavy_identity_fits_core_and_floor_with_a_margin() {
    const MARGIN: usize = 450;
    const CHARTER_CHARS: usize = 5_000;
    const STARRED_RULES: usize = 3;
    let story = Story::begin("bot:otto").await;
    let s = make_the_instance_heavy(&story).await;
    let prose: String = (0..CHARTER_CHARS / 12)
        .map(|word| format!("{}c ", slug(word + 40_000)))
        .collect();
    s.call("set_charter", json!({"bot": "bot:otto", "prose": prose}))
        .await;
    let paragraph: String = (0..60)
        .map(|word| format!("{}rule{word} ", slug(word)))
        .collect();
    for rule in 0..STARRED_RULES {
        s.call(
            "capture",
            json!({"subject": "bot:otto", "content": format!("{rule}: {paragraph}"),
                   "provenance": "testimony", "fields": {"starred": "true"}}),
        )
        .await;
    }
    // No `resume`: a boot that sends one withholds the charter, and the charter
    // is what makes this identity heavy.
    let (named, _) = story.call("start_here", json!({"bot": "otto"})).await;
    let whole = named.raw().chars().count();
    let budget = jojobot_domain::text::BOOT_ANSWER.budget;
    assert_eq!(
        named.json()["identity"]["charter_elided"],
        false,
        "the charter is part of the weight this pin measures: {}",
        named.json()["identity"]
    );
    assert!(
        named.json().get("over_the_ceiling").is_none(),
        "the named boot says its floor is over the ceiling: {}",
        named.json()["over_the_ceiling"]
    );
    eprintln!(
        "named boot of a heavy identity (a charter of {CHARTER_CHARS} characters, {STARRED_RULES} \
         starred rules, {HEAVY_BOTS} bots on the roster): {whole} of {budget} characters, margin {}",
        budget.saturating_sub(whole)
    );
    assert!(
        whole + MARGIN <= budget,
        "the named boot is {whole} of {budget} characters, which leaves {} and the pin keeps \
         {MARGIN}",
        budget.saturating_sub(whole),
    );
    story.finish().await;
}

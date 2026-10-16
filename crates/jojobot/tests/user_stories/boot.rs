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
    vocabulary.says("\"kinds\"");

    // **The half that makes the line above mean anything.** The boot counts the
    // kinds and says where they are named, so the count is read out of the answer
    // and a zero or a missing one cannot pass for an answer.
    let named = vocabulary.json()["snapshot"]["vocabulary"]["kinds"]
        .as_u64()
        .expect("the boot counts the kinds it ships");
    assert!(
        vocabulary.json()["snapshot"]["vocabulary"]["kinds_listed_in"].is_string(),
        "…and says where they are named: {}",
        vocabulary.raw()
    );
    assert!(
        named > 5,
        "a boot naming {named} kind(s) is a build that ships almost none, or a list that came \
         back empty and read as an answer",
    );

    story.finish().await;
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
pub(super) fn slug(i: usize) -> String {
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

/// How many bots a boot lists by name before it counts the rest.
const BOTS_LISTED: usize = 20;

/// **The roster of a large instance, built the way the heavy pins build it**,
/// with the caller's own bot sorting after every other handle: a cap that kept
/// the first handles in order would drop the caller, and this is the case that
/// shows it. Returns every bot handle on the instance, in handle order.
async fn a_roster_of(story: &Story, count: usize, caller: &str) -> Vec<String> {
    let s = story.session().await;
    let mut roster: Vec<String> = (0..count).map(|i| format!("bot:{}", slug(i))).collect();
    for (i, handle) in roster.iter().enumerate() {
        s.add(handle, &slug(i + 10_000)).await;
    }
    roster.push("bot:assistant".to_string());
    roster.push(caller.to_string());
    roster.sort();
    roster.dedup();
    roster
}

/// **A named boot lists the bot that booted and counts the rest; an anonymous
/// boot lists at most twenty and counts the rest.** Only the booting bot's own
/// box is ever counted, so it is the one bot a named boot can name as having mail
/// waiting; the others are a count and the two calls that list them. An
/// anonymous boot is choosing an identity, so it names up to the cap, in handle
/// order. Paired with a small roster below, which an anonymous boot lists whole
/// and carries no count for.
#[tokio::test]
async fn a_named_boot_lists_the_caller_and_counts_the_rest_while_an_anonymous_one_lists_twenty() {
    let story = Story::begin("bot:upsilon").await;
    let roster = a_roster_of(&story, 150, "bot:upsilon").await;
    let (booted, _) = story.call("start_here", json!({"bot": "upsilon"})).await;
    let booted = booted.json();
    let entities = &booted["snapshot"]["entities"];
    assert_eq!(
        entities["by_kind"]["bot"],
        roster.len(),
        "the roster this case built is not the instance's: {entities}"
    );

    // ── the named boot: the caller, with its mail, and a count of the rest ──
    let listed: Vec<&serde_json::Value> = entities["bots"]
        .as_array()
        .expect("a list of bots")
        .iter()
        .collect();
    assert_eq!(listed.len(), 1, "{entities}");
    assert_eq!(listed[0]["handle"], "bot:upsilon", "{entities}");
    assert_eq!(listed[0]["yours"], true, "{entities}");
    assert!(listed[0]["mail"]["counts"].is_object(), "{entities}");
    let left_out = &entities["bots_left_out"];
    assert_eq!(left_out["count"], roster.len() - 1, "{entities}");
    let how = left_out["how_to_list"].as_str().unwrap_or_default();
    assert!(how.contains("list_entities"), "{left_out}");
    assert!(how.contains("colleagues"), "{left_out}");

    // ── the anonymous boot: twenty names in order, and the count of the rest ──
    let (anonymous, _) = story.call("start_here", json!({})).await;
    let anonymous = anonymous.json();
    let entities = &anonymous["snapshot"]["entities"];
    let listed: Vec<&str> = entities["bots"]
        .as_array()
        .expect("a list of bots")
        .iter()
        .map(|bot| bot["handle"].as_str().expect("a handle"))
        .collect();
    let expected: Vec<&str> = roster
        .iter()
        .map(String::as_str)
        .take(BOTS_LISTED)
        .collect();
    assert_eq!(listed, expected, "{entities}");
    assert_eq!(
        entities["bots_left_out"]["count"],
        roster.len() - BOTS_LISTED,
        "{entities}"
    );
    story.finish().await;
}

/// The other half of the pair: an anonymous boot of a roster under the cap lists
/// every bot, and says nothing about leaving any out, so the note means something
/// when it appears. A named boot of the same roster lists itself and counts the
/// others.
#[tokio::test]
async fn a_boot_of_a_small_roster_lists_every_bot_to_a_caller_choosing_one() {
    let story = Story::begin("bot:upsilon").await;
    let roster = a_roster_of(&story, 5, "bot:upsilon").await;
    assert!(roster.len() < BOTS_LISTED);

    let anonymous = story.call("start_here", json!({})).await.0;
    let entities = &anonymous.json()["snapshot"]["entities"];
    let listed: Vec<&str> = entities["bots"]
        .as_array()
        .expect("a list of bots")
        .iter()
        .map(|bot| bot["handle"].as_str().expect("a handle"))
        .collect();
    assert_eq!(listed, roster, "{entities}");
    assert!(entities.get("bots_left_out").is_none(), "{entities}");

    let named = story.call("start_here", json!({"bot": "upsilon"})).await.0;
    let entities = &named.json()["snapshot"]["entities"];
    assert_eq!(
        entities["bots"].as_array().map(Vec::len),
        Some(1),
        "{entities}"
    );
    assert_eq!(
        entities["bots_left_out"]["count"],
        roster.len() - 1,
        "{entities}"
    );
    story.finish().await;
}

/// **The refusal for a bot that does not exist lists the roster the same way**:
/// twenty handles in order, the count of the rest and the calls that list them.
/// The caller named no bot that exists, so no handle is kept for it.
#[tokio::test]
async fn the_refusal_for_an_unknown_bot_lists_twenty_bots_and_counts_the_rest() {
    let story = Story::begin("bot:upsilon").await;
    let roster = a_roster_of(&story, 150, "bot:upsilon").await;
    let refusal = story
        .refused_at_the_door("start_here", json!({"bot": "nobody-by-this-name"}))
        .await
        .json();
    let listed: Vec<&str> = refusal["bots"]
        .as_array()
        .expect("a list of bots")
        .iter()
        .map(|bot| bot.as_str().expect("a handle"))
        .collect();
    let expected: Vec<&str> = roster
        .iter()
        .map(String::as_str)
        .take(BOTS_LISTED)
        .collect();
    assert_eq!(listed, expected, "{refusal}");
    let left_out = &refusal["bots_left_out"];
    assert_eq!(left_out["count"], roster.len() - BOTS_LISTED, "{refusal}");
    let how = left_out["how_to_list"].as_str().unwrap_or_default();
    assert!(how.contains("list_entities"), "{left_out}");
    assert!(how.contains("colleagues"), "{left_out}");
    // The prose names the same twenty and no more: a handle past the cap is in
    // neither list.
    let past_the_cap = &roster[BOTS_LISTED];
    assert!(
        !refusal["how_to_proceed"]
            .as_str()
            .unwrap_or_default()
            .contains(past_the_cap.as_str()),
        "{refusal}"
    );

    // Paired: a small roster is named whole and carries no count.
    let small = Story::begin("bot:upsilon").await;
    let few = a_roster_of(&small, 5, "bot:upsilon").await;
    let refusal = small
        .refused_at_the_door("start_here", json!({"bot": "nobody-by-this-name"}))
        .await
        .json();
    let listed: Vec<&str> = refusal["bots"]
        .as_array()
        .expect("a list of bots")
        .iter()
        .map(|bot| bot.as_str().expect("a handle"))
        .collect();
    assert_eq!(listed, few, "{refusal}");
    assert!(refusal.get("bots_left_out").is_none(), "{refusal}");
    small.finish().await;
    story.finish().await;
}

/// **A compaction report beside a skill is refused**, as `bot` beside `skill`
/// is. A fetch answers before the boot, so it would drop the report without a
/// word and the run would go on believing the server knew what it had forgotten.
/// Paired with the report alone, which a live run makes and is answered.
#[tokio::test]
async fn a_compaction_report_beside_a_skill_is_refused_rather_than_dropped() {
    let story = Story::begin("bot:gamma").await;
    let refusal = story
        .refused_at_the_door(
            "start_here",
            json!({"skill": "recommend", "compacted": true}),
        )
        .await
        .json();
    assert!(
        refusal["how_to_proceed"]
            .as_str()
            .unwrap_or_default()
            .contains("compacted"),
        "the refusal names the argument it refused: {refusal}"
    );
    // The same refusal for a section, which is a fetch too.
    story
        .refused_at_the_door(
            "start_here",
            json!({"section": "opening", "compacted": true}),
        )
        .await;
    // Paired: the skill alone is read.
    let read = story.skill("recommend").await;
    assert_eq!(read["skill"]["name"], "recommend", "{read}");
    story.finish().await;
}

/// **The route the refusal and the essay name reaches the bots a capped roster
/// left out.** A roster over the cap is listed twenty at a time, and the
/// snapshot and the refusal for a name that is no bot say the rest are counted
/// under `bots_left_out`. Each answer
/// is read for the call it names, the call is made, and it returns a bot the
/// capped list did not carry.
#[tokio::test]
async fn the_route_to_the_bots_a_capped_roster_left_out_returns_them() {
    let story = Story::begin("bot:upsilon").await;
    let roster = a_roster_of(&story, 30, "bot:upsilon").await;
    let past_the_cap = roster[BOTS_LISTED].clone();
    let session = story.as_bot("bot:upsilon").await;

    // ── the snapshot, which the essay sends a caller without a name to ──────
    let (booted, _) = story.call("start_here", json!({})).await;
    let booted = booted.json();
    let snapshot = &booted["snapshot"]["entities"];
    let listed: Vec<&str> = snapshot["bots"]
        .as_array()
        .expect("a list of bots")
        .iter()
        .map(|bot| bot["handle"].as_str().expect("a handle"))
        .collect();
    assert!(
        !listed.contains(&past_the_cap.as_str()),
        "the capped list carries the bot the route must find: {snapshot}"
    );
    let how = snapshot["bots_left_out"]["how_to_list"]
        .as_str()
        .unwrap_or_else(|| panic!("the snapshot names no route: {snapshot}"));
    assert!(how.contains("list_entities"), "{how}");

    // ── the refusal for a name that is no bot ───────────────────────────────
    let refusal = story
        .refused_at_the_door("start_here", json!({"bot": "nobody-by-this-name"}))
        .await
        .json();
    let how_refused = refusal["bots_left_out"]["how_to_list"]
        .as_str()
        .unwrap_or_else(|| panic!("the refusal names no route: {refusal}"));
    assert_eq!(how_refused, how, "one route in both answers");

    // ── the call both name returns the bot the list left out ───────────
    let all = session.call("list_entities", json!({"kind": "bot"})).await;
    assert!(
        all.raw().contains(&past_the_cap),
        "list_entities kind bot does not return {past_the_cap}: {}",
        all.raw()
    );
    story.finish().await;
}

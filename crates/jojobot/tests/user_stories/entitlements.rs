//! "Yes I have the pass — you should know this."
//!
//! It was written down months earlier and it was written down correctly. The
//! session that needed it asked the user instead, because nothing about
//! pulling up the event mentioned that somebody held anything.
//!
//! **So this story is about RETRIEVAL and not about storage.** Every way of
//! recording it would have stored it fine; what was missing is that reading
//! the thing says who holds what that gets them in. ⛔️ **A story that asks for
//! the pass and receives it would pass on the build that failed** — this one
//! asks for the event and nothing else, exactly as a session that does not
//! know there is anything to ask about would.
//!
//! The two records are both true and a reader acts on them differently: one is
//! good today, one ran out. Dropping the second answers "can I get in" with a
//! silence again, one layer along.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn reading_the_thing_says_who_holds_what_gets_them_in() {
    let story = Story::begin("bot:otto").await;

    // ── months earlier · somebody says what they hold, and it is written ────
    let s = story.session().await;
    s.add("event:krusty-fest", "The Krusty Fest").await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("person:bart", "Bart").await;

    // His own words, so the record is testimony: he is going to act on this at
    // a door, and a guess reading back as settled is how somebody is turned
    // away.
    s.event_with(
        "person:milhouse",
        "holds a full pass for the whole run",
        json!({
            "admits": "event:krusty-fest",
            "tier": "full",
            "valid_until": "2026-12-31",
        }),
        &[],
    )
    .await;

    // True when it was written and no longer true today. **Kept, because "you
    // had one and it ran out" is what a reader needs next.**
    s.event_with(
        "person:bart",
        "held a day pass",
        json!({
            "admits": "event:krusty-fest",
            "tier": "day",
            "valid_until": "2026-01-04",
        }),
        &[],
    )
    .await;

    s.add("place:moes", "Moe's").await;
    s.wrap("wrote down what people said they hold").await;

    // ── months later · a session pulls the event, having been told nothing ──
    let later = story.session().await;
    let pulled = later.recall("event:krusty-fest").await;

    // **It asked for the event. It was told about the pass.** No walk, no
    // relation named, no key mentioned anywhere in the call.
    pulled
        .says("holds a full pass for the whole run")
        .says("person:milhouse")
        .says("\"liveness\":\"live\"")
        .says("\"provenance\":\"testimony\"");

    // ── the hedge the operator wrote, and it must survive the block ────────
    //
    // ⭐ **A pass somebody THINKS they have is not a pass they have.** The
    // operator said it and said they were unsure, and both halves are recorded —
    // so a session reading this block to tell them they are covered has to be able
    // to see the second one. A dropped hedge turns musing into a fact at the
    // exact moment somebody acts on it.
    let unsure = story.session().await;
    unsure.add("person:nelson", "Nelson").await;
    unsure
        .hedged_with(
            "person:nelson",
            "thinks he still has a pass from last year",
            json!({ "admits": "event:krusty-fest", "tier": "full" }),
        )
        .await;
    unsure
        .wrap("wrote down the one he was not sure about")
        .await;

    let with_the_hedge = story.session().await.recall("event:krusty-fest").await;
    // **Both keys, on the same object, in one read.** They answer different
    // questions — how sure the operator was, and whether the pass is in force
    // on the day — and asserting either alone passes on a build that still
    // carries one word for both.
    with_the_hedge
        .says("thinks he still has a pass")
        .says("\"standing\":\"open\"")
        .says("\"liveness\":\"live\"");
    // …and the claim nobody hedged says so under the same key, so this cannot
    // pass on a build that marks everything open.
    with_the_hedge.says("\"standing\":\"settled\"");

    // The one that ran out is here and says so, rather than being dropped or
    // being served as though it still worked.
    pulled
        .says("held a day pass")
        .says("\"liveness\":\"lapsed\"");

    // ── and the one that was taken back ────────────────────────────────────
    // **A pass somebody withdrew must not read as one they hold.** The block
    // arrives unasked on every read, so a claim the operator took back would
    // otherwise tell every later session that they are covered — which costs
    // far more than saying nothing.
    let withdrawn = later
        .event_with(
            "person:bart",
            "was promised a pass by the organisers",
            json!({ "admits": "event:krusty-fest", "tier": "guest" }),
            &[],
        )
        .await;
    later
        .retract(&withdrawn, "the organisers never issued it")
        .await;
    let after = later.recall("event:krusty-fest").await;
    after
        .says("holds a full pass for the whole run")
        .never_says("was promised a pass by the organisers");

    // ── and the other silence, which is a different answer ──────────────────
    let nothing = later.recall("place:moes").await;
    nothing
        .says("nothing points at this")
        .never_says("\"standing\":\"live\"");
}

/// **A record that only mentions a thing is not read as a rule about it.**
///
/// The held block said "nothing here admits anybody" over records that point at
/// a place through a key other than `admits`, and the same words over a read
/// that follows links of its own and lists none. Both read as a permission
/// rule about the place. The block says the records mention the thing, and a
/// reader who followed a link of its own is told where to see them.
#[tokio::test]
async fn a_record_that_only_mentions_a_thing_is_not_read_as_a_rule_about_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("place:moes", "Moe's").await;
    s.add("place:leftorium", "The Leftorium").await;
    s.add("thing:the-couch", "The Couch").await;
    s.event_with(
        "thing:the-couch",
        "the couch goes on a trip",
        json!({
            "departs_from": "place:moes",
            "arrives_at": "place:leftorium",
            "leaves_on": "2026-08-01",
            "returns_on": "2026-08-09",
        }),
        &[],
    )
    .await;

    // Records point at the place, none of them through `admits`.
    let pointed_at = s.recall("place:moes").await;
    pointed_at
        .says("the couch goes on a trip")
        .says("\"liveness\":\"related\"")
        .says("mention");

    // A call that follows links of its own lists none of them, and says where
    // they are.
    let walking = s
        .call(
            "recall",
            json!({
                "subject": "place:leftorium",
                "follow": {"shape": "location", "direction": "in"},
            }),
        )
        .await;
    walking.says("mention");
    story.finish().await;
}

/// Ten people who hold or mention the hall, in the roster's own names.
const HOLDERS: [&str; 10] = [
    "person:butters",
    "person:kenny",
    "person:kyle",
    "person:apu",
    "person:burns",
    "person:gene",
    "person:homer",
    "person:krusty",
    "person:randy",
    "person:stan",
];

/// **One record per person about the hall, each long enough that ten of them do
/// not fit the block's budget**, written under `key`.
async fn ten_records_about_the_hall(s: &super::dsl::Session, key: &str) {
    s.add("thing:springfield-hall", "Springfield Hall").await;
    for (at, holder) in HOLDERS.iter().enumerate() {
        s.add(holder, holder.trim_start_matches("person:")).await;
        s.event_with(
            holder,
            &format!(
                "record {at} about the hall: the stunt rehearsal, the long afternoon set-up, \
                 the load-in through the side door and the clear-down after, written out in \
                 full so that ten of these cannot all fit one answer's budget"
            ),
            json!({ key: "thing:springfield-hall" }),
            &[],
        )
        .await;
    }
}

/// The held block's note on a read of the hall, and how many it left out.
async fn held_of_the_hall(s: &super::dsl::Session) -> (String, u64) {
    let read = s.recall("thing:springfield-hall").await;
    let held = &read.json()["objects"][0]["held"];
    (
        held["note"].as_str().unwrap_or_default().to_string(),
        held["left_out"].as_u64().unwrap_or_default(),
    )
}

/// **A held block of records that only mention the hall, too many to fit, says
/// they link without admitting AND names the walk that reads them all.** The walk
/// is followed, and it returns every holder.
#[tokio::test]
async fn an_overflowing_block_of_related_records_names_the_walk_that_reads_them_all() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.call(
        "declare_type",
        json!({"name": "booking", "fields": [{"key": "booked_into", "holds": "reference"}]}),
    )
    .await;
    ten_records_about_the_hall(&s, "booked_into").await;

    let (note, left_out) = held_of_the_hall(&s).await;
    assert!(left_out > 0, "ten records overflow the budget: {note}");
    assert!(
        note.contains("direction: \"in\""),
        "the note names the walk in: {note}"
    );
    assert!(
        !note.contains("relation: \"admits\""),
        "the walk it names is not the admits one: {note}"
    );

    let walk = s
        .call(
            "recall",
            json!({"subject": "thing:springfield-hall", "follow": {"direction": "in"}}),
        )
        .await;
    for holder in HOLDERS {
        walk.says(holder);
    }
    story.finish().await;
}

/// **An overflowing block of records that admit keeps its note** (the paired
/// regression): it names the walk along `admits`.
#[tokio::test]
async fn an_overflowing_block_of_admitting_records_keeps_its_own_note() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    ten_records_about_the_hall(&s, "admits").await;
    let (note, left_out) = held_of_the_hall(&s).await;
    assert!(left_out > 0, "{note}");
    assert!(
        note.contains("admits") && note.contains("did not fit"),
        "the note names the admits walk: {note}"
    );
    assert!(!note.contains("mention"), "{note}");
    story.finish().await;
}

/// **A block of related records that fits says the related note only** (the
/// other half of the pair): no walk is named for what nothing left out.
#[tokio::test]
async fn a_related_block_that_fits_says_only_that_the_records_mention_the_hall() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.call(
        "declare_type",
        json!({"name": "booking", "fields": [{"key": "booked_into", "holds": "reference"}]}),
    )
    .await;
    s.add("thing:springfield-hall", "Springfield Hall").await;
    s.add("person:homer", "Homer").await;
    s.event_with(
        "person:homer",
        "booked the hall for the rehearsal",
        json!({"booked_into": "thing:springfield-hall"}),
        &[],
    )
    .await;
    let (note, left_out) = held_of_the_hall(&s).await;
    assert_eq!(left_out, 0, "{note}");
    assert!(note.contains("mention"), "{note}");
    assert!(!note.contains("did not fit"), "{note}");
    story.finish().await;
}

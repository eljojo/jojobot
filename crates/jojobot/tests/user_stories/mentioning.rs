//! "@person:milhouse rode @thing:handcart" — a sentence that names things.
//!
//! **An agent writes handles into prose because that is how it talks.** The
//! sentence is the claim, and the handles in it are meant as the things they
//! name rather than as words that happen to look like them.
//!
//! What this story shows is that the naming survives the sentence: a claim
//! reads back with every handle it was written with, and a mention of something
//! that is not there is refused rather than filed as a spelling nobody can
//! follow.
//!
//! **And it shows the payoff.** `rename_entity` is now a verb a caller can
//! reach, so the case that watches a mention follow a thing after a rename no
//! longer has to live only in the shared contract — it is reachable through
//! the served surface, and below it is.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_session_writes_handles_into_a_sentence_and_reads_them_all_back() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("person:milhouse", "Milhouse").await;
    s.add("pet:santas-little-helper", "The Dog").await;
    s.add("place:shelbyville", "Shelbyville").await;
    s.add("thing:handcart", "The Handcart").await;

    // **Four mentions across four kinds in one claim**, which is how the
    // sentence was actually asked for: a mechanism that stopped at the first
    // would pass a case with one.
    let written = s
        .call(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": "took @pet:santas-little-helper to @place:shelbyville \
                            on @thing:handcart with @person:milhouse driving",
                "provenance": "testimony",
            }),
        )
        .await;
    written.says("person:milhouse");

    // Every handle comes back, out of the read a later session makes.
    let read = s.recall("person:milhouse").await;
    for named in [
        "pet:santas-little-helper",
        "place:shelbyville",
        "thing:handcart",
    ] {
        read.says(named);
    }

    // ⭐ **And what search holds is what a reader reads.** The query is a word
    // out of the claim, and what is asserted is the HANDLE inside the hit — so
    // an index holding the stored form instead would answer with a badge here
    // and fail. Asserting the query's own word back would pass either way.
    s.call("search", json!({"query": "driving"}))
        .await
        .says("pet:santas-little-helper");

    s.wrap("recorded one outing, naming everyone who was on it")
        .await;
}

/// **A sentence that names something jojobot has never heard of is refused**,
/// with nothing written — the rule an edge's object already faces.
///
/// **Paired with the write that lands**, because a build refusing every claim
/// with an `@` in it would satisfy the first half and serve nobody.
#[tokio::test]
async fn a_sentence_naming_something_that_is_not_there_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("person:milhouse", "Milhouse").await;

    s.refused(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "went out with @person:nelson",
            "provenance": "testimony",
        }),
    )
    .await
    .says("person:nelson");

    // …and the same sentence lands once the thing it names is there.
    s.add("person:nelson", "Nelson").await;
    s.call(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "went out with @person:nelson",
            "provenance": "testimony",
        }),
    )
    .await
    .says("person:milhouse");
    s.recall("person:milhouse").await.says("person:nelson");

    s.wrap("named somebody jojobot did not know, then introduced them")
        .await;
}

/// **The payoff this file's own header used to say it could not show.** For
/// months the operator filed Shelbyville under a typo — `place:shelbyvile` —
/// and wrote about it from that name: a claim on the place itself, and a
/// separate claim on Milhouse mentioning it in prose, four months apart.
/// Correcting the typo has to reach both without anybody rewriting either
/// claim.
///
/// **Paired with a control**, on the same subject as the mention: a rename
/// that repointed every pointer on the page rather than the one it was asked
/// to move would pass every assertion above it by accident.
#[tokio::test]
async fn a_rename_reaches_months_of_mentions_written_under_the_old_name() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("place:shelbyvile", "Shelbyville").await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("thing:handcart", "The Handcart").await;

    // January: a claim about the place itself, under the typo.
    s.call(
        "capture",
        json!({
            "subject": "place:shelbyvile",
            "content": "is where the operator's parents still live",
            "provenance": "testimony",
            "recorded_at": "2026-01-12",
        }),
    )
    .await;

    // April: a separate claim, on a different subject, mentioning the place
    // in prose — the shape a mention actually arrives in.
    s.call(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "grew up in @place:shelbyvile",
            "provenance": "testimony",
            "recorded_at": "2026-04-19",
        }),
    )
    .await;

    // The control, on the same subject: a claim mentioning something the
    // rename below never touches.
    s.call(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "rode @thing:handcart the same summer",
            "provenance": "testimony",
            "recorded_at": "2026-04-19",
        }),
    )
    .await;

    // The correction, months later — the typo is retired.
    s.call(
        "rename_entity",
        json!({
            "handle": "place:shelbyvile",
            "to": "place:shelbyville",
            "recorded_at": "2026-08-02",
        }),
    )
    .await;

    // **The old handle still resolves.** A caller who only ever knew it as
    // `place:shelbyvile` and tries to act on it again is refused — but the
    // refusal recognizes the staleness and names what it is called now,
    // never a bare miss that reads the same as a handle that never existed.
    s.refused(
        "rename_entity",
        json!({"handle": "place:shelbyvile", "to": "place:x"}),
    )
    .await
    .says("place:shelbyville");

    // **And it is the same thing.** The claim recorded in January, on the
    // place itself, is exactly where it was left — under its current name.
    s.recall("place:shelbyville")
        .await
        .says("is where the operator's parents still live");

    // **`recall` itself answers to the old handle, not only `rename_entity`'s
    // own refusal above.** A rename never rewrites what points at a thing
    // (rule 243, decision log 272): a caller who only ever knew this place as
    // `place:shelbyvile` and asks `recall` for it by that name reaches the
    // same claim, not a miss. This is the door a caller actually knocks on —
    // the contract test behind this fix called `Memory::recall` directly and
    // never traveled the served verb's own walk, so a regression in the walk
    // alone would have passed it.
    s.recall("place:shelbyvile")
        .await
        .says("is where the operator's parents still live");

    // **The stored mention, four months old, renders under the new name.**
    // Neither January's nor April's claim was rewritten; what changed is
    // what a read of the pointer renders back.
    let milhouse = s.recall("person:milhouse").await;
    milhouse
        .says("place:shelbyville")
        .never_says("place:shelbyvile");

    // **The control is untouched.**
    milhouse.says("thing:handcart");

    // **The negative control the two `recall`s above need.** Nothing here
    // resembles `person:zzz-nobody` — not the renamed place, not Milhouse,
    // not the handcart — so a `recall` that had started resolving anything
    // at all, rather than genuinely walking the old-handle pointer, would
    // have left both positives above green while this one goes green for
    // the wrong reason. The refusal has to name its candidates as empty,
    // not merely carry `status: blocked`: the field is on every blocked
    // body regardless of what fired it, so only the empty list says nothing
    // was found to resemble.
    let miss = s
        .refused("recall", json!({"subject": "person:zzz-nobody"}))
        .await;
    assert_eq!(miss.json()["attempted"], "person:zzz-nobody");
    assert_eq!(
        miss.json()["candidates"].as_array().expect("a list").len(),
        0,
        "nothing here resembles this handle: {}",
        miss.json()
    );

    s.wrap("corrected a four-month-old typo; everything written under it still finds the place")
        .await;
}

/// **The payoff reaches a session's own chronology too.** A journal beat is
/// prose exactly like a claim's content, and rule 260 resolves a mention in
/// it the same way — proven here by writing one across a rename rather than
/// reading it back unchanged. `wrap_session`'s own answer is what a story
/// reads: it carries the whole chronology, so the beat comes back rendered
/// without a second call.
///
/// **Paired, in the same beat**: a second mention, on an entity the rename
/// never touches, so a beat rewritten wholesale cannot pass this by accident.
#[tokio::test]
async fn a_journal_beat_written_before_a_rename_renders_it_on_the_way_out() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("place:shelbyvile", "Shelbyville").await;
    s.add("thing:handcart", "The Handcart").await;

    s.journal("looking into a claim near @place:shelbyvile, using notes off @thing:handcart")
        .await;

    s.call(
        "rename_entity",
        json!({"handle": "place:shelbyvile", "to": "place:shelbyville"}),
    )
    .await;

    // **The mention, not the record of the add.** The automatic `add_entity`
    // beat two entries up legitimately names `place:shelbyvile` forever — that
    // was its handle at the moment the beat happened, a historical fact rather
    // than a stored pointer. The `@` marks the difference: only a rendered
    // mention carries it.
    s.call(
        "wrap_session",
        json!({"story": "closed out the trip notes"}),
    )
    .await
    .says("@place:shelbyville")
    .never_says("@place:shelbyvile")
    .says("@thing:handcart");
}

/// **And a mailbox message.** Rule 260 covers a message's body the same way,
/// so a note filed before a rename still names the thing correctly once read
/// back — through `search`, the one door onto anybody's mail.
#[tokio::test]
async fn a_posted_message_written_before_a_rename_renders_it_on_the_way_out() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("place:shelbyvile", "Shelbyville").await;

    s.post(
        "assistant",
        "a note about the trip",
        "left the receipts near @place:shelbyvile for whoever picks this up",
    )
    .await;

    s.call(
        "rename_entity",
        json!({"handle": "place:shelbyvile", "to": "place:shelbyville"}),
    )
    .await;

    s.call("search", json!({"query": "receipts", "include_mail": true}))
        .await
        .says("place:shelbyville")
        .never_says("place:shelbyvile");
}

/// **"Everything that points at this thing" reaches all three routes, each
/// told apart from the others** — an edge somebody drew on purpose, a
/// mention sitting in a claim's own words, and a `refs` entry naming it with
/// no claim about how. None of the three is the same claim as the others,
/// and a walk that named no shape at all is what reaches them together.
#[tokio::test]
async fn a_walk_with_no_shape_named_reaches_an_edge_a_mention_and_a_ref_at_once() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("topic:widgets", "The Widgets Project").await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("person:nelson", "Nelson").await;
    s.add("person:bart", "Bart").await;

    s.fact_about(
        "person:milhouse",
        "is the one actually running it",
        "about",
        "topic:widgets",
    )
    .await;

    s.call(
        "capture",
        json!({
            "subject": "person:nelson",
            "content": "helped pull an all-nighter on @topic:widgets before the deadline",
            "provenance": "testimony",
        }),
    )
    .await;

    s.call(
        "capture",
        json!({
            "subject": "person:bart",
            "content": "was asked to keep his mouth shut about the schedule",
            "provenance": "testimony",
            "refs": ["topic:widgets"],
        }),
    )
    .await;

    let walked = s
        .shape(
            "everything that points at the widgets project",
            json!({
                "subject": "topic:widgets",
                "follow": {"direction": "in"},
            }),
        )
        .await;

    walked
        .says("person:milhouse")
        .says("person:nelson")
        .says("\"mention\":true")
        .says("person:bart")
        .says("\"ref\":true");

    s.wrap("found everyone touching the widgets project, however each one pointed at it")
        .await;
}

/// "I set that one aside because it copies @thing:handcart's dispatch."
///
/// **The reason a message was set aside is free text a caller writes**, exactly
/// as the notes of a retirement are, so a handle written into it is a link and
/// not a spelling: a thing renamed afterwards reads under its new name in the
/// reason a refusal quotes. Every sibling text field already did this; the
/// quarantine reason stored what it was sent.
#[tokio::test]
async fn a_quarantine_reason_that_names_a_thing_follows_it_through_a_rename() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:handcart", "The Handcart").await;
    let stray = s
        .post("gamma", "Twice", "This one went out twice by mistake.")
        .await;

    s.call(
        "mark_processed",
        json!({
            "message_id": &stray,
            "quarantine": "a copy of the dispatch about @thing:handcart, and @thing:the-fern",
        }),
    )
    .await
    .says("@thing:handcart");

    // The thing is renamed, and the reason reads under the new name.
    s.call(
        "rename_entity",
        json!({"handle": "thing:handcart", "to": "thing:kettle"}),
    )
    .await;
    s.refused("read_message", json!({"message_id": &stray}))
        .await
        .says("@thing:kettle")
        .never_says("@thing:handcart")
        // A mention nothing resolves to is left exactly as it was written, the
        // treatment every sibling field gives one: it is not refused.
        .says("@thing:the-fern");

    // **Every path that quotes the reason renders it**, not only the read: a
    // second attempt to retire the same message is refused with it too.
    s.refused(
        "mark_processed",
        json!({"message_id": &stray, "notes": "handled"}),
    )
    .await
    .says("@thing:kettle")
    .never_says("@thing:handcart");

    s.wrap("set a copy aside").await;
    story.finish().await;
}

/// **"This is waiting on that." Then that gets a better name.**
///
/// A link written as a field under a key nobody declared has to follow the
/// thing it names. The first thing says what it waits on; the second is renamed
/// and its old name is taken by another; the first still reads as waiting on it,
/// under the new name, and the walk reaches it from either end.
#[tokio::test]
async fn a_link_under_a_key_nobody_declared_follows_its_target_through_a_rename() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("work:sigma", "The First Job").await;
    s.add("thing:handcart", "The Handcart").await;
    s.event_with(
        "thing:handcart",
        "waits on the work before it moves",
        json!({ "blocks": "work:sigma" }),
        &[],
    )
    .await;

    // The positive the rename is measured against: before it, the link reads.
    s.recall("thing:handcart").await.says("work:sigma");

    s.call(
        "rename_entity",
        json!({
            "handle": "work:sigma",
            "to": "work:phi",
            "recorded_at": "2026-08-02",
        }),
    )
    .await;

    // **And the old name is taken by something new, before anybody reads the
    // link back.** The link stays with the thing it was written about: it
    // neither moves to the newcomer nor reads as its.
    s.add("work:sigma", "Sigma, the second").await;

    // Read back, the first thing names its target under the name it has now.
    s.recall("thing:handcart")
        .await
        .says("\"blocks\":\"work:phi\"")
        .never_says("\"blocks\":\"work:sigma\"");

    // The walk reaches the renamed thing, out from the one that waits…
    s.shape(
        "what the cart waits on",
        json!({"subject": "thing:handcart", "follow": {"direction": "out"}}),
    )
    .await
    .says("work:phi");
    // …and back from the renamed thing to the one that waits on it…
    s.shape(
        "what waits on the renamed work",
        json!({"subject": "work:phi", "follow": {"direction": "in"}}),
    )
    .await
    .says("thing:handcart");
    // …while the newcomer has nothing waiting on it.
    s.shape(
        "what waits on the newcomer",
        json!({"subject": "work:sigma", "follow": {"direction": "in"}}),
    )
    .await
    .never_says("thing:handcart");

    story.finish().await;
}

/// **"I wrote it the way it is stored." It is turned back.**
///
/// The way jojobot stores a link to a thing belongs to jojobot. A caller who
/// writes it into a field, a claim, a journal beat or a message would draw a link
/// to whatever wears that badge without the check that the thing exists, so each
/// of those is refused with nothing written — and the same sentence written as a
/// handle lands and reads back.
#[tokio::test]
async fn the_way_a_link_is_stored_is_refused_when_a_caller_writes_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("work:sigma", "The First Job").await;
    s.add("thing:handcart", "The Handcart").await;

    s.refused(
        "capture",
        json!({
            "subject": "thing:handcart",
            "content": "waits on the work",
            "provenance": "testimony",
            "fields": { "blocks": "@#k7h2mn" },
        }),
    )
    .await
    .says("blocked");
    s.refused(
        "capture",
        json!({
            "subject": "thing:handcart",
            "content": "waits on @#k7h2mn",
            "provenance": "testimony",
        }),
    )
    .await
    .says("blocked");
    s.refused("journal", json!({"entry": "found @#k7h2mn"}))
        .await
        .says("blocked");
    s.refused(
        "post_message",
        json!({"to": "gamma", "subject": "a link", "body": "see @#k7h2mn"}),
    )
    .await
    .says("blocked");

    // The positive the refusals are measured against: as a handle it lands, and
    // nothing of the refused writes did.
    s.event_with(
        "thing:handcart",
        "waits on @work:sigma",
        json!({ "blocks": "work:sigma" }),
        &[],
    )
    .await;
    s.recall("thing:handcart")
        .await
        .says("\"blocks\":\"work:sigma\"")
        .never_says("k7h2mn");
    story.finish().await;
}

/// **A bot's outbox follows it through a rename.** Mail is stored under the
/// handle its sender wore the day it was sent, and a rename changes the handle
/// and not the bot. Asking after the renamed bot's outbox by the handle it
/// wears now finds the mail it sent under the old one, beside the mail it sends
/// afterwards, and finds nothing of a colleague's.
#[tokio::test]
async fn a_renamed_bots_outbox_still_lists_the_mail_it_sent_under_its_old_handle() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:sigma", "Sigma").await;
    s.add("bot:omega", "Omega").await;
    let sigma = story.as_bot("bot:sigma").await;
    let omega = story.as_bot("bot:omega").await;

    // Mail sent under the handle the bot wears today, and a colleague's beside
    // it, which no question about the renamed bot may return.
    let before = sigma
        .post("otto", "before the rename", "the kiln is relined")
        .await;
    let colleagues = omega.post("otto", "not sigma's", "the flue is clear").await;
    assert_eq!(
        s.call("list_sent", json!({"sender": "bot:sigma"}))
            .await
            .number("/count", 1)
            .json()["messages"][0]["id"],
        before.as_str(),
        "before the rename the outbox holds the one message"
    );

    s.call(
        "rename_entity",
        json!({"handle": "bot:sigma", "to": "bot:psi", "recorded_at": "2026-08-02"}),
    )
    .await;

    // ── asked by the handle the bot wears now ───────────────────────────────
    // An id is a short counter, so it is matched as the `id` entry it is and
    // not as a digit that any timestamp also carries.
    let id = |id: &str| format!("\"id\":\"{id}\"");
    let listed = s.call("list_sent", json!({"sender": "bot:psi"})).await;
    listed
        .number("/count", 1)
        .says(&id(&before))
        .says("the kiln is relined")
        .never_says(&id(&colleagues))
        .never_says("the flue is clear");
    assert_eq!(
        listed.json()["messages"][0]["sender"],
        "bot:psi",
        "the sender reads as the handle it wears now: {}",
        listed.json()
    );

    // ── and the mail it sends afterwards joins the same listing ─────────────
    let psi = story.as_bot("bot:psi").await;
    let after = psi
        .post("otto", "after the rename", "the damper is hand-cut")
        .await;
    let both = s.call("list_sent", json!({"sender": "bot:psi"})).await;
    both.number("/count", 2)
        .says(&id(&before))
        .says(&id(&after));

    s.wrap("renamed a bot and asked after its outbox").await;
    story.finish().await;
}

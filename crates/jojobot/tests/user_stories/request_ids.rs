//! "My connection dropped before the answer came back. Did that write land?"
//!
//! **A write that carries a `request_id` can be sent again.** If the first call
//! landed, the second one returns the first one's receipt, marked as a repeat,
//! and writes nothing. If the first one wrote nothing, the second one runs.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// How many claims `person:milhouse` holds.
async fn claims(s: &Session) -> usize {
    s.shape(
        "what Milhouse holds",
        json!({ "subject": "person:milhouse", "facts": true }),
    )
    .await
    .json()["objects"][0]["facts"]
        .as_array()
        .map_or(0, Vec::len)
}

fn capture(content: &str, request_id: &str) -> Value {
    json!({
        "subject": "person:milhouse", "content": content,
        "provenance": "testimony", "request_id": request_id,
    })
}

/// The receipt without what a repeat adds to it.
fn without_repeat_marks(mut receipt: Value) -> Value {
    if let Some(body) = receipt.as_object_mut() {
        for key in ["repeat", "repeat_note", "arguments_differed", "first_verb"] {
            body.remove(key);
        }
    }
    receipt
}

#[tokio::test]
async fn a_write_sent_again_returns_the_first_receipt_and_writes_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;

    let first = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    // The positive the negative rests on: the first call wrote.
    assert_eq!(claims(&s).await, 1);
    assert!(
        first.get("repeat").is_none(),
        "a first call is not a repeat: {first}"
    );

    let again = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert_eq!(again["repeat"], true, "the answer is marked: {again}");
    assert_eq!(
        without_repeat_marks(again.clone()),
        without_repeat_marks(first.clone()),
        "the answer is the first receipt"
    );
    assert!(
        again.get("arguments_differed").is_none(),
        "the same arguments did not differ: {again}"
    );
    assert_eq!(claims(&s).await, 1, "nothing was written the second time");

    story.finish().await;
}

#[tokio::test]
async fn the_same_id_from_another_bot_writes_normally() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.call("capture", capture("wears glasses", "r1")).await;
    assert_eq!(claims(&s).await, 1);

    let other = story.as_bot("bot:assistant").await;
    let theirs = other
        .call("capture", capture("owns a radio", "r1"))
        .await
        .json();
    assert!(
        theirs.get("repeat").is_none(),
        "another bot's id is its own: {theirs}"
    );
    assert_eq!(claims(&s).await, 2);

    story.finish().await;
}

#[tokio::test]
async fn an_id_sent_with_other_arguments_returns_the_first_receipt_and_says_so() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    let first = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();

    let again = s
        .call("capture", capture("owns a radio", "r1"))
        .await
        .json();
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(again["arguments_differed"], true, "{again}");
    assert_eq!(
        without_repeat_marks(again),
        without_repeat_marks(first),
        "the first receipt comes back"
    );
    assert_eq!(claims(&s).await, 1, "the second claim was not written");

    story.finish().await;
}

#[tokio::test]
async fn a_restart_between_the_two_calls_still_recognises_the_repeat() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    let first = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();

    let restarted = story.restarted().await;
    // The same bot, in a run of the new process.
    let s = restarted.as_bot("bot:otto").await;
    let again = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(
        without_repeat_marks(again),
        without_repeat_marks(first),
        "the first receipt survives the restart"
    );
    assert_eq!(claims(&s).await, 1);

    restarted.finish().await;
}

#[tokio::test]
async fn a_write_that_stored_nothing_spends_no_id() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // Milhouse is not known yet: the write is blocked and stores nothing.
    let blocked = s
        .refused("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert!(blocked.get("repeat").is_none(), "{blocked}");

    // The same id, once the subject exists, is a first call.
    s.add("person:milhouse", "Milhouse").await;
    let landed = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert!(
        landed.get("repeat").is_none(),
        "the blocked call spent nothing: {landed}"
    );
    assert_eq!(claims(&s).await, 1);

    story.finish().await;
}

#[tokio::test]
async fn an_id_needs_a_session_and_a_good_shape() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;

    // No session: refused, and nothing written.
    let (anonymous, _) = story
        .call_either(
            "capture",
            json!({
                "subject": "person:milhouse", "content": "wears glasses",
                "provenance": "testimony", "request_id": "r1",
            }),
        )
        .await;
    anonymous.says("request_id");
    assert_eq!(claims(&s).await, 0);

    // A shape no id has.
    for bad in ["", " r1", &"x".repeat(129)] {
        s.refused("capture", capture("wears glasses", bad)).await;
    }
    assert_eq!(claims(&s).await, 0);

    // An id is counted in bytes: a hundred two-byte letters are 200 bytes, which is
    // over the limit, and the refusal says the limit is in bytes.
    s.refused("capture", capture("wears glasses", &"é".repeat(100)))
        .await
        .says("bytes")
        .never_says("characters");
    assert_eq!(claims(&s).await, 0);

    // The positive: 64 of them are 128 bytes, the most an id may be, and a plain id
    // lands too.
    s.call("capture", capture("owns a radio", &"é".repeat(64)))
        .await;
    s.call("capture", capture("wears glasses", "r1")).await;
    assert_eq!(claims(&s).await, 2);

    story.finish().await;
}

/// A write sent twice under one id leaves one write behind, and the second
/// answer is the first one's receipt.
fn is_a_repeat(answer: &Value) {
    assert_eq!(answer["repeat"], true, "the answer is marked: {answer}");
}

#[tokio::test]
async fn an_entity_made_twice_under_one_id_is_made_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let make = |id: &str| {
        json!({
            "kind": "person", "handle": "lisa", "name": "Lisa",
            "source": "user-named", "request_id": id,
        })
    };
    let first = s.call("add_entity", make("e1")).await.json();
    assert!(first.get("repeat").is_none(), "{first}");
    // Without the id the same call is a near-miss of the thing just made: that is
    // what the repeat saves a caller from meeting.
    let again = s.call("add_entity", make("e1")).await.json();
    is_a_repeat(&again);
    assert_eq!(
        without_repeat_marks(again),
        without_repeat_marks(first),
        "the first receipt"
    );

    // A thing made with its first fields commits the thing and its first claim
    // together; the id covers both.
    let with_fields = json!({
        "kind": "person", "handle": "homer", "name": "Homer", "source": "user-named",
        "sets": { "mood": "content" }, "request_id": "e3",
    });
    s.call("add_entity", with_fields.clone()).await;
    is_a_repeat(&s.call("add_entity", with_fields).await.json());
    // A rename of the label alone is an edit that sets nothing.
    let relabel = json!({ "handle": "person:homer", "name": "Homer J", "request_id": "e4" });
    s.call("update_entity", relabel.clone()).await;
    is_a_repeat(&s.call("update_entity", relabel).await.json());

    // A bot is made in two parts, so it takes no id.
    s.refused(
        "add_entity",
        json!({
            "kind": "bot", "handle": "rho", "name": "Rho",
            "source": "user-named", "request_id": "e2",
        }),
    )
    .await
    .says("request_id");
    // The positive: the same bot without an id is made.
    s.call(
        "add_entity",
        json!({ "kind": "bot", "handle": "rho", "name": "Rho", "source": "user-named" }),
    )
    .await;

    story.finish().await;
}

#[tokio::test]
async fn an_edit_sent_twice_under_one_id_counts_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    // A counter adds every write, so a doubled edit would show as a doubled total.
    s.call(
        "declare_type",
        json!({ "name": "snacking", "fields": [{ "key": "donuts", "folds": "sum" }] }),
    )
    .await;
    let claim = s
        .event_with("person:milhouse", "ate lunch", json!({}), &[])
        .await;

    let edit = |id: &str| json!({ "address": claim, "sets": { "donuts": "1" }, "request_id": id });
    s.call("update_fact", edit("u1")).await;
    is_a_repeat(&s.call("update_fact", edit("u1")).await.json());
    s.shape(
        "what Milhouse holds",
        json!({ "subject": "person:milhouse" }),
    )
    .await
    .says("\"donuts\":\"1\"");

    // The same on the entity's own edit.
    let update = |id: &str| json!({ "handle": "person:milhouse", "sets": { "donuts": "1" }, "request_id": id });
    s.call("update_entity", update("u2")).await;
    is_a_repeat(&s.call("update_entity", update("u2")).await.json());
    s.shape(
        "what Milhouse holds",
        json!({ "subject": "person:milhouse" }),
    )
    .await
    .says("\"donuts\":\"2\"");

    story.finish().await;
}

#[tokio::test]
async fn an_archive_a_merge_and_a_retraction_sent_twice_under_one_id_are_made_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:krusty", "Krusty").await;
    s.add("person:skinner", "Skinner").await;
    s.add("person:apu", "Apu").await;
    let claim = s
        .event_with("person:apu", "runs the store", json!({}), &[])
        .await;

    // Archive: the second call, without the id, would meet an archived thing.
    let archive = json!({ "handle": "person:apu", "reason": "filed twice", "request_id": "a1" });
    s.call("archive_entity", archive.clone()).await;
    is_a_repeat(&s.call("archive_entity", archive).await.json());

    // Restore: the second call, without the id, would meet a thing not archived.
    let restore = json!({
        "handle": "person:apu", "reason": "back again", "restore": true, "request_id": "a2",
    });
    s.call("archive_entity", restore.clone()).await;
    is_a_repeat(&s.call("archive_entity", restore).await.json());

    // Merge: the second call, without the id, would meet a thing already folded.
    let merge = json!({
        "duplicate": "person:skinner", "survivor": "person:krusty", "request_id": "m1",
    });
    s.call("merge_entities", merge.clone()).await;
    is_a_repeat(&s.call("merge_entities", merge).await.json());

    // Retract: the second call, without the id, would meet a claim taken back.
    let retract = json!({ "address": claim, "reason": "mistaken", "request_id": "t1" });
    s.call("retract", retract.clone()).await;
    is_a_repeat(&s.call("retract", retract).await.json());

    story.finish().await;
}

#[tokio::test]
async fn a_type_and_a_charter_sent_twice_under_one_id_keep_the_first() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let declare =
        |key: &str| json!({ "name": "snacking", "fields": [{ "key": key }], "request_id": "d1" });
    s.call("declare_type", declare("donuts")).await;
    let again = s.call("declare_type", declare("buns")).await.json();
    is_a_repeat(&again);
    assert_eq!(again["arguments_differed"], true, "{again}");
    // The second declaration was not made: the type still names the first key.
    s.shape(
        "the type",
        json!({ "answers_type": "snacking", "kind": "person" }),
    )
    .await
    .says("donuts");

    let charter =
        |prose: &str| json!({ "bot": "bot:assistant", "prose": prose, "request_id": "c1" });
    s.call("set_charter", charter("The first words.")).await;
    let again = s
        .call("set_charter", charter("Different words."))
        .await
        .json();
    is_a_repeat(&again);
    assert_eq!(again["arguments_differed"], true, "{again}");
    s.shape(
        "the charter as written",
        json!({ "subject": "bot:assistant", "prose": true }),
    )
    .await
    .says("The first words.")
    .never_says("Different words.");

    story.finish().await;
}

#[tokio::test]
async fn a_message_posted_and_retired_twice_under_one_id_is_posted_and_retired_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let other = story.as_bot("bot:assistant").await;

    let post = json!({
        "to": "bot:otto", "subject": "hello", "body": "first words", "request_id": "p1",
    });
    let first = other.call("post_message", post.clone()).await.json();
    assert!(first.get("repeat").is_none(), "{first}");
    is_a_repeat(&other.call("post_message", post).await.json());
    // One message reached the box, not two.
    let counts = s
        .call("read_mailbox", json!({ "counts_only": true }))
        .await
        .json();
    assert_eq!(counts["counts"]["new"], 1, "{counts}");

    // Setting a message aside twice under one id is one quarantine.
    other
        .call(
            "post_message",
            json!({ "to": "bot:otto", "subject": "second", "body": "set aside" }),
        )
        .await;
    let delivered = s.call("read_mailbox", json!({})).await.json();
    let aside = delivered["messages"]
        .as_array()
        .and_then(|all| all.iter().find(|m| m["subject"] == "second"))
        .and_then(|m| m["id"].as_str())
        .expect("the second message arrived")
        .to_string();
    let quarantine = json!({
        "message_id": aside, "quarantine": "not for me", "request_id": "q1",
    });
    s.call("mark_processed", quarantine.clone()).await;
    is_a_repeat(&s.call("mark_processed", quarantine).await.json());

    // Retire the first one twice under one id with other notes the second time.
    let id = delivered["messages"]
        .as_array()
        .and_then(|all| all.iter().find(|m| m["subject"] == "hello"))
        .and_then(|m| m["id"].as_str())
        .expect("the first message arrived")
        .to_string();
    let retire = |notes: &str| json!({ "message_id": id, "notes": notes, "request_id": "r9" });
    s.call("mark_processed", retire("done the first way")).await;
    let again = s
        .call("mark_processed", retire("done another way"))
        .await
        .json();
    is_a_repeat(&again);
    assert_eq!(again["arguments_differed"], true, "{again}");
    s.call("read_message", json!({ "message_id": id }))
        .await
        .says("done the first way")
        .never_says("done another way");

    story.finish().await;
}

/// What the run has been charged and told so far. **The characters are read the
/// way a caller reads them**, from `list_runs`. **The beat is read from the
/// store**, because no verb that leaves the run open serves the text of a beat
/// that jojobot wrote: `recall` of the run gives only how many beats it holds,
/// and a repeat that replayed a beat would add to the tally in a beat's text and
/// not to that count. Reading charges the run too, which is why the case below
/// measures what a read costs on its own.
async fn bookkeeping(story: &Story, s: &Session) -> (u64, Option<String>) {
    use jojobot_domain::session::Sessions;
    let runs = s.call("list_runs", json!({})).await.json();
    let served = runs["runs"]
        .as_array()
        .and_then(|all| all.iter().find(|run| run["run"] == s.run()))
        .and_then(|run| run["served"]["characters"].as_u64())
        .unwrap_or_else(|| panic!("list_runs does not say what the run was handed: {runs}"));
    let held = story
        .sessions_store()
        .all_sessions()
        .await
        .expect("the board reads");
    let beat = held
        .iter()
        .find(|run| {
            run.sid
                .as_ref()
                .is_some_and(|name| name.as_str() == s.run())
        })
        .and_then(|run| {
            run.entries
                .iter()
                .find(|entry| entry.beat.as_deref() == Some("capture"))
        })
        .map(|entry| entry.text.clone());
    (served, beat)
}

/// **A repeat is answered and charges the run nothing, and replays no beat.**
/// Both reads are charged, so the case first measures what a pair of reads costs
/// with nothing between them. A repeat between two reads adds less than its own
/// answer; a capture that is not a repeat adds at least that much, and moves the
/// beat. The second half is the positive the first rests on.
#[tokio::test]
async fn a_repeat_replays_no_beat_and_no_accounting() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.call("capture", capture("wears glasses", "r1")).await;
    let (first, beat) = bookkeeping(&story, &s).await;
    assert!(beat.is_some(), "the first capture left its beat");

    // What a pair of reads costs, with nothing between them.
    let (second, still) = bookkeeping(&story, &s).await;
    let reading = second - first;
    assert!(reading > 0, "a read is charged: {first} then {second}");
    assert_eq!(still, beat, "reading moved no beat");

    let repeat = s.call("capture", capture("wears glasses", "r1")).await;
    is_a_repeat(&repeat.json());
    let answered = repeat.raw().chars().count() as u64;
    let (third, after_repeat) = bookkeeping(&story, &s).await;
    assert!(
        third - second < reading + answered / 2,
        "a repeat of {answered} characters was charged: reads cost {reading}, then {}",
        third - second
    );
    assert_eq!(after_repeat, beat, "a repeat replays no beat");

    // The positive: a capture that is not a repeat is charged and moves the beat.
    let fresh = s.call("capture", capture("owns a radio", "r2")).await;
    let charged = fresh.raw().chars().count() as u64;
    let (fourth, after_fresh) = bookkeeping(&story, &s).await;
    assert!(
        fourth - third > reading + charged / 2,
        "a new write of {charged} characters is charged: reads cost {reading}, then {}",
        fourth - third
    );
    assert_ne!(after_fresh, beat, "a new write moves the beat's count");

    story.finish().await;
}

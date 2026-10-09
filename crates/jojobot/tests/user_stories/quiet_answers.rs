//! "Why does every answer carry a dozen keys that say nothing?"
//!
//! **An answer says what is there, and a key with no value is not there.** A
//! claim recorded without a day, an entity with no parent, a boot with no
//! session yet: each used to arrive as `"key":null`, on every answer, for every
//! verb. An absent key says the same and costs nothing. The few nulls that mean
//! something stay, by name.
//!
//! The pin is over the ordinary answers of the ordinary verbs, because a
//! null-free recall proves nothing about a null-free capture.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_ordinary_answers_of_the_ordinary_verbs_carry_no_null_keys() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // A write of each shape the verbs have: an entity, a claim, a correction.
    let made = s
        .call(
            "add_entity",
            json!({"kind": "person", "handle": "lisa", "name": "Lisa", "source": "user-named"}),
        )
        .await;
    let captured = s
        .call(
            "capture",
            json!({"subject": "person:lisa", "content": "likes the diner", "provenance": "testimony"}),
        )
        .await;
    let recalled = s
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await;
    let found = s.call("search", json!({"query": "diner"})).await;
    let listed = s.call("list_entities", json!({"kind": "person"})).await;

    for (verb, answer) in [
        ("add_entity", &made),
        ("capture", &captured),
        ("recall", &recalled),
        ("search", &found),
        ("list_entities", &listed),
    ] {
        answer.never_says(":null");
        // The positive each absence rests on: the answer is not empty.
        assert!(
            answer.size() > 40,
            "{verb} answered nothing: {}",
            answer.raw()
        );
    }
    s.wrap("looked at what a plain answer carries").await;

    // The claim is there and says what it holds; what it does not hold is not
    // spelled out.
    recalled.says("likes the diner");
    recalled.never_says("happened_at");

    story.finish().await;
}

/// "I read a lot before I wrote anything. Did I miss what the claims are for?"
/// You did not: the teaching about claims rides on the write verbs, so reading
/// does not spend it, and the first write after any number of reads carries it.
#[tokio::test]
async fn a_session_that_reads_first_is_taught_on_its_first_write() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    writer.add("person:lisa", "Lisa").await;
    writer.fact("person:lisa", "likes the diner").await;
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // ── reading teaches nothing about claims, however much of it there is ────
    reader
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await
        .says("likes the diner")
        .never_says("\"teaching\"");
    reader
        .call("search", json!({"query": "diner"}))
        .await
        .says("likes the diner")
        .never_says("\"teaching\"");

    // ── the first write carries it ───────────────────────────────────────────
    reader
        .call(
            "capture",
            json!({"subject": "person:lisa", "content": "orders the pie", "provenance": "testimony"}),
        )
        .await
        .says("\"teaching\"")
        .says("does not destroy the one already there");

    story.finish().await;
}

/// "I searched twice and it said the same long caveat twice." It says it once:
/// the caveat about what matching can miss rides the first search of a session
/// and any answer too thin to trust, and is left out of a second full answer.
#[tokio::test]
async fn a_second_full_search_leaves_the_caveats_out_and_an_empty_one_brings_them_back() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    for slug in ["alpha", "beta", "gamma"] {
        writer
            .add(&format!("place:{slug}"), &format!("The {slug} Diner"))
            .await;
    }
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // Three hits is a full answer; the first of the session carries the caveat.
    let first = reader.call("search", json!({"query": "diner"})).await;
    first.number("/count", 3).says("\"corpus\"");
    // The second does not, and still says what the query was.
    let second = reader.call("search", json!({"query": "diner"})).await;
    second.number("/count", 3).never_says("\"corpus\"");
    second.says("\"exact\"");
    // An empty answer is where the gap shows, so it brings the caveat back.
    let empty = reader
        .call("search", json!({"query": "zzzqxnomatch"}))
        .await;
    empty.number("/count", 0).says("\"corpus\"");

    story.finish().await;
}

/// "I asked who backs two keys of the pump and got the whole pedigree, with
/// one long note shipped four times." Backing obeys `keys` as the fields do,
/// and a claim that backs several keys says its note once.
#[tokio::test]
async fn backing_answers_the_keys_asked_for_and_a_shared_claims_note_arrives_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("thing:floor-pump", "The Floor Pump").await;
    // One claim backs four keys and carries a long note; a second claim backs
    // a fifth key and carries none.
    let shared = s
        .call(
            "capture",
            json!({
                "subject": "thing:floor-pump",
                "content": "what the pump is",
                "provenance": "testimony",
                "details": "read off the box in the shed, and wobbleprobe is the one word to count",
                "fields": {
                    "pressure": "90",
                    "hose": "braided",
                    "valve": "presta",
                    "gauge": "dial",
                },
            }),
        )
        .await
        .field("address");
    s.call(
        "capture",
        json!({
            "subject": "thing:floor-pump",
            "content": "what colour the pump is",
            "provenance": "testimony",
            "fields": {"colour": "red"},
        }),
    )
    .await;

    // ── two keys asked for: backing for those two, the note once ────────────
    let asked = s
        .shape(
            "who backs the pressure and the hose",
            json!({"subject": "thing:floor-pump", "keys": ["pressure", "hose"], "backing": true}),
        )
        .await;
    let object = &asked.json()["objects"][0];
    let claim = shared.rsplit('#').next().expect("an address names a claim");
    let mut backed: Vec<&str> = object["fields_backing"]
        .as_object()
        .expect("backing was asked for")
        .keys()
        .map(String::as_str)
        .collect();
    backed.sort_unstable();
    assert_eq!(backed, ["hose", "pressure"], "{}", asked.raw());
    for key in ["pressure", "hose"] {
        assert_eq!(
            object["fields_backing"][key]["claim"],
            claim,
            "each key names the claim that backs it: {}",
            asked.raw(),
        );
    }
    assert_eq!(
        asked.raw().matches("wobbleprobe").count(),
        1,
        "the shared claim's note is shipped once, and it is shipped: {}",
        asked.raw(),
    );
    assert!(
        object["fields_backing_notes"][claim]
            .as_str()
            .is_some_and(|note| note.contains("wobbleprobe")),
        "the note is found by the claim that carries it: {}",
        asked.raw(),
    );
    // The keys left out are named, for the fields and for their backing.
    for left_out in ["valve", "gauge", "colour"] {
        for place in ["fields_left_out", "fields_backing_left_out"] {
            assert!(
                object[place]
                    .as_str()
                    .is_some_and(|said| said.contains(left_out)),
                "{place} does not name {left_out}: {}",
                asked.raw(),
            );
        }
    }

    // ── no keys asked for: all five are backed, the note is still once ──────
    let whole = s
        .shape(
            "who backs everything about the pump",
            json!({"subject": "thing:floor-pump", "backing": true}),
        )
        .await;
    let all = whole.json()["objects"][0]["fields_backing"]
        .as_object()
        .expect("backing was asked for")
        .len();
    assert_eq!(all, 5, "{}", whole.raw());
    assert_eq!(
        whole.raw().matches("wobbleprobe").count(),
        1,
        "{}",
        whole.raw()
    );
    assert!(
        whole.json()["objects"][0]
            .get("fields_backing_left_out")
            .is_none(),
        "nothing was narrowed, so nothing is said to be left out: {}",
        whole.raw(),
    );

    story.finish().await;
}

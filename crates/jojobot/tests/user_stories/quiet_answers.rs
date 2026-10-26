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
/// You did not: the first answer that shows a session a claim carries the
/// teaching about claims, before it decides how to write one. It rides once, so
/// the next read and the first write after any number of reads do not repeat it.
#[tokio::test]
async fn a_session_that_reads_first_is_taught_on_its_first_read_of_a_claim() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    writer.add("person:lisa", "Lisa").await;
    writer.fact("person:lisa", "likes the diner").await;
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // ── a read that shows no claim teaches nothing about claims ──────────────
    reader
        .call("recall", json!({"subject": "person:lisa"}))
        .await
        .never_says("\"teaching\"");

    // ── the first read that shows a claim carries it ─────────────────────────
    let first = reader
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await;
    first.says("likes the diner");
    let taught = first.teachings();
    assert!(!taught.is_empty(), "the first read of a claim teaches");

    // ── and no later answer does, whichever verb comes next ──────────────────
    let found = reader.call("search", json!({"query": "diner"})).await;
    found.says("likes the diner");
    found.never_says("\"teaching\"");
    // A capture may carry teachings of its own; the claims teaching is not among
    // them.
    let written = reader
        .call(
            "capture",
            json!({"subject": "person:lisa", "content": "orders the pie", "provenance": "testimony"}),
        )
        .await;
    for line in &taught {
        assert!(
            !written.teachings().contains(line),
            "a session taught about claims is not taught again: {}",
            written.raw()
        );
    }

    story.finish().await;
}

/// "My first look at the diner was a search." A search that surfaces a claim
/// carries the same teaching once, and a session that first met a claim there is
/// not taught again by the recall that follows.
#[tokio::test]
async fn a_session_whose_first_look_at_a_claim_is_a_search_is_taught_there() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    writer.add("person:lisa", "Lisa").await;
    writer.fact("person:lisa", "likes the diner").await;
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // The teaching about claims, as a session that meets a claim by recall
    // first is handed it: the reference this search is held to.
    writer.add("bot:gamma", "Gamma").await;
    let claims = story
        .as_bot("bot:gamma")
        .await
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await
        .teachings();
    assert!(!claims.is_empty(), "a first read of a claim teaches");

    let found = reader.call("search", json!({"query": "diner"})).await;
    found.says("likes the diner");
    let taught = found.teachings();
    for line in &claims {
        assert!(
            taught.contains(line),
            "the first search to show a claim carries the teaching about claims: {}",
            found.raw()
        );
    }
    let recalled = reader
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await;
    recalled.says("likes the diner");
    for line in &claims {
        assert!(
            !recalled.teachings().contains(line),
            "the recall that follows is not taught again: {}",
            recalled.raw()
        );
    }

    story.finish().await;
}

/// "I searched twice and it said the same long caveat twice." It says it once:
/// the caveat about what matching can miss rides the first search of a session,
/// and a later answer leaves it out, an empty one included; the empty one says
/// what it looked through.
#[tokio::test]
async fn a_second_full_search_leaves_the_caveats_out_and_an_empty_one_says_what_it_searched() {
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
    // Notes are taught once for each session and wording, so an empty answer does
    // not teach the caveat again; it says what it looked through and left out.
    let empty = reader
        .call("search", json!({"query": "zzzqxnomatch"}))
        .await;
    empty.number("/count", 0).never_says("\"corpus\"");
    assert!(
        empty.json()["searched"].is_string(),
        "an empty answer says what it looked through: {}",
        empty.raw()
    );

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
                "sets": {
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
            "sets": {"colour": "red"},
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
    // The keys left out are named once, for the answer, and no object repeats it.
    for place in ["fields_left_out", "fields_backing_left_out"] {
        assert!(
            object.get(place).is_none(),
            "{place} on the object: {}",
            asked.raw()
        );
    }
    assert_eq!(
        asked.json()["narrowed"]["left_out_keys"],
        json!(["colour", "gauge", "valve"]),
        "{}",
        asked.raw(),
    );

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

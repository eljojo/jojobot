//! "I have never declared a type. Why does jojobot already know what a trip
//! is?"
//!
//! **Because the software ships its own vocabulary.** A session that was told
//! nothing writes `leaves_on` and `cadence_days` and finds that the questions
//! over them already work — where a vocabulary each instance had to declare for
//! itself is one every instance spells differently, and nothing written under
//! one spelling is reachable from another.
//!
//! It is also what gives the protection over shipped names something to guard.
//! Until the software declared a type of its own, no caller could meet that
//! refusal and no caller could see an origin that was not its own: the only way
//! to learn a name was closed was to write over it and read the answer.
//!
//! **And the two type questions are answered here on one thing**, which is
//! where the difference between them stops being a doc note. A trip somebody
//! has planned only half of carries no dates: it ANSWERS the type, with the
//! gaps named, and it does not FIT it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_software_ships_a_vocabulary_a_session_never_declared() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    // ── the things ──────────────────────────────────────────────────────────
    s.add("thing:gravel-bike", "The Gravel Bike").await;
    s.add("thing:torque-wrench", "The Torque Wrench").await;
    s.add(
        "event:gorge-safety-inspection",
        "The Gorge Safety Inspection",
    )
    .await;
    s.add("event:krusty-fest", "The Krusty Fest").await;
    s.add("place:springfield", "Springfield").await;
    s.add("place:daredevil-gorge", "The Daredevil Gorge").await;

    // ── a session writes the shipped keys, having declared nothing ───────────
    //
    // **A cadence is TIME.** The chain gets looked at every sixty days; how far
    // the bike went between them is something a check-in measures, never a unit
    // of the schedule — so there is no distance here and no unit key.
    s.event_with(
        "thing:gravel-bike",
        "chain checked, still within wear",
        json!({
            "cadence_days": "60",
            "advances_from": "the day it happened",
            "counts_from": "2026-06-30",
            "last_check_in": "2026-06-30",
            "outcome": "no play in it",
        }),
        &[],
    )
    .await;

    // The same loop, set up and never run. This is the one both questions get
    // asked about.
    s.event_with(
        "thing:torque-wrench",
        "wants calibrating twice a year, nobody has yet",
        json!({ "cadence_days": "180", "advances_from": "the day it fell due" }),
        &[],
    )
    .await;

    // A trip, in the shipped words: where from, where to, and the two dates.
    s.event_with(
        "event:gorge-safety-inspection",
        "four days up the gorge",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:daredevil-gorge",
            "leaves_on": "2026-09-03",
            "returns_on": "2026-09-07",
        }),
        &[],
    )
    .await;

    // Half a trip: where from and where to, and nobody has picked the dates.
    s.event_with(
        "event:krusty-fest",
        "we are going, at some point",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:daredevil-gorge",
        }),
        &[],
    )
    .await;

    // ── the tolerant question: what is described like a trip ────────────────
    let described = s
        .call("search", json!({ "answers_type": "trip", "limit": 50 }))
        .await;
    // The positive the strict answer below rests on: nobody declared `trip` on
    // this instance and the question still works.
    described.says("event:gorge-safety-inspection");
    described.says("\"complete\":true");
    // ⭐ The half-planned one comes back, saying what it lacks BY NAME — which
    // is what a reader acts on. A count would leave them nothing to fill in.
    described.says("event:krusty-fest");
    described.says("\"lacking\":[\"leaves_on\",\"returns_on\"]");
    // …and the negative in the same answer that just proved it is not empty:
    // the bike shares no key with a trip, so it is not a weak match.
    described.never_says("thing:gravel-bike");

    // ── the strict question, about the same two things ──────────────────────
    //
    // ⭐ **The difference between the two, on one thing.** The krusty fest
    // answered the type a moment ago and is not here, because a trip with no
    // dates does not FIT one. Which of the two questions you are asking is
    // yours to choose, and the tolerant one is what you get when you name
    // neither.
    let are_trips = s
        .call("search", json!({ "fits_type": "trip", "limit": 50 }))
        .await;
    are_trips.says("event:gorge-safety-inspection");
    are_trips.never_says("event:krusty-fest");
    are_trips.never_says("thing:gravel-bike");

    // **And a key written on one thing over two sittings reads off the KEY.**
    // The fold takes the newest write of each, so this one row answers without
    // reading a single record back.
    let planned = s
        .shape(
            "the trips and when each leaves",
            json!({ "answers_type": "trip" }),
        )
        .await;
    planned.says("\"leaves_on\":\"2026-09-03\"");
    planned.says("\"arrives_at\":\"place:daredevil-gorge\"");

    // ── a type of the session's own, beside the ones it was given ───────────
    let declared = s
        .call(
            "declare_type",
            json!({
                "name": "loan",
                "fields": [
                    { "key": "borrowed_by", "holds": "reference" },
                    { "key": "due_back", "holds": "date" },
                ],
            }),
        )
        .await;
    // **Both origins in one read.** An answer saying `declared` on everything
    // would pass a beat that only ever looked at the caller's own type, and a
    // caller that cannot tell the two apart learns which names are closed by
    // being refused — the refusal springs rather than being avoidable.
    declared.says("\"name\":\"loan\",\"origin\":\"declared\"");
    declared.says("\"name\":\"trip\",\"origin\":\"shipped\"");

    // ── and the name the software owns is closed ────────────────────────────
    let refused = s
        .refused(
            "declare_type",
            json!({
                "name": "trip",
                "fields": [{ "key": "cadence_km", "holds": "number" }],
            }),
        )
        .await;
    // It names the type it is about and the verb to call with a name of your
    // own, because re-sending this call will never work.
    refused.says("trip");
    refused.says("declare_type");

    // The write really was refused, and the vocabulary is read back rather than
    // believed — through calls that WRITE NOTHING. A re-read that declared
    // anything would be asking what the types are by changing one of them.
    //
    // The list first: asking a question of a type nobody declared comes back
    // naming every type that does exist, which is how a session learns what the
    // vocabulary is without declaring something to find out.
    let listed = s
        .refused("search", json!({ "answers_type": "warranty" }))
        .await;
    listed.says("Types that do exist");
    listed.says("trip");
    // …and the keys under that name are the ones the software shipped: the
    // half-planned trip still lacks exactly the two it never wrote, and the key the
    // refused call would have replaced them with is nowhere.
    let still = s
        .call("search", json!({ "answers_type": "trip", "limit": 50 }))
        .await;
    still.says("\"lacking\":[\"leaves_on\",\"returns_on\"]");
    still.never_says("cadence_km");

    s.wrap("used the vocabulary the software came with, and found its own name refused")
        .await;
    story.finish().await;
}

#[tokio::test]
async fn a_cold_session_asking_for_a_type_nothing_holds_is_told_its_keys_and_writes_under_them() {
    // **A session that was told nothing.** The boot lists the shipped types by
    // name only, so the first question it can ask is `answers_type: trip`, and
    // an empty answer that said nothing more sent it away without the spelling
    // of a single key.
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add(
        "event:gorge-safety-inspection",
        "The Gorge Safety Inspection",
    )
    .await;
    s.add("place:springfield", "Springfield").await;
    s.add("place:daredevil-gorge", "The Daredevil Gorge").await;

    // ── nothing is a trip yet, and the answer says what one is ──────────────
    let asked = s
        .call("recall", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert!(
        asked["objects"].as_array().is_some_and(Vec::is_empty),
        "nothing carries a trip's keys yet, or this proves nothing: {asked}"
    );
    let described = asked["type_keys"]
        .as_array()
        .and_then(|types| types.iter().find(|t| t["name"] == "trip"))
        .unwrap_or_else(|| {
            panic!("the empty answer does not describe the type it was asked about: {asked}")
        });
    let keys = described["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("the type's keys are a list: {described}"));
    let named: Vec<&str> = keys.iter().filter_map(|k| k["key"].as_str()).collect();
    for key in ["departs_from", "arrives_at", "leaves_on", "returns_on"] {
        assert!(named.contains(&key), "{key} is not named in {described}");
    }

    // ── a session writes under exactly the keys it was told, by what each holds ─
    let mut fields = serde_json::Map::new();
    for key in keys {
        let name = key["key"].as_str().expect("a key has a name");
        let value = match key["holds"].as_str().expect("a key says what it holds") {
            "date" => "2026-09-03",
            "reference" => "place:springfield",
            other => panic!("a trip key holds {other}, which this story cannot write"),
        };
        fields.insert(name.to_string(), json!(value));
    }
    s.event_with(
        "event:gorge-safety-inspection",
        "four days up the gorge",
        serde_json::Value::Object(fields),
        &[],
    )
    .await;

    // ── and the keys it wrote are the ones the store asks by ────────────────
    let found = s
        .call(
            "recall",
            json!({ "fields": [{ "key": "leaves_on", "value": "2026-09-03" }] }),
        )
        .await
        .json();
    assert_eq!(
        found["objects"][0]["id"], "event:gorge-safety-inspection",
        "a thing written under the keys the answer named is found by them: {found}"
    );
    story.finish().await;
}

#[tokio::test]
async fn the_first_empty_read_of_a_shipped_type_says_to_capture_under_its_keys_and_says_it_once() {
    // **A session that read, concluded, and kept the conclusion in prose.** An
    // empty answer under a type the software ships is the moment it decides
    // whether to write what it just worked out. The note rides that one answer,
    // once, and never on an answer that held something or a type the caller
    // declared.
    let story = Story::begin("bot:gamma").await;

    // ── a session that reads before it writes ───────────────────────────────
    let s = story.session().await;
    let first = s
        .call("recall", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert!(
        first["objects"].as_array().is_some_and(Vec::is_empty),
        "nothing carries a trip's keys yet, or this proves nothing: {first}"
    );
    let taught = first["teaching"]
        .as_array()
        .unwrap_or_else(|| panic!("the first empty read of a shipped type teaches: {first}"));
    assert_eq!(taught.len(), 1, "one note, once: {first}");
    // The answer still carries the keys to write under, beside the note.
    assert!(first["type_keys"][0]["name"] == "trip", "{first}");

    let again = s
        .call("recall", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert!(
        again["objects"].as_array().is_some_and(Vec::is_empty),
        "still nothing, or the second read proves nothing: {again}"
    );
    assert!(
        again.get("teaching").is_none(),
        "the same session reading an empty answer again is not taught twice: {again}"
    );

    // ── a session whose first reads are not empty shipped-type reads ────────
    let t = story.session().await;
    t.add(
        "event:gorge-safety-inspection",
        "The Gorge Safety Inspection",
    )
    .await;
    t.add("place:springfield", "Springfield").await;
    t.add("place:daredevil-gorge", "The Daredevil Gorge").await;
    t.event_with(
        "event:gorge-safety-inspection",
        "four days up the gorge",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:daredevil-gorge",
            "leaves_on": "2026-09-03",
            "returns_on": "2026-09-07",
        }),
        &[],
    )
    .await;
    let held = t
        .call("recall", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert_eq!(
        held["objects"][0]["id"], "event:gorge-safety-inspection",
        "{held}"
    );
    assert!(
        held.get("teaching").is_none(),
        "an answer that holds something teaches nothing about an empty one: {held}"
    );

    t.call(
        "declare_type",
        json!({ "name": "loan", "fields": [{ "key": "due_back", "holds": "date" }] }),
    )
    .await;
    let declared = t
        .call("recall", json!({ "answers_type": "loan" }))
        .await
        .json();
    assert!(
        declared["objects"].as_array().is_some_and(Vec::is_empty),
        "{declared}"
    );
    assert!(
        declared.get("teaching").is_none(),
        "a type the caller declared is not one the software ships: {declared}"
    );

    // The two reads above spent nothing: this session's first EMPTY read of a
    // shipped type still carries the note.
    let empty = t
        .call("recall", json!({ "answers_type": "entitlement" }))
        .await
        .json();
    assert!(
        empty["objects"].as_array().is_some_and(Vec::is_empty),
        "{empty}"
    );
    assert!(
        empty["teaching"].as_array().is_some_and(|t| t.len() == 1),
        "a non-empty read and a declared type left the note unspent: {empty}"
    );
    story.finish().await;
}

#[tokio::test]
async fn an_empty_search_for_a_shipped_type_names_its_keys_and_shares_the_teaching_with_recall() {
    // **The same gap on the other front door.** A session that searched twice
    // for a trip and found nothing wrote its own start and end keys, because the
    // empty search named none of the shipped ones.
    let story = Story::begin("bot:gamma").await;

    // ── search first: the answer names the keys and carries the note ────────
    let a = story.session().await;
    let searched = a
        .call("search", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert_eq!(searched["count"], 0, "nothing is a trip yet: {searched}");
    assert_eq!(searched["type_keys"][0]["name"], "trip", "{searched}");
    assert_eq!(
        searched["teaching"].as_array().map(Vec::len),
        Some(1),
        "the first empty search of a shipped type teaches: {searched}"
    );
    // …and the session taught on search is not taught again on recall.
    let recalled = a
        .call("recall", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert_eq!(recalled["type_keys"][0]["name"], "trip", "{recalled}");
    assert!(
        recalled.get("teaching").is_none(),
        "one ledger row for both verbs: {recalled}"
    );

    // ── recall first, then search: the other direction ──────────────────────
    let b = story.session().await;
    let recalled = b
        .call("recall", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert_eq!(
        recalled["teaching"].as_array().map(Vec::len),
        Some(1),
        "{recalled}"
    );
    let searched = b
        .call("search", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert_eq!(
        searched["type_keys"][0]["name"], "trip",
        "the keys ride every empty answer, taught or not: {searched}"
    );
    assert!(
        searched.get("teaching").is_none(),
        "a session taught on recall is not taught again on search: {searched}"
    );

    // ── a search that holds something carries neither, and spends nothing ───
    let c = story.session().await;
    c.add("event:gorge-survey", "The Gorge Survey").await;
    c.add("place:springfield", "Springfield").await;
    c.add("place:north-gorge", "The North Gorge").await;
    c.event_with(
        "event:gorge-survey",
        "four days up the gorge",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:north-gorge",
            "leaves_on": "2026-09-03",
            "returns_on": "2026-09-07",
        }),
        &[],
    )
    .await;
    let held = c
        .call("search", json!({ "answers_type": "trip" }))
        .await
        .json();
    assert!(held["count"].as_u64().is_some_and(|n| n > 0), "{held}");
    assert!(held.get("type_keys").is_none(), "{held}");
    assert!(held.get("teaching").is_none(), "{held}");

    // A type the caller declared names its keys on an empty search and is not
    // one the software ships, so it is not taught.
    c.call(
        "declare_type",
        json!({ "name": "loan", "fields": [{ "key": "due_back", "holds": "date" }] }),
    )
    .await;
    let declared = c
        .call("search", json!({ "answers_type": "loan" }))
        .await
        .json();
    assert_eq!(declared["type_keys"][0]["name"], "loan", "{declared}");
    assert!(declared.get("teaching").is_none(), "{declared}");

    // The reads above spent nothing: this session's first empty search of a
    // shipped type still carries the note.
    let empty = c
        .call("search", json!({ "answers_type": "entitlement" }))
        .await
        .json();
    assert_eq!(
        empty["teaching"].as_array().map(Vec::len),
        Some(1),
        "{empty}"
    );
    story.finish().await;
}

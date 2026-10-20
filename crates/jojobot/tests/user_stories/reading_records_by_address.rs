//! "I have three addresses from three notes. What does each record say now?"
//!
//! **A bot handed several record addresses has one question: what do they say
//! today.** Before, the only way to read a record by its address was the whole
//! history of that one record, so three addresses cost three calls and every
//! past wording of each. The wording a reader wants is the newest one, and the
//! older ones are noise that has to be read past.
//!
//! ⭐ **One call, any subjects.** The records may sit on different things, and
//! each comes back as it stands now with its address, its subject, who backs it
//! and how many times it was written.
//!
//! ⚠️ **The negative is what gives it meaning.** A record rewritten twice
//! answers with its newest wording and none of the older two. An answer that
//! carried them would be the history door again, under another name.
//!
//! ⛔️ **An address that names nothing is named, and the rest are served.** One
//! stale address in a list must not cost the reader the others.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_reader_with_three_addresses_reads_what_each_says_now_in_one_call() {
    let story = Story::begin("bot:otto").await;

    // ── Spring · two things, and one claim written three times ────────────────
    let spring = story.session_on("2026-04-14", None).await;
    spring
        .add("org:springfield-town-council", "Springfield Town Council")
        .await;
    spring.add("thing:the-mailbox", "The Mailbox").await;
    let meets = spring
        .fact("org:springfield-town-council", "meets on Tuesdays")
        .await;
    let visits = spring
        .fact(
            "org:springfield-town-council",
            "holds its site visits at the gorge car park",
        )
        .await;
    let painted = spring.fact("thing:the-mailbox", "is painted green").await;
    for wording in ["meets on Wednesdays", "meets on the first Thursday"] {
        spring
            .call(
                "update_fact",
                json!({"address": &meets, "content": wording, "provenance": "testimony"}),
            )
            .await;
    }
    spring.wrap("wrote down the council and the mailbox").await;

    // ── December · the question ───────────────────────────────────────────────
    let winter = story.session_on("2026-12-20", Some("new")).await;
    let asked = winter
        .shape(
            "what do these say now?",
            json!({"records": [&meets, &visits, &painted]}),
        )
        .await;

    // ⭐ **One answer per address, in the order asked, each as it stands.**
    asked
        .says("meets on the first Thursday")
        .says("holds its site visits at the gorge car park")
        .says("is painted green")
        .number("/count", 3)
        .number("/records/0/revisions", 3)
        .number("/records/1/revisions", 1);
    assert_eq!(
        asked.json()["records"][2]["subject"],
        "thing:the-mailbox",
        "a record on another subject answers in the same call: {}",
        asked.raw()
    );

    // ⚠️ **And not a word of what it used to say.**
    for earlier in ["on Tuesdays", "on Wednesdays"] {
        assert!(
            !asked.raw().contains(earlier),
            "an earlier wording ({earlier}) came back with the current one: {}",
            asked.raw()
        );
    }

    // ⛔️ **A stale address in the list is named, and the others are served.**
    let stale = format!("{}9", meets.trim_end_matches(|c: char| c.is_ascii_digit()));
    let beside = winter
        .shape(
            "what do these say now, one of them stale?",
            json!({"records": [&meets, stale, &painted]}),
        )
        .await;
    beside
        .says("meets on the first Thursday")
        .says("is painted green");
    assert!(
        beside.json()["records"][1]["missing"].is_string()
            && beside.json()["records"][1].get("content").is_none(),
        "the stale address is not named missing: {}",
        beside.raw()
    );

    // **The history door is still where the earlier wordings are.**
    winter
        .shape(
            "and what did it say before?",
            json!({"history_record": &meets}),
        )
        .await
        .says("meets on Tuesdays")
        .says("meets on Wednesdays");
}

/// **A day, a window or a clock sent beside `records` is refused, and says which
/// argument it was.** Reading records by address is a read of what stands now.
/// `as_of`, `day`, `within_days` and `clock` are not arguments of `recall` at
/// all (`overdue` and `near` carry them), so the surface turns each back by name,
/// and `overdue` and `near` themselves are refused beside the list. The same list
/// sent alone is served, so none of the refusals is about the list.
#[tokio::test]
async fn a_day_a_window_or_a_clock_beside_a_list_of_records_is_refused_by_name() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:the-mailbox", "The Mailbox").await;
    let painted = s.fact("thing:the-mailbox", "is painted green").await;

    let alone = s.call("recall", json!({"records": [&painted]})).await;
    assert_eq!(
        alone.json()["records"][0]["content"],
        "is painted green",
        "{}",
        alone.raw()
    );

    for (name, extra) in [
        ("as_of", json!({"as_of": "2026-01-01"})),
        ("day", json!({"day": "2026-01-01"})),
        ("within_days", json!({"within_days": 3})),
        ("clock", json!({"clock": "taken_in"})),
        ("near", json!({"near": {"day": "2026-01-01"}})),
        ("overdue", json!({"overdue": {"as_of": "2026-01-01"}})),
    ] {
        let mut call = json!({"records": [&painted]});
        for (key, value) in extra.as_object().expect("an object") {
            call[key] = value.clone();
        }
        let refusal = s.refused("recall", call).await;
        assert_eq!(refusal.json()["wrote"], false, "{name}: {}", refusal.raw());
        refusal.says(name);
    }
    story.finish().await;
}

/// **An address on a thing that was merged away is named in its own place, and
/// the rest of the list is served.** The merge maps the claims the duplicate
/// held then, so those addresses still read. A number the duplicate never held
/// maps to nothing, and the reader is told the thing was merged and where its
/// survivor reads. The control is the survivor's own address in the same list,
/// served whole.
#[tokio::test]
async fn an_address_on_a_merged_away_thing_is_named_and_the_rest_are_served() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("person:bart", "Bart").await;
    let cornet = s.fact("person:milhouse", "plays the cornet").await;
    let skates = s.fact("person:bart", "skates to school").await;
    s.call(
        "merge_entities",
        json!({"duplicate": "person:bart", "survivor": "person:milhouse",
               "reason": "the same boy, filed twice"}),
    )
    .await;
    let never_held = format!("{}9", skates.trim_end_matches(|c: char| c.is_ascii_digit()));

    let asked = s
        .shape(
            "what do these say, one on a merged-away thing?",
            json!({"records": [&cornet, &never_held, &skates]}),
        )
        .await;

    asked
        .says("plays the cornet")
        .says("skates to school")
        .number("/count", 3);
    let missing = asked.json()["records"][1]["missing"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "the merged-away address is not named missing: {}",
                asked.raw()
            )
        })
        .to_string();
    assert!(
        missing.contains("person:milhouse"),
        "the reason names the survivor to read: {missing}"
    );
    assert_eq!(asked.json()["records"][1]["address"], never_held.as_str());
}

/// **An offset past the last record says how many there are.** The list sent is
/// the same one that reads whole at offset 0, so the empty part is the offset's
/// doing and the answer says so.
#[tokio::test]
async fn an_offset_past_the_last_record_says_how_many_records_there_are() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:the-mailbox", "The Mailbox").await;
    let painted = s.fact("thing:the-mailbox", "is painted green").await;
    let dented = s.fact("thing:the-mailbox", "has a dent in the flag").await;

    let whole = s
        .call("recall", json!({"records": [&painted, &dented]}))
        .await;
    whole.number("/count", 2);
    assert!(
        whole.json().get("past_the_end").is_none(),
        "{}",
        whole.raw()
    );

    let past = s
        .call(
            "recall",
            json!({"records": [&painted, &dented], "offset": 2}),
        )
        .await;
    past.number("/count", 0);
    let said = past.json()["past_the_end"]
        .as_str()
        .unwrap_or_else(|| panic!("an offset past the end says so: {}", past.raw()))
        .to_string();
    assert!(
        said.contains('2'),
        "it says how many records there are: {said}"
    );
}

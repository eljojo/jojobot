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

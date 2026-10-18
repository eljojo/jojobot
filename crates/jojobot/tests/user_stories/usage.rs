//! "How many times did each bot report a compaction this week and last, and how
//! much was each served?"
//!
//! **The answer is one ordinary read.** A `usage` record is one bot's one ISO
//! week, computed from the history of the bot's runs when it is read and held in
//! no store, so nobody can write one and none can go stale. A bot reads its own
//! weeks and those of every bot below it on the chart; the head of the chart
//! reads all; a bot with no place above another reads nothing of it, and is told
//! how many weeks it was not shown.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

fn reporting_to(subject: &str, manager: &str) -> Value {
    json!({
        "subject": subject, "content": format!("{subject} reports to {manager}"),
        "provenance": "testimony", "sets": {"reports_to": manager},
    })
}

/// The run writes once, which gives it a record to be served on, and then reads:
/// every answer it is handed counts towards its total.
async fn is_served(who: &Session, reads: usize) {
    who.journal("working").await;
    for _ in 0..reads {
        who.call("recall", json!({"subject": "person:lisa", "facts": true}))
            .await;
    }
}

/// The run tells jojobot its context was compacted.
async fn compacts(who: &Session) {
    who.call("start_here", json!({"compacted": true, "brief": true}))
        .await;
}

/// Each total a compaction set back, with the day it was recorded, as the run
/// reads them from its own history.
async fn resets_of(who: &Session) -> Vec<(u64, String)> {
    let history = who
        .call(
            "recall",
            json!({"subject": format!("session:{}", who.run()), "history": "served_chars"}),
        )
        .await
        .json();
    history["objects"][0]["history"]["writes"]
        .as_array()
        .unwrap_or_else(|| panic!("the run's served history: {history}"))
        .iter()
        .filter(|write| write["change"] == "served_reset")
        .map(|write| {
            (
                write["value"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .unwrap(),
                write["written_at"].as_str().unwrap()[..10].to_string(),
            )
        })
        .collect()
}

/// What the run has been served since its last compaction, as it lists its own
/// runs. **Asked after the read it is compared with, and with nothing else
/// asked of the run in between**: a run is charged for the answers it is handed,
/// the one that lists its runs included, so the figure is the one the read saw.
async fn held_by(who: &Session) -> u64 {
    let listed = who.call("list_runs", json!({})).await.json();
    listed["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|run| run["run"] == who.run()))
        .and_then(|run| run["served_since_compaction"]["characters"].as_u64())
        .unwrap_or_else(|| panic!("the run lists what it was served: {listed}"))
}

/// The records of a read, by handle.
fn by_handle(read: &Value) -> std::collections::BTreeMap<String, Value> {
    read["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a read of usage: {read}"))
        .iter()
        .map(|object| (object["id"].as_str().unwrap().to_string(), object.clone()))
        .collect()
}

fn field(record: &Value, key: &str) -> String {
    record["fields"][key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} on {record}"))
        .to_string()
}

#[tokio::test]
async fn the_assistant_reads_each_bots_weeks_and_a_bot_reads_only_the_weeks_it_may() {
    // ── the first week: Monday 2026-10-05 to Sunday 2026-10-11 ──────────────
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-07").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    s.add("bot:milhouse", "Milhouse").await;
    // Bart reports to Marge; Milhouse is on nobody's chart.
    let marge = story.as_bot("bot:marge").await;
    marge
        .call("capture", reporting_to("bot:bart", "bot:marge"))
        .await;

    let bart_first = story.as_bot("bot:bart").await;
    is_served(&bart_first, 3).await;
    compacts(&bart_first).await;
    is_served(&bart_first, 2).await;
    is_served(&marge, 2).await;
    let milhouse = story.as_bot("bot:milhouse").await;
    is_served(&milhouse, 2).await;
    compacts(&milhouse).await;
    let bart_first_resets = resets_of(&bart_first).await;
    let marge_resets = resets_of(&marge).await;
    let milhouse_resets = resets_of(&milhouse).await;
    assert_eq!(bart_first_resets.len(), 1, "{bart_first_resets:?}");
    assert_eq!(bart_first_resets[0].1, "2026-10-07");
    assert!(marge_resets.is_empty(), "{marge_resets:?}");

    // ── the second week: Monday 2026-10-12 to Sunday 2026-10-18 ─────────────
    let later = story.restarted_pretending_it_is("2026-10-14").await;
    let bart_second = later.as_bot("bot:bart").await;
    is_served(&bart_second, 2).await;
    compacts(&bart_second).await;
    is_served(&bart_second, 1).await;
    compacts(&bart_second).await;
    is_served(&bart_second, 1).await;
    let bart_second_resets = resets_of(&bart_second).await;
    assert_eq!(bart_second_resets.len(), 2, "{bart_second_resets:?}");
    assert!(
        bart_second_resets
            .iter()
            .all(|(_, day)| day == "2026-10-14")
    );

    // Marge's run from the first week goes on into this one: it began last week
    // and is served again this week, so no run of Marge's began in this week.
    let marge = later.session_carried_from(&marge).await;
    is_served(&marge, 1).await;

    // ── the head of the chart reads every bot's weeks, newest first ─────────
    let assistant = later.as_bot("bot:assistant").await;
    // A run's chronology is its bot's alone: the head of the chart is shown none
    // of Bart's runs. The weeks below are the first thing it can read of them.
    let runs = assistant
        .call("recall", json!({"kind": "session", "parent": "bot:bart"}))
        .await
        .json();
    assert!(runs["objects"].as_array().unwrap().is_empty(), "{runs}");
    assert!(runs["withheld"].as_u64().unwrap() >= 2, "{runs}");

    let all = assistant
        .call("recall", json!({"view": "usage"}))
        .await
        .json();
    let weeks = by_handle(&all);
    // ── the same read after the store changed underneath is the same ────────
    // Every record but the reader's own: reading is served to the reader, so its
    // own weeks grow with each answer it is handed.
    later.the_store_changed_underneath().await;
    let again = assistant
        .call("recall", json!({"view": "usage"}))
        .await
        .json();
    let others = |read: &std::collections::BTreeMap<String, Value>| {
        read.iter()
            .filter(|(handle, _)| !handle.starts_with("usage:assistant-"))
            .map(|(handle, record)| (handle.clone(), record.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    assert_eq!(others(&by_handle(&again)), others(&weeks));
    // What each run holds now is what the read saw: nothing has been asked of
    // the runs since.
    let (bart_first_held, bart_second_held) =
        (held_by(&bart_first).await, held_by(&bart_second).await);
    let (marge_held, milhouse_held) = (held_by(&marge).await, held_by(&milhouse).await);
    let from_days: Vec<String> = all["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|object| field(object, "from"))
        .collect();
    assert!(
        from_days.windows(2).all(|pair| pair[0] >= pair[1]),
        "newest week first: {from_days:?}"
    );

    // Bart's second week: one run begun, two compactions, and the run's whole
    // life served, matching what the run itself says it was handed.
    let bart_w42 = &weeks["usage:bart-2026-w42"];
    assert_eq!(field(bart_w42, "week"), "2026-W42");
    assert_eq!(field(bart_w42, "from"), "2026-10-12");
    assert_eq!(field(bart_w42, "to"), "2026-10-18");
    assert_eq!(field(bart_w42, "runs"), "1");
    assert_eq!(field(bart_w42, "compactions"), "2");
    let set_back: u64 = bart_second_resets.iter().map(|(total, _)| total).sum();
    assert_eq!(
        field(bart_w42, "served_at_compaction"),
        set_back.to_string()
    );
    assert_eq!(
        field(bart_w42, "served_by_run_start"),
        (bart_second_held + set_back).to_string()
    );
    assert_eq!(field(bart_w42, "served_basis"), "day");
    // The week is exact by day: its total is the sum of the days it names, and a
    // day that was not served has no key.
    let days: Vec<(&String, u64)> = bart_w42["fields"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| key.starts_with("served_2026-"))
        .map(|(key, value)| (key, value.as_str().unwrap().parse().unwrap()))
        .collect();
    assert_eq!(
        days.len(),
        1,
        "the one day the run was served on: {bart_w42}"
    );
    let served_in_the_week = days.iter().map(|(_, chars)| chars).sum::<u64>();
    assert!(served_in_the_week > 0, "{bart_w42}");
    assert_eq!(
        field(bart_w42, "served_chars"),
        served_in_the_week.to_string(),
        "the week's total is the sum of its days: {bart_w42}"
    );
    // What the runs that began in the week were served over their lives is its own
    // field. The one run began in the week and was served only in it, so the whole
    // of its life is the week's days, and it is also what the run holds now plus
    // what its two compactions set back.
    assert_eq!(
        field(bart_w42, "served_by_run_start"),
        served_in_the_week.to_string(),
        "{bart_w42}"
    );
    assert_eq!(field(bart_w42, "partial"), "true", "the open week");
    assert_eq!(field(bart_w42, "counted_from"), "2026-10-07");

    // Bart's first week: the compaction of the run that began in it.
    let bart_w41 = &weeks["usage:bart-2026-w41"];
    assert_eq!(field(bart_w41, "runs"), "1");
    assert_eq!(field(bart_w41, "compactions"), "1");
    assert_eq!(
        field(bart_w41, "served_at_compaction"),
        bart_first_resets[0].0.to_string()
    );
    assert_eq!(
        field(bart_w41, "served_by_run_start"),
        (bart_first_held + bart_first_resets[0].0).to_string()
    );
    assert_eq!(field(bart_w41, "partial"), "false", "a week that is over");

    // A bot that never compacted has weeks with none, and Milhouse's one is its own.
    let marge_w41 = &weeks["usage:marge-2026-w41"];
    assert_eq!(field(marge_w41, "compactions"), "0");
    assert_eq!(
        field(marge_w41, "served_by_run_start"),
        marge_held.to_string()
    );
    let milhouse_w41 = &weeks["usage:milhouse-2026-w41"];
    assert_eq!(field(milhouse_w41, "compactions"), "1");
    assert_eq!(
        field(milhouse_w41, "served_by_run_start"),
        (milhouse_held + milhouse_resets[0].0).to_string()
    );

    // One bot's weeks, asked by the view's input: Bart's two, newest first.
    let only_bart = assistant
        .call(
            "recall",
            json!({"view": "usage", "inputs": {"bot": "bot:bart"}}),
        )
        .await
        .json();
    let ids: Vec<&str> = only_bart["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|object| object["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["usage:bart-2026-w42", "usage:bart-2026-w41"]);

    // This week and last are a question of the week's own dates, asked on the
    // reader's day: the declared dates license the ordering. Asked of both weeks,
    // and of who was served rather than who began a run, so a run carried from
    // last week into this one is among them. The reader's own weeks grow with
    // each read it makes, so they are left out of the comparison.
    let others_only = |read: &Value| -> Vec<String> {
        by_handle(read)
            .into_keys()
            .filter(|handle| !handle.starts_with("usage:assistant-"))
            .collect()
    };
    let served_in_both = assistant
        .call(
            "recall",
            json!({"kind": "usage", "where": "from >= today-13d && served_chars > 0"}),
        )
        .await
        .json();
    assert_eq!(
        others_only(&served_in_both),
        [
            "usage:bart-2026-w41",
            "usage:bart-2026-w42",
            "usage:marge-2026-w41",
            "usage:marge-2026-w42",
            "usage:milhouse-2026-w41",
            "usage:otto-2026-w41"
        ],
        "the weeks of the last fortnight in which a bot was served: {served_in_both}"
    );
    // Marge's week this week is there although no run of hers began in it…
    let carried = &by_handle(&served_in_both)["usage:marge-2026-w42"];
    assert_eq!(field(carried, "runs"), "0", "{carried}");
    assert!(
        field(carried, "served_chars").parse::<u64>().unwrap() > 0,
        "{carried}"
    );
    // …and the question that asks which bots began a run drops it: the positive
    // beside it is the bot whose run did begin this week.
    let began_a_run = assistant
        .call(
            "recall",
            json!({"kind": "usage", "where": "from >= today-6d && runs > 0"}),
        )
        .await
        .json();
    assert_eq!(
        others_only(&began_a_run),
        ["usage:bart-2026-w42"],
        "weeks that began on or after the 8th, with a run begun: {began_a_run}"
    );

    // ── a bot reads its own weeks and those of the bots below it ────────────
    let managed = marge.call("recall", json!({"view": "usage"})).await.json();
    let seen: Vec<String> = by_handle(&managed).into_keys().collect();
    assert_eq!(
        seen,
        [
            "usage:bart-2026-w41",
            "usage:bart-2026-w42",
            "usage:marge-2026-w41",
            "usage:marge-2026-w42"
        ],
        "Marge's own weeks and Bart's, who reports to it: {managed}"
    );
    assert!(managed["withheld"].as_u64().unwrap() > 0, "{managed}");

    // ── a bot on nobody's chart reads only its own, and is told of the rest ─
    let milhouse_later = later.as_bot("bot:milhouse").await;
    let alone = milhouse_later
        .call("recall", json!({"view": "usage"}))
        .await
        .json();
    let seen: Vec<String> = by_handle(&alone).into_keys().collect();
    assert_eq!(seen, ["usage:milhouse-2026-w41", "usage:milhouse-2026-w42"]);
    assert!(
        alone["withheld"].as_u64().unwrap() >= 4,
        "the weeks of Bart, Marge and the assistant are counted, not shown: {alone}"
    );
    // Naming a week it may not read gets none of its numbers and none of its
    // neighbours' handles. The answer says the week is there and is not the
    // asker's, and who may read it — the sentence for a week, not the one for a
    // run's chronology beside it.
    let named = milhouse_later
        .refused("recall", json!({"subject": "usage:bart-2026-w41"}))
        .await;
    named
        .says("\"fix_by\":\"change\"")
        .says("assistant")
        .never_says("compactions");
    named.never_says("chronology");
    // A near miss on a week it may not read offers none of that bot's handles,
    // and the same miss from the head of the chart offers them: the positive the
    // negative depends on.
    let typo = json!({"subject": "usage:bart-2026-w401"});
    let hidden = milhouse_later.refused("recall", typo.clone()).await;
    let offered = assistant.refused("recall", typo).await;
    offered.says("\"handle\":\"usage:bart-2026-w41\"");
    hidden.never_says("\"handle\":\"usage:bart-");
    // …and Bart, the bot below, reads nobody's but its own.
    let bart_later = later.as_bot("bot:bart").await;
    let below = bart_later
        .call("recall", json!({"view": "usage"}))
        .await
        .json();
    let seen: Vec<String> = by_handle(&below).into_keys().collect();
    assert_eq!(seen, ["usage:bart-2026-w41", "usage:bart-2026-w42"]);

    // ── nothing writes a usage record, and the refusal says where to read ───
    let refused = assistant
        .refused(
            "capture",
            json!({"subject": "usage:bart-2026-w41", "content": "fewer compactions"}),
        )
        .await;
    refused.says("\"fix_by\":\"change\"").says("recall");
    refused.never_says("add_entity");
    let created = assistant
        .refused(
            "add_entity",
            json!({
                "kind": "usage", "handle": "usage:bart-2026-w43",
                "name": "A week to come", "source": "the story",
            }),
        )
        .await;
    created.says("\"fix_by\":\"change\"").says("recall");
    // The read the refusals name is open to the same asker.
    let named = assistant
        .call("recall", json!({"subject": "usage:bart-2026-w41"}))
        .await;
    named.says("\"compactions\":\"1\"");

    later.finish().await;
}

/// **A usage record has no write history, and says so.** It is computed when it
/// is read and nobody writes it, so asking for the history of one of its keys
/// is not asking about an entity jojobot does not know. The answer is the empty
/// history, with the reason.
#[tokio::test]
async fn the_history_of_a_key_on_a_usage_record_says_it_has_none() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-07").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:bart", "Bart").await;
    let bart = story.as_bot("bot:bart").await;
    is_served(&bart, 1).await;

    let read = bart
        .call(
            "recall",
            json!({"subject": "usage:bart-2026-w41", "history": "calls"}),
        )
        .await
        .json();
    let history = &read["objects"][0]["history"];
    assert_eq!(history["key"], "calls", "{read}");
    assert_eq!(history["count"], 0, "{read}");
    assert_eq!(history["writes"], json!([]), "{read}");
    assert!(
        history["note"]
            .as_str()
            .is_some_and(|note| !note.is_empty()),
        "the answer says why there is none: {read}"
    );
    // The record itself is still there to read, with the key it was asked about.
    assert!(read["objects"][0]["fields"]["calls"].is_string(), "{read}");
    story.finish().await;
}

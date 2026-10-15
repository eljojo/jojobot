//! "It stopped being owed. Keep the date."
//!
//! **A window that has closed leaves its date behind.** The operator keeps the
//! desk, so the return window no longer owes anything, and the day it ran out
//! is still true of the desk. The way to say that is the word `ended`: a thing
//! that carries it stops being owed and keeps `runs_out` or `decide_by` where
//! they were. Clearing the source of the due day also stops the owing, and
//! erases the date with it.
//!
//! **Each case is paired with what it must not do**: the same thing without
//! `ended` is still listed, and the refusal that names the word still lets the
//! clear route stand.

use serde_json::json;

use super::dsl::{Answer, Story};

const DESK: &str = "thing:standing-desk";
const BOOTS: &str = "thing:boots";
const ASKED: &str = "2026-12-06";

/// The handles the overdue read lists.
async fn overdue(s: &super::dsl::Session) -> Vec<String> {
    let listed = s
        .call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": ASKED}}),
        )
        .await;
    ids(&listed)
}

fn ids(answer: &Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn a_runs_out_thing_that_ended_is_not_owed_and_keeps_its_date() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(DESK, "Standing desk").await;
    s.call(
        "capture",
        json!({"subject": DESK, "content": "Return window runs out 8 Mar 2026.",
               "provenance": "inference", "recorded_at": "2026-02-08",
               "sets": {"runs_out": "2026-03-08"}}),
    )
    .await;
    assert_eq!(overdue(&s).await, [DESK], "owed before it ended");

    // ── the stored due moment cannot be cleared, and the refusal names the
    // way out that keeps the date before the one that erases it ──────────────
    let address = format!("{DESK}#f1");
    let refusal = s
        .refused(
            "update_fact",
            json!({"address": address, "clear_fields": ["due_on"]}),
        )
        .await;
    let words = refusal.raw().to_string();
    let ended_at = words.find("ended").expect("the refusal names the word");
    let clear_at = words
        .find("clear the one it came from")
        .expect("the refusal still names the other route");
    assert!(ended_at < clear_at, "the word comes first: {words}");

    // ── following it ────────────────────────────────────────────────────────
    s.call(
        "capture",
        json!({"subject": DESK, "content": "The operator keeps the desk.",
               "provenance": "testimony", "recorded_at": "2026-03-15",
               "sets": {"ended": "kept"}}),
    )
    .await;
    assert!(
        overdue(&s).await.is_empty(),
        "an ended thing is no longer owed"
    );
    let held = s
        .call(
            "recall",
            json!({"subject": DESK, "keys": ["runs_out", "ended"]}),
        )
        .await;
    let fields = held.json()["objects"][0]["fields"].clone();
    assert_eq!(
        fields["runs_out"],
        "2026-03-08",
        "the date stays: {}",
        held.raw()
    );
    assert_eq!(fields["ended"], "kept", "{}", held.raw());

    // The pair: a thing that has not ended is still owed beside it.
    s.add(BOOTS, "The boots").await;
    s.call(
        "capture",
        json!({"subject": BOOTS, "content": "Sole guarantee runs out 8 Mar 2026.",
               "provenance": "inference", "recorded_at": "2026-03-15",
               "sets": {"runs_out": "2026-03-08"}}),
    )
    .await;
    assert_eq!(overdue(&s).await, [BOOTS], "the unended thing is owed");
    story.finish().await;
}

#[tokio::test]
async fn a_decide_by_thing_that_ended_is_not_owed_and_keeps_its_date() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;
    s.call(
        "capture",
        json!({"subject": "project:atlas", "content": "Decide by 1 Mar 2026.",
               "provenance": "inference", "recorded_at": "2026-02-08",
               "sets": {"decide_by": "2026-03-01"}}),
    )
    .await;
    assert_eq!(overdue(&s).await, ["project:atlas"], "owed before it ended");
    s.call(
        "capture",
        json!({"subject": "project:atlas", "content": "Decided.",
               "provenance": "testimony", "recorded_at": "2026-03-02",
               "sets": {"ended": "decided"}}),
    )
    .await;
    assert!(
        overdue(&s).await.is_empty(),
        "an ended decision is not owed"
    );
    let held = s
        .call(
            "recall",
            json!({"subject": "project:atlas", "keys": ["decide_by"]}),
        )
        .await;
    assert_eq!(
        held.json()["objects"][0]["fields"]["decide_by"],
        "2026-03-01",
        "{}",
        held.raw()
    );
    story.finish().await;
}

/// **A store the previous build filled** can already hold a thing with both
/// `runs_out` and `ended`, and the stored due day the old build left on it. The
/// word did nothing there; it does now, so the thing stops being owed without
/// anybody writing to it again.
#[tokio::test]
async fn a_thing_the_old_build_left_holding_both_runs_out_and_ended_is_not_owed() {
    use jojobot_domain::memory::{EntityId, NewEntity, NewFact};

    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let store = story.memory_store();
    let desk = EntityId(DESK.into());
    store
        .add_entity(NewEntity::new(desk.clone(), "Standing desk", "user-named"))
        .await
        .expect("the desk is made")
        .written()
        .expect("nothing blocks it");
    // What the old build wrote: the date, the due moment it computed from it,
    // and a word that read as nothing.
    store
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [
                ("runs_out", "2026-03-08"),
                ("due_on", "2026-03-08"),
                ("ended", "kept"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            ..NewFact::about(
                desk,
                "Return window runs out 8 Mar 2026.",
                "2026-02-08".parse().expect("a day"),
            )
        }))
        .await
        .expect("the old build's record lands")
        .written()
        .expect("nothing blocks it");
    story.the_store_changed_underneath().await;

    assert!(
        overdue(&s).await.is_empty(),
        "the thing holds `ended`, so it is not owed"
    );
    // The same answer from the shipped view that lists what falls due by a day,
    // which a caller reaches without writing a `fields` filter.
    let due = s
        .call(
            "recall",
            json!({"view": "due", "inputs": {"by": "2027-01-01"}}),
        )
        .await;
    assert!(
        ids(&due).is_empty(),
        "the due view does not list it either: {}",
        due.raw()
    );
    story.finish().await;
}

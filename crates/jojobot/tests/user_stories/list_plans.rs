//! "Marge's plan for the week says what the week holds, and I did not write that
//! part."
//!
//! **A plan is a list that says which period it covers**: one calendar day, or
//! one ISO week. Reading the plan adds what is due in the period, what is
//! overdue before it, and which of that work can be started, worked out when the
//! plan is read and stored nowhere. Carrying an unfinished entry into the next
//! plan is an explicit act that records where it came from; nothing rolls over
//! on its own.
//!
//! The roster names are the cast: a camping trip, errand lists, pieces of work.

use serde_json::{Value, json};

use super::dsl::{Answer, Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};

/// The day the stories are read on: a Wednesday, in ISO week 2026-W43, which
/// runs from Monday 2026-10-19 to Sunday 2026-10-25.
const TODAY: &str = "2026-10-21";

/// A piece of work with a status and a day it must be decided by.
async fn work_due(s: &Session, handle: &str, status: &str, due: &str, depends_on: Option<&str>) {
    let (_, slug) = handle.split_once(':').expect("a handle is kind:slug");
    let mut sets = json!({"status": status, "decide_by": due});
    if let Some(depends_on) = depends_on {
        sets["depends_on"] = json!(depends_on);
    }
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": sets}),
    )
    .await;
}

/// The handles of one section of a plan, in the order it serves them.
fn handles(plan: &Value, section: &str) -> Vec<String> {
    plan[section]
        .as_array()
        .unwrap_or_else(|| panic!("the plan answers with {section}: {plan}"))
        .iter()
        .map(|item| {
            item.as_str()
                .or_else(|| item["handle"].as_str())
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// A read of a list.
async fn read(s: &Session, list: &str) -> Value {
    let answer: Answer = s.call("recall", json!({"subject": list})).await;
    answer.json()["objects"][0].clone()
}

/// This week's work: two things fall due in the week (one blocked), one fell due
/// last week, one falls due next week, one is finished.
async fn the_week_of_work(s: &Session) {
    work_due(s, "work:order-the-lumber", "next", "2026-12-01", None).await;
    work_due(s, "work:pour-the-slab", "next", "2026-10-14", None).await;
    work_due(s, "work:frame-the-walls", "now", "2026-10-22", None).await;
    work_due(
        s,
        "work:paint-the-door",
        "next",
        "2026-10-23",
        Some("work:order-the-lumber"),
    )
    .await;
    work_due(s, "work:lay-the-tiles", "next", "2026-10-28", None).await;
    work_due(s, "work:build-the-roof", "done", "2026-10-21", None).await;
    // The edges of the week: its Monday and its Sunday.
    work_due(s, "work:hang-the-doors", "next", "2026-10-19", None).await;
    work_due(s, "work:wire-the-outlets", "next", "2026-10-25", None).await;
}

/// **A week plan reads its entries and what is due, overdue and ready that
/// week.** Finished work and work due next week are not in it, and a list that
/// covers nothing carries no plan.
#[tokio::test]
async fn a_week_plan_reads_what_is_due_overdue_and_ready_that_week() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "errands", json!({"tick": "sink", "covers": "2026-W43"})).await;
    the_week_of_work(&s).await;
    add_entry(&s, "list:errands", "buy flour", None).await;
    add_entry(&s, "list:errands", "bake", None).await;

    let plan = read(&s, "list:errands").await;
    assert_eq!(plan["entries"][0]["text"], "buy flour", "{plan}");
    assert_eq!(plan["plan"]["covers"], "2026-W43", "{plan}");
    assert_eq!(plan["plan"]["from"], "2026-10-19", "{plan}");
    assert_eq!(plan["plan"]["to"], "2026-10-25", "{plan}");
    assert_eq!(
        handles(&plan["plan"], "due"),
        [
            "work:hang-the-doors",
            "work:frame-the-walls",
            "work:paint-the-door",
            "work:wire-the-outlets"
        ],
        "the week's Monday and Sunday are in it, and the next week's work is not: {plan}"
    );
    assert_eq!(
        handles(&plan["plan"], "overdue"),
        ["work:pour-the-slab"],
        "{plan}"
    );
    // Late as the week began, on Monday the 19th: work due on the 14th.
    assert_eq!(plan["plan"]["overdue"][0]["overdue_by_days"], 5, "{plan}");
    assert_eq!(
        handles(&plan["plan"], "ready"),
        [
            "work:pour-the-slab",
            "work:hang-the-doors",
            "work:frame-the-walls",
            "work:wire-the-outlets"
        ],
        "work:paint-the-door waits on work:order-the-lumber, so it is not ready: {plan}"
    );

    // Read again after the facts moved: nothing was stored on the plan.
    s.call(
        "update_entity",
        json!({"handle": "work:pour-the-slab", "sets": {"status": "done"}}),
    )
    .await;
    let later = read(&s, "list:errands").await;
    assert!(
        handles(&later["plan"], "overdue").is_empty(),
        "finished work is no longer overdue: {later}"
    );
    assert_eq!(
        handles(&later["plan"], "ready"),
        [
            "work:hang-the-doors",
            "work:frame-the-walls",
            "work:wire-the-outlets"
        ],
        "{later}"
    );

    // A list that covers nothing is not a plan.
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "rotation", "name": "Rotation",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "back"}}),
    )
    .await;
    let none = read(&s, "list:rotation").await;
    assert!(none.get("plan").is_none(), "{none}");
    story.finish().await;
}

/// **A plan for a day reads that day only**: what falls due on it, and what was
/// overdue before it. Work due on another day of the same week is not in it.
#[tokio::test]
async fn a_day_plan_reads_that_day_only() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    a_list_on_the_trip(
        &s,
        "errands",
        json!({"tick": "sink", "covers": "2026-10-22"}),
    )
    .await;
    the_week_of_work(&s).await;

    let plan = read(&s, "list:errands").await;
    assert_eq!(plan["plan"]["covers"], "2026-10-22", "{plan}");
    assert_eq!(plan["plan"]["from"], "2026-10-22", "{plan}");
    assert_eq!(plan["plan"]["to"], "2026-10-22", "{plan}");
    assert_eq!(
        handles(&plan["plan"], "due"),
        ["work:frame-the-walls"],
        "{plan}"
    );
    assert_eq!(
        handles(&plan["plan"], "overdue"),
        ["work:pour-the-slab", "work:hang-the-doors"],
        "{plan}"
    );
    story.finish().await;
}

/// The two plans: this week's with three entries, the first two done, and next
/// week's, empty. The addresses of this week's entries.
async fn two_weeks_of_plans(s: &Session) -> Vec<String> {
    a_list_on_the_trip(s, "errands", json!({"tick": "sink", "covers": "2026-W43"})).await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "checklist", "name": "Next week",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "sink", "covers": "2026-W44"}}),
    )
    .await;
    let mut at = Vec::new();
    for text in ["buy flour", "bake", "clean the oven"] {
        at.push(add_entry(s, "list:errands", text, None).await);
    }
    for done in &at[..2] {
        s.call(
            "capture",
            json!({"subject": "list:errands", "content": "done", "provenance": "testimony",
                   "tick": {"entry": done, "act": "done"}}),
        )
        .await;
    }
    at
}

/// **Carrying an open entry into next week's plan makes a new entry there that
/// names the old one, and last week's plan still reads the old entry as open and
/// carried, with its done entries done.** Before the carry, nothing has moved.
#[tokio::test]
async fn carrying_an_open_entry_forward_names_it_and_the_old_plan_still_reads_it_open() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    let at = two_weeks_of_plans(&s).await;

    // Nothing moves between plans without the act.
    let before_next = read(&s, "list:checklist").await;
    assert_eq!(before_next["entries"], json!([]), "{before_next}");
    let before_old = read(&s, "list:errands").await;
    assert!(
        before_old["entries"][0].get("carried_to").is_none(),
        "nothing was carried: {before_old}"
    );

    // The act: capture the entry on the next plan, naming the one it comes from.
    let carried = s
        .call(
            "capture",
            json!({"subject": "list:checklist", "content": "clean the oven",
                   "provenance": "inference", "derived_from": at[2]}),
        )
        .await;
    let new_address = carried.json()["address"]
        .as_str()
        .unwrap_or_else(|| panic!("a capture answers with the address: {}", carried.raw()))
        .to_string();

    let next = read(&s, "list:checklist").await;
    assert_eq!(next["entries"][0]["text"], "clean the oven", "{next}");
    assert_eq!(next["entries"][0]["state"], "open", "{next}");
    assert_eq!(next["entries"][0]["carried_from"], at[2].as_str(), "{next}");

    let old = read(&s, "list:errands").await;
    let states: Vec<String> = old["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| {
            format!(
                "{}:{}",
                entry["text"].as_str().unwrap_or_default(),
                entry["state"].as_str().unwrap_or_default()
            )
        })
        .collect();
    assert_eq!(
        states,
        ["clean the oven:open", "buy flour:done", "bake:done"],
        "{old}"
    );
    assert_eq!(
        old["entries"][0]["carried_to"],
        json!([new_address]),
        "{old}"
    );
    assert!(old["entries"][1].get("carried_to").is_none(), "{old}");
    story.finish().await;
}

/// The hint a tick receipt carries, whole; empty when the receipt has none,
/// which is how a receipt with nothing to teach reads.
fn the_line(receipt: &Answer) -> String {
    receipt.json()["hint"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// A plan or a checklist with three entries, as their addresses.
async fn three_entries(s: &Session, covers: Option<&str>) -> Vec<String> {
    let mut sets = json!({"tick": "sink"});
    if let Some(covers) = covers {
        sets["covers"] = json!(covers);
    }
    a_list_on_the_trip(s, "errands", sets).await;
    let mut at = Vec::new();
    for text in ["buy flour", "bake", "clean the oven"] {
        at.push(add_entry(s, "list:errands", text, None).await);
    }
    at
}

/// Ticks `done` on the first `n` entries in one call.
async fn tick_many(s: &Session, at: &[String], n: usize) -> Answer {
    let ticks: Vec<Value> = at[..n]
        .iter()
        .map(|entry| json!({"entry": entry, "act": "done"}))
        .collect();
    s.call(
        "capture",
        json!({"subject": "list:errands", "provenance": "testimony", "ticks": ticks}),
    )
    .await
}

/// **Ticks that leave an entry open on a plan teach the carry in the receipt**:
/// the next plan takes the entry with `derived_from` set to the old address. The
/// same ticks on a list that covers no period say nothing of it, and a plan with
/// nothing left open says nothing either.
#[tokio::test]
async fn ticks_leaving_an_entry_open_on_a_plan_teach_the_carry() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    let at = three_entries(&s, Some("2026-W43")).await;

    let two = tick_many(&s, &at, 2).await;
    assert!(the_line(&two).contains("derived_from"), "{}", two.raw());

    let last = s
        .call(
            "capture",
            json!({"subject": "list:errands", "content": "done", "provenance": "testimony",
                   "tick": {"entry": at[2], "act": "done"}}),
        )
        .await;
    assert!(
        !the_line(&last).contains("derived_from"),
        "nothing is open after it: {}",
        last.raw()
    );
    story.finish().await;
}

/// **A single tick on a plan carries the same line**, and a list with no period
/// does not, for one tick or for several.
#[tokio::test]
async fn one_tick_on_a_plan_teaches_the_carry_and_a_checklist_does_not() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    let at = three_entries(&s, Some("2026-10-22")).await;
    let one = s
        .call(
            "capture",
            json!({"subject": "list:errands", "content": "done", "provenance": "testimony",
                   "tick": {"entry": at[0], "act": "done"}}),
        )
        .await;
    assert!(the_line(&one).contains("derived_from"), "{}", one.raw());
    story.finish().await;

    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    let at = three_entries(&s, None).await;
    let one = s
        .call(
            "capture",
            json!({"subject": "list:errands", "content": "done", "provenance": "testimony",
                   "tick": {"entry": at[0], "act": "done"}}),
        )
        .await;
    assert!(!the_line(&one).contains("derived_from"), "{}", one.raw());
    let two = tick_many(&s, &at[1..], 2).await;
    assert!(!the_line(&two).contains("derived_from"), "{}", two.raw());
    story.finish().await;
}

/// **A capture that ticks nothing teaches nothing of the carry**, even on a plan
/// with entries open.
#[tokio::test]
async fn a_capture_that_ticks_nothing_on_a_plan_does_not_teach_the_carry() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    three_entries(&s, Some("2026-W43")).await;
    let added = s
        .call(
            "capture",
            json!({"subject": "list:errands", "content": "paint", "provenance": "testimony"}),
        )
        .await;
    assert!(
        !the_line(&added).contains("derived_from"),
        "{}",
        added.raw()
    );
    story.finish().await;
}

/// **A batch of new entries on a plan ticks nothing, so it teaches nothing of the
/// carry** while an entry is open.
#[tokio::test]
async fn a_batch_of_entries_on_a_plan_does_not_teach_the_carry() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    three_entries(&s, Some("2026-W43")).await;
    let added = s
        .call(
            "capture",
            json!({"subject": "list:errands", "provenance": "testimony",
                   "entries": ["paint", "sweep"]}),
        )
        .await;
    assert!(
        !the_line(&added).contains("derived_from"),
        "{}",
        added.raw()
    );
    story.finish().await;
}

/// One tick of `done` on an entry, as its own call.
async fn tick_one(s: &Session, entry: &str) -> Answer {
    s.call(
        "capture",
        json!({"subject": "list:errands", "content": "done", "provenance": "testimony",
               "tick": {"entry": entry, "act": "done"}}),
    )
    .await
}

/// **A batch that ticks every open entry of a plan teaches nothing of the carry**:
/// the receipt is judged once, after the last tick, and no item of the batch
/// leaves a sentence behind. Paired with the partial batch above, which teaches.
#[tokio::test]
async fn a_batch_that_ticks_every_open_entry_of_a_plan_does_not_teach_the_carry() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    let at = three_entries(&s, Some("2026-W43")).await;
    let all = tick_many(&s, &at, 3).await;
    assert!(
        !the_line(&all).contains("derived_from"),
        "nothing is open after the batch: {}",
        all.raw()
    );
    story.finish().await;
}

/// **A session is taught the carry once**: a second tick that still leaves an
/// entry open says nothing, and a session that has not been taught is taught.
#[tokio::test]
async fn a_session_is_taught_the_carry_once_and_a_fresh_session_is_taught_again() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    let at = three_entries(&s, Some("2026-W43")).await;
    add_entry(&s, "list:errands", "sweep", None).await;

    let first = tick_one(&s, &at[0]).await;
    assert!(the_line(&first).contains("derived_from"), "{}", first.raw());
    let second = tick_one(&s, &at[1]).await;
    assert!(
        !the_line(&second).contains("derived_from"),
        "this session was taught: {}",
        second.raw()
    );

    // A second boot of one bot is offered its stopped run; `new` starts another.
    let other = story.session_on(TODAY, Some("new")).await;
    let third = tick_one(&other, &at[2]).await;
    assert!(the_line(&third).contains("derived_from"), "{}", third.raw());
    story.finish().await;
}

/// **A batch spends the same teaching a single tick does**, so a batch that taught
/// the carry leaves the next single tick of the session silent.
#[tokio::test]
async fn a_batch_that_taught_the_carry_leaves_the_next_single_tick_silent() {
    let story = Story::begin_pretending_it_is("bot:gamma", TODAY).await;
    let s = story.session().await;
    let at = three_entries(&s, Some("2026-W43")).await;
    add_entry(&s, "list:errands", "sweep", None).await;

    let batch = tick_many(&s, &at, 1).await;
    assert!(the_line(&batch).contains("derived_from"), "{}", batch.raw());
    let single = tick_one(&s, &at[1]).await;
    assert!(
        !the_line(&single).contains("derived_from"),
        "this session was taught: {}",
        single.raw()
    );
    story.finish().await;
}

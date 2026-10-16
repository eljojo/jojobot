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
    work_due(s, "work:epsilon", "next", "2026-12-01", None).await;
    work_due(s, "work:phi", "next", "2026-10-14", None).await;
    work_due(s, "work:sigma", "now", "2026-10-22", None).await;
    work_due(s, "work:delta", "next", "2026-10-23", Some("work:epsilon")).await;
    work_due(s, "work:tau", "next", "2026-10-28", None).await;
    work_due(s, "work:omega", "done", "2026-10-21", None).await;
    // The edges of the week: its Monday and its Sunday.
    work_due(s, "work:kappa", "next", "2026-10-19", None).await;
    work_due(s, "work:lambda", "next", "2026-10-25", None).await;
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
        ["work:kappa", "work:sigma", "work:delta", "work:lambda"],
        "the week's Monday and Sunday are in it, and the next week's work is not: {plan}"
    );
    assert_eq!(handles(&plan["plan"], "overdue"), ["work:phi"], "{plan}");
    // Late as the week began, on Monday the 19th: work due on the 14th.
    assert_eq!(plan["plan"]["overdue"][0]["overdue_by_days"], 5, "{plan}");
    assert_eq!(
        handles(&plan["plan"], "ready"),
        ["work:phi", "work:kappa", "work:sigma", "work:lambda"],
        "work:delta waits on work:epsilon, so it is not ready: {plan}"
    );

    // Read again after the facts moved: nothing was stored on the plan.
    s.call(
        "update_entity",
        json!({"handle": "work:phi", "sets": {"status": "done"}}),
    )
    .await;
    let later = read(&s, "list:errands").await;
    assert!(
        handles(&later["plan"], "overdue").is_empty(),
        "finished work is no longer overdue: {later}"
    );
    assert_eq!(
        handles(&later["plan"], "ready"),
        ["work:kappa", "work:sigma", "work:lambda"],
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
    assert_eq!(handles(&plan["plan"], "due"), ["work:sigma"], "{plan}");
    assert_eq!(
        handles(&plan["plan"], "overdue"),
        ["work:phi", "work:kappa"],
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

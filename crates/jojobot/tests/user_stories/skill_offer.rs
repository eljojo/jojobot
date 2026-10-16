//! "I have never read the skills, and I am about to do what one of them is for."
//!
//! **The first time a session does something a skill covers without having read
//! that skill, the answer offers the skill once.** It names the skill and the
//! call that reads it, and never offers it a second time in that session. Reading
//! the skill first earns silence. The act itself is never refused or delayed, and
//! an act no skill covers offers nothing.
//!
//! The roster names are the cast: a cat that is wormed every month, a campaign,
//! and a person who owns a film.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// Whether an answer carries a teaching line that offers this skill: the line
/// names the skill and the verb that reads it. Read from the answer's own
/// `teaching` list, which is where a teaching rides.
fn offers(answer: &Answer, skill: &str) -> bool {
    answer.json()["teaching"].as_array().is_some_and(|lines| {
        lines.iter().any(|l| {
            l.as_str()
                .is_some_and(|l| l.contains("start_here") && l.contains(&format!("\"{skill}\"")))
        })
    })
}

/// A loop with a name and a last day, ready to be checked in on.
async fn a_worming_loop(s: &Session) {
    s.add("pet:snowball", "Snowball").await;
    s.add_under("pet:snowball", "rhythm:worming", "Worming")
        .await;
    s.event_with(
        "rhythm:worming",
        "gave the tablet, same as every month",
        json!({ "name": "Worming", "last_check_in": "2026-08-01",
                "cadence_days": "30", "advances_from": "due_date" }),
        &[],
    )
    .await;
}

async fn check_in(s: &Session, on: &str) -> Answer {
    s.call(
        "capture",
        json!({
            "subject": "rhythm:worming", "content": on,
            "provenance": "testimony", "recorded_at": "2026-09-01", "check_in": "ran",
        }),
    )
    .await
}

/// **A fresh session that checks in is offered the rhythms skill, once.** The
/// check-in lands either way, and the second one in the session is silent.
#[tokio::test]
async fn the_first_check_in_offers_the_rhythms_skill_and_the_second_does_not() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_worming_loop(&s).await;

    let first = check_in(&s, "gave the tablet").await;
    assert!(offers(&first, "rhythms"), "{}", first.raw());
    first.says("\"sets_count\":4");

    let second = check_in(&s, "gave the tablet again").await;
    assert!(!offers(&second, "rhythms"), "{}", second.raw());
    second.says("\"sets_count\":4");
    story.finish().await;
}

/// **A session that read the skill first is not offered it** — and another
/// session on the same loop, which did not read it, is. The pair is what makes
/// the silence mean something.
#[tokio::test]
async fn a_session_that_read_the_rhythms_skill_is_not_offered_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_worming_loop(&s).await;
    let reader = story.session_in("UTC", Some("new")).await;
    reader
        .call("start_here", json!({ "skill": "rhythms" }))
        .await
        .says("rhythms");

    let after_reading = check_in(&reader, "gave the tablet").await;
    assert!(
        !offers(&after_reading, "rhythms"),
        "{}",
        after_reading.raw()
    );
    after_reading.says("\"sets_count\":4");

    let unread = check_in(&s, "gave the tablet").await;
    assert!(offers(&unread, "rhythms"), "{}", unread.raw());
    story.finish().await;
}

/// **The first write on a project offers the projects skill once, and an
/// ordinary capture on a person offers nothing.** Reading the skill is per skill:
/// the session that read `rhythms` is still offered `projects`.
#[tokio::test]
async fn a_write_on_a_project_offers_the_projects_skill_once_and_a_person_offers_nothing() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.call("start_here", json!({ "skill": "rhythms" })).await;

    let person = s
        .call(
            "capture",
            json!({"subject": "person:milhouse", "content": "owns the film",
                   "provenance": "testimony"}),
        )
        .await;
    assert!(!offers(&person, "projects"), "{}", person.raw());
    assert!(!offers(&person, "rhythms"), "{}", person.raw());

    let created = s
        .call(
            "add_entity",
            json!({"kind": "project", "handle": "atlas", "name": "The Campaign",
                   "source": "user-named"}),
        )
        .await;
    assert!(offers(&created, "projects"), "{}", created.raw());

    let after = s
        .call(
            "capture",
            json!({"subject": "project:atlas", "content": "launch moved",
                   "provenance": "testimony"}),
        )
        .await;
    assert!(!offers(&after, "projects"), "{}", after.raw());
    story.finish().await;
}

/// **A session that read the projects skill is not offered it on a project** —
/// and another session, which did not read it, is. The project was made by a
/// third session, so neither of these two spent the offer on its creation.
#[tokio::test]
async fn a_session_that_read_the_projects_skill_is_not_offered_it() {
    let story = Story::begin("bot:gamma").await;
    let maker = story.session().await;
    maker.add("project:atlas", "The Campaign").await;

    let reader = story.session_in("UTC", Some("new")).await;
    reader
        .call("start_here", json!({ "skill": "projects" }))
        .await;
    let write = reader
        .call(
            "capture",
            json!({"subject": "project:atlas", "content": "launch is fixed",
                   "provenance": "testimony"}),
        )
        .await;
    assert!(!offers(&write, "projects"), "{}", write.raw());
    write.says("project:atlas");

    let other = story.session_in("UTC", Some("new")).await;
    let offered = other
        .call(
            "capture",
            json!({"subject": "project:atlas", "content": "needs a cut",
                   "provenance": "testimony"}),
        )
        .await;
    assert!(offers(&offered, "projects"), "{}", offered.raw());
    story.finish().await;
}

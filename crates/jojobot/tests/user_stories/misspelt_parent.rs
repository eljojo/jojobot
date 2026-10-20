//! "Is there no work in the shed, or did I misspell the shed?"
//!
//! **An empty answer means nothing is there, never that the place was misspelt.**
//! A selection that names a parent no thing answers to is refused, naming the
//! nearest handles, exactly as a subject that names nothing is. A parent that
//! exists and holds nothing still answers empty.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

const SHED: &str = "project:the-shed";

/// The shed with one piece of work in `now`.
async fn a_shed_with_work(s: &Session) {
    s.add(SHED, "The shed").await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "pour-the-slab", "name": "Pour the slab",
               "source": "user-named", "parent": SHED, "sets": {"status": "now"}}),
    )
    .await;
}

/// The shed's name with a letter too many, a handle no thing answers to.
fn misspelt_shed() -> String {
    format!("{SHED}d")
}

fn refusal_names(read: &Value, handle: &str) -> bool {
    read["candidates"]
        .as_array()
        .is_some_and(|all| all.iter().any(|one| one["handle"] == handle))
}

#[tokio::test]
async fn a_parent_that_names_nothing_is_refused_with_the_nearest_handles() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_shed_with_work(&s).await;

    // ── paired: a misspelt subject names the shed among its nearest ──────────
    let subject = s
        .refused("recall", json!({"subject": misspelt_shed()}))
        .await
        .json();
    assert!(refusal_names(&subject, SHED), "{subject}");

    // ── the ruler's first line: with and without a where clause and an order ──
    for (what, call) in [
        ("plain", json!({"kind": "work", "parent": misspelt_shed()})),
        (
            "with a where clause",
            json!({"kind": "work", "parent": misspelt_shed(), "where": "status = now"}),
        ),
        (
            "with an order",
            json!({"kind": "work", "parent": misspelt_shed(), "order": {"key": "status"}}),
        ),
    ] {
        let refused = s.refused("recall", call).await.json();
        assert!(
            refusal_names(&refused, SHED),
            "{what}: the refusal names the nearest handles, {SHED} among them: {refused}"
        );
    }
    story.finish().await;
}

/// **A parent that exists and holds nothing still answers empty**, as an empty
/// board does. Paired with the shed, which holds work and answers it.
#[tokio::test]
async fn a_real_parent_that_holds_no_work_still_answers_empty() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_shed_with_work(&s).await;
    s.add("project:the-barn", "The barn").await;
    for call in [
        json!({"kind": "work", "parent": "project:the-barn"}),
        json!({"kind": "work", "parent": "project:the-barn", "where": "status = now"}),
        json!({"kind": "work", "parent": "project:the-barn", "order": {"key": "status"}}),
    ] {
        let empty = s.call("recall", call).await.json();
        assert_eq!(
            empty["count"], 0,
            "a real parent with no work is empty: {empty}"
        );
        assert!(empty.get("candidates").is_none(), "{empty}");
    }
    // …and the shed's own work still answers.
    let work = s
        .call(
            "recall",
            json!({"kind": "work", "parent": SHED, "where": "status = now"}),
        )
        .await
        .json();
    assert_eq!(work["count"], 1, "{work}");
    story.finish().await;
}

/// **A view that takes a parent through an input behaves the same.** `board` and
/// `ready` take the project as an input and pass it as the parent.
#[tokio::test]
async fn a_view_whose_input_is_the_parent_refuses_a_misspelt_project_too() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_shed_with_work(&s).await;
    for view in ["board", "ready"] {
        let refused = s
            .refused(
                "recall",
                json!({"view": view, "inputs": {"project": misspelt_shed()}}),
            )
            .await
            .json();
        assert!(
            refusal_names(&refused, SHED),
            "{view}: the refusal names the nearest handles, {SHED} among them: {refused}"
        );
        let found = s
            .call("recall", json!({"view": view, "inputs": {"project": SHED}}))
            .await
            .json();
        assert_eq!(
            found["count"], 1,
            "{view}: the real project answers: {found}"
        );
    }
    story.finish().await;
}

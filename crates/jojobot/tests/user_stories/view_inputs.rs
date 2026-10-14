//! "What can I start under this project?"
//!
//! **A view is a saved recall with typed inputs**, so the simplest session asks a
//! premade view by name and fills in what it takes, and never learns the query
//! language. This session has never sent a `where` or a `fields`: it lists the
//! views, reads what the ready view takes, asks it for one project, and is told
//! plainly when it gets an input wrong.
//!
//! **The negatives are the half that makes the positive mean something.** The
//! same view asked with nothing answers for every project; a handle of the wrong
//! kind, an input nobody declared and a view declared with an input it never uses
//! are each refused naming what the view takes.

use serde_json::json;

use super::dsl::Story;

/// The handles an answer lists, in order.
fn listed(answer: &super::dsl::Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn a_session_asks_the_ready_view_for_one_project_without_learning_the_query_language() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("project:atlas", "Atlas").await;
    s.add("project:the-move", "The Move").await;
    s.add("person:bart", "Bart").await;
    let work = |slug: &str, parent: &str, status: &str| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": {"status": status}})
    };
    s.call("add_entity", work("sigma", "project:atlas", "next"))
        .await;
    s.call("add_entity", work("tau", "project:atlas", "now"))
        .await;
    s.call("add_entity", work("phi", "project:the-move", "next"))
        .await;
    s.call("add_entity", work("omega", "project:atlas", "done"))
        .await;

    // ── the cold session finds the view and what it takes ───────────────────
    let views = s.call("recall", json!({"kind": "view"})).await;
    views.says("view:ready");
    views.says("project: kind project?");

    // ── asked with its input, it answers for that project only ──────────────
    let under_atlas = s
        .call(
            "recall",
            json!({"view": "ready", "inputs": {"project": "project:atlas"}}),
        )
        .await;
    assert_eq!(
        listed(&under_atlas),
        ["work:sigma", "work:tau"],
        "{}",
        under_atlas.raw()
    );

    // …and it is the answer the long way round gives on the same store.
    let long_way = s
        .call(
            "recall",
            json!({"view": "ready", "parent": "project:atlas"}),
        )
        .await;
    assert_eq!(
        listed(&long_way),
        listed(&under_atlas),
        "{}",
        long_way.raw()
    );

    // ── the same view asked with nothing answers for all the ready work ─────
    let everything = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&everything),
        ["work:phi", "work:sigma", "work:tau"],
        "{}",
        everything.raw()
    );

    // ── an input of the wrong kind is refused, naming what the view takes ───
    let wrong = s
        .refused(
            "recall",
            json!({"view": "ready", "inputs": {"project": "person:bart"}}),
        )
        .await;
    wrong.says("project: kind project?");
    wrong.says("person:bart");

    // ── an input nobody declared is refused, naming the declared ones ───────
    let unknown = s
        .refused(
            "recall",
            json!({"view": "ready", "inputs": {"projekt": "project:atlas"}}),
        )
        .await;
    unknown.says("projekt");
    unknown.says("project: kind project?");

    // ── inputs sent with no view are refused rather than ignored ────────────
    s.refused(
        "recall",
        json!({"kind": "work", "inputs": {"project": "project:atlas"}}),
    )
    .await
    .says("view");

    // ── a rename leaves the question answering ──────────────────────────────
    s.call(
        "rename_entity",
        json!({"handle": "project:atlas", "to": "project:atlas-two"}),
    )
    .await;
    for handle in ["project:atlas", "project:atlas-two"] {
        let after = s
            .call(
                "recall",
                json!({"view": "ready", "inputs": {"project": handle}}),
            )
            .await;
        assert_eq!(
            listed(&after),
            ["work:sigma", "work:tau"],
            "asked as {handle}: {}",
            after.raw()
        );
    }

    s.wrap("asked the ready view for one project by name").await;
}

#[tokio::test]
async fn a_view_declared_with_inputs_is_asked_by_name_and_a_bad_declaration_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("project:atlas", "Atlas").await;
    s.add("project:the-move", "The Move").await;
    let work = |slug: &str, parent: &str, due: &str| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": {"status": "next", "due": due}})
    };
    s.call("add_entity", work("sigma", "project:atlas", "2026-10-20"))
        .await;
    s.call("add_entity", work("tau", "project:atlas", "2026-12-01"))
        .await;
    s.call("add_entity", work("phi", "project:the-move", "2026-10-21"))
        .await;

    // ── a view that takes a project and a day ───────────────────────────────
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "my-projects", "name": "My projects",
               "source": "user-named",
               "sets": {"selects": "work",
                        "inputs": "project: kind project, before: date?",
                        "parent": "$project",
                        "where": "due <= $before"}}),
    )
    .await;
    s.call(
        "declare_type",
        json!({"name": "dated-work", "fields": [{"key": "due", "holds": "date"}]}),
    )
    .await;

    let both = s
        .call(
            "recall",
            json!({"view": "my-projects",
                   "inputs": {"project": "project:atlas", "before": "2026-11-01"}}),
        )
        .await;
    assert_eq!(listed(&both), ["work:sigma"], "{}", both.raw());

    // The optional input left out drops its term: every piece under the project.
    let only_project = s
        .call(
            "recall",
            json!({"view": "my-projects", "inputs": {"project": "project:atlas"}}),
        )
        .await;
    assert_eq!(
        listed(&only_project),
        ["work:sigma", "work:tau"],
        "{}",
        only_project.raw()
    );

    // The required input left out is refused, naming both.
    let missing = s
        .refused("recall", json!({"view": "my-projects", "inputs": {}}))
        .await;
    missing.says("project: kind project");
    missing.says("before: date?");

    // A date that is not a date is refused.
    s.refused(
        "recall",
        json!({"view": "my-projects",
               "inputs": {"project": "project:atlas", "before": "soon"}}),
    )
    .await
    .says("before");

    // ── a declaration that does not hold together is refused when written ───
    s.refused(
        "add_entity",
        json!({"kind": "view", "handle": "my-friends", "name": "My friends",
               "source": "user-named",
               "sets": {"selects": "work", "inputs": "project: kind project",
                        "where": "parent = $projekt"}}),
    )
    .await
    .says("projekt");
    s.refused(
        "add_entity",
        json!({"kind": "view", "handle": "my-friends", "name": "My friends",
               "source": "user-named",
               "sets": {"selects": "work", "inputs": "project kind project"}}),
    )
    .await
    .says("name: type");

    s.wrap("declared a view with inputs and asked it by name")
        .await;
}

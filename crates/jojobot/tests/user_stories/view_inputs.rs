//! "What can I start on the shed?"
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

    s.add("project:the-shed", "The shed").await;
    s.add("project:kitchen-floor", "The kitchen floor").await;
    s.add("person:bart", "Bart").await;
    let work = |slug: &str, parent: &str, status: &str| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": {"status": status}})
    };
    s.call(
        "add_entity",
        work("frame-the-walls", "project:the-shed", "next"),
    )
    .await;
    s.call(
        "add_entity",
        work("build-the-roof", "project:the-shed", "now"),
    )
    .await;
    s.call(
        "add_entity",
        work("lay-the-tiles", "project:kitchen-floor", "next"),
    )
    .await;
    s.call(
        "add_entity",
        work("pour-the-slab", "project:the-shed", "done"),
    )
    .await;

    // ── the cold session finds the view and what it takes ───────────────────
    let views = s.call("recall", json!({"kind": "view"})).await;
    views.says("view:ready");
    views.says("project: kind project?");

    // ── asked with its input, it answers for that project only ──────────────
    let under_the_shed = s
        .call(
            "recall",
            json!({"view": "ready", "inputs": {"project": "project:the-shed"}}),
        )
        .await;
    assert_eq!(
        listed(&under_the_shed),
        ["work:build-the-roof", "work:frame-the-walls"],
        "{}",
        under_the_shed.raw()
    );

    // …and it is the answer the long way round gives on the same store.
    let long_way = s
        .call(
            "recall",
            json!({"view": "ready", "parent": "project:the-shed"}),
        )
        .await;
    assert_eq!(
        listed(&long_way),
        listed(&under_the_shed),
        "{}",
        long_way.raw()
    );

    // ── the same view asked with nothing answers for all the ready work ─────
    let everything = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&everything),
        [
            "work:build-the-roof",
            "work:frame-the-walls",
            "work:lay-the-tiles"
        ],
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
            json!({"view": "ready", "inputs": {"projekt": "project:the-shed"}}),
        )
        .await;
    unknown.says("projekt");
    unknown.says("project: kind project?");

    // ── inputs sent with no view are refused rather than ignored ────────────
    s.refused(
        "recall",
        json!({"kind": "work", "inputs": {"project": "project:the-shed"}}),
    )
    .await
    .says("view");

    // ── a rename leaves the question answering ──────────────────────────────
    s.call(
        "rename_entity",
        json!({"handle": "project:the-shed", "to": "project:backyard-shed"}),
    )
    .await;
    for handle in ["project:the-shed", "project:backyard-shed"] {
        let after = s
            .call(
                "recall",
                json!({"view": "ready", "inputs": {"project": handle}}),
            )
            .await;
        assert_eq!(
            listed(&after),
            ["work:build-the-roof", "work:frame-the-walls"],
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

    s.add("project:the-shed", "The shed").await;
    s.add("project:kitchen-floor", "The kitchen floor").await;
    let work = |slug: &str, parent: &str, due: &str| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": {"status": "next", "due": due}})
    };
    s.call(
        "add_entity",
        work("frame-the-walls", "project:the-shed", "2026-10-20"),
    )
    .await;
    s.call(
        "add_entity",
        work("build-the-roof", "project:the-shed", "2026-12-01"),
    )
    .await;
    s.call(
        "add_entity",
        work("lay-the-tiles", "project:kitchen-floor", "2026-10-21"),
    )
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
                   "inputs": {"project": "project:the-shed", "before": "2026-11-01"}}),
        )
        .await;
    assert_eq!(listed(&both), ["work:frame-the-walls"], "{}", both.raw());

    // The optional input left out drops its term: every piece under the project.
    let only_project = s
        .call(
            "recall",
            json!({"view": "my-projects", "inputs": {"project": "project:the-shed"}}),
        )
        .await;
    assert_eq!(
        listed(&only_project),
        ["work:build-the-roof", "work:frame-the-walls"],
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
               "inputs": {"project": "project:the-shed", "before": "soon"}}),
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

/// "What is coming up, and what is already late?" A cold session lists the views,
/// finds `due` and the one input it takes, and asks it with nothing for the week
/// ahead and with a `by` for further out. **It names no kind**: a loop and a
/// promise come back in one list, the late one with how late it is and the one
/// not yet due with how soon.
#[tokio::test]
async fn a_session_asks_the_due_view_what_is_coming_and_what_is_late() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-03-15").await;
    let s = story.session().await;

    s.add("thing:kettle", "The kettle").await;
    s.add_under("thing:kettle", "rhythm:descale", "Descale the kettle")
        .await;
    s.event_with(
        "rhythm:descale",
        "descaled it",
        json!({
            "name": "Descale the kettle",
            "last_check_in": "2026-03-01",
            "counts_from": "2026-03-01",
            "advances_from": "due_date",
            "cadence_days": "7",
        }),
        &[],
    )
    .await;
    s.add("project:atlas", "Atlas").await;
    let promise = |slug: &str, day: &str| {
        json!({"kind": "promise", "handle": slug, "name": slug, "parent": "project:atlas",
               "source": "user-named", "sets": {"promised_by": day}})
    };
    s.call("add_entity", promise("delta", "2026-03-18")).await;
    s.call("add_entity", promise("gamma", "2026-04-20")).await;

    // ── the cold session finds the view and the input it takes ──────────────
    let views = s.call("recall", json!({"kind": "view"})).await;
    views.says("view:due");
    views.says("by: date? = today+7d");

    // ── asked with nothing, a week ahead: the late loop, then the near promise
    let week = s.call("recall", json!({"view": "due"})).await;
    assert_eq!(
        listed(&week),
        ["rhythm:descale", "promise:delta"],
        "{}",
        week.raw()
    );
    let objects = week.json()["objects"].as_array().expect("objects").clone();
    assert_eq!(objects[0]["overdue_by_days"], 7, "{}", week.raw());
    assert_eq!(objects[1]["due_in_days"], 3, "{}", week.raw());

    // ── what the view left out says why, by kind, and the call that lists it ─
    let because = &week.json()["overdue_excluded_because"];
    assert_eq!(
        because["reasons"]["not_due_yet"]["kinds"]["promise"],
        1,
        "the far promise is left out because it is not due yet: {}",
        week.raw()
    );
    assert!(
        because["how_to_read"]
            .as_str()
            .is_some_and(|how| how.contains("kind")),
        "{}",
        week.raw()
    );
    let promises = s.call("recall", json!({"kind": "promise"})).await;
    let mut promises = listed(&promises);
    promises.sort();
    assert_eq!(
        promises,
        ["promise:delta", "promise:gamma"],
        "the kind the note names lists the far promise too"
    );

    // ── asked as of a later day, the far promise joins them ─────────────────
    let later = s
        .call(
            "recall",
            json!({"view": "due", "inputs": {"by": "2026-05-01"}}),
        )
        .await;
    assert_eq!(
        listed(&later),
        ["rhythm:descale", "promise:delta", "promise:gamma"],
        "{}",
        later.raw()
    );

    s.wrap("asked the due view what is coming and what is late")
        .await;
}

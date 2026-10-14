//! "What is on this project's board, and what can it start next?"
//!
//! The software ships the questions a session asks about work, and each takes the
//! project it is about. This session has never sent a `where`: it names a view
//! and a project.
//!
//! **Every question is paired with what it must leave out.** The board of one
//! project holds none of another's work, and the plan holds none of another
//! project's blockers.

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
async fn a_session_reads_the_board_of_one_project_by_name() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("project:atlas", "Atlas").await;
    s.add("project:the-move", "The Move").await;
    s.add("person:bart", "Bart").await;
    let work = |slug: &str, parent: &str, sets: serde_json::Value| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": sets})
    };
    s.call(
        "add_entity",
        work(
            "sigma",
            "project:atlas",
            json!({"status": "now", "owner": "person:bart", "effort": "large"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "tau",
            "project:atlas",
            json!({"status": "next", "depends_on": "work:sigma"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work("omega", "project:atlas", json!({"status": "done"})),
    )
    .await;
    s.call(
        "add_entity",
        work("phi", "project:the-move", json!({"status": "now"})),
    )
    .await;

    // ── the board of one project, grouped by status ─────────────────────────
    let board = s
        .call(
            "recall",
            json!({"view": "board", "inputs": {"project": "project:atlas"}}),
        )
        .await;
    assert_eq!(
        listed(&board),
        ["work:omega", "work:tau", "work:sigma"],
        "{}",
        board.raw()
    );
    // It shows the three keys a board is read by, and says what it left out.
    let sigma = board.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .find(|o| o["id"] == "work:sigma")
        .expect("sigma")
        .clone();
    assert_eq!(sigma["fields"]["status"], "now", "{}", board.raw());
    assert_eq!(sigma["fields"]["owner"], "person:bart", "{}", board.raw());
    assert!(sigma["fields"].get("effort").is_none(), "{}", board.raw());
    assert!(
        sigma["fields_left_out"]
            .as_str()
            .is_some_and(|said| said.contains("effort")),
        "{}",
        board.raw()
    );

    // ── what it must not include ────────────────────────────────────────────
    board.never_says("work:phi");

    // ── the project is required ─────────────────────────────────────────────
    s.refused("recall", json!({"view": "board", "inputs": {}}))
        .await
        .says("project: kind project");

    s.wrap("read the board of one project by name").await;
}

#[tokio::test]
async fn the_plan_view_takes_a_project_and_lists_only_its_plan() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("project:atlas", "Atlas").await;
    s.add("project:the-move", "The Move").await;
    let work = |slug: &str, parent: &str, sets: serde_json::Value| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": sets})
    };
    s.call(
        "add_entity",
        work("sigma", "project:atlas", json!({"status": "now"})),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "tau",
            "project:atlas",
            json!({"status": "next", "depends_on": "work:sigma"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work("phi", "project:the-move", json!({"status": "next"})),
    )
    .await;

    // The views list what the plan takes.
    s.call("recall", json!({"kind": "view"}))
        .await
        .says("view:plan");

    let plan = s
        .call(
            "recall",
            json!({"view": "plan", "inputs": {"project": "project:atlas"}}),
        )
        .await;
    assert_eq!(listed(&plan), ["work:sigma", "work:tau"], "{}", plan.raw());
    // The blocked piece is in the plan with what stops it.
    let tau = &plan.json()["objects"][1];
    assert_eq!(tau["ready"], false, "{}", plan.raw());
    plan.never_says("work:phi");

    // Asked with nothing, it is every project's plan, as it was.
    let everything = s.call("recall", json!({"view": "plan"})).await;
    assert_eq!(
        listed(&everything),
        ["work:phi", "work:sigma", "work:tau"],
        "{}",
        everything.raw()
    );

    s.wrap("read the plan of one project by name").await;
}

#[tokio::test]
async fn a_session_asks_which_things_are_of_a_type_and_is_told_what_each_lacks() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.call(
        "declare_type",
        json!({"name": "bike", "fields": [
            {"key": "frame_size", "holds": "text"},
            {"key": "wheel_size", "holds": "text"}]}),
    )
    .await;
    s.add("thing:sigma", "Sigma").await;
    s.add("thing:tau", "Tau").await;
    s.add("thing:omega", "Omega").await;
    for (handle, sets) in [
        (
            "thing:sigma",
            json!({"frame_size": "56", "wheel_size": "700c"}),
        ),
        ("thing:tau", json!({"wheel_size": "26in"})),
        ("thing:omega", json!({"wattage": "900"})),
    ] {
        s.call(
            "capture",
            json!({"subject": handle, "content": "described", "provenance": "testimony",
                   "sets": sets}),
        )
        .await;
    }

    let bikes = s
        .call(
            "recall",
            json!({"view": "of-type", "inputs": {"type": "bike"}}),
        )
        .await;
    assert_eq!(
        listed(&bikes),
        ["thing:sigma", "thing:tau"],
        "{}",
        bikes.raw()
    );
    let answers = |handle: &str| {
        bikes.json()["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .find(|o| o["id"] == handle)
            .unwrap_or_else(|| panic!("{handle} is listed: {}", bikes.raw()))["answers"]
            .clone()
    };
    assert_eq!(
        answers("thing:sigma")["lacking"],
        json!([]),
        "{}",
        bikes.raw()
    );
    assert_eq!(
        answers("thing:tau")["lacking"],
        json!(["frame_size"]),
        "{}",
        bikes.raw()
    );

    // ── what it must not include ────────────────────────────────────────────
    bikes.never_says("thing:omega");

    // ── the type is required, and a type nobody declared is refused ─────────
    s.refused("recall", json!({"view": "of-type", "inputs": {}}))
        .await
        .says("type: text");
    s.refused(
        "recall",
        json!({"view": "of-type", "inputs": {"type": "unicycle"}}),
    )
    .await
    .says("bike");

    // ── the view says what it cannot find ───────────────────────────────────
    s.call("recall", json!({"subject": "view:of-type"}))
        .await
        .says("things only");

    s.wrap("asked which things are bikes by name").await;
}

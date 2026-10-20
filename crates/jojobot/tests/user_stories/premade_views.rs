//! "What is on the shed's board, and what can it start next?"
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

    s.add("project:the-shed", "The shed").await;
    s.add("project:kitchen-floor", "The kitchen floor").await;
    s.add("person:bart", "Bart").await;
    let work = |slug: &str, name: &str, parent: &str, sets: serde_json::Value| {
        json!({"kind": "work", "handle": slug, "name": name, "parent": parent,
               "source": "user-named", "sets": sets})
    };
    s.call(
        "add_entity",
        work(
            "frame-the-walls",
            "Frame the walls",
            "project:the-shed",
            json!({"status": "now", "owner": "person:bart", "effort": "large"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "build-the-roof",
            "Build the roof",
            "project:the-shed",
            json!({"status": "next", "depends_on": "work:frame-the-walls"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "pour-the-slab",
            "Pour the slab",
            "project:the-shed",
            json!({"status": "done"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "lay-the-tiles",
            "Lay the tiles",
            "project:kitchen-floor",
            json!({"status": "now"}),
        ),
    )
    .await;

    // ── the board of one project, grouped by status ─────────────────────────
    let board = s
        .call(
            "recall",
            json!({"view": "board", "inputs": {"project": "project:the-shed"}}),
        )
        .await;
    assert_eq!(
        listed(&board),
        [
            "work:pour-the-slab",
            "work:build-the-roof",
            "work:frame-the-walls"
        ],
        "{}",
        board.raw()
    );
    // It shows the three keys a board is read by, and says what it left out.
    let walls = board.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .find(|o| o["id"] == "work:frame-the-walls")
        .expect("the walls")
        .clone();
    assert_eq!(walls["fields"]["status"], "now", "{}", board.raw());
    assert_eq!(walls["fields"]["owner"], "person:bart", "{}", board.raw());
    assert!(walls["fields"].get("effort").is_none(), "{}", board.raw());
    // The third key a board is read by: what a piece waits on, on the piece that
    // waits.
    let roof = board.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .find(|o| o["id"] == "work:build-the-roof")
        .expect("the roof")
        .clone();
    assert_eq!(
        roof["fields"]["depends_on"],
        "work:frame-the-walls",
        "{}",
        board.raw()
    );
    assert!(
        walls["fields_left_out"]
            .as_str()
            .is_some_and(|said| said.contains("effort")),
        "{}",
        board.raw()
    );

    // ── what it must not include ────────────────────────────────────────────
    board.never_says("work:lay-the-tiles");

    // ── the project is required ─────────────────────────────────────────────
    let refusal = s
        .refused("recall", json!({"view": "board", "inputs": {}}))
        .await;
    assert_eq!(
        refusal.json()["attempted"],
        "view:board",
        "{}",
        refusal.raw()
    );
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    refusal.says("\"wrote\":false");

    s.wrap("read the board of one project by name").await;
}

/// **A read that selects nothing answers with nothing.** A project with no work
/// has an empty board, and the board's order key is not a misspelling just
/// because nothing holds it yet. The same order key on a project WITH work is
/// still checked against the keys that work holds.
#[tokio::test]
async fn a_project_with_no_work_has_an_empty_board_and_a_misspelt_order_is_still_caught_where_work_exists()
 {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("project:backyard-shed", "The backyard shed").await;
    s.add("project:the-shed", "The shed").await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "frame-the-walls", "name": "Frame the walls",
               "parent": "project:the-shed", "source": "user-named",
               "sets": {"status": "now"}}),
    )
    .await;

    // The empty board is an answer.
    let empty = s
        .call(
            "recall",
            json!({"view": "board", "inputs": {"project": "project:backyard-shed"}}),
        )
        .await;
    assert!(listed(&empty).is_empty(), "{}", empty.raw());

    // A misspelt order key on a selection that holds nothing answers empty too.
    let misspelt_on_nothing = s
        .call(
            "recall",
            json!({"kind": "work", "parent": "project:backyard-shed",
                   "order": {"key": "statuz", "direction": "ascending"}}),
        )
        .await;
    assert!(
        listed(&misspelt_on_nothing).is_empty(),
        "{}",
        misspelt_on_nothing.raw()
    );

    // The same project with work still orders by status.
    let board = s
        .call(
            "recall",
            json!({"view": "board", "inputs": {"project": "project:the-shed"}}),
        )
        .await;
    assert_eq!(listed(&board), ["work:frame-the-walls"], "{}", board.raw());

    // And a misspelt order key where work exists is refused, naming the keys held.
    let refused = s
        .refused(
            "recall",
            json!({"kind": "work", "parent": "project:the-shed",
                   "order": {"key": "statuz", "direction": "ascending"}}),
        )
        .await;
    // The refusal names the misspelt key and lists the keys the work holds, so
    // the right spelling is in the answer. Read at the place the advice lives,
    // because every refusal body carries `"status":"blocked"`.
    let advice = refused.text_at("/how_to_proceed");
    assert!(
        advice.contains("'statuz'"),
        "the refusal does not name the misspelt key: {advice}"
    );
    let held: Vec<&str> = advice
        .split("the keys held among them are: ")
        .nth(1)
        .unwrap_or_else(|| panic!("the refusal lists no keys held: {advice}"))
        .split(". ")
        .next()
        .unwrap_or_default()
        .split(", ")
        .map(|key| key.trim().trim_end_matches('.'))
        .collect();
    assert!(
        held.contains(&"status"),
        "the keys held do not include the right spelling: {held:?}"
    );
    story.finish().await;
}

#[tokio::test]
async fn the_plan_view_takes_a_project_and_lists_only_its_plan() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("project:the-shed", "The shed").await;
    s.add("project:kitchen-floor", "The kitchen floor").await;
    let work = |slug: &str, name: &str, parent: &str, sets: serde_json::Value| {
        json!({"kind": "work", "handle": slug, "name": name, "parent": parent,
               "source": "user-named", "sets": sets})
    };
    s.call(
        "add_entity",
        work(
            "frame-the-walls",
            "Frame the walls",
            "project:the-shed",
            json!({"status": "now"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "build-the-roof",
            "Build the roof",
            "project:the-shed",
            json!({"status": "next", "depends_on": "work:frame-the-walls"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "lay-the-tiles",
            "Lay the tiles",
            "project:kitchen-floor",
            json!({"status": "next"}),
        ),
    )
    .await;

    // The views list what the plan takes.
    s.call("recall", json!({"kind": "view"}))
        .await
        .says("view:plan");

    let plan = s
        .call(
            "recall",
            json!({"view": "plan", "inputs": {"project": "project:the-shed"}}),
        )
        .await;
    assert_eq!(
        listed(&plan),
        ["work:build-the-roof", "work:frame-the-walls"],
        "{}",
        plan.raw()
    );
    // The blocked piece is in the plan with what stops it.
    let roof = &plan.json()["objects"][0];
    assert_eq!(roof["ready"], false, "{}", plan.raw());
    assert_eq!(
        roof["blocked_by"],
        json!([{"blocker": "dependency", "handle": "work:frame-the-walls",
                "why": "unfinished"}]),
        "{}",
        plan.raw()
    );
    // The piece that waits on nothing is ready and names no blocker.
    let walls = &plan.json()["objects"][1];
    assert_eq!(walls["ready"], true, "{}", plan.raw());
    assert!(walls.get("blocked_by").is_none(), "{}", plan.raw());
    plan.never_says("work:lay-the-tiles");

    // Asked with nothing, it is every project's plan, as it was.
    let everything = s.call("recall", json!({"view": "plan"})).await;
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
    s.add("thing:gravel-bike", "Gravel bike").await;
    s.add("thing:road-bike", "Road bike").await;
    s.add("thing:floor-pump", "Floor pump").await;
    for (handle, sets) in [
        (
            "thing:gravel-bike",
            json!({"frame_size": "56", "wheel_size": "700c"}),
        ),
        ("thing:road-bike", json!({"wheel_size": "26in"})),
        ("thing:floor-pump", json!({"max_pressure": "160psi"})),
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
        ["thing:gravel-bike", "thing:road-bike"],
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
        answers("thing:gravel-bike")["lacking"],
        json!([]),
        "{}",
        bikes.raw()
    );
    assert_eq!(
        answers("thing:road-bike")["lacking"],
        json!(["frame_size"]),
        "{}",
        bikes.raw()
    );

    // ── what it must not include ────────────────────────────────────────────
    bikes.never_says("thing:floor-pump");

    // ── the type is required, and a type nobody declared is refused ─────────
    let refusal = s
        .refused("recall", json!({"view": "of-type", "inputs": {}}))
        .await;
    assert_eq!(
        refusal.json()["attempted"],
        "view:of-type",
        "{}",
        refusal.raw()
    );
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    refusal.says("\"wrote\":false");
    // A type nobody declared is refused, and the refusal lists the types that exist.
    s.refused(
        "recall",
        json!({"view": "of-type", "inputs": {"type": "unicycle"}}),
    )
    .await
    .says("bike");

    // ── the view says what it cannot find ───────────────────────────────────
    let view = s.call("recall", json!({"subject": "view:of-type"})).await;
    assert_eq!(
        view.json()["objects"][0]["fields"]["selects"],
        "thing",
        "{}",
        view.raw()
    );

    s.wrap("asked which things are bikes by name").await;
}

//! "Is this a thing with parts, or a thing on its own?"
//!
//! A session reads one project and is told, in the same answer, how many pieces
//! sit directly under it, how many were archived, and the call that lists them.
//! It makes that call and gets the pieces. It reads a piece with nothing under it
//! and is told nothing about children, because there are none.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_project_says_it_has_parts_and_a_leaf_says_nothing() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;
    for (handle, name) in [
        ("work:sigma", "Sigma"),
        ("work:tau", "Tau"),
        ("work:kappa", "Kappa"),
        ("work:omega", "Omega"),
    ] {
        s.add_under("project:atlas", handle, name).await;
    }
    s.call(
        "archive_entity",
        json!({"handle": "work:omega", "reason": "no longer part of the plan"}),
    )
    .await;

    // ── the project, asked about alone ──────────────────────────────────────
    let project = s.recall("project:atlas").await;
    let children = &project.json()["objects"][0]["children"];
    assert_eq!(children["count"], 3, "{}", project.raw());
    assert_eq!(children["archived"], 1, "{}", project.raw());
    // **The answer names the call that lists the parts**, and the call is read
    // from it: the handle in quotes is the `parent` to send. This is the positive
    // the leaf's silence below depends on.
    let how = children["how_to_list"]
        .as_str()
        .unwrap_or_else(|| panic!("the project says how to list its parts: {}", project.raw()));
    let parent = how
        .split('"')
        .nth(1)
        .unwrap_or_else(|| panic!("how_to_list quotes the parent: {how}"));
    assert_eq!(parent, "project:atlas", "{how}");

    // ── the call the answer names returns those parts ───────────────────────
    let parts = s.call("recall", json!({"parent": parent})).await;
    let body = parts.json();
    let handles: Vec<&str> = body["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(
        handles,
        ["work:kappa", "work:sigma", "work:tau"],
        "{}",
        parts.raw()
    );

    // ── a piece with nothing under it ───────────────────────────────────────
    s.recall("work:sigma").await.never_says("how_to_list");

    story.finish().await;
}

//! "I call that view something else."
//!
//! A view the software ships is the build's, but its name and its other names
//! are the operator's to set, the way the facts on it already are. This session
//! gives a shipped view a name of its own and another name, then reads it back
//! every way a session reads a thing: a recall, a search, the page, and the
//! screen a new thing faces when it takes the same name.
//!
//! **The negatives make the positive mean something.** The source stays the
//! build's and is refused with no token, a thing that already has the name is
//! refused it, and before the edit the view is called what the build calls it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_operator_gives_a_shipped_view_a_name_and_another_name() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── before: the build's name ────────────────────────────────────────────
    let before = s
        .call("recall", json!({"subject": "view:colleagues"}))
        .await;
    assert_eq!(
        before.json()["objects"][0]["name"],
        "The Colleagues",
        "{}",
        before.raw()
    );

    // ── the edit ────────────────────────────────────────────────────────────
    let edited = s
        .call(
            "update_entity",
            json!({"handle": "view:colleagues", "name": "The crew", "aliases": ["Bench"]}),
        )
        .await;
    assert!(edited.json().get("name").is_none(), "{}", edited.raw());
    assert_eq!(edited.json()["success"], true);

    // ── after: every read shows it ──────────────────────────────────────────
    let after = s
        .call("recall", json!({"subject": "view:colleagues"}))
        .await;
    let view = &after.json()["objects"][0];
    assert_eq!(view["name"], "The crew", "{}", after.raw());
    assert_eq!(view["alternateName"], json!(["Bench"]), "{}", after.raw());
    assert_eq!(
        view["source"],
        "jojobot",
        "the build keeps the source: {}",
        after.raw()
    );
    for word in ["Bench", "crew"] {
        let found = s.call("search", json!({"query": word})).await;
        assert!(
            found.json()["results"].as_array().is_some_and(|hits| hits
                .iter()
                .any(|hit| hit["hit"] == "entity" && hit["id"] == "view:colleagues")),
            "a search for {word} finds the view itself: {}",
            found.raw()
        );
    }
    story
        .page("view:colleagues")
        .await
        .says("The crew")
        .says("Bench");

    // ── the screen reads the same names ─────────────────────────────────────
    let taken = s
        .refused(
            "add_entity",
            json!({"kind": "view", "handle": "bench", "name": "Bench", "source": "user-named"}),
        )
        .await;
    taken.says("view:colleagues");

    // ── what stays the build's ──────────────────────────────────────────────
    let source = s
        .refused(
            "update_entity",
            json!({"handle": "view:colleagues", "source": "me"}),
        )
        .await;
    assert!(
        !source.raw().contains("override_token"),
        "a refusal nothing can lift offers no token: {}",
        source.raw()
    );

    s.wrap("gave the colleagues view a name of my own and another name")
        .await;
    story.finish().await;
}

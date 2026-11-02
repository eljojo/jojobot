//! "I declared a type before I had started. What now?"
//!
//! A declaration is made by a run, so a call with no session is refused, and the
//! refusal names the door that hands one out. The same declaration, from a
//! booted session, lands.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_declaration_with_no_session_is_refused_naming_start_here_and_a_booted_one_lands() {
    let story = Story::begin("bot:otto").await;
    let declaration = json!({
        "name": "snacking",
        "fields": [{ "key": "donuts", "folds": "sum" }, { "key": "mood" }],
    });

    // ── no session: refused, with the way to get one ────────────────────────
    let refused = story
        .refused_at_the_door("declare_type", declaration.clone())
        .await;
    assert_eq!(
        refused.json()["refusal_code"],
        "SessionUnbound",
        "{}",
        refused.raw()
    );
    refused.says("start_here");

    // ── a booted session declares the same type ─────────────────────────────
    let s = story.session().await;
    let declared = s.call("declare_type", declaration).await;
    declared.says("snacking").never_says("SessionUnbound");

    s.wrap("declared a type after booting").await;
    story.finish().await;
}

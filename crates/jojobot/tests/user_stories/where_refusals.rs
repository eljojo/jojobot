//! "I asked the board and it said no. What do I send instead?"
//!
//! **A refusal of a question over keys names the call that fixes it, and the
//! story makes that call.** Each case reads the refusal, makes the call it
//! names, and asserts the answer, so the advice is proven to work and not only
//! printed. The negative stands beside each: a refusal that says one thing does
//! not say it for a caller who made a different mistake.
//!
//! The board is the one `which_things` uses: `gamma` is open and due soon,
//! `delta` is open and due later, `epsilon` is done, `omega` holds no status.

use serde_json::json;

use super::dsl::{Answer, Session, Story};
use super::which_things::a_board;

/// The handles an answer named, in order.
fn handles_of(answer: &Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a recall answers with objects: {}", answer.raw()))
        .iter()
        .filter_map(|o| o["id"].as_str().map(str::to_string))
        .collect()
}

/// A board whose keys no type declares, so an ordering on them is not licensed.
async fn an_undeclared_board(s: &Session) {
    for (handle, name, size, touches) in [
        ("thing:gamma", "Gamma", "10", "essay.rs, skills.rs"),
        ("thing:delta", "Delta", "2", "essay.rs"),
    ] {
        s.add(handle, name).await;
        s.event_with(
            handle,
            "a record",
            json!({ "size": size, "touches": touches }),
            &[],
        )
        .await;
    }
}

/// **A doubled equals sign and a quoted date are refused with what to write**, and
/// the call they name answers.
#[tokio::test]
async fn a_parse_refusal_names_the_spelling_that_works() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;

    s.refused(
        "recall",
        json!({ "kind": "thing", "where": "status == done" }),
    )
    .await
    .says("==");
    let fixed = s
        .call(
            "recall",
            json!({ "kind": "thing", "where": "status = done" }),
        )
        .await;
    assert_eq!(handles_of(&fixed), ["thing:epsilon"], "{}", fixed.raw());

    s.refused(
        "recall",
        json!({ "kind": "thing", "where": "due < \"2026-10-15\"" }),
    )
    .await
    .says("literal");
    let fixed = s
        .call(
            "recall",
            json!({ "kind": "thing", "where": "due < 2026-10-15" }),
        )
        .await;
    assert_eq!(
        handles_of(&fixed),
        ["thing:epsilon", "thing:gamma"],
        "{}",
        fixed.raw()
    );
    story.finish().await;
}

//! "I declared a counter, and the kettle's number changed under me."
//!
//! A key folds one way for the whole instance, whichever type names it. A type
//! that counts a key some thing already holds would change what that thing reads
//! back, so declaring it asks first: it names the key and how many things would
//! change, writes nothing, and is lifted by the token it names with a reason in
//! words. The receipt then says how many things now read a total.

use serde_json::json;

use super::dsl::Session;
use super::dsl::Story;

/// What the kettle's `visits` reads now.
async fn visits(s: &Session) -> String {
    s.call("recall", json!({"subject": "thing:kettle"}))
        .await
        .json()["objects"][0]["fields"]["visits"]
        .as_str()
        .expect("the kettle holds visits")
        .to_string()
}

fn tally(counting: bool) -> serde_json::Value {
    json!({
        "name": "tally",
        "fields": [{
            "key": "visits", "holds": "number",
            "folds": if counting { "sum" } else { "newest" },
        }],
    })
}

/// The token a refusal names, read out of its advice as a caller has to.
fn token_in(refusal: &serde_json::Value) -> String {
    refusal["how_to_proceed"]
        .as_str()
        .and_then(|advice| advice.split("override_token: \"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .expect("the advice names the token")
        .to_string()
}

#[tokio::test]
async fn declaring_a_counter_over_a_held_key_asks_first_and_counts_what_it_changes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    for count in ["3", "5"] {
        s.call(
            "capture",
            json!({"subject": "thing:kettle", "content": format!("visited, total {count}"),
                   "provenance": "testimony", "sets": {"visits": count}}),
        )
        .await;
    }
    assert_eq!(visits(&s).await, "5");

    // ── the declaration asks first, naming the key and the count ────────────
    let refused = s.refused("declare_type", tally(true)).await;
    refused.says("`visits`").says("1 thing would change");
    assert_eq!(visits(&s).await, "5", "a refused call changes nothing read");
    let token = token_in(&refused.json());

    // ── the token with a written reason lands it, and the receipt counts ────
    let mut lifted = tally(true);
    lifted["override_token"] = json!(token);
    lifted["override_reason"] = json!("the kettle really is a counter");
    let landed = s.call("declare_type", lifted).await.json();
    assert_eq!(landed["fold_moved"][0]["key"], "visits", "{landed}");
    assert_eq!(landed["fold_moved"][0]["things"], 1, "{landed}");
    assert_eq!(
        landed["override_reason"], "the kettle really is a counter",
        "the receipt keeps the reason: {landed}"
    );
    assert_eq!(visits(&s).await, "8");

    // ── stopping the count is a change too, and asks the same way ───────────
    let stopping = s.refused("declare_type", tally(false)).await;
    stopping.says("1 thing would change");
    assert_eq!(visits(&s).await, "8");
    story.finish().await;
}

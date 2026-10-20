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

/// **A declaration over a key nothing holds lands without asking**, the
/// counterpart of the case above: the refusal is for a fold that would change
/// what a thing already reads back, and with no such thing there is nothing to
/// ask about. The receipt names no fold that moved, and the declaration is in
/// force afterwards: the next writes of the key total.
#[tokio::test]
async fn declaring_a_counter_over_a_key_nothing_holds_lands_without_asking() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;

    let landed = s.call("declare_type", tally(true)).await.json();
    assert_ne!(landed["status"], "blocked", "{landed}");
    assert!(
        landed.get("fold_moved").is_none(),
        "no thing changed, so no fold moved: {landed}"
    );

    for count in ["3", "5"] {
        s.call(
            "capture",
            json!({"subject": "thing:kettle", "content": format!("visited, total {count}"),
                   "provenance": "testimony", "sets": {"visits": count}}),
        )
        .await;
    }
    assert_eq!(
        visits(&s).await,
        "8",
        "the declared counter totals its writes"
    );
    story.finish().await;
}

/// "I declared a type that counts `starred`, and every bot's rule starred itself
/// onto the bot."
///
/// The six keys that describe a record (`starred`, `subject`, `purpose`,
/// `read_from`, `read_ref`, `recorded_by`) stay on their claim whatever a type
/// says. A declaration that names one with any fold of its own is refused,
/// naming the key, and the refusal on writing `starred` onto a thing stands
/// before and after. A declaration over an ordinary key is unchanged.
#[tokio::test]
async fn a_type_cannot_give_a_describing_key_a_fold_of_its_own() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    let star_the_kettle = || {
        json!({"subject": "thing:kettle", "content": "starred", "provenance": "testimony",
               "sets": {"starred": "true"}})
    };

    // ── a star cannot be set on a thing: the baseline the declaration must keep ─
    s.refused("capture", star_the_kettle())
        .await
        .says("starred");

    // ── (1) a counter over a describing key is refused, naming the key ──────
    for fold in ["sum", "newest"] {
        let refused = s
            .refused(
                "declare_type",
                json!({"name": "pantry", "fields": [{
                    "key": "starred", "holds": "number", "folds": fold,
                }]}),
            )
            .await;
        refused.says("starred").says("describe");
        refused.holds_at("/fix_by", "change");
    }

    // ── (2) the star is still refused on a thing afterwards ─────────────────
    s.refused("capture", star_the_kettle())
        .await
        .says("starred");

    // ── (3) a counter over an ordinary key still lands ──────────────────────
    let landed = s
        .call(
            "declare_type",
            json!({"name": "pantry", "fields": [{
                "key": "donuts", "holds": "number", "folds": "sum",
            }]}),
        )
        .await;
    landed.says("pantry");

    s.wrap("a describing key stays a label").await;
    story.finish().await;
}

/// What the kettle's `visits` holds, after two visits recorded as a total.
async fn a_kettle_that_was_visited_twice(s: &Session) {
    s.add("thing:kettle", "Kettle").await;
    for count in ["3", "5"] {
        s.call(
            "capture",
            json!({"subject": "thing:kettle", "content": format!("visited, total {count}"),
                   "provenance": "testimony", "sets": {"visits": count}}),
        )
        .await;
    }
}

/// **A declaration that is invalid is refused as invalid, and is never handed a
/// token that could lift it.** Over a key a thing already holds, a valid counter
/// asks first and names a token; the same declaration naming the key twice is
/// refused for that, with nothing to override, and the type is not declared.
#[tokio::test]
async fn a_declaration_naming_a_held_key_twice_is_refused_with_no_token_to_lift_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_kettle_that_was_visited_twice(&s).await;

    // The positive: the valid counter over the held key is refused with a token.
    let asking = s.refused("declare_type", tally(true)).await.json();
    let _ = token_in(&asking);

    let twice = json!({
        "name": "tally",
        "fields": [
            {"key": "visits", "holds": "number", "folds": "sum"},
            {"key": "visits", "holds": "number", "folds": "sum"},
        ],
    });
    let refused = s.refused("declare_type", twice.clone()).await;
    refused.holds_at("/fix_by", "change");
    assert!(
        !refused.raw().contains("override_token"),
        "an invalid declaration has no token: {}",
        refused.raw()
    );

    // A token the valid declaration was given lifts nothing for the invalid one.
    let mut lifted = twice;
    lifted["override_token"] = json!(token_in(&asking));
    lifted["override_reason"] = json!("trying the token on another declaration");
    s.refused("declare_type", lifted).await;
    assert_eq!(
        visits(&s).await,
        "5",
        "nothing was declared, so nothing moved"
    );
    story.finish().await;
}

/// **A token is for the declaration it was issued for.** The same name with one
/// more field is another declaration; the first one's token refuses it, and still
/// lifts the first.
#[tokio::test]
async fn a_token_lifts_only_the_declaration_it_was_issued_for() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_kettle_that_was_visited_twice(&s).await;

    let token = token_in(&s.refused("declare_type", tally(true)).await.json());

    let mut other = tally(true);
    other["fields"]
        .as_array_mut()
        .expect("fields")
        .push(json!({"key": "laps", "holds": "number", "folds": "sum"}));
    other["override_token"] = json!(token);
    other["override_reason"] = json!("the token was issued for another declaration");
    s.refused("declare_type", other).await;
    assert_eq!(
        visits(&s).await,
        "5",
        "the other declaration changed nothing"
    );

    let mut first = tally(true);
    first["override_token"] = json!(token);
    first["override_reason"] = json!("the kettle really is a counter");
    let landed = s.call("declare_type", first).await.json();
    assert_eq!(landed["fold_moved"][0]["key"], "visits", "{landed}");
    assert_eq!(visits(&s).await, "8");
    story.finish().await;
}

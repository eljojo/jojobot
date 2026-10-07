//! "I called the verb and left something off. What do I send instead?"
//!
//! A call missing an argument the verb requires used to be turned back by the
//! parameter layer with serde's own words, which name no way forward — and a
//! session that read them gave up on the verb and invented its own key. **The
//! answer has to read the way the answer to an argument the verb does not have
//! reads**: blocked, nothing written, what is missing, and what the verb takes.
//!
//! Three shapes, because the gate is one mechanism asked at three places: a
//! required argument of the call, a required argument inside a list's element,
//! and the next refusal a caller meets once they have sent everything — a name
//! the software ships.
//!
//! **The positive is in every case**: the same call with the argument present
//! lands, so a gate that refused every call would not pass.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_call_missing_a_required_argument_names_it_and_what_the_verb_takes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // Neither required argument is sent, so each one the refusal names is read
    // off what the verb takes and not echoed from the call.
    let refusal = s.refused("declare_type", json!({"label": "service"})).await;
    refusal
        .says("how_to_proceed")
        .says("fields")
        .says("name")
        .never_says("failed to deserialize");

    // **Every verb, not the one a run met.** Another verb with another
    // required argument gets the same answer.
    let capture = s
        .refused("capture", json!({"content": "a loose note"}))
        .await;
    capture
        .says("how_to_proceed")
        .says("subject")
        .never_says("failed to deserialize");

    // And with everything it needs, the call lands.
    s.call(
        "declare_type",
        json!({"name": "service", "fields": [{"key": "cost"}]}),
    )
    .await;
}

#[tokio::test]
async fn an_element_missing_its_required_argument_is_named_by_its_path() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    let refusal = s
        .refused(
            "declare_type",
            json!({"name": "service", "fields": [{"key": "cost"}, {"holds": "number"}]}),
        )
        .await;
    refusal
        .says("how_to_proceed")
        .says("fields[1]")
        .says("fields[1].key")
        .never_says("failed to deserialize");
}

#[tokio::test]
async fn declaring_a_shipped_type_says_it_exists_and_where_to_read_its_keys() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    let refusal = s
        .refused(
            "declare_type",
            json!({"name": "runs-out", "fields": [{"key": "expires"}]}),
        )
        .await;
    // The caller sent the name `runs-out` and the refusal echoes it, so that
    // spelling proves nothing. What only this refusal carries is where to read
    // the shipped type's keys, and that nothing was written.
    refusal.says("answers_type");
    assert_eq!(refusal.json()["wrote"], false, "{}", refusal.raw());

    // The way forward it names works: the shipped type's keys are readable.
    s.call("recall", json!({"answers_type": "runs-out"}))
        .await
        .says("runs_out");
}

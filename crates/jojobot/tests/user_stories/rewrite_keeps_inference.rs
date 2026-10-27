//! "Yesterday's run wrote a status note. Today's run changes one word of it."
//!
//! **An inference rewritten by another run stays an inference when the rewrite
//! names no provenance.** The run before had no way to be the operator or a
//! system: its words were its own reading, and the new words are another run's
//! reading. Keeping the label says what is true, so the rewrite lands. **The
//! other two kinds still ask**: an observation names a system as the source, and
//! words that system never held cannot go on carrying its name; testimony is the
//! operator's and is not rewritten in place by another run at all.

use serde_json::json;

use super::dsl::Story;

fn address_of(captured: &serde_json::Value) -> String {
    captured["address"]
        .as_str()
        .or_else(|| captured["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string()
}

/// **A later run rewrites an earlier run's inference and says nothing about
/// how it is known.** The words change, and the claim reads back as the
/// inference it was.
#[tokio::test]
async fn a_later_run_rewrites_an_earlier_runs_inference_and_it_stays_an_inference() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("thing:handcart", "The Handcart").await;
    let note = address_of(
        &first
            .call(
                "capture",
                json!({"subject": "thing:handcart", "content": "the wheel squeaks",
                       "provenance": "inference"}),
            )
            .await
            .json(),
    );
    first.wrap("wrote a note").await;

    let later = story.session_in("UTC", Some("new")).await;
    let rewritten = later
        .call(
            "update_fact",
            json!({"address": note, "content": "the wheel squeaks when loaded"}),
        )
        .await
        .json();
    assert_eq!(
        rewritten["provenance"], "inference",
        "an inference stays an inference: {rewritten}"
    );
    later
        .recall("thing:handcart")
        .await
        .claim(&note)
        .says("the wheel squeaks when loaded")
        .says("\"provenance\":\"inference\"");
    story.finish().await;
}

/// **The same rewrite of an observation is still refused**, and says what to
/// supply; nothing is written. Paired with the case above, so a gate that
/// refused nothing would not pass this one.
#[tokio::test]
async fn a_later_run_rewriting_an_observation_still_has_to_say_how_it_is_known() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("thing:handcart", "The Handcart").await;
    let seen = address_of(
        &first
            .call(
                "capture",
                json!({"subject": "thing:handcart", "content": "the wheel squeaks",
                       "provenance": "observation",
                       "fields": {"read_from": "the workshop log"}}),
            )
            .await
            .json(),
    );
    first.wrap("read the workshop log").await;

    let later = story.session_in("UTC", Some("new")).await;
    later
        .refused(
            "update_fact",
            json!({"address": seen, "content": "the wheel squeaks when loaded"}),
        )
        .await
        .says("provenance");
    later
        .recall("thing:handcart")
        .await
        .claim(&seen)
        .says("the wheel squeaks")
        .never_says("when loaded");
    story.finish().await;
}

/// **And the operator's words are not rewritten in place by another run**,
/// whether or not the rewrite names a provenance.
#[tokio::test]
async fn a_later_run_rewriting_testimony_with_no_provenance_is_still_refused() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("thing:handcart", "The Handcart").await;
    let said = first.fact("thing:handcart", "the handcart is red").await;
    first.wrap("wrote down what the operator said").await;

    let later = story.session_in("UTC", Some("new")).await;
    later
        .refused(
            "update_fact",
            json!({"address": said, "content": "the handcart is blue"}),
        )
        .await
        .says("archived");
    later
        .recall("thing:handcart")
        .await
        .claim(&said)
        .says("the handcart is red")
        .never_says("the handcart is blue");
    story.finish().await;
}

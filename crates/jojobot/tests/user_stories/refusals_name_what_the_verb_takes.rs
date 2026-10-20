//! "It told me to send an argument the verb I called does not take."
//!
//! **A refusal names the argument of the verb that was called.** A machine read
//! is stored as `read_from`, and `capture` takes it under that name, while
//! `put_document` takes it as `source`. And **a receipt means a claim was
//! settled**: it is accepted with a move to settled, or on a claim that is
//! settled already, and it is refused, with nothing written, on a claim that
//! stays open.

use serde_json::json;

use super::dsl::Story;

/// A machine read with no system named is refused by name, per verb.
#[tokio::test]
async fn a_read_with_no_system_is_refused_naming_the_argument_of_the_verb_called() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("document:homer", "Homer").await;
    s.add("thing:gamma", "Gamma").await;

    // put_document takes the system as `source`, so that is what it is told.
    let put = s
        .refused(
            "put_document",
            json!({
                "document": "document:homer", "markdown": "# Plan\n\nthe plan\n",
                "provenance": "observation",
            }),
        )
        .await;
    put.says("`source`");
    assert!(
        !put.raw().contains("read_from"),
        "put_document has no read_from argument: {}",
        put.raw()
    );

    // capture takes it as the field `read_from`, and is told so.
    s.refused(
        "capture",
        json!({
            "subject": "thing:gamma", "content": "the rigging holds",
            "provenance": "observation",
        }),
    )
    .await
    .says("read_from");

    // The pair: naming the system lets the same put through.
    let landed = s
        .call(
            "put_document",
            json!({
                "document": "document:homer", "markdown": "# Plan\n\nthe plan\n",
                "provenance": "observation", "source": "the planning wiki",
            }),
        )
        .await;
    landed.says("sections");
    story.finish().await;
}

/// A receipt without a move to settled is refused; with one, it settles.
#[tokio::test]
async fn a_receipt_is_accepted_only_with_a_move_to_settled() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:gamma", "Gamma").await;
    let claim = s.guess("thing:gamma", "the rigging holds").await;

    // An open claim, a receipt, and no standing: refused, nothing written.
    let refused = s
        .refused(
            "update_fact",
            json!({"address": claim, "receipt": {"check": "openly-checked the rigging"}}),
        )
        .await;
    refused.says("`standing`");
    s.recall("thing:gamma").await.never_says("openly-checked");

    // The pair: the same receipt with standing settled settles the claim.
    s.settle_on_a_check(&claim, "openly-checked the rigging")
        .await;
    s.recall("thing:gamma").await.says("openly-checked");

    // A claim that is settled already takes a new receipt without naming
    // standing, and the new one replaces the old.
    s.call(
        "update_fact",
        json!({"address": claim, "receipt": {"check": "freshly-checked the rigging"}}),
    )
    .await;
    let read = s.recall("thing:gamma").await;
    read.says("freshly-checked").never_says("openly-checked");
    story.finish().await;
}

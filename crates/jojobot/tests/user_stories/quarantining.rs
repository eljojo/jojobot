//! "Set that one aside, and say why — and if I forget to say why, tell me."
//!
//! A deliberate quarantine records a decision, so it needs the decision's
//! reason. A caller that sends a blank reason has not made one, and the verb
//! must not read that as a request to retire the message with whatever else the
//! call carried: that would file the message as handled when nobody handled it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_blank_quarantine_reason_is_refused_and_retires_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let stray = s
        .post("gamma", "Twice", "This one went out twice by mistake.")
        .await;
    let real = s.post("gamma", "Once", "This one is real work.").await;
    s.wrap("posted two messages").await;

    let g = story.as_bot("bot:gamma").await;

    // ── a blank reason is refused, whether it is empty or only spaces ───────
    g.refused(
        "mark_processed",
        json!({"message_id": &stray, "quarantine": "   "}),
    )
    .await
    .says("quarantine")
    .says("reason");
    // **Even beside notes**, which would otherwise have been read as the
    // retirement this call never asked for.
    g.refused(
        "mark_processed",
        json!({"message_id": &stray, "quarantine": "", "notes": "handled"}),
    )
    .await
    .says("reason");

    // **Nothing was retired.** Both messages are still waiting, so neither
    // refusal changed a state.
    g.call("read_mailbox", json!({"counts_only": true}))
        .await
        .says("\"new\":2")
        .says("\"processed\":0");

    // ── the positives the refusals rest on ──────────────────────────────────
    //
    // A real reason quarantines it.
    g.call(
        "mark_processed",
        json!({"message_id": &stray, "quarantine": "posted twice by mistake"}),
    )
    .await
    .says("\"state\":\"quarantined\"");
    // And a call that names no quarantine at all still retires with its notes.
    g.call(
        "mark_processed",
        json!({"message_id": &real, "notes": "did the work"}),
    )
    .await
    .says("\"state\":\"processed\"")
    .says("did the work");

    g.wrap("set the stray aside and finished the real one")
        .await;
    story.finish().await;
}

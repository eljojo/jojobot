//! "Set that one aside, and say why — and if I forget to say why, tell me."
//!
//! A deliberate quarantine records a decision, so it needs the decision's
//! reason. A caller that sends a blank reason has not made one, and the verb
//! must not read that as a request to retire the message with whatever else the
//! call carried: that would file the message as handled when nobody handled it.

use serde_json::{Value, json};

use super::dsl::Story;

/// **The verb's own refusal, and no other.** A blocked answer that wrote nothing,
/// wears `change`, and names each argument it is about in backticks. A refusal that merely mentions the words, such as one
/// that says an argument is unknown, is none of these, so it cannot stand in.
fn is_the_verbs_own_refusal(refusal: &Value, arguments: &[&str]) {
    assert_eq!(refusal["status"], "blocked", "{refusal}");
    assert_eq!(refusal["wrote"], false, "{refusal}");
    assert_eq!(refusal["fix_by"], "change", "{refusal}");
    let how = refusal["how_to_proceed"].as_str().expect("a way forward");
    for argument in arguments {
        assert!(how.contains(&format!("`{argument}`")), "{refusal}");
    }
}

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
    let spaces = g
        .refused(
            "mark_processed",
            json!({"message_id": &stray, "quarantine": "   "}),
        )
        .await
        .json();
    is_the_verbs_own_refusal(&spaces, &["quarantine"]);
    // **Even beside notes**, which would otherwise have been read as the
    // retirement this call never asked for.
    let beside_notes = g
        .refused(
            "mark_processed",
            json!({"message_id": &stray, "quarantine": "", "notes": "handled"}),
        )
        .await
        .json();
    is_the_verbs_own_refusal(&beside_notes, &["quarantine"]);

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

/// A quarantine records no outcome, so `notes` beside it has nowhere to go.
///
/// The verb's own description says the two are mutually exclusive. Taking one
/// and dropping the other without a word would cost a caller the text it wrote
/// and teach it nothing, so the pair is refused and nothing is written.
#[tokio::test]
async fn a_quarantine_beside_notes_is_refused_and_nothing_is_written() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let stray = s
        .post("gamma", "Twice", "This one went out twice by mistake.")
        .await;
    s.wrap("posted one message").await;

    let g = story.as_bot("bot:gamma").await;

    // The pair is refused, and the refusal names both halves and the way out.
    let pair = g
        .refused(
            "mark_processed",
            json!({
                "message_id": &stray,
                "quarantine": "posted twice by mistake",
                "notes": "ignored the copy",
            }),
        )
        .await
        .json();
    is_the_verbs_own_refusal(&pair, &["quarantine", "notes"]);

    // **Nothing was written**: the message is still waiting, neither
    // quarantined nor retired.
    g.call("read_mailbox", json!({"counts_only": true}))
        .await
        .says("\"new\":1")
        .says("\"processed\":0");

    // The positive each half rests on: the same reason quarantines. Blank notes
    // beside it count as absent, the rule `notes` has always read by, so they do
    // not make a pair to refuse.
    g.call(
        "mark_processed",
        json!({"message_id": &stray, "quarantine": "posted twice by mistake", "notes": "  "}),
    )
    .await
    .says("\"state\":\"quarantined\"");

    g.wrap("refused the pair, then set the stray aside").await;
    story.finish().await;
}

/// **A finished message is never set aside.** Processed is terminal and its
/// archive stays readable for ever, and no verb lifts a quarantine, so
/// quarantining a processed message would make a finished record unreadable for
/// good. Another run of the same bot asks for it; the call is refused, nothing
/// is written, and the message still reads back processed with its notes.
#[tokio::test]
async fn a_processed_message_is_refused_a_quarantine_and_stays_readable() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:lisa", "Lisa").await;
    let done = s.post("lisa", "Done", "This one was handled.").await;
    let open = s
        .post("lisa", "Open", "This one is still being worked.")
        .await;
    s.wrap("posted two messages").await;

    let first = story.as_bot("bot:lisa").await;
    first
        .call(
            "mark_processed",
            json!({"message_id": &done, "notes": "handled on the first run"}),
        )
        .await
        .says("\"state\":\"processed\"");

    // ── another run of the same bot asks to set it aside ────────────────────
    let second = story.as_bot("bot:lisa").await;
    let late = second
        .refused(
            "mark_processed",
            json!({"message_id": &done, "quarantine": "second thoughts"}),
        )
        .await
        .json();
    assert_eq!(late["status"], "blocked", "{late}");
    assert_eq!(late["wrote"], false, "{late}");
    assert!(
        late["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("already processed")),
        "{late}"
    );
    // The refusal names the verb that still reads it.
    assert!(late.to_string().contains("read_message"), "{late}");

    // **Nothing was written**: the archive reads back processed, with its
    // notes, and the box counts it as processed.
    let read = second
        .call("read_message", json!({"message_id": &done}))
        .await;
    read.says("\"state\":\"processed\"")
        .says("handled on the first run");
    second
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .says("\"processed\":1");

    // ── the positive the refusal rests on: a message not yet processed is set
    // aside by the same call ────────────────────────────────────────────────
    let landed = second
        .call(
            "mark_processed",
            json!({"message_id": &open, "quarantine": "posted by mistake"}),
        )
        .await;
    landed.says("\"state\":\"quarantined\"");
    // A quarantine that lands sends the caller nowhere.
    assert!(!landed.json().to_string().contains("read_message"));

    second.wrap("left the archive alone").await;
    story.finish().await;
}

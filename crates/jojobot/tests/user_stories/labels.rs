//! "Why does the bot hold `starred`?"
//!
//! A claim carries labels about itself — where it was read, what it is for,
//! whether it is marked to ride a boot — and every key on every claim used to
//! fold onto the thing the claim is about, so a bot appeared to hold its own
//! rule's marks and a loop held what one of its check-ins read from. The build
//! declares six keys as describing their record: the claim keeps them and a
//! read of the claim sees them, and only the thing's own fields do not.

use serde_json::json;

use super::dsl::Story;

/// **A rule's marks are the rule's, not the bot's.**
#[tokio::test]
async fn a_rules_marks_belong_to_the_rule_and_not_to_the_bot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    let rule = s
        .call(
            "capture",
            json!({
                "subject": "bot:omega", "content": "keep the kettle descaled",
                "provenance": "testimony",
                "fields": {
                    "starred": "true", "subject": "the kettle", "purpose": "hard-line",
                    "recorded_by": "somebody at the bench", "colour": "teal",
                },
            }),
        )
        .await;
    let address = rule.json()["address"]
        .as_str()
        .expect("an address")
        .to_string();

    // ── the bot's own fields: the ordinary key folds, the marks do not ──────
    let bot = s
        .call("recall", json!({"subject": "bot:omega"}))
        .await
        .json();
    let fields = bot["objects"][0]["fields"]
        .as_object()
        .expect("a thing's fields");
    assert_eq!(fields.get("colour"), Some(&json!("teal")), "{bot}");
    for label in ["starred", "subject", "purpose", "recorded_by"] {
        assert!(!fields.contains_key(label), "the bot holds {label}: {bot}");
    }

    // ── the rule keeps every one, and a read of the record sees them ────────
    let record = s
        .call("recall", json!({"subject": "bot:omega", "facts": true}))
        .await
        .json();
    let held = &record["objects"][0]["facts"][0]["fields"];
    for label in ["starred", "subject", "purpose", "recorded_by"] {
        assert!(held.get(label).is_some(), "the rule lost {label}: {record}");
    }
    // A selection asked of the RECORD finds it by the mark; one asked of the
    // THING finds no bot that holds it.
    s.call(
        "recall",
        json!({"subject": "bot:omega", "facts": true,
               "fields": [{"key": "starred", "value": "true", "scope": "record"}]}),
    )
    .await
    .says(&address);
    let by_thing = s
        .call(
            "recall",
            json!({"kind": "bot", "fields": [{"key": "starred", "value": "true"}]}),
        )
        .await
        .json();
    assert_eq!(by_thing["count"], 0, "no bot holds starred: {by_thing}");

    s.wrap("wrote a rule with its marks").await;
    story.finish().await;
}

/// **Asking for a mark by `keys` is told where the mark lives.** The six marks
/// stay on their claim, so narrowing a thing's fields to one of them returns an
/// object with nothing under it, which reads as "the bot holds no such mark" and
/// is true and useless. The answer says that the mark is on the claim and which
/// read returns it, and it says so only when a mark was asked for.
#[tokio::test]
async fn asking_for_a_mark_by_keys_is_told_the_mark_is_on_the_claim() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.call(
        "capture",
        json!({
            "subject": "bot:omega", "content": "keep the kettle descaled",
            "provenance": "testimony",
            "fields": {"starred": "true", "one_liner": "keeps the kettle descaled"},
        }),
    )
    .await;

    let asked = s
        .call(
            "recall",
            json!({"subject": "bot:omega", "keys": ["starred", "one_liner"]}),
        )
        .await
        .json();
    // The ordinary key comes back and the mark does not: the positive that
    // the narrowing worked, beside the absence the note explains.
    let fields = asked["objects"][0]["fields"].as_object().expect("fields");
    assert_eq!(
        fields.get("one_liner"),
        Some(&json!("keeps the kettle descaled")),
        "{asked}"
    );
    assert!(!fields.contains_key("starred"), "{asked}");
    let note = asked["keys_note"].as_str().expect("the answer says why");
    for word in ["starred", "facts", "claim"] {
        assert!(note.contains(word), "the note names {word}: {note}");
    }
    // A note is a sentence somebody reads: one line, one space between words.
    assert!(
        !note.contains("  ") && !note.contains('\n'),
        "the note reads as one line: {note:?}"
    );
    assert!(
        !note.contains("one_liner"),
        "the note is about the mark and not the ordinary key: {note}"
    );

    // Nothing to explain when only ordinary keys were asked for.
    let plain = s
        .call(
            "recall",
            json!({"subject": "bot:omega", "keys": ["one_liner"]}),
        )
        .await
        .json();
    assert!(plain.get("keys_note").is_none(), "{plain}");

    // The read the note names returns the mark, on the claim.
    s.call("recall", json!({"subject": "bot:omega", "facts": true}))
        .await
        .says("\"starred\":\"true\"");

    s.wrap("asked for a mark by keys").await;
    story.finish().await;
}

/// **A check-in's source is the check-in's, and the loop still advances.**
#[tokio::test]
async fn a_check_ins_source_does_not_fold_onto_its_loop_and_the_loop_still_advances() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add_under("bot:otto", "rhythm:weekly-review", "Weekly review")
        .await;
    s.event_with(
        "rhythm:weekly-review",
        "look back over the week",
        json!({"cadence_days": "7", "advances_from": "due_date", "counts_from": "2026-06-25"}),
        &[],
    )
    .await;
    s.call(
        "capture",
        json!({
            "subject": "rhythm:weekly-review", "content": "did the review, an hour",
            "provenance": "testimony", "recorded_at": "2026-07-05", "check_in": "ran",
            "fields": {
                "read_from": "the kitchen ledger", "read_ref": "page four",
            },
        }),
    )
    .await;

    // ── the loop holds its schedule and not what the check-in read ──────────
    let loop_ = s
        .call("recall", json!({"subject": "rhythm:weekly-review"}))
        .await
        .json();
    let fields = loop_["objects"][0]["fields"].as_object().expect("fields");
    assert_eq!(fields.get("cadence_days"), Some(&json!("7")), "{loop_}");
    for label in ["read_from", "read_ref"] {
        assert!(
            !fields.contains_key(label),
            "the loop holds {label}: {loop_}"
        );
    }
    // The check-in record keeps its source.
    s.call(
        "recall",
        json!({"subject": "rhythm:weekly-review", "facts": true}),
    )
    .await
    .says("the kitchen ledger");

    // ── and the check-in still moved the loop ───────────────────────────────
    //
    // Ran on the fifth, falling due on the second of July: the next is the
    // ninth, so the eighth finds it quiet and the tenth finds it due.
    s.shape(
        "which loops are due by the eighth",
        json!({"kind": "rhythm", "overdue": {"as_of": "2026-07-08"}}),
    )
    .await
    .never_says("rhythm:weekly-review");
    s.shape(
        "which loops are due by the tenth",
        json!({"kind": "rhythm", "overdue": {"as_of": "2026-07-10"}}),
    )
    .await
    .says("rhythm:weekly-review");

    s.wrap("checked a loop in with a source").await;
    story.finish().await;
}

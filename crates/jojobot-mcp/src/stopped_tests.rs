//! **One decision of a stopped call's advice, held by cases that name each cause.**
//! The verbs' own cases (`resend_advice`, `request_ids_in_parts`) travel the
//! surface; these hold the decision where it is made.

use crate::answer::{FixBy, WayForward, stamp_error};
use crate::stopped::Stopped;
use jojobot_domain::mailbox::MailboxError;
use jojobot_domain::memory::MemoryError;
use jojobot_domain::session::SessionError;
use rmcp::ErrorData as McpError;

fn word_and_text(way: WayForward) -> (Option<FixBy>, String) {
    let mut body = serde_json::json!({});
    way.write_into(&mut body);
    (
        body[FixBy::KEY].as_str().and_then(FixBy::of_token),
        body["how_to_proceed"].as_str().unwrap_or_default().into(),
    )
}

/// **Each error type maps its word to a cause, and a refusal keeps its words.**
#[test]
fn a_store_failure_is_store_a_refusal_keeps_its_words_and_damage_is_unexplained() {
    assert!(matches!(
        Stopped::of_memory(&MemoryError::Conflict),
        Stopped::Store
    ));
    assert!(matches!(
        Stopped::of_session(&SessionError::Conflict),
        Stopped::Store
    ));
    assert!(matches!(
        Stopped::of_mailbox(&MailboxError::Conflict),
        Stopped::Store
    ));
    match Stopped::of_memory(&MemoryError::Refused("a rule broke".into())) {
        Stopped::Refused(why) => assert!(why.contains("a rule broke"), "{why}"),
        other => panic!("a refusal is not {other:?}"),
    }
    assert!(matches!(
        Stopped::of_mailbox(&MailboxError::Refused("a rule broke".into())),
        Stopped::Refused(_)
    ));
    assert!(matches!(
        Stopped::of_session(&SessionError::Refused("a rule broke".into())),
        Stopped::Refused(_)
    ));
    assert!(matches!(
        Stopped::of_memory(&MemoryError::StoredDamage { what: "x".into() }),
        Stopped::Unexplained
    ));
}

/// **An answer that is a refusal carries its word and words into the cause**, and
/// a protocol error does the same from its `data`.
#[test]
fn an_answer_and_a_protocol_error_give_the_cause_their_word_names() {
    let blocked = |word: Option<FixBy>| {
        let mut body = serde_json::json!({"status": "blocked", "how_to_proceed": "say it"});
        if let Some(word) = word {
            body[FixBy::KEY] = word.as_token().into();
        }
        crate::answer::json_result(&body).expect("an answer")
    };
    assert!(matches!(
        Stopped::of_answer(&blocked(Some(FixBy::Retry))),
        Stopped::Store
    ));
    assert!(matches!(
        Stopped::of_answer(&blocked(Some(FixBy::Change))),
        Stopped::Refused(_)
    ));
    // A blocked answer with no word is a refusal of the call.
    match Stopped::of_answer(&blocked(None)) {
        Stopped::Refused(why) => assert_eq!(why, "say it"),
        other => panic!("{other:?}"),
    }
    let error = |word| stamp_error(McpError::internal_error("it ended", None), Some(word));
    assert!(matches!(
        Stopped::of_error(&error(FixBy::Retry)),
        Stopped::Store
    ));
    assert!(matches!(
        Stopped::of_error(&error(FixBy::Person)),
        Stopped::Unexplained
    ));
    // An error with no word is not told as either.
    assert!(matches!(
        Stopped::of_error(&McpError::internal_error("it ended", None)),
        Stopped::Unexplained
    ));
}

/// **The advice for each cause is the word that cause earns**, and only a store
/// failure promises that a resend finishes the step.
#[test]
fn only_a_store_failure_is_told_that_a_resend_finishes_the_step() {
    let landed = "The rename was written, and the box was not moved.";
    let resend = "Send the same call again with the same request_id and it moves the box.";
    let (word, text) = word_and_text(Stopped::Store.unfinished(landed, resend));
    assert_eq!(word, Some(FixBy::Retry));
    assert!(text.contains(landed) && text.contains(resend), "{text}");

    let (word, text) =
        word_and_text(Stopped::Refused("the box is named twice".into()).unfinished(landed, resend));
    assert_eq!(word, Some(FixBy::Change));
    assert!(text.contains("the box is named twice"), "{text}");
    assert!(
        !text.contains(resend),
        "a refusal is not told to resend: {text}"
    );

    let (word, text) = word_and_text(Stopped::Unexplained.unfinished(landed, resend));
    assert_eq!(word, Some(FixBy::Person));
    assert!(!text.contains(resend), "{text}");
}

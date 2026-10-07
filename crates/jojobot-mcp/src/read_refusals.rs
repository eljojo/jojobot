//! **A refusal a read verb can earn does not say a write was skipped.**
//!
//! `recall`, `search`, `list_entities`, `list_sent` and `read_message` write
//! nothing, so "Nothing was written" tells their caller about work it never
//! asked for. The gates and builders these verbs share with the write verbs
//! open with what is true of every verb that reaches them: the call did not
//! run. Every refusal here is earned through the verb a caller uses.
//!
//! **Paired with a write verb's own refusal**, which still says it: a build
//! that rewrote every opening would pass the reads alone.

use crate::harness::*;
use crate::mailboxes::list_sent::ListSentArgs;
use crate::mailboxes::read_message::ReadMessageArgs;
use crate::memory::list_entities::ListEntitiesArgs;
use crate::memory::testing::*;
use crate::*;
use rmcp::handler::server::wrapper::Parameters;

fn advice(result: &rmcp::model::CallToolResult) -> String {
    blocked(result)["how_to_proceed"]
        .as_str()
        .expect("a refusal says what to do")
        .to_string()
}

#[tokio::test]
async fn a_refusal_a_read_verb_earns_does_not_say_a_write_was_skipped() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    let mut refusals: Vec<(&str, String)> = Vec::new();

    refusals.push((
        "recall, a subject nobody holds",
        advice(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(sid.clone()),
                    ..recall_args("person:milhouse")
                }))
                .await
                .expect("an answer"),
        ),
    ));
    refusals.push((
        "recall, a malformed sid",
        advice(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some("!!".into()),
                    ..recall_args("person:milhouse")
                }))
                .await
                .expect("an answer"),
        ),
    ));
    refusals.push((
        "recall, a sid nothing holds",
        advice(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some("zzzz".into()),
                    ..recall_args("person:milhouse")
                }))
                .await
                .expect("an answer"),
        ),
    ));
    refusals.push((
        "search, nothing to narrow by",
        advice(
            &jojobot
                .search(Parameters(SearchArgs {
                    sid: Some(sid.clone()),
                    ..search_args()
                }))
                .await
                .expect("an answer"),
        ),
    ));
    refusals.push((
        "list_entities, a parent nobody holds",
        advice(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    parent: Some("person:milhouse".into()),
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("an answer"),
        ),
    ));
    refusals.push((
        "list_sent, no sid",
        advice(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    sender: None,
                    to: None,
                    limit: None,
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("an answer"),
        ),
    ));
    refusals.push((
        "read_message, an id nobody holds",
        advice(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: "nope99".into(),
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("an answer"),
        ),
    ));
    refusals.push((
        "recall, an argument it does not implement",
        advice(
            &jojobot
                .unimplemented_arguments(
                    &rmcp::model::CallToolRequestParams::new("recall").with_arguments(
                        serde_json::json!({"subject": "person:milhouse", "flavour": "x"})
                            .as_object()
                            .expect("an object")
                            .clone(),
                    ),
                )
                .expect("the gate turns it back"),
        ),
    ));

    for (what, said) in &refusals {
        assert!(
            !said.contains("written"),
            "{what} says a write was skipped, and the verb writes nothing: {said}"
        );
    }

    // The positive: a verb that writes still says so where it is true.
    let capture = jojobot
        .capture(Parameters(CaptureArgs {
            sid: Some(sid),
            ..capture_args("person:milhouse", "a claim about nobody")
        }))
        .await
        .expect("an answer");
    assert!(
        advice(&capture).contains("written"),
        "a capture refused for an unknown subject says nothing was written"
    );
}

/// **The reader of a quarantined message is told what is true of a read.**
/// `read_message` takes delivery and writes nothing, and the refusal for a
/// message somebody quarantined is shared with the verb that quarantines.
#[tokio::test]
async fn a_quarantined_message_read_by_id_does_not_say_a_write_was_skipped() {
    use crate::mailboxes::testing::*;
    let jojobot = mailbox_handler();
    let sid = owning(&jojobot, "otto").await;
    let posted = send(&jojobot, "otto", "epsilon", "something to set aside").await;
    let id = posted["id"].as_str().expect("an id").to_string();
    jojobot
        .mark_processed(Parameters(
            crate::mailboxes::mark_processed::MarkProcessedArgs {
                message_id: id.clone(),
                notes: None,
                sid: Some(sid.clone()),
                quarantine: Some("set aside on purpose".into()),
            },
        ))
        .await
        .expect("the quarantine lands");

    let refused = jojobot
        .read_message(Parameters(ReadMessageArgs {
            message_id: id,
            sid: Some(sid),
        }))
        .await
        .expect("an answer");
    let said = advice(&refused);
    assert!(
        said.contains("set aside on purpose"),
        "it is the quarantine's own refusal that was reached: {said}"
    );
    assert!(!said.contains("written"), "{said}");
}

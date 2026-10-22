//! **An archive by defrag and the review mark it earns land in one act, or the
//! answer says the mark did not.** The mark is a named part of the archive's
//! request: with the store refusing it the archive still lands and the receipt
//! names the mark it could not make, and a resend under the same `request_id`
//! makes the mark once.

use super::testing::*;
use super::update_fact::UpdateFactArgs;
use super::*;
use crate::harness::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::request::testing::InMemoryRequests;
use rmcp::handler::server::wrapper::Parameters;
use std::sync::Arc;

/// A handler over `memory`, keeping the ids it is sent in `ledger`.
fn healthy(memory: Arc<InMemoryMemory>, ledger: &Arc<InMemoryRequests>) -> Jojobot {
    handler_over(memory).with_requests(ledger.clone())
}

/// **A handler whose store refuses every claim write from the second edit on**:
/// the archive is the first edit and lands, and the mark that follows is refused.
fn mark_refusing(memory: Arc<InMemoryMemory>, ledger: &Arc<InMemoryRequests>) -> Jojobot {
    Jojobot::new(
        Arc::new(FailsOnCapture {
            inner: memory,
            fails_from: 1,
            seen: Default::default(),
            edits_fail_from: 2,
            edits_seen: Default::default(),
        }),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    )
    .with_requests(ledger.clone())
}

/// A thread holding one claim, and that claim's address, in a store whose writes
/// are kept in `ledger`.
async fn a_thread_with_a_claim(
    memory: &Arc<InMemoryMemory>,
    ledger: &Arc<InMemoryRequests>,
) -> String {
    let writer = healthy(memory.clone(), ledger);
    make_bot(&writer, "defrag").await;
    ensure(&writer, "thread:alpha").await;
    let landed = capture_ok(
        &writer,
        capture_args("thread:alpha", "the bus leaves at eight"),
    )
    .await;
    address_of(&landed)
}

/// Defrag's archive of `address`, sent as the dispatch sends a call that carries
/// a `request_id`: the id is looked up, the verb runs inside the scope that lookup
/// made, and the answer is handed back to the ledger.
async fn defrag_archives(
    jojobot: &Jojobot,
    address: &str,
    request_id: Option<&str>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let sid = as_bot(jojobot, "defrag");
    let args = UpdateFactArgs {
        status: Some("archived".into()),
        details: Some("folded into the timetable".into()),
        sid: Some(sid.clone()),
        request_id: request_id.map(str::to_string),
        ..update_args(address)
    };
    let Some(id) = request_id else {
        return jojobot.update_fact(Parameters(args)).await;
    };
    let params = rmcp::model::CallToolRequestParams::new("update_fact").with_arguments(
        serde_json::json!({ "request_id": id })
            .as_object()
            .expect("an object")
            .clone(),
    );
    let crate::request::Begun::Fresh(scope, _flight) =
        jojobot.begin_request(&params, Some(&sid)).await
    else {
        panic!("the id is new or only partly written");
    };
    let ran =
        jojobot_domain::request::within(scope.clone(), jojobot.update_fact(Parameters(args))).await;
    if let Ok(answered) = &ran {
        let complete = rmcp::model::CallToolResponse::Complete(answered.clone());
        let _ = jojobot.end_request(&scope, &complete).await;
    }
    ran
}

/// The things defrag has not reviewed, by handle.
async fn the_queue(jojobot: &Jojobot) -> Vec<String> {
    let read = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                unreviewed: Some(true),
                ..recall_args("thread:alpha")
            }))
            .await
            .expect("recall ok"),
    );
    read["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect()
}

/// **An archive by defrag marks the thing reviewed, and the receipt says so.** The
/// thing leaves the queue. Paired with a refused mark in the next case.
#[tokio::test]
async fn an_archive_by_defrag_marks_the_thing_and_the_receipt_says_so() {
    let ledger = InMemoryRequests::new();
    let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let address = a_thread_with_a_claim(&memory, &ledger).await;
    let jojobot = healthy(memory, &ledger);
    assert!(
        the_queue(&jojobot)
            .await
            .contains(&"thread:alpha".to_string()),
        "the thing waits before the archive"
    );

    let answered = json_of(
        &defrag_archives(&jojobot, &address, None)
            .await
            .expect("the archive answers"),
    );
    assert_eq!(answered["reviewed"], true, "{answered}");
    assert!(answered.get("review_mark_not_made").is_none(), "{answered}");
    assert!(
        !the_queue(&jojobot)
            .await
            .contains(&"thread:alpha".to_string()),
        "the archive was a review"
    );
}

/// **With the mark refused, the archive lands and the receipt names the mark it
/// could not make.** The thing stays in the queue, which is the safe side, and the
/// receipt does not say reviewed.
#[tokio::test]
async fn a_refused_mark_leaves_the_archive_landed_and_the_receipt_names_it() {
    let ledger = InMemoryRequests::new();
    let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let address = a_thread_with_a_claim(&memory, &ledger).await;
    let refusing = mark_refusing(memory.clone(), &ledger);

    let answered = json_of(
        &defrag_archives(&refusing, &address, None)
            .await
            .expect("the archive answers"),
    );
    assert!(answered["review_mark_not_made"].is_string(), "{answered}");
    assert!(answered.get("reviewed").is_none(), "{answered}");

    let reading = healthy(memory, &ledger);
    assert!(
        the_queue(&reading)
            .await
            .contains(&"thread:alpha".to_string()),
        "the thing still waits"
    );
    let held = json_of(
        &reading
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                ..recall_args("thread:alpha")
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        held.to_string().contains("\"status\":\"archived\""),
        "the archive landed: {held}"
    );
}

/// **A call under a `request_id` whose mark was refused is not finished, and the
/// same call sent again makes the mark once.** The first answer says to send it
/// again with the same id and writes nothing more; the resend, with the store
/// back, marks the thing and does not archive the claim a second time.
#[tokio::test]
async fn a_resend_under_the_same_id_makes_the_mark_once() {
    let ledger = InMemoryRequests::new();
    let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let address = a_thread_with_a_claim(&memory, &ledger).await;
    let refusing = mark_refusing(memory.clone(), &ledger);

    let first = json_of(
        &defrag_archives(&refusing, &address, Some("archive-1"))
            .await
            .expect("the first call answers"),
    );
    assert_eq!(first["fix_by"], "retry", "{first}");
    assert!(
        first["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("same request_id")),
        "{first}"
    );

    let back = healthy(memory.clone(), &ledger);
    let resent = json_of(
        &defrag_archives(&back, &address, Some("archive-1"))
            .await
            .expect("the resend answers"),
    );
    assert_eq!(resent["reviewed"], true, "{resent}");
    assert!(!the_queue(&back).await.contains(&"thread:alpha".to_string()));

    // The mark exists once, and the claim was archived once.
    let facts = json_of(
        &back
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                review_marks: Some(true),
                ..recall_args("thread:alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let text = facts.to_string();
    assert_eq!(text.matches("review_mark").count(), 1, "{text}");
}

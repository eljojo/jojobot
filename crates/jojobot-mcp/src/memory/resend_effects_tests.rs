//! **A resend leaves the session the effects a clean call leaves.** A write that
//! ended what a thing owed fails to take the stored due moment off, answers with
//! the retry, and the same call sent again finishes it. What the write did for
//! the session before that point is done once, by the first call, because the
//! finishing path rebuilds the receipt from the claim and does not run it again.

use super::testing::*;
use super::update_fact::UpdateFactArgs;
use super::*;
use crate::harness::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::request::testing::InMemoryRequests;
use rmcp::handler::server::wrapper::Parameters;
use std::sync::Arc;

/// A handler whose store takes the claim write and refuses every edit from the
/// `edits_fail_from`th on,
/// so the stored due moment cannot be taken off.
fn clear_refusing(
    memory: Arc<InMemoryMemory>,
    ledger: &Arc<InMemoryRequests>,
    edits_fail_from: usize,
) -> Jojobot {
    Jojobot::new(
        Arc::new(FailsOnCapture {
            inner: memory,
            fails_from: usize::MAX,
            seen: Default::default(),
            edits_fail_from,
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

/// A call sent the way the dispatch sends one that carries a `request_id`.
async fn sent_under_an_id<F, Fut>(
    jojobot: &Jojobot,
    verb: &'static str,
    id: &str,
    call: F,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<rmcp::model::CallToolResult, rmcp::ErrorData>>,
{
    let params = rmcp::model::CallToolRequestParams::new(verb).with_arguments(
        serde_json::json!({ "request_id": id })
            .as_object()
            .expect("an object")
            .clone(),
    );
    let (scope, _flight) = match jojobot.begin_request(&params, Some(TEST_SID)).await {
        crate::request::Begun::Fresh(scope, flight) => (scope, flight),
        crate::request::Begun::Repeat(receipt) => return Ok(receipt),
        _ => panic!("the id is new, only partly written, or spent"),
    };
    let ran = jojobot_domain::request::within(scope.clone(), call()).await;
    if let Ok(answered) = &ran {
        let complete = rmcp::model::CallToolResponse::Complete(answered.clone());
        let _ = jojobot.end_request(&scope, &complete).await;
    }
    ran
}

/// What the bot holds under `working_on`, or none.
async fn working_on(jojobot: &Jojobot) -> Option<String> {
    fields_of(jojobot, "bot:otto").await["working_on"]
        .as_str()
        .map(str::to_string)
}

/// Owned work with a due moment, waiting its turn, and the address of its claim.
async fn owned_work_with_a_due_moment(jojobot: &Jojobot) -> String {
    make_bot(jojobot, "otto").await;
    ensure(jojobot, "work:phi").await;
    let made = capture_ok(
        jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            sets: Some(
                [
                    ("owner".to_string(), "bot:otto".to_string()),
                    ("status".to_string(), "next".to_string()),
                    ("decide_by".to_string(), "2026-12-01".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args("work:phi", "wire the shed")
        },
    )
    .await;
    assert_eq!(
        fields_of(jojobot, "work:phi").await["due_on"],
        "2026-12-01",
        "the moment the edit takes off has to be stored first"
    );
    assert_eq!(working_on(jojobot).await, None);
    address_of(&made)
}

/// **An edit that moves owned work to `now` and ends what it owed points the bot
/// at the work once the resend has finished it.**
#[tokio::test]
async fn an_edit_that_ended_what_was_owed_fills_working_on_when_the_resend_finishes_it() {
    let ledger = InMemoryRequests::new();
    let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let healthy = handler_over(memory.clone()).with_requests(ledger.clone());
    let address = owned_work_with_a_due_moment(&healthy).await;

    let edit = || UpdateFactArgs {
        request_id: Some("start-and-end".into()),
        sid: Some(TEST_SID.into()),
        sets: Some(
            [
                ("status".to_string(), "now".to_string()),
                ("ended".to_string(), "2026-10-01".to_string()),
            ]
            .into_iter()
            .collect(),
        ),
        ..update_args(&address)
    };
    let refusing = clear_refusing(memory.clone(), &ledger, 2);
    let cut = json_of(
        &sent_under_an_id(&refusing, "update_fact", "start-and-end", || {
            refusing.update_fact(Parameters(edit()))
        })
        .await
        .expect("the first call answers"),
    );
    assert_eq!(cut["fix_by"], "retry", "{cut}");

    let resent = json_of(
        &sent_under_an_id(&healthy, "update_fact", "start-and-end", || {
            healthy.update_fact(Parameters(edit()))
        })
        .await
        .expect("the resend answers"),
    );
    assert_eq!(resent["repeat"], true, "{resent}");
    assert!(
        fields_of(&healthy, "work:phi")
            .await
            .get("due_on")
            .is_none(),
        "the resend took the due moment off"
    );
    assert_eq!(working_on(&healthy).await.as_deref(), Some("work:phi"));
}

/// **A capture that moves owned work to `now` and ends what it owed points the
/// bot at the work once the resend has finished it.**
#[tokio::test]
async fn a_capture_that_ended_what_was_owed_fills_working_on_when_the_resend_finishes_it() {
    let ledger = InMemoryRequests::new();
    let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let healthy = handler_over(memory.clone()).with_requests(ledger.clone());
    owned_work_with_a_due_moment(&healthy).await;

    let capture = || CaptureArgs {
        request_id: Some("start-and-end".into()),
        sid: Some(TEST_SID.into()),
        provenance: Some("testimony".into()),
        sets: Some(
            [
                ("status".to_string(), "now".to_string()),
                ("ended".to_string(), "2026-10-01".to_string()),
            ]
            .into_iter()
            .collect(),
        ),
        ..capture_args("work:phi", "started, and nothing is owed")
    };
    let refusing = clear_refusing(memory.clone(), &ledger, 1);
    let cut = json_of(
        &sent_under_an_id(&refusing, "capture", "start-and-end", || {
            refusing.capture(Parameters(capture()))
        })
        .await
        .expect("the first call answers"),
    );
    assert_eq!(cut["fix_by"], "retry", "{cut}");

    let resent = json_of(
        &sent_under_an_id(&healthy, "capture", "start-and-end", || {
            healthy.capture(Parameters(capture()))
        })
        .await
        .expect("the resend answers"),
    );
    assert_eq!(resent["repeat"], true, "{resent}");
    assert!(
        fields_of(&healthy, "work:phi")
            .await
            .get("due_on")
            .is_none(),
        "the resend took the due moment off"
    );
    assert_eq!(working_on(&healthy).await.as_deref(), Some("work:phi"));
}

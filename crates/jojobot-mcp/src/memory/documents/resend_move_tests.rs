//! **A move that stopped partway ends in the state a clean move leaves, whether
//! or not the resend names its `request_id`.** The section's own place is the
//! first write and the rest of the plan follows it: the chunks that carry it on,
//! the leads of the neighbours the move joined, and what the edit ended.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::request::testing::InMemoryRequests;
use std::sync::Arc;

/// A handler whose store takes the first edit and refuses every one after it.
fn stops_after_the_first_edit(
    inner: &Arc<jojobot_domain::memory::testing::InMemoryMemory>,
) -> Jojobot {
    Jojobot::new(
        Arc::new(FailsOnCapture {
            inner: inner.clone(),
            fails_from: usize::MAX,
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
}

async fn put_text(jojobot: &Jojobot, text: &str) {
    ensure(jojobot, "document:alpha").await;
    let put = json_of(
        &jojobot
            .put_document(Parameters(PutDocumentArgs {
                request_id: None,
                document: "document:alpha".into(),
                markdown: Some(text.into()),
                restore_to: None,
                confirmed_by_user: None,
                after: None,
                provenance: None,
                source: None,
                source_ref: None,
                recorded_at: None,
                sid: Some(TEST_SID.into()),
            }))
            .await
            .expect("put_document answers"),
    );
    assert_ne!(put["status"], "blocked", "{put}");
}

/// An edit that must answer without a refusal.
async fn update_ok(jojobot: &Jojobot, args: UpdateFactArgs) {
    let body = json_of(
        &jojobot
            .update_fact(Parameters(args))
            .await
            .expect("update_fact answers"),
    );
    assert_ne!(body["status"], "blocked", "{body}");
}

/// The address of the record of the document whose words are `first_line`.
async fn address_of(jojobot: &Jojobot, first_line: &str) -> String {
    let mut offset = 0;
    loop {
        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    offset: Some(offset),
                    ..recall_args("document:alpha")
                }))
                .await
                .expect("recall answers"),
        );
        if let Some(fact) = body["objects"][0]["facts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|fact| fact["content"] == first_line)
        {
            return fact["address"].as_str().expect("an address").to_string();
        }
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next as u32,
            None => panic!("no section headed {first_line}"),
        }
    }
}

/// The whole document, read in as many parts as it takes.
async fn text_of(jojobot: &Jojobot) -> String {
    let mut text = String::new();
    let mut offset = None;
    loop {
        let part = json_of(
            &jojobot
                .read_document(Parameters(ReadDocumentArgs {
                    document: "document:alpha".into(),
                    offset,
                    as_of: None,
                    sid: Some(TEST_SID.into()),
                }))
                .await
                .expect("read_document answers"),
        );
        text.push_str(part["markdown"].as_str().expect("a part carries markdown"));
        match part["next_offset"].as_u64() {
            Some(next) => offset = Some(next as usize),
            None => return text,
        }
    }
}

/// **A forward move resent without an id writes the leads the first call
/// planned.** Sections written with no blank line between them: moving one past
/// the next joins two neighbours, and the plan mends the lead of the one that
/// follows. The store stops after the section's own place, so that mend is in
/// the part that did not land.
#[tokio::test]
async fn a_forward_move_resent_without_an_id_writes_the_leads_the_first_call_planned() {
    let text = "# Alpha\nfirst\n# Beta\nsecond\n# Gamma\nthird\n# Delta\nfourth\n";

    let clean = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::seeded());
    let clean_handler = handler_over(clean.clone());
    put_text(&clean_handler, text).await;
    let beta = address_of(&clean_handler, "# Beta").await;
    let gamma = address_of(&clean_handler, "# Gamma").await;
    clean_handler
        .update_fact(Parameters(UpdateFactArgs {
            after: Some(gamma),
            ..update_args(&beta)
        }))
        .await
        .expect("a clean move answers");
    let expected = text_of(&clean_handler).await;

    let inner = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::seeded());
    let healthy = handler_over(inner.clone());
    put_text(&healthy, text).await;
    let beta = address_of(&healthy, "# Beta").await;
    let gamma = address_of(&healthy, "# Gamma").await;
    let stopped = stops_after_the_first_edit(&inner)
        .update_fact(Parameters(UpdateFactArgs {
            after: Some(gamma.clone()),
            ..update_args(&beta)
        }))
        .await;
    assert!(stopped.is_err(), "the store stopped after the heading");

    healthy
        .update_fact(Parameters(UpdateFactArgs {
            after: Some(gamma),
            ..update_args(&beta)
        }))
        .await
        .expect("the resend answers");
    assert_eq!(text_of(&healthy).await, expected);
}

/// An edit sent the way the dispatch sends a call that carries a `request_id`:
/// the id is looked up, the verb runs inside the scope that lookup made, and the
/// answer is handed back to the ledger.
async fn sent_under_an_id(
    jojobot: &Jojobot,
    args: UpdateFactArgs,
) -> Result<CallToolResult, McpError> {
    let id = args.request_id.clone().expect("the call carries an id");
    let params = rmcp::model::CallToolRequestParams::new("update_fact").with_arguments(
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
    let ran =
        jojobot_domain::request::within(scope.clone(), jojobot.update_fact(Parameters(args))).await;
    if let Ok(answered) = &ran {
        let complete = rmcp::model::CallToolResponse::Complete(answered.clone());
        let _ = jojobot.end_request(&scope, &complete).await;
    }
    ran
}

/// **A move that also ended what the document owed takes the stored due moment
/// off when the resend finishes it.** The edit sets `ended` beside the document's
/// `runs_out`, held by another section; the store stops after the section's own
/// place, so the clear that follows the move did not run.
#[tokio::test]
async fn a_move_that_ended_what_was_owed_and_stopped_takes_the_due_moment_off_on_the_resend() {
    let text = "# Alpha\nfirst\n# Beta\nsecond\n# Gamma\nthird\n# Delta\nfourth\n";
    let ledger = InMemoryRequests::new();
    let inner = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let healthy = handler_over(inner.clone()).with_requests(ledger.clone());
    put_text(&healthy, text).await;
    let beta = address_of(&healthy, "# Beta").await;
    let gamma = address_of(&healthy, "# Gamma").await;
    let alpha = address_of(&healthy, "# Alpha").await;
    update_ok(
        &healthy,
        UpdateFactArgs {
            sets: Some(
                [("runs_out".to_string(), "2026-12-01".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..update_args(&alpha)
        },
    )
    .await;
    assert_eq!(
        fields_of(&healthy, "document:alpha").await["due_on"],
        "2026-12-01",
        "the moment this case takes off has to be stored first"
    );

    let ending = || UpdateFactArgs {
        request_id: Some("end-and-move".into()),
        after: Some(gamma.clone()),
        sets: Some(
            [("ended".to_string(), "2026-10-01".to_string())]
                .into_iter()
                .collect(),
        ),
        sid: Some(TEST_SID.into()),
        ..update_args(&beta)
    };
    let stopped = stops_after_the_first_edit(&inner).with_requests(ledger.clone());
    let cut = json_of(
        &sent_under_an_id(&stopped, ending())
            .await
            .expect("the first call answers"),
    );
    assert_eq!(cut["fix_by"], "retry", "{cut}");
    assert_eq!(
        fields_of(&healthy, "document:alpha").await["due_on"],
        "2026-12-01",
        "the stopped call took it off"
    );

    let resent = json_of(
        &sent_under_an_id(&healthy, ending())
            .await
            .expect("the resend answers"),
    );
    assert_eq!(resent["repeat"], true, "{resent}");
    let after = fields_of(&healthy, "document:alpha").await;
    assert_eq!(after["ended"], "2026-10-01", "the edit landed: {after}");
    assert!(
        after.get("due_on").is_none(),
        "the resend finished the move and left the due moment: {after}"
    );
}

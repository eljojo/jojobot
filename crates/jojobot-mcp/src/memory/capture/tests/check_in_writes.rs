use super::*;
use jojobot_domain::request::testing::InMemoryRequests;
use jojobot_domain::request::{RequestId, Requests};
use std::sync::Arc;

/// The same request boundary the dispatch uses: its ledger remains open until
/// both records land, and a spent request answers with its stored receipt.
async fn capture_under_id(jojobot: &Jojobot, mut args: CaptureArgs) -> serde_json::Value {
    let sid = writing_as(jojobot);
    args.sid = Some(sid.clone());
    let params = rmcp::model::CallToolRequestParams::new("capture").with_arguments(
        serde_json::json!({
            "request_id": args.request_id,
            "subject": args.subject,
            "content": args.content,
            "check_in": args.check_in,
            "recorded_at": args.recorded_at,
        })
        .as_object()
        .expect("arguments")
        .clone(),
    );
    let (scope, _flight) = match jojobot.begin_request(&params, Some(&sid)).await {
        crate::request::Begun::Fresh(scope, flight) => (scope, flight),
        crate::request::Begun::Repeat(receipt) => return json_of(&receipt),
        _ => panic!("the request can run or repeat"),
    };
    let answer = jojobot_domain::request::within(scope.clone(), jojobot.capture(Parameters(args)))
        .await
        .expect("capture answers");
    let complete = rmcp::model::CallToolResponse::Complete(answer.clone());
    let _ = jojobot.end_request(&scope, &complete).await;
    json_of(&answer)
}

#[tokio::test]
async fn a_check_in_resumes_its_saved_schedule_without_a_second_source_claim() {
    let ledger = InMemoryRequests::new();
    let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let jojobot = handler_over(memory.clone()).with_requests(ledger.clone());
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "due_date").await;
    ledger.fail_the_next_commit_of("check_in_schedule");
    let args = CaptureArgs {
        request_id: Some("check-in-1".into()),
        check_in: Some("ran".into()),
        provenance: Some("testimony".into()),
        recorded_at: Some("2026-08-10".into()),
        ..capture_args("rhythm:descale", "the turn was completed")
    };
    let first = capture_under_id(&jojobot, args.clone()).await;
    assert!(first["address"].is_string(), "{first}");
    assert!(first["schedule_not_written"].is_string(), "{first}");
    assert_eq!(first["fix_by"], "retry", "{first}");
    assert_ne!(first["wrote"], false, "{first}");
    assert!(ledger.is_empty(), "the whole id is not spent");
    assert_eq!(ledger.parts_held(), 1, "only the source landed");
    let parts = ledger
        .parts(
            &EntityId::person("bot:otto"),
            &RequestId::parse("check-in-1").unwrap(),
        )
        .await
        .unwrap();
    assert!(
        parts[0]
            .recorded
            .note
            .as_deref()
            .unwrap()
            .starts_with("check_in_schedule:"),
        "the stored plan's spelling survives a build change"
    );

    // A new basis between attempts must not change the plan already committed
    // beside the source claim: a due-date check-in had advanced August 1 by 7.
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some([("counts_from".to_string(), "2026-09-01".to_string())].into()),
            ..capture_args("rhythm:descale", "the basis changed between attempts")
        },
    )
    .await;
    let resumed = capture_under_id(&jojobot, args.clone()).await;
    assert_eq!(resumed["address"], first["address"], "{resumed}");
    assert!(resumed["schedule_not_written"].is_null(), "{resumed}");
    assert_eq!(
        resumed["parts_already_written"],
        serde_json::json!(["claim"])
    );
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger.parts_held(), 2);
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["counts_from"], "2026-08-08", "{held}");
    assert_eq!(held["due_on"], "2026-08-15", "{held}");

    let repeated = capture_under_id(&jojobot, args).await;
    assert_eq!(repeated["address"], first["address"], "{repeated}");
    let facts = memory
        .recall(&EntityId::person("rhythm:descale"))
        .await
        .unwrap();
    let source = facts
        .iter()
        .find(|fact| fact.address().to_string() == first["address"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        facts
            .iter()
            .filter(|fact| fact.content == source.content)
            .count(),
        1
    );
    assert_eq!(
        facts
            .iter()
            .filter(|fact| fact.derived_from == Some(source.address()))
            .count(),
        1
    );
}

#[tokio::test]
async fn a_check_in_without_an_id_reports_a_landed_source_when_the_schedule_fails() {
    let memory = Arc::new(InMemoryMemory::seeded());
    let healthy = handler_over(memory.clone());
    a_weekly_rhythm(&healthy, "descale", "2026-08-01", "check_in_date").await;
    let refusing = Jojobot::new(
        Arc::new(FailsOnCapture {
            inner: memory.clone(),
            fails_from: 2,
            seen: Default::default(),
            edits_fail_from: usize::MAX,
            edits_seen: Default::default(),
        }),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let body = json_of(
        &refusing
            .capture(Parameters(CaptureArgs {
                sid: Some(writing_as(&refusing)),
                check_in: Some("ran".into()),
                provenance: Some("observation".into()),
                fields: Some([("read_from".to_string(), "the kiln log".to_string())].into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "the turn was completed")
            }))
            .await
            .expect("the partial write answers"),
    );
    assert!(body["address"].is_string(), "{body}");
    assert_eq!(body["provenance"], "observation", "{body}");
    assert!(body["schedule_not_written"].is_string(), "{body}");
    assert_ne!(body["status"], "blocked", "{body}");
    assert_ne!(body["wrote"], false, "{body}");
    assert_eq!(body["fix_by"], "retry", "{body}");
    let held = fields_of(&healthy, "rhythm:descale").await;
    assert_eq!(held["counts_from"], "2026-08-01", "{held}");
    assert!(held["outcome"].is_null(), "{held}");
    let facts = memory
        .recall(&EntityId::person("rhythm:descale"))
        .await
        .unwrap();
    assert_eq!(facts.len(), 2);
    let source = facts
        .iter()
        .find(|fact| fact.content == "the turn was completed")
        .unwrap();
    assert_eq!(source.provenance, Provenance::Observation);
    assert_eq!(source.fields["read_from"], "the kiln log");
}

#[tokio::test]
async fn an_observed_check_in_without_read_from_refuses_before_either_record() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    let body = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                provenance: Some("observation".into()),
                sid: Some(writing_as(&jojobot)),
                ..capture_args("rhythm:descale", "the turn was completed")
            }))
            .await
            .expect("the refusal answers"),
    );
    assert_eq!(body["status"], "blocked", "{body}");
    assert_eq!(body["wrote"], false, "{body}");
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("rhythm:descale")))
            .await
            .unwrap(),
    );
    assert_eq!(read["objects"][0]["facts"].as_array().unwrap().len(), 1);
    assert_eq!(read["objects"][0]["fields"]["counts_from"], "2026-08-01");
}

#[tokio::test]
async fn a_check_in_schedule_takes_no_extra_slot_but_the_next_caller_claim_is_refused() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some([("record_capacity".to_string(), "3".to_string())].into()),
            ..capture_args("rhythm:descale", "three caller claims fit")
        },
    )
    .await;
    let check_in = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            provenance: Some("testimony".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "the turn was completed")
        },
    )
    .await;
    assert!(check_in["schedule_not_written"].is_null(), "{check_in}");
    assert_eq!(
        fields_of(&jojobot, "rhythm:descale").await["counts_from"],
        "2026-08-10"
    );
    let refused = json_of(
        &jojobot
            .capture(Parameters(capture_args(
                "rhythm:descale",
                "one claim too many",
            )))
            .await
            .unwrap(),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
}

async fn a_schedule_write_keeps_its_cause(cause: Cause, word: &str) {
    let ledger = InMemoryRequests::new();
    let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
    let healthy = handler_over(memory.clone()).with_requests(ledger.clone());
    a_weekly_rhythm(&healthy, "descale", "2026-08-01", "check_in_date").await;
    let refusing = Jojobot::new(
        Arc::new(DownMemory(
            Down::WritesAfter(1, Default::default(), cause),
            memory,
        )),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    )
    .with_requests(ledger.clone());
    let body = capture_under_id(
        &refusing,
        CaptureArgs {
            request_id: Some("check-in-1".into()),
            check_in: Some("ran".into()),
            ..capture_args("rhythm:descale", "the turn was completed")
        },
    )
    .await;
    assert!(body["address"].is_string(), "{body}");
    assert!(body["schedule_not_written"].is_string(), "{body}");
    assert_eq!(body["fix_by"], word, "{body}");
    assert_ne!(body["wrote"], false, "{body}");
    assert!(ledger.is_empty());
    assert_eq!(ledger.parts_held(), 1);
}

#[tokio::test]
async fn a_schedule_write_outage_still_wears_retry_after_the_source_lands() {
    a_schedule_write_keeps_its_cause(Cause::Outage, "retry").await;
}

#[tokio::test]
async fn a_schedule_write_with_stored_damage_wears_person_after_the_source_lands() {
    a_schedule_write_keeps_its_cause(Cause::Damage("the schedule row is damaged"), "person").await;
}

#[tokio::test]
async fn a_schedule_write_refusal_wears_change_after_the_source_lands() {
    a_schedule_write_keeps_its_cause(Cause::Refusal("the schedule is refused"), "change").await;
}

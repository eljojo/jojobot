//! **A move behind a section whose own move stopped partway does not panic.** A
//! forward move writes the section's place first and its chunks after, so a
//! store that stops between the two leaves the chunks reading before the section
//! they carry on. A later move that names that section as its target reads the
//! run in the order it reads now, and the head of the run is the record with no
//! head of its own, wherever it sorts.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;
use std::sync::Arc;

/// The address of the record of `document` whose words are `first_line`, read in
/// as many parts as `recall` needs.
async fn address_of(jojobot: &Jojobot, document: &str, first_line: &str) -> String {
    let mut offset = 0;
    loop {
        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    offset: Some(offset),
                    ..recall_args(document)
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

fn moving(address: &str, after: &str) -> UpdateFactArgs {
    UpdateFactArgs {
        after: Some(after.into()),
        ..update_args(address)
    }
}

#[tokio::test]
async fn a_move_behind_a_section_whose_move_stopped_after_its_heading_does_not_panic() {
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    let text = format!("# Small\n\nsmall\n\n{big}# Last\n\nend\n");
    let inner = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::seeded());
    let healthy = handler_over(inner.clone());
    ensure(&healthy, "document:alpha").await;
    let put = json_of(
        &healthy
            .put_document(Parameters(PutDocumentArgs {
                request_id: None,
                document: "document:alpha".into(),
                markdown: Some(text),
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
    let small = address_of(&healthy, "document:alpha", "# Small").await;
    let big_head = address_of(&healthy, "document:alpha", "# Big").await;
    let last = address_of(&healthy, "document:alpha", "# Last").await;

    // The section's own place lands, and the store stops before its chunks.
    let failing = Jojobot::new(
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
    );
    let stopped = failing
        .update_fact(Parameters(moving(&big_head, &last)))
        .await
        .expect_err("a store that stopped is an error");
    let data = stopped.data.clone().expect("the error carries data");
    assert_eq!(data["moved"].as_array().map(Vec::len), Some(1), "{data}");

    // A move behind the section whose chunks still read before it.
    let answered = tokio::spawn({
        let healthy = healthy.clone();
        async move {
            healthy
                .update_fact(Parameters(moving(&small, &big_head)))
                .await
        }
    })
    .await
    .expect("the move answers rather than panicking")
    .expect("update_fact answers");
    let body = json_of(&answered);
    assert_ne!(body["status"], "blocked", "{body}");
    let mut text = String::new();
    let mut offset = None;
    loop {
        let part = json_of(
            &healthy
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
            None => break,
        }
    }
    let at = |heading: &str| text.find(heading).unwrap_or_else(|| panic!("{heading}"));
    assert!(
        at("# Big\n") < at("# Small\n"),
        "the moved section reads behind the target's heading"
    );
    let (before_small, small_body) = text.split_once("# Small\n").unwrap();
    assert!(
        before_small.ends_with("\n\n") && !before_small.ends_with("\n\n\n"),
        "the move leaves one blank line before the moved heading"
    );
    assert_eq!(small_body, "\nsmall\n\n", "the moved body and tail survive");
    assert!(
        text.contains("end\n\n# Big\n\n"),
        "the interrupted target still has a blank seam after its predecessor"
    );
    assert_eq!(
        text.matches(paragraph.trim_end()).count(),
        60,
        "the interrupted target keeps every paragraph, including its chunks"
    );
}

/// One call through the served router, which is where a `request_id` is begun,
/// answered as the JSON body of its first content block.
async fn served(server: &Jojobot, verb: &str, mut args: serde_json::Value) -> serde_json::Value {
    use rmcp::{RoleServer, model::*, service::serve_directly, transport::OneshotTransport};
    args["sid"] = TEST_SID.into();
    let request = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": verb, "arguments": args}
    }))
    .expect("a tool request");
    let (transport, mut replies) = OneshotTransport::<RoleServer>::new(request);
    let running = serve_directly(server.clone(), transport, Some(ClientInfo::default()));
    let reply = replies.recv().await.expect("the router answers");
    let reply = serde_json::to_value(reply).expect("a JSON reply");
    running.cancel().await.expect("the call ends");
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("an answer with a body: {reply}"));
    serde_json::from_str(text).expect("a JSON body")
}

/// **A forward move under a `request_id` that stops after the section's own
/// place says so.** The answer is the refusal shape, `blocked` with `wrote:
/// false`, and its way forward counts the parts that landed and tells the caller
/// to send the same call again with the same id. The boot text says the same of
/// every multi-step call, so a caller that reads `wrote: false` is not left to
/// guess whether the section moved.
#[tokio::test]
async fn a_move_under_an_id_that_stops_partway_names_the_parts_that_landed() {
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    let text = format!("# Small\n\nsmall\n\n{big}# Last\n\nend\n");
    let ledger = jojobot_domain::request::testing::InMemoryRequests::new();
    let inner = Arc::new(
        jojobot_domain::memory::testing::InMemoryMemory::seeded().with_requests(ledger.clone()),
    );
    let healthy = handler_over(inner.clone());
    ensure(&healthy, "document:alpha").await;
    let put = json_of(
        &healthy
            .put_document(Parameters(PutDocumentArgs {
                request_id: None,
                document: "document:alpha".into(),
                markdown: Some(text),
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
    let big_head = address_of(&healthy, "document:alpha", "# Big").await;
    let last = address_of(&healthy, "document:alpha", "# Last").await;

    let failing = Jojobot::new(
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
    .with_requests(ledger);
    let stopped = served(
        &failing,
        "update_fact",
        serde_json::json!({
            "address": big_head,
            "after": last,
            "request_id": "move-big-1",
        }),
    )
    .await;
    assert_eq!(stopped["status"], "blocked", "{stopped}");
    assert_eq!(stopped["wrote"], false, "{stopped}");
    assert_eq!(stopped["refusal_code"], "HandleDeclined", "{stopped}");
    let way = stopped["how_to_proceed"].to_string();
    assert!(
        way.contains("1 of "),
        "the landed parts are counted: {stopped}"
    );
    assert!(way.contains("request_id"), "{stopped}");
}

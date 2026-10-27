//! Mail is readable down the chart; only its owner takes delivery.

use super::*;
use crate::harness::*;
use crate::mailboxes::testing::*;
use crate::memory::search::SearchArgs;
use crate::memory::testing::{capture_args, capture_as, search_args};
use jojobot_adapters::search::{FullTextIndex, IndexedMailboxes, Refresh, Retrieval};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::session::testing::InMemorySessions;
use std::collections::BTreeSet;

async fn chart() -> (Jojobot, [String; 4]) {
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let indexed = Arc::new(IndexedMailboxes::new(boxes, index.clone()));
    let retrieval = Arc::new(Retrieval::new(
        index,
        vec![indexed.clone() as Arc<dyn Refresh>],
    ));
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        retrieval,
        indexed,
        Arc::new(InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let homer = owning(&jojobot, "homer").await;
    let bart = owning(&jojobot, "bart").await;
    let lisa = owning(&jojobot, "lisa").await;
    let marge = owning(&jojobot, "marge").await;
    for (bot, manager, sid) in [("bart", "homer", &homer), ("lisa", "bart", &bart)] {
        capture_as(
            &jojobot,
            sid,
            crate::memory::capture::CaptureArgs {
                sets: Some([("reports_to".into(), format!("bot:{manager}"))].into()),
                ..capture_args(&format!("bot:{bot}"), "who manages this bot")
            },
        )
        .await;
    }
    (jojobot, [homer, bart, lisa, marge])
}

async fn find(jojobot: &Jojobot, sid: Option<&str>, offset: u32, limit: u32) -> serde_json::Value {
    json_of(
        &jojobot
            .search(Parameters(SearchArgs {
                query: Some("chartneedle".into()),
                include_mail: Some(true),
                sid: sid.map(str::to_string),
                offset: Some(offset),
                limit: Some(limit),
                ..search_args()
            }))
            .await
            .expect("search answers"),
    )
}

fn found_boxes(answer: &serde_json::Value) -> BTreeSet<String> {
    answer["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter(|hit| hit["hit"] == "message")
        .map(|hit| hit["mailbox"].as_str().expect("mailbox").to_string())
        .collect()
}

#[tokio::test]
async fn mail_visible_search_follows_the_whole_chart_before_paging() {
    let (jojobot, sids) = chart().await;
    for name in ["homer", "bart", "lisa", "marge"] {
        send(&jojobot, name, "otto", "chartneedle").await;
    }
    a_persons_box(&jojobot, "milhouse").await;
    send(&jojobot, "person:milhouse", "otto", "chartneedle").await;
    for (sid, expected) in sids.iter().zip([
        vec!["homer", "bart", "lisa"],
        vec!["bart", "lisa"],
        vec!["lisa"],
        vec!["marge"],
    ]) {
        let expected: BTreeSet<String> = expected.into_iter().map(str::to_string).collect();
        let answer = find(&jojobot, Some(sid), 0, 20).await;
        assert_eq!(found_boxes(&answer), expected, "{answer}");
        assert_eq!(answer["total"], expected.len(), "{answer}");
        let mut paged = BTreeSet::new();
        for offset in 0..expected.len() as u32 {
            let page = find(&jojobot, Some(sid), offset, 1).await;
            assert_eq!(page["results"].as_array().unwrap().len(), 1, "{page}");
            paged.extend(found_boxes(&page));
        }
        assert_eq!(paged, expected);
        assert!(
            find(&jojobot, Some(sid), expected.len() as u32, 1).await["results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert!(found_boxes(&find(&jojobot, None, 0, 20).await).is_empty());
}

async fn open(jojobot: &Jojobot, sid: &str, id: &str, history: bool) -> serde_json::Value {
    json_of(
        &jojobot
            .read_message(Parameters(ReadMessageArgs {
                message_id: id.into(),
                sid: Some(sid.into()),
                history: Some(history),
            }))
            .await
            .expect("read answers"),
    )
}

#[tokio::test]
async fn mail_visible_by_id_down_the_chart_takes_nothing_and_refuses_above_and_beside() {
    let (jojobot, [homer, bart, lisa, marge]) = chart().await;
    for recipient in ["bart", "lisa"] {
        let receipt = send(&jojobot, recipient, "otto", "chartneedle").await;
        let id = receipt["id"].as_str().unwrap();
        let key = MessageId(id.into());
        let before = jojobot
            .mailboxes
            .message_by_id(&key)
            .await
            .unwrap()
            .unwrap();
        let events = jojobot.mailboxes.message_history(&key).await.unwrap();
        let peek = open(&jojobot, &homer, id, false).await;
        assert_eq!(peek["body"], "chartneedle", "{peek}");
        assert_eq!(peek["state"], "new", "{peek}");
        assert_eq!(peek["taken"], false, "{peek}");
        assert_eq!(open(&jojobot, &homer, id, true).await["taken"], false);
        assert_eq!(
            jojobot
                .mailboxes
                .message_by_id(&key)
                .await
                .unwrap()
                .unwrap(),
            before
        );
        assert_eq!(
            jojobot.mailboxes.message_history(&key).await.unwrap(),
            events
        );
        assert_eq!(counts(&jojobot, recipient).await["counts"]["new"], 1);
        assert_eq!(counts(&jojobot, "homer").await["counts"]["total"], 0);
        let owner = if recipient == "bart" { &bart } else { &lisa };
        let delivered = open(&jojobot, owner, id, false).await;
        assert_eq!(delivered["state"], "read");
        assert_eq!(delivered["seen_before"], false);
        assert_eq!(open(&jojobot, &homer, id, false).await["state"], "read");
        jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                request_id: None,
                message_id: id.into(),
                notes: None,
                sid: Some(owner.clone()),
                quarantine: None,
                message_ids: None,
            }))
            .await
            .unwrap();
        assert_eq!(
            open(&jojobot, &homer, id, false).await["state"],
            "processed"
        );
        for history in [false, true] {
            let beside = open(&jojobot, &marge, id, history).await;
            assert_eq!(beside["status"], "blocked", "{beside}");
            assert_eq!(beside["wrote"], false);
        }
    }
    let receipt = send(&jojobot, "homer", "otto", "chartneedle").await;
    let id = receipt["id"].as_str().unwrap();
    for history in [false, true] {
        let above = open(&jojobot, &bart, id, history).await;
        assert_eq!(above["status"], "blocked", "{above}");
        assert!(
            above["how_to_proceed"].as_str().unwrap().contains("above"),
            "{above}"
        );
        assert_eq!(above["wrote"], false);
    }
}

#[test]
fn mail_visible_search_description_names_the_chart_rule() {
    let tools = Jojobot::tool_router().list_all();
    let description = tools
        .iter()
        .find(|tool| tool.name == "search")
        .unwrap()
        .description
        .as_deref()
        .unwrap();
    for term in ["reports_to", "below", "own"] {
        assert!(description.contains(term), "missing {term}: {description}");
    }
}

#[tokio::test]
async fn mail_visible_chart_changes_apply_to_search_and_read_by_id_on_the_next_call() {
    let (jojobot, [homer, _, _, marge]) = chart().await;
    let note = send(&jojobot, "lisa", "otto", "chartneedle").await;
    let id = note["id"].as_str().unwrap();
    assert_eq!(
        found_boxes(&find(&jojobot, Some(&homer), 0, 20).await),
        BTreeSet::from(["lisa".into()])
    );
    assert_eq!(open(&jojobot, &homer, id, false).await["state"], "new");
    capture_as(
        &jojobot,
        &homer,
        crate::memory::capture::CaptureArgs {
            sets: Some([("reports_to".into(), "bot:marge".into())].into()),
            ..capture_args("bot:bart", "Bart now reports to Marge")
        },
    )
    .await;
    assert!(found_boxes(&find(&jojobot, Some(&homer), 0, 20).await).is_empty());
    assert_eq!(open(&jojobot, &homer, id, false).await["status"], "blocked");
    assert_eq!(
        found_boxes(&find(&jojobot, Some(&marge), 0, 20).await),
        BTreeSet::from(["lisa".into()])
    );
    assert_eq!(open(&jojobot, &marge, id, false).await["state"], "new");
}

async fn sent(jojobot: &Jojobot, sid: &str, sender: &str, bodies: bool) -> serde_json::Value {
    json_of(
        &jojobot
            .list_sent(Parameters(ListSentArgs {
                sender: Some(sender.into()),
                to: None,
                limit: None,
                offset: None,
                include_bodies: Some(bodies),
                sid: Some(sid.into()),
            }))
            .await
            .expect("listing answers"),
    )
}

#[tokio::test]
async fn mail_visible_other_sender_lists_withhold_a_peers_message_and_its_digest() {
    let (jojobot, [_, _, _, marge]) = chart().await;
    send(&jojobot, "lisa", "homer", "chartneedle").await;
    for bodies in [false, true] {
        let hidden = sent(&jojobot, &marge, "bot:homer", bodies).await;
        assert_eq!(hidden["messages"], serde_json::json!([]), "{hidden}");
        assert_eq!(hidden["withheld"], 1, "{hidden}");
        assert_eq!(hidden["sent_total"], 0, "{hidden}");
        assert!(!hidden.to_string().contains("chartneedle"), "{hidden}");
        assert!(!hidden.to_string().contains("lisa"), "{hidden}");
        assert!(
            hidden["how_to_read"].is_null(),
            "no body pointer can open withheld mail: {hidden}"
        );
    }
    let anonymous = json_of(
        &jojobot
            .list_sent(Parameters(ListSentArgs {
                sender: Some("bot:homer".into()),
                to: None,
                limit: None,
                offset: None,
                include_bodies: Some(true),
                sid: None,
            }))
            .await
            .unwrap(),
    );
    assert_eq!(anonymous["messages"], serde_json::json!([]), "{anonymous}");
    assert_eq!(anonymous["withheld"], 1, "{anonymous}");
    let visible = send(&jojobot, "marge", "homer", "a visible report").await;
    let listed = sent(&jojobot, &marge, "bot:homer", true).await;
    assert_eq!(listed["messages"].as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed["messages"][0]["id"], visible["id"], "{listed}");
    assert_eq!(listed["withheld"], 1, "{listed}");
    assert_eq!(listed["sent_total"], 1, "{listed}");
}

#[tokio::test]
async fn mail_visible_own_outbox_is_readable_above_and_beside_in_all_three_reads() {
    let (jojobot, [_, bart, _, marge]) = chart().await;
    for recipient in ["homer", "marge"] {
        let note = send(&jojobot, recipient, "bart", "chartneedle").await;
        let peek = open(&jojobot, &bart, note["id"].as_str().unwrap(), false).await;
        assert_eq!(peek["body"], "chartneedle", "{peek}");
        assert_eq!(peek["state"], "new", "{peek}");
        assert_eq!(peek["taken"], false, "{peek}");
        assert_eq!(counts(&jojobot, recipient).await["counts"]["new"], 1);
    }
    assert_eq!(
        found_boxes(&find(&jojobot, Some(&bart), 0, 20).await),
        BTreeSet::from(["homer".into(), "marge".into()])
    );
    let listed = sent(&jojobot, &bart, "bot:bart", true).await;
    assert_eq!(listed["messages"].as_array().unwrap().len(), 2, "{listed}");
    assert_eq!(listed["withheld"], 0, "{listed}");
    a_persons_box(&jojobot, "milhouse").await;
    let private = send(&jojobot, "person:milhouse", "bart", "chartneedle").await;
    assert_eq!(
        open(&jojobot, &bart, private["id"].as_str().unwrap(), false).await["status"],
        "blocked"
    );
    let listed = sent(&jojobot, &bart, "bot:bart", true).await;
    let receipt = listed["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["id"] == private["id"])
        .unwrap();
    assert!(receipt.get("body").is_none(), "{receipt}");
    assert!(receipt.get("body_head").is_none(), "{receipt}");
    assert_eq!(listed["withheld"], 0, "{listed}");
    let other_sender = sent(&jojobot, &marge, "bot:bart", true).await;
    assert_eq!(
        other_sender["messages"].as_array().unwrap().len(),
        2,
        "{other_sender}"
    );
    assert_eq!(
        other_sender["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["mailbox"] == "marge")
            .count(),
        1,
        "{other_sender}"
    );
    assert_eq!(other_sender["withheld"], 1, "{other_sender}");
    assert!(
        !other_sender.to_string().contains("person-milhouse"),
        "{other_sender}"
    );
    assert_eq!(
        found_boxes(&find(&jojobot, Some(&bart), 0, 20).await).len(),
        2
    );
}

//! **A page whose prose could not be asked about is named, never counted as
//! having none (rule 130).**
//!
//! A recall that leaves prose out says how many pages it left out. A store that
//! cannot say whether a page holds prose must not read as a page with none: the
//! answer names the pages it could not ask about, beside the count of the ones
//! it could.

use std::sync::Arc;

use jojobot_adapters::counted::Counted;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Memory};
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::teaching::testing::InMemoryTeachings;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};

use crate::Jojobot;
use crate::harness::{TEST_SID, json_of, seeded_registry};
use crate::memory::recall::RecallArgs;
use crate::memory::testing::{SpySearch, add_args};

/// Two people whose pages hold prose, over a store a case can stop answering
/// for one page.
async fn two_pages() -> (Jojobot, Arc<Counted>) {
    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let jojobot = Jojobot::new(
        counted.clone(),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    );
    for (handle, name) in [("homer", "Homer"), ("lisa", "Lisa")] {
        jojobot
            .add_entity(Parameters(add_args("person", handle, name)))
            .await
            .expect("add_entity ok");
        counted
            .set_prose(
                &EntityId::person(format!("person:{handle}")),
                "A page with words on it.",
                None,
            )
            .await
            .expect("the page is set");
    }
    (jojobot, counted)
}

async fn recall_documents(jojobot: &Jojobot) -> Value {
    let args: RecallArgs = serde_json::from_value(json!({
        "kind": "person",
        "facts": false,
        "sid": TEST_SID,
    }))
    .expect("recall args");
    json_of(&jojobot.recall(Parameters(args)).await.expect("recall ok"))
}

/// **Both pages read: both are counted and none is named unread.** The positive
/// the failing case rests on.
#[tokio::test]
async fn pages_that_read_are_counted_and_none_is_named_unread() {
    let (jojobot, _) = two_pages().await;
    let body = recall_documents(&jojobot).await;
    assert_eq!(body["prose_left_out"]["count"], 2, "{body}");
    assert!(body.get("prose_unread").is_none(), "{body}");
}

/// **A page the store cannot answer for is named; the page it can answer for is
/// still counted.**
#[tokio::test]
async fn a_page_whose_prose_cannot_be_asked_about_is_named_not_counted_as_none() {
    let (jojobot, counted) = two_pages().await;
    counted.fail_prose_of(Some("person:lisa"));
    let body = recall_documents(&jojobot).await;
    assert_eq!(body["prose_left_out"]["count"], 1, "{body}");
    assert_eq!(
        body["prose_unread"]["handles"],
        json!(["person:lisa"]),
        "{body}"
    );
    for object in body["objects"].as_array().expect("objects") {
        assert!(object.get("_prose_unread").is_none(), "{object}");
    }

    counted.fail_prose_of(None);
    let healed = recall_documents(&jojobot).await;
    assert_eq!(healed["prose_left_out"]["count"], 2, "{healed}");
    assert!(healed.get("prose_unread").is_none(), "{healed}");
}

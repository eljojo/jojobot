//! **A write over an unreadable instance record stores the due day the readable
//! path would, or none and says so** (rule 130, rule 598). The record maps the
//! operator's status words, and a word it maps to `done` finishes a work item.
//! The blind handler and the healthy one share a store, so a work item each of
//! them creates is judged by the same record, one of them unable to read it.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// An instance that maps `shipped` to `done`, written through the healthy handler.
async fn an_instance_mapping_shipped_to_done(healthy: &Jojobot) {
    let _ = writing_as(healthy);
    healthy
        .add_entity(Parameters(add_args("topic", "instance", "Instance")))
        .await
        .expect("add ok");
    capture_ok(
        healthy,
        CaptureArgs {
            sets: Some(std::collections::BTreeMap::from([(
                format!("{}shipped", jojobot_domain::memory::kinds::STATUS_WORD),
                "done".to_string(),
            )])),
            provenance: Some("testimony".into()),
            ..capture_args("topic:instance", "maps shipped to done")
        },
    )
    .await;
}

/// A piece of work made with a decision day and a status, through `jojobot`.
async fn made_with_status(jojobot: &Jojobot, slug: &str, status: &str) -> serde_json::Value {
    let made = jojobot
        .add_entity(Parameters(AddEntityArgs {
            sets: Some(std::collections::BTreeMap::from([
                ("decide_by".to_string(), "2026-11-01".to_string()),
                ("status".to_string(), status.to_string()),
            ])),
            sid: Some(writing_as(jojobot)),
            ..add_args("work", slug, slug)
        }))
        .await
        .expect("add_entity call ok");
    let body = json_of(&made);
    assert_ne!(body["status"], "blocked", "{slug}: {body}");
    body
}

/// **A word the unread record may map to done stores no day, and the receipt
/// says so; a shipped word, which the record cannot change, stores the day the
/// healthy path stores.** On the healthy handler `shipped` means done, so the
/// work item owes nothing and holds no due day; the blind handler cannot know,
/// and used to store the decision day on a work item the operator's words had
/// finished. A work item at the shipped word `now` is paired: both handlers
/// store its day, and the blind receipt says nothing about the record.
#[tokio::test]
async fn a_word_the_unread_record_may_map_stores_no_due_day_and_a_shipped_word_stores_it() {
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("topic:instance"));
    an_instance_mapping_shipped_to_done(&healthy).await;

    made_with_status(&healthy, "tau", "shipped").await;
    let blind_receipt = made_with_status(&blind, "zeta", "shipped").await;
    assert_eq!(
        fields_of(&healthy, "work:tau").await.get("due_on"),
        None,
        "a work item the operator's word finished holds no day"
    );
    assert_eq!(
        fields_of(&healthy, "work:zeta").await.get("due_on"),
        None,
        "the blind write stores the day the healthy one does: none"
    );
    assert!(
        blind_receipt["instance_unread"]["not_worked_out"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key == "due_on")),
        "and says it did not work the day out: {blind_receipt}"
    );

    made_with_status(&healthy, "phi", "now").await;
    let blind_now = made_with_status(&blind, "epsilon", "now").await;
    for slug in ["phi", "epsilon"] {
        assert_eq!(
            fields_of(&healthy, &format!("work:{slug}")).await["due_on"],
            "2026-11-01",
            "a shipped word is judged the same blind or healthy"
        );
    }
    assert!(
        blind_now.get("instance_unread").is_none(),
        "nothing was left unworked: {blind_now}"
    );
}

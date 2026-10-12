//! "I recorded my answer on a view the software ships, and asking for it by its
//! key found nothing."
//!
//! **A key set on a record the build supplies is found by that key exactly as
//! one set on a stored record is.** The shipped view and the operator's own view
//! hold the same key here, and one question, by the key and no kind, finds both.
//! The pair is the point: a lookup that found only the stored one would pass any
//! case written about the stored one.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The handles an answer lists, sorted.
fn handles(answer: &Value) -> Vec<String> {
    let mut found: Vec<String> = answer["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists no objects: {answer}"))
        .iter()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect();
    found.sort();
    found
}

async fn asked_by_key(s: &Session, key: &str, kind: Option<&str>) -> Vec<String> {
    let mut args = json!({"fields": [{"key": key}], "keys": [key]});
    if let Some(kind) = kind {
        args["kind"] = json!(kind);
    }
    handles(&s.call("recall", args).await.json())
}

#[tokio::test]
async fn a_key_set_on_a_shipped_view_is_found_by_key_like_one_set_on_a_stored_view() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("view:my-bikes", "My Bikes").await;
    for subject in ["view:loops", "view:my-bikes"] {
        s.call(
            "capture",
            json!({"subject": subject, "content": "an answer",
                   "provenance": "testimony", "sets": {"answer_9": "kept here"}}),
        )
        .await;
    }

    let both = vec!["view:loops".to_string(), "view:my-bikes".to_string()];
    assert_eq!(
        asked_by_key(&s, "answer_9", None).await,
        both,
        "by key alone"
    );
    assert_eq!(
        asked_by_key(&s, "answer_9", Some("view")).await,
        both,
        "by key and the kind"
    );
    // The positive the absence rests on: a key nobody set matches nothing.
    assert!(asked_by_key(&s, "answer_8", None).await.is_empty());

    story.finish().await;
}

/// "I tried to archive a view the software ships, and the refusal talked about a
/// row." A session knows records, not rows: the refusal for an archive of a
/// record the build supplies says there is no stored record under it to move.
/// Paired with the archive of a view the operator made, which lands.
#[tokio::test]
async fn a_refused_archive_of_a_supplied_record_speaks_of_records_not_rows() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("view:my-bikes", "My Bikes").await;

    let refused = s
        .refused(
            "archive_entity",
            json!({"handle": "view:loops", "reason": "tidying"}),
        )
        .await;
    refused.says("record the build supplies");
    assert!(
        !refused.raw().contains(" row"),
        "the refusal speaks of a row: {}",
        refused.raw()
    );

    // The pair: a view of the operator's own is archived, so the refusal above
    // is about the supplied record and not about every view.
    s.call(
        "archive_entity",
        json!({"handle": "view:my-bikes", "reason": "tidying"}),
    )
    .await;

    story.finish().await;
}

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
    let (_story, s) = Story::begin_with_session("bot:otto").await;
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
}

/// An archive of a record the build supplies is refused with nothing written
/// and a change for the caller to make. Paired with the archive of a view the
/// operator made, which lands.
#[tokio::test]
async fn an_archive_of_a_supplied_record_is_refused_and_one_of_the_operators_own_lands() {
    let (_story, s) = Story::begin_with_session("bot:otto").await;
    s.add("view:my-bikes", "My Bikes").await;

    let refused = s
        .refused(
            "archive_entity",
            json!({"handle": "view:loops", "reason": "tidying"}),
        )
        .await;
    let body = refused.json();
    assert_eq!(body["wrote"], false, "nothing was written: {body}");
    assert_eq!(body["fix_by"], "change", "the call must change: {body}");

    // The pair: a view of the operator's own is archived, so the refusal above
    // is about the supplied record and not about every view.
    s.call(
        "archive_entity",
        json!({"handle": "view:my-bikes", "reason": "tidying"}),
    )
    .await;
}

/// **A sets-only edit of a record the build supplies lands, through the served
/// surface**: the answer is not blocked, it carries the address of the claim it
/// wrote, and the key reads back on the shipped view beside what the build
/// ships. Paired with the same edit of a view of the operator's own, which
/// lands: a refusal of the shipped one would be about the record the build
/// supplies and not about the verb.
#[tokio::test]
async fn a_sets_only_edit_of_a_shipped_view_lands_through_the_surface_like_one_of_a_stored_view() {
    let (_story, s) = Story::begin_with_session("bot:otto").await;
    s.add("view:my-bikes", "My Bikes").await;
    // **The premise of the story, read through the surface**: the listing names the
    // source of each row, and the store's row wins where both exist, so a shipped
    // view the store holds no row for reads as the build's own, beside the stored
    // view of the operator's, which reads as theirs.
    let listed = s
        .call("list_entities", json!({"kind": "view"}))
        .await
        .json();
    let source_of = |handle: &str| {
        listed["entities"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"] == handle))
            .map(|row| row["source"].clone())
            .unwrap_or_else(|| panic!("{handle} is not listed: {listed}"))
    };
    assert_eq!(source_of("view:my-bikes"), "user-named", "{listed}");
    assert_eq!(
        source_of("view:loops"),
        "jojobot",
        "the served store holds a row for the shipped view, so the story does not reach \
         the overlay branch: {listed}"
    );
    for handle in ["view:my-bikes", "view:loops"] {
        let landed = s
            .call(
                "update_entity",
                json!({"handle": handle, "sets": {"note_9": "kept on top"}}),
            )
            .await
            .json();
        assert_ne!(landed["status"], "blocked", "{handle}: {landed}");
        // The served store holds no row for the shipped view, so its edit lands
        // through the overlay branch; both answer with the address of the claim.
        assert!(landed["address"].is_string(), "{handle}: {landed}");
    }
    let read = s
        .call("recall", json!({"subject": "view:loops"}))
        .await
        .json();
    assert_eq!(
        read["objects"][0]["fields"]["note_9"], "kept on top",
        "{read}"
    );
    assert!(
        read["objects"][0]["fields"]["selects"].is_string(),
        "what the build ships is still there under the edit: {read}"
    );
}

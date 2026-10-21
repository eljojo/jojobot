//! "Mark these three reviewed, and if one cannot be, mark none."
//!
//! A list of handles marks each thing reviewed. The list is judged whole before
//! the first thing is marked: a handle that cannot be marked (a voucher is
//! issued, never edited) refuses the call and names it, and the things before it
//! are not marked. A list that all pass marks all; a list cut by a store failure
//! is finished by sending it again under its `request_id`, which the argument
//! says.
//!
//! The roster names are the cast: three people and a voucher.

use serde_json::{Value, json};

use super::chain_of_trust::issue_through_the_store;
use super::dsl::{Session, Story};

/// The handles defrag's queue holds now.
async fn queue(defrag: &Session) -> Vec<String> {
    defrag
        .call("recall", json!({"unreviewed": true}))
        .await
        .json()["objects"]
        .as_array()
        .map(|objects| {
            objects
                .iter()
                .filter_map(|o| o["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// **A list with a handle that cannot be marked marks none of the list.** The
/// voucher is second, so a mark that checks each handle as it goes would mark the
/// first. The paired positive is a list of three that all pass, which marks all
/// three.
#[tokio::test]
async fn a_list_with_a_refused_handle_marks_none_and_a_list_that_passes_marks_all() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add("person:bart", "Bart").await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:patty", "Patty").await;
    issue_through_the_store(
        &story,
        &jojobot_domain::memory::EntityId("voucher:boot-as-patty".into()),
    )
    .await;
    let defrag = story.as_bot("bot:defrag").await;

    let refused = defrag
        .refused(
            "update_entity",
            json!({"handle": "person:homer,voucher:boot-as-patty,person:bart",
                   "reviewed": true}),
        )
        .await;
    refused.says("voucher:boot-as-patty");
    let waiting = queue(&defrag).await;
    for handle in ["person:homer", "person:bart", "person:lisa"] {
        assert!(
            waiting.contains(&handle.to_string()),
            "{handle} was marked by a list that was refused: {waiting:?}"
        );
    }

    // ── the positive: a list that passes marks every thing in it ────────────
    let marked: Value = defrag
        .call(
            "update_entity",
            json!({"handle": "person:homer,person:bart,person:lisa", "reviewed": true}),
        )
        .await
        .json();
    assert_eq!(marked["reviewed"]["count"], 3, "{marked}");
    let waiting = queue(&defrag).await;
    for handle in ["person:homer", "person:bart", "person:lisa"] {
        assert!(
            !waiting.contains(&handle.to_string()),
            "{handle} is still waiting after its list was marked: {waiting:?}"
        );
    }
    story.finish().await;
}

/// **The `reviewed` argument names the route for a list a store failure cut**:
/// the same call again under the same `request_id` marks the rest and none twice.
#[tokio::test]
async fn the_reviewed_argument_names_the_route_for_a_cut_list() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let described = s.argument_description("update_entity", "reviewed").await;
    assert!(
        described.contains("request_id"),
        "the reviewed argument does not name the resend route: {described}"
    );
    story.finish().await;
}

/// **A list cut partway is finished by sending it again under its `request_id`**,
/// and no thing is marked twice. The second mark fails to commit once; the first
/// stands, and the resend marks the rest and says how many it found marked.
#[tokio::test]
async fn a_cut_list_is_finished_by_the_same_call_under_its_request_id() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add("person:bart", "Bart").await;
    s.add("person:lisa", "Lisa").await;
    let defrag = story.as_bot("bot:defrag").await;
    let call = json!({"handle": "person:homer,person:bart,person:lisa",
                      "reviewed": true, "request_id": "mark-three"});

    story.fails_the_next_commit_of("mark-1");
    defrag.refused("update_entity", call.clone()).await;
    let waiting = queue(&defrag).await;
    assert!(
        !waiting.contains(&"person:homer".to_string()),
        "the first mark stands: {waiting:?}"
    );
    assert!(waiting.contains(&"person:bart".to_string()), "{waiting:?}");

    let finished: Value = defrag.call("update_entity", call).await.json();
    assert_eq!(
        finished["finished_a_cut_list"]["already_marked"], 1,
        "{finished}"
    );
    assert_eq!(
        finished["finished_a_cut_list"]["marked_now"], 2,
        "{finished}"
    );
    let waiting = queue(&defrag).await;
    for handle in ["person:homer", "person:bart", "person:lisa"] {
        assert!(
            !waiting.contains(&handle.to_string()),
            "{handle}: {waiting:?}"
        );
    }
    story.finish().await;
}

//! "I read the whole camping box together; let me mark it read in one go."
//!
//! **Defrag marks several things reviewed in one call**, by listing their
//! handles in `handle`, comma-joined. Each thing gets its own review mark, as a
//! single mark gives it, so the queue keeps working thing by thing. **And it reads
//! its queue one container at a time**: `unreviewed` beside `parent` lists what
//! waits under that container and nothing under another.
//!
//! The roster names are the cast: Homer, Bart and Lisa, and the Springfield boxes.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// The handles an answer lists, in order.
fn listed(answer: &Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

/// What defrag's queue holds now.
async fn queue(defrag: &Session) -> Vec<String> {
    listed(&defrag.call("recall", json!({"unreviewed": true})).await)
}

/// How many live review marks a thing holds.
async fn marks_of(reader: &Session, handle: &str) -> usize {
    let answer = reader
        .call(
            "recall",
            json!({"subject": handle, "facts": true, "review_marks": true}),
        )
        .await
        .json();
    answer["objects"][0]["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .filter(|fact| fact["fields"]["review_mark"].is_string())
        .count()
}

/// Homer, Bart and Lisa, each written to by another bot so each waits.
async fn three_waiting(dev: &Session) -> [&'static str; 3] {
    let people = ["person:homer", "person:bart", "person:lisa"];
    for (handle, name) in people.iter().zip(["Homer", "Bart", "Lisa"]) {
        dev.add(handle, name).await;
    }
    people
}

/// **Defrag marks three things reviewed in one call, and the receipt names how
/// many and which.** Each leaves the queue and holds one mark of its own, and a
/// later write by another bot puts back only the thing it wrote to. A second
/// bulk mark rewrites each thing's one mark rather than adding one.
#[tokio::test]
async fn defrag_marks_several_things_reviewed_in_one_call() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    let people = three_waiting(&dev).await;
    let defrag = story.as_bot("bot:defrag").await;
    let waiting = queue(&defrag).await;
    for handle in people {
        assert!(waiting.contains(&handle.to_string()), "{waiting:?}");
    }

    let marked = defrag
        .call(
            "update_entity",
            json!({"handle": people.join(","), "reviewed": true}),
        )
        .await;
    let receipt = marked.json();
    assert_eq!(receipt["reviewed"]["count"], 3, "{receipt}");
    assert_eq!(
        receipt["reviewed"]["handles"],
        json!(people),
        "the receipt names each thing marked, in the order sent: {receipt}"
    );
    let after = queue(&defrag).await;
    for handle in people {
        assert!(!after.contains(&handle.to_string()), "{after:?}");
        assert_eq!(marks_of(&defrag, handle).await, 1, "{handle}");
    }

    dev.call(
        "capture",
        json!({"subject": "person:bart", "content": "skateboards", "provenance": "testimony"}),
    )
    .await;
    let back = queue(&defrag).await;
    assert!(back.contains(&"person:bart".to_string()), "{back:?}");
    assert!(!back.contains(&"person:homer".to_string()), "{back:?}");
    assert!(!back.contains(&"person:lisa".to_string()), "{back:?}");

    defrag
        .call(
            "update_entity",
            json!({"handle": people.join(","), "reviewed": true}),
        )
        .await;
    for handle in people {
        assert_eq!(
            marks_of(&defrag, handle).await,
            1,
            "one mark each: {handle}"
        );
    }
    assert!(!queue(&defrag).await.contains(&"person:bart".to_string()));
    story.finish().await;
}

/// **Another bot's bulk mark is refused and marks nothing**, as a single mark
/// is. The paired positive is defrag's own bulk mark, above.
#[tokio::test]
async fn a_bulk_mark_by_another_bot_is_refused_and_marks_nothing() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    let people = three_waiting(&dev).await;
    let defrag = story.as_bot("bot:defrag").await;

    dev.refused(
        "update_entity",
        json!({"handle": people.join(","), "reviewed": true}),
    )
    .await
    .holds_at("/fix_by", "change")
    .says("\"wrote\":false");
    let waiting = queue(&defrag).await;
    for handle in people {
        assert!(waiting.contains(&handle.to_string()), "{waiting:?}");
        assert_eq!(marks_of(&defrag, handle).await, 0, "{handle}");
    }
    story.finish().await;
}

/// **A bulk mark is whole or not at all**: a handle in the list that names
/// nothing refuses the call, naming it, and no thing in the list is marked. A
/// list sent with anything but `reviewed` is refused too, because the other
/// edits are made to one thing.
#[tokio::test]
async fn a_bulk_mark_with_a_missing_handle_or_another_edit_marks_nothing() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    let people = three_waiting(&dev).await;
    let defrag = story.as_bot("bot:defrag").await;

    defrag
        .refused(
            "update_entity",
            json!({"handle": "person:homer,person:nelson,person:lisa", "reviewed": true}),
        )
        .await
        .says("person:nelson")
        .says("\"wrote\":false");
    defrag
        .refused(
            "update_entity",
            json!({"handle": "person:homer,person:lisa", "reviewed": true, "name": "Homer"}),
        )
        .await
        .says("\"wrote\":false");
    let waiting = queue(&defrag).await;
    for handle in people {
        assert!(waiting.contains(&handle.to_string()), "{waiting:?}");
        assert_eq!(marks_of(&defrag, handle).await, 0, "{handle}");
    }
    story.finish().await;
}

/// **The queue reads one container at a time**: `unreviewed` beside `parent`
/// lists what waits directly under that container, and nothing that waits under
/// another. The paired control is the whole queue, which holds both boxes'
/// things.
#[tokio::test]
async fn the_queue_beside_a_parent_lists_only_that_containers_things() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("thing:springfield-camping", "Camping box").await;
    dev.add("thing:springfield-hall", "Hall box").await;
    for (handle, parent) in [
        ("gamma", "thing:springfield-camping"),
        ("delta", "thing:springfield-camping"),
        ("sigma", "thing:springfield-hall"),
        ("tau", "thing:springfield-hall"),
    ] {
        dev.call(
            "add_entity",
            json!({"kind": "thing", "handle": handle, "name": handle, "source": "user-named",
                   "parent": parent}),
        )
        .await;
    }
    let defrag = story.as_bot("bot:defrag").await;
    let whole = queue(&defrag).await;
    for handle in ["thing:gamma", "thing:delta", "thing:sigma", "thing:tau"] {
        assert!(whole.contains(&handle.to_string()), "{whole:?}");
    }

    let camping = listed(
        &defrag
            .call(
                "recall",
                json!({"unreviewed": true, "parent": "thing:springfield-camping"}),
            )
            .await,
    );
    let mut sorted = camping.clone();
    sorted.sort();
    assert_eq!(sorted, ["thing:delta", "thing:gamma"], "{camping:?}");

    defrag
        .call(
            "update_entity",
            json!({"handle": camping.join(","), "reviewed": true}),
        )
        .await;
    let rest = listed(
        &defrag
            .call(
                "recall",
                json!({"unreviewed": true, "parent": "thing:springfield-camping"}),
            )
            .await,
    );
    assert!(rest.is_empty(), "{rest:?}");
    let hall = listed(
        &defrag
            .call(
                "recall",
                json!({"unreviewed": true, "parent": "thing:springfield-hall"}),
            )
            .await,
    );
    let mut sorted = hall.clone();
    sorted.sort();
    assert_eq!(sorted, ["thing:sigma", "thing:tau"], "{hall:?}");
    story.finish().await;
}

/// **A bulk mark sent again under its `request_id` is a repeat**: the second
/// answer is the first receipt marked `repeat`, and each thing still holds one
/// mark. The paired positive is the first call, which is no repeat.
#[tokio::test]
async fn a_bulk_mark_sent_again_under_its_request_id_is_a_repeat() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    let people = three_waiting(&dev).await;
    let defrag = story.as_bot("bot:defrag").await;
    let call = json!({"handle": people.join(","), "reviewed": true,
                      "request_id": "defrag-springfield-sweep"});

    let first = defrag.call("update_entity", call.clone()).await;
    assert!(first.json().get("repeat").is_none(), "{}", first.raw());
    assert_eq!(first.json()["reviewed"]["count"], 3, "{}", first.raw());
    let again = defrag.call("update_entity", call).await;
    again.says("repeat");
    assert_eq!(again.json()["reviewed"]["count"], 3, "{}", again.raw());
    for handle in people {
        assert_eq!(marks_of(&defrag, handle).await, 1, "{handle}");
    }
    story.finish().await;
}

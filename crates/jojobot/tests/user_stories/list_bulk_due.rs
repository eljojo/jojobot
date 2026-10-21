//! **A batch on a list that owes something**: a list that carries `runs_out` falls
//! due, and a batch whose `sets` end it takes the stored due moment off the list.
//! The batch does that once, after its last item. A step that fails there leaves
//! every item written and says so, and the same call under the same `request_id`
//! finishes it without writing any item twice.
//!
//! The cast is the packing lists of `list_bulk`.

use serde_json::{Value, json};

use super::dsl::{Session, Story};
use super::lists::a_list_on_the_trip;

/// The words of every standing claim on a list but its own first one, in order.
/// An ended list reads no `entries`, so the claims are read as they are written.
async fn read_states(s: &Session, list: &str) -> Vec<String> {
    let read = s
        .call("recall", json!({"subject": list, "facts": true}))
        .await
        .json();
    read["objects"][0]["facts"]
        .as_array()
        .map(|facts| {
            facts
                .iter()
                .filter(|fact| CHORES.contains(&fact["content"].as_str().unwrap_or("")))
                .map(|fact| format!("{}:open", fact["content"].as_str().unwrap_or("?")))
                .collect()
        })
        .unwrap_or_default()
}

/// The stored due moment of a list, when it holds one.
async fn due_on(s: &Session, list: &str) -> Option<String> {
    let read = s
        .call("recall", json!({"subject": list, "keys": ["due_on"]}))
        .await
        .json();
    read["objects"][0]["fields"]["due_on"]
        .as_str()
        .map(str::to_string)
}

const CHORES: [&str; 3] = ["mow the lawn", "wash the car", "fix the kettle"];

fn ending(list: &str, id: &str) -> Value {
    json!({"subject": list, "provenance": "testimony", "request_id": id,
           "entries": CHORES, "sets": {"ended": "kept"}})
}

/// **A batch that ends what a list owes, whose last step fails, says every item
/// landed.** The step that takes the stored due moment off fails once. The three
/// chores are written, and the answer names all three with `wrote` true and asks
/// for the same call again, which finishes the step and writes no chore twice.
/// The pair is the same batch on another list with nothing failing.
#[tokio::test]
async fn a_batch_whose_ending_step_fails_names_every_item_landed_and_a_resend_finishes_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(
        &s,
        "homer-chores",
        json!({"tick": "sink", "runs_out": "2099-12-01"}),
    )
    .await;
    assert_eq!(
        due_on(&s, "list:homer-chores").await.as_deref(),
        Some("2099-12-01")
    );

    story.fails_the_next_commit_of("clear_due");
    let stopped = s
        .refused("capture", ending("list:homer-chores", "r1"))
        .await
        .json();
    assert_eq!(stopped["wrote"], true, "{stopped}");
    assert_eq!(stopped["fix_by"], "retry", "{stopped}");
    let landed: Vec<&str> = stopped["landed"]
        .as_array()
        .expect("the items that landed")
        .iter()
        .map(|item| item["item"].as_str().expect("the item's words"))
        .collect();
    assert_eq!(landed, CHORES, "every item is named as landed: {stopped}");
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        [
            "mow the lawn:open",
            "wash the car:open",
            "fix the kettle:open"
        ]
    );
    assert_eq!(
        due_on(&s, "list:homer-chores").await.as_deref(),
        Some("2099-12-01"),
        "the step that failed has not run"
    );

    let finished = s
        .call("capture", ending("list:homer-chores", "r1"))
        .await
        .json();
    assert_eq!(
        finished["finished_a_cut_batch"]["already_written"], 3,
        "{finished}"
    );
    assert_eq!(finished["finished_a_cut_batch"]["written_now"], 0);
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        [
            "mow the lawn:open",
            "wash the car:open",
            "fix the kettle:open"
        ],
        "each chore once"
    );
    assert_eq!(
        due_on(&s, "list:homer-chores").await,
        None,
        "the resend took the stored due moment off"
    );

    // The pair: nothing fails, and the batch ends what is owed once.
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "marge-errands", "name": "Errands",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "sink", "runs_out": "2099-12-01"}}),
    )
    .await;
    let clean = s
        .call("capture", ending("list:marge-errands", "r2"))
        .await
        .json();
    assert!(clean.get("finished_a_cut_batch").is_none(), "{clean}");
    assert_eq!(read_states(&s, "list:marge-errands").await.len(), 3);
    assert_eq!(due_on(&s, "list:marge-errands").await, None);
    story.finish().await;
}

/// **A batch that ends what a list owes and is cut at its third item is finished
/// by the same call, each item once.** The third item's write fails in the store.
/// The resend skips the two that landed, writes the third, and takes the stored
/// due moment off, and the server answers it (it used to stop on the resend).
#[tokio::test]
async fn a_cut_batch_that_ends_what_a_list_owes_is_finished_by_the_same_call_each_item_once() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(
        &s,
        "homer-chores",
        json!({"tick": "sink", "runs_out": "2099-12-01"}),
    )
    .await;

    story.fails_the_next_commit_of("item-2");
    let stopped = s
        .refused("capture", ending("list:homer-chores", "r1"))
        .await
        .json();
    assert_eq!(stopped["wrote"], true, "{stopped}");
    assert_eq!(stopped["landed"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["mow the lawn:open", "wash the car:open"]
    );

    let finished = s
        .call("capture", ending("list:homer-chores", "r1"))
        .await
        .json();
    assert_eq!(
        finished["finished_a_cut_batch"]["already_written"], 2,
        "{finished}"
    );
    assert_eq!(finished["finished_a_cut_batch"]["written_now"], 1);
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        [
            "mow the lawn:open",
            "wash the car:open",
            "fix the kettle:open"
        ],
        "each chore once"
    );
    assert_eq!(due_on(&s, "list:homer-chores").await, None);
    story.finish().await;
}

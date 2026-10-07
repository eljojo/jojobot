//! "Make this piece of work, and say where it stands."
//!
//! Creating a thing and giving it its first fields used to take two calls, and
//! a plan of thirty-five items cost about seventy-five. `add_entity` takes
//! `sets`, the keys to set on the new thing, and writes them as its first claim
//! in the same act.
//!
//! **Creation is one act, so it is whole or it is not there.** A field the kind
//! refuses, or a link to a handle nobody holds, refuses the whole call and
//! creates nothing: there is never a half-made thing for a later session to find
//! and wonder about. Each refusal is paired with the same call that lands, so a
//! build that refused every creation would not pass.

use serde_json::json;

use super::dsl::Story;

/// **One call makes the thing and says where it stands.**
#[tokio::test]
async fn one_call_makes_a_piece_of_work_and_says_where_it_stands() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;
    s.add("person:lisa", "Lisa").await;

    let made = s
        .call(
            "add_entity",
            json!({
                "kind": "work", "handle": "phi", "name": "Phi", "source": "user-named",
                "parent": "project:atlas",
                "sets": {"status": "next", "owner": "person:lisa"},
            }),
        )
        .await
        .json();
    assert_eq!(made["id"], "work:phi", "{made}");
    // The first claim has an address a later edit goes through.
    assert_eq!(made["first_claim"]["address"], "work:phi#f1", "{made}");

    // What the one call wrote is what a read of the key finds, with no second
    // call in between.
    let phi = s.recall("work:phi").await;
    phi.says("\"status\":\"next\"").says("person:lisa");
    let next = s
        .call(
            "recall",
            json!({"kind": "work", "fields": [{"key": "status", "value": "next"}]}),
        )
        .await;
    next.says("work:phi");
    // The index knows the new thing and what it was said with.
    s.find("Phi").await.says("work:phi");
    story.finish().await;
}

/// **A field the kind refuses leaves nothing behind.** The same call with a
/// word the kind accepts then lands, which it could not if the refused one had
/// left the handle taken.
#[tokio::test]
async fn a_refused_field_refuses_the_whole_call_and_creates_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;

    let sent = |status: &str| {
        json!({
            "kind": "work", "handle": "sigma", "name": "Sigma", "source": "user-named",
            "parent": "project:atlas",
            "sets": {"status": status},
        })
    };
    s.refused("add_entity", sent("drafted, not asked"))
        .await
        .says("status");
    s.list("work").await.never_says("work:sigma");

    // The same call with an accepted word lands: nothing was left in the way.
    let made = s.call("add_entity", sent("now")).await.json();
    assert_eq!(made["id"], "work:sigma", "{made}");
    s.list("work").await.says("work:sigma");
    story.finish().await;
}

/// **A link to a handle nobody holds leaves nothing behind either.** It is
/// blocked, and the answer says the field named it, not the parent.
#[tokio::test]
async fn a_link_to_nothing_blocks_the_call_and_creates_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;
    s.add("person:lisa", "Lisa").await;

    let sent = |owner: &str| {
        json!({
            "kind": "work", "handle": "first-mix", "name": "First mix", "source": "user-named",
            "parent": "project:atlas",
            "sets": {"owner": owner},
        })
    };
    let refused = s
        .refused("add_entity", sent("person:contract-nobody"))
        .await;
    refused.says("person:contract-nobody").never_says("parent");
    s.list("work").await.never_says("work:first-mix");

    // Pointed at somebody who exists, the same call lands.
    s.call("add_entity", sent("person:lisa")).await;
    s.list("work").await.says("work:first-mix");
    story.finish().await;
}

/// **A handle the build supplies cannot be taken by a creation that carries
/// fields**, any more than by one that carries none.
#[tokio::test]
async fn a_supplied_handle_is_refused_with_fields_as_it_is_without() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let taken = s
        .refused(
            "add_entity",
            json!({
                "kind": "view", "handle": "colleagues", "name": "My Colleagues",
                "source": "user-named", "sets": {"selects": "person"},
            }),
        )
        .await;
    taken.says("colleagues");
    story.finish().await;
}

/// **The keys that make a thing fall due are worked out on creation, as on a
/// capture.** A promise made with its day comes back with the moment it falls
/// due stored beside it, which nobody typed, so it is in what is owed from the
/// day it is made.
#[tokio::test]
async fn a_promise_made_with_its_day_has_its_due_moment_stored() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;

    s.call(
        "add_entity",
        json!({
            "kind": "promise", "handle": "return-the-wrench", "name": "Return the wrench",
            "source": "user-named", "parent": "project:atlas",
            "sets": {"promised_by": "2026-11-20"},
        }),
    )
    .await;
    s.recall("promise:return-the-wrench")
        .await
        .says("\"due_on\":\"2026-11-20\"");
    let owed = s
        .call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-11-21"}}),
        )
        .await;
    owed.says("promise:return-the-wrench");
    story.finish().await;
}

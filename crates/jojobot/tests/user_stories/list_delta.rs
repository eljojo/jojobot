//! "Add one thing it depends on. Do not make me resend the rest."
//!
//! A key declared a list holds its items as one value, so changing one item by
//! resending the whole list is how a careless edit drops another. `sets_add` and
//! `sets_remove` change one item against what the thing holds now; the write
//! that follows is an ordinary whole-value write.
//!
//! The roster names are the cast: pieces of work, written in Greek letters.

use serde_json::json;

use super::dsl::{Session, Story};

/// A piece of work waiting on two others, both finished, so it can start.
async fn work_waiting_on_two(s: &Session) -> String {
    for (handle, name) in [
        ("work:delta", "Delta"),
        ("work:epsilon", "Epsilon"),
        ("work:gamma", "Gamma"),
        ("work:phi", "Phi"),
    ] {
        s.add(handle, name).await;
    }
    s.add("work:tau", "Tau").await;
    for done in ["work:delta", "work:epsilon"] {
        s.call(
            "capture",
            json!({"subject": done, "content": "finished", "provenance": "testimony",
                   "sets": {"status": "done"}}),
        )
        .await;
    }
    let first = s
        .call(
            "capture",
            json!({"subject": "work:tau", "content": "waits on two",
                   "provenance": "testimony",
                   "sets": {"depends_on": "work:delta, work:epsilon", "status": "next"}}),
        )
        .await;
    first.json()["address"]
        .as_str()
        .expect("an address")
        .to_string()
}

/// What the thing holds under `depends_on` now.
async fn depends_on(s: &Session) -> String {
    let read = s.call("recall", json!({"subject": "work:tau"})).await;
    read.json()["objects"][0]["fields"]["depends_on"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// **Adding one item keeps the rest and appends it, with nothing resent.** The
/// paired negative is the whole replacement, which still replaces.
#[tokio::test]
async fn adding_one_dependency_keeps_the_others_and_appends_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    work_waiting_on_two(&s).await;
    assert_eq!(depends_on(&s).await, "work:delta, work:epsilon");

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "also waits on gamma",
               "provenance": "testimony",
               "sets_add": {"depends_on": "work:gamma"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:delta, work:epsilon, work:gamma");

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "starts over from phi",
               "provenance": "testimony", "sets": {"depends_on": "work:phi"}}),
    )
    .await;
    assert_eq!(
        depends_on(&s).await,
        "work:phi",
        "a whole replacement replaces"
    );
    story.finish().await;
}

/// **Removing one item keeps the others, through `update_fact` as well as
/// `capture`.** Removing an item the list does not hold changes nothing and is
/// no error.
#[tokio::test]
async fn removing_one_dependency_keeps_the_others_through_either_verb() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let record = work_waiting_on_two(&s).await;

    s.call(
        "update_fact",
        json!({"address": record, "sets_add": {"depends_on": "work:gamma"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:delta, work:epsilon, work:gamma");

    s.call(
        "update_fact",
        json!({"address": record, "sets_remove": {"depends_on": "work:epsilon"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:delta, work:gamma");

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "drops one it never had",
               "provenance": "testimony", "sets_remove": {"depends_on": "work:phi"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:delta, work:gamma");

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "swaps one for another",
               "provenance": "testimony",
               "sets_remove": {"depends_on": "work:delta"},
               "sets_add": {"depends_on": "work:phi"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:gamma, work:phi");
    story.finish().await;
}

/// **An item added must be a thing that exists, and a refusal names the
/// nearest.** Nothing lands. The positive is the same call naming a real thing.
#[tokio::test]
async fn adding_a_dependency_that_names_nothing_is_refused_with_candidates() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    work_waiting_on_two(&s).await;

    s.refused(
        "capture",
        json!({"subject": "work:tau", "content": "waits on a ghost",
               "provenance": "testimony", "sets_add": {"depends_on": "work:gamm"}}),
    )
    .await
    .says("work:gamma")
    .says("\"wrote\":false");
    assert_eq!(depends_on(&s).await, "work:delta, work:epsilon");

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "waits on gamma",
               "provenance": "testimony", "sets_add": {"depends_on": "work:gamma"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:delta, work:epsilon, work:gamma");
    story.finish().await;
}

/// **A reference to a thing that is gone can come off.** Removing a name that
/// resolves to nothing is not refused. Paired with the removal of a thing that
/// is archived, which stays a thing and comes off just the same.
#[tokio::test]
async fn a_dependency_on_something_gone_can_be_removed() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    work_waiting_on_two(&s).await;
    s.call(
        "archive_entity",
        json!({"handle": "work:epsilon", "reason": "dropped"}),
    )
    .await;

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "lets go of epsilon and a ghost",
               "provenance": "testimony",
               "sets_remove": {"depends_on": "work:epsilon, work:ghost"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:delta");

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "lets go of the last one",
               "provenance": "testimony", "sets_remove": {"depends_on": "work:delta"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "", "a list emptied holds nothing");
    let ready = s
        .call("recall", json!({"view": "ready", "subject": "work:tau"}))
        .await;
    ready.says("\"ready\":true");
    story.finish().await;
}

/// **Only a key declared a list takes the forms**, and a refusal names the key.
/// The same words under `sets` are the ordinary write they always were. A key
/// named in `sets` and in a delta in one call is two answers and is refused.
#[tokio::test]
async fn a_key_that_is_not_a_list_refuses_the_forms_and_a_clash_with_sets() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    work_waiting_on_two(&s).await;
    s.add("person:lisa", "Lisa").await;

    s.refused(
        "capture",
        json!({"subject": "work:tau", "content": "adds an owner",
               "provenance": "testimony", "sets_add": {"owner": "person:lisa"}}),
    )
    .await
    .says("owner")
    .says("\"wrote\":false");
    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "sets an owner",
               "provenance": "testimony", "sets": {"owner": "person:lisa"}}),
    )
    .await;

    s.refused(
        "capture",
        json!({"subject": "work:tau", "content": "says it twice",
               "provenance": "testimony", "sets": {"depends_on": "work:phi"},
               "sets_add": {"depends_on": "work:gamma"}}),
    )
    .await
    .says("depends_on")
    .says("\"wrote\":false");
    assert_eq!(depends_on(&s).await, "work:delta, work:epsilon");
    story.finish().await;
}

/// **The ready view sees the dependency the moment it is added.** Before, the
/// work can start; after adding an unfinished one, it names what stops it.
#[tokio::test]
async fn the_ready_view_sees_a_dependency_added_without_resending_the_list() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    work_waiting_on_two(&s).await;

    let before = s
        .call("recall", json!({"view": "ready", "subject": "work:tau"}))
        .await;
    before.says("\"ready\":true");

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "also waits on gamma",
               "provenance": "testimony", "sets_add": {"depends_on": "work:gamma"}}),
    )
    .await;
    let after = s
        .call("recall", json!({"view": "ready", "subject": "work:tau"}))
        .await;
    after.says("\"ready\":false").says("work:gamma");
    story.finish().await;
}

/// **A delta is resolved against what the thing holds, not against the record it
/// rides on.** An earlier claim set two items; a later one added a third. Taking
/// one off through the earlier claim keeps the third, which only the thing's own
/// value holds. The paired positive is the later claim's result, read first.
#[tokio::test]
async fn a_delta_is_resolved_against_the_thing_and_not_the_record_it_rides_on() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let earlier = work_waiting_on_two(&s).await;

    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "later waits on gamma",
               "provenance": "testimony", "sets_add": {"depends_on": "work:gamma"}}),
    )
    .await;
    assert_eq!(depends_on(&s).await, "work:delta, work:epsilon, work:gamma");

    s.call(
        "update_fact",
        json!({"address": earlier, "sets_remove": {"depends_on": "work:epsilon"}}),
    )
    .await;
    assert_eq!(
        depends_on(&s).await,
        "work:delta, work:gamma",
        "the earlier claim never held gamma, and the thing does"
    );
    story.finish().await;
}

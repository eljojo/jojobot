//! "If the person changes in the meantime, I need the system to show me what's
//! updated."
//!
//! A session that was served a thing keeps how it stood. When another session
//! writes to it, the first session's next answer, whatever verb it asked, names
//! the thing and what changed. A session whose held things did not change sees
//! nothing extra, and a session's own writes are not news to it.
//!
//! The roster names are the cast: Homer and Lisa, and two bots.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// Otto holds Homer: he was recalled with his claims. Returns the story, Otto,
/// the other bot's session and the address of the claim on Homer.
async fn otto_holds_homer() -> (Story, Session, Session, String) {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("person:homer", "Homer").await;
    otto.add("person:lisa", "Lisa").await;
    let claim = otto
        .call(
            "capture",
            json!({"subject": "person:homer", "content": "likes donuts"}),
        )
        .await
        .json()["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let other = story.as_bot("bot:assistant").await;
    otto.call("recall", json!({"subject": "person:homer", "facts": true}))
        .await;
    (story, otto, other, claim)
}

/// A read that has nothing to do with Homer.
async fn some_other_read(otto: &Session) -> Value {
    otto.call("recall", json!({"subject": "person:lisa"}))
        .await
        .json()
}

/// **A claim edited by another session is named on the next answer**, with the
/// thing it is on and what changed. Any verb carries it, and the paired negative
/// is the same read before the edit, which carries nothing.
#[tokio::test]
async fn an_edit_by_another_session_is_named_on_the_next_answer() {
    let (story, otto, other, claim) = otto_holds_homer().await;
    assert!(some_other_read(&otto).await.get("held_changed").is_none());

    other
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer", "provenance": "inference"}),
        )
        .await;

    let next = otto
        .call("search", json!({"query": "springfield"}))
        .await
        .json();
    let changed = next["held_changed"].as_array().expect("held_changed");
    assert_eq!(changed.len(), 1, "{next}");
    assert_eq!(changed[0]["handle"], "person:homer", "{next}");
    assert_eq!(changed[0]["claims_edited"], json!([claim]), "{next}");
    story.finish().await;
}

/// **A claim added by another session is named as new.**
#[tokio::test]
async fn a_claim_added_by_another_session_is_named_as_new() {
    let (story, otto, other, _) = otto_holds_homer().await;

    let added = other
        .call(
            "capture",
            json!({"subject": "person:homer", "content": "plays the saxophone"}),
        )
        .await
        .json()["address"]
        .as_str()
        .expect("an address")
        .to_string();

    let next = some_other_read(&otto).await;
    assert_eq!(next["held_changed"][0]["handle"], "person:homer", "{next}");
    assert_eq!(
        next["held_changed"][0]["claims_new"],
        json!([added]),
        "{next}"
    );
    story.finish().await;
}

/// **A change is told once.** The answer after the one that carried it carries
/// nothing.
#[tokio::test]
async fn a_change_is_told_once() {
    let (story, otto, other, claim) = otto_holds_homer().await;
    other
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer", "provenance": "inference"}),
        )
        .await;
    assert!(some_other_read(&otto).await.get("held_changed").is_some());

    assert!(some_other_read(&otto).await.get("held_changed").is_none());
    story.finish().await;
}

/// **A write to something the session does not hold adds nothing to its
/// answers.**
#[tokio::test]
async fn an_unrelated_edit_adds_nothing() {
    let (story, otto, other, _) = otto_holds_homer().await;

    other
        .call(
            "capture",
            json!({"subject": "person:lisa", "content": "plays the saxophone"}),
        )
        .await;

    assert!(some_other_read(&otto).await.get("held_changed").is_none());
    story.finish().await;
}

/// **A session's own write is not news to it.** The paired positive is the same
/// edit made by the other session in the case above.
#[tokio::test]
async fn a_session_is_not_told_of_its_own_edit() {
    let (story, otto, _other, claim) = otto_holds_homer().await;

    let edit = otto
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer",
                   "provenance": "inference"}),
        )
        .await
        .json();

    assert!(edit.get("held_changed").is_none(), "{edit}");
    assert!(some_other_read(&otto).await.get("held_changed").is_none());
    story.finish().await;
}

/// **A report of compaction clears what the session holds**, so a change made
/// before it is not told after it.
#[tokio::test]
async fn a_compaction_report_clears_what_is_held() {
    let (story, otto, other, claim) = otto_holds_homer().await;
    other
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer",
                   "provenance": "inference"}),
        )
        .await;

    let reported = otto
        .call("start_here", json!({"compacted": true, "brief": true}))
        .await
        .json();

    assert!(reported.get("held_changed").is_none(), "{reported}");
    assert!(some_other_read(&otto).await.get("held_changed").is_none());
    story.finish().await;
}

/// **A resent request id answers with its first receipt and carries no notice.**
/// The first answer told Otto what changed; the repeat is the receipt as it was
/// stored, which holds no notice.
#[tokio::test]
async fn a_repeat_of_a_write_carries_no_notice() {
    let (story, otto, other, claim) = otto_holds_homer().await;
    other
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer", "provenance": "inference"}),
        )
        .await;
    let write = json!({"subject": "person:lisa", "content": "plays the clarinet",
                       "request_id": "lisa-clarinet"});

    let first = otto.call("capture", write.clone()).await.json();
    let repeat = otto.call("capture", write).await.json();

    assert!(first.get("held_changed").is_some(), "{first}");
    assert!(repeat.get("held_changed").is_none(), "{repeat}");
    assert_eq!(repeat["repeat"], true, "{repeat}");
    story.finish().await;
}

/// **A held thing that this answer serves again is not also named.** It comes
/// back whole, and the notice would only repeat what the answer shows.
#[tokio::test]
async fn a_thing_served_again_is_not_named_beside_itself() {
    let (story, otto, other, claim) = otto_holds_homer().await;
    other
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer",
                   "provenance": "inference"}),
        )
        .await;

    let again = otto
        .call("recall", json!({"subject": "person:homer", "facts": true}))
        .await
        .json();

    assert_eq!(again["objects"][0]["id"], "person:homer", "{again}");
    assert!(again.get("held_changed").is_none(), "{again}");
    story.finish().await;
}

/// **A rename and an archive by another session are named.**
#[tokio::test]
async fn a_rename_and_an_archive_by_another_session_are_named() {
    let (story, otto, other, _) = otto_holds_homer().await;
    other
        .call(
            "update_entity",
            json!({"handle": "person:homer", "name": "Homer J. Simpson"}),
        )
        .await;
    let renamed = some_other_read(&otto).await;
    assert_eq!(
        renamed["held_changed"][0]["handle"], "person:homer",
        "{renamed}"
    );
    assert_eq!(
        renamed["held_changed"][0]["name"], "Homer J. Simpson",
        "{renamed}"
    );

    other
        .call(
            "archive_entity",
            json!({"handle": "person:homer", "reason": "a test"}),
        )
        .await;
    let archived = some_other_read(&otto).await;
    assert_eq!(archived["held_changed"][0]["archived"], true, "{archived}");
    story.finish().await;
}

/// **A message another run of the bot processed is named, and one the session
/// processed itself is not.**
#[tokio::test]
async fn a_message_another_run_processed_is_named_and_one_this_run_processed_is_not() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    let second = story.as_bot("bot:otto").await;
    let sender = story.as_bot("bot:assistant").await;
    let mut ids = Vec::new();
    for subject in ["first note", "second note"] {
        let posted = sender
            .call(
                "post_message",
                json!({"to": "otto", "subject": subject, "body": "read me"}),
            )
            .await
            .json();
        ids.push(posted["id"].as_str().expect("an id").to_string());
    }
    first.call("read_mailbox", json!({})).await;

    second
        .call("mark_processed", json!({"message_id": ids[0]}))
        .await;
    let told = first
        .call("search", json!({"query": "springfield"}))
        .await
        .json();
    assert_eq!(told["held_changed"][0]["message"], ids[0], "{told}");
    assert_eq!(told["held_changed"][0]["state"], "processed", "{told}");

    first
        .call("mark_processed", json!({"message_id": ids[1]}))
        .await;
    assert!(
        first
            .call("search", json!({"query": "springfield"}))
            .await
            .json()
            .get("held_changed")
            .is_none()
    );
    story.finish().await;
}

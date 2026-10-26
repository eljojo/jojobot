//! "If the person changes in the meantime, I need the system to show me what's
//! updated."
//!
//! A session that was served a thing keeps how it stood. When another session
//! writes to it, the first session's next ordinary answer names the thing and
//! what changed; a search defers news unrelated to its hits. A session whose held
//! things did not change sees nothing extra, and its own writes are not news to it.
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
/// thing it is on and what changed. An unrelated search leaves it for the next
/// ordinary read; the same read before the edit carries nothing.
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

    let searched = otto
        .call("search", json!({"query": "springfield"}))
        .await
        .json();
    assert!(searched.get("held_changed").is_none(), "{searched}");
    let next = some_other_read(&otto).await;
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
/// answers.** The positive stands first: a write to Homer, whom Otto holds, is
/// named, so the silence after the write to Lisa is not a notice that never
/// comes.
#[tokio::test]
async fn an_unrelated_edit_adds_nothing() {
    let (story, otto, other, claim) = otto_holds_homer().await;
    other
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer", "provenance": "inference"}),
        )
        .await;
    let held = some_other_read(&otto).await;
    assert_eq!(held["held_changed"][0]["handle"], "person:homer", "{held}");

    // Bart is nobody Otto was shown, and the read above served Lisa, not Bart.
    other.add("person:bart", "Bart").await;
    other
        .call(
            "capture",
            json!({"subject": "person:bart", "content": "plays the saxophone"}),
        )
        .await;

    assert!(some_other_read(&otto).await.get("held_changed").is_none());
    story.finish().await;
}

/// **A session's own write is not news to it.** Otto edits the claim and hears
/// nothing; the other session then adds a claim, and Otto's next answer names
/// that one alone, so the silence about Otto's own edit sits beside a notice
/// that does come.
#[tokio::test]
async fn a_session_is_not_told_of_its_own_edit() {
    let (story, otto, other, claim) = otto_holds_homer().await;

    let edit = otto
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and beer",
                   "provenance": "inference"}),
        )
        .await
        .json();
    assert!(edit.get("held_changed").is_none(), "{edit}");

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
    assert_eq!(
        next["held_changed"][0]["claims_new"],
        json!([added]),
        "{next}"
    );
    assert!(
        next["held_changed"][0].get("claims_edited").is_none(),
        "Otto's own edit is not named beside the other session's claim: {next}"
    );
    story.finish().await;
}

/// **A report of compaction clears what the session holds**, so a change made
/// before it is not told after it. The positive stands first: the same kind of
/// change, made and read without a compaction between, is told.
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
    let told = some_other_read(&otto).await;
    assert_eq!(told["held_changed"][0]["handle"], "person:homer", "{told}");

    other
        .call(
            "update_fact",
            json!({"address": claim, "content": "likes donuts and lager",
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
    let searched = first
        .call("search", json!({"query": "springfield"}))
        .await
        .json();
    assert!(searched.get("held_changed").is_none(), "{searched}");
    let told = first
        .call("recall", json!({"subject": "bot:otto"}))
        .await
        .json();
    assert_eq!(told["held_changed"][0]["message"], ids[0], "{told}");
    assert_eq!(told["held_changed"][0]["state"], "processed", "{told}");

    first
        .call("mark_processed", json!({"message_id": ids[1]}))
        .await;
    assert!(
        first
            .call("recall", json!({"subject": "bot:otto"}))
            .await
            .json()
            .get("held_changed")
            .is_none()
    );
    story.finish().await;
}

/// **A message another run of the bot quarantined is named, with its state and
/// nothing of what it said, and it is named once.** A quarantined card is not
/// readable, so the run that holds it cannot read its state; the history still
/// answers for it. The paired negative is a message the other run only reads
/// again, which changes nothing for the first run.
#[tokio::test]
async fn a_message_another_run_quarantined_is_named_once_without_its_words() {
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
        .call("read_message", json!({"message_id": ids[1]}))
        .await;
    assert!(
        first
            .call("recall", json!({"subject": "bot:otto"}))
            .await
            .json()
            .get("held_changed")
            .is_none(),
        "a message the other run only read again is no news"
    );

    second
        .call(
            "mark_processed",
            json!({"message_id": ids[0], "quarantine": "posted twice by mistake"}),
        )
        .await;
    let searched = first
        .call("search", json!({"query": "springfield"}))
        .await
        .json();
    assert!(searched.get("held_changed").is_none(), "{searched}");
    let told = first
        .call("recall", json!({"subject": "bot:otto"}))
        .await
        .json();
    assert_eq!(told["held_changed"][0]["message"], ids[0], "{told}");
    assert_eq!(told["held_changed"][0]["state"], "quarantined", "{told}");
    assert!(
        told["held_changed"][0].get("subject").is_none()
            && told["held_changed"][0].get("body").is_none(),
        "the notice names the state and none of the words: {told}"
    );

    assert!(
        first
            .call("recall", json!({"subject": "bot:otto"}))
            .await
            .json()
            .get("held_changed")
            .is_none(),
        "a change is told once"
    );
    story.finish().await;
}

/// **A claim held on its own is told when another session wrote it before this
/// one did.** Otto reads one claim of Homer by its address, so he holds the
/// claim and not Homer. The other session edits it, and Otto edits it too
/// without reading it again. The newest write is Otto's own, and the answer to
/// his edit still names the claim, because the other session wrote after the
/// version he held. The paired negative is the same read when only Otto wrote after it.
#[tokio::test]
async fn a_claim_another_session_wrote_before_this_one_did_is_named() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    // The other session makes Homer and the claim, so Otto is never served
    // Homer himself and holds the claim alone.
    let other = story.as_bot("bot:assistant").await;
    other.add("person:homer", "Homer").await;
    other.add("person:lisa", "Lisa").await;
    let claim = other
        .call(
            "capture",
            json!({"subject": "person:homer", "content": "likes donuts"}),
        )
        .await
        .json()["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let edit = |content: &'static str| json!({"address": claim, "content": content, "provenance": "inference"});

    // Otto holds the claim and not Homer, and he alone writes after it.
    otto.call("recall", json!({"records": [claim.clone()]}))
        .await;
    otto.call("update_fact", edit("likes donuts and beer"))
        .await;
    let own = some_other_read(&otto).await;
    assert!(own.get("held_changed").is_none(), "{own}");

    // Otto holds it again, the other session edits it, and Otto edits it too.
    otto.call("recall", json!({"records": [claim.clone()]}))
        .await;
    other
        .call("update_fact", edit("likes donuts and lager"))
        .await;
    let next = otto
        .call("update_fact", edit("likes donuts and stout"))
        .await
        .json();
    let told: Vec<&Value> = next["held_changed"]
        .as_array()
        .map(|all| all.iter().filter(|n| n["address"] == claim).collect())
        .unwrap_or_default();
    assert_eq!(told.len(), 1, "the claim is named once: {next}");
    assert_eq!(told[0]["claim"], "edited", "{next}");
    story.finish().await;
}

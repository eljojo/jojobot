//! "Keeping track in a clean way of what a bot is doing is important to me."
//!
//! A bot's `working_on` points at the thing it is doing now. A session declares
//! or clears it through `journal`, and moving work the bot owns to `now` fills
//! it. A reader sees the thing only while the session that wrote it is live.
//!
//! The roster names are the cast: bots are Otto and the assistant the software ships, pieces of work are
//! written in Greek letters.

use serde_json::json;

use super::dsl::{Session, Story};

/// What the bot holds under `working_on` as a reader sees it, or none.
async fn working_on(reader: &Session, bot: &str) -> Option<String> {
    let read = reader.call("recall", json!({"subject": bot})).await;
    read.json()["objects"][0]["fields"]["working_on"]
        .as_str()
        .map(str::to_string)
}

/// **A session declares what its bot is doing, and any reader sees it.** The
/// paired negative is the bot before it declares: no key, not an empty one.
#[tokio::test]
async fn a_session_declares_what_its_bot_is_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    let reader = story.as_bot("bot:assistant").await;
    assert_eq!(working_on(&reader, "bot:otto").await, None);

    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;

    assert_eq!(
        working_on(&reader, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **A session clears it with an empty `working_on`**, and the key goes away
/// rather than reading as an empty string.
#[tokio::test]
async fn a_session_clears_what_its_bot_was_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );

    otto.call(
        "journal",
        json!({"entry": "put sigma down", "working_on": ""}),
    )
    .await;

    assert_eq!(working_on(&otto, "bot:otto").await, None);
}

/// A piece of work owned by `owner`, waiting its turn.
async fn owned_work(s: &Session, handle: &str, name: &str, owner: &str) -> String {
    s.add(handle, name).await;
    let made = s
        .call(
            "capture",
            json!({"subject": handle, "content": "waits its turn",
                   "sets": {"owner": owner, "status": "next"}}),
        )
        .await;
    made.json()["address"]
        .as_str()
        .expect("an address")
        .to_string()
}

/// **Moving work the bot owns to `now` points the bot at it**, with no call to
/// `journal`. The paired negative is a move to any other status.
#[tokio::test]
async fn capturing_owned_work_as_now_fills_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    owned_work(&otto, "work:sigma", "Sigma", "bot:otto").await;
    otto.call(
        "capture",
        json!({"subject": "work:sigma", "content": "queued behind others",
               "provenance": "testimony", "sets": {"status": "waiting"}}),
    )
    .await;
    assert_eq!(working_on(&otto, "bot:otto").await, None);

    otto.call(
        "capture",
        json!({"subject": "work:sigma", "content": "starting it",
               "provenance": "testimony", "sets": {"status": "now"}}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **An edit that moves owned work to `now` fills it the same way.**
#[tokio::test]
async fn editing_owned_work_to_now_fills_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let address = owned_work(&otto, "work:sigma", "Sigma", "bot:otto").await;
    assert_eq!(working_on(&otto, "bot:otto").await, None);

    otto.call(
        "update_fact",
        json!({"address": address, "sets": {"status": "now"}}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );
}

/// **Another bot moving the work fills nothing for its owner**, and nothing for
/// itself, because it does not own the work.
#[tokio::test]
async fn another_bot_moving_the_work_fills_nothing() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let address = owned_work(&otto, "work:sigma", "Sigma", "bot:otto").await;
    let other = story.as_bot("bot:assistant").await;

    other
        .call(
            "update_fact",
            json!({"address": address, "sets": {"status": "now"}}),
        )
        .await;

    assert_eq!(working_on(&other, "bot:otto").await, None);
    assert_eq!(working_on(&other, "bot:assistant").await, None);
}

/// **A rename of the thing keeps the link**, and the bot reads the new handle.
#[tokio::test]
async fn renaming_the_thing_keeps_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;

    otto.call(
        "rename_entity",
        json!({"handle": "work:sigma", "to": "work:zeta"}),
    )
    .await;

    assert_eq!(
        working_on(&otto, "bot:otto").await.as_deref(),
        Some("work:zeta")
    );
}

/// **After the writing session wraps, a reader sees no `working_on`**, and the
/// answer says why the key is not there. The paired positive is the same read
/// before the wrap.
#[tokio::test]
async fn a_wrapped_session_leaves_working_on_unknown() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    let reader = story.as_bot("bot:assistant").await;
    assert_eq!(
        working_on(&reader, "bot:otto").await.as_deref(),
        Some("work:sigma")
    );

    otto.call("wrap_session", json!({"story": "done for now"}))
        .await;

    let after = reader.call("recall", json!({"subject": "bot:otto"})).await;
    assert_eq!(
        after.json()["objects"][0]["fields"]["working_on"],
        json!(null)
    );
    assert!(
        after.json()["working_on_note"].is_string(),
        "{}",
        after.json()
    );
    let backed = reader
        .call("recall", json!({"subject": "bot:otto", "backing": true}))
        .await;
    assert_eq!(
        backed.json()["objects"][0]["fields_backing"]["working_on"],
        json!(null)
    );
}

/// **The bot's own page shows the key while the writing session is live and
/// says it is unknown after the wrap**, the same answer a `recall` gives.
#[tokio::test]
async fn the_page_of_a_bot_shows_working_on_only_while_its_session_is_live() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    let live = story.page("bot:otto").await;
    live.section("fields").says("working_on").says("work:sigma");
    live.never_says("working-on-unknown");

    otto.call("wrap_session", json!({"story": "done for now"}))
        .await;

    let after = story.page("bot:otto").await;
    after.never_says("history=working_on");
    after.says("working-on-unknown");
}

/// **Only work fills it.** A project the bot owns, moved to `now`, points the
/// bot at nothing.
#[tokio::test]
async fn a_project_moved_to_now_does_not_fill_working_on() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("project:atlas", "Sigma").await;

    otto.call(
        "capture",
        json!({"subject": "project:atlas", "content": "starting it",
               "sets": {"owner": "bot:otto", "status": "now"}}),
    )
    .await;

    assert_eq!(working_on(&otto, "bot:otto").await, None);
}

/// **A handle that names nothing is refused with the entry unwritten**, so the
/// call can be sent again whole.
#[tokio::test]
async fn a_working_on_that_names_nothing_is_refused() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;

    otto.refused(
        "journal",
        json!({"entry": "picked up a ghost", "working_on": "work:ghost"}),
    )
    .await
    .says("\"wrote\":false")
    .says("work:ghost");

    assert_eq!(working_on(&otto, "bot:otto").await, None);
}

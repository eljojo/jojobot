//! "Frame the walls waits on the slab."
//!
//! `waiting_on` says WHO a piece of work waits on, a bot or a person, and
//! `depends_on` says WHAT it waits on. A work handle under `waiting_on` is the
//! natural mistake, and the refusal sends the writer to the key that takes it,
//! so a card that waits on another card is never parked where the walk that
//! lists a release's waiting work cannot see it.
//!
//! The roster names are the cast: pieces of work and a bot.

use serde_json::json;

use super::dsl::Story;

async fn work(s: &super::dsl::Session, slug: &str) {
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": {"status": "next"}}),
    )
    .await;
}

/// **A work handle under `waiting_on` is refused naming `depends_on`; the same
/// handle under `depends_on`, and a bot under `waiting_on`, land.**
#[tokio::test]
async fn a_work_handle_under_waiting_on_is_refused_toward_depends_on() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    work(&s, "pour-the-slab").await;
    work(&s, "frame-the-walls").await;

    let refused = s
        .refused(
            "capture",
            json!({"subject": "work:frame-the-walls", "content": "waits on the slab",
                   "provenance": "testimony",
                   "sets": {"waiting_on": "work:pour-the-slab"}}),
        )
        .await;
    refused.says("depends_on");
    let held = s
        .call("recall", json!({"subject": "work:frame-the-walls"}))
        .await;
    assert!(
        held.json()["objects"][0]["fields"]
            .get("waiting_on")
            .is_none(),
        "the refused write left nothing behind: {}",
        held.raw()
    );

    s.call(
        "capture",
        json!({"subject": "work:frame-the-walls", "content": "waits on the slab",
               "provenance": "testimony",
               "sets": {"depends_on": "work:pour-the-slab"}}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": "work:frame-the-walls", "content": "waits on gamma",
               "provenance": "testimony", "sets": {"waiting_on": "bot:gamma"}}),
    )
    .await;
    let held = s
        .call("recall", json!({"subject": "work:frame-the-walls"}))
        .await;
    let fields = &held.json()["objects"][0]["fields"];
    assert_eq!(fields["depends_on"], "work:pour-the-slab", "{}", held.raw());
    assert_eq!(fields["waiting_on"], "bot:gamma", "{}", held.raw());
    story.finish().await;
}

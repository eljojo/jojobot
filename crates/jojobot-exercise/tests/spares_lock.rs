//! **The glasses' and keys' November spare-check locks, held against a
//! real room.**
//!
//! Both negatives replay one real regression, verbatim: a paid run of
//! `vault.md` (`transcripts/year-run-23-2026-09-17-opus`, kept outside
//! this repo, not copied here), asked on 2026-11-08 whether every spare
//! was covered before a trip, correctly worked out that the glasses and
//! the keys already had answers on record — and wrote ONE claim saying so,
//! filed on `topic:the-five-words`, rather than a fresh touch on
//! `thing:glasses` and `thing:house-keys` themselves. Both things were
//! genuinely covered; neither got its own November-dated record, so
//! either lock, asking whether TODAY answered the question, finds nothing
//! on the thing itself.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape the other lock-proof files open their
/// rooms with.
async fn furnished() -> (Room, Surface, String) {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(expectations::VAULT_ROOM)
        .expect("the room has furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");
    let booted = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("the shipped identity boots");
    let sid = booted["session"]["sid"]
        .as_str()
        .expect("a handle")
        .to_string();
    (room, surface, sid)
}

/// **The exact real regression: one summary, filed on the topic, saying
/// glasses and keys are both covered — neither thing touched directly.**
async fn file_the_summary_on_the_topic(surface: &Surface, sid: &str) {
    surface
        .must(
            "add_entity",
            json!({
                "handle": "the-five-words",
                "kind": "topic",
                "name": "The Five Words",
                "source": "the operator's note moving their notes in",
                "sid": sid,
            }),
        )
        .await
        .expect("the topic exists — the real sitting created it in January");
    surface
        .must(
            "capture",
            json!({
                "subject": "topic:the-five-words",
                "content": "On 2026-11-08 only @thing:glasses and @thing:house-keys carried \
                            `spare` — every other thing here was asked the question and the \
                            answer was recorded as not known",
                "details": "The record of the spare pass they asked for before the trip.",
                "object": "place:capital-city",
                "shape": "about",
                "fields": {
                    "asked_on": "2026-11-08",
                    "purpose": "what the spare pass covered, and what it deliberately did not",
                },
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-11-08",
                "sid": sid,
            }),
        )
        .await
        .expect("the summary is captured — on the topic, exactly as the real sitting did");
}

fn the_glasses_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("the answer that has been on record since January was not put on them")
        })
        .expect("the vault ships the glasses lock")
}

fn the_keys_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("a spare that has been on record since February was either not found")
        })
        .expect("the vault ships the keys lock")
}

/// **The control: the glasses touched directly on the day asked.**
#[tokio::test]
async fn the_glasses_lock_holds_when_touched_directly() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:glasses",
                "content": "Still a spare pair in the hall drawer, checked before the trip",
                "fields": {"spare": "hall drawer"},
                "provenance": "testimony",
                "standing": "settled",
                "recorded_at": "2026-11-08",
                "sid": sid,
            }),
        )
        .await
        .expect("the glasses are touched directly");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_glasses_lock().check(&seen).await;
    assert!(
        outcome.held,
        "touched directly, the glasses lock still failed: {}",
        outcome.saying,
    );
}

/// **The real regression: the glasses lock reds when only the summary was
/// filed on the topic.**
#[tokio::test]
async fn the_glasses_lock_reds_when_only_the_summary_is_filed_on_the_topic() {
    let (_room, surface, sid) = furnished().await;
    file_the_summary_on_the_topic(&surface, &sid).await;

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_glasses_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "only the topic carried the summary, and the glasses lock held anyway: {}",
        outcome.saying,
    );
}

/// **The control: the keys touched directly on the day asked.**
#[tokio::test]
async fn the_keys_lock_holds_when_touched_directly() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:house-keys",
                "content": "Teddy's spare set still stands, checked before the trip",
                "object": "person:teddy",
                "shape": "connection",
                "fields": {"spare": "with Teddy"},
                "provenance": "testimony",
                "standing": "settled",
                "recorded_at": "2026-11-08",
                "sid": sid,
            }),
        )
        .await
        .expect("the keys are touched directly");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_keys_lock().check(&seen).await;
    assert!(
        outcome.held,
        "touched directly, the keys lock still failed: {}",
        outcome.saying,
    );
}

/// **The real regression: the keys lock reds when only the summary was
/// filed on the topic.**
#[tokio::test]
async fn the_keys_lock_reds_when_only_the_summary_is_filed_on_the_topic() {
    let (_room, surface, sid) = furnished().await;
    file_the_summary_on_the_topic(&surface, &sid).await;

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_keys_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "only the topic carried the summary, and the keys lock held anyway: {}",
        outcome.saying,
    );
}

//! **The fair-to-college connection lock, held against a real room.**
//!
//! The negative here is a literal replay: a real paid run of `vault.md`
//! (`transcripts/year-run-23-2026-09-17-opus`, kept outside this repo, not
//! copied here) captured, verbatim, "the one thing still to do for the fair
//! is the food-handling certificate" on `event:wagstaff-fair` on 2026-09-13
//! — with no edge to `org:quahog-community-college`, the place that
//! certificate actually comes from, mentioned to the same session three
//! months earlier in a different context (Gayle's own course). The miss
//! was a retrieval failure — that September sitting never connected its
//! own question back to a fact from a different month — which this test
//! does not reproduce. What it proves instead is narrower and still real:
//! that the shipped lock correctly reddens on the state that sitting
//! actually left behind, and holds on the state a sitting that DID make
//! the connection would leave. The positive side is constructed, not
//! observed — no sitting in the real run ever captured it — which is
//! exactly the difference from `desk_lock.rs`/`wharf_lock.rs`/
//! `furnace_lock.rs`/`trip_lock.rs`, where both sides are drawn from real
//! calls the run actually made.

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

/// **The lock, read straight off the shipped document.** Matched by its own
/// sentence rather than by position, so a lock added above it in the same
/// phase does not silently start pointing this case at the wrong one.
fn the_fair_college_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("or it wrote the answer where the fair cannot be walked to it")
        })
        .expect("the vault ships the fair-college lock")
}

/// **The control: the same September note, this time drawing the
/// connection to the college the certificate actually comes from.** This
/// side is constructed — no sitting in the real run captured it this way —
/// see the module doc.
#[tokio::test]
async fn the_fair_college_lock_holds_when_the_connection_is_drawn() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "event:wagstaff-fair",
                "content": "The one thing still to do for the fair is the food-handling \
                            certificate, from the course at @org:quahog-community-college",
                "object": "org:quahog-community-college",
                "shape": "connection",
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-09-13",
                "sid": sid,
            }),
        )
        .await
        .expect("the fair's note draws the connection to the college");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_fair_college_lock().check(&seen).await;
    assert!(
        outcome.held,
        "the connection was drawn, and the lock still failed: {}",
        outcome.saying,
    );
}

/// **The control the last scout flagged as missing: a mention alone, no
/// edge and no `refs`.** The lock's mechanism (`recall` on the fair's own
/// facts, then a bare-handle substring) is provably route-agnostic by
/// construction — a mention renders back to `@org:quahog-community-college`
/// before the answer is built, landing the handle in `content` exactly as
/// an edge's object lands in `edge.object` — but nothing had isolated it
/// until this case.
#[tokio::test]
async fn the_fair_college_lock_holds_when_only_a_mention_draws_the_connection() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "event:wagstaff-fair",
                "content": "The one thing still to do for the fair is the food-handling \
                            certificate, from the course at @org:quahog-community-college",
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-09-13",
                "sid": sid,
            }),
        )
        .await
        .expect("the fair's note mentions the college, with no edge and no refs");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_fair_college_lock().check(&seen).await;
    assert!(
        outcome.held,
        "the college was named only in the note's own words, and the lock still failed: {}",
        outcome.saying,
    );
}

/// **The other control the last scout flagged: `refs` alone, no edge and no
/// mention.** Same reasoning as the mention case — a `refs` entry renders as
/// a bare array element carrying the handle, which the lock's substring test
/// cannot tell from an edge's object either.
#[tokio::test]
async fn the_fair_college_lock_holds_when_only_a_ref_draws_the_connection() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "event:wagstaff-fair",
                "content": "The one thing still to do for the fair is the food-handling \
                            certificate",
                "refs": ["org:quahog-community-college"],
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-09-13",
                "sid": sid,
            }),
        )
        .await
        .expect("the fair's note names the college only in refs, with no edge and no mention");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_fair_college_lock().check(&seen).await;
    assert!(
        outcome.held,
        "the college was named only in refs, and the lock still failed: {}",
        outcome.saying,
    );
}

/// **The exact real regression, replayed verbatim: the same note, with no
/// connection to the college at all.**
#[tokio::test]
async fn the_fair_college_lock_reds_when_no_connection_is_drawn() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "event:wagstaff-fair",
                "content": "The one thing still to do for the @event:wagstaff-fair is the \
                            food-handling certificate",
                "details": "My reading on 2026-09-13, not the operator's words — they asked \
                            what is still owed on October and to put it on the thing itself, \
                            and this is what the record leaves owed.",
                "fields": {
                    "purpose": "what October still owes, on the thing it is owed for",
                    "still_to_do": "the food-handling certificate",
                },
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-09-13",
                "sid": sid,
            }),
        )
        .await
        .expect(
            "the fair's note is captured — with no connection, exactly as the real sitting did",
        );

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_fair_college_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "no connection was drawn to the college, and the lock held anyway: {}",
        outcome.saying,
    );
}

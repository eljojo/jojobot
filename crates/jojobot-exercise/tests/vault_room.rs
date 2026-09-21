//! **The vault's archived-exclusion counter, held against a real room.**
//!
//! There is no synthetic occupant for `vault.md` yet — unlike `year.md`, no
//! driver plays its thirteen sittings by hand. What is under test here is
//! one lock in isolation: December's everyday listing of people says how
//! many an ordinary browse left out, not only which ones. The room is
//! furnished for real and the archival is made through the surface, the way
//! a sitting would make it — the lock is then read straight off the shipped
//! document and run against that state.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed, Outcome, boundary, boundary_names};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// The document a run is driven by, read from the root of the workspace.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::VAULT_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// Every lock the vault registers, run against the room as it stands and
/// against the readings the run took as it went — the same shape
/// `year_room.rs`'s own `judge_all` uses, and for the same reason: a lock
/// scoped to one phase's window has nothing to read without the boundaries.
async fn judge_all(room: &Surface, boundaries: &[Boundary]) -> Vec<Outcome> {
    let seen = Observed { room, boundaries };
    let checks = expectations::for_playbook(expectations::VAULT_ROOM).expect("the vault asserts");
    let mut outcomes = Vec::new();
    for check in checks {
        outcomes.push(check.check(&seen).await);
    }
    assert!(
        !outcomes.is_empty(),
        "the vault registered no locks, so a run would report a pass over an empty list",
    );
    outcomes
}

/// What the run would say, so a failure names the lock that failed.
fn saying(outcomes: &[Outcome]) -> String {
    outcomes
        .iter()
        .enumerate()
        .map(|(at, o)| format!("\n  {at:>2} [{}] {}", o.held, o.saying))
        .collect()
}

/// **The readings a run takes, with nothing done between any of them.**
///
/// One boundary before the first phase and one after every phase, exactly
/// the sequence `year_room.rs`'s `work_the_year` produces when a phase is
/// skipped — copied rather than reached for through that function, because
/// the year's own version also carries its sabotage variants, which have no
/// vault equivalent yet and would be a shape this file never asked for.
async fn unworked_boundaries(room: &Surface, vault: &Playbook) -> Vec<Boundary> {
    let named = boundary_names(vault);
    let mut boundaries = vec![boundary(room, &named[0]).await];
    for at in 0..vault.phases.len() {
        boundaries.push(boundary(room, &named[at + 1]).await);
    }
    boundaries
}

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `bike_room.rs` opens its rooms with.
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

/// **December's counted listing, read straight off the shipped document.**
///
/// Matched by its own sentence rather than by position, so a lock added
/// above it in the same phase does not silently start pointing this case at
/// the wrong one.
fn the_counted_listing_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("has no way to tell an empty vault from one quietly missing a name")
        })
        .expect("the vault ships the everyday-listing count lock")
}

async fn archive(surface: &Surface, sid: &str, handle: &str, reason: &str) {
    surface
        .must(
            "archive_entity",
            json!({"handle": handle, "reason": reason, "sid": sid}),
        )
        .await
        .unwrap_or_else(|e| panic!("{handle} did not archive: {e:#}"));
}

/// **The negative this rests on: nobody archived, so the lock must red.**
///
/// Hugo is furniture and stands by default — an everyday listing finds him,
/// which fails both the presence half of the lock and, since he was never
/// excluded, the count half too.
#[tokio::test]
async fn the_count_lock_reds_when_nobody_was_archived() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_counted_listing_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "a room nobody touched held the count lock: {}",
        outcome.saying,
    );
}

/// **The second negative, isolated from the first: the exclusion is right
/// and the COUNT alone is wrong.**
///
/// Hugo is archived, so the presence half of the lock is satisfied on its
/// own — Hugo is out, Gayle is still in. A second person is archived beside
/// him, so the everyday listing now excludes two rather than one. A lock
/// that held here would be proving nothing about the count it names.
#[tokio::test]
async fn the_count_lock_reds_when_the_count_alone_is_wrong() {
    let (_room, surface, sid) = furnished().await;
    archive(&surface, &sid, "person:hugo", "not real to me").await;
    archive(&surface, &sid, "person:gene", "not relevant to this test").await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_counted_listing_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "the everyday listing excluded two rather than one, and the count lock held anyway: {}",
        outcome.saying,
    );
}

/// **The positive: exactly Hugo archived, and the lock holds.**
#[tokio::test]
async fn the_count_lock_holds_once_exactly_hugo_is_archived() {
    let (_room, surface, sid) = furnished().await;
    archive(&surface, &sid, "person:hugo", "not real to me").await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_counted_listing_lock().check(&seen).await;
    assert!(
        outcome.held,
        "exactly one archival (Hugo) did not satisfy the count lock: {}",
        outcome.saying,
    );
}

/// **A vault nobody worked in fails every lock.**
///
/// The room is furnished with its own cast and a brief, so nothing any lock
/// claims is true of it. This is the half of the blanket driver pair
/// `year_room.rs`, `bike_room.rs`, `loop_room.rs` and `handover_room.rs`
/// already carry and `vault.md` has never had — see this file's own module
/// doc. Landed on its own, ahead of the worked half: a wall of green from the
/// worked side alone would be indistinguishable from a suite of locks that
/// cannot fail, which is the exact condition this test exists to rule out.
#[tokio::test]
async fn a_vault_nobody_worked_in_fails_every_lock() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = unworked_boundaries(&surface, &room_document()).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a furnished vault nobody worked in held a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **The driver above proven discriminating, not merely quiet.**
///
/// `a_vault_nobody_worked_in_fails_every_lock` reads as meaningful only if
/// `judge_all`/`unworked_boundaries` can ever report a lock held at all — a
/// bug that wired the boundaries wrong, or a check that always reads false,
/// would pass that test for a reason with nothing to do with the room. This
/// archives Hugo with a reason — the one piece of real work already proven
/// against December's count lock by `the_count_lock_holds_once_exactly_hugo_is_archived`
/// — then runs it back through THIS file's own driver: populated boundaries
/// and all 67 locks, not the empty boundary list and single lock the older
/// case checks directly.
///
/// **A first draft of this case expected exactly one lock to move and was
/// wrong** — run and read, not assumed: archiving Hugo with a reason also
/// satisfies December's "Hugo carries no archived reason" lock and its
/// "an ordinary browse... still turns up Hugo" lock, both correctly, since
/// both ask about the very same act. Three locks share Phase 13's archival
/// story; the assertion now names all three rather than forcing a count
/// the room's own design does not hold to.
#[tokio::test]
async fn the_driver_moves_exactly_the_locks_the_real_work_satisfies() {
    let (_room, surface, sid) = furnished().await;
    archive(&surface, &sid, "person:hugo", "not real to me").await;
    let boundaries = unworked_boundaries(&surface, &room_document()).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    let held: Vec<&str> = outcomes
        .iter()
        .filter(|o| o.held)
        .map(|o| o.name.as_str())
        .collect();
    let expected = [
        "Hugo carries no archived reason",
        "still turns up Hugo after he was taken out",
        "has no way to tell an empty vault from one quietly missing a name",
    ];
    for fragment in expected {
        assert!(
            held.iter().any(|name| name.contains(fragment)),
            "archiving Hugo did not move the lock naming {fragment:?}: {}",
            saying(&outcomes),
        );
    }
    assert_eq!(
        held.len(),
        expected.len(),
        "archiving Hugo moved a lock beyond the three Phase 13 names about that act: {}",
        saying(&outcomes),
    );
}

//! **Five of vault.md's locks, held against a real room, before and after
//! archival — no live model.**
//!
//! Paid run 23 found five locks unsatisfiable by construction: a lock is
//! evaluated once, after every phase, against the final world
//! (`jojobot_exercise::run::go`), and two room patterns collide with that.
//! The kitchen-floor's `status` moves considering → doing → done on the
//! room's own later phases, so a lock reading the folded value can never
//! hold at final evaluation for anything but `done`. And the piano's loop
//! is archived outright in the room's last phase, which takes an archived
//! entity out of any kind-scoped browse — the same exclusion
//! `list_entities` has always had — so a kind-scoped lock cannot see a
//! check-in the piano genuinely held all year.
//!
//! The fix reads the KEY'S OWN HISTORY for the kitchen floor (a write that
//! happened is never un-happened) and the LOOP'S OWN HANDLE for the piano
//! (naming it directly skips the kind-scoped browse's archived filter
//! entirely). This file replays, by hand through the surface, exactly the
//! writes a correctly-behaved December-13 sitting leaves, and checks the
//! five rewritten locks against a real room — then breaks one write at a
//! time and checks the matching lock alone reddens.

use jojobot_exercise::expectations;
use jojobot_exercise::lock::{Lock, locks_of};
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished the way a run furnishes it, and a handle to write into
/// it as the occupant would.
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

/// A call the occupant would make.
async fn as_the_occupant(
    room: &Surface,
    sid: &str,
    verb: &str,
    mut args: serde_json::Value,
) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

/// One of vault.md's own locks, found by a distinctive fragment of its
/// authored `say` sentence — the room's document is the primary source, so
/// this reads the shipped file rather than re-typing the query here.
fn lock_named(fragment: &str) -> Lock {
    let mut found: Vec<Lock> = locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .filter(|l| l.name().contains(fragment))
        .collect();
    match found.len() {
        1 => found.remove(0),
        0 => panic!("no lock in vault.md carries {fragment:?}"),
        _ => panic!("more than one lock in vault.md carries {fragment:?}, name it more precisely"),
    }
}

async fn held(room: &Surface, lock: &Lock) -> Outcome {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    lock.check(&seen).await
}

async fn write_kitchen_floor_status(surface: &Surface, sid: &str, status: &str) {
    as_the_occupant(
        surface,
        sid,
        "capture",
        json!({
            "subject": "project:kitchen-floor",
            "content": format!("the kitchen floor is {status}"),
            "provenance": "testimony",
            "fields": {"status": status},
        }),
    )
    .await;
}

/// **January's and April's kitchen-floor locks hold once the project
/// finishes moving.** The floor is the one project the room moves this
/// year — considering in January, doing in April, done in July — and a
/// lock reading the key's own history sees all three regardless of order.
#[tokio::test]
async fn the_kitchen_floor_status_locks_hold_once_the_project_finishes_moving() {
    let (_room, surface, sid) = furnished().await;
    for status in ["considering", "doing", "done"] {
        write_kitchen_floor_status(&surface, &sid, status).await;
    }

    for fragment in [
        "does not carry a considering write",
        "does not carry a doing write",
    ] {
        let outcome = held(&surface, &lock_named(fragment)).await;
        assert!(outcome.held, "{fragment}: {}", outcome.saying);
    }
}

/// **The paired bar.** The doing write is skipped entirely; the doing lock
/// must redden alone while the considering lock — an independent write —
/// stays held. Proves the rewrite reads what actually happened rather than
/// holding regardless.
#[tokio::test]
async fn the_kitchen_floor_doing_lock_reddens_when_doing_was_never_written() {
    let (_room, surface, sid) = furnished().await;
    for status in ["considering", "done"] {
        write_kitchen_floor_status(&surface, &sid, status).await;
    }

    let considering = held(&surface, &lock_named("does not carry a considering write")).await;
    let doing = held(&surface, &lock_named("does not carry a doing write")).await;
    assert!(
        considering.held,
        "the independent control reddened too, so this proves nothing about the doing lock \
         specifically: {}",
        considering.saying,
    );
    assert!(
        !doing.held,
        "the doing lock held with no doing write ever made, so it measures nothing: {}",
        doing.saying,
    );
}

async fn write_piano_check_in(surface: &Surface, sid: &str, date: &str, said: &str) {
    as_the_occupant(
        surface,
        sid,
        "capture",
        json!({
            "subject": "rhythm:sit-at-the-piano",
            "content": said,
            "provenance": "testimony",
            "fields": {"last_check_in": date},
        }),
    )
    .await;
}

async fn drop_the_piano_loop(surface: &Surface, sid: &str) {
    as_the_occupant(
        surface,
        sid,
        "archive_entity",
        json!({
            "handle": "rhythm:sit-at-the-piano",
            "reason": "the operator asked to stop reminding altogether",
        }),
    )
    .await;
}

const APRIL_FRAGMENT: &str = "does not carry the twelfth";
const AUGUST_FRAGMENT: &str = "does not carry the eleventh";
const PARTY_FRAGMENT: &str = "does not carry the day of the party";

/// **All three piano locks hold after the loop is archived.** Two check-ins
/// logged over the year, a third logged from the party the same sitting
/// that drops the loop, then the archival itself — exactly what a
/// correctly-behaved December-13 sitting leaves. Each lock now names the
/// piano's own handle, which archival does not remove from view.
#[tokio::test]
async fn the_piano_locks_hold_after_the_loop_is_archived() {
    let (_room, surface, sid) = furnished().await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-04-12",
        "sat at the piano on the twelfth",
    )
    .await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-08-11",
        "sat at the piano on the eleventh",
    )
    .await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-12-05",
        "the party on 2026-12-05 was a turn at the piano too",
    )
    .await;
    drop_the_piano_loop(&surface, &sid).await;

    for fragment in [APRIL_FRAGMENT, AUGUST_FRAGMENT, PARTY_FRAGMENT] {
        let outcome = held(&surface, &lock_named(fragment)).await;
        assert!(outcome.held, "{fragment}: {}", outcome.saying);
    }
}

/// **The paired bar, April.** The April check-in is the one skipped; only
/// April's lock may redden.
#[tokio::test]
async fn the_april_piano_lock_reddens_when_the_april_check_in_was_never_logged() {
    let (_room, surface, sid) = furnished().await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-08-11",
        "sat at the piano on the eleventh",
    )
    .await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-12-05",
        "the party on 2026-12-05 was a turn at the piano too",
    )
    .await;
    drop_the_piano_loop(&surface, &sid).await;

    let april = held(&surface, &lock_named(APRIL_FRAGMENT)).await;
    let august = held(&surface, &lock_named(AUGUST_FRAGMENT)).await;
    let party = held(&surface, &lock_named(PARTY_FRAGMENT)).await;
    assert!(
        !april.held,
        "the april lock held with no april check-in ever logged: {}",
        april.saying,
    );
    assert!(
        august.held,
        "an independent control reddened too: {}",
        august.saying
    );
    assert!(
        party.held,
        "an independent control reddened too: {}",
        party.saying
    );
}

/// **The paired bar, August.**
#[tokio::test]
async fn the_august_piano_lock_reddens_when_the_august_check_in_was_never_logged() {
    let (_room, surface, sid) = furnished().await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-04-12",
        "sat at the piano on the twelfth",
    )
    .await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-12-05",
        "the party on 2026-12-05 was a turn at the piano too",
    )
    .await;
    drop_the_piano_loop(&surface, &sid).await;

    let april = held(&surface, &lock_named(APRIL_FRAGMENT)).await;
    let august = held(&surface, &lock_named(AUGUST_FRAGMENT)).await;
    let party = held(&surface, &lock_named(PARTY_FRAGMENT)).await;
    assert!(
        april.held,
        "an independent control reddened too: {}",
        april.saying
    );
    assert!(
        !august.held,
        "the august lock held with no august check-in ever logged: {}",
        august.saying,
    );
    assert!(
        party.held,
        "an independent control reddened too: {}",
        party.saying
    );
}

/// **The paired bar, the party.** Here the party check-in is the one
/// skipped — the loop is still archived on schedule, so this also proves
/// the party lock's hold above was not merely "the loop exists".
#[tokio::test]
async fn the_party_piano_lock_reddens_when_the_party_check_in_was_never_logged() {
    let (_room, surface, sid) = furnished().await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-04-12",
        "sat at the piano on the twelfth",
    )
    .await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-08-11",
        "sat at the piano on the eleventh",
    )
    .await;
    drop_the_piano_loop(&surface, &sid).await;

    let april = held(&surface, &lock_named(APRIL_FRAGMENT)).await;
    let august = held(&surface, &lock_named(AUGUST_FRAGMENT)).await;
    let party = held(&surface, &lock_named(PARTY_FRAGMENT)).await;
    assert!(
        april.held,
        "an independent control reddened too: {}",
        april.saying
    );
    assert!(
        august.held,
        "an independent control reddened too: {}",
        august.saying
    );
    assert!(
        !party.held,
        "the party lock held with no party check-in ever logged: {}",
        party.saying,
    );
}

/// **The bug itself, shown directly.** The OLD query these locks used to
/// run (`kind: "rhythm"`) cannot see the piano at all once it is archived,
/// even though the piano's own handle can see the same check-in the whole
/// time — proving the room's original locks were unsatisfiable by
/// construction rather than by an accident of wording.
#[tokio::test]
async fn a_kind_scoped_browse_cannot_see_the_archived_piano_but_its_own_handle_can() {
    let (_room, surface, sid) = furnished().await;
    write_piano_check_in(
        &surface,
        &sid,
        "2026-04-12",
        "sat at the piano on the twelfth",
    )
    .await;
    drop_the_piano_loop(&surface, &sid).await;

    let by_kind = surface
        .call(
            "recall",
            json!({"kind": "rhythm", "history": "last_check_in"}),
        )
        .await;
    assert!(
        !by_kind.contains("2026-04-12"),
        "a kind-scoped browse found the archived piano's history, so the room's original lock \
         was never actually broken: {by_kind}",
    );

    let by_handle = surface
        .call(
            "recall",
            json!({"subject": "rhythm:sit-at-the-piano", "history": "last_check_in"}),
        )
        .await;
    assert!(
        by_handle.contains("2026-04-12"),
        "the piano's own handle could not see its own check-in after archival: {by_handle}",
    );
}

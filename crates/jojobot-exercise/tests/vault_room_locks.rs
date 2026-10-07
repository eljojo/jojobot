//! **Five of vault.md's locks, held against a real room, before and after
//! archival — no live model.**
//!
//! Paid run 23 found five locks unsatisfiable by construction: a lock is
//! evaluated once, after every phase, against the final world
//! (`jojobot_exercise::run::go`), and two room patterns collide with that.
//! The kitchen-floor's `status` moves next → now → done on the
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
/// year — next in January, now in April, done in July — and a
/// lock reading the key's own history sees all three regardless of order.
#[tokio::test]
async fn the_kitchen_floor_status_locks_hold_once_the_project_finishes_moving() {
    let (_room, surface, sid) = furnished().await;
    for status in ["next", "now", "done"] {
        write_kitchen_floor_status(&surface, &sid, status).await;
    }

    for fragment in ["does not carry a next write", "does not carry a now write"] {
        let outcome = held(&surface, &lock_named(fragment)).await;
        assert!(outcome.held, "{fragment}: {}", outcome.saying);
    }
}

/// **The paired bar.** The now write is skipped entirely; the now lock
/// must redden alone while the next lock — an independent write —
/// stays held. Proves the rewrite reads what actually happened rather than
/// holding regardless.
#[tokio::test]
async fn the_kitchen_floor_doing_lock_reddens_when_doing_was_never_written() {
    let (_room, surface, sid) = furnished().await;
    for status in ["next", "done"] {
        write_kitchen_floor_status(&surface, &sid, status).await;
    }

    let next = held(&surface, &lock_named("does not carry a next write")).await;
    let now = held(&surface, &lock_named("does not carry a now write")).await;
    assert!(
        next.held,
        "the independent control reddened too, so this proves nothing about the now lock \
         specifically: {}",
        next.saying,
    );
    assert!(
        !now.held,
        "the now lock held with no now write ever made, so it measures nothing: {}",
        now.saying,
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
        "played for about an hour at Tina's party on 2026-12-05",
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
        "played for about an hour at Tina's party on 2026-12-05",
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
        "played for about an hour at Tina's party on 2026-12-05",
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

// ── The four locks about what only works with one machine ───────────────────
//
// A field whose value is another thing's handle is a link, whatever its key,
// so these locks ask from the machine's end — what points here — and do not
// care by which route the pointing was written.

/// **The ways a sitting can say a thing only works with a machine.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// An edge from the thing to the machine.
    Edge,
    /// The machine's handle written into the claim's words.
    Mention,
    /// The machine's handle in the claim's reference list.
    Ref,
    /// The machine's handle as the value of a key, whatever the key.
    Field,
    /// The words alone: "only works with the server".
    Prose,
}

/// **What a claim carries to say it is for `machine`, by `route`.**
fn saying_for(route: Route, machine: &str, words: &str) -> serde_json::Value {
    let mut said = json!({
        "content": words,
        "provenance": "testimony",
    });
    match route {
        Route::Edge => {
            said["shape"] = json!("connection");
            said["object"] = json!(machine);
        }
        Route::Mention => said["content"] = json!(format!("{words} @{machine}")),
        Route::Ref => said["refs"] = json!([machine]),
        Route::Field => said["fields"] = json!({"for": machine}),
        Route::Prose => {}
    }
    said
}

async fn note_on(
    surface: &Surface,
    sid: &str,
    subject: &str,
    day: &str,
    mut said: serde_json::Value,
) {
    said["subject"] = json!(subject);
    said["recorded_at"] = json!(day);
    let answer = as_the_occupant(surface, sid, "capture", said).await;
    assert!(
        !answer.contains("\"status\":\"blocked\""),
        "the sitting's own write was refused: {answer}"
    );
}

/// **April's drive, June's dock and October's note**, the way the year writes
/// them, with the link written by `route`. `note_on_the_dock` is the wrong
/// October: a note put on everything that is for a machine.
async fn the_year_of_the_two_machines(
    surface: &Surface,
    sid: &str,
    route: Route,
    note_on_the_dock: bool,
) {
    for (handle, name) in [
        ("backup-drive", "The Backup Drive"),
        ("laptop-dock", "The Laptop Dock"),
    ] {
        let added = as_the_occupant(
            surface,
            sid,
            "add_entity",
            json!({"kind": "thing", "handle": handle, "name": name, "source": "the operator"}),
        )
        .await;
        assert!(!added.contains("\"status\":\"blocked\""), "{added}");
    }
    note_on(
        surface,
        sid,
        "thing:backup-drive",
        "2026-04-19",
        saying_for(route, "machine:omicron", "formatted for the server"),
    )
    .await;
    note_on(
        surface,
        sid,
        "thing:laptop-dock",
        "2026-06-14",
        saying_for(route, "machine:theta", "only works with the laptop"),
    )
    .await;
    note_on(
        surface,
        sid,
        "thing:backup-drive",
        "2026-10-11",
        json!({"content": "goes with the server when it goes", "provenance": "testimony"}),
    )
    .await;
    if note_on_the_dock {
        note_on(
            surface,
            sid,
            "thing:laptop-dock",
            "2026-10-11",
            json!({"content": "packed up with everything else", "provenance": "testimony"}),
        )
        .await;
    }
}

/// The four locks, in the order the year asks them.
const FOR_FRAGMENTS: [&str; 4] = [
    "no thing is on record as being for Omicron",
    "no thing is on record as being for Theta",
    "the thing that only works with Omicron carries no note",
    "the thing that only works with Theta was either lost",
];

async fn the_four_for_locks(surface: &Surface) -> Vec<bool> {
    let mut held_all = Vec::new();
    for fragment in FOR_FRAGMENTS {
        held_all.push(held(surface, &lock_named(fragment)).await.held);
    }
    held_all
}

/// **Every route that links the thing to the machine holds all four locks.**
/// The locks read what points at the machine, so an edge, a mention, a
/// reference and a key holding the handle each pass.
#[tokio::test]
async fn a_thing_linked_to_the_machine_by_an_edge_holds_the_for_locks() {
    let (_room, surface, sid) = furnished().await;
    the_year_of_the_two_machines(&surface, &sid, Route::Edge, false).await;
    assert_eq!(the_four_for_locks(&surface).await, vec![true; 4]);
}

#[tokio::test]
async fn a_thing_linked_to_the_machine_by_a_mention_holds_the_for_locks() {
    let (_room, surface, sid) = furnished().await;
    the_year_of_the_two_machines(&surface, &sid, Route::Mention, false).await;
    assert_eq!(the_four_for_locks(&surface).await, vec![true; 4]);
}

#[tokio::test]
async fn a_thing_linked_to_the_machine_by_a_reference_holds_the_for_locks() {
    let (_room, surface, sid) = furnished().await;
    the_year_of_the_two_machines(&surface, &sid, Route::Ref, false).await;
    assert_eq!(the_four_for_locks(&surface).await, vec![true; 4]);
}

#[tokio::test]
async fn a_thing_linked_to_the_machine_by_a_key_holding_the_handle_holds_the_for_locks() {
    let (_room, surface, sid) = furnished().await;
    the_year_of_the_two_machines(&surface, &sid, Route::Field, false).await;
    assert_eq!(the_four_for_locks(&surface).await, vec![true; 4]);
}

/// **A sitting that wrote only prose fails all four**: nothing points at
/// either machine, so a later question about what goes with the server cannot
/// find the drive.
#[tokio::test]
async fn a_thing_that_only_says_the_server_in_words_fails_the_for_locks() {
    let (_room, surface, sid) = furnished().await;
    the_year_of_the_two_machines(&surface, &sid, Route::Prose, false).await;
    assert_eq!(the_four_for_locks(&surface).await, vec![false; 4]);
}

/// **A note put on everything that is for a machine reddens the Theta lock
/// alone.** The route that links is right and the October sitting selected on
/// the link instead of on the machine, so the dock was given a note it was
/// not meant to get.
#[tokio::test]
async fn a_note_on_the_dock_too_reddens_only_the_theta_selection_lock() {
    let (_room, surface, sid) = furnished().await;
    the_year_of_the_two_machines(&surface, &sid, Route::Edge, true).await;
    assert_eq!(
        the_four_for_locks(&surface).await,
        vec![true, true, true, false]
    );
}

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

/// **A session for one sitting, in the day that sitting is in** — the same
/// shape `year_room.rs`'s own `sitting` takes, copied rather than reinvented:
/// each phase is a run of its own, on its own day, and a boot meeting a run
/// still in flight hands back the resume-or-new choice rather than a handle.
async fn sitting(room: &Surface, day: &str) -> String {
    let booted = room
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day}),
        )
        .await
        .expect("the shipped identity boots");
    if let Some(sid) = booted["session"]["sid"].as_str() {
        return sid.to_string();
    }
    let offered = booted["session"]["choices"]
        .as_array()
        .unwrap_or_else(|| panic!("the boot on {day} handed back neither a handle nor a choice"));
    let working: Vec<&serde_json::Value> = offered
        .iter()
        .filter(|run| run["state"] == "active")
        .collect();
    assert!(
        working.is_empty(),
        "the boot on {day} was offered a run still ACTIVE, so an earlier sitting did not go \
         quiet in this run's frame: {working:?}",
    );
    let answered = room
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day, "resume": "new"}),
        )
        .await
        .expect("the boot answering the choice is ok");
    answered["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("answering `new` on {day} handed back no handle: {answered}"))
        .to_string()
}

/// A call the occupant would make.
async fn did(room: &Surface, sid: &str, verb: &str, mut args: serde_json::Value) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

/// The address of the record on `subject` whose content carries `needle` —
/// the same helper `year_room.rs` uses to find a furniture record's own
/// address before editing it.
async fn address_of(room: &Surface, subject: &str, needle: &str) -> String {
    let read = room
        .call("recall", json!({"subject": subject, "facts": true}))
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&read).expect("the read is json");
    parsed["objects"][0]["facts"]
        .as_array()
        .expect("the records behind the fields")
        .iter()
        .find(|fact| {
            fact["content"]
                .as_str()
                .is_some_and(|said| said.contains(needle))
        })
        .and_then(|fact| fact["address"].as_str())
        .unwrap_or_else(|| panic!("no record on {subject} says {needle:?}: {read}"))
        .to_string()
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

/// January: the vault moves in. New windows and habits, everything under the
/// operator's own five words.
async fn january(room: &Surface, sid: &str) {
    did(room, sid, "read_mailbox", json!({})).await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "machine:theta", "content": "bought 2026-01-10 from Globex, two years of cover from that day",
               "provenance": "testimony", "fields": {"runs_out": "2028-01-10"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:glasses", "content": "there is a spare pair in the hall drawer",
               "provenance": "testimony", "fields": {"spare": "hall drawer"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "considering the flat-pack kit, or building one",
               "provenance": "testimony", "fields": {"status": "considering"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:kitchen-floor", "content": "considering",
               "provenance": "testimony", "fields": {"status": "considering"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "38 minutes on 2026-01-08, my way to work",
               "provenance": "testimony", "fields": {"minutes": "38"}}),
    )
    .await;
}

/// February: a desk, a storm and a set of keys.
async fn february(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "thing", "handle": "standing-desk", "name": "The Standing Desk", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:standing-desk", "content": "came on the 6th; thirty days to send it back if I don't get on with it",
               "provenance": "testimony", "fields": {"runs_out": "2026-03-08"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:wonder-wharf", "content": "the bridge is shut so I went in by the wharf road Friday the 6th — 52 minutes, never again",
               "provenance": "testimony", "fields": {"minutes": "52"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "storm week: the bridge is shut for repairs until the 20th",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:house-keys", "content": "Teddy has had a set since the lockout last year",
               "provenance": "testimony", "fields": {"spare": "Teddy"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "still can't decide; the kit's on offer again",
               "provenance": "testimony", "fields": {"status": "considering"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:swim", "content": "swam Tuesday the 3rd",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-02-03"}),
    )
    .await;
}

/// March: boots and a tablet. The desk is deliberately left untouched.
async fn march(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "thing", "handle": "boots", "name": "The Boots", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:boots", "content": "from Costington's on the 8th, a year's guarantee on the soles",
               "provenance": "testimony", "fields": {"runs_out": "2027-03-08"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "pet:snowball", "content": "the vet has put her on a tablet twice a day from now on, for good — the cattery on Ocean Avenue will not board a cat on it",
               "provenance": "testimony"}),
    )
    .await;
}

/// April: a yes, a drive and a floor.
async fn april(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:wagstaff-fair", "content": "Linda roped me into running the cake stall on 17 October and I said yes — whoever runs it has to hold the food-handling certificate, which I don't",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "thing", "handle": "backup-drive", "name": "The Backup Drive", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:backup-drive", "content": "bought Friday the 17th, formatted for the server — the laptop cannot read it",
               "provenance": "testimony", "fields": {"for": "machine:omicron"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:kitchen-floor", "content": "happening — Gene's mate is booked for June",
               "provenance": "testimony", "fields": {"status": "doing"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "41 minutes on Tuesday the 14th",
               "provenance": "testimony", "fields": {"minutes": "41"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:sit-at-the-piano", "content": "sat down Sunday the 12th",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-04-12"}),
    )
    .await;
}

/// May: the furnace, and a man in the hall.
async fn may(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:the-furnace", "content": "Mr. Plow serviced it on the 6th — one free service a year, but it has to be used before 1 February or it lapses",
               "provenance": "testimony", "fields": {"runs_out": "2027-02-01"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:teddy", "content": "came round to look at the furnace and had to stand in the hall the whole time — badly allergic to cats",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "went round it again with Linda; nothing decided",
               "provenance": "testimony", "fields": {"status": "considering"}}),
    )
    .await;
}

/// June: lunch with Gayle.
async fn june(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:quahog-community-college", "content": "runs the kitchen safety course Gayle just finished — six Tuesday evenings, a new intake starts the first Tuesday of every month",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "thing", "handle": "laptop-dock", "name": "The Laptop Dock", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:laptop-dock", "content": "bought Thursday the 11th — only works with Theta",
               "provenance": "testimony", "fields": {"for": "machine:theta"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:kitchen-floor", "content": "half done",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:call-gayle", "content": "the usual call, Tuesday the 9th",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-06-09"}),
    )
    .await;
}

/// July: the floor is done.
async fn july(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:kitchen-floor", "content": "done",
               "provenance": "testimony", "fields": {"status": "done"}}),
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "thing", "handle": "mobile-phone", "name": "The Mobile Phone", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:mobile-phone", "content": "new phone on the 10th, from Globex again, two years of cover",
               "provenance": "testimony", "fields": {"runs_out": "2028-07-10"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "36 minutes on Tuesday the 14th",
               "provenance": "testimony", "fields": {"minutes": "36"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:swim", "content": "swam that evening, Tuesday the 14th",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-07-14"}),
    )
    .await;
}

/// August: the pool shuts.
async fn august(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:wagstaff-pool", "content": "shut on the 1st — the roof — not reopening before 1 April 2027",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "Louise says just buy the kit; still torn",
               "provenance": "testimony", "fields": {"status": "considering"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:sit-at-the-piano", "content": "sat down Tuesday the 11th",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-08-11"}),
    )
    .await;
}

/// September: a trip booked and a month looked ahead to.
async fn september(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:wagstaff-fair", "content": "still need the food-handling certificate — offered by @org:quahog-community-college",
               "provenance": "testimony", "shape": "about", "object": "org:quahog-community-college",
               "fields": {"status": "active"}}),
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "event", "handle": "capital-city-trip", "name": "Capital City trip", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:capital-city-trip", "content": "Linda and I are going to Capital City, booked yesterday",
               "provenance": "testimony", "fields": {"leaves_on": "2026-11-12", "returns_on": "2026-11-22"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "40 minutes on Wednesday the 9th",
               "provenance": "testimony", "fields": {"minutes": "40"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:call-gayle", "content": "spoke to Gayle Tuesday the 8th",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-09-08"}),
    )
    .await;
}

/// October: a rule of thumb, a server retired.
async fn october(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:wonder-wharf", "content": "Louise reckons it's quicker now — unsettling this, it might really be one bad morning rather than a rule",
               "provenance": "testimony", "standing": "open"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:backup-drive", "content": "retiring the server — Teddy is taking it at the end of the month, so noting again this only goes with it",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:call-gayle", "content": "spoke to Gayle Tuesday the 29th",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-09-29"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:quahog-community-college", "content": "started the October intake anyway, Tuesday evenings — too late for the stall but I want the certificate regardless",
               "provenance": "testimony", "happened_at": "2026-10-06", "happened_through": "2026-11-10"}),
    )
    .await;
}

/// November: four days before the trip.
async fn november(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:capital-city-trip", "content": "four days out — need someone to feed @pet:snowball twice a day since Teddy can't do it, allergic",
               "provenance": "testimony", "shape": "about", "object": "pet:snowball",
               "fields": {"status": "active"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:phone-charger", "content": "not sure whether there is a spare — cannot tell",
               "provenance": "testimony", "standing": "open"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:glasses", "content": "checked again — the spare is still in the hall drawer",
               "provenance": "inference", "fields": {"spare": "hall drawer"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:house-keys", "content": "the spare set is with @person:teddy",
               "provenance": "testimony", "shape": "connection", "object": "person:teddy",
               "fields": {"status": "active"}}),
    )
    .await;
}

/// December: a party and a horizon.
async fn december(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:boots", "content": "the guarantee closes inside the horizon — need to look at these before the end of March",
               "provenance": "testimony"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:the-furnace", "content": "the free service also lapses inside the horizon",
               "provenance": "testimony"}),
    )
    .await;
}

/// Later December: what went quiet and why.
async fn later_december(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:sit-at-the-piano", "content": "played for about an hour at Tina's party",
               "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-12-05"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:swim", "content": "the pool won't reopen before then, so leaving this until it does",
               "provenance": "testimony", "fields": {"resumes_on": "2027-04-01"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "still going round on this — giving myself a deadline to decide",
               "provenance": "testimony", "fields": {"decide_by": "2027-01-15"}}),
    )
    .await;
    // **Dropping a schedule is `update_fact`'s `clear_fields`, not a fresh
    // capture with a null field** — `capture`'s own `fields` argument takes
    // `String` values, so a null is a schema error rather than a clear.
    // The address has to be looked up: the piano's cadence was set by the
    // room's own furniture record, never by this driver.
    // **Dropping a loop means carrying NONE of the schedule interface, not
    // clearing one key of it.** `Rhythms::interface` matches on any of
    // `cadence_days`/`advances_from`/`counts_from`; clearing only the first
    // leaves the thing "half-built" — matched, but `schedule_of` fails to
    // read it — which reads as `Due::Unreadable`, loudly OVERDUE, worse than
    // before. This is the exact trap the room's own December 15 lock names.
    // All three have to go together for the carrier to stop matching at all
    // and fall through to `Due::Never`.
    let piano_address =
        address_of(room, "rhythm:sit-at-the-piano", "at least once a fortnight").await;
    did(
        room,
        sid,
        "update_fact",
        json!({"address": piano_address, "clear_fields": ["cadence_days", "advances_from", "counts_from"]}),
    )
    .await;
    archive(room, sid, "person:hugo", "not real to me").await;
}

/// **The vault worked sitting by sitting, taking the readings a run takes.**
///
/// One driver for the whole year, the same shape `year_room.rs`'s own
/// `work_the_year` takes for its worked case — but with no sabotage variants,
/// because nothing here has any yet: `unworked_boundaries` already proves
/// every lock reds on a room nobody touched, so this is the other half of
/// that same pair, not a second copy of the mechanism with wrong turns
/// wired in.
/// One phase's own worked function, boxed so all thirteen — each `async fn`
/// with its own distinct anonymous type — can sit in one array.
type PhaseFn = for<'a> fn(
    &'a Surface,
    &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>;

async fn worked_the_vault(room: &Surface, vault: &Playbook) -> Vec<Boundary> {
    let named = boundary_names(vault);
    let phases: [(&str, PhaseFn); 13] = [
        ("2026-01-12", |r, s| Box::pin(january(r, s))),
        ("2026-02-08", |r, s| Box::pin(february(r, s))),
        ("2026-03-15", |r, s| Box::pin(march(r, s))),
        ("2026-04-19", |r, s| Box::pin(april(r, s))),
        ("2026-05-10", |r, s| Box::pin(may(r, s))),
        ("2026-06-14", |r, s| Box::pin(june(r, s))),
        ("2026-07-19", |r, s| Box::pin(july(r, s))),
        ("2026-08-16", |r, s| Box::pin(august(r, s))),
        ("2026-09-13", |r, s| Box::pin(september(r, s))),
        ("2026-10-11", |r, s| Box::pin(october(r, s))),
        ("2026-11-08", |r, s| Box::pin(november(r, s))),
        ("2026-12-06", |r, s| Box::pin(december(r, s))),
        ("2026-12-13", |r, s| Box::pin(later_december(r, s))),
    ];
    let mut boundaries = vec![boundary(room, &named[0]).await];
    for (at, (day, work)) in phases.iter().enumerate() {
        let sid = sitting(room, day).await;
        work(room, &sid).await;
        boundaries.push(boundary(room, &named[at + 1]).await);
    }
    boundaries
}

/// **THE ONE LOCK THIS DRIVER CANNOT SATISFY, NAMED RATHER THAN WORKED
/// AROUND — a finding for `pm`, not a gap in this test.**
///
/// February's own lock on `place:wonder-wharf` (`carries "provenance":
/// "testimony"`, `lacks "standing":"open"`) and October's (`carries
/// "standing":"open"`) both query the identical subject with the identical
/// shape (`{"subject": "place:wonder-wharf", "facts": true}`), and neither
/// carries a `window own-phase` annotation — `grep -n "window" rooms/
/// vault.md` finds only the English word, never the directive. Both are
/// therefore `Live`: evaluated once, against the room as it stands when the
/// WHOLE run finishes, not at their own phase's boundary.
///
/// The room's own words say what has to happen: the February claim is
/// settled testimony, and October "unsettles" it — "a claim that was
/// settled and is now open was opened by somebody" (February's comment),
/// and October's own comment says a rewrite-in-place or a fresh claim
/// beside it "both leave an open claim on the wharf." Nothing in the room
/// ever re-settles it afterwards. So by the time the run finishes — the
/// only moment either lock is ever asked — `facts: true` on wonder-wharf
/// contains an open claim, unconditionally. February's `lacks "standing":
/// "open"` cannot hold at that same moment, regardless of whether October
/// edits the original record or adds a second one: an edit still leaves
/// the live projection open, and a second record still puts the substring
/// in the same JSON blob February's query reads. Verified empirically, not
/// just reasoned: a diagnostic read after a real run showed both records
/// (`place:wonder-wharf#f1`, settled; `#f2`, open) side by side in the one
/// `facts: true` response.
///
/// This is not something this driver can fix — `vault.md`, its locks and
/// its brief are out of scope for this slice — and it is not a workaround:
/// the lock is excluded by its own exact sentence, loudly, rather than
/// silently dropped from the count.
const WONDER_WHARF_CONTRADICTS_ITS_OWN_SIBLING_LOCK: &str = "the operator's \"never again\" is on record as a guess rather than as their word, so \
     October cannot open a verdict that was never closed";

/// **The vault is solvable, and this is the case that says so — for every
/// lock but the one named above, which cannot be satisfied by any run.**
///
/// A different kind of case from `a_vault_nobody_worked_in_fails_every_lock`:
/// that one proves the locks discriminate against a store somebody put
/// nothing into. This one proves every question the vault asks CAN be
/// answered through the served surface — without it a vault whose own
/// arithmetic is wrong would be unreachable by every session that ever
/// enters it, the unworked case would still pass, and the failure would
/// surface on a paid run reading as a defect in the product.
#[tokio::test]
async fn every_lock_holds_once_the_vault_is_worked() {
    let (_room, surface) = {
        let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
            .await
            .expect("a room");
        expectations::seed_for(expectations::VAULT_ROOM)
            .expect("the room has furniture")
            .furnish(&surface)
            .await
            .expect("the room is furnished");
        (room, surface)
    };
    let boundaries = worked_the_vault(&surface, &room_document()).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        if outcome
            .saying
            .contains(WONDER_WHARF_CONTRADICTS_ITS_OWN_SIBLING_LOCK)
        {
            assert!(
                !outcome.held,
                "the wonder-wharf contradiction named above no longer reproduces — read this \
                 test's own doc comment, then read pm and see whether it was resolved: {}",
                outcome.saying,
            );
            continue;
        }
        assert!(
            outcome.held,
            "a vault worked the way it is meant to be failed a lock: {}",
            saying(&outcomes),
        );
    }
}

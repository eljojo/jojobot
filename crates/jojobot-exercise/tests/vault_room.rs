//! **The vault's archived-exclusion counter, held against a real room.**
//!
//! There is no synthetic occupant for `vault.md` yet — unlike `year.md`, no
//! driver plays its fourteen sittings by hand. What is under test here is
//! one lock in isolation: December's everyday listing of people says how
//! many an ordinary browse left out, not only which ones. The room is
//! furnished for real and the archival is made through the surface, the way
//! a sitting would make it — the lock is then read straight off the shipped
//! document and run against that state.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Expectation, Observed, Outcome, boundary, boundary_asking, boundary_names,
    phase_end_queries,
};
use jojobot_exercise::surface::Surface;
use serde_json::json;
use std::sync::LazyLock;

/// **The day this room's server runs on**, read from the room's own document
/// (the one the paid path reads) and carried to the child as its own clock. Every moment the server stamps is this day, whatever the real date is,
/// so no needle can be met by a stamp on one real day. It is a day no needle
/// of this room holds, and `no_needle_here_is_the_servers_own_day` asks that.
static SERVER_DAY: LazyLock<String> = LazyLock::new(|| {
    room_document()
        .stated_server_day()
        .unwrap_or_else(|e| panic!("{e:#}"))
        .to_string()
});

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
    let locks = expectations::for_playbook(expectations::VAULT_ROOM).expect("the vault asserts");
    let mut boundaries = vec![boundary(room, &named[0]).await];
    for at in 0..vault.phases.len() {
        let asked = phase_end_queries(&locks, &format!("Phase {}", at + 1));
        boundaries.push(boundary_asking(room, &named[at + 1], &asked).await);
    }
    boundaries
}

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `bike_room.rs` opens its rooms with.
async fn furnished() -> (Room, Surface, String) {
    let (room, surface) = Room::open_with_client_stating(
        &server_binary().expect("a jojobot binary"),
        SERVER_DAY.as_str(),
    )
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

/// **December's cross-subject ticket-deadline lock, read straight off the
/// shipped document.** Matched by its own sentence for the same reason
/// `the_counted_listing_lock` is.
fn the_ticket_deadline_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("reads as the wrong thing or as nothing at all")
        })
        .expect("the vault ships the ticket-deadline lock")
}

/// **December's Hugo-reason lock, read straight off the shipped document.**
/// Matched by its own sentence for the same reason `the_counted_listing_lock`
/// is.
fn the_hugo_reason_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("still standing in the vault as if it belonged there")
        })
        .expect("the vault ships Hugo's own archived-reason lock")
}

/// **December's overdue-count lock, read straight off the shipped document.**
/// Matched by its own sentence for the same reason `the_counted_listing_lock`
/// is.
fn the_overdue_count_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("fails to say how many of the rest it correctly left out")
        })
        .expect("the vault ships the overdue-count lock")
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

/// **The negative Hugo's own reason lock rests on: nobody archived him at
/// all, so the lock must red.**
///
/// Furniture, untouched: no `archived` object exists on the record at all,
/// so neither `carries "reason":"` nor the presence of `person:hugo` in an
/// archived answer can be satisfied.
#[tokio::test]
async fn the_hugo_reason_lock_reds_when_hugo_was_never_archived() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_hugo_reason_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "Hugo was never archived, and his own reason lock held anyway: {}",
        outcome.saying,
    );
}

/// **The positive: archived with a reason in the operator's own words, and
/// the lock holds.** The lock no longer pins those words — see this room's
/// own commit — so this also stands as the receipt that a real answer,
/// worded exactly as the operator said it, still satisfies the rewritten
/// check.
#[tokio::test]
async fn the_hugo_reason_lock_holds_once_archived_with_a_reason() {
    let (_room, surface, sid) = furnished().await;
    archive(&surface, &sid, "person:hugo", "not real to me").await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_hugo_reason_lock().check(&seen).await;
    assert!(
        outcome.held,
        "Hugo was archived with a reason, and his own reason lock still failed: {}",
        outcome.saying,
    );
}

/// **The negative this rests on: filed on the person it rode in on, and the
/// lock must red.**
///
/// The fair's ticket deadline never mentions the fair by name — the
/// operator hears it from Louise. A sitting that captures it as a fact
/// about `person:louise` rather than `event:wagstaff-fair` has made exactly
/// the mistake the overdue read cannot see past: found by key rather than
/// kind, Louise would read as owed and the fair would read as having
/// nothing due. `carries event:wagstaff-fair` fails because she is not the
/// fair, and `lacks person:louise` fails because she is exactly what came
/// back — both halves miss for the one reason this lock exists to catch.
#[tokio::test]
async fn the_ticket_deadline_lock_reds_when_filed_on_the_wrong_subject() {
    let (_room, surface, sid) = furnished().await;
    did(
        &surface,
        &sid,
        "capture",
        json!({"subject": "person:louise", "content": "mentioned tickets stop selling 10 October",
               "provenance": "testimony", "sets": {"runs_out": "2026-10-10"}}),
    )
    .await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_ticket_deadline_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "the ticket deadline filed on Louise instead of the fair held the lock anyway: {}",
        outcome.saying,
    );
}

/// **The positive: filed on the fair, and the lock holds.**
#[tokio::test]
async fn the_ticket_deadline_lock_holds_once_filed_on_the_fair() {
    let (_room, surface, sid) = furnished().await;
    did(
        &surface,
        &sid,
        "capture",
        json!({"subject": "event:wagstaff-fair", "content": "Louise mentioned tickets stop selling 10 October",
               "provenance": "testimony", "sets": {"runs_out": "2026-10-10"}}),
    )
    .await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_ticket_deadline_lock().check(&seen).await;
    assert!(
        outcome.held,
        "the ticket deadline filed on the fair did not satisfy the lock: {}",
        outcome.saying,
    );
}

/// **January's laptop lock, read straight off the shipped document.** Matched
/// by its own sentence, as the locks above are.
fn the_laptop_cover_date_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("the laptop's cover does not read as the day it ends")
        })
        .expect("the vault ships January's laptop lock")
}

/// **What January's laptop lock answers when the cover is filed as given.**
/// `None` files the cover with no `runs_out` at all.
async fn january_laptop_lock_answers(runs_out: Option<&str>) -> Outcome {
    let (_room, surface, sid) = furnished().await;
    let mut capture = json!({
        "subject": "machine:theta",
        "content": "bought 2026-01-10 from Globex, two years of cover from that day",
        "provenance": "testimony",
    });
    if let Some(day) = runs_out {
        capture["sets"] = json!({"runs_out": day});
    }
    did(&surface, &sid, "capture", capture).await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    the_laptop_cover_date_lock().check(&seen).await
}

/// **"Two years of cover from 2026-01-10" has two right end days**: the
/// anniversary, 2028-01-10, and the last day covered, 2028-01-09. A lock fails
/// on what a sitting got wrong, never on which right reading it chose.
#[tokio::test]
async fn januarys_laptop_lock_holds_for_the_anniversary_and_for_the_last_covered_day() {
    for day in ["2028-01-10", "2028-01-09"] {
        let outcome = january_laptop_lock_answers(Some(day)).await;
        assert!(
            outcome.held,
            "a laptop cover that runs out on {day} failed the lock: {}",
            outcome.saying,
        );
    }
}

/// **The same lock still fails for every reading that is wrong**: a day on
/// either side of the two right ones, a prose duration under the key, and a
/// cover with no `runs_out` at all.
#[tokio::test]
async fn januarys_laptop_lock_reds_for_any_other_day_and_for_no_date() {
    for runs_out in [
        Some("2028-01-08"),
        Some("2028-01-11"),
        Some("2027-01-10"),
        Some("two years"),
        None,
    ] {
        let outcome = january_laptop_lock_answers(runs_out).await;
        assert!(
            !outcome.held,
            "a laptop cover filed as {runs_out:?} held the lock: {}",
            outcome.saying,
        );
    }
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
/// both ask about the very same act. Three locks share Phase 14's archival
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
        "archiving Hugo moved a lock beyond the three Phase 14 names about that act: {}",
        saying(&outcomes),
    );
}

/// **How January files the laptop's cover**, which has two right end days.
#[derive(Clone, Copy)]
enum January {
    /// The anniversary of the purchase.
    OnTheAnniversary,
    /// The last day the cover holds.
    OnTheLastCoveredDay,
}

/// January: the vault moves in. New windows and habits, everything under the
/// operator's own five words.
async fn january_with(room: &Surface, sid: &str, files: January) {
    let covered_to = match files {
        January::OnTheAnniversary => "2028-01-10",
        January::OnTheLastCoveredDay => "2028-01-09",
    };
    did(room, sid, "read_mailbox", json!({})).await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "machine:theta", "content": "bought 2026-01-10 from Globex, two years of cover from that day",
               "provenance": "testimony", "shape": "about", "object": "org:globex",
               "sets": {"runs_out": covered_to}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:glasses", "content": "there is a spare pair in the hall drawer",
               "provenance": "testimony", "sets": {"spare": "hall drawer"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "considering the flat-pack kit, or building one",
               "provenance": "testimony", "sets": {"status": "next"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:kitchen-floor", "content": "considering",
               "provenance": "testimony", "sets": {"status": "next"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "38 minutes on 2026-01-08, my way to work",
               "provenance": "testimony", "sets": {"minutes": "38"}}),
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
               "provenance": "testimony", "sets": {"runs_out": "2026-03-08"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:wonder-wharf", "content": "the bridge is shut so I went in by the wharf road, the one through Wonder Wharf, Friday the 6th — 52 minutes, never again",
               "provenance": "testimony", "sets": {"minutes": "52"}}),
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
               "provenance": "testimony", "sets": {"spare": "Teddy"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "still can't decide; the kit's on offer again",
               "provenance": "testimony", "sets": {"status": "next"}}),
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
               "provenance": "testimony", "sets": {"runs_out": "2027-03-08"}}),
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
               "provenance": "testimony", "sets": {"for": "machine:omicron"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:kitchen-floor", "content": "happening — Gene's mate is booked for June",
               "provenance": "testimony", "sets": {"status": "now"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "41 minutes on Tuesday the 14th",
               "provenance": "testimony", "sets": {"minutes": "41"}}),
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
    may_with(room, sid, json!({"runs_out": "2027-02-01"})).await;
}

/// May again, with the furnace's deadline worked out as the last bookable day.
/// The operator's own definition of `runs_out` is the last day a thing can
/// still be done, so "before 1 February" reads as 31 January.
async fn may_on_the_last_bookable_day(room: &Surface, sid: &str) {
    may_with(room, sid, json!({"runs_out": "2027-01-31"})).await;
}

/// May again, with the furnace's deadline under a key no shipped type holds.
async fn may_under_another_key(room: &Surface, sid: &str) {
    may_with(room, sid, json!({"lapses_on": "2027-02-01"})).await;
}

/// May, with the furnace's deadline written as `sets`.
async fn may_with(room: &Surface, sid: &str, furnace_sets: serde_json::Value) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:the-furnace", "content": "Mr. Plow serviced it on the 6th — one free service a year, but it has to be used before 1 February or it lapses",
               "provenance": "testimony", "sets": furnace_sets}),
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
               "provenance": "testimony", "sets": {"status": "next"}}),
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
               "provenance": "testimony", "sets": {"for": "machine:theta"}}),
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
               "provenance": "testimony", "sets": {"status": "done"}}),
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
               "provenance": "testimony", "shape": "about", "object": "org:globex",
               "sets": {"runs_out": "2028-07-10"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "36 minutes on Tuesday the 14th",
               "provenance": "testimony", "sets": {"minutes": "36"}}),
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
               "provenance": "testimony", "sets": {"status": "next"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:wagstaff-fair", "content": "Louise mentioned tickets stop selling 10 October",
               "provenance": "testimony", "sets": {"runs_out": "2026-10-10"}}),
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

/// **The packing list as September keeps it**, hung on the trip the sitting
/// made a moment ago, or kept the wrong way the variant names.
async fn packing_list(room: &Surface, sid: &str, packing: SeptemberList) {
    if matches!(packing, SeptemberList::InProse) {
        did(
            room,
            sid,
            "capture",
            json!({"subject": "event:capital-city-trip",
                   "content": "packing list: the passports, the travel adapter and the camera",
                   "provenance": "testimony"}),
        )
        .await;
        return;
    }
    let mut made = json!({"kind": "list", "handle": "packing", "name": "Packing",
                          "source": "user-named", "sets": {"tick": "sink"}});
    if !matches!(packing, SeptemberList::OnNothing) {
        made["parent"] = json!("event:capital-city-trip");
    }
    let made = did(room, sid, "add_entity", made).await;
    assert!(
        !made.contains("blocked"),
        "the packing list was refused: {made}"
    );
    let things: &[&str] = if matches!(packing, SeptemberList::WithoutTheCamera) {
        &["passports", "travel adapter"]
    } else {
        &["passports", "travel adapter", "camera"]
    };
    for thing in things {
        let said = did(
            room,
            sid,
            "capture",
            json!({"subject": "list:packing", "content": thing, "provenance": "testimony"}),
        )
        .await;
        assert!(!said.contains("blocked"), "{thing} was refused: {said}");
    }
}

/// One tick on a list, as a sitting sends it: the entry, named by the address
/// the read gave, and the act.
async fn tick(room: &Surface, sid: &str, list: &str, text: &str, act: &str) {
    let read = room.call("recall", json!({"subject": list})).await;
    let parsed: serde_json::Value = serde_json::from_str(&read).expect("the read is json");
    let entry = parsed["objects"][0]["entries"]
        .as_array()
        .expect("a list answers with its entries")
        .iter()
        .find(|entry| {
            entry["text"]
                .as_str()
                .is_some_and(|said| said.contains(text))
        })
        .and_then(|entry| entry["address"].as_str())
        .unwrap_or_else(|| panic!("no entry on {list} says {text:?}: {read}"))
        .to_string();
    let said = did(
        room,
        sid,
        "capture",
        json!({"subject": list, "content": format!("{text}: {act}"),
               "provenance": "testimony", "tick": {"entry": entry, "act": act}}),
    )
    .await;
    assert!(
        !said.contains("blocked"),
        "the tick on {text} was refused: {said}"
    );
}

/// September: a trip booked and a month looked ahead to.
/// How September keeps the packing list.
#[derive(Clone, Copy)]
enum SeptemberList {
    /// A list hung on the trip, the three things the operator named open on it.
    Kept,
    /// A list with no parent, so it hangs on nothing and the trip has no child.
    OnNothing,
    /// A list on the trip without the camera.
    WithoutTheCamera,
    /// A sentence on the trip and no list at all.
    InProse,
}

async fn september_with(room: &Surface, sid: &str, packing: SeptemberList) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:wagstaff-fair", "content": "still need the food-handling certificate — offered by @org:quahog-community-college",
               "provenance": "testimony", "shape": "about", "object": "org:quahog-community-college",
               "sets": {"status": "active"}}),
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
               "provenance": "testimony", "sets": {"leaves_on": "2026-11-12", "returns_on": "2026-11-22"}}),
    )
    .await;
    packing_list(room, sid, packing).await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:ocean-avenue", "content": "40 minutes on Wednesday the 9th",
               "provenance": "testimony", "sets": {"minutes": "40"}}),
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

/// October again, with the course filed on an event that links to the college.
/// The operator gave a start and a finish; what a later read needs is a span
/// with both ends that it can reach from the college.
async fn october_with_the_course_on_a_linked_event(room: &Surface, sid: &str) {
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
        "add_entity",
        json!({"kind": "event", "handle": "kitchen-safety-course-oct-2026",
               "name": "Kitchen Safety Course, October 2026 Intake", "source": "user-named"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:kitchen-safety-course-oct-2026",
               "content": "started the October intake anyway, Tuesday evenings — too late for the stall but I want the certificate regardless",
               "provenance": "testimony", "shape": "about", "object": "org:quahog-community-college",
               "happened_at": "2026-10-06", "happened_through": "2026-11-10"}),
    )
    .await;
}

/// **How the four sittings that name the bins keep the rotation**, March to June.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rotation {
    /// A list that takes a turn and goes to the end, hung on the bins, with the
    /// operator and Linda on it and one turn ticked in each sitting.
    Kept,
    /// Every turn a sentence on the bins, and no list.
    InProse,
    /// The list is kept and ticked in March, April and May. June's turn is a
    /// sentence on the bins.
    ThreeTurns,
    /// The list is kept and ticked every sitting, and hangs on Linda.
    OnLinda,
    /// The list is kept and hangs on the bins, and every sitting ticks the
    /// operator's entry.
    OneEntryEveryTime,
    /// Each entry is ticked twice running, so four turns leave the open order
    /// where it began and no two neighbouring turns alternate.
    TwiceEach,
}

/// **One sitting's turn at the bins.** `sitting` counts from March at 0. The
/// turns alternate, the operator first, and the second and fourth are sent as
/// `rotated` so both acts a back list takes are read.
async fn the_bins_turn(room: &Surface, sid: &str, sitting: usize, how: Rotation) {
    let in_prose = how == Rotation::InProse || (how == Rotation::ThreeTurns && sitting == 3);
    if in_prose {
        let said = did(
            room,
            sid,
            "capture",
            json!({"subject": "thing:the-bins", "content": "it was my go at the bins tonight, so Linda is next",
                   "provenance": "testimony"}),
        )
        .await;
        assert!(
            !said.contains("blocked"),
            "the sentence was refused: {said}"
        );
        return;
    }
    if sitting == 0 {
        let parent = if how == Rotation::OnLinda {
            "person:linda"
        } else {
            "thing:the-bins"
        };
        let made = did(
            room,
            sid,
            "add_entity",
            json!({"kind": "list", "handle": "bin-turns", "name": "Bin turns", "source": "user-named",
                   "parent": parent, "sets": {"tick": "back"}}),
        )
        .await;
        assert!(
            !made.contains("blocked"),
            "the rotation was refused: {made}"
        );
        for who in ["me", "Linda"] {
            let said = did(
                room,
                sid,
                "capture",
                json!({"subject": "list:bin-turns", "content": who, "provenance": "testimony"}),
            )
            .await;
            assert!(!said.contains("blocked"), "{who} was refused: {said}");
        }
    }
    let turns = if how == Rotation::TwiceEach {
        [
            ("me", "done"),
            ("me", "rotated"),
            ("Linda", "done"),
            ("Linda", "rotated"),
        ]
    } else if how == Rotation::OneEntryEveryTime {
        [
            ("me", "done"),
            ("me", "rotated"),
            ("me", "done"),
            ("me", "rotated"),
        ]
    } else {
        [
            ("me", "done"),
            ("Linda", "rotated"),
            ("me", "done"),
            ("Linda", "rotated"),
        ]
    };
    let (who, act) = turns[sitting];
    tick(room, sid, "list:bin-turns", who, act).await;
}

/// November: four days before the trip.
/// How November ticks the packing list and plans the week.
#[derive(Clone, Copy)]
enum NovemberList {
    /// The passports and the adapter ticked, the camera open; the week of
    /// 2026-W46 planned with its three errands open.
    Kept,
    /// Every entry ticked, the camera too.
    EverythingTicked,
    /// Nothing on the packing list ticked; the week planned as in `Kept`.
    NothingPacked,
    /// The plan covers the week before, 2026-W45.
    TheWrongWeek,
    /// Both written as sentences on the trip, no ticks and no plan.
    InProse,
}

async fn november_with(room: &Surface, sid: &str, kept: NovemberList) {
    packed_and_planned(room, sid, kept).await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:capital-city-trip", "content": "four days out — need someone to feed @pet:snowball twice a day since Teddy can't do it, allergic",
               "provenance": "testimony", "shape": "about", "object": "pet:snowball",
               "sets": {"status": "active"}}),
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
               "provenance": "inference", "sets": {"spare": "hall drawer"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:house-keys", "content": "the spare set is with @person:teddy",
               "provenance": "testimony", "shape": "connection", "object": "person:teddy",
               "sets": {"status": "active"}}),
    )
    .await;
}

/// **November's packing and its week plan.**
async fn packed_and_planned(room: &Surface, sid: &str, kept: NovemberList) {
    if matches!(kept, NovemberList::InProse) {
        did(
            room,
            sid,
            "capture",
            json!({"subject": "event:capital-city-trip",
                   "content": "packed the passports and the adapter, the camera is left. The week \
                               ahead: return the library book, buy the travel insurance, collect the \
                               printed tickets",
                   "provenance": "testimony"}),
        )
        .await;
        return;
    }
    if !matches!(kept, NovemberList::NothingPacked) {
        tick(room, sid, "list:packing", "passports", "done").await;
        tick(room, sid, "list:packing", "adapter", "done").await;
    }
    if matches!(kept, NovemberList::EverythingTicked) {
        tick(room, sid, "list:packing", "camera", "done").await;
    }
    let covers = if matches!(kept, NovemberList::TheWrongWeek) {
        "2026-W45"
    } else {
        "2026-W46"
    };
    let made = did(
        room,
        sid,
        "add_entity",
        json!({"kind": "list", "handle": "errands", "name": "The week ahead", "source": "user-named",
               "sets": {"tick": "sink", "covers": covers}}),
    )
    .await;
    assert!(
        !made.contains("blocked"),
        "the week plan was refused: {made}"
    );
    for errand in [
        "return the library book",
        "buy the travel insurance",
        "collect the printed tickets",
    ] {
        let said = did(
            room,
            sid,
            "capture",
            json!({"subject": "list:errands", "content": errand, "provenance": "testimony"}),
        )
        .await;
        assert!(!said.contains("blocked"), "{errand} was refused: {said}");
    }
}

/// How November the 9th accounts for the plan.
#[derive(Clone, Copy)]
enum DayAfter {
    /// The two errands ticked done, the tickets carried onto tomorrow's plan
    /// by a capture that names the entry it comes from.
    Carried,
    /// The tickets carried and nothing ticked, so the plan says nothing was done.
    NothingTicked,
    /// The tickets typed afresh on tomorrow's plan, naming nothing.
    ByHand,
    /// A sentence on the trip and no tick, no carry and no plan.
    InProse,
}

/// **November, two days on.** The sitting is cold: it reads yesterday's plan
/// out of the store, records what was done, and carries the unfinished entry.
async fn november_the_day_after(room: &Surface, sid: &str, account: DayAfter) {
    if matches!(account, DayAfter::InProse) {
        did(
            room,
            sid,
            "capture",
            json!({"subject": "event:capital-city-trip",
                   "content": "returned the library book and bought the insurance; the tickets \
                               are not collected and go on tomorrow's list",
                   "provenance": "testimony"}),
        )
        .await;
        return;
    }
    if !matches!(account, DayAfter::NothingTicked) {
        tick(room, sid, "list:errands", "library", "done").await;
        tick(room, sid, "list:errands", "insurance", "done").await;
    }
    let made = did(
        room,
        sid,
        "add_entity",
        json!({"kind": "list", "handle": "checklist", "name": "Tomorrow", "source": "user-named",
               "sets": {"tick": "sink", "covers": "2026-11-11"}}),
    )
    .await;
    assert!(
        !made.contains("blocked"),
        "tomorrow's plan was refused: {made}"
    );
    let read = room
        .call("recall", json!({"subject": "list:errands"}))
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&read).expect("the read is json");
    let tickets = parsed["objects"][0]["entries"]
        .as_array()
        .expect("a plan answers with its entries")
        .iter()
        .find(|entry| {
            entry["text"]
                .as_str()
                .is_some_and(|said| said.contains("tickets"))
        })
        .and_then(|entry| entry["address"].as_str())
        .unwrap_or_else(|| panic!("the plan holds no tickets entry: {read}"))
        .to_string();
    let mut carry = json!({"subject": "list:checklist", "content": "collect the printed tickets",
                           "provenance": "inference"});
    if !matches!(account, DayAfter::ByHand) {
        carry["derived_from"] = json!(tickets);
    }
    let said = did(room, sid, "capture", carry).await;
    assert!(!said.contains("blocked"), "the carry was refused: {said}");
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

/// December without the note on the furnace: the boots get theirs and the
/// furnace's free service, which also lapses inside the horizon, gets none.
async fn december_without_the_furnace_note(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:boots", "content": "the guarantee closes inside the horizon — need to look at these before the end of March",
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
               "provenance": "testimony", "sets": {"resumes_on": "2027-04-01"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-shed", "content": "still going round on this — giving myself a deadline to decide",
               "provenance": "testimony", "sets": {"decide_by": "2027-01-15"}}),
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

/// **The overdue count is December's own answer, however the year goes on.**
///
/// `overdue_excluded` counts every carrier of the key in the whole room on the
/// day asked, so a thing filed after December changes it and no record in the
/// answer can pin it to December. The lock asks at the end of its own phase
/// and reads that answer, so a later sitting that files one more dated thing
/// does not move it.
///
/// **The control is read off the room.** The same question asked now says five,
/// so a lock that held here for any reason but the kept answer would have to
/// be reading something else.
#[tokio::test]
async fn the_overdue_count_lock_answers_as_december_left_the_room() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault(&surface, &room_document()).await;
    let asked = json!({"answers_type": "runs-out", "overdue": {"as_of": "2026-12-06"}});
    let december = surface.call("recall", asked.clone()).await;
    assert!(december.contains("\"overdue_excluded\":4"), "{december}");

    let later = sitting(&surface, "2026-12-20").await;
    did(
        &surface,
        &later,
        "add_entity",
        json!({"kind": "thing", "handle": "tau", "name": "Late Addition",
               "source": "the operator"}),
    )
    .await;
    did(
        &surface,
        &later,
        "capture",
        json!({"subject": "thing:tau", "content": "a window that runs out next summer",
               "provenance": "testimony", "sets": {"runs_out": "2027-06-30"}}),
    )
    .await;
    let now = surface.call("recall", asked).await;
    assert!(
        now.contains("\"overdue_excluded\":5"),
        "the late carrier did not move the live count, so this case proves nothing: {now}",
    );

    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_overdue_count_lock().check(&seen).await;
    assert!(
        outcome.held,
        "the lock read the room as the year left it and not as December did: {}",
        outcome.saying,
    );
}

/// **The vault worked sitting by sitting, taking the readings a run takes.**
///
/// One driver for the whole year, the same shape `year_room.rs`'s own
/// `work_the_year` takes for its worked case — but with no sabotage variants,
/// because nothing here has any yet: `unworked_boundaries` already proves
/// every lock reds on a room nobody touched, so this is the other half of
/// that same pair, not a second copy of the mechanism with wrong turns
/// wired in.
/// One phase's own worked function, boxed so all fourteen — each `async fn`
/// with its own distinct anonymous type — can sit in one array.
type PhaseFn = for<'a> fn(
    &'a Surface,
    &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>;

async fn worked_the_vault(room: &Surface, vault: &Playbook) -> Vec<Boundary> {
    worked_the_vault_on(
        room,
        vault,
        May::PlanDay,
        October::OnTheCollege,
        December::Whole,
    )
    .await
}

/// **How the three sittings that keep lists keep them.** The right occupant
/// keeps every list; each wrong one changes one thing.
#[derive(Clone, Copy)]
struct Lists {
    september: SeptemberList,
    november: NovemberList,
    day_after: DayAfter,
    rotation: Rotation,
}

/// Every list kept where a later sitting reads it.
const LISTS_KEPT: Lists = Lists {
    september: SeptemberList::Kept,
    november: NovemberList::Kept,
    day_after: DayAfter::Carried,
    rotation: Rotation::Kept,
};

/// Every list written as a sentence on the trip.
const LISTS_IN_PROSE: Lists = Lists {
    september: SeptemberList::InProse,
    november: NovemberList::InProse,
    day_after: DayAfter::InProse,
    rotation: Rotation::Kept,
};

/// How May files the furnace's deadline.
#[derive(Clone, Copy)]
enum May {
    /// The day the plan itself names.
    PlanDay,
    /// The last day the operator can still book it.
    LastBookableDay,
    /// Under a key no shipped type holds.
    AnotherKey,
}

/// Where October files the course span.
#[derive(Clone, Copy)]
enum October {
    /// On the college's own records.
    OnTheCollege,
    /// On an event whose claim links to the college.
    OnALinkedEvent,
}

/// What December writes.
#[derive(Clone, Copy)]
enum December {
    /// A note on each of the two windows that close inside the horizon.
    Whole,
    /// A note on the boots and none on the furnace.
    NoFurnaceNote,
}

async fn worked_the_vault_on(
    room: &Surface,
    vault: &Playbook,
    may_files: May,
    october_files: October,
    december_writes: December,
) -> Vec<Boundary> {
    worked_the_vault_with(
        room,
        vault,
        may_files,
        october_files,
        december_writes,
        LISTS_KEPT,
        January::OnTheAnniversary,
    )
    .await
}

async fn worked_the_vault_with(
    room: &Surface,
    vault: &Playbook,
    may_files: May,
    october_files: October,
    december_writes: December,
    lists: Lists,
    january_files: January,
) -> Vec<Boundary> {
    let named = boundary_names(vault);
    let september_phase: PhaseFn = match lists.september {
        SeptemberList::Kept => |r, s| Box::pin(september_with(r, s, SeptemberList::Kept)),
        SeptemberList::OnNothing => |r, s| Box::pin(september_with(r, s, SeptemberList::OnNothing)),
        SeptemberList::WithoutTheCamera => {
            |r, s| Box::pin(september_with(r, s, SeptemberList::WithoutTheCamera))
        }
        SeptemberList::InProse => |r, s| Box::pin(september_with(r, s, SeptemberList::InProse)),
    };
    let november_phase: PhaseFn = match lists.november {
        NovemberList::Kept => |r, s| Box::pin(november_with(r, s, NovemberList::Kept)),
        NovemberList::EverythingTicked => {
            |r, s| Box::pin(november_with(r, s, NovemberList::EverythingTicked))
        }
        NovemberList::NothingPacked => {
            |r, s| Box::pin(november_with(r, s, NovemberList::NothingPacked))
        }
        NovemberList::TheWrongWeek => {
            |r, s| Box::pin(november_with(r, s, NovemberList::TheWrongWeek))
        }
        NovemberList::InProse => |r, s| Box::pin(november_with(r, s, NovemberList::InProse)),
    };
    let day_after_phase: PhaseFn = match lists.day_after {
        DayAfter::Carried => |r, s| Box::pin(november_the_day_after(r, s, DayAfter::Carried)),
        DayAfter::NothingTicked => {
            |r, s| Box::pin(november_the_day_after(r, s, DayAfter::NothingTicked))
        }
        DayAfter::ByHand => |r, s| Box::pin(november_the_day_after(r, s, DayAfter::ByHand)),
        DayAfter::InProse => |r, s| Box::pin(november_the_day_after(r, s, DayAfter::InProse)),
    };
    let may_phase: PhaseFn = match may_files {
        May::PlanDay => |r, s| Box::pin(may(r, s)),
        May::LastBookableDay => |r, s| Box::pin(may_on_the_last_bookable_day(r, s)),
        May::AnotherKey => |r, s| Box::pin(may_under_another_key(r, s)),
    };
    let october_phase: PhaseFn = match october_files {
        October::OnTheCollege => |r, s| Box::pin(october(r, s)),
        October::OnALinkedEvent => |r, s| Box::pin(october_with_the_course_on_a_linked_event(r, s)),
    };
    let december_phase: PhaseFn = match december_writes {
        December::Whole => |r, s| Box::pin(december(r, s)),
        December::NoFurnaceNote => |r, s| Box::pin(december_without_the_furnace_note(r, s)),
    };
    let january_phase: PhaseFn = match january_files {
        January::OnTheAnniversary => |r, s| Box::pin(january_with(r, s, January::OnTheAnniversary)),
        January::OnTheLastCoveredDay => {
            |r, s| Box::pin(january_with(r, s, January::OnTheLastCoveredDay))
        }
    };
    let phases: [(&str, PhaseFn); 14] = [
        ("2026-01-12", january_phase),
        ("2026-02-08", |r, s| Box::pin(february(r, s))),
        ("2026-03-15", |r, s| Box::pin(march(r, s))),
        ("2026-04-19", |r, s| Box::pin(april(r, s))),
        ("2026-05-10", may_phase),
        ("2026-06-14", |r, s| Box::pin(june(r, s))),
        ("2026-07-19", |r, s| Box::pin(july(r, s))),
        ("2026-08-16", |r, s| Box::pin(august(r, s))),
        ("2026-09-13", september_phase),
        ("2026-10-11", october_phase),
        ("2026-11-08", november_phase),
        ("2026-11-10", day_after_phase),
        ("2026-12-06", december_phase),
        ("2026-12-13", |r, s| Box::pin(later_december(r, s))),
    ];
    let locks = expectations::for_playbook(expectations::VAULT_ROOM).expect("the vault asserts");
    let mut boundaries = vec![boundary(room, &named[0]).await];
    for (at, (day, work)) in phases.iter().enumerate() {
        let sid = sitting(room, day).await;
        work(room, &sid).await;
        // March to June: the sittings that name the bins.
        if (2..=5).contains(&at) {
            the_bins_turn(room, &sid, at - 2, lists.rotation).await;
        }
        let asked = phase_end_queries(&locks, &format!("Phase {}", at + 1));
        boundaries.push(boundary_asking(room, &named[at + 1], &asked).await);
    }
    boundaries
}

/// **The vault is solvable, and this is the case that says so.**
///
/// A different kind of case from `a_vault_nobody_worked_in_fails_every_lock`:
/// that one proves the locks discriminate against a store somebody put
/// nothing into. This one proves every question the vault asks CAN be
/// answered through the served surface — without it a vault whose own
/// arithmetic is wrong would be unreachable by every session that ever
/// enters it, the unworked case would still pass, and the failure would
/// surface on a paid run reading as a defect in the product.
///
/// **February's own lock on `place:wonder-wharf` used to be unsatisfiable
/// by any run.** It and October's lock query the identical subject with
/// the identical shape, and both were `Live` — judged once, against the
/// room as it stood when the whole run finished. October's job is to open
/// the claim and nothing ever re-settles it, so a Live read of February's
/// `lacks "standing":"open"`, taken after October has run, was reading a
/// world October had already changed. `vault.md`'s February lock now
/// carries `window own-phase`, read from the boundary right after
/// February — before October exists to touch anything, which is the one
/// moment "settled today" is actually about.
#[tokio::test]
async fn every_lock_holds_once_the_vault_is_worked() {
    let (_room, surface) = {
        let (room, surface) = Room::open_with_client_stating(
            &server_binary().expect("a jojobot binary"),
            SERVER_DAY.as_str(),
        )
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
        assert!(
            outcome.held,
            "a vault worked the way it is meant to be failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **The locks that red when the vault is worked with these lists**, by their
/// own sentences. The whole year is played, so a list a wrong September keeps
/// is the list a later sitting finds.
async fn red_when_the_lists_are(lists: Lists) -> Vec<String> {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_with(
        &surface,
        &room_document(),
        May::PlanDay,
        October::OnTheCollege,
        December::Whole,
        lists,
        January::OnTheAnniversary,
    )
    .await;
    judge_all(&surface, &boundaries)
        .await
        .into_iter()
        .filter(|outcome| !outcome.held)
        .map(|outcome| outcome.saying)
        .collect()
}

/// The sentence of each of the six locks the three list sittings add.
const PACKING_LIST_LOCK: &str = "no list holds the passports, the adapter and the camera";
const ON_THE_TRIP_LOCK: &str = "the packing list hangs on nothing";
const TICKED_LOCK: &str = "does not show the passports and the adapter ticked";
const WEEK_PLAN_LOCK: &str = "nothing covers the week of 2026-W46";
const ACCOUNT_LOCK: &str = "the week's plan does not show the library book";
const CARRY_LOCK: &str = "nothing covers 2026-11-11";

/// The sentence of each of the five locks the rotation adds: one per sitting, and the end lock.
const MARCH_TURN_LOCK: &str = "nothing hangs on the bins as a list that turns";
const APRIL_TURN_LOCK: &str = "does not hold two turns";
const MAY_TURN_LOCK: &str = "does not hold three turns";
const JUNE_TURN_LOCK: &str = "does not hold four turns";
const ALTERNATING_LOCK: &str = "the turns on the bins do not alternate";

/// **Exactly these locks are red, and no other.** Each fragment names one lock,
/// and one lock only.
async fn reds_exactly(lists: Lists, fragments: &[&str]) {
    let red = red_when_the_lists_are(lists).await;
    for fragment in fragments {
        assert_eq!(
            red.iter()
                .filter(|saying| saying.contains(fragment))
                .count(),
            1,
            "the lock naming {fragment:?} did not red exactly once: {red:#?}",
        );
    }
    assert_eq!(
        red.len(),
        fragments.len(),
        "a lock reddened beyond {fragments:?}: {red:#?}",
    );
}

/// **The lists the vault is meant to be worked with are what its list locks
/// hold on.** The solvability case is `every_lock_holds_once_the_vault_is_worked`;
/// this one is apart from it by name and says the six hold on their own.
#[tokio::test]
async fn the_six_list_locks_hold_when_every_list_is_kept() {
    reds_exactly(LISTS_KEPT, &[]).await;
}

/// **The same year written entirely in prose reds all six list locks and no
/// other.** The packing list, the ticks, the week and its account are each a
/// sentence on the trip, the way a sitting that never reached for a list
/// writes them. Everything the year holds elsewhere still holds.
#[tokio::test]
async fn a_year_whose_lists_are_all_sentences_reds_the_six_list_locks_and_no_other() {
    reds_exactly(
        LISTS_IN_PROSE,
        &[
            PACKING_LIST_LOCK,
            ON_THE_TRIP_LOCK,
            TICKED_LOCK,
            WEEK_PLAN_LOCK,
            ACCOUNT_LOCK,
            CARRY_LOCK,
        ],
    )
    .await;
}

/// **The bins written as sentences red the five rotation locks and no other.**
/// Every other list is kept, so each red is the rotation's.
#[tokio::test]
async fn a_rotation_written_as_sentences_reds_the_five_rotation_locks_and_no_other() {
    reds_exactly(
        Lists {
            rotation: Rotation::InProse,
            ..LISTS_KEPT
        },
        &[
            MARCH_TURN_LOCK,
            APRIL_TURN_LOCK,
            MAY_TURN_LOCK,
            JUNE_TURN_LOCK,
            ALTERNATING_LOCK,
        ],
    )
    .await;
}

/// **A rotation ticked three times reds the count that wants four and the end
/// lock, and not the first three sittings' locks.**
#[tokio::test]
async fn a_rotation_ticked_three_times_reds_the_fourth_sittings_lock_and_the_end_lock() {
    reds_exactly(
        Lists {
            rotation: Rotation::ThreeTurns,
            ..LISTS_KEPT
        },
        &[JUNE_TURN_LOCK, ALTERNATING_LOCK],
    )
    .await;
}

/// **One entry ticked four times holds every count and reds only the end
/// lock.** Four ticks stand, the open order is not where it began, and no
/// substring can tell it from four turns taken in turn.
#[tokio::test]
async fn one_entry_ticked_four_times_reds_only_the_end_lock() {
    reds_exactly(
        Lists {
            rotation: Rotation::OneEntryEveryTime,
            ..LISTS_KEPT
        },
        &[ALTERNATING_LOCK],
    )
    .await;
}

/// **Each entry ticked twice running reds only the end lock, though the open
/// order is back where it began.** The open order alone would hold; only the
/// alternation of the turns tells this from four turns taken in turn.
#[tokio::test]
async fn each_entry_ticked_twice_running_reds_only_the_end_lock() {
    reds_exactly(
        Lists {
            rotation: Rotation::TwiceEach,
            ..LISTS_KEPT
        },
        &[ALTERNATING_LOCK],
    )
    .await;
}

/// **A rotation kept and turned every sitting on the wrong thing reds all five.**
/// The turns are all there; the lock asks the bins for a child.
#[tokio::test]
async fn a_rotation_hung_on_linda_reds_the_five_rotation_locks() {
    reds_exactly(
        Lists {
            rotation: Rotation::OnLinda,
            ..LISTS_KEPT
        },
        &[
            MARCH_TURN_LOCK,
            APRIL_TURN_LOCK,
            MAY_TURN_LOCK,
            JUNE_TURN_LOCK,
            ALTERNATING_LOCK,
        ],
    )
    .await;
}

/// **The bins' turns are ticks, and November's count of ticks is not theirs.**
/// A November that ticks nothing on the packing list reds the lock that reads
/// two packed, though four turns of the rotation stand in the same kind.
#[tokio::test]
async fn a_november_that_packs_nothing_reds_the_ticked_lock_whatever_the_bins_hold() {
    reds_exactly(
        Lists {
            november: NovemberList::NothingPacked,
            ..LISTS_KEPT
        },
        &[TICKED_LOCK],
    )
    .await;
}

/// **A packing list hung on nothing reds only the lock that asks the trip for
/// a child.** The three things are still open entries on it.
#[tokio::test]
async fn a_packing_list_hung_on_nothing_reds_only_the_lock_that_asks_the_trip() {
    reds_exactly(
        Lists {
            september: SeptemberList::OnNothing,
            ..LISTS_KEPT
        },
        &[ON_THE_TRIP_LOCK],
    )
    .await;
}

/// **A packing list that never held the camera reds the list lock and, with
/// it, November's, which reads the camera still open.** Two locks and the
/// second is the same mistake read a sitting later.
#[tokio::test]
async fn a_packing_list_without_the_camera_reds_the_list_lock_and_novembers_open_camera() {
    reds_exactly(
        Lists {
            september: SeptemberList::WithoutTheCamera,
            ..LISTS_KEPT
        },
        &[PACKING_LIST_LOCK, TICKED_LOCK],
    )
    .await;
}

/// **A November that ticks the camera too reds only the lock that reads it
/// open.** Two ticks are still in the history, so a count alone would hold.
#[tokio::test]
async fn a_november_that_ticks_everything_reds_only_the_open_camera_lock() {
    reds_exactly(
        Lists {
            november: NovemberList::EverythingTicked,
            ..LISTS_KEPT
        },
        &[TICKED_LOCK],
    )
    .await;
}

/// **A week plan on the wrong week reds the lock that asks for 2026-W46 and,
/// with it, the account that reads the same week a day later.**
#[tokio::test]
async fn a_week_plan_on_the_wrong_week_reds_the_plan_lock_and_the_account_that_reads_it() {
    reds_exactly(
        Lists {
            november: NovemberList::TheWrongWeek,
            ..LISTS_KEPT
        },
        &[WEEK_PLAN_LOCK, ACCOUNT_LOCK],
    )
    .await;
}

/// **An account that ticks nothing reds only the account lock.** The carry is
/// still on tomorrow's plan, so the other holds.
#[tokio::test]
async fn an_account_that_ticks_nothing_reds_only_the_account_lock() {
    reds_exactly(
        Lists {
            day_after: DayAfter::NothingTicked,
            ..LISTS_KEPT
        },
        &[ACCOUNT_LOCK],
    )
    .await;
}

/// **Tickets typed afresh on tomorrow's plan are not a carry**: the plan that
/// holds them names no origin and the old entry names no destination, so the
/// account and the carry both red.
#[tokio::test]
async fn tickets_typed_afresh_on_tomorrows_plan_red_the_account_and_the_carry() {
    reds_exactly(
        Lists {
            day_after: DayAfter::ByHand,
            ..LISTS_KEPT
        },
        &[ACCOUNT_LOCK, CARRY_LOCK],
    )
    .await;
}

/// **The furnace's free service is a window with two right days.** `runs_out`
/// is the last day the operator can still do something about the thing, so
/// "before 1 February" is 31 January, and the plan itself names 1 February.
/// A year that worked it out as the last bookable day holds every lock, as the
/// year that kept the plan's day does.
#[tokio::test]
async fn every_lock_holds_when_the_furnace_deadline_is_the_last_bookable_day() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_on(
        &surface,
        &room_document(),
        May::LastBookableDay,
        October::OnTheCollege,
        December::Whole,
    )
    .await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a vault that worked the furnace out as 31 January failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **The laptop's cover is a window with two right end days.** Two years of
/// cover from 2026-01-10 ends on the anniversary or on the last covered day. A
/// year that filed the last covered day holds every lock, January's and
/// December's among them, as the year that filed the anniversary does.
#[tokio::test]
async fn every_lock_holds_when_the_laptop_cover_is_filed_on_the_last_covered_day() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_with(
        &surface,
        &room_document(),
        May::PlanDay,
        October::OnTheCollege,
        December::Whole,
        LISTS_KEPT,
        January::OnTheLastCoveredDay,
    )
    .await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a vault that filed the laptop's last covered day failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **December's note on the furnace is one lock, and only that lock.** The year
/// is worked as it is meant to be, except that December gives the boots their
/// note and the furnace none. The lock that asks for the furnace's note fails
/// and no other lock does.
#[tokio::test]
async fn december_leaving_the_furnace_without_a_note_reds_only_the_furnace_note_lock() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_on(
        &surface,
        &room_document(),
        May::PlanDay,
        October::OnTheCollege,
        December::NoFurnaceNote,
    )
    .await;
    let outcomes = judge_all(&surface, &boundaries).await;
    let failed: Vec<&str> = outcomes
        .iter()
        .filter(|o| !o.held)
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(
        failed.len(),
        1,
        "a furnace without a December note failed more than its own lock: {}",
        saying(&outcomes),
    );
    assert!(
        failed[0].contains("the furnace's free service lapses inside the horizon and got no note"),
        "the lock that failed is not the furnace note lock: {}",
        saying(&outcomes),
    );
}

/// **The course span has two right homes.** The operator gave a start and a
/// finish together; a later read needs a span with both ends that it can reach
/// from the college. A year that filed the span on an event whose claim links to
/// the college holds every lock, as the year that filed it on the college does.
#[tokio::test]
async fn every_lock_holds_when_the_course_span_is_filed_on_a_linked_event() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_on(
        &surface,
        &room_document(),
        May::PlanDay,
        October::OnALinkedEvent,
        December::Whole,
    )
    .await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a vault that filed the course span on a linked event failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **A deadline filed under a key no shipped type holds is read by every lock
/// that asks for the day, and the room says which.** The furnace's day is under
/// `lapses_on`, so the May lock, December's note lock and the comparison lock
/// all fail. They fail together because each reads the same key, so this is not
/// a control for any one of them.
#[tokio::test]
async fn a_furnace_deadline_under_another_key_reds_the_locks_that_read_the_day() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault_on(
        &surface,
        &room_document(),
        May::AnotherKey,
        October::OnTheCollege,
        December::Whole,
    )
    .await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for fragment in [
        "the one window nobody would think to call a deadline",
        "the furnace's free service lapses inside the horizon and got no note",
        "cannot be compared as a date",
    ] {
        assert!(
            outcomes
                .iter()
                .any(|o| !o.held && o.name.contains(fragment)),
            "a furnace deadline under another key did not fail the lock naming {fragment:?}: {}",
            saying(&outcomes),
        );
    }
}

/// 🚨 **No lock in this room rests on a needle that matches somewhere else.**
///
/// The reasoning is written where this was first built, in the year room's
/// own case. **This was the second room left uncovered**, this time for
/// nothing more than the room not having asked yet.
///
/// **Green, not blind.** Eight of the nine needles this case first found
/// were scoped to their own sitting. Seven by a companion needle only that
/// sitting's own record or content could satisfy, the same shape October's own
/// dependency lock already uses. One, `archived_excluded`, is an envelope
/// value no partner needle can pin, so `needle_summary` is given the run's
/// boundaries and drops an envelope needle that is false before its lock's
/// own phase and true after it.
///
/// **`overdue_excluded` is the one named exception, and no needle can scope
/// it.** It counts the whole room's `runs_out` carriers on the day asked, so
/// no December record can appear in the answer to pin it to December, and it
/// appears in no boundary text for the boundary test to read. Turning it into
/// a `check` hatch would misuse that escape valve — `checks.rs`'s own doc says
/// a hatch names something the query-and-assertion vocabulary cannot say, and
/// this one says it fine. It is named here, the same way
/// `needle_hygiene_coverage.rs`'s own `PENDING` names a room-level gap, so a
/// NEW finding still reds this case instead of hiding beside it.
const ACCEPTED_ENVELOPE_FINDINGS: &[&str] = &["\"overdue_excluded\":4"];

/// **Needles the walk cannot read, and the locks read fine.** The walk matches
/// one key and its value at a time, so a needle that runs from one key of a list
/// entry into the next reads as nowhere. These two do, on purpose: an entry is
/// `state` then `text`, and what a lock says is that THE camera is still open
/// and that THE carried entry is open. No single pair says that. Each lock holds
/// on the worked vault, and the case that reddens it alone (the
/// `a_november_that_ticks_everything…` and `tickets_typed_afresh…` cases) proves
/// it can fail, so nothing here is a lock that is silently failing.
const ACCEPTED_NOWHERE: &[&str] = &[
    "\"state\":\"open\",\"text\":\"camera\"",
    "\"],\"state\":\"open\"",
];

#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = worked_the_vault(&surface, &room_document()).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(expectations::VAULT_ROOM),
        &boundaries,
    )
    .await;
    let unread: Vec<&String> = summary
        .nowhere
        .iter()
        .filter(|found| {
            !ACCEPTED_NOWHERE
                .iter()
                .any(|needle| found.ends_with(needle))
        })
        .collect();
    assert!(
        unread.is_empty(),
        "a needle matched nowhere, so either its lock is failing or the walk could not read the \
         answer — and those are different: {unread:?}",
    );
    let unaccepted: Vec<&String> = summary
        .findings
        .iter()
        .filter(|finding| {
            !ACCEPTED_ENVELOPE_FINDINGS
                .iter()
                .any(|needle| finding.ends_with(needle))
        })
        .collect();
    assert!(
        unaccepted.is_empty(),
        "a lock rests on a needle that matches somewhere else, with nothing else in that lock \
         only its own sitting could satisfy, and it is not one of the named envelope \
         exceptions: {unaccepted:?}",
    );
    for needle in ACCEPTED_NOWHERE {
        assert!(
            summary.nowhere.iter().any(|found| found.ends_with(needle)),
            "{needle} is listed as unreadable by the walk but it reads now — the exception is \
             stale",
        );
    }
    // **The exception list is a receipt, not a promise.** An entry that
    // stopped matching anything would be a stale allowlist masking nothing —
    // the same failure mode `needle_hygiene_coverage.rs`'s own
    // `every_named_entry_is_a_room_this_build_still_ships` guards against.
    for needle in ACCEPTED_ENVELOPE_FINDINGS {
        assert!(
            summary
                .findings
                .iter()
                .any(|finding| finding.ends_with(needle)),
            "{needle} is listed as an accepted envelope finding but no longer matches anything — \
             it was fixed, or the lock changed, and the exception is stale",
        );
    }
}

/// **The wharf locks recall `place:wonder-wharf`, so the operator's own words
/// have to say that the wharf road is that place.** The furnished place is
/// bare, and a sitting that is told only "the wharf road" has nothing to tie
/// it to the place the locks read. The tie is the operator's: the sentence
/// that says *wharf road* in February's entry names the place as the room
/// furnishes it. No lock accepts a place by guess.
#[test]
fn the_february_entry_ties_the_wharf_road_to_the_furnished_place() {
    let source = std::fs::read_to_string(expectations::room_document(expectations::VAULT_ROOM))
        .expect("the shipped room reads");
    let furnished = source
        .lines()
        .find_map(|line| line.strip_prefix("entity  place:wonder-wharf |"))
        .expect("the room furnishes the place the wharf locks recall")
        .trim()
        .to_string();
    let february = room_document()
        .phases
        .into_iter()
        .find(|phase| phase.prompt.contains("8 February 2026"))
        .expect("the room has a February entry");
    let sentence = february
        .prompt
        .split(". ")
        .find(|sentence| sentence.contains("wharf road"))
        .expect("February's entry speaks of the wharf road");
    assert!(
        sentence.contains(&furnished),
        "the sentence on the wharf road does not name {furnished}: {sentence}"
    );
}

/// **The room's server runs on the day the room gave it, and never on the wall
/// clock.** `ping` says which day the server is acting out. A server on the real
/// clock says none, so this fails on every day if the pin is missing.
#[tokio::test]
async fn the_rooms_server_runs_on_the_day_the_room_gave_it() {
    let (_room, surface, _sid) = furnished().await;
    assert_eq!(
        surface.stated_day().await.as_deref(),
        Some(SERVER_DAY.as_str()),
        "the server is not acting out the day the room gave it"
    );
}

/// **No needle of this room holds the day its server runs on.** A needle is
/// matched as text in an answer, and the server writes its day into the moments
/// it stamps, so a needle holding that day could be met by a stamp.
#[test]
fn no_needle_here_is_the_servers_own_day() {
    let named = jojobot_exercise::lock::needles_naming(
        &jojobot_exercise::lock::locks_of(expectations::VAULT_ROOM),
        SERVER_DAY.as_str(),
    );
    assert!(
        named.is_empty(),
        "a needle holds the day the server runs on, so a stamp can meet it: {named:?}"
    );
}

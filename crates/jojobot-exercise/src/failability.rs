//! **The registry of whether a lock can ever fail.**
//!
//! A lock is checked once, at the end of the whole playbook, against the
//! live room's final state — [`crate::lock`] says why. Nothing today asks
//! whether a given lock is even CAPABLE of reddening: a lock that happens to
//! hold on every furniture this build ships looks identical, from a green
//! run, to one that would catch a real regression. Adding alternation (an
//! `or` between two branches) to the lock format would make this worse
//! rather than better — whether a branch is vacuously true depends on the
//! room's furniture rather than on the lock's own text, so a branch that can
//! never fail hides behind the one that can and a green run tells the two
//! apart from nothing. **The guard belongs before the feature.**
//!
//! **A negative control is a real demonstration, elsewhere in this crate's
//! test suite, that a specific lock reddens under a specific sabotage.**
//! Registering one here names the file and the function where that proof
//! lives — it is a claim about this crate's other tests, not something this
//! module executes. **This gate's stated boundary is deliberate, not a gap
//! waiting to close**: it checks that a registered proof's file and function
//! still EXIST — mechanically, by reading the named file for the named
//! function — never that the proof still reddens the lock it claims to.
//! Running the proof itself would buy a second copy of a failure that is
//! already loud: that proof runs on every `cargo test`, independently of
//! this gate, and a broken one already fails there. What is genuinely
//! unguarded without this check is the DANGLING NAME — rename or delete the
//! function and the registry's claim rots in silence, the same class of rot
//! `tests/failability_gate.rs`'s own
//! `every_pending_entry_still_names_a_shipped_lock` already guards against
//! for [`PENDING`].
//!
//! **Executing the proof is excluded on purpose, not left for later.** A
//! hatch — the mechanism that would have to run it — is read-only by
//! design: [`crate::lock`]'s own doc says a lock carrying a session is the
//! harness coaching the occupant, which is the failure that rule exists to
//! prevent. A control's proof is not read-only — it boots an identity and
//! writes, to set the scenario up — so granting it a session to make this
//! gate tidier would be the worst trade on this board. If execution is
//! wanted later, it starts with that rule and the operator, not with a
//! quiet refactor here.
//!
//! **This gate first landed with every one of this build's 128 shipped
//! locks named as [`PENDING`]** — nobody had yet written the negative
//! control that would move a given entry to [`NEGATIVE_CONTROLS`]. That
//! count was the finding: the number of locks nobody had yet proven
//! failable, and it was not a number anybody had before this gate walked
//! every shipped room and counted. Since then: seven locks shipped with a
//! control freshly written for them; two more existing locks were REWRITTEN
//! (kind-scoped to subject-scoped, fixing a false failure) and proven by a
//! control in the same move; and 63 more turned out to be proven ALREADY —
//! see [`NEGATIVE_CONTROLS`]'s own doc for all shapes. The backlog now
//! stands at 57 of 129, and every one of those 57 is a `vault.md` lock with
//! no existing proof of its own.

use crate::expectations::{BIKE_ROOM, HANDOVER_ROOM, LOOP_ROOM, VAULT_ROOM, YEAR_ROOM};
use crate::{expectations, lock};

/// **One lock a real negative control has been written for**, somewhere in
/// this crate's own tests: a sabotage that reddens exactly this lock and
/// leaves an independent lock unaffected, the same bar `scripts/sabotage`
/// holds code to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegativeControl<'a> {
    pub room: &'a str,
    pub lock: &'a str,
    /// **Where the proof lives**, relative to this crate's own manifest
    /// directory — `"tests/desk_lock.rs"`, never a workspace-rooted path.
    /// [`proof_exists`] resolves it the same way [`expectations::room_document`]
    /// resolves a room.
    pub file: &'a str,
    /// **The function that proves it**, bare — no `fn`, no arguments, no
    /// module path. [`proof_exists`] checks only that a function of this
    /// name is defined in `file`; it never runs it. See the module's own
    /// doc comment for why.
    pub function: &'a str,
    /// **Which of two things this proof actually establishes.** See
    /// [`Strength`] — the registry's whole point is that these are not the
    /// same claim, and nothing here may collapse them into one count.
    pub strength: Strength,
}

/// **Which of two things a control's proof establishes**, because a lock
/// reddening on an unworked room and a lock reddening on one specific wrong
/// act are different findings, and only one of them is evidence against the
/// hazard this gate exists for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    /// **Proves the lock notices an unworked room** — furnished, plausible,
    /// but nothing done. Real: it depends on the room's own furniture, not
    /// an empty store. But a lock vacuously satisfied by any non-empty
    /// state that merely never touched it passes this and nothing harder,
    /// so it is not evidence that the lock notices any SPECIFIC mistake.
    Blanket,
    /// **Proves the lock notices one specific wrong act, with everything
    /// else right** — the room is otherwise correctly worked, and only the
    /// one named mistake is present. This is the bar the gate exists for.
    Discriminating,
}

/// **Every lock this build has actually proven failable, so far.**
///
/// 72 entries, of five different shapes: 62 blanket and 10 discriminating.
/// Quote the discriminating count as ten, with one constructed positive —
/// never as a bare ten; see the note on `tests/fair_lock.rs` below for why.
///
/// Seven are freshly written, landing with their lock in the same slice:
/// `tests/desk_lock.rs` replays Run 23's exact real regression (a
/// `clear_fields` write nobody asked for, wiping the desk's return window);
/// `tests/wharf_lock.rs` replays another — a timing filed on an invented
/// entity rather than the furniture that already represents it;
/// `tests/furnace_lock.rs` replays a third — a deadline filed on the
/// service company rather than the thing it services;
/// `tests/trip_lock.rs` replays a fourth — a trip's dates filed as a
/// claim's own timing (`happened_at`/`happened_through`) rather than the
/// shipped `trip` type's own fields (`leaves_on`/`returns_on`);
/// `tests/fair_lock.rs` proves a fifth, with a real half and a constructed
/// one — see its own module doc; and `tests/spares_lock.rs` proves a sixth
/// and seventh from one real call — a summary about two things filed on a
/// topic, rather than a fresh touch on either thing itself. Each proves its
/// lock reddens for the real mistake, then proves it holds when filed
/// correctly. Nothing here was retrofit onto an older lock.
///
/// **`tests/fair_lock.rs` is not quite the same claim as the other six,
/// and `Strength` does not currently say so.** Its negative is a verbatim
/// replay — the real run's own capture call, unchanged. Its positive is
/// constructed: no sitting in the real run ever drew the connection this
/// room wants, so what the lock holds against is a plausible correct call
/// nobody actually made, not an observed one. The other six replay both
/// sides from the real run. Both are legitimate discriminating proofs —
/// the lock demonstrably tells the two states apart either way — but they
/// are different claims about how much of the scenario is observed versus
/// inferred, and `Strength::Discriminating` collapses that difference.
/// Carded rather than split into a third variant: one entry needing the
/// distinction is prose; a second would be evidence it belongs in the type.
///
/// Two are locks that were REWRITTEN rather than born here.
/// `tests/piano_lock.rs` covers both: April's and August's own check-in
/// locks used to read `{"kind": "rhythm", ...}` — a listing over every
/// rhythm — and a later, unrelated, entirely correct act (the operator
/// dropping the reminder, archived in December) silently excluded the
/// piano from that listing by the time every lock runs against the
/// finished room, failing a question that was actually settled on its own
/// day. Rewritten to name `rhythm:sit-at-the-piano` directly, the same way
/// this room's own December lock on this rhythm already does. The fix and
/// the control are one move: each rewritten lock is proven to hold once
/// archived (the false failure is gone) and to still redden when the
/// check-in genuinely was never written (the real failure still works).
///
/// One is `vault_room.rs`'s own: the everyday-listing count lock, already
/// proven by three real cases (nobody archived, the wrong two archived,
/// exactly the right one archived) that were simply never registered.
///
/// The other 62 were already proven and simply never registered.
/// `bike_room.rs`, `loop_room.rs`, `handover_room.rs` and `year_room.rs` each
/// carry a pair of cases that already do exactly what a control is for —
/// `every_check_fails_on_a_room_nobody_worked_in` (or, in `year_room.rs`,
/// `a_year_nobody_worked_in_fails_every_lock`) proves EVERY lock the room
/// ships reddens on a real, furnished, untouched room, and its sibling
/// (`every_check_holds_once_both_phases_are_worked` /
/// `every_lock_holds_once_the_year_is_worked`) proves every one holds once
/// the room's own goal is actually worked. Both run for real, through the
/// served surface, on every `cargo test` — the proof simply was not named
/// here. `vault.md` has no such driver — its own locks stay in [`PENDING`]
/// except the one `vault_room.rs` already covers.
pub const NEGATIVE_CONTROLS: &[NegativeControl<'static>] = &[
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 3 — March: the desk's return window was cleared outright rather than left \
               on record once the operator said only that the desk stays, so nothing later can \
               find the deadline that closed",
        file: "tests/desk_lock.rs",
        function: "the_desk_history_lock_reds_when_the_window_is_cleared_outright",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 13 — later December: the everyday listing of people does not say it left \
               exactly one out, so a browse with no reason to know Hugo ever existed has no way \
               to tell an empty vault from one quietly missing a name",
        file: "tests/vault_room.rs",
        function: "the_count_lock_reds_when_nobody_was_archived",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 2 — February: the wharf road does not carry the one timing as a write of \
               the key, so October cannot see that the verdict rests on one morning",
        file: "tests/wharf_lock.rs",
        function: "the_wharf_timing_lock_reds_when_filed_on_an_invented_entity",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 5 — May: the furnace does not carry the day its free service lapses, so \
               the one window nobody would think to call a deadline is not on record as a \
               date",
        file: "tests/furnace_lock.rs",
        function: "the_furnace_deadline_lock_reds_when_filed_on_the_service_company",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 9 — September: nothing carries the trip's dates under the keys the store \
               already asks by, so November's plan is either unrecorded or invisible to the \
               question \"when am I next away\"",
        file: "tests/trip_lock.rs",
        function: "the_trip_dates_lock_reds_when_filed_as_happened_at_and_through",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 9 — September: the fair carries no note from this sitting pointing at the \
               college, so either the sitting never found where the certificate comes from or \
               it wrote the answer where the fair cannot be walked to it",
        file: "tests/fair_lock.rs",
        function: "the_fair_college_lock_reds_when_no_connection_is_drawn",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 4 — April: no loop carries the twelfth as a check-in, so the piano's \
               record of being played is short a turn",
        file: "tests/piano_lock.rs",
        function: "aprils_lock_reds_when_the_check_in_was_never_written",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 8 — August: no loop carries the eleventh as a check-in, so the last \
               logged turn at the piano is missing and December's third silence starts on the \
               wrong day",
        file: "tests/piano_lock.rs",
        function: "augusts_lock_reds_when_the_check_in_was_never_written",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 11 — November: the glasses either got no answer today or the answer that \
               has been on record since January was not put on them",
        file: "tests/spares_lock.rs",
        function: "the_glasses_lock_reds_when_only_the_summary_is_filed_on_the_topic",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "Phase 11 — November: the keys do not carry today's answer pointing at Teddy, so \
               a spare that has been on record since February was either not found or written \
               where a question about Teddy will not reach it",
        file: "tests/spares_lock.rs",
        function: "the_keys_lock_reds_when_only_the_summary_is_filed_on_the_topic",
        strength: Strength::Discriminating,
    },
    NegativeControl {
        room: BIKE_ROOM,
        lock: "Phase 1 — the brief is still sitting new in the box, so the occupant never learnt what the work is",
        file: "tests/bike_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: BIKE_ROOM,
        lock: "Phase 1 — the service day went onto the bike as prose, so what has been done to it cannot be asked for",
        file: "tests/bike_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: BIKE_ROOM,
        lock: "Phase 1 — the distance key does not carry a third write with this year's number, so the year went somewhere a question cannot reach",
        file: "tests/bike_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: BIKE_ROOM,
        lock: "Phase 1 — the bike's distance does not read as this year's number, so the newest thing recorded about how far it goes is not this year",
        file: "tests/bike_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: BIKE_ROOM,
        lock: "Phase 1 — no message was left and no open run says what it was doing, so the next session arrives at what the occupant found and not at what it did",
        file: "tests/bike_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: LOOP_ROOM,
        lock: "Phase 1 — the brief is still sitting new in the box, so the occupant never learnt what the work is",
        file: "tests/loop_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: LOOP_ROOM,
        lock: "Phase 1 — one of the two jobs the brief named does not stand as a loop under the thing it belongs to",
        file: "tests/loop_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: LOOP_ROOM,
        lock: "Phase 1 — no loop with a sixty-day frequency hangs under the kettle carrying the day the brief gave it",
        file: "tests/loop_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: LOOP_ROOM,
        lock: "Phase 1 — the loop the operator keeps no schedule for was given one, which nobody said — or it is not under the filter at all",
        file: "tests/loop_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: LOOP_ROOM,
        lock: "Phase 2 — the cold session moved a loop that was not due, or left the one that was where it stood",
        file: "tests/loop_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 1 — January: the brief is still sitting new, so nobody took delivery of what the year is built on",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 1 — January: no loop carries the ninety days and the day it was last done, so nothing can ever fall due",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 1 — January: nothing on Milhouse points at Springfield, so the move was recorded somewhere a later sitting will not look",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: the pump the operator lent is not a thing jojobot knows, so September has nothing to ask about",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: nothing on the pump reaches Ralph, so who has it is only in the prose of a sitting that is gone",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: the club cannot be walked to its members, so August's question has no answer but a guess",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: nothing on the canoe carries this sitting's own day, so the soft spot the operator noticed today is not on record for December to draw on",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 3 — March: nothing says the club meets on Tuesdays, so July has nothing to take back",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 3 — March: nothing on the canoe carries this sitting's own day, so the patch the operator made today is not on record for December to draw on",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 4 — April: nothing on Milhouse points at Shelbyville, so the move was recorded somewhere a later sitting will not look",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 4 — April: the Springfield claim is either gone or still standing as current, and it should be there and marked as no longer true",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 5 — May: the washout was not filed against the trail that already existed — either nothing was filed, or a second trail was stood up to carry it",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 5 — May: nothing on the canoe carries this sitting's own day, so the varnish the operator put on today is not on record for December to draw on",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 6 — June: the survey cannot be walked to who was at it, so August's question is answerable only by reading prose",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 6 — June: the chain check does not say it was done on the day this sitting claims, so it is either untouched or stamped with the day the run happened",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 6 — June: no single record points at two different kinds of thing as handles, so the sitting wrote words where it could have written pointers and a later sitting has nothing to follow",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 7 — July: no retraction appeared on the club in this sitting's own window, so the March claim about Tuesdays was either left standing or rewritten in place instead of withdrawn",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 7 — July: nothing on the canoe carries this sitting's own day, so the foot brace the operator replaced today is not on record for December to draw on",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 8 — August: somebody who was never at the survey is now recorded as having been there",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 8 — August: nothing on the club carries this sitting's own day, so the one thing it was asked to record is not there",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 9 — nothing on the pump currently carries the day it came back, so a reader is left with no day to find — whether it was never recorded, or a later, legitimate correction cleared the only trace of it",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 9 — September: nothing on the canoe carries this sitting's own day, so the crack the operator patched today is not on record for December to draw on",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 10 — October: September's account of the pump was not corrected in place — either it still stands unrevised, or it was retracted rather than rewritten",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 10 — October: the survey cannot be walked to the place it was held at, so where it happened is in one sitting's sentence and nowhere a later reader of the event will look",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): Nelson's survey attendance is not marked taken back, so a session reading his page later still finds him at an event he never went to",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): Bart cannot be walked to the club, so he is a name in a transcript and nothing on the roster",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): somebody who was never at the survey was put there by this sitting, which was asked to take an attendance away rather than add one",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): June's own claim still renders the survey under the handle it wore before October's reason to rename it, so a stored mention is not resolving to what the thing is called now",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 13 — late November: the year's turns are not on file as derivations, so they were written by hand rather than checked in, and December has nothing to notice about what the loop's standing rests on",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 13 — late November: this sitting's day is not on the loop January opened — either a question asked in the operator's own words reached nothing and the turn went unrecorded, or it was filed on a second loop standing beside the first, and neither one can say when the chain was last done",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 14 — December: the loop that has gone quiet by now did not gain a record on this sitting's own day, so nothing here shows the sitting noticing what jojobot's own arithmetic already knows",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 14 — December: the overdue note does not name what it was worked out from, so a later reader has no way back to the claim that makes it arithmetic rather than a guess",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 14 — December: the walk back from the loop's own cadence does not reach the overdue note, so a later reader who follows the lineage forward finds nothing",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: what the bike lock's claim used to say is not on the record, so either the sitting never reached the correction's own history or it answered from the claim as it stands",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: a record's trace does not agree with what the run actually wrote to it, so the history read is inventing or losing a write and a reader is told jojobot changed its mind about something it never did",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: nothing marks a record as standing for the canoe's year of small repairs, so nobody folded the pile even though this sitting asked for exactly that",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the fold either lost one of the canoe's five repairs, left it retracted, or the mark does not actually name it — the full picture is supposed to still be one recall away",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the folded record states a date that appears in none of the five repairs it is supposed to be drawn from, which is the fabrication this mark exists to prevent",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike lock's correction did not land as one record with two writes, so either this sitting never corrected it or it split the correction into a retraction and a fresh claim instead of a plain rewrite",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: a plain read of the canoe — no stood_for — still hands back the pile behind the fold, so an agent that never learns the flag exists sees every repair the synthesis was supposed to shorten",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the canoe's own sources are not reachable even by name, so the lock above proves nothing about elision — only that nothing here works",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike's distance does not read as the year's two figures summed, so either they were never joined under a key jojobot folds, or one replaced the other instead of adding to it",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the survey's own planning claim does not read as still open, so either it was never recorded as a hedge or something settled it that had no business doing so",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the drivetrain job does not read as fixed to the operator's own word, so either the declaration was never kept past its own sitting or nothing here reached it",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the March job no longer reads as paid, so a word that was already right was painted over",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the floor pump's job does not read as fixed to the operator's own word, so the fold-in's second off-vocabulary word was never caught",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike lock's own job does not read as waived, so either it was never filed under the operator's own word or this sitting repainted it while fixing the others",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike does not carry what is actually invoiced across the year's jobs, so either the sitting did not select on the operator's own word or it summed something other than what the record says",
        file: "tests/year_room.rs",
        function: "a_year_nobody_worked_in_fails_every_lock",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: HANDOVER_ROOM,
        lock: "Phase 1 — the brief is still sitting new in the box, so the occupant never learnt what the work is",
        file: "tests/handover_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: HANDOVER_ROOM,
        lock: "Phase 1 — there is no second identity carrying a box of its own, so there is nobody to hand anything to",
        file: "tests/handover_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: HANDOVER_ROOM,
        lock: "Phase 1 — the reading pile is not waiting in the colleague's box as three separate things sent by the assistant",
        file: "tests/handover_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
    NegativeControl {
        room: HANDOVER_ROOM,
        lock: "Phase 2 — nothing on the colleague says how much of the pile is still waiting, so a later session has to go and find out again",
        file: "tests/handover_room.rs",
        function: "every_check_fails_on_a_room_nobody_worked_in",
        strength: Strength::Blanket,
    },
];

/// **One lock shipped with no negative control yet**, named rather than left
/// to a silent gap. Owed work, not an accepted risk: there is nothing here
/// to weigh, only a control nobody has written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending<'a> {
    pub room: &'a str,
    pub lock: &'a str,
}

/// **The backlog.** Every `vault.md` lock nobody has proven yet. `vault.md`
/// has no driver proving its own locks the way `bike_room.rs`,
/// `loop_room.rs`, `handover_room.rs` and `year_room.rs` each prove theirs
/// — see [`NEGATIVE_CONTROLS`]'s own doc — and `vault_room.rs` covers only
/// one lock, already moved there. As a control gets written for one, move
/// its entry from here to [`NEGATIVE_CONTROLS`].
pub const PENDING: &[Pending<'static>] = &[
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 1 — January: the brief is still sitting new, so nobody took delivery of the vault",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 1 — January: the laptop's cover does not read as the day it ends, so the one window the operator dated is not on record as a date",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 1 — January: the glasses carry no spare, so the one thing the operator said has a spare is not on record as having one",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 1 — January: the shed does not hold considering under the operator's own key, so December cannot see how long it has been going round",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 1 — January: the kitchen floor does not hold considering under the operator's own key, so it cannot be told apart from the shed when December asks which one moved",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 1 — January: the road does not carry the January timing as a write of the key the operator times by, so October has nothing to count",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 1 — January: the operator's own timing is on record as a guess rather than as their word, so every later question about what is in doubt starts from a store where everything is",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 2 — February: no thing carries the day the desk's return period ends, so a window the operator can still act on this month is not on record as a date",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 2 — February: the operator's \"never again\" is on record as a guess rather than as their word, so October cannot open a verdict that was never closed",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 2 — February: nothing on Ocean Avenue carries this sitting's own day, so the week the bridge was shut is not on record and the wharf timing stands as if it were an ordinary morning",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 2 — February: the keys carry no spare, so the set Teddy holds was filed under the friend rather than under the thing it is a spare of",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 2 — February: nothing on the shed carries this sitting's own day, so a month of going round on it is not on record for December to count",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 2 — February: no loop carries the third as a check-in, so the swim the operator reported is not on the record that decides when swimming went quiet",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 3 — March: no thing carries the day the boots' guarantee ends, so a window that closes inside December's horizon is not on record as a date",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 3 — March: nothing on the cat carries this sitting's own day, so the constraint that decides who can look after her in November is not on record",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 4 — April: nothing on the fair carries this sitting's own day, so the stall the operator said yes to is not on record and September has nothing to owe",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 4 — April: no thing is on record as being for Omicron, so what gets packed off with the server in October cannot be asked",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 4 — April: the kitchen floor does not hold doing, so the one project that moves this year does not read as having moved",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 4 — April: the road does not carry the April timing, so the count October needs is short",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 5 — May: nothing on Teddy carries this sitting's own day, so the one fact that rules him out as the cat's sitter is not on record",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 5 — May: nothing on the shed carries this sitting's own day, so another month of going round on it is not on record for December to count",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 6 — June: nothing on the college carries this sitting's own day, so what the course takes and when it starts is filed under whoever mentioned it, or nowhere",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 6 — June: no thing is on record as being for Theta, so October's selection has nothing to leave alone",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 6 — June: no loop carries the ninth as a check-in, so the Tuesday call is short a turn",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 7 — July: the kitchen floor does not hold done, so the project that finished reads like the one that never moved",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 7 — July: nothing carries the day the phone's cover ends, so December's selection has one fewer thing it must leave alone",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 7 — July: the road does not carry the July timing, so the count October needs is short",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 7 — July: no loop carries the fourteenth as a check-in, so the last swim before the pool shut is not on the record that decides when swimming went quiet",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 8 — August: nothing on the pool carries this sitting's own day, so the reason the swim went quiet is not on record anywhere",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 8 — August: nothing on the shed carries this sitting's own day, so the fourth month of going round on it is not on record for December to count",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 9 — September: the road does not carry the September timing, so the count October needs is short",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 9 — September: no loop carries the eighth as a check-in, so the Tuesday call is short a turn",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 10 — October: nothing on the wharf road reads as in doubt, so either the sitting never counted what the verdict rests on or it counted and left a one-morning rule standing as settled",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 10 — October: either the usual road no longer carries its four timings, or one of them was opened along with the wharf's — a verdict resting on four mornings was treated like one resting on one",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 10 — October: the thing that only works with Omicron carries no note from this sitting, so it goes wherever the boxes go",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 10 — October: the thing that only works with Theta was either lost or given a note it was not supposed to get, so the selection was on the key rather than on the machine",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 10 — October: no loop carries the twenty-ninth as a check-in, so the last call before the course took Tuesdays is not on record and December cannot see what crowded it out",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 10 — October: the course the operator actually started is not on record as a span with both ends, so a stretch of many Tuesdays reads as a single day or as prose a later question cannot compare a date against",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 11 — November: nothing on the trip points at the cat, so the one thing that has to be arranged before Thursday is either unnoticed or written where the trip cannot be walked to it",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 11 — November: the charger carries no open claim from this sitting, so the one spare nobody ever spoke about was either asserted or skipped",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 12 — December: the boots' guarantee closes inside the horizon and got no note, so a window that was mentioned once in March converts to a closed one in silence",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 12 — December: the furnace's free service lapses inside the horizon and got no note, so the one window nobody would call a deadline is the one that is missed",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 12 — December: the desk's return period closed in March and was flagged anyway, so the comparison ran one way and a closed window was offered as an open one",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 12 — December: the laptop's cover runs to 2028 and was given a note anyway, so the selection was on the key rather than on the date",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 12 — December: the phone's cover runs to 2028 and was given a note anyway, so the selection was on the key rather than on the date",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 12 — December: the key the operator dates windows by cannot be compared as a date, so the question \"what runs out by the end of March\" is unaskable and was answered, if at all, by reading every thing there is",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 12 — December: asking what is overdue across every runs_out thing, naming no kind, either misses the one window that has already passed or fails to say how many of the rest it correctly left out",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: no loop carries the day of the party, so the one silence the operator did not actually let happen was not logged from the record that proves it",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: either the call was touched when the operator said leave it, or the piano still reads as quiet after a sitting that was told to log it from whatever it could find",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: no loop carries the day the pool reopens, so the silence that could not be helped was either not explained from the record or explained in words rather than a date",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: the shed carries no decide_by, so a year of going round on it reads as in progress rather than as stuck",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: the kitchen floor was given a decide_by, so a project that went considering, doing, done was treated like one that never left considering",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: the piano's loop still carries a cadence, so asking jojobot to stop reminding altogether left the schedule standing rather than dropping it",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: the piano reads overdue again by next June, so today's turn only postponed the reminder rather than actually dropping the schedule that would have raised it",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: Hugo carries no archived reason, so a name the operator does not recognise is still standing in the vault as if it belonged there",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: the shed cannot be found by the shipped decide-by question without naming it directly, so a general \"what have I been putting off\" would reach nothing where the operator's own key actually landed",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: an ordinary browse of every person still turns up Hugo after he was taken out, so archiving him did not actually remove him from the everyday read that finds everyone else",
    },
];

/// **Whether a lock, named by its room and its own name, is accounted for** —
/// proven failable, or named as owed work. Taking both lists as parameters
/// rather than reading the constants is what lets the mechanism be proven on
/// its own, synthetic data — see the gate's own test in
/// `tests/failability_gate.rs`.
pub fn is_named(
    room: &str,
    name: &str,
    controls: &[NegativeControl<'_>],
    pending: &[Pending<'_>],
) -> bool {
    controls.iter().any(|c| c.room == room && c.lock == name)
        || pending.iter().any(|p| p.room == room && p.lock == name)
}

/// **The two populations, counted apart.** Never collapse this into one
/// number: `(blanket, discriminating)` answers two different questions, and
/// their sum answers neither — "N locks are proven failable" is true of no
/// population here and would tell a reader nothing about which bar was
/// actually cleared.
pub fn tally(controls: &[NegativeControl<'_>]) -> (usize, usize) {
    let blanket = controls
        .iter()
        .filter(|c| c.strength == Strength::Blanket)
        .count();
    let discriminating = controls
        .iter()
        .filter(|c| c.strength == Strength::Discriminating)
        .count();
    (blanket, discriminating)
}

/// **Every lock in every shipped room, named by room and by name** — the
/// shape both the gate and its own rot checks walk.
pub fn shipped_locks() -> Vec<(String, String)> {
    expectations::shipped_rooms()
        .flat_map(|room| {
            lock::locks_of(room)
                .into_iter()
                .map(move |l| (room.to_string(), l.name))
        })
        .collect()
}

/// **Whether a registered control's proof still names something real** —
/// mechanically, by reading the file it names for a function of the name it
/// also names. This never runs that function; see the module doc for why.
///
/// Two ways to fail, and they are told apart because they send a reader to
/// different places: the file itself is gone, or the file is there but
/// nothing in it is called that.
pub fn proof_exists(control: &NegativeControl<'_>) -> Result<(), String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(control.file);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "{}: the proof names a file that does not exist: {e}",
            control.file,
        )
    })?;
    if !defines_function(&text, control.function) {
        return Err(format!(
            "{}: the proof names no function called {} in that file",
            control.file, control.function,
        ));
    }
    Ok(())
}

/// **Whether `text` defines a function called exactly `name`.**
///
/// `text.contains("fn {name}")` is not this: a function renamed by APPENDING
/// to it — `..._reds_when_the_window_is_cleared_outright_v2` — still contains
/// the old name as its own prefix, so a bare substring test would read a
/// renamed function as the one that was renamed away. This checks the
/// character right after the match too, and only counts it when that
/// character cannot continue an identifier — the boundary a real rename
/// actually crosses.
fn defines_function(text: &str, name: &str) -> bool {
    let needle = format!("fn {name}");
    text.match_indices(&needle).any(|(at, _)| {
        text[at + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The gate itself, proven on a lock nothing shipped writes.** A bare
    /// room/lock pair is unnamed against empty lists — this is the sabotage
    /// the gate is FOR: remove a lock's registration (real or planted) and
    /// it reads as unnamed. Naming it in either list clears it, because a
    /// control and a pending entry are two different answers to the same
    /// question and either one is a decision somebody made.
    #[test]
    fn an_unnamed_lock_fails_the_gate_and_naming_it_either_way_clears_it() {
        let room = "not-a-shipped-room";
        let name = "a lock nothing here ships";

        assert!(
            !is_named(room, name, &[], &[]),
            "a lock named in neither list read as accounted for — the gate would pass anything",
        );

        let pending = [Pending { room, lock: name }];
        assert!(
            is_named(room, name, &[], &pending),
            "naming the lock as pending did not clear it",
        );

        let controls = [NegativeControl {
            room,
            lock: name,
            file: "not-a-real-file.rs",
            function: "not_a_real_function",
            strength: Strength::Discriminating,
        }];
        assert!(
            is_named(room, name, &controls, &[]),
            "naming the lock with a registered control did not clear it",
        );
    }

    /// **A dangling proof fails for the reason that is actually wrong**, not
    /// merely "some error" — a file that is not there and a function that is
    /// not in a real file send a reader to different places, and a check
    /// that collapsed them would leave a reader hunting for a missing file
    /// that is really a missing function, or the other way round. The
    /// positive is a real, currently-shipped control: [`NEGATIVE_CONTROLS`]'s
    /// own first entry, which must resolve, or nothing here would be measuring
    /// anything.
    #[test]
    fn a_dangling_proof_fails_for_the_reason_that_is_actually_wrong() {
        let missing_file = NegativeControl {
            room: "not-a-shipped-room",
            lock: "not a real lock",
            file: "tests/this_file_does_not_exist_anywhere.rs",
            function: "whatever",
            strength: Strength::Discriminating,
        };
        let err = proof_exists(&missing_file).expect_err("a missing file must fail");
        assert!(
            err.contains("does not exist"),
            "a missing file did not fail for that reason: {err}",
        );

        let missing_function = NegativeControl {
            room: "not-a-shipped-room",
            lock: "not a real lock",
            file: "tests/desk_lock.rs",
            function: "a_function_that_is_not_actually_there",
            strength: Strength::Discriminating,
        };
        let err = proof_exists(&missing_function).expect_err("a missing function must fail");
        assert!(
            err.contains("no function called"),
            "a missing function did not fail for that reason: {err}",
        );

        assert!(
            proof_exists(&NEGATIVE_CONTROLS[0]).is_ok(),
            "the one real registered control does not even resolve",
        );
    }

    /// **A function renamed by appending to it is not the function that was
    /// renamed away**, and this is the case a bare substring test gets
    /// wrong: `"fn the_original".contains("fn the_original")` is true of
    /// `"fn the_original_v2"` too, because the old name is that new name's
    /// own prefix. Both halves: the exact name still matches, and a longer
    /// name that merely starts with it does not.
    #[test]
    fn a_name_extended_by_a_suffix_is_not_a_match_for_the_original() {
        assert!(
            defines_function("async fn the_original() {}", "the_original"),
            "the exact name was not found",
        );
        assert!(
            !defines_function("async fn the_original_v2() {}", "the_original"),
            "a longer name that only starts with the searched one was read as a match",
        );
    }
}

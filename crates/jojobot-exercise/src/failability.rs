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
//! Registering one here is a name and a pointer to where that proof lives —
//! it is a claim about this crate's other tests, not something this module
//! executes. **That is this gate's stated boundary**: it checks that every
//! lock is NAMED — by a proof already written, or by an entry admitting none
//! exists yet — never that a named proof still actually reddens the lock it
//! claims to. Verifying a `NegativeControl`'s own claim would mean running
//! the proof it points at as part of the gate, which needs the proof
//! expressed as callable Rust the gate can invoke rather than a name a
//! reader has to go and check; nothing here does that yet, and
//! [`NEGATIVE_CONTROLS`] is empty, so the gap costs nothing today. The day
//! the first real entry lands is the day to revisit this rather than before.
//!
//! **This slice does not retrofit a control onto any shipped lock.** It
//! lands the registry and the gate with every one of this build's 128
//! shipped locks named as [`PENDING`] — nobody has yet written the negative
//! control that would move a given entry to [`NEGATIVE_CONTROLS`]. That
//! count is the finding: it is the number of locks nobody has yet proven
//! failable, and it was not a number anybody had before this gate walked
//! every shipped room and counted.

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
    /// The test that proves it, named so a reader can go and run it —
    /// never executed by this module itself. See the module's own doc
    /// comment for why.
    pub proof: &'a str,
}

/// **Every lock this build has actually proven failable, so far.**
///
/// One entry. Nothing in this slice retrofits a control onto a lock that
/// shipped before it — see the module doc — but this lock and its control
/// landed together: `tests/desk_lock.rs` replays Run 23's exact real
/// regression (a `clear_fields` write nobody asked for, wiping the desk's
/// return window) and proves the lock reddens for it, then proves it holds
/// when the window is left alone. An entry moves here from [`PENDING`] the
/// day somebody writes the sabotage that proves it, never before.
pub const NEGATIVE_CONTROLS: &[NegativeControl<'static>] = &[NegativeControl {
    room: expectations::VAULT_ROOM,
    lock: "Phase 3 — March: the desk's return window was cleared outright rather than left on \
           record once the operator said only that the desk stays, so nothing later can find \
           the deadline that closed",
    proof: "jojobot-exercise/tests/desk_lock.rs: \
            the_desk_history_lock_reds_when_the_window_is_cleared_outright",
}];

/// **One lock shipped with no negative control yet**, named rather than left
/// to a silent gap. Owed work, not an accepted risk: there is nothing here
/// to weigh, only a control nobody has written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending<'a> {
    pub room: &'a str,
    pub lock: &'a str,
}

/// **The backlog, as of this gate landing.** Every shipped lock, because
/// [`NEGATIVE_CONTROLS`] is empty — see the module doc for why this slice
/// does not close any of them. As a control gets written for one, move its
/// entry from here to [`NEGATIVE_CONTROLS`].
pub const PENDING: &[Pending<'static>] = &[
    Pending {
        room: BIKE_ROOM,
        lock: "Phase 1 — the brief is still sitting new in the box, so the occupant never learnt what the work is",
    },
    Pending {
        room: BIKE_ROOM,
        lock: "Phase 1 — the service day went onto the bike as prose, so what has been done to it cannot be asked for",
    },
    Pending {
        room: BIKE_ROOM,
        lock: "Phase 1 — the distance key does not carry a third write with this year's number, so the year went somewhere a question cannot reach",
    },
    Pending {
        room: BIKE_ROOM,
        lock: "Phase 1 — the bike's distance does not read as this year's number, so the newest thing recorded about how far it goes is not this year",
    },
    Pending {
        room: BIKE_ROOM,
        lock: "Phase 1 — no message was left and no open run says what it was doing, so the next session arrives at what the occupant found and not at what it did",
    },
    Pending {
        room: LOOP_ROOM,
        lock: "Phase 1 — the brief is still sitting new in the box, so the occupant never learnt what the work is",
    },
    Pending {
        room: LOOP_ROOM,
        lock: "Phase 1 — one of the two jobs the brief named does not stand as a loop under the thing it belongs to",
    },
    Pending {
        room: LOOP_ROOM,
        lock: "Phase 1 — no loop with a sixty-day frequency hangs under the kettle carrying the day the brief gave it",
    },
    Pending {
        room: LOOP_ROOM,
        lock: "Phase 1 — the loop the operator keeps no schedule for was given one, which nobody said — or it is not under the filter at all",
    },
    Pending {
        room: LOOP_ROOM,
        lock: "Phase 2 — the cold session moved a loop that was not due, or left the one that was where it stood",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 1 — January: the brief is still sitting new, so nobody took delivery of what the year is built on",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 1 — January: no loop carries the ninety days and the day it was last done, so nothing can ever fall due",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 1 — January: nothing on Milhouse points at Springfield, so the move was recorded somewhere a later sitting will not look",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: the pump the operator lent is not a thing jojobot knows, so September has nothing to ask about",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: nothing on the pump reaches Ralph, so who has it is only in the prose of a sitting that is gone",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: the club cannot be walked to its members, so August's question has no answer but a guess",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 2 — February: nothing on the canoe carries this sitting's own day, so the soft spot the operator noticed today is not on record for December to draw on",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 3 — March: nothing says the club meets on Tuesdays, so July has nothing to take back",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 3 — March: nothing on the canoe carries this sitting's own day, so the patch the operator made today is not on record for December to draw on",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 4 — April: nothing on Milhouse points at Shelbyville, so the move was recorded somewhere a later sitting will not look",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 4 — April: the Springfield claim is either gone or still standing as current, and it should be there and marked as no longer true",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 5 — May: the washout was not filed against the trail that already existed — either nothing was filed, or a second trail was stood up to carry it",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 5 — May: nothing on the canoe carries this sitting's own day, so the varnish the operator put on today is not on record for December to draw on",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 6 — June: the survey cannot be walked to who was at it, so August's question is answerable only by reading prose",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 6 — June: the chain check does not say it was done on the day this sitting claims, so it is either untouched or stamped with the day the run happened",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 6 — June: no single record points at two different kinds of thing as handles, so the sitting wrote words where it could have written pointers and a later sitting has nothing to follow",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 7 — July: no retraction appeared on the club in this sitting's own window, so the March claim about Tuesdays was either left standing or rewritten in place instead of withdrawn",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 7 — July: nothing on the canoe carries this sitting's own day, so the foot brace the operator replaced today is not on record for December to draw on",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 8 — August: somebody who was never at the survey is now recorded as having been there",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 8 — August: nothing on the club carries this sitting's own day, so the one thing it was asked to record is not there",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 9 — nothing on the pump currently carries the day it came back, so a reader is left with no day to find — whether it was never recorded, or a later, legitimate correction cleared the only trace of it",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 9 — September: nothing on the canoe carries this sitting's own day, so the crack the operator patched today is not on record for December to draw on",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 10 — October: September's account of the pump was not corrected in place — either it still stands unrevised, or it was retracted rather than rewritten",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 10 — October: the survey cannot be walked to the place it was held at, so where it happened is in one sitting's sentence and nowhere a later reader of the event will look",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): Nelson's survey attendance is not marked taken back, so a session reading his page later still finds him at an event he never went to",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): Bart cannot be walked to the club, so he is a name in a transcript and nothing on the roster",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): somebody who was never at the survey was put there by this sitting, which was asked to take an attendance away rather than add one",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 11 — October (again): June's own claim still renders the survey under the handle it wore before October's reason to rename it, so a stored mention is not resolving to what the thing is called now",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 13 — late November: the year's turns are not on file as derivations, so they were written by hand rather than checked in, and December has nothing to notice about what the loop's standing rests on",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 13 — late November: this sitting's day is not on the loop January opened — either a question asked in the operator's own words reached nothing and the turn went unrecorded, or it was filed on a second loop standing beside the first, and neither one can say when the chain was last done",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 14 — December: the loop that has gone quiet by now did not gain a record on this sitting's own day, so nothing here shows the sitting noticing what jojobot's own arithmetic already knows",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 14 — December: the overdue note does not name what it was worked out from, so a later reader has no way back to the claim that makes it arithmetic rather than a guess",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 14 — December: the walk back from the loop's own cadence does not reach the overdue note, so a later reader who follows the lineage forward finds nothing",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: what the bike lock's claim used to say is not on the record, so either the sitting never reached the correction's own history or it answered from the claim as it stands",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: a record's trace does not agree with what the run actually wrote to it, so the history read is inventing or losing a write and a reader is told jojobot changed its mind about something it never did",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: nothing marks a record as standing for the canoe's year of small repairs, so nobody folded the pile even though this sitting asked for exactly that",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the fold either lost one of the canoe's five repairs, left it retracted, or the mark does not actually name it — the full picture is supposed to still be one recall away",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the folded record states a date that appears in none of the five repairs it is supposed to be drawn from, which is the fabrication this mark exists to prevent",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike lock's correction did not land as one record with two writes, so either this sitting never corrected it or it split the correction into a retraction and a fresh claim instead of a plain rewrite",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: a plain read of the canoe — no stood_for — still hands back the pile behind the fold, so an agent that never learns the flag exists sees every repair the synthesis was supposed to shorten",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the canoe's own sources are not reachable even by name, so the lock above proves nothing about elision — only that nothing here works",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike's distance does not read as the year's two figures summed, so either they were never joined under a key jojobot folds, or one replaced the other instead of adding to it",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the survey's own planning claim does not read as still open, so either it was never recorded as a hedge or something settled it that had no business doing so",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the drivetrain job does not read as fixed to the operator's own word, so either the declaration was never kept past its own sitting or nothing here reached it",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the March job no longer reads as paid, so a word that was already right was painted over",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the floor pump's job does not read as fixed to the operator's own word, so the fold-in's second off-vocabulary word was never caught",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike lock's own job does not read as waived, so either it was never filed under the operator's own word or this sitting repainted it while fixing the others",
    },
    Pending {
        room: YEAR_ROOM,
        lock: "Phase 15 — later December: the bike does not carry what is actually invoiced across the year's jobs, so either the sitting did not select on the operator's own word or it summed something other than what the record says",
    },
    Pending {
        room: HANDOVER_ROOM,
        lock: "Phase 1 — the brief is still sitting new in the box, so the occupant never learnt what the work is",
    },
    Pending {
        room: HANDOVER_ROOM,
        lock: "Phase 1 — there is no second identity carrying a box of its own, so there is nobody to hand anything to",
    },
    Pending {
        room: HANDOVER_ROOM,
        lock: "Phase 1 — the reading pile is not waiting in the colleague's box as three separate things sent by the assistant",
    },
    Pending {
        room: HANDOVER_ROOM,
        lock: "Phase 2 — nothing on the colleague says how much of the pile is still waiting, so a later session has to go and find out again",
    },
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
        lock: "Phase 2 — February: the wharf road does not carry the one timing as a write of the key, so October cannot see that the verdict rests on one morning",
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
        lock: "Phase 4 — April: no loop carries the twelfth as a check-in, so the piano's record of being played is short a turn",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 5 — May: the furnace does not carry the day its free service lapses, so the one window nobody would think to call a deadline is not on record as a date",
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
        lock: "Phase 8 — August: no loop carries the eleventh as a check-in, so the last logged turn at the piano is missing and December's third silence starts on the wrong day",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 9 — September: the fair carries no note from this sitting pointing at the college, so either the sitting never found where the certificate comes from or it wrote the answer where the fair cannot be walked to it",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 9 — September: nothing carries the trip's dates under the keys the store already asks by, so November's plan is either unrecorded or invisible to the question \"when am I next away\"",
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
        lock: "Phase 11 — November: the glasses either got no answer today or the answer that has been on record since January was not put on them",
    },
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 11 — November: the keys do not carry today's answer pointing at Teddy, so a spare that has been on record since February was either not found or written where a question about Teddy will not reach it",
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
    Pending {
        room: VAULT_ROOM,
        lock: "Phase 13 — later December: the everyday listing of people does not say it left exactly one out, so a browse with no reason to know Hugo ever existed has no way to tell an empty vault from one quietly missing a name",
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
            proof: "planted for the gate's own proof, not a real control",
        }];
        assert!(
            is_named(room, name, &controls, &[]),
            "naming the lock with a registered control did not clear it",
        );
    }
}

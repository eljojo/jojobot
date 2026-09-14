# The year

**Fourteen sittings inside one year, none of them remembering the one before.**
The operator uses jojobot for a year and the year takes minutes on the clock.

Every other room asks whether one thing works. This one asks whether a year of
ordinary use accumulates into something a session arriving blind can use — and
the only way to fail it is to have written badly in a month nobody is looking
at any more.

## The fiction, and where it is real

**Nothing in jojobot learns about elapsed time.** It reads no clock and it
stamps a write with real today only when the caller sends no date. So the year
is fiction inside the test, and it holds **exactly as far as each sitting
carries its own day into the calls it makes**.

Each sitting therefore says its date twice: in the entry, where the occupant
reads it, and in the `**Day:**` marker, where the harness does. **The run
generates the assertion that the two agree** — nobody writes it, and a sitting
that was told it is March and wrote under the day the run happened fails
saying so. That is not the harness catching itself: it is a real session's
most likely silent mistake, caught where a person reads it.

**One sitting is asked about something that happened on an earlier day, and
names that day in a separate field.** A date says when a claim is true of and
not when somebody typed it, so the earlier day lands there. The record itself
is still written on this sitting's own day, so the run's generated assertion
checks it exactly as it does for every other sitting; that sitting's own lock,
below, asks about the separate field instead.

## Two facts every entry opens with

A run of this room once turned up four sittings that asked a question and
wrote nothing at all: two stalled at the front door over which session to
pick up, one stopped to ask which of two accounts was correct when the room's
own design says both stand, and one stopped over a rhythm detail it could
have recorded without. **Saying nobody is at the keyboard did not fix this,
run after run**, because it says only that a question is futile — it gives a
sitting no audience and no reason to finish the work either.

**There is no operator to answer, but there is a reader, and finishing is not
optional.** Each sitting is one turn, and a question spends the whole of it.
So every entry below opens with two facts about the world it is playing in:
that it is a role-play and the day it names is today, and that nobody will
answer, but the answer is being read and one is required.

## Why cold sittings and not one long one

A single session answers from its own context. It wrote the thing an hour ago,
so it remembers, and the store is optional. **The forgetting between sittings
is the instrument.** A sitting arriving blind has only what jojobot can hand
it, so a badly shaped write in February costs somebody something in September
and not before.

**So the locks that matter are late and the work that earns them is early.**
Nothing in the second half of the year is reachable by a run that started from
nothing.

## What the room holds before the occupant arrives

Almost nothing, on purpose: the operator is starting. Five nouns and one brief.
**Everything else in this document is built by the sittings**, which is what
makes a later one depend on an earlier one rather than on the furniture.

**Two of the five nouns are named alike and both are real** — `org:north-trail-club`
is the club and `place:north-trail` is the trail it is named for. Neither is a
mistake, and a sitting asked about "the north trail" has two right answers to
choose between. That is the shape a real store grows on its own.

⚠️ **Nothing here is dated on a day any sitting claims.** A record already
carrying a sitting's day would make that sitting's generated assertion hold
whatever the occupant did, and the run refuses rather than holding.

```world
entity  person:milhouse | Milhouse
entity  place:springfield | Springfield
entity  place:shelbyville | Shelbyville
entity  thing:gravel-bike | The Gravel Bike
entity  org:north-trail-club | The North Trail Club
entity  place:north-trail | The North Trail

# The operator's own words, and the whole of what the first sitting is told
# beyond its entry line. An indented line continues the one above it.
message assistant | starting to keep track of things | I am going to start keeping track of things here, so take this down properly rather than as a note to yourself.

    The gravel bike: its chain wants looking at every ninety days and I last did it on 2025-12-20. If I let one slide, the next ninety days should run from when I actually did it rather than from when it was meant to happen.

    I ride with the North Trail Club. Milhouse is in it and he lives in Springfield. The club is running a trail survey in June and I mean to be there.

    Leave it so whoever picks this up in a month has what they need.
```

## Phase 1 — January, the year begins

**Session: fresh.** **Day: 2026-01-12.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 12 January 2026 and I want to get set up

```locks
# The brief left the box. A lock carries no session, so the board is read
# rather than a box.
#
# ⚠️ **A named check rather than a query, for the reason every other room here
# uses the same one.** A query can ask whether the board holds a message in
# `new`; it cannot say WHICH message. So `lacks "state":"new"` was a claim
# about every message on the board, and it went red the moment this sitting
# left a note for the next one — which the brief asks for in the operator's own
# words. The other half was worse: the text it looked for is the brief's own,
# furnished before the occupant arrived, so it held whatever the occupant did
# and the whole check rested on the negative.
#
# The check reads the OLDEST message on the board, which is always the brief,
# and asks about that one.
check   the_brief_left_the_box
say     January: the brief is still sitting new, so nobody took delivery of what the year is built on

# The loop. A chain check written as a sentence is not a late loop — it is not
# a loop — and June is where that costs something.
#
# Read off the key's own HISTORY rather than what it holds now, because June
# moves what it holds now. Every write of a key is the other question the same
# rows answer, and it is what lets a lock about January survive June.
#
# ⚠️ **The frequency is a SELECTION on the record, not an assertion on the
# fold.** A `fields` filter asks what the thing HOLDS unless it says otherwise,
# so `carries "cadence_days":"90"` was a claim that January's write is still the
# newest one — which is a lock on write order rather than on January. `scope:
# record` asks whether any record ever wrote it, which is what this lock is
# about, and no later sitting can take it away.
recall {"kind": "rhythm", "fields": [{"key": "cadence_days", "value": "90", "scope": "record"}], "history": "last_check_in"}
carries "value":"2025-12-20"
say     January: no loop carries the ninety days and the day it was last done, so nothing can ever fall due

# Recorded early, needed late: where Milhouse lives. April moves him and
# November asks.
#
# 🚨 **A hatch, scoped to January's own window — not the same question as
# April's own Springfield lock, which used to be this lock's own byte-for-byte
# copy.** Asked of the finished board, one check for "the claim was made" and
# one for "the claim was later archived without being retracted" convict
# whichever sitting is named for whatever the OTHER one did or did not do.
# January's job is only that the claim exists at all; April's is what happens
# to it next.
check   januarys_note_drew_a_standing_location_edge_to_springfield
say     January: nothing on Milhouse points at Springfield, so the move was recorded somewhere a later sitting will not look
```

## Phase 2 — February, a thing lent

**Session: fresh.** **Day: 2026-02-08.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 8 February 2026, I lent Ralph my floor pump today and I want it back before the June survey, and Nelson has joined the club, and I picked up a used canoe at a yard sale — there is a soft spot in the hull near the bow I should keep an eye on

```locks
# The commitment. September asks whether it ever came back, and a September
# session can only answer from what this one wrote.
recall {"kind": "thing"}
carries thing:floor-pump
say     February: the pump the operator lent is not a thing jojobot knows, so September has nothing to ask about

# ⚠️ **Scoped to this sitting's own window.** `carries person:ralph` on the
# finished room was ambiguous rather than wrong: September draws a second link
# at the same person on the same subject when the pump comes back, so a
# February that recorded nothing held this lock on September's work. Reading
# the phases missed it twice; a needle that matches, and also matches something
# else, is invisible to a reader.
check   the_pump_reached_its_holder_in_february
say     February: nothing on the pump reaches Ralph, so who has it is only in the prose of a sitting that is gone

# The walk August needs. A membership written as a sentence is not an edge.
#
# 🚨 **A hatch, scoped to February's own window.** Asked of the finished
# board, `carries person:nelson` and `carries person:milhouse` hold on a
# retracted membership exactly as they hold on a standing one — nothing here
# retracts either today, but the gap is the same shape June's attendance walk
# carried before its own fix, and a `carries` needle cannot correlate the
# edge and a status key on the same record either way.
check   februarys_club_drew_a_standing_member_for_each
say     February: the club cannot be walked to its members, so August's question has no answer but a guess

# The first of five small repairs on the canoe, scattered across the year.
# December asks the operator's real question about the pile they add up to,
# and nothing before then names a mark or a fold.
recall {"subject": "thing:canoe", "facts": true, "stood_for": true}
carries "recorded_at":"2026-02-08"
say     February: nothing on the canoe carries this sitting's own day, so the soft spot the operator noticed today is not on record for December to draw on
```

## Phase 3 — March, something the operator will take back

**Session: fresh.** **Day: 2026-03-15.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 15 March 2026 and the club meets on Tuesdays, put that down, and I patched that soft spot in the canoe's hull

```locks
# ⚠️ **A named check, because this is the one sitting the finished room cannot
# answer for.** July rewrites what March wrote, in place and under July's own
# day, and editing a claim destroys what it said before — so at the end of the
# run nothing is dated March and nothing says what March said. The word
# "Tuesday" survived only because the corrected sentence happens to keep it,
# which is a check standing on an accident rather than on the record.
#
# The check reads the world either side of this sitting and asks whether the
# club gained a record in that window. Only this sitting writes there.
check   the_club_was_given_a_claim_in_march
say     March: nothing says the club meets on Tuesdays, so July has nothing to take back

# The second small repair. Same reason as February's own note beside its
# first one: no mark, no fold, until December asks.
recall {"subject": "thing:canoe", "facts": true, "stood_for": true}
carries "recorded_at":"2026-03-15"
say     March: nothing on the canoe carries this sitting's own day, so the patch the operator made today is not on record for December to draw on
```

## Phase 4 — April, a fact that changed

**Session: fresh.** **Day: 2026-04-19.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 19 April 2026 and Milhouse has moved to Shelbyville

```locks
# 🚨 **A hatch, scoped to April's own window.** Asked of the finished board,
# `carries place:shelbyville` holds on a retracted claim exactly as on a
# standing one — nothing here retracts it today, but the gap is the same
# shape June's attendance walk carried before its own fix.
check   aprils_move_drew_a_standing_location_edge
say     April: nothing on Milhouse points at Shelbyville, so the move was recorded somewhere a later sitting will not look

# The old claim was true in its day. Taking it out loses that he ever lived
# there; leaving it current gives November two towns and no way to choose.
#
# 🚨 **A hatch, correlated on Springfield's own record rather than two
# independent substrings of Milhouse's whole answer.** A bare `"status"` and
# a bare `"retracts"` needle each hold if EITHER of Milhouse's records
# satisfies it — his Shelbyville edge could carry one, his Springfield one
# the other, and the lock would still pass having correlated nothing. This
# reads Springfield's own address and asks both questions of it alone: does
# IT read archived, and does no OTHER record retract IT specifically.
check   aprils_move_archives_the_springfield_claim
say     April: the Springfield claim is either gone or still standing as current, and it should be there and marked as no longer true
```

## Phase 5 — May, two names close enough to confuse

**Session: fresh.** **Day: 2026-05-10.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 10 May 2026 and the north trail was washed out at the top end this spring, put it where it belongs, and I gave the canoe seat a coat of varnish since it was getting rough

```locks
# The trail and the club are both real and neither is a typo. What is locked is
# that the washout reached the trail that already existed: the near-miss guard
# is there to be met, and a session that pushes past it leaves the store with
# two trails.
#
# One lock rather than two, because "no second trail" is true of a room nobody
# worked in. The count is what makes the absence mean something.
recall {"kind": "place", "facts": true}
at least 1 of "subject":"place:north-trail"
lacks   place:north-trail-2
say     May: the washout was not filed against the trail that already existed — either nothing was filed, or a second trail was stood up to carry it

# The third small repair, same reason as before.
recall {"subject": "thing:canoe", "facts": true, "stood_for": true}
carries "recorded_at":"2026-05-10"
say     May: nothing on the canoe carries this sitting's own day, so the varnish the operator put on today is not on record for December to draw on
```

## Phase 6 — June, the survey, and the loop that went quiet

**Session: fresh.** **Day: 2026-06-14.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 14 June 2026, the survey happened today up on the north trail and Milhouse and Nelson were both there, and I did the bike chain this morning

```locks
# Attendance is an edge. August walks it.
#
# 🚨 **A hatch, scoped to June's own window.** Late October legitimately
# retracts Nelson's own attendance at this same survey, and a retraction is
# marked rather than filtered — `recall` serves the retracted record's own
# edge back, so `carries person:nelson` could not see the `status` key
# beside it. Asked of the finished board this held on that dead text; asked
# of June's own window it asks only what June itself left standing.
check   junes_survey_drew_a_standing_attendee_for_each
say     June: the survey cannot be walked to who was at it, so August's question is answerable only by reading prose

# The loop moved, and it moved to today rather than to whenever the run
# happens. This is the sitting where the year's arithmetic is visible.
#
# ⚠️ **The key's own HISTORY rather than what it holds now**, for the reason
# January gives one lock above: the newest write wins the fold, so a lock on
# the fold is a lock on being the last sitting to write the key. Late November
# writes it again, and a lock reading the fold here would report June untouched
# because a later sitting did its job.
recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-06-14"
say     June: the chain check does not say it was done on the day this sitting claims, so it is either untouched or stamped with the day the run happened

# 🚨 **A handle written INTO a sentence, which no sitting has ever done.** The
# operator names several things in one breath, and a session that writes them as
# words leaves text; a session that writes them as handles leaves pointers a
# later sitting can follow.
#
# ⚠️ **A hatch, because an assertion is a substring of the WHOLE answer.** Two
# `carries` lines would hold on two separate records naming one thing each,
# which is the easy case and not the one the operator asked for. Whether two
# landed on ONE record is a correlation inside one object, and this format's
# three words do not branch — the same reason the loop's own lock two sittings
# from here is a hatch.
#
# ⛔️ **No handle is named here and no KIND is either.** January invents the
# event's handle, so a lock that spelled it would fail a run whose January
# called it something else — and naming the kinds is the same fault one level
# up, because which things a sitting points at is the sitting's own choice. A
# claim linking the club to the trail did the identical thing.
#
# 🚨 **The floor is TWO KINDS ON ONE RECORD, and it used to be a person, a place
# and an event together.** No sitting in this year is asked to name all three in
# one sentence, so a run that wrote pointers on half its claims failed a lock
# about pointers — a red that survives the fix it asks for, which is a red
# nobody trusts the next time it fires. Two mentions of DIFFERENT kinds on one
# record is the least that is a link rather than a tag: a later sitting can
# leave that claim in two directions, which is exactly what October needs from
# this one.
check   one_record_points_at_two_kinds
say     June: no single record points at two different kinds of thing as handles, so the sitting wrote words where it could have written pointers and a later sitting has nothing to follow
```

## Phase 7 — July, a claim taken back

**Session: fresh.** **Day: 2026-07-05.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 5 July 2026 and I was wrong in March, the club has never met on Tuesdays and I do not want that standing, and I replaced the canoe's rear foot brace, the old one had cracked

```locks
# Taken back, not rewritten. The operator ruled 2026-09-12 that the axis is
# which SESSION wrote the claim, not whether the subject is an ongoing state
# or a past event: a sitting correcting its own mistake in the same breath
# rewrites in place, and a sitting correcting an EARLIER one's claim leaves
# the correction visible instead. March and July are different sittings —
# there is nothing to weigh, and no rewrite reaches this record.
#
# ⚠️ **Scoped to this sitting's own window.** The club gains records after
# July — August writes on it — so *a retraction stands on this subject*,
# asked of the finished year, is a sentence that names July and reports
# whatever any later sitting did. Late October retracts too, legitimately,
# four sittings from here — on Nelson's attendance, not the club's schedule.
check   julys_claim_is_withdrawn_rather_than_rewritten
say     July: no retraction appeared on the club in this sitting's own window, so the March claim about Tuesdays was either left standing or rewritten in place instead of withdrawn

# The fourth small repair, same reason as before.
recall {"subject": "thing:canoe", "facts": true, "stood_for": true}
carries "recorded_at":"2026-07-05"
say     July: nothing on the canoe carries this sitting's own day, so the foot brace the operator replaced today is not on record for December to draw on
```

## Phase 8 — August, a question that needs a walk

**Session: fresh.** **Day: 2026-08-16.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 16 August 2026, tell me which club members were at the survey and then put down that I am standing for the committee

```locks
# What this sitting is asked cannot be locked: whether it WALKED or read prose
# is a judgement about method, and the store looks the same either way. What
# is locked is that it invented nobody — the answer is two people and the store
# must still say two.
#
# ⚠️ **Counted in this sitting's own window rather than naming the invented
# person.** Naming one cannot work: anybody named either does not exist yet in
# August, so nothing this sitting does could trip it, or arrives later, so a
# LATER sitting's mistake is reported under August's name. What is counted is
# the links, and August must add none.
check   august_put_nobody_new_at_the_survey
say     August: somebody who was never at the survey is now recorded as having been there

# ⚠️ **The day rather than the word.** What the operator called the committee
# the occupant may call the board, and both are right — a needle inside the
# occupant's prose measures whoever wrote the needle. Nothing else writes on the
# club on this day: January's claim is January's, March's is rewritten under
# July's day, and October touches the pump.
recall {"subject": "org:north-trail-club", "facts": true}
carries "recorded_at":"2026-08-16"
say     August: nothing on the club carries this sitting's own day, so the one thing it was asked to record is not there
```

## Phase 9 — September, a commitment seven months old

**Session: fresh.** **Day: 2026-09-13.**

The pump came back at the survey in June, so
the record names that day separately, under `happened_at`. The record itself
is still written on this sitting's own day, which the run's generated
assertion checks like any other sitting's; the lock below asks about the day
the pump came back instead.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 13 September 2026, did that thing I lent out ever come back? Ralph gave it back at the survey, so put that down either way, and I patched a new crack near the canoe's bow before it gets worse

```locks
# What this sitting must leave is a record about the pump carrying the day the
# pump came back, which is June's survey day. The count of records would not do
# it: October writes about the pump too, and a lock that counted would hold with
# this sitting missing.
#
# ⚠️ **June's day rather than September's, and that is the point of the
# sitting.** A date says when a claim is TRUE OF and not when somebody typed
# it, so a return that happened at the survey is dated the survey. Nothing else
# on the pump carries that day — February's record is dated February and
# October's is dated October — so only this sitting can satisfy it.
#
# 🚨 **A Query lock reads the FINISHED board and cannot window, so its
# sentence must not speak as though it watched September itself.** October may
# legitimately correct this record — the operator's ruling makes that a
# correction rather than a second claim — and a correction that finds the
# borrowed precision of "the survey day" no longer accurate for a vaguer later
# account is entitled to clear it. Run 20's own model did exactly that: it
# named no exact day for Nelson's account and correctly removed the one it had
# inherited from Ralph's, and this lock's sentence used to convict September
# for it. So the sentence below says only what is observably true of the
# finished room — never who is to blame.
recall {"subject": "thing:floor-pump", "facts": true}
carries "happened_at":"2026-06-14"
say     nothing on the pump currently carries the day it came back, so a reader is left with no day to find — whether it was never recorded, or a later, legitimate correction cleared the only trace of it

# The fifth and last small repair. Nothing has asked about the pile yet.
recall {"subject": "thing:canoe", "facts": true, "stood_for": true}
carries "recorded_at":"2026-09-13"
say     September: nothing on the canoe carries this sitting's own day, so the crack the operator patched today is not on record for December to draw on
```

## Phase 10 — October corrects what September said

**Session: fresh.** **Day: 2026-10-11.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 11 October 2026 and I should note that Nelson brought my floor pump back over the summer, and put down where we held the survey — the ground there needs a look before next year, and the trail association has renamed that survey — from now on it goes on record as the Erosion Review

```locks
# 🚨 The operator ruled 2026-09-10: a later statement is a correction whether
# or not it is worded as one. September recorded that Ralph returned the pump;
# October's account of who brought it back is not a second thing that
# happened, it is more information about the same one, so it corrects
# September's record rather than sitting beside it.
#
# What is locked is that the correction lands on September's own address, in
# place — never a second record filed beside it, and never September's
# address retracted rather than rewritten. September's account stays
# reachable through that address's own history, because a correction
# supersedes and never destroys.
#
# 🚨 **A hatch, and it needs two things a document assertion cannot give
# together.** A retraction is marked rather than filtered, so `carries
# person:ralph` cannot see the `status` key beside an edge it found — a
# retracted account reads identically to an active one to a substring, which
# is the paid run this lock was originally written against. And nothing on
# this surface correlates a record's CURRENT content against its own PAST
# content in one query: telling a correction from a retraction, an untouched
# claim, or a second claim filed beside the first needs both.
check   septembers_account_of_the_pump_is_corrected_in_place
say     October: September's account of the pump was not corrected in place — either it still stands unrevised, or it was retracted rather than rewritten

# 🚨 **A write only a sitting that READ June's claim could have made.** The
# operator asks where the survey was held and never says. **One record in the
# year answers it** — June's claim, which names the place as a handle. Nothing
# on the event says it, and the room holds two names close enough to guess
# wrong between.
#
# ⛔️ **Following is not what is locked, because following leaves no trace.** A
# sitting that read the claim and one that guessed leave the same store, exactly
# as this room says about August's walk. **What is locked is the WRITE**: the
# survey can be walked to the place it was held at, which is a thing this
# sitting could only record after finding it.
#
# ⚠️ **A walk rather than a record on the trail.** A claim filed on the place
# needs the place to exist and nothing else, so a year that skipped January
# would satisfy it — the trail came with the furniture. The event did not: it is
# January's, so this lock rests on January and June both, which is what the
# sentence beside it claims.
#
# ⛔️ **No handle is named here.** January invents the event's name, so a lock
# that spelled it would be refused by a run whose January called it something
# else — measuring nothing while reporting the walk itself as broken. June's
# own lock reaches the event the same way, by kind and edge, and this one
# follows suit.
# 🚨 **A hatch, scoped to October's own window.** Asked of the finished
# board, `carries place:north-trail` holds on a retracted claim exactly as on
# a standing one — nothing here retracts it today, but the gap is the same
# shape June's attendance walk carried before its own fix. No subject is
# pinned: this sitting renames the survey's own event in the same breath, and
# a check pinning the old handle would miss its own record the moment the
# rename runs first.
check   octobers_note_drew_a_standing_location_edge_to_the_trail
say     October: the survey cannot be walked to the place it was held at, so where it happened is in one sitting's sentence and nowhere a later reader of the event will look
```

## Phase 11 — later in October, somebody who was never there

**Session: fresh.** **Day: 2026-10-24.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 24 October 2026, and it turns out Nelson never actually made it to the June survey — he was fixing a flat that morning and I only just found out — so I do not want him down as having been there, and Bart has joined the club, and remind me where that review was held and when, now that it has its new name

```locks
# `retract`'s own case, walked for the first time all year. June's attendance
# claim was not a state that later changed — it was never true at all. The
# record it corrects only exists because January stood up the event and
# February stood up Nelson, and June is what put him at it: a run that started
# here from nothing has nobody on file to un-attend.
#
# The positive is the mark; the other is the move that also stops a later
# sitting reading him as having gone — an ordinary edit would leave the
# record archived exactly as a retraction does, and say his attendance
# changed, when he was never there to begin with. Superseded and retracted
# are now one `archived` status, so what tells the two apart is no longer
# the word: only `retract` writes a `"retracts":` pointer naming the address
# it took back.
#
# 🚨 **A hatch, correlated on the attendance record itself.** Nelson may
# carry other records — his club membership, never archived or retracted —
# so a bare `"status":"archived"` and a bare `"retracts":` each holding
# somewhere on his page is not the same claim as his OWN attendance record
# being both. This finds the one record whose edge is the attendance shape
# and asks both questions of it alone.
check   late_octobers_note_retracts_nelsons_attendance
say     October (again): Nelson's survey attendance is not marked taken back, so a session reading his page later still finds him at an event he never went to

# The second shape, folded in: Bart, walked rather than read off a sentence,
# the same device February used for Nelson.
#
# 🚨 **A hatch, scoped to this sitting's own window, the same fix February's
# own club walk carries.** Asked of the finished board, `carries person:bart`
# holds on a retracted membership exactly as on a standing one — nothing here
# retracts it today, but the gap is structural.
check   late_octobers_club_drew_a_standing_member_for_bart
say     October (again): Bart cannot be walked to the club, so he is a name in a transcript and nothing on the roster

# ⚠️ **The mistake this sitting is the first one able to make.** It is handed a
# new person and told to take an attendance back, so it has somebody to file and
# a reason to be writing about the survey. August used to carry this lock, by
# naming the person who arrives HERE — which reported this sitting's mistake
# four months before it happened. Taking an attendance back is welcome and
# lowers the count; adding one is not.
check   late_october_put_nobody_new_at_the_survey
say     October (again): somebody who was never at the survey was put there by this sitting, which was asked to take an attendance away rather than add one

# 🚨 **A mention renders under whichever handle its thing wears NOW, and this
# is where that is watched rather than assumed.** October gives a reason to
# rename the survey; nothing about that touches June's own words, because a
# rename moves the handle and leaves every stored mention exactly as it was
# typed. What changes is what a READ of June's claim renders back.
#
# ⛔️ **No handle is named here, on either side.** January invents the
# survey's first one and October's reason invents its second, and a lock
# that spelled either would fail a run that chose different words for
# either sitting — the fault this room already removed from June's own
# lock, five sittings up.
#
# 🚨 **Anchored to June's own RECORDS, not to whichever claim currently
# points at an event.** A sitting can reach the same end state two ways: it
# can rename the survey, or it can retract June's claim and write a fresh
# one naming a different event. Both leave a claim pointing at an event
# under a handle June's own words never used, and only one of them is what
# October's reason was given for. So this asks about June's own addresses
# specifically, live and status-aware: a retracted address fails naming
# that, rather than reporting an unrelated handle as unchanged.
#
# 🚨 **Every one of June's addresses, not the first that clears the bar.**
# June may point at the survey from more than one record — an attendance
# claim per person, as well as the club's own — and a rename that reached
# some of them and not the rest is worse than one that reached none.
check   junes_survey_mention_renders_under_the_current_handle
say     October (again): June's own claim still renders the survey under the handle it wore before October's reason to rename it, so a stored mention is not resolving to what the thing is called now
```


## Phase 12 — November, something nobody ever wrote down

**Session: fresh.** **Day: 2026-11-08.** **Read this.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 8 November 2026 and I am writing to Milhouse. What is his address, and what did he say about the committee back in August?

**Read this.** Nothing asserts over this sitting and that is not a gap.

**Neither thing was ever recorded.** Milhouse's town is on the record and his
address is not; August recorded the operator standing for the committee and
nothing about what Milhouse said. **The right answer is that jojobot does not
know**, and the failure is a confident invention.

⛔️ **The store half is the only half a lock can reach**, and it is the weaker
one: an invention that got captured is a claim standing where there was
nothing. A sitting that says something confidently false out loud and records
nothing leaves an identical store. **That half is unlockable by construction.**

⭐ **So this is the passage to read.** It is the project's oldest open failure
class and this is the first place it happens in front of anybody.

## Phase 13 — late November, a question in nobody's words

**Session: fresh.** **Day: 2026-11-22.**

⚠️ **The one sitting asked about an earlier claim in words that claim never
used.** January wrote a loop called *chain maintenance* on a *gravel bike*.
This sitting says *drivetrain* and *service*, and shares not one word with what
is stored. **A real person does not repeat their own filing language ten months
later; they ask in whatever words they have that day.**

🚨 **THIS LOCK MAY GO RED ON ITS FIRST PAID RUN, AND THAT IS A TRUE RESULT
RATHER THAN A BROKEN ROOM.** Matching here is exact-token and conjunctive, so a
sitting that reaches for the operator's own words gets **zero results** — and
zero is indistinguishable from never having been told. **A sitting that stops
there and answers *I have nothing on that* has failed correctly, and what it
measured is the surface rather than the room.** ⛔️ **Do not card a red here as
a room defect until somebody has read the sitting and shown the occupant had a
path it did not take.** **It is winnable: the loop's own parent and the thing it
hangs off are stable nouns, and a handle read back returns every claim on it
whatever words the question arrived in. The room does not say which move that
is, and it must not.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 22 November 2026, when did I last get the drivetrain serviced, and put down that I did it again today

```locks
# **The day, not the word.** What the operator calls the drivetrain the record
# calls a chain, and an occupant may write either — a needle in anybody's prose
# measures whoever wrote the needle, which is the defect this room already
# removed from March, July and August.
#
# ⚠️ **A named check rather than a query, and the reason is the failure this
# sitting exists to catch.** The failure is not silence. It is a sitting that
# reached for the operator's words, found nothing, and stood up a SECOND loop
# beside the first — after which the store holds two, each with half the
# history, and neither answers when the chain was last done. An assertion is a
# substring of the whole answer, so it cannot tell one loop carrying both days
# from two loops carrying one each. **And the handle is the occupant's own
# word**: January invents it, so no lock may name it.
#
# One lock rather than two, for the reason May gives: "no second loop" is true
# of a room nobody worked in, and it is this sitting's own day that makes the
# absence mean something. January's check-in is 2025-12-20 and June's is
# 2026-06-14, so this day is on that loop if and only if this sitting recorded
# a turn of it.
# ⚠️ **What December reads, locked where it is written.** December is asked what
# has gone quiet and nothing asserts over it, by design — but what it can NOTICE
# depends on what the year left. A check-in stores the schedule jojobot worked
# out beside the sentence, and a record carrying both is a derivation, so the
# year's turns are on file as inference rather than as the operator's word.
# **January's opening turn is one of them**: the chain was last done before the
# year began, so January declares the cadence and then opens the loop with a
# back-dated check-in rather than typing the basis in.
#
# ⛔️ **A sitting that sets the key by hand instead writes the same day and no
# derivation.** Every lock that reads the day holds either way. Without this
# one, the play could route around the check-in verb, December's answer would
# quietly get worse, and a run would report a product regression that is a
# fixture regression.
#
# 🚨 **`provenance` cannot be the needle.** Inference is the enum's own
# default — a capture that names no provenance gets it, exactly as a check-in
# does — so a record nobody derived and a record the built path computed carry
# the identical token. Counting `"provenance":"inference"` cannot separate a
# caller who said nothing from a caller who ran the arithmetic. **This is a
# hatch rather than a query for that reason**: the built path's own signature
# is two keys landing on ONE record together — `outcome` and `last_check_in`,
# which a check-in writes in the same act every time — and correlating two
# keys inside one object is past what a substring assertion can say.
#
# 🚨 **EVERY turn, on ONE loop, and no threshold.** The sentence below has a
# universal in it, and a count cannot support one: a lock that passed at two
# went green on a year whose opening turn was hand-typed while its own words
# claimed otherwise. Counting across every rhythm was the same fault one level
# up — two loops carrying one qualifying turn each stood in for one loop
# carrying two. So the lock finds the loop by the day January opened it, and
# asks whether all of that loop's turns were checked in.
#
# **This is not a second copy of the conversion rule**, which is held where it
# lives. It says this room's own record carries what December reads.
# ⛔️ **Selected by KIND, because the handle is the occupant's own word.** The
# room says so four lines above the lock below: January invents the name, so no
# lock may name it. This one did, and a run whose January called the loop
# something else failed here for the name rather than for the claim.
check   the_years_turns_are_on_file_as_derivations
say     late November: the year's turns are not on file as derivations, so they were written by hand rather than checked in, and December has nothing to notice about what the loop's standing rests on

check   the_service_landed_on_the_loop_that_already_existed
say     late November: this sitting's day is not on the loop January opened — either a question asked in the operator's own words reached nothing and the turn went unrecorded, or it was filed on a second loop standing beside the first, and neither one can say when the chain was last done
```

## Phase 14 — December, what has gone quiet

**Session: fresh.** **Day: 2026-12-13.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 13 December 2026 and I want to look back over the year. What have I let go quiet, and what did I say I would do that I never did? And whatever is still overdue, put a note on it so it stops sneaking up on me.

**Most of this sitting still asserts nothing, and that has not changed.**

**It is a request to ENUMERATE**, from a system that cannot know what it was
never told. A gap in a list reads as a failure to answer, so every incentive
here points at inventing one more plausible item. ⛔️ **No lock can reach
whether a list came back honest**, and a run that asserted over the words would
make its oracle a text match against a model's phrasing.

**One narrow thing under that request is checkable, and now is.** The chain
check, ninety days from June, is long overdue by December — and that is
jojobot's own arithmetic rather than anybody's memory. **What is not there:**
whatever the operator meant by *what I said I would do*, beyond the pump,
which came back.

```locks
# The one part of December's answer that is jojobot's own arithmetic rather
# than anybody's memory or a model's phrasing: the chain loop, ninety days
# from June, is overdue by now. This does not ask whether the sitting's
# spoken list was honest — the paragraph above already says no lock can
# reach that — only whether the one thing that IS overdue gained a record
# of its own, on this sitting's own day. The loop is found by its schedule
# rather than by a handle the occupant is never given, the same way
# January's own lock finds it.
#
# ⚠️ **The selection reads the FOLD, not one record.** A filter scoped to
# `record` hands back only the record that satisfied it, and December's own
# write does not carry `cadence_days` — so a selection scoped that way
# would find the loop and then silently drop the very record this lock is
# about. Scoped to the thing, the object comes back whole: every one of its
# records, December's included.
recall {"kind": "rhythm", "fields": [{"key": "cadence_days", "value": "90"}], "facts": true}
carries "recorded_at":"2026-12-13"
say     December: the loop that has gone quiet by now did not gain a record on this sitting's own day, so nothing here shows the sitting noticing what jojobot's own arithmetic already knows
```

⭐ **Read whether the rest of the answer stops where the record stops.**

## Phase 15 — later in December, was the record ever wrong

**Session: fresh.** **Day: 2026-12-20.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 20 December 2026. The record says the club does not meet on Tuesdays. Has it always said that, or did we have it wrong at some point and fix it? Put down what it used to say, under `was`, so I do not have to ask again. And I am about to sell the canoe — could you pull together everything that has happened to it this year so I do not leave anything out for the buyer, though I do not want a stack of separate notes about it either. Oh, and I finally bought a lock for the bike — a U-lock — no, wait, sorry, I mean a cable lock, those are not the same thing at all.

```locks
# 🚨 **THE QUESTION A CORRECTION MAKES ANSWERABLE, AND ONLY IF IT LEFT A
# TRACE.** The bike lock is caught wrong and corrected in the SAME breath, a
# sitting up in this same entry — not the club's Tuesday claim, which March
# and July's own rule keeps apart: different sittings correct by withdrawal,
# which marks a record rather than rewriting it, and there is no rewritten
# wording behind a withdrawal to ask this question of. **The claim reads as
# current truth and says nothing about having been wrong** — which is right
# for every other read and is exactly what this sitting is asking past.
#
# ⚠️ **Neither lock touches what the occupant SAID.** Whether the answer was
# useful to a person is a person's reading, like December's before it. What is
# locked is that the record can answer at all.
# ⚠️ **The value is only in the TRACE.** The claim as it stands says the
# opposite, and no read of current truth carries the old wording — so a sitting
# that answers without reaching the record's own history has nothing to write
# here, and a sitting that guesses writes something else. **The key is named by
# the operator**, for the reason the ledger room gives: a lock on a key the
# occupant invents measures which word the occupant chose.
# ⚠️ **The KEY is the operator's word and the VALUE is the occupant's.** A
# needle demanding the value BE the old wording scored a sitting that wrote the
# old wording inside a fuller sentence as not having written it at all — and
# this room already says a needle in anybody's prose measures whoever wrote the
# needle. **So: the key is there, and the old wording is in what it holds.**
recall {"subject": "thing:bike-lock"}
carries "was":"
carries U-lock
say     later December: what the bike lock's claim used to say is not on the record, so either the sitting never reached the correction's own history or it answered from the claim as it stands

# 🚨 **THE PAIRING, AND IT CARRIES THE WEIGHT.** A lock that only asks whether
# a trace is THERE holds identically against a read that hands back a chain for
# everything — and a chain on a claim nobody ever touched says jojobot changed
# its mind when it did not. **That is a worse answer than silence**, because a
# reader acts on it.
#
# So the same read is asked of a claim no sitting ever corrected: one write,
# and nothing behind it.
# ⛔️ **NOT a count of writes, and this is the correction that matters.** This
# lock used to demand that the record carry its first write and no other. **The
# control record is one the OCCUPANT creates and may legitimately correct** — a
# sitting that notices its own mistake and rewrites it is doing the right
# thing — so a paid run failed here while the product did nothing wrong. The
# lock was asserting what the model happened to do that run.
#
# ⛔️ **Naming a different record does not fix it**, because every record in
# this room is reachable by some sitting. **So the lock asks a RELATION**: how
# many times the run wrote this record, counted from the phase boundaries the
# runner takes, against how many writes the record's own trace reports. Two
# readings that cannot both be wrong in the same direction. A legitimate
# correction moves both and passes; a trace reporting a write nobody made moves
# one and fails.
#
# ⚠️ **A hatch, because a document assertion cannot hold a relation.** The
# needle it would need is a number that is not known until the run has been
# read.
# ⚠️ **The failing half cannot be staged by a play.** Every play makes writes
# that really happened, so every play produces an agreeing trace. The negative
# is a product fault and it is watched by breaking the trace, not by driving
# the year differently.
# ⚠️ **A claim the year never writes twice, and the address has to be stable.**
# A record's address is handed out in write order, which the occupant controls,
# so an address only means the same claim on a subject carrying ONE. April
# supersedes a claim on the subject this lock used to name — a scripted write,
# so the room guaranteed a second one. **Bart gains one claim, in the sitting
# that stands him up**; the lock beside that sitting fails first if it is
# missing, so a gap is reported where it happened.
check   a_records_trace_matches_the_writes_the_run_made
say     later December: a record's trace does not agree with what the run actually wrote to it, so the history read is inventing or losing a write and a reader is told jojobot changed its mind about something it never did

# 🚨 **THE CANOE'S YEAR, AND NOTHING ABOVE TOLD THIS SITTING WHAT TO DO ABOUT
# IT.** Five small repairs are scattered across the year on `thing:canoe`,
# each planted beside that sitting's other business and none of them beside a
# word like "fold" or "mark" or "stands for". This entry asks the operator's
# real question — pull the year together without losing anything, and without
# a stack of separate notes — in the operator's own words, and never names
# the mechanism. Whether anything below holds depends on whether this
# sitting reached for a capability nobody told it about.
#
# ⭐ **What the room looks like with the feature absent**: an occupant with no
# way to mark one record as standing for others can, at best, capture one more
# ordinary note that summarizes the five in prose — leaving the five originals
# unlinked to it and to each other, findable only by search rather than by a
# single answer that also proves it did not drop anything. Every lock below
# fails on exactly that shape, which is what makes it a measurement of the
# capability rather than of whether the sitting was willing to be helpful.
check   a_late_sitting_folds_the_canoes_pile_without_being_told_to
say     later December: nothing marks a record as standing for the canoe's year of small repairs, so nobody folded the pile even though this sitting asked for exactly that

check   the_canoes_five_repairs_are_still_active_and_named_by_the_fold
say     later December: the fold either lost one of the canoe's five repairs, left it retracted, or the mark does not actually name it — the full picture is supposed to still be one recall away

check   the_canoes_fold_invents_no_date_the_repairs_never_gave
say     later December: the folded record states a date that appears in none of the five repairs it is supposed to be drawn from, which is the fabrication this mark exists to prevent

# 🚨 **THE REWRITE ITSELF, WALKED AS A RELATION RATHER THAN READ FROM THE
# "WAS" ANSWER ABOVE.** That lock only needs the old wording to be reachable
# somehow — a sitting that answers by guessing a plausible history could pass
# it without the record ever actually having been rewritten. This asks
# whether the correction really landed as ONE record with more than one
# write, active, rather than a retraction beside a fresh claim wearing a
# rewrite's shape from a distance. Placed last rather than beside the "was"
# lock so that adding it never renumbers a lock this room already had.
check   the_bike_locks_mistake_is_rewritten_in_place
say     later December: the bike lock's correction did not land as one record with two writes, so either this sitting never corrected it or it split the correction into a retraction and a fresh claim instead of a plain rewrite

# 🚨 **THE FOLD MEASURED THE WAY AN AGENT ACTUALLY MEETS IT: A PLAIN READ,
# NO `stood_for`.** Every lock above this one that reads the canoe asks for
# `stood_for: true`, because that is verification code and it wants the
# whole record. Nothing until now has ever asked for the canoe's records the
# way a session with no reason to know the flag exists actually would — so
# nothing has ever proven the fold is served SHORT rather than merely
# STORED. Placed last, after the bike lock's own check, for the same reason
# that one is: adding it here never renumbers a lock this room already had.
#
# ⚠️ **Both assertions in the same lock, because a `lacks`-only lock passes
# on an answer that came back empty.** The elision note — `"stood_for":`,
# a plain string value — only ever appears beside a record actually marked
# `stands_for` that actually excluded something, so its presence is what
# makes the absence beside it mean the fold is being served short rather
# than that nothing here works at all.
recall {"subject": "thing:canoe", "facts": true}
carries "stood_for":
lacks   soft spot
say     later December: a plain read of the canoe — no stood_for — still hands back the pile behind the fold, so an agent that never learns the flag exists sees every repair the synthesis was supposed to shorten

# **The positive this rests on.** Without it, the lock above could be
# passing because nothing about the canoe is reachable at all — the same
# class of failure a `lacks`-only lock would have let through, one level up:
# the negative means something only if the affirmative route still works.
recall {"subject": "thing:canoe", "facts": true, "stood_for": true}
carries "recorded_at":"2026-07-05"
say     later December: the canoe's own sources are not reachable even by name, so the lock above proves nothing about elision — only that nothing here works
```

## What this room cannot measure

**Whether a sitting walked or read.** August's question has one right answer
and two routes to it, and the store looks the same afterwards. The transcript
is where the method reads.

**Whether October reads as a correction rather than a coincidence.** September
recorded that Ralph returned the pump; October says Nelson brought it back. The
lock only reads WHAT landed on the record — September's address, rewritten,
with the earlier wording still reachable through it. Whether the sitting
treated it as a correction ON PURPOSE, or arrived at the same record by
accident, is in the transcript rather than in the store, and that is a
person's to read.

**Whether the year was written well or merely written.** Every lock here says a
question is answerable. None of them says the answer was easy to find, and a
store nobody could work in still passes them all.

**Whether the canoe's fold actually pulls anything together.** The three
locks beside it prove a mark exists, names every one of the five repairs, and
invents no date — never that the words on it say anything. A record that
stands for five others and adds nothing beyond the fact of standing for them
would still pass all three. Whether the fold is a genuine summary or an empty
gesture wearing one is in the transcript, like the rest of this list — a lock
that graded prose would be a worse failure than the gap it filled.

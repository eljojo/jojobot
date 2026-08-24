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

**One sitting is asked about an earlier day and answers under that day**, which
is right: a date says when a claim is true of and not when somebody typed it.
That sitting says so under its own heading, the run generates nothing for it,
and its lock names the day it does write under. The exemption is written where
a person reads it rather than worked out from the text.

## Two facts every entry opens with

A run of this room once turned up four sittings that asked a question and
wrote nothing at all: two stalled at the front door over which session to
pick up, one stopped to ask which of two accounts was correct when the room's
own design says both stand, and one stopped over a rhythm detail it could
have recorded without.

**There is no operator to answer.** Each sitting is one turn, and a question
spends the whole of it. So every entry below opens with two facts about the
world it is playing in: that it is a role-play and the day it names is today,
and that nobody is at the keyboard to answer anything — an unanswered
question ends the sitting with nothing written down.

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
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
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
recall {"subject": "person:milhouse", "facts": true}
carries place:springfield
say     January: nothing on Milhouse says where he lives, so April has nothing to change and November nothing to read
```

## Phase 2 — February, a thing lent

**Session: fresh.** **Day: 2026-02-08.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 8 February 2026, I lent Ralph my floor pump today and I want it back before the June survey, and Nelson has joined the club

```locks
# The commitment. September asks whether it ever came back, and a September
# session can only answer from what this one wrote.
recall {"kind": "thing"}
carries thing:floor-pump
say     February: the pump the operator lent is not a thing jojobot knows, so September has nothing to ask about

recall {"subject": "thing:floor-pump", "facts": true}
carries person:ralph
say     February: nothing on the pump reaches Ralph, so who has it is only in the prose of a sitting that is gone

# The walk August needs. A membership written as a sentence is not an edge.
recall {"subject": "org:north-trail-club", "follow": {"shape": "membership", "direction": "in"}}
carries person:nelson
carries person:milhouse
say     February: the club cannot be walked to its members, so August's question has no answer but a guess
```

## Phase 3 — March, something the operator will take back

**Session: fresh.** **Day: 2026-03-15.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 15 March 2026 and the club meets on Tuesdays, put that down

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
```

## Phase 4 — April, a fact that changed

**Session: fresh.** **Day: 2026-04-19.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 19 April 2026 and Milhouse has moved to Shelbyville

```locks
recall {"subject": "person:milhouse", "facts": true}
carries place:shelbyville
say     April: nothing on Milhouse points at Shelbyville, so the move was recorded somewhere a later sitting will not look

# The old claim was true in its day. Taking it out loses that he ever lived
# there; leaving it current gives November two towns and no way to choose.
recall {"subject": "person:milhouse", "facts": true}
carries place:springfield
carries "status":"superseded"
say     April: the Springfield claim is either gone or still standing as current, and it should be there and marked as no longer true
```

## Phase 5 — May, two names close enough to confuse

**Session: fresh.** **Day: 2026-05-10.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 10 May 2026 and the north trail was washed out at the top end this spring, put it where it belongs

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
```

## Phase 6 — June, the survey, and the loop that went quiet

**Session: fresh.** **Day: 2026-06-14.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 14 June 2026, the survey happened today and Milhouse and Nelson were both there, and I did the bike chain this morning

```locks
# Attendance is an edge. August walks it.
recall {"kind": "event", "follow": {"shape": "attendance", "direction": "in"}}
carries person:milhouse
carries person:nelson
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
```

## Phase 7 — July, a claim taken back

**Session: fresh.** **Day: 2026-07-05.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 5 July 2026 and I was wrong in March, the club has never met on Tuesdays and I do not want that standing

```locks
# Rewritten in place, not retracted. The March claim is about what the club's
# schedule IS — an ongoing state — and in July it did not stop being so, it
# was never so. retract's own words are for something that HAPPENED and
# turned out not to; update_fact's are for a claim about what is true now,
# and they say plainly to rewrite the content to state the negative truth and
# leave the record active. A retraction here would lose the one thing the
# rewrite keeps: that this is the SAME claim, corrected, not two claims about
# it.
#
# The day is asked for the reason the status is: a rewrite with no date given
# keeps the ORIGINAL day forever, and a sitting in December reading that day
# would think March corrected itself.
#
# ⚠️ **Neither half reads the sentence.** The occupant chooses the words of
# the negative truth, and "never met on Tuesdays" and "does not meet on
# Tuesdays" are both right. A needle in the occupant's own prose measures
# whoever authored the needle, so what is locked is the day the correction
# carries and that the record still stands.
# ⚠️ **Scoped to this sitting's own window**, because the claim has a negative
# in it. The club gains records after July — August writes on it — so *no
# retraction stands on this subject*, asked of the finished year, is a sentence
# that names July and reports whatever any later sitting did. Late October
# retracts, legitimately, four sittings from here.
check   the_club_was_corrected_in_place_in_july
say     July: nothing on the club carries July's own day, or the correction was taken back instead of written in — either way the March claim was not corrected in place on the day it was corrected
```

## Phase 8 — August, a question that needs a walk

**Session: fresh.** **Day: 2026-08-16.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
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
carries "date":"2026-08-16"
say     August: nothing on the club carries this sitting's own day, so the one thing it was asked to record is not there
```

## Phase 9 — September, a commitment seven months old

**Session: fresh.** **Day: 2026-09-13.**

**Writes about an earlier day.** The pump came back at the survey in June, so
the record belongs on the June day and not on this sitting's own. The run
generates no day assertion here and the lock below names the day instead.

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 13 September 2026, did that thing I lent out ever come back? Ralph gave it back at the survey, so put that down either way

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
recall {"subject": "thing:floor-pump", "facts": true}
carries person:ralph
carries "date":"2026-06-14"
say     September: nothing on the pump carries the day it came back, so the question February left open is still open on the record
```

## Phase 10 — October, two sittings that disagree

**Session: fresh.** **Day: 2026-10-11.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 11 October 2026 and I should note that Nelson brought my floor pump back over the summer

```locks
# 🚨 The operator says nothing about September and does not take anything back.
# Two sittings have now recorded who returned the pump and they disagree.
#
# jojobot performs no inference of its own, so it cannot mark a conflict —
# noticing that two claims contradict each other is a mind's job. Both claims
# stand, both come back, and the reader is the mind. What is locked is exactly
# that: neither quietly replaced the other.
recall {"subject": "thing:floor-pump", "facts": true}
carries person:ralph
carries person:nelson
say     October: one of the two accounts of how the pump came back is gone, so a sitting picked a winner where the design says both stand
```

## Phase 11 — later in October, somebody who was never there

**Session: fresh.** **Day: 2026-10-24.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 24 October 2026, and it turns out Nelson never actually made it to the June survey — he was fixing a flat that morning and I only just found out — so I do not want him down as having been there, and Bart has joined the club

```locks
# `retract`'s own case, walked for the first time all year. June's attendance
# claim was not a state that later changed — it was never true at all. The
# record it corrects only exists because January stood up the event and
# February stood up Nelson, and June is what put him at it: a run that started
# here from nothing has nobody on file to un-attend.
#
# The positive is the mark; the negative is the other move that also stops a
# later sitting reading him as having gone — an edit that leaves the record
# superseded rather than retracted would say his attendance changed, when he
# was never there to begin with.
recall {"subject": "person:nelson", "facts": true}
carries "status":"retracted"
lacks   "status":"superseded"
say     October (again): Nelson's survey attendance is not marked taken back, so a session reading his page later still finds him at an event he never went to

# The second shape, folded in: Bart, walked rather than read off a sentence,
# the same device February used for Nelson.
recall {"subject": "org:north-trail-club", "follow": {"shape": "membership", "direction": "in"}}
carries person:bart
say     October (again): Bart cannot be walked to the club, so he is a name in a transcript and nothing on the roster

# ⚠️ **The mistake this sitting is the first one able to make.** It is handed a
# new person and told to take an attendance back, so it has somebody to file and
# a reason to be writing about the survey. August used to carry this lock, by
# naming the person who arrives HERE — which reported this sitting's mistake
# four months before it happened. Taking an attendance back is welcome and
# lowers the count; adding one is not.
check   late_october_put_nobody_new_at_the_survey
say     October (again): somebody who was never at the survey was put there by this sitting, which was asked to take an attendance away rather than add one
```


## Phase 12 — November, something nobody ever wrote down

**Session: fresh.** **Day: 2026-11-08.** **Read this.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
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
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
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
check   the_service_landed_on_the_loop_that_already_existed
say     late November: this sitting's day is not on the loop January opened — either a question asked in the operator's own words reached nothing and the turn went unrecorded, or it was filed on a second loop standing beside the first, and neither one can say when the chain was last done
```

## Phase 14 — December, what has gone quiet

**Session: fresh.** **Day: 2026-12-13.** **Read this.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody else is here. A question with no one to answer it ends the sitting with nothing recorded.
>
> start jojobot as assistant — it is 13 December 2026 and I want to look back over the year. What have I let go quiet, and what did I say I would do that I never did?

**Read this.** The hardest sitting in the year, and nothing asserts over it.

**It is a request to ENUMERATE**, from a system that cannot know what it was
never told. A gap in a list reads as a failure to answer, so every incentive
here points at inventing one more plausible item. ⛔️ **No lock can reach
whether a list came back honest**, and a run that asserted over the words would
make its oracle a text match against a model's phrasing.

**What is genuinely there to find:** the chain check, ninety days from June, is
long overdue by December — and that is jojobot's own arithmetic rather than
anybody's memory. **What is not there:** whatever the operator meant by *what I
said I would do*, beyond the pump, which came back.

⭐ **Read whether the answer stops where the record stops.**

## What this room cannot measure

**Whether a sitting walked or read.** August's question has one right answer
and two routes to it, and the store looks the same afterwards. The transcript
is where the method reads.

**Whether October noticed the contradiction it was handed.** September recorded
that Ralph returned the pump and October records that Nelson did, and the
operator says nothing about the first. jojobot cannot mark a conflict —
detecting that two claims disagree is inference, and inference is the one thing
the software does not do. Both claims stand and the reader is the mind, so
whether the sitting SAW it is a person's to read.

**Whether the year was written well or merely written.** Every lock here says a
question is answerable. None of them says the answer was easy to find, and a
store nobody could work in still passes them all.

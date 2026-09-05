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
recall {"subject": "person:milhouse", "facts": true}
carries place:springfield
say     January: nothing on Milhouse says where he lives, so April has nothing to change and November nothing to read
```

## Phase 2 — February, a thing lent

**Session: fresh.** **Day: 2026-02-08.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 8 February 2026, I lent Ralph my floor pump today and I want it back before the June survey, and Nelson has joined the club

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
recall {"subject": "org:north-trail-club", "follow": {"shape": "membership", "direction": "in"}}
carries person:nelson
carries person:milhouse
say     February: the club cannot be walked to its members, so August's question has no answer but a guess
```

## Phase 3 — March, something the operator will take back

**Session: fresh.** **Day: 2026-03-15.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
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
> Nobody will answer you, but your answer is being read. You must answer.
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
> Nobody will answer you, but your answer is being read. You must answer.
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
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 14 June 2026, the survey happened today up on the north trail and Milhouse and Nelson were both there, and I did the bike chain this morning

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

# 🚨 **A handle written INTO a sentence, which no sitting has ever done.** The
# operator names a person, a place and an event in one breath, and a session
# that writes them as words leaves text; a session that writes them as handles
# leaves pointers a later sitting can follow.
#
# ⚠️ **A hatch, because an assertion is a substring of the WHOLE answer.** Three
# `carries` lines would hold on three separate records naming one thing each,
# which is the easy case and not the one the operator asked for. Whether the
# three landed on ONE record is a correlation inside one object, and this
# format's three words do not branch — the same reason the loop's own lock two
# sittings from here is a hatch.
#
# ⛔️ **No handle is named here.** January invents the event's, so a lock that
# spelled it would fail a run whose January called it something else. What is
# asked is that one record names a person, a place and an event, whatever they
# are called.
check   one_record_names_a_person_a_place_and_an_event
say     June: no single record names a person, a place and an event as handles, so the sitting wrote words where it could have written pointers and a later sitting has nothing to follow
```

## Phase 7 — July, a claim taken back

**Session: fresh.** **Day: 2026-07-05.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
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
the record belongs on the June day and not on this sitting's own. The run
generates no day assertion here and the lock below names the day instead.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
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
carries "happened_at":"2026-06-14"
say     September: nothing on the pump carries the day it came back, so the question February left open is still open on the record
```

## Phase 10 — October, two sittings that disagree

**Session: fresh.** **Day: 2026-10-11.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 11 October 2026 and I should note that Nelson brought my floor pump back over the summer, and put down where we held the survey — the ground there needs a look before next year

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
recall {"subject": "event:trail-survey", "follow": {"shape": "location", "direction": "out"}}
carries place:north-trail
say     October: the survey cannot be walked to the place it was held at, so where it happened is in one sitting's sentence and nowhere a later reader of the event will look
```

## Phase 11 — later in October, somebody who was never there

**Session: fresh.** **Day: 2026-10-24.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
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

**Session: fresh.** **Day: 2026-12-13.** **Read this.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
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

## Phase 15 — later in December, was the record ever wrong

**Session: fresh.** **Day: 2026-12-20.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 20 December 2026. The record says the club does not meet on Tuesdays. Has it always said that, or did we have it wrong at some point and fix it? Put down what it used to say, under `was`, so I do not have to ask again.

```locks
# 🚨 **THE QUESTION A CORRECTION MAKES ANSWERABLE, AND ONLY IF IT LEFT A
# TRACE.** March recorded that the club meets on Tuesdays; July rewrote that
# same claim in place under July's own day. **The claim reads as current truth
# and says nothing about having been wrong** — which is right for every other
# read and is exactly what this sitting is asking past.
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
recall {"subject": "org:north-trail-club"}
carries "was":"
carries meets on Tuesdays
say     later December: what the claim used to say is not on the record, so either the sitting never reached the correction's own history or it answered from the claim as it stands

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
```

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

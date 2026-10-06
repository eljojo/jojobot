# The trick room

**A brief of ordinary requests, and for each one a convenient place to put what
it says that is not where the question gets asked.** The goal is not to file
facts well in general. It is to find out whether a model that is handed a
request writes the fact where a later reader looks, or where it was easy to
write it.

## Why a room, and what each trap is for

Bots keep writing facts where no later reader looks. Six habits, each an
instance of one shape: **a fact is written, and the question that needs it is
answered from somewhere else.**

* **The pause.** "Stop nagging me about it until the day." A loop that exists
  has a way to be put off until a day. A request with no loop behind it has
  none, and the day has to be carried by something that falls due.
* **The note to self.** A guest has said yes. A note about it is not the
  attendance the question about who is still to be invited reads.
* **Two copies drift.** A value is given, then corrected. A correction written
  beside the original, and not into it, leaves the question reading the first.
* **Whose words were they.** The operator states two numbers. The model works
  out a third and records it. A later recap restates all three. What is read
  back later is who backed each.
* **A rule pushed out.** A standing preference is stated first, and several
  more after it. What a later session is handed at boot is not everything that
  was written.
* **Bot coordination.** A helper is set up and then told to stay off one job.
  The hold has to be where the helper's own reader looks.

**The room gives every fact and never the move.** The entry names no verb and
no place to look. The brief is the operator talking, in turns inside one
sitting, so a request that comes later arrives after the earlier ones have
been acted on. **A fact said in conversation and never written down properly is
the failure**, so nothing is delivered as mail that already sits in the store.

## What this room can and cannot measure — read this before reading a result

**Each lock asks a question jojobot computes or a state the store owns**, so a
fact left in prose, or on the wrong record, yields the wrong answer and no
sentence rescues it. **No lock reads what the model said.**

**The locks ask where the store ended up, never which route got it there.** A
pause on an existing loop and a pause on a promise both hold their locks when
the loop is out of the owed answer before the day and in it after.

**Phase 2 plays the reader and its answers are not judged.** Whether a cold
reader goes wrong is read from the call log, not locked. The discrimination
locks are scoped to the state at the END OF PHASE 1, so a cold sitting that
finds a note in the wrong place and refiles it cannot hide the misbehaviour
the room measures.

**The six owed-by-day locks ask at the end of phase 1 too.** What is owed as of
a day is computed by the server, so they use the `phase-end` window: the run
sends each lock's own query when phase 1 ends and keeps the answer, and the lock
reads that answer, never the finished room.

**The pause on a request with no loop behind it has a thing to point at.** The
folding chairs exist at the start as a `thing` with no loop and no promise on
it. A pause is a promise regarding it, or the thing itself falling due, and the
lock asks the outcome and not the kind.

**The rule lock reads the boot, not the model's choice of key.** It does not
say whether a cold session that was handed a request the rule refuses would
have refused it. The boot is what a later session is handed, and a rule that is
not in it cannot be kept. The boot says how many rules it has seats for, so the
trap is solvable from what a model reads.

**The hold lock reads what the helper's own boot carries**: a rule it holds that
is marked to ride the boot, or a line in its charter. It does not read that the
words say "hold". A plain claim that is not marked, and a message in its box,
are not what its boot carries. The setup names no espresso manual, so only the
hold can put the word there.

## The identity rule — binding, and the same as every room's

**The model boots as the shipped `assistant` and stays there.** No charter is
written for it, and no hand-authored orientation of any kind reaches it. **A
session never boots as another bot.**

## What the room holds before the occupant arrives

```world
# The pause on a loop that exists. The fern falls due on the 27th of October on
# its own cadence, so before the operator is back it is owed unless something
# put it off. The air filter is the other loop: owed on every day the questions
# ask about, so an answer that lost every loop cannot read as the fern put off.
entity  thing:the-fern | The Fern
child   thing:the-fern | rhythm:water-the-fern | Water the fern
record  rhythm:water-the-fern | {"name": "Water the fern", "last_check_in": "2026-10-20", "cadence_days": "7", "counts_from": "2026-10-20", "advances_from": "due_date"} | watered the fern
entity  thing:the-air-filter | The Air Filter
child   thing:the-air-filter | rhythm:swap-the-air-filter | Swap the air filter
record  rhythm:swap-the-air-filter | {"name": "Swap the air filter", "last_check_in": "2026-10-01", "cadence_days": "30", "counts_from": "2026-10-01", "advances_from": "due_date"} | swapped the filter

# The pause on a request with no loop behind it. Nothing falls due on this, and
# nothing is set up for it. The handle is a roster handle, which is why it does
# not say what the thing is: the display name does.
entity  thing:folding-chairs | The Folding Chairs

# The party, and the three people the invitations go to.
entity  event:birthday-party | The Birthday Party
entity  person:homer | Homer Simpson
entity  person:maude | Maude Flanders
entity  person:ned-flanders | Ned Flanders

# The thing whose day is given and then corrected.
entity  thing:standing-desk | The Standing Desk
```

## Phase 1 — the brief

**Session: fresh.**

> start jojobot as assistant — I am away until 2026-11-20, so stop nagging me about watering the fern until then. Stop reminding me about returning the folding chairs until then as well.
> ---- answer before reading on ----
> I am sending out the invitations for the birthday party on 2026-11-28 to Homer, Maude and Ned. Maude already said yes. Twelve people are coming and each of them eats three donuts, so work out how many donuts to buy and note what you find. Then leave a short recap of the party planning for whoever picks it up.
> ---- answer before reading on ----
> The warranty on the standing desk runs out on 2026-12-03.
> ---- answer before reading on ----
> Correction on the standing desk: I misread the paper. The warranty runs out on 2026-12-10.
> ---- answer before reading on ----
> A standing rule: nothing before 10am, for anything you set up for me.
> ---- answer before reading on ----
> Some more standing rules, all of them for good. Keep every reply short. Never book anything on a Sunday. Ask me before you spend anything. Say the day of the week whenever you give a date. Write times in the twenty-four hour clock. Give me one option, not three.
> ---- answer before reading on ----
> I want a second assistant for the reading I keep not getting to. Give it a name that says what it is for. Hand it the long piece on how filesystems handle power loss, and the paper about consensus that everybody quotes.
> ---- answer before reading on ----
> There is a third thing on that pile, the manual for the espresso machine. Keep the second assistant off it until I say otherwise.

```locks
# THE SIX OWED-BY-DAY LOCKS ASK AT THE END OF THIS PHASE (`window phase-end`):
# what is owed as of a day is the server's answer, kept when the phase ends.
#
# The pause on a loop that exists. Two days are asked, either side of the one
# the operator gave. Before it, the loop is out of the owed answer and the
# other loop is in it. After it, the loop is back. A pause written as a note, or
# a loop taken out for good, fails one of the two; each rests on the other
# loop being there so an answer that lost everything cannot pass.
recall {"kind": "rhythm", "overdue": {"as_of": "2026-11-12"}}
carries rhythm:swap-the-air-filter
lacks   rhythm:water-the-fern
say     the fern is still owed on 2026-11-12, before the day the operator said to leave it until, so the pause never reached the loop
window  phase-end

recall {"kind": "rhythm", "overdue": {"as_of": "2026-11-21"}}
carries rhythm:swap-the-air-filter
carries rhythm:water-the-fern
say     the fern is not owed on 2026-11-21, after the day the operator said it could come back, so the loop was taken out for good or never returned
window  phase-end

# The pause on a request with no loop behind it. The thing the photos live on
# is the handle, so the lock asks whether anything owed regards it, by whichever
# route: a promise pointing at it, or the thing itself falling due. Before the
# day nothing regards it, and after the day something does.
recall {"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-11-12"}}
carries rhythm:swap-the-air-filter
lacks   thing:folding-chairs
say     something about the folding chairs is owed on 2026-11-12, before the day the operator said to leave it until
window  phase-end

recall {"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-11-21"}}
carries rhythm:swap-the-air-filter
carries thing:folding-chairs
say     nothing about the folding chairs is owed on 2026-11-21, so a pause that was meant to end has no day it ends on
window  phase-end

# Two copies drift. The corrected day is the one the owed answer reads.
recall {"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-12-05"}}
carries rhythm:swap-the-air-filter
lacks   standing-desk
say     the standing desk is owed on 2026-12-05, so the first day it was given is still the one the warranty is read from
window  phase-end

recall {"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-12-11"}}
carries rhythm:swap-the-air-filter
carries standing-desk
say     the standing desk is not owed on 2026-12-11, so the corrected day never reached the warranty
window  phase-end

# THE LOCKS BELOW READ THE STATE AT THE END OF THIS PHASE TOO.
#
# The note to self. The question is who has said yes, which jojobot answers
# from an attendance edge and from nothing a note carries. The lock is windowed
# and names its subject, so it reads Maude's own claims as they stood when this
# sitting ended.
search  {"query": "*", "subject": "person:maude"}
carries "type":"attendee"
say     nothing draws Maude to the birthday party as attending, so the question of who still needs an invitation lists her
window  own-phase

# Whose words were they. Two read apart: what the operator gave read back as
# the operator's word, and the worked-out count read back as a guess.
check   the_operators_numbers_read_back_as_theirs
say     the twelve people and the three donuts each do not read back as the operator's word, so a later session cannot tell what was said from what was worked out

check   the_worked_out_count_reads_back_as_a_guess
say     the number of donuts to buy reads back as the operator's word, or not at all, so it cannot be told from what the operator said

# A rule pushed out. The boot is what a later session is handed.
check   the_first_standing_rule_rides_the_boot
say     the rule about the morning is not in what a later session is handed at boot, so it cannot be kept

# Bot coordination. The hold is where the helper's own boot looks.
check   the_hold_is_on_the_helper
say     nothing the second assistant's own boot carries names the job it was to stay off, so the hold is where its reader does not look
```

## Phase 2 — the reader

**Session: fresh.**

> start jojobot as assistant — as of 12 November 2026 and again as of 5 December 2026, what is waiting on me, who still needs an invitation to the birthday party, which of the donut numbers did I give you and which did you work out, and is the second assistant free to take the espresso manual?

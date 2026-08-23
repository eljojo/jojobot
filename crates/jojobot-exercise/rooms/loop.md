# The loop room

**Two phases, and the second one has no memory of the first.** That is the
whole design. Everything else — the goal, the furniture, the four locks — is
there to make the second phase ask a question the first phase can only have
made answerable by working properly.

## Why a room needs a cold second phase

Inside one session a model answers any question out of its own context. It
wrote the thing a minute ago, so it remembers what it wrote, and the store is
optional: a session that recorded everything as prose still answers *how far
did I ride it* correctly, from memory. **A room with one phase therefore
measures whether the work was done, and cannot measure whether it was done
well.**

A session with no memory of the first is what makes the store load-bearing. It
has to ask jojobot, and what it gets back is whatever the first session left
reachable. **That is the only arrangement in which a badly shaped write costs
anybody anything**, and it is what the last lock here is built on.

## Why this goal, and not another

The mechanism only bites where **jojobot's own arithmetic is the answer**. A
cold session can read a number out of a sentence: *"the year's tally for 2026 —
4100 km"* answers a question about distance whether it was stored as a value or
as prose, so a room built on recall punishes bad structure weakly.

*Which of these has gone quiet as of the first of October* is not in any
sentence. It is a cadence, an anchor and a day, worked out by the machinery.
**A loop written down as prose is not a late loop — it is not a loop**, and no
amount of careful reading gets a session past that.

**Whoever picks the next room: choose a goal whose terminal question is a
COMPUTATION, not a recall.** That is the difference between a room that tests
the surface and one that tests reading comprehension.

## For whoever builds the next room

Two rules, both learnt here, both cheaper to read than to rediscover.

### Apply the range test BEFORE choosing the goal

Ask what the terminal question needs, and build the room only if the answer is
one of these:

* **A computation jojobot does and a reader cannot** — due-ness from a cadence
  and an anchor, a total folded from every write of a key, a comparison across
  more records than anybody will read. This room is the first kind.
* **State the store owns and no session could have written down** — where a
  message you sent got to, what somebody else's box has done with it. No
  sentence anywhere holds it, because the session that would have written it
  was not there when it changed.

**And do not build the room if the answer is a value that can be read out of a
sentence.** A cold session reads *"the year's tally for 2026 — 4100 km"* as
easily out of prose as out of a key, so a room built on recall passes a session
that stored everything badly. Say so before building rather than after.

### Every room carries TWO kinds of case, and they are named apart

* **The checks discriminate.** A furnished room nobody worked in, the obvious
  route, another route to the same end state, and the room somebody worked
  entirely in prose. These prove a check can fail and can hold.
* **The room is solvable.** The terminal question, asked through the served
  surface after a first phase done properly, returning exactly the answer the
  room is built around.

**The second one is not optional and it is not covered by the first.** A room
whose own arithmetic is wrong — a cadence that never falls due by the named
day, an anchor a day out — is unreachable by every session that will ever enter
it, and **every case of the first kind still passes**. The failure surfaces on a
paid run, reads as a product defect, and costs money to find. A room can be
perfectly discriminating and still be impossible, and from the free suite the
two look identical.

## The identity rule — binding, and the same as every room's

**The model boots as the shipped `assistant` and stays there.** No charter is
written for it, and no hand-authored orientation of any kind reaches it. The
entry names the identity, because a line that does not is ambiguous against a
surface where a bot is addressed by name — and it names nothing else.

## What the room holds before the occupant arrives

Furnished out of band, through the verbs, as the shipped identity, and then
wrapped, so the room looks like one somebody left tidy.

* **Three things**: `thing:kettle`, `thing:the-air-filter` and `thing:the-fern`.
* **One loop, kept properly**, under the fern: watered every ninety days,
  counting from the first of August, with the day it was last done. **It is
  both the teacher and a control** — it is where the vocabulary is to be found,
  and it is one of the two loops that must still be untouched at the end.
* **The brief**, waiting in the `assistant` box: two more things the operator
  keeps, one with a frequency and one deliberately without.

```world
entity  thing:kettle | The Kettle
entity  thing:the-air-filter | The Air Filter
entity  thing:the-fern | The Fern

# The teacher and the control. It shows what a kept loop looks like, and it is
# one of the two that must be left alone at the end.
child   thing:the-fern | rhythm:water-the-fern | Water the fern
record  rhythm:water-the-fern | {"name": "Water the fern", "last_check_in": "2026-08-01", "cadence_days": "90", "counts_from": "2026-08-01", "advances_from": "due_date"} | water it every ninety days, and it survives being forgotten

# The work and none of the method: no key, no verb, and no place anything is
# kept. An indented line continues the one above it.
message assistant | the things I keep having to do | The fern is already on here and that one works the way I want. Two more things to keep the same way.

    The kettle needs descaling every sixty days. I last did it on 2026-06-15.

    The air filter I swap when it looks bad, so there is no schedule for that one at all. The last swap was 2026-09-20.

    Leave it so whoever comes next can pick it up.
```

## Phase 1 — the room

**Session: fresh.** The occupant arrives with no memory of anything and no
context but the line below.

> start jojobot as assistant

```locks
# Reaching for Rust here says the assertion vocabulary cannot correlate a
# message's subject with that message's state.
check   the_brief_left_the_box
say     the brief is still sitting new in the box, so the occupant never learnt what the work is

# Both jobs stand as loops, one under each thing. A session that wrote two
# sentences has recorded the same words and left nothing that can fall due.
#
# **The occupant names its own loops**, so nothing here pins a handle it chose:
# a loop is found by the thing it hangs under, which is furniture. The fern's
# loop is the positive — an answer that lost everything cannot read as two
# loops missing.
list_entities {"kind": "rhythm"}
carries "parent":"thing:kettle"
carries "parent":"thing:the-air-filter"
carries "parent":"thing:the-fern"
say     one of the two jobs the brief named does not stand as a loop under the thing it belongs to

# The scheduled loop carries its frequency and the day the brief gave it. It is
# SELECTED by the frequency and its day is read off the key's own HISTORY,
# because the cold phase moves what it holds now.
#
# ⚠️ **`scope: record` on the selection.** A `fields` filter asks what the thing
# HOLDS unless it says otherwise, so selecting on the fold made this a lock on
# nothing later rewriting the frequency. "The cold phase does not touch it" was
# true and was not the point: a phase-1 lock must not report phase 1 at fault
# for what a later phase wrote.
recall {"kind": "rhythm", "fields": [{"key": "cadence_days", "value": "60", "scope": "record"}], "history": "last_check_in"}
carries "parent":"thing:kettle"
carries "value":"2026-06-15"
say     no loop with a sixty-day frequency hangs under the kettle carrying the day the brief gave it

# 🚨 The load-bearing half, and it is an absence: the loop the operator keeps NO
# schedule for was taken anyway, with no frequency invented for it. A surface
# that demanded one would have lost that loop entirely.
#
# ⚠️ **`scope: record` on the selection**, for the reason the lock above gives.
# On the fold, a cold phase that moved this loop would empty the selection and
# this lock would report phase 1 never recording it — blaming the phase that did
# its job. The lock below is what says the filter must stay where it stands.
recall {"kind": "rhythm", "fields": [{"key": "last_check_in", "value": "2026-09-20", "scope": "record"}]}
carries "parent":"thing:the-air-filter"
lacks   cadence_days
say     the loop the operator keeps no schedule for was given one, which nobody said — or it is not under the filter at all
```

## Phase 2 — the day the question is asked

**Session: fresh.** No memory of phase 1 — deliberately, and it is the whole
instrument. Everything this phase needs, the phase before it had to leave
behind.

> start jojobot as assistant — as of 1 October 2026 one of the things I keep on there had gone quiet, and I have just done that one, so put it on the record

```locks
# The terminal lock, and the reason the room has a cold phase. As of the day the
# operator names, exactly one of the three has fallen due: the fern is not due
# for another month, the filter has no schedule and so can never be late, and
# the kettle went past its day in August.
#
# All three loops in one answer, each read by what it HOLDS now. A session that
# checked everything in did not answer the question, it painted the wall.
recall {"kind": "rhythm"}
carries "last_check_in":"2026-09-20"
carries "last_check_in":"2026-08-01"
lacks   "last_check_in":"2026-06-15"
say     the cold session moved a loop that was not due, or left the one that was where it stood
```

## What this room cannot measure

**Whether the occupant understood why the filter can never be late.** A loop
with no frequency is never overdue, and a session that leaves it alone because
it worked that out and one that leaves it alone because it never thought about
it leave the same store behind. The transcript is where that reads.

**Whether the goal was reached elegantly.** The room measures that the loops
can be asked about, that the one the operator keeps no schedule for survived
being recorded, and that a session with no memory could tell which one had
slipped. Everything else is a person's to read.

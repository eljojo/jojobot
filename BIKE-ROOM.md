# The bike room

**One goal, one line of entry, and every other thing the occupant has to find
for itself.** The model is not told what jojobot serves, which verb to reach
for, or that anybody is measuring it. What it gets is one line. Everything
after that it works out from the surface as shipped.

## Why this is not the cold-session suite

The suite beside this one walks a model through eighty numbered steps and asks
it to report on each. That measures whether a session **can** reach a
capability once somebody points at it, and it is the right instrument for that
question. What it cannot measure is whether a session **does**: a step that
says "call `read_mailbox` with counts only" has already found the mailbox for
the occupant, and a phase that asks for PASS or FAIL has said that a test is
running and that somebody is reading the answer.

So this room hands over a goal and nothing else. **The locks are the product's
own routing.** The brief is not in this document — it is waiting in a mailbox,
which is where the first lock is: a session that never finds its box never
learns what the work is, and every lock after that is out of reach.

**Nothing here asserts on the occupant's words.** Every lock is a claim about
the state the room is left in, read back through the served surface. What the
model said is a transcript a person reads, and it decides nothing.

## The identity rule — binding, and the same as the suite's

**The model boots as the shipped `assistant` and stays there.** No charter is
written for it, by the furniture or by anything else, and no hand-authored
orientation of any kind reaches it. A room that handed the occupant a bespoke
charter would measure the charter somebody wrote for the test.

**The entry line names that identity, and nothing else.** It did not, once:
the line was `start jojobot`, and a model read it as *boot as a bot called
jojobot* — the product and a bot are addressed the same way, so the line parses
both ways and a run died on the ambiguity in one turn. The identity was never
what this room measures. **The box is**, and the entry still names no box, no
verb and no store.

## The goal

**"Keep track of my bikes — what they are, what has been done to them, how much
I ride them, and what is still under warranty."**

A thing with a long life rather than a project with an end, so what it probes
is **accumulation**: the same measurement year after year, work done under it,
and the question asked months later. The room is already two years into that
when the occupant arrives, which is the whole design — a room with nothing in
it teaches no vocabulary, and every key the occupant invented would be as good
as any other.

## What the room holds before the occupant arrives

Furnished out of band, through the verbs, as the shipped identity — and then
that furnishing session is wrapped, so the room looks like one somebody left
tidy.

* **Two bikes**: `thing:gravel-bike`, ridden most weeks, and `thing:road-bike`,
  in the basement. The second is not decoration — without a bike whose cover
  has run out, "what is still under warranty" comes back with everything and
  looks like an answer.
* **What has accrued**: two years of distances on the ridden bike, under a key
  the occupant is never told about; the last service, with what was done and
  when; and the day each bike's frame cover runs out.
* **The brief**, waiting in the `assistant` box: what the operator wants, in an
  operator's words, with no verb anywhere in it.

**The words live in `crates/jojobot-exercise/src/bike_room.rs`, and they live
there once.** A copy of the brief in this document is a copy that drifts away
from the one a run actually posts.

## The locks

Each is asserted on store state, and each is written to hold for a route
nobody predicted. **They are numbered in the order the goal reaches them. That
is not a claim that each depends on the one above it** — locks 2, 3 and 4 all
stand on lock 1, and none of them stands on each other.

1. **The brief left the box.** The message waiting for `assistant` is no longer
   `new`. Delivery is the claim and not the verb that took it: draining the
   box, taking the one message, posting from inside it and retiring it straight
   from `new` all count. The positive it rests on is that the brief is on the
   board at all — an unfurnished room has nothing to move.
2. **The service went on the bike as a value.** The day the brief gives is
   readable off `thing:gravel-bike` as a value: under a key on a record, or as
   the day the record is dated. Which key is the occupant's business. A day
   written into a sentence does not hold, and the check says which of the two
   it found.
3. **This year's distance joined the years before it.** The key the earlier
   distances are under carries a third write, and the bike now reads this
   year's number. The room never says what that key is called; a session that
   starts a set of one leaves three numbers that are not a set.
4. **A handoff is waiting.** Either a message the occupant left, or a run left
   open saying what it was working on. Both are rails the product offers, and
   the check takes either — choosing one would fail a session that chose the
   other and call it a product failure.

## Phase 1 — the room

**Session: fresh.** The occupant arrives with no memory of anything and no
context but the line below.

> start jojobot as assistant

## What this room cannot measure

**Whether the warranty question was answered.** The brief asks which bike is
still covered, and the answer is prose: a session that reads both dates and
says the right one leaves the same store behind as a session that says nothing.
It is in the brief because it is the question the operator actually asks, and
it is a person's to read off the transcript.

**Whether the goal was reached well.** The room measures that a session with
one line of context found its work, put what it learnt where a later question
can reach it, and left the place better for whoever comes next. Everything
else is the transcript's.

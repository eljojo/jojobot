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

```world
entity  thing:gravel-bike | Gravel Bike
entity  thing:road-bike | Road Bike

fact    thing:gravel-bike | testimony | ridden most weeks
fact    thing:road-bike | testimony | hanging in the basement, unridden for two years

# Two years of distances, under a key the occupant is never told. It is there
# to be found, and finding it is what makes this year's number join the years
# before it instead of starting a set of one.
record  thing:gravel-bike | {"km": "2600", "year": "2024"} | the year's tally for 2024
record  thing:gravel-bike | {"km": "3800", "year": "2025"} | the year's tally for 2025
record  thing:gravel-bike | {"done_on": "2025-04-18", "work": "chain, cables, bearings"} | annual service
record  thing:gravel-bike | {"expires": "2029-04-11"} | frame warranty
record  thing:road-bike | {"expires": "2025-06-30"} | frame warranty

# The whole of what the occupant is told, and it arrives as mail. It carries
# the work and none of the method. An indented line continues the one above it.
message assistant | the bikes, and a few things I want off them | Both bikes are on here already, and the gravel one is the one I actually ride.

    Three things, none of them urgent.

    The gravel bike went in for its service on 2026-08-11 — chain and cables, and they left the bearings alone this time. Put it with the rest of what has been done to it.

    This year came to 4100 km on it. The road bike has not moved at all.

    And tell me which of the two is still covered by its warranty. I keep working that out by hand and I would rather ask.

    When you are done, leave what you did where whoever comes next will pick it up. I will ask again in a month.
```

## Phase 1 — the room

**Session: fresh.** The occupant arrives with no memory of anything and no
context but the line below.

> start jojobot as assistant

```locks
# The brief left the box. Delivery is the claim and not the verb that took it:
# draining the box, taking the one message, posting from inside it and retiring
# it straight from `new` all count. Reaching for Rust here says the assertion
# vocabulary cannot correlate a message's subject with that message's state:
# `lacks "state":"new"` is a claim about EVERY message, and it fails the moment
# the occupant posts one of its own.
check   the_brief_left_the_box
say     the brief is still sitting new in the box, so the occupant never learnt what the work is

# The service went on the bike as a VALUE — under a key on a record, or as the
# day the record is dated — rather than into a sentence. Which key is the
# occupant's business, and a query cannot ask whether a string is a value under
# SOME key rather than prose, so this one reaches for Rust.
check   the_service_day_is_a_value
say     the service day went onto the bike as prose, so what has been done to it cannot be asked for

# This year's number joined the years before it. Three writes of the key the
# room already keeps its distances under, and this year's among them.
recall {"subject": "thing:gravel-bike", "history": "km"}
at least 3 of "value":
carries "value":"4100"
say     the distance key does not carry a third write with this year's number, so the year went somewhere a question cannot reach

# And the bike READS as this year's now. The write could have landed and been
# older than the ones before it; what a thing holds is the newest write.
recall {"subject": "thing:gravel-bike"}
carries "km":"4100"
say     the bike's distance does not read as this year's number, so the newest thing recorded about how far it goes is not this year

# A handoff is waiting: EITHER a message the occupant left, or a run left open
# saying what it was working on. Both are rails the product offers, and an
# assertion has no way to say "either of these", so this one reaches for Rust.
check   a_handoff_is_waiting
say     no message was left and no open run says what it was doing, so the next session arrives at what the occupant found and not at what it did
```

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

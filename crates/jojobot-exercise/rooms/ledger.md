# The ledger room

**The words the operator uses, and the ones nobody agreed to.** Two phases, the
second with no memory of the first — the standard shape, and the reason it is
standard is written down in `loop.md`.

## The goal

**"I have been writing down the jobs I pay people for, and I want to be able to
ask which of them are still owing."** The operator names the three words for
that, once, in the brief. Everything else follows from whether the
first session did anything with them.

## Why this goal passes the range test

The terminal question is *which of these is filled in with a word I do not
use*, and it is answerable in exactly one way: **the set of allowed words lives
in a declaration, and a declaration is the only thing that carries it out of
the first phase.** The brief says the three words to a session that is gone by
the time the question is asked.

So a first phase that wrote the jobs down and declared nothing leaves the cold
session no way to tell `sent` from `paid` — on the record they look the same,
and nothing anywhere says which of them the operator uses. **A value outside a
declared set does not count as holding its key**, so with the declaration the
question answers itself and without it there is nothing to ask.

## What the room holds before the occupant arrives

Six things and four jobs, written months ago by somebody who had no type in
mind. Two of the jobs carry words the operator uses, two carry words nobody
agreed to, and two things carry no job at all.

**Two odd words rather than one, on purpose.** One outlier among four is a
pattern a reader can guess at; two make the question about the set rather than
about the odd one out.

**And the brief's new jobs go on the things that carry no job.** What a thing
holds is what its keys fold to, and conformance is asked of the thing rather
than of one record — so a new job on `thing:kettle` would fold over the word
nobody agreed to and hide the whole question. The first version of this room
did exactly that, and the terminal question came back saying everything was in
order.

```world
entity  thing:jukebox | The Jukebox
entity  thing:torque-wrench | The Torque Wrench
entity  thing:kettle | The Kettle
entity  thing:the-air-filter | The Air Filter
entity  thing:floor-pump | The Floor Pump
entity  thing:gravel-bike | The Gravel Bike

record  thing:jukebox | {"cost": "180", "settled": "paid"} | new valves
record  thing:torque-wrench | {"cost": "55", "settled": "invoiced"} | calibration
record  thing:kettle | {"cost": "25", "settled": "pending"} | descaled by the shop
record  thing:the-air-filter | {"cost": "18", "settled": "sent"} | filter swap

# The brief, and the only place the three words are ever said. An indented
# line continues the one above it.
message assistant | the jobs I pay for, and the words I want on them | I have been writing down the jobs I pay people for, and I want to be able to ask which of them are still owing.
    From now on there are three words for where a job has got to, and no others: invoiced, paid, waived.

    Two to put on, both done on 2026-08-11. The floor pump was serviced, thirty five, and I paid on the spot. The bike has a new chain, sixty, and they have invoiced me for that one.

    Leave it so whoever comes next can pick it up.
```

## Phase 1 — the room

**Session: fresh.**

> start jojobot as assistant

```locks
# A lock carries no session, so the board is read rather than a box. The rule
# and its reason live in the format's own text.
search {"query": "*", "include_mail": true, "limit": 200}
carries the jobs I pay for, and the words I want on them
lacks   "state":"new"
say     the brief is still sitting new in the box, so nobody took delivery of it

# The room keeps its jobs under two keys and tells the occupant neither: a job
# written under a key of somebody's own invention is a job the operator's
# question never reaches. So the assertion names the key as well as the value.
recall {"subject": "thing:floor-pump"}
carries "cost":"35"
carries "settled":"paid"
say     the pump job is not on the floor pump under the keys the older jobs use, with the cost and the word the brief gave it

recall {"subject": "thing:gravel-bike"}
carries "cost":"60"
carries "settled":"invoiced"
say     the chain job is not on the gravel bike under the keys the older jobs use, with the cost and the word the brief gave it
```

## Phase 2 — the word nobody agreed to

**Session: fresh.** No memory of phase 1.

> start jojobot as assistant — some of the jobs on there are filled in with words I do not use, and I want those ones saying invoiced instead

```locks
# The two jobs the cold session is here for.
recall {"subject": "thing:kettle"}
carries "settled":"invoiced"
say     the kettle still carries the word nobody agreed to, or its job lost its word altogether

recall {"subject": "thing:the-air-filter"}
carries "settled":"invoiced"
say     the air filter still carries the word nobody agreed to, or its job lost its word altogether

# The positive the two above rest on. A session that painted every job the same
# word leaves no word nobody agreed to and has answered nothing.
recall {"subject": "thing:jukebox"}
carries "settled":"paid"
say     the jukebox's job no longer says paid, so a word that was already right was painted over

recall {"subject": "thing:torque-wrench"}
carries "settled":"invoiced"
say     the torque wrench's job no longer says invoiced, so a word that was already right was painted over
```

## What this room cannot measure

**Whether the occupant declared the type for the right reason.** A session that
declared it because the brief asked for consistent words and one that declared
it because declaring seemed like the done thing both leave a declaration. The
transcript is where that reads.

**Whether the cold session found the odd words by asking or by guessing.** Two
words among four is a small enough field that a reader might pick them out by
eye. The room is built so that guessing is not reliable — nothing on the record
says which words are the operator's — but it cannot rule it out, and a person
reading the transcript can see which happened.

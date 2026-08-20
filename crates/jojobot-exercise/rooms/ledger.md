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
mind. Two of them carry words the operator uses — `thing:jukebox` says paid and
`thing:torque-wrench` says invoiced. Two carry words nobody agreed to —
`thing:kettle` says pending and `thing:the-air-filter` says sent.
`thing:floor-pump` and `thing:gravel-bike` carry no job at all, and the brief's
two new jobs belong on them.

**Two odd words rather than one, on purpose.** One outlier among four is a
pattern a reader can guess at; two make the question about the set rather than
about the odd one out.

**And the new jobs go on things that carry no job.** What a thing holds is what
its keys fold to, and conformance is asked of the thing rather than of one
record — so a new job on `thing:kettle` would fold over the word nobody agreed
to and hide the whole question. The first version of this room did exactly
that, and the terminal question came back saying everything was in order.

**The words live in `crates/jojobot-exercise/src/ledger_room.rs`, once.**

## The locks

1. **The brief left the box.** No longer `new`, whichever verb took delivery.
2. **The new jobs use the keys the old jobs use, and a word the operator uses.**
   The room keeps its jobs under two keys and names neither; a job under a key
   of the occupant's own invention is a job the operator's question never
   reaches. The positive it rests on: the older jobs are still there to have
   been read.
3. **The words nobody agreed to are gone, and the rest are as they were.** The
   terminal lock. Both halves — a run that swept every job to one word leaves
   no word nobody agreed to and has answered nothing, so the two that were
   already right must still say what they said.

## Phase 1 — the room

**Session: fresh.**

> start jojobot as assistant

## Phase 2 — the word nobody agreed to

**Session: fresh.** No memory of phase 1.

> start jojobot as assistant — some of the jobs on there are filled in with words I do not use, and I want those ones saying invoiced instead

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

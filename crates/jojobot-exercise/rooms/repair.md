# The repair room

**Three cold sittings, and the second and third only make sense if the first
left something to put right.** The goal is not to file facts well. It is to
find out whether a model that holds a mistake in the record can find the way to
mend it, from where it stands, with nobody naming the move.

## Why a room, and what each mistake is for

The paid runs have made four mistakes, and each has a repair that already
exists on the surface:

* **A fact under the wrong key.** A deadline written under a key of the
  model's own where a shipped type holds it: the `runs-out` type on the thing,
  or a promise, which is the kind a commitment about a thing belongs to. The
  repair is one edit that sets one key and takes the other off.
* **A fact on the wrong subject.** The repair is a capture on the right
  subject and an archive of the old claim.
* **A duplicate.** One place named twice, with a claim on each. The repair is
  `merge_entities`.
* **A retired loop that still falls due.** The repair is to take the loop out of
  the owed read, and the loop's history stays.

**The room gives every fact and never the move.** The entries name no verb, no
type and no key. They say what the operator wants put right, in the operator's
words.

## What this room can and cannot measure — read this before reading a result

**Only the first mistake is the model's own.** The loan is filed by the model in
the first sitting, in the operator's ordinary words, which is the wording that
has tempted a key of the model's own. A run that files it under a key a shipped
type holds the first time, `runs_out` or a promise's day, **never made the
mistake, and is not evidence about repair**. The call log says which:
`adoption_rows` over a kept stream lists the keys the run wrote, and a loan under
one of those keys alone with nothing cleared is such a run.
The second sitting then asks a question the invented key breaks, so a run that
did err has to notice and repair for the lock to hold.

**The other three are furniture.** The wrong person, the two names for one
place and the fern that was given away are in the room before the model
arrives, and the operator names what is wrong in the third sitting. They
measure whether the model can find and carry out the repair. They do not
measure whether it would have noticed. **A model that mends them has
demonstrated the repair, not the noticing.**

**The locks ask where the store ended up, never which route got it there.**
The place has two honest routes, a merge and a copy of the claim onto one place
with the other archived, so its lock is a named check that accepts either and
says in the run's own report which one held. A claim on the wrong person that
was rewritten into its own denial still carries the allergy key and reads red,
which is the room doing what it is for: the record still says the wrong thing
on the wrong person. That is a wrong end state and not another route.

**No lock asks that the repair used a link back to the old claim.** That would
grade the move, and the room would be coaching it.

## The identity rule — binding, and the same as every room's

**The model boots as the shipped `assistant` and stays there.** No charter is
written for it, and no hand-authored orientation of any kind reaches it.

## What the room holds before the occupant arrives

```world
# The wrong person. The allergy is a field on Bart, which is what the operator
# will say is not his; a claim archived or a key cleared takes it off him.
entity  person:bart | Bart Simpson
entity  person:homer | Homer Simpson
record  person:bart | {"allergy": "peanuts"} | Bart is allergic to peanuts

# One place named twice, with a claim on each handle. The names do not
# resemble one another, so the two could be created.
entity  place:atlas | The Atlas Tavern
entity  place:bet | Betty's Bar
fact    place:atlas | testimony | the back booth is the quiet one
fact    place:bet | testimony | the Thursday quiz night is at Betty's Bar

# Two loops, both owed by the fourth of November. One of them is the fern the
# operator is about to say they gave away; the other must stay owed, so an
# answer that lost every loop cannot read as the fern gone.
entity  thing:the-fern | The Fern
child   thing:the-fern | rhythm:water-the-fern | Water the fern
record  rhythm:water-the-fern | {"name": "Water the fern", "last_check_in": "2026-08-01", "cadence_days": "90", "counts_from": "2026-08-01", "advances_from": "due_date"} | watered the fern, and it keeps being forgotten
entity  thing:the-air-filter | The Air Filter
child   thing:the-air-filter | rhythm:swap-the-air-filter | Swap the air filter
record  rhythm:swap-the-air-filter | {"name": "Swap the air filter", "last_check_in": "2026-09-01", "cadence_days": "30", "counts_from": "2026-09-01", "advances_from": "due_date"} | swapped the filter, and it is due again
```

## Phase 1 — the loan

**Session: fresh.**

> start jojobot as assistant — Ned Flanders lent me his hedge trimmer and it has to go back to him by 2026-11-03. Keep track of that for me.

```locks
# The loan is on record under SOME key. Phase 2's lock asks for the shipped one,
# so the difference between the two is exactly the mistake: a run that holds
# this and not that wrote the day under a key of its own. Companion: the claim
# that carries it must be standing.
recall {"fields": [{"value": "2026-11-03"}], "facts": true}
carries "2026-11-03"
carries "status":"active"
say     the loan's last day is not on record under any key, so the first sitting never filed it
```

## Phase 2 — the question the loan has to answer

**Session: fresh.**

> start jojobot as assistant — as of 4 November 2026, what is waiting on me, and what has already gone past?

```locks
# The loan's last day is the day something is owed by, which is what the owed
# question reads. Jojobot stores that day under `due_on` for every shipped type
# that owes by a day, so a loan filed under `runs_out` and a loan filed as a
# promise end in the same place and both hold this. A model that wrote the day
# under a key of its own and never noticed leaves nothing the owed question can
# find; one that noticed and mended it leaves this. A model that got it right the
# first time leaves it too, and is not repair evidence.
recall {"fields": [{"key": "due_on", "value": "2026-11-03"}]}
carries "due_on":"2026-11-03"
say     the loan's last day is not owed by the day it was given, so the question about what is owed cannot find it and nothing put it right
```

## Phase 3 — three things to put right

**Session: fresh.**

> start jojobot as assistant — a few things to put right. Bart is not the one who is allergic to peanuts, Homer is. The Atlas Tavern and Betty's Bar are one and the same place. And I gave the fern away, so I am not watering it any more.

```locks
# The correction reached the person it was about.
recall {"subject": "person:homer", "facts": true}
carries "id":"person:homer"
carries "status":"active"
carries peanut
say     Homer carries nothing about peanuts, so the correction never reached the person it was about

# And it left the person it was not about. The allergy is a field, which the
# fold reads from active claims only, so an archived claim or a key taken off
# both clear it. Paired with Bart being there at all, so an answer that lost
# everything cannot pass.
recall {"subject": "person:bart"}
carries "id":"person:bart"
lacks   "allergy"
say     Bart still holds the allergy the operator said is not his, so the wrong person's record was left standing

# One place, not two. The operator's words give a state and not a move, so two
# end states hold it: a merge record on a place that carries both claims, or both
# claims active on one place with the other archived. The lock format does not
# branch, so a named check reads both and says which route held.
check   one_place_after_the_correction
say     the two names for one place are still two places

# The fern has left the owed read and the other loop has not. As of the day the
# operator was speaking in.
recall {"kind": "rhythm", "overdue": {"as_of": "2026-11-04"}}
carries rhythm:swap-the-air-filter
lacks   rhythm:water-the-fern
say     the fern the operator gave away is still owed, or the other loop went with it
```

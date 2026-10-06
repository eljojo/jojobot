# The adoption room, worded in the keys' own words

**One sitting, one line, two facts, and nothing that names a type, a key or a
verb.** It exists to answer one question about the surface: when the operator
hands over a fact a shipped type exists for, does the model write it under that
type's keys, and if it does not, what did it do instead?

## Why a room and not the year

A year room files dozens of facts and a lock fails somewhere in it. Many of
those failures are one failure — the model wrote a deadline under a key it
invented (`needed_by`, `guarantee_expires`) beside a shipped type that names the
right one — but a lock reports where the store ended up, and four different
routes end in the same place:

* it never looked at the type before it wrote;
* it looked, and the operator's words did not map to the key's name;
* it mapped them and took the free-form road because nothing pushed back;
* it was variance.

This room has one sitting and two facts, so a run's call log is short enough to
read, and the same sitting can be run many times under one condition at a time.
**Its output is the call log, read by `adoption_rows`. The locks only say
where the store ended up.**

## The two facts

* **A loan that has to end on a day** — a promise, whose day is the key
  `promised_by`. A thing under the shipped `runs-out` type, key `runs_out`,
  holds the same day, and it counts as a shipped date key in the call-log table
  and not as the loan's own.
* **A decision that has to be made by a day** — the shipped `decide-by` type,
  key `decide_by`.

The entry gives both facts and the goal, in the operator's own words, and
nothing else. **It never names a type, a key or a verb**, and a case holds that
against the verbs the room actually serves.

## The condition this document belongs to

This is **E1, the wording**: the same room as `adoption.md`, with each fact said
in the key's own word — *runs out on*, *decide by* — and nothing else changed.
It is a change to what the operator says, not a hint: the entry still names no
type, key or verb.

## The identity rule — binding, and the same as every room's

**The model boots as the shipped `assistant` and stays there.** No charter is
written for it and no hand-authored orientation reaches it.

## Phase 1 — the sitting

**Session: fresh.**

> start jojobot as assistant — Ned Flanders lent me his hedge trimmer and the loan runs out on 2026-11-03. Also I have to decide by 2026-10-20 about the tickets for the Krusty show. Keep track of both for me.

```locks
# Each lock is where the store ended up, with the day the entry gave. The
# loan's lock asks the day something is owed by, `due_on`, which jojobot stores
# for every shipped type that owes by a day: a promise and a thing under
# `runs_out` both leave it. The decision's lock asks the key its type holds. A
# model that wrote the day under a key of its own leaves neither: the day is in
# the store and nothing the types' reads can find.
recall {"fields": [{"key": "due_on", "value": "2026-11-03"}]}
carries "due_on":"2026-11-03"
say     the loan's last day is not owed by the day it was given, so nothing that asks what is owed can find it

recall {"fields": [{"key": "decide_by", "value": "2026-10-20"}]}
carries "decide_by":"2026-10-20"
say     the day the tickets have to be sorted by is not under the key the shipped type holds, so nothing that asks what is waiting on a decision can find it
```

## What this room cannot measure

**Why a model chose the key it chose.** A run that wrote `promised_by` after
looking and one that wrote it from habit leave the same store. The call log says
which calls came first; the transcript says what the model thought.

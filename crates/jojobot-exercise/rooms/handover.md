# The handover room

**A second identity, and whether the operator's assistant can tell what became
of the work.** Two phases, the second with no memory of the first — the
standard shape, and the reason it is standard is in `loop.md`.

## The goal

**"I want a second assistant for the reading I keep not getting to."** Give it
a name, tell it what it is for, hand it three things one at a time. Then, later
and cold: **how much of it has that assistant actually started on?**

## The constraint SHAPED this goal rather than limiting it

**A session is bound to its bot for life and never boots as another to reach
its data.** So the model cannot pick the work up from inside the box it just
created, and **this room cannot measure that the colleague did the work.**
Nothing can, over this surface, and a room built to measure it would be
unsolvable.

What it measures instead is the better question, and it is one a person
actually has: **can the operator's assistant tell whether the work got done?**
Anybody building a room about a second identity will hit the same wall. This is
the way through it.

## Why this goal passes the range test

Not on arithmetic — on the third lever: **state the store owns that no session
could have written down.** How much of a pile somebody else has picked up is
in no sentence anywhere, because the session that would have written it was not
there when it changed. The cold session cannot open the colleague's box, and it
cannot guess how many things were handed over.

**`list_sent` exists for exactly this** — where a message got to is not private
to whoever sent it — and it is the only way to the answer.

## What the room holds before the occupant arrives

**Almost nothing, and that is the room.** A seed cannot stand an identity up:
creating one is the behaviour under test. So the only furniture is the brief,
and everything the checks read is what the occupant made.

```world
# The whole of what the occupant is told. It asks for a colleague and hands
# over three things. It names no verb, says nothing about how an identity is
# made, and does not mention that a box comes with one — which is the part the
# room is watching for. An indented line continues the one above it.
message assistant | the reading pile, and somebody to take it | I want a second assistant for the reading I keep not getting to. Give it a name that says what it is for, and tell it what it is there to do.

    Three things to hand over, one at a time so they can be worked separately:

    The long piece on how filesystems handle power loss. The paper about consensus that everybody quotes and nobody has read. And the manual for the espresso machine, which is forty pages and which I am never going to read.

    Leave it so whoever comes next can pick it up.
```

⚠️ **Every lock in this room but the first reaches for Rust, and that is the
finding rather than a shortcut.** Three of the four are about **whichever bot
is not the one that ships** — a thing the OCCUPANT named. A lock's query is
text written before the run, nothing in the query surface computes *the one
that is not this one*, and pinning a handle would assert that the occupant
guessed the same word this document did.

## Phase 1 — the room

**Session: fresh.**

> start jojobot as assistant

```locks
check   the_brief_left_the_box
say     the brief is still sitting new in the box, so the occupant never learnt what the work is

# A box is not made: it opens with the bot that owns it, in the one act, and
# the occupant is never told that. Two colleagues where the brief asked for one
# is a fail — the room asked for a second assistant, not for a habit of making
# them.
check   a_colleague_exists_with_its_box
say     there is no second identity carrying a box of its own, so there is nobody to hand anything to

# One thing at a time, because the brief asks for them separately so they can
# be worked separately. A run that handed the whole pile over as one message
# has left a colleague one thing to finish rather than three.
check   the_pile_is_in_the_colleagues_box
say     the reading pile is not waiting in the colleague's box as three separate things sent by the assistant
```

## Phase 2 — what became of it

**Session: fresh.** No memory of phase 1.

> start jojobot as assistant — how much of that reading pile has the second assistant actually started on? put what you find onto that assistant itself, so I can see it there

```locks
# The terminal lock. Both halves: the count is written where the operator asked
# for it, and the pile really is untouched — so a run that wrote a number
# without looking is not credited with an answer that happens to be right.
#
# The count is accepted as a figure or as the word. Which one a session writes
# is not what this room measures.
check   what_became_of_the_pile_is_on_the_record
say     nothing on the colleague says how much of the pile is still waiting, so a later session has to go and find out again
```

## What this room cannot measure

**Whether the colleague is any good.** Nothing in this instance can boot as it,
so the pile stays where it was put. That is the constraint above, and it is why
the room asks what it asks.

**Whether the cold session used the sender's own view or guessed.** Three is a
small number and a session might write it down without looking. The room is
built so guessing is unreliable — nothing on the record says how many things
were handed over — but a person reading the transcript can see which happened.

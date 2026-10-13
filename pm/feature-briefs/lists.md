# Lists — ordered entries on the thing they are about, and what a tick does

> **jojobot** · kind: feature-brief · boot: on-demand

**A list is an ordered set of short entries hung on the thing it is about**: Bart's packing list on the camping trip, the checklist on a piece of work, a line's queue on its bot, Marge's week plan, the rotation of who takes out the bins. One kind serves them all, because what differs between them is only what happens when an entry is ticked.

## One kind, and its setting says what a tick does

**A list carries a closed `tick` setting: `sink`, `leave` or `back`.** A checklist sinks: a done entry stays visible, at the bottom, in the order things were done, because seeing what is finished is part of using a checklist. A queue leaves: a done entry drops out of the open view and stays in the history. A rotation goes back: ticking records that the turn happened and moves the entry to the end, still open, because a chore that has been done this week is next due after everyone else's.

**Doing an entry and dropping it are different acts.** An entry that no longer applies is dropped, and the history says it was dropped, never that it was done. A rotation that is ticked is not finished; it has taken a turn.

**"Next" is the first open entry in the list's order.** Nothing else is folded into it.

## An entry is a record on the list, and a tick never edits it

**An entry is a short claim on the list: a few words, or a claim whose edge points at a thing** (a piece of work, a person, a place). A link is membership, never a copy of the thing and never a transfer of it. **An entry is not an entity**, because a checklist line has no life of its own; one that earns ownership or dependencies points at a piece of work instead.

**A tick is its own record on the list, naming the entry, the act (done, undone, rotated, dropped) and the session**, written by a `tick` argument on capture. It never rewrites the entry. That keeps the entry's words, author and provenance whole, so a checklist Homer wrote stays Homer's testimony when a bot ticks it, and it makes the history an ordinary read: which entry was ticked, when, and by whom.

**The open order and each entry's state are computed from the tick records, not stored on the entries.** So a rotation can turn every day for years without rewriting anything, and a retried tick is recognised by its request id instead of rotating twice.

**The list's read returns its entries with their computed state. The ticks are their own paged read**, never the default dump: a daily rotation writes a year of ticks, and the list's own read stays bounded however many accumulate.

## Order is minted by the server, and a move wins until the next tick

**A deliberate move (put after, to the front, to the end) mints an order key between the neighbours**, the same way a document's sections are ordered, and minting is serialised per list so two callers can never mint the same key. **On a list whose order is computed from ticks, a move puts the entry where the caller asked, and that position holds until the entry is next ticked.**

## Position is not readiness

**A linked entry shows the linked thing's own state beside it**: a piece of work's status, owner and what it waits on. The list never skips a blocked entry or promotes a ready one, and ticking an entry says the list item is done, not that the work is. **The work's own status stays the one record of whether the work is finished**; otherwise a queue, a plan and the work item would be three places disagreeing about the same fact.

## A plan is a list that says what it covers

**A day or week plan is an ordinary list carrying the period it covers: one calendar day, or one ISO week, in the instance's zone.** It needs no day entity: the list is already the thing with an address and a history. What is due, overdue, ready or on the calendar for that period is read when the plan is read, never stored as the plan. **Carrying an unfinished entry into the next plan is an explicit act that records where it came from**; nothing rolls over silently, so a past week's plan still says what was chosen and what was done that week.

## What a bot is working on

**What a bot is working on is a key on the bot itself, pointing at the thing, written by its session.** It is declared and cleared with a `working_on` argument on journal, and filled in automatically when a session moves work its own bot owns to `now`. **It is shown only while the session that wrote it is live, and reads as unknown otherwise**, because a wrapped run's last link is history, not current activity. Any reader can see it, which is the point: who is doing what is the question every colleague asks.

## What it is not

- **Not three kinds.** A checklist, a queue and a rotation share ordered membership; three kinds would be three code paths for one behaviour that differs only in its tick.
- **Not an entity per entry.** A packing list is not a dozen things.
- **Not a task store.** Work lives on work items; a list orders and ticks references to them.
- **Not a calendar.** A plan says what was chosen for a period, not when anything happens in it.
- **Not a second notepad.** A bot's thoughts stay its private notes; a list is shared intent that others can read and tick.

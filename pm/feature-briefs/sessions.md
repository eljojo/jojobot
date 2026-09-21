# Sessions — one mortal run of a bot

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**A session is one run of one bot**, and it is the key that bot works through. It groups what the bot did, and it is how jojobot knows who is asking.

## The handle

**The handle is the only address, and it rides every verb — reads included.** That is what tells jojobot which bot is calling, on every single call.

**It is server-minted, four characters, and opaque.** It is never named after what the session is about, because a handle that describes its contents becomes a thing people read instead of an address.

**It is persisted outside the process**, so a session outlives the run that made it. Bot to session to receipts to journal is a chain somebody can follow, and that traceability is the reason.

**Two verbs take it optionally** — the ones a caller reaches before it has an identity. Optional means implemented: the handle is used and attributed exactly as any other read, because an argument accepted and dropped is a lie about work not done.

## Bound for life

**A session belongs to one bot, permanently.** Naming another bot's session is refused.

**That binding is also the check a session can run on itself.** A handle resolves under exactly one bot, so asking whether a handle is yours answers whether you are who you think you are.

## The lifecycle, and its one asymmetry

* **Wrapping happens from inside, by the bot, on the operator's instruction.** Starting a session never wraps another — **but it does ABANDON that bot's other live runs.** One worker runs one line at a time, and a run nobody is in should not go on looking live at the next boot. **Abandoning is the safe half of that: it walks back, and wrapping does not.**
* **A wrap folds the session's unfinished focus into its closing story, as ONE entry** — both things said, in one place, rather than a closing note and a dangling focus line that read as two events.
* **Terminal is asymmetric: a wrapped session never reopens.** Only an abandoned one walks back to active.
* **Abandoned means it was not wrapped up** — it is not a failure state, and resuming the most recent one always works unless it is stale.
* **A run that has gone quiet is swept to abandoned, and the sweep decides in the CALLER'S day rather than on a clock.** A run acting out a stretch of time says which day it is in, and without that every one of its own sittings would still look like it is working — so each later boot would meet a resume offer for a run that is long over.

## What a session carries

* **Its current focus is truth, not chronology.** What it is working on is revised freely as the work moves, and becomes an event only once something has happened.
* **Its journal is the events on its bot, each tagged with the session that did it.** There is no second journal mechanism.
* **What it has already been shown** rides with the session rather than the connection, so a resumed session inherits what its predecessor saw and is served the difference rather than the whole thing again.

## One bot, one worker

**Two workers are two bots, not two sessions of one bot.** Where two sessions of one bot do run, they share that bot's mailbox and sort it out between themselves — something a session lives with, not something the software resolves.

## What it is not

**Not a conversation.** A session is the unit of work and attribution; what was said inside it is not what jojobot keeps.

**Not something the tool ends.** A session ends when its owner says so.
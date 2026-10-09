# Sessions — one mortal run of a bot

> **jojobot** · kind: feature-brief · boot: on-demand

**A session is one run of one bot**, and it is the key that bot works through. It groups what the bot did, and it is how jojobot knows who is asking.

## The handle

**The handle is the only address, and it rides every verb — reads included.** That is what tells jojobot which bot is calling, on every single call.

**It is server-minted, four characters, and opaque.** It is never named after what the session is about, because a handle that describes its contents becomes a thing people read instead of an address.

**It is persisted outside the process**, so a session outlives the run that made it. Bot to session to receipts to journal is a chain somebody can follow, and that traceability is the reason.

**Four verbs require it** — journal, amend_journal, wrap_session and post_message, whose whole act is saying who did something. **Every other verb takes it optionally**, because a reader or a poster that never booted is legitimate. Optional means implemented: a handle that is carried is used and attributed exactly as on any other call, and a dead one is refused before the write rather than dropped, because an argument accepted and dropped is a lie about work not done.

## Bound for life

**A session belongs to one bot, permanently.** Naming another bot's session is refused.

**That binding is also the check a session can run on itself.** A handle resolves under exactly one bot, so asking whether a handle is yours answers whether you are who you think you are.

## The lifecycle, and its one asymmetry

* **Wrapping happens from inside, by the bot itself, whoever asked for it** — the operator, or a watcher asking a live session to wrap. Starting a session never wraps another, and never closes a live one: two sessions of one bot may run at once and share its box, so a new run sweeps only the runs that have gone quiet. **Abandoning a quiet run is the safe half: it walks back, and wrapping does not.**
* **A wrap is ONE closing entry: the story verbatim, and the unfinished focus beside it as that entry's own field.** Both are said in one place, so they never read as two events, and neither is rewritten into the other.
* **Terminal is asymmetric: a wrapped session never reopens.** Only an abandoned one walks back to active.
* **A wrap hands back a one-time code for a last change.** Resuming with it lets the wrapped run write again while it stays wrapped, until it wraps a second time or a newer run of the bot starts. Its first story stands, and the second wrap adds a closing entry beside it.
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
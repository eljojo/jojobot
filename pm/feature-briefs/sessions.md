# Sessions — one mortal run of a bot

> **jojobot** · kind: feature-brief · boot: on-demand

**A session is one run of one bot**, and it is the key that bot works through. It groups what the bot did, and it is how jojobot knows who is asking.

## The handle

**The handle is the only address, and it rides every verb — reads included.** That is what tells jojobot which bot is calling, on every single call.

**It is server-minted and opaque, in two halves.** The key (`sk-` and 26 drawn characters) is the run's secret and its only credential: it rides every call and is told to nobody. The run's name (six characters) is what records carry, and it opens nothing. Neither is named after what the session is about, because a handle that describes its contents becomes a thing people read instead of an address.

**It is persisted outside the process**, so a session outlives the run that made it. Bot to session to receipts to journal is a chain somebody can follow, and that traceability is the reason.

**Every verb that writes requires it**, because a write is a claim about who did something: a memory write, a post, the session's own journal and wrap. **A read takes it optionally**, because a reader that never booted is legitimate, except a read of mail, which needs it to know whose box. Optional means implemented: a handle that is carried is used and attributed exactly as on any other call, and a dead one is refused before the write rather than dropped, because an argument accepted and dropped is a lie about work not done.

## Bound for life

**A session belongs to one bot, permanently.** Naming another bot's session is refused.

**That binding is also the check a session can run on itself.** A handle resolves under exactly one bot, so asking whether a handle is yours answers whether you are who you think you are.

## The lifecycle, and its one asymmetry

* **Wrapping happens from inside, by the bot itself, whoever asked for it** — the operator, or a manager asking a live session to wrap. Starting a session never wraps another. A new run sweeps the runs that have gone quiet, and displaces a live one when it claims a role that run holds or takes over the bot's box: that run is told by whom, and stops renewing the role and taking mail. **Abandoning a run is the safe half: it walks back, and wrapping does not.**
* **A wrap is ONE closing entry: the story verbatim, and the unfinished focus beside it as that entry's own field.** Both are said in one place, so they never read as two events, and neither is rewritten into the other.
* **Terminal is asymmetric: a wrapped session is never active again.** Only an abandoned one walks back to active.
* **A wrap hands back a code that opens a window of two hours.** Resumed with it, the wrapped run works as a normal session — journal, mail, any write, its closing story rewritten in place — and need not wrap again. The window closes two hours after the wrap, at a second wrap, or when a newer run of the bot starts, and after that nothing writes. Two hours covers the correction made while the work is fresh; editing yesterday's story is what the window exists to refuse.
* **Abandoned means it was not wrapped up** — it is not a failure state, and resuming the most recent one always works unless it is stale.
* **A run that has gone quiet is swept to abandoned, and the sweep decides in the CALLER'S day rather than on a clock.** A run acting out a stretch of time says which day it is in, and without that every one of its own sittings would still look like it is working — so each later boot would meet a resume offer for a run that is long over.

## What a session carries

* **Its current focus is truth, not chronology.** What it is working on is revised freely as the work moves, and becomes an event only once something has happened.
* **Its journal is the events on its bot, each tagged with the session that did it.** There is no second journal mechanism.
* **What it has already been shown** rides with the session rather than the connection, so a resumed session inherits what its predecessor saw and is served the difference rather than the whole thing again.
* **Whether it is plain or enhanced.** A session started through the jojobot stack is stamped enhanced when the runtime attests which agent it is and its chores are done by software (*Lines — a bot asked to run, and the software that keeps it running*). The stamp says which features are present, and when it drops to plain, why. A plain session can do everything except be woken.

## One bot, one worker

**Two workers are two bots, not two sessions of one bot.** Where two sessions of one bot do run, **one of them reads the bot's box: the newest.** The older run is fenced from taking delivery and from closing mail, and told so, because two readers of one box each owe what the other took. Addressing one session of a bot directly, by its public run name, is a later step; until then each use case is its own bot.

## What it is not

**Not a conversation.** A session is the unit of work and attribution; what was said inside it is not what jojobot keeps.

**Not something the tool ends.** A session ends when its owner says so.
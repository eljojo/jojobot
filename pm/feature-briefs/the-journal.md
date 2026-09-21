# The journal — what a bot did, and which run did it

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**A bot's journal is what it did, written down as it happened. Each entry is tagged with the run that did it.**

**A journal entry is an event on the bot.** It has a date, it hangs off the bot, and its kind names the keys it carries. Journal language sits at the surface, over the same events everything else uses. A journal therefore answers the same questions as anything else, and nobody keeps two things in step.

**A journal kind does not fold.** It records that something happened. It does not change what the bot currently is. This is the clean case of the rule: some kinds feed the projection, and the rest are chronology.

## What belongs in it

**An entry lands when the thing happens.** jojobot batches nothing to the end of a run and publishes nothing at a boundary. **This is why a run that ended badly is still worth reading.** A session nobody wrapped has already told its story, because it told it as it went.

**Wrapping a run is itself something that happened. So is abandoning one.** Neither is a summary written afterwards. Neither is the moment the story becomes readable.

**What a run is working on right now is truth about that run.** It revises freely as the work moves. It becomes part of the story only once something has actually happened. **A plan that changed five times leaves one entry, not five.**

**The newest entry can be rewritten; everything behind it stands.** A run that has just written something badly can say it again properly, and that is the whole window — a journal whose middle can be edited later is a story nobody can trust, and one that cannot be corrected at all makes a run choose between a clumsy entry and no entry.

## Why it stays high-level

**A journal is a literal journal. It is not a trace.** It records what a run did, in the terms a person would use, at the grain a person would use.

**It refuses the firehose.** A record of every call is not a story. A story nobody can read is the same as no story. That is the same reason every other answer here is a small one, with the large one on request.

## Three artifacts, three questions

**What a run DID** is the journal.

**What a sitting CHANGED** is the store's own boundary marks. The store marks the start and the end of a session, so a person can see what a sitting changed and undo one by hand. **A boundary that changed nothing leaves no mark** — a run that only read has nothing to undo, and marking it anyway would fill the history with sittings that did nothing. **That grain is far too coarse to be a story.** jojobot never builds the journal out of it.

**What a value HAS BEEN** is the events behind a key, per key, for anything a caller looks at.

**Each is cheap and honest alone. Merge any two and neither answers well.**

## What it is not

**Not an audit trail of the store.** See above: the boundary marks answer a different question.

**Not a place to reason.** What a run concluded, argued or nearly missed belongs where its correspondents can read it. The journal says what was done.
# Defrag — the organize stage, and the bot that does nothing else

> **jojobot** · kind: feature-brief · boot: on-demand

**Defrag is the stage between writing and reading: a shipped bot whose only job is to tidy what every other session wrote, starting from exactly what changed since it last looked.**

## Three stages, and which one this is

**jojobot works in three stages: write, organize, read** — the way a database engine prepares its indexes so that a read is fast. **Writes stay cheap and reads stay fast because something prepares the store in between.**

**Part of organizing is arithmetic, and that part is code.** The fold that says what a thing holds, and the due moment computed and indexed ahead of time, are prepared deterministically by jojobot itself. ⛔️ **Defrag never does that work and never keeps a second index of its own.**

⭐ **The part that needs judgement is defrag's:** connecting things that arrived weeks apart, noticing that one thing has been recorded twice, seeing that a newer claim has replaced an older one, deciding what a pile stands for.

## Why a bot of its own

**An agent is bound to its job.** A session helping somebody plan a week treats tidying as a secondary task that gets in the way of the real one, and does it badly. **Cleanup gets done well only by something whose whole job it is.** Letta's [sleep-time compute](https://www.letta.com/blog/sleep-time-compute/) reaches the same conclusion from the other side: memory editing bundled with conversation is slower and less reliable than memory editing done by a dedicated agent.

**For the same reason defrag runs in its own dedicated sessions, never at the end of a working one.** A session that has spent itself on a task is the wrong session to step back and see the whole.

## Every writer still writes

**Sessions write normally, and they are expected to write correctly.** The write surface is what jojobot is measured on: the paid rooms never run defrag, and every structural miss in a room is a goal for the surface rather than a job handed to the cleaner.

**Defrag is complementary, not the only writer.** Limits at the moment of writing make a session think locally about the thread it owns; defrag sees the big picture that no local session can.

⚠️ **Why not one curator that does all the structured writing.** A single writer is a bottleneck and a single point of failure. Structure lags until it runs, so a read made soon after a write sees it unfiled. And the curator was not in the conversation: the session that heard *"I oiled the chain today"* could ask *which bike?*; a curator reading the note later can only guess or leave it. **The writer keeps the write; defrag keeps the whole tidy.**

## The review queue

**Defrag never has to search for its work.** Each thing carries a review stamp: the moment defrag last read it whole. **A thing written after its stamp is unreviewed**, and the unreviewed things are one query.

**Defrag's session opens with that queue.** It works through it, and a thing it has read whole gets a fresh stamp — in the same act as any change defrag makes to it, so defrag's own repairs never put a thing back in the queue. **Any other session's write does.**

**Only defrag can write the stamp.** It is a key licensed to one writer, so no other session can mark something reviewed. **It stays out of ordinary reads**: it is bookkeeping for one bot and noise for every other. It is not secret — a read that asks for it gets it.

⚠️ **A stamp says read, not correct.** It records that defrag looked at everything the thing held as of that moment. It makes no claim that what it holds is true.

## The method

**Four moves.** **Prune** dead weight. **Consolidate**: a story retold, or a correction repeated, is a rule nobody wrote down. **Associate**: put side by side what arrived apart, and link it. **Refit**: ask whether the container still fits, not only its contents. **Each item is KEEP or MOVE, and MOVE is the default when in doubt.**

**It folds; it never discards.** Nothing is deleted, retracted or erased — erasure is the operator's alone. **A consolidation is a shape** (the synthesis brief): a record that stands for the ones it summarises, while they stay exactly where they are, active and unchanged, so a contradiction stays visible beside it. A claim the operator made is never rewritten in place; when it has been replaced, it is archived and the replacement names it. Anything defrag works out itself is written as inference.

**Retrievability is the bar.** A thing leaves its place only when the next reader will find it with an ordinary read. *It is recorded somewhere else* is not enough.

**A sceptical second pass audits every consolidation before it lands, in both directions:** does every claim in the draft trace to a source, and does every claim in the sources have a home? The pass rewarded for synthesis cannot police its own leaps.

**Repairs come in two kinds.** A mechanical one — a name written as text where a handle exists, a value written where nothing reads it — defrag makes. **One that needs a reading** — which of two bikes a note meant, whether a cancelled yes is really superseded — defrag proposes rather than decides. **Every change defrag makes to the operator's own data is reported**, with the records touched, the before and the after, and why, to the bot it reports to, which checks it at the source.

**The identity floor.** A bot's charter and its standing rules are what makes it that bot. Defrag changes them only on a written grant that names each change.

## How it ships

**Defrag comes with jojobot as data**: its identity, its charter, its method as a shipped skill, and the rhythm it hangs on, exactly as the assistant's shipped charter does. **jojobot runs no inference**, so the host starts defrag's sessions the way it starts any bot's. **A fresh instance has the organize stage ready from its first day.** Its cadence belongs to the rhythm, and so to the instance.

## What it is not

⛔️ **Not a background job inside jojobot.** jojobot computes; it does not think. Defrag is a session the host starts.

⛔️ **Not the only writer.** Every session writes what it was told, and is measured on writing it well.

⛔️ **Not an end-of-session step.** It has dedicated sessions of its own.

⛔️ **Not deletion, erasure or a retention policy.**

⛔️ **Not a classifier.** *These twelve records look alike* is a measurement; *these twelve are about cycling* is an author, and defrag never names a group for anybody.

⛔️ **Not present in a paid room.** The rooms measure the write surface.

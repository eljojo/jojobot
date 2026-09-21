# Carried state — the few things a bot holds between runs, and why it is always full

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-11

**A bot carries a small, fixed number of thoughts from one run to the next. A new one arriving into a full room forces a choice, and being made to choose is the point of the feature.**

## Capacity is the feature

🚨 **CONTEXT IS ALWAYS LIMITED, SO PRIORITIZATION HAS TO HAPPEN SOMEWHERE.** **A state a bot can append to is a state nobody re-reads: it grows until it is too expensive to load and too undifferentiated to act on.** ⭐ **The limit is not a storage constraint. It exists to make a bot decide, every time it wants to hold something new, what has stopped being worth holding.**

**Capacity belongs to the bot rather than to the software** — a worker carrying one slice needs almost nothing across runs; a bot holding somebody's life needs more. **One mechanism answers both.**

## The room turns over, or it is dead

🚨 **A FULL ROOM OF STALE THOUGHTS LOOKS EXACTLY LIKE A FULL ROOM OF LIVE ONES.** ⛔️ **That is the failure this feature can produce and the one it cannot see: same count, no growth, nothing that announces itself.**

⭐ **So the measure is TURNOVER, not size** — how old the oldest thought is, and how many left this period. **Every thought records when it was last touched, which is what makes that answerable.**

**And turnover cannot depend on a ritual.** **Each run re-touches what it keeps; anything left untouched long enough ages toward the archive on its own.** ⭐ **The cap is a guard on that loop and never the mechanism — a cap without the loop is a static room.**

## A thought

**One complete idea, short, and mostly a pointer** — it says what is live and why it matters now. **The substance lives on whatever it points at.**

🚨 **A THOUGHT IS KEPT ONLY IF WHAT IT STANDS ON CAN BE FOUND AGAIN.** ⛔️ **Not merely *recorded somewhere* — that is unenforceable and it is the rule the prose version already failed.** ⭐ **The bar is retrieval: a thought may leave only when the path back to its substance is one somebody would actually take.**

**What makes that enforceable rather than hopeful: a thought must carry an address, and its body is capped short enough that restating what it points at will not fit.**

⭐ **Written that way a thought reads like this:** *Milhouse is moving to Shelbyville in the spring →* `thread:milhouse-moves`. **Everything about the move is on the thread; the thought only says it is live.**

## Getting in is a decision too

⛔️ **A room that always admits is a room where a busy week evicts a year.** **Thirty things touched once each, each one displacing something long-standing, and the slow important concerns go first because they are the least recently touched.**

⭐ **So admission is refusable.** **A new thought earns its place against what it would displace, or it does not get in.** **The bot chooses; the software only refuses to pretend the room is bigger than it is.**

## Threads

**A thread is an ongoing line in a life — a situation rather than a task.** ⭐ **A card ends when it is done; a thread ends when it stops being true.**

**A thread is capped too.** 🚨 **A bounded room pointing into an unbounded pile is the same failure one level down, and it is harder to see because the room above still reads clean.**

**When a thread is full, what shrinks it is RELOCATION and never compression.** ⛔️ **And relocation does not MOVE anything: what a record is about is part of what it says, so carrying it to another subject changes it.** ⭐ **Detail is relocated by making a new claim on the thing it is really about, and archiving the old one with a pointer at its replacement.** **Rewriting the same content denser is how a record becomes generic, and that is the failure this whole design is built against.**

## Leaving, and going back

⛔️ **A thought that leaves is ARCHIVED, never deleted.** ⭐ **What decays is how much a thing is in the way — never whether it happened.**

**Going back means two different reads, and only one of them is cheap.** **What a thought SAID before it was rewritten is its own history, which every record here already carries.** **What the room HELD on a given day is a different question, and it needs the archive to be readable by date.**

## Rewriting keeps its receipts

**A thought is revised in place as the thing it names moves.** ⚠️ **Every revision is a re-derivation, and a run of them turns something the operator said into something a bot worked out, with nothing marking the difference.** **So revisions carry their history and provenance through the machinery everything else here already uses.**

## The floor

🚨 **SOME THINGS ARE NOT SUBJECT TO CAPACITY, AND WHAT THEY HAVE IN COMMON IS THAT LOSING THEM CHANGES WHO THE BOT IS.** **The standing constraints on its own conduct — what never to re-offer, what never to chase, how to speak about a particular subject — are not thoughts about a situation. They are the bot.**

⛔️ **Efficiency that prunes those has not tidied the room; it has produced a different assistant.** **They live with the bot's own rules, outside the room, and no capacity pressure reaches them.**

## What this deliberately does not hold

**A NAG** — something owed that must resurface until done or declined. **It is the longest-sitting, least-changing thing in the room, so any capacity rule targets it first.** ⭐ **It belongs with the recurring loops, computed from what is owed.**

**A PROJECTION** of anything the task layer or the calendar already holds. **A copy in state goes stale silently.** ⭐ **It belongs in a question asked by name and answered fresh.**

## Prior art, and the debt is real

⭐ **Letta has built this and published what broke.** **Their** [**Context Constitution**](https://github.com/letta-ai/context-constitution) **states the same premise independently — *"the context window is a precious resource that must be actively managed"* — and two of its principles are sharper than anything derived here.**

**The retrieval bar above is theirs:** *"context should only be removed from the in-context memory and placed externally if the agent is confident that it has laid the groundwork to retrieve the context reliably when necessary in the future."*

**And the floor is theirs:** *"agents should be careful to avoid degrading their identity and sense-of-self through over-aggressive pruning. Efficiency should not come at the cost of losing the agent's identity, as this breaks the continuity."*

⚠️ **AND THE SOURCE TIERS ARE MARKED, because this design is about exactly that.** **The two passages above are verbatim from the Constitution, read at the source.** **What follows is weaker and is labelled so.**

**FROM A RESEARCH NOTE OF THEIRS, stated as motivating observations rather than as measurements:** memories going generic and lossy after repeated refinement, and memories coming out too specific to generalize. ⭐ **That is why compaction here relocates rather than compresses** — but it is their reasoning, not their data.

**FROM THEIR PUBLISHED MEMORY EVALUATION, reported rather than quoted:** weaker models answer feedback by logging it instead of rewriting, and memory drifts toward stale entries nobody removes. ⚠️ **Both reached this build through a summary of that page rather than the page itself, so they are its findings as reported and not its wording.**

⛔️ **AND ONE CLAIM IS NOT THEIRS AT ALL.** **That doing the task and learning from it pull against each other appears in a research note about training incentives.** ⭐ **Using it as the reason folding cannot happen inline is a reading made here** — the Constitution says nothing about inline versus boundary timing, and the ruling that folding happens at the edges is the operator's alone.

🚨 **AND THE DIVERGENCE, stated as a READING rather than as their claim.** **Their** [**context repositories**](https://www.letta.com/blog/context-repositories) **replace memory blocks with a versioned filesystem, progressive disclosure, and a defragmentation subagent.** ⛔️ **That post does not say the blocks were fixed-size, does not say a cap was abandoned, and gives no reason.** ⭐ **So "they moved away from a hard cap" is a reading made here from the direction of travel, not something they wrote.**

**This design bets the other way, on the ground that a discipline needing a job to run is a discipline that stops running.** ⚠️ **The operator was shown the divergence and kept the caps. It is a bet, it is named here as one, and turnover is how it would be shown wrong.**

## What it is not

⛔️ **Not a conversation log, and not a second memory** — nothing is known here that is not known somewhere else.

⛔️ **Not deletion and not a retention policy.** Nothing expires and nothing is removed.

⛔️ **Not a summary produced on the way out.** A read that shortens what it returns leaves nothing behind.

⛔️ **Not unlimited, and not configurable into being unlimited.** **A capacity a bot raises when it feels crowded is not a capacity.**
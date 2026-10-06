# Rules — what a bot is held to, and how the right one reaches it at the right moment

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-10-05

**A rule is a standing claim about how a bot, or the build, must behave. The feature is getting each rule in front of a mind at the moment it bears on what that mind is doing, and nothing else in front of it.**

## The problem fails both ways

**Too little, and a session misses a rule that applied — and usually fills the gap by inventing one.** **Too much, and a session skims, which misses the rule just as surely.** ⛔️ One mechanism has to answer both, so neither "load everything" nor "load less" is a design.

⭐ **A miss outweighs noise by a wide margin.** The agent that missed a rule cannot see the miss; the operator can, and catching it is work that lands on them. **So given the choice, hand a session something it did not need rather than let one rule it did need fail to arrive.**

## A boot cannot know the task

**What a session will be asked to do is not known when it boots, so the right rule cannot be pushed at boot.** It has to be *reachable* at the moment of need, from where the session is standing.

**That splits a bot's rules in two.**

- **The floor rides the boot.** Some rules bind on every turn whatever the task — how this bot speaks, what it never does, how it reads its own state. **So does any rule that bears on everything**: drawn as an edge to every thread it is noise on every walk, and drawn to none it is invisible. **The boot carries the floor in seats, and the number of seats is a ceiling on the bot like any other** (see *Containers — a bounded thing refuses what would overfill it*): sized to the bot, set by somebody other than the bot, and bounded overall by what one boot answer can carry. A worker on one slice needs a handful; a bot holding somebody's life needs far more.
- **Everything else is fetched when it applies.** A rule that binds at one moment rides on whatever is read at that moment: the skill fetched for that job, the refusal a verb gives on that path, the thread for the feature being worked. ⭐ **The place a rule lives is chosen by asking when it bites.**

## The seats are chosen, never accumulated

**Marking a rule to ride the boot is an admission decision, the same as a thought entering a full room.** When the seats are full, a new mark displaces one, and the writer is told which at the moment of writing.

⛔️ **Seats filled by whoever marked last are seats nobody chose.** Five rules picked on purpose decay, one write at a time, into the five most recent — and each write looked harmless. **A mark is therefore a claim that this rule binds on every turn, made against the rules it would push out.** A rule that only binds sometimes belongs where that time is, not in a seat.

## The log is a tree of threads

**The operator's rulings live in jojobot, one claim per ruling, homed on a thread per subject.** The subjects are the features — one thread per feature brief — plus the few that are not features: the terms the project operates under, what jojobot is, the bright lines, and how the build is run. Threads nest, because a handle is a path.

**A thread is bounded** (see *Containers — a bounded thing refuses what would overfill it*). **That bound is what makes the tree.** A flat page has no pressure to split, so it grows until it cannot be read whole and can only be searched. A bounded thread refuses the ruling that would overfill it, and the writer splits the thread or relocates something. ⭐ **Nobody designs the shape; it falls out of the limit.**

**The content is a graph, and the tree is only its skeleton.** One ruling about capacity bears on budgets, on carried state, on the boot and on bots at once. Filed in one place and linked nowhere, every other branch lies by omission. **So a ruling that bears on several threads draws edges at each of them.**

## The walk

**To learn what was ruled about a feature: read its thread whole, then everything that points at it, one hop.** ⭐ **Reading whole is affordable by construction, because the bound already capped it.** That is all of what is needed and nothing more.

**Search is not the walk.** A search finds only what the reader already has words for, and a ruling is written in the operator's vocabulary, not the reader's. ⛔️ **An empty search is never evidence that nothing was ruled.**

🚨 **The walk must be able to say it failed.** *Nothing has been ruled about this* and *nothing was found* are different answers and must read differently. A clean answer on the wrong question is worse than no answer, because it discharges the obligation to look.

## The edges are the expensive part

**Which rulings bear on which feature cannot be derived from the feature.** A ruling that decides a feature can share no word with it. ⭐ **So every edge is drawn once, by somebody who has read both ends and judged that the ruling would change what gets built** — not by whether it mentions the feature.

**An edge points where the rule FIRES, not only at what it is about.** A rule about smoky air bites when somebody plans Homer's Saturday, not only when somebody asks about the air; an edge to the subject alone is never walked at the moment that matters.

**Knowing which rulings bear on which feature is the same knowledge as knowing how to file them.** The key and the tree are one artifact, built in one pass.

## A ruling's life

**A hedge is a ruling whose standing is open** — it expires on a condition, and only the operator retires it. A list of live hedges is then a question asked of the store, never a hand-kept header that can silently stop being the answer.

**A ruling that is replaced is archived, pointing at what replaced it.** It stays reachable for *why*, and is never shown as standing.

**A ruling says what was decided and why. How a thing works is its brief's.** Where the two disagree about how, the brief wins. A ruling that explains a mechanism has moved into the wrong document.

## What it is not

**Not thoughts.** A thought is what is live now and is meant to turn over. A bot's rules are who it is; capacity pressure on a room never reaches them.

**Not a skill.** A skill is a procedure for a kind of job. A rule may ride on a skill because that is when it bites, but it is not one.

**Not everything at boot.** Seats are sized to what binds every turn; a seat spent on a rule that only binds sometimes is the "load more" half of the problem.

**Not an enumeration.** A ruling is a design principle. Building it as a list of today's cases makes it fire only on today's cases.

**Not a page someone has to remember to read.** A rule that is reached only by somebody deciding to go and look is reached by nobody at the moment it matters.

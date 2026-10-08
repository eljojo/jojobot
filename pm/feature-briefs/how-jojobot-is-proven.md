# How jojobot is proven — three layers, each answering what the others cannot

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-04

**Three layers. None of them substitutes for another.** A unit test says a feature works. A user story says an agent can *reach* the feature through the surface it actually holds. The escape room says a real model, meeting that surface cold, *gets there*.

## User stories

**Every feature appears in a story. A feature that appears in none is a finding.**

**A human reads a story, and what is possible must be evident from the reading.** A story is not a machine fixture wearing a narrative. The reading is the deliverable.

**A call with no assertion is coverage theatre.** Mark a beat nobody can write as a gap. Never work around it.

**A call that goes round the surface is worse than one with no assertion, because it looks like proof.** A story that reaches its result through a helper, a shortcut, or a path no real agent holds proves the code works and proves nothing about the product. **Reach through the surface an agent actually has.**

**And the scenario has to be one somebody would want the answer to.** A story invented to exercise a fixture is the failure, and the tell is a question nobody would ask. ⭐ **The names are fictional and the shape of the question must be real** — *which of this person's pets has had no vet visit since the spring* is a real shape wearing invented names; *which thing carries the key named alpha* is a fixture wearing a sentence. **Both keep the names fictional. Only one is worth reading.**

**The story language calls any verb with any parameter, and nobody writes an adapter first.** Named methods stay where they earn their place. Everything else goes through the generic call. A surface that needs an adapter per verb makes the next verb unreachable until somebody remembers.

## Proving a refusal

**A refusal is a capability, so it needs a story like any other.** jojobot refuses a value that does not hold what its key declared. It refuses a write that would remove a key a kind names. Neither is proven by a test that only writes things that work.

**A refusal test passes against a build that refuses everything.** So pair it. In the same read, assert that the bad write fails **and** that the good write still succeeds.

**Assert what the refusal says.** It names the key and what it wanted, because a bare rejection on a write path an agent uses constantly is worse than the defect.

**Assert that the thing is unchanged.** A refusal that half-applied is a worse state than either outcome.

**Assert the boundary.** The same write on a thing with strictness off succeeds. Otherwise the story proves a gate rather than a floor.

**Watch it fail first**, against the build that does not yet refuse.

## The escape room

**A playbook runs through a real model against a clean instance. Then somebody checks the assertions.**

**A room is one goal a person would actually have, and the agent is never told it is being tested.** The entry is a single line that names no verb and no place to look. What the room measures is whether the surface teaches an agent to reach the goal, so anything the entry explains is a thing the room has stopped measuring.

**A room is two phases, and the second one is cold.** Inside one session an agent answers any question from its own context — it just wrote the thing, so it remembers it, and no structure is required of the store. Only a session with no memory is forced to make the store the medium. So the terminal question lives in a fresh phase, where a session that took a shortcut in the first meets it in the one place nothing can rescue it.

**The intermediates depend on each other, and that is deliberate.** An agent that never finds the mailbox reaches nothing downstream, which is exactly the failure worth catching. A room whose steps merely run in order measures compliance.

**The mechanism has a range, and there are three answers that carry a terminal question across a cold boundary.** ⛔️ **Recall is not one of them: a cold session reads a record as easily as it reads a key, so *how far did I ride it* sits in prose exactly as readably. A goal that only needs recall tests reading comprehension.**

* **A computation jojobot does.** *Which of these has gone quiet as of a named day* is a cadence, an anchor and a day. No sentence holds it.
* **A vocabulary the store carries and the brief said once.** A word the operator uses, agreed in a message the cold session never reads — so a declaration is the only thing that carries it across the boundary. Without one, two words look identical on a record.
* **State the store owns that no session could have written down.** Where a message got to; what somebody else's box did with it. **The session that would have written it was not there when it changed.**

**Choose a goal whose terminal question is one of the three.**

**Hardness comes from the goal, never from the brief.** A room that is hard because it hides something measures puzzle-solving, and that is a worse instrument than the script it replaced.

* **A known starting state.** This is what makes assertion tractable. The check reads **the state the room is left in**, not the words the agent chose. A real run is non-deterministic, so a tight assertion on prose fails on wording and a loose one passes on nothing. **A paid run proves the agent READ, not only that the store ended up right:** the room plants an answer only the intended read yields, and the sitting must record it in the store, because an answer merely said leaves nothing to check.
* **Entry through the instance's own surface**, so the agent under test meets jojobot the way any agent does.
* **The shipped identity, with no charter installed.** The harness may furnish the room. It may not coach the occupant. **A run that falls short because the shipped charter falls short is a result, not a harness defect.** The batteries are the product, and this is the one test that measures them.
* **Each run builds and destroys its own instance.** It reaches nothing of the operator's.

**No rubric and no score.** A phase with no machine check reports that it has none. It does not count as passed. Whether jojobot did well is the operator's judgement, not a number.

🚨 **A ROOM MEASURES THE MODEL. A CONTRACT CASE MEASURES THE BUILD. The bars are not interchangeable and mixing them is the most expensive scoping error on this layer.**

⛔️ **So *this lock goes red when the capability is removed from the build* is a unit-test bar wearing a room's clothes.** **A room asserts over what the model LEFT in the store, and a capability whose output looks like its input leaves nothing different behind** — the only difference can be a refusal, and a refusal is not state a lock reads.

⚠️ **Ask a room for it anyway and the only way to satisfy it is a lock that rides on a MISPLAY** — a guard firing on a session that behaved badly. **That measures the model failing, not the build working, and it will pass on a well-played year for the wrong reason.**

⭐ **The right bar for a room is the one the rooms already have: a year written entirely in prose FAILS the locks, and a year that reached for the capability passes.** **That discriminates the thing a room is for — whether a session nobody told finds the capability — and it is the question no unit test can ask.**

**When a property needs proving rather than a behaviour, it belongs in the shared contract, where the fake and the real store both answer for it.**

## What counts as evidence

* **A red run is evidence only if you know why it was red.** A missing behaviour proves something. An earlier assertion blowing up proves nothing. An assertion malformed enough that it could never match proves nothing. All three look identical from outside.
* **A test nobody has watched fail against the unfixed code is not evidence.** For a race, that means watching it race.
* **Pair every negative with the positive it rests on.** *The wrong answer is not in the results* passes identically when the results are empty.
* **An assertion can pass for a reason unrelated to its claim.** It looks like coverage until somebody attacks it. **Three shapes are worth knowing by sight.**
  * It watches for **a name the thing will never have.** A guard waits for a verb. The capability ships as an argument on a verb that already exists. The guard stays green through the day it lands.
  * It reads a value that **also appears elsewhere in the same answer.** An ordering check over a whole page finds the current value in the summary above the list, and calls that the list.
  * It pins **a branch nothing can reach.** Hardcoding the value changes no test anywhere. **Here the honest move is to call the field unverifiable.** Do not write a test whose oracle is the thing under test.

  **One question finds all three: what would this print if the thing it measures were absent?** If that is indistinguishable from success, it is not measuring.
* **A green bar comes from a run whose exit code you read in the same command as its output.** A test result without its compile status is not a result.
* **A room can be perfectly discriminating and still be impossible, and the two look identical from the cheap suite.** If the arithmetic in a brief is wrong — a cadence that never falls due by the named day, an anchor a day out — the terminal question is unreachable by any session, every free case still passes, and the room fails every paid run while reading as a product defect. **So a room carries two different kinds of case, kept apart by name: the ones proving the checks discriminate, and one proving the room is SOLVABLE** — the terminal question asked through the surface after a first phase done properly, returning exactly the answer the room is built around. **They are different failures and only one of them looks like a bug in the product.**

## Doubles

**A double must be hostile where reality is.** A fake that stores bytes verbatim, in front of a real store that normalises them, keeps a whole suite green over a defect that reaches production.

**The lying goes both ways.** A faithful fake in front of a lossy adapter is invisible to every test that runs against the fake. **So a claim about storage belongs in the shared contract, which the fake and the real store both answer for.** A specification only one of them runs catches only one direction of the lie.

**Read-back proves an adapter is self-consistent. It does not prove the adapter stored what somebody gave it.**

## What it is not

**The cheap layers are not a gate somebody performs at a boundary.** Unit tests and stories run freely and often, and a cheap check that goes unrun is the failure they exist to prevent.

**The third layer is different, and the difference is its cost.** A run through a real model costs money, so it is triggered by the deploy boundary rather than by a cadence — a schedule for something expensive is either ignored or resented, while the boundary already exists and already carries the judgement the run feeds. **It never replaces the reading review**, which asks whether the code matches what was asked for and whether each piece earns its place. **This asks the one thing reading cannot: whether a model meeting the surface as shipped gets anywhere with it.**

⭐ **It is also how the software is developed, and not only the last gate before a deploy.** A run finds gaps in BOTH directions — in the product, and in the suite that was supposed to catch them — so a round improves both. **The bar it is read against is real delivered value to a person, never a check that passed.**

**Not a verdict.** They produce evidence. Reading it is a person's job.
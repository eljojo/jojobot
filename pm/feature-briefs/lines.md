# Lines — a bot that is running, and the runtime that runs it

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-10

**A bot is somebody jojobot knows. A line is that bot, running.** jojobot starts a line, sees whether one is live, and stops it. A runtime somewhere owns the processes — **and no agent using jojobot ever learns that runtime exists.**

## The abstraction IS the feature

**jojobot's job is to spend an agent's attention on the work rather than on the plumbing.** ⭐ **So the measure of this doorway is not what it can reach. It is how little a caller has to know.**

**The runtime underneath speaks in agents, workspaces, providers, model strings, permission modes and session handles.** ⛔️ **None of that crosses.** **A caller says: start a line for this bot. It gets back a line.**

🚨 **THE TEST, AND IT DECIDES EVERY VERB: could a bot use this without knowing the name of the thing underneath?** ⛔️ **If a parameter only makes sense to somebody who has read the runtime's documentation, it does not belong on the surface.**

**This is also why the doorway is not a mirror of what it fronts.** **A verb taken because it appeared in the runtime's list has added a hop and pushed a foreign vocabulary through it.**

## Identity arrives in the FIRST THING SAID, not in configuration

🚨 **A runtime does not have to let anyone install a persona, and this one does not.** **Creation takes a title, a provider, labels, a few settings and a first task. There is no slot for standing instructions and none for handing a session its own connections.**

⭐ **THE FIRST TASK IS THE SEAM, AND IT IS ENOUGH.** **jojobot composes it from what only jojobot knows — which bot this is, what its charter says, which mailbox is already addressed to it, and what it is being started for.** **A caller says one thing; jojobot assembles four.**

⚠️ **THE HONEST LIMIT THAT FOLLOWS: a first task is a MESSAGE, not a standing rule.** ⛔️ **A line cannot be made to obey its charter by construction — it can only be told, once, well.** **A doorway that pretended otherwise would be promising something the layer beneath it cannot deliver.**

**And a line reaches jojobot only if the machine it runs on already knows jojobot.** ⭐ **That is the operator's setup, not something this doorway installs** — which is the same line drawn at the end of this page, arriving earlier than expected.

## A line is a record, and liveness is a reading

**The line is jojobot's own object and it is stored:** which bot it runs, when it started, what it was started for, and where it works. ⭐ **The runtime can never hold that, because it never knew the bot.**

**What jojobot does NOT store is whether the line is still alive.** 🚨 **That is read through, live, every time it is asked.** ⛔️ **A stored liveness flag is a second authority that goes wrong silently.**

⭐ **The split is the whole design. What a line IS belongs to jojobot. Whether it is WORKING belongs to the runtime.** **An answer carrying both says when the second was checked.**

⚠️ **And *working* is narrower than it sounds.** **A runtime distinguishes a session that is thinking from one that is merely open, and a doorway that folds those together answers the wrong question.**

## The question nothing can answer today

**Is anybody working as** `**bot:dev**` **right now?**

**A board column asserts it because a coordinator moved a card, and goes on asserting it long after the session that earned it has died.** **A stopped session and a thinking session are identical from outside.**

⭐ **This turns that belief into a read**, and it is why the feature is worth building past the convenience of starting things.

## Stopping is cheaper than it sounds, and the danger is one layer down

⚠️ **Stopping a line does not destroy it.** **A runtime that keeps its sessions on disk can close one and resume it later; the record survives and the working files were never the runtime's to touch.** ⛔️ **So a doorway must not dress a stop up as an amputation.**

🚨 **WHAT IS ACTUALLY LOST IS THE LIVE TURN** — whatever the line was in the middle of thinking. **And what sits UNCOMMITTED in its working tree stays uncommitted, invisible to anybody who was not watching.**

⭐ **So a stop reports what is unsaved before it acts.** **Not because the stop would destroy it, but because the person stopping is the last one positioned to notice it.** **What they do with the answer is theirs; being unable to act without seeing it is the point.**

## What it is not

**Not a scheduler.** The runtime can run things on a cadence and so can other layers. **Taking that here would settle where recurring work lives as a side effect of connecting a doorway**, which is its own decision.

**Not a terminal, a browser or a voice.** **A doorway earns each verb by having a domain reason for it.**

**Not a second charter store.** The charter lives where charters live. **Composing it into a first task is a read, never a copy that drifts.**

**Not a way to make more work happen.** **Two lines running is not two lines producing**, and a doorway that makes starting frictionless makes starting-without-scoping frictionless too. **The card comes first, exactly as when a person starts one by hand.**

**Not ownership of the machine.** Where the runtime runs, what it may reach, whether it exists at all, and what credential reaches it stay the operator's. ⚠️ **A runtime whose door opens to ONE shared secret gives whoever holds it the whole daemon, not merely the verbs this feature needs** — so what jojobot is trusted with is the operator's decision before it is a design.
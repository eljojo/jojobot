# What you have already been shown

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**An answer can be a difference instead of a dump.** jojobot knows what a session has already been handed, so it can say what changed rather than repeating everything — and it can tell a caller what its own write did, rather than echoing back the thing the caller just wrote.

## Two questions, and they need different machinery

**"Did my write land as I intended?"** is answered by a **receipt of the difference**: what changed as a result of this call. The server holds both sides at the moment of the write, so nothing has to be remembered to produce it. This is the small half and it is the one that removes blindness.

**"Am I working from stale context?"** is answered by a **ledger of what this session has seen**: it last saw one version, the record has moved on, here is the gap. This needs storage and it is the larger half.

**Keeping them apart is the point.** They look like one feature in a sentence and they are different sizes, and a design that treats them as one ships the expensive half to solve the cheap problem.

## Why a receipt beats an echo

**Handing back the artifact costs the caller its own context to learn something it already knew.** It wrote the body; it does not need the body returned. What it does not know is what the write actually did — which is the one thing an echo makes it work out for itself, by diffing.

**So the shape is uniform across every write verb rather than decided per verb.** Response shaping is one axis over the whole surface: a verb that answers differently from its siblings is a thing every caller has to learn separately, and the learning is the cost.

## The ledger is keyed to the session

**A session outlives the connection that made it**, so what has been shown is remembered against the session rather than the socket. A run that resumes inherits what its predecessor saw, which is what makes resumption honest rather than a fresh start wearing an old name.

**The same ledger carries the one-time teachings**, so a convention a caller has to know before it writes arrives on the call that first touches the subject, and never again. ⭐ **That it took a second and a third consumer without changing is the evidence the ledger was the right shape** — a consumer that needed its own mechanism would have meant the first one was a special case wearing a general name.

**Anything already delivered comes back marked**, with its body left out and its size in place of it, so a caller can tell a leftover from something new without being charged twice for the same content.

## The limit that keeps this honest

**It holds only where jojobot owns the data.** A foreign service that quietly drops content is caught by reading its echo and by nothing else, so suppressing that echo is something a caller opts into rather than something jojobot decides for it.

**Stated plainly because the opposite is tempting:** this makes jojobot's own surface honest. It does not make anybody else's surface honest, and selling it as though it did would hide exactly the failure that motivated it.

## What it is not

**Not compression.** Nothing is dropped to save room, and nothing is ever silently missing.

⚠️ **There are TWO reasons a thing is left out, and only one of them is this feature.** *You have already been handed this* is the ledger, above. *You did not ask for this and it is larger than what you did ask for* is the *small answer* — a thing's claims, its prose, and the writes behind a key all come back on request rather than by default, because an answer that ships everything spends a caller's context on what it already has or never wanted.

**They share the one rule that makes either honest: whatever is left out is named, with the call that returns it.** A count of what was held back is not a courtesy; it is what separates a small answer from a lossy one, and it is why neither is compression.

**Not a version history.** The question is what changed since *you* looked, not what the record has been through. **A chronology of what a record has been through is a different feature and it has its own door** — a claim's own history, read by its address. Answering one question with the other gives a caller everything except the thing it asked.

**Not a diff the caller has to compute.** Returning two versions and letting the caller subtract them is the echo problem with extra steps.

**Not a per-verb feature.** A receipt on one write verb and not its siblings makes the surface inconsistent, which costs more than the tokens it saves.
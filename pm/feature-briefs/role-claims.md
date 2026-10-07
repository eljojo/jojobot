# Role claims — one holder per role, and how a role changes hands

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-10-07

**A role is a seat that exactly one session holds at a time** — the PM, a line, a watcher — and jojobot is what enforces that, by a lease the holder keeps renewing.

## Why jojobot holds it

**Two sessions working as one role is the failure this exists to prevent.** Two coordinators answer the same report differently; two copies of one line write into the same checkout. A rule that says "only one" holds only as well as each session's attention, so the store holds it instead.

**The proof that a session is alive is that it still writes, never that its process is running.** A session can stop answering while its process looks healthy: a conversation that a safety check has refused once is refused on every later turn, and it reads idle and well on every status check. **A session that died, hung or was refused all stop writing alike**, so a beat is the one signal that gets every case right, and nothing about the role needs the agent runtime to answer.

## The lease

**Claiming is part of booting.** A session names the role it claims when it boots. A bot that carries a role claims it by default, so a line that forgets to ask still takes its seat. Which role a bot carries is written about it by somebody else, never by the bot.

**Every write the holder makes renews the lease** — a journal beat, a capture, a message. The lease is 45 minutes. Past that with no write, the role is takeable, and the next claimant gets it.

**A claim has four answers, and each says plainly what it means.** *Taken*: you hold the role, including when you were already holding it. *Refused*: somebody else holds it, named, with the moment their lease runs out. *Conflict*: your claim collided with another write in the same instant; the store worked correctly, and you send the same call again. *Unavailable*: the store could not decide; you hold nothing.

**The holder is never refused its own role.** A lease is a claim against everyone else, not a lock its own holder has to fight back into.

**A claim on a role that another bot carries is refused**, naming the bot that carries it and offering to boot as that bot. A session that names the wrong bot is a mistake, and the answer says how to fix it rather than seating it somewhere it does not belong. A role no bot carries stays free to claim.

## Where a role lives

**A role is its own object, a child of the bot that holds it**, with plain keys: who holds it, when it was last claimed, and which runtime agent is running it. A bot that held two roles would hold two children, so nothing collides, and one read over every role answers "who holds what, and since when" in a single call.

**The watchers' marks about a role live on that role too** — when it was last probed and by whom, and when it was first seen empty — so every fact about one seat is in one place rather than spread across the bots that looked at it.

**Only the claim itself writes those keys.** A caller cannot edit a role's holder or its claim time, because a hand-written holder is a seat taken without a lease.

**A change to where roles are stored is read in both shapes until a full lease has passed.** A reader that understands only the new shape sees every role as empty, and a watcher that sees an empty role starts a second session for it. So the claim code, the watchers and every charter read old and new alike, and the old shape goes only after every live lease has renewed into the new one.

## How a role changes hands

**Wrapping releases the role at once**, so a fresh session can claim it in the same minute.

**A session that still answers is asked to wrap before it is ended.** Archiving one that holds a lease without a wrap keeps the seat blocked for nothing until the lease runs out.

**A session that holds no lease is archived without being woken.** Waking a cold session only to ask it to wrap reads its whole history again against the quota, and releases nothing.

**A role is replaced in two steps, never one.** A watcher that finds a stale holder probes it first; only a probe that goes unanswered for 20 minutes makes the replacement. The second step is what keeps a session that is merely thinking from being replaced.

## What it is not

* **Not a process check.** The runtime's view of a session is never what decides whether a role is held.
* **Not a lock the holder fights.** Renewing is free, and the holder always gets its own seat back.
* **Not a second mailbox.** One bot has one box however many sessions of it run.
* **Not on a timer inside jojobot.** jojobot runs no sweep of roles; a lapsed lease simply lets the next claim through. The watchers' sweep is a session outside jojobot that reads the roles like any caller.

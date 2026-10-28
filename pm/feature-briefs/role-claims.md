# Role claims — one holder per role, and how a role changes hands

> **jojobot** · kind: feature-brief · boot: on-demand

**A role is a seat that exactly one session holds at a time** — the PM, the EM, a line — and jojobot is what enforces that, by a lease the holder keeps renewing.

## Why jojobot holds it

**Two sessions working as one role is the failure this exists to prevent.** Two coordinators answer the same report differently; two copies of one line write into the same checkout. A rule that says "only one" holds only as well as each session's attention, so the store holds it instead.

**The proof that a session is alive is that it is still working, never that its process is running.** A session can stop answering while its process looks healthy: a conversation that a safety check has refused once is refused on every later turn, and it reads idle and well on every status check. **A session that died, hung or was refused all stop working alike**, so work is the one signal that gets every case right.

## The lease

**Claiming is part of booting.** A session names the role it claims when it boots. A bot that carries a role claims it by default, so a line that forgets to ask still takes its seat. Which role a bot carries is written about it by somebody else, never by the bot. **A claim needs a voucher for that role**; how one is issued and handed to a successor is the chain of trust's.

**Only work renews the lease:** every write the holder makes, every read of its own box, and, in a managed session, the agent activity its runtime reports, a turn taken or a tool called. **A process merely existing renews nothing**, because a process that exists is exactly what a refused or hung session looks like. The lease is 45 minutes. Past that with nothing renewing it, the role is takeable, and the next claimant gets it. **A renewal moves the stamp only once it is a tenth of the lease old**, so a lease can lapse about 40 minutes after the last renewal. Restamping on every write would store one write per write for seconds of expiry.

**A claim has four answers, and each says plainly what it means.** *Taken*: you hold the role, including when you were already holding it, and when another run of your own bot held it: a fresh boot is the bot's newest word on who works the seat, so that run is displaced, its lease is yours, and the answer names it (`took_over_from`). *Refused*: another bot's run holds it, named, with the moment its lease runs out. *Conflict*: your claim collided with another write in the same instant; the store worked correctly, and you send the same call again. *Unavailable*: the store could not decide; you hold nothing.

**The holder is never refused its own role, and a displaced run is no longer the holder.** A lease is a claim against everyone else, not a lock its own holder has to fight back into. But once a newer run of the bot has taken the seat, the older run is fenced: its writes renew nothing, its claim is refused, and it is told who took the seat. Otherwise two runs would pass the seat back and forth on every write.

**A claim on a role that another bot carries is refused**, naming the bot that carries it and offering to boot as that bot. A session that names the wrong bot is a mistake, and the answer says how to fix it rather than seating it somewhere it does not belong. A role no bot carries stays free to claim.

## Where a role lives

**A role is its own object, a child of the bot that holds it**, with plain keys: who holds it and when it was last claimed. A bot that held two roles would hold two children, so nothing collides, and one read over every role answers "who holds what, and since when" in a single call.

**Which runtime agent runs a role is read from its holder's binding**, the runtime's attested report of which agent serves which run (*Lines — a bot asked to run, and the software that keeps it running*). A hand-typed agent id on a role is legacy text: it decides nothing, because a stale one once ran two copies of one line.

**Only the claim itself writes the holder keys.** A caller cannot edit a role's holder or its claim time, because a hand-written holder is a seat taken without a lease.

**A change to where roles are stored moves every old claim to the new shape once, at startup.** A reader that understands only the new shape sees an unmoved role as empty, and a watcher that sees an empty role starts a second session for it. So the move runs at startup, nothing reads the old shape as a lease afterwards, and a role the move could not finish keeps its old keys and is moved again at the next start.

## How a role changes hands

**Wrapping releases the role at once**, so a fresh session can claim it in the same minute. A wrap whose release did not land says so, and a fresh run of the same bot still takes the seat.

**A session that still answers is asked to wrap before it is ended.** Ending one that holds a lease without a wrap keeps the seat blocked for nothing until the lease runs out.

**A session that holds no lease is ended without being woken.** Waking a cold session only to ask it to wrap reads its whole history again against the quota, and releases nothing.

**A holder is replaced in two steps, never one, and only as the bot's request says.** First its lease lapses; then a probe goes unanswered for the grace the request names. The second step is what keeps a session that is merely thinking from being replaced. Who replaces it is the hypervisor service, following the recovery policy written on the bot's request; with no policy written, nobody does, and the block is reported.

## What it is not

* **Not a process check.** The runtime's view of a process is never what decides whether a role is held; only work renews it.
* **Not a lock the holder fights.** Renewing is free, and the holder always gets its own seat back until a newer run of its bot displaces it.
* **Not a second mailbox.** One bot has one box however many sessions of it run.
* **Not on a timer inside jojobot.** jojobot runs no sweep of roles; a lapsed lease simply lets the next claim through. The service's sweep runs outside jojobot and reads the roles like any caller.

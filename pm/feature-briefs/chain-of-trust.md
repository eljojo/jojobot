# The chain of trust — vouchers passed down the org chart

> **jojobot** · kind: feature-brief · boot: on-demand

**A session does a gated act only while it holds a voucher for that act, and every voucher descends down the org chart from a root that only the operator can grant.** The operator grants the root with a one-time code that jojobot sends to them and to nobody else.

## The threat it answers

**A bot's identity is declared, and declaring it is the whole of booting.** A session that boots as the assistant holds the operator's life, and nothing stops a confused or misconfigured session from booting as the assistant, or any bot from raising its own rights. A rule against impersonation holds only as well as each session's attention, so the store holds it instead.

**A voucher is exactly as safe as the session handle it is bound to.** A caller's session handle is the only address it carries, so whoever holds that handle holds that session's vouchers. The design promises no more than that, and says so.

## What needs a voucher, and what needs the code

**Four acts need a voucher:** booting a session as a bot, claiming a role, changing a subordinate's charter, and creating a bot. These are the acts that decide who a session is and what another bot becomes.

**Two acts need the operator's code itself:** starting the assistant, and changing who the operator is. The second covers the `operator` key on the instance and every write to the operator's own person record. Both acts hand over the operator's life, so no chain of delegation is allowed to reach them.

**Everything else is ungated.** Reading, writing ordinary records, posting mail and the anonymous door (booting with no bot named) need no voucher. That keeps the cost of the guard on the few acts that change identity.

## The voucher

**A voucher is its own shipped kind.** It is separate from a role because a role answers *is this seat occupied*, and a voucher answers *may this session do this act on this target*. Three of the four acts are not about seats at all.

**A voucher carries six keys:** the right (which act), the target (which bot or role), the holder (a session), the issuer (a session), the parent voucher (its link back towards the root), and a state: issued, held, escrow, spent or void.

**Only the engine writes a voucher.** A well-shaped record that a caller captures is never a grant, for the same reason a caller cannot write a role's holder: a hand-written grant is a right taken without being given.

**The rights are a shipped named set:** one per gated act, and the root that only the operator's code grants. An instance cannot add one, so a voucher for a right no code knows is refused, not accepted and ignored. The behaviour of a right lives in the build, never in a record.

## The operator's code

**A session the operator starts by hand asks for a code for the bot it names, and jojobot writes a fresh code into the operator's own mailbox.** No bot reads that box. It reaches the operator's phone, and the web UI shows it, so the web UI is also the way in when the phone push is down. There is no second channel and no second display.

**One code path, two outcomes, decided by the bot the session names.** A boot as the assistant redeems the code as the root. A boot as any other bot redeems it as that role's ordinary voucher, exactly what the assistant would have issued. A hand-started session never depends on an assistant being live.

**The booting session receives a reference to the challenge, never the code.** The operator types the code into that session's chat, and the session presents it on boot. jojobot holds only the code's hash, and the hash is spent on first use. The message is a delivery copy: a processed message is never a credential.

**One challenge stands per bot at a time.** While one waits, a new request is refused and told when the waiting code ends, so a misfiring session cannot flood the operator's phone with codes. A challenge is not a voucher: it is void once redeemed, after three wrong codes, or 10 minutes after it was issued.

**A root lasts one session.** The assistant's next session needs a fresh code, and reopening a wrapped session restores no elevation. That is why a root never passes through escrow.

## Passing a voucher down

**A holder issues vouchers only to bots below it on the org chart, and only for rights it holds itself.** A bot never derives a right from its position, and no voucher lets a bot raise its own rights. A voucher to change a charter does not lift the rule that only a superior changes the chart.

**Launching a process is not authorizing it.** The session that starts a bot's process and the chain that grants it the right to boot as that bot can differ. When the right comes from above the launcher, it travels down to the launcher as a scoped grant.

**A voucher outlives the session that issued it.** A wrapped coordinator's lines keep working. Only a live superior's act voids a voucher.

**A worked example.** The hypervisor restarts the coordinator when it dies. Starting a process is the hypervisor's job, but the right to boot as the coordinator sits above the coordinator on the chart, with the assistant. So the assistant issues a start-as-coordinator voucher scoped to that one act, the coordinator passes it down to the hypervisor, and the hypervisor presents it when it boots the replacement. The hypervisor never holds that right because of where it sits.

## Succession

**A graceful wrap hands the session's voucher to jojobot, in escrow.** A successor started within 30 minutes takes it up. The two sessions never have to overlap, and jojobot holds the right safely in between.

**A crash is not escrow, because nothing was surrendered.** A live superior voids the dead session's voucher and issues a fresh one from its own. A session counts as dead by the same two steps that replace a role's holder: its lease lapses with no write, then a probe goes unanswered.

**When one of the two watchers dies, the survivor asks the coordinator above them both by mail, and the coordinator re-issues.**

**Three clocks inside jojobot, three jobs.** The watchers' probe is theirs, not the store's. The 45-minute role lease measures whether a seat's holder is still writing. The 30-minute escrow window bounds the pickup of a surrendered voucher. The 10-minute challenge window is the one expiry in the design, and it applies to a code nobody has redeemed. **A held voucher has no expiry.** The operator asked for none. The design's reason is that an expiry would be a second clock deciding what the lease already decides, and two rules for one question drift apart.

## The first boot after the upgrade

**A store upgraded into this design holds no voucher, so the gate starts open.** Until the first root is granted, boots are ungated and the boot's answer says so. The operator's first assistant start, with its code, turns the gate on. From then on every gated act needs a voucher, and the assistant issues the first ones down the chart. A gate that went live before any voucher existed would refuse every bot, the assistant included.

## Room for read grants

**The voucher's shape — a principal, a right, a target with its descendants, a lifecycle — is the shape a standing access grant will share.** A standing grant is permanent and set by the operator. A voucher can be delegated and is bound to a session. The rights vocabulary is left open to read rights, and this feature designs none.

## What it is not

* **Not a role lease.** The lease says a seat is occupied; a voucher says an act is permitted. Folding one into the other leaves three of the four acts with nowhere to live.
* **Not a token with a timer.** A held voucher ends by being spent, voided or left in escrow past the pickup window, never by a clock. Only an unredeemed code expires.
* **Not position.** Sitting above a bot on the chart grants nothing until a voucher is issued.
* **Not a second channel for the code.** The operator's mailbox is the one route, with the web UI as the way in when the push is down.
* **Not stronger than the session handle.** A voucher bound to a stolen handle is a stolen voucher.
* **Not a gate on ordinary work.** Reads, writes and mail stay ungated.

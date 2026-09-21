# Bots and identity — colleagues in an organisation

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**A bot is an entity, and bots are colleagues in an organisation.** A public directory, private mailboxes, work status carried on a board, and no impersonation ever.

The metaphor does real work: it says which things about a colleague you may look up and which you may not, without anybody having to enumerate them.

## What ships and what is data

**One bot ships with the software** — the assistant — because that is the one role a fresh instance cannot be without. **Every other bot is one instance's data**, the coordinator and the implementer included. A bot for a house or a machine belongs in that instance's graph, not in the product.

**A shipped bot arrives knowing what it is for.** Instantiating rather than designing is the whole promise: a fresh instance whose bots boot to an empty charter has shipped a shell.

## The charter is the public part

**A charter says what a role is for, what it must not do, and what is expected of it.** It is standing behaviour — what a bot is, always, before anything is fetched.

**A colleague can read another bot's charter whole**, without booting as it and without a session being created. It is the one thing a directory exists to be looked up.

**Private state stays private**: mail, sessions and journal are not what that opens.

**Booting as another bot to read its data is forbidden outright.** A session that needs something it cannot reach says so and stops.

## One surface, aligned rather than policed

**The surface does not vary by bot.** A charter shapes what a bot considers *appropriate*; it never decides what a bot is *allowed* to call.

**jojobot aligns, it does not police.** A per-bot surface would put the judgement in the wrong place and make the software responsible for something only a mind can weigh.

## Two layers, and nothing above knows

**A charter has a half the build supplies and a half the instance writes, and neither is a copy of the other.** The build's comes first; what an instance writes sits under it and narrows it rather than repealing it. A bot nobody has written for still answers with what the build supplies.

**No verb composes them.** The build supplies its text at an address and the store resolves it underneath every read, so the verbs above hold no word for it and a caller is never told a shipped half exists. **That is the point rather than an implementation detail:** shipping data with the software must not fork the feature that data belongs to, or every later capability pays the same bill again.

**The build's half is never stored.** That is what makes upgrading it a matter of shipping a new build — nothing to migrate, no record to reconcile, and no way for an instance to be frozen on the version that created it. A default written into the store once, at creation, would pass every other test and fail that one.

**An instance's adjustment is an ordinary fact on the bot.** There is no second mechanism for customisation, and nothing is edited in place in the shipped text.

**Writing back the composed text is refused rather than trimmed.** A caller that reads a charter and sends it back whole would store the build's own words as the instance's, where they stop moving when the software does — a shipped default quietly turned into a frozen customisation. Trimming it down to the half we wanted would be worse: it keeps something the caller did not write, and they never learn which half was kept.

## What it is not

**Not an access-control system.** The directory is about what is somebody's to know, not about permissions to enforce.

**Not a place identities can be switched.** A session belongs to one bot for its whole life, and the surface never suggests otherwise.
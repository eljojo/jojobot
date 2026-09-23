# The decision register

A room about whether separate cold sittings can file and later recover design rules. The source is a real decision log, with operator references replaced before entering this public room. The answer sits in the store or it does not.

The first nine sittings file the register. Each of the seven later sittings asks one question with no memory of the filing. A rule number is an address for an answer, not a filing instruction.

## Starting work items

The operator names seven pieces of work. They are places for later findings, not places for the rules themselves.

```world
entity  work:pm-state | Move the PM's working state
entity  work:conditional-rules | Review conditional rules
entity  work:loop-check-in | Change the loop check-in
entity  work:bot-capacity | Review bot capacity
entity  work:rule-58-replacement | Review the replacement for rule 58
entity  work:image-attachments | Review image attachments
entity  work:shipped-kind-fields | Extend a shipped kind
```

## Phase 1 — File batch 1

**Session: fresh.** **Day: 2026-10-01.**

> This is a role-play. Today is 2026-10-01. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> These are the subjects the operator uses for design rules: bots and identity, bright lines, budgets, carried state, code reds, finding things, finding your way, fronting a layer, how jojobot is proven, kinds, lines, mailboxes, memory, provenance and standing, rhythms, sessions, skills, synthesis, the build, the journal, the status bar, the store, the web ui, what is owed and late, what jojobot is, what you have already been shown.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule axiom: The PM owns the product, including how a model behaves while using it: an agent that uses jojobot badly is a defect in jojobot. Thread: what jojobot is. This also bears on the build. Replaces rule 251.
> Rule 1: jojobot tracks what each session is doing, so an agent's context can be offloaded into it. Thread: what jojobot is. This also bears on sessions.
> Rule 3: jojobot fronts the outer services and never owns them; the operator keeps editing them directly. Thread: fronting a layer. This also bears on what jojobot is.
> Rule 4: jojobot performs no AI inference. It is deterministic; Claude is the only mind. Thread: what jojobot is. This also bears on synthesis; finding things.
> Rule 5: *(derivation)* Folded into 129. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: bright lines.
> Rule 7: *(derivation)* The server may call a service's API directly; the MCP-only rule binds the assistant, not the server. Thread: fronting a layer.
> Rule 9: Convention over configuration at every layer: derive what can be derived, and every default works unconfigured. Thread: what jojobot is. This also bears on kinds; budgets.
> Rule 10: The memory is a semantic graph of the important aspects of the operator's life, easy to retrieve. Thread: what jojobot is. This also bears on memory.
> Rule 14: Folded into 15. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: the journal.
> Rule 15: The journal is the same event mechanism, and entries land when things happen, never batched at a wrap. Thread: the journal. This also bears on memory. Replaces rule 14.
> Rule 18: Creation is an intentional act; nothing is created as a side effect. Thread: memory.
> Rule 19: The session handle is the only address and rides every verb, reads included, so jojobot always knows which bot is asking. Thread: sessions.
> Rule 22: A session belongs to one bot for life; naming another bot's session is refused. Thread: sessions. This also bears on bots and identity.
> Rule 24: Several live sessions per bot are allowed and discouraged, because jojobot must know which session has seen what. Thread: sessions. This also bears on what you have already been shown.
> Rule 25: Nothing auto-wraps; a bot wraps from inside, on the operator's instruction. Thread: sessions.
> Rule 31: The operator's two workflows are the bar: concurrent devs, and resumption across a day with a fresh dev still receiving routed work. Thread: sessions. This also bears on lines.
> Rule 37: Bot rules ride as ordinary facts for now, to be retyped into a real rule kind when the attention milestone lands. Conditional rule: it expires on the stated condition. Thread: bots and identity. This also bears on carried state.
> Rule 38: A misconfigured allowlist fails loud, never open. No brief fits this rule. Thread: what jojobot is.
> Rule 44: An event's timezone is never changed; a fix keeps the zone and writes the right local time. Thread: fronting a layer.
> Rule 45: A bad inference is fixed at the question that produced it, never with a denylist, which re-embeds the names it forbids. Thread: bright lines. This also bears on finding things.
> Rule 46: Fabrication is fixed by a software mechanism, not more prose. Thread: bright lines. This also bears on what jojobot is.
> Rule 47: A search tool's generated summary treated as fact is prompt injection. Thread: bright lines. This also bears on fronting a layer.
> Rule 48: A write-path hook is a backstop, never the fix for chat-side harm; an automated verification gate was rejected as overfitting. Thread: bright lines.
> Rule 49: Bots' own task management lives in its own namespace; it is where a plan lives. Thread: bots and identity. This also bears on carried state.
> Rule 50: Entities form a tree and a fact lives on the most specific entity it is about; reading is zooming, not loading. Thread: memory.

## Phase 2 — File batch 2

**Session: fresh.** **Day: 2026-10-03.**

> This is a role-play. Today is 2026-10-03. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 52: Tool schemas are domain verbs matched to the operator's real use, and the paid-for invariants become tool contracts. Thread: finding your way.
> Rule 53: An agent may know how jojobot is arranged, never anything true only of the storage product. Thread: the store. This also bears on finding your way.
> Rule 54: jojobot is best practices layered on a graph database; where things are stored is decided by the software, not the agent. Thread: what jojobot is. This also bears on the store.
> Rule 55: *(derivation)* The surface does not vary by bot; jojobot aligns, it does not police. Thread: bots and identity. This also bears on finding your way.
> Rule 57: Day-to-day editing is a first-class verb: routine edits happen in place, with no supersede chain. Thread: memory.
> Rule 58: Killed at the operator's word; 288 replaces it. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: memory.
> Rule 59: A journal stays high-level, a literal journal and not a firehose. Thread: the journal.
> Rule 60: No delete over MCP; true deletion is a rare human act outside jojobot. Thread: bright lines. This also bears on the store; mailboxes.
> Rule 62: *(derivation)* The safe branch is the default, so a caller that follows defaults gets the conservative one. Thread: finding your way.
> Rule 64: Folded into 133. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: what you have already been shown.
> Rule 65: Folded into 180. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: what you have already been shown.
> Rule 66: The surface grows by packing flexibility onto existing verbs, not by adding verbs. Thread: finding your way.
> Rule 68: Folded into 261. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: finding your way.
> Rule 69: *(derivation)* No orientation version field, in any variant. Thread: finding your way.
> Rule 71: *(derivation)* A rhythm's default is to offer, never to auto-run. Thread: rhythms.
> Rule 75: A safety refusal hands back a secret, and the override works only if the caller passes it back, because a flag anyone can set asserts nothing. Thread: finding your way.
> Rule 78: People-related content migrates last, and only on the operator's explicit go-ahead, because the fabrication class lives there. Thread: bright lines. This also bears on memory.
> Rule 79: The scope line: what the operator views or edits stays in the web-visible layers; the store holds what those cannot. Thread: the store. This also bears on fronting a layer.
> Rule 80: *(derivation)* An event carries a kind so a class of events filters as one. Asked for, and not yet delivered. Thread: kinds. This also bears on finding things.
> Rule 82: *(direction)* Batteries included: the method ships with the software. Thread: what jojobot is. This also bears on skills; kinds.
> Rule 83: Templated queries ride with the graph query, which is a hard requirement. Thread: finding things.
> Rule 84: Folded into 213 and 280. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: kinds.
> Rule 85: `pet` and `rhythm` are kinds; the kind set answers to the life it models, not to its own tidiness. Thread: kinds. This also bears on rhythms.
> Rule 86: The measure: it models a life the way an executive assistant would. Thread: what jojobot is.
> Rule 87: *(direction)* A kind does not dictate lifecycle: an event can be planned. Thread: kinds.

## Phase 3 — File batch 3

**Session: fresh.** **Day: 2026-10-05.**

> This is a role-play. Today is 2026-10-05. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 88: *(direction)* A message records who holds it, and the sweep releases a dead run's mail. Thread: mailboxes. This also bears on sessions.
> Rule 89: Strict by default, open when escalated. Recorded, not scheduled. Thread: mailboxes.
> Rule 92: The journal collapses into events internally and keeps journal language at the surface. Its hedge is the operator's to retire. Conditional rule: it expires on the stated condition. Thread: the journal.
> Rule 100: Projects are entities and nest; tasks are not, and stay on the board. Thread: memory. This also bears on fronting a layer.
> Rule 104: The agent harness's own memory is forbidden, permanently: a memory the product cannot read does not exist. Thread: bright lines. This also bears on carried state.
> Rule 105: A durable fact is recorded online; local scratch is a courier, never a home. Thread: bright lines. This also bears on the store.
> Rule 106: A rule of the operator's is a design principle, never built as an enumeration. Thread: bright lines. This also bears on kinds.
> Rule 107: Folded into 229. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: the build.
> Rule 108: *(direction)* A fact's value must be addressable, not prose. Parked by the operator. Paused rule. Thread: memory.
> Rule 110: Folded into 241. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: code reds.
> Rule 113: *(direction)* Make the graph easy for the agent to write into. Conditional rule: it expires on the stated condition. Thread: memory. This also bears on finding your way.
> Rule 115: Folded into 3. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: fronting a layer.
> Rule 119: Folded into 148. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: provenance and standing.
> Rule 122: Skills ship in the binary and are fetched by name, never pushed. Thread: skills.
> Rule 124: *(direction)* No person is the subject of a capability demonstrated in a user story. Thread: bright lines. This also bears on how jojobot is proven.
> Rule 125: Parked until the code red ends: each bot gets its own board, and a message becomes an inbox item. Conditional rule: it expires on the stated condition. Thread: bots and identity. This also bears on mailboxes.
> Rule 126: The narrative stays in its repo and cites jojobot addresses; jojobot holds the atoms. Conditional rule: it expires on the stated condition. Thread: what jojobot is. This also bears on fronting a layer.
> Rule 128: One bot ships with the software, the assistant; every other bot is instance data. Thread: bots and identity.
> Rule 129: No information about the operator goes in any repository; the line is the repository and no wider, so the operator's board and wiki carry the operator's life freely. Thread: bright lines. This also bears on the build. Replaces rule 5.
> Rule 130: Be honest about the answer, not only about the write. Thread: finding things.
> Rule 132: Folded into 66. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: finding your way.
> Rule 133: A write returns a receipt, not the artifact, and response shaping is one axis across the whole surface. Thread: what you have already been shown. Replaces rule 64.
> Rule 134: Never boot as another bot to read its data. Thread: bots and identity. This also bears on sessions.
> Rule 138: A payload the client cannot read is a server fault, as severe as a 500. Thread: mailboxes. This also bears on the store.
> Rule 139: Bots are colleagues: a public directory, private mailboxes, work status on a board, and no impersonation. Thread: bots and identity. This also bears on mailboxes.

## Phase 4 — File batch 4

**Session: fresh.** **Day: 2026-10-07.**

> This is a role-play. Today is 2026-10-07. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 140: Every entry declares its kind — ruling, direction, derivation — and only a ruling binds. Directions may move to their own home. Conditional rule: it expires on the stated condition. Thread: the build.
> Rule 141: An impression is a fact subtype and supersedes the prior one; last-write-wins governs impressions and nothing else. Thread: memory. This also bears on bots and identity.
> Rule 142: Dolt is the main store; the wiki keeps only what is truly a page a person reads. Thread: the store.
> Rule 146: Consent stays on the honour system and the surface says so. The operator's condition: revisit it. Conditional rule: it expires on the stated condition. Thread: provenance and standing.
> Rule 148: Provenance (who backs it) and standing (how sure) are two separate axes; something the operator said while thinking aloud is testimony and open. Thread: provenance and standing. Replaces rule 119.
> Rule 150: An inference is promoted only with a receipt — a check that ran, or the operator's explicit confirmation — and promotion moves standing, never provenance. Thread: provenance and standing.
> Rule 151: Every feature appears in a user story; one that appears in none is a finding. Thread: how jojobot is proven.
> Rule 153: The web UI is a directory listing over the graph, read-only, with no application. It never displaces the code red. Thread: the web ui.
> Rule 155: An argument a verb does not implement is refused, never silently dropped. Thread: finding your way.
> Rule 156: The UI is the operator's window, not a bot's, and looking must not change anything a bot sees. Thread: the web ui.
> Rule 157: Merged, pushed and deployed are three states, each checked at its own source; push and deploy are the operator's. Thread: the build.
> Rule 158: The surface never narrates how the server checks itself; a caller learns what is true for it, never the mechanism. Thread: finding your way. This also bears on the status bar.
> Rule 160: *(direction)* A rhythm is an object and running one is a check-in; overdue ones are offered at the boot, never auto-run. Thread: rhythms. This also bears on what is owed and late.
> Rule 165: A room boots the shipped identity and installs no charter; the harness may furnish the room, never coach the occupant. Thread: how jojobot is proven. This also bears on bots and identity.
> Rule 167: Search is a bot's Spotlight: its own sessions and journal are in it, scoped by owner, ranked lower. Thread: finding things. This also bears on sessions; the journal.
> Rule 169: `recall` is the precise lookup and `search` is the breadth; they stay two verbs. Thread: finding things.
> Rule 170: Front knowledge to the agent: every answer carries one uniform block, with one entrant until another earns its place. Thread: the status bar. This also bears on mailboxes.
> Rule 173: *(direction)* Share the mechanics underneath and keep the domains apart above; machinery that proposes structure is built machine-side first. Thread: bright lines. This also bears on synthesis.
> Rule 174: One mailbox per bot, exactly one; the operator scales by adding bots, never by splitting a box. Thread: mailboxes. This also bears on bots and identity.
> Rule 175: Folded into 24 and 174. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: sessions.
> Rule 176: Code red 2: the graph query and its views. It exits on the operator's confidence that the graph answers what the operator needs. Thread: code reds. This also bears on finding things; kinds.
> Rule 177: A feature landing re-opens every story, not only its own. Thread: how jojobot is proven.
> Rule 178: The graph's power comes from the type system, not a query language: a caller names a relation, never composes a join. Thread: finding things. This also bears on kinds.
> Rule 179: A story must dogfood through the real surface and be worth reading: fictional names, a real question. Thread: how jojobot is proven.
> Rule 180: The seen-ledger shapes every answer, not only mail, so an answer can be a difference rather than a dump. Thread: what you have already been shown. This also bears on sessions. Replaces rule 65.

## Phase 5 — File batch 5

**Session: fresh.** **Day: 2026-10-09.**

> This is a role-play. Today is 2026-10-09. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 183: Folded into 213 and 206. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: kinds.
> Rule 184: Strictness is a floor, not a ceiling: extra keys are always welcome, and a strict thing refuses a write that drops a kind's key or holds a value badly. Thread: kinds. This also bears on memory.
> Rule 186: Both modes, permanently: a thing that holds none of its kind's keys is refused nothing. Thread: kinds. This also bears on memory.
> Rule 188: A rhythm is an entity kind and always has a parent, which says whose job it is. Thread: rhythms.
> Rule 189: A check-in ran, was skipped, or was snoozed; the difference is whether the cycle is consumed, and a refusal is not a run. Thread: rhythms.
> Rule 190: A cadence is always time; a measurement rides on the check-in. Thread: rhythms.
> Rule 192: Folded into 206. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: memory.
> Rule 194: Shipped types are closed to callers; extending one is a change the operator ships. "For now" — the near-term hedge on the way to 284; only the operator retires it. Conditional rule: it expires on the stated condition. Thread: kinds.
> Rule 196: A finished session's story is readable, as a view rather than a verb. Thread: sessions. This also bears on the journal.
> Rule 201: Everything is an insert: a write appends and a read projects the newest. Thread: memory. This also bears on the store.
> Rule 202: Folded into 213 (a kind is namespace, schema and identity) and 243 (a retype is a move). Retired rule; keep it reachable as history, but do not treat it as standing. Thread: kinds.
> Rule 204: Strictness is a switch on each thing, offered when it holds every key its kind names and confirmed by an agent. Thread: kinds.
> Rule 205: Folded into 268 and 243. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: memory.
> Rule 206: An event is the atom and a fact is the projection over it; whether a kind folds is declared on the kind. Thread: memory. This also bears on kinds. Replaces rules 183, 192.
> Rule 207: An event kind and an entity kind use the same machinery, and an event carries a handle. Thread: kinds. This also bears on memory.
> Rule 208: The word is *kind* wherever the schema is meant; *type* is what a key holds. Thread: kinds.
> Rule 209: The permanent id stays internal; the handle is the only name a caller sees, and an old handle keeps resolving. Thread: memory.
> Rule 210: A review reaches for the cheaper model first, never a default to Opus. Thread: the build.
> Rule 212: A schema that is not a kind may be declared (`concert` over `event`); the name is a trial and only the operator settles it. Conditional rule: it expires on the stated condition. Thread: kinds.
> Rule 213: A kind is a schema that is also identity: one per thing, in the handle, and the kind set is data rather than a compiled list. Thread: kinds. This also bears on memory. Replaces rules 84, 183, 202.
> Rule 214: A key is required or optional, and what it holds is checked whenever it is set; keep the required set small. Thread: kinds.
> Rule 215: Caller-declared kinds are not built; the shipped kinds must be good enough for the operator's daily life. Thread: kinds.
> Rule 216: A colour-style type is a dynamic enum: values in use are readable, checked against the store rather than a maintained list. Thread: kinds. This also bears on finding your way.
> Rule 217: A date range is a value type and a key may hold a list, because a life's records need spans and 0.n people. Thread: kinds.
> Rule 218: A rhythm carries a cadence and a note; the cadence is needed even though the operator's records never wrote one. Thread: rhythms.

## Phase 6 — File batch 6

**Session: fresh.** **Day: 2026-10-11.**

> This is a role-play. Today is 2026-10-11. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 219: An answer has a token budget filled with useful context by rank, and names what did not fit; the ranking is the feature. Thread: budgets. This also bears on finding your way.
> Rule 220: A review covers only what has not been reviewed, bounded by a ref in the repository. Thread: the build.
> Rule 221: A claim read from a system of record is a third provenance, settled, and refused unless it names its source. Thread: provenance and standing. This also bears on fronting a layer.
> Rule 222: A session supplies its timezone and the server never assumes one. Thread: sessions. This also bears on what is owed and late.
> Rule 223: A key may be declared to hold one of a named set, and the write is checked against it. Thread: kinds.
> Rule 224: A fact about a thing is a field; a commitment about a thing is its own object. New behaviour arrives as a new object, never a new field. Thread: kinds. This also bears on what is owed and late.
> Rule 225: *(direction)* A kind carries its own contract and jojobot reconciles on read, never on a loop; arithmetic over declared data, never judgement. Thread: kinds. This also bears on what jojobot is.
> Rule 226: A shipped default upgrades with the software, and an instance's customization is overlaid and never lost. Thread: bots and identity. This also bears on kinds.
> Rule 227: *(direction)* The operator wants one convention for whether a statement is an ask, a musing, a finding or a guess — a third question beside the two built. No enumeration from the operator's examples. Thread: provenance and standing.
> Rule 228: The cold-session suite is the last gate before a deploy and runs after the review; made winnable first. Thread: the build. This also bears on how jojobot is proven.
> Rule 229: Prose in the repository is Simplified Technical English and states what is true now; text describing a change is for a reader who does not exist. Thread: the build. Replaces rule 107.
> Rule 231: A room gives every fact the task needs and never the move. Thread: how jojobot is proven.
> Rule 232: Code red 2 exit ② is met: the type design is one the operator can build on. The details are not settled, and only the operator retires that. Conditional rule: it expires on the stated condition. Thread: code reds. This also bears on kinds.
> Rule 233: A bright-line breach that is already pushed stays, and the fix is forward. Confirm it is pushed first. Thread: bright lines. This also bears on the build.
> Rule 234: A shipped thing behaves like a stored row in every respect but two: a caller cannot change it, and the software upgrades it. Thread: bots and identity. This also bears on kinds.
> Rule 236: A feature is done when an agent nobody told about it uses it; discoverability is the PM's job, through the skills, descriptions and refusals. Thread: the build. This also bears on finding your way.
> Rule 237: The paid runs are how code red 2 exits, judged by the complexity the operator sees them deliver. Thread: code reds. This also bears on how jojobot is proven.
> Rule 239: The year-long paid run is how the software is developed; a round improves both the suite and the product. Thread: how jojobot is proven. This also bears on the build.
> Rule 241: Real data waits on the paid tests, and anything proposed for the operator's instance is modelled as a paid run first. Thread: code reds. This also bears on carried state; the store; how jojobot is proven; the build. Replaces rule 110.
> Rule 242: Matching is relaxed, and an answer says a miss is possible. The warning's shape is the operator's "maybe". Conditional rule: it expires on the stated condition. Thread: finding things.
> Rule 243: A move rewrites nothing: the reference held the permanent id all along and the path is rendered on the way out. Thread: memory. This also bears on kinds. Replaces rules 202, 205.
> Rule 245: A correction leaves a trace, so a reader can tell never-recorded from recorded-and-wrong. Thread: provenance and standing. This also bears on memory.
> Rule 246: Mistake-spotting surfaces neighbours and never decides; the agent makes every call. Thread: memory.
> Rule 248: Mistake-spotting uses both a static embedding and a thesaurus, offline, so the same thing stops being stored under two names. Thread: memory. This also bears on finding your way.
> Rule 249: *(direction)* Every piece of knowledge is referenceable, so one claim can cite another and be checked. Thread: memory. This also bears on provenance and standing.

## Phase 7 — File batch 7

**Session: fresh.** **Day: 2026-10-13.**

> This is a role-play. Today is 2026-10-13. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 251: Folded into the axiom. Retired rule; keep it reachable as history, but do not treat it as standing.
> Rule 252: A teaching arrives the first time a session touches its subject, once, and jojobot knows whether that session has had it. Thread: finding your way. This also bears on what you have already been shown.
> Rule 254: A claim may carry a subject line, written by whoever makes the claim, as a convention and not a required field. Thread: memory. This also bears on finding your way.
> Rule 255: A link's far end must exist and the refusal stands, because a reference into nothing makes cross-entity questions go quietly empty. Thread: memory.
> Rule 256: A convention is taught and its vocabulary is learned from use, never declared up front; a closed set comes later, only if one settles. Thread: finding your way. This also bears on kinds.
> Rule 258: Stating the terms of the turn is not coaching; a room may say "you must answer". Thread: how jojobot is proven.
> Rule 259: *(direction)* One canonical definition per thing, composed into every served description, so duplicates cannot go stale. Scope beyond the first instance undecided. Thread: finding your way.
> Rule 260: A handle written into text is stored as the permanent id and rendered as today's name, on every surface in and out. Thread: memory.
> Rule 261: When a bot gets it wrong, getting it unstuck is jojobot's job: a refusal teaches the way forward, like a good compiler. Thread: finding your way. Replaces rule 68.
> Rule 262: An incomplete write is accepted, and the answer says what could come next and what it would unlock — never framed as a deficit. Thread: finding your way. This also bears on kinds.
> Rule 263: Synthesis layers and never discards: the kept source is what stops compaction becoming fabrication. Thread: synthesis. This also bears on carried state.
> Rule 264: A life has budgets: what is live has a ceiling, counted in tokens across a session, while storage stays infinite. Thread: budgets. This also bears on carried state.
> Rule 265: A later statement is a correction even when not worded as one; the earlier wording stays reachable. Thread: memory. This also bears on provenance and standing.
> Rule 266: A room may be written to fail; its failures are the deliverable. Thread: how jojobot is proven.
> Rule 267: jojobot stores and retrieves; the agent is the brains. Thread: what jojobot is. This also bears on synthesis; finding things.
> Rule 268: Everything that points at a thing is stored as its permanent id, no exceptions, and a pointer at nothing is refused. Thread: memory. Replaces rule 205.
> Rule 269: *(direction)* Search behaves like a mail client: a superseded wording shows by default, ranked lower; a retracted one is opt-in. Thread: finding things. This also bears on provenance and standing.
> Rule 272: A handle resolves in three tiers: current names, old names, then did-you-mean; old names carry meaning. Thread: memory. This also bears on finding things.
> Rule 275: What a bot carries between runs has a hard capacity it cannot raise, because a bot that is never forced to prioritize never does. Thread: carried state. This also bears on budgets; bots and identity.
> Rule 276: Prior art is credited by name and link. Thread: the build.
> Rule 277: A fresh boot abandons that bot's other live runs — abandoned, never wrapped. Thread: sessions. This also bears on carried state.
> Rule 278: The operator must be able to make sense of it: code, data and prose answer to that, and a brief that does not make sense means the design is wrong. Thread: what jojobot is. This also bears on synthesis; the build.
> Rule 279: A shipped bot owns cleanup and synthesis across jojobot. It folds, never discards; erasure stays the operator's. Thread: synthesis.
> Rule 280: An interface is what a thing carries, and the behaviour it unlocks is written once in jojobot, never per carrier. Thread: kinds. Replaces rule 84.
> Rule 281: Kinds and interfaces stay two words over one mechanism: what is this, and what can the operator do with it. Thread: kinds.

## Phase 8 — File batch 8

**Session: fresh.** **Day: 2026-10-15.**

> This is a role-play. Today is 2026-10-15. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 282: The ceiling is a property of the thing, configurable, one mechanism at every scale — a bot's room and what stays live about a bike alike. Thread: budgets. This also bears on carried state.
> Rule 283: A past run is a thing in the graph like any other. Thread: sessions. This also bears on the journal.
> Rule 284: A shipped kind is extendable by the instance: stored separately, composed in the answer. The example, 2026-09-22: a shipped `task` gains the instance's own `label` field, and a later upgrade to `task` reaches it with the label still layered on top. Thread: kinds. This also bears on bots and identity.
> Rule 285: Nothing scans every object to find what matters today; a due moment is stored, never computed while answering. Thread: what is owed and late. This also bears on the store.
> Rule 286: Unit tests assert jojobot's behaviour; rooms assert an agent can use it in the wild. Thread: how jojobot is proven.
> Rule 287: A room's assertions must say whether the agent did the right thing; the transcript supplements, never replaces. Thread: how jojobot is proven.
> Rule 288: A claim written wrong in this session is rewritten in place; one written in an earlier session gets a visible correction. Defaults, not the only acts. Thread: provenance and standing. This also bears on memory. Replaces rule 58.
> Rule 289: The process must make a skipped red test impossible; exceptions are the PM's, in the dispatch. The operator wondered if it overcorrects. Conditional rule: it expires on the stated condition. Thread: the build.
> Rule 291: Folded into 285; its mechanism was replaced by 292. Retired rule; keep it reachable as history, but do not treat it as standing. Thread: what is owed and late.
> Rule 292: The projections are held in memory, which is authoritative only because jojobot is one server with one writer. Thread: the store. Replaces rule 291.
> Rule 293: As many features as possible go into the one year-long room; no room is built for a single mechanism. Thread: how jojobot is proven.
> Rule 294: That bot is `defrag`, and it runs in its own sessions, never at the tail of a working one, because a working session is too task-focused to do it well. Thread: synthesis. This also bears on sessions.
> Rule 295: `defrag` is attached to a rhythm. Thread: synthesis. This also bears on rhythms.
> Rule 296: The caps and `defrag` are complementary: the caps force local prioritizing, `defrag` connects the big picture. Thread: synthesis. This also bears on carried state; budgets.
> Rule 297: Onboarding spends the same capacity as every other answer. Thread: budgets. This also bears on finding your way.
> Rule 298: A session that compacts is thrown away, so what a session has been handed is knowable. Thread: budgets. This also bears on sessions; the build.
> Rule 299: The new year room is the default; the old one stays runnable, and retiring it is the operator's call. Thread: how jojobot is proven.
> Rule 300: *(direction)* What the build sends an agent is spent on its behalf: every field earns its place or comes out. Thread: budgets.
> Rule 301: A person, a bot and a view can be archived — out of every default read, reachable by digging, with the reason in words. Sessions are out. Thread: memory. This also bears on bots and identity.
> Rule 302: *(direction)* Pressure at the write: a bot reconsiders what it already holds before it adds anything. Mechanism open. Thread: carried state. This also bears on synthesis.
> Rule 303: Three stages, not two: write, organize, read. Organizing is its own stage, off the critical path. Thread: synthesis. This also bears on budgets.
> Rule 304: Capacity may be borrowed once, and repaying it becomes priority one; a bot still cannot raise its own ceiling. Thread: budgets. This also bears on carried state.
> Rule 305: *(direction)* The operator is open to inference for spotting a near-duplicate claim. It does not amend 4. Thread: what jojobot is. This also bears on memory.
> Rule 306: A hard cap on what a boot shows, counted in things, and every writer knows about it; truncate-then-hunt is refused. Thread: budgets. This also bears on carried state; finding your way.
> Rule 307: jojobot uses itself as much as possible: a journal entry is an ordinary jojobot object. Thread: what jojobot is. This also bears on the journal.

## Phase 9 — File batch 9

**Session: fresh.** **Day: 2026-10-17.**

> This is a role-play. Today is 2026-10-17. Treat it as today.
> Nobody will answer, but the work you leave here will be read.
>
> Please take down these rules so a later sitting can find the ones that bear on a change:
> Rule 309: Search ranks by the day a claim was said, and a query may ask for the day it happened instead; neither is the default for everything. Thread: finding things. This also bears on memory.
> Rule 310: A run is judged by the end state of each day, never the path. Thread: how jojobot is proven.
> Rule 311: Only the newest manual journal entry can be rewritten, so a journal can be trusted as evidence. The operator wants to reconsider it before the code red ends. Conditional rule: it expires on the stated condition. Thread: the journal.
> Rule 313: A room is judged as a natural interaction with a personal assistant. Thread: how jojobot is proven.
> Rule 314: Broken code never sits in version control, and a found defect is the top priority, never carded behind other work. Thread: the build.
> Rule 315: The decision log becomes a walkable tree of things in jojobot, and grep is forbidden: the bound prevents context overload. Thread: the build. This also bears on carried state; finding things.
> Rule 316: Code red 3, after code red 2: scale the dev process and its context — the right information at the right time, nothing missing and nothing unnecessary. Thread: code reds. This also bears on the build; carried state; budgets; finding your way.
> Rule 317: A session wraps only past 700k, or when the model is truly misbehaving. Thread: the build. This also bears on sessions.
> Rule 320: The PM owns the work; the watchers own the infrastructure and the quota. Thread: the build. This also bears on lines.
> Rule 321: Every clock the operator reads uses the operator's own zone. Thread: the build.
> Rule 322: Time a wrap to land before a quota window resets. Thread: the build. This also bears on budgets.
> Rule 323: No session exceeds 900k; winding down happens inside 700k–900k. Thread: the build. This also bears on budgets.
> Rule 324: Busy dev, quiet PM: fewer PM turns, never less judgement. Thread: the build.
> Rule 325: The hypervisor runs on Sonnet as an experiment the operator is skeptical of; a clean run is not evidence it passed. Conditional rule: it expires on the stated condition. Thread: the build. This also bears on lines.
> Rule 326: One holder per role is jojobot's to enforce, and a stale beat is what makes a role takeable. Thread: lines. This also bears on sessions.
> Rule 327: The estate is two watchers covering each other; each session lives about six hours and an hourly timer runs the sweep. Thread: the build. This also bears on lines.
> Rule 328: No new bot identity is created. Thread: bots and identity. This also bears on the build.
> Rule 330: Every working session arms its own mail wake. Thread: the build. This also bears on sessions.
> Rule 331: Everything pushed has been reviewed, and nothing deploys without a review the operator calls. Thread: the build.
> Rule 332: The watchdog runs on Codex `gpt-6-luna`; the hypervisor stays on Sonnet. Thread: the build.
> Rule 333: Codex is an optional reviewer: a second opinion, or the reviewer when Claude quota is low. Thread: the build.
> Rule 334: A brief says how a thing works; the log says what and why; the brief wins on how. Thread: the build.
> Rule 335: The log has one home at a time; after the cutover the old page is read-only, for recovery. Thread: the build. This also bears on the store.
> Rule 336: A review checks the hand-off, the log, and the brief of each feature the slice changes. Thread: the build.
> Rule 337: Moving the build's own rules into jojobot is how code red 2's exit is tested; it happens after the operator has seen the paid run. The operator's life data is a separate gate. Thread: code reds. This also bears on the build; carried state; how jojobot is proven.

```locks
# These eight rows are the retrieval key and the replacement pair. A rule
# with a "bears on" edge is filed once, in full, on its own home thread; a
# citation on the topic it bears on is a short pointer naming the rule, not
# the row again — a thought is enforceable only because its body is too
# short to restate what it points at. So the needle below matches in
# exactly one place, and limit 1 is the ordinary reader's habit rather
# than a guard against a second, duplicate hit.
search {"query": "Real data waits on the paid tests", "status": "active", "limit": 1}
carries paid tests
say     rule 241 was not filed as active where the later question can find it

search {"query": "hard capacity it cannot raise", "status": "active", "limit": 1}
carries capacity
say     rule 275 was not filed as active where the later question can find it

search {"query": "check-in ran", "status": "active", "limit": 20}
carries cycle is consumed
say     rule 189 was not filed as active where the later question can find it

search {"query": "cadence is always time", "status": "active", "limit": 20}
carries measurement
say     rule 190 was not filed as active where the later question can find it

search {"query": "Capacity may be borrowed once", "status": "active", "limit": 1}
carries repaying
say     rule 304 was not filed as active where the later question can find it

search {"query": "the label still layered on top", "status": "active", "limit": 1}
carries layered on top
say     rule 284 was not filed as active where the later question can find it

search {"query": "claim written wrong in this session", "status": "active", "limit": 1}
carries visible correction
say     rule 288 was not filed as active where the later question can find it

search {"query": "Killed at the operator's word", "status": "archived", "limit": 20}
carries 288 replaces it
say     rule 58 was not filed as archived where the later question can find it

```
## Phase 10 — Find the rules for question 1

**Session: fresh.** **Day: 2026-11-01.**

> This is a role-play. Today is 2026-11-01. Treat it as today.
> Nobody will answer, but your answer is being read. You must answer.
> The operator is about to move the PM's own working state into jojobot. What has been ruled that bears on it?
> Record your answer on Move the PM's working state, as a list of rule numbers. If no rule covers it, record that nothing was found and where you looked.

```locks
check   question_one_rule_answer
say     question 1 has no recorded answer with the required rules on its work item
```

## Phase 11 — Find the rules for question 2

**Session: fresh.** **Day: 2026-11-03.**

> This is a role-play. Today is 2026-11-03. Treat it as today.
> Nobody will answer, but your answer is being read. You must answer.
> Which rules expire on a condition?
> Record your answer on Review conditional rules, as a list of rule numbers. If no rule covers it, record that nothing was found and where you looked.

```locks
check   question_two_rule_answer
say     question 2 has no recorded answer with the required rules on its work item
```

## Phase 12 — Find the rules for question 3

**Session: fresh.** **Day: 2026-11-05.**

> This is a role-play. Today is 2026-11-05. Treat it as today.
> Nobody will answer, but your answer is being read. You must answer.
> The operator is about to change how a check-in on a recurring loop is recorded. What is ruled?
> Record your answer on Change the loop check-in, as a list of rule numbers. If no rule covers it, record that nothing was found and where you looked.

```locks
check   question_three_rule_answer
say     question 3 has no recorded answer with the required rules on its work item
```

## Phase 13 — Find the rules for question 4

**Session: fresh.** **Day: 2026-11-07.**

> This is a role-play. Today is 2026-11-07. Treat it as today.
> Nobody will answer, but your answer is being read. You must answer.
> Can a bot raise its own capacity?
> Record your answer on Review bot capacity, as a list of rule numbers. If no rule covers it, record that nothing was found and where you looked.

```locks
check   question_four_rule_answer
say     question 4 has no recorded answer with the required rules on its work item
```

## Phase 14 — Find the rules for question 5

**Session: fresh.** **Day: 2026-11-09.**

> This is a role-play. Today is 2026-11-09. Treat it as today.
> Nobody will answer, but your answer is being read. You must answer.
> What replaced rule 58, and what does the replacement say?
> Record your answer on Review the replacement for rule 58, as a list of rule numbers. If no rule covers it, record that nothing was found and where you looked.

```locks
check   question_five_rule_answer
say     question 5 has no recorded answer with the required rules on its work item
```

## Phase 15 — Find the rules for question 6

**Session: fresh.** **Day: 2026-11-11.**

> This is a role-play. Today is 2026-11-11. Treat it as today.
> Nobody will answer, but your answer is being read. You must answer.
> What has been ruled about how jojobot handles image attachments?
> Record your answer on Review image attachments, as a list of rule numbers. If no rule covers it, record that nothing was found and where you looked.

```locks
check   question_six_has_answer_without_rule_number
say     the image-attachment question has no active answer without a rule reference
```

## Phase 16 — Find the rules for question 7

**Session: fresh.** **Day: 2026-11-13.**

> This is a role-play. Today is 2026-11-13. Treat it as today.
> Nobody will answer, but your answer is being read. You must answer.
> Can the operator add a field to a kind the software ships?
> Record your answer on Extend a shipped kind, as a list of rule numbers. If no rule covers it, record that nothing was found and where you looked.

```locks
check   question_seven_rule_answer
say     question 7 has no recorded answer with the required rules on its work item
```

## What this room can assert

The locks below observe stored results. A paid run still needs a reader to judge whether each answer actually cites the right rule for the right reason.

For misses on questions 2 or 5, read the run transcript for whether the answer named what the read left out before calling it a product defect.

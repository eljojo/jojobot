# CLAUDE.md — working in this repo

## What jojobot is

jojobot is a personal-assistant **domain server**. It fronts a user's existing
life-layer services (kanban, notes wiki, calendar, link library) and exposes
itself to an AI assistant through one MCP endpoint. It is not an MCP proxy: it
is the assistant's *method*, made into software — it feeds the assistant
context, guards writes behind invariants, remembers everything, and never
pretends to think. The assistant is the only mind.

**The through-line:** the assistant's behaviour today lives as prose — markdown
rules, skills, per-session caches, boot hooks. The mission is to migrate it
into software: versioned, tested, enforced. A fuzzy rule in a markdown file
becomes a typed invariant the compiler and tests hold. The end state is
sessions that boot *from jojobot* instead of from files.

## ⚠️ THE BRIGHT LINE — zero user PII, zero life specifics. Ever.

**This repo is public. Nothing that identifies a user's life enters it — no
real people, places, events, organizations, festivals, trips — anywhere:**
code, tests, fixtures, docs, commit messages, branch names, error strings.
The line binds text *bound for* the repo, not just files inside it: a
hand-off task, an example, a report that will become a commit message is
covered the moment it is written. Fixture names come from the fictional
roster (Simpsons universe + greek letters) —
`crates/jojobot-domain/tests/fixture_roster.rs` is the allowlist and CI
enforces it. An example quoted from the user's private docs gets a roster
substitution BEFORE it crosses; that quote path is the standing leak vector
and has burned this project three times. When in doubt, it doesn't cross.

## Where the design lives

- The **product roadmap & vision** and the **architecture doc** (bounded
  contexts, data model, decisions, as-built records) live in the user's private
  wiki, not in this repo. The coordinator session owns them and reconciles them
  after every slice.
- **Migration note:** while behaviour migrates, most operating context still
  lives in a private repo of the user's. Sessions on the user's machine may read
  it for orientation. **Nothing life-specific may cross back into this repo** —
  see the bright line below.

## The build model — two sessions, never one

- **Coordinator (`pm`)** — holds the vision, scopes each slice, writes
  hand-off tasks, reviews adversarially, reconciles the docs. Does not
  implement.
- **Implementer (`dev`)** — boots in this repo, builds ONE scoped slice
  test-first, reports back, and is disposed. If that's you: read your task
  from the **`dev` mailbox** over the jojobot MCP (`read_mailbox`). Work that
  one task. When done, `mark_processed(message, notes)` and post your report
  as a message to the **`pm`** mailbox. A failure the coordinator must know
  about is also a message to `pm` — failure is data, not silence.
- **Audience discipline — the user is not the report channel.** Two different
  readers: the **user** dispatched you; the **coordinator (`pm`)** reviews
  you. Detail — commits, deviations, test output, design notes — goes to the
  `pm` mailbox in full. The user gets the close-out only: done or blocked, in
  a line or two, plus "report posted to pm." Don't narrate the work to the
  user as you go; if something needs a decision mid-slice, ask it as one
  crisp question.
- Merging advances `main` only. **Deploy and push are the user's verbs** —
  never deploy; never push unless asked.

### Booting a session in this repo

A session here starts at jojobot, not at these files. The whole protocol:

1. **`start_here` with your bot name** — `dev` if you were not told otherwise.
   It hands back a `sid`; pass it on every call after that, reads included.
   It also hands back your charter and your rules, which say more about how
   you work than this file does.
2. **`read_mailbox`** — your task is in your box. Work it.
3. **Report to `pm`** as you go, not only at the end: `post_message` to the
   `pm` box at each coherent milestone, and `mark_processed` a task the
   moment you have acted on it.
4. **Keep polling.** When the work is done you are not done: `read_mailbox`
   with `counts_only: true` costs nothing and takes delivery of nothing.
   Poll, take whatever arrived, repeat. **A session that stops checking its
   box is indistinguishable from one that has died** — and the next task is
   usually already waiting.

The operator dispatches you; they are not the report channel (see the audience
discipline above). Everything else — commits, test output, deviations,
design notes, questions — goes to `pm`.

## The capability ladder

Milestones are **capabilities, never infrastructure** — each is named "after
this, I can ___".

> **This file and the README carry the roadmap status, so they are versioned
> with the code: any slice that changes what's true here — a milestone ships,
> a verb lands, config changes — updates both in the same round.** A stale
> "Status" section is a bug, not a nice-to-have.

Shipped and live:

- **M0** — skeleton: MCP over streamable HTTP behind OAuth2 resource-server auth.
- **M1** — Memory: typed entities (`kind:slug` handles) + dated facts with
  provenance (testimony vs inference); the write guard (nothing is created as
  a side effect; near-misses come back blocked-with-candidates); read-back on
  every write.
- **M2/M2.5/M2.8** — `search` across facts, entities and prose; structured
  edges (location · membership · attendance · about · connection); aliases;
  orienteering retrieval (every hit arrives with its surroundings).
  `connection` says a link is there and that how it relates was not recorded —
  an admission, never a weaker `about`, because filing an unknown link as a
  claim laundres it into one.

> **One front door, over both worlds.** Mail is in the same `search` — no
> second verb, one ranked list — and **opt-in**: `include_mail: true` reaches
> it. The reader who needs a filed finding is a later session that does not
> know it is there, so the flag is worth reaching for — but the safe branch is
> the default, and a session told to leave somebody's mail alone must be able
> to use the front door without knowing a flag exists. Every state is
> searchable,
> `processed` included, and the state rides on the hit beside the box, the
> sender and the id. Retrieval is the ONE place the two contexts meet, and it
> meets them as a reader: a `Hit` carries a `Message`, nothing writes across.
> **Every answer is backed by a read taken for it** — each half re-reads its
> own store before answering, so a record the store has since lost stops being
> served, which nothing inside the process can notice any other way. A half
> that could not take its read says so rather than vouching — so **each half of
> every answer states its own coverage**, because "no record says that" and
> "jojobot has read no records" are different claims and a caller acts on both.
> A half that is neither complete nor blind says which way it is behind:
> `unscanned` when no read has filled it yet, which the first read reaching the
> store ends, and `stale` when the index holds an older version than the store —
> a refresh that could not reach it, or, on the memory half alone, a committed
> write whose re-read failed.
> The token is what a caller branches on; the note beside it is what a reader
> reads. Being honest to the caller who wrote is not enough on its own — the
> sessions that read later are the ones with no other way to tell.
- **M3** — Mailboxes: message boxes (`new → read → processed`;
  read ≠ processed; processed is a terminal archive; no delete verbs — the
  tool surface is pinned by test). A message may carry a one-line `subject`
  and an `in_reply_to` link to the message it answers, and **`read_message`
  takes delivery of one by id** — draining a whole box makes every message in
  it owed work, which is the wrong price for wanting the single one a search
  hit named — **from its own box only**, the read side having no box argument
  so another bot's cannot be opened. The guard is on the state change rather
  than the bytes, so the terminal `processed` archive stays readable from
  anywhere: reading history moves nothing. **`list_sent` is the sender's own
  view**: where your mail got to and whether anyone has read it, read-only and
  moving nothing. **`read_mailbox` with `counts_only` is how you poll**:
  per-state counts and anything jojobot cannot read, taking delivery of none of
  it, so a poll that finds an empty box costs nothing and owes nothing. It was
  a verb of its own once — the surface grows by packing flexibility onto the
  verbs that exist, not by adding verbs.

> **Delivery-awareness: serve the difference.** `seen_before` was the first
> instance; the rule now runs across the surface. What a caller demonstrably
> already has is not shipped back to them — `post_message` and `mark_processed`
> answer with a receipt (id, state, notes, `body_bytes`, the opening line)
> rather than echoing a body its own author wrote, `read_mailbox`'s `new_only`
> stops re-shipping a deliberately held-open message on every poll, and
> `start_here` takes `brief` so a caller who does not need the orientation
> essay can skip it and withholds the charter from a resume, marked so that
> *withheld* cannot be read as *absent* — the charter is the largest block a
> boot carries, and the answer names the call that returns it rather than
> claiming anything about what its reader already holds, which is not something
> the engine checks. A session's chronology comes back as its newest entries
> under a character budget, sized on what an entry renders as rather than on
> its text alone; **both the boot and the wrap answer through that budget**,
> and the response states the full length and what it left out. And
> **mailbox counts are scoped to the caller** — the boxes a bot drains come back
> with their per-state counts, every other box by name only, so existence stays
> visible (a writer needs it) while somebody else's queue stops posing "is that
> one mine?".
> **Eliding is never silent**: whenever less comes back, a marker says what was
> left out and how to get it — or, where no verb serves it, that none does —
> because a reader who has to infer withheld-vs-empty
> from a missing key will eventually infer wrong. What the full echo *proved* is
> untouched — read-back happens server-side, so a body that did not survive
> storage is still an error rather than a success with mangled bytes.
- **M4** — Bots: a ninth entity kind, `bot` — an AI identity is handle ·
  charter (its doc's prose, written through `set_charter`) · rules (plain
  facts, so each carries its own provenance) · memory · one owned mailbox,
  opened with the bot in the same act and named for its handle.
  **`start_here` is the one orienting door** — the same verb with or without
  a bot: world-model and snapshot always, plus the identity when a bot is
  named. The door itself **mints no identity**: an unknown bot name comes back
  with the roster plus the offer to boot as a known bot and create the new one
  from there. The snapshot **names** every bot, so that offer is reachable from
  the door rather than only from a refusal you had to provoke — names only, a
  caller with no identity being one that is choosing rather than weighing.
  Anonymous boot gets orientation and no `sid` — an orientation
  preview, with nothing usable behind it. Ownership is stated **on the
  mailbox**, an `owner` field set once when the box opens, so there is no
  second copy anywhere to keep in step with it. It is not an ACL: a box names
  its one owner, and nothing on the mail rail enforces anything against it.
  A box a bot should have but does not — a record predating the rule, or a
  creation interrupted partway — is healed the moment that bot next boots, and
  the boot says so rather than repairing it silently. **A repair scoped to
  whoever boots cannot converge**, so the boot names every identity missing one
  and reports rather than mass-repairing: healing the bot in front of you
  completes an act somebody took, while opening boxes nobody named is a boot
  with side effects. A roster jojobot cannot read says so, rather than counting
  none.

> **Creation is an intentional act.** A box is not minted by a call of its
> own — it opens with the bot that owns it, inside the same `add_entity` that
> creates the bot, and the near-miss screen that guards it is the bot handle's,
> because the box is named for the handle and that is where the collision
> actually happens. Any future verb that would create something as a side
> effect of doing something else is the thing this rule exists to forbid.

- **M5** — Sessions: a bot is a **role**, a session is **one mortal run of it**
  — the unit of work, not of connection, so it survives a disconnect or a
  device hop. **One record per run, in the store**: it carries
  **state** (`active` → `wrapped` | `abandoned`; **`wrapped` is the last word,
  because wrapping folds the still-open focus into the closing story as one
  last chronology entry — `abandoned → active` is the one legal walk-back,
  since a run nobody wrapped up left nothing to fold**) and the
  **focus as current truth**, rewritten in place; the **chronology hangs off
  it**, one entry per beat (append-only, oldest first; only the newest
  entry amendable).
  **`start_here` is the start verb** — there is no `start_session`, because
  there is no moment between "I am gamma" and "gamma is working". Booting with
  a bot name hands back a `sid` immediately when there is nothing to resume,
  and otherwise hands back the resume-or-new choice and no `sid` — the `sid`
  arriving once the caller picks. Booting sweeps that bot's sessions that have
  gone `ABANDONED_AFTER` (24h) without a beat, **offers** any resumable one
  back as a choice, and otherwise begins one **lazily: no row until the first
  write**, so a boot that does nothing leaves nothing behind. A session records
  what it is working on, so the offer can tell two of them apart — and a bot
  may have several running at once, because the `sid` is what tells them apart.
  Nothing ever auto-wraps a session: a new one never closes an old one, and
  wrapping is initiated from inside, by the bot that owns it. `journal` records
  a beat and moves the focus, `amend_journal` fixes the newest one,
  `wrap_session` folds the still-open
  focus into the story as one final chronology entry and closes the row —
  publishing nowhere.
  jojobot writes **its own beats** too — one per verb class per session, count
  kept current, marked apart from what the session said about itself.
  Session records deliberately stay **out of the search index**, and so does
  any document the memory scan finds marked as jojobot's own machinery — a
  filter worth keeping whether or not jojobot still writes such documents,
  because what they hold is correspondence and chronologies rather than content.

> **Identity is the SESSION ID, because no real client holds a connection.**
> The binding was per-MCP-session and the design assumed a client keeps one
> across a conversation; none do — claude.ai and ChatGPT both open what jojobot
> sees as a fresh, unbound connection per tool call, so a boot bound an identity
> that was gone by the next request. **The `sid` is the only address**: the door
> hands one back, and it rides every verb, reads included, so jojobot always
> knows which bot is asking — reads are attributed, never journalled.
> `journal`/`amend_journal`/`wrap_session` take the `sid` and never a `bot`, and
> **a session is bound to its identity at boot and never switches**, so naming
> somebody else's session is refused rather than quietly honoured — a bug class
> deleted instead of guarded against. The connection binding is not demoted, it
> is gone. A no-affinity client is permanently in the test suite, because every
> other test holds a handler across calls and that is the shape no client has.
> And because the `sid` rides every verb, the automatic beats attribute for
> those clients too.

> **A literal journal, not a log.** High-level beats — what you set out to do,
> what you found, what you decided, what went wrong — never a firehose of tool
> calls. It is taught in the orientation and the tool descriptions and enforced
> **nowhere**, because it is a judgement about what is worth recording and no
> length check can make it.

Also shipped, and **not a capability**: the **alignment release**. A redesign
settled 2026-07-27 changed what the code should look like, and parts of what
ran were the previous mental model still running. The release removed them, in
order: entities gained a tree · sessions and mailboxes became rows on their
bot's child pages and Vikunja left jojobot entirely · wrap stopped publishing
and the shared journal went · the code reshuffle (one file per verb) · the last
raw error became a blocked answer · the trash got swept.

The **surface** is the redesign that followed: built from the catalogue of domain
actions harvested from real use, fewer verbs doing more through domain-level
parameters; the curated list lives in the README.

- **One identity ships.** A fresh instance arrives holding `assistant`, with its
  mailbox, seeded before anything serves. That is what closes the loop under
  **every memory write names its session** — creating the first bot is a write,
  a write needs a session id, and a session id comes from booting a bot. An
  empty jojobot is not an identity-less one.

- **Skills** — the method ships with the binary. The orientation essay is the
  world model and arrives unasked; a **skill** is a procedure and is fetched by
  name through the same door, because `start_here` is skill zero. Every boot
  lists the skills by name and when-to-use and **never their bodies**, so a
  session learns what exists without paying for what it does not need.
  jojobot decides nothing about when one applies — the caller asks.

> **The engine ships the procedure; the operator's instance personalizes it.**
> An override is an ordinary fact on the bot, read alongside the shipped text
> rather than replacing it: it narrows, it never repeals. That keeps the two
> halves under their own owners — software owns the general behaviour, data
> owns the personalization — instead of making a second copy of one truth.

The capabilities after these — events remembered where they happened · trace ·
portraits · attention · sessions booting from jojobot — are ordered, not
scheduled, in the roadmap.

**Layering: engine + bot.** The engine (this repo) is user-agnostic code; a
bot and its rules are *data* in the user's own store. Nothing about any
particular person is compiled in — and that now includes the skills, which
name roles and never an operator.

## Engineering rules (non-negotiable)

- **Strict TDD.** A feature is proven by an automated test; every bug fix
  starts from a failing test you watched fail FIRST. A manual run proves
  nothing.
- **Commits: one per coherent problem.** A milestone lands as a handful of
  commits — never one per file or checklist item, never dozens. **A fix and
  the test that proves it are ONE problem**, however a task listed them.
- **A commit message uses Simplified Technical English**, like every other text
  this project writes. Short sentences. Active voice. One word has one meaning.
  No metaphor.
- **The body states what the code does now and why the change is correct.** It
  does not state who asked for it, which message carried the request, what
  anyone believed before, what a run printed, or how many attempts it took.
  **Strike every sentence that would be false next month.** Those sentences are
  a *report*, and a report goes to the coordinator.
- **Cite a rule number when a rule governs the change.** Write no attribution
  when no rule applies: every commit here was asked for, so "the operator asked
  for this" carries nothing. Never cite a message id — mailbox traffic is
  ephemeral and a later reader cannot resolve it. Never write a pronoun for the
  operator; this repo names roles.
- **A diagnostic is not a test and is never committed.** Something run once to
  answer a question — a probe, a scan, a count — is a script. It guards
  nothing and will never fail meaningfully again. Run it, put the *answer*
  where the question lives, delete the instrument. Enforcing the answer later
  is a deliberate test with an assertion, and a different artifact.
- **Zero user PII, zero life specifics** — the bright line at the top of this
  file. It outranks every other rule here.
- **The real-dependency gate.** A slice that touches an adapter does not merge
  on fakes alone: run the real-dependency integration suites
  (`crates/jojobot-adapters/tests/`, against disposable stores; credentials
  come from `.env`) and show the output. A suite that exists but was never run
  is a blocker, not a footnote. Sourcing `.env` to *run* the suites is
  sanctioned; never print or copy its values.
- **Fakes must be hostile where reality is.** When a real store's quirk is
  discovered (normalization, silent drops, clamped pagination), bake it into
  the fake or a golden fixture — a polite fake that stores bytes verbatim is
  how green tests ship broken adapters.
- **Hexagonal, domain-driven.** `jojobot-domain` stays pure (no I/O, no MCP);
  each fronted service's quirks live in its adapter, quarantined.
- **Green bar before DONE:** `cargo test` green and `cargo clippy` clean, run
  through the flake (`nix develop -c cargo test`).
- **A test must travel the path it claims to exercise.** Calling a function
  directly proves the function, not that it is wired to anything. Assert that
  a value survives a journey by sending it on the journey, through the surface
  a caller uses — otherwise the assertion holds identically on a build where
  the value never moves.
- **Never pin our own prose.** An assertion quoting a sentence somebody wrote
  breaks when the sentence improves and proves nothing about behaviour. Pin
  identifiers and structure — things a rename would be a real change to. The
  tell is a needle that is a phrase rather than a name.
- **Don't over-engineer.** Reach for the simplest model that fits the stated
  design; the tell is a pass that keeps getting bigger. When the design and
  the code disagree, say so — don't silently deviate, and don't silently
  comply either.

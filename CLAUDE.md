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
roster — characters from the Simpsons, South Park, Family Guy or Bob's
Burgers, plus greek letters —
`crates/jojobot-domain/tests/fixture_roster.rs` is the allowlist and the
test bar enforces it. An example quoted from the user's private docs gets
a roster substitution BEFORE it crosses; that quote path is the standing
leak vector and has burned this project three times. When in doubt, it
doesn't cross.

## Where the design lives

- **The roadmap is the work-queue board**, not a document. Every release, rock
  and slice lives there; a release that exists only in prose is one nobody
  works. **The decision log** — every rule the operator has set, one line each —
  and **the brief** that renders it live in the operator's private wiki, not in
  this repo. The coordinator session owns all three and reconciles them after
  every slice. There is no architecture document: it was retired for being false
  about the code in a dozen places, and as-built detail comes from the repo.
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
  provenance — the operator said it, an agent read it in a named system of
  record, or an agent worked it out, and a machine read is **refused unless it
  names the system**. A claim carries **three dates**: the day it is true of,
  the moment jojobot took it in, and optionally the day its reading stops
  being good. A derived claim names what it was worked out from, and that
  pointer is **walkable both ways** — from a claim to its source, and from a
  source to everything built on it — settable by an edit, and never pointing
  at a claim that was taken back; the write guard (nothing is created as
  a side effect; near-misses come back blocked-with-candidates); read-back on
  every write, taken server-side — a write that did not survive storage is an
  error rather than a success with mangled bytes, and the caller gets the
  receipt rather than the evidence.
- **M2/M2.5/M2.8** — `search` across facts, entities and prose; structured
  edges (location · membership · attendance · about · connection); aliases;
  orienteering retrieval (every hit arrives with its surroundings).
  `connection` says a link is there and that how it relates was not recorded —
  an admission, never a weaker `about`, because filing an unknown link as a
  claim laundres it into one.
- **The graph query** — `recall` is the precise lookup and `search` is the
  breadth. `recall` selects objects (a handle · a kind · a declared type · a
  key and its value · **the name of a VIEW, which fills the rest of the call in
  from that view's own keys**), says what of each comes back (facts · prose ·
  one key's history), and walks to a depth in either direction. **A view is a
  record of kind `view`, so the one the operator declares and the one the build
  supplies are the same shape and nothing branches on which.** **The build
  supplies a charter and two views**, assembled in one place — a capability
  ships data by adding an entry there, with no table, migration or verb. **Two kinds of link:** an **edge**, followed
  by one of the five shapes; and a **relation**, a key some type declared to
  hold a reference, followed by that key's own name. **A relation is
  key-scoped** — it reaches everything using that key, so *which of those are
  pets* is a selection (a kind plus a key filter), never a walk: traversal
  reaches, selection chooses. **The answer nests and is always objects**, never
  a bare fact list, so a caller does not branch on which question it asked. It
  reads the store directly rather than the search index, which is what makes it
  the way past an index that cannot scan.
- **Edit-in-place is the surface; append-only is the substrate.** A write to a
  key appends rather than overwriting, and a read projects those writes down to
  one value — by default the newest, so a caller edits a claim and reads it back
  changed, exactly as before. **The same rows answer both questions:** what a
  key holds now, and every time it was written. That is why a count of how many times something
  happened and a list of the occasions are one body of data rather than two,
  and it is what the next several capabilities stand on.
- **Fields, and what declaring a type buys** — **a record carries fields**: a
  flat bag of key/value pairs beside the claim, where a key the caller invents is
  kept as written and a key some type declared is held to what that type says.
  **A thing's fields are every write on it, folded together, and HOW they fold
  is the key's own declaration** — the newest write wins by default, while a key
  declared a counter comes back as the total of its writes, which is what makes
  a running total a read rather than the caller's arithmetic. So a thing gains
  fields a piece at a time and an edit to any record reaches it. **Conformance is asked of the THING, across all its records,
  never of one record alone** — `answers_type` selects things carrying *some* of
  a type's keys and says which each one lacks, and `fits_type` keeps only the
  things with no gaps. **Which of the two you want is the reader's question**,
  and the tolerant one is what a caller gets when they name neither. **A
  declaration does not gate what a thing may BE**: a thing is found by the keys
  it carries whether or not anybody declared the type, matching stays
  structural, and a key no type mentions is kept as written. **A key may also be narrowed to ONE OF A NAMED
  SET, and a declaration that leaves a key nothing it could ever hold is
  refused when it is declared.** ⛔️ **A declared type refuses no write. It is
  the QUERY vocabulary — how a caller asks which things answer a shape — and
  the KIND is what governs what may be written.** **What a KIND asks for is a
  FLOOR, and the floor is its REQUIRED keys:** once a thing holds every
  required key of its kind, a write that would take one away is refused,
  naming the kind and the key, and a narrowed key is checked on that write. **An optional key is welcome, never demanded, and never decides whether
  a thing is complete.** **What a key HOLDS is checked whenever it is set,
  required and optional alike** — so a value that is not what its key declared
  does not count as holding it. **Adding keys is never refused, and a
  thing that answers no type is a first-class thing.** **The software ships two
  types of its own — `entitlement` and `trip` — declared at every startup, and it
  ships KINDS the same way — RECONCILED at every startup, which is the stronger
  word: the build's set is authoritative, so a kind an older build shipped and
  this one dropped is taken back rather than left behind.** **A shipped name is
  closed to a caller's redeclaration, and what makes that checkable is a mark on
  the row: a row the binary owns records its origin as shipped, and the guard
  reads that column rather than any list of protected names** — so a row added to
  the build is protected by being written, and no list can go stale. A caller
  never supplies that origin; only in-process code can. **AND THE GENERAL FORM,
  which is bigger than any one capability: a guard that consults the store to
  decide something must see what the build SUPPLIES as well.** A read that
  resolves supplied records while the guard beside it reads only stored ones is
  the two halves disagreeing about what exists — and the guard is the half that
  fails silently, letting a caller take a name the build already uses. **This
  holds for one guard today and nothing yet enforces the rest.** **`rhythm` is a shipped
  KIND and it declares every key its loop uses** — a name
  and the day of the last check-in are required; the cadence in days, the day
  the next cycle counts from, which of the two dates a late check-in advances
  from, the outcome and a note are optional. **A loop is a thing in its own
  right, so it carries its own events**, which keys folded onto another thing
  cannot. A walk carries its own filters, and a key's declared value type
  licenses comparison on it —
  before/after on a date, less/greater on a number, equals on anything.
  **A key filter asks about the THING by default** — its folded value, the same
  map the type question is asked of — and asks about one record only when a
  caller says so. The two are different questions: *which friends have eaten
  three donuts* is the thing's, and *which visits cost more than fifty* is the
  record's.
  **A declared reference names the kind it points at**, so a handle of another
  kind is a mistake the declaration can see — and on a thing already at the
  floor it is refused rather than reported, because holding a key badly is not
  holding it. Declaring buys write-time help, ordering and traversal on the keys
  it names, and that floor.

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

> **Delivery-awareness: serve the difference.** The rule runs across the whole
> surface, and `seen_before` on a delivery is one instance of it. What a caller
> demonstrably already has is not shipped back to them. **A write verb answers with a
> receipt, and the prose its author just sent is the one thing it leaves
> out** — `capture`, `update_fact`, `journal`, `amend_journal`,
> `wrap_session`, `set_charter`, `post_message` and `mark_processed` all
> answer with the id, the state and what jojobot changed on the way in
> (a qualified subject, a defaulted provenance, a trimmed value), plus the
> byte count of what was STORED and enough of the opening to tell one write
> from another. Eliding is never silent: each answer names the call that
> returns the whole thing. **`post_message` also takes
> delivery of the caller's own box in the same call**, and the answer says how
> that delivery was taken: writing is a moment a bot is demonstrably present, so
> reading and posting are one round trip rather than two, and a bot that posts at
> the end of a piece of work does not meet its own mail afterwards flagged as
> something it has already seen. `read_mailbox`'s `new_only`
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
- **M4** — Bots: the `bot` entity kind — an AI identity is handle ·
  charter · rules (plain facts, so each carries its own provenance) ·
  memory · one owned mailbox, opened with the bot in the same act and named
  for its handle. **A charter has two layers, and neither is a copy of the
  other**: the text the build supplies for the identity the software ships,
  and the instance's own prose written through `set_charter`. **No verb
  composes them.** The build supplies its text at an address and the store
  resolves it underneath every read, so a caller is never told a shipped
  half exists and no reader carries a word for it — the boot door and
  `recall` both simply read prose. Reading a colleague's charter mints no
  session and hands back no handle, which is what makes a directory read
  possible without booting as the bot it belongs to. **So a bot nobody has
  written for still answers with what the build supplies**, while every
  other bot's charter is its written prose alone. The build's half is never
  stored: upgrading it is shipping a new build, with nothing to migrate and
  no instance frozen on the version that made it. **A write carrying it is
  refused rather than trimmed**, because a caller that reads a charter and
  sends it back would store the build's words as the instance's, where they
  stop moving when the software does. **The refusal names no core** — it
  says the text repeats what the software already says here, and to send
  only what is being added.
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
  Session records are **in the search index, scoped by owner**: a bot finds its
  own runs and nobody else's, and they rank below everything else because a run
  is context rather than an answer. A caller that names no bot gets no session
  hit at all. Any document the memory scan finds marked as jojobot's own
  machinery still stays out — a filter worth keeping whether or not jojobot
  still writes such documents, because what they hold is correspondence and
  chronologies rather than content.

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
scheduled, on the work-queue board.

**Layering: engine + bot.** The engine (this repo) is user-agnostic code; a
bot and its rules are *data* in the user's own store. Nothing about any
particular person is compiled in. The skills obey the same line: they name
roles and never an operator.

## Engineering rules (non-negotiable)

- **Strict TDD.** A feature is proven by an automated test; every bug fix
  starts from a failing test you watched fail FIRST. A manual run proves
  nothing.
- **To watch a test fail, use `scripts/sabotage`.** It keeps the file, makes
  the edit, runs the command, prints the verdict and puts the file back. **The
  restore is on a trap, so it happens on a pass, a failure, a raise or a
  stop** — there is no dirty tree to clean up afterwards and no moment where a
  git verb is the convenient answer. **It refuses unless the text appears
  exactly once**, and it reports an absence and an ambiguity apart, because
  those send you to different places. ⚠️ **A hard kill cannot be caught, so it
  names the copy on stdout when it starts.**
- **A sabotage proves nothing until you know it landed where you meant.** Two
  sites that look alike, one edit, and the verdict is about code nobody
  touched. **Assert the edit reached THAT site**, and prove the case moves when
  its own guard breaks and stays still when an independent one does. ⚠️
  **Choosing an independent neighbour is the hard part — reasoning about which
  one is independent is not enough. Run it.**
- 🚨 **A room runs against a BUILT binary, so a sabotage of served code is
  invisible to it unless you build first.** The rooms spawn the server, and
  `cargo test -p jojobot-exercise` does not rebuild it — **so an edit to any
  served source, checked only through the rooms, reads GREEN because the code
  under test was never loaded.** That is the strongest possible wrong reason
  for a pass. **Build the workspace in the same command as the run.** ⚠️ **And
  the same trap reads the other way: a REFUSAL you did not expect may be the
  old binary publishing the old schema rather than a second copy of a rule.**
- ⚠️ **A verdict comes from the exit code of the command you ran, so do not
  hand a pipeline to something that reports one.** A run piped through `grep`
  reports `grep`'s exit code, and the tool faithfully repeats it — **green,
  with the failing names printed directly above it.** The instrument is not at
  fault and should not second-guess what it was given. **Drop the pipe.**
- **Inserting code directly above an attribute orphans it onto what you
  inserted.** A `#[test]` or `#[cfg(test)]` line binds to the item below it, so
  a new item slipped in between takes the attribute and the old one loses it.
  ⚠️ **One instance ran a case TWICE while every suite stayed green** — a
  duplicated attribute is not a failure, it is a count nobody reads. **The
  linter finds these and the suite cannot.** Insert below the attribute, or
  read the two lines above your edit before you leave it.
- **Every feature appears in a user story.** A feature that no story exercises
  is a finding. This is a second bar, not the same one: a unit test proves the
  feature works, and a story proves the feature can be reached through the
  served surface. A capability can hold full unit coverage and still be
  invisible to every session that did not already know it was there. **A call
  with no assertion is not coverage** — a beat asserts what came back, what
  changed, or what a later read returns.
- **And a third bar, which neither of the first two can reach: whether a session
  that was told nothing FINDS the capability.** A story is written by somebody
  who already knows the answer, so it can only prove a capability is reachable —
  never that it was reached. A **playbook** is what a real model is driven
  through against a throwaway instance, and its assertions are over what the
  model LEFT in the store, never over what it said. `make paid` takes the
  playbook as a parameter, and the playbooks are **rooms**, in
  `crates/jojobot-exercise/rooms/`. **A run is KEPT** — written whole to a file
  the run names, before its exit code is decided, because a run that failed its
  expectations is the one most worth reading. `TRANSCRIPT=` puts it elsewhere.
  **A run that produced nothing says so in its own words**, so an empty file can
  only mean the capture never wrote — a different fault with a different fix.
  ⚠️ **This is what makes a run something to judge by after the fact rather
  than something somebody had to be watching**, and the judgement stays a
  person's: no score, no rubric, no summary. A room gives one goal in the operator's
  voice and a one-line entry, then leaves the agent to find the door, the box
  and the route. **A room is its own document: its starting world and its
  checks are written there.** A check that cannot be asked of jojobot is an
  escape into Rust, and a run prints how many escapes a room took, including
  none. It is in addition to the stories, never instead of them.
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
  on fakes alone: run the suites that exercise a real store and show the
  output. **They need no credentials and no separate gate** — every store
  jojobot fronts is a process it spawns itself, against a disposable database,
  so the suites are ordinary cases the whole run already covers. **That makes
  the gate cheap, which removes the last excuse for skipping it**: a suite that
  exists and was never run is a blocker, not a footnote.
- **Fakes must be hostile where reality is.** When a real store's quirk is
  discovered (normalization, silent drops, clamped pagination), bake it into
  the fake or a golden fixture — a polite fake that stores bytes verbatim is
  how green tests ship broken adapters.
- **Hexagonal, domain-driven.** `jojobot-domain` stays pure (no I/O, no MCP);
  each fronted service's quirks live in its adapter, quarantined.
- **Green bar before DONE:** `cargo test` green and `cargo clippy` clean, run
  through the flake (`nix develop -c cargo test`). That is `make check`, and it
  is free.
- **`make paid` is a third tier and `make check` never runs it.** It reaches the
  network, drives a real model through a playbook against an instance it spawns
  and throws away, and it costs money. **It must never run by accident**, which
  is why it stands apart rather than hiding behind an ignore marker. It needs
  the agent CLI on the PATH; this repo does not provision one.
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

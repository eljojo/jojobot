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

**One server instance, the only writer — so every read comes from memory.**
jojobot runs as exactly one process, and every write passes through it. That
is why the whole fold is held in memory and every read is answered from it
alone; each write updates the store and the fold in the same step, so the
fold is always current. **No read goes to the store.** ⚠️ A second writer of
any kind — another worker, a restore, a migration tool, a person running SQL
— makes the memory copy silently wrong. (Decision log 292.)

## ⚠️ THE BRIGHT LINE — zero user PII, zero life specifics. Ever.

**This repo is public. Nothing that identifies a user's life enters it —
no real people, places, events, organizations, festivals, trips —
anywhere:** code, tests, fixtures, docs, commit messages, branch names,
error strings. The line binds text *bound for* the repo, not just files
inside it: a hand-off task, an example, a report that will become a commit
message is covered the moment it is written. Fixture names come from the
fictional roster — characters from the Simpsons, South Park, Family Guy or
Bob's Burgers, plus greek letters —
`crates/jojobot-domain/tests/fixture_roster.rs` is the allowlist and the
test bar enforces it. The near-miss screen matches on containment, a known
nickname, and two or more shared rare words, so a new handle is refused when
an existing one is a prefix of it — qualifying a name already on the roster is
the way to trip it, and so is a second fixture sharing two words with the first
("Grant boot-as-patty" beside "Grant boot-as-nelson"); lift it with the token
and a reason, as the engine's own records do. Read the guard's own
answer instead of guessing at a rename. An example quoted from the user's
private docs gets a roster substitution BEFORE it crosses; that quote path
is the standing leak vector and has burned this project three times. When
in doubt, it doesn't cross.

## Where the design lives

- **The roadmap is the work-queue board**, not a document. Every release, rock
  and slice lives there; a release that exists only in prose is one nobody
  works. **The decision log** — every rule the operator has set, one claim each —
  **lives in jojobot itself**, under its decision-log topic, each rule on the
  rules topic of its subject; read it with `recall`. **The product brief** that
  renders it lives in the operator's private wiki, not in this repo. **The per-feature briefs DO live here, at
  `pm/feature-briefs/`** — one file per feature, saying what it is for, why it is
  shaped that way, and what it deliberately is not. **Read the brief for a
  feature before you reason about that feature**: it usually already answers what
  looks like a fresh design finding. The coordinator session owns all of them and
  reconciles them after every slice. There is no architecture document: it was
  retired for being false about the code in a dozen places, and as-built detail
  comes from the repo.
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
  never deploy; never push unless asked. The one exception: a dev line pushes
  its own branches to Forgejo (see "A dev line manages its own git" below).

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
   Poll, take whatever arrived, repeat, until your bot's own rules say when to
   stop. **A session that stops checking its
   box is indistinguishable from one that has died** — and the next task is
   usually already waiting.

The operator dispatches you; they are not the report channel (see the audience
discipline above). Everything else — commits, test output, deviations,
design notes, questions — goes to `pm`.

## The capability ladder

Milestones are **capabilities, never infrastructure** — each is named "after
this, I can ___".

> **This file and the README carry the roadmap status, so they are versioned
> with the code: any slice that changes what's true here — a milestone ships, a
> verb lands, config changes — updates both in the same round.** A stale
> "Status" section is a bug, not a nice-to-have.
>
> **They carry it at different altitudes.** This file is the detail. The README
> has one row per feature marked works, partly or not built, measured against
> the code on `main` — so a slice that moves a feature between those three
> states moves its row. **The README is for a person making sense of the
> project**, is held to about five minutes of reading, and carries no
> instructions for running the software: it is not meant to be run by anyone
> yet.

Shipped and live:

- **M0** — skeleton: MCP over streamable HTTP behind OAuth2 resource-server auth. On SIGINT or SIGTERM it stops taking calls, gives the calls in flight up to ten seconds to finish, then cuts the rest and exits 0, well inside a service manager's stop timeout.
- **M1** — Memory: typed entities (`kind:slug` handles) + dated facts with
  provenance — the operator said it, an agent read it in a named system of
  record, or an agent worked it out, and a machine read is **refused unless it
  names the system**. **Every write records the session that made it, and
  testimony is rewritten in place only by the session of its first write**; a
  claim with no recorded session counts as written by none, and its value
  fields are held to the same rule; sending the claim's own words and values
  back unchanged is not a rewrite, and a key the caller may not write is
  refused for that first. The refusal names
  the route — archive the claim, then capture the correction with
  `derived_from` — and says the original stays readable. Inference and
  observation are still rewritten in place. **Settling an open claim takes a
  receipt** — a check another agent can run again, or the day and place the
  operator confirmed it — and changes standing, never provenance: a settled
  inference reads back as an inference with its receipt, and
  `confirmed_by_user` only moves a claim to testimony. A claim carries **three dates**: the day it is true of,
  the moment jojobot took it in, and optionally the day its reading stops
  being good. A derived claim names what it was worked out from, and that
  pointer is **walkable both ways** — from a claim to its source, and from a
  source to everything built on it — settable by an edit, and never pointing
  at a claim that was taken back; the write guard (nothing is created as
  a side effect; near-misses come back blocked-with-candidates); read-back on
  every write, taken server-side — a write that did not survive storage is an
  error rather than a success with mangled bytes, and the caller gets the
  receipt rather than the evidence. **A write may carry a `request_id`**, so a
  resend whose answer was lost is safe: `capture`, `add_entity` (not a bot),
  `update_entity`, `update_fact` (not a section move), `archive_entity`,
  `retract`, `merge_entities`, `set_charter`, `declare_type`, `post_message` and
  `mark_processed` take one. It is spent in the write's own transaction, a
  repeat answers with the first receipt marked `repeat` and writes nothing, a
  call that wrote nothing spends none, and ids are kept 72 hours; a repeat whose
  receipt was not kept says the write landed and where.
- **A refusal says who fixes it in one word**, `fix_by`, before the prose:
  `retry` (send the same call again), `change` (the call itself must change,
  and the refusal names what) or `person` (stored damage, or a decision only a
  person can make). **A write the store refused and a store that cannot be
  reached are told apart**, on the memory, session, mailbox and teaching rails
  alike: a write the store's constraints refused wears `change`, and a store
  that could not be reached is an outage and wears `retry`.
- **M2/M2.5/M2.8** — `search` across facts, entities and prose; structured
  edges (location · membership · attendance · about · connection); aliases;
  orienteering retrieval (every hit arrives with its surroundings). **An
  archived claim comes back by default, ranked below the claims that stand**,
  a superseded one naming what replaced it; `status: active` narrows to those
  that stand. **A record's details are left out by default** on `recall` and
  `search`: each answer says once, under `details_left_out`, how many it left
  out, and `details: true` brings them back; a search hit on a document
  section shows its heading only.
  `connection` says a link is there and that how it relates was not recorded —
  an admission, never a weaker `about`, because filing an unknown link as a
  claim launders it into one. A walk says when the claim behind a link was
  taken back — marked rather than filtered, the way a fact read shows a
  retracted claim instead of hiding it; a link two records draw stays
  unmarked while either of them stands. **A field whose value is another
  thing's handle is a link too, whatever its key**, and so is a comma-joined
  list when every item in it is a handle; the key is the relation's name. **So a
  handle that names nothing is refused under any key**, with candidates, as an
  edge's object is; a handle inside a sentence stays prose.
- **The graph query** — `recall` is the precise lookup and `search` is the
  breadth. `recall` selects objects (a handle · a kind · a declared type · a key
  and its value, or **a value with no key, which asks whether ANY key holds it**
  — a filter naming neither is refused · **the name of a VIEW, which fills the
  rest of the call in from that view's own keys** · **a RECORD'S ADDRESS, which
  selects on its own: the address names its subject, so the call fills the rest
  in from it, and a subject naming something ELSE is refused rather than
  reconciled** · **`records`, a list of addresses on any subjects, each served
  as it stands now** — an address that names nothing is named in its place, and
  nothing else may select beside it · **a `parent`, which selects that thing's direct children**),
  says what of each comes back
  (facts · prose · the history of one key OR one claim · **`as_of`, the day of
  the newest write behind what each object holds, with `fields_as_of` for any
  key whose day differs** · **`keys`, which narrows
  each object's fields to the keys named**, walked objects included, while the
  answer says once, in `narrowed`, what it left out and how to read it — per
  object in `fields_left_out` only when a view's `shows_keys` supplied the
  keys), and
  walks to a depth in either direction. **A view carries its own list as
  `shows_keys`**, and a caller's `keys` wins over it.
  **`where` says which objects in one expression** — `=`, `!=`, the four
  orderings, `in (…)`, `contains`, a bare key, `!`, `&&`, `||` and parentheses,
  over values that may be a day relative to the asking session's own
  (`today+3d`, `today-2w`). An ordering needs a type declaring what the key
  holds; what it leaves out for want of one is counted in
  `left_out_unlicensed`, and a key nothing holds is refused, as is an
  expression over 4096 characters or nested past 32 levels. **Each refusal
  names the way forward**: a key only the records hold asks for `scope:
  record`, an ordering no declaration licenses names the declaration to make,
  and an `order` on a key nothing holds, a `scope` with no `where` and a view
  whose stored `where` cannot be read are refused rather than answered. **`order`** sorts
  before the answer is paged, and **`scope`** asks the expression of the thing
  or of its records. **A view may hold `where`, `order` and `scope`**, its
  `where` stored as an expression that is checked when it is written.
  **A view is a record of kind `view`, so the one the operator declares and the
  one the build supplies are the same shape and nothing branches on which.**
  **A view takes typed inputs** (`inputs`, each `name: type`, optional with a
  default or required) bound into its own keys, so one view answers for a named
  project, day or type — on `recall` and on its page alike. **A view that,
  with what the caller sent, narrows nothing is refused when asked**, naming
  the keys that would narrow it, and its page answers the same; a view asking
  `ready` or `plan` narrows to work on its own. An overdue read that left
  things out says why, by reason and kind (`not_due_yet`, `not_yet_opened`,
  `finished` — done or ended — and `no_schedule`), on `recall` and on the page.
  **The build supplies a charter and seven views** — `colleagues`, `loops`,
  `ready`, which lists unfinished work whose every dependency is finished and
  that waits on nothing — work already under way included, archived work never,
  and the page is cut after readiness so blocked work does not use it up — and,
  for a named piece of work, says what blocks it; `plan`, every unfinished piece
  with whether it can start and what stops it; `board`, one project's work
  grouped by status; `due`, what falls due by a day and what is already late;
  and `of-type`, the things that answer a declared type — assembled in one
  place: a capability ships data by adding an entry there, with no table,
  migration or verb. **Two kinds of link:** an **edge**, followed by one of the five shapes;
  and a **relation**, a key some type declared to hold a reference, followed by
  that key's own name. **A relation is key-scoped** — it reaches everything
  using that key, so *which of those are pets* is a selection (a kind plus a key
  filter), never a walk: traversal reaches, selection chooses. **The answer
  nests and is always objects**, never a bare fact list, so a caller does not
  branch on which question it asked. It never reads the
  search index — it answers from the fold in memory, like every read — which is
  what makes it the way past an index that cannot scan.
- **Edit-in-place is the surface; append-only is the substrate.** A write
  appends rather than overwriting, and a read projects those writes down to one
  — by default the newest, so a caller edits a claim and reads it back changed,
  exactly as before. **This holds for a claim's own words and for the keys
  beside them:** a key's writes project to what it holds now, and a claim's
  writes project to the claim, each write carrying the whole of it. **The same
  rows answer both questions:** what is true now, and every time it was written.
  That is why a count of how many times something happened and a list of the
  occasions are one body of data rather than two — and why a claim corrected to
  say the opposite can still be told from one that always said it, which a row
  rewritten in place cannot. **A thing's own row keeps its versions too**:
  `recall` with `history_record` on a bare handle lists them, each with its
  moment, run and keys; and a claim's history carries, per write, the links and
  marks it held, shown as the handles they answer to now.
  **A claim's writes carry TWO moments and they answer different questions:**
  when the claim first entered the record, which every write repeats unchanged,
  and **when each write itself happened**, stamped by the store at the append.
  ⛔️ **Conflating them gives back the wrong reading the second one exists to
  end.** **A write appended before that column existed has no moment of its own
  and reports as absent rather than borrowing the claim's.** **The surface says so at the moment of the write**,
  because a capability a caller does not know about does not change what the
  caller does.
  **An edit may name the revision it read** (`expected_revision`, the count of
  the claim's writes) and is refused with retry, naming the current one, if the
  claim moved since; the check is a read inside the write, and the store's own
  conflict refuses the second of two simultaneous edits. **An edit may change
  part of a claim's text** with `find` and `replace` on a named `field`: a match
  that is not exactly one is refused, and the receipt says what was removed and
  inserted. The find matches the text as it is served; only the replacement is
  read for mentions, and a find that starts or ends inside a link is refused.
- **A handle written into text is a link, not a spelling.** A caller writes
  `@kind:slug` into a claim's words, its nuance or a thing's prose; jojobot
  stores the permanent id that row wears and serves back the handle the thing
  answers to now. **So renaming, reparenting or retyping a thing changes what
  every past mention reads as, and no stored text is rewritten.** **A mention
  naming something absent is refused with candidates**, the rule an edge's
  object already faces — ⚠️ **except on a thing's prose, whose answer has no
  blocked shape to give.** The two unresolvable shapes are stored differently so
  a reader can tell a pointer whose thing is gone from words that never named
  one. ✅ **`rename_entity` moves a handle** — a slug, a kind or a parent — **and
  the id underneath does not move**, so everything above is what makes the verb
  safe rather than a promise waiting on one. **An archive is reversible:**
  `archive_entity` with `restore: true` brings the thing back and writes one
  claim saying when and why it was archived, and why it returned. **A thing
  folded into another by `merge_entities` keeps answering at its handle, says
  where it went, and leaves every default browse.** **A merge moves no stored
  row**: it appends one merge event, and every read serves the folded thing's
  claims, links, pointers and children as the survivor's. An address minted
  before the merge still reaches its claim, a chain of merges is followed to
  its end, and a loop is stored damage (`fix_by: person`). **No name moves
  either**: the survivor lists the folded names as `foldedNames`, beside
  `alternateName`, and the near-miss screen and search treat them as names it
  answers to. With `facts`, a survivor names under `claims_left_out` any claim
  under a folded key the event does not map. **A rename leaves the display
  name as it was** (the receipt says so when the name no longer spells the
  slug), keeps every forwarding row current in the same step, and is refused
  for a record the build supplies. 🚨 **EVERY surface that holds a
  handle in text is a link, not only a claim's** — **a journal beat and a mailbox
  message included** — **and text written before that was true is MIGRATED once
  onto permanent ids, so a rename reaches it.** ⛔️ **A surface that keeps naming
  the old handle is a defect to close, never a limit to document.** **Text
  inside code is literal:** a handle in a backtick span or a fenced block is
  never linked, resolved or refused.
- **Everything that points at a thing stores its permanent id.** Not only a
  handle written into text: an edge's object, the entries in a record's
  reference list, a child's parent, and **a field value that names a thing — a
  handle or a comma list of handles, under any key, declared or not** — all
  hold the id rather than the name. **Who points at a thing is answered from a
  table of links** that each field write keeps in the same transaction, never
  by matching values.
  **The store resolves a handle to an id before the row lands, and renders the
  handle again on the way out**, so a caller still writes and reads names.
  ⭐ **This is what makes a rename safe everywhere rather than in text alone.**
  **It also removes a whole class: a name that a rename frees can be taken by a
  new thing, and no pointer follows it**, because no pointer holds a name.
  🚨 **A pointer at a thing that does not exist is REFUSED, never stored.** The
  refusal names the way forward, which is the verb that creates the thing first.
  **Rows written before this are migrated once, at startup; a value that
  resolves through neither a current handle nor a former one stops the migration
  and is named rather than guessed at.**
- **Fields, and what declaring a type buys** — **a record carries fields**: a
  flat bag of key/value pairs beside the claim, where a key the caller invents is
  kept as written and a key some type declared is held to what that type says.
  **A thing's fields are every write on it, folded together, and HOW they fold
  is the key's own declaration** — the newest write wins by default, while a key
  declared a counter comes back as the total of its writes, which is what makes
  a running total a read rather than the caller's arithmetic. **A third fold is
  no fold: a key declared as DESCRIBING its record stays on the claim and never
  reaches the thing**, so a rule's star stays on the rule rather than on its
  bot. Only a shipped type may declare it, and a described key may not be
  required. So a thing gains
  fields a piece at a time and an edit to any record reaches it. **A new thing
  may take its first fields in the call that creates it** — `add_entity`'s
  `sets`, written as its first claim and held to every guard a capture faces.
  **A record carries two bags**: its own fields, and what it `sets` on its
  thing — `capture`, `update_fact` and `update_entity` take `sets` too, so one
  call records a claim and sets the thing; a key stays in the bag it was first
  written under, and `update_fact` naming in `sets` a key the record holds as
  its own moves it in one call. **Every read shows the two bags apart** — a
  record's `fields` and its `sets` — on recall, search hits, receipts and the
  web page. **Only the setting bag and the keys jojobot computes
  reach the thing; a key written under `fields` stays on the record** (the
  class `stays`). A row written before that keeps reaching its thing, and no
  boot rewrites a class. The receipt names which keys reached the thing and
  which stayed, from the class the store kept, and hints once when an own field
  that the kind declares stayed. Archiving or retracting a record takes off the
  thing the keys it set, and the receipt names them under `left_the_thing`.
  **Conformance is asked of the THING, across all its records,
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
  a thing is complete.** **A kind may also carry a rule over its keys taken
  together, and a rhythm does: a write that leaves a cadence without the key
  saying when the next cycle counts from, or the reverse, is refused, so a
  loop is whole from the moment it is made.** **Some keys on a bot are
  ceilings or grants it cannot write about itself** — its thought room, how
  many runs an untouched thought counts against it (`thought_ages_after_runs`),
  its boot seats and the role it claims — so a different identity sets them.
  A thought untouched past that many runs is quiet: it stays, outside the count,
  and touching it brings it back, which is refused when the room is full.
  **On a thread, the room and its ageing bind the bots that write into it**, so
  only a bot above one of those writers sets them, and a writer never does.
  **A thread holds three live records by default** (an active record filed on
  it); `record_capacity`, a whole number of one or more set by a bot above its
  writers, replaces that number. A capture, a merge or a retype into the thread
  kind that would pass it is refused with nothing written, listing the live
  records and naming four routes (rewrite, archive, split, document); one
  borrow lands one record over, once, and defrag's review mark takes no slot. **Who a
  bot reports to is changed only by a bot above it on the chart**, a bot with no
  manager is adopted only by the manager named or a bot above that manager, and **the head of a chart is
  given a manager by no bot**. One table of the build's own says who may write
  each guarded key, and a refusal names who may. **What a key HOLDS is checked whenever it is set,
  required and optional alike** — so a value that is not what its key declared
  does not count as holding it. **A key declared a list can be changed by
  item**: `capture` and `update_fact` take `sets_add` and `sets_remove` beside
  `sets`, read the current value under the thing's lock, remove then append
  (survivors keep their place, an item held is not added twice, removing one
  not held is no error), and store the whole value; a removed reference is
  followed to the thing it names now, and an added former handle is refused
  naming the current one. **Adding keys is never refused, and a
  thing that answers no type is a first-class thing.** **The software ships six
  types of its own — `entitlement`, `trip`, `runs-out`, `decide-by`,
  `computed-dates` (the optional dates `due_on`, `last_check_in` and
  `counts_from`), and `record-labels`, which declares the six describing keys (`read_from`,
  `read_ref`, `starred`, `subject`, `purpose`, `recorded_by`) — declared at every startup, and it
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
  fails silently, letting a caller take a name the build already uses.
  **The EXISTENCE gate honours it, across both stores: a store is told what the
  build supplies over it, and its gate reads the rows plus that set with a
  stored row winning — asking the same resolver the read layer asks, so it holds
  no inventory.** So a claim may name or point at a supplied record. **A write to one lands on
  top of what the build ships** — a claim, a `sets`-only `update_entity` — and
  is found by search by its words and keys; its name and aliases take an
  overlay that reads as the operator's content, while an edit of its source or
  crm is refused with no token, and rename, archive and merge are refused. **An upgrade never stops the server or strands those writes (rule
  508):** a stored row at a handle a new build starts to supply wins at boot,
  with a log line, and writes on a handle the build stops supplying are given a
  row at boot — the kind from the handle, the name its slug.
  **The CREATION screen reads that wider set too**, so a near-miss against a
  supplied record is caught exactly as one against a stored row is. ⭐ **The
  question this was held open on is answered by the override the guard already
  mints: a near-miss is BLOCKED WITH CANDIDATES AND A TOKEN, never refused — so
  a declaration that should stand still stands, by re-calling with the token.**
  **A resemblance by name alone** — the same name under another kind, one name
  inside the other, a known nickname of a thing of the same kind, or two or more
  shared rare words — takes the token plus `override_reason`, a reason that
  tells the two apart, kept as `told_apart_by` and shown on the refusals and
  search hits where both appear.
  **Only an exact handle collision is unforceable, and that is the shipped-row
  rule rather than the screen.** ⚠️ **Every other such guard is still
  unenforced, and only the doubled contract makes that mechanical.** **`rhythm` is a shipped
  KIND and it declares every key its loop uses** — a name
  and the day of the last check-in are required; the cadence in days, the day
  the next cycle counts from, which of the two dates a late check-in advances
  from, the outcome, a note and the day a snooze lasts until are optional.
  **`work` and `project` ship their keys the same way**: a status out of
  someday, next, now, waiting and done (a project may list its own `columns`,
  which must include those five), an owner, what a piece of work waits on and
  depends on, the commit that landed it and what verified it. **A loop is a thing in its own
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
- **Documents** — a `document` is a kind whose content is its sections, one
  claim each. `put_document` splits markdown at its headings: a frontmatter
  block stays whole as the first section, a section too long for a record is cut
  at a blank line and never inside a fenced block, and a put onto a document
  that already has sections names `after` or is refused. `read_document` returns
  the live sections joined byte for byte, paged by `offset`. The order of
  sections is the server's to mint, and `update_fact` with `after` moves one
  section behind another. A search hit on a section carries its document and is
  grouped with that document's other hits. **A document reads as it stood on a
  day** (`read_document` with `as_of`) **and is put back as it stood at a
  moment** (`put_document` with `restore_to`), by writing that old version on
  top — nothing is erased, a second restore to the same moment writes nothing,
  and a restore that would change testimony is refused whole unless
  `confirmed_by_user`. **A document's page shows it rendered from markdown**
  above its facts, joined as `read_document` joins it: raw HTML stays text, a
  link goes only to http, https, mailto, an anchor or a relative path, and no
  image is drawn. **A merge of two documents leaves one**: the folded sections
  follow the survivor's last, in their own order, each long section whole; a
  document merged with anything else is refused with the route. Not built yet:
  frontmatter read as fields.
- **Lists** — a `list` is a kind that keeps ordered entries on the thing it
  is about: a claim captured on a list is an entry, placed by the server
  (`capture` with `after`: an entry's address, or `first`; absent is the end)
  and read back in order on `recall`. **A tick is its own record on the list**
  (`capture` with `tick: {entry, act}`, act `done`, `undone`, `rotated` or
  `dropped`), naming the entry and the session, and it never edits the entry;
  the list's `tick` setting says what done does: `sink` keeps a done entry at
  the bottom in done order, `leave` drops it from the open view, `back` takes
  the turn and moves it to the end still open (absent reads as sink). A dropped
  entry never reads as done. `recall` with `ticks` reads the ticks, newest
  first, in parts, and a list read names what it left out under
  `entries_left_out`. **Merging two lists places the folded list's entries
  after the survivor's last**, each list in its own order, under both lists'
  placement locks, and ticks and entry addresses survive the merge.
  **`update_fact` with `after` moves an open entry** first, last or behind
  another open entry, placed against the order the list reads now, and the
  move holds until the entry is next ticked. An entry whose edge points at a
  piece of work reads that work's status, owner, readiness and blockers under
  `linked` (`ready::verdicts_of`, stored nowhere), and a tick's receipt says
  under `ticked` whether the work is still open; a tick never changes the
  work. **A list that sets `covers` is a plan** for one day (`YYYY-MM-DD`) or
  one ISO week (`YYYY-Www`, Monday to Sunday): its read adds `plan`, the
  period's `from` and `to` and what is `due`, `overdue` and `ready` in it,
  worked out at the read by the due and ready views' own functions and never
  stored. Carrying an entry forward is a `capture` on the next plan with
  `derived_from` naming it: the new entry reads `carried_from`, the old one
  `carried_to`, and the old entry keeps its words and its state.
- **Numbered collections** — a thing carrying `numbers_by` numbers its
  descendants by that key: each holds a whole number unique across the
  subtree, archived members included, so a number is never reused. `recall`
  of the collection says `next_number` (the largest held plus one; jojobot
  keeps the count, never the writer), a write repeating a number is refused
  naming its holder, and `<collection>/<N>` is a handle wherever one is read,
  written to or pointed at — never one a thing is made or renamed to. A miss
  on a number offers the numbers held. **A number stays with its member
  through every verb**: a change of number, a move out, a merge, a declaration
  that changes how the key folds, and re-keying or dropping `numbers_by` are
  refused while they would leave a number unheld or held by two, naming the
  member, each with advice for the verb that asked; `numbers_by` is taken only
  by a subtree that already keeps the promise (`MembersBreakNumbering`). Only a
  retraction frees a number.

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
> **Every answer comes from what this process holds** — the memory half from
> the fold in memory, the mail half from its own index of the boxes, which every
> verb that changes a message keeps current; the boxes are listed again only
> when that index cannot tell what changed. So a record removed or edited outside
> this process is seen only after a restart (the one-writer rule above). A half
> that could not take its read says so rather than vouching — so **each half of
> every answer states its own coverage**, because "no record says that" and
> "jojobot has read no records" are different claims and a caller acts on both.
> A half that is neither complete nor blind says which way it is behind:
> `unscanned` when no read has filled it yet, which the first read reaching the
> store ends, and `stale` when the index holds an older version than the store —
> a refresh that could not reach it, or a committed write whose re-read
> failed — on the memory half, the record's own re-read; on the mail half, a
> verb's listing of the boxes. **The write itself is answered as done**: the
> store committed it, so the failed re-read is logged and marks the half
> stale, and is never answered as the write's failure.
> The token is what a caller branches on; the note beside it is what a reader
> reads. Being honest to the caller who wrote is not enough on its own — the
> sessions that read later are the ones with no other way to tell.
- **M3** — Mailboxes: message boxes (`new → read → processed`;
  read ≠ processed; processed is a terminal archive; no delete verbs — the
  tool surface is pinned by test). A message may carry a one-line `subject`
  and an `in_reply_to` link to the message it answers. **A body no reader could
  be handed whole under the answer ceiling is refused at post**, naming the ways
  to send it: several messages, or the long text on a record and its address.
  **`read_message`
  takes delivery of one by id** — draining a whole box makes every message in
  it owed work, which is the wrong price for wanting the single one a search
  hit named — **from its own box only**, the read side having no box argument
  so another bot's cannot be opened. The guard is on the state change rather
  than the bytes, so the terminal `processed` archive of a bot's box stays
  readable from anywhere: reading history moves nothing. **The operator's box
  is the exception: a person owns it, and no bot reads it in any state** —
  `read_message` and `mark_processed` refuse it, `list_sent` shows mail to it
  by id, time and subject only, and search leaves it out and says so. **The
  operator reads it on their own page of the web UI**, served only to a
  one-person login, and marks a message processed there, opened or not; a page
  view moves nothing. **Every message that lands in a person's box is also
  pushed to their phone** when the instance is set up for ntfy: the store
  writes the push beside the message, and the sender retries it until its
  window ends. **`list_sent` is the sender's own
  view**: where your mail got to and whether anyone has read it, read-only and
  moving nothing. **`read_mailbox` with `counts_only` is how you poll**:
  per-state counts and anything jojobot cannot read, taking delivery of none of
  it, so a poll that finds an empty box costs nothing and owes nothing. It was
  a verb of its own once — the surface grows by packing flexibility onto the
  verbs that exist, not by adding verbs. **A sender may replace its own message**
  in the same box, with `post_message`'s `supersedes`: a replaced message still
  in `new` is no longer handed over and is counted as superseded, and
  `read_message` and `list_sent` name both ends. **`read_message` with
  `history: true` takes no delivery** and returns every state change of one
  message, each with its moment, run, box and notes, wherever a plain read is
  allowed. A boot whose box holds mail already handed over says how to take it
  again.

> **Delivery-awareness: serve the difference.** The rule runs across the whole
> surface, and `seen_before` on a delivery is one instance of it. What a caller
> demonstrably already has is not shipped back to them. **A write verb answers with a
> receipt, and the prose its author just sent is the one thing it leaves
> out** — `capture`, `update_fact`, `journal`, `amend_journal`,
> `wrap_session`, `set_charter` and `post_message` all
> answer with the id, the state and what jojobot changed on the way in
> (a qualified subject, a defaulted provenance, a trimmed value), plus the
> byte count of what was STORED and enough of the opening to tell one write
> from another. Eliding is never silent: each answer names the call that
> returns the whole thing. **Every write verb's receipt carries one `changed`
> block** — what the write did, entry by entry (`on`, `what`, a `key`, `from`
> and `to`), text named by its size and never its words, or `nothing` — read
> just before and just after the write, not under a lock, so another session's
> write to the same thing in that instant can appear in it. **A write that lands
> also says what it opened up:** `capture`, `add_entity` and `update_fact` add
> an `unlocks` block when the thing could answer more — each step the keys to
> add and what they let jojobot answer, a half-held declared type once per
> thing per session — and say nothing when there is nothing to unlock; and a
> claim whose words name another known thing that is not its subject carries
> `possible_subject_note`, once per thing per session, naming the route that
> moves a claim. The write lands either way. `mark_processed` answers with the message's
> envelope — id, sender, new state and the notes it stored — and nothing of its
> subject or body beyond the body's byte count. **`post_message` also takes
> delivery of the caller's own box in the same call**, lists only the mail
> nobody had been handed yet and counts the rest, and the answer says how
> that delivery was taken: writing is a moment a bot is demonstrably present, so
> reading and posting are one round trip rather than two, and a bot that posts at
> the end of a piece of work does not meet its own mail afterwards flagged as
> something it has already seen. `read_mailbox`'s `new_only`
> stops re-shipping a deliberately held-open message on every poll, and
> `start_here` takes `brief` so a caller who does not need the orientation
> essay can skip it, and withholds the charter from every resume, marked so that
> *withheld* cannot be read as *absent*, and the answer names the call that
> returns it rather than claiming anything about what its reader already holds,
> which is not something the engine checks. **The answer to a resume offer is
> short**: sent with that offer's own `offer_code`, it carries the run, key,
> claim, zone and any rule written since the offer, and names the essay,
> snapshot, charter, carried rules, unseated map and handover under `left_out`.
> **A run that lost its context says so** — `start_here` with `compacted: true`
> and its live `sid` — and jojobot forgets what that run was taught and shown,
> restarts its served total, marks the chronology, and answers with the full
> orientation and the charter. **One ceiling bounds the WHOLE
> serialized answer of every verb, the boot included, rather than any single
> block inside it.** A list fills against it in its own ranked order, after the
> rest of the answer is counted, and is never cut once finished: `search`,
> `recall` and `list_sent` name the `offset` that returns the next part, and a
> delivery takes every message while it ships the bodies that fit, then
> envelopes flagged `body_elided`, then ids under `not_shown`. What must
> ship is measured first, and everything else is ranked against what is left,
> so an identity too heavy to serve whole gets its core and is told what did
> not fit. **A named boot whose floor is over the ceiling says so**
> (`over_the_ceiling`), with the sizes and the ways down, and **a charter is
> held to the ceiling as the boot composes it**, the build's layer included. A session's chronology comes back as its newest entries under the
> same discipline, sized on what an entry renders as rather than on its text
> alone; **both the boot and the wrap answer through a budget**, and the
> response states the full length and what it left out. **A bot's own
> records are capped by COUNT rather than by length**: a writer marks which ones
> it wants carried, a boot serves at most a fixed number of those, and it names
> what it left behind rather than shortening any of them — a cut record reads
> like a whole one, so truncating is the one thing the cap must not do.
> **Each rule in force that did not get a seat is listed as one line, up to a
> cap, and the rest are counted by purpose**, so a session knows what it holds
> without loading it; `recall` takes a `status` that narrows the records it
> lists and counts the rest. And
> **mailbox counts are scoped to the caller** — the boxes a bot drains come back
> with their per-state counts, every other box by name only, so existence stays
> visible (a writer needs it) while somebody else's queue stops posing "is that
> one mine?".
> **Quiet answers.** A key holding no value is left out of every answer rather
> than sent as null, except the few whose null means something. **A teaching
> rides once per session, on the verb it is about** — the claims teaching on
> the first answer of any verb that returns a standing claim, a search's matching caveats on the first search and on any
> answer of fewer than three hits.
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
  from there. An anonymous boot's snapshot **names** the bots — up to twenty,
  in handle order, the rest counted with the call that lists them; a named
  boot lists only the booting bot and counts the rest under `bots_left_out` — so that offer is reachable from
  the door rather than only from a refusal you had to provoke — names only, a
  caller with no identity being one that is choosing rather than weighing.
  Anonymous boot gets orientation and no `sid` — an orientation
  preview, with nothing usable behind it. Ownership is stated **on the
  mailbox**, an `owner` field set once when the box opens, so there is no
  second copy anywhere to keep in step with it. For a bot's box it is not an
  ACL: a box names its one owner, and nothing on the mail rail enforces anything
  against it. **A box a person owns is private**, and that is enforced. **The
  operator is a person, never a bot:** `topic:instance` names them under
  `operator`, the boot names them, and theirs is the only person's box.
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
> creates the bot (the operator's box is the one exception: it opens inside the
> first `post_message` to them), and the near-miss screen that guards it is the bot handle's,
> because the box is named for the handle and that is where the collision
> actually happens. Any future verb that would create something as a side
> effect of doing something else is the thing this rule exists to forbid.

- **M5** — Sessions: a bot is a **role**, a session is **one mortal run of it**
  — the unit of work, not of connection, so it survives a disconnect or a
  device hop. **One record per run, in the store**: it carries
  **state** (`active` → `wrapped` | `abandoned`; **`wrapped` is the last word,
  because wrapping folds the still-open focus into the closing story as one
  last chronology entry — `abandoned → active` is the one legal walk-back,
  since a run nobody wrapped up left nothing to fold**; **the one way back into
  a wrapped run is its `wrap_code`**: the first wrap hands one back, stored only
  as a hash, and booting
  with it as `resume` reopens the run for one last change while it stays
  wrapped — a second wrap, or a newer run of the bot, ends that window; the
  handover a later boot reads is the run's closing entry, marked as such, never
  merely its newest) and the
  **focus as current truth**, rewritten in place; the **chronology hangs off
  it**, one entry per beat (append-only, oldest first; only the newest
  entry amendable). **A run keeps every change beside its row** — focus, zone,
  stated day, wrap window, close, served total, key, charter moment (read as
  "never told" for a run never handed its charter), never a wrap code or a
  key itself — and every wording of each journal entry;
  `recall` of a session with `history` (a field's name, or `journal`) reads
  them in parts, newest first.
  **`start_here` is the start verb** — there is no `start_session`, because
  there is no moment between "I am gamma" and "gamma is working". Booting with
  a bot name hands back a `sid` immediately when there is nothing to resume,
  and otherwise hands back the resume-or-new choice and no `sid` — the `sid`
  arriving once the caller picks. The choice carries a one-use `offer_code`
  bound to the bot and the moment, each run's `working_on`, and, for a run still
  inside its lease, the moment a name can take it. Booting sweeps that bot's sessions that have gone `ABANDONED_AFTER` without a
  beat, **offers** any resumable one back as a choice, and otherwise begins one **lazily: no row until the first
  write**, so a boot that does nothing leaves nothing behind; the record still
  dates the run from its boot, never from that first write. A session records
  what it is working on, so the offer can tell two of them apart — and a bot
  may have several running at once, because the `sid` is what tells them apart.
  **What a BOT is working on is a key on the bot, `working_on`**, distinct
  from a run's own focus: a link declared or cleared by `journal`'s
  `working_on` argument, or filled when a session moves work its own bot owns
  to `now` (or to a word the instance maps to `now`), and served only while
  the session that wrote it is live — otherwise fields, filters and walks
  read it as unknown, and a listing of the bot's records marks it stale.
  **The page of a piece of work or a project says who is working on it**:
  the bots whose live `working_on` points at it or at anything under it
  first, then a dated list of past runs that declared it, each row linking to
  the run's anchor on its bot's page. Live is `working_on_of`'s answer and
  nothing else, and a bot that only wrote to the work is not listed.
  **A role is claimed at the door** (`claim`) and held on a lease every write
  renews. **A role is its own object, `role:<name>`, a child of the bot that
  first claims it**, with the keys `holder`, `claimed_at` and `agent`; the flat
  `role/<name>/…` keys an older store held on the bot are moved onto the role
  object once at start, and nothing reads them after. Every role write goes
  through one function. **A claim for a role that belongs
  to another bot is refused at the door, naming the owner**, with no lease
  involved. **Only the owner or the chart head archives or restores a role
  object**, and a rename to a new parent or a merge touching one is judged the
  same way; while it stands archived, every claim on it is refused, the
  owner's too, naming the restore. `recall` with a `parent` lists a bot's
  roles.
  **A session the operator started by hand can ask for a code**
  (`start_here` with `ask_for_code`): the code lands in the operator's own box,
  the operator types it into the session, and the boot that presents it holds
  a **voucher** — an engine-written record nobody else creates, renames or
  archives, ended when the run wraps or is resumed. **Nothing requires a
  voucher yet**: the gate that does is the next slice of the chain of trust.
  **A running session is told when another identity rewrote its bot's
  charter**, on its status bar, until it reads the charter or the page.
  **A run may state the DAY it is working in, beside its zone, and everything day-grained defaults to it** — every write, every day-grained read, the staleness sweep and the offer window. A call naming its own date still wins, and a run that states no day is answered on the clock in its zone. **jojobot never derives the day and never advances it.** **A run's zone is the one it sent, else the one a resumed run holds, else the instance's own** — the `timezone` key on `topic:instance`, written once by the operator — **else UTC**, and the boot answer names the zone and where it came from. Nothing in the software names a zone.
  **AND THE SERVER MAY BE ACTING OUT A DAY OF ITS OWN.** `JOJOBOT_TODAY` sets one
  day for a whole run and the server's `now` becomes it — the date an undated
  write gets, when a loop falls due, whether an earlier run has gone quiet, and
  both stamped columns. A restart on the same stated day continues after the
  newest stamp the store holds on that day or later, stamping to the sub-second
  so stamps follow write order, and a boot that cannot read that stamp stops,
  naming the day, so the clock never runs backwards across processes. **Unset is the real clock.** ⭐ **So *the clock* above is
  the server's clock rather than the wall clock, and on a simulated instance a
  run that states nothing is answered on the fiction.** **A day that is no day
  refuses the boot rather than falling back.**
  🚨 **IT ANNOUNCES ITSELF at `start_here` and `ping`, and the boot logs it** —
  an instance acting out a day that did not say so is the silently-wrong-state
  class, and a real deployment could be handed one and never know. **The boot
  answer names the day and tells a caller it need not send one, which is
  narrower than it looks: send `today` only when YOUR day differs from the
  server's.** ⚠️ **So `ABANDONED_AFTER` is hours only for a run that states nothing: a run that states days is swept on stated days.** Nothing ever auto-wraps a session: a new one never closes an old one, and
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
> knows which bot is asking — reads are attributed, never journalled. **The
> `sid` is a secret key** (`sk-` and 26 characters), held by the store only as
> its hash and void once the run wraps; **the run's six-character name is
> public and opens nothing** — it is what a boot offers and what `resume`
> takes, and a resume by name replaces the key and voids the run's vouchers.
> A run begun before keys exchanges its old name for a key once, on its first
> call, and that call does not run; a wrapped or abandoned run cannot, and the
> exchange counts as a pick-up. **A pick-up by name also forgets what the run was
> taught and shown.** A refusal for a dead key lists the bot's runs in flight
> and names the one call that goes on.
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
parameters; the milestone list above records what shipped, and the live surface
is whatever the tool schemas and `start_here` name.

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
  Five ship: `recommend`, `rhythms`, `asking`, `projects` and `evidence`. The
  `projects` skill is named once, on a session's first write about a project
  or a piece of work.
  jojobot decides nothing about when one applies — the caller asks.

- **Attention** — what is owed and late, answered by one read. A thing carries
  the keys that make it a loop; jojobot computes when it next falls due, stores
  that moment, and keeps it current on every write. The read finds a carrier by
  the keys it holds rather than by its kind, so a thing that acquires them is
  answered without anybody declaring a new kind. Storing the moment is what
  makes it findable without reading every row. **A `work` item or a `project`
  at status `done` owes nothing**, and its date stays on the record; a status
  key on any other kind finishes nothing.

> **The engine ships the procedure; the operator's instance personalizes it.**
> An override is an ordinary fact on the bot, read alongside the shipped text
> rather than replacing it: it narrows, it never repeals. That keeps the two
> halves under their own owners — software owns the general behaviour, data
> owns the personalization — instead of making a second copy of one truth.

The capabilities after these — events remembered where they happened · trace ·
portraits · sessions booting from jojobot — are ordered, not scheduled, on the
work-queue board.

**Layering: engine + bot.** The engine (this repo) is user-agnostic code; a
bot and its rules are *data* in the user's own store. Nothing about any
particular person is compiled in. The skills obey the same line: they name
roles and never an operator.

## Engineering rules (non-negotiable)

- **Strict TDD.** A feature is proven by an automated test; every bug fix
  starts from a failing test you watched fail FIRST. A manual run proves
  nothing.
- **To watch a test fail, use `scripts/sabotage`.** It keeps the file, makes
  the edit, runs the command, prints the verdict and puts the file back.
  **The restore is on a trap, so it happens on a pass, a failure, a raise or
  a stop** — there is no dirty tree to clean up afterwards and no moment
  where a git verb is the convenient answer. **It refuses unless the text
  appears exactly once**, and it reports an absence and an ambiguity apart,
  because those send you to different places. **It also refuses when its own
  classifier fails its self-check** — a third reason, so read which one you
  got before hunting for an ambiguity that is not there, and a refusal is
  said on stdout as well as stderr, so filtering one stream never makes a
  refusal look like silence. **It refuses a run in which no test executed**,
  before any verdict, because a filter that selects nothing stays green under
  every break. **Each run keeps its own in-flight record**, naming its
  checkout and process, so two lines that sabotage at once never lose each
  other's. **A green verdict is cross-checked against the
  run's own output**, and a disagreement between the two is printed rather
  than enforced — the exit code is still the verdict. ⚠️ **A hard kill
  cannot be caught, so it names the copy on stdout when it starts: that name
  is what you copy back over the file, and no git verb is involved.**
- **A sabotage proves nothing until you know it landed where you meant.** Two
  sites that look alike, one edit, and the verdict is about code nobody
  touched. **Assert the edit reached THAT site**, and prove the case moves when
  its own guard breaks and stays still when an independent one does. ⚠️
  **Choosing an independent neighbour is the hard part — reasoning about which
  one is independent is not enough. Run it.** ⭐ **And the same holds for which
  of two mechanisms is producing a result you are about to build on:** a
  plausible, documented, entirely real hazard can be the wrong explanation for
  the case in front of you, and only printing what the code actually saw tells
  the two apart.
- ⚠️ **And a check that reads a merged file is only as good as the REF you read
  it out of.** `main` is not a name, it is two refs: the local branch and the
  remote-tracking one, which can be a hundred commits behind. **Reading the
  wrong one gives a false green and a false alarm in the same minute** — a
  commit read out of your own branch proves your commit contains your commit.
- 🚨 **A needle that is a PHRASE is weakened by approximate matching.** Search
  matches by stem and needs most of a query's terms, so a multi-word needle can
  match a record that shares one word with it. **Assert on one distinctive
  word**, and never rest an ABSENCE on a phrase — that case can go green over
  nothing the day the matcher loosens again.
- 🚨 **Asserting a field is present and non-empty says nothing about whether it
  can be READ.** A wrapped string literal whose continuation collapses passes
  every such check while serving runs of spaces mid-sentence. **For a field
  whose whole purpose is to be read, assert its shape** — one line, no double
  space — because that is most of what it is for.
- 🚨 **A GREEN sabotage is not a control until you read the line under it.** A
  case can execute the mutated line and not depend on what it computes, which
  looks identical to the code being irrelevant. **So on a green verdict the
  tool probes again with a panic at the same site and says which kind of green
  it was** — depended on, blind, or a site no panic can be written into.
  **Read that line: a green with no probe line under it means the tool could
  not answer, not that the case is sound.** It roughly doubles the run and it
  never gates the verdict.
- 🚨 **A FIXTURE THAT WAS INVENTED RATHER THAN OBSERVED WILL PASS WHATEVER THE
  CODE DOES WITH IT.** A case is only as good as the shape it is fed: hand it a
  structure the real system never produces and it exercises a world that does
  not exist. ⛔️ **The failure is silent and it protects the very bug it was
  written against** — a check that scanned text for a date passed fixtures with
  no address at all, because scanning text is all it did, and four of its own
  cases went red the moment it was made to read structure. **Take the shape from
  something the system actually emitted.** ⭐ **This is the hostile-double rule
  one layer down: that one says a stand-in must lie where reality lies, this one
  says the INPUT must look like what reality hands you.**
  **Read the mutation before you read the verdict, too** — ask what value it
  changes. A mutation that computes the same thing is a no-op, and the probe
  above cannot see it: the site executes, a panic there would be noticed, and
  the tool truthfully reports *depended on* about a change that changed nothing.
- 🚨 **A constant asserted through itself pins NOTHING.** An assertion whose
  expected value is the same constant the code under test used proves the code
  consistent with itself and says nothing about the VALUE — rename the constant
  and both sides move together. **That matters where the spelling is STORED or
  SERVED and nothing outside this process declares it**: a key in a fields bag,
  a marker inside a stored body, a token on the wire, a phrase a record is
  parsed back through. The store accepts the new spelling, only records written
  before the change disagree, **and a record cannot fail a test. There, one test
  pins the literal.** Where the far side DOES declare the spelling — a column, a
  table, a path, a zone, a binary on the PATH — **the real-dependency suite
  already pins it and a literal adds nothing**, because a wrong value fails the
  first time that dependency runs. **The question that tells the two apart is
  one question: does anything outside this process declare this spelling?**
  ⭐ **A pin can also be a property that determines the value uniquely rather
  than a literal.** ⛔️ **It is a rule and not a lint:** the predicate that would
  automate it — an assertion whose expected value is an identifier from the same
  crate — flagged 62 sites for 0 real, because whether a value is pinned lives
  in a different file and often in a case written about something else.
  **5 unpinned spellings across 3,697 assertions.**
- 🚨 **Check the exit path BEFORE a run whose expected verdict is GREEN,
  because nothing afterwards will.** Distrusting a surprising result catches a
  broken instrument; **belief in an expected one is not a signal at all**, and a
  sabotage you expect to come back green has no red in it to catch a verdict
  that was never measuring. **A pipe swallowing the runner's exit code prints
  exactly the answer you were hoping for.**
- 🚨 **A room runs against a BUILT binary, so a sabotage of served code is
  invisible to it unless you build first.** The rooms spawn the server, and
  `cargo test -p jojobot-exercise` does not rebuild it — **so an edit to any
  served source, checked only through the rooms, reads GREEN because the code
  under test was never loaded.** That is the strongest possible wrong reason
  for a pass. **Build the workspace in the same command as the run** —
  `make rooms` and `make room` do. ⚠️ **And
  the same trap reads the other way: a REFUSAL you did not expect may be the
  old binary publishing the old schema rather than a second copy of a rule.**
- ⚠️ **An exit code belongs to the LAST command in the pipeline, so any verdict
  you read through a filter is reporting the filter.** A run piped through
  `grep` reports `grep`'s exit code, and a tool handed that pipeline faithfully
  repeats it. **It lies in BOTH directions and the second one is the surprise:**
  a `grep` for failures that finds none exits 1, so **the cleaner the run, the
  more certain the false alarm** — and a `grep` that finds a failure exits 0,
  which reads as green with the failing names printed directly above it. ⛔️
  **The instrument is not at fault and must not second-guess what it was
  given.** ⭐ **Do not pipe the verdict at all: redirect to a log and read the
  code — the only form with nothing to get wrong.**
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
  changed, or what a later read returns. A case that only checks where the
  surface stops or what shape a line takes — a size limit, the answer ceiling,
  a boot budget — carries no beat a person follows: it lives in a `contract_*`
  file beside the stories, headed as a contract, and does not count as a story.
  **A story's server starts the way the binary does** — the memory image is
  filled on every start through the same wiring call boot makes — so a story
  that writes to the store beneath the served image calls
  `the_store_changed_underneath`, as a restart would, or it tests a server
  that cannot exist.
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
  and the route. **A room is its own document: its starting world, its
  checks and the server day it runs on are written there.** A check that cannot be asked of jojobot is an
  escape into Rust, and a run prints how many escapes a room took, including
  none. It is in addition to the stories, never instead of them.
- **Commits: one per coherent problem.** A milestone lands as a handful of
  commits — never one per file or checklist item, never dozens. **A fix and
  the test that proves it are ONE problem**, however a task listed them.
- **A safe rebase needs no one's permission.** Rebasing unpushed commits onto
  the main line, and joining them by fast-forward, is routine work. Never make
  a merge commit. Push and deploy stay the operator's.
- **A dev line manages its own git, and pushes its own branches to Forgejo.**
  This is the operator's standing grant, so a line asks nobody first. In its
  own checkout and on its own branches (`devN-work` and any `devN-*` branch it
  made), a line uses any git verb: rebase onto main, create, switch, reset and
  delete its own branches. It pushes those branches with
  `git push forgejo <branch>`, and a `--force-with-lease` after its own rebase
  is included. Not covered: `main`, another line's branch or checkout, any
  remote other than `forgejo`, and a merge commit. `main` moves only by the
  engineering manager's carry.
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
  is free. **It reports every target rather than stopping at the first failure,
  so its count is coverage** — a red run names each suite's verdict instead of
  leaving a reader to guess which parts ran.
- **The bar has levels, and `make check` runs no room.** Each suite that
  stands up a room sits behind a cargo feature, so the bar compiles and lints
  it and never runs it. `make rooms` runs every room but the year,
  `make room ROOM=<name>` runs one, and `make year` runs the year room. **Forgejo CI
  runs them**: a push to a `devN-work` branch runs `make report` (the check,
  less the slow jobs its recorded history lets go), and a pull request to
  `main` runs `check`, then `rooms` and `year` — the three checks `main`
  requires, so renaming a job drops a requirement. Every run keeps
  `target/bar/*.log`. `make prepush` runs by hand, and it is release QA: the
  bar, the real store, the rooms, the year room, `nix build` and `nix flake check`, reporting every step. **A slice that
  touches a room's document, a room's test file or the exercise harness runs
  the matching `make room`, or `make rooms`, before it reports** — the bar
  alone cannot see it. **`make check` and `make rooms` run their test binaries
  side by side**, eight at a time unless `BAR_JOBS` sets another number, and
  the verdict names the number; `make test` is the plain serial run. Each
  run's test processes get a run root as their temp directory, under
  `/dev/shm` when it has 2 GiB free and on disk otherwise, with one line
  saying why it fell back.
- **An upgrade is proven by a test, never by a rehearsal on a copy of
  production.** The upgrade gate restores a store the deployed build filled,
  boots the current binary on it, reads it back through the served surface,
  and fails on any warning or error line at boot. **It keeps a fixture from each
  of the last deployed builds — today deploys 3, 4, 5 and 6 (6 from
  c99fc847) — and one is always from the build production runs now**,
  recorded before the next deploy's review; the gate's boot cases read them.
  **It also boots the head over a start interrupted partway through each
  migration the head adds**, in both states a death leaves (the marker alone,
  and the change landed with no ledger row), built by `leave_interrupted`, the
  one helper the migrate unit cases share. `make refresh-upgrade-fixture DIR=<fixture directory>` records a fixture
  again from a ref (default `origin/main`, the deployed build) whenever the deployed build moves;
  it names no directory of its own and refuses without one. An empty
  `RUST_LOG` counts as unset. **`make carried` says what a carry would move**:
  this checkout's commits that main lacks, main's commits past origin/main, and
  whether main moved on.
- **`make paid` is a third tier and `make check` never runs it.** It reaches the
  network, drives a real model through a playbook against an instance it spawns
  and throws away, and it costs money. **It must never run by accident**, which
  is why it stands apart rather than hiding behind an ignore marker. It needs
  the agent CLI on the PATH; this repo does not provision one.
- **A change to served text commits the files `make snapshot` rewrites**
  (`crates/jojobot-mcp/served-texts`): `make check` fails on any difference
  between them and what a model reads. It also lints the migrations (one change
  per file) and the store passes (a changed pass takes a new name).
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

# Tools

Every tool the router serves: its description, the structure of its arguments, and the description of each argument at every depth.

Written by `every_text_a_model_reads_matches_the_snapshot` in `crates/jojobot-mcp/src/snapshot.rs` and compared with it at every `make check`. A commit that changes one of these texts updates this file in the same commit: `make snapshot` rewrites it. What the whole snapshot cannot reach is written at the top of that source file.

## Tools

Each tool: its description, the structure of its arguments with every description lifted out, then the description of each argument by the path it sits at.

### Tool `add_entity`

#### `add_entity` — description

```text
Bring a new entity into existence — the required first step before any other write may name it.
The kinds are the `kind` argument's, which is the one place they are listed; creating a bot is creating an identity, and it opens the mailbox that bot owns in the same act.
Send `sets` to set keys on the new thing in the same call: they are written as its first claim, under every guard a capture faces, and a key that is refused refuses the whole call and creates nothing.
The claim is inference: a field that needs the operator's own word (testimony), or one you read in a system of record (observation), is written with capture.
Answers with the entity as stored and says what it changed on the way in; with sets it also returns first_claim, the claim written with it.
If its handle or any of its names resembles something jojobot already knows, NOTHING is written: the result says status: blocked with candidates and how_to_proceed.
Use the candidate you meant, or re-call with the override_token that refusal carries if this genuinely is a different thing sharing a name — a token lifts the one refusal that minted it and no other.
An exact handle collision can never be forced — a handle has exactly one owner.
A creation whose aliases, or whose `sets` as a first claim, could not be read back whole in one answer is refused and nothing is made: send fewer or shorter aliases, or move the long text onto a claim.
```

#### `add_entity` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "aliases": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "boot": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "crm": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "handle": {
        "type": "string"
      },
      "kind": {
        "type": "string"
      },
      "name": {
        "type": "string"
      },
      "override_token": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "parent": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sets": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "source": {
        "type": "string"
      }
    },
    "required": [
      "kind",
      "handle",
      "name",
      "source"
    ],
    "type": "object"
  },
  "name": "add_entity"
}
```

#### `add_entity` — argument `/inputSchema.aliases`

```text
The other names this one answers to — nickname, short form, initials.
Screened and searched exactly as `name` is, so a nickname the user
actually says is both recognized and findable. No commas.
```

#### `add_entity` — argument `/inputSchema.boot`

```text
`always` marks this entity as part of the core an assistant loads at
the start of every session; the default `on-demand` is fetched when the
conversation reaches for it. Only the exact token `always` counts.
```

#### `add_entity` — argument `/inputSchema.crm`

```text
Optional cross-link to this entity in the task layer, in whatever form
that layer addresses things. One reference, no space and no comma.
```

#### `add_entity` — argument `/inputSchema.handle`

```text
The slug half of the handle (`[a-z0-9-]+`), or a full `kind:slug` id
whose kind must match `kind`. The handle is the name this entity is
addressed by, and the only name a caller ever sends: every fact, message
and journal beat that refers to it carries this string. Choose it with
care.
```

#### `add_entity` — argument `/inputSchema.kind`

```text
One of `person`, `project`, `place`, `event`, `work`, `thing`, `org`,
`topic`, `bot`, `pet`, `rhythm`, `promise`, `machine`, `view`, `thread`, `decision`, `role`.

**A pet is a `pet` and not a `thing`.** `thing` is a named possession —
a bike, a hand tool — and a companion animal is not one.

**A computer is a `machine` and not a `thing` either**, and the kind
names the object rather than a role: a guest running on another machine
is a `machine`, and so is the one it runs on.

**A `rhythm` is a recurring loop** — a thing that comes round on a
cadence — and, with `promise`, one of the two kinds that REQUIRE a
`parent`: the parent says whose job the loop is. Two loops on one object
are two rhythms, which is why they are entities rather than a label on
the object.

**A `promise` is something somebody has to do by a day** — a borrowed
thing that has to go back. It is its own entity for the same reason: it
keeps its own events, and two can exist about one thing. Its `parent`
says whose job it is to keep.

**A `work` is a piece of a `project`'s work, filed under it**, and it
carries the keys that say where it stands: `status`, `owner`,
`waiting_on` and `depends_on`. A `project` may list its own statuses under
`columns`. The projects skill, fetched through `start_here`, says how
to write and read them.

**`bot` is an ordinary kind here**, and creating one is what this verb
is for: nothing about an identity is compiled in, so every bot beyond
the one a fresh instance ships with is made through this call — and the
mailbox it owns opens in the same act.

**A `thread` is an ongoing line in somebody's life, not a `project`
and not a `topic`.** A project moves toward completion and ends when
it is DONE; a thread ends when it stops being TRUE. A topic is the glue noun
for a world-fact belonging to no person, place or project; a thread
belongs to somebody's life and is never the anchor of last resort.

**A `decision` is what was decided on a thread**, a child of it: `parent`
is the thread. Its words are an ordinary record on it, testimony when the
operator decided. Reading the thread puts its standing decisions first. A
decision that changes is a new `decision` whose record names the old one in
`derived_from`, and the old one is archived with the reason.

**A `role` is a seat one session holds at a time**, the child of the bot
that holds it: `holder`, `claimed_at` and `agent` say who has it, since
when and in which runtime agent. jojobot makes one the first time a role is
claimed, and only a claim writes its `holder` and `claimed_at`.
```

#### `add_entity` — argument `/inputSchema.name`

```text
Display name, as a human would write it.
```

#### `add_entity` — argument `/inputSchema.override_token`

```text
The token a previous call's refusal handed you, sent back after you read
its candidates and judged them a different entity. It lifts only the
refusal that minted it — a token you made up, or one from another
refusal, lifts nothing — and it never overrides an exact handle
collision.
```

#### `add_entity` — argument `/inputSchema.parent`

```text
**The entity this one sits under**, as `kind:slug`. Optional — most
entities are roots — and **it must already exist**, exactly as every
other handle a write names must: a parent jojobot does not know comes
back blocked with candidates, and nothing is written. Nothing may be its
own parent.

**Chosen here, and changed by `rename_entity`.** That verb takes a
`parent` and reparents with it (naming the same handle in `to` when only
the parent is moving), so where a thing sits is not final, but choose it
as deliberately as the handle: a reparent is a write like any other.

A `rhythm` requires one — it is a loop ON something, and the parent is
what says on what.
```

#### `add_entity` — argument `/inputSchema.sets`

```text
**What this call sets on the new thing**, written as its first claim in
the same act, so a piece of work is made and says where it stands in one
call. The claim says the thing's name and carries these keys, exactly as
the keys of a `capture` would, and it faces every guard a capture does: a
key the kind declares is held to what it declares, and a key that holds a
handle must name a thing that exists.

A key that only labels a claim (`read_from`, `purpose`, `subject`) is never
a property of the thing and is refused by name: create the thing, then
write the label with `capture`.

**Whole or not at all.** A key that is refused refuses the whole call and
nothing is created, so there is never a half-made thing. Leave it off and
the call creates the thing alone, as before.

The claim is `inference`, as a capture's is when it says nothing. To put
the operator's own word behind a key, write it with `capture`.
```

#### `add_entity` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `add_entity` — argument `/inputSchema.source`

```text
Where this entity came from — **never invented**: the user named it, or
a real source produced it (e.g. `user-named`, `crm-card`, `calendar`).
```

### Tool `amend_journal`

#### `amend_journal` — description

```text
Rewrite your session's MOST RECENT chronology entry, in place — for a beat you got wrong or want to finish saying.
Only the most recent one: everything older is append-only, because a journal that can be rewritten further back is not evidence of anything.
A session with no entries yet comes back status: blocked rather than quietly writing your text as a first entry — an amend that silently became an append leaves a chronology saying something you did not mean.
A closed session comes back blocked too.
Pass your `sid` on every call — it is the address, and it survives the fresh connection most clients open per tool call.
This verb never STARTS a session: there is nothing to amend in one that does not exist yet.
IT ANSWERS WITH A RECEIPT, NOT THE ENTRY: the id, the moment it was stamped, the byte count of what was stored and the opening line.
You wrote the text; start_here returns the chronology when you resume.
```

#### `amend_journal` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "entry": {
        "type": "string"
      },
      "sid": {
        "type": "string"
      }
    },
    "required": [
      "entry",
      "sid"
    ],
    "type": "object"
  },
  "name": "amend_journal"
}
```

#### `amend_journal` — argument `/inputSchema.entry`

```text
What the most recent entry should say instead. It replaces that entry
whole.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as
the handle that thing wears today, even after a rename.
```

#### `amend_journal` — argument `/inputSchema.sid`

```text
Your session id — the session whose newest entry to rewrite.
```

### Tool `archive_entity`

#### `archive_entity` — description

```text
Take an entity out of every default read — list_entities excludes it, and everything that browses rather than names a handle stops seeing it.
Nothing is deleted: the entity survives, a link's far end still resolves to it, and recalling it by its own handle serves it whole, including the reason and when it was archived.
TO UNDO IT send restore: true with the same handle and a reason: the entity returns to every default read, and a claim on it records when it was archived, why, and why it came back.
A role object (kind role) is archived or restored only by the bot it belongs to or a bot heading the chart; anyone else is refused and told who may.
Archiving one does not free its name: a claim on an archived role object is refused until it is restored.
THIS IS FOR SOMETHING THAT SHOULD NOT BE SURFACED BY DEFAULT ANY MORE — a mistaken write, one that should not have happened, somebody not relevant at all.
It does not touch the entity's claims or edges, which stand exactly as recorded: archiving says the SUBJECT is out of scope, never that anything said about it was wrong.
FOR A DUPLICATE THAT HOLDS CLAIMS THIS IS THE WRONG VERB: it leaves the claims on a thing no default read shows, and merge_entities is the verb that carries them to the thing it duplicates.
AN ARCHIVED LOOP DROPS OUT OF overdue READS, and the read counts it in archived_excluded.
Archiving something already archived comes back blocked, saying the entity is already in the state you asked for — that answer means jojobot holds what you wanted, not that nothing happened.
Restoring something that is not archived says the same.
A handle that names nothing comes back blocked with the nearest handles.
```

#### `archive_entity` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "handle": {
        "type": "string"
      },
      "reason": {
        "type": "string"
      },
      "restore": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "handle",
      "reason"
    ],
    "type": "object"
  },
  "name": "archive_entity"
}
```

#### `archive_entity` — argument `/inputSchema.handle`

```text
The entity's handle.
```

#### `archive_entity` — argument `/inputSchema.reason`

```text
Why, in your own words. Required: an archived entity with no reason
is a record nobody can interpret later, which defeats the point of
leaving it reachable by digging. A mistaken write, one that should
not have happened, somebody not relevant at all — examples, never a
set to choose from.
```

#### `archive_entity` — argument `/inputSchema.restore`

```text
**Send `true` to bring an archived entity back** into every default
read instead of archiving it. `reason` is then why it comes back. The
entity's claims, edges and children were never touched, so nothing else
has to be restored. The act is recorded as a claim on the entity that
names when it was archived and why.
```

#### `archive_entity` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `capture`

#### `capture` — description

```text
Remember one fact about an entity: the claim, when it became true, and whether it is testimony, observation or inference (default inference — a hypothesis, not a finding).
A NEW THING THAT HAPPENED IS A NEW CLAIM, even about a thing that already carries one — capture it again rather than reaching for update_fact, because capture adds and never erases what stood before.
OBSERVATION is a claim you READ out of a system of record, confidently — a statement, an app, a service — and it reads back settled, so it MUST say where: give fields a read_from naming the system, and a read_ref for what you read there if you have one.
Without read_from the call is refused, because who confirmed something and on what is one question and half of it is no answer.
Use it where nobody said it to you and you did not work it out.
PROVENANCE AND STANDING ARE TWO QUESTIONS: provenance says WHO BACKS IT, standing says HOW SURE anyone is (settled or open).
Leave standing off and it follows the provenance.
Set it when the two come apart, and the case that matters is the operator stating something first-hand and hedging it — that is provenance testimony with standing open, and there is no other way to record it.
It may also draw one typed edge at another entity.
Returns the stored fact with the address you later edit it through.
Every entity it names — the subject, and an edge's object — must ALREADY EXIST: one jojobot doesn't know comes back status: blocked with candidates and nothing is written.
A genuinely new entity is two deliberate steps — add_entity, then capture.
GIVE IT FIELDS: fields is a flat bag of key/value pairs jojobot stores as written; a handle or a comma list of handles is a link and must exist, and six keys stay on their claim.
`sets` is the other bag: keys that are properties of the thing the claim is about, set by the same call; a key a record holds belongs to one bag or the other.
refs names the entities the record touches — those are links whose meaning is deliberately unrecorded, so they are searchable but assert nothing, which is what makes them not `about` edges.
NOTHING HAS TO BE DECLARED FIRST and no name for the class of thing is asked for: a key you invent is kept as you wrote it, and a type is something the keys answer rather than something you announce.
A field you send that equals what jojobot ships as its default today is stored exactly as you sent it either way, and the receipt's echoes_defaults names which key that was.
derived_from names the claim this one was worked out from, as its address — use it when the source is another claim, not an entity.
IT ANSWERS WITH A RECEIPT, NOT THE RECORD: the address that edits it, the subject as it was qualified, the date, the provenance and the standing it was given — which is how a caller that named none of those learns what was recorded — and how many keys landed, with your claim elided and said to be.
The write is read back from the store before it is called a success; what stops is shipping you the words you just sent.
recall the subject to read it back.
A `thought` is a claim drawn as a connection edge with your OWN bot handle as the subject; `thought_capacity` caps how many are live at once.
When the room is full, name one to `drop` and say why in `drop_because` (required with it), or pass `borrow` to land one over capacity — once, as the emergency reserve.
A RULE FOR YOUR OWN BOOT: capture it on your own bot handle with `starred` set to "true" in fields and it competes for one of the few seats your boot spends on rules.
An unstarred rule is kept all the same and reads back with recall's facts; when a new star pushes the oldest starred rule off its seat, the receipt names it.
A PASS, TICKET, MEMBERSHIP OR REGISTRATION SOMEBODY HOLDS is a claim on the HOLDER, with fields `admits` (the handle of the event, place or organisation it opens), `valid_from` and `valid_until` (days; no window reads as live) and optionally `tier`.
That an event offers a tier is a claim on the event instead.
Holding is not attendance, not wanting to go and not a calendar entry: each is its own claim.
Recall the holder with facts to read what they hold, or recall the event and read its `held` block, which ranks who holds what, live first.
A claim too big to be read back whole in one answer is refused with nothing written: shorten it, split it into claims that each say one thing, or keep the long text outside and write a pointer claim.
```

#### `capture` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "borrow": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "check_in": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "content": {
        "type": "string"
      },
      "derived_from": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "details": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "drop": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "drop_because": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "fields": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "happened_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "happened_through": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "object": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "provenance": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "recorded_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "refs": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "sets": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "shape": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "stale_after": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "standing": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "subject": {
        "type": "string"
      }
    },
    "required": [
      "subject",
      "content"
    ],
    "type": "object"
  },
  "name": "capture"
}
```

#### `capture` — argument `/inputSchema.borrow`

```text
**The emergency reserve — usable once, and only once.** Set this
instead of `drop` and a write that would take a full room over its
ceiling lands anyway, over capacity, rather than being refused. A
room already over capacity — a borrow already outstanding — refuses
this exactly as it would with `borrow` unset: the priority once
you're over is steering back to a clean state, never borrowing
again on top of it. Bring the room back at or under capacity first
— archive a live thought with `update_fact`, not a `drop` here,
which swaps one in for one out and leaves the debt exactly where it
was. Ignored wherever `drop` already lands the write, or the room is
not at capacity to begin with.
```

#### `capture` — argument `/inputSchema.check_in`

```text
**Record this as a check-in on a rhythm** — `ran`, `skipped` or
`snoozed`. The subject must be a `rhythm`; on anything else this is
refused.

**The difference between the three is whether the cycle is consumed.**
`ran` happened. `skipped` did not happen and the cycle advances anyway,
with the record saying plainly that it did not — which is what lets a
skipped cycle be told from a completed one later. `snoozed` does NOT
consume the cycle: it lasts until the day it names in `snoozed_until`,
and the loop comes back then and is acted on again.

**jojobot does the arithmetic.** It writes `outcome`, `last_check_in`
(the day of this check-in, `recorded_at`, today when you send none) and,
when the cycle is consumed, the new
`counts_from` — worked out from the rhythm's own `advances_from`. Do not
compute those yourself and do not send them in `fields`.

**A snooze names the day it lasts until**, as `snoozed_until` in
`fields` (`YYYY-MM-DD`), a day after this check-in's own, which is
`recorded_at` (today when you send none). The loop is
not owed again before that day, and on it the loop comes back and is
acted on again; it falls due on the later of that day and its own. A
snooze with no day, or a day that is not after that day, comes back
blocked and nothing is written. The next `ran` or `skipped` check-in
ends the snooze, so send `snoozed_until` with `snoozed` only.

**What the check MEASURED is yours to send** in `fields`: a reading, a
distance, a count. A cadence is always time, so a measurement is a field
on the check-in and never a unit of the schedule.

A rhythm short of its `cadence_days` or its `advances_from` comes back
blocked naming the key it is short of, and nothing is written. A rhythm
short only of `counts_from` still takes a `ran` or `skipped` check-in —
that outcome OPENS the loop, and the basis becomes this check-in's own
date. `snoozed` cannot open a loop, so it still refuses on a
`counts_from` the rhythm does not hold.
```

#### `capture` — argument `/inputSchema.content`

```text
The crisp claim to remember — single line, no line breaks. Put the rest in
`details`.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as the
handle that thing wears today, even after a rename.
```

#### `capture` — argument `/inputSchema.derived_from`

```text
**The claim this one was derived from**, as its address
(`kind:slug#local-id`), when it was derived from another claim rather
than from an entity. An edge's object is an entity; this is not an
edge, because a claim has no entity to point at when what it came from
is itself a claim.
```

#### `capture` — argument `/inputSchema.details`

```text
Nuance, the why, merge notes — the description under the claim. It may
hold paragraph breaks and reads back with every one; the claim above
it stays one line. Carries `@kind:slug` mentions exactly as `content`
does.
```

#### `capture` — argument `/inputSchema.drop`

```text
**The address of a thought to drop, when this capture is what fills
the last free slot in a thing's own room** — the capped set of live
thoughts `thought_capacity` bounds; see `shape`, above, for what
makes a claim one. `kind:slug#local-id`,
exactly as `recall` serves one. Archived in the SAME act as this
fact is written — never a separate call — because a drop with
nothing yet written in its place is a state the room must never
reach. Ignored when the room this write would join is not full:
naming one costs nothing until it is actually spent. Requires
`drop_because`.
```

#### `capture` — argument `/inputSchema.drop_because`

```text
**The one-line reason the dropped thought no longer earns its
slot** — your own testimony, at the moment of the act. Archived onto
the dropped claim as its own `details`. Required whenever `drop` is.
```

#### `capture` — argument `/inputSchema.fields`

```text
**The record's fields**, as a flat bag of key/value pairs — anything
worth recording about this claim beyond the sentence.

Flat and free-form on purpose: jojobot stores what you put here, acts
only on the keys the build names, and keeps a key it has never seen
exactly as you wrote it. **A value that is a handle, or a comma list of
handles, is a link and must name something that exists**; a handle
inside a sentence is prose. **Nothing has to be declared first**, and no
name for the class of thing is asked for: the fields ARE what the record
says, and a type is something the keys answer rather than something you
announce.

**`"starred": "true"` competes for one of the few seats a bot's own
boot spends on its rules, and it stays on the claim: it never folds onto
the thing.** Going home unmarked is normal, not a
failure: an unstarred rule is fetched with `facts: true` when it is
needed rather than shown by default. Seats are few on purpose —
starring everything is the same as starring nothing. **How many seats
a bot has is `rule_seats`, written about it by a different identity** —
a bot cannot raise its own — and a star or a count that would leave
the boot over its size ceiling is refused, naming the overage.

A date under `runs_out`, `decide_by` or `promised_by` makes a thing fall due on that day. `cadence_days`, `advances_from`, `counts_from` and `snoozed_until` together set when a thing falls due. `outcome` and `ended` set no day and still change what a thing owes. A `project` or `work` whose `status` is `done` is not owed, and its dates stay on it. Every other key is kept as you wrote it and nothing acts on it: it never makes a thing fall due or appear in what is owed.

`read_from`, `read_ref`, `starred`, `subject`, `purpose` and `recorded_by` describe their own claim and never fold onto the thing: read them back with `facts: true`, and find by them with a `fields` filter with `scope: record`.
```

#### `capture` — argument `/inputSchema.happened_at`

```text
**The day the thing this claim is about HAPPENED**, `YYYY-MM-DD` —
a different question from `date`, which is about the claim.

⛔️ **Leave it off unless you were told a day.** *Over the summer* is not
a day: a claim that says nothing about when the thing happened is a
complete claim, and approximating one puts a date nobody stated on a
record that may carry the operator's own authority. **jojobot never
fills this in.**
```

#### `capture` — argument `/inputSchema.happened_through`

```text
**The far end, when the thing happened is a stretch of days rather
than one** — a festival, a course — `YYYY-MM-DD`. Send it alongside
`happened_at`, which is the start; an end with no start names a span
nobody can read.
```

#### `capture` — argument `/inputSchema.object`

```text
The entity the edge points at, as `kind:slug`. **It must already exist**,
exactly as `subject` must — an edge into a node nobody else references is
how a cross-entity question quietly starts coming back empty.
```

#### `capture` — argument `/inputSchema.provenance`

```text
`testimony` (the user said it), `observation` (you read it in a system
of record) or `inference` (you derived it). Defaults to `inference`:
anything not tied to the user's words is a hypothesis.

**`observation` must say where it was read** — the field `read_from`
naming the system, and `read_ref` for what was read there if you have
one. Without it the call is refused, because a claim that reads back
settled and cannot be gone back to is worse than one filed as a guess.

**What another bot instructed or decided within its own remit is an
`observation` read from it** — `read_from` its handle, `read_ref` the
message id, read back settled and never naming the operator — **but a
world fact a bot only reported is not covered** and stays `inference`
until you read it at its own source.
```

#### `capture` — argument `/inputSchema.recorded_at`

```text
**The day the claim was MADE**, `YYYY-MM-DD` — the day it was said,
decided or worked out. Defaults to the day your run is in.

Not the day the thing happened: that is `happened_at`, and it is
separate because one field answering both is how a vague answer becomes
an invented date.

**Naming a day that differs from the day your run is in is never
refused** — the receipt carries `recorded_at_note`, one sentence
naming both days, so a sitting acting out a different period does not
silently record under the wrong one by mistake.
```

#### `capture` — argument `/inputSchema.refs`

```text
The entities this record touches, as `kind:slug` — **each must already
exist**, exactly as `subject` must.

These are links whose MEANING is deliberately not recorded: the pointer
is real and searchable, and what the connection was is left unsaid
rather than guessed. That is why they are not `about` edges — `about`
asserts the record is about that entity, and this only admits that it
touches it.
```

#### `capture` — argument `/inputSchema.sets`

```text
**What this claim sets on its thing**, as key/value pairs: the keys that
are properties of the thing the claim is about, apart from `fields`, which
are the claim's own. One call records the claim and sets the thing. A key
belongs to one of the two for as long as this record holds it, so a key
named here and in `fields` is refused, and a key the record already holds
in the other is refused with the way to move it. A key that only labels
the claim (`read_from`, `purpose`, `subject`) is never a property of the
thing and is refused here by name.
```

#### `capture` — argument `/inputSchema.shape`

```text
The shape of the edge this fact draws: `location` (object is a place) ·
`membership` (an org) · `attendance` (an event) · `about` (any kind) ·
`connection` (any kind — a link is there and how it relates was not
recorded). Requires `object`; neither works alone.

**Draw `connection` with the subject as YOUR OWN bot handle and the
claim becomes a thought of yours, not an ordinary claim about an
entity.** A thing carrying `thought_capacity` holds only that many
thoughts at once. A write that would go over is refused, naming how
many are held and asking you to name one to drop and why — see
`drop`, below. One nobody touches for long enough goes quiet on its
own: still there, just no longer counted. How long is counted in the
bot's runs: its own `thought_ages_after_runs` setting, twenty when it
has none. `update_fact`'s `keep: true` is the designed way to say one
still matters, on purpose, without changing a word of it.

**`thought_capacity` is a ceiling, and the thing it binds cannot set
it.** Send this key about your own handle and the write is refused,
whatever else the call carries — only a different identity may raise
or lower it. The same holds for `thought_ages_after_runs`.
```

#### `capture` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `capture` — argument `/inputSchema.stale_after`

```text
**The day after which this reading stops being good**, `YYYY-MM-DD`.
Optional, and most claims never carry one.

It is a fact about jojobot's knowledge rather than about the world: a
pass that runs out on a date is a claim about the world and belongs in
the content. Past the day a read SAYS SO — **the claim does not stop
being true, it stops being trusted** — and **nothing else happens**: no
sweep, no reminder, nobody is coming to check it. **A claim carrying no
day made no promise**, which is neither fresh nor stale.
```

#### `capture` — argument `/inputSchema.standing`

```text
`settled` or `open` — **how sure anyone is**, which is a different
question from `provenance`'s *who backs it*.

Leave it off and it follows the provenance: the operator's word is
`settled`, a claim you worked out is `open`. Set it only when the two
come apart — and the case that matters is **the operator saying
something first-hand and hedging it**: that is
`provenance: testimony` with `standing: open`.

**Declared on honour, exactly as `provenance` is.** Nothing here asks
you to confirm a standing. `update_fact` is where the gate sits, and
only on the one move that takes a claim the operator hedged and calls
it settled.
```

#### `capture` — argument `/inputSchema.subject`

```text
The entity the fact is about — any `kind:slug` id (a bare handle is read
as a person). **It must already exist**: a subject jojobot doesn't know
comes back with candidates and nothing is written. Create it with
`add_entity` first if it is genuinely new.
```

### Tool `declare_type`

#### `declare_type` — description

```text
Declare a type: a name, and the keys a THING of it carries.
This is WRITE-TIME HELP and never a gate — it tells a writer which keys to fill and tells `search` which keys to look for.
THE UNIT IS THE THING: a thing answers a type over every write on it, folded, so a thing described over two sittings answers a type neither sitting answers alone.
It admits nothing: a thing carrying these keys is found by search answers_type whether or not anybody declared it, and a thing carrying some of them is found too, saying which it lacks.
So declaring a type never makes a thing findable and never stops one being found, and there is nothing to undeclare.
A type arrives COMPLETE: give at least one key, because a name with nothing under it is a type nobody can query by.
Declaring a name that already exists REPLACES its keys, whole — a type is the keys it names now, and one that accumulated every key it ever named would report keys the writer had already dropped as keys a thing lacks.
KEYS ARE SCOPED BY THE TYPE that names them and are registered nowhere: two types may use one key name and mean their own thing by it.
SOME TYPES SHIP WITH THE SOFTWARE and those are CLOSED: declaring over one comes back blocked, because a caller cannot extend, shrink or replace a type the software owns — declare a name of your own instead.
Every type you declare is yours.
The answer names every type that exists, by name only, so you can see what is there without asking a second time.
```

#### `declare_type` — structure

```json
{
  "inputSchema": {
    "$defs": {
      "FieldArgs": {
        "properties": {
          "folds": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "holds": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "key": {
            "type": "string"
          },
          "one_of": {
            "default": null,
            "items": {
              "type": "string"
            },
            "type": [
              "array",
              "null"
            ]
          },
          "required": {
            "default": false,
            "type": "boolean"
          }
        },
        "required": [
          "key"
        ],
        "type": "object"
      }
    },
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "fields": {
        "items": {
          "$ref": "#/$defs/FieldArgs"
        },
        "type": "array"
      },
      "name": {
        "type": "string"
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "name",
      "fields"
    ],
    "type": "object"
  },
  "name": "declare_type"
}
```

#### `declare_type` — argument `/inputSchema/$defs/FieldArgs`

```text
One key of a type, and what it holds.
```

#### `declare_type` — argument `/inputSchema/$defs/FieldArgs.folds`

```text
**How the writes of this key come down to the one value it holds.**
`newest` is the default and needs no declaring: the newest write wins,
which is how every key reads unless you say otherwise.

`sum` makes the key a **counter** — its writes add up. Write one donut
on each of three days and the thing reads back three, so a running total
is arithmetic jojobot does rather than arithmetic a session has to fetch,
add and write back. **Every write is still there**: ask for the key's
`history` and you get each occasion, with the claim it arrived in and
that claim's date.

⚠️ **It belongs to the KEY and never to a write.** Declared here once, a
counter cannot be written inconsistently; chosen per write, one caller
adds while another replaces and the value quietly means two things.
```

#### `declare_type` — argument `/inputSchema/$defs/FieldArgs.holds`

```text
What the value holds: `text`, `number`, `date`, `date_range`, `boolean`,
or `reference` (another entity's `kind:slug` handle, which is what makes
it walkable). Defaults to `text`, which holds anything — or to `number`
when you declare the key a counter, because a total of anything else is
not a total.

**A `date_range` is ONE value, not two keys**: two dates with a slash
between them, `2026-04-18/2026-04-25`. Two keys cannot say they belong
together, so a thing carrying a start and no end reads as a thing with a
key missing rather than as a span nobody finished.

**`list:` in front means zero or more of it** — `list:text`, or
`list:reference:person` for the people who came along. Written as the
items separated by commas, so a thing that has none of something holds
an empty value rather than lacking the key. Reach for it whenever the
answer can be more than one thing: a key that can hold only one is how a
thing ends up described as less than what happened.

**A reference can name the kind on the other end** — write
`reference:place` and the key holds a place's handle and no other kind's.
Plain `reference` holds a handle of any kind, so naming one narrows the
key rather than describing it.

**What you say here describes; it does not gate.** A declared type
refuses no write at all — a value that does not hold what the key
declares comes back flagged on the hit, so a read shows the mistake
rather than a write being turned away.
```

#### `declare_type` — argument `/inputSchema/$defs/FieldArgs.key`

```text
The key a thing carries, exactly as the writes on it spell it. **The key
name IS the schema**: `expires` and `expiry_date` are two different types
and nothing will point that out, because matching is structural and a key
means whatever the things carrying it mean by it.
```

#### `declare_type` — argument `/inputSchema/$defs/FieldArgs.one_of`

```text
**The named set this key holds one of** — a closed vocabulary.

⚠️ **What that buys depends on who declared the key.** On a KIND's key,
a write outside the set is refused with the values named. **A type you
declare here refuses no write**: the set is what a reader is told about
a value that does not match, so a thing carrying one comes back in an
`answers_type` read with that key reported and the values named.

Leave it off and the key is narrowed to nothing, which is what nearly
every key wants.

Reach for it when the set really is small, known and finite: the three
states a task moves through, the two ways a thing can be paid for.
⛔️ **Not a way to enumerate an open-ended set** — every colour, every
venue — because that is a list somebody then has to maintain, and it
goes stale the day a value it does not name is the true one. There, the
honest declaration is plain `text` and the values already in use are
readable.

It narrows what the key holds rather than replacing it, exactly as
`reference:place` narrows a reference — so it works on a `list:` key
too, where every item has to be one of the values.

Values are matched **whole, trimmed and case-sensitively**. A set with
no values, a value named twice, and a value carrying a comma are each
refused, because a comma is what separates the items of a list.
```

#### `declare_type` — argument `/inputSchema/$defs/FieldArgs.required`

```text
**Whether a thing has to hold this key to be one of these at all.**
Defaults to false, and leave it there unless the key really is what
makes the thing what it is: the required keys are the ones a thing is
measured against by the reader who asks *which of these ARE one of
these*.

⚠️ **This says nothing about what the key HOLDS.** The two are separate
questions, and the useful one is usually "the colour has to be a colour"
rather than "everything must have a colour". An optional key is welcome,
never demanded, and a thing without it is complete.

⚠️ **And a type you declare here holds no write to either answer.** On a
KIND's key, a value is checked against `holds` whenever the key is
written, and a required key a write would take away is refused — which
is why a required key on a kind is a refusal waiting to happen. Here it
is the vocabulary a reader asks with.
```

#### `declare_type` — argument `/inputSchema.fields`

```text
The keys a thing of this type carries. **At least one**: a type is the
keys it names, and a name with nothing under it is one nobody can query
by.
```

#### `declare_type` — argument `/inputSchema.name`

```text
What the type is called. Declaring a name that already exists
**replaces** its keys — a type is the set of keys it names now.

**Unless the software ships that type**, which is refused: a shipped
type is closed and the answer says so. Pick a name of your own.
```

#### `declare_type` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking.
```

### Tool `journal`

#### `journal` — description

```text
Record ONE beat in your session's chronology — a literal journal, not a log.
High-level: what you did, what you found, what went wrong.
Not every tool call, not every file: a reader months from now wants the story, and a firehose buries it.
`focus` rewrites what your session says it is working on RIGHT NOW, in place — the chronology is history, the focus is the present, and they answer different questions.
The first journal entry (or the first write of any kind) is what brings your session's record into being, so a boot that does nothing leaves nothing behind.
PASS `sid` — the session id the boot door gave you — ON EVERY CALL; it is the only address, and it is what tells jojobot which bot is writing.
A `sid` whose session is closed comes back status: blocked: a closed session takes no more entries, whichever end it reached.
The two ends part company on what comes NEXT — a run that stopped without being wrapped up is offered back at your next boot, and resuming it continues this same record, while a wrapped one is the last word — its story is told and nothing appends to it except through the wrap_code its wrap handed back, so carrying on means a fresh session.
IT ANSWERS WITH A RECEIPT, NOT YOUR BEAT: the id the entry was given, when it was stamped, the run it landed in, the byte count of what was stored and the opening line.
You wrote the entry; start_here returns the whole chronology when you resume.
IF YOUR SESSION HOLDS A CLAIMED ROLE, this beat renews it, exactly as any other write does — the lease is not journal's alone to keep fresh.
```

#### `journal` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "entry": {
        "type": "string"
      },
      "focus": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "type": "string"
      }
    },
    "required": [
      "entry",
      "sid"
    ],
    "type": "object"
  },
  "name": "journal"
}
```

#### `journal` — argument `/inputSchema.entry`

```text
One high-level beat: what you did, what you found, what went
wrong. Prose — paragraphs are fine.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as
the handle that thing wears today, even after a rename.
```

#### `journal` — argument `/inputSchema.focus`

```text
What you are working on NOW, in one line. Optional, and it **replaces**
the session's current focus rather than adding to it. Carries
`@kind:slug` mentions exactly as `entry` does.
```

#### `journal` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. A session is
bound to the bot that booted it; there is no way to write into another
one.
```

### Tool `list_entities`

#### `list_entities` — description

```text
List the entities jojobot knows, optionally narrowed to one kind and/or to one entity's DIRECT children (`parent`) — the inventory.
Use it to orient, or as the cheap existence check before a write that must name an entity; use search when you are looking for something.
`parent` never reaches a grandchild and never returns the whole subtree — one level at a time, the same way a tree is built one `parent` at a time; ask again with a child's own handle to go deeper.
A `parent` naming nothing comes back blocked with candidates, never an empty list.
Metadata only — no facts.
Ordered by handle, and filled in that order under the answer ceiling: `not_shown` says how many entities came after the ones carried and the `offset` that reads on, and an `offset` past the last says so.
An archived entity is excluded — recall it by its own handle to read it whole, including why and when it was archived, and call archive_entity with restore: true to bring it back.
So is one that was merged into another (counted apart, as merged_excluded): its handle still answers, and says where it went.
An empty answer carries `searched`: one line naming what it looked through and what it left out.
```

#### `list_entities` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "kind": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "offset": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "parent": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "list_entities"
}
```

#### `list_entities` — argument `/inputSchema.kind`

```text
Narrow to one kind; omit for every entity.
```

#### `list_entities` — argument `/inputSchema.offset`

```text
**How many entities you have already read.** The listing stops at the
answer ceiling and names the `offset` that reads on; repeat the call
with it to walk the rest.
```

#### `list_entities` — argument `/inputSchema.parent`

```text
**Narrow to this entity's DIRECT children only** — combines with
`kind` when both are named. Never a grandchild and never the whole
subtree: zoom one level at a time, the same way the tree itself is
built one `parent` at a time. A handle that names no entity comes
back blocked with candidates, never an empty list — an empty list
means "nothing is under it", which is a different answer.
```

#### `list_entities` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `list_runs`

#### `list_runs` — description

```text
See YOUR OWN bot's past runs — every one this identity has had, in whatever state it reached, what each was working on, and when.
WHOSE RUNS IS NOT AN ARGUMENT: the `sid` you pass says which bot is asking, exactly as it does for journal, and there is no way to name a different identity's runs — this reads what is yours and nothing else.
It is a plain read: nothing here is begun, closed or swept.
A bot with no runs at all comes back with an empty list rather than a blocked answer — that is not a fault, it is a bot that has never journalled.
Each run carries its own handle (`sid`, addressable through start_here's resume — `null` only for a run that predates stored handles, since a plain read mints none), what it was working on last, its state (active · wrapped · abandoned), when it began, when it last had anything to show for itself, and how many beats its chronology holds — never the chronology itself, which grows without bound and is not what this answers.
For a run's chronology without resuming it, read recall with kind: session and prose: true.
Read a live or abandoned run's beats by resuming it through start_here; the most recently wrapped run's closing story arrives there too, as the handover, on every boot of this identity.
Each run also carries `served`: the total characters this run has been answered, plus a token count that is an estimate derived from those characters, never a real tokenisation.
Newest first.
Twenty newest by default: `limit` is the size of a page.
An answer stops at the answer ceiling, never mid-run, and whatever it leaves out is counted under not_shown with the `offset` that reads the next part: repeat the same call with it.
A call with no `sid` at all comes back status: blocked — jojobot will not guess whose runs to read.
```

#### `list_runs` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "limit": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "offset": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "list_runs"
}
```

#### `list_runs` — argument `/inputSchema.limit`

```text
How many runs to return, newest first.
Defaults to twenty.
```

#### `list_runs` — argument `/inputSchema.offset`

```text
**How many of the newest runs you have already read.** The answer names
the `offset` that returns the next part whenever it left runs out;
repeat the same call with it. Defaults to 0.
```

#### `list_runs` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. It is what
tells jojobot which bot is asking — there is no other way to name
whose runs this reads, and no way to name a different one.
```

### Tool `list_sent`

#### `list_sent` — description

```text
See the mail YOU have sent and where it got to — read-only, and it moves NOTHING: no state changes, nobody's delivery is taken, and the messages stay exactly as owed as they were.
It answers whether something you sent arrived and whether anyone has read it — questions every other verb could only answer by taking delivery of the box you posted into.
A `to` that names no colleague comes back status: blocked with candidates, never an empty list, because an empty list would read as 'it never arrived'.
Messages jojobot cannot read are reported separately under `unreadable`: it cannot tell who sent them, so one of yours could be there.
Newest first, each with its state (`new` = nobody has picked it up · `read` = delivered, not yet finished with · `processed` = acted on) plus notes when the consumer recorded an outcome.
A message that has left `new` also carries `taken_by`: `reading` = the owner opened the box or took that message by id, so somebody looked · `posting` = it rode back with the answer to a post of the owner's own, so it reached them and nobody went looking.
Bodies are left out unless you ask for them — you wrote them — so each carries body_bytes and the opening line instead, and says body_elided: true rather than leaving you to guess, with the one instruction for getting them said once beside the list rather than on every message.
Twenty newest by default: `limit` is the size of a page.
An answer stops at the answer ceiling, never mid-message, and whatever it leaves out is counted under not_shown with the `offset` that reads the next page: repeat the same call with it.
OMIT `sender` for your own mail — your `sid` already says who that is.
Pass one to ask after somebody else's outgoing mail: it is matched exactly against the bot handle recorded on each message (`bot:gamma`), which is allowed, because where a message got to is not private to its sender.
MAIL TO THE OPERATOR is listed by id, time and subject only, even to you and even with include_bodies, because no bot reads the operator's box back.
Each message may also carry `sender_mail_waiting_at_send`, stamped once when it was posted — see `post_message`'s own description for what it means; `null` there is unknown, never zero.
```

#### `list_sent` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "include_bodies": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "limit": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "offset": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "sender": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "to": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "list_sent"
}
```

#### `list_sent` — argument `/inputSchema.include_bodies`

```text
Ship the bodies back too. Off by default: you wrote them, so the useful
answer is where they got to, not what they say.
```

#### `list_sent` — argument `/inputSchema.limit`

```text
How many messages to return, newest first. Defaults to twenty. This is
the size of a page: an answer that would pass the answer ceiling carries
fewer and says so, and `offset` reads on.
```

#### `list_sent` — argument `/inputSchema.offset`

```text
**How many of the newest messages you have already read.** The answer
names the `offset` that returns the next page whenever it left messages
out; repeat the same call with it. Defaults to 0.
```

#### `list_sent` — argument `/inputSchema.sender`

```text
Whose outgoing mail to show, matched **exactly** against the sender
recorded on each message. Omit it for your own — your `sid` says who
that is, and your own mail is what this verb is for.
```

#### `list_sent` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `list_sent` — argument `/inputSchema.to`

```text
**Only what you sent to this bot, or the operator** — a bare name like
`gamma`, or its full handle. Omit for everyone you have written to. You addressed a
colleague, so this is how you ask after them.
```

### Tool `mark_processed`

#### `mark_processed` — description

```text
Retire a message once it has been handled — terminal, an archive, never a deletion — optionally recording the outcome in `notes`.
THE CRASH CONTRACT: call this ONLY AFTER you have acted on the message.
Mark first and then fail, and the message is gone from every future delivery with nobody the wiser; act first and crash before marking, and the next read_mailbox hands it back as a leftover — recoverable.
A FAILURE IS DATA, NOT A STATE: record it in notes (and reply with a new message if someone needs to know) — there is no failed status, because a message whose handling failed has still been handled.
When a message asks nothing of you — its whole content is known to you once you have read it — READING IT IS THE ACTING, so process it with a note and move on; the order matters for work you still owe, not for work that was never owed.
Write the outcome you actually have: a note longer than the record holds is CUT to fit and says so (a trailing ellipsis, and a delta naming what was stored), never refused — the verb that retires a message will not fail over the length of its own record.
The answer confirms the move — state, notes, id — WITHOUT echoing the message's body back at you, since the read that handed it over already gave you that; it carries body_bytes and body_elided: true instead, and read_message returns the text unchanged for a processed message.
A message can be processed straight from `new`, no delivery first.
Two refusals wear the same status: blocked shape and mean different things: an id that names nothing at all (use one read_mailbox or post_message handed you), and an id naming an item jojobot cannot read, which comes back saying why — retrying that one will not help, a person has to repair it, and until then treat whatever it carried as unhandled and say so.
TO QUARANTINE A MESSAGE INSTEAD OF RETIRING IT — a decision, not damage — pass `quarantine` with your reason and leave `notes` unset; the message becomes unreadable from every future delivery, and its refusal names you and your reason rather than asking for repair.
Only your own mail: naming a message sitting in another box is refused, and nothing is written.
There is no verb that lifts a quarantine.
```

#### `mark_processed` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "message_id": {
        "type": "string"
      },
      "notes": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "quarantine": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "message_id"
    ],
    "type": "object"
  },
  "name": "mark_processed"
}
```

#### `mark_processed` — argument `/inputSchema.message_id`

```text
The message's id, exactly as `read_mailbox` returned it.
```

#### `mark_processed` — argument `/inputSchema.notes`

```text
What happened — including a failure. Optional, one plain line.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as
the handle that thing wears today, even after a rename.
```

#### `mark_processed` — argument `/inputSchema.quarantine`

```text
**Quarantine this message instead of retiring it — a decision, not
damage.** Give the reason here and the message becomes unreadable from
every future delivery, exactly like storage damage, except the refusal
names who decided and why instead of asking for repair. Only your own
mail: naming a message sitting in another box is refused, and nothing
is written. Mutually exclusive with `notes` — a quarantine records no
processing outcome, because none happened.
```

#### `mark_processed` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `merge_entities`

#### `merge_entities` — description

```text
Merge a duplicate into the thing it duplicates — the repair for one thing named twice.
A deliberate act rather than a flag on an edit, because it is the one call here that destroys structure.
Everything the duplicate held moves to the survivor: its claims, the edges drawn at it, the history under its keys, and its names, which become aliases of the survivor.
Beside them lands a dated record of the merge itself, on the survivor, naming the handle that went and the reason if you give one.
NOTHING IS REMOVED: the duplicate's handle keeps answering, so a handle written down anywhere still resolves — but a recall of it comes back with status merged, names the survivor in merged_into and holds nothing of its own.
It does not answer as the survivor: recall the survivor to read everything.
It simply stops being a thing of its own.
USE IT WHEN TWO HANDLES ARE ONE THING.
It does not ask whether they really are: that is your judgement, so putting two unrelated things together is allowed and is recorded exactly as legibly.
There is no way back, so if you are unsure, read both with recall first.
⚠️ THE CLAIMS THAT MOVE GET NEW ADDRESSES, because an address is local to the thing that holds it — any address you were holding for the duplicate's claims is stale afterwards, and the answer says how many moved.
A role object (kind role), on either side of a merge, is merged only by the bot it belongs to or a bot heading the chart; anyone else is refused and told who may.
A MERGE INTO YOUR OWN BOT IS REFUSED WHEN THE DUPLICATE CARRIES thought_capacity OR thought_body_cap, because the merge would raise your own ceiling: have a different identity merge it, or take the key off the duplicate first with update_fact and clear_fields.
THE DUPLICATE'S THOUGHTS BECOME THE SURVIVOR'S, so a merge that would leave a room over its capacity (every active thought counts, none ages out), or bring a thought over its body cap, is refused with nothing moved: archive thoughts, or shorten the one named, with update_fact, then merge again.
Naming one handle as both sides comes back status: blocked.
So does naming a handle that was already merged away, on either side, and that refusal names the handle to use instead.
A merge whose result, the survivor with the duplicate's names and keys or the record of the merge, could not be read back whole in one answer is refused and nothing moves.
```

#### `merge_entities` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "duplicate": {
        "type": "string"
      },
      "reason": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "recorded_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "survivor": {
        "type": "string"
      }
    },
    "required": [
      "duplicate",
      "survivor"
    ],
    "type": "object"
  },
  "name": "merge_entities"
}
```

#### `merge_entities` — argument `/inputSchema.duplicate`

```text
**The handle that stops being a thing of its own** — the duplicate.
Everything it holds moves to the survivor, and the handle itself goes on
resolving.
```

#### `merge_entities` — argument `/inputSchema.reason`

```text
Why the two are one thing, in one line. Optional — worth giving: this is
the one call that destroys structure, and an account of why is what
makes it readable a year later. Left out, the record says plainly that
no reason was given rather than inventing one.
```

#### `merge_entities` — argument `/inputSchema.recorded_at`

```text
**The day this merge was made**, `YYYY-MM-DD`. Defaults to today in
your session's zone.
```

#### `merge_entities` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it.
```

#### `merge_entities` — argument `/inputSchema.survivor`

```text
**The handle that survives.** Everything ends up here.
```

### Tool `ping`

#### `ping` — description

```text
Check that jojobot is reachable: returns its identity, the BUILD that is running, and the current time.
Use `build` to tell one deployment from another — `version` is a crate version and does not move.
If the verbs you can see look wrong for what you were told this server does, this is the call that says which server you are actually talking to.
CARRYING A `sid`? Pass it and the answer also says whether that handle still addresses your session — the other half of the same question, and a handle is never turned away here.
No side effects.
```

#### `ping` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "ping"
}
```

#### `ping` — argument `/inputSchema.sid`

```text
The session handle you are carrying, if you have one — the same `sid`
that rides every other call you make. A probe needs no identity, so it
is never required and **never turned away**: the answer says whether the
handle you carried still addresses your session, beside the build that
is running. Those are the two halves of one question when the surface
stops looking like the one you booted on.
```

### Tool `post_message`

#### `post_message` — description

```text
Leave a message for someone who is not in this conversation.
THE GATE IS THE ADDRESSEE'S MAILBOX, not the name alone: `to` must come down to exactly one box jojobot can read, so a name no colleague answers to and a colleague whose mail is missing or doubled both come back status: blocked with nothing written.
The answer says which — the first is a name to fix, offered the bots that do exist; the second is damage on a bot that really is there, and it says what repairs it.
No verb opens a bot's box: it arrives with the bot, so a name nobody answers to is a name nobody drains.
THE ONE PERSON YOU MAY ADDRESS IS THE OPERATOR, by their person handle (`person:` and the slug), which the boot names under `operator`: your first post to them opens their box, nobody else has one, and no bot reads it back, you included.
A body too long to be handed to its reader whole under the answer ceiling is refused with nothing written: split it, or put the long text on a record and send its address.
Returns the stored message, including the id that read_message and mark_processed later target.
Give it a `subject`: one line saying what the message is about, which is what a reader sees on the listing and on a search hit before opening anything — put it there rather than on the body's first line.
The `state` you get back is the state as it stands — it can already say `read` if a person picked the message up in between, and that is success, not a problem: the message exists and someone has it.
POSTING ALSO DELIVERS: anything in YOUR OWN box that no read has handed you yet rides back with this answer under your_mail — out of `new` and yours to finish, exactly as read_mailbox would have handed it over — so read it rather than treating this as a write.
Mail an earlier read already handed you is not listed again: it is counted under your_mail.leftovers, still owed, and read_mailbox returns it.
Posting into your own box delivers nothing, and a post that had nothing to hand over still succeeded.
THE MESSAGE ALSO CARRIES `sender_mail_waiting_at_send`: what was genuinely waiting in YOUR OWN box at the moment you sent it, the same count your_mail's delivery would have shown, read before this post could touch it.
`null` means jojobot could not tell (no session, no box, an unreadable board) — NEVER read it as "zero waiting"; `0` is a real answer and a different one.
It is stamped once, here, and carried unchanged on this same message wherever `read_mailbox` or `list_sent` returns it later, so a reader can check a claim about your own mail against what was actually true when you hit send.
The sender is not yours to declare: jojobot records the bot behind the `sid` you pass, so a reply can always find you and nothing can be posted under somebody else's name.
A `sid` jojobot is not holding comes back status: blocked and nothing is written.
YOUR BODY IS NOT ECHOED BACK — you wrote it, so the answer carries the id, the state and body_bytes with body_elided: true rather than the text.
`list_sent` with include_bodies returns it and takes no delivery.
`in_reply_to` links this message to the one it answers: optional, it must name a message that exists (a miss comes back blocked, nothing written), and it says only that the two are one exchange — it does not deliver the original, handle it, or oblige anybody.
Messages riding back under your_mail carry `written_by_other_run` when a different run posted them, exactly as read_mailbox's own description says.
```

#### `post_message` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "body": {
        "type": "string"
      },
      "in_reply_to": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "type": "string"
      },
      "subject": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "to": {
        "type": "string"
      }
    },
    "required": [
      "to",
      "body",
      "sid"
    ],
    "type": "object"
  },
  "name": "post_message"
}
```

#### `post_message` — argument `/inputSchema.body`

```text
The message itself. Prose: paragraphs are fine.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as
the handle that thing wears today, even after a rename.
```

#### `post_message` — argument `/inputSchema.in_reply_to`

```text
The id of the message this one answers, when it answers one. Optional.
It must name a message that exists — a miss comes back blocked and
nothing is written — and it links the two without saying anything about
either: it does not deliver, handle, or oblige.
```

#### `post_message` — argument `/inputSchema.sid`

```text
**Your session id.** Required here, because it is what jojobot records
as the sender: a message from nobody is a message nobody can reply to,
and identity that is merely declared is identity that can be wrong.
```

#### `post_message` — argument `/inputSchema.subject`

```text
What this message is about, in one line — a title, not a summary.
Optional, and worth giving: it is what a reader sees in a listing and on
a search hit before they open anything. Do NOT also repeat it as the
body's first line.

**Validated, not styled: one plain line of unformatted text.** It is
shown as a title rather than rendered, so a line break, a backtick or
any other control character is refused and nothing is written — name a
tool or a field in plain words, even though every other prose surface
here takes markdown. A title over 120 characters is refused rather than
cut, because shortening your own title is yours to do. Carries
`@kind:slug` mentions exactly as `body` does.
```

#### `post_message` — argument `/inputSchema.to`

```text
**The bot to write to, or the operator** — a colleague, not a container:
a bare name like `gamma`, or its full handle. You address the identity and jojobot
finds its mail; a bot has exactly one box, and what that box is called
is not something you should have to know.

**It must already exist** — a name no bot answers to comes back with
candidates and nothing is written.

**The operator is the one person you may write to**, by their person
handle (`person:` and the slug): the boot names it under `operator`.
Nobody else has a box, and no bot reads the operator's box back.
```

### Tool `read_mailbox`

#### `read_mailbox` — description

```text
Take delivery of everything unprocessed in YOUR OWN mailbox, oldest first, moving each message from `new` to `read`.
There is no peek: reading IS taking delivery.
WHICH BOX IS NOT AN ARGUMENT — the `sid` you pass says which bot is asking, and a bot reads the box it owns, full stop.
Reading somebody else's would move their mail out of `new` and make it no longer waiting for them; to reach another box, post_message writes into it without reading it, which is the shape of a request.
No box to open comes back status: blocked, saying which kind of nothing it found — no sid, no claim, or a claim nobody has opened — and delivers nothing.
Messages a previous read already handed over are leftovers from an interrupted earlier read, not fresh mail: they are named under `leftovers` (a count and their ids) and not shipped again.
A message somebody else finished while this delivery was in flight is left out, so a delivery can be smaller than counts you saw a moment ago.
Act on what you receive, then call mark_processed for each.
Two sessions of one bot share its one box, so a delivery taken by one is not waiting for the other; two workers that must not share a queue are two bots.
Draining a whole box makes every message in it yours to finish — use read_message when you want only one.
ONLY CHECKING WHETHER ANYTHING IS WAITING? Call this with counts_only: true — you get your box's per-state counts and its unreadable report, NOTHING moves out of new and nothing becomes yours to finish, so a poll that finds an empty box costs nothing and owes nothing.
BY DEFAULT you get the messages nobody has taken yet, whole: leftovers stay counted and owed, and `leftovers` names them by id (read_message returns one) — because you were handed them once already.
Pass new_only: false to get every message back whole, flagged seen_before, which is the read for a consumer recovering from a crash that no longer holds what it was given.
Either way it changes what is SHIPPED, never what is owed.
AN ANSWER STOPS AT THE ANSWER CEILING AND TAKES EVERY MESSAGE ANYWAY: the oldest bodies come whole, a body that does not fit is left out and flagged body_elided with body_bytes and body_head, and a message that does not fit even so is named by id under not_shown; read_message returns any of them whole.
Each message may also carry `sender_mail_waiting_at_send`, stamped once when it was posted — see `post_message`'s own description for what it means; `null` there is unknown, never zero.
A message that a DIFFERENT run posted than the one reading it carries `written_by_other_run`, naming that run and whether it has since ended (`null` when that could not be read); a message your own run posted does not carry the key.
A POLL ALSO KEEPS YOUR ROLE: reading your own box with your own sid renews the role claim your session holds, the same as a write does, so a session that only polls does not go stale.
AN EMPTY DELIVERY NAMES WHAT IT LOOKED THROUGH: it carries `searched`, one line naming the box, what was left out and the call that reads it.
```

#### `read_mailbox` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "counts_only": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "new_only": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "read_mailbox"
}
```

#### `read_mailbox` — argument `/inputSchema.counts_only`

```text
**Count what is waiting and take delivery of none of it.** Off by
default: this verb delivers, and a caller that reaches for a delivery
verb wants the delivery.

Pass `true` to poll. You get the per-state counts of your own box and
its unreadable report, nothing moves out of `new`, and nothing becomes
yours to finish — so a poll that finds an empty box costs nothing and
owes nothing. `new_only` has nothing to say here: no bodies are shipped
either way.
```

#### `read_mailbox` — argument `/inputSchema.new_only`

```text
Ship bodies only for messages nobody has taken yet — **the default**.
Leftovers, the ones flagged `seen_before`, still come back, still
counted, still owed; only their bodies are left out, and each says so.

Pass `false` to get those bodies back — the read a consumer makes when
it is recovering from a crash and no longer holds what it was given.
```

#### `read_mailbox` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `read_message`

#### `read_message` — description

```text
Take delivery of ONE message by id — the selective half of read_mailbox, for when you want a single message (the one a search hit named) and have no business owning the rest of the box.
That one moves `new` to `read`; nothing else in the box is touched.
IT MUST BE IN YOUR OWN BOX: reading is taking delivery, so opening somebody else's message would move their mail out of `new` and it would never look fresh to them again — the same reason read_mailbox takes no box name.
Ids are a plain counter, so the one beside yours is somebody else's; that comes back status: blocked and moves nothing.
To reach another box, post_message writes into it without reading it, which is the shape of a request.
THE ARCHIVE IS THE EXCEPTION, and it is not a loophole: a `processed` message comes back unchanged and flagged, from any bot's box, because processed is terminal — reading one moves nothing and takes nothing on, and it is what a search hit over old mail points at.
THE OPERATOR'S BOX IS NOT A BOT'S: no bot opens a message in it, in any state, whoever wrote it.
Same envelope a delivery hands over, seen_before and all: true means somebody had already taken this message, so it is a leftover rather than fresh mail.
Taking delivery is NOT handling: call mark_processed once you have acted, and only then.
Three refusals wear the status: blocked shape — an id that names nothing at all, an id naming an item jojobot cannot read (which comes back saying why, and needs a person rather than a retry), and a message that is not yours to take.
A message a DIFFERENT run posted than the one reading it carries `written_by_other_run`, naming that run and whether it has since ended.
```

#### `read_message` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "message_id": {
        "type": "string"
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "message_id"
    ],
    "type": "object"
  },
  "name": "read_message"
}
```

#### `read_message` — argument `/inputSchema.message_id`

```text
The message's id, exactly as a search hit, a delivery or `post_message`
returned it.
```

#### `read_message` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `recall`

#### `recall` — description

```text
The graph query: say the shape you want and get that shape back.
Use it when you can describe what you are after — a handle, a kind, a type, a key and its value — and use search when you are looking for something and only have words for it.
YOUR OWN PAST RUNS ARE A KIND: kind `session` reads this identity's own past runs and no other's, and prose: true brings back a run's chronology — list_runs gives each run's state and focus line, and this is the read for what it did.
THE FOCUS A RUN CLOSED ON IS NOT IN THAT PROSE: wrap_session's closing entry carries it as `closing_focus`.
A selection that reaches another identity's runs counts them as `withheld` and shows none of them, so `withheld` counts only runs that are not yours.
A filter that leaves out runs of YOUR OWN says how many as `own_runs_excluded`.
A run carries its `sid` as a field, which you can filter on and which start_here takes to resume that run, and `near` places a run on the day it started.
A call with no sid owns no run: boot with start_here and pass the sid it returns to read your own.
START WITH A VIEW IF ONE FITS: a view is a question somebody already worked out, asked for by NAME — `view: "colleagues"` for the identities here and what each is for, `view: "loops"` for the recurring things and what each last recorded.
SOME SHIP WITH THE SOFTWARE AND YOU CAN DECLARE YOUR OWN, with add_entity of kind view and the keys selects, shows and asks; a name that is no view comes back blocked naming the ones that are, which is how you find out what is here.
Anything you send beside the name wins, so a view is a starting point rather than a cage — and you never have to work the shape out first to get an answer.
Three axes, and they COMBINE into one question rather than three.
WHICH OBJECTS: subject (one handle), kind, answers_type (structural — an OBJECT answers a type by the keys every write on it leaves standing, whether or not anybody declared it one, and carrying SOME of them is enough: the answer says which it lacks), and fields (a key, and the value it holds; omit the value to ask only that the key is there).
WHAT OF EACH: every object always comes back as its FIELDS — every write on it folded, one value per key, the newest write of that key winning, and a write that takes a key off takes it off the thing.
A LOOP HOLDING A SNOOZE DAY carries `snooze` {until, in_force} beside its fields: in_force is true only while the loop's last check-in is that snooze, and after a ran or skipped check-in the day stays in the fields as history.
Those fields are what the thing HOLDS — its KIND is what it IS — and they answer most questions; the records behind them are bigger and say the same thing at length.
To see only some of the keys, name them in keys: every object, the ones a walk reaches included, then carries just those, an object that held others names them in fields_left_out, and asking again without keys reads them all, unless a view supplied the keys: then ask again without the view, sending its kind, or with keys naming them.
A view carries it as shows_keys, which is how view colleagues shows each bot's one_liner and reports_to.
Ask for facts when you need a claim's own wording, its provenance, or the address that edits it, and the answer says how many records it left out when you did not.
A RECORD MARKED stands_for ANOTHER (a synthesis, over update_fact) is served, and the records it stands for are left out of that same facts listing — the answer says how many and names stood_for as the way to read them back; ask for stood_for: true when you need those sources themselves rather than the shape that already speaks for them.
Then prose, off by default, which is the human half of the object's page, whole; charter, which is how you read a COLLEAGUE — a bot's charter whole, and no session is made and no handle comes back, because booting as another bot to read it is the one act the rules refuse; and history, which names ONE KEY and brings back every write of it, oldest first.
A read is current truth — one value per key, the newest write — and history is the other question the same data answers: every time that key was written, with the record each write arrived in and its date.
THE COUNT OF THE WRITES IS THE ANSWER TO HOW MANY TIMES, so a key a sitting records once each time something happens is how you count occurrences.
A key nobody wrote comes back as no writes, never as a refusal.
HISTORY_RECORD is the other half of that axis: it names ONE RECORD by its address and brings back every write of THAT CLAIM, oldest first — what it said, who backed it and whether it stood, each time somebody wrote it.
That is how you read what a claim used to say before it was corrected, and how you tell a claim nobody ever made from one somebody made and got wrong.
Each write says WHEN IT HAPPENED and where it sits in the order — and that moment is not the claim's own first-recorded one, which answers a different question.
A write kept before jojobot recorded one carries none.
Name a key or a record, never both.
AN ADDRESS IS SELECTION ENOUGH: history_record on its own answers about the thing its address names, so you do not repeat the subject beside it — and naming a subject that is something else is refused rather than guessed at.
A long history comes back CUT to its newest twenty, saying how many exist and how many it left out; history_most raises the window when you really want the far end, for either half.
VALUES names a key and answers with the values the selected objects already hold under it, most used first, each with how many hold it — what to write in a key like colour or status when you want the spelling everything else uses.
IT IS A READ AND NEVER A RULE: a value nobody has used is written exactly as any other, and this is how you see the list before adding to it.
The scope is whatever the call selected, so a `fields` filter naming the key alone asks it of everything carrying the key.
A key nobody has written comes back with nothing in use rather than as a refusal, and a long list is cut to its most used twenty with values_most to reach the rest.
BACKING answers who stands behind each value: name it and every object also carries, per key, the claim whose write won that key and that claim's own provenance and standing — because a value the user stated and one an assistant worked out are the same string otherwise.
It is off by default because it is a second read; ask for it when you are about to act on a value.
A key whose writes are summed has no single winning claim and is not there.
WHAT SOMEBODY HOLDS COMES BACK UNASKED: every object carries a held block naming the records that point at it, best first — what admits somebody to it and is good on the day, then what admits them and has lapsed, and, only when nothing admits anybody at all, whatever else points here through a declared key.
Each one says who holds it, whether it is in force on the day (liveness: live, lapsed, or related when nothing admits anybody at all), what backs it, and HOW SURE THE OPERATOR WAS (standing) — because a claim somebody made and a claim an assistant worked out are not the same claim to act on, and neither are a pass somebody confirmed and one they only think they hold.
It says nothing about whether anybody may go: that is a judgement and jojobot makes none.
Nothing pointing here at all and nothing gating it are different answers and the block says which.
It is bounded, and when it does not fit it names the walk that returns the rest.
A BOT'S OWN THOUGHTS AGE QUIETLY, AND FACTS SAYS SO UNASKED: asking facts of a bot carrying thought_capacity (capture's connection edges drawn on the bot's own handle — see capture's shape) adds a `room` block naming the capacity, how many count against it now, and how many have gone quiet from being untouched too long — still there, marked aged_out on the fact itself, excluded from the count rather than hidden.
THE BLOCK NAMES THE THRESHOLD: ages_after_runs is how many of the bot's runs a thought may go untouched, and ages_after_from says whether the bot's own thought_ages_after_runs setting gave it or the default did.
THE BLOCK IS SENT ONLY WHEN AT LEAST ONE THOUGHT HAS GONE QUIET: no block means none has, and the room is exactly as full as its live thoughts.
A write that would go over the capacity is refused the same way; this is the same room, read rather than written.
WHICH EDGES: follow {shape, direction, depth}, and THE ANSWER NESTS — a walked object carries the objects it reached, each carrying its own.
EACH HOP IS BOUNDED: a wide fan-in comes back cut to what fits, in the order the walk reached it rather than any ranking, and connected_left_out on the object above says how many did not fit — recall that handle again, or narrow the walk with keeping, to reach the rest.
NARROW A WALK WITH follow.fits_type, which is the stricter half of the pair: answers_type selects objects carrying SOME of a type's keys and reports the gaps, fits_type keeps only the ones carrying EVERY key.
Which of these are described like a pet, versus which of these ARE pets.
Direction is two different questions: `out` follows the edges this object's records draw, `in` follows the edges drawn AT it, so from a party `in` reaches its guests and from a guest `out` reaches the party.
A RELATION is the other kind of link, and a DECLARATION is what makes one: a KEY some type declared to hold a `reference` points at another entity, so it is walkable.
LEAVE shape AND relation BOTH UNSET and a walk also reaches a MENTION (an entity named in a claim's own words), a REF (an entity a claim touches with no claim about how) and a FIELD LINK (an entity whose handle a record holds as a whole field value, or as every item of a comma list, under any key, declared or not), each labelled mention, ref or field rather than as an edge — name a shape or a relation and none answers, because none has one.
Name the key and use `direction` — `out` reaches what a record points at ('owner' from a pet reaches the person), `in` reaches every record pointing here through that key ('owner' from that person reaches the pets).
Inbound is scoped by the KEY ALONE: it returns every record using that key, so a repair record carrying `owner` arrives beside the pets.
For one kind of thing, SELECT it — `kind` plus a `fields` filter on the key — rather than walking.
`keeping` narrows what the walk reaches, taking the same key filters, so 'this person's pets' becomes 'this person's pets born before a date'.
A key's DECLARED value type also licenses how you compare it: `before` and `after` on a declared date, `less` and `greater` on a declared number, `equals` always and on anything.
That is what declaring a type buys you — reach — and nothing is gated by it: an undeclared record is still found by the keys it carries.
That nesting is what a flat list cannot express: one call answers 'these objects, and what each of them is connected to' instead of one call per object.
Unlike search this returns claims of EVERY status, archived included, unless you pass `status`.
A named subject always comes back, even when the filters keep none of its records — naming a handle asks for that object, a filter asks which objects — while a handle that names nothing comes back blocked with the nearest handles, never as an empty answer.
An object that draws edges the walk did not follow says so; an empty `connected` with no such note is an object with nothing beyond it.
A query that narrows nothing is refused: name at least one of subject, kind, parent, answers_type or fields.
AN answers_type THAT SELECTS NOTHING STILL SAYS WHAT THE TYPE IS: the answer carries type_keys, the type's keys and what each holds, so the spelling to write under it is in the empty answer.
The first such answer in a session, under a type the software ships and with nothing else narrowing the read, also carries a teaching: reading again finds nothing, and what you worked out is kept by capturing under those keys.
AN ANSWER STOPS AT THE ANSWER CEILING, never mid-record: it carries the objects that fit in its own order, and `not_shown` says how many it left out and the `offset` that reads on — repeat the same call with it.
One subject with more facts than fit is read the same way, `offset` counting its facts, and an object that cannot fit even first in its part carries `facts_not_shown` naming that subject to recall alone, which reads its facts in parts, and a single fact larger than the ceiling still comes back whole.
HOW OLD WHAT AN OBJECT HOLDS IS: each object carries `as_of`, the day the store last took in a write about it, and `fields_as_of` for any key whose own day is older; an object with no `as_of` has no write the store could date.
That is a day the store saw, while `overdue`'s `as_of` is the day an overdue question is asked about.
AN EMPTY ANSWER NAMES WHAT IT LOOKED THROUGH: it carries `searched`, one line saying the population, what was left out and the call that widens it.
```

#### `recall` — structure

```json
{
  "inputSchema": {
    "$defs": {
      "FollowArgs": {
        "properties": {
          "depth": {
            "default": null,
            "format": "uint32",
            "minimum": 0,
            "type": [
              "integer",
              "null"
            ]
          },
          "direction": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "fits_type": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "keeping": {
            "items": {
              "$ref": "#/$defs/KeyFilterArgs"
            },
            "type": [
              "array",
              "null"
            ]
          },
          "relation": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "shape": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          }
        },
        "type": "object"
      },
      "KeyFilterArgs": {
        "properties": {
          "compare": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "key": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "scope": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "value": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          }
        },
        "type": "object"
      },
      "NearArgs": {
        "properties": {
          "clock": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "day": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "within_days": {
            "default": null,
            "format": "uint32",
            "minimum": 0,
            "type": [
              "integer",
              "null"
            ]
          }
        },
        "type": "object"
      },
      "OverdueArgs": {
        "properties": {
          "as_of": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          }
        },
        "type": "object"
      }
    },
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "answers_type": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "backing": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "built_on": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "charter": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "facts": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "fields": {
        "items": {
          "$ref": "#/$defs/KeyFilterArgs"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "follow": {
        "anyOf": [
          {
            "$ref": "#/$defs/FollowArgs"
          },
          {
            "type": "null"
          }
        ]
      },
      "history": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "history_most": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "history_record": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "keys": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "kind": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "near": {
        "anyOf": [
          {
            "$ref": "#/$defs/NearArgs"
          },
          {
            "type": "null"
          }
        ]
      },
      "offset": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "overdue": {
        "anyOf": [
          {
            "$ref": "#/$defs/OverdueArgs"
          },
          {
            "type": "null"
          }
        ]
      },
      "parent": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "prose": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "status": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "stood_for": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "subject": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "values": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "values_most": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "view": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "recall"
}
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs`

```text
The `follow` argument of a `recall` — which edges to walk, and how far.
Leave both `shape` and `relation` unset and a walk also reaches a
mention (an entity named in a claim's own words), a ref (an entity a
claim touches with no claim about how) and a field link (an entity whose
handle a record holds as a whole field value, or as every item of a comma
list, under any key) — each
labelled mention, ref or field, never as an edge. Naming a shape or a
relation narrows to that alone: none has one.
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs.depth`

```text
How many hops: 1 is the neighbours, 2 is the neighbours' neighbours.
Defaults to 1.
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs.direction`

```text
**Which end of the edge to leave by**, and it is two different
questions. `out` (the default) follows the edges this object's own
records draw — from a guest, the party they are attending. `in` follows
the edges other objects draw AT this one — from the party, its guests.
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs.fits_type`

```text
**Keep only what FITS this type**, by name — the objects carrying EVERY
key the type names, counted across everything recorded about each one.

⚠️ **This is a different question from `answers_type` on the call
itself, and the difference is the whole point of having two words.**
`answers_type` selects objects carrying SOME of a type's keys and
reports which each one lacks — it is for finding things worth looking
at, gaps included. `fits_type` keeps only the objects with no gaps. Use
`answers_type` to ask *which of these are described like a pet, and what
is missing*; use `fits_type` to ask *which of these ARE pets*.

It narrows a walk for a reason a partial match could not: a walk
travelling one of a type's own keys reaches things that answer the type
by construction, so admitting partials would exclude nothing.
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs.keeping`

```text
**What the walk keeps of what it reaches.** The same key filters the
selection takes, `scope` and all, applied at every hop rather than to
the roots — so "this person's pets" narrows to "this person's pets born
before a date". Omit to keep everything. An object kept by a `record`
filter arrives carrying the records that answered; one kept by a filter
asked of the thing arrives whole, because the fold answered and no
record had to. An object reached and not kept leaves the object that
points at it marked as having edges nobody followed.
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs.relation`

```text
**A declared relation to walk instead of an edge.** A relation is a KEY
that some type declared to hold a `reference`: the declaration says the
value must be an entity of a named kind, and that is what makes the key
walkable by its name. A value that is a handle is a link under any key;
an unscoped walk reaches it either way.

**Name the key, and use `direction` to say which way.** `out` follows
the key off this object's own records — from a pet, `owner` reaches the
person. `in` reaches every record pointing here through that key — from
that person, `owner` reaches the pets.

⚠️ **Inbound is scoped by the KEY and by nothing else.** It reaches
every record using that key, whatever the record otherwise is: a repair
record carrying `owner` comes back beside the pets. If you want one kind
of thing, select it — `kind` plus a `fields` filter on the key — rather
than walking.

A key may be spelled like an edge shape. Pass one or the other, never
both.
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs.shape`

```text
Narrow to one shape (`location` · `membership` · `attendance` · `about`
· `connection`). Omit for **any** edge — "whatever it is connected to"
— which is also what reaches a mention, a ref or a field link; naming a shape here
reaches edges of that shape alone.
```

#### `recall` — argument `/inputSchema/$defs/KeyFilterArgs`

```text
One key filter of a `recall` — a key, and optionally the value it holds.
```

#### `recall` — argument `/inputSchema/$defs/KeyFilterArgs.compare`

```text
**How the value is compared**, and what a key was DECLARED to hold is
what licenses it. `equals` is the default and needs no declaration.
`before` and `after` need a type declaring the key a `date`; `less` and
`greater` need one declaring it a `number`. Asking for an ordering a
declaration does not license comes back blocked, rather than quietly
answering the equality question instead.
```

#### `recall` — argument `/inputSchema/$defs/KeyFilterArgs.key`

```text
The key, exactly as it is spelled where it was written. Matching is
structural, so nothing has to have declared it.

⚠️ **Omit it to ask about the VALUE under any key at all** — *is this
held as a value anywhere*, as opposed to sitting in a claim's prose.
That is the question when you know what was recorded and not where it
went. A filter naming neither a key nor a value asks nothing and comes
back blocked.
```

#### `recall` — argument `/inputSchema/$defs/KeyFilterArgs.scope`

```text
**What this filter is asked OF**, and it is two different questions.

`thing` is the default: what the object HOLDS — the newest write of the
key, or the total when the key was declared a counter. *Which friends
have eaten three or more donuts* is this one, and it finds the friend
with three separate records of one as readily as the friend with one
record of three.

`record` asks about the occasions instead: an object comes back when a
single record of its answers every `record` filter, and it arrives
carrying the records that answered. *Which visits cost more than fifty*
is this one — it is about what happened, not about what anybody holds
now, and no folding can answer it.

⚠️ **The two scopes combine in one list, and every `record` filter must
hold on the SAME record** — they describe a single record rather than a
list of separate questions.
```

#### `recall` — argument `/inputSchema/$defs/KeyFilterArgs.value`

```text
The value it must hold. **Omit it to ask only that the key is there** —
a different question, and the one to ask when you want everything that
records a thing rather than everything that records it one way.
```

#### `recall` — argument `/inputSchema/$defs/NearArgs`

```text
**The neighbourhood question of a `recall`** — what else was recorded
around a day.

A sub-object rather than flat arguments, for the same reason `overdue` is
one: a window and a clock mean nothing unless a day is named, and three
flat arguments would admit a call that names a clock and filters nothing.

⛔️ **Not a query language and not inference.** A window over dates the
store already holds.
```

#### `recall` — argument `/inputSchema/$defs/NearArgs.clock`

```text
**Which clock to compare**: `recorded_on` — the day the claim was made,
the default — `taken_in`, the day jojobot took the record in, or
`happened_at`, the day the thing itself happened.

⚠️ **They answer different questions.** *What did Milhouse say back in
August* asks the first. *What was filed that week* asks the second.
*What was going on around then* asks the third — the day on the claim,
not the day it was said or filed.

🚨 **A record with nothing on the chosen clock carries none**, so a
read on that clock cannot place it. Those are counted in
`near_unplaced` rather than dropped: an answer that shrank silently
would be indistinguishable from a day with nothing around it. Every
record carries a recorded day, so only `taken_in` and `happened_at`
can be unplaceable.
```

#### `recall` — argument `/inputSchema/$defs/NearArgs.day`

```text
**The day to read around**, `YYYY-MM-DD`. Omit it and jojobot uses
today, and the answer says which day it used.
```

#### `recall` — argument `/inputSchema/$defs/NearArgs.within_days`

```text
**How many days either side count as near.** Symmetric, because what
came just before a day and just after it are equally what surrounded
it. Omit it for a week.
```

#### `recall` — argument `/inputSchema/$defs/OverdueArgs`

```text
**The overdue question of a `recall`** — which rhythms have gone quiet, as
of a date.

It is a sub-object rather than a flag beside a date because the date only
means anything when the question is asked. Two flat arguments would admit a
call that names a day and filters nothing, which reads as an answer to a
question nobody asked.
```

#### `recall` — argument `/inputSchema/$defs/OverdueArgs.as_of`

```text
**The date the question is asked about**, `YYYY-MM-DD`. Omit it and
the run's day answers: the day it stated, else today in its zone. The
answer says which day it used.

Naming a date is what makes *what has gone quiet by next Friday* a
question this can be asked.
```

#### `recall` — argument `/inputSchema.answers_type`

```text
**Objects that answer this type**, by name. Matching is STRUCTURAL — an
object carrying the type's keys answers it whether or not anybody
declared it one — and it is asked of the OBJECT: every write on it
counts, so a thing described over two sittings answers a type that
neither sitting answers alone.

⚠️ **Some of the keys is enough, and that is what separates this from
`follow.fits_type`.** An object holding a few of them comes back saying
which it lacks, because a filter that kept only whole ones would hide
exactly the objects worth finding. `fits_type`, on a walk, keeps only
the objects that hold EVERY key. Ask this one for *which of these are
described like a service, and what is missing*; ask that one for *which
of these ARE services*.

A name no type answers to comes back blocked, naming the types that do
exist.
```

#### `recall` — argument `/inputSchema.backing`

```text
**Where each folded value came from**, and who backs it: the claim whose
write won the key, its provenance and its standing.

**Off by default because it is a second read of each object**, and most
reads want the value rather than its pedigree. Ask for it when you are
about to act on a value, or when you need to know whether the user said
it. A key whose writes are summed has no single winning claim and is not
here.

**It follows `keys`**, and names the ones it left out. A claim that
backs several keys is named by each of them, and its note is shipped
once, in `fields_backing_notes`, under that claim.
```

#### `recall` — argument `/inputSchema.built_on`

```text
**The claims worked out from one claim**, by its address
`kind:slug#local-id` — lineage walked from the source's end.

A claim names what it rests on; this asks the other way, which is the
question somebody has the moment a claim is taken back. **Records of
every status come back**, because a claim that was itself withdrawn is
part of the answer to *what did we build on this*.

**The address is selection enough.** Send it on its own and the call
answers about the thing the address names — you do not have to repeat
the subject beside it. Name a `subject` as well and it has to be that
same thing, or the call is refused: two arguments pointing at two
things is a mistake worth hearing about rather than one jojobot picks
between.
```

#### `recall` — argument `/inputSchema.charter`

```text
**A bot's charter, whole** — what that identity answers with.

**This is how you read a colleague.** Booting as another bot would make
you it, and that is the one act the rules refuse, so the whole charter
is readable here — no session is created and no handle comes back.

⚠️ **Do not send a charter you read here back to `set_charter`.** Some
of it may be text the software supplies rather than text anybody wrote,
and a write repeating it comes back blocked. Send what you are writing.

Objects that are not bots carry no charter at all.
```

#### `recall` — argument `/inputSchema.facts`

```text
Whether each object's records come back — the claims its fields were
written in, each with its own wording, provenance and the address that
edits it.

**Off by default.** The fields are what a thing HOLDS and they come
back always; the records say the same thing at length, and shipping every one
of them unasked is the cost a caller cannot decline. Ask for them when
you need a claim's own words, where it came from, or its address —
and an answer that left them out says how many there were.

**A claim written more than once says so.** `revised`/`revision_count`
come back on any record that was rewritten — absent on one that was
not, exactly as `stale` is absent on a fresh claim — because two
claims that were rewritten a different number of times otherwise read
back identical. `history_record` on this same call is the door that
opens onto the earlier wording; the count only says the door is worth
opening.
```

#### `recall` — argument `/inputSchema.fields`

```text
**Which objects**, by a key and the value it holds. Omit a filter's
value to ask only that the key is there.

⚠️ **Asked of the THING unless a filter says otherwise**, which is
`scope` on the filter itself. Asked of the thing, a filter is answered
by the FOLD — the newest write of the key — so a value some record of an
object still carries, and the object no longer holds, selects nothing.
`scope: record` asks about the occasions instead, and **every `record`
filter must hold on ONE record**: those describe a single record rather
than separate questions.

**Several filters asked of the thing must ALL hold on one thing.** The
list is an AND, never an OR: an object comes back only when it holds every
key the filters name, each at its own value. To ask for either of two
values, make two calls.
```

#### `recall` — argument `/inputSchema.follow`

```text
Which edges to walk.
Omit to walk none, and the answer is flat.
```

#### `recall` — argument `/inputSchema.history`

```text
**The writes behind one key, oldest first** — name the key, and each
object comes back carrying every write of it, with the record each one
arrived in and that record's date.

A read is current truth: one value per key, the newest write. This is
the other question the same data answers — every time the key was
written — and **the count of the writes is the answer to "how many
times"**. Ask it of `weight` and you get what a thing has weighed over
the years; ask it of a key a sitting records once each time it happens
and you get how often.

Omit it and no history comes back at all, which is the normal read: a
caller who wants the value wants one value. A key nobody has written
comes back as no writes rather than as a refusal — the thing is there
and nothing was recorded under that key.
```

#### `recall` — argument `/inputSchema.history_most`

```text
**How many of those writes come back**, newest kept. Twenty when
you do not say.

A key written a thousand times would otherwise be a thousand entries in
an answer somebody asked one narrow question of. The answer always says
how many writes exist and how many it left out, so raising this is how
you reach the far end — deliberately, rather than by surprise.
```

#### `recall` — argument `/inputSchema.history_record`

```text
**The writes behind one RECORD, oldest first** — name a record's address
`kind:slug#local-id`, and the object it is filed under comes back
carrying every write of that claim: what it said, what it said
underneath, who backed it and whether it stood, each time it was
written.

The other half of the same axis. `history` names a KEY and traces a
value across every record that touched it; this names one CLAIM and
traces the claim itself, which is how you read what a record said before
somebody corrected it. Name one or the other, never both.

Each write says **when it happened** and where it sits in the order.
That moment is not the claim's own first-recorded one, which answers a
different question and is on the claim. A write kept before jojobot
recorded one carries none.

**The address is selection enough.** Send it on its own and the read
answers about the thing the address names — you do not have to repeat
the subject beside it. Name a `subject` as well and it has to be that
same thing, or the call is refused: two arguments pointing at two
things is a mistake worth hearing about rather than one jojobot picks
between.

Omit it and no chain comes back, which is the normal read.
```

#### `recall` — argument `/inputSchema.keys`

```text
**Which keys of each object's fields come back, and only those.** Name
the keys you want, and every object in the answer — the ones a walk
reaches included — carries just those of its fields.

**Eliding is never silent.** An object that holds other keys says so in
`fields_left_out`, naming them, and asking again without `keys` reads
them all. When a view supplied the keys, ask again without the view,
sending its kind, or with `keys` naming the ones left out. A key an
object does not hold is simply not there.

Omit it and every key comes back, which is the normal read. This
narrows what each object says, never which objects come back, and it
leaves facts, prose and the charter as the arguments for those say.
**A view can carry it** as its `shows_keys`, and what you send beside
the view's name wins.
```

#### `recall` — argument `/inputSchema.kind`

```text
Every entity of one kind.
`session` reads your own past runs.
```

#### `recall` — argument `/inputSchema.near`

```text
**What else was recorded around a day.** See [`NearArgs`].
```

#### `recall` — argument `/inputSchema.offset`

```text
**How many you have already read, when an answer stopped at the answer
ceiling.** The answer names the `offset` that returns the next part;
repeat the same call with it. A call that selects ONE object counts its
facts (`facts: true` on a subject with more than fit); a call that
selects several, a kind or a walk, counts the objects. Defaults to 0.
```

#### `recall` — argument `/inputSchema.overdue`

```text
**Keep only what is OWED as of a date** — what has a due moment that
day has reached.

It OFFERS and never acts: arithmetic on what a thing holds and the day
you name, never a judgement that something should happen. **It reads a
clock never** — the day is yours to give, so *what is owed as of next
Friday* is the same question asked about a different day.

**What falls due, and when, is the KIND's own answer.** A loop is next
due a cadence after the date its cycle counts from; another sort of owed
thing computes its moment its own way, and this filter compares the
moment rather than computing it.

**A kind that owes nothing is absent**, whatever it holds — a person is
never late, because nothing computes a due moment for a person.

**A thing that carries none of what its kind falls due on is absent
too**: a loop nobody has put a schedule on made no promise to be late
against.

**But a thing whose due moment cannot be READ comes back owed**,
carrying its fields so you can see which key it is short of. That is
deliberate: a half-built loop that surfaced at no read ever would never
be heard from again.

**Absence here is not proof a write landed.** This asks whether
something is owed as of the date named — never whether an edit you
just made took effect, and a write can succeed while leaving the very
thing it was meant to change untouched. To confirm an edit worked,
recall the thing by its own handle and read the field back directly;
leaving a filtered list is not the same claim as a field changing.

**The answer orders what it found, oldest due first, and each object
carries `overdue_by_days`** — how many days past the date named it
fell due, so the order is one you can check rather than one you have
to trust. `null` on the one thing this cannot measure: a due moment
that cannot be read sorts first, loudly, rather than behind a number
that would have to be guessed. Absent entirely when this call asked no
overdue question.
```

#### `recall` — argument `/inputSchema.parent`

```text
**Only the direct children of this entity**, as `kind:slug` (a bare
handle is read as a person). One level, never a subtree. It combines with
`kind`: `kind: "role"` with a bot as `parent` is that bot's roles, each
with its holder, when it was last claimed and its agent, in one read.
```

#### `recall` — argument `/inputSchema.prose`

```text
Whether each object's **prose** comes back — the human half of its page,
whole. Off by default, because a page is bigger than a claim and shipping
every one of them unasked is a cost the caller cannot decline.

**For a bot this is the instance's own layer**, which is what somebody
wrote and what `set_charter` replaces. Ask for `charter` instead to read
what that identity actually answers with.
```

#### `recall` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `recall` — argument `/inputSchema.status`

```text
**Which records `facts` lists: `active` or `archived`.** Left out, every
record comes back whatever its status — going straight to a known thing
is the direct door, and a retired record read that way is correct.
`active` is the records that still stand; the answer says how many
retired ones it left out and which call returns them, so a narrowed read
never passes for the whole of what a thing holds. `archived` is the
other half. It narrows the records of the objects the call selected.
```

#### `recall` — argument `/inputSchema.stood_for`

```text
**Serve a shape's sources alongside it**, on the same listing of
`facts`. Off by default: a record marked with `stands_for` already
carries its sources' content in its own words, so the listing that
also carries the shape leaves the named sources out and says how many
and how to reach them. Ask for this when you need the sources
themselves — their own wording, provenance or address — rather than
the shape that already speaks for them.

**Elision applies to the listing, never to a record asked for by
name.** `history_record` on a source's own address still serves it
whole, whatever this argument says.
```

#### `recall` — argument `/inputSchema.subject`

```text
One entity, as `kind:slug` (a bare handle is read as a person).

**A named subject always comes back**, even when the filters below keep
none of its records: naming a handle asks for that object, and a filter
asks which objects. A handle that names nothing comes back blocked with
the nearest handles, never as an empty answer.
```

#### `recall` — argument `/inputSchema.values`

```text
**The values already recorded under one key**, across the objects this
call selected, each with how many of them hold it — most used first.

**A read, never a rule.** It says what is there so a caller can pick a
spelling that is already in use instead of inventing a third one; a
value nobody has used is still written, and nothing here refuses one.

The values come from what each thing HOLDS — its folded fields — so a
value that was written and later replaced is not in use, and a thing
counts once however many times it was written. **The scope is the
selection**: name a `kind` and you get that kind's values, and a
`fields` filter naming the key alone is how you ask across everything
carrying it.

A key nobody has written comes back with nothing in use rather than as
a refusal.
```

#### `recall` — argument `/inputSchema.values_most`

```text
**How many values come back**, most used kept. Twenty when you do not
say. The answer always says how many exist and how many it left out.
```

#### `recall` — argument `/inputSchema.view`

```text
**A question asked by name** — a view's handle, or its bare slug.

A view is a record that holds a query, so asking for one is naming it.
**Some ship with the software and you can declare your own**, with
`add_entity` of kind `view` and the keys `selects`, `shows` and `asks`.
Both are records of the same shape, so nothing about the answer depends
on where the view came from.

What the view says is what this call asks; anything you send beside it
is yours and wins, so a view is a starting point rather than a cage.

A name that is no view comes back blocked, with the views that exist.
```

### Tool `rename_entity`

#### `rename_entity` — description

```text
Move a handle — a new slug, a new kind (a retype), a new parent, or any two together in one call, since the handle is one string and a retype is a rename.
What follows automatically, with nothing rewritten anywhere: a mention written as @kind:slug, an edge, a reference-typed field value, every claim's own home, a journal beat and a mailbox message all keep resolving under the new handle — the path renders on the way out, and text stored before any of that was true was migrated once, so it is not an exception.
Every entity naming this one as its parent is repointed in the same write, and a box follows its owner to the owner's new name.
A rename that moves an owner across the person line — a bot into a person, a person into a bot, or the operator out of the person kind — is refused, because a person's box is private.
A role object (kind role) is renamed, retyped or reparented only by the bot it belongs to or a bot heading the chart; anyone else is refused and told who may.
What does NOT follow, ever: a handle written as free prose that never used @kind:slug — nothing marked it as a link, so nothing resolves it.
And a thing renamed and later folded into another resolves one hop short of the survivor, through the folded thing, rather than composing the two.
`to` is always `kind:slug`, fully qualified — a bare slug is refused rather than guessed at, because guessing one would be guessing whether this call is a reslug or a retype.
`parent` reparents when given and leaves the current parent alone when omitted; there is no way to clear a parent through this verb.
`to` may name the same handle as `handle` when only the parent is moving — a reparent needs no reslug — but naming the same handle with no `parent`, or the parent it already has, is refused: that call would change nothing.
The destination faces the same near-miss guard a creation does: a collision with something that resembles it comes back status: blocked with candidates, liftable with override_token unless the collision is an exact handle, which can never be forced.
A `handle` that names nothing jojobot ever held is blocked with the nearest handles; one that named something BEFORE an earlier rename moved it is told what it is called now.
```

#### `rename_entity` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "handle": {
        "type": "string"
      },
      "override_token": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "parent": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "recorded_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "to": {
        "type": "string"
      }
    },
    "required": [
      "handle",
      "to"
    ],
    "type": "object"
  },
  "name": "rename_entity"
}
```

#### `rename_entity` — argument `/inputSchema.handle`

```text
The entity's current handle — what it answers to now, or what it
used to answer to before an earlier move already changed it. A stale
handle still resolves here, and through a mention, an edge, a
reference-typed field value, a claim's home, a journal beat and a
mailbox message — all of them stay current after a move. A stale
`handle` given here is refused with the name it moved to, rather
than a bare miss.
```

#### `rename_entity` — argument `/inputSchema.override_token`

```text
The token a previous call's refusal handed you, sent back after you
read its candidates for the DESTINATION handle and judged them a
different entity. It lifts only the refusal that minted it, and
never an exact handle collision — a handle has exactly one owner.
```

#### `rename_entity` — argument `/inputSchema.parent`

```text
Reparent onto this handle. Omit to leave the current parent alone.
**There is no way to clear a parent through this verb.** `to` may
name the same handle as `handle` when this is the only thing
changing — a reparent with no reslug — but naming the same handle
with no parent, or the parent it already has, is refused: that call
would change nothing.
```

#### `rename_entity` — argument `/inputSchema.recorded_at`

```text
The day this rename was made, `YYYY-MM-DD`. Defaults to today in the
caller's own day.
```

#### `rename_entity` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `rename_entity` — argument `/inputSchema.to`

```text
The destination, as `kind:slug`. Always fully qualified — unlike a
capture's subject, a bare slug is never read as a kind by default
here, because guessing one would be guessing whether this call is a
reslug or a retype. Changing the kind IS a retype; changing the slug
IS a reslug; the id is one string, so either, or both together, is
this same call.
```

### Tool `retract`

#### `retract` — description

```text
Take back a record — one way, never reversed, and a deliberate act rather than a flag on an edit.
Nothing is removed: the record keeps its address, its words and its place, and is marked archived; beside it lands a dated record of the retraction itself, naming what it takes back and the reason if you give one.
The two then read as one story.
An archived record is out of every default read and out of every later edit, INCLUDING a status flip back — there is no un-retract, so if you are unsure, capture what is so now instead.
THIS IS THE MOVE FOR SOMETHING THAT HAPPENED and turned out not to have.
For a claim about what is true NOW, archive it with update_fact and capture the replacement instead — the old words stay readable and the replacement stands beside them, where a retraction leaves a companion record.
Retracting a retraction comes back status: blocked: it is the last word on what it takes back.
Retracting something ALREADY archived comes back blocked as well, and reads differently on purpose: it says the record is archived, because it is — that answer tells you the state you asked for is the state jojobot holds, not that nothing happened.
An address that names no record comes back blocked too, with the addresses that do exist.
```

#### `retract` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "address": {
        "type": "string"
      },
      "reason": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "recorded_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "address"
    ],
    "type": "object"
  },
  "name": "retract"
}
```

#### `retract` — argument `/inputSchema.address`

```text
The record's global address, `kind:slug#local-id` — exactly as `recall`
or a search hit returned it.
```

#### `retract` — argument `/inputSchema.reason`

```text
Why it is being taken back, in one line. Optional — worth giving: a
record marked taken-back with no account of why is hard for a later
reader to tell from damage. Left out, the retraction says plainly that
no reason was given rather than inventing one.
```

#### `retract` — argument `/inputSchema.recorded_at`

```text
**The day the record was taken back**, `YYYY-MM-DD`. Defaults to today
in your session's zone.

A retraction leaves a record of its own, and this is the day THAT
record was made. **The day an operator changed their mind is the fact a
later reader most wants about a retraction**, and it is not always the
day the call is made — a session catching up on last week says so here.
```

#### `retract` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `search`

#### `search` — description

```text
The front door — use it first, and any time you do not already hold the exact handle or address.
One ranked list over entities, facts, free prose AND the messages in bots' mailboxes at once.
`query` is free text (words are matched by stem and a record needs most of its terms, not all of them) and is optional when a filter narrows it: kind · status (default active; archived is excluded unless named) · provenance · standing (`open` is how you ask which claims are still in doubt) · subject · edge {shape, object} · answers_type · fits_type; a call with neither query nor one of those filters is refused, and include_mail is not one of them — it says what to search, not what to narrow to.
The two type filters are one question asked two ways, and a call names one of them, never both: answers_type keeps the things carrying SOME of a type's keys and says which each one lacks, fits_type keeps only the things with no gaps.
Reach for answers_type unless you have a reason not to — a thing that arrives with its gaps named can be judged, and a thing fits_type leaves out looks exactly like a thing that is not there.
An answers_type that matches nothing carries type_keys, the type's keys, and the first such answer in a session under a type the software ships, with nothing else narrowing the search, also carries a teaching.
kind + edge answers a cross-entity question in one call ("which people are in X") by walking typed edges — prose that merely mentions X is not an answer.
No hit comes back bare: a fact carries the whole claim, its address (feed that to update_fact), and who it is `about` and where it is `home`d (a null name there means the handle names nothing — a real defect worth reporting); an entity or prose hit carries that entity's names and the edges its facts draw; a message hit carries its box, its state (new/read/processed — an archived report is findable, and the state is how you tell it from live work), its sender and the id read_message takes, plus a snippet rather than the whole body.
A person's box, the operator's, is never searched.
Mail is OPT-IN: pass include_mail: true to search messages too, and reach for it when you want what a session knows — a report filed for another session is exactly the context you would not know to go looking for, and this is the one call that finds it.
It is off unless you ask, because a message hit carries somebody's box, sender and a snippet, and this is the verb to reach for first.
Note that a `kind` filter leaves mail out even when you ask, since a message belongs to no entity and so has no kind to match.
ALWAYS read the `mail` field of the answer, in BOTH directions: searched: false means no message was searched at all, which is not the same as nothing matching; and searched: true can still be partial after a degraded start, where the hits are real but anything older than this server's start is missing.
Whenever `mail` carries a `note`, that note says which case you are in — read it before concluding a message does not exist.
`memory` answers the same question about entities, facts and prose: searched: false means nothing in memory is searchable right now, and searched: true with a note means the index is behind the store, where `behind` says how much: `unscanned` (the index holds only what jojobot has written since this server started) or `stale` (the index holds an older version of one entity than the store, so a hit may be a version the store has moved past).
The hits are real either way, and `recall` reads the store itself.
`clock` picks the day ranking reads: `recorded_on` (the default — the day a claim was said) or happened_at (the day the thing itself did); a claim with no happened_at ranks by recorded_on, and `rank_fallbacks` counts them.
AN ANSWER IS CUT AT THE CEILING, never mid-hit: it carries the best hits that fit, and `not_shown` says how many it left out and the `offset` that returns them — repeat the same call with it.
THE MATCHING AND CORPUS NOTES (what the matcher can miss, and the earlier wordings it never reaches) ride your first search of a session and every answer of fewer than three hits, and are left out of the rest.
AN EMPTY ANSWER NAMES WHAT IT LOOKED THROUGH: it carries `searched`, one line saying the population, what was left out and the call that widens it.
```

#### `search` — structure

```json
{
  "inputSchema": {
    "$defs": {
      "EdgeFilterArgs": {
        "properties": {
          "object": {
            "type": "string"
          },
          "shape": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          }
        },
        "required": [
          "object"
        ],
        "type": "object"
      }
    },
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "answers_type": {
        "type": [
          "string",
          "null"
        ]
      },
      "clock": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "edge": {
        "anyOf": [
          {
            "$ref": "#/$defs/EdgeFilterArgs"
          },
          {
            "type": "null"
          }
        ]
      },
      "fits_type": {
        "type": [
          "string",
          "null"
        ]
      },
      "include_history": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "include_mail": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "kind": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "limit": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "offset": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "provenance": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "query": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "standing": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "status": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "subject": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "search"
}
```

#### `search` — argument `/inputSchema/$defs/EdgeFilterArgs`

```text
The `edge` filter of a `search` — a shape and the entity it points at.
```

#### `search` — argument `/inputSchema/$defs/EdgeFilterArgs.object`

```text
The entity the edge must point at, as `kind:slug`.
```

#### `search` — argument `/inputSchema/$defs/EdgeFilterArgs.shape`

```text
Narrow to one shape (`location` · `membership` · `attendance` · `about` ·
`connection`).
Omit for **any** edge pointing at `object` — "what's connected to X".
```

#### `search` — argument `/inputSchema.answers_type`

```text
**THINGS that answer this type, by name.** Matching is STRUCTURAL: a
thing carrying the type's keys comes back whether or not anybody
declared it to be one, so this finds things nobody filed under it. It is
asked of the thing rather than of one of its records — what a thing is
gets written down a piece at a time, and every write on it counts.

A thing carrying only some of the keys comes back too, saying which it
lacks — partial matches are the ones usually worth finding, so they are
reported and never filtered out. Each hit carries an `answers_type`
saying which keys it holds, which it lacks by name, and any whose value
is not what the type said it holds.

⚠️ **Some of the keys is enough here.** If you want only the things
with no gaps — *which of these ARE services*, rather than *which are
described like one, and what is missing* — that is `fits_type`, below.

A name no type answers to comes back blocked, naming the types that do
exist.
```

#### `search` — argument `/inputSchema.clock`

```text
**Which day ranking reads: `recorded_on`** (the default — the day a
claim was said) **or `happened_at`** (the day the thing itself did).
Neither is the system's default over the other; this is a choice the
caller makes each time.

⚠️ **A claim with no `happened_at` ranks by `recorded_on` instead**,
and the answer's own `rank_fallbacks` says how many that happened to
— an incomplete answer, accepted and explained, never a silent mix
of two orderings.
```

#### `search` — argument `/inputSchema.edge`

```text
Facts drawing a matching edge. With `kind`, this is how a cross-entity
question ("which people are in X") is answered in one call.
```

#### `search` — argument `/inputSchema.fits_type`

```text
**Only the things that FIT this type**, by name — the ones carrying
EVERY key it names, counted over every write on each.

The strict half of the same question, and which one you are asking is
yours to choose: `answers_type` asks *which of these are described like
a service, and what is missing*, and this asks *which of these ARE
services*.

Reach for `answers_type` unless you have a reason not to. A thing that
arrives with its gaps named can be judged; a thing this filter leaves
out looks exactly like a thing that is not there.

Pass one or the other, never both. A name no type answers to comes back
blocked, naming the types that do exist.
```

#### `search` — argument `/inputSchema.include_history`

```text
**Whether matching also reaches a claim's earlier wordings.** Defaults
to false: an ordinary search matches only the current wording of a
claim, exactly as the corpus note says. Worth passing true when a
query on the current wording came back thin — a claim that was
corrected may hold, in what it used to say, the words you are
looking for. A hit that matches through this is the same current
record every other fact hit is; reading the earlier wording itself is
still recall with history_record.
```

#### `search` — argument `/inputSchema.include_mail`

```text
Whether messages left in mailboxes are searched too. **Defaults to
false, and worth passing true** when you are looking for what a session
knows: a report filed for another session is exactly the context you
would not know to go looking for, and this is the one call that finds
it. It is off unless you ask, because a message hit carries somebody's
box, sender and a snippet, and this verb is the one to reach for first.
```

#### `search` — argument `/inputSchema.kind`

```text
Narrow to one entity kind — an entity's own kind, a fact's subject's kind,
or the kind of the entity whose prose matched.
```

#### `search` — argument `/inputSchema.limit`

```text
**The size of a page**: how many results come back at most; defaults to
20. An answer carries the best of them that fit under the answer ceiling
and says how many it left out; `offset` reads on from there.
```

#### `search` — argument `/inputSchema.offset`

```text
**How many of the ranked results you have already read**, which is where
the next page starts. The answer stops at the ceiling and names the
`offset` that returns the next part; repeat the same call with it. The
same `limit` with a larger `offset` walks to the end of the same ranking
and returns every hit exactly once. Defaults to 0.
```

#### `search` — argument `/inputSchema.provenance`

```text
`testimony`, `observation` or `inference` — keep only the claims backed
that way. `observation` is a claim an AI read out of a system of record,
and it names the system it was read from.
```

#### `search` — argument `/inputSchema.query`

```text
Free text over entity handles/names, fact claims and details, and an
entity's prose. **Words are matched by stem, and a record needs most of
the query's terms, not all of them.** Optional when at least one filter
below is given.
```

#### `search` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `search` — argument `/inputSchema.standing`

```text
`settled` or `open` — **the other certainty axis, and the one that
answers *which of these am I not sure about***. `provenance` says who
backs a claim; this says how sure anybody is of it. A hedge the operator
gave you, and an inference nobody confirmed, are both `open`.
```

#### `search` — argument `/inputSchema.status`

```text
`active` (the default) or `archived`. An archived fact is **excluded
unless asked for by name** — a claim already moved past must not come
back as current truth.
```

#### `search` — argument `/inputSchema.subject`

```text
Facts about this entity, as `kind:slug`.
```

### Tool `set_charter`

#### `set_charter` — description

```text
Write a bot's charter: the orienting text start_here hands a session that boots as this bot — what this identity is, its hard lines, where its work lives.
A CHARTER THAT WOULD TAKE THE BOOT OVER ITS CEILING IS REFUSED, with floor_parts naming what the boot is made of; the way forward is a shorter charter.
IT REPLACES RATHER THAN ADDS, so send the whole charter rather than an addition.
SEND WHAT YOU ARE WRITING, never a charter you just read back: some of what a read hands you may be text the software already supplies, and a charter repeating it comes back status: blocked with nothing written and says so.
IT ANSWERS WITH A RECEIPT, NOT THE CHARTER: the bot it landed on, how many bytes were stored — compare it with what you sent and you learn the store trimmed it — and the opening line, so you can tell which charter this was.
You wrote the prose; start_here with this bot returns it in full.
A bot that does not exist comes back status: blocked with the nearest handles — add_entity first; nothing is created here.
Rules are not written here either: a rule is a fact about the bot, so capture it.
Neither is a short self-description for the colleagues view: capture fields.one_liner on this bot instead — that is the key view:colleagues reads, and a bot nobody has written one for shows up there with nothing to say.
Who the bot reports to goes the same way: capture fields.reports_to on this bot, holding its manager's handle, and the colleagues view shows whom each bot reports to.
The bot itself does not write it, except a bot that heads a chart, which has reports and no manager above it, and places itself: otherwise a bot above it on the chart does, and a bot with no manager is adopted by the manager named or by a bot above that manager.
The refusal names who may.
Who reports to a bot is the other direction, a second call: recall that bot with follow, relation reports_to, direction in.
```

#### `set_charter` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "bot": {
        "type": "string"
      },
      "prose": {
        "type": "string"
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "bot",
      "prose"
    ],
    "type": "object"
  },
  "name": "set_charter"
}
```

#### `set_charter` — argument `/inputSchema.bot`

```text
The bot whose charter this is: its bare slug, or its full handle.
```

#### `set_charter` — argument `/inputSchema.prose`

```text
The charter itself. Prose: paragraphs are fine.

**It replaces what this bot had**, so send the whole thing rather than
an addition.

⚠️ **Send what YOU are writing, not a charter you just read back.** Some
of what a read hands you may be text the software already supplies, and
sending that back comes back blocked with nothing written.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as the
handle that thing wears today, even after a rename.
```

#### `set_charter` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `start_here`

#### `start_here` — description

```text
New here? Call this first — it is the ONE door, whether or not you have an identity.
Explains what jojobot is and how its world fits together — entities, facts, provenance, edges, mailboxes — with worked examples, and returns a live snapshot of what exists right now (entities by kind, EVERY BOT NAMED so you can see which identities you could boot as, each with its own mail beside it — counts on the one box you drain, elided on everybody else's), so you start oriented instead of guessing.
IT ALSO LISTS THE SKILLS this build ships — a name and what each is FOR, never the procedures themselves.
When one of them matches the job in front of you, call this again with skill: its name and you get that body.
Nothing here decides when a skill applies; the index says what each is for and you choose.
AN ANONYMOUS BOOT WHOSE ESSAY DID NOT FIT WHOLE names every section it left out — call this again with section: one of those exact headings, or "opening" for the remainder's un-headed opening paragraphs, and you get that one unit whole, no bot and no session either.
CALLED THIS BEFORE? Pass brief: true and you get the snapshot without the essay — the essay is the only part that does not change between calls, and calling again without brief reads it in full.
NAME A BOT and the same answer also carries that identity: its charter (the orienting text — what this identity is, its hard lines, where its work lives), its rules as dated claims each carrying its own provenance (testimony and observation are settled, inference is a hypothesis — read them that way), and the per-state counts of the mailbox it owns.
ANSWERING THE RESUME-OR-NEW OFFER? That answer carries no charter: only a boot that shipped one hands the offer back, so you are holding it already.
It says so rather than reading as a bot with none, and booting again with no resume reads it in full.
THIS DOOR CREATES NO IDENTITY: a name that is no bot comes back status: blocked, listing the bots that do exist and offering to boot as one of them.
It does REPAIR one thing: a bot whose box is missing gets it opened here, because a box is part of what a bot is and its absence is damage rather than a setup step — and the answer says so plainly rather than reading as normal.
BOOTING STARTS OR RESUMES THAT BOT'S SESSION — there is no separate start verb.
It first sweeps that bot's sessions that have gone a day without a beat to `abandoned`.
That sweep and that repair are the only things a boot writes, except that a boot which starts a new run ends any wrap window the bot has open, and a boot given a wrap_code as `resume` opens that window: the wrapped run's own sid comes back, the run stays wrapped for one last change, and no role is taken, nothing is swept and no lease renews.
Name no bot at all and this is an orientation preview: read-only, the world and the snapshot, no identity and no session.
THIS DOOR IS WHERE A SESSION HANDLE COMES FROM, and it takes one too: that handle rides every verb after it, reads included, because it is how jojobot knows which bot is asking — and this call is one of them.
Pass `sid` if you are already carrying one and the answer says whether it still addresses your session; leave it off on a first boot, when you have none.
The `sid` you carry is never turned away here, whatever became of it: this is the door you come back to.
PASS `claim` WITH A ROLE NAME TO HOLD THAT ROLE for 45 minutes, so two sessions never work one role at once: the answer says `taken` (your claim succeeded and you hold the role), `refused` (somebody else holds it, and the answer names who and until when; or the role belongs to another bot, and the answer names the owner; or its role object is archived, and the answer names the archive_entity call that restores it), `conflict` (the same call, retried, is the answer) or `unavailable` (the store could not decide the claim, nothing is held, and the answer says what to do next).
A boot that passes no `claim` takes the role the bot carries in `claims_role`, if it carries one, and the holder of a role is never refused its own seat.
Any write renews the lease and wrapping releases it.
```

#### `start_here` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "bot": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "brief": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "claim": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "resume": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "section": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "skill": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "timezone": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "today": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "type": "object"
  },
  "name": "start_here"
}
```

#### `start_here` — argument `/inputSchema.bot`

```text
Optional. The bot to boot as: its bare slug, or its full `bot:`-prefixed
handle. A handle of any other kind is refused — this door boots bots.
Omit it for an anonymous orientation: you get the world and the
snapshot, and no sid.
```

#### `start_here` — argument `/inputSchema.brief`

```text
Skip the orientation essay and return only what changes between calls —
the snapshot, your identity, your session.
```

#### `start_here` — argument `/inputSchema.claim`

```text
**A role to claim, by a name you choose.** Boot names this when you
want jojobot to refuse a second claimant while your claim is fresh —
**`taken` means your claim succeeded and YOU hold the role; it never
means somebody else has it, which is `refused`.** The answer is one of
`taken`, `refused` naming who holds it and until
when — or, when the role belongs to another bot, naming that `owner` and
writing nothing, so boot as the owner to claim it — `conflict` when the claim collided with another write
landing the same instant, or `unavailable` when the store could not
decide it. **A conflict is the store working correctly, not a
mistake in what you sent** — retry the same call. **An unavailable
claim holds nothing**, and its answer says what to do next.
**A boot that names no claim claims the role the bot carries in
`claims_role`, if it carries one**, and is otherwise unchanged: two
sessions working two separate slices never meet a lease neither of
them claimed. `claims_role` is written about a bot by
a different identity, never by the bot itself.

**Every write you make while holding it renews the lease**, not
only a journal beat — capture, add_entity, post_message and the
rest all count — and the lease is 45 minutes. **Wrapping releases
whatever you held**, so a fresh session may claim the same role at
once.
```

#### `start_here` — argument `/inputSchema.resume`

```text
Your answer to the resume-or-new choice a boot hands back when this bot
has a session worth picking up: the `sid` of the one you are resuming,
exactly as the offer spelled it, or `new` for a fresh session. Leave it
off on a first boot — there is nothing to answer yet.

**Or the `wrap_code` a wrap handed back**, with `bot`: it hands the
wrapped run's own sid back so that run can take one last change. The run
stays wrapped. No role is taken, nothing is swept and no lease renews.
```

#### `start_here` — argument `/inputSchema.section`

```text
**One unit of the essay's remainder, left out of an anonymous boot's
answer by the ceiling** — by its exact `##` heading, or the literal
word `"opening"` for the remainder's un-headed opening paragraphs.
The answer carries that one unit, whole, and nothing else from the
essay — never the core, which always ships and needs no fetch.

It reads the same way `skill` does: no bot, no session, a pure fetch.
A name that matches no section comes back blocked, naming every
valid one, `"opening"` included.
```

#### `start_here` — argument `/inputSchema.sid`

```text
The session handle you are already carrying, if you have one — the same
`sid` that rides every other call you make. Leave it off when you have
none: this door is where one comes from, so a first boot has nothing to
pass. **A handle is never turned away here**, whatever became of it: the
answer says whether the one you carried still addresses your session, so
a session that came back to a server it does not recognise learns that
in the same call it re-orients with.
```

#### `start_here` — argument `/inputSchema.skill`

```text
**A skill to read, by the name the index gave it.** The boot lists every
skill by name and when-to-use and ships no bodies; this is how you get
one, once the index has told you it is relevant.

It is the same door because this door is skill zero: it is where a
session learns the model, so it is where a session learns the
procedures. A name that is no skill comes back blocked, naming the ones
that are.
```

#### `start_here` — argument `/inputSchema.timezone`

```text
**Omit this on a boot.** The instance holds the operator's zone and
answers every run that sends none in it; the boot says which zone it
used and where it came from. Send an IANA name only when this run really
works in another zone, and send it again when you resume from somewhere
else. A zone you invent moves the day for everything this run writes.

It is what *today* means for everything day-grained: the date a `capture`
gets when you name none, and whether a recurring loop has fallen due.
A zone you send wins over the instance's, and the boot tells you when the
one you sent differs from it.

⚠️ **Two sessions in different zones will disagree about what today is
for the same stored claim, and that is correct.** A claim captured at
nine in the evening is already the next day in a zone further east; both
runs are reading the same claim and answering in their own frame. It is
not a fault and there is nothing to work around.

Send none and the instance's zone answers, or UTC when its operator set
none. On a resume, sending none keeps the zone the run already had
rather than moving it.
```

#### `start_here` — argument `/inputSchema.today`

```text
**The day this run is in**, as `YYYY-MM-DD`, when it is not the day the
server is having.

**The sweep decides whether your other runs went quiet, and it decides in
this frame.** Send it when a run is not happening now: a session catching
up on last week, an instance restored from a backup, a run acting out a
stretch of time. Without it the sweep answers on the server's clock, so a
run that covers months in minutes leaves every one of its own sittings
looking like it is still working, and each boot after the first meets a
resume-or-new choice for runs that are long over.

**jojobot never derives it.** The zone beside it says how to name a day;
this says which day you are in, and no zone can tell the server that.

Send none and the sweep answers on the clock, which is what it always
did.
```

### Tool `update_entity`

#### `update_entity` — description

```text
Edit what an entity is called and where it came from (name/aliases/source/crm), in place.
This verb does not edit the handle — its slug, kind or parent — use `rename_entity` for that.
THIS VERB DOES NOT TOUCH MAILBOXES: a box is not a property of an entity that can be edited or reassigned — it belongs to the bot it is named for and opens with it, in add_entity (the operator's opens at the first post to them), so there is nothing here to point at a different one.
Any change to what it is CALLED — name or aliases — faces the same check a creation does, because an alias is a name: it can come back status: blocked with candidates, and the override_token that refusal carries is how you confirm a genuinely shared name — a token lifts the one refusal that minted it and no other.
Passing `aliases` REPLACES the whole set ([] clears it); source and crm edits are never questioned.
`sets` sets keys on the thing in the same call, through a claim of inference; a call that sends only `sets` leaves the entity itself alone, and a key that needs the operator's word is written with capture.
A rule's `starred` mark stays on its claim, so it is not set here: update_fact on the rule's address with fields {starred: "true"} or {starred: "false"} stars or unstars it.
A handle that names nothing comes back blocked with the nearest handles — it never creates.
Aliases or `sets` that would leave the thing, or its claim, too big to be read back whole in one answer are refused with nothing written.
```

#### `update_entity` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "aliases": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "crm": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "handle": {
        "type": "string"
      },
      "name": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "override_token": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sets": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "source": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "handle"
    ],
    "type": "object"
  },
  "name": "update_entity"
}
```

#### `update_entity` — argument `/inputSchema.aliases`

```text
The whole alias set, replaced. Omit to leave it alone; pass `[]` to clear
it. No commas.
```

#### `update_entity` — argument `/inputSchema.crm`

```text
New cross-link to this entity in the task layer, in whatever form that
layer addresses things. One reference, no space and no comma.
```

#### `update_entity` — argument `/inputSchema.handle`

```text
The entity's handle. It says which entity to edit; this verb does not
edit it.
```

#### `update_entity` — argument `/inputSchema.name`

```text
New display name.
```

#### `update_entity` — argument `/inputSchema.override_token`

```text
The token a previous call's refusal handed you, sent back after you read
its candidates for the name or alias you are claiming here and judged
them a different entity. It lifts only the refusal that minted it. Any
change to what this entity is CALLED is screened exactly as a creation
is.
```

#### `update_entity` — argument `/inputSchema.sets`

```text
**What this call sets on the thing**, as key/value pairs, written as one
claim about it in the same act as the edit. The claim says what was set
and carries these keys in its setting bag, and it faces every guard a
`capture` does: a key the kind declares is held to what it declares, and a
key that holds a handle must name a thing that exists. **Whole or not at
all**: a key that is refused refuses the call, and the edit is not made.
A call that sends only `sets` leaves the entity itself alone.

The claim is `inference`, as a capture's is when it says nothing. To put
the operator's own word behind a key, write it with `capture`.
```

#### `update_entity` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `update_entity` — argument `/inputSchema.source`

```text
New source.
```

### Tool `update_fact`

#### `update_fact` — description

```text
Edit an addressed fact in place (content/details/date/status/provenance/standing).
To record that something is NOT so, or never was, archive the claim (status: archived, with a note saying why) and capture the replacement; there is no negated status, and a rewrite is for a mistyped or reworded claim.
NOT FOR A PAST EVENT: turning a claim about one into its negation is never this rewrite — archive it, or retract it.
DATE REWRITES THE DAY THIS CLAIM WAS MADE — the day it was said, decided or worked out, YYYY-MM-DD — the same argument retract carries, and it is never the day the call happens to be made on.
Omit it and the record keeps the day of the claim it replaces; give it when the correction itself was made on a different day, or a rewrite made long after the fact keeps the ORIGINAL day forever, reading back as if it had always been made on a day it was never made on.
TWO MOVES NEED confirmed_by_user, and they are different: moving a claim TO testimony (who backs it), from inference or from observation alike — a claim you read in a system of record is not a step towards the operator having said it — and settling a claim that is already open (how sure anyone is).
THIS IS HOW A HEDGE IS CONFIRMED — the operator hedged the claim and no longer does, so set standing settled and leave provenance alone; the claim was theirs from the start.
Reopening is free.
THE GATE IS ON PROMOTION, NOT ON ASSERTION: a fresh capture may declare standing settled and nobody is asked to confirm it, exactly as it declares its provenance — what needs the operator's word is moving a claim they hedged.
IT ALSO REACHES THE RECORD'S FIELDS: fields sets the keys you name and leaves every other key alone, and clear_fields takes keys off; `sets` does the same for the keys that are properties of the thing the claim is about, apart from the claim's own `fields`.
Those are two arguments rather than one, because setting a key to an empty value and removing the key are different edits and a caller means one of them.
A field you set that equals what jojobot ships as its default today is stored exactly as you sent it either way, and the receipt's echoes_defaults names which key that was.
TO MOVE A VALUE TO ANOTHER KEY, send `fields` with the new key and `clear_fields` with the old one in the same call.
A CLAIM CANNOT CHANGE SUBJECT: to move one filed on the wrong thing, capture it on the right one, re-sending its dates, fields and edge, with `derived_from` naming this claim, then archive this one.
AND IT REACHES THE EDGE BOTH WAYS: shape with object draws or replaces one, and clear_edge takes it off.
Reach for clear_edge when the rewrite turns a claim about what is true NOW into its current negative — was a member and is not any more, was living somewhere and moved away — because then the edge belongs to the sentence you just erased.
A rewrite that stays positive is not this case: clearing the edge there stops every walk through it and makes the entity on the other end unreachable from this record, so leave it alone.
A PAST EVENT THAT TURNED OUT NEVER TO HAVE HAPPENED IS RETRACT'S CASE, NOT THIS ONE: retracting marks the record rather than rewriting it, and there is no un-retract.
stands_for MARKS THIS RECORD AS STANDING FOR THE CLAIMS NAMED HERE, each as its address: a synthesis, never a citation — the named claims stay active and readable exactly as they were, so recall still shows the full picture.
clear_stands_for takes the mark off.
A write replaces the whole set; naming an address that does not exist, this record's own address, the same address twice, or an empty set is refused.
An address that names no fact comes back status: blocked with the addresses that do exist — it never creates.
IT ANSWERS WITH A RECEIPT, NOT THE RECORD: the address, the date, the provenance, the standing, the status and how many keys the record now carries, with the claim itself elided and said to be.
The write is read back from the store before it is called a success; what stops is shipping you the words you just sent.
recall the subject to read the record back.
AND A REWRITE DESTROYS NOTHING: every write of a claim is kept, so what it said before this call is still readable — recall the subject with history_record: the address, and you get every version of it, oldest first.
So a claim you disagree with is safe to correct, unless it is testimony from an earlier session, which is archived and replaced instead (below): you are not deciding whether the old wording survives, only what the claim says now.
TESTIMONY IS REWRITTEN IN PLACE ONLY BY THE SESSION THAT WROTE IT: a content rewrite of testimony from an earlier session is refused, and so is one of testimony that records no session.
Archive it (status: archived, with a reason in details), then capture the corrected claim with derived_from naming the archived one.
The original stays readable.
A session that wrote a claim keeps its provenance when it sends none.
NAMING NOTHING TO CHANGE IS REFUSED, NOT SILENTLY HONOURED: a call that sets none of the arguments above still reaches this verb, and a claim is never re-asserted by accident.
keep IS THE DESIGNED WAY TO DO IT ON PURPOSE — set keep: true and nothing else to keep this claim exactly as it stands while moving when it was last touched.
keep IS REFUSED IF ANYTHING ELSE HERE WOULD ACTUALLY CHANGE THE CLAIM: there is no partial keep, only this alone or an ordinary edit naming what changes.
An edit that would leave the claim too big to be read back whole in one answer is refused with the claim unchanged: shorten it, split it into claims that each say one thing, or keep the long text outside and write a pointer claim.
```

#### `update_fact` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "address": {
        "type": "string"
      },
      "clear_derived_from": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "clear_edge": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "clear_fields": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "clear_happened_at": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "clear_happened_through": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "clear_stale_after": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "clear_stands_for": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "confirmed_by_user": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "content": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "derived_from": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "details": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "fields": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "happened_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "happened_through": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "keep": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "object": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "provenance": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "recorded_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sets": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "shape": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "stale_after": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "standing": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "stands_for": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "status": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "address"
    ],
    "type": "object"
  },
  "name": "update_fact"
}
```

#### `update_fact` — argument `/inputSchema.address`

```text
The fact's global address, `kind:slug#local-id` — exactly as `recall`
returned it.
```

#### `update_fact` — argument `/inputSchema.clear_derived_from`

```text
**Take the lineage pointer off.** Its own flag, because leaving it alone
and removing it are two different edits.
```

#### `update_fact` — argument `/inputSchema.clear_edge`

```text
**Take the edge off**, leaving a claim that points at nothing. Its own
flag for the reason the others are: an edit that names neither `shape`
nor `object` says nothing about edges and leaves the one already there
alone.

**Reach for this when the rewrite turns a claim about what is true
NOW into its current negative** — was a member and is not any more,
was living somewhere and moved away. Then the edge belongs to the
sentence you just erased, and leaving it standing has every walk go on
answering through a claim that now denies it. **A rewrite that stays
positive — who returned it, where it moved to — is not this case: the
edge still describes something true.** Clearing it anyway costs a
real path: the walk through it stops answering, and the entity on the
other end becomes unreachable from this record. **A PAST EVENT THAT
TURNED OUT NEVER TO HAVE HAPPENED IS RETRACT'S CASE, NOT THIS ONE**:
retracting marks the record rather than rewriting it, and there is no
un-retract.
```

#### `update_fact` — argument `/inputSchema.clear_fields`

```text
**Fields to remove**, by key. Its own argument rather than an empty
value in `fields`: an empty value is a value somebody wrote, and
setting a key to nothing and taking the key off the record are two
different edits.
```

#### `update_fact` — argument `/inputSchema.clear_happened_at`

```text
**Take the happened-on day off**, leaving a claim that says nothing
about when the thing happened.

Its own flag, because an absent `happened_at` means the patch does not
mention it. **This is the repair for a day somebody approximated** — the
guess comes off rather than being replaced by another guess.
```

#### `update_fact` — argument `/inputSchema.clear_happened_through`

```text
**Take the far end off**, leaving a single-day claim (or an undated
one, alongside `clear_happened_at`) rather than a span.
```

#### `update_fact` — argument `/inputSchema.clear_stale_after`

```text
**Take the day off**, leaving a claim that makes no promise about how
long it stays good. Its own flag, because leaving it alone and removing
it are two different edits.
```

#### `update_fact` — argument `/inputSchema.clear_stands_for`

```text
**Take the mark off**, leaving an ordinary record that stands for
nothing. Its own flag, because leaving it alone and removing it are
two different edits.
```

#### `update_fact` — argument `/inputSchema.confirmed_by_user`

```text
Required for two promotions of an EXISTING claim: anything → testimony
(inference or observation alike), and an open standing → settled. Set it only when the user has actually
confirmed the claim. Nothing else is gated on it — a fresh `capture`
declares its provenance and its standing on honour.
```

#### `update_fact` — argument `/inputSchema.content`

```text
Replacement claim.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as the
handle that thing wears today, even after a rename.
```

#### `update_fact` — argument `/inputSchema.derived_from`

```text
**The claim this one was worked out from**, as its address
`kind:slug#local-id`.

**Lineage is learned late**, so it is set here as well as at capture: a
claim is often written before anybody notices what it rests on. The
named claim must exist.
```

#### `update_fact` — argument `/inputSchema.details`

```text
Replacement details; pass an empty string to clear them. They may hold
paragraph breaks and read back with every one. Carries `@kind:slug`
mentions exactly as `content` does.
```

#### `update_fact` — argument `/inputSchema.fields`

```text
**Fields to set**, as key/value pairs. Each key named is written; a key
the record already carries and this does not name is left alone, so an
edit reaches one field without restating the rest.

A date under `runs_out`, `decide_by` or `promised_by` makes a thing fall due on that day. `cadence_days`, `advances_from`, `counts_from` and `snoozed_until` together set when a thing falls due. `outcome` and `ended` set no day and still change what a thing owes. A `project` or `work` whose `status` is `done` is not owed, and its dates stay on it. Every other key is kept as you wrote it and nothing acts on it: it never makes a thing fall due or appear in what is owed.

`read_from`, `read_ref`, `starred`, `subject`, `purpose` and `recorded_by` describe their own claim and never fold onto the thing: read them back with `facts: true`, and find by them with a `fields` filter with `scope: record`.
```

#### `update_fact` — argument `/inputSchema.happened_at`

```text
**The day the thing this claim is about HAPPENED**, `YYYY-MM-DD`.

A different question from `date`, and **learning it late is ordinary**:
a claim written when nobody knew the day gains one here. Leaving it off
keeps whatever the record says, like every other field this patch does
not name.
```

#### `update_fact` — argument `/inputSchema.happened_through`

```text
**The far end**, `YYYY-MM-DD`, when the thing happened is a stretch of
days rather than one. Leaving it off keeps whatever the record says.
```

#### `update_fact` — argument `/inputSchema.keep`

```text
**Keep this claim exactly as it stands — the designed way to bump when
it was last touched without changing anything else.** Worth reaching
for on a thought (`capture`'s `connection` on your own bot handle):
one nobody touches for long enough goes quiet on its own, and this is
how you say, on purpose, that one still matters. The one call
this verb refuses when nothing else is named: naming no change at all
is refused unless this is `true`, and this is refused if anything
else here would actually change the claim. There is no partial keep —
it is this alone, or an ordinary edit naming what changes.
```

#### `update_fact` — argument `/inputSchema.object`

```text
The entity the edge points at, as `kind:slug`. **It must already exist** —
`add_entity` first if it is genuinely new.
```

#### `update_fact` — argument `/inputSchema.provenance`

```text
`testimony`, `observation` or `inference`. Moving a claim TO `testimony`
needs `confirmed_by_user` whichever value it held: a claim you read
somewhere is not a step towards the user having said it. An
`observation` must carry `read_from`, here as at capture — and **taking
the source off a claim that stays an observation is refused for the same
reason**, because both leave a machine read of a system nobody named.
A claim that already carries one does not have to name it again.
```

#### `update_fact` — argument `/inputSchema.recorded_at`

```text
**The day this claim was MADE**, `YYYY-MM-DD` — the day it was said,
decided or worked out. Left alone when omitted — the record keeps the
day of the claim it replaces, exactly as any field this patch does
not name.

**Give this when the correction itself was made on a different day
than the one already on the record — never the day you happen to be
typing.** Rewriting content with no date given leaves the ORIGINAL day
on the record, permanently: a correction made months later would
otherwise read back as if it had been made on the original day
forever.

**Naming a day that differs from the day your run is in is never
refused** — the receipt carries `recorded_at_note`, one sentence
naming both days, so a sitting acting out a different period does not
silently record under the wrong one by mistake.
```

#### `update_fact` — argument `/inputSchema.sets`

```text
**Fields to set on the thing this claim is about**, as key/value pairs,
apart from `fields`, which are the claim's own. A key stays in the bag it
was first written under on this record: naming under `sets` a key the
record holds as its own, or under `fields` one it holds as a setting, is
refused with the way to move it — take it off with `clear_fields`, then
write it under the other. A key that only labels the claim is never a
property of the thing and is refused here by name.
```

#### `update_fact` — argument `/inputSchema.shape`

```text
The shape of an edge to attach: `location` · `membership` · `attendance` ·
`about` · `connection` (a link is there and how it relates was not
recorded). Requires `object`; neither works alone.
```

#### `update_fact` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `update_fact` — argument `/inputSchema.stale_after`

```text
**The day after which this reading stops being good**, `YYYY-MM-DD`.

It is a fact about jojobot's knowledge rather than about the world: a
pass that runs out on a date is a claim about the world and belongs in
the content. Past this day a read SAYS SO and **nothing else happens** —
no sweep, no reminder, nobody is coming to check it.
```

#### `update_fact` — argument `/inputSchema.standing`

```text
`settled` or `open`. **Moving a claim that is ALREADY open to settled
requires `confirmed_by_user`** — the operator hedged that claim, and
only the operator can withdraw the hedge. Reopening is free.

That is the whole of the gate: it is on this promotion, not on
declaring a standing. A fresh `capture` may state `settled` and is
taken at its word, the way it is taken at its word about provenance.
```

#### `update_fact` — argument `/inputSchema.stands_for`

```text
**Mark this record as standing for the claims named here**, each as
its address `kind:slug#local-id`. A synthesis, not a citation: the
named claims stay exactly as they are — active, readable, untouched —
so the full picture is still there for anyone who reads them. A write
replaces the whole set. Every named address must already exist;
naming this record's own address, or the same address twice, is
refused — and so is an empty set, because a mark with no sources
would read back as an ordinary record, indistinguishable from one a
session never finished marking.
```

#### `update_fact` — argument `/inputSchema.status`

```text
`active` or `archived`. **Archive a claim that changed or was never
true — do not negate it.** Rewriting `content` into its own denial
("the club does NOT meet on Tuesdays" replacing "the club meets on
Tuesdays") leaves a sentence about what is not so where a claim about
what is true belongs. Set `status: archived` and give `details`
saying why, then capture the true claim as a new record — name this
one as its `derived_from` when there is a direct replacement. A
negative is still an ordinary fact when it is not correcting
anything: "he did not attend" stands on its own, written with
`capture` like any other claim.
```

### Tool `wrap_session`

#### `wrap_session` — description

```text
End your session and tell its story.
Two things happen together: the story is recorded in your chronology as its final entry, and the session moves to `wrapped` — terminal both ways, so nothing appends to it afterwards and nothing reopens it except through the wrap_code described below, and a later journal/amend_journal/wrap_session on that id comes back status: blocked.
A wrap you have to retry finishes what the first attempt started rather than repeating it, so the story is told once in each place — which means it is your chronology's newest entry only when nothing was written between the attempts.
Write the story for somebody with none of your context: what this run was for, what actually happened, what is left.
A session that stops without wrapping is not lost — the next boot of the same identity sweeps it to `abandoned` after a day, its chronology stays readable, and the run itself can be picked up again — but its story was never told, and that is the difference between the two endings.
Pass your `sid` on every call.
When the work continues but this run has gotten long, wrapping is also how you ROTATE: wrap the story, then boot again for a fresh sid.
THE CLOSING ENTRY COMES BACK AS A RECEIPT and the story is not read back to you — it is in the chronology below it, where you can see what jojobot folded into it.
THE CHRONOLOGY THIS HANDS BACK IS THE NEWEST OF THE RECORD, not all of it: a long run's answer would be one no client can read.
`entry_count` is the whole length, and `chronology_elided` with `entries_omitted` says how much is not here.
Nothing was dropped from the record — what was written is stored whole.
THE FOCUS AS IT STOOD AT THE CLOSE rides on the closing entry as `closing_focus`, beside the story rather than inside its text; every other entry has none.
THE FIRST WRAP ALSO HANDS BACK A `wrap_code`: calling start_here with that code as its resume answer hands this run's own sid back for one last change, while the run stays wrapped.
The window ends when you wrap a second time, which adds a second closing entry beside the first and mints no code, or when a newer run of the bot starts.
Until then a later journal/amend_journal/wrap_session on that id is blocked, as above.
```

#### `wrap_session` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "sid": {
        "type": "string"
      },
      "story": {
        "type": "string"
      }
    },
    "required": [
      "story",
      "sid"
    ],
    "type": "object"
  },
  "name": "wrap_session"
}
```

#### `wrap_session` — argument `/inputSchema.sid`

```text
Your session id — the session to wrap.
```

#### `wrap_session` — argument `/inputSchema.story`

```text
The story of this session, for somebody with none of your context: what
it was for, what happened, what is left. It becomes the final entry in
this session's own chronology, and goes nowhere else.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as
the handle that thing wears today, even after a rename.
```


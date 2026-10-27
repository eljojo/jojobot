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
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
If its handle or any of its names resembles something jojobot already knows, NOTHING is written: the result says status: blocked with candidates and how_to_proceed.
Use the candidate you meant, or re-call with the override_token that refusal carries if this genuinely is a different thing sharing a name — a token lifts the one refusal that minted it and no other.
A refusal on the name alone (the same name under another kind, a name that holds another, a known nickname of another, or two rare words shared with another of the same kind) also needs override_reason, shown beside the name.
An exact handle collision can never be forced — a handle has exactly one owner.
A creation whose aliases, or whose `sets` as a first claim, could not be read back whole in one answer is refused and nothing is made: send fewer or shorter aliases, or move the long text onto a claim.
A `request_id` makes a resend safe: a repeat returns the first receipt and writes nothing, and a bot whose box did not open is finished by the same call.
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
      "override_reason": {
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
      "parent": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "request_id": {
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
Screened and searched exactly as `name` is, so a nickname the operator
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
`topic`, `bot`, `pet`, `rhythm`, `promise`, `machine`, `view`, `thread`, `decision`, `role`, `voucher` and `challenge` (issued by the engine, never created here), `document`, `list`, `trip`.

**A `list` is an ordered set of short entries on the thing it is about** — a
checklist, a queue, a rotation. Make it with `parent` set to that thing, then
`capture` each entry on the list: it goes at the end, after the entry named
in `after`, or at the front with `after: "first"`, and `recall` of the list
reads its entries in order under `entries`. Its optional keys, set in
`sets`, are `tick` (how a done entry is treated: `sink` for a checklist,
`leave` for a queue, `back` for a rotation, read as `sink` when absent; an
entry is ticked with `capture`, whose `tick` names the entry and its `act`) and
`covers` (the one day, `2026-10-19`, or ISO week, `2026-W43`, a plan is for).

**A `document` holds long markdown text**: write it with `put_document`, read it
with `read_document`. Its optional keys, set in `sets`, are `one_liner` (what it
is, in a line), `stale_after` (the day past which it promises nothing) and
`resource` (the page it mirrors elsewhere).

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

**A `bot` is made by this call**, and the mailbox it owns opens in the
same act.

**A `trip` is a journey**: `departs_from` and `arrives_at` (a place
each), `leaves_on` and `returns_on` (dates).

**A `thread` is an ongoing line in somebody's life, not a `project`
and not a `topic`.** A project moves toward completion and ends when
it is DONE; a thread ends when it stops being TRUE. A topic is the glue noun
for a world-fact belonging to no person, place or project; a thread
belongs to somebody's life and is never the anchor of last resort.

**A `decision` is what was decided on a thread**, a child of it: `parent`
is the thread. Its words are an ordinary record on it, testimony when the
operator decided. Reading the thread, or a topic that holds decisions, puts
its standing decisions first, in number order when the container numbers
them. A decision that changes is a new `decision` that names the old one in
`replaces` and in a record's `derived_from`, and the old one is archived with
the reason; reading the old one says `replaced_by` with the new words, and
`standing_replacement` when that one was replaced too.

**A `role` is a seat one session holds at a time**, the child of the bot
that holds it: `holder`, `claimed_at` and `agent` say who has it, since
when and in which runtime agent. jojobot makes one the first time a role is
claimed, and no caller writes its `holder` and `claimed_at`.

**A `voucher` is a grant to do one gated act, and a `challenge` is a code
waiting to be presented on a boot. Both are issued, never created here**:
a call that names one is refused with nothing written. A session gets a
voucher by booting with the operator's code: `start_here` with `bot` and
`ask_for_code` asks for one.
```

#### `add_entity` — argument `/inputSchema.name`

```text
Display name, as a human would write it.
```

#### `add_entity` — argument `/inputSchema.override_reason`

```text
**Why you insist**, sent beside `override_token` when the refusal rests
on the name alone: the same name under another kind, a name that holds or
sits inside another, a known nickname of another, or two rare words shared
with another of the same kind. Without it that refusal stands. The reason is
kept on the new thing as `told_apart_by`, shown beside its name.
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

A `rhythm` or a `promise` requires one — a rhythm is ON something and a promise is somebody's job, and the parent is
what says on what.
```

#### `add_entity` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the entity is made once. It needs a `sid`, and a call that
wrote nothing spends no id.
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

**A container numbers what it holds** when its `sets` carries `numbers_by`
naming a key, `{numbers_by: "number"}`. Make each member under it with that
key in its own `sets` (`{number: "32"}`) and recall it by the container's
handle, a slash and the number (`topic:all-rules/32`). A number is held by
one member for good, and a repeated one is refused naming its holder.

Creating work your bot owns with `status` set to `now` also points your
bot's `working_on` at it. A write whose status words could not be read from
`topic:instance` still lands, and its receipt carries `instance_unread`,
naming under `not_worked_out` what it left undone.
```

#### `add_entity` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

#### `add_entity` — argument `/inputSchema.source`

```text
Where this entity came from — **never invented**: the operator named it, or
a real source produced it (e.g. `user-named`, `crm-card`, `calendar`).
```

### Tool `amend_journal`

#### `amend_journal` — description

```text
Rewrite your session's MOST RECENT entry that you wrote yourself, in place — for a beat you got wrong or want to finish saying.
Never a line jojobot wrote about your calls, even when it is newer.
Only the most recent one: everything older is append-only, because a journal that can be rewritten further back is not evidence of anything.
A session with no entries of its own yet comes back status: blocked rather than quietly writing your text as a first entry — an amend that silently became an append leaves a chronology saying something you did not mean.
A closed session comes back blocked too, except a wrapped run inside its 2-hour wrap window, which takes it, the closing story included.
Pass your `sid` on every call — it is the address, and it survives the fresh connection most clients open per tool call.
This verb never STARTS a session: there is nothing to amend in one that does not exist yet.
IT ANSWERS WITH A RECEIPT, NOT THE ENTRY: the id, the moment it was stamped, the byte count of what was stored and a digest of about 120 characters.
You wrote the text; a resume through start_here returns the newest entries.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
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
Text inside backticks or a fenced block is literal: a handle there is
kept as written and never becomes a link.
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
Archiving one does not free its name: a role claim at boot on an archived role object is refused until it is restored.
THIS IS FOR SOMETHING THAT SHOULD NOT BE SURFACED BY DEFAULT ANY MORE — a mistaken write, one that should not have happened, somebody not relevant at all.
It does not touch the entity's claims or edges, which stand exactly as recorded: archiving says the SUBJECT is out of scope, never that anything said about it was wrong.
FOR A DUPLICATE THAT HOLDS CLAIMS THIS IS THE WRONG VERB: it leaves the claims on a thing no default read shows, and merge_entities is the verb that carries them to the thing it duplicates.
AN ARCHIVED LOOP DROPS OUT OF overdue READS, and the read counts it in archived_excluded.
Archiving something already archived comes back blocked, saying the entity is already in the state you asked for — that answer means jojobot holds what you wanted, not that nothing happened.
Restoring something that is not archived says the same.
A handle that names nothing comes back blocked with the nearest handles.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
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
      "request_id": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
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

#### `archive_entity` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the entity is archived or restored once. It needs a `sid`, and a call that
wrote nothing spends no id.
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
Remember one claim about an entity (a handle, or `<collection>/<N>`, the member numbered N): the words, when they became true, and who backs them.
PROVENANCE: testimony (the operator said it), observation (you READ it in a system of record, so `read_from` in fields must name the system or the call is refused) or inference (you worked it out; the default, and a hypothesis, not a finding).
STANDING (settled or open) is how sure anyone is, apart from provenance, which it follows unless you set it.
A NEW THING THAT HAPPENED IS A NEW CLAIM: capture it again, never update_fact.
Every entity the call names — the subject, an edge's object, refs — must ALREADY EXIST, or nothing is written and the answer is status: blocked with candidates: add_entity first, then capture.
`fields` is a flat bag that stays on the claim (a handle, or one in a comma list, is a link and must exist; six keys describe their own claim); `sets` sets keys on the thing itself in the same call, the only bag that reaches it.
A connection claim with your OWN bot handle as subject is a `thought`, capped in length (`thought_body_cap`, else 200 with `thought_capacity`) and in count (`thought_capacity`).
In a full room a new thought needs `drop` with `drop_because`, or `borrow` once to land one over; a room over capacity takes none until you archive back under it.
Another bot's such claim is ordinary and takes no seat.
`record_capacity` caps any thing's live records; a THREAD holds 3 unless a bot above its writers sets it.
On a `list`, `after` places an entry and `tick` marks one.
`sets_add` and `sets_remove` change a list key by its items.
It answers with a receipt, not the record: its address, what was recorded and how many keys landed, read back before it is called a success.
A claim too big to read back whole is refused.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
```

#### `capture` — structure

```json
{
  "inputSchema": {
    "$defs": {
      "ConfirmedArg": {
        "properties": {
          "at": {
            "type": "string"
          },
          "on": {
            "type": "string"
          }
        },
        "required": [
          "on",
          "at"
        ],
        "type": "object"
      },
      "EntryItem": {
        "anyOf": [
          {
            "type": "string"
          },
          {
            "properties": {
              "content": {
                "type": "string"
              },
              "details": {
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
              "shape": {
                "default": null,
                "type": [
                  "string",
                  "null"
                ]
              }
            },
            "required": [
              "content"
            ],
            "type": "object"
          }
        ]
      },
      "ReceiptArg": {
        "properties": {
          "check": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "confirmed": {
            "anyOf": [
              {
                "$ref": "#/$defs/ConfirmedArg"
              },
              {
                "type": "null"
              }
            ]
          }
        },
        "type": "object"
      },
      "TickArg": {
        "properties": {
          "act": {
            "type": "string"
          },
          "entry": {
            "type": "string"
          }
        },
        "required": [
          "entry",
          "act"
        ],
        "type": "object"
      }
    },
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "after": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
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
        "default": "",
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
      "entries": {
        "items": {
          "$ref": "#/$defs/EntryItem"
        },
        "type": [
          "array",
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
      "override_reason": {
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
      "provenance": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "receipt": {
        "anyOf": [
          {
            "$ref": "#/$defs/ReceiptArg"
          },
          {
            "type": "null"
          }
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
      "request_id": {
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
      "sets_add": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "sets_remove": {
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
      },
      "tick": {
        "anyOf": [
          {
            "$ref": "#/$defs/TickArg"
          },
          {
            "type": "null"
          }
        ]
      },
      "ticks": {
        "items": {
          "$ref": "#/$defs/TickArg"
        },
        "type": [
          "array",
          "null"
        ]
      }
    },
    "required": [
      "subject"
    ],
    "type": "object"
  },
  "name": "capture"
}
```

#### `capture` — argument `/inputSchema/$defs/ConfirmedArg`

```text
Where and when the operator confirmed a claim.
```

#### `capture` — argument `/inputSchema/$defs/ConfirmedArg.at`

```text
Where: a message id or the session.
```

#### `capture` — argument `/inputSchema/$defs/ConfirmedArg.on`

```text
The day the operator confirmed it, `YYYY-MM-DD`.
```

#### `capture` — argument `/inputSchema/$defs/EntryItem`

```text
**One entry of a batch**: a bare string is the entry's words, and an object
adds what a single entry can carry beside its words.
```

#### `capture` — argument `/inputSchema/$defs/EntryItem/anyOf[0]`

```text
The entry's words.
```

#### `capture` — argument `/inputSchema/$defs/EntryItem/anyOf[1]`

```text
The entry's words, with its own details or its own edge, so a checklist
step can link to a piece of work.
```

#### `capture` — argument `/inputSchema/$defs/EntryItem/anyOf[1].content`

```text
The entry's words, one line, as `content` is for a single entry.
```

#### `capture` — argument `/inputSchema/$defs/EntryItem/anyOf[1].details`

```text
The description under the entry, as `details` is for a single entry.
```

#### `capture` — argument `/inputSchema/$defs/EntryItem/anyOf[1].object`

```text
The entity this entry's edge points at, as `object` is for a single entry.
```

#### `capture` — argument `/inputSchema/$defs/EntryItem/anyOf[1].shape`

```text
This entry's edge shape, as `shape` is for a single entry.
```

#### `capture` — argument `/inputSchema/$defs/ReceiptArg`

```text
What a claim was settled on.
```

#### `capture` — argument `/inputSchema/$defs/ReceiptArg.check`

```text
**A check that ran**: what was checked, written so another agent can run
it again. It records that a check ran and never runs it.
```

#### `capture` — argument `/inputSchema/$defs/ReceiptArg.confirmed`

```text
**The operator confirmed it**: the day, and where (a message id or the
session).
```

#### `capture` — argument `/inputSchema/$defs/TickArg`

```text
**A tick on a list's entry**: which entry, and what was done to it.
```

#### `capture` — argument `/inputSchema/$defs/TickArg.act`

```text
One of `done`, `undone`, `rotated` or `dropped`. `done` does what the
list's `tick` setting says: a `sink` list keeps the entry in view at the
bottom, a `leave` list takes it out of the open view, and a `back` list
turns it to the end, still open. `undone` brings a done or dropped entry
back, but never to undo a turn on a `back` list. `rotated` is a turn, for a
`back` list only.
`dropped` says the entry no longer applies, and it never reads as done.
**A tick is refused when it changes nothing, or when the list cannot take
it**: `undone` on an open entry, `done` on an entry that is already done,
`rotated` on a list that is not `back`, and any act but `undone` on a
dropped entry. The refusal names the act that lands instead.
```

#### `capture` — argument `/inputSchema/$defs/TickArg.entry`

```text
The address of the entry, `list:packing#f2`, exactly as `recall` serves it
under `entries`. It must be an active entry of the list the capture is on.
```

#### `capture` — argument `/inputSchema.after`

```text
**Where a new entry goes on a list**, when `subject` is a thing of kind
`list`: the address of an entry the new one follows
(`list:packing#f2`, exactly as `recall` serves it), or the word `first`
for the front or `last` for the end. Absent means the end. The server mints each entry's
order key, so there is no number to pick.
Refused on any subject that is not a list, and refused when the address is
not an open entry of this list; the refusal says what to send instead.
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

**A thread holds three live records unless a bot above its writers
sets `record_capacity` on it**, and a capture past that is refused
with nothing written. The refusal names four routes: `rewrite` a record in place with
`update_fact`, `archive` one, `split` into a child thread, or move long
text to a `document`. `borrow` lands one record over the ceiling, once.
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
and the rhythm comes back then and is acted on again.

**jojobot does the arithmetic.** It writes `outcome`, `last_check_in`
(the day of this check-in, `recorded_at`, today when you send none) and,
when the cycle is consumed, the new
`counts_from` — worked out from `advances_from`, or its default. Do not
compute those yourself and do not send them in `fields`.

**A snooze names the day it lasts until**, as `snoozed_until` in
`sets` (`YYYY-MM-DD`), a day after this check-in's own, which is
`recorded_at` (today when you send none). The rhythm is
not owed again before that day, and on it the rhythm comes back and is
acted on again; it falls due on the later of that day and its own. A
snooze with no day, or a day that is not after that day, comes back
blocked and nothing is written. The next `ran` or `skipped` check-in
ends the snooze, so send `snoozed_until` with `snoozed` only.

**What the check MEASURED is yours to send** in `fields`: a reading, a
distance, a count. A cadence is always time, so a measurement is a field
on the check-in and never a unit of the schedule.

A rhythm short of a cadence (`cadence_days` or `rrule`), stored or sent in
this call's `sets`, comes back blocked naming both, and nothing is written. A rhythm
short only of `counts_from` still takes a `ran` or `skipped` check-in —
that outcome OPENS the rhythm, and the basis becomes this check-in's own
date. `snoozed` cannot open a rhythm, so it still refuses on a
`counts_from` the rhythm does not hold.
```

#### `capture` — argument `/inputSchema.content`

```text
The crisp claim to remember — single line, no line breaks. Put the rest in
`details`.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as the
handle that thing wears today, even after a rename.
Text inside backticks or a fenced block is literal: a handle there is
kept as written and never becomes a link.

A claim too big to be read back whole in one answer is refused with nothing
written: shorten it, split it into claims that each say one thing, or keep
the long text on a document (`put_document`) and write a pointer claim.

Required for one claim. Leave it off when you send `entries` or `ticks`.
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

#### `capture` — argument `/inputSchema.entries`

```text
**Add many entries to a list in one call**, instead of `content`: each
item is the entry's words, or an object with `content` and, if the entry
needs them, `details`, `shape` and `object`. The subject is the list.
Every entry is a claim of its own with its own address and shares this
call's provenance and standing, and they land in the order sent: at the
end, or as one run behind the entry `after` names. The receipt lists the
address of each, by position.

**All or nothing for anything you can fix.** Every item is checked first,
by the same rules a single entry meets; one that fails refuses the whole
call, names the item by its position and writes nothing. At most 50 items
a call. Send `request_id` on the first call: if a store failure stops the
batch partway, the receipt says how many landed, and the same call again
with the same `request_id` writes the rest and none twice. Not beside
`content`, `tick` or `ticks`.
```

#### `capture` — argument `/inputSchema.fields`

```text
**The record's fields**, as a flat bag of key/value pairs — anything
worth recording about this claim beyond the sentence.

Flat and free-form on purpose: jojobot stores what you put here, acts
only on the keys the build names, and keeps a key it has never seen
exactly as you wrote it. **A value that is a handle, or an item of a
comma list that is a handle, is a link and must name something that
exists**; the loose items beside it stay text, and a handle inside a
longer sentence is prose. **Nothing has to be declared first**, and no
name for the class of thing is asked for: the fields ARE what the record
says, and a type is something the keys answer rather than something you
announce.

**`"starred": "true"` competes for one of the few seats a bot's own
boot spends on its rules, and it stays on the claim: it never folds onto
the thing.** An unstarred rule is fetched with `facts: true` when it is
needed rather than shown by default. Seats are few on purpose — starring
everything is the same as starring nothing — and **a star into full
seats is refused, naming the seated rules: unstar one to make room.**
**How many seats
a bot has is `rule_seats`, written about it by a different identity** —
a bot cannot raise its own — and a star or a count that would leave
the boot over its size ceiling is refused, naming the overage.

**Six keys describe their own claim and never fold onto the thing:**
`read_from`, `read_ref`, `starred`, `subject`, `purpose` and `recorded_by`.
Read them back with `facts: true`; find by them with a `fields` filter and
`scope: record`. A field you send that equals what jojobot ships as its
default today is stored exactly as you sent it either way, and the receipt's
echoes_defaults names which key that was.

**A pass, ticket, membership or registration somebody holds** is a claim on
the HOLDER, with `admits` (the handle of the event, place or
organisation it opens), `valid_from` and `valid_until` (days; no window
reads as live) and optionally `tier`, all in `sets`. That an event offers a tier is a claim
on the event instead. Holding is not attendance, not wanting to go and not a
calendar entry: each is its own claim. Recall the holder with facts to read
what they hold, or the event for its `held` block, which ranks who holds
what, live first, on the day it names in `as_of`.

`read_from`, `read_ref`, `starred`, `subject`, `purpose` and `recorded_by` describe their own claim and never fold onto the thing: read them back with `facts: true`, and find by them with a `fields` filter with `scope: record`.
```

#### `capture` — argument `/inputSchema.happened_at`

```text
**The day the thing this claim is about HAPPENED**, `YYYY-MM-DD` —
a different question from `recorded_at`, which is about the claim.

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

#### `capture` — argument `/inputSchema.override_reason`

```text
**Sent beside `override_token`**, which this verb does not hand out at
present. Leave it off.
```

#### `capture` — argument `/inputSchema.override_token`

```text
**Nothing on this verb hands out a token at present**, so there is nothing
to send here. Leave it off. The refusals that carry a token are the ones
on `add_entity`, `update_entity`, `rename_entity` and `declare_type`.
```

#### `capture` — argument `/inputSchema.provenance`

```text
`testimony` (the operator said it), `observation` (you read it in a system
of record) or `inference` (you derived it). Defaults to `inference`:
anything not tied to the operator's words is a hypothesis.

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

#### `capture` — argument `/inputSchema.receipt`

```text
**Why this claim is settled**, when you declare `settled` on an
inference. Exactly one of two shapes: `check`, what was checked, written
so another agent can run it again; or `confirmed`, the operator said so,
as `{on: YYYY-MM-DD, at: a message id or the session}`. Never required
here: a capture is not a promotion. Provenance stays what you declared.
```

#### `capture` — argument `/inputSchema.recorded_at`

```text
**The day the claim was MADE**, `YYYY-MM-DD` — the day it was said,
decided or worked out. Defaults to the day your run is in.

Not the day the thing happened: that is `happened_at`, and it is
separate because one field answering both is how a vague answer becomes
an invented date.

**Naming a day that differs from your run's stated day is never
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

#### `capture` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time. It needs a `sid`, and a call that wrote nothing
spends no id.
```

#### `capture` — argument `/inputSchema.sets`

```text
**What this claim sets on its thing**, as key/value pairs: the keys that
are properties of the thing the claim is about, apart from `fields`, which
are the claim's own. One call records the claim and sets the thing. A key
belongs to one of the two for as long as this record holds it, so a key
named here and in `fields` is refused, and a key the record already holds
in the other is refused with the way to move it. A key that only labels
the claim (`read_from`, `read_ref`, `starred`, `subject`, `purpose`,
`recorded_by`) is never a property of the thing and is refused here by name.
The keys that make a thing fall due act on it only when sent here. A list
key (`depends_on`) changes by its items with `sets_add` and `sets_remove`:
its whole value sent here replaces what another session added. A key
comes off the thing again with `update_fact`'s `clear_fields` on the
record that carries it. Setting `status` to `now` on work your bot owns
also points your bot's `working_on` at it.
A write whose status words could not be read from `topic:instance` still lands, and its receipt carries `instance_unread`, naming under `not_worked_out` what it left undone.

**A container numbers what it holds** when its `sets` carries `numbers_by`
naming a key, `{numbers_by: "number"}`. Every thing under it, at any depth,
that sets that key holds a whole number no other holds, and keeps it after
it is archived. Number a member in its own `sets` (`{number: "32"}`) and
recall it by the container's handle, a slash and the number
(`topic:all-rules/32`). The container's own recall says its `next_number`,
and a number already held is refused naming its holder and the free number.

A date under `runs_out`, `decide_by` or `promised_by` in `sets` makes a thing fall due on that day; `ended` beside one ends it. `cadence_days`, `rrule`, `advances_from`, `counts_from`, `anchored_on` and `snoozed_until` together set when a thing falls due; `outcome` beside them changes it. A `project` or `work` whose `status` is `done` is not owed, and its dates stay on it. Every other key is kept as written and acts on nothing: it never makes a thing fall due or appear in what is owed.
```

#### `capture` — argument `/inputSchema.sets_add`

```text
**Items to add to a list key on the thing**, as key/value pairs where the
value is the items, separated by commas: `{depends_on: "work:gamma"}`.
Accepted only for a key declared a list. The thing's current value is read
inside this call and the items are appended to the end, in the order sent;
an item it already holds is not added twice, and every other item stays
where it was, so nothing is resent. A handle that names nothing is refused
with the nearest handles, as under `sets`. The answer is stored as an
ordinary whole value. Naming the same key in `sets` is refused.
```

#### `capture` — argument `/inputSchema.sets_remove`

```text
**Items to take off a list key on the thing**, in the form `sets_add`
takes. The rest of the list stays where it was. An item the list does not
hold is no error, and neither is a handle that no longer names anything,
so a reference to a thing that is gone can come off. With `sets_add` in one
call, the removals are applied first.
```

#### `capture` — argument `/inputSchema.shape`

```text
The shape of the edge this fact draws: `location` (object is a place) ·
`membership` (an org) · `attendance` (an event) · `about` (any kind) ·
`connection` (any kind — a link is there and how it relates was not
recorded). Requires `object`; neither works alone. A read serves the
shape by its schema.org word: `connection` reads back as `relatedTo`,
`membership` as `memberOf`, `attendance` as `attendee`, and `location`
and `about` as written.

**Draw `connection` with the subject as YOUR OWN bot handle and the
claim becomes a thought of yours, not an ordinary claim about an
entity.** A connection claim another bot writes on your handle is an
ordinary claim: it takes no seat and a full room does not refuse it.
A thing carrying `thought_capacity` holds only that many
thoughts at once. A write that would go over is refused, naming how
many are held and asking you to name one to drop and why, or to archive
when the room is already over — see `drop`, below. One nobody touches for long enough goes quiet on its
own: still there, just no longer counted. How long is counted in the
bot's runs: its own `thought_ages_after_runs` setting, twenty when it
has none. `update_fact`'s `keep: true` is the designed way to say one
still matters, on purpose, without changing a word of it.

**`thought_capacity` is a ceiling, and the thing it binds cannot set
it.** Send this key about your own handle and the write is refused,
whatever else the call carries — only a different identity may raise
or lower it. The same holds for `thought_ages_after_runs`.

A thought's content is capped at the bot's own `thought_body_cap`; with none,
at 200 characters if the bot carries `thought_capacity`, and a bot with
neither is not capped. A longer one is refused. Put the substance on the
thing it points at and keep the thought short.
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

It is a fact about jojobot's knowledge rather than about the world: a
pass that runs out on a date is a claim about the world and belongs in
the content. Past the day a read SAYS SO and **nothing else happens**:
no sweep, no reminder, nobody is coming to check it.

Optional, and most claims never carry one. **The claim does not stop being
true, it stops being trusted.** **A claim carrying no day made no
promise**, which is neither fresh nor stale.
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
you to confirm a standing. `update_fact` is where the gate sits, on two
moves of an EXISTING claim: anything to `testimony`, and an open
standing to `settled`, which takes a `receipt`.
```

#### `capture` — argument `/inputSchema.subject`

```text
The entity the fact is about — any `kind:slug` id (a bare handle is read
as a person), or `<collection>/<N>` for the member of a numbered
collection holding the number N. **It must already exist**: a subject jojobot doesn't know
comes back with candidates and nothing is written. Create it with
`add_entity` first if it is genuinely new.
```

#### `capture` — argument `/inputSchema.tick`

```text
**Tick an entry of a list**, instead of adding one: `{entry, act}`. The
subject is the list. One record lands on the list naming the entry, the
act, the session and the day; the entry itself is never edited, so its
words, author and provenance stay as they were. The list's own `tick`
setting says what `done` does, and `recall` of the list serves the order
and each entry's `state` computed from its ticks. `content` is the note
that goes with the tick. The receipt's `ticked` names the entry and the
act, and for an entry that points at a piece of work, `linked` says
whether that work is still open: a tick says the list item is done, never
that the work is, and the work's status is left as it was. A tick that
changes nothing is refused, with the act that lands named in its place (see
`act`). When that work could not be read, the tick still landed and the
receipt carries `linked_unread` in place of `linked`: an absent `linked` is
not work with no state. A retried tick
under one `request_id` lands once, so a rotation never turns twice. `recall` with `ticks` true reads the
ticks. Refused on any subject that is not a list, beside `after`, and for
an entry that is not an active entry of the list.
```

#### `capture` — argument `/inputSchema.ticks`

```text
**Tick many entries of a list in one call**, instead of `tick`: each item
is `{entry, act}` exactly as `tick` takes one. The subject is the list.
Each item is one tick record, as a single tick is, and each is judged
against the list as the ticks before it in the batch leave it. One item that fails refuses
the whole call, names it by its position and writes nothing: that includes
a tick that changes nothing, such as `done` twice on one entry with no
`undone` between, which is refused whole, while `done`, `undone`, `done` in
that order lands as three ticks. At most 50
items a call. A retried batch under one `request_id` lands once, so a
rotation never turns twice. Not beside `entries`, `tick` or `after`.
```

### Tool `declare_type`

#### `declare_type` — description

```text
Declare a type: a name, and the keys a THING of it carries.
Its `fields` are KEY DEFINITIONS — each a key with optional `holds`, `folds`, `required` and `one_of` — not the key/value bag capture and update_fact take as `fields`, nor the key filters recall takes.
Keys are WRITE-TIME HELP, never a gate — they tell writers which keys to fill and `search` which to look for.
THE UNIT IS THE THING: a thing answers a type over every write on it, folded, so a thing described over two sittings answers a type neither sitting answers alone.
It admits nothing: a thing carrying these keys is found by search answers_type whether or not anybody declared it, and a thing carrying some of them is found too, saying which it lacks.
So declaring a type never makes a thing findable and never stops one being found, and there is nothing to undeclare.
A type arrives COMPLETE: give at least one key, because a name with nothing under it is a type nobody can query by.
Declaring a name that already exists REPLACES its keys, whole — a type is the keys it names now, and one that accumulated every key it ever named would report keys the writer had already dropped as keys a thing lacks.
KEYS ARE REGISTERED NOWHERE; what a key holds is the type's, HOW IT FOLDS IS THE INSTANCE'S: a counter in any type adds up on every thing holding the key, so a declaration that changes how a held key reads is refused first, with the count (override_token and override_reason lift it).
SOME TYPES SHIP WITH THE SOFTWARE and those are CLOSED: declaring over one comes back blocked, because a caller cannot extend, shrink or replace a type the software owns — declare a name of your own instead.
Every type you declare is yours.
The answer names every type that exists, by name and origin, so you can see what is there without asking again.
The receipt's `changed` lists changes in the type, or is `nothing` or `not_read`.
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
      "override_reason": {
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
      "request_id": {
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

⚠️ **And to the whole instance, not to the type that names it.** A key any
type declares a counter adds up on every thing that holds it. A declaration
that would change how a key already held reads back is refused first, with
the count of things it would change.
```

#### `declare_type` — argument `/inputSchema/$defs/FieldArgs.holds`

```text
What the value holds: `text`, `number`, `date`, `date_range`, `period`,
`boolean`, `reference` (another entity's `kind:slug` handle, which is what
makes it walkable) or `expression` (a line in the expression recall's
`where` takes). Defaults to `text`, which holds anything — or to `number`
when you declare the key a counter, because a total of anything else is
not a total.

**A `date_range` is ONE value, not two keys**: two dates with a slash
between them, `2026-04-18/2026-04-25`. Two keys cannot say they belong
together, so a thing carrying a start and no end reads as a thing with a
key missing rather than as a span nobody finished. A `period` is one day
(`2026-10-19`) or one ISO week (`2026-W43`).

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

**What you say here describes; it does not gate.** A key's `holds`
refuses no write — a value that does not hold what the key declares comes
back flagged on the hit, so a read shows the mistake rather than a write
being turned away.
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
a write outside the set is refused with the values named. **On a type you
declare here the set refuses no write**: the
set is what a reader is told about a value that does not match, so a thing
carrying one comes back in an `answers_type` read with that key reported
and the values named.

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
makes the thing what it is: the required keys are the floor a WRITE to a
kind may not take away.

⚠️ **The reader's strict question is wider than the floor.** `fits_type`
(*which of these ARE one*) keeps only the things holding EVERY key the
type declares, optional ones included; `answers_type` keeps those holding
SOME and names what each lacks. An optional key is never demanded of a
write, and a thing without it is a complete thing, but not one `fits_type`
keeps.

⚠️ **This says nothing about what the key HOLDS.** The two are separate
questions, and the useful one is usually "the colour has to be a colour"
rather than "everything must have a colour".

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

Six keys label a claim and cannot be declared in a type: read_from,
read_ref, starred, subject, purpose and recorded_by. They stay on the
claim whatever a type says, so a declaration that names one is refused.
Name a key of your own.
```

#### `declare_type` — argument `/inputSchema.name`

```text
What the type is called. Declaring a name that already exists
**replaces** its keys — a type is the set of keys it names now.

**Unless the software ships that type**, which is refused: a shipped
type is closed and the answer says so. Pick a name of your own.
```

#### `declare_type` — argument `/inputSchema.override_reason`

```text
**Why you insist**, in words, sent beside `override_token`. A token with no
reason lifts nothing: the reason is what makes the override a deliberate act.
The receipt answers it back as `override_reason` and lists each key whose fold
moved as `fold_moved` (`key`, `things`, `now`); the server log keeps it with
the type, the keys whose fold moved and how many things hold each.
```

#### `declare_type` — argument `/inputSchema.override_token`

```text
**The token a refusal handed you**, sent back after you read it. A
declaration that would change how a key reads on things that already hold
it is refused first, naming the key and how many things would change; this
token lifts that one refusal and no other. It covers the exact declaration
that was refused: change a field and send it again, and the refusal comes
back with a new token. Send it with `override_reason`.
```

#### `declare_type` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the type is declared once. It needs a `sid`, and a call that
wrote nothing spends no id.
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
High-level: what you did, what you found, what went wrong — except a resume note, left in case this run goes quiet without wrapping, which is dense and specific.
Work that must move to another agent now is a wrap, not a note.
Not every tool call, not every file: a reader months from now wants the story, and a firehose buries it.
`focus` rewrites what your session says it is working on RIGHT NOW, in place — the chronology is history, the focus is the present, and they answer different questions.
The first journal entry (or the first write of any kind) is what brings your session's record into being; a boot alone keeps only a note of its key.
PASS `sid` — the session id the boot door gave you — ON EVERY CALL; it is the only address, and it is what tells jojobot which bot is writing.
A `sid` whose session is closed comes back status: blocked: a closed session takes no more entries, whichever end it reached — except a wrapped run whose wrap window is open, which still takes them.
The two ends part company on what comes NEXT — a run that stopped without being wrapped up is offered back at your next boot for 7 days, and resuming it continues this same record, while a wrapped one is the last word — its story is told and nothing appends to it except through the wrap_code its wrap handed back, so carrying on means a fresh session.
IT ANSWERS WITH A RECEIPT, NOT YOUR BEAT: the id the entry was given, when it was stamped, the run it landed in, the byte count of what was stored and a digest of about 120 characters.
You wrote the entry; a resume through start_here returns the newest entries and says how many older ones it left out.
IF YOUR SESSION HOLDS A CLAIMED ROLE, this beat renews it, under the same rule as any other write — the lease is not journal's alone to keep fresh.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
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
      "request_id": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "sid": {
        "type": "string"
      },
      "working_on": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
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
Text inside backticks or a fenced block is literal: a handle there is
kept as written and never becomes a link.
```

#### `journal` — argument `/inputSchema.focus`

```text
Optional: omit it to keep the session's current focus. What you are
working on NOW, in one line (200 characters at most, no backtick). When
sent it **replaces** the current focus, and a blank one is refused. Carries
`@kind:slug` mentions as `entry` does.
```

#### `journal` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the beat**, because a call sent without
one cannot be repeated safely. A beat is made of parts, the entry, the
focus it moves and the thing you say you are working on: if an answer is
lost or a part fails, send the same call again with the same
`request_id` and the parts that landed are not written again and the rest
are, so the entry lands once.
```

#### `journal` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. A session is
bound to the bot that booted it; there is no way to write into another
one.
```

#### `journal` — argument `/inputSchema.working_on`

```text
**The thing your bot is working on, as its handle** (`work:sigma`), shown
to any reader of the bot while this session is live. An empty string
takes it off. Optional; moving work your bot owns to `now` with `capture`
or `update_fact` points it for you.
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
WHOSE RUNS IS NOT AN ARGUMENT: the `sid` you pass says which bot is asking, exactly as it does for journal, and there is no way to name a different identity's runs.
It is a plain read: nothing here is begun, closed or swept.
A bot with no runs comes back with an empty list.
A run counted since the counts began carries `calls` and `refused`; recall of one run adds `calls_<verb>`, `refused_<verb>` and `refused_<verb>_<word>`.
Each run carries its name (`run`, the name start_here takes to resume the run once it has gone quiet; a wrapped run reopens only through its wrap_code — never its key, which a list does not show), what it was working on last, its state (active · wrapped · abandoned), when it began, when it last had anything to show for itself, and how many beats its chronology holds — never the chronology itself.
For a run's chronology without resuming it, read recall with kind: session and prose: true.
Read a live or abandoned run's beats by resuming it through start_here; the most recently wrapped run's closing story arrives there too, as the handover.
Each run also carries `served_since_compaction` and `served_lifetime`, which replace `served` (the characters answered since its last compaction, and over its whole life, each with a token estimate, never a real tokenisation), and `compactions`, how many it reported.
A compaction resets served_since_compaction; the weekly totals are kind usage (view: usage).
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
        "minimum": 1,
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
repeat the same call with it. Defaults to 0. An offset past the last run
answers no runs and a `past_the_end` note naming it, so an empty page is
never mistaken for a bot that has no runs.
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
It answers whether something you sent arrived and whether anyone has read it, which no other verb can answer without taking delivery of the box you posted into.
A `to` that names no colleague comes back status: blocked with candidates, never an empty list, because an empty list would read as 'it never arrived'.
Messages jojobot cannot read are reported separately under `unreadable`: it cannot tell who sent them, so one of yours could be there.
Newest first, each with its state (`new` = nobody has picked it up · `read` = delivered, not yet finished with · `processed` = acted on) plus notes when the consumer recorded an outcome; a message a read or a post delivered carries `taken_by` (`reading` = the owner opened the box or took that message by id · `posting` = it rode back with the answer to a post of the owner's own); absent says nothing was recorded, never that nobody looked, since a message processed straight from `new` and one delivered before the record existed have none.
Bodies are left out unless you ask for them — you wrote them — so each carries body_bytes and a digest of about 120 characters and says body_elided: true.
MAIL TO THE OPERATOR is listed by id, time and subject only, even with include_bodies, because no bot reads the operator's box back.
Twenty newest by default; an answer stops at the ceiling, never mid-message, and counts what it left out under not_shown with the `offset` that reads the next page.
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

Each message may also carry `sender_mail_waiting_at_send`, stamped once when
it was posted; see `post_message`'s `sid` for what it means (`null` is
unknown, never zero).
```

#### `list_sent` — argument `/inputSchema.limit`

```text
How many messages to return, newest first. Defaults to twenty. This is
the size of a page: an answer that would pass the answer ceiling carries
fewer and says so, and `offset` reads on.

Twenty newest by default: `limit` is the size of a page.
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

Omit it for your own mail: your `sid` already says who that is. Pass one to
ask after somebody else's outgoing mail; it is matched exactly against the
bot handle recorded on each message (`bot:gamma`), which is allowed, because
where a message got to is not private to its sender.
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
A FAILURE IS DATA, NOT A STATE: record it in notes (and reply with a new message if someone needs to know); there is no failed status, because a message whose handling failed has still been handled.
When a message asks nothing of you, READING IT IS THE ACTING: process it with a note and move on.
A message can be processed straight from `new`.
TO CLOSE MANY AT ONCE send `message_ids` instead of `message_id`; its description says how a refusal and a failure part-way read.
A reply you post with in_reply_to closes the message it answers only in your own box.
The answer confirms the move (for many: processed, ids, already_processed, notes) without echoing the body, which read_message returns unchanged for a processed message.
A refusal is an answer with status: blocked and nothing written, in three kinds: an id that names nothing (use one read_mailbox or post_message handed you); an item jojobot cannot read, which says why (no retry helps: a person has to repair it, and until then treat what it carried as unhandled and say so); and a message that is not yours to retire — one in another bot's box, or in the operator's, which no bot reads.
TO QUARANTINE A MESSAGE INSTEAD OF RETIRING IT — a decision, not damage — pass `quarantine` with your reason and leave `notes` unset; there is no verb that lifts a quarantine.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
```

#### `mark_processed` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "message_id": {
        "default": "",
        "type": "string"
      },
      "message_ids": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
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
      "request_id": {
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
  "name": "mark_processed"
}
```

#### `mark_processed` — argument `/inputSchema.message_id`

```text
The message's id, exactly as `read_mailbox` returned it. Send this or
`message_ids`, one of them.
```

#### `mark_processed` — argument `/inputSchema.message_ids`

```text
**Many messages closed in one call**, each id exactly as `read_mailbox`
returned it, up to 200. **All or nothing for a refusal**: an id that names
nothing, that is in another box or that cannot be read refuses the whole
call, names that id and closes none. **A store failure part-way is not a
refusal**: the messages already closed stay closed, and the same call sent
again closes the rest. A message already processed is not an error and is
left as it is, counted under `already_processed`. `notes` is recorded on
every message the call closes. Not beside `quarantine`.
```

#### `mark_processed` — argument `/inputSchema.notes`

```text
What happened — including a failure. Optional, one plain line.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as
the handle that thing wears today, even after a rename.
A note is one plain line, so no code is literal in it: one with a
backtick or a line break is refused and nothing is written.

Write the outcome you actually have: a note longer than the record holds is
CUT to fit and says so (a trailing ellipsis, and a delta naming what was
stored), never refused, because the verb that retires a message will not
fail over the length of its own record.
```

#### `mark_processed` — argument `/inputSchema.quarantine`

```text
**Quarantine this message instead of retiring it — a decision, not
damage.** Give the reason here and the message becomes unreadable from
every future delivery, exactly like storage damage, except the refusal
names who decided and why instead of asking for repair. Only your own
mail: naming a message sitting in another box is refused, and nothing
is written. **A processed message is never quarantined**: it is an
archive and stays readable, and the call is refused. Mutually exclusive
with `notes` — a quarantine records no
processing outcome, because none happened.
```

#### `mark_processed` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the message is retired once. It needs a `sid`, and a call that
wrote nothing spends no id.
```

#### `mark_processed` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking, and a retire
or a quarantine acts on the box that bot owns, so a call with none is
refused.
```

### Tool `merge_entities`

#### `merge_entities` — description

```text
Merge a duplicate into the thing it duplicates — the repair for one thing named twice.
A deliberate act rather than a flag on an edit, because there is no way back.
The survivor answers for the duplicate's claims, the edges drawn at it, the history under its keys, and its names, listed as `foldedNames`; its prose stays behind.
A dated record of the merge lands beside them.
NOTHING IS DELETED: the duplicate's handle keeps answering, so a handle written down anywhere still resolves, but a recall of it comes back with status merged, names the survivor in merged_into and holds nothing of its own.
It does not answer as the survivor: recall the survivor to read everything.
USE IT WHEN TWO HANDLES ARE ONE THING.
It does not ask whether they really are: that is your judgement, so putting two unrelated things together is allowed and is recorded exactly as legibly.
If you are unsure, read both with recall first.
THE SURVIVOR SERVES THE DUPLICATE'S CLAIMS UNDER ITS OWN HANDLE, each under a new address.
An address you were holding for one of them still names it: update_fact, retract and recall with history_record act on the claim where it is now.
The answer's claims_moved counts the claims the survivor serves under its own handle now.
TWO DOCUMENTS MERGE INTO ONE: the duplicate's sections follow the survivor's last.
A document and a thing do not merge.
A refusal is status: blocked with nothing moved, and the answer says why and who may; among them: one handle named as both sides; a thing folded into its own descendant; a handle already merged away; a role object merged by anyone but its owner or a bot heading the chart; a thought room or thought ceiling the merge would break; a number the merge would lose; or a result that could not be read back whole in one answer.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
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
      "request_id": {
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
The survivor answers for everything it holds, and the handle itself goes
on resolving.

A handle already merged away is refused, and the answer names the handle
to use instead. A role object (kind role), on either side, is merged only
by the bot it belongs to or a bot heading the chart, and the answer tells
you who may.

A merge into your own bot is refused when the duplicate carries a key a bot
cannot set about itself (thought_capacity, thought_body_cap,
thought_ages_after_runs, record_capacity, rule_seats, claims_role,
reports_to), because the merge would set it on you: have a different
identity merge it, or take the key off the duplicate first with update_fact
and clear_fields. A connection claim the survivor's own bot wrote counts
as its thought, and a claim another bot wrote arrives as an ordinary claim
that takes no seat. A merge that would leave a room over its capacity
(every thought that counts is held, none ages out), or bring a thought over
its body cap, is refused with nothing moved: archive thoughts, or shorten
the one named, with update_fact, then merge again.
```

#### `merge_entities` — argument `/inputSchema.reason`

```text
Why the two are one thing, in one line. Optional — worth giving: this is
the one call that destroys structure, and an account of why is what
makes it readable a year later. Left out, the record says plainly that
no reason was given rather than inventing one. The dated record of the
merge names the handle that went and this reason.
```

#### `merge_entities` — argument `/inputSchema.recorded_at`

```text
**The day this merge was made**, `YYYY-MM-DD`. Defaults to the
day your run is in.
```

#### `merge_entities` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the two are merged once. It needs a `sid`, and a call that
wrote nothing spends no id.
```

#### `merge_entities` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it.
```

#### `merge_entities` — argument `/inputSchema.survivor`

```text
**The handle that survives.** Everything the duplicate held is read from
here.

A merge that carries record_capacity (or another ceiling) onto a thread is
judged as writing it: a bot is one of a thread's writers when it keeps a
thought in it or wrote one of its live records, a writer may not raise a
ceiling by merging, and a bot above its writers may.
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
The session id you are carrying, if you have one — the same `sid`
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
`to` must come down to exactly one box jojobot can read: a bot by name or handle, or the operator by their person handle (`person:` and the slug; your first post to them opens their box and no bot reads it back).
A name no colleague answers to, or a colleague whose mail is missing or doubled, comes back status: blocked with nothing written, and the answer says which.
A post to a folded bot goes to the bot it was folded into; the receipt's `folded` block says so.
Give it a `subject`: one line on what it is about, which a reader sees on a listing before opening anything.
A body too long to be handed to its reader whole is refused with nothing written: split it, or put the text on a document (put_document) and send its address.
To correct or withdraw a message you sent, post again to the same box with `supersedes` set to its id: the reader is handed the new one, and the old one, if unread, is not handed over as fresh mail.
IT ANSWERS WITH A RECEIPT, NOT THE MESSAGE: the id that list_sent shows and `supersedes` names, the state as it stands (`read` already is success), body_bytes and body_elided: true.
Your body is not echoed back; list_sent with include_bodies returns it, taking no delivery.
POSTING ALSO DELIVERS: mail in YOUR OWN box that no read has handed you yet rides back under your_mail, out of `new` and yours to finish.
Mail an earlier read already handed you is not listed again: it is counted under your_mail.leftovers and still owed, and mail replaced before you read it is counted under your_mail.superseded; read_mailbox lists both, and read_message returns one by id.
The sender is the bot behind the `sid` you pass, never one you declare, and a sid jojobot is not holding comes back status: blocked with nothing written.
The receipt's `changed` lists what moved.
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
      "request_id": {
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
      "supersedes": {
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
Text inside backticks or a fenced block is literal: a handle there is
kept as written and never becomes a link.

To quote the operator, write the address of the claim that holds their
words, `@kind:slug#fN`: jojobot writes those words in beside it, and
refuses a claim they did not say themselves.
```

#### `post_message` — argument `/inputSchema.in_reply_to`

```text
The id of the message this one answers, when it answers one. Optional.
It must name a message that exists — a miss comes back blocked and
nothing is written. **A reply closes what it answers**: that message is
processed, with a note naming this reply, when it is in your own box, and
nothing changes when it is in another. **It also closes your older messages
on the same thread** in the box this one lands in that the reader has read
(one still new is left; `supersedes` replaces it). The receipt names what
closed, and what could not be closed, under `closed`. Messages riding back
under your_mail carry `written_by_other_run` when a different run posted them,
exactly as read_mailbox's `new_only` argument says.
```

#### `post_message` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the message is posted once. It needs a `sid`, and a call that
wrote nothing spends no id.
```

#### `post_message` — argument `/inputSchema.sid`

```text
**Your session id.** Required here, because it is what jojobot records
as the sender: a message from nobody is a message nobody can reply to,
and identity that is merely declared is identity that can be wrong.

The message carries `sender_mail_waiting_at_send`: what was genuinely
waiting in YOUR OWN box at the moment you sent it, the same count
your_mail's delivery would have shown, read before this post could touch it.
`null` means jojobot could not tell (no session, no box, an unreadable
board) and is NEVER zero; `0` is a real and different answer. It is stamped
once, here, and carried unchanged wherever read_mailbox or list_sent returns
the message later, so a reader can check a claim about your own mail against
what was true when you hit send.
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

#### `post_message` — argument `/inputSchema.supersedes`

```text
**The id of a message you sent that this one replaces.** Optional. Use it
to correct or withdraw: the reader is handed this message, and the one
it replaces is not handed over as fresh mail when nobody has read it.
It must be a message YOU sent into the SAME box as this one, and one no
other message has replaced; each refusal names its reason and writes
nothing. If the reader already has the original, this message arrives
saying it replaces one they have seen. The original is not deleted or
edited: read_message returns it with `superseded_by`, and list_sent
shows you the same. On a post to the operator, the page they read marks
the original as replaced and names this message, and no push goes out for
the original.
```

#### `post_message` — argument `/inputSchema.to`

```text
**The bot to write to, or the operator** — a colleague, not a container:
a bare name like `gamma`, or its full handle. You address the identity and jojobot
finds its mail; a bot has exactly one box, and what that box is called
is not something you should have to know.

**It must already exist** — a name no bot answers to comes back with
candidates and nothing is written.

**A bot that was folded into another has no box of its own**: a post to
its handle goes to the bot it was folded into, and the receipt's `folded`
block names both.

**The operator is the one person you may write to**, by their person
handle (`person:` and the slug): the boot names it under `operator`.
Nobody else has a box, and no bot reads the operator's box back.

A post into your own box delivers nothing, and a post that had nothing to
hand over still succeeded.
```

### Tool `put_document`

#### `put_document` — description

```text
Write a whole markdown file onto a document: it is split at its headings and every heading with what sits under it is stored as one claim on the document, in order, each with its own address, history and provenance.
read_document returns the file byte for byte; update_fact edits one section and leaves the others untouched.
A section too long for one record is cut at the last blank line, else line end, else character that fits, never inside a fenced block.
The document must already exist (add_entity, kind document).
Onto a document with sections the call is refused unless it names `after`, a section's address, so an edited file put twice cannot double the text.
`restore_to` (a day or an instant) instead of `markdown` puts the document back as `read_document` showed it then: a section that differs is edited back to its old wording; one archived since, or with testimony in either version, comes back as a new record that names the old one as what it derives from, and a standing one is archived; one made since is archived; over testimony it needs `confirmed_by_user`.
Every section's size and links are checked before any is written.
Answers with a receipt, not the text.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
A refusal before any section lands is an answer with status: blocked and nothing written; one after some landed also carries the addresses that landed and where the rest begins (with a `request_id`, a count).
A `request_id` makes a resend of a put finish the sections that did not land.
```

#### `put_document` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "after": {
        "default": null,
        "type": [
          "string",
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
      "document": {
        "type": "string"
      },
      "markdown": {
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
      "request_id": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "restore_to": {
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
      "source": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "source_ref": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      }
    },
    "required": [
      "document"
    ],
    "type": "object"
  },
  "name": "put_document"
}
```

#### `put_document` — argument `/inputSchema.after`

```text
**A section's address**, `document:homer#local-id`: write the new sections
right after it. **Required when the document already has sections**, so
that putting an edited file again cannot silently double the document.
To change a section that is there, edit its record with `update_fact`;
to take one away, archive its record.
```

#### `put_document` — argument `/inputSchema.confirmed_by_user`

```text
**The operator has confirmed a restore over their own words.** A restore
that would change a section whose wording is testimony is refused whole
without this, naming those sections. Only meaningful with `restore_to`.
```

#### `put_document` — argument `/inputSchema.document`

```text
The document to write onto, `document:homer`. **It must already exist**:
a document jojobot does not know comes back with candidates and nothing
is written. Make it with `add_entity` first, kind `document`; its `name`
is its title.
```

#### `put_document` — argument `/inputSchema.markdown`

```text
**The whole markdown file**, as it should read back. It is split at its
headings (`#` to `######`, not inside a fenced block of code) and each
heading with what sits under it becomes one record on the document. A
section too long for one record is cut into pieces that read back whole.
Reading it back with `read_document` returns these exact bytes.

A `@kind:slug` outside a code span or fence links to a thing that exists
and is refused when it names nothing; inside code it is literal.
```

#### `put_document` — argument `/inputSchema.provenance`

```text
`testimony` (the operator said these words), `observation` (you READ
them in a system of record, so `source` is required) or `inference`
(you wrote or derived them; the default). One provenance covers every
section of the call.
```

#### `put_document` — argument `/inputSchema.recorded_at`

```text
The day the sections were made, `YYYY-MM-DD`. Defaults to the day your
run is in.
```

#### `put_document` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of a put of `markdown`**, because a call sent
without one cannot be repeated safely. The put is one record per section:
if an answer is lost or the store stops partway, send the same call again
with the same `request_id` and the sections that landed are not written
again and the rest are, in the places the first call chose. It needs a
`sid`, and a call that wrote nothing spends no id. A `restore_to` takes
none: putting a document back to a moment twice writes nothing the second
time.
```

#### `put_document` — argument `/inputSchema.restore_to`

```text
**Put the document back as it stood at a moment, instead of writing a
file**: a day, `YYYY-MM-DD`, meaning the end of that day in your zone, or
an instant. Send this OR `markdown`, never both; with this, `after` and
`markdown` are not needed. A section that differs is edited back to its
old wording. One archived since, or with testimony in either version,
comes back as a new record that names the old one in `derived_from`, and
a standing one is archived. A section made since is archived. History is
never rewritten. Read the moment first with `read_document` and `as_of`.
```

#### `put_document` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking.
```

#### `put_document` — argument `/inputSchema.source`

```text
**The system the document was read from**, for an `observation`: stored on
every section as its `read_from`, the same field `capture` takes under that
name. It is not where an entity came from, which is `add_entity`'s `source`.
```

#### `put_document` — argument `/inputSchema.source_ref`

```text
**What was read there**, when it has a reference: an id, a path, a line.
Stored on every section as its `read_ref`, the field `capture` takes under
that name.
```

### Tool `read_document`

#### `read_document` — description

```text
Read a document as markdown: its live sections joined in order, byte for byte as they were put, as one markdown string, with the `addresses` of the sections shown, in order: the address update_fact edits and put_document places after.
Archived sections are left out and counted as `archived_sections_left_out`; recall the document with facts reads them.
A document longer than one answer comes back in parts: the sections from `offset` that fit, with `next_offset` and a not_shown block naming the call that reads the rest.
The document's own name and its keys ride beside the text, never inside it: one_liner says what it is, stale_after the day past which it promises nothing, resource the page it mirrors.
`as_of` (a day or an instant) reads the document as it stood then: each section in the wording, place and archive state it had, a section made later absent, and the answer names the moment it used.
A document that does not exist is a blocked result with candidates.
```

#### `read_document` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "as_of": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "document": {
        "type": "string"
      },
      "offset": {
        "default": null,
        "format": "uint",
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
    "required": [
      "document"
    ],
    "type": "object"
  },
  "name": "read_document"
}
```

#### `read_document` — argument `/inputSchema.as_of`

```text
**Read the document as it stood then**: a day, `YYYY-MM-DD`, meaning the
end of that day in your zone, or an instant such as
`2026-08-03T12:00:00Z`. Each section reads in the wording, place and
archive state it had at that moment; a section made later is absent and
one archived later is there. The answer names the moment it used. Leave it
off to read the document as it stands.
```

#### `read_document` — argument `/inputSchema.document`

```text
The document, `document:homer`.
```

#### `read_document` — argument `/inputSchema.offset`

```text
**The record to start from, counting from 0**, when a document is too
long for one answer; a section cut for size is one record per piece. The
answer names the next in `next_offset`.
```

#### `read_document` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled.
```

### Tool `read_mailbox`

#### `read_mailbox` — description

```text
Take delivery of everything unprocessed in YOUR OWN mailbox, oldest first, moving each message from `new` to `read`.
There is no peek: reading IS taking delivery, and every message delivered is yours to finish — act on it, then call mark_processed.
WHICH BOX IS NOT AN ARGUMENT: the `sid` says which bot is asking, and a bot reads the box it owns.
To reach another box, post_message.
ONLY CHECKING WHETHER ANYTHING IS WAITING? Pass counts_only: true: you get the per-state counts and the unreadable report, nothing moves out of `new` and nothing becomes yours to finish, so a poll that finds an empty box costs nothing and owes nothing.
By default you get the messages nobody has taken yet, whole.
Messages a previous read already handed you are LEFTOVERS, still owed: they are counted under `leftovers` with the five oldest ids (`ids_left_out` counts the rest) and not shipped again, read_message returns one by id, and new_only: false returns every message whole, flagged seen_before (the read for a crash that lost what it was given).
Either way it changes what is shipped, never what is owed.
The answer stops at the ceiling and takes every message anyway: a body that does not fit is flagged body_elided, and a message that does not fit even so is named under not_shown.
No box to open comes back status: blocked and delivers nothing.
Reading your own box with your own sid, a poll included, renews a role claim your session holds, so a session that only polls does not go stale.
Mail its sender replaced before anyone read it is not delivered: it stays in new, is listed under superseded with the id that replaced it, and read_message returns it.
Mail from the sender `jojobot` is a release note, which lists what a deploy changed: act on it and mark it processed like any other.
`jojobot` has no box, so a post to it is refused and writes nothing.
An empty delivery names what it looked through, as searched.
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

Draining a whole box makes every message in it yours to finish; use
read_message when you want only one.
```

#### `read_mailbox` — argument `/inputSchema.new_only`

```text
Ship bodies only for messages nobody has taken yet — **the default**.
Leftovers, the ones a previous read already handed you, are still owed
but are not shipped again: they are counted under `leftovers`, with the
five oldest ids and `ids_left_out` for the rest, and `read_message` returns
one whole.

Pass `false` to get those bodies back — the read a consumer makes when
it is recovering from a crash and no longer holds what it was given.

A message somebody else finished while this delivery was in flight is left
out, so a delivery can be smaller than counts you saw a moment ago. Two
sessions of one bot share its one box, so a delivery taken by one is not
waiting for the other; two workers that must not share a queue are two bots.
Each message may carry `sender_mail_waiting_at_send`, stamped once when it
was posted (see post_message's `sid`; `null` is unknown, never zero), and
`written_by_other_run` when a DIFFERENT run posted it than the one reading,
naming that run and whether it has since ended (`null` when that could not
be read).
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
A message in another bot's box comes back status: blocked and moves nothing.
To reach another box, post_message writes into it without reading it, which is the shape of a request.
THE ARCHIVE IS THE EXCEPTION, and it is not a loophole: a `processed` message comes back unchanged and flagged, from any bot's box, because processed is terminal — reading one moves nothing and takes nothing on, and it is what a search hit over old mail points at.
THE OPERATOR'S BOX IS NOT A BOT'S: no bot opens a message in it, in any state, whoever wrote it.
Same envelope a delivery hands over, seen_before and all: true means somebody had already taken this message, so it is a leftover rather than fresh mail.
Taking delivery is NOT handling: call mark_processed once you have acted, and only then.
It renews a role claim your session holds, as a read of your own box does.
WITH `history: true` THIS VERB TAKES NO DELIVERY: it answers the message's changes of state in order, with the run that made each, and leaves the message where it is.
A refusal is an answer with status: blocked and nothing taken, in three kinds: an id that names nothing at all; an id naming an item jojobot cannot read (which comes back saying why, and needs a person rather than a retry); and a message that is not yours to take.
A message a DIFFERENT run posted than the one reading it carries `written_by_other_run`, naming that run and whether it has since ended.
```

#### `read_message` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "history": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
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

#### `read_message` — argument `/inputSchema.history`

```text
**Read the message's history of state changes instead of taking it.**
With `true` the call takes NO delivery and moves nothing: the message is
left in the state it is in, and the answer is its changes in the order
they happened, each naming the run, when, the state it left, and the
notes or reason it was given. Allowed exactly where reading the message
is. Without it this verb takes delivery, as always.
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
Use it for a handle, a kind, a type or a key and its value; search is for words.
THREE AXES COMBINE INTO ONE QUESTION.
WHICH OBJECTS: subject (one handle or `<collection>/<N>`; a named handle that names nothing is blocked with the nearest), kind, parent, answers_type, fields (a key and the value it holds), where (one line over keys, status != done && owner = bot:gamma) or view (a question worked out, asked by name; start with one if it fits).
A call that narrows nothing is refused: name one of subject, kind, parent, answers_type, fields, where, view, unreviewed or near, or a record's address (history_record, built_on).
WHAT OF EACH: every object comes back as its FIELDS, what the thing HOLDS: every write on it folded, the newest write of each key winning (a snoozed rhythm also carries `snooze` {until, in_force}).
facts, prose, charter, history, history_record, values, backing, stood_for and review_marks ask for more, each off by default, each saying what it left out; `keys` asks for less, list blocks too, and `record_keys` for less of each fact record (`records_narrowed` names what it hid; `fields_left_out` does for a view's shows_keys).
A merged thing carries `foldedNames` and can carry `claims_left_out`: claims under a thing merged into it that it cannot serve, each with a reason and an address that history_record reads with no subject (records reads several).
An app's claim carries a server-set `recorded_by` {app, grant, received_at}, not the describing key; archive and re-capture it, never rewrite it.
WHICH EDGES: follow {shape or relation, direction, depth, keeping, fits_type}, and the answer nests.
Unlike search, recall returns claims of EVERY status unless you pass `status`.
Your own past runs are kind session.
AN ANSWER STOPS AT THE CEILING, never mid-record: `limit` sizes pages; not_shown names the next offset.
Empty answers name what they searched.
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
            "anyOf": [
              {
                "$ref": "#/$defs/Keeping"
              },
              {
                "type": "null"
              }
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
      "Keeping": {
        "anyOf": [
          {
            "type": "string"
          },
          {
            "items": {
              "$ref": "#/$defs/KeyFilterArgs"
            },
            "type": "array"
          }
        ]
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
      "OrderArgs": {
        "properties": {
          "direction": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "key": {
            "type": "string"
          }
        },
        "required": [
          "key"
        ],
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
      "details": {
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
      "held": {
        "default": null,
        "type": [
          "boolean",
          "null"
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
      "inputs": {
        "additionalProperties": true,
        "default": null,
        "type": [
          "object",
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
      "limit": {
        "default": null,
        "format": "uint32",
        "minimum": 1,
        "type": [
          "integer",
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
      "order": {
        "anyOf": [
          {
            "$ref": "#/$defs/OrderArgs"
          },
          {
            "type": "null"
          }
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
      "record_keys": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "records": {
        "default": null,
        "items": {
          "type": "string"
        },
        "type": [
          "array",
          "null"
        ]
      },
      "review_marks": {
        "default": null,
        "type": [
          "boolean",
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
      "ticks": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "ticks_offset": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "unreviewed": {
        "default": null,
        "type": [
          "boolean",
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
      },
      "where": {
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
handle a record holds as a whole field value, or as an item of a comma
list, under any key) — each
labelled mention, ref or field, never as an edge. Naming a shape or a
relation narrows to that alone: none has one.
```

#### `recall` — argument `/inputSchema/$defs/FollowArgs.depth`

```text
How many hops: 1 is the neighbours, 2 is the neighbours' neighbours.
Defaults to 1; 1 to 5 are accepted, and 0 or more than 5 is refused.
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

**It also takes one expression**, the same line a call's `where` takes
(see there): `status != done && owner = bot:gamma`. The expression is
asked of what each reached thing holds now, and of nothing else: a
`scope: record` sent beside it is refused. To keep what a walk reaches by
one record, send `keeping` as a list of key filters, each with
`scope: record`.
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
· `connection`), or the word a read served for it (`memberOf`,
`attendee`, `relatedTo`): either is understood. Omit for **any** edge — "whatever it is connected to"
— which is also what reaches a mention, a ref or a field link; naming a shape here
reaches edges of that shape alone.
```

#### `recall` — argument `/inputSchema/$defs/Keeping`

```text
**What a walk keeps of what it reaches**: key filters, or one expression.
```

#### `recall` — argument `/inputSchema/$defs/Keeping/anyOf[0]`

```text
One expression over keys, asked of what each reached thing holds.
```

#### `recall` — argument `/inputSchema/$defs/Keeping/anyOf[1]`

```text
Key filters, each with its own scope.
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

⚠️ **A key a claim holds as one of its own `fields` is held by no thing.**
It stays on that claim, so the default `thing` scope never finds it: ask for
it with `scope: record`. A key sent in `sets` is held by the thing and is
found by either scope.

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
**Which clock to compare**: `recorded_on` — the day in a claim's
`recorded_at`, the day it was made, the default — `taken_in`, the day jojobot took the record in, or
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

#### `recall` — argument `/inputSchema/$defs/OrderArgs`

```text
**The order the answer comes back in**: one key and a direction.
```

#### `recall` — argument `/inputSchema/$defs/OrderArgs.direction`

```text
`ascending` (the default; smallest first) or `descending`. Things that
hold no such key come last in either direction.
```

#### `recall` — argument `/inputSchema/$defs/OrderArgs.key`

```text
The key the things are ordered by, as it is spelled where it was
written. A key declared a date or a number orders as one, so 2 comes
before 10; any other key orders as text, so 10 comes before 2, and the
answer says so when the values would have parsed.
```

#### `recall` — argument `/inputSchema/$defs/OverdueArgs`

```text
**The overdue question of a `recall`** — which things are owed, loops and
promises among them, as of a date.

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

An answer that selects nothing still says what the type is: it carries
`type_keys`, the type's keys and what each holds, so the spelling to write
under it is in the empty answer. The first such answer in a session, under a
type the software ships and with nothing else narrowing the read, also
carries a teaching: reading again finds nothing, and what you worked out is
kept by capturing under those keys. In `type_keys`, `required: true` marks a
key the type counts as its floor and never means required to write: a record
that holds some of the keys is kept and found here, with what it lacks named.
`fits_type` keeps only the things that hold every key, optional ones too.
```

#### `recall` — argument `/inputSchema.backing`

```text
**Where each folded value came from**, and who backs it: the claim whose
write won the key, its provenance and its standing.

**Off by default because it is a second read of each object**, and most
reads want the value rather than its pedigree. Ask for it when you are
about to act on a value, or when you need to know whether the operator said
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

#### `recall` — argument `/inputSchema.details`

```text
**Whether each record comes back with its details.** Off by default: a
listing leads with each record's one-line claim, its address, provenance
and standing, and the details (the reasons under a claim) are a paragraph
the reader has not asked for. An answer that left details out says once,
in `details_left_out`, how many it left out, the addresses of the first twenty, and how to ask. Send `true` to
read them. Applies to every record this call returns, `records` included.
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

**Who holds what comes back unasked** on a read that names no `keys`: every
object carries a `held` block listing the records that point at it, best
first, each marked by its `liveness`: what admits somebody and is good on the
day (`live`), what admits them and has lapsed (`lapsed`), and, only when
nothing admits anybody, whatever else points here through a declared key
(`related`). Each says who holds it (`holder`), what backs it (`provenance`)
and how sure the operator was (`standing`). It says nothing about whether
anybody may go, which is a judgement jojobot does not make; it is bounded,
counts what it left out (`left_out`) and, for admitting records, names the
walk that returns the rest. **A bot's own thoughts age
quietly:** asking facts of a bot that carries thought_capacity adds a `room`
block naming the capacity, how many thoughts count against it now and how
many have gone quiet from being untouched too long (still there, marked
aged_out on the fact, excluded from the count rather than hidden). The block
names its threshold (`ages_after_runs`, and `ages_after_from` saying whether
the bot's own thought_ages_after_runs gave it or the default did) and is
sent when at least one thought has gone quiet or the room is over its
capacity (`over`). When one has gone quiet it names the call that brings
it back (`bring_back`). A write that would go over the capacity is
refused the same way.
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
Which edges to walk. Omit to walk none, and the answer is flat.

**Walking.** The answer nests: a walked object carries the objects it
reached, each carrying its own, so one call answers 'these objects, and what
each is connected to'. Each hop is bounded: a wide fan-in comes back cut to
what fits, in the order the walk reached it, and `connected_left_out` on the
object above says how many did not fit (recall that handle again, or narrow
with `keeping`). An object that draws edges the walk did not follow says so;
an empty `connected` with no such note is an object with nothing beyond it.
With `shape` and `relation` both unset a walk also reaches a MENTION (an
entity named in a claim's own words), a REF (an entity a claim touches with
no claim about how) and a FIELD LINK (an entity whose handle a record holds
as a whole field value, or as an item of a comma list, under any key),
each labelled mention, ref or field rather than as an edge; naming a shape
or a relation answers only that. For one kind of thing, SELECT it (`kind`
plus a `fields` filter on the key) rather than walking: traversal reaches,
selection chooses. A key's declared value type licenses how you compare it
(`before`/`after` on a date, `less`/`greater` on a number, `equals` on
anything), and nothing is gated by a declaration: an undeclared record is
still found by the keys it carries.
```

#### `recall` — argument `/inputSchema.held`

```text
**Bring the `held` block back on a read that names `keys`.** A read that
names keys carries, on each object, its handle, name, the keys asked and what
the read computes, and says once, for the answer, what the objects left out.
Who holds what points at an object (`held`) is one of those: send `held: true`
to have it on each object, or `facts: true`, which reads the records the block
is made of. A read that names no keys carries `held` as it always did.
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

The count of the writes is the answer to how many times: a key a sitting
records once each time something happens is how you count occurrences. A key
nobody wrote comes back as no writes, never as a refusal. A long history is
cut to its newest twenty, saying how many exist and how many it left out;
`history_most` raises the window.

On a session it traces the run's own fields, named focus, timezone,
stated_day, wrap_window, state, served_chars, charter_told or key, and
`journal` returns the wordings of each entry written more than once. A
run's history comes back in parts under the answer ceiling, newest
first: `history_most` counts the changes or wordings of a part, and
`offset` reads on. Each change of a field carries its `position`, its
place in the order the run's changes were made.
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
recorded one carries none. **Each write also names the `run` that made
it**, by the run's public name, which opens nothing: a caller reads its
own runs with `recall` kind session. A write no run made, and one kept
before runs were recorded, carries none.

**The address is selection enough.** Send it on its own and the read
answers about the thing the address names — you do not have to repeat
the subject beside it. Name a `subject` as well and it has to be that
same thing, or the call is refused: two arguments pointing at two
things is a mistake worth hearing about rather than one jojobot picks
between.

**Or a thing's own handle**, `kind:slug`, with no `#`: then the object
comes back carrying every write of the THING itself — the name, aliases,
prose and archive state it held before somebody changed them, and the run
that changed them. Each version holds what the thing held after that write;
a key it held nothing for is left out. A write kept before jojobot kept
versions carries only its place in the order and its moment, and the
answer says the earlier values were not kept. **A thing others were
folded into carries their versions too**, in the order they were written,
each marked with the `thing` that wrote it; a child a fold moved under it
has a version of its own for the move.

Omit it and no chain comes back, which is the normal read.
```

#### `recall` — argument `/inputSchema.inputs`

```text
**The values a view takes**, as `{name: value}`, beside `view`.

A view is a saved question with typed inputs, and this is how you fill
them: ask `view: "ready"` with `inputs: {project: "project:atlas"}` and it
lists only the work under that project. **What a view takes is on the
view**: recall it (`subject: "view:ready"`, or `kind: "view"` for all of
them) and its `inputs` field reads `name: type`, where a type is `kind
<kind>` (a handle of that kind), `handle` (a handle of any kind), `date`,
`number` or `text`, and a `?` after it means the input may be left out. A
value is checked as a write is: a handle must exist and be of the kind it
names, a date takes the relative days `where` takes, a number must parse. An input left out takes its
default when the view declares one (`by: date? = today+7d`) and otherwise
drops the part of the question that uses it. **A call with a required input missing, an
input the view does not declare, or a value of the wrong type is refused,
and the refusal names every input the view takes**, so a view shows
its shape.
```

#### `recall` — argument `/inputSchema.keys`

```text
**Which keys of each object's fields come back, and only those.** Name
the keys you want, and every object in the answer — the ones a walk
reaches included — carries its handle and name, just those of its fields
and what the read computes. Its other identity fields (alternateName,
archived, boot, crm, source, type, parent, as_of) are not there unless a key
names them, and `connected` is left out where nothing was reached.

**Eliding is never silent.** The answer says once, in `narrowed`, what
the objects left out: the keys they hold that were not brought
(`left_out_keys`) and, where it is true, that their records, their `held`
block and who backs each value are not here, each with the argument that
returns it; asking again without `keys` reads them all. When a view
supplied the keys, each object says so in `fields_left_out`, naming them:
ask again without the view, sending its kind, or with `keys` naming the
ones left out. A key an object does not hold is simply not there.

**A bot's `working_on`** is the thing its session said it is doing, and
it is a field only while that session is live. After the wrap or the
sweep the key is not there and the answer carries `working_on_note`.

Omit it and every key comes back, which is the normal read. This
narrows what each object says, never which objects come back, and it
leaves facts, prose and the charter as the arguments for those say.
**A list's blocks are keys too**: `entries`, `plan`, `progress`, `ticked`,
`last_ticked` and `ticks` come back only when `keys` names them, so a list
read that sends `keys` and wants its entries sends `entries`.
**A view can carry it** as its `shows_keys`, and what you send beside
the view's name wins.

**How old what an object holds is:** each object carries `as_of`, the day
the store last took in a write about it, and `fields_as_of` for any key
whose own day is older; an object with no `as_of` has no write the store
could date. **A read that names `keys` leaves `as_of` out unless `as_of` is
one of them**, while `fields_as_of` still dates the keys it returns that
are older than the object. That is a day the store saw, while
`overdue`'s `as_of` is the day an overdue question is asked about.
```

#### `recall` — argument `/inputSchema.kind`

```text
Every entity of one kind. `session` reads your own past runs.

**YOUR OWN PAST RUNS ARE A KIND:** `session` reads this identity's own past
runs and no other's, and `prose: true` brings back a run's chronology
(list_runs gives each run's state and focus line; this is the read for what
it did). The focus a run closed on is not in that prose: wrap_session's
closing entry carries it as `closing_focus`. A selection that reaches
another identity's runs counts them as `withheld` and shows none of them; a
filter that leaves out runs of YOUR OWN says how many as
`own_runs_excluded`. A run carries its name as the `sid` field, which you
can filter on and which is never its key, and `near` places a run on the day
it started. start_here resumes an unwrapped run by that name; only the
`wrap_code` a wrap handed back reopens a wrapped one. A call with no sid owns no run: boot with start_here and pass
the sid it returns to read your own. A run carries `served_since_compaction`,
`served_lifetime` and `compactions` (how many it reported), and a read of
one run adds a `served_<YYYY-MM-DD>` key for each day it was served on.

**WEEKS OF USAGE ARE A KIND:** `usage` reads what each bot's runs add up
to, one record per bot per ISO week (`week`, `from` and `to`; `partial`
is true while the week is still open): runs begun, compactions reported,
characters served, and the calls the bot made. `compactions` counts the
compactions reported in the week, on the day the run was in when it
reported (a run that states no day, on the day it was recorded), and
`served_at_compaction` sums the totals they set back. `counted_from` is the earliest dated change of the
bot's runs, which bounds the compactions: one before it left no dated
record, so an earlier week can read low.

`served_chars` is the week's total of what the bot was served, counted on
the day each answer was served (`served_basis` is `day`), with one
`served_<YYYY-MM-DD>` key for each day that had any. `served_counted_from`
is the first day anything was counted for the bot. A week before it holds
`served_chars` 0 and is not exact: read `served_by_run_start` for it, what
the runs that began in the week were served over their whole lives.

`calls` and `refused` are the week's totals; `calls_<verb>` counts every
call to a verb, `refused_<verb>` the ones refused, and
`refused_<verb>_<retry|change|person>` the refusals by who fixes them. A
verb never called, and a word that never fixed a refusal, has no key, so a
bare key (`where: "refused_capture"`) asks whether it happened, but only
from `calls_counted_from` on, the first day a call of the bot was counted.
A week before it holds no call keys whatever the bot did, so the answer
there is unknown, never no. A call is counted once, after it is answered,
on the day the run was in; a resend of a request id that landed is not
counted again. A call is counted under the bot of its session, and a boot
that opens or resumes a run is counted under that run. A call with no
session, an anonymous boot and a refused boot count for no bot; a resume
offer, for the bot named. A call no verb serves is counted as the verb
`unserved`. They are computed when read and nobody writes one. You read
your own bot's weeks and those of every bot below it on the chart; the
assistant reads all; the rest are counted as `withheld`.
```

#### `recall` — argument `/inputSchema.limit`

```text
**The size of a page**: the most a part of the answer holds, counting
what `offset` counts, so on a call that selects several objects it is
objects, and on one subject with `facts` (or a list's entries) it is
records, and on a run named with `history` its changes or wordings. The answer ceiling can make a part shorter, never longer, and
`not_shown` names the `offset` that reads on. At least 1; omit it and a
part holds whatever fits.
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
facts (`facts: true` on a subject with more than fit), or a list's
entries; a call that selects several, a kind or a walk, counts the
objects. Defaults to 0.

One subject with more facts than fit is read the same way, `offset` counting
its facts, and a list with more entries than fit pages its entries the same
way, and a list read with `facts: true` counts its entries and then its
records; an object that cannot fit even first in its part carries
`facts_not_shown`, or for a list `entries_not_shown`, naming that subject to
recall alone, which reads them in parts, and a single fact larger than the
ceiling still comes back whole.

**On a session named as the subject with `history`, `offset` counts the
changes or wordings of its history already read, newest first.** A run
holds no facts, so that call has one thing to read on through.
```

#### `recall` — argument `/inputSchema.order`

```text
**The order things come back in**, applied to the whole selection before
the answer is cut to size, so each part you read on with `offset` is a
part of that order. Things that hold no such key come last.
```

#### `recall` — argument `/inputSchema.overdue`

```text
**Keep only what is OWED as of a date** — what has a due moment that
day has reached.

It OFFERS and never acts: arithmetic on what a thing holds and the day
you name, never a judgement that something should happen. **Name the
day in `as_of` or it reads the run's day** — the day it stated, else
today in its zone — so *what is owed as of next Friday* is the same
question asked about a different day.

**What falls due, and when, is the answer of the keys a thing carries,
whatever its kind.** A thing on a schedule (`cadence_days`/`rrule`,
`anchored_on`, `counts_from`, `snoozed_until`) is next due a cadence
after the day its cycle counts from; a thing carrying `runs_out`,
`decide_by` or `promised_by` is due on that day. This filter compares the
moment rather than computing it. A `work` item or `project` at `done`
owes nothing, and neither does a thing whose `ended` is set beside
`runs_out`, `decide_by` or `promised_by`.

**A thing that carries none of those keys is absent**, whatever its
kind: a rhythm nobody has put a schedule on made no promise to be late
against, and a person who holds none of them is never late.

**But a thing whose due moment cannot be READ comes back owed**,
carrying its fields so you can see which key it is short of. That is
deliberate: a half-built rhythm that surfaced at no read ever would never
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

**It filters what another way of selecting chose** (a kind, a parent, a
view): a call that names only `overdue` is refused as one that narrows
nothing.
```

#### `recall` — argument `/inputSchema.parent`

```text
**Only the direct children of this entity**, as `kind:slug` (a bare
handle is read as a person). One level, never a subtree. It combines with
`kind`: `kind: "role"` with a bot as `parent` is that bot's roles, each
with its holder, the moment of its last claim or renewal and its agent, in
one read.
```

#### `recall` — argument `/inputSchema.prose`

```text
Whether each object's **prose** comes back — the human half of its page,
whole. Off by default, because a page is bigger than a claim and shipping
every one of them unasked is a cost the caller cannot decline. A run's
chronology is its prose, and an answer that left some out says so once,
under `prose_left_out`.

**For a bot this is the instance's own layer**, which is what somebody
wrote and what `set_charter` replaces. Ask for `charter` instead to read
what that identity actually answers with.
```

#### `recall` — argument `/inputSchema.record_keys`

```text
**Which keys of each fact record come back, and only those**, as `keys` does
for an object's fields. Name them as a record carries them (`content`,
`provenance`, `standing`, `recorded_at`, `details`, `fields`, `sets`,
`edge`, `refs` and the rest), and every fact record in the answer, on the
objects a walk reaches too, carries its `address` and just those keys.
Naming any asks for the records, so `facts: true` need not be sent, and
naming `details` brings the details without `details: true`.

**Eliding is never silent.** The answer says once, in `records_narrowed`,
which keys the records it carries held and did not bring
(`left_out_keys`); asking again with `facts: true` and `details: true`, without `record_keys`, reads them all. A
key a record does not hold is simply not there. A review that needs a
record's claim, who backs it, how sure anyone is and its day sends
`content`, `provenance`, `standing` and `recorded_at`.
```

#### `recall` — argument `/inputSchema.records`

```text
**Record addresses to read as they stand now**, `kind:slug#fN` each, on
any subjects, in one call. Each comes back with its current wording, its
address and subject, provenance, standing, the day it was recorded and how
many times it has been written, and none of the writes behind it:
`history_record` is the door for those. An address that names nothing is
named as missing and the others are still served. The call is its own
question, so name no subject, kind or other selection beside it. A long
list stops at the answer ceiling, and `not_shown` names the `offset` that
reads on.
```

#### `recall` — argument `/inputSchema.review_marks`

```text
**Whether `facts` lists a thing's review mark.** Off by default: the mark
is the one record defrag leaves on a thing it has reviewed, which is
bookkeeping to every other reader. An object that has one says under
`review_marks_left_out` how many the listing left out and how to read
them. Send `true` to list it. `search` takes the same argument. A record
asked for by `history_record` or `records` is served whole either way.

**It also lists a bot's record of the release notes it was told**, which
jojobot keeps and a listing leaves out the same way, counted under
`release_notes_record_left_out`.
```

#### `recall` — argument `/inputSchema.scope`

```text
**What `where` is asked of**: `thing` (the default) is what each thing
holds now, and `record` is what one record says, so every term must hold
on the same record and the object arrives with the records that answered.
Names neither scope for `fields`, which carry their own.
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

**A member of a numbered collection answers to `<collection>/<N>`**, as
`topic:contract-ordinances/32`: the descendant holding the number N under the key the
collection numbers by. A number nobody holds comes back blocked with the
numbers that exist. A collection says `next_number`, the number its next
member takes.

**A list answers `entries`**: its entries in order, each with its
`address`, its `text`, its `link` when the entry draws an edge, and its
`state`. An entry whose edge points at a piece of work also carries
`linked`: that work's `status`, `owner` and `ready`, with `blocked_by`
beside it when it is not ready, read now from the work (the same verdict
the `ready` view gives). The list never skips a blocked entry or promotes a
ready one. **It also answers `ticked`, how far along it is by its own
ticks**: `done` out of `of` on a checklist, `done` and `open` on a queue and
`turns` on a rotation, with nothing for a list with no entry that is not dropped. **A list whose
entries link work also answers `progress`**: `done` out of `of` pieces of linked work, with `reads` saying it; work archived before it finished counts in neither. **A list that sets `covers` (a day, or an ISO week) is a plan**
and also answers `plan`: its `from` and `to` days, what falls `due` in
them, what is `overdue` (fell due before `from` and is still owed, with
`overdue_by_days` at `from`) and which of that work is `ready`, worked out
at the read by the due and ready views' own passes and stored nowhere. An
entry carried from another plan (capture it with `derived_from` naming the
old entry) reads `carried_from`, and the old entry reads `carried_to`; it
stays as it was. It is the list's own read, so no `facts` are needed for it.

**A named subject comes back** even when the filters below keep none of
its records: naming a handle asks for that object, and a filter asks
which objects. **`overdue` and `unreviewed` also filter a named subject**.
A handle naming nothing is blocked with nearest handles, never an empty answer.
A missing `work:q-N` offers work holding `card` number N first.
```

#### `recall` — argument `/inputSchema.ticks`

```text
**Read a list's ticks**, newest first: each says the entry ticked, the act,
when, and which session. A list's ordinary answer never carries them, and
says in `entries_left_out` how many done or dropped entries it left out.
`history_most` counts the ticks of a part (twenty when you do not say) and
`ticks_offset` reads on; the answer says how many are left.
```

#### `recall` — argument `/inputSchema.ticks_offset`

```text
**How many ticks of a list were already read**, newest first, when `ticks`
reads on. `offset` pages the answer's objects and is not this.
```

#### `recall` — argument `/inputSchema.unreviewed`

```text
**Only the things defrag has not reviewed**, oldest change first: a thing
whose newest write is later than the stamp of defrag's add_entity, capture,
update_fact or update_entity, or that has none. It names what to recall.
Send `true`; the shipped view `unreviewed` asks the same.
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
`add_entity` of kind `view` and the keys `selects`, `shows` and `asks`,
with `where` (a line over keys, as the call's own `where`), `order` (a
key, then `ascending` or `descending`) and `scope` (`thing` or `record`)
for which of them and in what order.
**A view that takes values declares them in `inputs`** (`project: kind
project`) and uses each as `$name` in `parent` (`parent: "$project"`), in
`where`, in `as_of` (the day an overdue ask is asked about) or in
`answers_type` (the declared type whose things it finds). `shows_keys`
names the keys each object comes back with.
**A view needs something that narrows it**: `selects` (a kind), `parent`,
`asks: overdue`, `asks: ready`, `asks: plan` or `asks: unreviewed`,
`answers_type` or `where`.
Asked as it stands without one, it is refused, naming the view and those
keys; a kind, subject, parent, answers_type or fields sent beside it count.
A write that leaves it with none still lands, with a note saying so.
Both are records of the same shape, so nothing about the answer depends
on where the view came from.

What the view says is what this call asks; anything you send beside it
is yours and wins, so a view is a starting point rather than a cage. The
one exception: `ready` and `plan` answer for the work kind only, and a
`kind` or `subject` of another kind beside them is refused.

**A view may take inputs, and you fill them with `inputs`** rather than
with a `where` of your own: `view: "ready", inputs: {project:
"project:atlas"}` lists the ready work under that project. Recall a view's
handle, or `kind: "view"` for all of them, to read what each takes.

A name that is no view comes back blocked, with the views that exist.

`view: "colleagues"` lists the identities here and what each is for; `view:
"loops"` lists the recurring things (rhythms) and what each last recorded;
`view: "ready"` lists the work that is not finished, whose dependencies are
all finished and that waits on no one. It does not check `status`, so work
parked at someday comes back too. The blocked work is counted as left out,
and the answer names the call that says why: send the view with `subject`
set to one piece of work and it comes back with `ready: false` and
`blocked_by`. A view's answer can be narrowed with `where`: `view: "ready"`
with `where` set to your board's not-started status, for example
`where: "status = next"`, lists the ready work not yet started. On a
view that holds a `where` of its own, yours replaces it. `view:
"plan"` lists all the work that is not finished, ready or not, each piece
with `ready` and, when it is not ready, `blocked_by`; send it with `parent`
set to a project to read that project's whole plan in one call. A view
carries its own `shows_keys`, and a `keys` you send wins over it. `view:
"due"` lists what is coming up by the day `inputs: {by: ...}` names (a week
ahead when you send none) and what is already late, whatever kinds owe
something: an object past due carries `overdue_by_days`, one not
yet due `due_in_days`. `view: "board"` takes the `project` it is about and
lists that project's work ordered by status, each piece with its status,
what it depends on and its owner. `view: "of-type"` takes a `type` (text)
and lists the things that answer that declared type, each with the keys it
lacks; it finds things only, so for a type another kind answers send
`answers_type` to `recall` directly. `view: "unreviewed"` lists the things
defrag has not reviewed, oldest change first, as `unreviewed: true` does.
`view: "usage"` lists what each bot's runs add up to, newest week first;
`inputs: {bot: ...}` narrows it to one bot. `view: "lists"` lists every
list with its tick setting, how far along it is by its own ticks
(`ticked`) and the day it was last ticked (`last_ticked`);
`inputs: {under: ...}` narrows it to the lists hung on one thing, of any kind.
```

#### `recall` — argument `/inputSchema.where`

```text
**Which things, in one short expression over keys**, for
example `status != done && owner = bot:gamma`,
`due <= today+7d`, `tags contains kitchen`. The words are `=`, `!=`, `<`,
`<=`, `>`, `>=`, `in (a, b)` (any of), `contains` (a list holds an item),
`&&`, `||`, `!` and parentheses; a bare key asks that it is there. There
are no wildcards. Quote a value that holds spaces or operator characters.

**It reaches keys only.** `subject`, `kind`, `parent`, `answers_type` and
`follow` stay arguments beside it. It combines with `fields`: both must
hold.

**One `scope` covers the whole expression**: what each thing holds now
(the default), or what one record says. A negation keeps only things that
hold the key: `status != done` is the things holding some other status,
and a thing with no status is not among them; `!(status = done)` is the
complement and does keep it. A comparison needs a declaration: `<`, `<=`,
`>`, `>=` need a key declared a date or a number, and `contains` a key
declared a list. A type named like a kind belongs to that kind, and its
word on a key wins for things of that kind; a type under any other name
licenses any thing carrying its key. Each thing is asked its own kind,
named or not. A thing the declarations do not license does not match, and
the answer counts what it left out by kind in `left_out_unlicensed`,
saying which key and what to declare.
`today`, `today+3d` and `today-2w` are dates relative to your day.

A mistake is refused with the token, its position and the words accepted
there; a key that nothing holds is refused with the keys that are held,
rather than answering empty.
```

### Tool `rename_entity`

#### `rename_entity` — description

```text
Move a handle — a new slug, a new kind (a retype), a new parent, or any two together in one call, since the handle is one string and a retype is a rename.
What follows automatically, with nothing rewritten anywhere: a mention written as @kind:slug, an edge, a reference-typed field value, every claim's own home, a journal beat and a mailbox message all keep resolving under the new handle.
Every entity naming this one as its parent is repointed in the same write, and a box follows its owner to the owner's new name.
What does NOT follow: a handle written as free prose that never used @kind:slug, and a thing renamed and later folded into another resolves one hop short of the survivor.
A rename that moves an owner across the person line (a bot into a person, a person into a bot, or the operator out of the person kind) is refused, because a person's box is private.
A role object (kind role) is renamed, retyped or reparented only by the bot it belongs to or a bot heading the chart; anyone else is refused and told who may.
The display name stays as it was: update_entity's name changes it.
`to` is always `kind:slug`, fully qualified: a bare slug is refused rather than guessed at.
The destination faces the same near-miss guard a creation does: a collision with something that resembles it comes back status: blocked with candidates, liftable with override_token unless the collision is an exact handle, which can never be forced; a collision on the name alone also needs override_reason, shown beside the name.
A `handle` that names nothing jojobot ever held is blocked with the nearest handles; one that named something BEFORE an earlier rename moved it is told what it is called now.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
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
      "override_reason": {
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
      "request_id": {
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
The entity's current handle — what it answers to now. A handle it
answered to before an earlier move still resolves in a mention, an edge, a
reference-typed field value, a claim's home, a journal beat and a
mailbox message — all of them stay current after a move. Given here, a
stale handle is refused with the name it moved to, rather than a bare
miss.
```

#### `rename_entity` — argument `/inputSchema.override_reason`

```text
**Why you insist**, sent beside `override_token` when the refusal rests
on the name alone: the same name under another kind, a name that holds or
sits inside another, a known nickname of another, or two rare words shared
with another of the same kind. Without it that refusal stands. The reason is
kept on the renamed entity as `told_apart_by`, shown beside its name.
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

`parent` reparents when given and leaves the current parent alone when
omitted; there is no way to clear a parent through this verb.
```

#### `rename_entity` — argument `/inputSchema.recorded_at`

```text
The day this rename was made, `YYYY-MM-DD`. Defaults to today in the
caller's own day.
```

#### `rename_entity` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. This call is made of parts, the entity, the
override reason and the box: if an answer is lost or a part fails, send
the same call again with the same `request_id` and the parts that landed
are not written again and the rest are. It needs a `sid`, and a call that
wrote nothing spends no id.
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

`to` may name the same handle as `handle` when only the parent is moving,
since a reparent needs no reslug; but naming the same handle with no
`parent`, or the parent it already has, is refused, because that call would
change nothing.
```

### Tool `retract`

#### `retract` — description

```text
Take back a record — one way, never reversed, and a deliberate act rather than a flag on an edit.
ARCHIVE, RETRACT OR REWRITE — one rule, stated the same in update_fact, its status argument and retract.
A claim that was true and has changed, or that was never true: archive it with update_fact (status: archived, the reason in details) and capture the replacement, naming the old claim in derived_from.
A claim about a past EVENT that turned out never to have happened: retract it, which also leaves a dated record of the retraction.
A mistyped or reworded claim: rewrite it.
Archiving and retracting are one-way for a claim: nothing edits an archived claim, status included, so there is no un-archive (an archived ENTITY comes back with restore: true).
Nothing is removed: the record keeps its address, its words and its place and is marked archived, and beside it lands a dated record of the retraction itself, naming what it takes back and the reason if you give one.
The two read as one story.
The receipt names the keys the thing lost under left_the_thing.
recall returns claims of every status unless you ask it for one, so a retracted record is out of a default search, not out of a recall.
Nothing edits an archived claim, a status flip back included, so if you are unsure, capture what is so now instead.
A refusal is an answer with status: blocked and nothing written.
Retracting a retraction is refused: it is the last word on what it takes back.
Retracting a record that is already archived is refused too, and says so, because the state you asked for is the state jojobot holds.
An address that names no record is refused with the addresses that do exist.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
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
      "request_id": {
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
**The day the record was taken back**, `YYYY-MM-DD`. Defaults to the day
your run is in.

A retraction leaves a record of its own, and this is the day THAT
record was made. **The day an operator changed their mind is the fact a
later reader most wants about a retraction**, and it is not always the
day the call is made — a session catching up on last week says so here.
```

#### `retract` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the claim is taken back once. It needs a `sid`, and a call that
wrote nothing spends no id.
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
One ranked list over entities, facts, free prose AND the messages in bots' mailboxes at once (the operator's box is never searched).
`query` matches by stem, needing at least half its terms, rounded up.
It is optional when a filter narrows: kind, status (unset: active claims, then archived ones ranked below them, retracted ones left out; `active` is current truth only; `archived` reads the archive, retracted claims too), provenance, standing (`open` asks which claims are still in doubt), subject, edge {shape, object}, answers_type or fits_type (one of the two, never both; prefer answers_type, which names each thing's gaps).
A call with neither a query nor one of those filters is refused, and include_mail is not a filter.
kind + edge walks typed edges.
A fact carries its matching passage, its address (feed it to update_fact), and its parent (and a different subject when present); an entity or prose hit carries the entity's names; include_links adds relationships and references.
A message hit carries its box, state (new, read or processed), sender and the id read_message takes, plus a snippet; section hits come grouped with a document block.
MAIL IS OPT-IN: pass include_mail: true to search messages too.
Read the `mail` and `memory` fields of the answer: searched: false means nothing there was searched, not the same as nothing matching; a `note` says which case.
AN ANSWER IS CUT AT THE CEILING, never mid-hit: not_shown says how many it left out and the offset that returns them.
Notes name default exclusions (left_out), what an empty search looked through (searched), and a page past the last hit (past_the_end).
With sid, each note is taught once per session; anonymous searches keep their notes.
A bot with setting_token_saving=on gets only results; off is the default.
If the setting cannot be read, the normal mode is served with a note.
See each argument for details.
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
      "details": {
        "default": null,
        "type": [
          "boolean",
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
      "include_links": {
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
      "review_marks": {
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
`connection`), or the word a read served for it (`memberOf`, `attendee`,
`relatedTo`): either is understood.
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

An answers_type that matches nothing carries `type_keys`, the type's keys,
and the first such answer in a session under a type the software ships, with
nothing else narrowing the search, also carries a teaching. In `type_keys`,
`required: true` marks a key the type counts as its floor and never means
required to write: a record that holds some of the keys is kept and found here.
`fits_type` keeps only the things that hold every key, optional ones too. Reach for
answers_type unless you have a reason not to: a thing that arrives with its
gaps named can be judged, and a thing fits_type leaves out looks exactly
like a thing that is not there.
```

#### `search` — argument `/inputSchema.clock`

```text
**The recency tie-break day: `recorded_on`** (the default) or `happened_at`.
More matching query terms and closer words lead before recency is considered.

⚠️ **A claim with no `happened_at` ranks by `recorded_on` instead**,
and the answer's own `rank_fallbacks` says how many that happened to
— an incomplete answer, accepted and explained, never a silent mix
of two orderings.

`clock` picks the day ranking reads: `recorded_on` (the default) is the
day in a claim's `recorded_at`, and `happened_at` is the day the thing
itself did.
```

#### `search` — argument `/inputSchema.details`

```text
**Whether each claim comes back with its details.** Off by default: a hit
leads with the matching passage, its address, provenance and standing; the
details (the reasons under a claim) are a paragraph the reader has not
asked for. `details_left_out` counts the omitted fields and names their
records. The note that says how to ask is taught once per session.
Send `true` to read them. Words in a claim's details still match.
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

#### `search` — argument `/inputSchema.include_links`

```text
Include relationships and references on results.
Omitted by default.
```

#### `search` — argument `/inputSchema.include_mail`

```text
Whether messages left in mailboxes are searched too. **Defaults to
false, and worth passing true** when you are looking for what a session
knows: a report filed for another session is exactly the context you
would not know to go looking for, and this is the one call that finds
it. It is off unless you ask, because a message hit carries somebody's
box, sender and a snippet, and this verb is the one to reach for first.

Mail is off unless you ask, because a message hit carries somebody's box,
sender and a snippet and this is the verb to reach for first. Reach for it
when you want what a session knows: a report filed for another session is
exactly the context you would not know to go looking for. `mail` in the
answer says whether messages were searched (`searched: false` means none
was) and can still be partial after a degraded start, where the hits are
real but anything older than this server's start is missing. `memory` says
the same for entities, facts and prose, and with a note the index is behind
the store, where `behind` says how much: `unscanned` (the index holds only
what jojobot has written since this server started) or `stale` (the index
holds an older version of one entity than the store, so a hit may be a
version the store has moved past). The hits are real either way, and
`recall` is complete.
```

#### `search` — argument `/inputSchema.kind`

```text
Narrow to one entity kind — an entity's own kind, a fact's subject's kind,
or the kind of the entity whose prose matched.

A `kind` filter leaves mail out even when you ask for it, since a message
belongs to no entity and so has no kind to match.
```

#### `search` — argument `/inputSchema.limit`

```text
**The size of a page**: how many results come back at most; defaults to
5. An answer carries the best of them that fit under the answer ceiling
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
entity's prose. **Words are matched by stem, and a record needs at least
half of the query's terms, rounded up: two terms need one.** Optional when at least one filter
below is given.
```

#### `search` — argument `/inputSchema.review_marks`

```text
**Whether a thing's review mark is among the hits.** Off by default: the
mark is the one record defrag leaves on a thing it has reviewed, which is
bookkeeping to every other reader. A `left_out` note teaches this once
per session. Send `true` to read it. `recall` takes the same argument.
A bot's record of the release notes it was told is jojobot's bookkeeping
too, left out and read back the same way.
```

#### `search` — argument `/inputSchema.sid`

```text
**Your session id**, exactly as the boot door returned it. Pass it on
every call — it is what tells jojobot which bot is asking. Reads are
attributed, never journalled. With one, your own past runs also come
back as hits, ranked last; without one, no run does.
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
`active` or `archived`. **`status`, `provenance`, `standing`, `subject` and
`edge` ask about facts, so a search that uses one returns facts only;
`answers_type` and `fits_type` ask about things and return things only.
`left_out` does not list the kinds of hit they leave out.**
**Left unset, the search behaves like a mail
client**: active claims first, then archived ones — a claim that was
true once and was replaced or moved past — ranked below every active
one, and a hit that was replaced names the claim that replaced it in
`superseded_by` when include_links is set. A claim taken back as never true (retracted) is left
out. `active` is current truth only. `archived` reads every archived
claim, the retracted ones too.
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
IT ANSWERS WITH A RECEIPT, NOT THE CHARTER: the bot it landed on, how many bytes were stored — compare it with what you sent and you learn the store trimmed it — and a digest of about 120 characters of it, so you can tell which it was.
The receipt's `changed` lists what moved.
A bot that does not exist comes back status: blocked with the nearest handles — add_entity first; nothing is created here.
Rules are not written here either: a rule is a fact about the bot, so capture it.
Neither is a short self-description for the colleagues view: capture sets.one_liner on this bot instead — that is the key view:colleagues reads, and a bot nobody has written one for shows up there with nothing to say.
Who the bot reports to goes the same way: capture sets.reports_to on this bot, holding its manager's handle, and the colleagues view shows whom each bot reports to.
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
      "request_id": {
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
Text inside backticks or a fenced block is literal: a handle there is
kept as written and never becomes a link.
```

#### `set_charter` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the charter is written once. It needs a `sid`, and a call that
wrote nothing spends no id.
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
With no bot it is a read-only orientation: what jojobot is, a live snapshot (entities by kind, UP TO TWENTY BOTS NAMED so you can see which identities you could boot as, the rest counted) and the skills index (a name and what each is FOR, never the procedures: call again with skill: its name to get one).
NAME A BOT and the answer also carries that identity: its charter, its starred rules as dated claims each with provenance (testimony and observation are settled, inference is a hypothesis), and the counts of the mailbox it owns.
THIS DOOR CREATES NO IDENTITY: a name that is no bot comes back blocked, listing up to twenty bots.
Booting starts or resumes that bot's session — there is no separate start verb — and THIS DOOR IS WHERE A SESSION ID COMES FROM: that `sid` rides every verb after it, reads included, and passing `sid` here tells you whether it still addresses your session.
A bot with a run to pick up gets a resume-or-new choice and no sid until it answers with `resume`: the name of the run (its `run`), `new`, or the wrap_code a wrap handed back.
CALLED THIS BEFORE? Pass brief: true to skip the essay; calling without brief reads the essay.
`claim` holds a named role for 45 minutes so two sessions never work one role at once; a run that holds it keeps it until it wraps or its lease runs out, or the operator's code takes it, abandoning an active holder and leaving a wrapped one wrapped (taken, refused, conflict or unavailable; a claim is refused while its role object is archived, and archive_entity restores it); every write, and read_mailbox and read_message, renews the lease unless the last renewal is less than 4 minutes 30 seconds old, and wrapping releases it.
A boot writes the sweep, a missing box's repair, a key note and, when asked, a claim, a pick-up, a spent code or a wrap window.
```

#### `start_here` — structure

```json
{
  "inputSchema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "ask_for_code": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
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
      "challenge": {
        "default": null,
        "type": [
          "string",
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
      "code": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "compacted": {
        "default": null,
        "type": [
          "boolean",
          "null"
        ]
      },
      "offer_code": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "request_id": {
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

#### `start_here` — argument `/inputSchema.ask_for_code`

```text
**Ask for a code to boot as the bot `bot` names.** A session the operator
started by hand has no voucher, and this is how it gets one. jojobot
writes a fresh code into the operator's own mailbox, which is where the
operator reads their mail, and answers with a reference to the challenge and
never the code. The call starts no session. One challenge stands per bot:
while a code is waiting a new request is refused and names when it ends, a
code is good for 10 minutes, and three wrong codes void it sooner.

The operator types the code into this session. Boot again with `bot`,
`challenge` and `code` to present it.
```

#### `start_here` — argument `/inputSchema.bot`

```text
Optional. The bot to boot as: its bare slug, or its full `bot:`-prefixed
handle. A handle of any other kind is refused — this door boots bots.
Omit it for an anonymous orientation: you get the world and the
snapshot, and no sid.

Booting sweeps that bot's sessions that have gone a day without a beat to
`abandoned` (a run that states its own day: once its stated day is two days
past its last beat's); an abandoned run is offered back at a later boot for 7 days. It
does REPAIR one thing: a bot whose box is missing gets it opened here,
because a box is part of what a bot is and its absence is damage rather than
a setup step, and the answer says so plainly. A boot that starts a new run
ends any wrap window the bot has open.
```

#### `start_here` — argument `/inputSchema.brief`

```text
Skip the orientation essay and return only what changes between calls —
the snapshot, your identity, your session.

The essay does not change between calls.
An anonymous boot whose essay did not fit whole names every section it left
out; `section` fetches one.
```

#### `start_here` — argument `/inputSchema.challenge`

```text
**The reference to a challenge**, as the answer to `ask_for_code` named
it. Sent with `code` and `bot` on a boot.
```

#### `start_here` — argument `/inputSchema.claim`

```text
**A role to claim, by a name you choose.** Boot names this when you
want jojobot to refuse a second claimant while your claim is fresh —
**`taken` means your claim succeeded and YOU hold the role; it never
means somebody else has it, which is `refused`.** **A fresh lease held
by another run, of your own bot or any other, refuses you, naming the run, when its
lease was last renewed and when it runs out: the role changes hands when that run
wraps, when the lease runs out, or at the operator's word. A claim that
presents the operator's code (`ask_for_code`, then `challenge` and `code`)
takes it, and the answer says `took_over_from` that run and
`abandoned_by` the operator's word, and `holder_still_active` when
that run lost the role but could not be closed.** The answer is one of
`taken`, `refused` naming who holds it and until
when — or, when the role belongs to another bot, naming that `owner`, or,
when its role object is archived, saying who may restore it, and writing
nothing — `conflict` when the claim collided with another write
landing the same instant, or `unavailable` when the store could not
decide it. **A conflict is the store working correctly** —
retry the same call. **An unavailable
claim holds nothing**, and its answer says what to do next.
**A boot that names no claim claims the role the bot carries in
`claims_role`, if it carries one**, and is otherwise unchanged.
`claims_role` is written about a bot by
a different identity, never by the bot itself.

**Every write you make while holding it renews the lease**, not
only a journal beat — capture, add_entity, post_message and the
rest all count, declare_type and amend_journal too, and so does a
read_mailbox of your own box — and the lease is 45 minutes. A write less than 4 minutes 30 seconds
after the last renewal renews nothing, so a lapse can come up to that
much sooner than 45 minutes after your last write. **Wrapping releases
whatever you held**, so a fresh session may claim the same role at
once.
```

#### `start_here` — argument `/inputSchema.code`

```text
**The code the operator typed in.** A boot that presents the right code
for its bot is given a voucher: the root when the bot is the assistant,
that bot's ordinary voucher otherwise. The code is spent by the boot that
gets a sid, and the third wrong code voids the challenge.
```

#### `start_here` — argument `/inputSchema.compacted`

```text
**Send `true` with your live `sid` when your context was compacted.** If
you no longer hold the `sid`, send `bot` alone: it reports the bot's one
live run, and is refused, naming the runs, when the bot has none or more
than one. A compaction takes away what jojobot taught you and showed you,
and jojobot cannot tell. This call says so: jojobot forgets what it believed this run
was taught and shown, restarts the running total of what the run has been
handed, and writes a dated mark in the run's chronology. The compaction is
also counted on that week's usage record (recall with view: usage).

**The run stays the run.** Same `sid`, same chronology, same identity: no
new key, no voucher moves, and no other run of the bot is touched. The
answer is the full orientation, charter included, whatever `brief` says,
because that is what the compaction took. A `resume` or an `offer_code`
beside it changes neither the answer nor the run. A `skill` or a `section`
beside it is refused: a fetch touches no run. A `sid` that opens no
run is refused. Naming `bot` is optional; when you do, it has to be the
bot the `sid` belongs to.
```

#### `start_here` — argument `/inputSchema.offer_code`

```text
**The `offer_code` the resume-or-new offer carried**, sent beside `resume`.
With it, and only with it, the answer to that offer carries what is new.
Without it, or with another bot's or a used code, the answer is the boot less the charter.
```

#### `start_here` — argument `/inputSchema.request_id`

```text
**A name of your choosing for a compaction report**, up to 128 bytes, sent
with `compacted: true` and your `sid`. The report is made of parts, the
mark in the chronology and the restart of the running total: if an answer
is lost or a part fails, send the same call again with the same
`request_id` and the mark is not written twice. A boot takes none.
```

#### `start_here` — argument `/inputSchema.resume`

```text
Your answer to the resume-or-new choice a boot hands back when this bot
has a session worth picking up: the name of a stopped or quiet run (its
`run`), exactly as the offer spelled it, or `new` for a fresh session. A
name is not a key: never send a `sid` here. Leave it off on a first boot —
there is nothing to answer yet.

**Or the `wrap_code` a wrap handed back**, with `bot`: for 2 hours after
the wrap it hands the wrapped run a new sid, and the run takes every
write a live run takes, with no second wrap needed. No role is taken,
nothing is swept and no lease renews.

Answering the resume-or-new offer? Send the offer's `offer_code` beside
it. With the code, the answer carries what is new (the run, its key, the
role claim, the zone, and any rule written since the offer) and leaves out
what the offer shipped: the essay, the snapshot, the charter, the carried
rules and the map of those at home, and the handover story. `left_out` names
all but the charter, and the call that returns them, a boot with no
`resume`.
Without the code, or with a code that is another bot's or already used,
the answer is the full boot less the charter. A wrap code sent after the
2 hours is refused and the answer says when the window closed.
```

#### `start_here` — argument `/inputSchema.section`

```text
**One unit of the essay's remainder, left out of an anonymous boot's
answer by the ceiling** — by its exact `##` heading, or the literal
word `"opening"` for the remainder's un-headed opening paragraphs.
The answer carries that one unit, whole, and nothing else from the
essay — never the core, which always ships and needs no fetch.

It takes no bot and starts no session, and it writes nothing.
A name that matches no section comes back blocked, naming every
valid one, `"opening"` included.
```

#### `start_here` — argument `/inputSchema.sid`

```text
The session id you are already carrying, if you have one — the same
`sid` that rides every other call you make. Leave it off when you have
none: this door is where one comes from, so a first boot has nothing to
pass. **Outside `compacted`, a handle is never turned away here**, whatever
became of it: the
answer says whether the one you carried still addresses your session, so
a session that came back to a server it does not recognise learns that
in the same call it re-orients with.
```

#### `start_here` — argument `/inputSchema.skill`

```text
**A skill to read, by the name the index gave it.** A boot lists the
skills meant for it by name and when-to-use and ships no bodies; this is
how you get one, once the index has told you it is relevant. Any skill can
be fetched by name, whether or not your boot lists it.

It is the same door because this door is skill zero: it is where a
session learns the model, so it is where a session learns the
procedures. A name that is no skill comes back blocked, naming the ones
that are. A fetch that sends your `sid` records that this session read
the skill, so the act it covers is not offered to you again; it changes
no run.

**A booted session may send `skill` beside its own `bot` and its live
`sid`.** Any other boot argument beside a fetch (`resume`, `claim`,
`timezone`, `today`, `brief`, `request_id`, `ask_for_code`, `challenge`,
`code`, `offer_code`) refuses the whole call and names it, because a
fetch touches no run and would drop it. A `bot` with no live `sid` of
that bot is refused too.
```

#### `start_here` — argument `/inputSchema.timezone`

```text
**Omit this on a boot.** The instance holds the operator's zone and
answers every run that sends none in it; the boot says which zone it
used and where it came from. Send an IANA name only when this run really
works in another zone, and send it again when you resume from somewhere
else. A zone you invent moves the day for everything this run writes.

It is what *today* means for everything day-grained: the date a `capture`
gets when you name none, and whether a rhythm has fallen due.
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
It does not touch mailboxes: a box belongs to the bot it is named for and opens with it, in add_entity (the operator's, at the first post to them).
Any change to what it is CALLED — name or aliases — faces the same check a creation does, because an alias is a name: it can come back status: blocked with candidates, and the override_token that refusal carries is how you confirm a genuinely shared name — a token lifts the one refusal that minted it and no other.
A refusal on the name alone also needs override_reason, shown beside the name.
Passing `aliases` REPLACES the whole set ([] clears it); source and crm edits are never questioned.
`sets` sets keys on the thing in the same call, through a claim of inference; a call that sends only `sets` leaves the entity itself alone, and a key that needs the operator's word is written with capture.
A rule's `starred` mark stays on its claim, so it is not set here: update_fact on the rule's address with fields {starred: "true"} or {starred: "false"} stars or unstars it.
On a record the build supplies (a shipped view), `name` and `aliases` are yours, laid over the build's: once you name it, the shipped name no longer answers (it returns only as an alias you list), so a new thing may take it; your aliases ADD to the build's and `[]` clears only yours; an upgrade leaves yours on top.
`source` and `crm` stay the build's, refused with no token.
A handle that names nothing comes back blocked with the nearest handles — it never creates.
Aliases or `sets` that would leave the thing, or its claim, too big to be read back whole in one answer are refused with nothing written.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
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
      "override_reason": {
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
      "request_id": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "reviewed": {
        "default": null,
        "type": [
          "boolean",
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
edit it. **With `reviewed`, several handles joined by commas** mark each
thing reviewed in one call (at most 50), and take nothing else.
```

#### `update_entity` — argument `/inputSchema.name`

```text
New display name.
```

#### `update_entity` — argument `/inputSchema.override_reason`

```text
**Why you insist**, sent beside `override_token` when the refusal rests
on the name alone: the same name under another kind, a name that holds or
sits inside another, a known nickname of another, or two rare words shared
with another of the same kind. Without it that refusal stands. The reason is
kept on this entity as `told_apart_by`, shown beside its name.
```

#### `update_entity` — argument `/inputSchema.override_token`

```text
The token a previous call's refusal handed you, sent back after you read
its candidates for the name or alias you are claiming here and judged
them a different entity. It lifts only the refusal that minted it. Any
change to what this entity is CALLED is screened exactly as a creation
is.
```

#### `update_entity` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the edit is made once. It needs a `sid`, and a call that
wrote nothing spends no id.
```

#### `update_entity` — argument `/inputSchema.reviewed`

```text
**Mark the thing reviewed, and change nothing else.** Only the bot
`defrag` may send it, and only `true`: jojobot stamps the thing with the
moment, and it leaves the review queue (`recall` with `unreviewed`) until
somebody else writes to it. Any other bot is refused. The mark is ONE
standing record per thing, rewritten in place by every later mark, and it
takes no slot under a record ceiling.

**Several things at once**: send their handles joined by commas in
`handle`. Each is marked as one handle would be, and the receipt's
`reviewed` names how many and which. A handle that names nothing, or that
cannot be edited (a voucher), refuses the call before any thing is marked.
A list takes a `request_id`: if a store failure cuts it, send the same call
again with the same `request_id` and it marks the rest and none twice. To
read the queue one container at a time, `recall` with `unreviewed` and
`parent`.
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

**Sending again the keys your own earlier "Set ... on" claim holds, and no
other claim does, rewrites that claim in place** rather than adding one;
its earlier values stay readable in its history. Any other write adds a claim.

The claim is `inference`, as a capture's is when it says nothing. To put
the operator's own word behind a key, write it with `capture`.
Setting `status` to `now` on work your bot owns also points your bot's
`working_on` at it.
A write whose status words could not be read from `topic:instance` still lands, and its receipt carries `instance_unread`, naming under `not_worked_out` what it left undone.
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
Edit one addressed claim in place — fix the source, never an addendum.
It changes `content`, `details`, `recorded_at` (the day the claim was MADE, never the day you call), `status`, `provenance`, `standing`, `fields`, `sets`, `stands_for` and the claim's edge.
ARCHIVE, RETRACT OR REWRITE — one rule, stated the same in update_fact, its status argument and retract.
A claim that was true and has changed, or that was never true: archive it with update_fact (status: archived, the reason in details) and capture the replacement, naming the old claim in derived_from.
A claim about a past EVENT that turned out never to have happened: retract it, which also leaves a dated record of the retraction.
A mistyped or reworded claim: rewrite it.
Archiving and retracting are one-way for a claim: nothing edits an archived claim, status included, so there is no un-archive (an archived ENTITY comes back with restore: true).
A rewrite destroys nothing — every earlier wording stays readable (recall with history_record: the address) — so a claim you disagree with is safe to correct, except testimony from an earlier session: TESTIMONY IS REWRITTEN IN PLACE ONLY BY THE SESSION THAT WROTE IT, and any other is archived and replaced, with derived_from naming the archived claim.
Two moves are gated: anything to testimony needs confirmed_by_user; open to settled needs a `receipt` (`check` or `confirmed`), which leaves provenance alone.
It answers with a receipt read back, not the record.
A call naming no change is refused; keep: true alone touches it.
expected_revision (recall's revision_count): a stale edit is refused with retry.
find, replace, field (content or details): change one span, if find occurs once.
Naming an own key in `sets` moves it.
An unknown address is blocked with candidates.
`sets_add` and `sets_remove` change a list key by its items.
`after` moves a list entry (`first`, `last`, an address) or a section.
The receipt's `changed` lists what moved.
```

#### `update_fact` — structure

```json
{
  "inputSchema": {
    "$defs": {
      "ConfirmedArg": {
        "properties": {
          "at": {
            "type": "string"
          },
          "on": {
            "type": "string"
          }
        },
        "required": [
          "on",
          "at"
        ],
        "type": "object"
      },
      "ReceiptArg": {
        "properties": {
          "check": {
            "default": null,
            "type": [
              "string",
              "null"
            ]
          },
          "confirmed": {
            "anyOf": [
              {
                "$ref": "#/$defs/ConfirmedArg"
              },
              {
                "type": "null"
              }
            ]
          }
        },
        "type": "object"
      }
    },
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "properties": {
      "address": {
        "type": "string"
      },
      "after": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
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
      "expected_revision": {
        "default": null,
        "format": "uint32",
        "minimum": 0,
        "type": [
          "integer",
          "null"
        ]
      },
      "field": {
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
      "find": {
        "default": null,
        "type": [
          "string",
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
      "override_reason": {
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
      "provenance": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "receipt": {
        "anyOf": [
          {
            "$ref": "#/$defs/ReceiptArg"
          },
          {
            "type": "null"
          }
        ]
      },
      "recorded_at": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "replace": {
        "default": null,
        "type": [
          "string",
          "null"
        ]
      },
      "request_id": {
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
      "sets_add": {
        "additionalProperties": {
          "type": "string"
        },
        "default": null,
        "type": [
          "object",
          "null"
        ]
      },
      "sets_remove": {
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

#### `update_fact` — argument `/inputSchema/$defs/ConfirmedArg`

```text
Where and when the operator confirmed a claim.
```

#### `update_fact` — argument `/inputSchema/$defs/ConfirmedArg.at`

```text
Where: a message id or the session.
```

#### `update_fact` — argument `/inputSchema/$defs/ConfirmedArg.on`

```text
The day the operator confirmed it, `YYYY-MM-DD`.
```

#### `update_fact` — argument `/inputSchema/$defs/ReceiptArg`

```text
What a claim was settled on.
```

#### `update_fact` — argument `/inputSchema/$defs/ReceiptArg.check`

```text
**A check that ran**: what was checked, written so another agent can run
it again. It records that a check ran and never runs it.
```

#### `update_fact` — argument `/inputSchema/$defs/ReceiptArg.confirmed`

```text
**The operator confirmed it**: the day, and where (a message id or the
session).
```

#### `update_fact` — argument `/inputSchema.address`

```text
The fact's global address, `kind:slug#local-id` — exactly as `recall`
returned it. **A claim cannot change subject:** to move one filed on the
wrong thing, capture it on the right one, re-sending its dates, `fields`, `sets` and
edge, with `derived_from` naming this claim, then archive this one.
```

#### `update_fact` — argument `/inputSchema.after`

```text
**Move an entry of a list: `first`, `last`, or the address of the open
entry it should follow**, as `recall` serves it under `entries`. The entry
reads where you put it until it is next ticked; then the list goes on as
its `tick` setting says (a rotation turns, a checklist sinks the done).
Only an open entry moves, and only behind an open one: a done entry sits
where it was done, and a dropped one is out of view. Refused, with nothing
written, and the refusal says what to send instead. The server mints the place
(`order`, `held_turn`, `held_since`); a caller never sends them.

**Move a section: the address of the section it should follow**, `kind:slug#local-id`
of a live section of the same document. The section goes right behind it,
with every chunk that carries it on, and no other section is written.
Refused, with nothing written, when this record is not a live section of a
document (an ordinary claim has no place to move), when it is a chunk that
carries on the section before it (move that section), or when the target is
not a live section of the same document. A section can be moved and edited
in one call. A move of a long section that stops partway is finished by
sending the same call again. **A move never touches the section's words**:
it changes where the section sits, so it needs no say-so from the session
that wrote them, and an edit of the words sent in the same call is still
held to the rule for them.
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

**It takes the key off the thing as well as off the record**, whichever
bag holds it, so this is the way to take a key off a thing. Send it to
the active record that carries the key: `recall` the thing with
`backing` true and the key names that record. A counter key names none,
because its total has no single record behind it: `recall` with
`history` set to the key lists the records that wrote it, oldest first,
and `history_most` widens the list when its count says more exist.
Clearing the key on one of them takes the whole total off the thing.

To move a value to another key, send the new key in `sets` (or in `fields`
for a key of the claim itself) and `clear_fields` with the old one in the
same call.
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
Required to promote an EXISTING claim to testimony (inference or
observation alike). Set it only when the operator has actually
confirmed the claim. Nothing else is gated on it — a fresh `capture`
declares its provenance and its standing on honour. Settling an open
claim takes a `receipt` instead, and this flag does not settle it.

The flag is trusted, not checked: nothing verifies that the operator said
yes.
```

#### `update_fact` — argument `/inputSchema.content`

```text
Replacement claim.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as the
handle that thing wears today, even after a rename.
Text inside backticks or a fenced block is literal: a handle there is
kept as written and never becomes a link.

An edit that would leave the claim too big to be read back whole in one
answer is refused with the claim unchanged: shorten it, split it into claims
that each say one thing, or keep the long text on a document (`put_document`)
and write a pointer claim.
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
mentions exactly as `content` does. **With `status: archived` they are the
reason**: the record keeps it as `archive_reason` and its own details stay.
```

#### `update_fact` — argument `/inputSchema.expected_revision`

```text
**The revision you read**, when the edit is only right if nobody has
touched the claim since: the `revision_count` `recall` served for it. A
claim `recall` serves without one has been written once, so it is
revision 1. When the claim has been written since, the edit is refused
with `retry`, naming the revision it is at now, and nothing is written.
Leave it off and the edit lands as it always did. An edit's receipt
carries `revision`, the claim's count after that edit: send it as the
next guarded edit's `expected_revision` without a `recall` between.
```

#### `update_fact` — argument `/inputSchema.field`

```text
**Which text `find` searches: `content` or `details`.** Required with
`find`, and no default, so the edit never lands on the wrong text by
being left out. The receipt's `text_edit` says what was taken out, what
was put in, and the bytes before and after.
```

#### `update_fact` — argument `/inputSchema.fields`

```text
**Fields to set**, as key/value pairs. Each key named is written; a key
the record already carries and this does not name is left alone, so an
edit reaches one field without restating the rest.

A field you set that equals what jojobot ships as its default today is
stored exactly as you sent it either way, and the receipt's echoes_defaults
names which key that was.

`read_from`, `read_ref`, `starred`, `subject`, `purpose` and `recorded_by` describe their own claim and never fold onto the thing: read them back with `facts: true`, and find by them with a `fields` filter with `scope: record`.
```

#### `update_fact` — argument `/inputSchema.find`

```text
**Words to find in the claim's text, to change just that part.** Send it
with `replace` and `field`, and nothing else of the text has to be sent
back. The match is exact, case-sensitive and byte for byte; no pattern.
The words have to occur exactly once in the text as it stands now:
none, or more than one, is refused naming the count, and nothing is
written. Not combined with a whole `content` (or `details`) for the
same field; the other field may be sent. Works with `expected_revision`
and without it.
```

#### `update_fact` — argument `/inputSchema.happened_at`

```text
**The day the thing this claim is about HAPPENED**, `YYYY-MM-DD` —
a different question from `recorded_at`, which is about the claim.

**Learning it late is ordinary**: a claim written when nobody knew the
day gains one here. Leaving it off keeps whatever the record says, like
every other field this patch does not name.
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
how you say, on purpose, that one still matters. A quiet thought is
outside the room's count, so keeping it into a full room is refused
like a new thought, until a live thought is archived. The one call
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

#### `update_fact` — argument `/inputSchema.override_reason`

```text
**Sent beside `override_token`**, which this verb does not hand out at
present. Leave it off.
```

#### `update_fact` — argument `/inputSchema.override_token`

```text
**Nothing on this verb hands out a token at present**, so there is nothing
to send here. Leave it off. The refusals that carry a token are the ones
on `add_entity`, `update_entity`, `rename_entity` and `declare_type`.
```

#### `update_fact` — argument `/inputSchema.provenance`

```text
`testimony`, `observation` or `inference`. Moving a claim TO `testimony`
needs `confirmed_by_user` whichever value it held: a claim you read
somewhere is not a step towards the operator having said it. An
`observation` must carry `read_from`, here as at capture — and **taking
the source off a claim that stays an observation is refused for the same
reason**, because both leave a machine read of a system nobody named.
A claim that already carries one does not have to name it again.

A session that wrote a claim keeps its provenance when it rewrites the
words and sends none. Another session rewriting the words must restate
it, even when unchanged. An edit that leaves the words alone keeps it.
```

#### `update_fact` — argument `/inputSchema.receipt`

```text
**Why this claim is settled**, required to move an open claim to
`settled`. Exactly one of two shapes: `check`, what was checked, written
so another agent can run it again; or `confirmed`, the operator said so,
as `{on: YYYY-MM-DD, at: a message id or the session}`. It records that a
check ran or that the operator confirmed; it never runs the check, and it
never changes who backs the claim.
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

**Naming a day that differs from your run's stated day is never
refused** — the receipt carries `recorded_at_note`, one sentence
naming both days, so a sitting acting out a different period does not
silently record under the wrong one by mistake.
```

#### `update_fact` — argument `/inputSchema.replace`

```text
**The words that take the place of `find`.** An empty string deletes
them. Sent with `find`, never alone.
```

#### `update_fact` — argument `/inputSchema.request_id`

```text
**A name of your choosing for this write**, up to 128 bytes.
**Send it on the first call of the write**, because a call sent without
one cannot be repeated safely. If an answer is lost, send the
same call again with the same `request_id`: if
the first call wrote, you get its receipt back marked `repeat` and nothing
is written a second time, so the edit is made once. It needs a `sid`, and a call that
wrote nothing spends no id.
```

#### `update_fact` — argument `/inputSchema.sets`

```text
**Fields to set on the thing this claim is about**, as key/value pairs,
apart from `fields`, which are the claim's own. **Naming under `sets` a
key the record holds as its own field MOVES it to the settings in this
call**, with the value you send, and the receipt says it moved. The other
way is two calls: a setting named under `fields` is refused, so take it
off with `clear_fields`, then write it under `fields`. A key that only
labels the claim is never a
property of the thing and is refused here by name. The keys that make a
thing fall due act on it only when sent here. A list key (`depends_on`)
changes by its items with `sets_add` and `sets_remove`: its whole value
sent here replaces what another session added. Setting `status` to `now`
on work your bot owns also points your bot's `working_on` at it.
A write whose status words could not be read from `topic:instance` still lands, and its receipt carries `instance_unread`, naming under `not_worked_out` what it left undone.

A date under `runs_out`, `decide_by` or `promised_by` in `sets` makes a thing fall due on that day; `ended` beside one ends it. `cadence_days`, `rrule`, `advances_from`, `counts_from`, `anchored_on` and `snoozed_until` together set when a thing falls due; `outcome` beside them changes it. A `project` or `work` whose `status` is `done` is not owed, and its dates stay on it. Every other key is kept as written and acts on nothing: it never makes a thing fall due or appear in what is owed.
```

#### `update_fact` — argument `/inputSchema.sets_add`

```text
**Items to add to a list key on the thing this claim is about**, as
key/value pairs where the value is the items, separated by commas:
`{depends_on: "work:gamma"}`. Accepted only for a key declared a list. The
thing's current value is read inside this call, whichever record wrote it,
and the items are appended to the end, in the order sent; an item it
already holds is not added twice and every other item stays where it was.
A handle that names nothing is refused with the nearest handles. The answer
is stored as an ordinary whole value. Naming the same key in `sets` is
refused, and so is sending this beside `after`.
```

#### `update_fact` — argument `/inputSchema.sets_remove`

```text
**Items to take off a list key on the thing this claim is about**, in the
form `sets_add` takes. The rest of the list stays where it was. An item the
list does not hold is no error, and neither is a handle that no longer names
anything, so a reference to a thing that is gone can come off. With
`sets_add` in one call, the removals are applied first.
```

#### `update_fact` — argument `/inputSchema.shape`

```text
The shape of an edge to attach: `location` · `membership` · `attendance` ·
`about` · `connection` (a link is there and how it relates was not
recorded). Requires `object`; neither works alone. A read serves the
shape by its schema.org word: `connection` reads back as `relatedTo`,
`membership` as `memberOf`, `attendance` as `attendee`, and `location`
and `about` as written.
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
the content. Past the day a read SAYS SO and **nothing else happens**:
no sweep, no reminder, nobody is coming to check it.
```

#### `update_fact` — argument `/inputSchema.standing`

```text
`settled` or `open`. **Moving a claim that is ALREADY open to settled
requires a `receipt`**, and is refused without one. Reopening is free,
and takes the receipt off the claim; the claim's history keeps it.

That is the whole of the gate: it is on this promotion, not on
declaring a standing. A fresh `capture` may state `settled` and is
taken at its word, the way it is taken at its word about provenance.

Settling never changes provenance: a settled inference reads back as an
inference, with its receipt. When the operator hedged a claim and no
longer does, settle it with the `confirmed` receipt and leave provenance
alone; the claim was theirs from the start.
```

#### `update_fact` — argument `/inputSchema.stands_for`

```text
**Mark this record as standing for the claims named here**, each as
its address `kind:slug#local-id`. A synthesis, not a citation: the
named claims stay exactly as they are — active, readable, untouched —
so the full picture is still there for anyone who reads them. A write
replaces the whole set. Every named address must already exist;
naming this record's own address, or the same address twice, is
refused — and so is an empty set (clear the mark instead) or a record
that already stands for others, because a shape cannot name a shape.
```

#### `update_fact` — argument `/inputSchema.status`

```text
`active` or `archived`. ARCHIVE, RETRACT OR REWRITE — one rule, stated the
same in update_fact, its status argument and retract. A claim that was
true and has changed, or that was never true: archive it with update_fact
(status: archived, the reason in details) and capture the replacement,
naming the old claim in derived_from. A claim about a past EVENT that turned
out never to have happened: retract it, which also leaves a dated record of
the retraction. A mistyped or reworded claim: rewrite it. Archiving and
retracting are one-way for a claim: nothing edits an archived claim,
status included, so there is no un-archive (an archived ENTITY comes back
with restore: true). Rewriting `content` into its own denial ("the club
does NOT meet on Tuesdays" replacing "the club meets on Tuesdays") leaves a
sentence about what is not so where a claim about what is true belongs. A
negative is still an ordinary fact when it is not correcting anything: "the guest
did not attend" stands on its own, written with `capture` like any other
claim. The receipt of an archive names the keys the thing lost, under
`left_the_thing`.
```

### Tool `wrap_session`

#### `wrap_session` — description

```text
End your session and tell its story.
Two things happen together: the story is recorded in your chronology as its final entry, and the session moves to `wrapped` — terminal both ways: nothing appends to it or reopens it except through the wrap_code below, and a later journal, amend_journal or wrap_session on that id comes back blocked unless its wrap window is open.
A retried wrap finishes the first one.
The next boot of this identity is handed the story as the handover, so it is not private.
A session that stops without wrapping is not lost: the next boot sweeps it to `abandoned` after a day (two stated days if it states them) and offers it back to resume for 7 days; its story was never told.
Wrapping is also how you ROTATE a long run (wrap, then boot again for a fresh sid) and how work moves to another agent NOW: the wrap frees your role at once (a role still held is in the answer's `roles`, with `held_until` and the reason), and the next agent boots fresh and reads your story.
A wrap ends every voucher your run holds, and so does picking a run up by name with start_here's resume; a session that needs one again asks for a code with start_here's ask_for_code.
A resume note in `journal` is for a run that goes quiet; it is picked up by name once the lease has passed, and a key never travels.
THE FIRST WRAP ALSO HANDS BACK A `wrap_code`: start_here with it as resume reopens the run for 2 hours as a normal session, any write and the closing story included, with no second wrap needed; a second wrap or a newer run ends it early.
A second wrap's answer carries no `wrap_code`.
THE ANSWER IS A RECEIPT: it omits the story; the chronology it hands back is the newest of the record, story included (`entry_count` is the whole length; `chronology_elided` and `entries_omitted` say how much is not here, and nothing was dropped).
`closing_focus` holds the focus, unless the story repeats it.
The receipt's `changed` lists changes, or is `nothing` or `not_read`.
Pass your `sid`.
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
this session's own chronology, and the next boot of this identity is
handed it as the handover, so it is not private to the session.

Write `@kind:slug` to link to something that already exists, e.g.
`@person:milhouse` — stored as the name that does not move, served as
the handle that thing wears today, even after a rename.
Text inside backticks or a fenced block is literal: a handle there is
kept as written and never becomes a link.
```


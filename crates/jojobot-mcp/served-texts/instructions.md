# Server instructions

The instructions the server sends a client when it connects, with the keys that make a thing fall due built from the shipped carriers.

Written by `every_text_a_model_reads_matches_the_snapshot` in `crates/jojobot-mcp/src/snapshot.rs` and compared with it at every `make check`. A commit that changes one of these texts updates this file in the same commit: `make snapshot` rewrites it. What the whole snapshot cannot reach is written at the top of that source file.

## Instructions sent at connection

```text
jojobot — a personal-assistant server. Two worlds live here.

**MEMORY.** What jojobot knows is **entities** — each with a typed handle, `kind:slug` — the name it is addressed by, and the only name a caller ever sends — and **facts** about them: single dated claims, each carrying an **address** (`kind:slug#local-id`) it can be edited through and a **provenance** saying who backs it: a claim nobody confirmed is labelled a hypothesis until the user confirms it. **jojobot is where durable things are kept**, so a fact you want a later session to find goes here. A fact may also draw one typed **edge** at another entity — and edges are what make cross-entity questions ("which people are in X") answerable without reading everything. **Start with `search`**: one ranked list over entities, facts and free prose, every hit arriving with its surroundings — and over mailbox messages too when you pass `include_mail: true`.

**A record carries FIELDS** — a flat bag of key/value pairs beside the claim, stored as you wrote it. Nothing has to be declared first, and a key you invent is kept as you wrote it. **A value that is a handle, or a comma list of handles, is a link and is refused when it names nothing**, and six keys describe their own claim and never reach the thing. **A thing's fields are every write on it, folded, the newest write of each key winning, and a key declared a counter summing** — a write that takes a key off takes it off the thing — and **carrying keys is what makes a thing a type**: declare a type to say which keys it names, and a thing holding all of them fits it. Declaring admits nothing — a thing is found by the keys it carries whether or not anybody declared the type.

**jojobot hands back the small answer and keeps the large one reachable.** A write returns a receipt, not the thing you wrote; a body is not echoed to its author; a delivery leaves out what it handed you once; prose is off by default on a read. Context is the scarce thing, and **eliding is never silent** — the answer says what was left out and which call returns it.

**MAILBOXES.** A place to leave a message for someone who is not in this conversation. A mailbox is a named box (`[a-z0-9-]+`); a message in one moves from left, to delivered, to acted on — a terminal archive, never deleted. **Messages are searchable, on request**: `search` with `include_mail: true` returns them beside the memory hits, in every state including the processed archive (the operator's box is the one exception: it is never searched), each hit carrying its box, its state, its sender and the id `read_message` takes — so a message left for one session is findable by any of them. `read_message` takes delivery of that one message; `read_mailbox` takes the whole box, and everything in it becomes yours to finish.

**Three rules of engagement.** 1. **Everything a write NAMES must already exist.** jojobot never brings an entity or a box into being as a side effect — not a capture's subject, not an edge's object, not the box you post into (the operator's own box is the one that opens at the first post to them). Something genuinely new is two deliberate steps: create it, then write. 2. **Confirm, don't guess.** A creation, or a change to what something is CALLED, that resembles something jojobot already knows comes back as a SUCCESSFUL result whose body says `status: blocked`, `wrote: false`, with `candidates` and `how_to_proceed` — nothing was written; use the candidate you meant, or re-call with the `override_token` that refusal carries if it truly is a different thing sharing a name — the token lifts the one refusal that minted it and no other. **Naming something that does not exist is blocked too**, with whatever is nearby — never a plain error, so branch on `status`, not on whether the call errored. A plain error is a malformed call, or the store failing. Nothing on this surface deletes anything. 3. **Mark a message processed only AFTER acting on it**: mark first and then fail, and it is gone from every future delivery with nobody the wiser; act first and crash, and the next read hands it back, flagged `seen_before` — recoverable.

That is the shape. The vocabulary — which provenance, which edge, which kind, which mailbox state — lives on `start_here` and on each tool's own description, and moves faster than this text could stay true to; call `start_here` for the full orientation and worked examples.

Responses name types the schema.org way; input stays lowercase, `kind:slug`.

**WHAT FALLS DUE.** A date under `runs_out`, `decide_by` or `promised_by` makes a thing fall due on that day. `cadence_days`, `advances_from`, `counts_from` and `snoozed_until` together set when a thing falls due. `outcome` and `ended` set no day and still change what a thing owes. A `project` or `work` whose `status` is `done` is not owed, and its dates stay on it. Every other key is kept as you wrote it and nothing acts on it: it never makes a thing fall due or appear in what is owed.
```


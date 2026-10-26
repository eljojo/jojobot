# Server instructions

The instructions the server sends a client when it connects, with the keys that make a thing fall due built from the shipped carriers.

Written by `every_text_a_model_reads_matches_the_snapshot` in `crates/jojobot-mcp/src/snapshot.rs` and compared with it at every `make check`. A commit that changes one of these texts updates this file in the same commit: `make snapshot` rewrites it. What the whole snapshot cannot reach is written at the top of that source file.

## Instructions sent at connection

```text
jojobot — a personal-assistant server with two worlds.

**MEMORY.** **Entities** have typed handles, `kind:slug`, the name each is addressed by. **Facts** are dated claims about them, each with an **address** (`kind:slug#local-id`) and a **provenance** saying who backs it; a claim nobody confirmed is a hypothesis until the operator confirms it. **jojobot is where durable things are kept.** A thing's **fields** are its `sets` writes, folded, the newest write of each key winning (a handle, or one in a comma list, is a link and must exist; a counter key sums). **Start with `search`**, which reaches mail with `include_mail: true`.

**MAILBOXES.** A message left for someone not here moves new, read, processed. `read_mailbox` takes the whole box, all of it yours to finish; `read_message` takes one. `search` finds mail in every state except the operator's box, which no bot reads.

**Three rules.** 1. Everything a write NAMES must already exist; something new is two steps, create then write. 2. A name resembling something known comes back as a SUCCESSFUL result with `status: blocked`, `wrote: false`, `candidates` and `how_to_proceed`: use the candidate, or re-call with the `override_token` that refusal carries. Branch on `status`; a plain error is a malformed call. 3. Mark a message processed only AFTER acting on it.

A write returns a receipt, and eliding is never silent: it says what it left out and which call returns it. `start_here` has the vocabulary.

**WHAT FALLS DUE.** A date under `runs_out`, `decide_by` or `promised_by` in `sets` makes a thing fall due on that day; `ended` beside one ends it. `cadence_days`, `rrule`, `advances_from`, `counts_from`, `anchored_on` and `snoozed_until` together set when a thing falls due; `outcome` beside them changes it. A `project` or `work` whose `status` is `done` is not owed, and its dates stay on it. Every other key is kept as written and acts on nothing: it never makes a thing fall due or appear in what is owed.
```


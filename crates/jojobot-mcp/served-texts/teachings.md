# Teachings

Every teaching that rides once per session on the verb it is about.

Written by `every_text_a_model_reads_matches_the_snapshot` in `crates/jojobot-mcp/src/snapshot.rs` and compared with it at every `make check`. A commit that changes one of these texts updates this file in the same commit: `make snapshot` rewrites it. What the whole snapshot cannot reach is written at the top of that source file.

## Teachings

Each rides once per session, on the verb it is about.

### Teaching `CLAIMS_TEACHING`

```text
A further claim does not destroy the one already there — capture a second claim about the same thing and it does not erase the first, even when the two contradict each other. jojobot runs no inference and settles nothing, so two accounts that disagree are both allowed to stand: recording the new one is not a judgment that it is the true one, and deciding between them was never the job. Reach for capture when a new thing happened; reach for update_fact only to correct what the record already says — it rewrites a claim in place, and recall with a history argument reads the earlier wording back.

Three corrections, three different moves. Mistyped it just now? update_fact in place — the record becomes what it should have said, and the wrong wording stays readable through history. It changed, or it was never true at all? Archive the old claim with update_fact (status: archived, details saying why) and capture the new one, naming the archived claim as the new one's derived_from when there is a direct replacement. Archive it, never negate it: rewriting a claim into its own denial — 'the club does NOT meet on Tuesdays' replacing 'the club meets on Tuesdays' — leaves a sentence about what is not so where a claim about what is true belongs, and the archived claim already says it stopped standing. A negative is still an ordinary fact when it is not correcting anything: 'he did not attend' stands on its own.
```

### Teaching `CLAIM_SUBJECT_TEACHING`

```text
A claim's fields may carry two keys nothing enforces.
`subject` is a one-line label, the way an email has a subject.
`purpose` says what the claim is FOR — its value is open, read back from whatever is already in use rather than chosen from a fixed list, so it settles by use rather than by a list somebody maintains.
Both are written by whoever makes the claim.
Together they let a later read tell what a claim is about and why it was written without opening it, and keep two sessions from inventing two different names for the same idea.
```

### Teaching `RHYTHM_HISTORY_TEACHING`

```text
A rhythm's `counts_from` and `due_on` are jojobot's arithmetic, never a caller's to type in — even for the first cycle.
A loop whose last run already happened, before this session opened it, is opened the same way an ordinary cycle is closed: capture a check-in on it, and jojobot works the rest of the schedule out from there.
The day that check-in dates the schedule from is recorded_at, never happened_at — send the day the turn actually happened as recorded_at, and jojobot derives the rest.
```

### Teaching `CLAIM_DIRECTION_TEACHING`

```text
A claim hangs on the thing it is ABOUT; the edge names the other party.
Ask which entity the sentence itself describes, and put the claim there — the edge then points at whichever other entity it also involves.
Filing it on the wrong side stores a claim that is true and finds nothing: a read scoped to the entity the claim should have been about never sees it, because that read returns what it was asked for and an inbound edge grants the other party nothing.
```

### Teaching `PROJECTS_SKILL_TEACHING`

```text
You are writing about a project's work: start_here with skill: "projects" has the routes for a date that moves, a decision, an open question and what you worked out from what was said.
It also holds the keys that say where a piece of work stands (`status`) and whose it is (`owner`).
```

### Teaching `RHYTHMS_SKILL_TEACHING`

```text
You are checking in on a loop: start_here with skill: "rhythms" has what a loop holds, how a check-in closes one three ways, and how to look ahead and back at what falls due.
```

### Teaching `DUPLICATE_REPAIR_TEACHING`

```text
You created this past a near-miss refusal.
If it later turns out to be the same thing as one of those, merge_entities folds one into the other: the claims, the edges drawn at it and the history under its keys move to the survivor.
A recall of the handle you merge away answers status merged, names the survivor in merged_into and holds nothing itself, so read the survivor instead.
archive_entity is not that repair.
It takes an entity out of default reads and leaves its claims where they are.
```

### Teaching `RHYTHM_ARCHIVE_TEACHING`

```text
Archiving a claim does not stop a rhythm's cadence.
The schedule is keys set on the loop — cadence_days, counts_from — and a thing's fields are the writes that set them, the newest write of each key winning, from whichever active record made it.
Archiving one claim takes only that record's own writes out of the running; a schedule key last written on a different, still-active record keeps counting.
To silence it, clear counts_from with update_fact: an overdue read leaves the rhythm out as not_yet_opened and nothing computes a next due date, while the loop itself, its name and its history stay exactly as they were.
Archiving is still the right move when a claim about the rhythm was wrong or is no longer worth keeping — it is simply not what stops the cadence.
```

### Teaching `CHECK_IN_DATE_TEACHING`

```text
A check-in's schedule is dated by recorded_at, never by happened_at — happened_at never reaches the scheduling engine at all.
This check-in named both, for different days: to record a turn that happened on an earlier day, send that day as recorded_at.
```

### Teaching `OPERATOR_QUOTE_TEACHING`

```text
To quote the operator, write the address of the claim that holds their words, `@kind:slug#fN`, and jojobot writes those words into the message beside the address.
The claim must be one they said (testimony); an inference is refused.
Quotation marks you type yourself are left alone and prove nothing.
A record's content is a claim, not a transcript, so a quote by address does not prove verbatim speech.
```

### Teaching `DEPENDS_ON_READ_TEACHING`

```text
This write sets `depends_on`.
Three reads answer from it.
`recall` with view: "ready" and `parent` set to the project lists the work not finished, whose dependencies are all finished and that waits on no one.
It does not check `status`, so narrow it with `where`.
The same view with `subject` set to one piece of work answers ready: false and `blocked_by`, a list of what stops it.
`recall` with `subject` and follow: {"relation": "depends_on", "direction": "in"} lists the work that waits on it.
A `waiting_on` handle also keeps work out of ready.
start_here with skill: "projects" has the examples.
```

### Teaching `session_id_teaching` (with a sample sid and session)

```text
Your run is called a1b2, and the `session` c3d4 in this answer is its stored record.
Cite the run by its name, which is what a later boot offers when it asks you to resume.
Your `sid` is the run's key: it is what every call takes, it is yours alone, and nothing you write should hold it.
```

### Teaching `field_shadows_argument_teaching` (with a sample key and verb)

```text
The field "note" is stored as ordinary data — jojobot never reads a record's fields back into how it treats the claim itself.
It changes nothing: capture's own note argument is what does that.
Call capture again naming note as an argument, not a field, to change it.
```


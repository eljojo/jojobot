# Refusals

The refusals the three error lanes raise, one row per error kind, and the refusal of a status outside the vocabulary rendered from a live write. Refusals written inline at the verb that raises them are not here.

Written by `every_text_a_model_reads_matches_the_snapshot` in `crates/jojobot-mcp/src/snapshot.rs` and compared with it at every `make check`. A commit that changes one of these texts updates this file in the same commit: `make snapshot` rewrites it. What the whole snapshot cannot reach is written at the top of that source file.

## Refusals

One row per error kind in each of the three lanes, built from the sample the lane's own table of kinds holds: the error as it reads, then the answer the caller gets. The sample data is placeholder text. **Not here:** a refusal written inline at the verb that raises it, from data it only has at that moment. Its words appear in this file only where the same words are in a description or an error kind's template.

### Note on a status outside the vocabulary, rendered from a live write

```text
vocabulary (WORK_STATUSES): someday, next, now, waiting, done
status_word_note: 'nonsense' is not one of the words this work holds: someday, next, now, waiting, done. It counts as not finished. One write in `sets` binds it: add it to `columns` on topic:instance (every board), or write `status_word/nonsense` on topic:instance set to the shipped word it means, one of someday, next, now, waiting, done.
```

### Memory refusal `InvalidFact`

```text
reads: invalid fact: empty
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid fact: empty. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `InvalidSubject`

```text
reads: invalid entity id x
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid entity id x. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `WeekOfUsageWritten`

```text
reads: 'usage:bart-2026-w41' is a week of usage, computed from the history of runs when it is read, so nothing is written to it and no such record is made.
Read it with recall: name the handle as the subject, or ask view: usage
answer: {
  "attempted": "usage:bart-2026-w41",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'usage:bart-2026-w41' is a week of usage, computed from the history of runs when it is read, so nothing is written to it and no such record is made. Read it with recall: name the handle as the subject, or ask view: usage.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `InvalidAddress`

```text
reads: invalid fact address 'x': expected kind:slug#local-id, e.g.
person:alpha#f3
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid fact address 'x': expected kind:slug#local-id, e.g. person:alpha#f3. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `InvalidEntity`

```text
reads: invalid entity: x
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid entity: x. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `InvalidEdge`

```text
reads: invalid edge: x
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid edge: x. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `InvalidQuery`

```text
reads: invalid query: x
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid query: x. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `InvalidType`

```text
reads: invalid type: x
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid type: x. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `UnstatedProvenance`

```text
reads: a claim's content cannot be replaced without saying how the new words are known: give 'provenance' — testimony, observation or inference — even to say it is known the same way as before
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: a claim's content cannot be replaced without saying how the new words are known: give 'provenance' — testimony, observation or inference — even to say it is known the same way as before. Nothing is missing from the store and nothing here needs the operator — the call itself is what jojobot cannot carry out. Send the same capture call again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `UnknownEntity`

```text
reads: no entity 'person:lisa'
answer: {
  "attempted": "person:lisa",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. 'person:lisa' is not an entity jojobot knows, and nothing resembles it. capture cannot create an entity: call add_entity to create 'person:lisa' first, then re-call capture.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `UnknownFact`

```text
reads: no fact at 'person:alpha#f9'; that entity has no facts yet
answer: {
  "attempted": "person:alpha#f9",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. 'person:alpha#f9' addresses no fact jojobot holds, and this verb never creates one. That entity holds no facts at all yet, so there is nothing here to edit — capture one first. Recall the entity if none of them is what you meant — every fact comes back carrying the address that edits it.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `NotYours`

```text
reads: 'zz99' is not yours to read
answer: {
  "attempted": "zz99",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was read: 'zz99' is not yours to read. It is there, and it is not yours — sending this again will not change the answer. A session's chronology is its own bot's, and yours is what your own handle reaches.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `AlreadyRetracted`

```text
reads: 'person:alpha#f1' is already archived
answer: {
  "attempted": "person:alpha#f1",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "'person:alpha#f1' is already archived — the record jojobot holds is the one you asked for. This call wrote nothing because there was nothing left to write, and a further attempt would say the same. Archiving is one-way: nothing takes a record back out of it. If that was itself a mistake, capture what is so now as a new record.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `AlreadyArchived`

```text
reads: 'person:alpha' is already archived
answer: {
  "attempted": "person:alpha",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "'person:alpha' is already archived — the entity jojobot holds is the one you asked for. This call wrote nothing because there was nothing left to write, and a further attempt would say the same. Recall it by handle to read why and when it was archived.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `NotArchived`

```text
reads: 'person:alpha' is not archived
answer: {
  "attempted": "person:alpha",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "'person:alpha' is not archived — it is already in every default read. This call wrote nothing because there was nothing to restore, and a further attempt would say the same.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `UnsourcedObservation`

```text
reads: a claim read out of a system of record has to name it: give the field 'read_from' the system that was read — and 'read_ref' what was read there, if you have it — or capture it as an inference instead
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: a claim read out of a system of record has to name it: give the field 'read_from' the system that was read — and 'read_ref' what was read there, if you have it — or capture it as an inference instead. The claim itself is fine — send it again with read_from naming the system, or with provenance inference.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `NotRetractable`

```text
reads: cannot retract 'person:alpha#f1': a rule
answer: {
  "attempted": "person:alpha#f1",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. 'person:alpha#f1' cannot be retracted: a rule.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `TextChangedWhileEditing`

```text
reads: the details of person:alpha#f1 changed while the edit was being applied; it is at revision 2 now
answer: {
  "attempted": "person:alpha#f1",
  "candidates": [],
  "fix_by": "retry",
  "how_to_proceed": "Nothing was written: the details of person:alpha#f1 changed while the edit was being applied; it is at revision 2 now. Read the claim again with recall (history_record: \"person:alpha#f1\") to see what changed, then send the edit again.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `TextEditMatches`

```text
reads: the text to find occurs 2 times in the details of person:alpha#f1, and it has to occur exactly once
answer: {
  "attempted": "person:alpha#f1",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: the text to find occurs 2 times in the details of person:alpha#f1, and it has to occur exactly once. Send a longer `find`, with the words around the place you mean, so it names one place.",
  "matches": 2,
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `StaleRevision`

```text
reads: person:alpha#f1 is at revision 2, and this edit was made against revision 1
answer: {
  "attempted": "person:alpha#f1",
  "candidates": [],
  "fix_by": "retry",
  "how_to_proceed": "Nothing was written: person:alpha#f1 is at revision 2, and this edit was made against revision 1. Read the claim again with recall (history_record: \"person:alpha#f1\") to see what changed, then send the edit again with expected_revision: 2 if it still stands.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `TestimonyRewritten`

```text
reads: person:alpha#f1 is testimony from an earlier session, so its words, its details and its fields are not rewritten in place: archive it with a reason (update_fact, status: archived, details), then capture the corrected claim with derived_from naming it.
The original stays readable: recall the subject with history_record: person:alpha#f1
answer: {
  "attempted": "person:alpha#f1",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: person:alpha#f1 is testimony from an earlier session, so its words, its details and its fields are not rewritten in place: archive it with a reason (update_fact, status: archived, details), then capture the corrected claim with derived_from naming it. The original stays readable: recall the subject with history_record: person:alpha#f1. Nothing is missing from the store and nothing here needs the operator.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `RepeatsShipped`

```text
reads: this text repeats what the software already says here, so storing it would keep today's wording after the software has moved on
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: this text repeats what the software already says here, so storing it would keep today's wording after the software has moved on. Sending this again will not change the answer. Send only the part you are adding — what you write is kept beside what is already there, and a read hands back both.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `ShippedType`

```text
reads: 'trip' is a type that ships with the software, and a shipped type is closed to callers
answer: {
  "attempted": "trip",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'trip' is a type that ships with the software, and a shipped type is closed to callers. A shipped type cannot be extended, shrunk or replaced from here, so sending this call again will not change the answer — changing one is a change to the software. To see the keys 'trip' already has, call recall with answers_type: 'trip'. To declare a type of your own instead, call capture with a different name, and that type is yours to declare and redeclare as you like.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `BreaksFit`

```text
reads: this would leave 'pet' incomplete on this thing: it needs owner
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: this would leave 'pet' incomplete on this thing: it needs owner. This thing answers 'pet' now, and capture would take owner off it, so sending this call again will not change the answer. Leave the key where it is, or write what you meant under a key of your own — adding keys is never refused, and a thing may carry anything beyond what a type asks for.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `BreaksSchedule`

```text
reads: this would leave a rhythm holding 'cadence_days' and no 'due_date', so nothing could say when its next cycle falls due
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: this would leave a rhythm holding 'cadence_days' and no 'due_date', so nothing could say when its next cycle falls due. Send 'due_date' in `sets` in the same call, as a number of days. To switch the schedule off, clear both keys in one call. A loop with no schedule at all is allowed and needs neither.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `ViewNarrowsNothing`

```text
reads: the view 'view:plain-listing' narrows nothing as it is asked
answer: {
  "attempted": "view:plain-listing",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was read: the view 'view:plain-listing' narrows nothing as it is asked. A view narrows by one of these keys: selects, parent, asks: overdue, asks: ready or asks: plan, asks: unreviewed, answers_type, where. A key written `$name` narrows only when its input is sent. Or ask with a subject, kind, parent, answers_type or fields of your own beside the view, which win over it.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `BreaksType`

```text
reads: 'starts' holds date on anything that is a 'trip', and 'soon' is not one
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'starts' holds date on anything that is a 'trip', and 'soon' is not one. This thing is a 'trip' now, so 'starts' has to go on holding date — sending the same value again will not change the answer. Write a value that holds date, or say what you meant under a key of your own: adding keys is never refused, and only the keys a type names are held to what it declared.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `RemapsShippedWord`

```text
reads: 'done' is a shipped status word and means itself, so it cannot be mapped to 'someday'
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'done' is a shipped status word and means itself, so it cannot be mapped to 'someday'. 'done' is one of the five words the software ships and it means itself on every instance, so sending this again will not change the answer. Map a word of your own instead: write `status_word/<your word>` in `sets`, set to the shipped word it should mean, one of someday, next, now, waiting, done.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `ThingTooLarge`

```text
reads: 'thing:gamma' would be 29000 characters as a read serves it before any record on it, and the most one answer can carry of it is 27000
answer: {
  "attempted": "thing:gamma",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'thing:gamma' would be 29000 characters as a read serves it before any record on it, and the most one answer can carry of it is 27000. The thing's own name, aliases and keys are what is over (2000 characters), not the claim you sent, and a read could not serve any claim on it until they are smaller. Shrink them: shorten its name or aliases with update_entity, take keys off with update_fact and clear_fields, move detail to the thing it is about, or keep the long text on a document (put_document) behind a pointer claim on another thing. Then send capture again.",
  "over": 2000,
  "room": 27000,
  "size": 29000,
  "status": "blocked",
  "what_is_over": "the thing's name, aliases and keys",
  "wrote": false
}
```

### Memory refusal `ClaimTooLarge`

```text
reads: this record would be 30000 characters as a read serves it, and the most one answer can carry of a record on 'thing:gamma' is 27000
answer: {
  "attempted": "thing:gamma",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: this record would be 30000 characters as a read serves it, and the most one answer can carry of a record on 'thing:gamma' is 27000. Shorten it (3000 characters over); or split it into claims that each say one thing, which capture can write one after another; or keep the long text on a document (add_entity of kind document, then put_document) and write a pointer claim that names it.",
  "over": 3000,
  "room": 27000,
  "size": 30000,
  "status": "blocked",
  "what_is_over": "the record",
  "wrote": false
}
```

### Memory refusal `BootTooHeavy`

```text
reads: this would leave the boot of 'bot:gamma' with 30000 characters it cannot cut, over the 28000 a boot may carry by 2000
answer: {
  "attempted": "bot:gamma",
  "budget": 28000,
  "candidates": [],
  "fix_by": "change",
  "floor": 30000,
  "floor_parts": [
    {
      "characters": 20000,
      "part": "charter"
    }
  ],
  "how_to_proceed": "Nothing was written: this would leave the boot of 'bot:gamma' with 30000 characters it cannot cut, over the 28000 a boot may carry by 2000. The largest part of the floor is charter (20000 characters); floor_parts lists every part. A star is changed with update_fact on the rule's address and fields {starred: \"true\"} or {starred: \"false\"}; update_entity cannot, because starred stays on its claim. Unstar a rule on bot:gamma that way, shorten the rules that are starred or its charter with set_charter, or have a different identity lower rule_seats on bot:gamma — a bot cannot write rule_seats about itself. A rule that binds at one moment is better carried by a skill than by a seat, and a group of rules that all bind is better carried by one record that stands for the others: write it, mark it with stands_for naming them, keep the others unstarred, and read the boot back.",
  "over": 2000,
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `KeyNotYours`

```text
reads: 'bot:gamma' cannot write reports_to: only 'bot:gamma' itself may write it
answer: {
  "attempted": "bot:gamma",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'bot:gamma' cannot write reports_to: only 'bot:gamma' itself may write it. 'bot:gamma' has to write 'reports_to' itself.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `ChartCycle`

```text
reads: naming 'bot:delta' as the manager of 'bot:gamma' would put 'bot:gamma' under one of its own reports
answer: {
  "attempted": "bot:gamma",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: naming 'bot:delta' as the manager of 'bot:gamma' would put 'bot:gamma' under one of its own reports. 'bot:delta' is at or below 'bot:gamma' in the chart. Name a manager that is not, or move 'bot:delta' out from under 'bot:gamma' first.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `MergeCarriesGuardedKeys`

```text
reads: merging 'bot:gamma' into 'bot:delta', the caller's own bot, would carry rule_seats onto it: only a different identity may raise or lower a ceiling
answer: {
  "attempted": "bot:gamma",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. 'bot:gamma' carries rule_seats, and merging it into 'bot:delta', your own bot, would raise your own ceiling, which only a different identity may do. Ask a different identity to make this merge, or take rule_seats off 'bot:gamma' first: update_fact the record that sets it with clear_fields, then merge again.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `RoleFieldGuarded`

```text
reads: 'holder' is the boot door's own field: claim the dev role there, not here
answer: {
  "attempted": "holder",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'holder' is the boot door's own field: claim the dev role there, not here. Claim the 'dev' role through start_here's own claim argument — that is the only door either of a role's own two fields opens through.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `VoucherIssued`

```text
reads: 'voucher:boot-as-patty' is a voucher: vouchers are issued, not written
answer: {
  "attempted": "voucher:boot-as-patty",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'voucher:boot-as-patty' is a voucher: vouchers are issued, not written. A voucher is a grant the engine issues to a session down the org chart; no record a caller writes is a grant. 'voucher:boot-as-patty' stays as it is, and nothing here creates, edits or takes one back. A session that needs one asks for a code with start_here's ask_for_code and boots with the code the operator types in.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `ChallengeIssued`

```text
reads: 'challenge:abcd' is a challenge: challenges are issued, not written
answer: {
  "attempted": "challenge:abcd",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'challenge:abcd' is a challenge: challenges are issued, not written. A challenge holds the hash of a code jojobot sent to the operator, and only the engine writes it. 'challenge:abcd' stays as it is; a boot asks for a new code with start_here's ask_for_code.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `UnknownRight`

```text
reads: no code knows the right 'fly'
answer: {
  "attempted": "fly",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: no code knows the right 'fly'. A right grants only what code gives it, so a voucher for a word nothing implements is refused. The rights known here are: boot-as.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `RoomFull`

```text
reads: 'bot:gamma's room already holds 3 of 3: name one to drop
answer: {
  "aged_out": 0,
  "ageing": {
    "default": 20,
    "setting": "thought_ages_after_runs"
  },
  "archive_needed": null,
  "attempted": "bot:gamma",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'bot:gamma's room already holds 3 of 3. Re-call capture naming drop (one of the addresses above) and drop_because (why it no longer earns its slot) — or, once and only once, borrow: true to let this one land over the ceiling anyway — or wait, and let this one go unwritten for now.",
  "room": [],
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `MergeOverfillsRoom`

```text
reads: 'bot:gamma's room holds 3 of 3, and this merge brings 1 more thoughts
answer: {
  "archive_needed": 1,
  "attempted": "bot:gamma",
  "fix_by": "change",
  "how_to_proceed": "Nothing was merged: 'bot:gamma's room holds 3 of 3, and this merge brings 1 more thoughts. Archive 1 of the thoughts above with update_fact (status: archived, and details saying why it no longer earns its slot), then re-call capture.",
  "incoming": 1,
  "room": [],
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `ThoughtTooLong`

```text
reads: 'bot:gamma' holds a thought 900 characters long, over its cap of 500: put the substance on the thing it points at, and keep the thought itself short
answer: {
  "attempted": "bot:gamma",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'bot:gamma' holds a thought 900 characters long, over its cap of 500: put the substance on the thing it points at, and keep the thought itself short. Re-call capture with the substance captured onto the thing this thought points at, and the thought as a pointer at or under 500 characters (this one was 900). If you were editing a thought that already stands, archive it with update_fact (status: archived) and capture the pointer as a new claim with the old one as its derived_from; do not rewrite it shorter in place.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `MergeThoughtTooLong`

```text
reads: 'bot:gamma' would hold the thought x, 900 characters long and over its cap of 500
answer: {
  "attempted": "bot:gamma",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was merged: 'bot:gamma' would hold the thought x, 900 characters long and over its cap of 500. Move the substance of x onto the thing it points at with capture, archive x with update_fact (status: archived, details saying where the substance went), and capture a pointer at or under 500 characters (it is 900) with x as its derived_from. Then re-call capture.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `NothingToMerge`

```text
reads: 'person:alpha' cannot be folded into itself: name the other handle
answer: {
  "attempted": "person:alpha",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was merged: 'person:alpha' cannot be folded into itself: name the other handle. Sending this again with the same handle on both sides will not change the answer — name a survivor that differs from 'person:alpha'.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `DocumentMergedWithAThing`

```text
reads: 'document:alpha' is a document and 'thing:handcart' is not: a document merges only into another document
answer: {
  "attempted": "document:alpha",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was merged: 'document:alpha' is a document and 'thing:handcart' is not: a document merges only into another document. Merge two documents to get one that holds both. To bring 'thing:handcart' and 'document:alpha' together anyway, carry over by hand what you want, in whichever direction: read 'document:alpha' with read_document and capture what is wanted as claims on 'thing:handcart', or write what is wanted from 'thing:handcart' into 'document:alpha' with put_document. Then put the one you no longer need away with archive_entity.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `NumberFreed`

```text
reads: 'topic:widgets' numbers its members by 'number', and this write would leave thing:handcart without 2
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'topic:widgets' numbers its members by 'number', and this write would leave thing:handcart without 2. A number names one member for ever, so it stays with thing:handcart while it is a member of topic:widgets, and no token overrides this. Send the number it holds, or change something else. An archived member keeps its number. A number given by mistake is taken back by retracting the claim that gave it, and then the right one can be given. To read the holder, recall topic:widgets/2.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `MembersBreakNumbering`

```text
reads: 'topic:widgets' cannot number its members by 'number': thing:handcart (5), thing:handcart-2 (5)
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: 'topic:widgets' cannot number its members by 'number': thing:handcart (5), thing:handcart-2 (5). topic:widgets would promise that every member holds a positive whole number under number and that no two hold the same one, and the members named already break that. Give each a number nobody else holds (a number given by mistake is taken back by retracting the claim that gave it), or number topic:widgets by a key they do not hold, and then send numbers_by again.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `NothingToRename`

```text
reads: 'person:alpha' names no change: give a different handle, or a parent that differs from the one it already has
answer: {
  "attempted": "person:alpha",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was renamed: 'person:alpha' names no change: give a different handle, or a parent that differs from the one it already has. Sending this again with the same handle, the same parent and nothing else different will not change the answer — name a destination that differs from 'person:alpha', or a parent that differs from the one it already has.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `MergeIntoDescendant`

```text
reads: 'org:springfield-cyclery' sits below 'org:springfield-town-council': 'org:springfield-town-council' cannot be folded into its own descendant
answer: {
  "attempted": "org:springfield-town-council",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was merged: 'org:springfield-cyclery' sits below 'org:springfield-town-council': 'org:springfield-town-council' cannot be folded into its own descendant. To fold them anyway, either call capture the other way, folding 'org:springfield-cyclery' into 'org:springfield-town-council', or move 'org:springfield-cyclery' out from under 'org:springfield-town-council' first with rename_entity and its parent argument, then fold.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `AlreadyMerged`

```text
reads: 'person:alpha' was already folded into 'person:beta': name 'person:beta' instead
answer: {
  "attempted": "person:alpha",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was merged: 'person:alpha' was already folded into 'person:beta': name 'person:beta' instead. Re-call capture naming 'person:beta' instead of 'person:alpha' — that is where it resolves now.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `HandleMoved`

```text
reads: 'person:alpha' was renamed to 'person:beta': use 'person:beta' instead
answer: {
  "attempted": "person:alpha",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was renamed: 'person:alpha' was renamed to 'person:beta': use 'person:beta' instead. Re-call capture with handle: \"person:beta\" — that is what 'person:alpha' answers to now.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `SuppliedHandle`

```text
reads: 'view:colleagues' is a record the build supplies: it exists, and there is no stored record under it to move
answer: {
  "attempted": "view:colleagues",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing changed: 'view:colleagues' is a record the build supplies: it exists, and there is no stored record under it to move. Sending this again will not change the answer — 'view:colleagues' is part of the software rather than something stored that capture could move. Facts and settings written to this record with capture are kept on top of what the build ships; only renaming, archiving or merging the record itself is refused.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `UnreceiptedSettling`

```text
reads: settling an open claim requires a receipt, one of two shapes: `check` (what was checked, written so another agent can run it again) or `confirmed` (the operator confirmed it: the day, and where, a message id or the session).
A receipt records that a check ran or that the operator said so; it does not run the check, and settling never changes who backs the claim.
confirmed_by_user no longer settles a claim: it only moves one to testimony
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: settling an open claim requires a receipt, one of two shapes: `check` (what was checked, written so another agent can run it again) or `confirmed` (the operator confirmed it: the day, and where, a message id or the session). A receipt records that a check ran or that the operator said so; it does not run the check, and settling never changes who backs the claim. confirmed_by_user no longer settles a claim: it only moves one to testimony. The claim is unchanged. Send capture again with `receipt` set to `{check: ...}` or `{confirmed: {on: YYYY-MM-DD, at: ...}}`.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `KindsNeverLoaded`

```text
reads: the kind set was never loaded, so no handle can be read: this process has seeded nothing.
The repair is a boot that reaches the store, not a different handle — nothing about the one you sent is wrong
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "person",
  "how_to_proceed": "Nothing was written: the kind set was never loaded, so no handle can be read: this process has seeded nothing. The repair is a boot that reaches the store, not a different handle — nothing about the one you sent is wrong. The call is not what is wrong, and sending it again will not help: jojobot loaded no kinds when it started, and nothing a caller does re-reads them. This one needs the operator.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `UnconfirmedPromotion`

```text
reads: promoting inference → testimony requires the user's explicit confirmation (confirmed_by_user); jojobot infers freely but never blesses on its own
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "person",
  "how_to_proceed": "Nothing was written: promoting inference → testimony requires the user's explicit confirmation (confirmed_by_user); jojobot infers freely but never blesses on its own. This is not a malformed call and re-sending it will not change the answer — what is missing is the operator's word. Ask, and re-call capture with confirmed_by_user only once they have actually said so.",
  "status": "blocked",
  "wrote": false
}
```

### Memory refusal `RoleTaken`

```text
reads: 'dev' is held by '4nfx' until 1970-01-01T00:00:00Z
answer: {
  "attempted": "dev",
  "fix_by": "retry",
  "holder": "4nfx",
  "how_to_proceed": "Nothing was written: 'dev' is held by '4nfx' until 1970-01-01T00:00:00Z. Wait until 1970-01-01T00:00:00Z, or ask '4nfx' to release 'dev' by wrapping its session.",
  "status": "blocked",
  "until": "1970-01-01T00:00:00Z",
  "wrote": false
}
```

### Memory refusal `Conflict`

```text
reads: this write collided with another that landed the same instant; nothing was written here — the store is working, and retrying the same call is the right response to a transient conflict rather than a mistake in what was sent
answer: protocol error: this call was rejected: another write landed at the same instant, and the store's own concurrency check caught the collision before either could corrupt the other. Nothing was written here, the store is working correctly, and retrying the same call is the right response — this is a transient conflict, not a mistake in what was sent and not something a person needs to look at.
{
  "fix_by": "retry"
}
```

### Memory refusal `Refused`

```text
reads: the store refused this write on a rule it enforces (a key held twice); nothing was written, and sending the same call again will meet the same refusal
answer: protocol error: this call was refused: it breaks a rule jojobot enforces (a key held twice). Nothing was written. Sending the same call again will meet the same refusal, so change the call; the rule in parentheses says what it breaks.
{
  "fix_by": "change"
}
```

### Memory refusal `Store`

```text
reads: store error: connection refused
answer: protocol error: this call could not be completed — jojobot's own storage failed, which is not something your call did wrong and not something you can fix by calling differently. Nothing was written that you should rely on. Try once more; if it fails again, tell the operator, because it needs a person.
{
  "fix_by": "retry"
}
```

### Memory refusal `StoredDamage`

```text
reads: the store holds damage a person must repair: the merge events in the store lead back to where they began: person:alpha, person:beta
answer: protocol error: this call could not be completed — what jojobot stores is damaged: the merge events in the store lead back to where they began: person:alpha, person:beta. The store is working and answered, so sending the same call again will meet the same damage, and nothing in the call changes it. Only a person can repair it, and it is not something you can do from here: tell the operator what is named here.
{
  "fix_by": "person"
}
```

### Mailbox refusal `InvalidName`

```text
reads: invalid mailbox name 'In Box!': names are [a-z0-9-]+, starting and ending alphanumeric
answer: {
  "attempted": "In Box!",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid mailbox name 'In Box!': names are [a-z0-9-]+, starting and ending alphanumeric. Nothing about this needs the operator and nothing is missing from the board — the call itself is what jojobot cannot carry out. Send it again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `InvalidMessageId`

```text
reads: invalid message id 'x': ids are [a-z0-9-]+
answer: {
  "attempted": "x",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid message id 'x': ids are [a-z0-9-]+. Nothing about this needs the operator and nothing is missing from the board — the call itself is what jojobot cannot carry out. Send it again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `InvalidMessage`

```text
reads: invalid message: empty
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid message: empty. Nothing about this needs the operator and nothing is missing from the board — the call itself is what jojobot cannot carry out. Send it again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `UnknownMessage`

```text
reads: no message '9' in any mailbox jojobot manages
answer: {
  "attempted": "9",
  "candidates": [],
  "fix_by": "change",
  "how_to_proceed": "Nothing ran. No message jojobot holds has the id '9', in any mailbox. Ids are minted by jojobot and handed back by search, read_mailbox and post_message — use an id from one of those rather than composing one.",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `NotYourMessage`

```text
reads: message '9' is in the 'gamma' box, not 'otto' — a bot may quarantine only its own mail
answer: {
  "attempted": "9",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. Message '9' is in 'gamma', not 'otto' — a bot may quarantine only its own mail. To reach that box, post_message writes into it without reading it, which is the shape of a request.",
  "mailbox": "gamma",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `NameTaken`

```text
reads: the mailbox name 'gamma' already belongs to bot:gamma, so this rename would collide with it.
Choose a destination whose slug is not already a mailbox name
answer: protocol error: the mailbox name 'gamma' already belongs to bot:gamma, so this rename would collide with it. Choose a destination whose slug is not already a mailbox name
{
  "fix_by": "change"
}
```

### Mailbox refusal `Quarantined`

```text
reads: message '9' cannot be read: unreadable.
jojobot will not act on a record it cannot parse, and no retry will change that — a person has to repair it.
Treat whatever it carried as unhandled and say so
answer: {
  "attempted": "9",
  "fix_by": "person",
  "how_to_proceed": "Nothing ran, and retrying will not help — this is not a missing message. jojobot can see 9 but cannot read it as a message, so no verb will act on it. Repairing it takes a person, and it is not something you can do from here: tell the operator. Until then, treat whatever it was carrying as unhandled and say so rather than reporting it delivered.",
  "reason": "message 9 exists but cannot be read as a record. Something about how it is stored was changed by hand, and only a person can put it back.",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `QuarantinedOnPurpose`

```text
reads: message '9' is quarantined: bot:gamma quarantined it, saying "not for me".
This was a decision, not damage — there is nothing here to repair.
answer: {
  "attempted": "9",
  "fix_by": "person",
  "how_to_proceed": "Nothing ran, and retrying will not help — this is not damage. 'bot:gamma' quarantined 9 on purpose, saying \"not for me\". That is a decision, not a fault, and lifting it is not something this call can do.",
  "quarantine_reason": "not for me",
  "quarantined_by": "bot:gamma",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `OwnerHasMultipleBoxes`

```text
reads: bot:gamma owns 2 mailboxes, not one, so a rename cannot tell which one to follow: gamma, kiln-spare.
This is damage from before this guard existed — a person has to decide which box is the real one before the rename can proceed
answer: protocol error: bot:gamma owns 2 mailboxes, not one, so a rename cannot tell which one to follow: gamma, kiln-spare. This is damage from before this guard existed — a person has to decide which box is the real one before the rename can proceed
{
  "fix_by": "person"
}
```

### Mailbox refusal `KindsNeverLoaded`

```text
reads: the kind set was never loaded, so no handle can be read: this process has seeded nothing.
The repair is a boot that reaches the store, not a different handle — nothing about the one you sent is wrong
answer: {
  "attempted": "",
  "candidates": [],
  "fix_by": "person",
  "how_to_proceed": "Nothing ran: the kind set was never loaded, so no handle can be read: this process has seeded nothing. The repair is a boot that reaches the store, not a different handle — nothing about the one you sent is wrong. The call is not what is wrong, and sending it again will not help: jojobot loaded no kinds when it started, and nothing a caller does re-reads them. This one needs the operator.",
  "status": "blocked",
  "wrote": false
}
```

### Mailbox refusal `Conflict`

```text
reads: this write collided with another that landed the same instant; nothing was written here — the store is working, and retrying the same call is the right response to a transient conflict rather than a mistake in what was sent
answer: protocol error: this call was rejected: another write landed at the same instant, and the store's own concurrency check caught the collision before either could corrupt the other. Nothing was written here, the store is working correctly, and retrying the same call is the right response — this is a transient conflict, not a mistake in what was sent and not something a person needs to look at.
{
  "fix_by": "retry"
}
```

### Mailbox refusal `Refused`

```text
reads: the store refused this write on a rule it enforces (a key held twice); nothing was written, and sending the same call again will meet the same refusal
answer: protocol error: this call was refused: it breaks a rule jojobot enforces (a key held twice). Nothing was written. Sending the same call again will meet the same refusal, so change the call; the rule in parentheses says what it breaks.
{
  "fix_by": "change"
}
```

### Mailbox refusal `Store`

```text
reads: store error: connection refused
answer: protocol error: this call could not be completed — jojobot's own storage failed, which is not something your call did wrong and not something you can fix by calling differently. Nothing was written that you should rely on. Try once more; if it fails again, tell the operator, because it needs a person.
{
  "fix_by": "retry"
}
```

### Session refusal `InvalidId`

```text
reads: invalid session id 'x': ids are [a-z0-9-]+
answer: {
  "attempted": "",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid session id 'x': ids are [a-z0-9-]+. Nothing about this needs the operator and no session is missing — the call itself is what jojobot cannot carry out. Send it again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `InvalidEntry`

```text
reads: invalid entry: empty
answer: {
  "attempted": "",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written: invalid entry: empty. Nothing about this needs the operator and no session is missing — the call itself is what jojobot cannot carry out. Send it again with that fixed.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `UnknownSession`

```text
reads: no session 'zz99': no bot's sessions carry a run with that id
answer: {
  "attempted": "zz99",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. jojobot holds no session with the id 'zz99'. Ids are minted by jojobot and handed back by start_here when you boot as your identity — use the sid it gives you rather than composing one.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `Closed (abandoned)`

```text
reads: session 'zz99' is abandoned — closed, so it takes no entry, no amend and no focus change.
Its chronology stands as the record of what happened.
A wrapped session is the last word; an abandoned one can be picked back up by resuming it
answer: {
  "attempted": "a1b2",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. Session 'a1b2' is abandoned — it stopped without being wrapped up, so it takes no write as it stands. That is not a failure and not the end of it: resume it. Call start_here with your bot name (with no bot it lists them), and either take it from the offer or pass resume with its name, 'a1b2' — it reopens where it left off and its chronology continues.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `Closed (wrapped)`

```text
reads: session 'zz99' is wrapped — closed, so it takes no entry, no amend and no focus change.
Its chronology stands as the record of what happened.
A wrapped session is the last word; an abandoned one can be picked back up by resuming it
answer: {
  "attempted": "a1b2",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. Session 'a1b2' is wrapped — its story has been told, so this end is the last word. Its chronology stands as the record of what happened. If you hold the wrap_code its wrap handed back, and no newer run of your bot has started, call start_here with your bot name and resume set to that code: the run takes one last change and stays wrapped. Otherwise, if there is more to say, it belongs to a new session: boot again (or rotate) and start_here mints one.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `NotWrapped`

```text
reads: session 'zz99' is active, so it has no wrap window: only a wrapped run does
answer: protocol error: session 'zz99' is active, so it has no wrap window: only a wrapped run does
{
  "fix_by": "change"
}
```

### Session refusal `NoEntries`

```text
reads: session 'zz99' has no entry of its own yet, so there is no most-recent one to amend — journal it instead.
Only an entry the session wrote itself can be amended, never a line jojobot wrote about its calls
answer: {
  "attempted": "zz99",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. Session 'zz99' has no entry of its own yet, so there is no most-recent one to amend — journal it instead. Only an entry the session wrote itself can be amended, never a line jojobot wrote about its calls.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `NotABeat`

```text
reads: entry 'e1' on session 'zz99' is not an automatic beat — it is what the session itself recorded, and those are append-only.
Only the most recent entry can be amended, through amend_journal
answer: {
  "attempted": "e1",
  "fix_by": "change",
  "how_to_proceed": "Nothing was written. Entry 'e1' on session 'zz99' is one the session recorded itself, and those are append-only wherever they sit. Only the most recent entry can be amended, through amend_journal.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `KindsNeverLoaded`

```text
reads: the kind set was never loaded, so no handle can be read: this process has seeded nothing.
The repair is a boot that reaches the store, not a different handle — nothing about the one you sent is wrong
answer: {
  "attempted": "",
  "fix_by": "person",
  "how_to_proceed": "Nothing was written: the kind set was never loaded, so no handle can be read: this process has seeded nothing. The repair is a boot that reaches the store, not a different handle — nothing about the one you sent is wrong. The call is not what is wrong, and sending it again will not help: jojobot loaded no kinds when it started, and nothing a caller does re-reads them. This one needs the operator.",
  "status": "blocked",
  "wrote": false
}
```

### Session refusal `Conflict`

```text
reads: this write collided with another that landed the same instant; nothing was written here — the store is working, and retrying the same call is the right response to a transient conflict rather than a mistake in what was sent
answer: protocol error: this call was rejected: another write landed at the same instant, and the store's own concurrency check caught the collision before either could corrupt the other. Nothing was written here, the store is working correctly, and retrying the same call is the right response — this is a transient conflict, not a mistake in what was sent and not something a person needs to look at.
{
  "fix_by": "retry"
}
```

### Session refusal `Refused`

```text
reads: the store refused this write on a rule it enforces (a key held twice); nothing was written, and sending the same call again will meet the same refusal
answer: protocol error: this call was refused: it breaks a rule jojobot enforces (a key held twice). Nothing was written. Sending the same call again will meet the same refusal, so change the call; the rule in parentheses says what it breaks.
{
  "fix_by": "change"
}
```

### Session refusal `Store`

```text
reads: store error: connection refused
answer: protocol error: this call could not be completed — jojobot's own storage failed, which is not something your call did wrong and not something you can fix by calling differently. Nothing was written that you should rely on. Try once more; if it fails again, tell the operator, because it needs a person.
{
  "fix_by": "retry"
}
```

### Refusal `session_unbound` (a call with no sid)

```text
{
  "fix_by": "change",
  "how_to_proceed": "Nothing ran. This call carried no `sid`, and jojobot will not guess which session is asking. Call start_here with your bot name; if you were not told one, call start_here with no bot, which boots nobody and lists the bots you could boot as. A bot with a run in flight is handed a choice and no key: call start_here again with resume set to the name of the run you are picking up, or to \"new\" for a fresh session. Then pass the handle on every call — reads included. It is the only address, and it is what tells jojobot which bot is asking: most clients open a fresh connection per tool call, so nothing about who you are survives from your last one. IF START_HERE ALSO HANDS BACK NO HANDLE, this is not a call you can fix: the session world is unreachable, so no identity can be issued and no write can be attributed until it is back. Tell the operator — nothing is lost and nothing ran.",
  "status": "blocked",
  "wrote": false
}
```


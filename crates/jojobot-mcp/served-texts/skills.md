# Skills

Every skill the build ships: the index line a boot lists and the body a call fetches.

Written by `every_text_a_model_reads_matches_the_snapshot` in `crates/jojobot-mcp/src/snapshot.rs` and compared with it at every `make check`. A commit that changes one of these texts updates this file in the same commit: `make snapshot` rewrites it. What the whole snapshot cannot reach is written at the top of that source file.

## Skills

### Skill `recommend` — index line (when to use)

```text
Before you give the operator a real-world recommendation that they will act on.
This includes where to eat, where to go, what to buy, and which product, place or service to choose.
```

### Skill `recommend` — body

```markdown
# recommend

Do not say that something is the best choice unless you have read its own
source in this turn. Memory is not a source. A search-result summary is not
a source.

## Procedure

1. Apply the operator's stated preferences first. Reject a candidate that
   breaks one. Do not look it up. No source makes it acceptable. Read the
   operator's recorded preferences if you are not sure. Do not override an
   explicit refusal or an explicit request.

2. Read each remaining candidate's own source in this turn. Its own source is
   its review consensus, its own menu, or the seller's own catalog. A review
   consensus is one rating with a high count, or two or more independent
   sources that agree. An aggregator page, a "top ten" page and a web-search
   result are not its own source. Do not give the operator the name of a
   candidate whose own source you have not read.

3. Give options with their sources. For each candidate, give the name, the
   signal from its own source, and the link. The operator chooses. You may
   name your preference only if its source is on the same line.

4. If you cannot read a source, say so. Say that you have not checked, and
   give the link. The operator can then check it before they act.

## Limit

This procedure is a behaviour. No mechanism enforces it. It removes the step
that fails, and it puts the source in the operator's hands each time. The
operator can then find a bad recommendation before they act on it.
```

### Skill `rhythms` — index line (when to use)

```text
When you are about to record a check-in on a loop, or the operator asks to look ahead or back.
```

### Skill `rhythms` — body

```markdown
# rhythms

A rhythm is a recurring loop. You offer it. The operator decides.

A rhythm is an entity of kind `rhythm`. Its parent says whose job the loop is.
A maintenance loop sits under the thing maintained. A review loop sits under
the bot that carries it. Two loops on one object are two rhythms.

## What a rhythm holds

`cadence_days` is how long one cycle lasts. A cadence is always time. What the
check measures — a reading, a distance, a count — belongs on the check-in and
is never a unit of the schedule.

`advances_from` says which date the next cycle counts from when a check-in is
late. It takes `due_date`, the day the cycle fell due, or `check_in_date`, the
day the check-in happened. It has no default. Whose loop it is says who picks
it, and the rhythm's `parent` says whose loop it is. On a loop in the
operator's own life, the operator picks, for each rhythm. A loop under the bot
that carries it is that bot's own work, and that bot picks. When the operator
hands the pick over, you pick: write both keys with `capture` on the rhythm,
with `provenance: "inference"`, and say in `details` that the operator handed
the pick over. A wrong pick is silent: every late check-in re-arms the loop it
was meant to settle.

`counts_from` is the date this cycle counts from. jojobot writes it. The
rhythm falls due `cadence_days` after it.

Write the loop with `add_entity` as soon as the operator names one, and put in
the keys the operator gave you. `cadence_days` and `advances_from` go together
or not at all. A write that holds one without the other is refused. A loop with
neither is allowed, and you write it at once.

If the loop is in the operator's own life and the operator gave a cadence
without saying which date it counts from, ask which. Write the loop without `cadence_days` while you wait. When the operator
answers, send both keys in one call. The loop itself is never a reason to wait.

A loop with neither key has no schedule, and it is not late. A loop nobody
wrote tracks nothing, and nothing later reports that it is absent.

A loop with both keys falls due only once a check-in opens it. A loop that has
never been checked in on is not late. Check in on it, dated the day it last
happened, and the check-in opens it.

## How to find what is due

Call `recall` with `kind: "rhythm"` and `overdue: {}`. You get the loops that
have gone quiet as of the run's day: the day it stated, else today in its zone. Put
a date in `as_of` to ask about another day.

The answer says which day it used. Read it.

A rhythm that holds only part of a schedule comes back overdue, with the
fields it does hold. Get the key it lacks from whoever picks it, as above. Do
not guess one.

## Something that has to be done by a day

A promise is not a loop. It is something somebody has to do by a day: a thing
borrowed that has to go back, a reply owed. It is an entity of kind `promise`.
Its parent says whose job it is to keep. Write it with `add_entity` as soon as
the operator names one.

`promised_by` is the day. `regarding` is the thing it is about, when there is
one. Two promises can be about one thing, and each keeps its own events.

A promise ends when you record how: `ended` takes `delivered`, `withdrawn` or
`overtaken`. Capture it on the promise, with the day it happened. An ended
promise is no longer owed, and the record still says which of the three it was.
Being shown in the owed answer ends nothing.

To see what is owed, loops and promises together, call `recall` with
`fields: [{"key": "due_on"}]` and `overdue: {}`. Add `kind: "promise"` to see
the promises alone.

`due_on` is the key you ask by, and jojobot sets it. Do not write it and do not
clear it: a write that sends it is refused. Write the day on the thing that
carries it (`promised_by` on a promise, `runs_out`, `decide_by`, or a loop's
cadence), and jojobot keeps `due_on` current. To move a day, change that key.
To take it away, clear that key.

## Keep the pressure low

Offer a rhythm in one line at the start of a session. Then do the work the
operator opened the session for. If the operator does not take the offer,
stop. Do not offer the same rhythm again in the same window.

Remove a rhythm that the operator finds stressful. Remove it with the
operator. Add a new rhythm with the operator. Do not add one alone.

## How to close one

Call `capture` on the rhythm and pass `check_in`. It takes one of three words,
and the difference between them is whether the cycle is consumed.

`ran` — it happened, and the cycle advances.

`skipped` — it did not happen, and the cycle advances anyway. The record says
that it did not happen.

`snoozed` — the cycle is not consumed, and the snooze names the day the loop
comes back. Send that day in `fields` as `snoozed_until`, a date after the
check-in's own. A snooze with no day is refused, so ask the operator which day
if they have not said. A snoozed loop falls due on the later of its own due day
and the snooze day: it is not due before that day, and it is due on it.

A refusal is not a run. Ask the operator whether the cycle moves on, which is
`skipped`, or comes back, which is `snoozed`. A refusal recorded as `ran` says
the work was done, and nothing later can tell it from work that was done.

jojobot writes `outcome`, `last_check_in` and `counts_from` itself. Do not
compute them and do not send them. Put what the check measured in `fields`.

A loop whose last run already happened, before this session opened it, is
opened the same way: capture a check-in on it, dated the day it last ran, and
jojobot works the rest of the schedule out from there. Give the loop its
cadence first — `cadence_days` and `advances_from` are set by whoever picks
them, as above, and no check-in can state them, so a loop that holds neither is
refused until it does. A snooze does not open a loop, because it moves nothing and there is
nothing yet to leave where it was.

## How to run a rhythm

Read what is due. Offer each rhythm that is due, in one line. Record a
check-in when the operator runs it, and when the operator turns it down.

## The two shapes

A forward rhythm gives a short summary of the period ahead. State what is
fixed, what conflicts, and the one or two decisions the operator must make.
Do not create work. Do not add a date that the operator did not give.

A backward rhythm reviews the period that ended. State what got attention,
what stopped, and what did not start. It is a conversation. Do not produce a
write-up, a count or a dashboard.

## What belongs to the operator

Which rhythms exist, how often each one runs, and when each one last ran.
On a loop in the operator's own life, these are the operator's decisions and
the operator's data. On a loop under the bot that carries it, how often it
runs (`cadence_days`) is that bot's pick, by the same rule as `advances_from`.
This procedure is only how you offer a rhythm and how you close it.
```

### Skill `asking` — index line (when to use)

```text
When you need something out of the graph and are about to work out how to ask for it.
Also when a capability seems to be missing, because it usually is not.
```

### Skill `asking` — body

```markdown
# asking

**Getting something out of the graph, without working the shape out first.**

## 1. Look for a view before you build a question

A **view** is a question somebody already worked out, asked for by name. Ask
`recall` with `view` and the name:

    recall  view: "colleagues"     the identities here, what each is for, and whom each reports to
    recall  view: "loops"          the recurring things, and what each last recorded

The colleagues view says whom each bot reports to and nothing inward. Who
reports to a bot is a second call, a walk along `reports_to` the other way:

    recall  subject: "bot:omega"  follow: {"relation": "reports_to", "direction": "in"}

**Some ship with the software and you can declare your own.** A name that is no
view comes back blocked and names the ones that are — so **guessing a name is
how you find out what is here**, and it costs one call.

Declare your own with the surface you already know: `add_entity` of kind `view`,
then the keys.

    selects   what it looks at — a kind. Required.
    shows     what of each comes back: facts, prose, charter.
    asks      overdue, when the question is what has fallen due.

Anything you send beside the name wins, so a view is a starting point rather
than a cage: `recall  view: "loops"  facts: true` is the shipped question with
one thing changed.

## 2. 🚨 A capability arrives as an ARGUMENT on a verb you already know

**Never as a new verb.** This surface grows by widening what exists, so when you
go looking for a capability and find no verb named for it, **that is not an
answer** — read the arguments of the verb whose job it is.

Views are an argument on `recall`. So are walking a relation, asking what has
fallen due, and reading one key's history. **Mail is an argument on `search`.**
The list of verbs is short on purpose and it is not the list of what jojobot can
do.

## 3. When no view fits, say the shape

`recall` takes three axes and they combine into ONE question:

- **which objects** — `subject` (one handle), `kind`, `answers_type`, or
  `fields` (a key and its value)
- **what of each** — `facts`, `prose`, `charter`, `fields`, `history`
- **where to walk** — `follow`, which takes an edge or a key that holds a
  handle, in either direction

*Which people are in Springfield* is a `kind` plus a `follow` on the location
edge. *Which visits cost more than fifty* is a key filter on the record.
*Which friends have eaten three donuts* is a key filter on the THING — the same
key, folded. Those two are different questions and the difference is whose value
you are asking about.

**Selection chooses; traversal reaches.** If you find yourself walking to narrow
something down, you wanted a filter.

## 4. Use `search` when you only have words

`recall` is for when you can describe the shape. `search` is for when you are
looking for something and have words rather than a shape — one ranked list over
entities, facts and prose, and over mail too when you ask for it with
`include_mail: true`.

## 5. If you asked and got nothing

Read the answer rather than re-sending it. A blocked answer says what to do
next and names what exists. An empty result is an answer, but it does not
always mean nobody wrote it down: words are matched by stem and a query needs
most of its terms, so the wording can be what missed. A `recall` that selected
nothing carries a `searched` line saying what it looked through, what it left
out and how to widen it; a `search` carries a `matching` note saying how loosely
it matched. Read those before you conclude anything. **They are different from
a blocked answer**, and neither is fixed by asking again unchanged.

**A person with no entity of their own.** Somebody the operator mentions may
have none, so there is no handle to recall. Their name can still be in what was
written about other things: `search` for the name as words and read the
surroundings of each hit. A `recall` of a guessed handle that names nothing
comes back blocked with the nearest handles, which shows whether an entity exists
under another spelling. If both come back empty and nothing was left out, jojobot
holds no record of them: say so, and ask the operator rather than adding an entity
on a guess.
```

### Skill `projects` — index line (when to use)

```text
When you are recording a project's work, its decisions or its open questions.
```

### Skill `projects` — body

```markdown
# projects

**A project's work is dates that move, decisions made in a room, questions
nobody has asked yet, and things you worked out from what was said.** Each has
one route the engine accepts. Each also has a convenient wrong place: a
sentence, which a later question cannot read.

## A date that moves

A slip is a new due day. Write the day under the same key every time it
changes. Do not write "the copy moved again" and do not keep a count.

    capture  subject: "project:atlas"  fields: {"copy_due": "2026-04-06"}

"How many times did it move" is then a read of that key's history:

    recall  subject: "project:atlas"  history: "copy_due"

The answer holds every write of the key, oldest first, with the day each was
written. The count of the writes is the answer.

## A decision

Write the operator's own words as the operator's word: `provenance: "testimony"`.
Do not rephrase them. Then write each option that was weighed as a claim of
its own, say whose option it was by writing `@` and their handle in the words, and say
which one was turned down. A decision without the rejected option cannot be
told from one nobody weighed.

## What you worked out from what somebody said

This is two records. The first is their words, as testimony. The second is your
conclusion, with `provenance: "inference"` and `derived_from` set to the
address of the first. Never write your conclusion as the operator's word.

    recall  built_on: "<address of their words>"

lists what was worked out from them. When the words turn out to be misread,
that is the list to check.

## A pause until a day, on something with no loop

Nothing falls due on a thing that has no loop. A pause on it is a promise
regarding it: a `promise` entity with `promised_by` the day and `regarding`
the thing. See the `rhythms` skill for what is owed and how to read it.

## An open question

A question nobody has asked yet is a piece of work of its own. Make it with
`add_entity`, kind `work`, and file it under its project with `parent`. Write
`state` on the question as `drafted`, `asked` or `answered`, and `blocks` as
the handle of what waits on it. Write the same key again on the question when
the state changes. A question carries no `status`: one that is drafted and
deliberately not asked is not next work, so it stays out of the status read.
Each question folds its own `state`, so a later read for the questions that are
drafted and never asked is a read of that key, and it finds every one:

    add_entity  kind: "work"  handle: "phi"  name: "Payment provider"  parent: "project:visa"  source: "drafted in conversation"
    capture  subject: "work:phi"  content: "which payment provider do we use?"  fields: {"state": "drafted", "blocks": "work:handcart"}
    recall  fields: [{"key": "state", "value": "drafted"}]  facts: true

A question is a `work` thing because no kind for questions is built in: a
question is a piece of work, and it blocks other work.

## Where a piece of work stands

A piece of work is a `work` thing filed under its project. Four keys say where
it stands. Write them on the work as keys. A sentence cannot answer "what is
next". A claim that says "now" or "next" is chronology, and the next write of
`status` outdates it, so read where the work stands from the key, never from a
sentence.

`status` is where the work stands. It takes one of five words: `someday`,
`next`, `now`, `waiting` or `done`. A word outside the five is refused, and the
refusal names them. `owner` is the handle of the person who does the work.
`waiting_on` is the handle of the person whose move it is. When that person is
the operator, their handle is the one the boot names on its operator line.
`depends_on` lists
the handles of the work that must come first, separated by commas. A work item
or a project at `done` is not owed, and its dates stay on it, so do not clear a
date to quiet finished work.

    capture  subject: "work:sigma"  fields: {"status": "waiting", "waiting_on": "person:lisa", "depends_on": "work:phi, work:first-mix"}

A project may add statuses of its own. Write the whole list on the project, in
the order you want it, under `columns`. The work filed under that project may
then take any word on the list. A project that is not filed under a project
takes its own list for its own status. A project filed under a project keeps
that project's list, and a list of its own does not widen it. The list always
keeps the shipped words. A word outside the list is refused, and the refusal
names the list and the project that holds it.

    capture  subject: "project:atlas"  fields: {"columns": "inbox, someday, next, now, waiting, done"}

Each question is then a read of one key. The first read lists what is next. The
second lists what waits on someone, and on whom. The third lists what stands on
the work named in `subject`.

    recall  kind: "work"  fields: [{"key": "status", "value": "next"}]
    recall  kind: "work"  fields: [{"key": "waiting_on"}]
    recall  subject: "work:phi"  follow: {"relation": "depends_on", "direction": "in"}

## A label on one claim

A key you write on a claim becomes a field of the claim's subject. A label that
belongs to one claim, such as a ticket number or a cross-reference, is not a
key: put it in the claim's words or in its `details`. Six keys describe their own
record and do not fold onto the subject: `read_from`, `read_ref`, `starred`,
`subject`, `purpose` and `recorded_by`. Any other key folds onto the subject, as
every field does.

## A retrospective

A retrospective is a history read. A long read is cut: a key's history comes
back cut to its newest writes, and says how many it left out. Raise
`history_most` before you conclude anything from it.

    recall  subject: "project:atlas"  history: "health"  history_most: 100

## An answer the operator asked to have kept

Write it down. An answer you gave in the conversation is gone for the next
session. When the operator asks for an answer under a name, write it under
that name, on what it is about, so a later session reads it from the record.
```

### Skill `evidence` — index line (when to use)

```text
Before you write anything that the operator will read later.
This includes a claim about a person, a summary, a portrait, and a note.
```

### Skill `evidence` — body

```markdown
# evidence

Each claim you write has two properties, and a later reader needs both.

The first property is who backs the claim. The operator told you, or you
worked it out.

The second property is how settled the claim is. It is settled, or it is
still open. These properties are independent. The operator can tell you
something and say that they are not sure of it. That claim is theirs, and it
is still open. Do not record the operator's own doubt as your guess.

## Mark a claim you worked out

If you cannot point to the operator's words, you worked the claim out. Mark
it. You are rewarded for confident structure, and this is why an unmarked
guess is a risk: the next session reads it with the authority of a statement
the operator made.

Marking who worked it out is not the same as marking how settled it is. A
claim you worked out starts open. A claim the operator states and hedges is
also open, and it is still theirs.

## What another bot told you

What another bot instructed or decided within its own remit you record as an
observation, with `read_from` set to its handle and `read_ref` to the message
id: it reads back settled and never names the operator, but a world fact the
bot only reported is not covered, and stays a claim you worked out until you
read it at its own source.

## Four rules

1. Only the operator settles a claim. Write claims that you worked out.
   Do not change one to a claim the operator made. Only the operator
   confirms.

2. Do not put both kinds in one list. A confirmed item makes an unconfirmed
   item beside it look confirmed. Keep them in separate groups. Do not put
   them under one heading that asserts them together.

3. Record a correction. When the operator says that a claim you worked out is
   wrong, write the correction down and date it. Subtract it before you write
   the same kind of claim again. If you do not, the next pass repeats the
   error.

4. A check is not a write. Review your own over-claims in a separate pass.
   A check inside the pass that produced the claim does not work.

## This also binds what you say

Do not state a claim you worked out as a fact about a person when you speak
to the operator. If you worked it out, ask.

## The error to watch for

A weak relation written as a strong one. Two things that appear together
become closeness. One thing that follows another becomes cause. Membership
of a set becomes meaning. Each one is a reasonable step and none of them is
a fact.
```


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
   result are not its own source. Do not recommend a candidate whose own
   source you have not read. Step 4 says how to name one as unchecked.

3. Give options with their sources. For each candidate, give the name, the
   signal from its own source, and the link. The operator chooses. You may
   name your preference only if its source is on the same line.

4. If you cannot read a source, name that candidate only as unchecked. Say
   that you have not checked it, and give the link. The operator can then check
   it before they act.

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

A loop's keys are properties of the loop, so you write them in `sets`. A key sent
in `fields` stays on the claim, and the loop never holds it.

`cadence_days` is how long one cycle lasts. A cadence is always time. What the
check measures — a reading, a distance, a count — belongs on the check-in and
is never a unit of the schedule.

`advances_from` says which date the next cycle counts from when a check-in is
late. It takes `due_date`, the day the cycle fell due, or `check_in_date`, the
day the check-in happened. It has no default. Whose loop it is says who picks
it, and the rhythm's `parent` says whose loop it is. On a loop in the
operator's own life, the operator picks, for each rhythm. A loop under the bot
that carries it is that bot's own work, and that bot picks. When the operator
hands the pick over, you pick: write both keys in `sets` with `capture` on the rhythm,
with `provenance: "inference"`, and say in `details` that the operator handed
the pick over. A wrong pick is silent: every late check-in re-arms the loop it
was meant to settle.

`counts_from` is the date this cycle counts from. jojobot writes it. The
rhythm falls due `cadence_days` after it.

Write the loop with `add_entity` as soon as the operator names one, and put the
keys the operator gave you in `sets`. `cadence_days` and `advances_from` go together
or not at all. A write that holds one without the other is refused. A loop with
neither is allowed, and you write it at once.

If the loop is in the operator's own life and the operator gave a cadence
without saying which date it counts from, ask which. Write the loop without `cadence_days` while you wait. When the operator
answers, send both keys in `sets` in one call. The loop itself is never a reason to wait.

A loop with neither key has no schedule, and it is not late. A loop nobody
wrote tracks nothing, and nothing later reports that it is absent.

A loop with both keys falls due only once a check-in opens it. A loop that has
never been checked in on is not late. Check in on it, with `recorded_at` set to
the day it last happened, and the check-in opens it.

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
`overtaken`. Capture it on the promise with `ended` in `sets`, with the day it happened. An ended
promise is no longer owed, and the record still says which of the three it was.
Being shown in the owed answer ends nothing.

To see what is owed, loops and promises together, call `recall` with
`fields: [{"key": "due_on"}]` and `overdue: {}`. Add `kind: "promise"` to see
the promises alone.

`due_on` is the key you ask by, and jojobot sets it. Do not write it and do not
clear it: a write that sends it is refused. Write the day in `sets` on the thing that
carries it (`promised_by` on a promise, `runs_out`, `decide_by`, or a loop's
cadence), and jojobot keeps `due_on` current. To move a day, change that key.
To take `runs_out` or `decide_by` away, clear that key. A promise is the exception: it
cannot lose `promised_by`, and a write that takes it off is refused. End a
promise with `ended` instead.

## Keep the pressure low

Offer a rhythm in one line at the start of a session. Then do the work the
operator opened the session for. If the operator does not take the offer,
stop. Do not offer the same rhythm again in the same window.

Archive a rhythm that the operator finds stressful, with `archive_entity` and the
reason. Archive it with the operator. Add a new rhythm with the operator. Do not
add one alone.

## How to close one

Call `capture` on the rhythm and pass `check_in`. It takes one of three words,
and the difference between them is whether the cycle is consumed.

`ran` — it happened, and the cycle advances.

`skipped` — it did not happen, and the cycle advances anyway. The record says
that it did not happen.

`snoozed` — the cycle is not consumed, and the snooze names the day the loop
comes back. Send that day in `sets` as `snoozed_until`, a date after the
check-in's own. A snooze with no day is refused, so ask the operator which day
if they have not said. A snoozed loop falls due on the later of its own due day
and the snooze day: it is not due before that day, and it is due on it.

A refusal is not a run. Ask the operator whether the cycle moves on, which is
`skipped`, or comes back, which is `snoozed`. A refusal recorded as `ran` says
the work was done, and nothing later can tell it from work that was done.

jojobot writes `outcome`, `last_check_in` and `counts_from` itself. Do not
compute them and do not send them. Put what the check measured in `fields`.

A loop whose last run already happened, before this session opened it, is
opened the same way: capture a check-in on it, with `recorded_at` set to the day
it last ran, and jojobot works the rest of the schedule out from there. Give the loop its
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
    recall  view: "ready"          the work not finished, unblocked and waiting on no one, and how many it left out
    recall  view: "plan"           the work not finished, ready or blocked, each with what stops it
    recall  view: "due"            what falls due by a day, and what is late
    recall  view: "board"          one project's work grouped by status
    recall  view: "of-type"        the things answering a declared type, each with the keys it lacks

Some views take a value in `inputs`: due takes the day `by` (a week ahead when
you send none), board the `project` it is about (required), of-type the `type`.
Recall `kind: "view"` and read each one's `inputs`, or ask without the value and
read the refusal, which names every input.

The colleagues view says whom each bot reports to and nothing inward. Who
reports to a bot is a second call, a walk along `reports_to` the other way:

    recall  subject: "bot:omega"  follow: {"relation": "reports_to", "direction": "in"}

**Some ship with the software and you can declare your own.** A name that is no
view comes back blocked and names the ones that are — so **guessing a name is
how you find out what is here**, and it costs one call.

Declare your own with the surface you already know: `add_entity` of kind `view`,
then the keys.

    selects   what it looks at — a kind. A view needs something that narrows it:
              a kind, an ask (overdue, ready or plan), a parent, answers_type or a
              filter on keys.
    shows     what of each comes back: facts, prose, charter.
    asks      overdue, when the question is what has fallen due; ready, when
              it is which work can be started.
    where     which of them, as one line over keys: status != done && owner = bot:gamma
    order     a key, then ascending or descending.
    scope     what `where` is asked of: thing (the default) or record.
    inputs    what a call fills in, as `name: type` separated by commas: `project: kind project`,
              `by: date?`, `type: text`. A `?` after the type leaves it optional.
    parent    the project or container whose children it lists: `$project` takes the input.
    as_of     the day an overdue question is asked about: `$by` takes the input.
    answers_type
              the declared type whose things it finds: `$type` takes the input.
    shows_keys
              the keys each object comes back with.

A view that narrows nothing as it is asked is refused, naming the view and the
keys that would narrow it: `selects`, `parent`, `asks: overdue`, `asks: ready`
or `asks: plan`, `answers_type` and `where`. A kind, subject, parent,
answers_type or fields sent beside the view count. A key written `$name`
narrows only when its input is sent. A write that leaves a view holding none of
those still lands, with a note saying so, and a view made with no keys is filled
in a key at a time.

A `$name` stands for the input of that name in `parent`, `where`, `as_of` and
`answers_type`. A view that takes a project is declared and asked like this:

    add_entity  kind: "view"  handle: "urgent-things"  source: "user-named"
                sets: {"selects": "work", "inputs": "project: kind project", "parent": "$project"}
    recall      view: "urgent-things"  inputs: {"project": "project:atlas"}

Anything you send beside the name wins, so a view is a starting point rather
than a cage: `recall  view: "colleagues"  charter: true` is the shipped question
with one thing added, and each charter comes back with it.

## 2. 🚨 A capability arrives as an ARGUMENT on a verb you already know

**Rarely as a new verb — the document verbs are the exception, because a
document is read and written whole.** This surface grows by widening what
exists, so when you go looking for a capability and find no verb named for it,
**that is not an answer** — read the arguments of the verb whose job it is.

Views are an argument on `recall`. So are walking a relation, asking what has
fallen due, and reading one key's history. **Mail is an argument on `search`.**
The list of verbs is short on purpose and it is not the list of what jojobot can
do.

## 3. When no view fits, say the shape

`recall` takes three axes and they combine into ONE question:

- **which objects** — `subject` (one handle), `kind`, `answers_type`,
  `fields` (a key and its value), or `where`: one line over keys. `order`
  puts them in order.

      recall  kind: "work"  where: "status != done && due <= today+7d"  order: {"key": "due"}

  It takes `=`, `!=`, `<`, `<=`, `>`, `>=`, `in (a, b)`, `contains`, `&&`, `||`,
  `!` and parentheses. `status != done` keeps the things holding another
  status, and `!(status = done)` also keeps the things holding none.
- **what of each** — `facts`, `prose`, `charter`, `keys` (which of each object's
  fields come back) and `history`
- **where to walk** — `follow`, which takes an edge or a key that holds a
  handle, in either direction

*Which people are in Springfield* is a walk inward from the place:

    recall  subject: "place:springfield"  follow: {"shape": "location", "direction": "in"}

`follow` reaches out from what you selected and does not narrow it, so a `kind`
of person plus a `follow` lists every person.

*Which visits cost more than fifty* is a key filter on the record. `scope:
record` asks about the occasions, and `compare` orders the value: `greater` and
`less` need the key declared a number, `before` and `after` a date, and
`equals` needs nothing.

    declare_type  name: "visit"  fields: [{"key": "cost", "holds": "number"}]
    recall  fields: [{"key": "cost", "compare": "greater", "value": "50", "scope": "record"}]

*Which friends have eaten three donuts* is the same filter with the default
`scope`, which asks the THING — the value the key holds now. The two are
different questions and the difference is whose value you are asking about.

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

    capture  subject: "project:atlas"  sets: {"copy_due": "2026-04-06"}

"How many times did it move" is then a read of that key's history:

    recall  subject: "project:atlas"  history: "copy_due"

The answer holds every write of the key, oldest first, with the day each was
written. The count of the writes is the answer.

## A decision

A decision on a thread is its own child of the thread, of kind `decision`.
Make it with `add_entity`, `parent` the thread, then write the operator's own
words on it as the operator's word: `provenance: "testimony"`. Do not rephrase
them. Reading the thread then puts its standing decisions first, apart from the
research gathered around it, so a session that reads the thread cold learns what
was chosen before what was found.

    add_entity  kind: "decision"  handle: "oak-cabinets"  parent: "thread:kitchen"
    capture     subject: "decision:oak-cabinets"  provenance: "testimony"

Say in the words which option was turned down, and whose each option was by
writing `@` and their handle. A decision without the rejected option cannot be
told from one nobody weighed.

A decision that changes is never reworded. Make a new decision child, write its
words with `derived_from` set to the address of the old decision's record, and
archive the old child with `archive_entity` and the reason. The thread then
leads with the new one and names the archived one, which still answers by its
own handle.

A decision that sits on something other than a thread, such as a project, is
a claim of its own on that thing: the operator's words as testimony, and one
claim for each option that was weighed.

## What you worked out from what somebody said

This is two records. The first is their words, as testimony. The second is your
conclusion, with `provenance: "inference"` and `derived_from` set to the
address of the first. Never write your conclusion as the operator's word.

    recall  built_on: "<address of their words>"

lists what was worked out from them. When the words turn out to be misread,
that is the list to check.

## A checklist on a piece of work

A checklist is a `list` hung on the work, one claim per line, kept in the order
you place them. A handle is one name across the whole instance, so name the list
for the work it hangs on. Make the list once, under the work it is about:

    add_entity  kind: "list"  handle: "phi-checklist"  name: "Checklist"  parent: "work:phi"  source: "user-named"  sets: {"tick": "sink"}

Then capture each step on the list. It goes at the end, or after the step you
name by its address. The list's own `tick` setting is its first claim, so the
first step is `#f2`, and every capture answers with the address of its step:

    capture  subject: "list:phi-checklist"  content: "book the room"  provenance: "testimony"
    capture  subject: "list:phi-checklist"  content: "send the invites"  after: "list:phi-checklist#f2"

`after: "first"` puts a step at the front. `recall` the list to read its open
steps in order, under `entries`. A step may carry an edge to a piece of work, and
the link reads back beside it.

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
    capture  subject: "work:phi"  content: "which payment provider do we use?"  sets: {"state": "drafted", "blocks": "work:handcart"}
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
`next`, `now`, `waiting` or `done`. A word outside the five lands, counts as not
finished, and the answer names the words that govern and the write that binds
it. `owner` is the handle of the person who does the work.
`waiting_on` is the handle of the person whose move it is. When that person is
the operator, their handle is the `operator` the boot names.
`depends_on` lists
the handles of the work that must come first, separated by commas. A work item
or a project at `done` is not owed, and its dates stay on it, so do not clear a
date to quiet finished work.

    capture  subject: "work:sigma"  sets: {"status": "waiting", "waiting_on": "person:lisa", "depends_on": "work:phi, work:first-mix"}

Ask the board a question over those keys with `where`, in a recall. The ready
view lists work that is unblocked and not finished, so work already under way
is in it. Narrow it to the work not started, which is your board's not-started
status (`next` on the shipped words):

    recall  view: "ready"  where: "status = next"

Name a project as `parent` to see the ready work of that project only:

    recall  view: "ready"  parent: "project:atlas"  where: "status = next"

See a whole plan in one call, every piece of work under a project that is not
finished, whether or not it can start:

    recall  view: "plan"  parent: "project:atlas"

Each piece comes back with `ready`, and when it is not ready with `blocked_by`,
so the blockers of the whole plan are in one answer. Work that is finished or
archived is left out and counted. To read one piece alone, name it:

    recall  view: "ready"  subject: "work:first-mix"

A piece that is not ready says `ready: false` and `blocked_by`, a list of what
stops it. A blocker is a `dependency` that is `unfinished`, `archived` or `dangling` (its
handle names nothing), a `waiting_on` handle, or the work itself being
`finished` or `archived`. Archived work is never ready, however it is named.
A `waiting_on` handle keeps work out of the view for as long as the key holds it.
To wait on an event, which is not a handle, name in `waiting_on` the person who
owns that move, and change the key when the move is made.

Or ask what waits on one person:

    recall  kind: "work"  where: "status = waiting && waiting_on = person:lisa"

A project may add statuses of its own. Write the whole list on the project, in
the order you want it, under `columns`. The list has to hold all five shipped
words, `someday`, `next`, `now`, `waiting` and `done`, in any order. A list that
drops one is refused. The work filed under that project may then take any word on
the list. A project that is not filed under a project
takes its own list for its own status. A project filed under a project keeps
that project's list, and a list of its own does not widen it. A word outside the
list lands, counts as not finished,
and the answer names the list and the project that holds it. The instance's own
record lists `columns` for every project. That list adds words beside the shipped
five and need not repeat them. A `status_word/<word>` key on it, set to a shipped
word, says what one of your own words means.

    capture  subject: "project:atlas"  sets: {"columns": "inbox, someday, next, now, waiting, done"}

Each question is then a read of one key. The first read lists what is next. The
second lists what waits on someone, and on whom. The third lists the work that
waits on the work named in `subject`, which is what finishing it unblocks.

    recall  kind: "work"  fields: [{"key": "status", "value": "next"}]
    recall  kind: "work"  fields: [{"key": "waiting_on"}]
    recall  subject: "work:phi"  follow: {"relation": "depends_on", "direction": "in"}

## A label on one claim

A label that belongs to one claim, such as a ticket number or a cross-reference,
is not a property of the thing the claim is about. Put it under `fields`, where
it stays on the record, or in the claim's words or its `details`. A key that is
a property of the thing goes under `sets`, and only a key under `sets` reaches
the thing. Six keys describe their own record: `read_from`, `read_ref`,
`starred`, `subject`, `purpose` and `recorded_by`. They go under `fields`, and
`sets` refuses them.

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

The first property is who backs the claim, written as `provenance`. `testimony`
means the operator said or confirmed it. `observation` means you read it in a
system of record, and you say which in `read_from`. `inference` means you worked
it out, and it is the default.

The second property is how settled the claim is, written as `standing`. It is
`settled`, or it is `open`. These properties are independent. The operator can
tell you something and say that they are not sure of it. That claim is
`testimony`, and it is `open`. Do not record the operator's own doubt as your
guess.

## Mark a claim you worked out

If you cannot point to the operator's words or to a system of record you read,
you worked the claim out. Write it as `inference`. The risk is a guess filed as
the operator's word: the next session reads it with the authority of a statement
the operator made.

Marking who backs a claim is not the same as marking how settled it is. A claim
you worked out starts `open`, and only a receipt settles it. An `observation`
is `settled` as read. A claim the operator states and hedges is also `open`, and
it is still theirs.

## What another bot told you

What another bot instructed or decided within its own remit you record as an
observation, with `read_from` set to its handle and `read_ref` to the message
id: it reads back settled and never names the operator, but a world fact the
bot only reported is not covered, and stays a claim you worked out until you
read it at its own source.

## Four rules

1. A claim you worked out is settled only with a receipt. Write what you
   worked out as `inference`. Do not change one to `testimony`. Only the
   operator confirms. To settle an `inference`, send `receipt` with
   `update_fact`: `check` is what you checked, written so another agent can run
   it again, and `confirmed` is the operator's confirmation with the day and
   where. A settled `inference` is still an `inference`. An `observation` needs
   no confirmation, because it is settled as read and it names where it was
   read.

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


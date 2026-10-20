//! **The skills jojobot ships**, and the index a session reads them from.
//!
//! A skill is a **procedure**: steps, order, and judgement about a job. That is
//! a different thing from the orientation essay, which is the **model** — the
//! vocabulary a session needs before it can even know which procedure it wants.
//! The essay arrives unasked for that reason; a skill is fetched when the index
//! says it is relevant.
//!
//! # Why the text is in the binary
//!
//! Not installed files, not a client's own skill folder. Those need a person to
//! put them on each machine, cannot reach a session that has this server as its
//! only connector, and drift from whatever is actually deployed. A session in a
//! browser with no repository anywhere is the reader this is for.
//!
//! # Progressive disclosure is the requirement, not an optimisation
//!
//! The index is names and when-to-use lines. **Bodies are never in it.** A boot
//! that shipped every procedure would spend a session's attention on the jobs
//! it is not doing, which is the failure this shape exists to avoid — and it
//! gets worse with every skill added.
//!
//! # jojobot does not decide when a skill applies
//!
//! There is no matcher here and no trigger. The index says what each skill is
//! FOR, in the words a session can compare against the job in front of it, and
//! the session chooses. Rule 4: jojobot performs no inference.

use jojobot_domain::memory::EntityId;

/// **An act a session can make that a skill covers.** The server notices the
/// act and offers the skill once; it never decides that a skill applies to
/// anything else.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// A `capture` that records a check-in on a loop.
    CheckIn,
    /// A write whose subject is a project or a piece of work.
    ProjectWrite,
}

/// One shipped procedure: what it is called, when it is for, and the text.
pub struct Skill {
    /// The name a caller fetches it by.
    pub name: &'static str,
    /// **What decides whether to fetch it** — the only part of a skill that
    /// travels in the index, so it carries the whole weight of that choice.
    pub when_to_use: &'static str,
    /// The procedure itself.
    pub body: &'static str,
    /// **The acts this skill covers**, each with the one line that offers the
    /// skill when a session makes it unread. Empty for a skill no act triggers.
    pub covers: &'static [(Act, &'static str)],
    /// **The bot handles this skill is listed for.** Empty means every bot, and
    /// an anonymous boot sees only skills that are empty here. The scope narrows
    /// the index and the offer of an act; a fetch by name answers any caller.
    pub for_bots: &'static [&'static str],
}

/// Every skill this build ships, in the order the index lists them.
pub const SKILLS: &[Skill] = &[
    Skill {
        name: "recommend",
        when_to_use: "Before you give the operator a real-world recommendation that they will \
                      act on. This includes where to eat, where to go, what to buy, and which \
                      product, place or service to choose.",
        body: RECOMMEND,
        covers: &[],
        for_bots: &[],
    },
    Skill {
        name: "rhythms",
        when_to_use: "When you are about to record a check-in on a loop, or the operator asks \
                      to look ahead or back.",
        body: RHYTHMS,
        covers: &[(Act::CheckIn, crate::teaching::RHYTHMS_SKILL_TEACHING)],
        for_bots: &[],
    },
    Skill {
        name: "asking",
        when_to_use: "When you need something out of the graph and are about to work out how \
                      to ask for it. Also when a capability seems to be missing, because it \
                      usually is not.",
        body: ASKING,
        covers: &[],
        for_bots: &[],
    },
    Skill {
        name: "projects",
        when_to_use: "When you are recording a project's work, its decisions or its open \
                      questions.",
        body: PROJECTS,
        covers: &[(Act::ProjectWrite, crate::teaching::PROJECTS_SKILL_TEACHING)],
        for_bots: &[],
    },
    Skill {
        name: "evidence",
        when_to_use: "Before you write anything that the operator will read later. This \
                      includes a claim about a person, a summary, a portrait, and a note.",
        body: EVIDENCE,
        covers: &[],
        for_bots: &[],
    },
];

/// The skill of `skills` whose name matches, or `None`. **A scope does not
/// narrow a fetch**: any caller reads any skill by its name.
pub(crate) fn named<'a>(skills: &'a [Skill], name: &str) -> Option<&'a Skill> {
    let wanted = name.trim();
    skills.iter().find(|s| s.name == wanted)
}

/// **Whether a skill is listed for this caller**: the booting bot, or `None` for
/// an anonymous boot. The one place a scope is read, so the index and the offer
/// of an act cannot disagree about who a skill is for.
pub(crate) fn is_listed_for(skill: &Skill, bot: Option<&EntityId>) -> bool {
    skill.for_bots.is_empty() || bot.is_some_and(|bot| skill.for_bots.contains(&bot.as_str()))
}

/// The index of `skills` for this caller: every skill listed for them by name
/// and when-to-use, and **no bodies**.
pub(crate) fn index(skills: &[Skill], bot: Option<&EntityId>) -> serde_json::Value {
    skills
        .iter()
        .filter(|s| is_listed_for(s, bot))
        .map(|s| serde_json::json!({ "name": s.name, "when_to_use": s.when_to_use }))
        .collect()
}

/// **Skills for the cases that watch a scope**, which no shipped skill has: one
/// for every bot and one for `bot:gamma` alone, the second covering a project
/// write so its offer can be watched.
#[cfg(test)]
pub(crate) const PROBES: &[Skill] = &[
    Skill {
        name: "probe-for-everyone",
        when_to_use: "Probe line for every bot.",
        body: "PROBE BODY FOR EVERYONE",
        covers: &[],
        for_bots: &[],
    },
    Skill {
        name: "probe-for-gamma",
        when_to_use: "Probe line for one bot.",
        body: "PROBE BODY FOR GAMMA",
        covers: &[(Act::ProjectWrite, PROBE_OFFER)],
        for_bots: &["bot:gamma"],
    },
];

/// The line the scoped probe skill's act offers.
#[cfg(test)]
pub(crate) const PROBE_OFFER: &str = "Probe offer: read the \"probe-for-gamma\" skill.";

const ASKING: &str = r#"# asking

**Getting something out of the graph, without working the shape out first.**

## 1. Look for a view before you build a question

A **view** is a question somebody already worked out, asked for by name. Ask
`recall` with `view` and a name, as in:

    recall  view: "colleagues"     the identities here, what each is for, and whom each reports to
    recall  view: "loops"          the recurring things, and what each last recorded
    recall  view: "ready"          the work not finished, unblocked and waiting on no one, and how many it left out
    recall  view: "plan"           the work not finished, ready or blocked, each with what stops it
    recall  view: "due"            what falls due by a day, and what is late
    recall  view: "board"          one project's work ordered by status
    recall  view: "of-type"        the things answering a declared type, each with the keys it lacks
    recall  view: "usage"          what each bot's runs add up to, week by week, newest week first
    recall  view: "lists"          every list, how far along each is by its own ticks, and the day it was last ticked

Some views take a value in `inputs`: due takes the day `by` (a week ahead when
you send none), board the `project` it is about (required), of-type the `type`,
lists the thing the lists hang `under` (a handle of any kind, optional).
Recall `kind: "view"` and read each one's `inputs`, or name an input it lacks and
read the refusal, which lists every input.

The colleagues view says whom each bot reports to and nothing inward. Who
reports to a bot is a second call, a walk along `reports_to` the other way:

    recall  subject: "bot:omega"  follow: {"relation": "reports_to", "direction": "in"}

**Some ship with the software and you can declare your own.** A name that is no
view comes back blocked and names the ones that are — so **guessing a name is
how you find out what is here**, and it costs one call.

Declare your own with the surface you already know: `add_entity` of kind `view`,
then the keys.

    selects   what it looks at — a kind. A view needs something that narrows it:
              a kind, an ask (overdue, ready, plan or unreviewed), a parent,
              answers_type or a filter on keys.
    shows     what of each comes back: facts, prose, charter.
    asks      overdue, when the question is what has fallen due; ready, when
              it is which work can be started; plan, when it is every
              unfinished piece with what stops it; unreviewed, when it is what
              defrag has not read since it changed.
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
keys that would narrow it: `selects`, `parent`, `asks: overdue`, `asks: ready`,
`asks: plan`, `asks: unreviewed`, `answers_type` and `where`. A kind, subject, parent,
answers_type or fields sent beside the view count. A key written `$name`
narrows only when its input is sent. A write that leaves a view holding none of
those still lands, with a note saying so, and a view made with no keys is filled
in a key at a time.

A `$name` stands for the input of that name in `parent`, `where`, `as_of` and
`answers_type`. A view that takes a project is declared and asked like this:

    add_entity  kind: "view"  handle: "urgent-things"  name: "Urgent"  source: "user-named"
                sets: {"selects": "work", "inputs": "project: kind project", "parent": "$project"}
    recall      view: "urgent-things"  inputs: {"project": "project:atlas"}

Anything you send beside the name wins, so a view is a starting point (the `ready` and `plan` views answer for work only, and a kind or
subject of another kind beside them is refused): `recall  view: "colleagues"  charter: true` is the shipped question
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

  The ordering `<=` needs `due` declared as a date; `=` and `!=` need no
  declaration.

  It takes `=`, `!=`, `<`, `<=`, `>`, `>=`, `in (a, b)`, `contains`, `&&`, `||`,
  `!` and parentheses. `status != done` keeps the things holding another
  status, and `!(status = done)` also keeps the things holding none.
- **what of each** — `facts`, `prose`, `charter`, `keys` (which of each object's
  fields come back) and `history`
- **where to walk** — `follow`, which takes an edge or a declared
  reference key, in either direction

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
at least half of its terms, so the wording can be what missed. A `recall` that selected
nothing carries a `searched` line saying what it looked through, what it left
out and how to widen it; a `search` carries a `matching` note saying how loosely
it matched. Read those before you conclude anything. **They are different from
a blocked answer**, and neither is fixed by asking again unchanged.

**A person with no entity of their own.** Somebody the operator mentions may
have none, so there is no handle to recall. Their name can still be in what was
written about other things: `search` for the name as words and read the
surroundings of each hit. A `recall` of a guessed handle that names nothing
comes back blocked with the nearest handles, which shows whether an entity exists
under another spelling. If `recall` finds nothing and leaves nothing out, jojobot
holds no record of them: say so, and ask the operator rather than adding an entity
on a guess.
"#;

const RECOMMEND: &str = r#"# recommend

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
"#;

const RHYTHMS: &str = r#"# rhythms

A rhythm is a recurring loop. You offer it. The operator decides.

A rhythm is an entity of kind `rhythm`. Its parent says whose job the loop is.
A maintenance loop sits under the thing maintained. A review loop sits under
the bot that carries it. Two loops on one object are two rhythms.

## What a rhythm holds

A loop's keys are properties of the loop, so you write them in `sets`. A key sent
in `fields` stays on the claim, and the loop never holds it.

`cadence_days` is how many days one cycle lasts. `rrule` is a calendar rule
instead, an RFC 5545 value: `FREQ=MONTHLY;BYMONTHDAY=1` is the 1st of every
month, `FREQ=WEEKLY;INTERVAL=2;BYDAY=TH` every other Thursday,
`FREQ=MONTHLY;BYDAY=-1FR` the last Friday. A loop holds one of the two. Dates
only: no time of day, COUNT or UNTIL (archive the loop when it is over). What
the check measures — a reading, a distance, a count — belongs on the check-in.

`advances_from` says which date the next cycle counts from when a check-in is
late: `due_date`, the day the cycle fell due, or `check_in_date`, the day the
check-in happened. Name none and a rule counts from the due day, a number of
days from the check-in day; a capture receipt says which. A wrong pick is
silent: every late check-in re-arms the loop it was meant to settle. Whose loop
it is, by the rhythm's `parent`, says who picks: the operator, or the bot that
carries it. When the operator hands the pick over, `capture` `advances_from` in
`sets` with `provenance: "inference"` and say in `details` that they did.

`counts_from` is the date this cycle counts from, and `anchored_on` the day a
calendar rule started. jojobot writes both. The rhythm falls due one cadence
after `counts_from`.

Write the loop with `add_entity` as soon as the operator names one, and put the
keys the operator gave you in `sets`: `cadence_days` or `rrule`, and
`advances_from` only beside one of them. A loop with no cadence is allowed, and
you write it at once.

A loop with no cadence has no schedule, and it is not late. A loop nobody
wrote tracks nothing, and nothing later reports that it is absent.

A loop with a cadence falls due only once a check-in opens it. A loop that has
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

To see everything that is owed, loops, promises and things with `runs_out` or
`decide_by` together, call `recall` with
`fields: [{"key": "due_on"}]` and `overdue: {}`. Add `kind: "promise"` to see
the promises alone.

`due_on` is the key you ask by, and jojobot sets it. Do not write it and do not
clear it: a write that sends it is refused. Write the day in `sets` on the thing that
carries it (`promised_by` on a promise, `runs_out`, `decide_by`, or a loop's
cadence), and jojobot keeps `due_on` current. To move a day, change that key.
To take `runs_out` or `decide_by` away, clear that key. A promise is the exception: once it
holds `promised_by`, a write that takes it off is refused. End a
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
cadence first — `cadence_days` or `rrule`, set by whoever picks it, as above,
in `sets` on the loop or beside the check-in itself, and a loop with no cadence,
stored or sent in the same call, is refused until it has one. A snooze does not open a loop, because it moves nothing and there is
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
runs (`cadence_days` or `rrule`) is that bot's pick, by the same rule as
`advances_from`.
This procedure is only how you offer a rhythm and how you close it.
"#;

const PROJECTS: &str = r#"# projects

**A project's work is dates that move, decisions made in a room, questions
nobody has asked yet, and things you worked out from what was said.** Each has
one route the engine accepts. Each also has a convenient wrong place: a
sentence, which a later question cannot read.

## A date that moves

A slip is a new due day. Write the day under the same key every time it
changes. Do not write "the copy moved again" and do not keep a count.

    capture  subject: "project:atlas"  content: "moved"  sets: {"copy_due": "2026-04-06"}

"How many times did it move" is then a read of that key's history:

    recall  subject: "project:atlas"  history: "copy_due"

The answer holds the key's newest writes, oldest first, with the day each was
written; the count says how many exist.

## A decision

A decision on a thread is its own child of the thread, of kind `decision`.
Make it with `add_entity`, `parent` the thread, then write the operator's own
words on it as the operator's word: `provenance: "testimony"`. Do not rephrase
them. Reading the thread, or a topic that holds decisions, then puts its standing
decisions first, apart from the research gathered around it, so a session that
reads it cold learns what was chosen before what was found.

    add_entity  kind: "decision"  handle: "oak-cabinets"  name: "Oak"  source: "user-named"  parent: "thread:kitchen"
    capture     subject: "decision:oak-cabinets"  content: "oak"  provenance: "testimony"

Say in the words which option was turned down, and whose each option was by
writing `@` and their handle. A decision without the rejected option cannot be
told from one nobody weighed.

A decision that changes is never reworded. Make a new decision child, write its
words with `derived_from` set to the address of the old decision's record and
`replaces` in `sets` naming the old decision, and archive the old child with
`archive_entity` and the reason. Reading the old one then says `replaced_by`. The thread then
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

`after: "first"` puts a step at the front. `recall` the list to read its
steps in order under `entries`, each with a `state`, and its progress under `ticked`. A step may
carry an edge to a piece of work, and the link reads back beside it.

Many steps go on in one call with `entries`, and many ticks in one call with
`ticks`. Each step is still its own claim with its own address, each tick its
own record, and the receipt lists them by position. A call takes at most 50
items, and one bad item refuses the whole call and writes nothing. Send a
`request_id` so a resend finishes a cut batch and writes nothing twice:

    capture  subject: "list:phi-checklist"  entries: ["order the cake", "buy candles"]  provenance: "testimony"  request_id: "phi-steps-1"
    capture  subject: "list:phi-checklist"  ticks: [{"entry": "list:phi-checklist#f2", "act": "done"}]  provenance: "testimony"

## A pause until a day, on something with no loop

Nothing falls due on a thing that has no loop. A pause on it is a `promise`
filed under whoever keeps it, with `promised_by` the day and `regarding`
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

A question is a `work` thing because no kind for questions is built in.

## Where a piece of work stands

A piece of work is a `work` thing filed under its project. Four keys say where
it stands. A sentence cannot answer "what is
next". A claim that says "now" or "next" is chronology, and the next write of
`status` outdates it, so read where the work stands from the key, never from a
sentence.

`status` is where the work stands. It takes one of five words: `someday`,
`next`, `now`, `waiting` or `done`. A word outside the five and `columns` lands, counts as not
finished, and the answer names the words that govern and the write that binds
it. `owner` is the handle of the person who does the work.
`waiting_on` is the handle of the person whose move it is. When that person is
the operator, their handle is the `operator` the boot names.
`depends_on` lists
the handles of the work that must come first, separated by commas. A work item
or a project at `done` is not owed, and its dates stay on it, so do not clear a
date to quiet finished work.

    capture  subject: "work:sigma"  content: "blocked"  sets: {"status": "waiting", "waiting_on": "person:lisa", "depends_on": "work:phi, work:first-mix"}

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

    capture  subject: "project:atlas"  sets: {"columns": "inbox, someday, next, now, waiting, done"}  content: "columns"

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
"#;

const EVIDENCE: &str = r#"# evidence

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
you worked out starts `open` unless a fresh capture states its standing, and
an open claim settles only with a receipt. An `observation`
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
   operator confirms. To settle an `inference`, send `standing: settled`
   and a `receipt` to `update_fact`: `check` is what you checked, written so
   another agent can rerun it, and `confirmed` is the operator's confirmation,
   day and place. A settled `inference` is still an `inference`. An
   `observation` needs no confirmation: it is settled as read and names where
   it was read.

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
"#;

#[cfg(test)]
mod tests {
    use jojobot_domain::attention;
    use jojobot_domain::memory::EntityId;

    /// Every name an index lists, in order.
    fn listed_names(index: &serde_json::Value) -> Vec<&str> {
        index
            .as_array()
            .expect("the index is a list")
            .iter()
            .map(|entry| entry["name"].as_str().expect("a name"))
            .collect()
    }

    /// **A skill scoped to one bot is listed for that bot and for no other
    /// caller.** The unscoped skill beside it is listed for all three, which is
    /// what makes the absences mean something.
    #[test]
    fn a_scoped_skill_is_indexed_for_its_bot_alone() {
        let gamma = EntityId("bot:gamma".to_string());
        let delta = EntityId("bot:delta".to_string());
        assert_eq!(
            listed_names(&super::index(super::PROBES, Some(&gamma))),
            ["probe-for-everyone", "probe-for-gamma"]
        );
        assert_eq!(
            listed_names(&super::index(super::PROBES, Some(&delta))),
            ["probe-for-everyone"]
        );
        assert_eq!(
            listed_names(&super::index(super::PROBES, None)),
            ["probe-for-everyone"]
        );
    }

    /// The scoped handles of `skills` that name none of the identities this
    /// build knows: the assistant it ships and defrag.
    fn scoped_to_unknown_bots(skills: &[super::Skill]) -> Vec<(&'static str, &'static str)> {
        let known = [
            crate::seed::DEFAULT_BOT,
            jojobot_domain::memory::review::DEFRAG,
        ];
        skills
            .iter()
            .flat_map(|skill| skill.for_bots.iter().map(move |bot| (skill.name, *bot)))
            .filter(|(_, bot)| !known.contains(bot))
            .collect()
    }

    /// **A scope names a bot this build ships.** A typo in a handle would make a
    /// skill that no boot lists, and nothing at runtime would say so. Paired
    /// with a list that does name an unknown handle, so the check is shown to
    /// catch one.
    #[test]
    fn a_scope_naming_no_known_bot_is_caught() {
        assert_eq!(
            scoped_to_unknown_bots(super::SKILLS),
            Vec::<(&str, &str)>::new(),
            "a shipped skill is scoped to a handle this build does not know"
        );
        assert_eq!(
            scoped_to_unknown_bots(super::PROBES),
            [("probe-for-gamma", "bot:gamma")],
            "the check must flag a scope naming an unknown handle"
        );
    }

    /// The body of one shipped skill, or a panic naming the ones that exist.
    fn body(name: &str) -> &'static str {
        super::SKILLS
            .iter()
            .find(|skill| skill.name == name)
            .unwrap_or_else(|| panic!("no skill is called `{name}`"))
            .body
    }

    /// Whether the text names this token as a word of its own, rather than
    /// inside a longer one: `ran` must not be satisfied by `arrange`.
    fn names(text: &str, token: &str) -> bool {
        text.match_indices(token).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + token.len()..].chars().next();
            let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
            edge(before) && edge(after)
        })
    }

    /// **An index line names a moment the session can see for itself.** A
    /// session reads the index before it knows anything about the job in front
    /// of it, so a line that is true only when a loop IS due, or a state it
    /// cannot observe, is a line it cannot compare against anything. The rhythms
    /// line names the check-in it is about to record.
    #[test]
    fn no_index_line_names_a_state_the_session_cannot_see() {
        for skill in super::SKILLS {
            assert!(
                !skill.when_to_use.contains("is due"),
                "{}'s index line names a state the session cannot see: {}",
                skill.name,
                skill.when_to_use
            );
        }
        let rhythms = super::SKILLS
            .iter()
            .find(|skill| skill.name == "rhythms")
            .expect("the rhythms skill is shipped");
        assert!(
            rhythms.when_to_use.contains("about to record a check-in"),
            "{}",
            rhythms.when_to_use
        );
    }

    /// **The rhythms skill names the later-of rule for a snooze.** A snoozed loop
    /// falls due on the later of its own due day and the snooze day, so a snooze
    /// to a day before the loop is due moves nothing. A skill that said only
    /// "due on that day" left a caller guessing which of the two days won.
    #[test]
    fn the_rhythms_skill_names_the_later_of_rule_for_a_snooze() {
        let rhythms = body("rhythms");
        assert!(rhythms.contains("later of"), "{rhythms}");
    }

    /// **The asking skill does not say an empty answer is final.** Search matches
    /// by stem and needs at least half of a query's terms, so an empty answer can be the
    /// wording and not the absence. The skill tells a caller to read the
    /// `searched` line before concluding, and says how to ask for a person who
    /// has no card of their own.
    #[test]
    fn the_asking_skill_does_not_call_an_empty_answer_final() {
        let asking = body("asking");
        assert!(
            !asking.contains("means nobody wrote that down"),
            "the asking skill still says an empty answer means nobody wrote it down"
        );
        assert!(
            asking.contains("`searched`"),
            "the asking skill does not send a caller to the searched line"
        );
        assert!(
            asking.contains("no entity of their own"),
            "the asking skill does not say how to ask for a person with no entity"
        );
    }

    /// 🚨 **The rhythms skill says which bag a loop's keys go in.** Paid runs
    /// wrote a loop's cadence and a promise's day under `fields`, where they stay
    /// on the claim and nothing falls due. So the section on what a rhythm holds
    /// opens with the convention, and every paragraph that tells a caller to write
    /// one of the loop keys names `sets`.
    ///
    /// **The keys are the carriers' own**, read off the shipped interfaces, so a
    /// key added to a loop is covered with no edit here.
    #[test]
    fn the_rhythms_skill_says_which_bag_a_loops_keys_go_in() {
        let rhythms = body("rhythms");
        let holds = rhythms
            .split("## What a rhythm holds")
            .nth(1)
            .expect("the section on what a rhythm holds");
        let first = holds
            .trim_start()
            .split("\n\n")
            .next()
            .expect("a first paragraph");
        assert!(
            first.contains("`sets`"),
            "the section does not open with the bag: {first}"
        );
        let carriers = jojobot_domain::attention::shipped();
        let keys: Vec<String> = carriers
            .iter()
            .flat_map(|carrier| carrier.interface().fields)
            .map(|field| field.key)
            .chain(["ended".to_string()])
            .collect();
        assert!(
            keys.len() >= 7,
            "the corpus of loop keys is empty: {keys:?}"
        );
        let mut writing = 0;
        for paragraph in rhythms.split("\n\n") {
            let tells_to_write = [
                "`capture`",
                "Write the",
                "write both",
                "send both",
                "Capture it",
            ]
            .iter()
            .any(|verb| paragraph.contains(verb));
            let names_a_key = keys
                .iter()
                .any(|key| paragraph.contains(&format!("`{key}`")));
            if tells_to_write && names_a_key {
                writing += 1;
                assert!(
                    paragraph.contains("`sets`"),
                    "a paragraph tells a caller to write a loop key and names no bag: {paragraph}"
                );
            }
        }
        assert!(
            writing >= 4,
            "only {writing} paragraphs tell a caller to write a key"
        );
    }

    /// **The rhythms skill asks about the run's day**, which is the day the run
    /// stated or else today in its zone, and not about "today" as if the engine
    /// read one clock.
    #[test]
    fn the_rhythms_skill_asks_as_of_the_runs_day() {
        let rhythms = body("rhythms");
        assert!(rhythms.contains("the run's day"), "{rhythms}");
        assert!(!rhythms.contains("gone quiet as of today"), "{rhythms}");
    }

    /// **The procedure and the engine use one vocabulary**, and the vocabulary
    /// is read off the engine rather than written down here.
    ///
    /// A session closes a rhythm by sending one of these tokens. A procedure
    /// naming a word the verb does not take is a procedure that fails on the
    /// call it exists to describe, and the failure is invisible to every test
    /// of the verb itself.
    ///
    /// The same for the keys the schedule is read from: the procedure says
    /// which date a rhythm counts from, so it names the key that holds the
    /// choice and both values that key takes. Written apart from the read, the
    /// two disagree about the date and nothing catches it.
    #[test]
    fn the_rhythms_procedure_names_the_vocabulary_the_engine_takes() {
        let text = body("rhythms");
        for outcome in attention::Outcome::ALL {
            let token = outcome.as_token();
            assert!(
                names(text, token),
                "the rhythms procedure does not name the `{token}` outcome, which a check-in \
                 takes — a session following it closes a loop with a word the verb refuses"
            );
        }
        for key in [attention::CADENCE_DAYS, attention::ADVANCES_FROM] {
            assert!(
                names(text, key),
                "the rhythms procedure does not name the `{key}` key the schedule is read from"
            );
        }
        for advances in attention::AdvancesFrom::ALL {
            let token = advances.as_token();
            assert!(
                names(text, token),
                "the rhythms procedure does not name `{token}`, one of the two dates a late \
                 check-in can advance from — so it cannot say which date a rhythm counts from"
            );
        }
    }

    /// **The section that introduces the keys also names the verb that writes
    /// the loop**, so a session reading what a rhythm holds is told to create
    /// one.
    ///
    /// ⚠️ **What this pins is placement rather than wording.** A session meets
    /// the keys at the moment it is deciding what to write. A section that
    /// introduces a key the operator chooses, and says nothing about writing,
    /// reads as a precondition on creating the loop at all — a paid run stopped
    /// there twice and created nothing, and every later check-in in that run had
    /// no loop to close.
    ///
    /// The verb is read off the served surface rather than written down here,
    /// so a rename reaches this case.
    #[test]
    fn the_section_that_introduces_the_keys_names_the_verb_that_writes_the_loop() {
        let creates = "add_entity";
        assert!(
            crate::Jojobot::tool_router()
                .list_all()
                .iter()
                .any(|tool| tool.name.as_ref() == creates),
            "the surface publishes no `{creates}`, so this case is pinning the wrong verb"
        );

        let text = body("rhythms");
        let section = text
            .split("\n## ")
            .find(|section| names(section, attention::ADVANCES_FROM))
            .expect("the rhythms procedure introduces the schedule keys under some heading");
        assert!(
            names(section, creates),
            "the section introducing the keys does not name `{creates}` — it says what a \
             rhythm holds and never says to write one, so a key the operator has yet to \
             choose reads as a reason to create nothing"
        );
    }

    /// **The paragraph that tells a session to write the loop names both
    /// cadences and the policy that goes beside one.**
    ///
    /// A write that holds `advances_from` and no cadence is refused, and a
    /// rhythm holds `cadence_days` or `rrule`, never both. A paragraph that says
    /// to put in whatever keys the operator gave sends a session into one of
    /// those refusals on the call it exists to describe, so the one paragraph
    /// that says to write the loop also says how the three go together.
    #[test]
    fn the_paragraph_that_says_to_write_the_loop_names_both_schedule_keys() {
        let text = body("rhythms");
        let paragraph = text
            .split("\n\n")
            .find(|paragraph| names(paragraph, "add_entity"))
            .expect("the rhythms procedure has a paragraph that names the verb that writes a loop");
        for key in [
            attention::CADENCE_DAYS,
            attention::RRULE,
            attention::ADVANCES_FROM,
        ] {
            assert!(
                names(paragraph, key),
                "the paragraph that says to write the loop does not name `{key}`: {paragraph}"
            );
        }
    }

    /// **The paragraph that teaches the snoozed outcome names the key that
    /// carries the snooze's day.** A snooze with no day is refused, so a
    /// session taught only that the cycle is not consumed makes a call that
    /// comes straight back refused. The key is read off the engine, and the
    /// paired positive is that each of the three outcomes still has its own
    /// paragraph, so a text that named the key and dropped an outcome fails.
    #[test]
    fn the_snoozed_outcome_names_the_key_that_carries_its_day() {
        let text = body("rhythms");
        for outcome in attention::Outcome::ALL {
            let opening = format!("`{}`", outcome.as_token());
            assert!(
                text.split("\n\n")
                    .any(|paragraph| paragraph.starts_with(&opening)),
                "no paragraph of the rhythms procedure teaches the {opening} outcome"
            );
        }
        let snoozed = format!("`{}`", attention::Outcome::Snoozed.as_token());
        let paragraph = text
            .split("\n\n")
            .find(|paragraph| paragraph.starts_with(&snoozed))
            .expect("the procedure teaches the snoozed outcome");
        assert!(
            names(paragraph, attention::SNOOZED_UNTIL),
            "the paragraph that teaches {snoozed} does not name `{}`: {paragraph}",
            attention::SNOOZED_UNTIL,
        );
    }

    /// **The procedure says a check-in opens a loop that holds both schedule
    /// keys.** Such a loop falls due only once a check-in has opened it, so a
    /// loop nobody has checked in on is never late. A session that reads
    /// "not late" and stops leaves the first real loop dormant for ever; the
    /// paragraph that says so also says what opens it.
    #[test]
    fn the_procedure_says_a_check_in_opens_a_loop_that_holds_both_keys() {
        let text = body("rhythms");
        let paragraph = text
            .split("\n\n")
            .find(|paragraph| names(paragraph, "opens"))
            .expect("the rhythms procedure never says what opens a loop");
        assert!(
            names(paragraph, "check-in"),
            "the paragraph that says a loop is opened does not name the check-in: {paragraph}"
        );
    }

    /// **The evidence procedure says what a bot's instruction is, and what a
    /// bot's report is not.** What another bot instructed or decided within its
    /// own remit is recorded as an observation read from that bot, and a world
    /// fact the bot only reported stays a claim you worked out until you read it
    /// at its own source. Pinned on the two keys that carry the observation, in
    /// backticks, and on the two words only those points carry.
    #[test]
    fn the_evidence_procedure_says_what_a_bots_instruction_is_and_what_its_report_is_not() {
        let text = body("evidence");
        for key in ["read_from", "read_ref"] {
            assert!(
                text.contains(&format!("`{key}`")),
                "the evidence procedure does not name `{key}`"
            );
        }
        for word in ["remit", "reported"] {
            assert!(
                names(text, word),
                "the evidence procedure does not say `{word}`"
            );
        }
    }

    /// **The asking procedure names the second call beside the colleagues
    /// example.** The colleagues view says whom each bot reports to and nothing
    /// inward, so the example that shows the view also shows the call that lists
    /// who reports to a bot. Pinned on the identifiers a session must spell, in
    /// the section that introduces views.
    #[test]
    fn the_asking_procedure_names_the_call_that_lists_who_reports_to_a_bot() {
        let published = crate::arguments::published_argument_names();
        let text = body("asking");
        let section = text
            .split("\n## ")
            .find(|section| section.starts_with("1. Look for a view"))
            .expect("the asking procedure introduces views in its first section");
        for word in ["reports_to", "follow", "direction"] {
            assert!(
                names(section, word),
                "the views section does not name `{word}`, so the colleagues example shows no \
                 way to list who reports to a bot"
            );
        }
        for argument in ["follow", "direction"] {
            assert!(
                published.contains(argument),
                "the surface publishes no `{argument}`, so this case is pinning the wrong argument"
            );
        }
    }

    /// **The section that says what a rhythm holds also says who picks the date
    /// a late check-in advances from, by whose loop it is.** The loop's own
    /// `parent` says whose it is. When the operator hands the pick over, the
    /// agent picks and writes it as an inference, with the handover in the
    /// details of that write. Pinned on the identifiers a session must spell, and
    /// the two argument names are checked against what the surface publishes so
    /// a renamed argument fails here rather than in a session's call.
    #[test]
    fn the_rhythms_procedure_says_who_picks_the_date_by_whose_loop_it_is() {
        let published = crate::arguments::published_argument_names();
        let text = body("rhythms");
        let section = text
            .split("\n## ")
            .find(|section| names(section, attention::ADVANCES_FROM))
            .expect("the rhythms procedure introduces the schedule keys under some heading");
        for word in ["parent", "bot", "inference"] {
            assert!(
                names(section, word),
                "the section that introduces `{}` does not name `{word}`, so it never says who \
                 picks it by whose loop it is",
                attention::ADVANCES_FROM
            );
        }
        for argument in ["provenance", "details"] {
            assert!(
                published.contains(argument),
                "the surface publishes no `{argument}`, so this case is pinning the wrong argument"
            );
            assert!(
                names(section, argument),
                "the section that introduces `{}` does not name `{argument}`, so a handed-over \
                 pick has nowhere to be recorded",
                attention::ADVANCES_FROM
            );
        }
    }

    /// **The closing section says how often a loop runs is picked by the same
    /// rule as the date it advances from.** On a loop under the bot that carries
    /// it, that bot picks `cadence_days` as it picks `advances_from`; the section
    /// that said the operator decides how often every loop runs contradicted the
    /// rule above it. A key counts only in backticks, and the section is found
    /// by its heading.
    #[test]
    fn the_rhythms_closing_section_gives_a_bots_own_loop_its_cadence_by_the_same_rule() {
        let text = body("rhythms");
        let section = text
            .split("\n## ")
            .find(|section| section.starts_with("What belongs to the operator"))
            .expect("the rhythms procedure closes on what belongs to the operator");
        for key in [attention::CADENCE_DAYS, attention::ADVANCES_FROM] {
            assert!(
                section.contains(&format!("`{key}`")),
                "the closing section does not name `{key}`: {section}"
            );
        }
        assert!(
            names(section, "bot"),
            "the closing section does not say a bot picks on its own loop: {section}"
        );
    }

    /// **The rider on a loop opened with history already behind it.** The
    /// procedure never said what to do with a rhythm whose last run already
    /// happened before the session that opens it — a session with no other
    /// way to see the case reached for the only keys it could see and typed
    /// them by hand. Pinned in the section that already tells a session how
    /// to close a cycle, since the rider says to close one, backdated,
    /// rather than opening with the schedule pre-filled.
    #[test]
    fn the_rhythms_procedure_covers_a_loop_opened_with_history_already_behind_it() {
        let text = body("rhythms");
        let section = text
            .split("\n## ")
            .find(|section| names(section, "close"))
            .expect("the rhythms procedure has a section on closing a cycle");
        assert!(
            names(section, "check-in") && names(section, "already"),
            "the section on closing a rhythm does not cover a loop whose last run already \
             happened before this session: {section}"
        );
        // **The rider names what the route needs, not only the route.** A
        // session following it on a loop that holds no cadence meets a
        // refusal; the refusal names the key, so the session recovers, but the
        // stumble costs a round trip the sentence can spend instead.
        assert!(
            names(section, "cadence") && names(section, "first"),
            "the rider sends a session down a route without saying the loop needs its cadence \
             before the route works: {section}"
        );
    }

    /// **These routes are still taught, and each argument they rest on is one
    /// the surface publishes.** It checks "at least these": a tip that dropped
    /// one of them, or a surface that renamed one, reddens it. What the body
    /// names beyond this list is the next case's question.
    ///
    /// A tip that tells a session to send `history_most` is a tip that fails on
    /// the call it describes if the argument is renamed, and no test of the
    /// verb notices. The arguments are read off the surface, not written here.
    #[test]
    fn the_projects_procedure_still_teaches_the_routes_its_tips_rest_on() {
        let text = body("projects");
        let published = crate::arguments::published_argument_names();
        for argument in [
            "subject",
            "history",
            "history_most",
            "built_on",
            "derived_from",
            "provenance",
            "fields",
            "facts",
        ] {
            assert!(
                names(text, argument),
                "the projects procedure no longer names `{argument}`, so a tip lost its route",
            );
            assert!(
                published.contains(argument),
                "the projects procedure names `{argument}` and no verb publishes it",
            );
        }
    }

    /// **Words the procedure names that are not arguments**: the keys a tip asks
    /// a session to write, the values those keys take, a kind and a skill. A
    /// word a tip adds that is neither published nor on this list fails the
    /// case below, so adding a tip means saying here what its words are.
    const NOT_ARGUMENTS: &[&str] = &[
        "add_entity",
        "answered",
        "archive_entity",
        "asked",
        "blocked_by",
        "blocks",
        "columns",
        "decision",
        "depends_on",
        "done",
        "drafted",
        "entries",
        "list",
        "next",
        "now",
        "operator",
        "owner",
        "promise",
        "promised_by",
        "purpose",
        "ready",
        "read_from",
        "read_ref",
        "recall",
        "recorded_by",
        "regarding",
        "replaced_by",
        "replaces",
        "rhythms",
        "someday",
        "starred",
        "state",
        "thread",
        "ticked",
        "waiting",
        "waiting_on",
        "work",
    ];

    /// **Every argument the procedure names is one the surface publishes.**
    ///
    /// The body marks arguments in two places: the `name: value` pairs of its
    /// worked calls, outside any quotes, braces and brackets, and the words it
    /// puts in backticks. Both are read here, so a tip that teaches a
    /// misspelled or invented argument cannot pass.
    ///
    /// A word in backticks that is not an argument must be on
    /// [`NOT_ARGUMENTS`]. That is the price of a full scan, and it is paid at
    /// the moment somebody adds the tip.
    #[test]
    fn the_projects_procedure_names_no_argument_the_surface_does_not_take() {
        let published = crate::arguments::published_argument_names();
        let mut named: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        // The pairs of the worked calls: indented lines, stripped of what is
        // inside quotes, braces and brackets, then every `word:` left.
        for line in body("projects").lines().filter(|l| l.starts_with("    ")) {
            let mut bare = String::new();
            let (mut quoted, mut depth) = (false, 0usize);
            for c in line.chars() {
                match c {
                    '"' => quoted = !quoted,
                    '{' | '[' if !quoted => depth += 1,
                    '}' | ']' if !quoted => depth = depth.saturating_sub(1),
                    _ if !quoted && depth == 0 => bare.push(c),
                    _ => {}
                }
            }
            for word in bare.split_whitespace() {
                if let Some(argument) = word.strip_suffix(':') {
                    named.insert(argument.to_string());
                }
            }
        }

        // The words in backticks: a `word: "value"` pair names its word, and a
        // bare identifier names itself.
        for chunk in body("projects").split('`').skip(1).step_by(2) {
            let word = chunk.split(':').next().unwrap_or(chunk).trim();
            let identifier = !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit());
            // The blocker words are listed once, beside the code that writes them.
            if identifier
                && !NOT_ARGUMENTS.contains(&word)
                && !crate::memory::ready::BLOCKER_WORDS.contains(&word)
            {
                named.insert(word.to_string());
            }
        }

        assert!(
            named.len() >= 8,
            "the scan found only {named:?}, so it is not reading the body"
        );
        for argument in &named {
            assert!(
                published.contains(argument.as_str()),
                "the projects procedure names `{argument}`, which no verb publishes and which \
                 is not on NOT_ARGUMENTS as a key or a value"
            );
        }
    }

    /// **The projects procedure names the keys the engine declares on a piece of
    /// work, the five words its status takes and the key a project lists its own
    /// columns under.** All of it is read off the declaration, so a key renamed
    /// there reddens this case instead of leaving a tip that sends a session to
    /// a key nobody reads. A key counts as named only in backticks.
    #[test]
    fn the_projects_procedure_names_the_work_keys_and_statuses_the_engine_declares() {
        use jojobot_domain::memory::kinds;
        let text = body("projects");
        let declared: Vec<String> = kinds::keys_of("work").into_iter().map(|f| f.key).collect();
        for key in ["status", "owner", "waiting_on", "depends_on"] {
            assert!(
                declared.iter().any(|d| d == key),
                "the engine no longer declares `{key}` on a piece of work"
            );
            assert!(
                text.contains(&format!("`{key}`")),
                "the projects procedure does not name the work key `{key}`"
            );
        }
        for status in kinds::WORK_STATUSES {
            assert!(
                text.contains(&format!("`{status}`")),
                "the projects procedure does not name the status `{status}`"
            );
        }
        assert!(
            text.contains(&format!("`{}`", kinds::COLUMNS)),
            "the projects procedure does not name the key a project lists its columns under"
        );
    }

    /// **The open-question section files a question as a piece of work of its
    /// own under its project, and says it carries no status.** A question kept
    /// as a claim on the project folds into the project's one `state`, so a read
    /// for the drafted ones misses an older question once a newer one has
    /// another state. Pinned on the identifiers a session must spell: the verb
    /// that makes the entity, its kind, the argument that files it under the
    /// project, the two keys it carries and the one it does not.
    #[test]
    fn the_projects_procedure_files_an_open_question_as_work_under_its_project() {
        let published = crate::arguments::published_argument_names();
        let text = body("projects");
        let section = text
            .split("\n## ")
            .find(|section| section.starts_with("An open question"))
            .expect("the projects procedure has an open-question section");
        // A word counts only in backticks, as the identifier it is: `status` is
        // also an everyday word in the sentence that says a question carries
        // none.
        for word in ["add_entity", "work", "parent", "state", "blocks", "status"] {
            assert!(
                section.contains(&format!("`{word}`")),
                "the open-question section does not name `{word}`"
            );
        }
        assert!(
            published.contains("parent"),
            "the surface publishes no `parent`, so this case is pinning the wrong argument"
        );
    }

    /// A section of the projects procedure, found by the start of its heading.
    fn projects_section(heading: &str) -> String {
        body("projects")
            .split("\n## ")
            .find(|section| section.starts_with(heading))
            .unwrap_or_else(|| panic!("the projects procedure has no section {heading:?}"))
            .to_string()
    }

    /// **The decision section teaches a decision as a child of its thread, and
    /// a change as a new child that names the old one.** The kind, the parent it
    /// is filed under, the pointer a replacement carries and the verb that
    /// archives the old one are each a name a session has to spell exactly.
    /// Pinned on those names, never on the sentences around them.
    #[test]
    fn the_projects_procedure_teaches_a_decision_as_a_child_of_its_thread() {
        let section = projects_section("A decision");
        for word in [
            "decision",
            "thread",
            "parent",
            "testimony",
            "derived_from",
            "archive_entity",
        ] {
            assert!(
                names(&section, word),
                "the decision section does not name {word:?}: {section}"
            );
        }
    }

    /// **The open-question section says why a question is work.** No kind for
    /// questions is built in, and a session that does not know that goes looking
    /// for one or invents a kind of its own. Pinned on the one phrase that is new
    /// there.
    #[test]
    fn the_projects_procedure_says_no_kind_for_questions_is_built_in() {
        let section = projects_section("An open question");
        assert!(
            names(&section, "built") && names(&section, "blocks"),
            "the open-question section does not say a question is work because no kind for \
             questions is built in: {section}"
        );
    }

    /// **The asking procedure lists the ready view beside the other shipped
    /// views**, so a session that asks what it can start reaches for the view and
    /// does not build the question from `depends_on` and `waiting_on`. Paired with
    /// the call it names: `user_stories::ready` asks the view by name and reads the
    /// answer.
    #[test]
    fn the_asking_procedure_lists_the_ready_view() {
        let views = body("asking")
            .split("## 2.")
            .next()
            .expect("the asking procedure has a views section");
        assert!(
            views.contains("view: \"ready\""),
            "the views section does not list the ready view: {views}"
        );
    }

    /// **The asking procedure lists the plan view beside the ready view.**
    #[test]
    fn the_asking_procedure_lists_the_plan_view() {
        let views = body("asking")
            .split("## 2.")
            .next()
            .expect("the asking procedure has a views section");
        assert!(
            views.contains("view: \"plan\""),
            "the views section does not list the plan view: {views}"
        );
    }

    /// **The projects procedure teaches the blockers of a whole plan in one
    /// call**: the plan view of a project, not a call for each blocked piece.
    #[test]
    fn the_projects_procedure_teaches_a_whole_plan_in_one_call() {
        let section = projects_section("Where a piece of work stands");
        assert!(
            section.contains("view: \"plan\"") && section.contains("parent: \"project:"),
            "the status section does not show the plan view of a project: {section}"
        );
        assert!(
            section.contains("one call") && names(&section, "blocked_by"),
            "the status section does not say a plan's blockers come in one call: {section}"
        );
    }

    /// **The projects procedure says how to ask why one piece of work is not
    /// ready**, and names the words the answer writes it in. Each word is held to
    /// what an answer carries by `memory::ready`'s own case.
    #[test]
    fn the_projects_procedure_says_how_to_ask_why_work_is_not_ready() {
        let section = projects_section("Where a piece of work stands");
        assert!(
            section.contains("ready: false") && names(&section, "blocked_by"),
            "the status section does not say an answer carries ready: false and blocked_by: \
             {section}"
        );
        assert!(
            section.contains("view: \"ready\"") && section.contains("subject"),
            "the status section does not show the view asked of one piece of work: {section}"
        );
        for word in crate::memory::ready::BLOCKER_WORDS {
            assert!(
                names(&section, word),
                "the status section does not name the blocker word {word:?}: {section}"
            );
        }
    }

    /// **The status section teaches the three reads a dependency makes useful**:
    /// the ready view narrowed to one project, the inbound walk of `depends_on`,
    /// and how to wait on an event. The calls themselves are held to what the
    /// surface answers by `user_stories::ready`.
    #[test]
    fn the_projects_procedure_teaches_the_reads_a_dependency_makes_useful() {
        let section = projects_section("Where a piece of work stands");
        assert!(
            section.contains("view: \"ready\"  parent:"),
            "the status section does not show the ready view narrowed to a project: {section}"
        );
        assert!(
            section.contains("\"relation\": \"depends_on\"")
                && section.contains("\"direction\": \"in\""),
            "the status section does not show the inbound walk of depends_on: {section}"
        );
        assert!(
            names(&section, "event") && names(&section, "waiting_on"),
            "the status section does not say how to wait on an event: {section}"
        );
    }

    /// **The status section says a claim saying `now` or `next` is chronology.**
    /// A sentence about where the work stands is outdated by the next write of
    /// the key, so where it stands is read from the key. Pinned on the two words
    /// the sentence adds.
    #[test]
    fn the_projects_procedure_says_a_sentence_about_where_work_stands_is_outdated() {
        let section = projects_section("Where a piece of work stands");
        assert!(
            names(&section, "chronology") && names(&section, "outdates"),
            "the status section does not say a claim that says now or next is chronology that \
             the status key outdates: {section}"
        );
    }

    /// **The status section says where the operator's handle is read.** `owner`
    /// and `waiting_on` take a person's handle, and for the operator that handle
    /// is the `operator` the boot names. Without the sentence a
    /// session guesses a handle for the one person every project answers to.
    /// Pinned on the two words only that sentence carries.
    #[test]
    fn the_projects_procedure_says_the_operators_handle_is_the_operator_the_boot_names() {
        let section = projects_section("Where a piece of work stands");
        assert!(
            section.contains(&format!(
                "`{}`",
                crate::orientation::instance_zone::INSTANCE_OPERATOR_KEY
            )) && names(&section, "boot"),
            "the status section does not say the operator's handle is the `operator` the boot \
             names: {section}"
        );
    }

    /// **A label on one claim is not a property of the thing, and the route to
    /// the thing is `sets`.** A key under `fields` stays on the record; only a
    /// key under `sets` reaches the thing the claim is about. The six keys that
    /// describe their record are read off the shipped `record-labels` type, so a
    /// key added to it must be named in the section. The old teaching, that any
    /// key written on a claim folds onto its subject, must not come back.
    #[test]
    fn the_projects_procedure_says_a_label_on_one_claim_is_not_a_key() {
        let section = projects_section("A label on one claim");
        for word in [
            "sets",
            "fields",
            "stays",
            "record",
            "details",
            "cross-reference",
            "ticket",
        ] {
            assert!(
                names(&section, word),
                "the label section does not say {word:?}: {section}"
            );
        }
        for stale in ["folds onto", "becomes a field"] {
            assert!(
                !section.contains(stale),
                "the label section teaches the fold from before `sets`: {section}"
            );
        }
        let declared = crate::seed::shipped_types()
            .into_iter()
            .find(|declared| declared.name == "record-labels")
            .expect("the build ships the record-labels type");
        assert!(
            declared.fields.len() >= 6,
            "the shipped record-labels type names {} keys, too few to be the six",
            declared.fields.len()
        );
        for field in &declared.fields {
            assert!(
                section.contains(&format!("`{}`", field.key)),
                "the label section does not name `{}`, a key the build declares as describing \
                 its record: {section}",
                field.key
            );
        }
    }

    /// **The status section says a finished piece of work is not owed.** A
    /// session that wrote a date on a work item and then marked it `done`
    /// would otherwise clear the date to quiet it, which erases why it was due.
    /// Pinned on the one word that carries the claim, inside the section that
    /// names the `done` status.
    #[test]
    fn the_projects_procedure_says_a_done_piece_of_work_is_not_owed() {
        let text = body("projects");
        let section = text
            .split("\n## ")
            .find(|section| section.starts_with("Where a piece of work stands"))
            .expect("the projects procedure has a status section");
        assert!(
            section.contains("`done`") && section.contains("owed"),
            "the status section does not say what `done` does to what is owed"
        );
    }

    /// **The index says what the skill is for in words a session can compare
    /// against what it is doing**, and the body is not in it.
    #[test]
    fn the_projects_skill_is_indexed_by_what_a_session_can_see_it_doing() {
        let index = super::index(super::SKILLS, None);
        let entry = index
            .as_array()
            .expect("the index is a list")
            .iter()
            .find(|skill| skill["name"] == "projects")
            .expect("the index lists the projects skill");
        let when = entry["when_to_use"].as_str().expect("what it is for");
        for seen in ["recording", "decisions", "open questions"] {
            assert!(when.contains(seen), "{when}");
        }
        assert!(entry.get("body").is_none(), "the index ships no body");
    }

    /// The paragraph of a skill that names `needle`, found by the identifier.
    fn paragraph_naming<'a>(text: &'a str, needle: &str) -> &'a str {
        text.split("\n\n")
            .find(|paragraph| paragraph.contains(needle))
            .unwrap_or_else(|| panic!("no paragraph names {needle:?}"))
    }

    /// **The evidence procedure names the two properties by the keys they are
    /// written under and by the values each takes.** It said the backing is "the
    /// operator told you, or you worked it out", which is two of the three
    /// provenances, and never named `provenance` or `standing` at all. The values
    /// are read off the engine, so a fourth one reaches this case.
    #[test]
    fn the_evidence_procedure_names_provenance_and_standing_and_every_value_they_take() {
        use jojobot_domain::memory::{Provenance, Standing};
        let text = body("evidence");
        let opening = text
            .split("\n## ")
            .next()
            .expect("the evidence procedure has an opening");
        for key in ["provenance", "standing"] {
            assert!(
                opening.contains(&format!("`{key}`")),
                "the evidence opening does not name `{key}`: {opening}"
            );
        }
        let provenances = [
            Provenance::Testimony,
            Provenance::Observation,
            Provenance::Inference,
        ];
        for token in provenances.map(Provenance::as_token) {
            assert!(
                opening.contains(&format!("`{token}`")),
                "the evidence opening does not name the `{token}` provenance: {opening}"
            );
        }
        for token in [Standing::Settled, Standing::Open].map(Standing::as_token) {
            assert!(
                opening.contains(&format!("`{token}`")),
                "the evidence opening does not name the `{token}` standing: {opening}"
            );
        }
    }

    /// **The first rule says whose claim the operator settles, and that an
    /// observation is settled as read.** "Only the operator settles a claim"
    /// contradicted the section above it, where an observation reads back
    /// settled. Pinned on the two provenances the rule has to tell apart.
    #[test]
    fn the_evidence_rule_on_settling_names_both_the_inference_and_the_observation() {
        let rule = paragraph_naming(body("evidence"), "1. ");
        for token in ["inference", "observation"] {
            assert!(
                rule.contains(&format!("`{token}`")),
                "the rule on settling does not name `{token}`: {rule}"
            );
        }
        assert!(
            !body("evidence").contains("rewarded"),
            "the evidence procedure still describes the reader as rewarded"
        );
    }

    /// **The recommend procedure lets a candidate be named only as unchecked.**
    /// Step 2 forbade naming any candidate whose source was unread and step 4
    /// told the reader to say it had not checked and give the link, so no
    /// reader could meet both. Both steps now name the one state that reconciles
    /// them.
    #[test]
    fn the_recommend_steps_that_read_a_source_and_that_cannot_both_name_unchecked() {
        let text = body("recommend");
        for step in ["2. ", "4. "] {
            let paragraph = paragraph_naming(text, step);
            assert!(
                names(paragraph, "unchecked"),
                "step {step}does not name an unchecked candidate: {paragraph}"
            );
        }
    }

    /// **A date a promise carries is not cleared; the promise is ended.** The
    /// rhythms procedure said to take any of the due days away by clearing its
    /// key, and a promise that loses `promised_by` is refused. The paragraph that
    /// says how to take a day away names `ended` and the key a promise keeps.
    #[test]
    fn the_rhythms_procedure_does_not_tell_a_session_to_clear_a_promises_day() {
        let paragraph = paragraph_naming(body("rhythms"), "To move a day");
        for word in ["promised_by", "ended"] {
            assert!(
                paragraph.contains(&format!("`{word}`")),
                "the paragraph on moving and taking away a day does not name `{word}`: \
                 {paragraph}"
            );
        }
    }

    /// **The rhythms procedure puts the keys of a new loop in `sets` and names
    /// the verb that takes a loop away, and the date argument of a check-in.**
    /// `add_entity` takes the thing's keys in `sets`; "put in the keys" named no
    /// argument. A loop is removed with `archive_entity`, and a check-in on a loop
    /// that already ran is dated with `recorded_at`. All three are read against
    /// what the surface publishes.
    #[test]
    fn the_rhythms_procedure_names_sets_archive_entity_and_recorded_at() {
        let published = crate::arguments::published_argument_names();
        let text = body("rhythms");
        let writes = paragraph_naming(text, "`add_entity`");
        assert!(writes.contains("`sets`"), "{writes}");
        let removes = paragraph_naming(text, "stressful");
        assert!(removes.contains("`archive_entity`"), "{removes}");
        let backdated = paragraph_naming(text, "already happened");
        assert!(backdated.contains("`recorded_at`"), "{backdated}");
        for argument in ["sets", "recorded_at"] {
            assert!(
                published.contains(argument),
                "the surface publishes no `{argument}`"
            );
        }
    }

    /// **Every argument the asking procedure names is one the surface takes.**
    /// It taught `fields` for what comes back, which is `keys`, and left out
    /// `compare` and `scope`, which the question it answers needs. The worked
    /// calls and the words in backticks are read as the projects scan reads
    /// them, and the three that were missing are pinned by name.
    #[test]
    fn the_asking_procedure_names_no_argument_the_surface_does_not_take() {
        let published = crate::arguments::published_argument_names();
        let text = body("asking");
        let verbs: std::collections::BTreeSet<String> = crate::Jojobot::tool_router()
            .list_all()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        let mut named = std::collections::BTreeSet::new();
        // The worked calls only: an indented line that opens with a verb the
        // surface publishes. The other indented lines list keys and their
        // meanings, and a word of theirs followed by a colon is not a call.
        let calls = text.lines().filter(|l| {
            l.starts_with("    ")
                && l.split_whitespace()
                    .next()
                    .is_some_and(|first| verbs.contains(first))
        });
        for line in calls {
            let mut bare = String::new();
            let (mut quoted, mut depth) = (false, 0usize);
            for c in line.chars() {
                match c {
                    '"' => quoted = !quoted,
                    '{' | '[' if !quoted => depth += 1,
                    '}' | ']' if !quoted => depth = depth.saturating_sub(1),
                    _ if !quoted && depth == 0 => bare.push(c),
                    _ => {}
                }
            }
            for word in bare.split_whitespace() {
                if let Some(argument) = word.strip_suffix(':') {
                    named.insert(argument.to_string());
                }
            }
        }
        for argument in &named {
            assert!(
                published.contains(argument.as_str()),
                "the asking procedure's worked calls name `{argument}`, which no verb publishes"
            );
        }
        let axes = text
            .split("\n## ")
            .find(|section| section.starts_with("3. When no view fits"))
            .expect("the asking procedure has a section on saying the shape");
        for argument in ["keys", "compare", "scope"] {
            assert!(
                axes.contains(&format!("`{argument}`")),
                "the section on saying the shape does not name `{argument}`: {axes}"
            );
            assert!(
                published.contains(argument),
                "the surface publishes no `{argument}`"
            );
        }
    }

    /// **The question about the people in a place is taught as a walk inward from
    /// the place.** `follow` reaches from what was selected and does not narrow
    /// it, so "a kind plus a follow" lists every person. The paragraph names the
    /// subject it walks from and the two arguments of the walk.
    #[test]
    fn the_asking_procedure_teaches_the_people_in_a_place_as_a_walk_from_the_place() {
        // The question, its worked call and the sentence after it are three
        // paragraphs; the walk is all of them up to the next question.
        let text = body("asking");
        let from = text
            .find("*Which people are in Springfield*")
            .expect("the asking procedure asks which people are in Springfield");
        let to = text[from..]
            .find("*Which visits")
            .expect("the next question follows");
        let paragraph = &text[from..from + to];
        for word in ["subject", "follow", "shape", "direction"] {
            assert!(
                names(paragraph, word),
                "the paragraph on the people in a place does not name `{word}`: {paragraph}"
            );
        }
    }

    /// **A project's list of columns has to hold the five shipped words, and the
    /// section says so.** A list that drops one is refused. The words are read
    /// off the engine, and the paragraph that teaches the key names each.
    #[test]
    fn the_projects_procedure_says_a_projects_columns_hold_every_shipped_word() {
        use jojobot_domain::memory::kinds;
        let paragraph = paragraph_naming(body("projects"), "A project may add statuses");
        for status in kinds::WORK_STATUSES {
            assert!(
                paragraph.contains(&format!("`{status}`")),
                "the paragraph on a project's columns does not name `{status}`: {paragraph}"
            );
        }
    }

    /// **Every `add_entity` and `capture` call a skill shows names the arguments
    /// its verb requires.** A call copied from a skill and refused as malformed
    /// teaches the reader that the skill is wrong. A call is an indented line
    /// that starts with the verb, with the deeper-indented lines that continue it.
    /// The corpus is counted first, so the case cannot pass over nothing.
    #[test]
    fn the_example_calls_in_every_skill_name_what_their_verb_requires() {
        let mut checked = 0;
        for skill in super::SKILLS {
            let mut lines = skill.body.lines().peekable();
            while let Some(line) = lines.next() {
                let Some(call) = line.strip_prefix("    ") else {
                    continue;
                };
                let (verb, needs): (&str, &[&[&str]]) = match call.split_whitespace().next() {
                    Some("add_entity") => (
                        "add_entity",
                        &[&["kind:"], &["handle:"], &["name:"], &["source:"]],
                    ),
                    Some("capture") => (
                        "capture",
                        &[&["subject:"], &["content:", "entries:", "ticks:"]],
                    ),
                    _ => continue,
                };
                let mut text = call.to_string();
                while let Some(next) = lines.peek().filter(|next| next.starts_with("        ")) {
                    text.push(' ');
                    text.push_str(next.trim());
                    lines.next();
                }
                for any_of in needs {
                    assert!(
                        any_of.iter().any(|argument| text.contains(argument)),
                        "a {verb} call in the {} skill does not name one of {any_of:?}: {text}",
                        skill.name
                    );
                }
                checked += 1;
            }
        }
        assert!(
            checked >= 10,
            "only {checked} example calls were found in the skills"
        );
    }
}

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

/// One shipped procedure: what it is called, when it is for, and the text.
pub(crate) struct Skill {
    /// The name a caller fetches it by.
    pub(crate) name: &'static str,
    /// **What decides whether to fetch it** — the only part of a skill that
    /// travels in the index, so it carries the whole weight of that choice.
    pub(crate) when_to_use: &'static str,
    /// The procedure itself.
    pub(crate) body: &'static str,
}

/// Every skill this build ships, in the order the index lists them.
pub(crate) const SKILLS: &[Skill] = &[
    Skill {
        name: "recommend",
        when_to_use: "Before you give the operator a real-world recommendation that they will \
                      act on. This includes where to eat, where to go, what to buy, and which \
                      product, place or service to choose.",
        body: RECOMMEND,
    },
    Skill {
        name: "rhythms",
        when_to_use: "When a loop is due, something has to be done by a day, or the operator \
                      asks to look ahead or back.",
        body: RHYTHMS,
    },
    Skill {
        name: "asking",
        when_to_use: "When you need something out of the graph and are about to work out how \
                      to ask for it. Also when a capability seems to be missing, because it \
                      usually is not.",
        body: ASKING,
    },
    Skill {
        name: "projects",
        when_to_use: "When you are recording a project's work, its decisions or its open \
                      questions.",
        body: PROJECTS,
    },
    Skill {
        name: "evidence",
        when_to_use: "Before you write anything that the operator will read later. This \
                      includes a claim about a person, a summary, a portrait, and a note.",
        body: EVIDENCE,
    },
];

/// The skill whose name matches, or `None`.
pub(crate) fn named(name: &str) -> Option<&'static Skill> {
    let wanted = name.trim();
    SKILLS.iter().find(|s| s.name == wanted)
}

/// The index: every skill by name and when-to-use, and **no bodies**.
pub(crate) fn index() -> serde_json::Value {
    SKILLS
        .iter()
        .map(|s| serde_json::json!({ "name": s.name, "when_to_use": s.when_to_use }))
        .collect()
}

const ASKING: &str = r#"# asking

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
next and names what exists; an empty result is a real answer and means nobody
wrote that down. **They are different**, and neither is fixed by asking again.
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
"#;

const RHYTHMS: &str = r#"# rhythms

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
have gone quiet as of today. Put a date in `as_of` to ask about another day.

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
if they have not said. The loop is not due before that day, and it is due on it.

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
"#;

const PROJECTS: &str = r#"# projects

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

## Where a piece of work stands

A piece of work is a `work` thing filed under its project. Four keys say where
it stands. Write them on the work as keys. A sentence cannot answer "what is
next".

`status` is where the work stands. It takes one of five words: `someday`,
`next`, `now`, `waiting` or `done`. A word outside the five is refused, and the
refusal names them. `owner` is the handle of the person who does the work.
`waiting_on` is the handle of the person whose move it is. `depends_on` lists
the handles of the work that must come first, separated by commas. A work item
or a project at `done` is not owed, and its dates stay on it, so do not clear a
date to quiet finished work.

    capture  subject: "work:sigma"  fields: {"status": "waiting", "waiting_on": "person:lisa", "depends_on": "work:phi, work:first-mix"}

A project may add statuses of its own. Write the whole list on the project, in
the order you want it, under `columns`. The work filed under that project may
then take any word on the list. A word outside the list is refused, and the
refusal names the list.

    capture  subject: "project:atlas"  fields: {"columns": "inbox, someday, next, now, waiting, done"}

Each question is then a read of one key. The first read lists what is next. The
second lists what waits on someone, and on whom. The third lists what stands on
the work named in `subject`.

    recall  kind: "work"  fields: [{"key": "status", "value": "next"}]
    recall  kind: "work"  fields: [{"key": "waiting_on"}]
    recall  subject: "work:phi"  follow: {"relation": "depends_on", "direction": "in"}

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
"#;

#[cfg(test)]
mod tests {
    use jojobot_domain::attention;

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
    /// schedule keys together.**
    ///
    /// A write that holds `cadence_days` without `advances_from`, or the
    /// reverse, is refused. A paragraph that says to put in whatever keys the
    /// operator gave sends a session into that refusal on the call it exists
    /// to describe, so the one paragraph that says to write the loop is also
    /// the one that says the two keys go as a pair.
    #[test]
    fn the_paragraph_that_says_to_write_the_loop_names_both_schedule_keys() {
        let text = body("rhythms");
        let paragraph = text
            .split("\n\n")
            .find(|paragraph| names(paragraph, "add_entity"))
            .expect("the rhythms procedure has a paragraph that names the verb that writes a loop");
        for key in [attention::CADENCE_DAYS, attention::ADVANCES_FROM] {
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
        "asked",
        "blocks",
        "columns",
        "depends_on",
        "done",
        "drafted",
        "next",
        "now",
        "owner",
        "promise",
        "promised_by",
        "regarding",
        "rhythms",
        "someday",
        "state",
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
            if identifier && !NOT_ARGUMENTS.contains(&word) {
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
        let index = super::index();
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
}

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
        when_to_use: "When a recurring loop is due, or when the operator asks to look forward \
                      over a period or back over one.",
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

    recall  view: "colleagues"     the identities here, and what each is for
    recall  view: "loops"          the recurring things, and what each last recorded

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
day the check-in happened. It has no default. The operator picks it for each
rhythm. A wrong pick is silent: every late check-in re-arms the loop it was
meant to settle.

`counts_from` is the date this cycle counts from. jojobot writes it. The
rhythm falls due `cadence_days` after it.

Write the loop with `add_entity` as soon as the operator names one, and put in
the keys the operator gave you. A key the operator has not chosen yet is a
question you ask after the loop exists. It is never a reason to wait.

A loop that holds part of a schedule tracks from the day it is written and
comes back overdue with the fields it does hold, which is what puts the
missing key in front of the operator. A loop nobody wrote tracks nothing, and
nothing later reports that it is absent.

## How to find what is due

Call `recall` with `kind: "rhythm"` and `overdue: {}`. You get the loops that
have gone quiet as of today. Put a date in `as_of` to ask about another day.

The answer says which day it used. Read it.

A rhythm that holds only part of a schedule comes back overdue, with the
fields it does hold. Ask the operator for the key it lacks. Do not guess one.

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

`snoozed` — the cycle is not consumed. The rhythm comes back at its own date.

A refusal is not a run. Ask the operator whether the cycle moves on, which is
`skipped`, or comes back, which is `snoozed`. A refusal recorded as `ran` says
the work was done, and nothing later can tell it from work that was done.

jojobot writes `outcome`, `last_check_in` and `counts_from` itself. Do not
compute them and do not send them. Put what the check measured in `fields`.

A loop whose last run already happened, before this session opened it, is
opened the same way: capture a check-in on it, dated the day it last ran, and
jojobot works the rest of the schedule out from there. Give the loop its
cadence first — `cadence_days` and `advances_from` are the operator's word and
no check-in can state them, so a loop that holds neither is refused until it
does. A snooze does not open a loop, because it moves nothing and there is
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
These are the operator's decisions and the operator's data. This procedure
is only how you offer a rhythm and how you close it.
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
}

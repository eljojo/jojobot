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

There are two kinds. A timed rhythm is due on a cadence and has a last-run
stamp. A weave has no stamp and starts when its trigger occurs.

## Keep the pressure low

Offer a rhythm in one line at the start of a session. Then do the work the
operator opened the session for. If the operator does not take the offer,
stop.

A refusal counts as a run. Record the stamp and continue. Do not offer the
same rhythm again in the same window.

Remove a rhythm that the operator finds stressful. Remove it with the
operator. Add a new rhythm with the operator. Do not add one alone.

## How to run a rhythm

Read the stamps. Offer each timed rhythm that is due, in one line. Record the
stamp when the operator runs it and when the operator refuses it. Test each
weave's trigger and start the ones that match.

## The two timed shapes

A forward rhythm gives a short summary of the period ahead. State what is
fixed, what conflicts, and the one or two decisions the operator must make.
Do not create work. Do not add a date that the operator did not give.

A backward rhythm reviews the period that ended. State what got attention,
what stopped, and what did not start. It is a conversation. Do not produce a
document, a count or a dashboard.

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

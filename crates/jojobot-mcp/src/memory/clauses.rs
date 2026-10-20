//! **The defining clauses two verbs say in one voice.**
//!
//! `capture` and `update_fact` both take `stale_after` and `happened_at`, and
//! each described them in its own words, which drifted. The clause that
//! DEFINES the argument is written here once and spliced into both. **Only the
//! definition is shared**: what a verb teaches around it (when to leave the day
//! off, what learning it late means) stays in that verb's own description.
//!
//! A clause is a macro that expands to a string literal, so `concat!` can splice
//! it into an argument's `#[schemars(description = ...)]` attribute; an argument
//! written as a doc comment cannot take a spliced text. The const beside each
//! macro is the same text for a test to name.

/// The defining clause of `stale_after`.
macro_rules! stale_after_clause {
    () => {
        concat!(
            "**The day after which this reading stops being good**, `YYYY-MM-DD`.\n\n",
            "It is a claim about jojobot's knowledge, not the world: a\n",
            "pass that runs out on a date is a claim about the world and belongs in\n",
            "the content. Past the day a read SAYS SO and **nothing else happens**:\n",
            "no sweep, no reminder, nobody is coming to check it."
        )
    };
}
pub(crate) use stale_after_clause;

/// The defining clause of `happened_at`.
macro_rules! happened_at_clause {
    () => {
        concat!(
            "**The day the thing this claim is about HAPPENED**, `YYYY-MM-DD` \u{2014}\n",
            "a different question from `recorded_at`, which is about the claim."
        )
    };
}
pub(crate) use happened_at_clause;

/// [`stale_after_clause!`] as a value, for a test to name.
#[cfg(test)]
pub(crate) const STALE_AFTER_CLAUSE: &str = stale_after_clause!();

/// [`happened_at_clause!`] as a value, for a test to name.
#[cfg(test)]
pub(crate) const HAPPENED_AT_CLAUSE: &str = happened_at_clause!();

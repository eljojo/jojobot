//! The words a name is made of that two things could share.
//!
//! One definition for every reader of it: the ledger of what a session was
//! shown, and the answer to a search that found nothing.

use std::collections::BTreeSet;

/// The fewest letters a word needs before two things sharing it is worth a
/// question. Shorter words are articles and connectives, which two unrelated
/// things share.
pub const MIN_WORD: usize = 4;

/// **The words a name is made of that two things could share.** Lowercased, at
/// least [`MIN_WORD`] letters, and never a bare number.
///
/// There is deliberately no stoplist and no rarity test. Over 188 real
/// creations neither removed a single pair, so a threshold would be invented. A
/// run showing a common word that fires a false question is what would justify
/// adding rarity (how many other things of the kind carry the word), and it
/// should be added then, with the run as its sample.
///
/// **The one word removed is not a rarity rule.** A run did show a false
/// question: two things under one project shared the project's own slug. That
/// word is not common and it is not rare; it is given by the parent, so it says
/// nothing about whether the two are the same thing. Callers remove the parent's
/// slug words for a thing under that parent and for nothing else, so no word is
/// dropped for how often it occurs.
pub fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_WORD)
        .filter(|word| word.chars().any(char::is_alphabetic))
        .map(str::to_string)
        .collect()
}

/// **The things whose name, slug or aliases share a word with `text`**, each
/// with the words it shares, the most shared first and then by handle.
///
/// A word a thing wears only because of the project it sits under is the
/// parent's, not the thing's: the parent's slug words are removed from it before
/// the comparison, and from nothing else. Things archived or folded into another
/// are not offered. `entities` is whatever the caller already narrowed, so a
/// kind asked for is the caller's to apply.
pub fn sharing_a_word<'a>(
    text: &str,
    entities: &'a [super::Entity],
) -> Vec<(&'a super::Entity, Vec<String>)> {
    let asked = words(text);
    if asked.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(&super::Entity, Vec<String>)> = entities
        .iter()
        .filter(|entity| entity.browsable())
        .filter_map(|entity| {
            let given: BTreeSet<String> = entity
                .parent
                .as_ref()
                .map(|parent| words(parent.slug()))
                .unwrap_or_default();
            let theirs: BTreeSet<String> = entity
                .labels()
                .into_iter()
                .chain(std::iter::once(entity.id.slug()))
                .flat_map(words)
                .filter(|word| !given.contains(word))
                .collect();
            let shared: Vec<String> = asked.intersection(&theirs).cloned().collect();
            (!shared.is_empty()).then_some((entity, shared))
        })
        .collect();
    found.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.id.cmp(&b.0.id)));
    found
}

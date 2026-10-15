//! The words a name is made of that two things could share.
//!
//! One definition for every reader of it: the ledger of what a session was
//! shown, and the answer to a search that found nothing.

use std::collections::BTreeSet;

/// The fewest letters a word needs before two things sharing it is worth a
/// question. Shorter words are articles and connectives, which two unrelated
/// things share.
pub const MIN_WORD: usize = 4;

/// **Function words that are four letters or more, and so pass [`MIN_WORD`]**,
/// kept sorted so a lookup can bisect. Used by the shared-word reason of the
/// name guard and by nothing else: search and the notes that compare claims keep
/// the rarity rule alone, so this list changes what they match not at all.
///
/// **Why rarity alone is not enough.** A word is evidence only when few other
/// things of the kind carry it, and in a store of two things "with" is carried
/// once, so it counts as rare. "Pickles with Mustard" would then be refused
/// beside "Bread with Butter". A fixed list of the words that join other words
/// says so in any store.
const STOP_WORDS: &[&str] = &[
    "about", "above", "after", "again", "also", "among", "around", "because", "been", "before",
    "being", "below", "between", "both", "cannot", "could", "does", "doing", "down", "during",
    "each", "either", "else", "from", "further", "have", "having", "here", "hers", "into",
    "itself", "just", "might", "more", "most", "must", "never", "only", "other", "over", "same",
    "shall", "should", "since", "some", "such", "than", "that", "their", "them", "then", "there",
    "these", "they", "this", "those", "through", "thus", "under", "until", "upon", "very", "were",
    "what", "when", "where", "whether", "which", "while", "whom", "whose", "will", "with",
    "within", "without", "would", "your", "yours",
];

/// Whether `word` is a stop word, whatever its case.
pub fn is_stop_word(word: &str) -> bool {
    STOP_WORDS
        .binary_search(&word.to_lowercase().as_str())
        .is_ok()
}

/// **A name's words and the words inside them, for the shared-word reason of
/// the name guard.** [`words`] is unchanged and keeps `KwikEMart` whole; this
/// adds the parts a capital starts, so the name shares `mart` with `Kwik E
/// Mart`. A run of capitals is one part (`HTMLParser` is `html` and `parser`),
/// a part is a word in its own right (4 letters or more), and a name with no
/// capital inside a word reads exactly as `words` reads it.
pub fn camel_words(text: &str) -> BTreeSet<String> {
    let mut found = words(text);
    for segment in text.split(|c: char| !c.is_alphanumeric()) {
        let chars: Vec<char> = segment.chars().collect();
        let mut part = String::new();
        for (at, &c) in chars.iter().enumerate() {
            let after_lower =
                at > 0 && (chars[at - 1].is_lowercase() || chars[at - 1].is_numeric());
            let before_lower = chars.get(at + 1).is_some_and(|next| next.is_lowercase());
            let starts_part = c.is_uppercase()
                && !part.is_empty()
                && (after_lower || (chars[at - 1].is_uppercase() && before_lower));
            if starts_part {
                found.extend(words(&part));
                part.clear();
            }
            part.push(c);
        }
        found.extend(words(&part));
    }
    found
}

/// How many other things of one kind must carry a word for the word to be
/// common: a common word is never evidence that two things are one. See
/// [`Rarity`].
///
/// **Its sample:** 4 false shared-word questions in one live session,
/// 2026-10-13; carriers counted over 49 of 97 works. The words were carried by
/// 5, 5, 3 and 1 other works. A limit of 3 silences the first three and leaves
/// the fourth.
///
/// **What this test cannot reach:** the fourth word had one other carrier, so it
/// was rare and the question was false all the same. Rarity cuts the class of
/// common words and leaves that one. It is a known limit of the rule.
pub const COMMON_AT: usize = 3;

/// **The words a name is made of that two things could share.** Lowercased, at
/// least [`MIN_WORD`] letters, and never a bare number.
///
/// There is deliberately no stoplist: a stoplist is a list of names, and a word
/// that is common in one store is rare in another. How common a word is comes
/// from the store itself, as [`Rarity`] counts it, and that is the only test of
/// how often a word occurs.
///
/// **The one word removed here is not a rarity rule.** Two things under one
/// project share the project's own slug. That word is given by the parent, so it
/// says nothing about whether the two are the same thing. Callers remove the
/// parent's slug words for a thing under that parent and for nothing else.
pub fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_WORD)
        .filter(|word| word.chars().any(char::is_alphabetic))
        .map(str::to_string)
        .collect()
}

/// **How many things of each kind carry each word**, counted over the
/// browsable things of a store. It is the one rule for whether a shared word is
/// evidence: a word counts only when fewer than [`COMMON_AT`] other things of
/// that kind carry it, so a word that many works of a store wear ("with",
/// "from") is never the reason two works are asked about.
///
/// A thing carries a word when its name, an alias or its slug holds it, the
/// same words the question compares, a camelCase word counting through its parts
/// (see [`counted_for`]). **The count is over the OTHER things**:
/// the caller leaves out the thing it asks about, so a creation and a search
/// that has no thing yet count alike.
///
/// **A word nobody carries is not rare.** An empty count comes from a store that
/// could not be read, and a question must not be asked on a count that was
/// never taken.
#[derive(Debug, Default, Clone)]
pub struct Rarity {
    carriers: std::collections::BTreeMap<(String, String), usize>,
}

impl Rarity {
    /// Count the words of `entities`, kind by kind. Archived and folded things
    /// are not counted, as they are not offered.
    pub fn of(entities: &[super::Entity]) -> Self {
        let mut carriers: std::collections::BTreeMap<(String, String), usize> = Default::default();
        for entity in entities.iter().filter(|entity| entity.browsable()) {
            let Some(kind) = entity.id.kind() else {
                continue;
            };
            for word in counted_for(entity) {
                *carriers
                    .entry((kind.as_token().to_string(), word))
                    .or_default() += 1;
            }
        }
        Self { carriers }
    }

    /// How many things of `kind` carry `word`.
    pub fn carriers(&self, kind: &str, word: &str) -> usize {
        self.carriers
            .get(&(kind.to_string(), word.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// **Whether `word` counts as shared evidence between two things of `kind`.**
    pub fn is_rare(&self, kind: &str, word: &str) -> bool {
        (1..COMMON_AT).contains(&self.carriers(kind, word))
    }
}

/// **The words one thing is counted as carrying**: the words it carries, and the
/// parts of a camelCase word among them, read through [`camel_words`] as the
/// shared-word reason reads a name. A word that reason finds in two names is
/// counted here, so a `KwikEMart` carries `mart`. Search keeps [`carried_by`],
/// which reads a name whole.
fn counted_for(entity: &super::Entity) -> BTreeSet<String> {
    entity
        .labels()
        .into_iter()
        .chain(std::iter::once(entity.id.slug()))
        .flat_map(|name| camel_words(&name))
        .collect()
}

/// The words one thing carries: its labels and its slug.
fn carried_by(entity: &super::Entity) -> BTreeSet<String> {
    entity
        .labels()
        .into_iter()
        .chain(std::iter::once(entity.id.slug()))
        .flat_map(words)
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
    let rarity = Rarity::of(entities);
    let mut found: Vec<(&super::Entity, Vec<String>)> = entities
        .iter()
        .filter(|entity| entity.browsable())
        .filter_map(|entity| {
            let given: BTreeSet<String> = entity
                .parent
                .as_ref()
                .map(|parent| words(parent.slug()))
                .unwrap_or_default();
            let theirs: BTreeSet<String> = carried_by(entity)
                .into_iter()
                .filter(|word| !given.contains(word))
                .collect();
            let kind = entity.id.kind()?.as_token();
            let shared: Vec<String> = asked
                .intersection(&theirs)
                .filter(|word| rarity.is_rare(kind, word))
                .cloned()
                .collect();
            (!shared.is_empty()).then_some((entity, shared))
        })
        .collect();
    found.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.id.cmp(&b.0.id)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Entity, EntityId};

    const LETTERS: [&str; 4] = ["alpha", "omega", "kappa", "sigma"];

    fn entity(id: &str, name: &str) -> Entity {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let id = EntityId(id.into());
        Entity {
            folded_names: Vec::new(),
            kind: id.kind().expect("test ids are well-formed"),
            id,
            name: name.into(),
            aliases: Vec::new(),
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        }
    }

    /// A store with `COMMON_AT` works that carry `with`, one work and one place
    /// that carry `wharf`, and one work folded into another.
    fn store() -> Vec<Entity> {
        let mut held: Vec<Entity> = (0..COMMON_AT)
            .map(|at| entity(&format!("work:{}", LETTERS[at]), "Row with batter"))
            .collect();
        held.push(entity("work:delta", "Wharf Road"));
        held.push(entity("place:wonder-wharf", "Wharf Lane"));
        let mut folded = entity("work:gamma", "Wharf Road Again");
        folded.merged_into = Some(EntityId("work:delta".into()));
        held.push(folded);
        held
    }

    /// **`words` is not changed by the new reason**: a camelCase name stays one
    /// word, so search tokenizes exactly as it did. `camel_words` is a separate
    /// function that only the shared-word reason calls, and it reads the name as
    /// its whole words and the words inside them: `KwikEMart` shares `mart` with
    /// `Kwik E Mart`. Snake case already splits at the underscore. A part is a
    /// word in its own right (4 letters or more), a run of capitals is one part
    /// (`HTMLParser`), and a part never straddles a capital.
    #[test]
    fn camel_words_reads_the_parts_of_a_camel_case_name_and_words_does_not() {
        let got = camel_words("KwikEMart and snake_case plus HTMLParser");
        for expected in [
            "kwikemart",
            "kwik",
            "mart",
            "snake",
            "case",
            "htmlparser",
            "html",
            "parser",
        ] {
            assert!(
                got.contains(expected),
                "{expected} is a word of the name: {got:?}"
            );
        }
        assert!(
            !got.contains("and"),
            "a short word is no name word: {got:?}"
        );
        assert!(
            !got.contains("emart"),
            "no part straddles a capital: {got:?}"
        );
        assert_eq!(
            words("KwikEMart"),
            ["kwikemart".to_string()].into_iter().collect(),
            "words keeps a camel case name whole"
        );
        assert_eq!(
            camel_words("plain lowercase words"),
            words("plain lowercase words"),
            "a name with no inner capitals reads as words reads it"
        );
    }

    /// The list is bisected, so it must stay sorted and free of repeats.
    #[test]
    fn the_stop_words_stay_sorted() {
        assert!(STOP_WORDS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            STOP_WORDS
                .iter()
                .all(|word| word.chars().count() >= MIN_WORD)
        );
    }

    /// **A stop word is never evidence that two names mean one thing.** Rarity
    /// cannot say so alone: in a store of two things "with" is carried once, so
    /// it counts as rare. The list is closed and fixed, so these pin the words
    /// that matter and a name word that must stay out of it.
    #[test]
    fn a_stop_word_is_named_and_a_name_word_is_not() {
        for stop in ["with", "from", "that", "this", "their", "about", "which"] {
            assert!(is_stop_word(stop), "{stop} is a stop word");
        }
        for name in ["plant", "tavern", "nuclear", "springfield", "kwik"] {
            assert!(!is_stop_word(name), "{name} names something");
        }
        assert!(is_stop_word("WITH"), "case does not matter");
    }

    /// **A word is rare when no more than the limit of things of its kind carry
    /// it, counted per kind.** `with` is carried by one more than the limit among
    /// works; `wharf` by one work and one place, so each kind counts it once. The
    /// negatives sit beside the positive: a word nobody carries is not rare, and
    /// a thing folded into another is not counted.
    #[test]
    fn a_word_is_rare_when_few_things_of_its_kind_carry_it() {
        let rarity = Rarity::of(&store());
        assert_eq!(rarity.carriers("work", "with"), COMMON_AT);
        assert!(!rarity.is_rare("work", "with"));
        assert_eq!(
            rarity.carriers("work", "wharf"),
            1,
            "the folded work is not counted"
        );
        assert!(rarity.is_rare("work", "wharf"));
        assert!(rarity.is_rare("place", "wharf"));
        assert!(
            !rarity.is_rare("work", "nobody"),
            "a word nobody carries is not rare",
        );
        assert!(
            !Rarity::default().is_rare("work", "wharf"),
            "an empty count makes no word rare",
        );
    }

    /// **The search answer and the count are one rule**: a thing sharing only a
    /// common word is not offered, and one sharing a rare word is.
    #[test]
    fn a_thing_sharing_only_a_common_word_is_not_offered() {
        let held = store();
        let common = sharing_a_word("with", &held);
        assert!(common.is_empty(), "{common:?}");
        let rare = sharing_a_word("wharf", &held);
        assert_eq!(rare.len(), 2, "{rare:?}");
    }

    /// **The limit is three other carriers.** Two others carrying a word leave
    /// it rare and three make it common. The value is a design decision with a
    /// sample behind it, so it is pinned as the number and not through the
    /// constant.
    #[test]
    fn three_other_carriers_make_a_word_common_and_two_do_not() {
        let works = |count: usize| -> Vec<Entity> {
            (0..count)
                .map(|at| entity(&format!("work:{}", LETTERS[at]), "Row with batter"))
                .collect()
        };
        assert!(Rarity::of(&works(2)).is_rare("work", "with"));
        assert!(!Rarity::of(&works(3)).is_rare("work", "with"));
    }
}

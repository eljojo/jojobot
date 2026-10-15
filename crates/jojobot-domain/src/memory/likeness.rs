//! **Ways two names of one kind can mean one thing without being a typo
//! apart** (card 2257): a known nickname, and two rare words in common. The
//! rarity rule behind the second lives in [`super::words`] because it is the
//! same word rule search applies.
//!
//! Pure: no I/O, no clock. The nickname table is a vendored flat file read at
//! compile time, so there is no network and no state.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use super::words::{camel_words, is_stop_word};

/// The vendored table: one family per line, a full name, a colon, then the
/// short forms that go with it. Public data, read at compile time.
const NICKNAMES: &str = include_str!("../../data/nicknames.txt");

type Families = BTreeMap<String, BTreeSet<String>>;

fn families() -> &'static Families {
    static TABLE: OnceLock<Families> = OnceLock::new();
    TABLE.get_or_init(|| {
        NICKNAMES
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split_once(':'))
            .map(|(full, shorts)| {
                (
                    full.trim().to_lowercase(),
                    shorts
                        .split_whitespace()
                        .map(str::to_lowercase)
                        .collect::<BTreeSet<String>>(),
                )
            })
            .fold(Families::new(), |mut table, (full, shorts)| {
                table.entry(full).or_default().extend(shorts);
                table
            })
    })
}

/// The lowercase alphanumeric words of a name.
fn name_words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether one word is a short form of the other: the table lists the short
/// form under the full name. Two full names that share a short form are not
/// linked, and neither are two short forms.
fn short_form_of(short: &str, full: &str) -> bool {
    families()
        .get(full)
        .is_some_and(|shorts| shorts.contains(short))
}

/// **Whether two names are one through a known nickname.** The names have the
/// same number of words, every word pair agrees or is a short form of the
/// other's, and at least one pair is a short form. A different surname, a name
/// with more words and the same name twice are not.
pub fn same_through_nicknames(a: &str, b: &str) -> bool {
    let (a, b) = (name_words(a), name_words(b));
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    let mut linked = false;
    for (x, y) in a.iter().zip(&b) {
        if x == y {
            continue;
        }
        if short_form_of(x, y) || short_form_of(y, x) {
            linked = true;
        } else {
            return false;
        }
    }
    linked
}

/// **How many shared words make two names of one kind alike.** One shared word
/// is a family name or a category (`Simpson`, `bike`): measured over the rooms'
/// things and the roster, every pair it flagged was two different things, and
/// no pair sharing two words was. A one-line change if the answer changes.
pub const SHARED_WORDS_NEEDED: usize = 2;

/// **The words two names share that are evidence they mean one thing**: words
/// of 4 letters or more that are no stop word and that `is_rare` says few other
/// things of the kind carry, read through [`super::words::camel_words`] so
/// `KwikEMart` shares `mart` with `Kwik E Mart`.
pub fn shared_words(a: &str, b: &str, is_rare: &dyn Fn(&str) -> bool) -> Vec<String> {
    let (a, b) = (camel_words(a), camel_words(b));
    a.intersection(&b)
        .filter(|word| !is_stop_word(word) && is_rare(word))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Two names are one through a nickname when every word agrees or is a
    /// short form of the other's, and at least one word is.** The negatives sit
    /// beside the positives: a different surname, the same name twice, and two
    /// full names that share a short form each.
    #[test]
    fn a_name_is_its_own_nickname_form_and_nothing_else() {
        for (short, full) in [
            ("Bart", "Bartholomew"),
            ("Bartholomew", "Bart"),
            ("Bart Simpson", "Bartholomew Simpson"),
            ("Marge", "Margaret"),
            ("Chris", "Christopher"),
            ("Chris", "Christine"),
            ("Moe", "Maurice"),
            ("Lisa", "Elizabeth"),
        ] {
            assert!(same_through_nicknames(short, full), "{short} / {full}");
        }
        for (a, b) in [
            ("Bart", "Margaret"),
            ("Bart Simpson", "Bartholomew Smithers"),
            ("Bart", "Bart"),
            ("Homer", "Homer"),
            ("Christopher", "Christine"),
            ("Bart Simpson", "Bartholomew"),
        ] {
            assert!(!same_through_nicknames(a, b), "{a} / {b}");
        }
    }

    /// **The words two names share are the rare, real ones.** A stop word, a
    /// short word and a word `is_rare` rejects are left out, and the camelCase
    /// parts of a name count.
    #[test]
    fn shared_words_are_the_rare_real_words_two_names_have_in_common() {
        let any = |_: &str| true;
        assert_eq!(
            shared_words("Springfield Nuclear Plant", "Nuclear Plant Tours", &any),
            vec!["nuclear".to_string(), "plant".to_string()]
        );
        assert_eq!(
            shared_words("Pickles With Mustard", "Bread With Mustard", &any),
            vec!["mustard".to_string()],
            "a stop word is not shared"
        );
        assert_eq!(
            shared_words("Kwik E Mart", "KwikEMart", &any),
            vec!["kwik".to_string(), "mart".to_string()],
            "the parts of a camelCase name count"
        );
        assert!(
            shared_words("Springfield Nuclear Plant", "Nuclear Plant Tours", &|w| w
                != "plant")
            .len()
                < SHARED_WORDS_NEEDED,
            "a word the store calls common is dropped"
        );
        assert_eq!(
            SHARED_WORDS_NEEDED, 2,
            "one word is a family name or a category"
        );
    }
}

//! **Ways two names of one kind can mean one thing without being a typo
//! apart** (card 2257): a known nickname, and a likeness of sound or spelling.
//! The shared-word reason lives in [`super::words`] because it is the same
//! word rule search applies.
//!
//! Pure: no I/O, no clock. The nickname table is a vendored flat file read at
//! compile time, so there is no network and no state.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use super::words::{camel_words, is_stop_word};

/// The similarity at or above which two names of one kind are alike. Set from
/// the labelled pairs in the tests below.
pub const TRIGRAM_THRESHOLD: f64 = 0.6;

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

/// The trigrams of a name, as the three-letter windows of each word padded with
/// two spaces before and one after (the way pg_trgm reads a word).
fn trigrams(name: &str) -> BTreeSet<[char; 3]> {
    let mut found = BTreeSet::new();
    for word in name_words(name) {
        let padded: Vec<char> = [' ', ' ']
            .into_iter()
            .chain(word.chars())
            .chain([' '])
            .collect();
        for window in padded.windows(3) {
            found.insert([window[0], window[1], window[2]]);
        }
    }
    found
}

/// **Trigram similarity of two names**, from 0 to 1: the trigrams they share
/// over the trigrams either holds.
pub fn trigram_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (trigrams(a), trigrams(b));
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / union as f64
}

/// The Soundex code of one word: its first letter and three digits.
fn soundex(word: &str) -> String {
    let digit = |c: char| match c {
        'b' | 'f' | 'p' | 'v' => Some('1'),
        'c' | 'g' | 'j' | 'k' | 'q' | 's' | 'x' | 'z' => Some('2'),
        'd' | 't' => Some('3'),
        'l' => Some('4'),
        'm' | 'n' => Some('5'),
        'r' => Some('6'),
        _ => None,
    };
    let letters: Vec<char> = word.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let Some(&first) = letters.first() else {
        return String::new();
    };
    let mut code = String::from(first.to_ascii_uppercase());
    let mut last = digit(first);
    for &c in &letters[1..] {
        let d = digit(c);
        if let Some(d) = d
            && Some(d) != last
        {
            code.push(d);
        }
        // h and w do not break a run of the same digit; a vowel does.
        if !matches!(c, 'h' | 'w') {
            last = d;
        }
        if code.len() == 4 {
            break;
        }
    }
    while code.len() < 4 {
        code.push('0');
    }
    code
}

/// **A phonetic code for a whole name**: the Soundex code of each word, in
/// order, joined by a space.
pub fn phonetic_code(name: &str) -> String {
    name_words(name)
        .iter()
        .map(|word| soundex(word))
        .collect::<Vec<_>>()
        .join(" ")
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

/// Whether two names are alike by trigram similarity.
pub fn looks_alike(a: &str, b: &str) -> bool {
    trigram_similarity(a, b) >= TRIGRAM_THRESHOLD
}

/// Whether two names sound alike: the same phonetic code, word for word.
pub fn sounds_alike(a: &str, b: &str) -> bool {
    let code = phonetic_code(a);
    !code.is_empty() && code == phonetic_code(b)
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

    /// Labelled pairs from the roster: names that mean one thing spelled
    /// apart, and names that mean two things. The thresholds are the numbers
    /// that keep every pair in its column here, so a change to either moves a
    /// pair across and this fails.
    const ALIKE: &[(&str, &str)] = &[
        ("Kwik E Mart", "Kwik-E-Mart"),
        (
            "Springfield Nuclear Plant",
            "Springfield Nuclear Power Plant",
        ),
        ("Moe's Tavern", "Moes Tavern"),
        ("Krusty Burger", "Krusty Burgers"),
        ("Bartholomew Simpson", "Bartholomew Simpsen"),
    ];
    const APART: &[(&str, &str)] = &[
        ("Bart Simpson", "Lisa Simpson"),
        ("Moe's Tavern", "Krusty Burger"),
        ("Springfield Mall", "Springfield Elementary"),
        ("Homer Simpson", "Marge Simpson"),
        ("Apu Nahasapeemapetilon", "Ned Flanders"),
        ("Plant", "Springfield Nuclear Plant"),
    ];

    #[test]
    fn the_trigram_threshold_keeps_alike_names_and_apart_names_in_their_columns() {
        for (a, b) in ALIKE {
            let score = trigram_similarity(a, b);
            assert!(score >= TRIGRAM_THRESHOLD, "{a} / {b} scored {score}");
        }
        for (a, b) in APART {
            let score = trigram_similarity(a, b);
            assert!(score < TRIGRAM_THRESHOLD, "{a} / {b} scored {score}");
        }
        assert_eq!(trigram_similarity("", ""), 0.0);
        assert_eq!(trigram_similarity("Alpha", "Alpha"), 1.0);
    }

    #[test]
    fn the_phonetic_code_reads_a_name_by_ear() {
        assert_eq!(soundex("robert"), "R163");
        assert_eq!(soundex("rupert"), "R163");
        assert_eq!(soundex("tymczak"), "T522");
        assert_eq!(soundex("pfister"), "P236");
        assert_eq!(phonetic_code("Homer Simpson"), "H560 S512");
        assert_eq!(
            phonetic_code("Homer Sympson"),
            phonetic_code("Homer Simpson")
        );
        assert_ne!(phonetic_code("Bart Simpson"), phonetic_code("Lisa Simpson"));
        assert_eq!(phonetic_code(""), "");
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

    /// Measured over the rooms' things and the roster's handles, and held out
    /// of the guard until the operator rules: the same labelled pairs the
    /// thresholds were set from.
    #[test]
    fn looking_alike_and_sounding_alike_read_the_labelled_pairs() {
        for (a, b) in ALIKE {
            assert!(looks_alike(a, b), "{a} / {b}");
        }
        for (a, b) in APART {
            assert!(!looks_alike(a, b), "{a} / {b}");
        }
        assert!(sounds_alike("Homer Simpson", "Homer Sympson"));
        assert!(!sounds_alike("Bart Simpson", "Lisa Simpson"));
        assert!(!sounds_alike("", ""));
    }
}

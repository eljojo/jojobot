//! **A number after a slug is read only when its digits end the segment**, and a
//! chain of numbered segments is one address. `/2abc` is words after a
//! collection, never member 2; `/2/3` is the nested member, never member 2 with
//! `/3` left behind.

use super::*;
use crate::memory::testing::InMemoryMemory;

/// The collection the cases write, a numbered topic of decisions.
const ROOT: &str = "topic:rules-springfield";

/// The tails of the handles a text names after [`ROOT`], with the root cut off.
fn tails(written: &str) -> Vec<String> {
    let _booted = InMemoryMemory::booted();
    let text = format!("see @{ROOT}{written} then");
    named(&text)
        .iter()
        .map(|handle| handle.as_str().replacen(ROOT, "", 1))
        .collect()
}

/// A digit run that letters or a hyphen follow is no number: the mention is the
/// collection and the rest is words.
#[test]
fn digits_followed_by_letters_or_a_hyphen_are_no_number() {
    assert_eq!(tails("/2abc"), [""]);
    assert_eq!(tails("/2-x"), [""]);
    assert_eq!(tails("/2/3abc"), ["/2"]);
}

/// Where digits do end the segment the address is read: before punctuation,
/// before a space, and as a chain.
#[test]
fn digits_that_end_the_segment_are_a_number_and_a_chain_is_one_address() {
    assert_eq!(tails(","), [""]);
    assert_eq!(tails("/2,"), ["/2"]);
    assert_eq!(tails("/2."), ["/2"]);
    assert_eq!(tails("/2"), ["/2"]);
    assert_eq!(tails("/2/3"), ["/2/3"]);
    assert_eq!(tails("/2/3."), ["/2/3"]);
    assert_eq!(tails("/2/"), ["/2"]);
}

/// What is code is not read, whatever follows the slash.
#[test]
fn a_handle_in_code_is_not_read() {
    let _booted = InMemoryMemory::booted();
    let text = format!("code `@{ROOT}/2abc` and `@{ROOT}/2/3`");
    assert!(named(&text).is_empty());
}

/// The roster-aware reading ends a mention at the last slash where the roster
/// does not say the address holds, and keeps a held nested address whole.
#[test]
fn a_held_nested_address_is_one_mention_and_an_unheld_one_ends_at_the_slash() {
    let _booted = InMemoryMemory::booted();
    let text = format!("see @{ROOT}/2/3 now");
    let at = text.find('@').expect("a mention");
    let nested = format!("{ROOT}/2/3");
    let held = |id: &EntityId| id.as_str() == nested;
    let Attempt::Mention(handle, end) = mention_among(&text, at, &held) else {
        panic!("a held address is a mention");
    };
    assert_eq!(handle.as_str(), nested);
    assert_eq!(&text[..end], format!("see @{nested}"));
    let Attempt::Mention(handle, end) = mention_among(&text, at, &|_| false) else {
        panic!("the collection is still a mention");
    };
    assert_eq!(handle.as_str(), format!("{ROOT}/2"));
    assert_eq!(&text[..end], format!("see @{ROOT}/2"));
}

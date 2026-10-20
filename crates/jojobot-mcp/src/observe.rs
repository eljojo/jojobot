//! **What a call came to, told to whatever watches this process.**
//!
//! The handler counts every call it answers in the store, under the bot and the
//! day (see [`crate::calls`]). A process can also keep its own numbers, for a
//! scraper to read, and those come from the SAME event: the observer is told at
//! the one point where the call is counted, so the two counts cannot disagree
//! about which calls happened (rule 498).
//!
//! What the observer is told is closed: the verb's name, which the router
//! bounds, a word from [`CallEnd`], and how long the call took. No argument, no
//! handle and no text a caller sent reaches it.

use std::time::Duration;

use jojobot_domain::usage::CallOutcome;

use crate::answer::FixBy;

/// **The crate's version**, which moves only when the crate does.
pub const VERSION: &str = crate::SERVER_VERSION;

/// **The build this process runs**: the identifier that tells one deployment
/// from the next, as `ping` serves it.
pub const BUILD: &str = env!("JOJOBOT_BUILD");

/// **How a call ended**, in the words a scraper's label holds: answered, or
/// refused with the word that says who fixes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallEnd {
    /// The verb answered.
    Answered,
    /// Refused, and the same call may succeed.
    Retry,
    /// Refused, and the call itself must change.
    Change,
    /// Refused, and only a person can fix it.
    Person,
    /// Refused with no word, or with a word that is none of the three.
    Refused,
}

impl CallEnd {
    /// Every word a call can end with, so a reader can hold a label to the set.
    pub const ALL: [CallEnd; 5] = [
        CallEnd::Answered,
        CallEnd::Retry,
        CallEnd::Change,
        CallEnd::Person,
        CallEnd::Refused,
    ];

    /// The word on the label.
    pub fn as_str(self) -> &'static str {
        match self {
            CallEnd::Answered => "answered",
            CallEnd::Retry => FixBy::Retry.as_token(),
            CallEnd::Change => FixBy::Change.as_token(),
            CallEnd::Person => FixBy::Person.as_token(),
            CallEnd::Refused => "refused",
        }
    }

    /// **The end of a call as the store counts it.** The word a refusal carries
    /// is text in an answer, so it is matched against the three the build
    /// writes, and anything else is a plain refusal.
    pub(crate) fn of(outcome: &CallOutcome) -> Self {
        match outcome {
            CallOutcome::Landed => CallEnd::Answered,
            CallOutcome::Refused { fix_by } => match fix_by.as_deref() {
                Some(word) if word == FixBy::Retry.as_token() => CallEnd::Retry,
                Some(word) if word == FixBy::Change.as_token() => CallEnd::Change,
                Some(word) if word == FixBy::Person.as_token() => CallEnd::Person,
                _ => CallEnd::Refused,
            },
        }
    }
}

/// **Told once for every call the store counts**, after its answer is known.
pub trait CallObserver: Send + Sync {
    /// `verb` is the name the call is counted under, which the router bounds:
    /// a name it serves, or `unserved`.
    fn call_answered(&self, verb: &str, end: CallEnd, elapsed: Duration);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_keeps_its_word_and_a_word_the_build_never_writes_is_a_plain_refusal() {
        let refused = |word: Option<&str>| {
            CallEnd::of(&CallOutcome::Refused {
                fix_by: word.map(str::to_string),
            })
        };
        assert_eq!(CallEnd::of(&CallOutcome::Landed), CallEnd::Answered);
        assert_eq!(refused(Some("retry")), CallEnd::Retry);
        assert_eq!(refused(Some("change")), CallEnd::Change);
        assert_eq!(refused(Some("person")), CallEnd::Person);
        assert_eq!(refused(None), CallEnd::Refused);
        assert_eq!(refused(Some("@person:milhouse")), CallEnd::Refused);
    }

    /// The words are what a scraper's label stores, and nothing outside this
    /// process declares them, so the literals are pinned here.
    #[test]
    fn the_words_on_the_label_are_pinned() {
        let words: Vec<&str> = CallEnd::ALL.iter().map(|end| end.as_str()).collect();
        assert_eq!(words, ["answered", "retry", "change", "person", "refused"]);
    }
}

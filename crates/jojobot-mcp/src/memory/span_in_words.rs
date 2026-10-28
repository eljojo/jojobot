//! **A claim whose words give a first day and a last day, sent without the span
//! keys, is told where a span goes** (rule 456).
//!
//! A course or a festival is a stretch of days. `happened_at` is its start and
//! `happened_through` its end, and a reader who asks what was going on at a day
//! finds the claim by them. A claim that says "6 October to 10 November" in its
//! words and sends neither key keeps the span in prose only. The receipt names
//! the two keys, once a session. **Nothing is inferred and nothing is written:**
//! the words are only read to decide whether to say it.

use super::*;
use crate::teaching::{SPAN_IN_WORDS_DOMAIN, SPAN_IN_WORDS_TEACHING};

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Words that join the two ends of a span.
const CONNECTORS: [&str; 4] = ["to", "through", "thru", "until"];

/// A word of the text with where it starts and ends.
struct Token<'a> {
    word: &'a str,
    start: usize,
    end: usize,
}

fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut found = Vec::new();
    let mut start = None;
    for (at, ch) in text.char_indices() {
        match (ch.is_alphanumeric(), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                found.push(Token {
                    word: &text[from..at],
                    start: from,
                    end: at,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push(Token {
            word: &text[from..],
            start: from,
            end: text.len(),
        });
    }
    found
}

fn month(word: &str) -> bool {
    // Lower-case "may" is the verb far more often than the month.
    if word.eq_ignore_ascii_case("may") && word != "May" {
        return false;
    }
    let lower = word.to_lowercase();
    MONTHS.contains(&lower.as_str())
        || (lower.len() == 3
            && MONTHS
                .iter()
                .any(|full| full.starts_with(&lower) && lower != "may"))
}

/// A day of a month: one or two digits, with an ordinal ending allowed.
fn day(word: &str) -> bool {
    let digits = word.trim_end_matches(|c: char| c.is_alphabetic());
    let ending = &word[digits.len()..];
    (1..=2).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
        && digits.parse::<u8>().is_ok_and(|n| (1..=31).contains(&n))
        && matches!(
            ending.to_lowercase().as_str(),
            "" | "st" | "nd" | "rd" | "th"
        )
}

fn year(word: &str) -> bool {
    word.len() == 4 && word.chars().all(|c| c.is_ascii_digit())
}

/// **The dates the words name**, each as the first and last word it spans: a day
/// then a month ("6 October"), a month then a day ("October 6"), or a full
/// numeric date ("2026-10-06").
fn mentions(text: &str, words: &[Token<'_>]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < words.len() {
        let one = |n: usize| words.get(at + n).map(|t| t.word);
        let joined_by_dash =
            |from: usize| text[words[from].end..words[from + 1].start].trim() == "-";
        let span = if year(words[at].word)
            && at + 2 < words.len()
            && words[at + 1].word.len() == 2
            && words[at + 2].word.len() == 2
            && words[at + 1].word.chars().all(|c| c.is_ascii_digit())
            && words[at + 2].word.chars().all(|c| c.is_ascii_digit())
            && joined_by_dash(at)
            && joined_by_dash(at + 1)
        {
            Some(2)
        } else if (day(words[at].word) && one(1).is_some_and(month))
            || (month(words[at].word) && one(1).is_some_and(day))
        {
            Some(1)
        } else {
            None
        };
        match span {
            Some(extra) => {
                found.push((at, at + extra));
                at += extra + 1;
            }
            None => at += 1,
        }
    }
    found
}

/// **Whether the words name a first day and a last day**: two dates joined by a
/// word such as "to", or by a dash, with at most a year between the first and
/// the joint. One date is not a span, and two dates joined by "and" are two
/// days.
pub(crate) fn names_a_span(text: &str) -> bool {
    let words = tokens(text);
    let dates = mentions(text, &words);
    dates.windows(2).any(|pair| {
        let (first, second) = (pair[0], pair[1]);
        let mut between: Vec<&Token<'_>> = words[first.1 + 1..second.0].iter().collect();
        if between.first().is_some_and(|t| year(t.word)) {
            between.remove(0);
        }
        match between.as_slice() {
            [] => {
                let gap = &text[words[first.1].end..words[second.0].start];
                gap.contains(['-', '\u{2013}', '\u{2014}'])
            }
            [one] => CONNECTORS.contains(&one.word.to_lowercase().as_str()),
            _ => false,
        }
    })
}

impl Jojobot {
    /// **Teach the span keys on a claim whose words give a span and that sent
    /// neither.** Asked before the ledger row is spent, because spending is a side
    /// effect and a write that gave no span must not use up the session's one line.
    pub(crate) async fn note_span_in_words(
        &self,
        fact: &Fact,
        caller: &Caller,
        body: &mut serde_json::Value,
    ) {
        if fact.happened_at.is_some() || fact.happened_through.is_some() {
            return;
        }
        let mut text = fact.content.clone();
        if let Some(details) = &fact.details {
            text.push('\n');
            text.push_str(details);
        }
        if !names_a_span(&text) {
            return;
        }
        if self.first_contact(SPAN_IN_WORDS_DOMAIN, Some(caller)).await {
            crate::answer::note_teaching(body, SPAN_IN_WORDS_TEACHING);
        }
    }
}

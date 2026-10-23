//! Passage selection over word locations; indexing and stemming stay at the port.

use super::Fact;
use std::collections::{BTreeMap, BTreeSet};

/// A matched query term, in text order, with offsets in the original UTF-8 text.
#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub term: usize,
    pub start: usize,
    pub end: usize,
}

/// Literal vocabulary for ports without a stemmer, including the in-memory double.
pub fn literal_matches(text: &str, query: &str) -> Vec<Match> {
    let terms: BTreeSet<_> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut at = 0;
    let mut found = Vec::new();
    for piece in text.split_inclusive(|c: char| !c.is_alphanumeric()) {
        let word = piece.trim_end_matches(|c: char| !c.is_alphanumeric());
        for (term, wanted) in terms.iter().enumerate() {
            if word.to_lowercase() == *wanted {
                found.push(Match {
                    term,
                    start: at,
                    end: at + word.len(),
                });
            }
        }
        at += piece.len();
    }
    found
}

fn window(matches: &[Match]) -> (usize, std::cmp::Reverse<usize>, usize) {
    let mut counts = BTreeMap::<usize, usize>::new();
    let (mut left, mut right) = (0, 0);
    let mut best = (0, std::cmp::Reverse(usize::MAX), 0);
    for anchor in matches {
        let start = anchor.start.saturating_sub(80);
        while left < right && matches[left].start < start {
            let count = counts.get_mut(&matches[left].term).expect("a held match");
            *count -= 1;
            if *count == 0 {
                counts.remove(&matches[left].term);
            }
            left += 1;
        }
        while right < matches.len() && matches[right].start < start {
            right += 1;
            left = right;
        }
        while right < matches.len() && matches[right].end <= start + 320 {
            *counts.entry(matches[right].term).or_default() += 1;
            right += 1;
        }
        let (at, end) = if left < right {
            (matches[left].start, matches[right - 1].end)
        } else {
            (anchor.start, anchor.end)
        };
        let candidate = (counts.len(), std::cmp::Reverse(end - at), at);
        if (candidate.0, candidate.1) >= (best.0, best.1) {
            best = candidate;
        }
    }
    best
}

/// The dense matching passage, with enough context to read; never split UTF-8.
pub fn snippet(text: &str, matches: &[Match]) -> String {
    let (_, std::cmp::Reverse(span), at) = window(matches);
    let mut start = at.saturating_sub((320 - span.min(320)) / 2).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + 320).min(text.len());
    while !text.is_char_boundary(end) {
        end += 1;
    }
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        text[start..end].trim(),
        if end < text.len() { "…" } else { "" }
    )
}

/// A fact's heading cannot displace the details that answered the query.
pub fn of_fact(fact: &Fact, locate: impl Fn(&str) -> Vec<Match>) -> String {
    let text = match &fact.details {
        Some(details) => format!("{}\n\n{details}", fact.content),
        None => fact.content.clone(),
    };
    snippet(&text, &locate(&text))
}

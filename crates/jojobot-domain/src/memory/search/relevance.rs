//! Search order: preserve the kind of answer, then prefer more and closer words.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use super::{Fact, FactStatus, RankClock};

/// Answer kind, term coverage/proximity, chosen clock, BM25, then recency ties.
pub type Rank = (u64, Relevance, u64, f32, u64);

/// The complete key is available before the index cuts its bounded page.
pub fn rank(
    priority: u64,
    positions: &[(usize, u64)],
    text: f32,
    day: u64,
    clock_chosen: bool,
) -> Rank {
    let chosen_day = if clock_chosen { day } else { 0 };
    (priority, at(positions), chosen_day, text, day)
}

/// The caller's recency clock, encoded in calendar order; zero is no claim date.
pub fn day(fact: &Fact, clock: RankClock) -> u64 {
    let date = match clock {
        RankClock::RecordedAt => fact.recorded_at,
        RankClock::HappenedAt => fact.happened_at.unwrap_or(fact.recorded_at),
    };
    (i32::from(date.year()) + 10_000) as u64 * 10_000
        + date.month() as u64 * 100
        + date.day() as u64
}

/// The same order the bounded collector uses, with a total float comparison.
pub fn compare(a: &Rank, b: &Rank) -> std::cmp::Ordering {
    a.0.cmp(&b.0)
        .then_with(|| a.1.cmp(&b.1))
        .then_with(|| a.2.cmp(&b.2))
        .then_with(|| a.3.total_cmp(&b.3))
        .then_with(|| a.4.cmp(&b.4))
}

/// Current source claims, entities, prose and mail lead archived and derived claims.
pub const CURRENT: u64 = 3;
/// A run supplies context after the matching memory and mail.
pub const SESSION: u64 = 0;

/// Archived claims and derivations keep their place below current source claims.
pub fn fact_priority(fact: &Fact) -> u64 {
    if fact.status == FactStatus::Archived {
        1
    } else if fact.derived_from.is_some() {
        2
    } else {
        CURRENT
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
/// The distinct query words found, followed by the width of their tightest passage.
pub struct Relevance {
    terms: usize,
    span: Reverse<u64>,
}

/// Original query-term identities and token positions, sorted by position.
/// More distinct terms win; a shorter window covering them breaks the tie.
pub fn at(positions: &[(usize, u64)]) -> Relevance {
    let terms = positions
        .iter()
        .map(|(term, _)| term)
        .collect::<BTreeSet<_>>()
        .len();
    if terms <= 1 {
        return Relevance {
            terms,
            span: Reverse(0),
        };
    }
    let mut held = BTreeMap::<usize, usize>::new();
    let mut left = 0;
    let mut span = u64::MAX;
    for &(term, end) in positions {
        *held.entry(term).or_default() += 1;
        while held.len() == terms {
            span = span.min(end - positions[left].1);
            let count = held.get_mut(&positions[left].0).expect("held term");
            *count -= 1;
            if *count == 0 {
                held.remove(&positions[left].0);
            }
            left += 1;
        }
    }
    Relevance {
        terms,
        span: Reverse(span),
    }
}

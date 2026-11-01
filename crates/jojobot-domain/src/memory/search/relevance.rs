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

/// How close two text scores are, as a share of the larger, for them to be one score.
const SCORE_TOLERANCE: f32 = 1.0e-4;

/// **Whether two scores are within a float's rounding of each other.** The index scores
/// identical documents slightly apart once its statistics move, and that noise must not
/// outrank the day. A share of the larger score has no unit to underflow, so it holds
/// for a score of any size.
fn scores_tie(a: f32, b: f32) -> bool {
    a == b
        || (a.is_finite()
            && b.is_finite()
            && (a - b).abs() <= SCORE_TOLERANCE * a.abs().max(b.abs()))
}

/// **Put a page that is sorted best first, by [`compare`], into the order it is served
/// in.** A tolerance is not an ordering: if a is within it of b and b of c, a and c may
/// not be, and the key the collector cuts the page by needs a total order. So the key
/// holds the raw score, and the tie is settled here, on the page: scores that tie
/// the highest score of their group, with the same answer kind, closeness of words and
/// chosen clock, are one group, and a group is served by the day, newest first, then by
/// `then`. A group is anchored on its highest score, so it never spans more than the
/// tolerance, wherever the scores sit.
///
/// **The cost, named:** the collector cuts the page by the raw score, so a pair of tied
/// hits that straddles the cut can lose the newer one, and a longer page can group a pair
/// that a shorter page cut apart.
pub fn settle_ties<T>(
    page: &mut [T],
    rank_of: impl Fn(&T) -> &Rank,
    then: impl Fn(&T, &T) -> std::cmp::Ordering,
) {
    let mut start = 0;
    while start < page.len() {
        let first = *rank_of(&page[start]);
        let mut end = start + 1;
        while end < page.len() {
            let next = rank_of(&page[end]);
            let same_group = next.0 == first.0
                && next.1 == first.1
                && next.2 == first.2
                && scores_tie(first.3, next.3);
            if !same_group {
                break;
            }
            end += 1;
        }
        page[start..end].sort_by(|a, b| rank_of(b).4.cmp(&rank_of(a).4).then_with(|| then(a, b)));
        start = end;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The order a page of hits is served in, best first: sorted by the key the
    /// collector cuts by, then settled.
    fn page_order(mut hits: Vec<(Rank, &'static str)>) -> Vec<&'static str> {
        hits.sort_by(|a, b| compare(&b.0, &a.0));
        settle_ties(&mut hits, |hit| &hit.0, |a, b| a.1.cmp(b.1));
        hits.into_iter().map(|(_, name)| name).collect()
    }

    /// **Scores a float's rounding apart rank by the day whichever side of a multiple
    /// they fall.** 28.99499 and 28.99501 straddle 28.995, the edge between two buckets
    /// of 0.01 at this size, so any rounding of the score into buckets puts the older
    /// hit above the newer one; a tolerance has no such edge. Paired in one read with a
    /// real gap, which still decides before the day, and with a pair inside a bucket,
    /// which the day decides as before (the pair and the gap the earlier case held).
    #[test]
    fn scores_straddling_a_rounding_edge_rank_by_the_day() {
        let newer = rank(CURRENT, &[], 28.99499, 120_260_315, false);
        let older = rank(CURRENT, &[], 28.99501, 120_260_105, false);
        assert_eq!(
            page_order(vec![(older, "older"), (newer, "newer")]),
            ["newer", "older"],
            "the scores differ by 7e-7 of themselves, across a bucket edge: the later day comes first"
        );
        let stronger = rank(CURRENT, &[], 29.5, 120_260_105, false);
        assert_eq!(
            page_order(vec![(newer, "newer"), (stronger, "stronger")]),
            ["stronger", "newer"],
            "a real gap in text score still decides before the day"
        );
        let inside_newer = rank(CURRENT, &[], 28.9981, 120_260_315, false);
        let inside_older = rank(CURRENT, &[], 28.998_102, 120_260_105, false);
        assert_eq!(
            page_order(vec![(inside_older, "older"), (inside_newer, "newer")]),
            ["newer", "older"],
            "two scores inside one bucket are decided by the day"
        );
    }

    /// **A group of tied scores is anchored on its highest score**, so it never spans
    /// more than the tolerance, and a score far below does not join it however new its
    /// day is. Paired with the tie itself in the same read: the two scores within the
    /// tolerance of the highest are served by the day.
    #[test]
    fn a_group_of_tied_scores_never_spans_more_than_the_tolerance() {
        let top = rank(CURRENT, &[], 100.0, 120_260_105, false);
        let tied_newer = rank(CURRENT, &[], 99.992, 120_260_315, false);
        let apart_newest = rank(CURRENT, &[], 99.984, 120_260_901, false);
        assert_eq!(
            page_order(vec![
                (apart_newest, "apart"),
                (top, "top"),
                (tied_newer, "tied")
            ]),
            ["tied", "top", "apart"],
            "100.0 and 99.992 are one score and the day orders them; 99.984 is within 1e-4 of 99.992 but 1.6e-4 below 100.0, so it stays below the group"
        );
    }

    /// **A score of any size ties by a share of itself**, so a score far below the
    /// range a rounding step can represent still ties. Paired with a score a share of
    /// 1e-3 above, which does not.
    #[test]
    fn tiny_scores_tie_by_a_share_of_themselves() {
        let low = rank(CURRENT, &[], f32::from_bits(1_000_000), 120_260_105, false);
        let tied_newer = rank(CURRENT, &[], f32::from_bits(1_000_050), 120_260_315, false);
        let apart_oldest = rank(CURRENT, &[], f32::from_bits(1_001_000), 120_250_101, false);
        assert_eq!(
            page_order(vec![
                (low, "low"),
                (apart_oldest, "apart"),
                (tied_newer, "tied")
            ]),
            ["apart", "tied", "low"],
            "the score 1e-3 above stays first; the two within 1e-4 are ordered by the day"
        );
    }
}

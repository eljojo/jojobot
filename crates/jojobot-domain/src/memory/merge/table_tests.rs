//! **How a merge orders the writes of one key held by two things**, as a table
//! at the rule's owner. The table is the one the real store's adapter held; it
//! moved here, so the rule has one proof and a store's case need only show the
//! stamp reaches [`interleave`] from the real column.

use super::*;

/// **A field write as a merge orders it**: its key, its place in its own
/// thing's history, the claim that carried it and the moment the store stamped it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StampedWrite {
    key: String,
    ordinal: i64,
    fact_id: String,
    written_at: Option<jiff::Timestamp>,
}

impl ServedWrite for StampedWrite {
    fn at(&self) -> (&str, i64, Option<jiff::Timestamp>, &str) {
        (&self.key, self.ordinal, self.written_at, &self.fact_id)
    }
    fn place(&mut self, ordinal: i64, fact: Option<&str>) {
        self.ordinal = ordinal;
        if let Some(fact) = fact {
            self.fact_id = fact.to_string();
        }
    }
}

/// One write of `status` as a merge sees it: its place in its own thing's
/// history and the moment the store stamped it (`None` for a write an older
/// build appended).
fn written(ordinal: i64, at: Option<&str>) -> StampedWrite {
    StampedWrite {
        key: "status".to_string(),
        ordinal,
        fact_id: "f1".to_string(),
        written_at: at.map(|text| text.parse().expect("a stamp")),
    }
}

/// What order a merge puts two histories in, as (side, original ordinal),
/// oldest first.
fn order(folded: &[StampedWrite], survivor: &[StampedWrite]) -> Vec<(Side, i64)> {
    interleave(folded.to_vec(), survivor.to_vec())
        .into_iter()
        .map(|(side, write)| (side, write.ordinal))
        .collect()
}

/// **The write made later is placed later, whichever thing held it.** Both
/// directions are asserted, because an order that always put the survivor's
/// writes first passes one of them by luck.
#[test]
fn the_later_write_is_placed_later_whichever_thing_held_it() {
    let early = written(1, Some("2026-08-02T10:00:00Z"));
    let late = written(1, Some("2026-08-03T10:00:00Z"));
    assert_eq!(
        order(std::slice::from_ref(&early), std::slice::from_ref(&late)),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
        "the survivor's write is the later one and is placed last",
    );
    assert_eq!(
        order(&[late], &[early]),
        vec![(Side::Survivor, 1), (Side::Folded, 1)],
        "the duplicate's write is the later one and is placed last",
    );
}

/// **On a tie the survivor's write is placed after the duplicate's**, so the
/// survivor wins, and the same holds when neither write was ever stamped.
#[test]
fn a_tie_places_the_survivors_write_last() {
    let same = written(1, Some("2026-08-03T10:00:00Z"));
    assert_eq!(
        order(std::slice::from_ref(&same), std::slice::from_ref(&same)),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
    );
    let legacy = written(1, None);
    assert_eq!(
        order(std::slice::from_ref(&legacy), std::slice::from_ref(&legacy)),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
        "two writes with no stamp are a tie too",
    );
}

/// **A write with no stamp is older than every stamped one.** The stamp came
/// into being at one moment, so every write that carries one is later than every
/// write that cannot.
#[test]
fn an_unstamped_write_is_older_than_any_stamped_one() {
    let legacy = written(1, None);
    let stamped = written(1, Some("2026-08-02T10:00:00Z"));
    assert_eq!(
        order(
            std::slice::from_ref(&legacy),
            std::slice::from_ref(&stamped)
        ),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
        "the duplicate's legacy write is older than the survivor's stamped one",
    );
    assert_eq!(
        order(&[stamped], &[legacy]),
        vec![(Side::Survivor, 1), (Side::Folded, 1)],
        "the survivor's legacy write is older than the duplicate's stamped one",
    );
}

/// **Each thing's own order survives the merge**, even when a clock stepped
/// back inside one history: the two are merged, not sorted, so no write passes
/// another write of its own thing.
#[test]
fn a_things_own_order_is_kept_when_its_clock_stepped_back() {
    let first = written(1, Some("2026-08-05T10:00:00Z"));
    let second = written(2, Some("2026-08-01T10:00:00Z"));
    let other = written(1, Some("2026-08-03T10:00:00Z"));
    let merged = order(&[first, second], &[other]);
    let own: Vec<i64> = merged
        .iter()
        .filter(|(side, _)| *side == Side::Folded)
        .map(|(_, ordinal)| *ordinal)
        .collect();
    assert_eq!(own, vec![1, 2], "the duplicate's writes were reordered");
    assert_eq!(merged.len(), 3, "every write is placed: {merged:?}");
}

/// **A side that holds nothing leaves the other side as it was.** The scenario
/// the stores meet for a key only one thing holds.
#[test]
fn a_key_only_one_side_holds_keeps_its_order() {
    let one = written(1, Some("2026-08-01T10:00:00Z"));
    let two = written(2, Some("2026-08-02T10:00:00Z"));
    assert_eq!(
        order(&[one.clone(), two.clone()], &[]),
        vec![(Side::Folded, 1), (Side::Folded, 2)]
    );
    assert_eq!(
        order(&[], &[one, two]),
        vec![(Side::Survivor, 1), (Side::Survivor, 2)]
    );
    assert!(order(&[], &[]).is_empty());
}

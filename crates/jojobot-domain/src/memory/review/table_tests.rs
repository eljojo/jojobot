//! **The two rules that tell a change of a thing from jojobot's bookkeeping,
//! as tables at their owner.** No store is asked: `ledger_records` finds the
//! records that carry a ledger, `counts_as_a_change` reads its answer.

use super::*;
use std::collections::BTreeSet;

/// **Which records carry the ledger of release notes.** The label is a pair:
/// the engine-record key AND the release-notes value. Either half alone, or the
/// value on another key, is an ordinary write.
#[test]
fn a_record_carries_the_ledger_only_when_both_halves_of_the_label_are_on_it() {
    use crate::memory::capacity::{ENGINE_RECORD, RELEASE_NOTES_RECORD};
    //   (key, value, record, carries the ledger)
    let table = [
        (ENGINE_RECORD, Some(RELEASE_NOTES_RECORD), 1, true),
        (ENGINE_RECORD, Some(RELEASE_NOTES_RECORD), 2, true),
        (ENGINE_RECORD, Some("another_engine_record"), 3, false),
        (ENGINE_RECORD, None, 4, false),
        ("status", Some(RELEASE_NOTES_RECORD), 5, false),
        ("status", Some("done"), 6, false),
    ];
    let found = ledger_records(
        table
            .iter()
            .map(|(key, value, record, _)| (*key, *value, *record)),
    );
    let expected: BTreeSet<i32> = table.iter().filter(|row| row.3).map(|row| row.2).collect();
    assert_eq!(found, expected, "{table:?}");
    assert!(
        ledger_records(std::iter::empty::<(&str, Option<&str>, i32)>()).is_empty(),
        "no writes, no ledger"
    );
    // A record named twice is one record.
    let twice = [
        (ENGINE_RECORD, Some(RELEASE_NOTES_RECORD), 7),
        (ENGINE_RECORD, Some(RELEASE_NOTES_RECORD), 7),
    ];
    assert_eq!(ledger_records(twice), BTreeSet::from([7]));
}

/// **A write is a change of the thing unless it is bookkeeping.** Bookkeeping
/// is a bookkeeping KEY, or ANY key of a record in the ledger set. A write that
/// names no key counts unless its record carries the ledger.
#[test]
fn a_write_counts_as_a_change_unless_it_is_jojobots_own_bookkeeping() {
    let ledgers: BTreeSet<i32> = BTreeSet::from([10]);
    //   (key, record, counts)
    let table: [(Option<&str>, i32, bool); 8] = [
        (Some("status"), 1, true),
        (None, 1, true),
        (Some(DEFRAGGED_AT), 1, false),
        (Some(crate::memory::RELEASE_NOTES_TOLD), 1, false),
        (Some("status"), 10, false),
        (None, 10, false),
        (Some(DEFRAGGED_AT), 10, false),
        (Some("defragged_at_not"), 1, true),
    ];
    for (key, record, counts) in table {
        assert_eq!(
            counts_as_a_change(key, &record, &ledgers),
            counts,
            "key {key:?}, record {record}"
        );
    }
    let none: BTreeSet<i32> = BTreeSet::new();
    assert!(
        counts_as_a_change(Some("status"), &10, &none),
        "no ledger, an ordinary write"
    );
}

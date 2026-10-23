//! Which day a reset lands on, and how a change is picked from a run's writes.

use super::history::{Change, SessionChange, next_change};
use super::*;

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_790_000_000 + second).expect("a moment")
}

fn day(text: &str) -> Date {
    text.parse().expect("a day")
}

/// **A compaction carries the day the run was in when it reported.** The run
/// states a day when it begins, moves it, and reports twice; a run that states
/// none has no day to carry, so its compaction is counted on the day it was
/// recorded.
#[test]
fn a_compaction_carries_the_day_the_run_was_in_when_it_reported() {
    let mut kept: Vec<SessionChange> = Vec::new();
    let mut push = |change: Change<'_>, second: i64| {
        let highest = kept.last().map(|held| held.ordinal);
        kept.push(next_change(highest, change, at(second)));
    };
    push(Change::ServedReset(100), 1);
    push(Change::StatedDay(Some(day("2026-10-09"))), 2);
    push(Change::ServedReset(40), 3);
    push(Change::StatedDay(None), 4);
    push(Change::ServedReset(7), 5);

    let stated = resets_of(&kept, Some(day("2026-10-07")));
    assert_eq!(
        stated.iter().map(|reset| reset.day).collect::<Vec<_>>(),
        vec![Some(day("2026-10-07")), Some(day("2026-10-09")), None],
        "the day at each report: the day it began on, the moved day, and none once cleared"
    );
    assert_eq!(
        stated.iter().map(|reset| reset.total).collect::<Vec<_>>(),
        vec![100, 40, 7]
    );
    let unstated = resets_of(&kept[..1], None);
    assert_eq!(unstated[0].day, None, "a run that states no day has none");
}

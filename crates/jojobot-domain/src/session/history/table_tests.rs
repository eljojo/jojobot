//! **Which changes of a run move the search index, one row for each kind.**
//! The index reads the text of an entry, the focus of a run and the run coming
//! to be; a change nothing searches for is recorded and moves nothing. The rule
//! decides what the in-memory counter counts, so a kind moved by mistake makes
//! every refresh recount and a kind left out makes a search stale.
//!
//! The existing case `a_token_round_trips_and_only_five_kinds_move_the_index`
//! pins the five by their order in `ALL`; this table names every kind, so a
//! thirteenth kind cannot join `ALL` without a row saying which side it is on.

use super::*;

#[test]
fn each_kind_of_change_is_on_one_side_of_the_index() {
    //   (kind, moves the index)
    let table = [
        (ChangeKind::Begin, true),
        (ChangeKind::Entry, true),
        (ChangeKind::AmendLast, true),
        (ChangeKind::AmendBeat, true),
        (ChangeKind::Focus, true),
        (ChangeKind::Timezone, false),
        (ChangeKind::Agent, false),
        (ChangeKind::StatedDay, false),
        (ChangeKind::WrapWindow, false),
        (ChangeKind::State, false),
        (ChangeKind::ServedReset, false),
        (ChangeKind::Key, false),
        (ChangeKind::CharterTold, false),
    ];
    for (kind, moves) in table {
        assert_eq!(kind.moves_the_index(), moves, "{kind:?}");
    }
    for kind in ChangeKind::ALL {
        assert!(
            table.iter().any(|(named, _)| *named == kind),
            "{kind:?} has no row: say which side of the index it is on"
        );
    }
    assert_eq!(table.len(), ChangeKind::ALL.len());
}

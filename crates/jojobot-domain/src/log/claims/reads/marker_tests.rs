use super::super::tests::{archived, body, claim, created, moment, object, stamped, write};
use super::super::{ClaimBody, ClaimEvent, MovedRecord, ObjectBody, State};
use crate::memory::{capacity, review};

fn state_at(events: Vec<(Option<&str>, ClaimEvent)>) -> State {
    let mut state = State::default();
    for (at, event) in events {
        state.apply(&stamped(at.map(moment), None, event));
    }
    state
}

fn made(name: &str) -> ClaimEvent {
    ClaimEvent::ObjectCreated {
        object: object(name),
        body: ObjectBody {
            ..body("work", name)
        },
    }
}

fn revised(id: &str, home: &str, content: &str) -> ClaimEvent {
    ClaimEvent::RecordRevised {
        record: super::super::tests::record(id),
        claim: ClaimBody {
            content: content.to_string(),
            ..claim("f1", home, content, 3)
        },
    }
}

/// **The marker counts the writes of the claims homed on a thing and of its own
/// row, and only grows**: a claim, its revision, a key written and a row write
/// each move it by their own writes, and another thing's writes do not.
#[test]
fn the_marker_counts_the_writes_behind_a_thing_and_only_grows() {
    let mut events: Vec<(Option<&str>, ClaimEvent)> =
        vec![(None, made("work:alpha")), (None, made("work:beta"))];
    let mut seen = Vec::new();
    let mut step = |events: &mut Vec<(Option<&str>, ClaimEvent)>, more: ClaimEvent| {
        events.push((None, more));
        let state = state_at(events.clone());
        seen.push(state.write_marker(&object("work:alpha")));
    };
    step(
        &mut events,
        created("r1", claim("f1", "work:alpha", "a", 3)),
    );
    step(
        &mut events,
        write("r1", "work:alpha", "owner", 1, "bot:alpha"),
    );
    step(&mut events, revised("r1", "work:alpha", "a2"));
    step(&mut events, archived("r1", "f1", "work:alpha"));
    step(&mut events, created("r2", claim("f1", "work:beta", "b", 3)));
    // The row write, then the claim; a key written adds no claim write; a
    // revision and a taking back add one each; another thing's claim none.
    assert_eq!(seen, vec![2, 2, 3, 4, 4]);
}

/// **The newest change is the newest moment among the row and every claim write
/// of any status**, a skipped claim is left out, and a thing whose writes carry
/// no moment has none.
#[test]
fn the_newest_change_is_the_newest_moment_of_any_write_that_counts() {
    let state = state_at(vec![
        (Some("2026-04-01T00:00:00Z"), made("work:alpha")),
        (
            Some("2026-04-02T00:00:00Z"),
            created("r1", claim("f1", "work:alpha", "a", 3)),
        ),
        (
            Some("2026-04-09T00:00:00Z"),
            created("r2", claim("f2", "work:alpha", "ledger", 3)),
        ),
        (
            Some("2026-04-09T00:00:00Z"),
            write(
                "r2",
                "work:alpha",
                capacity::ENGINE_RECORD,
                1,
                capacity::RELEASE_NOTES_RECORD,
            ),
        ),
        (
            Some("2026-04-05T00:00:00Z"),
            archived("r1", "f1", "work:alpha"),
        ),
        (None, made("work:beta")),
        (Some("2026-04-01T00:00:00Z"), made("work:delta")),
        (
            Some("2026-04-02T00:00:00Z"),
            created("r4", claim("f1", "work:delta", "d", 3)),
        ),
        (
            Some("2026-04-12T00:00:00Z"),
            write("r4", "work:delta", review::DEFRAGGED_AT, 1, "x"),
        ),
        (Some("2026-04-01T00:00:00Z"), made("work:gamma")),
        (
            Some("2026-04-02T00:00:00Z"),
            created("r3", claim("f1", "work:gamma", "g", 3)),
        ),
        (
            Some("2026-04-07T00:00:00Z"),
            ClaimEvent::ObjectRevised {
                object: object("work:gamma"),
                body: ObjectBody {
                    archived: true,
                    ..body("work", "work:gamma")
                },
            },
        ),
    ]);
    assert_eq!(
        state.newest_change(&object("work:gamma")),
        Some(moment("2026-04-07T00:00:00Z")),
        "the row of the thing counts too"
    );
    assert_eq!(
        state.newest_change(&object("work:alpha")),
        Some(moment("2026-04-05T00:00:00Z")),
        "a taken-back claim still counts; the claim that carries the ledger of \
         release notes, and its writes, do not"
    );
    assert_eq!(
        state.newest_change(&object("work:delta")),
        Some(moment("2026-04-02T00:00:00Z")),
        "a bookkeeping key is no change of the thing"
    );
    assert_eq!(state.newest_change(&object("work:beta")), None);
}

/// **A write of a key counts with its own moment, newer than every claim write
/// and the row**: the thing is as new as its newest field write, paired with a
/// bookkeeping-only write that does not move it.
#[test]
fn a_field_write_newer_than_the_claim_is_the_newest_change() {
    let state = state_at(vec![
        (Some("2026-04-01T00:00:00Z"), made("work:eta")),
        (
            Some("2026-04-02T00:00:00Z"),
            created("r1", claim("f1", "work:eta", "e", 3)),
        ),
        (
            Some("2026-04-10T00:00:00Z"),
            write("r1", "work:eta", "phase", 1, "now"),
        ),
        (
            Some("2026-04-12T00:00:00Z"),
            write("r1", "work:eta", review::DEFRAGGED_AT, 1, "x"),
        ),
    ]);
    assert_eq!(
        state.newest_change(&object("work:eta")),
        Some(moment("2026-04-10T00:00:00Z"))
    );
}

fn merging(folded: &str, survivor: &str, moved: &[(&str, &str)]) -> ClaimEvent {
    ClaimEvent::ObjectsMerged {
        folded: object(folded),
        survivor: object(survivor),
        records: moved
            .iter()
            .map(|(id, label)| MovedRecord {
                record: super::super::tests::record(id),
                label: label.to_string(),
            })
            .collect(),
        writes: Vec::new(),
    }
}

/// **The marker never falls across a merge**, whether the claims of the folded
/// thing are mapped to fresh labels or left unmapped under a label the survivor
/// already holds: a claim is counted by being homed on the thing, not by the
/// label slot it holds.
#[test]
fn the_marker_does_not_fall_when_a_merge_leaves_two_claims_one_label() {
    for moved in [vec![("r2", "f2")], Vec::new()] {
        let mut state = State::default();
        for (at, event) in [
            made("work:alpha"),
            made("work:beta"),
            created("r1", claim("f1", "work:alpha", "a", 3)),
            revised("r1", "work:alpha", "a2"),
            revised("r1", "work:alpha", "a3"),
            created("r2", claim("f1", "work:beta", "b", 3)),
        ]
        .into_iter()
        .map(|event| (None::<&str>, event))
        {
            state.apply(&stamped(at.map(moment), None, event));
        }
        let before = state.write_marker(&object("work:alpha"));
        state.apply(&stamped(
            None,
            None,
            merging("work:beta", "work:alpha", &moved),
        ));
        let after = state.write_marker(&object("work:alpha"));
        // alpha: row 1 + claim writes 3; beta: row 1 + claim writes 1.
        assert_eq!(before, 4, "{moved:?}");
        assert_eq!(after, 6, "{moved:?}: both claims and both rows are counted");
        assert!(after >= before);
    }
}

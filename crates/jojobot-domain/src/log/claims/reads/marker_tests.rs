use super::super::tests::{archived, body, claim, created, moment, object, stamped, write};
use super::super::{ClaimBody, ClaimEvent, ObjectBody, State};

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
            Some("2026-04-05T00:00:00Z"),
            archived("r1", "f1", "work:alpha"),
        ),
        (None, made("work:beta")),
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
        state.newest_change(&object("work:gamma"), &|_| false),
        Some(moment("2026-04-07T00:00:00Z")),
        "the row of the thing counts too"
    );
    let home = object("work:alpha");
    assert_eq!(
        state.newest_change(&home, &|_| false),
        Some(moment("2026-04-09T00:00:00Z"))
    );
    assert_eq!(
        state.newest_change(&home, &|held| held.claim.label == "f2"),
        Some(moment("2026-04-05T00:00:00Z")),
        "a taken-back claim still counts, a skipped one does not"
    );
    assert_eq!(state.newest_change(&object("work:beta"), &|_| false), None);
}

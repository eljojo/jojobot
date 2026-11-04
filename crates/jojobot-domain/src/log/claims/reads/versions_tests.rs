use super::super::tests::{body, moment, object, stamped, under};
use super::super::{ClaimEvent, ObjectBody, State};
use crate::actor::Actor;
use crate::log::Event;

fn made(name: &str, title: &str) -> ClaimEvent {
    ClaimEvent::ObjectCreated {
        object: object(name),
        body: ObjectBody {
            ..body("work", title)
        },
    }
}

fn renamed(name: &str, title: &str, archived: bool) -> ClaimEvent {
    ClaimEvent::ObjectRevised {
        object: object(name),
        body: ObjectBody {
            archived,
            ..body("work", title)
        },
    }
}

fn merged(folded: &str, survivor: &str) -> ClaimEvent {
    ClaimEvent::ObjectsMerged {
        folded: object(folded),
        survivor: object(survivor),
        records: Vec::new(),
        writes: Vec::new(),
    }
}

fn names(state: &State, name: &str) -> Vec<(String, String)> {
    state
        .entity_versions(&object(name))
        .into_iter()
        .map(|version| (version.thing.0.clone(), version.body.name.clone()))
        .collect()
}

/// **An object keeps every write of its own row, oldest first**, each with the
/// object as that write left it and what the envelope says of the write: the
/// actor, the run, and the moment, which an imported write takes from its source.
#[test]
fn an_object_keeps_every_write_of_its_own_row() {
    let mut state = State::default();
    state.apply(&Event {
        written_at: Some(moment("2026-04-01T00:00:00Z")),
        imported_source_time: Some(moment("2026-02-01T00:00:00Z")),
        ..under(
            Some(Actor::Operator),
            Some("run-a"),
            made("work:alpha", "Alpha"),
        )
    });
    state.apply(&stamped(
        Some(moment("2026-04-02T00:00:00Z")),
        None,
        renamed("work:alpha", "Alpha two", false),
    ));
    state.apply(&stamped(
        None,
        None,
        renamed("work:alpha", "Alpha two", true),
    ));
    let versions = state.entity_versions(&object("work:alpha"));
    assert_eq!(
        versions
            .iter()
            .map(|v| (v.body.name.as_str(), v.body.archived))
            .collect::<Vec<_>>(),
        vec![("Alpha", false), ("Alpha two", false), ("Alpha two", true)]
    );
    assert_eq!(versions[0].actor, Some(Actor::Operator));
    assert_eq!(
        versions[0].run.as_ref().map(|run| run.0.as_str()),
        Some("run-a")
    );
    assert_eq!(versions[0].written_at, Some(moment("2026-02-01T00:00:00Z")));
    assert_eq!(versions[1].written_at, Some(moment("2026-04-02T00:00:00Z")));
    assert_eq!(versions[2].written_at, None);
}

/// **A thing's versions include those of every thing folded into it, at any
/// depth, in the order they were made**, each saying which thing it is; a thing
/// no merge touched has only its own.
#[test]
fn a_thing_holds_the_versions_of_everything_folded_into_it() {
    let mut state = State::default();
    for event in [
        made("work:alpha", "Alpha"),
        made("work:beta", "Beta"),
        made("work:gamma", "Gamma"),
        renamed("work:beta", "Beta two", false),
        merged("work:beta", "work:alpha"),
        renamed("work:alpha", "Alpha two", false),
        merged("work:alpha", "work:gamma"),
    ] {
        state.apply(&stamped(None, None, event));
    }
    let want = vec![
        ("work:alpha".to_string(), "Alpha".to_string()),
        ("work:beta".to_string(), "Beta".to_string()),
        ("work:gamma".to_string(), "Gamma".to_string()),
        ("work:beta".to_string(), "Beta two".to_string()),
        ("work:alpha".to_string(), "Alpha two".to_string()),
    ];
    assert_eq!(names(&state, "work:gamma"), want);
    assert_eq!(
        names(&state, "work:alpha"),
        want,
        "a folded thing answers as its survivor"
    );
    let mut lone = State::default();
    lone.apply(&stamped(None, None, made("work:delta", "Delta")));
    lone.apply(&stamped(None, None, made("work:eta", "Eta")));
    assert_eq!(
        names(&lone, "work:delta"),
        vec![("work:delta".to_string(), "Delta".to_string())]
    );
}

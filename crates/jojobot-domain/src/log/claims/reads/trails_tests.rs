use super::super::tests::{archived, body, claim, clear, created, key, object, record, write};
use super::super::{ClaimBody, ClaimEvent, FieldValue, ObjectBody, State};
use crate::memory::{FactId, FactStatus, Provenance, Standing, WriteClass};

fn thing() -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object("work:alpha"),
            body: ObjectBody {
                ..body("work", "Alpha")
            },
        },
        ClaimEvent::AddressBound {
            object: object("work:alpha"),
            address: "work:alpha".to_string(),
        },
    ]
}

fn replayed(events: &[ClaimEvent]) -> State {
    let mut state = State::default();
    for event in events {
        state.apply(&super::super::tests::envelope(event.clone()));
    }
    state
}

/// A write of a key that stays on its record.
fn stays(id: &str, k: &str, ordinal: u64, value: &str) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(id),
        subject: object("work:alpha"),
        key: key(k),
        ordinal,
        class: WriteClass::Stays,
        write: FieldValue::Set(value.to_string()),
        targets: Vec::new(),
    }
}

/// **The trail of a key is its writes that reach the thing, oldest first**, each
/// with the claim it arrived in as that claim is now: a write inside a claim
/// that was taken back still happened, so it is reported, as archived.
#[test]
fn the_trail_of_a_key_is_every_write_that_reaches_the_thing() {
    let noted = ClaimBody {
        details: Some("why".to_string()),
        provenance: Provenance::Observation,
        standing: Standing::Open,
        ..claim("f2", "work:alpha", "b", 3)
    };
    let events = [
        thing(),
        vec![
            created("r1", claim("f1", "work:alpha", "a", 3)),
            write("r1", "work:alpha", "owner", 1, "bot:alpha"),
            created("r2", noted),
            write("r2", "work:alpha", "owner", 2, "bot:beta"),
            clear("r1", "work:alpha", "owner", 3),
            archived("r2", "f2", "work:alpha"),
        ],
    ]
    .concat();
    let trails = replayed(&events).key_trails(&object("work:alpha"));
    let owner = &trails.history["owner"];
    assert_eq!(
        owner.iter().map(|w| w.value.clone()).collect::<Vec<_>>(),
        vec![
            Some("bot:alpha".to_string()),
            Some("bot:beta".to_string()),
            None
        ]
    );
    assert_eq!(owner[1].fact.local, FactId("f2".to_string()));
    assert_eq!(owner[1].fact.home.0, "work:alpha");
    assert_eq!(owner[1].status, FactStatus::Archived);
    assert_eq!(trails.history.len(), 1);
    assert!(trails.kept_off.is_empty());
    assert!(trails.record_writes.is_empty());
}

/// **A key that stays on its record is kept off the thing, and its writes are
/// kept with the record**, ordered by key and then place. Taking the key off the
/// record ends it being kept off; a write of the key that reaches the thing is
/// in the trail and not among the record's.
#[test]
fn a_key_that_stays_on_its_record_is_kept_off_the_thing() {
    let events = [
        thing(),
        vec![
            created("r1", claim("f1", "work:alpha", "a", 3)),
            stays("r1", "heading", 1, "two"),
            stays("r1", "heading", 2, "three"),
            stays("r1", "place", 1, "x"),
            write("r1", "work:alpha", "owner", 1, "bot:alpha"),
            created("r2", claim("f2", "work:alpha", "b", 3)),
            stays("r2", "place", 2, "y"),
            clear("r2", "work:alpha", "place", 3),
        ],
    ]
    .concat();
    let trails = replayed(&events).key_trails(&object("work:alpha"));
    let kept = trails
        .kept_off
        .iter()
        .map(|(fact, keys)| (fact.0.clone(), keys.iter().cloned().collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    assert_eq!(
        kept,
        vec![(
            "f1".to_string(),
            vec!["heading".to_string(), "place".to_string()]
        )],
        "f2 took its place key off, so nothing is kept off it"
    );
    let stayed = trails
        .record_writes
        .iter()
        .map(|w| (w.key.clone(), w.ordinal, w.fact.0.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        stayed,
        vec![
            ("heading".to_string(), 1, "f1".to_string()),
            ("heading".to_string(), 2, "f1".to_string()),
            ("place".to_string(), 1, "f1".to_string()),
            ("place".to_string(), 2, "f2".to_string()),
        ]
    );
    assert_eq!(
        trails.history.keys().cloned().collect::<Vec<_>>(),
        vec!["owner".to_string(), "place".to_string()]
    );
}

/// **A write whose claim the State does not hold is left out of every part.**
#[test]
fn a_write_without_its_claim_is_left_out_of_the_trails() {
    let events = [
        thing(),
        vec![
            write("lost", "work:alpha", "owner", 1, "bot:alpha"),
            stays("lost", "heading", 2, "two"),
        ],
    ]
    .concat();
    let trails = replayed(&events).key_trails(&object("work:alpha"));
    assert!(trails.history.is_empty());
    assert!(trails.record_writes.is_empty());
    assert!(trails.kept_off.is_empty());
}

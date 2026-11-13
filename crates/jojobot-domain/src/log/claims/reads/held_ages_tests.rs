//! **A thing's field ages and newest change are held when a write lands, and equal
//! what a fresh pass over its writes works out after every kind of event.** The age
//! of a held key sits in its entry beside the value; the newest change is one
//! moment held with the thing. A read looks them up: once the writes a snapshot
//! covers are gone, a read could not work them out.

use super::super::tests::{body, claim, clear, created, moment, object, record, stamped, write};
use super::super::{ClaimEvent, State};
use crate::memory::types::{DeclaredType, Field, ValueType};
use crate::memory::{FactStatus, capacity, review};

/// The moment `n` days into April, 2026.
fn at(n: u32) -> jiff::Timestamp {
    moment(&format!("2026-04-{n:02}T00:00:00Z"))
}

fn apply_at(state: &mut State, day: u32, event: ClaimEvent) {
    state.apply(&stamped(Some(at(day)), None, event));
}

/// One thing with the handle `person:alpha`, and one with `person:beta`.
fn two_things() -> State {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    for (id, handle) in [("k1", "person:alpha"), ("k2", "person:beta")] {
        apply_at(
            &mut state,
            1,
            ClaimEvent::ObjectCreated {
                object: object(id),
                body: body("person", handle),
            },
        );
        apply_at(
            &mut state,
            1,
            ClaimEvent::AddressBound {
                object: object(id),
                address: handle.to_string(),
            },
        );
    }
    state
}

fn agrees(state: &State, after: &str) {
    if let Err(differs) = state.held_fields_are_what_a_fold_makes() {
        panic!("after {after}: {differs}");
    }
}

/// **After each kind of event the held age and newest change are what a fresh
/// pass makes, and the newest change is the moment the rule says.** A key write and
/// a rewrite move a key's age; a clear takes the key and its age away; a
/// bookkeeping key moves no newest change; a record carrying the ledger of release
/// notes counts for none, whatever it wrote before; a claim revision and a write of
/// the thing's own row are changes; a merge and a declaration leave the answer one a
/// fresh pass makes.
#[test]
fn the_held_ages_and_newest_change_are_what_a_fresh_pass_makes_after_each_event() {
    let mut state = two_things();
    let alpha = object("k1");
    let ages = |state: &State| state.held_since(&alpha, &state.declared_types_shared());
    agrees(&state, "the things are made");
    assert_eq!(state.newest_change(&alpha), Some(at(1)), "the own row");

    apply_at(
        &mut state,
        2,
        created("k1/r1", claim("f1", "k1", "a claim", 1)),
    );
    apply_at(&mut state, 3, write("k1/r1", "k1", "mood", 1, "glad"));
    agrees(&state, "a key write");
    assert_eq!(state.newest_change(&alpha), Some(at(3)));
    assert_eq!(ages(&state)["mood"], at(3));

    apply_at(&mut state, 4, write("k1/r1", "k1", "mood", 2, "calm"));
    apply_at(&mut state, 4, write("k1/r1", "k1", "due", 3, "2026-05-01"));
    agrees(&state, "a rewrite");
    assert_eq!(ages(&state)["mood"], at(4), "the rewrite holds the key now");
    assert_eq!(ages(&state)["due"], at(4));

    apply_at(&mut state, 5, clear("k1/r1", "k1", "mood", 4));
    agrees(&state, "a clear");
    assert!(
        !ages(&state).contains_key("mood"),
        "a cleared key holds none"
    );
    assert_eq!(state.newest_change(&alpha), Some(at(5)), "a clear counts");

    apply_at(
        &mut state,
        6,
        write("k1/r1", "k1", review::DEFRAGGED_AT, 5, "2026-04-06"),
    );
    agrees(&state, "a bookkeeping key");
    assert_eq!(
        state.newest_change(&alpha),
        Some(at(5)),
        "a bookkeeping key is no change of the thing"
    );

    apply_at(
        &mut state,
        7,
        created("k1/ledger", claim("f2", "k1", "release notes told", 1)),
    );
    assert_eq!(
        state.newest_change(&alpha),
        Some(at(7)),
        "until it is found"
    );
    apply_at(
        &mut state,
        8,
        write(
            "k1/ledger",
            "k1",
            capacity::ENGINE_RECORD,
            1,
            capacity::RELEASE_NOTES_RECORD,
        ),
    );
    agrees(&state, "a record that carries the ledger");
    assert_eq!(
        state.newest_change(&alpha),
        Some(at(5)),
        "the ledger record's earlier and later writes count for none"
    );

    let mut revised = claim("f1", "k1", "a claim, said again", 1);
    revised.status = FactStatus::Active;
    apply_at(
        &mut state,
        9,
        ClaimEvent::RecordRevised {
            record: record("k1/r1"),
            claim: revised,
        },
    );
    agrees(&state, "a claim revision");
    assert_eq!(state.newest_change(&alpha), Some(at(9)));

    apply_at(
        &mut state,
        10,
        ClaimEvent::ObjectRevised {
            object: object("k1"),
            body: body("person", "person:alpha"),
        },
    );
    agrees(&state, "a write of the thing's own row");
    assert_eq!(state.newest_change(&alpha), Some(at(10)));

    apply_at(
        &mut state,
        11,
        ClaimEvent::TypeDeclared {
            kind: object("person"),
            declaration: DeclaredType::new("person", vec![Field::new("due", ValueType::Date)]),
        },
    );
    agrees(&state, "a declaration");

    apply_at(&mut state, 12, created("k2/r1", claim("f1", "k2", "b", 1)));
    apply_at(&mut state, 12, write("k2/r1", "k2", "mood", 1, "sour"));
    apply_at(
        &mut state,
        12,
        ClaimEvent::ObjectsMerged {
            folded: object("k2"),
            survivor: object("k1"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    );
    agrees(&state, "a merge");
    assert_eq!(
        state.newest_change(&alpha),
        Some(at(12)),
        "the folded thing's writes are the survivor's"
    );
}

/// **A read of the ages visits no write**: they are looked up in the held entry.
#[test]
fn a_read_of_the_ages_looks_at_no_stream() {
    let mut state = two_things();
    apply_at(&mut state, 2, created("k1/r1", claim("f1", "k1", "a", 1)));
    apply_at(&mut state, 3, write("k1/r1", "k1", "mood", 1, "glad"));
    super::STREAMS_EXAMINED.with(|n| n.set(0));

    let held = state.held_since(&object("k1"), &state.declared_types_shared());
    let newest = state.newest_change(&object("k1"));

    assert_eq!(held["mood"], at(3));
    assert_eq!(newest, Some(at(3)));
    assert_eq!(super::STREAMS_EXAMINED.with(|n| n.get()), 0);
}

/// **A State built again from the same events holds the same ages and newest
/// change.** The claims State is in no snapshot: it is replayed from its events, by
/// the one apply the live State used, so the entries are held again as they were
/// held the first time. A key write, a clear, a ledger record, a claim revision, an
/// own-row write and a merge are in the sequence.
#[test]
fn a_replayed_state_holds_the_ages_and_newest_change_of_the_live_one() {
    let mut events: Vec<crate::log::Event<ClaimEvent>> = Vec::new();
    let mut live = State::default();
    crate::memory::kinds::load_shipped();
    let mut say = |day: u32, event: ClaimEvent| {
        let event = stamped(Some(at(day)), None, event);
        live.apply(&event);
        events.push(event);
    };
    for (id, handle) in [("k1", "person:alpha"), ("k2", "person:beta")] {
        say(
            1,
            ClaimEvent::ObjectCreated {
                object: object(id),
                body: body("person", handle),
            },
        );
        say(
            1,
            ClaimEvent::AddressBound {
                object: object(id),
                address: handle.to_string(),
            },
        );
    }
    say(2, created("k1/r1", claim("f1", "k1", "a claim", 1)));
    say(3, write("k1/r1", "k1", "mood", 1, "glad"));
    say(4, write("k1/r1", "k1", "due", 2, "2026-05-01"));
    say(5, clear("k1/r1", "k1", "mood", 3));
    say(
        6,
        created("k1/ledger", claim("f2", "k1", "release notes", 1)),
    );
    say(
        7,
        write(
            "k1/ledger",
            "k1",
            capacity::ENGINE_RECORD,
            1,
            capacity::RELEASE_NOTES_RECORD,
        ),
    );
    say(8, created("k2/r1", claim("f1", "k2", "b", 1)));
    say(9, write("k2/r1", "k2", "mood", 1, "sour"));
    say(
        10,
        ClaimEvent::RecordRevised {
            record: record("k1/r1"),
            claim: claim("f1", "k1", "said again", 1),
        },
    );
    say(
        11,
        ClaimEvent::ObjectRevised {
            object: object("k1"),
            body: body("person", "person:alpha"),
        },
    );
    say(
        12,
        ClaimEvent::ObjectsMerged {
            folded: object("k2"),
            survivor: object("k1"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    );

    let restored = State::replay(events.iter());

    for id in ["k1", "k2"] {
        let thing = object(id);
        assert_eq!(
            restored.held_since(&thing, &restored.declared_types_shared()),
            live.held_since(&thing, &live.declared_types_shared()),
            "{id}: the ages"
        );
        assert_eq!(
            restored.newest_change(&thing),
            live.newest_change(&thing),
            "{id}: the newest change"
        );
    }
    assert_eq!(live.newest_change(&object("k1")), Some(at(11)));
    assert_eq!(restored.held_fields_are_what_a_fold_makes(), Ok(()));
}

/// **A declaration that names no key is no type the port lists**, as the store
/// lists none: a kind that holds no key has no row of a type. The declaration stays in
/// force for what it says of the kind.
#[test]
fn a_declaration_with_no_keys_is_not_listed_as_a_type() {
    let mut state = State::default();
    for (kind, fields) in [
        ("pet", Vec::new()),
        ("person", vec![Field::new("due", ValueType::Date)]),
    ] {
        apply_at(
            &mut state,
            1,
            ClaimEvent::TypeDeclared {
                kind: object(kind),
                declaration: DeclaredType::new(kind, fields),
            },
        );
    }
    let listed: Vec<String> = state
        .declared_types_shared()
        .iter()
        .map(|held| held.name.clone())
        .collect();
    assert_eq!(
        listed,
        ["person"],
        "only a declaration with a key is listed"
    );
    assert!(
        state.declared(&object("pet")).is_some(),
        "the keyless declaration is still in force"
    );
}

/// **A merge holds the survivor's newest change with the folded thing's claims in
/// it.** The folded thing's claim is revised after its last field write; the
/// merge joins that claim to the survivor, so the survivor's newest change is the
/// revision and not the moment of the folded thing's last write.
#[test]
fn a_merge_holds_the_newest_change_of_a_claim_revised_after_its_last_write() {
    let mut state = two_things();
    apply_at(&mut state, 2, created("k2/r1", claim("f1", "k2", "b", 1)));
    apply_at(&mut state, 3, write("k2/r1", "k2", "mood", 1, "sour"));
    let mut revised = claim("f1", "k2", "b, said again", 1);
    revised.status = FactStatus::Active;
    apply_at(
        &mut state,
        9,
        ClaimEvent::RecordRevised {
            record: record("k2/r1"),
            claim: revised,
        },
    );
    apply_at(
        &mut state,
        10,
        ClaimEvent::ObjectsMerged {
            folded: object("k2"),
            survivor: object("k1"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    );
    agrees(
        &state,
        "a merge of a thing whose claim was revised after its last write",
    );
    assert_eq!(
        state.newest_change(&object("k1")),
        Some(at(9)),
        "the folded thing's revised claim is the survivor's newest change"
    );
}

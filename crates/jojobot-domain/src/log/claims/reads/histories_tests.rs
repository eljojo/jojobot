use super::super::tests::{archived, body, claim, created, moment, object, record, stamped, under};
use super::super::{ClaimBody, ClaimEvent, MovedRecord, ObjectBody, State};
use crate::actor::Actor;
use crate::log::Event;
use crate::memory::{EntityId, FactAddress, FactId, FactStatus, Provenance};

fn things() -> Vec<ClaimEvent> {
    ["work:alpha", "work:beta"]
        .into_iter()
        .flat_map(|name| {
            [
                ClaimEvent::ObjectCreated {
                    object: object(name),
                    body: ObjectBody {
                        ..body("work", name)
                    },
                },
                ClaimEvent::AddressBound {
                    object: object(name),
                    address: name.to_string(),
                },
            ]
        })
        .collect()
}

fn revised(id: &str, content: &str, provenance: Provenance) -> ClaimEvent {
    ClaimEvent::RecordRevised {
        record: record(id),
        claim: ClaimBody {
            provenance,
            ..claim("f1", "work:alpha", content, 3)
        },
    }
}

/// **A claim keeps every write of it, oldest first**, each with the claim as
/// that write left it and what the envelope says of the write: the actor, the
/// run, and the moment, which an imported write takes from its source.
#[test]
fn a_claim_keeps_every_write_of_it_oldest_first() {
    let mut state = State::default();
    for event in things() {
        state.apply(&stamped(None, None, event));
    }
    state.apply(&Event {
        written_at: Some(moment("2026-04-01T00:00:00Z")),
        imported_source_time: Some(moment("2026-02-01T00:00:00Z")),
        ..under(
            Some(Actor::Operator),
            Some("run-a"),
            created("r1", claim("f1", "work:alpha", "first", 3)),
        )
    });
    state.apply(&stamped(
        Some(moment("2026-04-02T00:00:00Z")),
        Some(moment("2026-03-01T00:00:00Z")),
        revised("r1", "second", Provenance::Observation),
    ));
    state.apply(&stamped(
        Some(moment("2026-04-03T00:00:00Z")),
        None,
        archived("r1", "f1", "work:alpha"),
    ));
    let chain = state.claim_history(&record("r1"));
    assert_eq!(
        chain
            .iter()
            .map(|w| w.claim.content.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "taken back"]
    );
    assert_eq!(chain[0].actor, Some(Actor::Operator));
    assert_eq!(chain[0].run.as_ref().map(|r| r.0.as_str()), Some("run-a"));
    assert_eq!(
        chain[0].written_at,
        Some(moment("2026-02-01T00:00:00Z")),
        "the first write keeps the moment of its source too"
    );
    assert_eq!(chain[1].claim.provenance, Provenance::Observation);
    assert_eq!(
        chain[1].written_at,
        Some(moment("2026-03-01T00:00:00Z")),
        "an imported write keeps the moment of its source"
    );
    assert_eq!(chain[2].claim.status, FactStatus::Archived);
    assert_eq!(chain[2].written_at, Some(moment("2026-04-03T00:00:00Z")));
    assert_eq!(
        state.record(&record("r1")).map(|r| &r.claim),
        Some(&chain[2].claim),
        "the last write is the claim now"
    );
}

/// **A claim the State does not hold has no writes.**
#[test]
fn a_claim_the_state_does_not_hold_has_no_writes() {
    let state = State::default();
    assert!(state.claim_history(&record("missing")).is_empty());
}

/// **The histories of a thing are the chains of the claims homed on it**, and
/// no other thing's; a merge moves a claim's chain with the claim.
#[test]
fn the_histories_of_a_thing_are_those_of_the_claims_homed_on_it() {
    let mut state = State::default();
    let events = [
        things(),
        vec![
            created("r1", claim("f1", "work:alpha", "a", 3)),
            revised("r1", "a2", Provenance::Testimony),
            created("r2", claim("f1", "work:beta", "b", 3)),
        ],
    ]
    .concat();
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    let ids = |state: &State, name: &str| {
        state
            .claim_histories(&object(name))
            .into_iter()
            .map(|(id, chain)| (id.0.clone(), chain.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&state, "work:alpha"), vec![("r1".to_string(), 2)]);
    assert_eq!(ids(&state, "work:beta"), vec![("r2".to_string(), 1)]);
    state.apply(&stamped(
        None,
        None,
        ClaimEvent::ObjectsMerged {
            folded: object("work:beta"),
            survivor: object("work:alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f2".to_string(),
            }],
            writes: Vec::new(),
        },
    ));
    assert_eq!(
        ids(&state, "work:alpha"),
        vec![("r1".to_string(), 2), ("r2".to_string(), 1)]
    );
    assert_eq!(
        ids(&state, "work:beta"),
        ids(&state, "work:alpha"),
        "the folded thing answers as its survivor"
    );
}

/// **A served write names the session and the bot the envelope names.** The bot
/// is the handle it answers to now, the session is the run, and a write whose
/// envelope names neither serves neither.
#[test]
fn a_served_write_names_the_session_and_the_bot_of_its_envelope() {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    for event in things() {
        state.apply(&stamped(None, None, event));
    }
    state.apply(&under(
        Some(Actor::Bot(EntityId("work:beta".into()))),
        Some("run-a"),
        created("r1", claim("f1", "work:alpha", "first", 3)),
    ));
    state.apply(&stamped(
        None,
        None,
        revised("r1", "second", Provenance::Observation),
    ));
    let parts = state.claim_chains_of(&object("work:alpha"));
    let writes = &parts[&FactId("f1".into())];
    assert_eq!(
        writes
            .iter()
            .map(|write| (
                write.session.as_deref(),
                write.bot.as_ref().map(|b| b.0.as_str())
            ))
            .collect::<Vec<_>>(),
        [(Some("run-a"), Some("work:beta")), (None, None)]
    );
}

/// **A claim keeps the lineage and the mark the import kept, when the claim they
/// name is not in the State.** The import carries a pointer at a record nothing
/// holds as the record id it was filed under. The current claim and its history
/// serve it at the home and the local id that id names; a source the State holds
/// is still served at the handle and the label it wears now.
#[test]
fn a_claim_serves_the_lineage_and_mark_of_a_record_the_state_does_not_hold() {
    let mut state = State::default();
    for event in things() {
        state.apply(&stamped(None, None, event));
    }
    state.apply(&stamped(
        None,
        None,
        created("zz/f7", claim("f1", "work:beta", "the source", 1)),
    ));
    state.apply(&stamped(
        None,
        None,
        created(
            "work:alpha/f2",
            ClaimBody {
                derived_from: Some(record("view:loops/f1")),
                stands_for: vec![record("zz/f7"), record("person:moe/f9")],
                ..claim("f2", "work:alpha", "follows from nothing held", 2)
            },
        ),
    ));
    let missing = FactAddress::new(EntityId("view:loops".into()), FactId("f1".into()));
    let held = FactAddress::new(EntityId("work:beta".into()), FactId("f1".into()));
    let gone = FactAddress::new(EntityId("person:moe".into()), FactId("f9".into()));

    let fact = state.fact(&record("work:alpha/f2")).unwrap();
    assert_eq!(fact.derived_from, Some(missing.clone()), "the lineage");
    assert_eq!(
        fact.stands_for,
        [held.clone(), gone.clone()],
        "the mark: a held source at its handle, a missing one as filed"
    );

    let parts = state.claim_chains_of(&object("work:alpha"));
    let write = parts[&FactId("f2".into())].last().unwrap();
    assert_eq!(
        write.derived_from,
        Some(missing),
        "the lineage in the history"
    );
    assert_eq!(
        write.stands_for.as_deref(),
        Some([held, gone].as_slice()),
        "the mark in the history"
    );
}

/// **A missing record is served under the handle its home wears now.** The import
/// filed a lineage at `k9/f3`, and nothing holds that record; the object `k9` is
/// held and has been renamed since. The store maps a stored home through the
/// current handle, so the State serves the lineage under `work:gamma` and not
/// under the id it was filed with.
///
/// **A missing record of a supplied home is served under the home's handle.** A
/// supplied record's object id carries a `/` (`supplied/<handle>`), so the record
/// id of a claim filed on it carries two. The last one splits the local id off, and
/// the home that is left is the object the State holds, served at its handle.
#[test]
fn a_missing_record_is_served_under_its_homes_handle() {
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (home, handle, former) in [
        ("k9", "work:gamma", Some("work:epsilon")),
        ("supplied/work:delta", "work:delta", None),
    ] {
        let mut state = State::default();
        for event in things() {
            state.apply(&stamped(None, None, event));
        }
        state.apply(&stamped(
            None,
            None,
            ClaimEvent::ObjectCreated {
                object: object(home),
                body: body("work", handle),
            },
        ));
        if let Some(former) = former {
            for event in [
                ClaimEvent::AddressBound {
                    object: object(home),
                    address: former.to_string(),
                },
                ClaimEvent::AddressRetired {
                    object: object(home),
                    address: former.to_string(),
                    day: jiff::civil::date(2026, 4, 1),
                },
            ] {
                state.apply(&stamped(None, None, event));
            }
        }
        state.apply(&stamped(
            None,
            None,
            ClaimEvent::AddressBound {
                object: object(home),
                address: handle.to_string(),
            },
        ));
        state.apply(&stamped(
            None,
            None,
            created(
                "work:alpha/f2",
                ClaimBody {
                    derived_from: Some(record(&format!("{home}/f3"))),
                    ..claim("f2", "work:alpha", "follows from a record nothing holds", 2)
                },
            ),
        ));
        let served = FactAddress::new(EntityId(handle.into()), FactId("f3".into()));
        let fact = state.fact(&record("work:alpha/f2")).unwrap();
        let parts = state.claim_chains_of(&object("work:alpha"));
        let write = parts[&FactId("f2".into())].last().unwrap();
        actual.push((fact.derived_from, write.derived_from.clone()));
        expected.push((Some(served.clone()), Some(served)));
    }
    assert_eq!(
        actual, expected,
        "the current fact and the history for each home"
    );
}

/// **A field written by a bot that was merged away is served under the handle
/// of the bot that took it in**, as `ClaimWrite.bot` is: the writer follows the
/// survivor, never the handle the folded bot wore. A second bot nobody merged is
/// the control, served under its own handle.
#[test]
fn a_field_written_by_a_merged_bot_names_the_survivor() {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    let mut events = things();
    for name in ["bot:sigma", "bot:theta", "bot:gamma"] {
        events.push(ClaimEvent::ObjectCreated {
            object: object(name),
            body: body("bot", name),
        });
        events.push(ClaimEvent::AddressBound {
            object: object(name),
            address: name.to_string(),
        });
    }
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    for (record, label, bot, ordinal) in
        [("r1", "f1", "bot:sigma", 1), ("r2", "f2", "bot:gamma", 2)]
    {
        state.apply(&under(
            Some(Actor::Bot(EntityId(bot.into()))),
            None,
            created(record, claim(label, "work:alpha", "set", 3)),
        ));
        state.apply(&under(
            Some(Actor::Bot(EntityId(bot.into()))),
            None,
            super::super::tests::write(record, "work:alpha", "stage", ordinal, bot),
        ));
    }
    state.apply(&stamped(
        None,
        None,
        ClaimEvent::ObjectsMerged {
            folded: object("bot:sigma"),
            survivor: object("bot:theta"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    ));
    let trails = state.key_trails(&object("work:alpha"));
    let writers: Vec<Option<String>> = trails.history["stage"]
        .iter()
        .map(|write| {
            write
                .writer
                .as_ref()
                .and_then(Actor::bot)
                .map(|b| b.0.clone())
        })
        .collect();
    assert_eq!(
        writers,
        [Some("bot:theta".to_string()), Some("bot:gamma".to_string())]
    );
    let served: Vec<String> = state.claim_chains_of(&object("work:alpha"))[&FactId("f1".into())]
        .iter()
        .filter_map(|write| write.bot.as_ref().map(|b| b.0.clone()))
        .collect();
    assert_eq!(
        served,
        ["bot:theta"],
        "ClaimWrite.bot follows the survivor too"
    );
}

/// **A claim derived from a claim a merge relabelled is served at the new label**,
/// in its history, as the store serves it: the address a read gives follows the
/// merge's relabel, never the label the claim was last written under. A stand-in
/// carried by `stands_for` follows too.
#[test]
fn a_lineage_follows_the_label_a_merge_gave_the_claim() {
    let mut state = State::default();
    for event in things() {
        state.apply(&stamped(None, None, event));
    }
    state.apply(&stamped(
        None,
        None,
        created("work:beta/f1", claim("f1", "work:beta", "the source", 3)),
    ));
    state.apply(&stamped(
        None,
        None,
        created(
            "work:alpha/f1",
            ClaimBody {
                derived_from: Some(record("work:beta/f1")),
                stands_for: vec![record("work:beta/f1")],
                ..claim("f1", "work:alpha", "worked out from the source", 4)
            },
        ),
    ));
    state.apply(&stamped(
        None,
        None,
        ClaimEvent::ObjectsMerged {
            folded: object("work:beta"),
            survivor: object("work:alpha"),
            records: vec![MovedRecord {
                record: record("work:beta/f1"),
                label: "f2".into(),
            }],
            writes: Vec::new(),
        },
    ));
    let moved = FactAddress::new(EntityId("work:alpha".into()), FactId("f2".into()));
    let parts = state.claim_chains_of(&object("work:alpha"));
    let write = &parts[&FactId("f1".into())][0];
    assert_eq!(write.derived_from, Some(moved.clone()), "the history");
    assert_eq!(write.stands_for, Some(vec![moved]), "the stand-in");
}

/// **A child a merge moved is a merge-touched thing**: the store appends a version
/// of the child for the move, which the State does not hold yet, so a read of the
/// child's versions and ages is the store's. A child of a thing no merge folded,
/// and the survivor's other holders, are not touched.
/// TEMPORARY(9.3.4) `q-934-merge-version-row-in-state`.
#[test]
fn a_child_of_a_folded_thing_is_a_merge_touched_thing() {
    let mut state = State::default();
    let child = |name: &str, parent: &str| {
        [
            ClaimEvent::ObjectCreated {
                object: object(name),
                body: ObjectBody {
                    parent: Some(object(parent)),
                    ..body("work", name)
                },
            },
            ClaimEvent::AddressBound {
                object: object(name),
                address: name.to_string(),
            },
        ]
    };
    let mut events = things();
    events.extend(child("work:delta", "work:beta"));
    events.extend(child("work:epsilon", "work:alpha"));
    events.push(ClaimEvent::ObjectsMerged {
        folded: object("work:beta"),
        survivor: object("work:alpha"),
        records: Vec::new(),
        writes: Vec::new(),
    });
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    assert!(state.was_merged(&object("work:delta")), "the moved child");
    assert!(
        !state.was_merged(&object("work:epsilon")),
        "a child of the survivor"
    );
}

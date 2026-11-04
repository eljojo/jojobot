use std::collections::BTreeMap;

use jiff::Timestamp;

use super::*;
use crate::app::{AppId, AppStamp};
use crate::log::claims::State;
use crate::log::claims::tests::{body, claim, key, object};
use crate::memory::merge;
use crate::memory::types::DeclaredType;
use crate::memory::{FactId, FactStatus, KeyWrite, RETRACTS};

fn at(second: i64) -> Option<Timestamp> {
    Some(Timestamp::from_second(1_800_000_000 + second).unwrap())
}

fn entity(id: &str, handle: &str) -> EntityRow {
    let made = body("person", handle);
    EntityRow {
        key: object(id),
        handle: handle.into(),
        kind: made.kind.0,
        name: made.name,
        source: made.source,
        boot: None,
        parent: None,
        aliases: Vec::new(),
        crm: None,
        prose: String::new(),
        badge: None,
        archived: None,
        merged_into: None,
        taken_in: None,
    }
}

fn write_of(ordinal: u64, content: &str, run: Option<&str>, stamp: i64) -> FactWriteRow {
    FactWriteRow {
        ordinal,
        claim: claim("f1", "k1", content, 1),
        run: run.map(String::from),
        bot: None,
        run_bot: None,
        written_at: at(stamp),
    }
}

fn fact(home: &str, local: &str, writes: Vec<FactWriteRow>) -> FactRow {
    let head = writes
        .last()
        .map(|last| last.claim.clone())
        .unwrap_or_else(|| claim(local, home, "head", 1));
    FactRow {
        home: object(home),
        local: local.into(),
        head,
        writes,
        event_kind: None,
    }
}

fn set(
    entity: &str,
    k: &str,
    ordinal: u64,
    value: &str,
    local: &str,
    pointer: Option<u64>,
) -> FieldWriteRow {
    FieldWriteRow {
        entity: object(entity),
        key: key(k),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Set(value.into()),
        fact_home: object(entity),
        fact_local: local.into(),
        fact_write_ordinal: pointer,
        written_at: at(ordinal as i64 * 10),
        targets: Vec::new(),
    }
}

fn state_of(rows: &Rows) -> State {
    State::replay(&imported(rows))
}

fn stream<'a>(state: &'a State, subject: &str, k: &str) -> &'a [super::super::Write] {
    state
        .streams
        .get(&(object(subject), key(k)))
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// Entity and alias rows: the object, its spelling and its source moment.
#[test]
fn an_entity_row_becomes_an_object_answering_to_its_handle() {
    let mut row = entity("k1", "person:homer");
    row.aliases = vec!["Homie".into()];
    row.crm = Some("crm-7".into());
    row.taken_in = at(5);
    let events = imported(&Rows {
        entities: vec![row],
        ..Rows::default()
    });
    assert_eq!(events[0].imported_source_time, at(5));
    let state = State::replay(&events);
    assert_eq!(state.resolve("person:homer"), Some(&object("k1")));
    let held = state.object(&object("k1")).unwrap();
    assert_eq!(held.aliases, vec!["Homie".to_string()]);
    assert_eq!(held.metadata.get("crm").map(String::as_str), Some("crm-7"));
    assert!(!held.archived);
}

/// An entity row with no moment stays without one.
#[test]
fn an_entity_row_with_no_moment_arrives_with_none() {
    let events = imported(&Rows {
        entities: vec![entity("k1", "person:homer")],
        ..Rows::default()
    });
    assert!(
        events
            .iter()
            .all(|event| event.imported_source_time.is_none())
    );
}

/// A renamed handle: the new spelling answers, the former one is remembered.
#[test]
fn a_renamed_handle_keeps_its_former_spelling() {
    let rows = Rows {
        entities: vec![entity("k1", "person:marge")],
        former_handles: vec![FormerHandleRow {
            former: "person:homer".into(),
            object: object("k1"),
            ordinal: 1,
        }],
        ..Rows::default()
    };
    let state = state_of(&rows);
    assert_eq!(state.resolve("person:marge"), Some(&object("k1")));
    assert_eq!(state.resolve("person:homer"), None);
    assert_eq!(
        state.retired_addresses.get("person:homer"),
        Some(&object("k1"))
    );
}

/// An archived entity row stays archived, with its reason.
#[test]
fn an_archived_entity_keeps_its_reason_and_moment() {
    let mut row = entity("k1", "person:homer");
    row.archived = Some(("duplicate".into(), at(9).unwrap()));
    let state = state_of(&Rows {
        entities: vec![row],
        ..Rows::default()
    });
    let held = state.object(&object("k1")).unwrap();
    assert!(held.archived);
    assert_eq!(
        held.metadata.get("archived_reason").map(String::as_str),
        Some("duplicate")
    );
}

/// An edited claim: each write keeps its own run and its own writer.
#[test]
fn an_edited_claim_keeps_each_writes_own_run() {
    let mut first = write_of(1, "first", Some("run-a"), 1);
    first.bot = Some(EntityId("bot:lisa".into()));
    let second = write_of(2, "second", Some("run-b"), 2);
    let mut fallback = write_of(3, "fallback", Some("run-c"), 3);
    fallback.run_bot = Some(EntityId("bot:bart".into()));
    let stamp = AppStamp {
        app: "springfield-ci".into(),
        grant: "record_deploy_event".into(),
        received_at: "2026-10-18T12:00:00Z".into(),
    };
    let mut app = write_of(4, "app", Some("run-d"), 4);
    app.claim.app = Some(stamp.clone());
    let mut targeted = set("k1", "mood", 3, "quiet", "f1", Some(3));
    targeted.targets = vec![object("k2")];
    let rows = Rows {
        entities: vec![entity("k1", "person:homer"), entity("k2", "person:marge")],
        facts: vec![fact("k1", "f1", vec![first, second, fallback, app])],
        field_writes: vec![
            set("k1", "mood", 1, "calm", "f1", Some(1)),
            set("k1", "mood", 2, "loud", "f1", Some(2)),
            targeted,
            set("k1", "mood", 4, "bright", "f1", Some(4)),
        ],
        ..Rows::default()
    };
    let events = imported(&rows);
    for event in &events {
        assert_eq!(event.meaning_version, 1);
    }
    let state = State::replay(&events);
    let writes = stream(&state, "k1", "mood");
    assert_eq!(writes[0].run, Some(RunId("run-a".into())));
    assert_eq!(
        writes[0].writer,
        Some(Actor::Bot(EntityId("bot:lisa".into())))
    );
    assert_eq!(writes[1].run, Some(RunId("run-b".into())));
    assert_eq!(writes[1].writer, None);
    assert_eq!(writes[2].run, Some(RunId("run-c".into())));
    assert_eq!(
        writes[2].writer,
        Some(Actor::Bot(EntityId("bot:bart".into())))
    );
    assert_eq!(writes[3].run, Some(RunId("run-d".into())));
    assert_eq!(
        writes[3].writer,
        Some(Actor::App(AppId::parse("springfield-ci").unwrap()))
    );
    assert!(writes[0].targets.is_empty());
    assert_eq!(writes[2].targets, vec![object("k2")]);
    let record = state.record(&record_id(&object("k1"), "f1")).unwrap();
    assert_eq!(record.claim.content, "app");
    assert_eq!(record.claim.app, Some(stamp));
    assert_eq!(record.revisions, 3);
}

/// A write with no pointer to its claim write has no writer and no run, even
/// when its claim has both.
#[test]
fn a_write_with_no_pointer_has_no_run_though_its_claim_has_one() {
    let rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![fact(
            "k1",
            "f1",
            vec![write_of(1, "only", Some("run-a"), 1)],
        )],
        field_writes: vec![set("k1", "mood", 1, "calm", "f1", None)],
        ..Rows::default()
    };
    let state = state_of(&rows);
    let writes = stream(&state, "k1", "mood");
    assert_eq!(writes[0].run, None);
    assert_eq!(writes[0].writer, None);
}

/// An unstamped claim and an unstamped write stay unstamped.
#[test]
fn an_unstamped_claim_and_write_get_no_moment() {
    let mut write = set("k1", "mood", 1, "calm", "f1", None);
    write.written_at = None;
    let rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![fact("k1", "f1", Vec::new())],
        field_writes: vec![write],
        ..Rows::default()
    };
    let events = imported(&rows);
    assert!(
        events
            .iter()
            .all(|event| event.imported_source_time.is_none())
    );
    let state = State::replay(&events);
    assert_eq!(stream(&state, "k1", "mood")[0].written_at, None);
}

/// **A claim with no stored writes still arrives as one record**, created with the
/// head the row holds: the writes are the only other thing that creates it, and a
/// field write that points at the claim is kept whether or not the claim is there.
/// Paired with a claim whose head is its one write, which creates it once.
#[test]
fn a_claim_with_no_stored_writes_is_imported_as_one_record_holding_its_head() {
    let created = |events: &[Event<ClaimEvent>]| -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match &event.payload {
                ClaimEvent::RecordCreated { claim, .. } => Some(claim.content.clone()),
                _ => None,
            })
            .collect()
    };
    let bare = imported(&Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![fact("k1", "f1", Vec::new())],
        ..Rows::default()
    });
    assert_eq!(created(&bare), ["head"]);
    let state = State::replay(&bare);
    let record = state.record(&record_id(&object("k1"), "f1")).unwrap();
    assert_eq!(record.claim.content, "head");
    assert_eq!(record.revisions, 0);
    let written = imported(&Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![fact(
            "k1",
            "f1",
            vec![write_of(1, "only", Some("run-a"), 1)],
        )],
        ..Rows::default()
    });
    assert_eq!(created(&written), ["only"]);
}

/// A head row that no write explains arrives as one more revision with no stamp.
#[test]
fn a_head_that_differs_from_its_last_write_is_one_unstamped_revision() {
    let mut row = fact("k1", "f1", vec![write_of(1, "written", Some("run-a"), 1)]);
    row.head.content = "edited in the row".into();
    let events = imported(&Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![row],
        ..Rows::default()
    });
    let last = events.last().unwrap();
    assert!(matches!(last.payload, ClaimEvent::RecordRevised { .. }));
    assert_eq!((last.imported_source_time, last.run.clone()), (None, None));
}

/// A cleared key leaves the thing without it, and the clear stays a write.
#[test]
fn a_cleared_key_is_a_write_that_takes_the_key_off() {
    let mut clear = set("k1", "mood", 2, "", "f1", Some(1));
    clear.write = FieldValue::Clear;
    let rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![fact("k1", "f1", vec![write_of(1, "x", None, 1)])],
        field_writes: vec![set("k1", "mood", 1, "calm", "f1", Some(1)), clear],
        ..Rows::default()
    };
    let state = state_of(&rows);
    assert_eq!(stream(&state, "k1", "mood").len(), 2);
    assert!(state.fields(&object("k1"), &[]).is_empty());
}

fn with_status(mut row: FactRow, status: FactStatus) -> FactRow {
    row.head.status = status;
    for write in &mut row.writes {
        write.claim.status = status;
    }
    row
}

/// An archived claim and a retraction: their writes stay in the stream, and the
/// thing does not hold what they set.
#[test]
fn an_archived_and_a_retracted_claim_stay_but_do_not_fold() {
    let rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![
            with_status(
                fact("k1", "f1", vec![write_of(1, "a", None, 1)]),
                FactStatus::Archived,
            ),
            with_status(
                fact("k1", "f2", vec![write_of(1, "b", None, 2)]),
                FactStatus::Archived,
            ),
        ],
        field_writes: vec![
            set("k1", "mood", 1, "calm", "f1", Some(1)),
            set("k1", RETRACTS, 1, "k1/f1", "f2", Some(1)),
        ],
        ..Rows::default()
    };
    let state = state_of(&rows);
    assert_eq!(stream(&state, "k1", RETRACTS).len(), 1);
    assert!(state.fields(&object("k1"), &[]).is_empty());
}

/// A numbered member taken back: its number is still a write on the stream.
#[test]
fn a_numbered_member_taken_back_keeps_its_number_write() {
    let rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![with_status(
            fact("k1", "f1", vec![write_of(1, "n", None, 1)]),
            FactStatus::Archived,
        )],
        field_writes: vec![set("k1", "num", 1, "7", "f1", Some(1))],
        ..Rows::default()
    };
    let state = state_of(&rows);
    assert_eq!(
        stream(&state, "k1", "num")[0].write,
        FieldValue::Set("7".into())
    );
    assert!(state.fields(&object("k1"), &[]).is_empty());
}

fn merged_pair() -> Rows {
    let survivor_write = write_of(1, "ours", Some("run-s"), 1);
    let mut folded_write = write_of(1, "theirs", Some("run-f"), 2);
    folded_write.claim.home = object("k1");
    folded_write.claim.label = "f9".into();
    Rows {
        entities: vec![entity("k1", "person:homer"), entity("k2", "person:marge")],
        // The merge gave the folded thing's claim `f9` under the survivor.
        facts: vec![
            fact("k1", "f1", vec![survivor_write]),
            fact("k1", "f9", vec![folded_write]),
        ],
        field_writes: vec![
            set("k1", "mood", 1, "ours", "f1", Some(1)),
            FieldWriteRow {
                written_at: at(100),
                ..set("k2", "mood", 1, "theirs", "f1", Some(1))
            },
        ],
        merges: vec![MergeRow {
            no: 1,
            folded: object("k2"),
            survivor: object("k1"),
            merged_at: at(50).unwrap(),
            session: Some("run-m".into()),
            claims: vec![("f1".into(), "f9".into())],
        }],
        ..Rows::default()
    }
}

/// A merged pair: the folded thing's write is interleaved by the ledger's rule,
/// and its spelling answers to the survivor.
#[test]
fn a_merged_pair_arrives_interleaved_under_the_survivor() {
    let state = state_of(&merged_pair());
    let writes = stream(&state, "k1", "mood");
    assert_eq!(
        writes.iter().map(|w| w.ordinal).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(writes[1].run, Some(RunId("run-f".into())));
    assert_eq!(state.resolve("person:marge"), Some(&object("k1")));
    assert!(stream(&state, "k2", "mood").is_empty());
    assert_eq!(state.fields(&object("k1"), &[])["mood"], "theirs");
}

/// A merge keeps the moment it landed and the run that made it, as every
/// other imported row keeps its own.
#[test]
fn a_merge_keeps_its_moment_and_its_run() {
    let events = imported(&merged_pair());
    let merged: Vec<_> = events
        .iter()
        .filter(|event| matches!(event.payload, ClaimEvent::ObjectsMerged { .. }))
        .collect();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].imported_source_time, at(50));
    assert_eq!(merged[0].run, Some(RunId("run-m".into())));
}

/// **A merge made before the merge-event table arrives as a redirect the store
/// never numbered.** The folded entity's own mark is the only source, so the
/// thing still answers to the survivor, and the highest merge number stays the
/// highest the store holds: the next merge is numbered one above it, as the
/// store numbers it. The shape is the upgrade gate's: a chain whose first merge
/// lost its event row. **Paired**: the same chain with the event row present
/// counts both merges.
#[test]
fn a_merge_with_no_event_row_arrives_as_a_redirect_the_store_does_not_count() {
    let merged = |id: &str, handle: &str, into: &str| {
        let mut row = entity(id, handle);
        row.merged_into = Some(object(into));
        row
    };
    let step = |no: i64, folded: &str, survivor: &str| MergeRow {
        no,
        folded: object(folded),
        survivor: object(survivor),
        merged_at: at(10).unwrap(),
        session: None,
        claims: Vec::new(),
    };
    let rows = |merges: Vec<MergeRow>| Rows {
        entities: vec![
            merged("k1", "person:homer", "k2"),
            merged("k2", "person:marge", "k3"),
            entity("k3", "person:bart"),
        ],
        merges,
        ..Rows::default()
    };
    let legacy = rows(vec![step(1, "k2", "k3")]);
    let state = state_of(&legacy);
    assert_eq!(state.resolve("person:homer"), Some(&object("k3")));
    assert_eq!(state.resolve("person:marge"), Some(&object("k3")));
    assert_eq!(
        state.highest_merge_number(),
        1,
        "the store holds one numbered merge"
    );
    let repaired = rows(vec![step(1, "k1", "k2"), step(2, "k2", "k3")]);
    let state = state_of(&repaired);
    assert_eq!(state.resolve("person:homer"), Some(&object("k3")));
    assert_eq!(state.highest_merge_number(), 2);
}

/// **The rows the upgrade gate keeps: the event row of the first merge of a
/// chain is deleted and the later merges keep the numbers they had.** The
/// numbers run 2, 3, 4 and the folded entity of the deleted row carries its mark
/// (`kitchen_sink.rs` `old_merges`: two chains of three, the first chain's first
/// merge loses its row). The import accepts the hole, because a marked entity
/// with no row is there to hold it, answers every old handle, and leaves the
/// highest merge number at the store's highest, 4. **Paired**: a hole with no
/// marked entity to account for it is still refused.
#[test]
fn the_rows_the_upgrade_gate_keeps_import_with_the_number_of_the_deleted_row() {
    let merged = |id: &str, handle: &str, into: &str| {
        let mut row = entity(id, handle);
        row.merged_into = Some(object(into));
        row
    };
    let step = |no: i64, folded: &str, survivor: &str| MergeRow {
        no,
        folded: object(folded),
        survivor: object(survivor),
        merged_at: at(10).unwrap(),
        session: None,
        claims: Vec::new(),
    };
    let kept = vec![
        step(2, "k2", "k3"),
        step(3, "k4", "k5"),
        step(4, "k5", "k6"),
    ];
    let rows = Rows {
        entities: vec![
            merged("k1", "thing:zeta", "k2"),
            merged("k2", "thing:eta", "k3"),
            entity("k3", "thing:theta"),
            merged("k4", "thing:phi", "k5"),
            merged("k5", "thing:chi", "k6"),
            entity("k6", "thing:psi"),
        ],
        merges: kept.clone(),
        ..Rows::default()
    };
    let state = state_of(&rows);
    for handle in ["thing:zeta", "thing:eta"] {
        assert_eq!(state.resolve(handle), Some(&object("k3")), "{handle}");
    }
    for handle in ["thing:phi", "thing:chi"] {
        assert_eq!(state.resolve(handle), Some(&object("k6")), "{handle}");
    }
    assert_eq!(state.highest_merge_number(), 4, "the store's highest");
    let unmarked = Rows {
        entities: vec![
            entity("k1", "thing:zeta"),
            merged("k2", "thing:eta", "k3"),
            entity("k3", "thing:theta"),
        ],
        merges: vec![step(2, "k2", "k3")],
        ..Rows::default()
    };
    assert_eq!(
        import(&unmarked),
        Err(ImportError::MergeNumberGap {
            expected: 1,
            found: 2
        })
    );
}

/// A folded write of a claim the merge did not map is left out, as the claim is.
#[test]
fn a_folded_write_of_an_unmapped_claim_is_left_out() {
    let mut rows = merged_pair();
    rows.merges[0].claims.clear();
    let events = imported(&rows);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.payload, ClaimEvent::FieldWritten { .. }))
            .count(),
        1
    );
}

fn imported(rows: &Rows) -> Vec<Event<ClaimEvent>> {
    import(rows).unwrap()
}

fn declaration(name: &str, must: Option<&str>) -> DeclaredType {
    DeclaredType {
        name: name.into(),
        fields: Vec::new(),
        origin: crate::memory::types::Origin::Declared,
        must: must.map(String::from),
    }
}

/// A type declared twice: the history keeps both, the newest is in force, and
/// each declaration keeps its session and moment.
#[test]
fn a_type_written_twice_arrives_as_two_declarations() {
    let rows = Rows {
        type_writes: vec![
            TypeWriteRow {
                ordinal: 2,
                session: Some("run-b".into()),
                written_at: at(2),
                declaration: declaration("pet", Some("newer")),
            },
            TypeWriteRow {
                ordinal: 1,
                session: Some("run-a".into()),
                written_at: None,
                declaration: declaration("pet", None),
            },
        ],
        ..Rows::default()
    };
    let events = imported(&rows);
    assert_eq!(events[0].run, Some(RunId("run-a".into())));
    assert_eq!(events[0].imported_source_time, None);
    assert_eq!(events[1].imported_source_time, at(2));
    let state = State::replay(&events);
    let kind = object("pet");
    assert_eq!(state.declarations_of(&kind).len(), 2);
    assert_eq!(
        state.declared(&kind).unwrap().must.as_deref(),
        Some("newer")
    );
}

/// A claim holding an event_kind is refused by name, and so is a metadata row.
#[test]
fn an_event_kind_or_a_metadata_row_is_refused_by_name() {
    let mut held = fact("k1", "f1", vec![write_of(1, "x", None, 1)]);
    held.event_kind = Some("visit".into());
    let refused = import(&Rows {
        facts: vec![held],
        ..Rows::default()
    });
    assert_eq!(
        refused,
        Err(ImportError::EventKindHeld {
            home: "k1".into(),
            local: "f1".into()
        })
    );
    let refused = import(&Rows {
        metadata: vec![MetadataRow {
            home: object("k1"),
            local: "f1".into(),
            key: "colour".into(),
            value: "red".into(),
        }],
        ..Rows::default()
    });
    assert_eq!(
        refused,
        Err(ImportError::MetadataHeld {
            home: "k1".into(),
            local: "f1".into(),
            key: "colour".into()
        })
    );
}

/// Today's store numbers a merge one above the highest, and the state counts
/// merges, so numbers with a gap would make the count lower than the highest
/// stored number. A gap, a repeat or a first number past one is refused by name.
#[test]
fn merges_numbered_with_a_gap_are_refused_by_name() {
    let merge = |no: i64, folded: &str, survivor: &str| MergeRow {
        no,
        folded: object(folded),
        survivor: object(survivor),
        merged_at: at(0).unwrap(),
        session: None,
        claims: Vec::new(),
    };
    let rows = |numbers: [i64; 2]| Rows {
        entities: vec![
            entity("k1", "person:homer"),
            entity("k2", "person:marge"),
            entity("k3", "person:bart"),
        ],
        merges: vec![merge(numbers[0], "k2", "k1"), merge(numbers[1], "k3", "k1")],
        ..Rows::default()
    };
    assert_eq!(
        import(&rows([1, 3])),
        Err(ImportError::MergeNumberGap {
            expected: 2,
            found: 3
        })
    );
    assert_eq!(
        import(&rows([1, 1])),
        Err(ImportError::MergeNumberGap {
            expected: 2,
            found: 1
        })
    );
    assert_eq!(
        import(&rows([2, 3])),
        Err(ImportError::MergeNumberGap {
            expected: 1,
            found: 2
        })
    );
    let events = imported(&rows([1, 2]));
    assert_eq!(State::replay(&events).highest_merge_number(), 2);
}

/// The folded side's former handle still resolves to the survivor, and a moved
/// claim keeps the label and the lineage it wears under the survivor.
#[test]
fn a_merged_pairs_former_handle_and_moved_claim_survive() {
    let mut rows = merged_pair();
    rows.former_handles.push(FormerHandleRow {
        former: "person:bart".into(),
        object: object("k2"),
        ordinal: 1,
    });
    let lineage = Some(record_id(&object("k1"), "f1"));
    rows.facts[1].head.derived_from = lineage.clone();
    rows.facts[1].writes[0].claim.derived_from = lineage.clone();
    // Replay already holds this claim under the survivor, so only the imported
    // payload can show whether the merge kept its mapped record and label.
    let events = imported(&rows);
    let records: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::ObjectsMerged { records, .. } => Some(records.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        records,
        vec![vec![MovedRecord {
            record: RecordId("k1/f9".into()),
            label: "f9".into(),
        }]]
    );
    let state = State::replay(&events);
    assert_eq!(
        state.retired_addresses.get("person:bart"),
        Some(&object("k2"))
    );
    assert_eq!(state.survivor(&object("k2")), &object("k1"));
    let moved = state.record(&record_id(&object("k1"), "f9")).unwrap();
    assert_eq!(moved.claim.label, "f9");
    assert_eq!(moved.claim.derived_from, lineage);
}

/// The folded fields hold on a merged fixture: the survivor serves the newest
/// write of each key across both sides.
#[test]
fn a_merged_fixture_folds_to_what_the_rows_fold_to() {
    let rows = merged_pair();
    let state = state_of(&rows);
    assert_eq!(state.fields(&object("k1"), &[]), folded_over(&rows, "k1"));
    assert_eq!(state.fields(&object("k1"), &[])["mood"], "theirs");
}

/// Production holds thousands of claims and thousands of key writes, and the
/// import runs once at activation. Each key write finds its claim by one lookup,
/// so the cost grows with the rows and never with writes times claims.
#[test]
fn the_import_finds_each_writes_claim_without_scanning_every_claim() {
    const CLAIMS: usize = 20_000;
    let mut rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        ..Rows::default()
    };
    for at in 0..CLAIMS {
        let local = format!("f{at}");
        rows.facts
            .push(fact("k1", &local, vec![write_of(1, "c", None, 1)]));
        rows.field_writes
            .push(set("k1", "mood", at as u64 + 1, "calm", &local, Some(1)));
    }
    let began = std::time::Instant::now();
    let events = imported(&rows);
    assert!(
        began.elapsed() < std::time::Duration::from_secs(5),
        "{CLAIMS} claims and writes took {:?}",
        began.elapsed()
    );
    assert!(events.len() > 2 * CLAIMS);
}

/// The spelling of a claim's identity is stored on the log and nothing outside
/// this process declares it.
#[test]
fn a_claim_identity_is_its_home_and_local_id() {
    assert_eq!(record_id(&object("k1"), "f3"), RecordId("k1/f3".into()));
    assert_eq!(EVENT_KIND, "claim");
}

struct Dice(u64);

impl Dice {
    fn roll(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % below
    }
}

/// Rows drawn the way the store makes them: claims are edited, some are
/// archived, key writes point at a claim write or at none.
fn drawn(dice: &mut Dice) -> Rows {
    let keys = ["mood", "num", "colour"];
    let mut rows = Rows::default();
    let handles = ["person:homer", "person:marge", "person:bart"];
    for (n, id) in ["k1", "k2", "k3"].iter().enumerate() {
        rows.entities.push(entity(id, handles[n]));
        let claims = 1 + dice.roll(3);
        for c in 0..claims {
            let local = format!("f{}", c + 1);
            let mut writes = Vec::new();
            for e in 0..1 + dice.roll(2) {
                let mut w = write_of(
                    e as u64 + 1,
                    &format!("c{n}{c}{e}"),
                    Some(&format!("run-{n}{c}{e}")),
                    dice.roll(50) as i64,
                );
                w.claim.home = object(id);
                w.claim.label = local.clone();
                if dice.roll(4) == 0 {
                    w.claim.status = FactStatus::Archived;
                }
                writes.push(w);
            }
            rows.facts.push(fact(id, &local, writes));
        }
        let mut ordinals: BTreeMap<&str, u64> = BTreeMap::new();
        for _ in 0..dice.roll(6) {
            let k = keys[dice.roll(keys.len())];
            let ordinal = ordinals.entry(k).and_modify(|o| *o += 1).or_insert(1);
            let mut w = set(
                id,
                k,
                *ordinal,
                &format!("v{}", dice.roll(9)),
                "f1",
                Some(1),
            );
            // **Every class a stored write can carry**, so the fold is drawn over
            // writes that reach their thing and writes that stay on their record.
            w.class = [
                WriteClass::Legacy,
                WriteClass::Own,
                WriteClass::Sets,
                WriteClass::Stays,
            ][dice.roll(4)];
            w.written_at = if dice.roll(5) == 0 {
                None
            } else {
                at(dice.roll(50) as i64)
            };
            if dice.roll(5) == 0 {
                w.write = FieldValue::Clear;
            }
            if dice.roll(5) == 0 {
                w.fact_write_ordinal = None;
            }
            rows.field_writes.push(w);
        }
    }
    rows
}

/// The fold as the store reads it: the stored writes of a thing, joined to the
/// head of their claim, handed to the one fold. It reads rows and never events.
fn folded_over(rows: &Rows, thing: &str) -> BTreeMap<String, String> {
    let mut held: std::collections::HashMap<String, Vec<KeyWrite>> = rows
        .entities
        .iter()
        .map(|e| (e.key.0.clone(), stored_writes(rows, &e.key.0)))
        .collect();
    let mut steps: Vec<_> = rows.merges.iter().collect();
    steps.sort_by_key(|step| step.no);
    let steps: Vec<merge::MergeStep> = steps
        .into_iter()
        .map(|step| merge::MergeStep {
            merge_no: step.no,
            folded: step.folded.0.clone(),
            survivor: step.survivor.0.clone(),
            claims: step.claims.iter().cloned().collect(),
        })
        .collect();
    merge::apply_merges(&mut held, &steps);
    crate::memory::folded_fields(&held.remove(thing).unwrap_or_default(), &[])
}

/// The claim a stored write names: where it is filed, or where the merge that
/// moved it filed it.
fn claim_of<'a>(rows: &'a Rows, write: &FieldWriteRow) -> Option<&'a FactRow> {
    let at = |home: &ObjectId, local: &str| {
        rows.facts
            .iter()
            .find(|f| &f.home == home && f.local == local)
    };
    at(&write.fact_home, &write.fact_local).or_else(|| {
        let step = rows
            .merges
            .iter()
            .find(|step| step.folded == write.entity)?;
        let (_, new) = step
            .claims
            .iter()
            .find(|(old, _)| *old == write.fact_local)?;
        at(&step.survivor, new)
    })
}

/// The writes stored on one thing, each joined to the head of its claim.
fn stored_writes(rows: &Rows, thing: &str) -> Vec<KeyWrite> {
    let mut writes = Vec::new();
    for write in rows
        .field_writes
        .iter()
        .filter(|w| w.entity == object(thing))
    {
        let Some(fact) = claim_of(rows, write) else {
            continue;
        };
        if !crate::memory::reaches_the_thing(write.class) {
            continue;
        }
        let origin = fact
            .writes
            .iter()
            .find(|held| Some(held.ordinal) == write.fact_write_ordinal)
            .map(origin_of);
        let by = write_origin::written_by(origin, fact.head.app.as_ref());
        writes.push(KeyWrite {
            key: write.key.0.clone(),
            ordinal: write.ordinal,
            value: match &write.write {
                FieldValue::Set(v) => Some(v.clone()),
                FieldValue::Clear => None,
            },
            fact: FactId(write.fact_local.clone()),
            status: fact.head.status,
            provenance: fact.head.provenance,
            standing: fact.head.standing,
            note: fact.head.details.clone(),
            recorded_by: by.recorded_by,
            writer: by.writer,
            run: by.run,
            written_at: write.written_at,
        });
    }
    writes
}

/// Mapping the rows and applying the events gives the folded fields the fold
/// gives over the rows themselves, for every drawn row set.
#[test]
fn mapped_rows_fold_to_what_the_rows_fold_to() {
    for seed in 0..80u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let rows = drawn(&mut dice);
        let state = state_of(&rows);
        for thing in ["k1", "k2", "k3"] {
            assert_eq!(
                state.fields(&object(thing), &[]),
                folded_over(&rows, thing),
                "seed {seed} thing {thing}"
            );
        }
        // The histories: each write keeps the run, writer and moment its row names.
        for write in &rows.field_writes {
            let fact = rows
                .facts
                .iter()
                .find(|f| f.home == write.fact_home && f.local == write.fact_local)
                .unwrap();
            let origin = fact
                .writes
                .iter()
                .find(|held| Some(held.ordinal) == write.fact_write_ordinal)
                .map(origin_of);
            let by = write_origin::written_by(origin, fact.head.app.as_ref());
            let kept = stream(&state, &write.entity.0, &write.key.0)
                .iter()
                .find(|held| held.ordinal == write.ordinal)
                .unwrap();
            assert_eq!(
                (&kept.run, &kept.writer, kept.written_at),
                (&by.run.map(RunId), &by.writer, write.written_at),
                "seed {seed} {:?} {}",
                write.key,
                write.ordinal
            );
        }
    }
}

/// No stamp exists in the output that was not in the input.
#[test]
fn no_stamp_is_invented() {
    for seed in 0..80u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let rows = drawn(&mut dice);
        let given: Vec<Timestamp> = rows
            .field_writes
            .iter()
            .filter_map(|w| w.written_at)
            .chain(
                rows.facts
                    .iter()
                    .flat_map(|f| f.writes.iter().filter_map(|w| w.written_at)),
            )
            .chain(rows.entities.iter().filter_map(|e| e.taken_in))
            .collect();
        for event in imported(&rows) {
            if let Some(stamp) = event.imported_source_time {
                assert!(given.contains(&stamp), "seed {seed}: {stamp} not a row's");
            }
            assert_eq!(event.written_at, None);
            assert_eq!(event.occurred_at, None);
        }
    }
}

/// **A write keeps the class it was stored under, and the class decides whether
/// it reaches its thing.** One claim sets four keys, one under each class a
/// stored write carries. The import keeps each class on its event and on the
/// state's write, and the folded fields hold the three that reach (legacy, own,
/// sets) and leave out the one that stays on its record. The expected fields are
/// written out and not read through `reaches_the_thing`, so a mapper that gave
/// every write one class cannot agree with them.
#[test]
fn a_write_keeps_its_class_and_the_class_decides_what_the_thing_holds() {
    let classes = [
        ("old", WriteClass::Legacy),
        ("own", WriteClass::Own),
        ("sets", WriteClass::Sets),
        ("stays", WriteClass::Stays),
    ];
    let rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        facts: vec![fact(
            "k1",
            "f1",
            vec![write_of(1, "a claim", Some("run-a"), 1)],
        )],
        field_writes: classes
            .iter()
            .map(|(k, class)| {
                let mut w = set("k1", k, 1, &format!("v-{k}"), "f1", Some(1));
                w.class = *class;
                w
            })
            .collect(),
        ..Rows::default()
    };
    let events = imported(&rows);
    for (k, class) in classes {
        let kept: Vec<WriteClass> = events
            .iter()
            .filter_map(|event| match &event.payload {
                ClaimEvent::FieldWritten {
                    key: held, class, ..
                } if held.0 == k => Some(*class),
                _ => None,
            })
            .collect();
        assert_eq!(kept, vec![class], "the event for {k}");
    }
    let state = State::replay(&events);
    for (k, class) in classes {
        assert_eq!(stream(&state, "k1", k)[0].class, class, "the state's {k}");
    }
    let expected: BTreeMap<String, String> =
        [("old", "v-old"), ("own", "v-own"), ("sets", "v-sets")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
    assert_eq!(state.fields(&object("k1"), &[]), expected);
}

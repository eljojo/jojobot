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
            day: jiff::civil::date(2026, 3, 9),
        }],
        ..Rows::default()
    };
    // The retirement carries the day the row holds, whatever the clock says.
    let retired_day = imported(&rows)
        .iter()
        .find_map(|event| match &event.payload {
            ClaimEvent::AddressRetired { day, .. } => Some(*day),
            _ => None,
        });
    assert_eq!(
        retired_day,
        Some(jiff::civil::date(2026, 3, 9)),
        "the retirement carries the row's own day"
    );
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
    merged_pair_as_the_store_leaves_it()
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
        day: jiff::civil::date(2026, 3, 9),
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
            record: RecordId("k2/f1".into()),
            label: "f9".into(),
        }]]
    );
    let state = State::replay(&events);
    assert_eq!(
        state.retired_addresses.get("person:bart"),
        Some(&object("k2"))
    );
    assert_eq!(state.survivor(&object("k2")), &object("k1"));
    let moved = state.record(&record_id(&object("k2"), "f1")).unwrap();
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

/// **No stored row binds an object to an app, so an import leaves every object
/// unbound.** Ownership rows come with the 9.3.3 to 9.3.5 build (rule 748, the
/// app-api brief); until then nothing in a store says which app owns a thing,
/// and the import has nothing to read or to refuse. **A claim an app stamped is
/// not a binding**: the stamp says who wrote the claim, and the object stays
/// unbound.
///
/// **The two patterns below name every column of an entity row and every row
/// family, with no `..`.** The build that adds an ownership row adds a column or
/// a family, which stops this case compiling: whoever builds the rows reads this
/// case, maps the rows into `bound_to` in `object_body`, and replaces it with one
/// that binds.
#[test]
fn no_stored_row_binds_an_object_to_an_app_so_an_import_leaves_every_object_unbound() {
    let mut owned = entity("k1", "person:homer");
    owned.archived = Some(("retired".into(), at(5).unwrap()));
    let mut stamped = write_of(1, "seen by the forge", Some("run-a"), 1);
    stamped.claim.app = Some(crate::app::AppStamp {
        app: "bart-ci".into(),
        grant: "report".into(),
        received_at: "2026-11-04T00:00:00Z".into(),
    });
    let rows = Rows {
        entities: vec![owned, entity("k2", "person:marge")],
        facts: vec![fact("k1", "f1", vec![stamped])],
        ..Rows::default()
    };
    let EntityRow {
        key: _,
        handle: _,
        kind: _,
        name: _,
        source: _,
        boot: _,
        parent: _,
        aliases: _,
        crm: _,
        prose: _,
        badge: _,
        archived: _,
        merged_into: _,
        taken_in: _,
    } = rows.entities[0].clone();
    let Rows {
        entities: _,
        entity_writes: _,
        supplied: _,
        former_handles: _,
        facts: _,
        field_writes: _,
        merges: _,
        type_writes: _,
        metadata: _,
    } = rows.clone();
    let state = state_of(&rows);
    for id in ["k1", "k2"] {
        let held = state.object(&object(id)).expect("the object is imported");
        assert_eq!(held.bound_to, None, "{id} is bound to no app");
    }
}

/// **Rows that arrive in the wrong order import to the events the same rows give
/// in the right one.** Each rule below sorts its rows by the number or ordinal the
/// row carries, and the store hands rows back in no order a case may lean on. A
/// write stored on the first thing of a chain of two merges ends under the last
/// thing, with the id its claim wears there, only when the merges are applied in
/// their numbers' order; the merge events then arrive in that order too; and a
/// numbering whose rows come back as 2, 1 is the numbering 1, 2.
fn a_chain_of_two_merges(descending: bool) -> Rows {
    let mut written = write_of(1, "calm", Some("run-a"), 1);
    written.claim.home = object("k3");
    written.claim.label = "f9".into();
    let step = |no: i64, folded: &str, survivor: &str, from: &str, to: &str| MergeRow {
        no,
        folded: object(folded),
        survivor: object(survivor),
        merged_at: at(10 * no).unwrap(),
        session: None,
        claims: vec![(from.into(), to.into())],
    };
    let mut merges = vec![
        step(1, "k1", "k2", "f1", "f5"),
        step(2, "k2", "k3", "f5", "f9"),
    ];
    if descending {
        merges.reverse();
    }
    Rows {
        entities: vec![
            entity("k1", "person:homer"),
            entity("k2", "person:marge"),
            entity("k3", "person:bart"),
        ],
        facts: vec![fact("k3", "f9", vec![written])],
        field_writes: vec![set("k1", "mood", 1, "calm", "f1", Some(1))],
        merges,
        ..Rows::default()
    }
}

#[test]
fn merges_given_in_descending_order_import_as_they_do_in_ascending_order() {
    let ascending = imported(&a_chain_of_two_merges(false));
    // The anchor: in order, the write sits under the last thing with the id its
    // claim wears there, and the merges arrive oldest first.
    let written: Vec<(&ObjectId, &RecordId)> = ascending
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::FieldWritten {
                subject, record, ..
            } => Some((subject, record)),
            _ => None,
        })
        .collect();
    assert_eq!(
        written,
        [(&object("k3"), &RecordId("k3/f9".into()))],
        "the write follows both merges"
    );
    let folded: Vec<&ObjectId> = ascending
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::ObjectsMerged { folded, .. } => Some(folded),
            _ => None,
        })
        .collect();
    assert_eq!(folded, [&object("k1"), &object("k2")], "oldest merge first");
    // The same rows, merges listed newest first.
    assert_eq!(
        import(&a_chain_of_two_merges(true)),
        Ok(ascending),
        "the order the rows come back in is not the order of the merges"
    );
}

#[test]
fn former_handles_given_in_descending_order_import_as_they_do_in_ascending_order() {
    let formers = |descending: bool| {
        let mut rows = vec![
            FormerHandleRow {
                former: "person:nelson".into(),
                object: object("k1"),
                ordinal: 1,
                day: jiff::civil::date(2026, 3, 9),
            },
            FormerHandleRow {
                former: "person:milhouse".into(),
                object: object("k1"),
                ordinal: 2,
                day: jiff::civil::date(2026, 3, 10),
            },
        ];
        if descending {
            rows.reverse();
        }
        Rows {
            entities: vec![entity("k1", "person:homer")],
            former_handles: rows,
            ..Rows::default()
        }
    };
    let ascending = imported(&formers(false));
    let bound: Vec<&str> = ascending
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::AddressBound { address, .. } if address != "person:homer" => {
                Some(address.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        bound,
        ["person:nelson", "person:milhouse"],
        "the first former handle is bound first"
    );
    assert_eq!(import(&formers(true)), Ok(ascending));
}

#[test]
fn a_claims_writes_given_in_descending_order_import_as_they_do_in_ascending_order() {
    let claim_of = |descending: bool| {
        let first = write_of(1, "first", Some("run-a"), 1);
        let second = write_of(2, "second", Some("run-b"), 2);
        let mut row = fact(
            "k1",
            "f1",
            if descending {
                vec![second.clone(), first.clone()]
            } else {
                vec![first.clone(), second.clone()]
            },
        );
        row.head = second.claim.clone();
        Rows {
            entities: vec![entity("k1", "person:homer")],
            facts: vec![row],
            ..Rows::default()
        }
    };
    let ascending = imported(&claim_of(false));
    let state = State::replay(&ascending);
    let record = state.record(&record_id(&object("k1"), "f1")).unwrap();
    assert_eq!(
        (record.claim.content.as_str(), record.revisions),
        ("second", 1),
        "created by the first write and revised by the second"
    );
    assert_eq!(import(&claim_of(true)), Ok(ascending));
}

fn snapshot_of(name: &str) -> ObjectSnapshot {
    ObjectSnapshot {
        kind: "person".into(),
        name: name.into(),
        source: "fixture".into(),
        crm: None,
        parent: None,
        boot: String::new(),
        archived_reason: None,
        archived_at: None,
        merged_into: None,
        aliases: Vec::new(),
        prose: String::new(),
    }
}

fn entity_write(
    thing: &str,
    ordinal: u64,
    stamp: Option<i64>,
    session: Option<&str>,
    snapshot: Option<ObjectSnapshot>,
) -> EntityWriteRow {
    EntityWriteRow {
        thing: object(thing),
        filed_under: match thing {
            "k1" => "person:homer",
            _ => "person:marge",
        }
        .into(),
        ordinal,
        written_at: stamp.and_then(at),
        session: session.map(String::from),
        snapshot,
    }
}

/// What an observation event says, with the envelope it wears.
#[derive(Debug, PartialEq)]
struct Seen {
    object: ObjectId,
    filed_under: String,
    ordinal: u64,
    snapshot: Option<ObjectSnapshot>,
    moment: Option<Timestamp>,
    run: Option<RunId>,
}

fn observations(events: &[Event<ClaimEvent>]) -> Vec<Seen> {
    events
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::ObjectWriteObserved {
                object,
                filed_under,
                ordinal,
                snapshot,
            } => Some(Seen {
                object: object.clone(),
                filed_under: filed_under.clone(),
                ordinal: *ordinal,
                snapshot: snapshot.clone(),
                moment: event.imported_source_time,
                run: event.run.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// An `entity_write` row is one observation of its own: the stream it was filed
/// in, its ordinal, what it left, its stamp as the source's moment and its
/// session as the run. A row with no stamp has no moment and one with no snapshot
/// has none, and the entity's own moment is not put in their place.
#[test]
fn an_entity_write_row_is_an_observation_with_its_own_moment_and_run() {
    let mut homer = entity("k1", "person:homer");
    homer.taken_in = at(5);
    let mut full = snapshot_of("Homer");
    full.aliases = vec!["Homie".into()];
    full.prose = "a note".into();
    full.crm = Some("crm-7".into());
    full.parent = Some(object("k2"));
    let rows = Rows {
        entities: vec![homer, entity("k2", "person:marge")],
        entity_writes: vec![
            entity_write("k1", 1, Some(10), Some("run-a"), Some(full.clone())),
            entity_write("k1", 2, None, None, None),
        ],
        ..Rows::default()
    };
    assert_eq!(
        observations(&imported(&rows)),
        [
            Seen {
                object: object("k1"),
                filed_under: "person:homer".into(),
                ordinal: 2,
                snapshot: None,
                moment: None,
                run: None,
            },
            Seen {
                object: object("k1"),
                filed_under: "person:homer".into(),
                ordinal: 1,
                snapshot: Some(full),
                moment: at(10),
                run: Some(RunId("run-a".into())),
            },
        ],
        "a row nobody stamped sorts first, as the store reads it"
    );
}

/// **The current state at the cut is the baseline and never a version.** No
/// `ObjectCreated` is made for an entity, each entity has one baseline, the state
/// still serves the object, and the observed writes number the rows and no more.
#[test]
fn the_current_state_is_a_baseline_and_not_a_version() {
    let rows = Rows {
        entities: vec![entity("k1", "person:homer")],
        entity_writes: vec![entity_write(
            "k1",
            1,
            Some(10),
            None,
            Some(snapshot_of("Homer")),
        )],
        ..Rows::default()
    };
    let events = imported(&rows);
    let created = events
        .iter()
        .filter(|event| matches!(event.payload, ClaimEvent::ObjectCreated { .. }))
        .count();
    let baselined = events
        .iter()
        .filter(|event| matches!(event.payload, ClaimEvent::ObjectBaselined { .. }))
        .count();
    assert_eq!((created, baselined), (0, 1));
    let state = State::replay(&events);
    assert_eq!(state.object(&object("k1")).unwrap().name, "person:homer");
    assert_eq!(state.observed_writes(&object("k1")).len(), 1);
    assert_eq!(
        state.observed_writes(&object("k1"))[0].source_time,
        at(10),
        "the state keeps the observation's own moment"
    );
}

/// The writes of a thing folded into another are read in one order with the
/// survivor's: unstamped first, then by moment, and the folded thing's own key and
/// its snapshot's `merged_into` stay as stored.
#[test]
fn a_folded_things_writes_arrive_in_one_order_with_the_survivors() {
    let mut folded = snapshot_of("Marge");
    folded.merged_into = Some(object("k1"));
    let rows = Rows {
        entities: vec![entity("k1", "person:homer"), entity("k2", "person:marge")],
        entity_writes: vec![
            entity_write("k1", 1, Some(30), None, Some(snapshot_of("Homer"))),
            entity_write("k2", 1, Some(20), None, Some(snapshot_of("Marge"))),
            entity_write("k2", 2, None, None, None),
            entity_write("k2", 3, Some(25), Some("run-m"), Some(folded.clone())),
        ],
        merges: vec![MergeRow {
            no: 1,
            folded: object("k2"),
            survivor: object("k1"),
            merged_at: at(26).unwrap(),
            session: None,
            claims: Vec::new(),
        }],
        ..Rows::default()
    };
    let seen = observations(&imported(&rows));
    let order: Vec<(&str, u64)> = seen
        .iter()
        .map(|one| (one.object.0.as_str(), one.ordinal))
        .collect();
    assert_eq!(order, [("k2", 2), ("k2", 1), ("k2", 3), ("k1", 1)]);
    assert_eq!(
        seen[2].snapshot.as_ref().unwrap().merged_into,
        Some(object("k1"))
    );
    let state = State::replay(&imported(&rows));
    assert_eq!(state.observed_writes(&object("k2")).len(), 3);
    assert_eq!(state.observed_writes(&object("k1")).len(), 1);
}

fn supplied(handle: &str) -> SuppliedRow {
    SuppliedRow {
        handle: handle.into(),
        kind: "view".into(),
        name: "Loops".into(),
        source: "jojobot".into(),
    }
}

fn created_objects(events: &[Event<ClaimEvent>]) -> Vec<&ObjectId> {
    events
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::ObjectCreated { object, .. } => Some(object),
            _ => None,
        })
        .collect()
}

fn bound_addresses(events: &[Event<ClaimEvent>]) -> Vec<(&ObjectId, &str)> {
    events
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::AddressBound { object, address } => Some((object, address.as_str())),
            _ => None,
        })
        .collect()
}

/// **A supplied record is created under its own identity and answers to its
/// handle.** The key is the supply identity, not the handle and not a stored key.
#[test]
fn a_supplied_record_is_created_under_its_supply_identity_and_bound_to_its_handle() {
    let events = imported(&Rows {
        supplied: vec![supplied("view:loops")],
        ..Default::default()
    });
    let supply = supply_identity("view:loops");
    assert_eq!(created_objects(&events), vec![&supply]);
    assert_eq!(bound_addresses(&events), vec![(&supply, "view:loops")]);
}

/// **A stored row at a supplied handle holds the binding** (rule 508). The
/// supplied object is still created, and stays unbound.
#[test]
fn a_stored_row_at_a_supplied_handle_keeps_the_binding() {
    let events = imported(&Rows {
        entities: vec![entity("k1", "view:loops")],
        supplied: vec![supplied("view:loops")],
        ..Default::default()
    });
    let supply = supply_identity("view:loops");
    assert_eq!(created_objects(&events), vec![&supply]);
    assert!(
        bound_addresses(&events)
            .iter()
            .all(|(object, _)| **object != supply),
        "the supplied object was bound to a handle a stored row wears: {events:?}"
    );
}

/// **The supply identity is the same every time and no stored key can take it**:
/// a badge holds no `/`, and neither does a handle.
#[test]
fn the_supply_identity_is_stable_and_cannot_be_a_stored_key() {
    assert_eq!(supply_identity("view:loops"), supply_identity("view:loops"));
    assert_ne!(supply_identity("view:loops"), supply_identity("view:board"));
    assert!(supply_identity("view:loops").0.contains('/'));
    assert_ne!(supply_identity("view:loops").0, "view:loops");
}

/// **A supplied handle a stored thing once wore still answers to the supplied
/// record.** The thing left the handle for another, so no stored row wears it
/// now, and the replay of its former handles must not take the binding away.
/// The answer is asked of the replayed State, where the order of the events
/// decides it.
#[test]
fn a_supplied_handle_a_stored_thing_once_wore_answers_to_the_supplied_record() {
    let rows = Rows {
        entities: vec![entity("k1", "view:board")],
        former_handles: vec![FormerHandleRow {
            former: "view:loops".into(),
            object: object("k1"),
            ordinal: 1,
            day: jiff::civil::date(2026, 3, 9),
        }],
        supplied: vec![supplied("view:loops")],
        ..Default::default()
    };
    let state = state_of(&rows);
    assert_eq!(
        state.resolve("view:loops"),
        Some(&supply_identity("view:loops")),
        "the supplied record lost its handle to the former one's retirement"
    );
    assert_eq!(state.resolve("view:board"), Some(&object("k1")));
}

/// **The merged pair as the store leaves it.** A merge moves no row: the folded
/// thing's claim stays under the folded key with the local id it had, and the
/// merge row says which id it wears under the survivor now.
fn merged_pair_as_the_store_leaves_it() -> Rows {
    let survivor_write = write_of(1, "ours", Some("run-s"), 1);
    let mut folded_write = write_of(1, "theirs", Some("run-f"), 2);
    folded_write.claim.home = object("k2");
    Rows {
        entities: vec![entity("k1", "person:homer"), entity("k2", "person:marge")],
        facts: vec![
            fact("k1", "f1", vec![survivor_write]),
            fact("k2", "f1", vec![folded_write]),
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

fn moved_records(events: &[Event<ClaimEvent>]) -> Vec<Vec<MovedRecord>> {
    events
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::ObjectsMerged { records, .. } => Some(records.clone()),
            _ => None,
        })
        .collect()
}

/// **A merge names the record the claim was created under.** The claim is
/// created under the folded key and its old id; the merge moves that record and
/// gives it the label it wears under the survivor.
#[test]
fn a_merge_moves_the_record_the_claim_was_created_under() {
    let events = imported(&merged_pair_as_the_store_leaves_it());
    assert_eq!(
        moved_records(&events),
        vec![vec![MovedRecord {
            record: record_id(&object("k2"), "f1"),
            label: "f9".into(),
        }]]
    );
}

/// **The survivor serves the folded thing's key writes.** Asked of the replayed
/// State: both sides' writes of the key, the folded one newest.
#[test]
fn the_survivor_keeps_the_folded_things_key_writes_after_the_replay() {
    let state = state_of(&merged_pair_as_the_store_leaves_it());
    let writes = stream(&state, "k1", "mood");
    assert_eq!(
        writes.iter().map(|w| w.ordinal).collect::<Vec<_>>(),
        vec![1, 2],
        "the survivor lost the folded claim's key write"
    );
    assert_eq!(state.fields(&object("k1"), &[])["mood"], "theirs");
    assert!(stream(&state, "k2", "mood").is_empty());
}

/// **A claim moved twice is followed through both merges.** It is created under
/// the first folded key and its first id; the second merge moves the record it
/// was created as, whatever it is called by then.
#[test]
fn a_claim_moved_twice_keeps_naming_the_record_it_was_created_under() {
    let mut rows = merged_pair_as_the_store_leaves_it();
    // lisa (k3) is folded into marge (k2), then marge into homer.
    let mut deepest = write_of(1, "deepest", None, 3);
    deepest.claim.home = object("k3");
    rows.entities.push(entity("k3", "person:lisa"));
    rows.facts.push(fact("k3", "f1", vec![deepest]));
    rows.merges = vec![
        MergeRow {
            no: 1,
            folded: object("k3"),
            survivor: object("k2"),
            merged_at: at(40).unwrap(),
            session: None,
            claims: vec![("f1".into(), "f5".into())],
        },
        MergeRow {
            no: 2,
            folded: object("k2"),
            survivor: object("k1"),
            merged_at: at(50).unwrap(),
            session: None,
            claims: vec![("f1".into(), "f9".into()), ("f5".into(), "f10".into())],
        },
    ];
    let events = imported(&rows);
    assert_eq!(
        moved_records(&events),
        vec![
            vec![MovedRecord {
                record: record_id(&object("k3"), "f1"),
                label: "f5".into(),
            }],
            vec![
                MovedRecord {
                    record: record_id(&object("k2"), "f1"),
                    label: "f9".into(),
                },
                MovedRecord {
                    record: record_id(&object("k3"), "f1"),
                    label: "f10".into(),
                },
            ],
        ]
    );
}

/// **A citation of a merged claim's served address names the record the claim was
/// created as.** The store validates a later claim's `derived_from` against the
/// address the merge now serves (survivor, new label), and stores that address.
/// The record behind it is the one created under the folded key, so a pointer
/// kept as the served address names a record nothing created.
#[test]
fn a_pointer_to_a_merged_claims_served_address_names_the_record_it_was_created_as() {
    let mut rows = merged_pair_as_the_store_leaves_it();
    let served = record_id(&object("k1"), "f9");
    let mut citing = write_of(1, "cites the saxophone", None, 4);
    citing.claim.home = object("k1");
    citing.claim.label = "f2".into();
    citing.claim.derived_from = Some(served.clone());
    citing.claim.stands_for = vec![served.clone()];
    let mut citing_fact = fact("k1", "f2", vec![citing]);
    citing_fact.head.derived_from = Some(served.clone());
    citing_fact.head.stands_for = vec![served];
    rows.facts.push(citing_fact);

    let state = state_of(&rows);
    let made = state
        .record(&record_id(&object("k1"), "f2"))
        .expect("the citing claim is created");
    let original = record_id(&object("k2"), "f1");
    assert_eq!(made.claim.derived_from, Some(original.clone()));
    assert_eq!(made.claim.stands_for, vec![original.clone()]);
    assert!(
        state.record(&original).is_some(),
        "the record the citation names exists"
    );
}

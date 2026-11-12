use std::collections::BTreeSet;

use super::*;

/// The envelope the helpers wrap an event in: the operator, no run.
pub(super) fn envelope(payload: ClaimEvent) -> Event<ClaimEvent> {
    under(Some(Actor::Operator), None, payload)
}

pub(super) fn under(
    actor: Option<Actor>,
    run: Option<&str>,
    payload: ClaimEvent,
) -> Event<ClaimEvent> {
    Event {
        event_kind: "claim".into(),
        meaning_version: 1,
        actor,
        run: run.map(|run| RunId(run.into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: None,
        imported_source_time: None,
        payload,
    }
}

pub(super) fn apply(state: &mut State, event: &ClaimEvent) {
    state.apply(&envelope(event.clone()));
}

pub(super) fn replay<'a>(events: impl IntoIterator<Item = &'a ClaimEvent>) -> State {
    let mut state = State::default();
    for event in events {
        apply(&mut state, event);
    }
    state
}

pub(super) fn object(name: &str) -> ObjectId {
    ObjectId(name.to_string())
}

pub(super) fn record(name: &str) -> RecordId {
    RecordId(name.to_string())
}

pub(super) fn key(name: &str) -> KeyId {
    KeyId(name.to_string())
}

pub(super) fn day(d: i8) -> Date {
    jiff::civil::date(2026, 4, d)
}

pub(super) fn body(kind: &str, name: &str) -> ObjectBody {
    ObjectBody {
        kind: KindId(kind.to_string()),
        parent: None,
        name: name.to_string(),
        source: "user-named".to_string(),
        aliases: Vec::new(),
        boot: None,
        metadata: BTreeMap::new(),
        archived: false,
        bound_to: None,
    }
}

pub(super) fn claim(label: &str, home: &str, content: &str, on: i8) -> ClaimBody {
    ClaimBody {
        label: label.to_string(),
        home: object(home),
        content: content.to_string(),
        details: None,
        provenance: Provenance::Testimony,
        standing: Standing::Settled,
        status: FactStatus::Active,
        recorded_at: day(on),
        happened_at: None,
        happened_through: None,
        stale_after: None,
        edge: None,
        derived_from: None,
        refs: Vec::new(),
        stands_for: Vec::new(),
        receipt: None,
        app: None,
        inserted_at: None,
    }
}

pub(super) fn created(id: &str, body: ClaimBody) -> ClaimEvent {
    ClaimEvent::RecordCreated {
        record: record(id),
        claim: body,
    }
}

pub(super) fn archived(id: &str, label: &str, home: &str) -> ClaimEvent {
    ClaimEvent::RecordRevised {
        record: record(id),
        claim: ClaimBody {
            status: FactStatus::Archived,
            ..claim(label, home, "taken back", 2)
        },
    }
}

pub(super) fn write(id: &str, subject: &str, k: &str, ordinal: u64, value: &str) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(id),
        subject: object(subject),
        key: key(k),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Set(value.to_string()),
        targets: Vec::new(),
    }
}

pub(super) fn clear(id: &str, subject: &str, k: &str, ordinal: u64) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(id),
        subject: object(subject),
        key: key(k),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Clear,
        targets: Vec::new(),
    }
}

fn held(state: &State, subject: &str) -> BTreeMap<String, String> {
    state.fields(&object(subject), &[])
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// **The proof table**: each event kind, applied to a state, and the one thing
/// it must change.
#[test]
fn each_kind_changes_what_it_says() {
    let mut state = State::default();
    apply(
        &mut state,
        &ClaimEvent::ObjectCreated {
            object: object("alpha"),
            body: body("thing", "Alpha"),
        },
    );
    assert_eq!(state.object(&object("alpha")).unwrap().name, "Alpha");

    apply(
        &mut state,
        &ClaimEvent::ObjectRevised {
            object: object("alpha"),
            body: ObjectBody {
                archived: true,
                ..body("thing", "Alpha")
            },
        },
    );
    assert!(state.object(&object("alpha")).unwrap().archived);

    apply(
        &mut state,
        &ClaimEvent::AddressBound {
            object: object("alpha"),
            address: "thing:alpha".into(),
        },
    );
    assert_eq!(state.resolve("thing:alpha"), Some(&object("alpha")));
    apply(
        &mut state,
        &ClaimEvent::AddressRetired {
            object: object("alpha"),
            address: "thing:alpha".into(),
            day: day(3),
        },
    );
    assert_eq!(state.resolve("thing:alpha"), None);

    apply(&mut state, &created("r1", claim("f1", "alpha", "first", 1)));
    assert_eq!(state.record(&record("r1")).unwrap().claim.label, "f1");
    apply(
        &mut state,
        &ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim("f1", "alpha", "second wording", 1),
        },
    );
    let held_record = state.record(&record("r1")).unwrap();
    assert_eq!(held_record.claim.content, "second wording");
    assert_eq!(held_record.revisions, 1);

    apply(&mut state, &write("r1", "alpha", "laps", 1, "3"));
    assert_eq!(held(&state, "alpha"), map(&[("laps", "3")]));
}

/// **A retired spelling of another object is not retired by a stale event.**
#[test]
fn retiring_a_spelling_the_object_does_not_hold_changes_nothing() {
    let mut state = State::default();
    apply(
        &mut state,
        &ClaimEvent::AddressBound {
            object: object("beta"),
            address: "thing:delta".into(),
        },
    );
    apply(
        &mut state,
        &ClaimEvent::AddressRetired {
            object: object("alpha"),
            address: "thing:delta".into(),
            day: day(3),
        },
    );
    assert_eq!(state.resolve("thing:delta"), Some(&object("beta")));
}

/// **A revision never replaces the first writer.**
#[test]
fn a_revision_keeps_the_first_writer() {
    let events = [
        created("r1", claim("f1", "alpha", "first", 1)),
        ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim("f1", "alpha", "edited", 1),
        },
    ];
    let state = replay(&events);
    assert_eq!(
        state.record(&record("r1")).unwrap().first_writer,
        Some(Actor::Operator)
    );
}

/// **The newest write of a key wins, however late its event arrived**: the order
/// is the stream ordinal and never the position in the log.
#[test]
fn the_ordinal_decides_not_the_arrival() {
    let events = [
        created("r1", claim("f1", "alpha", "old", 1)),
        created("r2", claim("f2", "alpha", "new", 2)),
        write("r2", "alpha", "mood", 2, "calm"),
        write("r1", "alpha", "mood", 1, "tense"),
    ];
    assert_eq!(held(&replay(&events), "alpha"), map(&[("mood", "calm")]));
}

/// **A clear is a write, and an older record does not put the key back.**
#[test]
fn a_clear_is_not_undone_by_an_older_record() {
    let events = [
        created("r1", claim("f1", "alpha", "old", 1)),
        created("r2", claim("f2", "alpha", "new", 2)),
        write("r1", "alpha", "mood", 1, "tense"),
        write("r2", "alpha", "mood", 2, "calm"),
        clear("r2", "alpha", "mood", 3),
    ];
    assert_eq!(held(&replay(&events), "alpha"), map(&[]));
}

/// **A taken-back record's writes are passed over**: the newest write that still
/// counts wins.
#[test]
fn a_taken_back_record_stops_counting() {
    let events = [
        created("r1", claim("f1", "alpha", "old", 1)),
        created("r2", claim("f2", "alpha", "new", 2)),
        write("r1", "alpha", "mood", 1, "tense"),
        write("r2", "alpha", "mood", 2, "calm"),
        archived("r2", "f2", "alpha"),
    ];
    assert_eq!(held(&replay(&events), "alpha"), map(&[("mood", "tense")]));
}

/// **A key that stays on its record never reaches the thing.**
#[test]
fn a_write_that_stays_does_not_reach() {
    let events = [
        created("r1", claim("f1", "alpha", "x", 1)),
        ClaimEvent::FieldWritten {
            record: record("r1"),
            subject: object("alpha"),
            key: key("chain_wear"),
            ordinal: 1,
            class: WriteClass::Stays,
            write: FieldValue::Set("9".into()),
            targets: Vec::new(),
        },
    ];
    assert_eq!(held(&replay(&events), "alpha"), map(&[]));
}

/// **A summing key totals the writes of active records**, through the shared
/// fold, and a revision that retires one changes the total.
#[test]
fn a_counter_sums_active_writes_and_follows_a_status_change() {
    let declared = [DeclaredType::new(
        "running",
        vec![memory::types::Field::summing("laps")],
    )];
    let mut events = vec![
        created("r1", claim("f1", "alpha", "a", 1)),
        created("r2", claim("f2", "alpha", "b", 2)),
        write("r1", "alpha", "laps", 1, "2"),
        write("r2", "alpha", "laps", 2, "5"),
    ];
    let state = replay(&events);
    assert_eq!(
        state.fields(&object("alpha"), &declared),
        map(&[("laps", "7")])
    );
    events.push(archived("r2", "f2", "alpha"));
    let state = replay(&events);
    assert_eq!(
        state.fields(&object("alpha"), &declared),
        map(&[("laps", "2")])
    );
}

/// **A merge moves claims, writes and spellings to the survivor**, keeps the
/// decided interleave, and an old spelling resolves to the survivor.
#[test]
fn a_merge_moves_everything_and_keeps_the_decided_interleave() {
    let events = [
        created("r1", claim("f1", "alpha", "alpha says", 1)),
        created("r2", claim("f1", "beta", "beta says", 2)),
        created("r3", claim("f2", "alpha", "alpha again", 3)),
        write("r1", "alpha", "mood", 1, "tense"),
        write("r3", "alpha", "mood", 3, "steady"),
        write("r2", "beta", "mood", 5, "calm"),
        ClaimEvent::AddressBound {
            object: object("beta"),
            address: "thing:beta".into(),
        },
        ClaimEvent::ObjectsMerged {
            folded: object("beta"),
            survivor: object("alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f2".into(),
            }],
            writes: vec![MovedWrite {
                record: record("r2"),
                key: key("mood"),
                from_ordinal: 5,
                ordinal: 2,
            }],
        },
    ];
    let state = replay(&events);
    assert_eq!(state.resolve("thing:beta"), Some(&object("alpha")));
    assert_eq!(state.record(&record("r2")).unwrap().claim.label, "f2");
    assert_eq!(state.active_records(&object("beta")).len(), 3);
    // The moved write sits between the survivor's two, so the later one holds.
    assert_eq!(held(&state, "alpha"), map(&[("mood", "steady")]));
    assert_eq!(held(&state, "beta"), map(&[("mood", "steady")]));
}

/// **A merge renumbers the survivor's own writes of a key when the interleave
/// puts a folded write among them**: the store numbers a key's writes from one
/// without a gap, so a folded write placed before the survivor's newest takes
/// that place, and the newest moves on.
#[test]
fn a_merge_renumbers_the_survivors_writes_it_places_a_folded_write_before() {
    let events = [
        created("r1", claim("f1", "alpha", "alpha says", 1)),
        created("r2", claim("f1", "beta", "beta says", 2)),
        created("r3", claim("f2", "alpha", "alpha again", 3)),
        write("r1", "alpha", "mood", 1, "tense"),
        write("r3", "alpha", "mood", 2, "steady"),
        write("r2", "beta", "mood", 1, "calm"),
        ClaimEvent::ObjectsMerged {
            folded: object("beta"),
            survivor: object("alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f3".into(),
            }],
            writes: vec![
                MovedWrite {
                    record: record("r2"),
                    key: key("mood"),
                    from_ordinal: 1,
                    ordinal: 2,
                },
                MovedWrite {
                    record: record("r3"),
                    key: key("mood"),
                    from_ordinal: 2,
                    ordinal: 3,
                },
            ],
        },
    ];
    let state = replay(&events);
    assert_eq!(held(&state, "alpha"), map(&[("mood", "steady")]));
}

/// **The pair a claim was created under finds that claim after a merge moved it**,
/// and the survivor's own claim under the label the moved one is now served by is
/// found by its own pair. The expected records are the ones the events name, not
/// ones read back from the State.
#[test]
fn a_merge_moves_the_label_a_claim_is_served_by_and_not_the_pair_it_was_created_under() {
    let events = vec![
        created("r1", claim("f1", "alpha", "the survivor's own", 1)),
        created("r2", claim("f1", "beta", "the folded thing's", 2)),
        ClaimEvent::ObjectsMerged {
            folded: object("beta"),
            survivor: object("alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f2".into(),
            }],
            writes: Vec::new(),
        },
    ];
    let state = replay(&events);
    // Served: the moved claim now wears f2 on the survivor.
    assert_eq!(state.record(&record("r2")).unwrap().claim.label, "f2");
    // Created under: each pair finds the claim made under it.
    assert_eq!(
        state.record_at_origin(&object("alpha"), "f1"),
        Some(record("r1"))
    );
    assert_eq!(
        state.record_at_origin(&object("beta"), "f1"),
        Some(record("r2"))
    );
    // No claim was ever created under the label the merge handed out.
    assert_eq!(state.record_at_origin(&object("alpha"), "f2"), None);
}

/// **A state knows the log it was built from holds a completed import** from the
/// `ImportCompleted` event alone, so one that starts from a snapshot covering the
/// import knows it without the batch.
#[test]
fn a_state_holds_that_the_import_completed_from_its_event() {
    let completed = ClaimEvent::ImportCompleted { events: 0 };
    assert!(replay([&completed]).import_completed());
    let mut applied = replay(&[created("r1", claim("f1", "alpha", "a", 1))]);
    assert!(!applied.import_completed());
    apply(&mut applied, &completed);
    assert!(applied.import_completed());
}

/// **A merge that would close a loop of redirects changes nothing**: a self-merge
/// and a merge of an object into one that already folded into it both leave
/// every read answering, and the first merge still stands.
#[test]
fn a_merge_that_would_loop_the_redirects_is_ignored() {
    let merged = |folded: &str, survivor: &str| ClaimEvent::ObjectsMerged {
        folded: object(folded),
        survivor: object(survivor),
        records: Vec::new(),
        writes: Vec::new(),
    };
    let events = vec![
        created("r1", claim("f1", "alpha", "alpha says", 1)),
        created("r2", claim("f1", "beta", "beta says", 2)),
        merged("alpha", "alpha"),
        merged("beta", "alpha"),
        merged("alpha", "beta"),
    ];
    // A loop makes `survivor` spin forever, so the read runs on its own thread.
    let (sent, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let state = replay(&events);
        let _ = sent.send((
            state.survivor(&object("alpha")).clone(),
            state.survivor(&object("beta")).clone(),
        ));
    });
    let (alpha, beta) = answer
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("a read of the claims state returns");
    assert_eq!((alpha, beta), (object("alpha"), object("alpha")));
}

/// **Links follow the write holding the key**: a cleared key points at nothing,
/// and a merged target answers as its survivor.
#[test]
fn links_follow_the_holding_write() {
    let pointing = ClaimEvent::FieldWritten {
        record: record("r1"),
        subject: object("alpha"),
        key: key("friend"),
        ordinal: 1,
        class: WriteClass::Sets,
        write: FieldValue::Set("thing:beta".into()),
        targets: vec![object("beta")],
    };
    let mut events = vec![created("r1", claim("f1", "alpha", "a", 1)), pointing];
    let state = replay(&events);
    assert_eq!(
        state.links_into(&object("beta")),
        BTreeSet::from([object("alpha")])
    );

    events.push(ClaimEvent::ObjectsMerged {
        folded: object("beta"),
        survivor: object("gamma"),
        records: Vec::new(),
        writes: Vec::new(),
    });
    let state = replay(&events);
    assert_eq!(
        state.links_into(&object("beta")),
        BTreeSet::from([object("alpha")])
    );
    assert_eq!(
        state.links_from(&object("alpha")),
        BTreeSet::from([object("gamma")])
    );

    // The claim that holds the key clears it.
    events.push(clear("r1", "alpha", "friend", 2));
    let state = replay(&events);
    assert!(state.links_into(&object("gamma")).is_empty());
}

/// **A late-arriving older write does not become the link**: the ordinal says
/// which write holds the key, not the position in the log.
#[test]
fn the_link_is_the_newest_by_ordinal_however_late_it_arrived() {
    let pointing = |id: &str, ordinal: u64, target: &str| ClaimEvent::FieldWritten {
        record: record(id),
        subject: object("alpha"),
        key: key("friend"),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Set(target.into()),
        targets: vec![object(target)],
    };
    let events = [
        created("r1", claim("f1", "alpha", "old", 1)),
        created("r2", claim("f2", "alpha", "new", 2)),
        pointing("r2", 2, "gamma"),
        pointing("r1", 1, "beta"),
    ];
    let state = replay(&events);
    assert_eq!(
        state.links_from(&object("alpha")),
        BTreeSet::from([object("gamma")])
    );
}

/// **Who wrote an event and in which run come from its envelope.** Two writes of
/// one key under two envelopes give two key writes, each naming its own writer
/// and run, and a revision under a third actor leaves the first writer alone.
#[test]
fn the_envelope_names_the_writer_and_the_run_of_each_write() {
    let first = Actor::System("first".into());
    let second = Actor::System("second".into());
    let third = Actor::System("third".into());
    let mut state = State::default();
    state.apply(&under(
        Some(first.clone()),
        Some("run-1"),
        created("r1", claim("f1", "alpha", "a", 1)),
    ));
    state.apply(&under(
        Some(first.clone()),
        Some("run-1"),
        write("r1", "alpha", "mood", 1, "tense"),
    ));
    state.apply(&under(
        Some(second.clone()),
        Some("run-2"),
        write("r1", "alpha", "mood", 2, "calm"),
    ));
    state.apply(&under(
        Some(third),
        Some("run-3"),
        ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim("f1", "alpha", "edited", 1),
        },
    ));
    state.apply(&under(None, None, write("r1", "alpha", "mood", 3, "flat")));

    let mut writes = state.key_writes(&object("alpha"));
    writes.sort_by_key(|write| write.ordinal);
    let seen: Vec<_> = writes
        .iter()
        .map(|write| (write.writer.clone(), write.run.clone()))
        .collect();
    assert_eq!(
        seen,
        [
            (Some(first.clone()), Some("run-1".to_string())),
            (Some(second), Some("run-2".to_string())),
            (None, None),
        ]
    );
    assert_eq!(
        state.record(&record("r1")).unwrap().first_writer,
        Some(first)
    );
}

pub(super) fn moment(text: &str) -> Timestamp {
    text.parse().expect("a timestamp")
}

/// An envelope that names the moment the store took the event in and, for an
/// import, the moment the source row was written.
pub(super) fn stamped(
    written_at: Option<Timestamp>,
    imported_source_time: Option<Timestamp>,
    payload: ClaimEvent,
) -> Event<ClaimEvent> {
    Event {
        written_at,
        imported_source_time,
        ..envelope(payload)
    }
}

/// **A key write keeps the moment of its own event, never the claim's.** The
/// claim was inserted at one moment; two writes of one key arrived at two later
/// ones, an imported write carries its source's moment and not the importer's,
/// and a write whose event names no moment has none.
#[test]
fn each_key_write_keeps_the_moment_of_its_own_event() {
    let (t0, t1, t2) = (
        moment("2026-04-01T00:00:00Z"),
        moment("2026-04-02T00:00:00Z"),
        moment("2026-04-03T00:00:00Z"),
    );
    let importer = moment("2026-05-01T00:00:00Z");
    let mut state = State::default();
    let stamped_claim = ClaimBody {
        inserted_at: Some(t0),
        ..claim("f1", "alpha", "a", 1)
    };
    state.apply(&envelope(created("r1", stamped_claim)));
    for (ordinal, written, source) in [
        (1, Some(t1), None),
        (2, Some(t2), None),
        (3, Some(importer), Some(t1)),
        (4, None, None),
    ] {
        state.apply(&stamped(
            written,
            source,
            write("r1", "alpha", "mood", ordinal, "calm"),
        ));
    }

    let mut writes = state.key_writes(&object("alpha"));
    writes.sort_by_key(|write| write.ordinal);
    let seen: Vec<_> = writes.iter().map(|write| write.written_at).collect();
    assert_eq!(seen, [Some(t1), Some(t2), Some(t1), None]);
}

/// **A claim's own moment follows the same rule as a write's: an imported
/// event's source moment, else the moment the store took the event in, else
/// none.** It holds for the event that creates the claim and for the one that
/// revises it, and the newest of those is the one the claim keeps. Each event
/// below names a source moment and an importer's moment that differ, so a record
/// that took the importer's reads the wrong one; the controls name one moment or
/// none.
#[test]
fn a_claim_keeps_the_source_moment_of_the_event_that_created_or_revised_it() {
    let (t0, t1, t2) = (
        moment("2026-04-01T00:00:00Z"),
        moment("2026-04-02T00:00:00Z"),
        moment("2026-04-03T00:00:00Z"),
    );
    let (importer, later_importer) = (
        moment("2026-05-01T00:00:00Z"),
        moment("2026-05-02T00:00:00Z"),
    );
    let mut state = State::default();
    state.apply(&stamped(
        Some(importer),
        Some(t1),
        created("r1", claim("f1", "alpha", "a", 1)),
    ));
    assert_eq!(
        state.last_touched(&record("r1")),
        Some(t1),
        "created from an import: the source's moment, not the importer's"
    );
    state.apply(&stamped(
        Some(later_importer),
        Some(t2),
        ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim("f1", "alpha", "b", 1),
        },
    ));
    assert_eq!(
        state.last_touched(&record("r1")),
        Some(t2),
        "revised from an import: the source's moment, not the importer's"
    );

    state.apply(&stamped(
        Some(t0),
        None,
        created("r2", claim("f2", "alpha", "c", 1)),
    ));
    assert_eq!(
        state.last_touched(&record("r2")),
        Some(t0),
        "no source moment: the moment the store took the event in"
    );
    state.apply(&stamped(
        Some(t2),
        None,
        ClaimEvent::RecordRevised {
            record: record("r2"),
            claim: claim("f2", "alpha", "d", 1),
        },
    ));
    assert_eq!(
        state.last_touched(&record("r2")),
        Some(t2),
        "a revision with no source moment: its own"
    );
    state.apply(&stamped(
        None,
        None,
        created("r3", claim("f3", "alpha", "e", 1)),
    ));
    assert_eq!(
        state.last_touched(&record("r3")),
        None,
        "an event that names no moment leaves the claim without one"
    );
}

fn declaration(name: &str, must: Option<&str>) -> DeclaredType {
    DeclaredType {
        name: name.to_string(),
        fields: Vec::new(),
        origin: memory::types::Origin::Declared,
        must: must.map(str::to_string),
    }
}

/// Each declaration of a kind is one event; the newest is in force and the
/// earlier ones stay in the history, oldest first.
#[test]
fn a_redeclared_type_keeps_its_history_and_the_newest_is_in_force() {
    let mut state = State::default();
    let kind = object("pet");
    for must in [None, Some("a"), Some("b")] {
        state.apply(&envelope(ClaimEvent::TypeDeclared {
            kind: kind.clone(),
            declaration: declaration("pet", must),
        }));
    }
    assert_eq!(state.declared(&kind).unwrap().must.as_deref(), Some("b"));
    let history: Vec<_> = state
        .declarations_of(&kind)
        .iter()
        .map(|held| held.must.as_deref())
        .collect();
    assert_eq!(history, [None, Some("a"), Some("b")]);
    assert!(state.declared(&object("toy")).is_none());
    let in_force: Vec<_> = state
        .declared_types()
        .iter()
        .map(|held| held.must.clone())
        .collect();
    assert_eq!(
        in_force,
        [Some("b".to_string())],
        "only the newest is served"
    );
    assert!(state.declarations_of(&object("toy")).is_empty());
}

/// **A spelling the object gave up still stands for it**, and answers with the
/// handle it wears now; one that was never worn answers with nothing, and a
/// spelling that is current again wins over the one that gave it up.
#[test]
fn a_former_spelling_stands_for_the_handle_the_thing_wears_now() {
    memory::kinds::load_shipped();
    let mut state = State::default();
    for event in [
        ClaimEvent::ObjectCreated {
            object: object("k1"),
            body: body("thing", "Alpha"),
        },
        ClaimEvent::AddressBound {
            object: object("k1"),
            address: "thing:beta".into(),
        },
        ClaimEvent::AddressRetired {
            object: object("k1"),
            address: "thing:beta".into(),
            day: day(2),
        },
        ClaimEvent::AddressBound {
            object: object("k1"),
            address: "thing:alpha".into(),
        },
    ] {
        apply(&mut state, &event);
    }
    fn handle(state: &State, spelling: &str) -> Option<String> {
        state.canonical_handle(spelling).map(|handle| handle.0)
    }
    assert_eq!(handle(&state, "thing:beta").as_deref(), Some("thing:alpha"));
    assert_eq!(
        handle(&state, "thing:alpha").as_deref(),
        Some("thing:alpha")
    );
    assert_eq!(handle(&state, "thing:gamma"), None);
    apply(
        &mut state,
        &ClaimEvent::ObjectCreated {
            object: object("k2"),
            body: body("thing", "Other"),
        },
    );
    apply(
        &mut state,
        &ClaimEvent::AddressBound {
            object: object("k2"),
            address: "thing:beta".into(),
        },
    );
    assert_eq!(
        handle(&state, "thing:beta").as_deref(),
        Some("thing:beta"),
        "a spelling worn again by another thing is that thing's"
    );
}

/// A withdrawn declaration leaves the kind with none, and only that kind; a kind
/// declared again afterwards is held again.
#[test]
fn a_withdrawn_declaration_leaves_the_kind_with_none() {
    let mut state = State::default();
    let (pet, toy) = (object("pet"), object("toy"));
    for kind in [&pet, &toy] {
        state.apply(&envelope(ClaimEvent::TypeDeclared {
            kind: kind.clone(),
            declaration: declaration(&kind.0, None),
        }));
    }
    state.apply(&envelope(ClaimEvent::TypeWithdrawn { kind: pet.clone() }));
    assert!(state.declared(&pet).is_none());
    assert!(state.declarations_of(&pet).is_empty());
    assert!(state.declared(&toy).is_some(), "another kind is untouched");
    state.apply(&envelope(ClaimEvent::TypeDeclared {
        kind: pet.clone(),
        declaration: declaration("pet", Some("again")),
    }));
    assert_eq!(state.declarations_of(&pet).len(), 1);
}

/// **A chain of folds keeps every folded thing's data under the last survivor, in
/// whichever order the marks arrive.** A legacy chain A into B and B into C comes
/// from marks that carry no order, and the import applies them in the order of the
/// entity rows. The control is the chain in time order.
#[test]
fn a_chain_of_folds_reaches_the_last_survivor_in_either_order() {
    let redirected = |folded: &str, survivor: &str| ClaimEvent::ObjectRedirected {
        folded: object(folded),
        survivor: object(survivor),
    };
    let things = ["a", "b", "c"].map(|name| {
        [
            created(
                &format!("r-{name}"),
                claim(&format!("f-{name}"), name, "says", 1),
            ),
            write(&format!("r-{name}"), name, &format!("key-{name}"), 1, name),
        ]
    });
    let all = map(&[("key-a", "a"), ("key-b", "b"), ("key-c", "c")]);
    for (label, order) in [
        ("time order", [redirected("a", "b"), redirected("b", "c")]),
        (
            "reverse order",
            [redirected("b", "c"), redirected("a", "b")],
        ),
    ] {
        let events: Vec<ClaimEvent> = things.iter().flatten().cloned().chain(order).collect();
        let state = replay(&events);
        assert_eq!(
            held(&state, "c"),
            all,
            "{label}: the last survivor holds all three"
        );
    }
}

/// **The State serves each thing the guard and the measure read as an entity**:
/// the handle it answers to, its names, its parent as a handle. A thing folded
/// into another is not served, and its name rides on the survivor.
#[test]
fn the_state_serves_each_thing_as_an_entity() {
    memory::kinds::load_shipped();
    let named = |handle: &str, name: &str, parent: Option<&str>| {
        vec![
            ClaimEvent::ObjectCreated {
                object: object(handle),
                body: ObjectBody {
                    parent: parent.map(object),
                    aliases: vec![format!("{name}-nickname")],
                    ..body("thing", name)
                },
            },
            ClaimEvent::AddressBound {
                object: object(handle),
                address: handle.to_string(),
            },
        ]
    };
    let state = replay(
        &[
            named("thing:alpha", "Alpha", None),
            named("thing:beta", "Beta", Some("thing:alpha")),
            named("thing:gamma", "Gamma", None),
            vec![ClaimEvent::ObjectRedirected {
                folded: object("thing:gamma"),
                survivor: object("thing:alpha"),
            }],
        ]
        .concat(),
    );

    let served = state.entities();
    let handles: Vec<String> = served.iter().map(|e| e.id.to_string()).collect();
    assert_eq!(handles, ["thing:alpha", "thing:beta"]);
    let alpha = state
        .entity(&memory::EntityId("thing:alpha".into()))
        .unwrap();
    assert_eq!(alpha.name, "Alpha");
    assert_eq!(alpha.aliases, ["Alpha-nickname"]);
    assert_eq!(alpha.folded_names, ["Gamma"]);
    let beta = state
        .entity(&memory::EntityId("thing:beta".into()))
        .unwrap();
    assert_eq!(beta.parent, Some(memory::EntityId("thing:alpha".into())));
    assert!(
        state
            .entity(&memory::EntityId("thing:gamma".into()))
            .is_none()
    );
}

/// **The payload is everything the State serves now, at any status, and nothing
/// that is history.** An object counts its name, source, aliases, boot text and
/// metadata, archived or folded or not. A claim counts its content, its details and
/// the key and value of its current write of each key, at any status. A claim's
/// earlier wording, and a write of a key that a later write of the same claim
/// replaced, count for nothing.
#[test]
fn the_payload_counts_what_the_state_serves_now_and_no_history() {
    let mut detailed = claim("f2", "alpha", "calm", 2);
    detailed.details = Some("slow".into());
    let events = [
        ClaimEvent::ObjectCreated {
            object: object("alpha"),
            body: body("thing", "Alpha"),
        },
        created("r1", claim("f1", "alpha", "tense", 1)),
        created("r2", detailed),
        created("r3", claim("f3", "alpha", "gone", 3)),
        write("r1", "alpha", "mood", 1, "tense"),
        // The same claim writes the key again: the first write is history.
        write("r1", "alpha", "mood", 2, "wired"),
        write("r3", "alpha", "colour", 3, "red"),
        // r3 then clears the key: its current write holds no value.
        clear("r3", "alpha", "colour", 4),
        // r1 is revised: its first wording is history.
        ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim("f1", "alpha", "stressed", 1),
        },
        // r3 is archived with the wording it had.
        ClaimEvent::RecordRevised {
            record: record("r3"),
            claim: ClaimBody {
                status: FactStatus::Archived,
                ..claim("f3", "alpha", "gone", 3)
            },
        },
    ];
    let payload = replay(&events).payload();
    // alpha: "Alpha" 5 + "user-named" 10. r1: "stressed" 8 + "mood" 4 + "wired" 5.
    // r2: "calm" 4 + "slow" 4. r3, archived: "gone" 4, its cleared key holds nothing.
    assert_eq!(payload.bytes, 15 + 17 + 8 + 4, "{payload:?}");
    assert_eq!(payload.records, 4, "one object and three claims");
}

/// **Archiving changes nothing a reader is served as payload**: the claim and the
/// object stay in the State with the text they had.
#[test]
fn archiving_a_claim_or_an_object_leaves_the_payload_unchanged() {
    let before = [
        ClaimEvent::ObjectCreated {
            object: object("alpha"),
            body: body("thing", "Alpha"),
        },
        created("r1", claim("f1", "alpha", "tense", 1)),
        write("r1", "alpha", "mood", 1, "tense"),
    ];
    let mut state = replay(&before);
    let held = state.payload();
    assert!(held.bytes > 0 && held.records == 2, "{held:?}");
    apply(
        &mut state,
        &ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: ClaimBody {
                status: FactStatus::Archived,
                ..claim("f1", "alpha", "tense", 1)
            },
        },
    );
    apply(
        &mut state,
        &ClaimEvent::ObjectRevised {
            object: object("alpha"),
            body: ObjectBody {
                archived: true,
                ..body("thing", "Alpha")
            },
        },
    );
    assert_eq!(state.payload(), held);
}

fn removed(id: &str, subject: &str, k: &str, ordinal: u64) -> ClaimEvent {
    ClaimEvent::FieldRemoved {
        record: record(id),
        subject: object(subject),
        key: key(k),
        ordinal,
        class: WriteClass::Sets,
        targets: Vec::new(),
    }
}

/// **The kept payload equals a fresh walk of the State after every event.** The
/// walk is the oracle; the count a reader takes is the one the events kept. Each
/// kind of event that changes what the State serves is in the sequence, in the
/// shapes that move the count both ways: a thing written again over a smaller or a
/// larger body, a claim created again or revised, a key written again by the same
/// claim and by another, cleared and set again, removed, and two things merged with
/// the same key written under both.
#[test]
fn the_kept_payload_equals_a_fresh_walk_after_every_event() {
    let mut wide = body("thing", "Alpha is wide");
    wide.aliases = vec!["al".into(), "alfa".into()];
    wide.boot = Some("boot text".into());
    wide.metadata = map(&[("shelf", "top"), ("size", "m")])
        .into_iter()
        .collect();
    let mut detailed = claim("f1", "alpha", "tense", 1);
    detailed.details = Some("long details".into());
    let events = [
        ClaimEvent::ObjectBaselined {
            object: object("alpha"),
            body: body("thing", "Alpha"),
        },
        // A baseline over a held body replaces it, wider.
        ClaimEvent::ObjectBaselined {
            object: object("alpha"),
            body: wide.clone(),
        },
        ClaimEvent::ObjectCreated {
            object: object("beta"),
            body: body("thing", "Beta"),
        },
        // A body written again, narrower.
        ClaimEvent::ObjectRevised {
            object: object("alpha"),
            body: body("thing", "A"),
        },
        ClaimEvent::ObjectRevised {
            object: object("alpha"),
            body: wide,
        },
        created("r1", claim("f1", "alpha", "tense", 1)),
        // A claim created again under the same record replaces the first.
        created("r1", detailed),
        created("r2", claim("f1", "beta", "calm", 2)),
        created("r3", claim("f2", "alpha", "gone", 3)),
        ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim("f1", "alpha", "x", 1),
        },
        // A revision of a record nothing holds changes nothing.
        ClaimEvent::RecordRevised {
            record: record("r9"),
            claim: claim("f9", "alpha", "never held", 1),
        },
        write("r1", "alpha", "mood", 1, "tense"),
        // The same claim writes the key again: the first write is history.
        write("r1", "alpha", "mood", 2, "wired-up"),
        // Another claim writes the same key on the same thing.
        write("r3", "alpha", "mood", 3, "steady"),
        write("r3", "alpha", "colour", 4, "red"),
        clear("r3", "alpha", "colour", 5),
        write("r3", "alpha", "colour", 6, "blue"),
        removed("r3", "alpha", "colour", 7),
        write("r2", "beta", "mood", 8, "calm"),
        write("r2", "beta", "shape", 9, "round"),
        ClaimEvent::AddressBound {
            object: object("beta"),
            address: "thing:beta".into(),
        },
        ClaimEvent::ObjectsMerged {
            folded: object("beta"),
            survivor: object("alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f3".into(),
            }],
            writes: vec![MovedWrite {
                record: record("r2"),
                key: key("mood"),
                from_ordinal: 8,
                ordinal: 2,
            }],
        },
        // A write after the merge, by a claim homed on the folded thing.
        write("r2", "alpha", "mood", 9, "calmer"),
        ClaimEvent::ObjectRevised {
            object: object("beta"),
            body: body("thing", "Beta, folded"),
        },
        ClaimEvent::ObjectCreated {
            object: object("gamma"),
            body: body("thing", "Gamma"),
        },
        ClaimEvent::ObjectRedirected {
            folded: object("gamma"),
            survivor: object("alpha"),
        },
        ClaimEvent::AddressRetired {
            object: object("alpha"),
            address: "thing:alpha".into(),
            day: day(4),
        },
    ];
    let mut state = State::default();
    for (at, event) in events.iter().enumerate() {
        apply(&mut state, event);
        assert_eq!(
            state.payload(),
            state.payload_by_walk(),
            "after event {at}: {event:?}"
        );
    }
    let held = state.payload();
    assert!(held.bytes > 0 && held.records > 0, "{held:?}");
}

/// **A merge that leaves one claim two writes of a key counts its newest write
/// once**, as the walk of the merged stream does.
#[test]
fn a_merge_that_leaves_one_claim_two_writes_of_a_key_counts_its_newest() {
    let events = [
        created("r1", claim("f1", "alpha", "one", 1)),
        created("r2", claim("f1", "beta", "two", 2)),
        write("r1", "alpha", "mood", 1, "tense"),
        write("r1", "beta", "mood", 1, "a much longer mood than the other"),
        write("r2", "beta", "mood", 2, "calm"),
        ClaimEvent::ObjectsMerged {
            folded: object("beta"),
            survivor: object("alpha"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    ];
    let mut state = State::default();
    for (at, event) in events.iter().enumerate() {
        apply(&mut state, event);
        assert_eq!(
            state.payload(),
            state.payload_by_walk(),
            "after event {at}: {event:?}"
        );
    }
}

#[test]
fn the_claims_snapshot_serializes_streams_history_and_indexes() {
    let events = [
        ClaimEvent::ObjectCreated {
            object: object("alpha"),
            body: body("thing", "Alpha"),
        },
        ClaimEvent::AddressBound {
            object: object("alpha"),
            address: "thing:alpha".into(),
        },
        created("beta", claim("f1", "alpha", "first", 1)),
        write("beta", "alpha", "status", 1, "queued"),
        ClaimEvent::RecordRevised {
            record: record("beta"),
            claim: claim("f1", "alpha", "second", 2),
        },
    ];
    let state = replay(&events);
    let value = serde_json::to_value(&state).expect("the populated State serializes as JSON");
    assert_eq!(
        value["records"]["beta"]["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        value["streams"][0][0],
        serde_json::json!(["alpha", "status"])
    );
    assert_eq!(value["streams"][0][1][0]["write"]["Set"], "queued");
    assert_eq!(
        value["indexes"]["top_ordinal"][0],
        serde_json::json!([["alpha", "status"], 1])
    );
    assert_eq!(
        value["indexes"]["labels"][0],
        serde_json::json!([["alpha", "f1"], "beta"])
    );
    assert_eq!(value["objects"]["alpha"]["name"], "Alpha");
}

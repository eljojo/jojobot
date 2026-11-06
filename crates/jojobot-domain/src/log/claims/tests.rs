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
        clear("r2", "alpha", "mood", 2),
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

    events.push(created("r2", claim("f2", "alpha", "b", 2)));
    events.push(clear("r2", "alpha", "friend", 2));
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

/// **Applying the rest of a prefix gives the state that replaying the whole
/// gives**, whatever the split.
#[test]
fn applying_in_parts_agrees_with_replay() {
    let events = [
        created("r1", claim("f1", "alpha", "a", 1)),
        write("r1", "alpha", "mood", 1, "tense"),
        ClaimEvent::ObjectCreated {
            object: object("alpha"),
            body: body("thing", "Alpha"),
        },
    ];
    for split in 0..=events.len() {
        let mut state = replay(&events[..split]);
        for event in &events[split..] {
            apply(&mut state, event);
        }
        assert_eq!(state, replay(&events), "split at {split}");
    }
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

fn moment(text: &str) -> Timestamp {
    text.parse().expect("a timestamp")
}

/// An envelope that names the moment the store took the event in and, for an
/// import, the moment the source row was written.
fn stamped(
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
    assert!(state.declarations_of(&object("toy")).is_empty());
}

use super::*;

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).unwrap()
}

fn request(generation: u64) -> Request {
    Request {
        key: RequestKey {
            actor: ObjectId("homer".into()),
            id: RequestId("alpha".into()),
            generation,
        },
        verb: "capture".into(),
        digest: "digest-alpha".into(),
    }
}

fn commit(generation: u64) -> Commit {
    Commit {
        request: request(generation),
        landed: "{\"address\":\"topic:alpha#f1\"}".into(),
        receipt: None,
        written_at: at(0),
        expires_at: at(10),
    }
}

fn part(name: &str) -> Part {
    Part {
        request: request(1),
        name: name.into(),
        recorded: super::super::Recorded {
            landed: name.into(),
            note: Some("frozen-plan".into()),
        },
        written_at: at(0),
        expires_at: at(10),
    }
}

fn app() -> AppAttempt {
    AppAttempt {
        key: AppKey {
            app_id: "homer".into(),
            event_key: "alpha".into(),
        },
        digest: "app-digest".into(),
        landed: "app-result".into(),
        written_at: at(0),
    }
}

/// One interpreter table covers every event, with independent actor, generation
/// and app controls. A lost receipt leaves the landed result available.
#[test]
fn ledger_events_interpret_the_request_and_app_lifecycles() {
    let mut state = State::default();
    let first = commit(1);
    let events = [
        LedgerEvent::RequestPartCommitted(part("entity")),
        LedgerEvent::RequestPartCommitted(part("claim")),
        LedgerEvent::RequestCompleted {
            commit: first.clone(),
            required_parts: vec!["entity".into(), "claim".into()],
        },
        LedgerEvent::RequestReceiptRecorded {
            key: first.request.key.clone(),
            receipt: "receipt-one".into(),
        },
        LedgerEvent::AppAttemptCommitted(app()),
        LedgerEvent::RequestExpired {
            key: first.request.key.clone(),
        },
        LedgerEvent::RequestCommitted(commit(2)),
        LedgerEvent::RequestReceiptRecorded {
            key: first.request.key.clone(),
            receipt: "late-old-receipt".into(),
        },
    ];
    for (index, event) in events.iter().enumerate() {
        state.apply(event).unwrap();
        let key = &first.request.key;
        match index {
            0 | 1 => {
                assert!(state.lookup(&key.actor, &key.id, at(0)).is_none());
                assert_eq!(state.parts(&key.actor, &key.id, at(0)).len(), index + 1);
            }
            2 => assert_eq!(state.lookup(&key.actor, &key.id, at(0)), Some(&first)),
            3 => {
                assert_eq!(
                    state
                        .lookup(&key.actor, &key.id, at(0))
                        .unwrap()
                        .receipt
                        .as_deref(),
                    Some("receipt-one")
                );
                assert!(
                    state
                        .lookup(&ObjectId("marge".into()), &key.id, at(0))
                        .is_none()
                );
            }
            4 => assert_eq!(state.app(&app().key), Some(&app())),
            5 => {
                assert!(state.lookup(&key.actor, &key.id, at(0)).is_none());
                assert!(state.parts(&key.actor, &key.id, at(0)).is_empty());
                assert_eq!(state.app(&app().key), Some(&app()));
            }
            6 | 7 => {
                let held = state.lookup(&key.actor, &key.id, at(10)).unwrap();
                assert_eq!(held.request.key.generation, 2);
                assert_eq!(held.receipt, None);
                assert!(state.lookup(&key.actor, &key.id, at(11)).is_none());
                assert_eq!(state.app(&app().key), Some(&app()));
            }
            _ => unreachable!(),
        }
        assert_eq!(state.history(), &events[..=index]);
        assert_eq!(State::replay(&events[..=index]).unwrap(), state);
    }
    let parts = &state.request(&first.request.key).unwrap().parts;
    assert_eq!(parts["claim"].recorded.note.as_deref(), Some("frozen-plan"));
}

#[test]
fn ledger_events_cannot_complete_a_missing_part_or_fill_an_uncommitted_receipt() {
    let mut state = State::default();
    let commit = commit(1);
    let before = state.clone();
    assert_eq!(
        state.apply(&LedgerEvent::RequestCompleted {
            commit: commit.clone(),
            required_parts: vec!["claim".into()],
        }),
        Err(MissingPart)
    );
    assert_eq!(state, before);
    state
        .apply(&LedgerEvent::RequestReceiptRecorded {
            key: commit.request.key.clone(),
            receipt: "not-a-commit".into(),
        })
        .unwrap();
    assert!(
        state
            .lookup(&commit.request.key.actor, &commit.request.key.id, at(0))
            .is_none()
    );
    state
        .apply(&LedgerEvent::RequestCommitted(commit.clone()))
        .unwrap();
    assert_eq!(
        state.lookup(&commit.request.key.actor, &commit.request.key.id, at(0)),
        Some(&commit)
    );
}

/// Every retained source column reaches the baseline event and the projection.
/// Import never claims that a receipt row proves a particular historical path.
#[test]
fn ledger_events_import_each_legacy_row_shape_without_inventing_completion() {
    let key = request(7).key;
    let rows = [
        LegacyRow::RequestPart {
            bot_key: key.actor.clone(),
            request_id: key.id.clone(),
            part: "entity".into(),
            verb: "add_entity".into(),
            args_digest: "part-digest".into(),
            landed: "part-result".into(),
            note: Some("plan".into()),
            written_at: at(1),
        },
        LegacyRow::RequestReceipt {
            bot_key: key.actor.clone(),
            request_id: key.id.clone(),
            verb: "capture".into(),
            args_digest: "receipt-digest".into(),
            landed: "landed-result".into(),
            receipt: None,
            written_at: at(2),
        },
        LegacyRow::AppWrite {
            app_id: "homer".into(),
            event_key: "alpha".into(),
            digest: "app-digest".into(),
            landed: "app-result".into(),
            written_at: at(3),
        },
    ];
    let mut state = State::default();
    for row in rows {
        let event = row.clone().into_event(7);
        assert_eq!(
            event,
            LedgerEvent::Imported {
                row: row.clone(),
                generation: 7
            }
        );
        state.apply(&event).unwrap();
        match row {
            LegacyRow::RequestPart {
                verb,
                args_digest,
                landed,
                note,
                written_at,
                ..
            } => {
                let held = &state.request(&key).unwrap().parts["entity"];
                assert_eq!(
                    (&held.request.verb, &held.request.digest),
                    (&verb, &args_digest)
                );
                assert_eq!(
                    (&held.recorded.landed, &held.recorded.note),
                    (&landed, &note)
                );
                assert_eq!(
                    (held.written_at, held.expires_at),
                    (written_at, written_at + super::super::KEPT_FOR)
                );
                assert!(state.request(&key).unwrap().committed.is_none());
            }
            LegacyRow::RequestReceipt {
                verb,
                args_digest,
                landed,
                receipt,
                written_at,
                ..
            } => {
                let held = state.request(&key).unwrap().committed.as_ref().unwrap();
                assert_eq!(
                    (&held.request.verb, &held.request.digest),
                    (&verb, &args_digest)
                );
                assert_eq!((&held.landed, &held.receipt), (&landed, &receipt));
                assert_eq!(
                    (held.written_at, held.expires_at),
                    (written_at, written_at + super::super::KEPT_FOR)
                );
                assert!(state.lookup(&key.actor, &key.id, held.expires_at).is_some());
                assert!(
                    state
                        .lookup(
                            &key.actor,
                            &key.id,
                            held.expires_at + jiff::SignedDuration::from_secs(1)
                        )
                        .is_none()
                );
            }
            LegacyRow::AppWrite {
                app_id,
                event_key,
                digest,
                landed,
                written_at,
            } => {
                let held = state.app(&AppKey { app_id, event_key }).unwrap();
                assert_eq!(
                    (&held.digest, &held.landed, held.written_at),
                    (&digest, &landed, written_at)
                );
            }
        }
    }
    assert_eq!(state.history().len(), 3);
    assert_eq!(State::replay(state.history()).unwrap(), state);
}

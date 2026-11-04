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

/// A completion replayed after the receipt fill must not take the receipt back.
#[test]
fn ledger_events_keep_a_filled_receipt_when_completion_replays() {
    let first = commit(1);
    let key = first.request.key.clone();
    let events = [
        LedgerEvent::RequestCommitted(first.clone()),
        LedgerEvent::RequestReceiptRecorded {
            key: key.clone(),
            receipt: "receipt-one".into(),
        },
        LedgerEvent::RequestCompleted {
            commit: first,
            required_parts: Vec::new(),
        },
    ];
    let state = State::replay(&events).unwrap();
    assert_eq!(
        state
            .lookup(&key.actor, &key.id, at(0))
            .unwrap()
            .receipt
            .as_deref(),
        Some("receipt-one")
    );
}

/// Parts answer in written_at order, then by name, and only while unexpired.
#[test]
fn ledger_events_order_parts_by_time_then_name_and_hide_expired_ones() {
    let timed = |name: &str, written: i64, expires: i64| Part {
        written_at: at(written),
        expires_at: at(expires),
        ..part(name)
    };
    let mut state = State::default();
    for part in [
        timed("b-late", 5, 20),
        timed("z-early", 1, 20),
        timed("a-tie", 5, 20),
        timed("m-short", 3, 8),
    ] {
        state
            .apply(&LedgerEvent::RequestPartCommitted(part))
            .unwrap();
    }
    let key = request(1).key;
    let names = |when| {
        state
            .parts(&key.actor, &key.id, when)
            .iter()
            .map(|part| part.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(at(0)), ["z-early", "m-short", "a-tie", "b-late"]);
    assert_eq!(names(at(8)), ["z-early", "m-short", "a-tie", "b-late"]);
    assert_eq!(names(at(9)), ["z-early", "a-tie", "b-late"]);
}

/// An expiry or a receipt that arrives before its commit changes nothing; the
/// log order decides, so a later commit stands unexpired and unfilled.
#[test]
fn ledger_events_ignore_expiry_and_receipt_that_precede_their_commit() {
    let first = commit(1);
    let key = first.request.key.clone();
    let mut state = State::default();
    state
        .apply(&LedgerEvent::RequestExpired { key: key.clone() })
        .unwrap();
    state
        .apply(&LedgerEvent::RequestReceiptRecorded {
            key: key.clone(),
            receipt: "early".into(),
        })
        .unwrap();
    state
        .apply(&LedgerEvent::RequestCommitted(first.clone()))
        .unwrap();
    assert_eq!(state.lookup(&key.actor, &key.id, at(0)), Some(&first));
}

/// Only the newest generation answers a retry, so expiring it hides the request
/// and does not reveal the older generation that is still held.
#[test]
fn ledger_events_do_not_reveal_a_held_older_generation_when_the_newest_expires() {
    let older = commit(1);
    let newer = commit(2);
    let key = older.request.key.clone();
    let mut state = State::default();
    for event in [
        LedgerEvent::RequestPartCommitted(part("entity")),
        LedgerEvent::RequestCommitted(older.clone()),
    ] {
        state.apply(&event).unwrap();
    }
    assert_eq!(state.lookup(&key.actor, &key.id, at(0)), Some(&older));
    assert_eq!(state.parts(&key.actor, &key.id, at(0)).len(), 1);
    state
        .apply(&LedgerEvent::RequestCommitted(newer.clone()))
        .unwrap();
    assert_eq!(state.lookup(&key.actor, &key.id, at(0)), Some(&newer));
    state
        .apply(&LedgerEvent::RequestExpired {
            key: newer.request.key.clone(),
        })
        .unwrap();
    assert!(state.lookup(&key.actor, &key.id, at(0)).is_none());
    assert!(state.parts(&key.actor, &key.id, at(0)).is_empty());
    let current = state.current(&key.actor, &key.id).unwrap();
    assert_eq!(current.committed.as_ref(), Some(&newer));
    assert!(current.expired);
    assert!(!state.request(&older.request.key).unwrap().expired);
}

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
            2 => {
                assert_eq!(state.lookup(&key.actor, &key.id, at(0)), Some(&first));
                let parts = &state.request(key).unwrap().parts;
                assert_eq!(parts["claim"].recorded.note.as_deref(), Some("frozen-plan"));
            }
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
        assert_eq!(State::replay(&events[..=index]).unwrap(), state);
    }
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
    let mut imported = Vec::new();
    for row in rows {
        let event = row.clone().into_event(7);
        imported.push(event.clone());
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
    assert_eq!(State::replay(&imported).unwrap(), state);
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
    assert!(state.current(&key.actor, &key.id).is_none());
    assert!(state.request(&newer.request.key).is_none());
    assert!(
        state.request(&older.request.key).is_none(),
        "the older generation goes with it"
    );
}

/// A write under an id joins the generation that still holds a live row and starts
/// the next one when every row of the newest generation has aged out or been
/// expired, so the number replay reproduces never reuses an old attempt.
#[test]
fn ledger_events_number_the_generation_a_new_write_belongs_to() {
    let mut state = State::default();
    let actor = ObjectId("homer".into());
    let id = RequestId("alpha".into());
    assert_eq!(
        state.generation_for(&actor, &id, at(0)),
        1,
        "an id never seen"
    );

    state
        .apply(&LedgerEvent::RequestPartCommitted(part("entity")))
        .unwrap();
    assert_eq!(
        state.generation_for(&actor, &id, at(5)),
        1,
        "a live part holds the generation open for the rest of the write"
    );
    assert_eq!(
        state.generation_for(&actor, &id, at(11)),
        2,
        "a part past its keep window is no longer held"
    );

    state
        .apply(&LedgerEvent::RequestCommitted(commit(2)))
        .unwrap();
    assert_eq!(state.generation_for(&actor, &id, at(5)), 2);
    assert_eq!(state.generation_for(&actor, &id, at(11)), 3);

    state
        .apply(&LedgerEvent::RequestExpired {
            key: request(2).key,
        })
        .unwrap();
    assert_eq!(
        state.generation_for(&actor, &id, at(5)),
        1,
        "an expired generation is evicted, so none is joined"
    );
    assert_eq!(
        state.generation_for(&ObjectId("marge".into()), &id, at(5)),
        1,
        "another actor's id is its own"
    );
}

fn commit_of(id: &str, written: i64) -> Commit {
    Commit {
        request: Request {
            key: RequestKey {
                actor: ObjectId("homer".into()),
                id: RequestId(id.into()),
                generation: 1,
            },
            verb: "capture".into(),
            digest: "digest-beta".into(),
        },
        landed: "{}".into(),
        receipt: None,
        written_at: at(written),
        expires_at: at(written + 10),
    }
}

/// A generation whose rows have all expired leaves the state when an event
/// that carries a later instant is applied. The instant is the event's own, so
/// replaying the same events holds the same state.
#[test]
fn ledger_events_evict_a_generation_when_an_event_past_its_window_is_applied() {
    let old = commit(1);
    let key = old.request.key.clone();
    let events = [
        LedgerEvent::RequestPartCommitted(part("entity")),
        LedgerEvent::RequestCommitted(old),
        LedgerEvent::RequestCommitted(commit_of("beta", 10)),
    ];
    let mut state = State::default();
    for event in &events {
        state.apply(event).unwrap();
    }
    assert!(
        state.request(&key).is_some(),
        "an event at the last instant of the window evicts nothing"
    );
    let late = LedgerEvent::RequestCommitted(commit_of("gamma", 11));
    state.apply(&late).unwrap();
    assert!(state.request(&key).is_none(), "the expired generation left");
    assert!(
        state.request(&commit_of("beta", 10).request.key).is_some(),
        "a generation still inside its window stays"
    );
    let replayed = State::replay(events.iter().chain([&late])).unwrap();
    assert_eq!(replayed, state, "replay evicts the same generations");
}

/// A fill or an expiry that addresses an evicted generation changes nothing, so
/// it can never attach to a newer attempt that reuses the id.
#[test]
fn ledger_events_ignore_a_receipt_for_an_evicted_generation() {
    let old = commit(1);
    let key = old.request.key.clone();
    let mut state = State::default();
    state.apply(&LedgerEvent::RequestCommitted(old)).unwrap();
    state
        .apply(&LedgerEvent::RequestCommitted(commit_of("beta", 11)))
        .unwrap();
    assert!(state.request(&key).is_none());
    let before = state.clone();
    state
        .apply(&LedgerEvent::RequestReceiptRecorded {
            key: key.clone(),
            receipt: "late".into(),
        })
        .unwrap();
    state.apply(&LedgerEvent::RequestExpired { key }).unwrap();
    assert_eq!(state, before);
}

/// **An id is kept for 72 hours and no longer, to the second.** At the last
/// instant of the window the id is still spent and a write under it joins its
/// generation; one second later it is unspent, and a write under it starts the
/// next generation, whose event evicts the old one by its own instant.
#[test]
fn ledger_events_keep_an_id_for_the_window_and_not_one_second_longer() {
    use crate::request::KEPT_FOR;
    let written = at(0);
    let spent = Commit {
        request: request(1),
        landed: "{}".into(),
        receipt: None,
        written_at: written,
        expires_at: written + KEPT_FOR,
    };
    let key = spent.request.key.clone();
    let mut state = State::default();
    state
        .apply(&LedgerEvent::RequestCommitted(spent.clone()))
        .unwrap();

    let last = written + KEPT_FOR;
    let after = last + jiff::SignedDuration::from_secs(1);
    assert!(state.lookup(&key.actor, &key.id, last).is_some());
    assert_eq!(state.generation_for(&key.actor, &key.id, last), 1);
    assert!(state.lookup(&key.actor, &key.id, after).is_none());
    assert_eq!(state.generation_for(&key.actor, &key.id, after), 2);

    // An event at the last instant evicts nothing; one a second later does.
    let mut again = spent.clone();
    again.request.key.generation = 2;
    again.written_at = last;
    again.expires_at = last + KEPT_FOR;
    let mut at_the_edge = state.clone();
    at_the_edge
        .apply(&LedgerEvent::RequestCommitted(commit_of("beta", 72 * 3600)))
        .unwrap();
    assert!(
        at_the_edge.request(&key).is_some(),
        "an event at the last instant evicts nothing"
    );

    again.written_at = after;
    again.expires_at = after + KEPT_FOR;
    state
        .apply(&LedgerEvent::RequestCommitted(again.clone()))
        .unwrap();
    assert!(
        state.request(&key).is_none(),
        "the first generation left when an event a second past its window was applied"
    );
    assert_eq!(
        state
            .lookup(&key.actor, &key.id, after)
            .map(|held| held.request.key.generation),
        Some(2)
    );
}

/// **The check answers as an apply on a copy would.** A batch is fine when every
/// part a completion names is held already or committed earlier in the same
/// batch, whether by its own event or by an imported row, and it is refused
/// when the part is missing or comes after the completion.
#[test]
fn a_batch_check_agrees_with_applying_the_batch_to_a_copy() {
    let completion = || LedgerEvent::RequestCompleted {
        commit: commit(1),
        required_parts: vec!["entity".into()],
    };
    let entity = || LedgerEvent::RequestPartCommitted(part("entity"));
    let imported = || LedgerEvent::Imported {
        row: LegacyRow::RequestPart {
            bot_key: ObjectId("homer".into()),
            request_id: RequestId("alpha".into()),
            part: "entity".into(),
            verb: "capture".into(),
            args_digest: "digest-alpha".into(),
            landed: "entity".into(),
            note: None,
            written_at: at(0),
        },
        generation: 1,
    };
    let mut holding = State::default();
    holding.apply(&entity()).unwrap();
    let table: [(&str, State, Vec<LedgerEvent>, bool); 7] = [
        ("an empty batch", State::default(), vec![], true),
        (
            "a completion whose part is held",
            holding.clone(),
            vec![completion()],
            true,
        ),
        (
            "a completion whose part is missing",
            State::default(),
            vec![completion()],
            false,
        ),
        (
            "the part comes first in the batch",
            State::default(),
            vec![entity(), completion()],
            true,
        ),
        (
            "the part comes after the completion",
            State::default(),
            vec![completion(), entity()],
            false,
        ),
        (
            "an imported row commits the part first",
            State::default(),
            vec![imported(), completion()],
            true,
        ),
        (
            "a commit with nothing to complete",
            State::default(),
            vec![LedgerEvent::RequestCommitted(commit(1))],
            true,
        ),
    ];
    for (name, state, batch, fine) in table {
        let mut copy = state.clone();
        let applied = batch.iter().try_for_each(|event| copy.apply(event));
        assert_eq!(applied.is_ok(), fine, "{name}: the oracle");
        assert_eq!(state.check(&batch).is_ok(), fine, "{name}: the check");
    }
}

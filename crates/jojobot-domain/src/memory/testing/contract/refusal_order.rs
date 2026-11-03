//! **Which refusal wins when a capture carries two faults.** The faults here are
//! names that no entity answers to: the subject, the edge's object, a ref and the
//! value of a reference-typed field. The store judges them in one order and a
//! caller is told the first miss, so a caller that fixes the named handle and
//! sends the call again meets the next.
//!
//! The decided order is the real store's: subject, edge, refs, reference fields.
//! The pairs where both stores already hold it are run by both. **A pair where the
//! fake judges in another order is run against the real store only**, and its doc
//! names the fake's run as the one that reddens until the admission moves.
//!
//! Run by one test of each store, not listed with the contract, so the contract's
//! file keeps its size.

use super::support::ensure;
use super::*;

/// The handle a capture's refusal says missed, or a panic naming what came back.
async fn missed<M: Memory>(store: &M, fact: NewFact) -> String {
    match store.capture(fact).await {
        Ok(Guarded::Blocked { attempted, .. }) => attempted.to_string(),
        other => panic!("a capture naming nobody must be blocked, got {other:?}"),
    }
}

/// **A malformed write is refused before a missing subject is named.** A setting
/// that has no value is a fault of the write alone, and it is judged first, so the
/// answer is the refusal of the write and not a request to create the subject.
/// **The real store judges the subject first and answers `Blocked`**, so this case
/// reddens against it until the admission moves, and only the fake runs it.
pub async fn a_setting_with_no_value_is_refused_before_a_missing_subject<M: Memory>(store: &M) {
    let fact = NewFact {
        sets: ["contract_order_setting".to_string()].into(),
        ..NewFact::about(
            EntityId("thing:contract-order-malformed-subject".into()),
            "a malformed write on nobody",
            date(2026, 7, 1),
        )
    };
    match store.capture(fact).await {
        Err(MemoryError::InvalidFact(_)) => {}
        other => panic!("a malformed write is refused before the subject is judged, got {other:?}"),
    }
}

/// **The subject is judged before the edge's object.** Both name nobody; the
/// answer names the subject.
pub async fn a_missing_subject_is_refused_before_a_missing_edge_object<M: Memory>(store: &M) {
    let fact = NewFact {
        edge: Some(Edge::new(
            EdgeShape::Location,
            EntityId("place:contract-order-no-place".into()),
        )),
        ..NewFact::about(
            EntityId("thing:contract-order-no-subject".into()),
            "two names that miss",
            date(2026, 7, 1),
        )
    };
    assert_eq!(
        missed(store, fact).await,
        "thing:contract-order-no-subject",
        "the subject is the first name judged"
    );
}

/// **The edge's object is judged before the refs.** The subject exists; the
/// edge's object and a ref both name nobody; the answer names the edge's object.
pub async fn a_missing_edge_object_is_refused_before_a_missing_ref<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-order-edge-subject".into());
    ensure(store, &subject).await;
    let fact = NewFact {
        edge: Some(Edge::new(
            EdgeShape::Location,
            EntityId("place:contract-order-no-edge".into()),
        )),
        refs: vec![EntityId("person:contract-order-edge-ref".into())],
        ..NewFact::about(subject, "two names that miss", date(2026, 7, 1))
    };
    assert_eq!(
        missed(store, fact).await,
        "place:contract-order-no-edge",
        "the edge's object is judged before the refs"
    );
}

/// **The refs are judged before the value of a reference-typed field.** The
/// subject exists; a ref and a reference field both name nobody; the answer names
/// the ref. **The fake judges the field first**, so this case reddens against it
/// until the admission moves, and only the real store runs it.
pub async fn a_missing_ref_is_refused_before_a_missing_reference_field<M: Memory>(store: &M) {
    store
        .declare_type(
            DeclaredType::new(
                "contract-order-loan",
                vec![Field::required("order_loaned_to", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("declaring a type of my own is accepted");
    let subject = EntityId("thing:contract-order-field-subject".into());
    ensure(store, &subject).await;
    let fact = NewFact {
        fields: [(
            "order_loaned_to".to_string(),
            "person:contract-order-the-field-value".to_string(),
        )]
        .into_iter()
        .collect(),
        refs: vec![EntityId("person:contract-order-first-ref".into())],
        ..NewFact::about(subject, "two names that miss", date(2026, 7, 1))
    };
    assert_eq!(
        missed(store, fact).await,
        "person:contract-order-first-ref",
        "the refs are judged before a reference field's value"
    );
}

/// **A thought over its body cap is refused before a full room is.** The room
/// holds its one thought and the new thought is too long; the answer names the
/// cap, so a caller that shortens the thought then meets the full room.
pub async fn a_thought_over_its_cap_is_refused_before_a_full_room<M: Memory>(store: &M) {
    let owner = EntityId("thing:contract-order-room-owner".into());
    let first = EntityId("thing:contract-order-room-first".into());
    let second = EntityId("thing:contract-order-room-second".into());
    for handle in [&owner, &first, &second] {
        ensure(store, handle).await;
    }
    let settings = super::support::state(NewFact {
        fields: [
            (crate::memory::THOUGHT_CAPACITY.to_string(), "1".to_string()),
            (crate::memory::THOUGHT_BODY_CAP.to_string(), "5".to_string()),
        ]
        .into_iter()
        .collect(),
        ..NewFact::about(owner.clone(), "room of one", date(2026, 7, 1))
    });
    store.capture(settings).await.expect("the settings land");
    let filling = NewFact {
        edge: Some(Edge::new(EdgeShape::Connection, first)),
        ..NewFact::about(owner.clone(), "fills", date(2026, 7, 2))
    };
    store
        .capture(filling)
        .await
        .expect("the room's one thought lands");
    let too_long = NewFact {
        edge: Some(Edge::new(EdgeShape::Connection, second)),
        ..NewFact::about(owner, "far too long for five", date(2026, 7, 3))
    };
    match store.capture(too_long).await {
        Err(MemoryError::ThoughtTooLong { .. }) => {}
        other => panic!("the cap is judged before the room, got {other:?}"),
    }
}

/// **A full room is refused before a full thread.** The thread holds its three
/// live records, one of them a thought, and its room of one thought is full; a
/// second thought breaks both limits. The answer names the room.
pub async fn a_full_room_is_refused_before_a_full_thread<M: Memory>(store: &M) {
    let thread = EntityId("thread:contract-order-ceiling".into());
    let first = EntityId("thing:contract-order-ceiling-first".into());
    let second = EntityId("thing:contract-order-ceiling-second".into());
    for handle in [&thread, &first, &second] {
        ensure(store, handle).await;
    }
    let settings = super::support::state(NewFact {
        fields: [(crate::memory::THOUGHT_CAPACITY.to_string(), "1".to_string())]
            .into_iter()
            .collect(),
        ..NewFact::about(thread.clone(), "room of one", date(2026, 7, 1))
    });
    store.capture(settings).await.expect("the settings land");
    let filling = NewFact {
        edge: Some(Edge::new(EdgeShape::Connection, first)),
        ..NewFact::about(thread.clone(), "fills the room", date(2026, 7, 2))
    };
    store.capture(filling).await.expect("the one thought lands");
    let third = NewFact::about(thread.clone(), "a plain record", date(2026, 7, 3));
    store.capture(third).await.expect("the third record lands");
    let both = NewFact {
        edge: Some(Edge::new(EdgeShape::Connection, second)),
        ..NewFact::about(thread, "breaks both limits", date(2026, 7, 4))
    };
    match store.capture(both).await {
        Err(MemoryError::RoomFull { .. }) => {}
        other => panic!("the room is judged before the thread's records, got {other:?}"),
    }
}

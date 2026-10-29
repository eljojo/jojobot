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

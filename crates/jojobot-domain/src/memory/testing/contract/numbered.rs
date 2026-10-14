use super::support::{add, capture, edit, other_caller, state, state_patch};
use super::*;
use std::collections::BTreeMap;

/// **A numbered collection and its members**, built the same way for every
/// case: a collection that numbers by `number`, three decisions numbered 1, 2
/// and 4, and a fourth, numbered 3, that has been archived. Handles are the
/// suite's own, so cases that share a store never meet.
struct Ordinances {
    collection: EntityId,
    curfew: EntityId,
    parking: EntityId,
    fireworks: EntityId,
    leaves: EntityId,
}

async fn ordinances<M: Memory>(store: &M, name: &str, handles: [&str; 5]) -> Ordinances {
    let [collection, curfew, parking, leaves, fireworks] = handles.map(|h| EntityId(h.to_string()));
    add(
        store,
        NewEntity::new(collection.clone(), name, "contract-fixture"),
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([("numbers_by".to_string(), "number".to_string())]),
            ..NewFact::about(collection.clone(), "numbers its members", date(2026, 5, 1))
        }),
    )
    .await;
    for (id, title, number) in [
        (&curfew, "Curfew", "1"),
        (&parking, "Parking", "2"),
        (&leaves, "Leaf burning", "3"),
        (&fireworks, "Fireworks", "4"),
    ] {
        add(
            store,
            NewEntity {
                parent: Some(collection.clone()),
                ..NewEntity::new(id.clone(), format!("{title} of {name}"), "contract-fixture")
            },
        )
        .await;
        capture(
            store,
            state(NewFact {
                fields: BTreeMap::from([
                    ("number".to_string(), number.to_string()),
                    ("title".to_string(), title.to_string()),
                ]),
                ..NewFact::about(
                    id.clone(),
                    format!("{title} is ordinance {number}"),
                    date(2026, 5, 2),
                )
            }),
        )
        .await;
    }
    store
        .archive_entity(&leaves, "repealed", None)
        .await
        .expect("archive ok");
    Ordinances {
        collection,
        curfew,
        parking,
        fireworks,
        leaves,
    }
}

fn numbered(collection: &EntityId, number: u64) -> EntityId {
    EntityId(format!("{collection}/{number}"))
}

/// **A member is named by its number, and the collection says the next one.**
/// `<collection>/<N>` reads the member holding N, an archived member's number
/// still resolves, and the next number is the largest held plus one, archived
/// members included.
pub async fn a_member_is_named_by_its_number_and_the_collection_says_the_next<M: Memory>(
    store: &M,
) {
    let o = ordinances(
        store,
        "Town ordinances",
        [
            "topic:contract-ordinances-named",
            "decision:contract-ordinance-named-curfew",
            "decision:contract-ordinance-named-parking",
            "decision:contract-ordinance-named-leaves",
            "decision:contract-ordinance-named-fireworks",
        ],
    )
    .await;
    let read = store
        .fields(&numbered(&o.collection, 2))
        .await
        .expect("a number a member holds resolves");
    assert_eq!(
        read.get("title").map(String::as_str),
        Some("Parking"),
        "/2 is the member holding 2: {read:?}"
    );
    let formers = store.former_handles().await.expect("the handles list");
    assert!(
        formers
            .iter()
            .any(|f| f.former == numbered(&o.collection, 2)),
        "a number a member holds is a handle it answers to, and the list says so: {formers:?}"
    );
    assert!(
        !formers
            .iter()
            .any(|f| f.former == numbered(&o.collection, 9)),
        "a number nobody holds is no handle"
    );
    let archived = store
        .fields(&numbered(&o.collection, 3))
        .await
        .expect("an archived member's number still resolves");
    assert_eq!(
        archived.get("title").map(String::as_str),
        Some("Leaf burning")
    );

    let numbering = store
        .numbering_of(&o.collection)
        .await
        .expect("the collection reads")
        .expect("it numbers its members");
    assert_eq!(numbering.key, "number");
    assert_eq!(
        numbering.next_number, 5,
        "the largest held is 4, and an archived 3 is held too"
    );
    assert_eq!(
        store.numbering_of(&o.parking).await.expect("reads"),
        None,
        "a member that numbers nothing says nothing"
    );
}

/// **A number is held by one member, archived or not**, and a bad value is
/// refused beside the good one. The refusal names the holder and the next free
/// number; sending the next free number lands.
pub async fn a_number_is_held_by_one_member_and_a_bad_value_is_refused<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Village bylaws",
        [
            "topic:contract-ordinances-guard",
            "decision:contract-ordinance-guard-curfew",
            "decision:contract-ordinance-guard-parking",
            "decision:contract-ordinance-guard-leaves",
            "decision:contract-ordinance-guard-fireworks",
        ],
    )
    .await;
    let sign = EntityId("decision:contract-ordinance-guard-signs".into());
    add(
        store,
        NewEntity {
            parent: Some(o.collection.clone()),
            ..NewEntity::new(sign.clone(), "Sign height", "contract-fixture")
        },
    )
    .await;
    let write = |value: &str| {
        state(NewFact {
            fields: BTreeMap::from([("number".to_string(), value.to_string())]),
            ..NewFact::about(
                sign.clone(),
                format!("signs take {value}"),
                date(2026, 5, 3),
            )
        })
    };

    // The number an archived member holds: refused, naming the holder.
    let Err(MemoryError::NumberTaken {
        holder,
        number,
        next,
        key,
        ..
    }) = store.capture(write("3")).await
    else {
        panic!("a number an archived member holds must be refused");
    };
    assert_eq!((holder.as_str(), number, next), (o.leaves.as_str(), 3, 5));
    assert_eq!(key, "number");
    // A number a live member holds: refused the same way.
    let Err(MemoryError::NumberTaken { holder, .. }) = store.capture(write("4")).await else {
        panic!("a number a live member holds must be refused");
    };
    assert_eq!(holder, o.fireworks.to_string());
    // Not a number: refused, and the refusal is not the duplicate one.
    for bad in ["two", "0", "-1", "2.5", "04", ""] {
        let refused = store.capture(write(bad)).await;
        assert!(
            matches!(refused, Err(MemoryError::NotANumber { .. })),
            "{bad:?} holds no positive whole number: {refused:?}"
        );
    }
    // Nothing was written by any refusal: the next number has not moved.
    assert_eq!(
        store
            .numbering_of(&o.collection)
            .await
            .expect("reads")
            .expect("numbered")
            .next_number,
        5
    );
    // The positive of the pair: the next free number lands, and the next moves.
    store
        .capture(write("5"))
        .await
        .expect("capture ok")
        .written()
        .expect("the next free number is not refused");
    assert_eq!(
        store
            .numbering_of(&o.collection)
            .await
            .expect("reads")
            .expect("numbered")
            .next_number,
        6
    );
    // An edit that moves a member onto a held number is refused too.
    let claim = capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "6".to_string())]),
            ..NewFact::about(sign.clone(), "signs take six", date(2026, 5, 4))
        }),
    )
    .await;
    let refused = store
        .update_fact(
            &claim.address(),
            state_patch(FactPatch {
                fields: BTreeMap::from([("number".to_string(), "1".to_string())]),
                ..FactPatch::default()
            }),
            &other_caller(),
        )
        .await;
    let Err(MemoryError::NumberTaken { holder, .. }) = refused else {
        panic!("an edit onto a held number must be refused: {refused:?}");
    };
    assert_eq!(holder, o.curfew.to_string());
    let edited = edit(
        store,
        &claim.address(),
        state_patch(FactPatch {
            fields: BTreeMap::from([("number".to_string(), "7".to_string())]),
            ..FactPatch::default()
        }),
    )
    .await;
    assert_eq!(edited.fields.get("number").map(String::as_str), Some("7"));
}

/// **The whole subtree is the scope, and nothing outside it is.** A grandchild
/// under a numbered member holds a number the collection knows; a thing outside
/// the collection may hold any number.
pub async fn the_whole_subtree_is_the_scope_and_a_thing_outside_is_not<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "County statutes",
        [
            "topic:contract-ordinances-scope",
            "decision:contract-ordinance-scope-curfew",
            "decision:contract-ordinance-scope-parking",
            "decision:contract-ordinance-scope-leaves",
            "decision:contract-ordinance-scope-fireworks",
        ],
    )
    .await;
    let grandchild = EntityId("decision:contract-ordinance-scope-amendment".into());
    add(
        store,
        NewEntity {
            parent: Some(o.curfew.clone()),
            ..NewEntity::new(grandchild.clone(), "Curfew amendment", "contract-fixture")
        },
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([
                ("number".to_string(), "6".to_string()),
                ("title".to_string(), "Curfew amendment".to_string()),
            ]),
            ..NewFact::about(grandchild.clone(), "amends the curfew", date(2026, 5, 3))
        }),
    )
    .await;
    assert_eq!(
        store
            .numbering_of(&o.collection)
            .await
            .expect("reads")
            .expect("numbered")
            .next_number,
        7,
        "a grandchild's 6 is part of the scope"
    );
    let found = store
        .fields(&numbered(&o.collection, 6))
        .await
        .expect("a grandchild resolves through the collection");
    assert_eq!(
        found.get("title").map(String::as_str),
        Some("Curfew amendment")
    );
    // A second grandchild cannot take 6.
    let twin = EntityId("decision:contract-ordinance-scope-twin".into());
    add(
        store,
        NewEntity {
            parent: Some(o.parking.clone()),
            ..NewEntity::new(twin.clone(), "Parking amendment", "contract-fixture")
        },
    )
    .await;
    let refused = store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "6".to_string())]),
            ..NewFact::about(twin.clone(), "amends parking", date(2026, 5, 4))
        }))
        .await;
    assert!(
        matches!(&refused, Err(MemoryError::NumberTaken { holder, .. }) if holder == &grandchild.to_string()),
        "a deep member's number is held across the subtree: {refused:?}"
    );

    // Outside the collection, the same key may hold any number.
    let outside = EntityId("decision:contract-ordinance-scope-outside".into());
    add(
        store,
        NewEntity::new(outside.clone(), "Outside", "contract-fixture"),
    )
    .await;
    store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "2".to_string())]),
            ..NewFact::about(outside.clone(), "holds 2 outside", date(2026, 5, 5))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("a thing outside the subtree is not held to the collection");
    assert_eq!(
        store
            .fields(&numbered(&o.collection, 2))
            .await
            .expect("still resolves")
            .get("title")
            .map(String::as_str),
        Some("Parking"),
        "/2 still names the member of the collection"
    );
}

/// **A number nobody holds is an unknown handle with the numbers that exist.**
pub async fn a_number_nobody_holds_is_unknown_and_offers_the_numbers_held<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Borough codes",
        [
            "topic:contract-ordinances-miss",
            "decision:contract-ordinance-miss-curfew",
            "decision:contract-ordinance-miss-parking",
            "decision:contract-ordinance-miss-leaves",
            "decision:contract-ordinance-miss-fireworks",
        ],
    )
    .await;
    let refused = store.fields(&numbered(&o.collection, 9)).await;
    let Err(MemoryError::UnknownEntity { attempted, nearest }) = refused else {
        panic!("a number nobody holds is no thing: {refused:?}");
    };
    assert_eq!(attempted, numbered(&o.collection, 9).to_string());
    for held in [1, 2, 3, 4] {
        assert!(
            nearest
                .iter()
                .any(|m| m.handle == numbered(&o.collection, held)),
            "the numbers that exist are what makes it repairable: {nearest:?}"
        );
    }
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_numbered<M: Memory>(store: &M) {
    a_member_is_named_by_its_number_and_the_collection_says_the_next(store).await;
    a_number_is_held_by_one_member_and_a_bad_value_is_refused(store).await;
    the_whole_subtree_is_the_scope_and_a_thing_outside_is_not(store).await;
    a_number_nobody_holds_is_unknown_and_offers_the_numbers_held(store).await;
}

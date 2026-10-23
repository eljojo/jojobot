use super::support::{add, capture, edit, other_caller, state, state_patch};
use super::*;
use std::collections::BTreeMap;

/// **A numbered collection and its members**, built the same way for every
/// case: a collection that numbers by `number`, three decisions numbered 1, 2
/// and 4, and a fourth, numbered 3, that has been archived. Handles are the
/// suite's own, so cases that share a store never meet.
pub(super) struct Ordinances {
    pub(super) collection: EntityId,
    pub(super) curfew: EntityId,
    pub(super) parking: EntityId,
    pub(super) fireworks: EntityId,
    pub(super) leaves: EntityId,
}

pub(super) async fn ordinances<M: Memory + ?Sized>(
    store: &M,
    name: &str,
    handles: [&str; 5],
) -> Ordinances {
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

pub(super) fn numbered(collection: &EntityId, number: u64) -> EntityId {
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
    let five = store
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
    let claim = five;
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
    // **A member keeps the number it holds: no write gives it a different one**,
    // so no other member can ever take it. Giving it another number is refused
    // by a capture and by an edit alike, naming the member and its number, and
    // the way to a different number is to retract the claim that gave this one.
    let kept = |refused: Result<_, MemoryError>, what: &str| match refused {
        Err(MemoryError::NumberFreed { holder, number, .. }) => {
            assert_eq!((holder.as_str(), number), (sign.as_str(), 5), "{what}");
        }
        other => panic!("{what}: a held number is not replaced: {other:?}"),
    };
    kept(
        store.capture(write("6")).await,
        "a capture of another number",
    );
    kept(
        store
            .update_fact(
                &claim.address(),
                state_patch(FactPatch {
                    fields: BTreeMap::from([("number".to_string(), "7".to_string())]),
                    ..FactPatch::default()
                }),
                &other_caller(),
            )
            .await,
        "an edit to another number",
    );
    assert_eq!(
        store
            .numbering_of(&o.collection)
            .await
            .expect("reads")
            .expect("numbered")
            .next_number,
        6,
        "the refusals wrote nothing, and 6 and 7 are nobody's"
    );
    // Sending the number the member holds again lands, by an edit and by a
    // capture: the holder it would collide with is itself.
    let edited = edit(
        store,
        &claim.address(),
        state_patch(FactPatch {
            fields: BTreeMap::from([("number".to_string(), "5".to_string())]),
            ..FactPatch::default()
        }),
    )
    .await;
    assert_eq!(edited.fields.get("number").map(String::as_str), Some("5"));
    store
        .capture(write("5"))
        .await
        .expect("capture ok")
        .written()
        .expect("a member's own number is not taken from it");
}

/// **The numbers end below the largest a machine holds, so the next one always
/// exists.** The next number is the largest held plus one; a member that held the
/// largest number there is would leave none. So that number is no number: a write
/// of it is refused as a value that is no number, and the highest one a member can
/// hold leaves a next that is the largest, read and named without a panic or a
/// wrap. **Paired**: a smaller number lands beside it, and the refusal of a number
/// held names the next one.
pub async fn the_highest_number_a_member_can_hold_still_leaves_a_next<M: Memory>(store: &M) {
    let collection = EntityId("topic:contract-ordinances-ceiling".into());
    let first = EntityId("decision:contract-ordinance-ceiling-first".into());
    let second = EntityId("decision:contract-ordinance-ceiling-second".into());
    add(
        store,
        NewEntity::new(collection.clone(), "Ceiling codes", "contract-fixture"),
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
    for (id, name) in [(&first, "Highest ruling"), (&second, "Lowest ruling")] {
        add(
            store,
            NewEntity {
                parent: Some(collection.clone()),
                ..NewEntity::new(id.clone(), name, "contract-fixture")
            },
        )
        .await;
    }
    let write = |id: &EntityId, number: u64| {
        state(NewFact {
            fields: BTreeMap::from([("number".to_string(), number.to_string())]),
            ..NewFact::about(id.clone(), format!("takes {number}"), date(2026, 5, 2))
        })
    };
    let highest = u64::MAX - 1;
    store
        .capture(write(&first, highest))
        .await
        .expect("capture ok")
        .written()
        .expect("the highest number there is room above is a number");
    assert_eq!(
        store
            .numbering_of(&collection)
            .await
            .expect("the collection reads")
            .expect("numbered")
            .next_number,
        u64::MAX,
        "the next number is the largest there is, and no wrap"
    );
    let refused = store.capture(write(&second, u64::MAX)).await;
    assert!(
        matches!(refused, Err(MemoryError::NotANumber { .. })),
        "the largest number leaves no next, so it is no number: {refused:?}"
    );
    let Err(MemoryError::NumberTaken { holder, next, .. }) =
        store.capture(write(&second, highest)).await
    else {
        panic!("a held number is refused, and naming the next one does not overflow");
    };
    assert_eq!((holder.as_str(), next), (first.as_str(), u64::MAX));
    store
        .capture(write(&second, 1))
        .await
        .expect("capture ok")
        .written()
        .expect("a smaller number lands beside the highest");
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

/// **A numbered handle is never given to a thing.** It names the member that
/// holds the number, so a thing made or renamed to one would be found before
/// the member. Refused for both, and the member still answers beside it.
pub async fn a_numbered_handle_is_never_given_to_a_new_thing<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Council rulings",
        [
            "topic:contract-ordinances-given",
            "decision:contract-ordinance-given-curfew",
            "decision:contract-ordinance-given-parking",
            "decision:contract-ordinance-given-leaves",
            "decision:contract-ordinance-given-fireworks",
        ],
    )
    .await;
    let taken = numbered(&o.collection, 2);
    let made = store
        .add_entity(NewEntity::new(
            taken.clone(),
            "Impostor",
            "contract-fixture",
        ))
        .await;
    assert!(
        matches!(made, Err(MemoryError::InvalidEntity(_))),
        "a numbered handle is not a handle to make: {made:?}"
    );
    let renamed = store
        .rename_entity(
            &o.curfew,
            &taken,
            None,
            date(2026, 5, 6),
            crate::memory::Override::none(),
            None,
        )
        .await;
    assert!(
        matches!(renamed, Err(MemoryError::InvalidEntity(_))),
        "nor one to rename a thing to: {renamed:?}"
    );
    let still = store
        .fields(&taken)
        .await
        .expect("the member still answers to its number");
    assert_eq!(still.get("title").map(String::as_str), Some("Parking"));
}

/// A member under `parent`, holding `number` under the key when it is given.
async fn member_of<M: Memory>(
    store: &M,
    parent: &EntityId,
    handle: &str,
    number: Option<&str>,
) -> (EntityId, FactAddress) {
    let id = EntityId(handle.to_string());
    add(
        store,
        NewEntity {
            parent: Some(parent.clone()),
            ..NewEntity::new(id.clone(), handle, "contract-fixture")
        },
    )
    .await;
    let mut fields = BTreeMap::from([("title".to_string(), handle.to_string())]);
    if let Some(number) = number {
        fields.insert("number".to_string(), number.to_string());
    }
    let claim = capture(
        store,
        state(NewFact {
            fields,
            ..NewFact::about(
                id.clone(),
                format!("{} was filed", handle.rsplit('-').next().unwrap_or(handle)),
                date(2026, 5, 2),
            )
        }),
    )
    .await;
    (id, claim.address())
}

/// A collection that does not number yet, with a claim to write `numbers_by` on.
async fn plain_collection<M: Memory>(store: &M, handle: &str) -> (EntityId, FactAddress) {
    let id = EntityId(handle.to_string());
    add(
        store,
        NewEntity::new(id.clone(), handle, "contract-fixture"),
    )
    .await;
    let claim = capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([("title".to_string(), handle.to_string())]),
            ..NewFact::about(
                id.clone(),
                format!("{handle} holds things"),
                date(2026, 5, 1),
            )
        }),
    )
    .await;
    (id, claim.address())
}

fn numbers_by(key: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("numbers_by".to_string(), key.to_string())])
}

/// **A collection keeps its promise from the moment it exists.** Writing
/// `numbers_by` onto a thing whose descendants already hold a duplicate or a
/// value that is no number under that key is refused, naming those members, by
/// a capture and by an edit alike; a subtree that already keeps the promise
/// takes the key and is held to it afterwards. Changing `numbers_by` to another
/// key frees the numbers held under the old one, so it is refused while a member
/// holds one.
pub async fn a_collection_is_founded_only_over_members_that_already_keep_its_promise<M: Memory>(
    store: &M,
) {
    // **First write of `numbers_by` in the store**, so a store that numbers
    // nothing yet is asked too.
    // Two members hold 5: refused by a capture and by an edit, naming both.
    let (twice, twice_claim) = plain_collection(store, "topic:contract-founding-twice").await;
    let (a, a_claim) = member_of(store, &twice, "decision:contract-fm-twice-lamp", Some("5")).await;
    let (b, _) = member_of(store, &twice, "decision:contract-fm-twice-bench", Some("5")).await;
    let by_capture = store
        .capture(state(NewFact {
            fields: numbers_by("number"),
            ..NewFact::about(twice.clone(), "numbers its members", date(2026, 5, 3))
        }))
        .await;
    let Err(MemoryError::MembersBreakNumbering { members, key, .. }) = &by_capture else {
        panic!("a duplicated subtree must be refused: {by_capture:?}");
    };
    assert_eq!(key, "number");
    for member in [&a, &b] {
        assert!(
            members.iter().any(|m| m.contains(member.as_str())),
            "the refusal names {member}: {members:?}"
        );
    }
    let by_edit = store
        .update_fact(
            &twice_claim,
            state_patch(FactPatch {
                fields: numbers_by("number"),
                ..FactPatch::default()
            }),
            &other_caller(),
        )
        .await;
    assert!(
        matches!(&by_edit, Err(MemoryError::MembersBreakNumbering { .. })),
        "an edit is held to the same refusal: {by_edit:?}"
    );
    assert!(
        store.numbering_of(&twice).await.expect("reads").is_none(),
        "a refusal wrote nothing"
    );
    // A clean subtree: distinct numbers, a member with none, a grandchild, and
    // an archived member whose number still counts.
    let (tidy, tidy_claim) = plain_collection(store, "topic:contract-founding-tidy").await;
    let (first, _) = member_of(
        store,
        &tidy,
        "decision:contract-fm-tidy-pavement",
        Some("1"),
    )
    .await;
    member_of(store, &tidy, "decision:contract-fm-tidy-blank", None).await;
    let (retired, _) = member_of(
        store,
        &tidy,
        "decision:contract-fm-tidy-repealed",
        Some("3"),
    )
    .await;
    member_of(
        store,
        &first,
        "decision:contract-fm-tidy-grandchild",
        Some("2"),
    )
    .await;
    store
        .archive_entity(&retired, "repealed", None)
        .await
        .expect("archive ok");
    edit(
        store,
        &tidy_claim,
        state_patch(FactPatch {
            fields: numbers_by("number"),
            ..FactPatch::default()
        }),
    )
    .await;
    assert_eq!(
        store
            .numbering_of(&tidy)
            .await
            .expect("reads")
            .expect("a clean subtree took the key")
            .next_number,
        4,
        "the archived member's 3 counts"
    );
    let late = member_of(store, &tidy, "decision:contract-fm-tidy-newcomer", None)
        .await
        .0;
    let taken = store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "2".to_string())]),
            ..NewFact::about(late, "takes a held number", date(2026, 5, 3))
        }))
        .await;
    assert!(
        matches!(&taken, Err(MemoryError::NumberTaken { .. })),
        "the collection holds its members to the key once it has it: {taken:?}"
    );
    // The same key again changes nothing and lands.
    capture(
        store,
        state(NewFact {
            fields: numbers_by("number"),
            ..NewFact::about(tidy.clone(), "numbers by the same key", date(2026, 5, 4))
        }),
    )
    .await;

    // Repair one holder and the key lands.
    edit(
        store,
        &a_claim,
        state_patch(FactPatch {
            fields: BTreeMap::from([("number".to_string(), "6".to_string())]),
            ..FactPatch::default()
        }),
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: numbers_by("number"),
            ..NewFact::about(twice.clone(), "numbers its members", date(2026, 5, 5))
        }),
    )
    .await;
    assert_eq!(
        store
            .numbering_of(&twice)
            .await
            .expect("reads")
            .expect("numbered once the duplicate was repaired")
            .next_number,
        7
    );

    // A value that is no number: refused, naming the member that holds it.
    let (words, _) = plain_collection(store, "topic:contract-founding-words").await;
    let (spelled, _) = member_of(
        store,
        &words,
        "decision:contract-fm-words-spelling",
        Some("five"),
    )
    .await;
    let refused = store
        .capture(state(NewFact {
            fields: numbers_by("number"),
            ..NewFact::about(words.clone(), "numbers its members", date(2026, 5, 3))
        }))
        .await;
    let Err(MemoryError::MembersBreakNumbering { members, .. }) = &refused else {
        panic!("a value that is no number must be refused: {refused:?}");
    };
    assert!(
        members.iter().any(|m| m.contains(spelled.as_str())),
        "{members:?}"
    );

    // Another key frees the numbers held under the old one.
    let rekey = ordinances(
        store,
        "Founding rekey",
        [
            "topic:contract-founding-rekey",
            "decision:contract-fm-rekey-one",
            "decision:contract-fm-rekey-two",
            "decision:contract-fm-rekey-three",
            "decision:contract-fm-rekey-four",
        ],
    )
    .await;
    let freed = store
        .capture(state(NewFact {
            fields: numbers_by("ordinal"),
            ..NewFact::about(
                rekey.collection.clone(),
                "numbers by ordinal",
                date(2026, 5, 3),
            )
        }))
        .await;
    assert!(
        matches!(&freed, Err(MemoryError::NumberFreed { key, .. }) if key == "number"),
        "a member holds a number under the old key: {freed:?}"
    );
    // **Taking the key off frees the numbers just as another key does.** The
    // collection's own claim gives it `numbers_by`; clearing the key on it is
    // refused while a member holds a number, and nothing is written.
    let gave_key = store
        .recall(&rekey.collection)
        .await
        .expect("the collection reads")
        .into_iter()
        .find(|fact| fact.fields.contains_key("numbers_by"))
        .expect("a claim gives the collection its key")
        .address();
    let dropped = store
        .update_fact(
            &gave_key,
            state_patch(FactPatch {
                clear_fields: vec!["numbers_by".to_string()],
                ..FactPatch::default()
            }),
            &other_caller(),
        )
        .await;
    assert!(
        matches!(&dropped, Err(MemoryError::NumberFreed { key, .. }) if key == "number"),
        "a member holds a number under the key being taken off: {dropped:?}"
    );
    assert!(
        store
            .numbering_of(&rekey.collection)
            .await
            .expect("reads")
            .is_some(),
        "a refused removal leaves the collection numbering"
    );
    let still = store
        .fields(&numbered(&rekey.collection, 2))
        .await
        .expect("a number still names its member");
    assert_eq!(still.get("title").map(String::as_str), Some("Parking"));
    // With no member holding a number under the old key, the key may change.
    let (open, _) = plain_collection(store, "topic:contract-founding-open").await;
    capture(
        store,
        state(NewFact {
            fields: numbers_by("number"),
            ..NewFact::about(open.clone(), "numbers its members", date(2026, 5, 3))
        }),
    )
    .await;
    member_of(store, &open, "decision:contract-fm-open-vacant", None).await;
    capture(
        store,
        state(NewFact {
            fields: numbers_by("ordinal"),
            ..NewFact::about(open.clone(), "numbers by ordinal", date(2026, 5, 4))
        }),
    )
    .await;

    // A collection no member holds a number in can take the key off.
    let (bare, bare_claim) = plain_collection(store, "topic:contract-founding-bare").await;
    member_of(store, &bare, "decision:contract-fm-bare-empty", None).await;
    edit(
        store,
        &bare_claim,
        state_patch(FactPatch {
            fields: numbers_by("number"),
            ..FactPatch::default()
        }),
    )
    .await;
    edit(
        store,
        &bare_claim,
        state_patch(FactPatch {
            clear_fields: vec!["numbers_by".to_string()],
            ..FactPatch::default()
        }),
    )
    .await;
    assert!(
        store.numbering_of(&bare).await.expect("reads").is_none(),
        "with no number held, the key came off and the thing numbers nothing"
    );
}

/// **A number a collection issued is never issued again, a retracted claim's
/// included.** A member takes the next number, its claim is retracted, and the
/// collection still says the number after it as next; another member asking for
/// the retracted number is refused as retired and the refusal writes nothing; a
/// number nobody was issued still lands, and the next number moves past it.
pub async fn a_retracted_members_number_is_never_issued_again<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Retired bylaws",
        [
            "topic:contract-ordinances-retired",
            "decision:contract-ordinance-retired-curfew",
            "decision:contract-ordinance-retired-parking",
            "decision:contract-ordinance-retired-leaves",
            "decision:contract-ordinance-retired-fireworks",
        ],
    )
    .await;
    async fn next_of<M: Memory>(store: &M, collection: &EntityId) -> u64 {
        store
            .numbering_of(collection)
            .await
            .expect("the collection reads")
            .expect("it numbers its members")
            .next_number
    }
    let next = |_why: &'static str| next_of(store, &o.collection);
    let [signs, banner] = ["signs", "banner"]
        .map(|slug| EntityId(format!("decision:contract-ordinance-retired-{slug}")));
    for (id, title) in [(&signs, "Sandwich boards"), (&banner, "Streamer widths")] {
        add(
            store,
            NewEntity {
                parent: Some(o.collection.clone()),
                ..NewEntity::new(id.clone(), title, "contract-fixture")
            },
        )
        .await;
    }
    let number = |value: &str| BTreeMap::from([("number".to_string(), value.to_string())]);
    assert_eq!(next("before").await, 5, "the fixture holds 1 to 4");
    let given = capture(
        store,
        state(NewFact {
            fields: number("5"),
            ..NewFact::about(signs.clone(), "signs are ordinance five", date(2026, 6, 3))
        }),
    )
    .await;
    assert_eq!(next("given").await, 6, "the positive the rest rests on");

    store
        .retract(
            &given.address(),
            Some("written in error"),
            date(2026, 6, 4),
            &other_caller(),
            None,
        )
        .await
        .expect("a retraction is served");
    assert!(
        !store
            .fields(&signs)
            .await
            .expect("the member reads")
            .contains_key("number"),
        "the retraction took the number off the member"
    );
    assert_eq!(next("retracted").await, 6, "the collection still issued 5");

    let before = store.fields(&banner).await.expect("banner reads");
    let refused = store
        .capture(state(NewFact {
            fields: number("5"),
            ..NewFact::about(banner.clone(), "banner takes five", date(2026, 6, 5))
        }))
        .await;
    assert!(
        matches!(&refused, Err(MemoryError::NumberRetired { number: 5, next: 6, key, .. })
            if key == "number"),
        "a number issued and taken back is refused, naming the next: {refused:?}"
    );
    assert_eq!(
        store.fields(&banner).await.expect("banner reads"),
        before,
        "the refusal wrote nothing"
    );
    capture(
        store,
        state(NewFact {
            fields: number("6"),
            ..NewFact::about(banner.clone(), "banner takes six", date(2026, 6, 5))
        }),
    )
    .await;
    assert_eq!(
        next("six").await,
        7,
        "the next number moves past the one given"
    );
}

/// **A merge keeps the numbers a collection issued.** A member whose claim was
/// retracted holds no number, so a merge folding it away is not refused by the
/// guards for a held number; the number its claim carried is still issued when
/// the survivor is a sibling in the collection.
pub async fn a_merge_keeps_the_numbers_a_collection_issued<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Merged bylaws",
        [
            "topic:contract-ordinances-merged",
            "decision:contract-ordinance-merged-curfew",
            "decision:contract-ordinance-merged-parking",
            "decision:contract-ordinance-merged-leaves",
            "decision:contract-ordinance-merged-fireworks",
        ],
    )
    .await;
    let [gate, sibling] = ["gate", "sibling"]
        .map(|slug| EntityId(format!("decision:contract-ordinance-merged-{slug}")));
    let member = |id: &EntityId, title: &str| {
        add(
            store,
            NewEntity {
                parent: Some(o.collection.clone()),
                ..NewEntity::new(id.clone(), title, "contract-fixture")
            },
        )
    };
    member(&gate, "Gate hours").await;
    let number = |value: &str| BTreeMap::from([("number".to_string(), value.to_string())]);
    let mut claims = Vec::new();
    for (id, value) in [(&gate, "5")] {
        claims.push(
            capture(
                store,
                state(NewFact {
                    fields: number(value),
                    ..NewFact::about(id.clone(), format!("{value} was given"), date(2026, 6, 3))
                }),
            )
            .await,
        );
    }
    for claim in &claims {
        store
            .retract(
                &claim.address(),
                Some("written in error"),
                date(2026, 6, 4),
                &other_caller(),
                None,
            )
            .await
            .expect("a retraction is served");
    }
    // **The survivor joined after the number was given and taken back**, so the
    // record it inherits is the folded member's and is judged by when that member
    // joined, not by when the survivor did.
    member(&sibling, "Lamp spacing").await;
    for (folded, survivor) in [(&gate, &sibling)] {
        store
            .merge(
                folded,
                survivor,
                Some("one thing filed twice"),
                date(2026, 6, 5),
                &other_caller(),
                None,
            )
            .await
            .expect("a member that holds no number merges");
    }
    let numbering = store
        .numbering_of(&o.collection)
        .await
        .expect("the collection reads")
        .expect("it numbers its members");
    assert_eq!(numbering.next_number, 6, "5 was issued before the merge");
    for taken in ["5"] {
        let refused = store
            .capture(state(NewFact {
                fields: number(taken),
                ..NewFact::about(sibling.clone(), format!("{taken} again"), date(2026, 6, 6))
            }))
            .await;
        assert!(
            matches!(&refused, Err(MemoryError::NumberRetired { number, next: 6, .. })
                if number.to_string() == taken),
            "{taken} stays issued after the merge: {refused:?}"
        );
    }
}

/// **A member that leaves the collection leaves no number free.** A member whose
/// claim was retracted holds no number, but the collection issued the one that
/// claim carried. Taking the member out, by a merge into a thing outside or by a
/// move to another parent, must not lose it: whether the write is refused or lands,
/// the collection still says the number after it as next, and a member asking for
/// the retracted number is refused. Archiving the member, which keeps it in the
/// collection, is the way out that is served.
pub async fn a_member_that_leaves_takes_no_issued_number_with_it<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Departures register",
        [
            "topic:contract-ordinances-left",
            "decision:contract-ordinance-left-curfew",
            "decision:contract-ordinance-left-parking",
            "decision:contract-ordinance-left-leaves",
            "decision:contract-ordinance-left-fireworks",
        ],
    )
    .await;
    let [gate, kerb, taker, outside] = ["gate", "kerb", "taker", "outside"]
        .map(|slug| EntityId(format!("decision:contract-ordinance-left-{slug}")));
    for (id, title, parent) in [
        (&gate, "Latch timings", Some(&o.collection)),
        (&kerb, "Verge colour", Some(&o.collection)),
        (&taker, "Bollard gaps", Some(&o.collection)),
        (&outside, "Distant statute", None),
    ] {
        add(
            store,
            NewEntity {
                parent: parent.cloned(),
                ..NewEntity::new(id.clone(), title, "contract-fixture")
            },
        )
        .await;
    }
    let number = |value: &str| BTreeMap::from([("number".to_string(), value.to_string())]);
    for (id, value) in [(&gate, "5"), (&kerb, "6")] {
        let given = capture(
            store,
            state(NewFact {
                fields: number(value),
                ..NewFact::about(id.clone(), format!("{value} was given"), date(2026, 6, 3))
            }),
        )
        .await;
        store
            .retract(
                &given.address(),
                Some("written in error"),
                date(2026, 6, 4),
                &other_caller(),
                None,
            )
            .await
            .expect("a retraction is served");
    }
    // Both ways out are refused, naming the member and the number it carries.
    let merged = store
        .merge(
            &gate,
            &outside,
            Some("one thing filed twice"),
            date(2026, 6, 5),
            &other_caller(),
            None,
        )
        .await;
    assert!(
        matches!(&merged, Err(MemoryError::NumberFreed { holder, number: 5, taken_back: true, .. })
            if holder == gate.as_str()),
        "a merge out of the collection is refused for the number gate carries: {merged:?}"
    );
    let moved = store
        .rename_entity(
            &kerb,
            &kerb,
            Some(outside.clone()),
            date(2026, 6, 5),
            crate::memory::Override::none(),
            None,
        )
        .await;
    assert!(
        matches!(&moved, Err(MemoryError::NumberFreed { holder, number: 6, taken_back: true, .. })
            if holder == kerb.as_str()),
        "a move out of the collection is refused for the number kerb carries: {moved:?}"
    );
    // The way out that is served: archiving keeps the member and the record.
    store
        .archive_entity(&gate, "repealed", None)
        .await
        .expect("archiving a member that carries a number is served");
    let numbering = store
        .numbering_of(&o.collection)
        .await
        .expect("the collection reads")
        .expect("it numbers its members");
    assert_eq!(
        numbering.next_number, 7,
        "5 and 6 were issued before they left"
    );
    for taken in ["5", "6"] {
        let refused = store
            .capture(state(NewFact {
                fields: number(taken),
                ..NewFact::about(taker.clone(), format!("{taken} again"), date(2026, 6, 6))
            }))
            .await;
        assert!(
            matches!(&refused, Err(MemoryError::NumberRetired { number, next: 7, .. })
                if number.to_string() == taken),
            "{taken} stays issued after its member left: {refused:?}"
        );
    }
}

/// **A number given before the collection numbered is not one it issued.** A child
/// of a thing that numbers nothing is given a number and the claim is retracted;
/// the thing then numbers its members by that key. The collection issued nothing,
/// so the next number is 1 and the number the child once held is free.
pub async fn a_number_given_before_the_collection_numbered_was_never_issued<M: Memory>(store: &M) {
    let [topic, child, member] = ["topic:contract-ordinances-early", "child", "member"].map(|h| {
        EntityId(if h.starts_with("topic:") {
            h.to_string()
        } else {
            format!("decision:contract-ordinance-early-{h}")
        })
    });
    add(
        store,
        NewEntity::new(topic.clone(), "Early bylaws", "contract-fixture"),
    )
    .await;
    for (id, title) in [(&child, "Awning heights"), (&member, "Planter sizes")] {
        add(
            store,
            NewEntity {
                parent: Some(topic.clone()),
                ..NewEntity::new(id.clone(), title, "contract-fixture")
            },
        )
        .await;
    }
    let number = |value: &str| BTreeMap::from([("number".to_string(), value.to_string())]);
    let given = capture(
        store,
        state(NewFact {
            fields: number("99"),
            ..NewFact::about(child.clone(), "99 was given early", date(2026, 6, 3))
        }),
    )
    .await;
    store
        .retract(
            &given.address(),
            Some("written in error"),
            date(2026, 6, 4),
            &other_caller(),
            None,
        )
        .await
        .expect("a retraction is served");
    capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([("numbers_by".to_string(), "number".to_string())]),
            ..NewFact::about(topic.clone(), "numbers its members", date(2026, 6, 5))
        }),
    )
    .await;
    let numbering = store
        .numbering_of(&topic)
        .await
        .expect("the collection reads")
        .expect("it numbers its members");
    assert_eq!(numbering.next_number, 1, "nothing was issued yet");
    capture(
        store,
        state(NewFact {
            fields: number("99"),
            ..NewFact::about(member.clone(), "99 is taken", date(2026, 6, 6))
        }),
    )
    .await;
}

/// **A number taken back before the member joined is not one this collection
/// issued.** A thing outside a numbered collection is given a number and the claim
/// is retracted; the thing then joins the collection holding no number. The
/// collection issued nothing, so its next number stays 1 and the number the
/// outsider once held is free for a member.
pub async fn a_number_taken_back_before_the_member_joined_was_never_issued_here<M: Memory>(
    store: &M,
) {
    let [topic, outsider, member] =
        ["topic:contract-ordinances-joined", "outsider", "member"].map(|h| {
            EntityId(if h.starts_with("topic:") {
                h.to_string()
            } else {
                format!("decision:contract-ordinance-joined-{h}")
            })
        });
    add(
        store,
        NewEntity::new(topic.clone(), "Joined bylaws", "contract-fixture"),
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([("numbers_by".to_string(), "number".to_string())]),
            ..NewFact::about(topic.clone(), "numbers its members", date(2026, 6, 1))
        }),
    )
    .await;
    add(
        store,
        NewEntity {
            parent: Some(topic.clone()),
            ..NewEntity::new(member.clone(), "Fence colours", "contract-fixture")
        },
    )
    .await;
    add(
        store,
        NewEntity::new(outsider.clone(), "Shed permits", "contract-fixture"),
    )
    .await;
    let number = |value: &str| BTreeMap::from([("number".to_string(), value.to_string())]);
    let given = capture(
        store,
        state(NewFact {
            fields: number("99"),
            ..NewFact::about(outsider.clone(), "99 was given outside", date(2026, 6, 3))
        }),
    )
    .await;
    store
        .retract(
            &given.address(),
            Some("written in error"),
            date(2026, 6, 4),
            &other_caller(),
            None,
        )
        .await
        .expect("a retraction is served");
    let next = || async {
        store
            .numbering_of(&topic)
            .await
            .expect("the collection reads")
            .expect("it numbers its members")
            .next_number
    };
    assert_eq!(next().await, 1, "nothing was issued before the join");

    store
        .rename_entity(
            &outsider,
            &outsider,
            Some(topic.clone()),
            date(2026, 6, 5),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("a thing that holds no number joins");
    assert_eq!(
        next().await,
        1,
        "the number it once held was not issued here"
    );
    capture(
        store,
        state(NewFact {
            fields: number("99"),
            ..NewFact::about(member.clone(), "99 is taken", date(2026, 6, 6))
        }),
    )
    .await;
    assert_eq!(next().await, 100, "and 99 lands once a member asks for it");
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_numbered<M: Memory>(store: &M) {
    a_member_is_named_by_its_number_and_the_collection_says_the_next(store).await;
    a_number_is_held_by_one_member_and_a_bad_value_is_refused(store).await;
    the_whole_subtree_is_the_scope_and_a_thing_outside_is_not(store).await;
    a_number_nobody_holds_is_unknown_and_offers_the_numbers_held(store).await;
    a_numbered_handle_is_never_given_to_a_new_thing(store).await;
    a_collection_is_founded_only_over_members_that_already_keep_its_promise(store).await;
    the_highest_number_a_member_can_hold_still_leaves_a_next(store).await;
    a_retracted_members_number_is_never_issued_again(store).await;
    a_member_that_leaves_takes_no_issued_number_with_it(store).await;
    a_number_given_before_the_collection_numbered_was_never_issued(store).await;
    a_merge_keeps_the_numbers_a_collection_issued(store).await;
    a_number_taken_back_before_the_member_joined_was_never_issued_here(store).await;
    super::numbering_paths::no_write_path_skips_a_numbering_guard_except_a_retraction_by_design(
        store,
    )
    .await;
    super::numbering_paths::a_write_that_keeps_every_number_with_one_holder_still_lands(store)
        .await;
    super::numbering_paths::a_folded_things_child_is_held_to_the_survivors_numbers(store).await;
    super::numbering_paths::declaring_the_numbering_key_a_counter_is_held_to_the_numbers(store)
        .await;
    super::numbering_paths::redeclaring_the_numbering_key_without_its_counter_is_held_to_the_numbers(
        store,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::testing::fake::InMemoryMemory;

    #[tokio::test]
    async fn fake_never_issues_a_retracted_members_number_again() {
        a_retracted_members_number_is_never_issued_again(&InMemoryMemory::booted()).await;
    }

    #[tokio::test]
    async fn fake_loses_no_issued_number_when_a_member_leaves() {
        a_member_that_leaves_takes_no_issued_number_with_it(&InMemoryMemory::booted()).await;
    }

    #[tokio::test]
    async fn fake_issues_nothing_for_a_number_taken_back_before_the_member_joined() {
        a_number_taken_back_before_the_member_joined_was_never_issued_here(
            &InMemoryMemory::booted(),
        )
        .await;
    }

    #[tokio::test]
    async fn fake_issues_nothing_before_the_collection_numbers() {
        a_number_given_before_the_collection_numbered_was_never_issued(&InMemoryMemory::booted())
            .await;
    }

    #[tokio::test]
    async fn fake_keeps_the_numbers_a_collection_issued_through_a_merge() {
        a_merge_keeps_the_numbers_a_collection_issued(&InMemoryMemory::booted()).await;
    }
}

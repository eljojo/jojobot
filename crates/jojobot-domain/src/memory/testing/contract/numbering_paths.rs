//! **No write path skips a guard a collection's numbers depend on** (card 2395),
//! proven against any [`Memory`] adapter.
//!
//! A number in a numbered collection names one member for ever. Three guards
//! keep that true: **one holder** (no two members hold a number), **keeps its
//! number** (a number held before a write is held after it) and **the kind
//! floor** (a write may not take a required key off a thing that held it).
//! Capture and update_fact met all three; merge and a reparent changed what a
//! collection holds with none of them. **A retraction is held to none of them
//! on purpose**: it says the record should never have been written, so it is
//! never refused for what the record gave. "Never reused" is about a member
//! that is retired, which is archived and keeps its number.
//!
//! A fourth guard asks the collection itself: **founding** (a write that gives a
//! thing `numbers_by`, another key or none is judged over the members it already
//! has).
//!
//! The table below is every write path against every guard. [`attempt`] matches
//! the pair with no wildcard, so a new path or a new guard is a compile error
//! until its cells are written, and a cell that is not a refusal says why.

use super::numbered::{Ordinances, numbered, ordinances};
use super::support::{add, capture, other_caller, state};
use super::*;
use std::collections::BTreeMap;

/// The verbs that change what a collection or a member holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Path {
    Capture,
    UpdateFact,
    Merge,
    Reparent,
    Retract,
}

const PATHS: [Path; 5] = [
    Path::Capture,
    Path::UpdateFact,
    Path::Merge,
    Path::Reparent,
    Path::Retract,
];

/// What a write is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Guard {
    OneHolder,
    KeepsNumber,
    KindFloor,
    Founding,
}

const GUARDS: [Guard; 4] = [
    Guard::OneHolder,
    Guard::KeepsNumber,
    Guard::KindFloor,
    Guard::Founding,
];

/// What one cell of the table did.
enum Cell {
    /// The write was refused. `names` is a word the refusal has to carry: the
    /// member that holds the number, or the key the floor protects.
    Refused {
        result: Result<(), MemoryError>,
        names: String,
    },
    /// The path cannot reach the guard, and the reason it cannot.
    CannotReach(&'static str),
    /// The path is not held to the guard on purpose, so the write lands.
    ServedOnPurpose {
        result: Result<(), MemoryError>,
        why: &'static str,
    },
}

/// The world every cell is attempted in. **Every refusal writes nothing**, so
/// one world serves them all, and each cell is checked against what the world
/// held before it.
struct World {
    o: Ordinances,
    /// A member that holds no number.
    signs: EntityId,
    /// A member that holds 8, from one claim.
    banner: EntityId,
    banner_claim: FactAddress,
    /// A member that holds 7.
    rider: EntityId,
    rider_claim: FactAddress,
    /// A thing outside the collection that holds 2, a number the collection holds.
    outsider: EntityId,
    /// A thing outside the collection with no number.
    elsewhere: EntityId,
    /// Parking's own claim, the one that gives it 2.
    parking_claim: FactAddress,
    /// The collection's own claim, the one that gives it `numbers_by`.
    collection_claim: FactAddress,
    /// A second collection with one numbered member, whose key a retraction takes
    /// off.
    annals_claim: FactAddress,
    /// A promise in the collection that holds its day, the key its kind requires.
    pledge: EntityId,
    pledge_claim: FactAddress,
    /// A thing outside the collection that wrote a day, then took it off.
    former: EntityId,
    /// A member that holds no number and has a child that holds 11.
    annex: EntityId,
    annex_child: EntityId,
}

fn member(collection: &EntityId, id: &EntityId, name: &str) -> NewEntity {
    NewEntity {
        parent: Some(collection.clone()),
        ..NewEntity::new(id.clone(), name, "contract-fixture")
    }
}

async fn claim_about<M: Memory>(
    store: &M,
    about: &EntityId,
    words: &str,
    fields: &[(&str, &str)],
) -> Fact {
    capture(
        store,
        state(NewFact {
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..NewFact::about(about.clone(), words, date(2026, 6, 1))
        }),
    )
    .await
}

async fn the_world<M: Memory>(store: &M) -> World {
    // The promise kind's required day is a key the store holds once the kinds
    // are seeded, which a store's boot does and this case may not assume.
    crate::memory::kinds::seed(store)
        .await
        .expect("the kinds are seeded");
    let o = ordinances(
        store,
        "Pathways collection",
        [
            "topic:contract-ordinances-paths",
            "decision:contract-ordinance-paths-curfew",
            "decision:contract-ordinance-paths-parking",
            "decision:contract-ordinance-paths-leaves",
            "decision:contract-ordinance-paths-fireworks",
        ],
    )
    .await;
    let handle = |raw: &str| EntityId(raw.to_string());
    let signs = handle("decision:contract-ordinance-paths-signs");
    let banner = handle("decision:contract-ordinance-paths-banner");
    let rider = handle("decision:contract-ordinance-paths-rider");
    let outsider = handle("decision:contract-ordinance-paths-outsider");
    let elsewhere = handle("decision:contract-ordinance-paths-elsewhere");
    let pledge = handle("promise:contract-ordinance-paths-pledge");
    let former = handle("thing:contract-ordinance-paths-former");
    let annex = handle("decision:contract-ordinance-paths-outbuilding");
    let annex_child = handle("decision:contract-ordinance-paths-extension");
    for (id, name) in [
        (&signs, "Pathways signs"),
        (&banner, "Pathways banner"),
        (&rider, "Pathways rider"),
        (&pledge, "Pathways pledge"),
        (&annex, "Pathways outbuilding"),
    ] {
        add(store, member(&o.collection, id, name)).await;
    }
    add(store, member(&annex, &annex_child, "Pathways extension")).await;
    claim_about(store, &annex_child, "wing is eleven", &[("number", "11")]).await;
    for (id, name) in [
        (&outsider, "Pathways outsider"),
        (&elsewhere, "Pathways elsewhere"),
        (&former, "Pathways former"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let banner_fact = claim_about(store, &banner, "banner is eight", &[("number", "8")]).await;
    let rider_fact = claim_about(store, &rider, "rider is seven", &[("number", "7")]).await;
    claim_about(store, &outsider, "outsider holds two", &[("number", "2")]).await;
    let pledge_fact = claim_about(
        store,
        &pledge,
        "pledge falls due",
        &[("promised_by", "2026-12-01")],
    )
    .await;
    // The former thing wrote a day and then took it off, after the pledge wrote
    // its own: folded into the pledge, its newest write of the key is the removal.
    let former_fact = claim_about(
        store,
        &former,
        "former had a day",
        &[("promised_by", "2026-12-02")],
    )
    .await;
    store
        .update_fact(
            &former_fact.address(),
            FactPatch {
                clear_fields: vec!["promised_by".to_string()],
                ..Default::default()
            },
            &other_caller(),
        )
        .await
        .expect("a thing with no floor may drop its day");
    let parking_claim = store
        .recall(&o.parking)
        .await
        .expect("parking reads")
        .into_iter()
        .find(|f| f.fields.contains_key("number"))
        .expect("parking holds a claim that gives it a number")
        .address();
    let collection_claim = store
        .recall(&o.collection)
        .await
        .expect("the collection reads")
        .into_iter()
        .find(|f| f.fields.contains_key("numbers_by"))
        .expect("the collection holds a claim that gives it its key")
        .address();
    let annals = handle("topic:contract-ordinance-paths-annals");
    let entry = handle("decision:contract-ordinance-paths-entry");
    add(
        store,
        NewEntity::new(annals.clone(), "Pathways annals", "contract-fixture"),
    )
    .await;
    let annals_fact =
        claim_about(store, &annals, "annals number", &[("numbers_by", "number")]).await;
    add(store, member(&annals, &entry, "Pathways entry")).await;
    claim_about(store, &entry, "entry is one", &[("number", "1")]).await;
    World {
        o,
        collection_claim,
        annals_claim: annals_fact.address(),
        signs,
        banner,
        banner_claim: banner_fact.address(),
        rider,
        rider_claim: rider_fact.address(),
        outsider,
        elsewhere,
        parking_claim,
        pledge,
        pledge_claim: pledge_fact.address(),
        former,
        annex,
        annex_child,
    }
}

/// **What the world holds**, as a string two reads can be compared by: the
/// numbering, the fields of every member the table touches and where the
/// moved things sit.
async fn held<M: Memory>(store: &M, w: &World) -> String {
    let mut out = format!(
        "{:?}",
        store
            .numbering_of(&w.o.collection)
            .await
            .expect("the collection reads")
    );
    for id in [
        &w.o.curfew,
        &w.o.parking,
        &w.o.fireworks,
        &w.signs,
        &w.banner,
        &w.rider,
        &w.outsider,
        &w.elsewhere,
        &w.pledge,
        &w.former,
        &w.annex_child,
    ] {
        let fields = store.fields(id).await;
        out.push_str(&format!("\n{id}: {fields:?}"));
    }
    let listed = store.list_entities(None).await.expect("the roster lists");
    for id in [
        &w.o.parking,
        &w.outsider,
        &w.elsewhere,
        &w.former,
        &w.signs,
        &w.annex,
    ] {
        let row = listed.iter().find(|e| &e.id == id);
        out.push_str(&format!(
            "\n{id} sits under {:?}, folded into {:?}",
            row.and_then(|e| e.parent.clone()),
            row.and_then(|e| e.merged_into.clone())
        ));
    }
    out
}

async fn next_number<M: Memory>(store: &M, collection: &EntityId) -> u64 {
    store
        .numbering_of(collection)
        .await
        .expect("the collection reads")
        .expect("it numbers its members")
        .next_number
}

fn landed<T>(result: Result<T, MemoryError>) -> Result<(), MemoryError> {
    result.map(|_| ())
}

fn refused(result: Result<(), MemoryError>, names: &EntityId) -> Cell {
    Cell::Refused {
        result,
        names: names.to_string(),
    }
}

fn refused_key(result: Result<(), MemoryError>, key: &str) -> Cell {
    Cell::Refused {
        result,
        names: key.to_string(),
    }
}

async fn merge<M: Memory>(
    store: &M,
    folded: &EntityId,
    survivor: &EntityId,
) -> Result<(), MemoryError> {
    landed(
        store
            .merge(
                folded,
                survivor,
                Some("one thing filed twice"),
                date(2026, 6, 2),
                &other_caller(),
                Some("session-paths"),
            )
            .await,
    )
}

async fn reparent<M: Memory>(
    store: &M,
    thing: &EntityId,
    parent: &EntityId,
) -> Result<(), MemoryError> {
    landed(
        store
            .rename_entity(
                thing,
                thing,
                Some(parent.clone()),
                date(2026, 6, 2),
                crate::memory::Override::none(),
                None,
            )
            .await,
    )
}

async fn retract<M: Memory>(store: &M, address: &FactAddress) -> Result<(), MemoryError> {
    landed(
        store
            .retract(
                address,
                Some("written in error"),
                date(2026, 6, 2),
                &other_caller(),
                None,
            )
            .await,
    )
}

async fn update<M: Memory>(
    store: &M,
    address: &FactAddress,
    patch: FactPatch,
) -> Result<(), MemoryError> {
    landed(store.update_fact(address, patch, &other_caller()).await)
}

/// **One cell of the table.** No wildcard: a pair left out does not compile.
async fn attempt<M: Memory>(store: &M, w: &World, path: Path, guard: Guard) -> Cell {
    match (path, guard) {
        (Path::Capture, Guard::OneHolder) => refused(
            landed(
                store
                    .capture(state(NewFact {
                        fields: BTreeMap::from([("number".to_string(), "4".to_string())]),
                        ..NewFact::about(w.signs.clone(), "signs take four", date(2026, 6, 2))
                    }))
                    .await,
            ),
            &w.o.fireworks,
        ),
        // The rider holds 7, and a capture of another number would free it.
        (Path::Capture, Guard::KeepsNumber) => refused(
            landed(
                store
                    .capture(state(NewFact {
                        fields: BTreeMap::from([("number".to_string(), "9".to_string())]),
                        ..NewFact::about(w.rider.clone(), "rider takes nine", date(2026, 6, 2))
                    }))
                    .await,
            ),
            &w.rider,
        ),
        (Path::Capture, Guard::KindFloor) => refused_key(
            landed(
                store
                    .capture(state(NewFact {
                        fields: BTreeMap::from([("promised_by".to_string(), "soon".to_string())]),
                        ..NewFact::about(w.pledge.clone(), "pledge, vaguely", date(2026, 6, 2))
                    }))
                    .await,
            ),
            "promised_by",
        ),

        // The collection's numbers belong to its members, so a key to number them
        // by is judged against them: another key frees every number held.
        (Path::Capture, Guard::Founding) => refused(
            landed(
                store
                    .capture(state(NewFact {
                        fields: BTreeMap::from([("numbers_by".to_string(), "ordinal".to_string())]),
                        ..NewFact::about(
                            w.o.collection.clone(),
                            "numbers by ordinal",
                            date(2026, 6, 2),
                        )
                    }))
                    .await,
            ),
            &w.banner,
        ),

        (Path::UpdateFact, Guard::OneHolder) => refused(
            update(store, &w.rider_claim, state_patch_of("number", "1")).await,
            &w.o.curfew,
        ),
        (Path::UpdateFact, Guard::KeepsNumber) => refused(
            update(
                store,
                &w.rider_claim,
                FactPatch {
                    clear_fields: vec!["number".to_string()],
                    ..Default::default()
                },
            )
            .await,
            &w.rider,
        ),
        (Path::UpdateFact, Guard::KindFloor) => refused_key(
            update(
                store,
                &w.pledge_claim,
                FactPatch {
                    clear_fields: vec!["promised_by".to_string()],
                    ..Default::default()
                },
            )
            .await,
            "promised_by",
        ),

        // Taking the key off frees every number held under it.
        (Path::UpdateFact, Guard::Founding) => refused(
            update(
                store,
                &w.collection_claim,
                FactPatch {
                    clear_fields: vec!["numbers_by".to_string()],
                    ..Default::default()
                },
            )
            .await,
            &w.banner,
        ),

        // The outsider holds 2 and parking holds 2: signs, which holds none,
        // would serve the outsider's 2 and the collection would have two holders.
        (Path::Merge, Guard::OneHolder) => {
            refused(merge(store, &w.outsider, &w.signs).await, &w.o.parking)
        }
        // Parking leaves the collection with its 2.
        (Path::Merge, Guard::KeepsNumber) => {
            refused(merge(store, &w.o.parking, &w.elsewhere).await, &w.o.parking)
        }
        // The former thing's newest write of the day is the removal.
        (Path::Merge, Guard::KindFloor) => {
            refused_key(merge(store, &w.former, &w.pledge).await, "promised_by")
        }

        (Path::Merge, Guard::Founding) => {
            Cell::CannotReach("a merge reads its survivor through the fold and founds nothing")
        }

        (Path::Reparent, Guard::OneHolder) => refused(
            reparent(store, &w.outsider, &w.o.collection).await,
            &w.o.parking,
        ),
        (Path::Reparent, Guard::KeepsNumber) => refused(
            reparent(store, &w.o.parking, &w.elsewhere).await,
            &w.o.parking,
        ),
        (Path::Reparent, Guard::KindFloor) => {
            Cell::CannotReach("a reparent writes no field, so no key can leave the thing")
        }

        (Path::Reparent, Guard::Founding) => {
            Cell::CannotReach("a reparent writes no field, so no numbers_by is set or taken off")
        }

        // **A retraction is served on purpose, by all three guards**: it says the
        // claim should never have been written, so a claim somebody wants taken
        // back is never held back by what it gave (the floor of a kind, a number).
        // The guards are for the ordinary writes. Banner's only claim gives it 8;
        // parking's only claim gives it 2.
        (Path::Retract, Guard::OneHolder) => Cell::ServedOnPurpose {
            result: retract(store, &w.banner_claim).await,
            why: "a retraction takes back a record that should never have been written",
        },
        (Path::Retract, Guard::KeepsNumber) => Cell::ServedOnPurpose {
            result: retract(store, &w.parking_claim).await,
            why: "a number given by mistake must be takeable back; a retired member is archived",
        },
        (Path::Retract, Guard::KindFloor) => Cell::ServedOnPurpose {
            result: retract(store, &w.pledge_claim).await,
            why: "a retraction takes a record back and is never held back by a floor",
        },
        (Path::Retract, Guard::Founding) => Cell::ServedOnPurpose {
            result: retract(store, &w.annals_claim).await,
            why: "a key given by mistake must be takeable back",
        },
    }
}

fn state_patch_of(key: &str, value: &str) -> FactPatch {
    super::support::state_patch(FactPatch {
        fields: BTreeMap::from([(key.to_string(), value.to_string())]),
        ..FactPatch::default()
    })
}

/// Whether a refusal is the guard's own.
fn is_the_refusal_of(guard: Guard, error: &MemoryError) -> bool {
    match guard {
        Guard::OneHolder => matches!(error, MemoryError::NumberTaken { .. }),
        Guard::KeepsNumber => matches!(error, MemoryError::NumberFreed { .. }),
        Guard::KindFloor => matches!(
            error,
            MemoryError::BreaksFit { .. } | MemoryError::BreaksType { .. }
        ),
        Guard::Founding => matches!(
            error,
            MemoryError::NumberFreed { .. } | MemoryError::MembersBreakNumbering { .. }
        ),
    }
}

/// **Every write path is held to every guard, and a refusal writes nothing.**
/// The one exception is a retraction, and the table says so in three cells: it
/// is never refused for what the record gave, because it says the record should
/// never have been written (the comment on `guard_fit`).
///
/// A cell is a refusal by that guard's own error, naming the member or the key,
/// with the world exactly as it was before the attempt. A cell that is not a
/// refusal says why, so a path that quietly skips a guard cannot hide as one.
/// **Paired** by [`a_write_that_keeps_every_number_with_one_holder_still_lands`]:
/// the refusals are about the numbers, not about merging, moving or retracting.
pub async fn no_write_path_skips_a_numbering_guard_except_a_retraction_by_design<M: Memory>(
    store: &M,
) {
    let w = the_world(store).await;
    let mut served_on_purpose = Vec::new();
    // **Every cell is attempted and every failure is named**, so a path that
    // skips two guards is read as two cells and not as the first of them.
    let mut failures: Vec<String> = Vec::new();
    for path in PATHS {
        for guard in GUARDS {
            let before = held(store, &w).await;
            let cell = attempt(store, &w, path, guard).await;
            let cell_name = format!("{path:?} x {guard:?}");
            match cell {
                Cell::Refused { result, names } => match result {
                    Ok(()) => failures.push(format!("{cell_name}: the write landed")),
                    Err(error) if !is_the_refusal_of(guard, &error) => {
                        failures.push(format!("{cell_name}: refused by another guard: {error:?}"))
                    }
                    Err(error) if !error.to_string().contains(&names) => failures.push(format!(
                        "{cell_name}: the refusal does not name {names}: {error}"
                    )),
                    Err(_) => {
                        if held(store, &w).await != before {
                            failures.push(format!(
                                "{cell_name}: a refused write changed what the world holds"
                            ));
                        }
                    }
                },
                Cell::CannotReach(why) => {
                    assert!(
                        !why.is_empty(),
                        "{cell_name}: a cell that cannot be reached says why"
                    );
                }
                Cell::ServedOnPurpose { result, why } => match result {
                    Ok(()) => served_on_purpose.push(cell_name),
                    Err(error) => failures.push(format!("{cell_name}: {why}: {error:?}")),
                },
            }
        }
    }
    assert!(
        failures.is_empty(),
        "a write path skipped a guard:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        served_on_purpose,
        [
            "Retract x OneHolder",
            "Retract x KeepsNumber",
            "Retract x KindFloor",
            "Retract x Founding"
        ],
        "the only path left unguarded on purpose is a retraction, for all four"
    );
    // **A number under a member that moves is held with it.** The annex holds no
    // number; what is under it does. Folding the annex into a thing outside the
    // collection, or moving it out, would carry the 11 out too.
    let before = held(store, &w).await;
    let folded_out = merge(store, &w.annex, &w.elsewhere).await;
    let moved_out = reparent(store, &w.annex, &w.elsewhere).await;
    for (how, result) in [("merge", folded_out), ("reparent", moved_out)] {
        let error = result.expect_err(&format!("{how}: the annex carries a number out"));
        assert!(
            matches!(&error, MemoryError::NumberFreed { holder, number: 11, .. }
                if holder == &w.annex_child.to_string()),
            "{how}: refused for the number under the annex: {error:?}"
        );
    }
    assert_eq!(held(store, &w).await, before, "the refusals wrote nothing");

    // **What "never reused" means**: a member that is retired is archived and
    // keeps its number, so nobody else takes it. Leaves is archived; the 3 it holds
    // is refused, naming it. **A number a retraction took back is issued no more
    // either**: parking's 2 above is held by nobody and refused as retired, and the
    // collection's next number is unchanged by it.
    let refused = store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "3".to_string())]),
            ..NewFact::about(w.signs.clone(), "signs take three", date(2026, 6, 3))
        }))
        .await;
    assert!(
        matches!(&refused, Err(MemoryError::NumberTaken { holder, .. })
            if holder == &w.o.leaves.to_string()),
        "a retired member keeps its number: {refused:?}"
    );
    let refused = store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "2".to_string())]),
            ..NewFact::about(w.signs.clone(), "signs take two", date(2026, 6, 3))
        }))
        .await;
    assert!(
        matches!(&refused, Err(MemoryError::NumberRetired { number: 2, .. })),
        "a number a retraction took back is not issued again: {refused:?}"
    );
    let next = store
        .numbering_of(&w.o.collection)
        .await
        .expect("the collection reads")
        .expect("it numbers its members")
        .next_number;
    store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), next.to_string())]),
            ..NewFact::about(w.signs.clone(), "signs take the next", date(2026, 6, 3))
        }))
        .await
        .expect("a number nobody was issued lands")
        .written()
        .expect("not blocked");
}

/// **A number names one member, whatever was merged.** A thing outside the
/// collection holds a child; the thing is folded into a member of the collection.
/// The child is now served under that member, so a write on it is held to the
/// collection's numbers exactly as a write on the member's own child: a number
/// the collection holds is refused, naming its holder, and the number the child
/// holds cannot be freed. **Paired**: the child's own number and a write that
/// sets none still land, so the refusals are about the numbers.
pub async fn a_folded_things_child_is_held_to_the_survivors_numbers<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Folding collection",
        [
            "topic:contract-ordinances-folding",
            "decision:contract-ordinance-folding-curfew",
            "decision:contract-ordinance-folding-parking",
            "decision:contract-ordinance-folding-leaves",
            "decision:contract-ordinance-folding-fireworks",
        ],
    )
    .await;
    let handle = |raw: &str| EntityId(raw.to_string());
    let survivor = handle("decision:contract-ordinance-folding-survivor");
    let folded = handle("decision:contract-ordinance-folding-former");
    let child = handle("decision:contract-ordinance-folding-child");
    add(store, member(&o.collection, &survivor, "Folding survivor")).await;
    claim_about(store, &survivor, "survivor is five", &[("number", "5")]).await;
    add(
        store,
        NewEntity::new(folded.clone(), "Folding former", "contract-fixture"),
    )
    .await;
    add(store, member(&folded, &child, "Folding child")).await;
    claim_about(store, &child, "child is twelve", &[("number", "12")]).await;

    merge(store, &folded, &survivor)
        .await
        .expect("a thing outside the collection folds into a member");

    let taken = store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "5".to_string())]),
            ..NewFact::about(child.clone(), "child takes five", date(2026, 6, 3))
        }))
        .await;
    assert!(
        matches!(&taken, Err(MemoryError::NumberTaken { holder, .. })
            if holder == &survivor.to_string()),
        "a number the collection holds is refused on a folded thing's child: {taken:?}"
    );
    let freed = store
        .capture(state(NewFact {
            fields: BTreeMap::from([("number".to_string(), "13".to_string())]),
            ..NewFact::about(child.clone(), "child takes thirteen", date(2026, 6, 3))
        }))
        .await;
    assert!(
        matches!(&freed, Err(MemoryError::NumberFreed { holder, number: 12, .. })
            if holder == &child.to_string()),
        "the number a folded thing's child holds cannot be freed: {freed:?}"
    );

    for fields in [
        BTreeMap::from([("number".to_string(), "12".to_string())]),
        BTreeMap::from([("note".to_string(), "no number here".to_string())]),
    ] {
        store
            .capture(state(NewFact {
                fields,
                ..NewFact::about(child.clone(), "child, again", date(2026, 6, 4))
            }))
            .await
            .expect("a write that keeps the numbers lands")
            .written()
            .expect("not blocked");
    }
    let twelve = store
        .fields(&numbered(&o.collection, 12))
        .await
        .expect("12 names the folded thing's child");
    assert_eq!(twelve.get("number").map(String::as_str), Some("12"));
}

/// **A write that keeps every number with one holder lands, and the number keeps
/// naming its member.** The negative of the table: merging an unnumbered member
/// into a numbered one, merging a numbered one into an unnumbered member (the
/// survivor serves the number), moving a thing with a free number into the
/// collection, moving a member within it, and retracting a claim that does not
/// set the number.
pub async fn a_write_that_keeps_every_number_with_one_holder_still_lands<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Landings collection",
        [
            "topic:contract-ordinances-lands",
            "decision:contract-ordinance-lands-curfew",
            "decision:contract-ordinance-lands-parking",
            "decision:contract-ordinance-lands-leaves",
            "decision:contract-ordinance-lands-fireworks",
        ],
    )
    .await;
    let handle = |raw: &str| EntityId(raw.to_string());
    let signs = handle("decision:contract-ordinance-lands-signs");
    let stand = handle("decision:contract-ordinance-lands-stand");
    let latecomer = handle("decision:contract-ordinance-lands-latecomer");
    add(store, member(&o.collection, &signs, "Landings signs")).await;
    add(store, member(&o.collection, &stand, "Landings stand")).await;
    add(
        store,
        NewEntity::new(latecomer.clone(), "Landings latecomer", "contract-fixture"),
    )
    .await;
    claim_about(store, &latecomer, "latecomer is nine", &[("number", "9")]).await;
    // A member under a member: the whole subtree is the collection's scope.
    let wing = handle("decision:contract-ordinance-lands-wing");
    add(store, member(&o.fireworks, &wing, "Landings wing")).await;
    claim_about(store, &wing, "wing is six", &[("number", "6")]).await;
    let note = claim_about(
        store,
        &o.curfew,
        "curfew has a note",
        &[("note", "start at ten")],
    )
    .await;

    assert_eq!(next_number(store, &o.collection).await, 7);

    // An unnumbered member folded into a numbered one: 1 stays with curfew.
    merge(store, &signs, &o.curfew)
        .await
        .expect("an unnumbered member folds into a numbered one");
    let one = store
        .fields(&numbered(&o.collection, 1))
        .await
        .expect("1 still names a member");
    assert_eq!(one.get("title").map(String::as_str), Some("Curfew"));

    // A numbered member folded into an unnumbered one: the survivor serves 4.
    merge(store, &o.fireworks, &stand)
        .await
        .expect("a numbered member folds into an unnumbered one");
    let four = store
        .fields(&numbered(&o.collection, 4))
        .await
        .expect("4 still names a member");
    assert_eq!(four.get("title").map(String::as_str), Some("Fireworks"));
    assert_eq!(
        next_number(store, &o.collection).await,
        7,
        "no number was freed and none was added"
    );
    // The member's own child is still a member of the collection, served now
    // under the survivor.
    let six = store
        .fields(&numbered(&o.collection, 6))
        .await
        .expect("6 still names the child of the folded member");
    assert_eq!(six.get("number").map(String::as_str), Some("6"));

    // A thing outside the collection holding a number it does not hold moves in.
    reparent(store, &latecomer, &o.collection)
        .await
        .expect("a thing with a free number joins the collection");
    assert_eq!(next_number(store, &o.collection).await, 10);
    let nine = store
        .fields(&numbered(&o.collection, 9))
        .await
        .expect("9 names the new member");
    assert_eq!(nine.get("number").map(String::as_str), Some("9"));

    // A member moved within the collection keeps its number.
    reparent(store, &o.parking, &o.curfew)
        .await
        .expect("a member moves under another member");
    let two = store
        .fields(&numbered(&o.collection, 2))
        .await
        .expect("2 still names a member");
    assert_eq!(two.get("title").map(String::as_str), Some("Parking"));

    // A claim that does not set the number can be taken back.
    retract(store, &note.address())
        .await
        .expect("a claim that sets no number is retracted");
    assert_eq!(next_number(store, &o.collection).await, 10);
}

/// **Declaring the numbering key a counter is held to the numbers.** A counter
/// reads back as the total of its writes, so a declaration changes what every
/// member holds under the key without any member being written. A member that
/// confirmed its own number once holds it twice, and as a counter it holds the
/// sum: here the first entry's 1 written twice would total 2, the number the
/// second entry holds. The declaration is refused for the number two members
/// would share, and nothing is declared or changed. **Paired**: a counter on a
/// key the collection does not number by is declared.
pub async fn declaring_the_numbering_key_a_counter_is_held_to_the_numbers<M: Memory>(store: &M) {
    use crate::memory::types::{DeclaredType, Field};
    let book = EntityId("topic:contract-tally-book".into());
    let first = EntityId("decision:contract-entry-first".into());
    let second = EntityId("decision:contract-entry-second".into());
    add(
        store,
        NewEntity::new(book.clone(), "Tally book", "contract-fixture"),
    )
    .await;
    claim_about(
        store,
        &book,
        "numbers its entries",
        &[("numbers_by", "tally_slot")],
    )
    .await;
    for (id, name, slot) in [
        (&first, "First tally entry", "1"),
        (&second, "Second tally entry", "2"),
    ] {
        add(store, member(&book, id, name)).await;
        claim_about(store, id, &format!("slot {slot}"), &[("tally_slot", slot)]).await;
    }
    claim_about(store, &first, "slot 1 confirmed", &[("tally_slot", "1")]).await;
    let slot = |held: BTreeMap<String, String>| held.get("tally_slot").cloned();
    assert_eq!(
        slot(store.fields(&first).await.expect("fields read")),
        Some("1".to_string()),
        "an undeclared key takes its newest write"
    );

    let counter = DeclaredType::new("contract-tally-counter", vec![Field::summing("tally_slot")]);
    let refused = store
        .declare_type(counter, None)
        .await
        .expect_err("two entries would share a number");
    assert!(
        matches!(refused, MemoryError::NumberTaken { .. }),
        "refused for the number two members would hold: {refused:?}"
    );
    assert!(
        store
            .declared_types()
            .await
            .expect("the roster reads")
            .iter()
            .all(|declared| declared.name != "contract-tally-counter"),
        "a refused declaration declares nothing"
    );
    for (id, expected) in [(&first, "1"), (&second, "2")] {
        assert_eq!(
            slot(store.fields(id).await.expect("fields read")),
            Some(expected.to_string()),
            "{id} holds what it held"
        );
    }

    let unrelated = DeclaredType::new(
        "contract-tally-visits",
        vec![Field::summing("tally_visits")],
    );
    store
        .declare_type(unrelated, None)
        .await
        .expect("a counter on a key the collection does not number by is declared");
}

/// **Redeclaring a type without the numbering key's counter is held to the
/// numbers.** A counter holds the total of a key's writes and a key nothing
/// declares holds the newest, so a redeclaration that leaves the key out changes
/// what every member holds under it without any member being written. Here the
/// first entry wrote 1 and then 0 and holds 1 as a counter, the second wrote 2
/// and then 0 and holds 2: without the counter both hold 0. The redeclaration is refused for the number
/// two members would share, and the type still sums. **Paired**: a collection
/// whose newest writes and totals agree lets the same redeclaration land.
pub async fn redeclaring_the_numbering_key_without_its_counter_is_held_to_the_numbers<M: Memory>(
    store: &M,
) {
    use crate::memory::types::{DeclaredType, Field};
    let summed = |declared: &[DeclaredType], name: &str, key: &str| {
        declared
            .iter()
            .find(|held| held.name == name)
            .map(|held| {
                held.fields
                    .iter()
                    .any(|field| field.key == key && field.folds == crate::memory::types::Fold::Sum)
            })
            .unwrap_or(false)
    };
    let slot = |held: BTreeMap<String, String>, key: &str| held.get(key).cloned();

    let shelf = EntityId("topic:contract-ledger-shelf".into());
    let first = EntityId("decision:contract-ledger-first".into());
    let second = EntityId("decision:contract-ledger-second".into());
    store
        .declare_type(
            DeclaredType::new(
                "contract-ledger-counter",
                vec![Field::summing("ledger_slot")],
            ),
            None,
        )
        .await
        .expect("a counter is declared before any collection numbers by it");
    add(
        store,
        NewEntity::new(shelf.clone(), "Ledger shelf", "contract-fixture"),
    )
    .await;
    claim_about(
        store,
        &shelf,
        "numbers its entries",
        &[("numbers_by", "ledger_slot")],
    )
    .await;
    add(store, member(&shelf, &first, "First ledger entry")).await;
    add(store, member(&shelf, &second, "Second ledger entry")).await;
    claim_about(store, &first, "slot 1", &[("ledger_slot", "1")]).await;
    claim_about(store, &second, "slot 2", &[("ledger_slot", "2")]).await;
    for id in [&first, &second] {
        claim_about(store, id, "slot cleared", &[("ledger_slot", "0")]).await;
    }
    for (id, expected) in [(&first, "1"), (&second, "2")] {
        assert_eq!(
            slot(store.fields(id).await.expect("fields read"), "ledger_slot"),
            Some(expected.to_string()),
            "{id} holds the total of its writes as a counter"
        );
    }

    let dropped = DeclaredType::new(
        "contract-ledger-counter",
        vec![Field::new("ledger_visits", ValueType::Number)],
    );
    let refused = store
        .declare_type(dropped.clone(), None)
        .await
        .expect_err("a member would hold no number");
    assert!(
        matches!(
            &refused,
            MemoryError::NotANumber { collection, key, value }
                if collection == &shelf.0 && key == "ledger_slot" && value == "0"
        ),
        "refused for the number a member would hold: {refused:?}"
    );
    assert!(
        summed(
            &store.declared_types().await.expect("the roster reads"),
            "contract-ledger-counter",
            "ledger_slot"
        ),
        "a refused redeclaration leaves the counter declared"
    );
    for (id, expected) in [(&first, "1"), (&second, "2")] {
        assert_eq!(
            slot(store.fields(id).await.expect("fields read"), "ledger_slot"),
            Some(expected.to_string()),
            "{id} holds what it held"
        );
    }

    let rack = EntityId("topic:contract-ledger-rack".into());
    let third = EntityId("decision:contract-ledger-third".into());
    let fourth = EntityId("decision:contract-ledger-fourth".into());
    store
        .declare_type(
            DeclaredType::new(
                "contract-ledger-aisles",
                vec![Field::summing("ledger_aisle")],
            ),
            None,
        )
        .await
        .expect("a second counter is declared");
    add(
        store,
        NewEntity::new(rack.clone(), "Ledger rack", "contract-fixture"),
    )
    .await;
    claim_about(
        store,
        &rack,
        "numbers its entries",
        &[("numbers_by", "ledger_aisle")],
    )
    .await;
    for (id, name, aisle) in [
        (&third, "Third ledger entry", "1"),
        (&fourth, "Fourth ledger entry", "2"),
    ] {
        add(store, member(&rack, id, name)).await;
        claim_about(
            store,
            id,
            &format!("aisle {aisle}"),
            &[("ledger_aisle", aisle)],
        )
        .await;
    }
    store
        .declare_type(
            DeclaredType::new(
                "contract-ledger-aisles",
                vec![Field::new("ledger_visits", ValueType::Number)],
            ),
            None,
        )
        .await
        .expect("members whose totals and newest writes agree keep their numbers");
    assert!(
        !summed(
            &store.declared_types().await.expect("the roster reads"),
            "contract-ledger-aisles",
            "ledger_aisle"
        ),
        "the redeclaration landed"
    );
    for (id, expected) in [(&third, "1"), (&fourth, "2")] {
        assert_eq!(
            slot(store.fields(id).await.expect("fields read"), "ledger_aisle"),
            Some(expected.to_string()),
            "{id} holds the number it held"
        );
    }
}

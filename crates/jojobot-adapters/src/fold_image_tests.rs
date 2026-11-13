//! **The in-memory image (decision log 292), held to the store it mirrors.**
//!
//! The walk in [`crate::fold_battery`] writes every kind of write through the
//! fold and, after each, asks every read of the fold and of the store. These
//! cases run it over the fake; `tests/fold_image_dolt.rs` runs the same walk
//! over the real store.

use std::sync::Arc;

use jojobot_domain::memory::testing::InMemoryMemory;

use crate::counted::Counted;
use crate::fold::Folded;
use crate::fold_battery::{
    Probe, agree_after_a_reload, the_fold_answers_who_points_at_every_name_as_the_store_does,
    walk_a_merge_among_holders_of_one_name, walk_a_merge_and_a_rename_among_holders,
    walk_every_kind_of_write,
};

/// The reads served from memory once the fold has loaded. Each read is added
/// here in the commit that switches it.
const SWITCHED: &[&str] = &[
    "list_entities",
    "former_handles",
    "declared_types",
    "declared_kinds",
    "recall",
    "merge_events",
    "fields",
    "field_ages",
    "claim_histories",
    "claim_history",
    "scan_entity",
    "scan",
    "history",
    "keys_kept_off_the_thing",
    "key_trails",
    "backing",
    "built_on",
    "referring_to",
    "children",
    "thought_writers",
    "entity_history",
    "write_summary",
];

/// **The instrument first.** With nothing switched the walk compares a fold
/// that forwards everything with the store, so it can only pass; a failure here
/// is the battery disagreeing with itself and says nothing about the image.
#[tokio::test]
async fn the_battery_agrees_with_a_fold_that_serves_nothing_from_memory() {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let probe = Probe {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: &[],
        kept_off_misses_are_empty: false,
        last_signal: Default::default(),
    };
    walk_every_kind_of_write(&probe).await;
}

/// **After startup the switched reads make no call to the store, and answer
/// as the store does, after every kind of write.**
#[tokio::test]
async fn switched_reads_answer_from_memory_exactly_as_the_store_does() {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let probe = Probe {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: SWITCHED,
        kept_off_misses_are_empty: false,
        last_signal: Default::default(),
    };
    walk_every_kind_of_write(&probe).await;
}

/// **A boot declares the types the build ships, every time, and a store that
/// already holds them has nothing to fold again.** A declaration the image
/// already holds, word for word, changes how no thing folds, so it must not read
/// the whole store again: with the image full, six such declarations at boot read
/// it six times over and the server took minutes to start. Paired with the
/// positive: a declaration that differs still reads the image again and changes
/// what a counter key folds to.
#[tokio::test]
async fn declaring_a_type_the_image_already_holds_reads_nothing_again() {
    use jojobot_domain::memory::Memory;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    thing_holding(&folded, "thing:delta", "rank", "3").await;

    let ranked = DeclaredType::new(
        "fold-ranked",
        vec![Field::required("rank", ValueType::Number)],
    );
    folded
        .declare_type(ranked.clone(), None)
        .await
        .expect("declared");

    counted.reset();
    folded
        .declare_type(ranked, None)
        .await
        .expect("declared again");
    assert_eq!(
        counted.seen(),
        std::collections::BTreeMap::from([("declare_type", 1)]),
        "declaring what the image already holds reached the store for more than the declaration"
    );

    counted.reset();
    folded
        .declare_type(
            DeclaredType::new("fold-ranked", vec![Field::summing("rank")]),
            None,
        )
        .await
        .expect("declared differently");
    assert_eq!(
        counted.seen(),
        std::collections::BTreeMap::from([("declare_type", 1)]),
        "a declaration that differs reads no thing again: the State builds what a thing holds"
    );
    let delta = jojobot_domain::memory::EntityId("thing:delta".into());
    assert_eq!(
        folded.fields(&delta).await.expect("the fold reads"),
        store.fields(&delta).await.expect("the store reads"),
        "the fold serves what a thing holds under the new declaration as the store does"
    );
}

/// **A claim that sets one key on a thing, written through the fold.**
async fn thing_holding(folded: &Folded, handle: &str, key: &str, value: &str) {
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

    let thing = EntityId(handle.into());
    folded
        .add_entity(NewEntity::new(thing.clone(), "Delta Station", "fold-image"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    let mut fact = state(NewFact::about(
        thing,
        "holds a value",
        jiff::civil::date(2026, 9, 2),
    ));
    fact.session = Some("fold-image".into());
    fact.fields.insert(key.into(), value.into());
    fact.sets.insert(key.into());
    folded
        .capture(fact)
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
}

/// **A new type over a store holding things reads no thing again.** The fold
/// took the whole store back in for each new declaration, under the write gate,
/// and a store of some hundreds of things held every other write for minutes.
/// The count is of the store's reads: a declaration reaches it once, for its
/// own write. **The positive is in the same case**: the image answers with the
/// new type afterwards, and its list is the store's.
#[tokio::test]
async fn declaring_a_new_type_reads_the_store_only_for_the_declaration() {
    use jojobot_domain::memory::Memory;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    thing_holding(&folded, "thing:delta", "weight", "3").await;

    // Takes the shape of a type with a list of references and a narrowed key,
    // over keys no thing holds, and one over a key a thing holds that no
    // declaration changes the reading of.
    for declared in [
        DeclaredType::new(
            "fold-new-unheld",
            vec![
                Field::required("lane", ValueType::Text),
                Field::listing("members", ValueType::Reference),
            ],
        ),
        DeclaredType::new(
            "fold-new-held",
            vec![Field::required("weight", ValueType::Number)],
        ),
    ] {
        let name = declared.name.clone();
        counted.reset();
        folded.declare_type(declared, None).await.expect("declared");
        assert_eq!(
            counted.seen(),
            std::collections::BTreeMap::from([("declare_type", 1)]),
            "declaring {name} read the store for more than the declaration"
        );
        let held = folded.declared_types().await.expect("types read");
        assert!(held.iter().any(|d| d.name == name), "{name} is not held");
        assert_eq!(
            held,
            store.declared_types().await.expect("store types read"),
            "the image lists types other than the store's after {name}"
        );
    }
}

/// **A declaration that changes how a held key reads reads nothing again, and
/// the thing is served under it.** A counter changes what a thing holds, and a
/// reference changes how its value is served; the State builds both when a read
/// asks, so the store is reached for the declaration alone.
#[tokio::test]
async fn declaring_what_changes_how_a_held_key_reads_no_thing_again() {
    use jojobot_domain::memory::Memory;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    for (name, field) in [
        ("fold-sums-held", Field::summing("weight")),
        (
            "fold-points-held",
            Field::new("weight", ValueType::Reference),
        ),
    ] {
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store.clone()));
        let folded = Folded::new(counted.clone());
        folded.rebuild().await.expect("the fold loads");
        thing_holding(&folded, "thing:delta", "weight", "3").await;

        counted.reset();
        folded
            .declare_type(DeclaredType::new(name, vec![field]), None)
            .await
            .expect("declared");
        assert_eq!(
            counted.seen(),
            std::collections::BTreeMap::from([("declare_type", 1)]),
            "{name} changes how a held key reads and a thing was read again"
        );
        let delta = jojobot_domain::memory::EntityId("thing:delta".into());
        assert_eq!(
            folded.fields(&delta).await.expect("the fold reads"),
            store.fields(&delta).await.expect("the store reads"),
            "{name}: the fold serves the thing as the store does"
        );
    }
}

/// **Re-declaring a held key as a list reads no thing again.** The store splits
/// the value of a list of references per item; the State builds the value under
/// the declaration in force when a read asks, so the store is reached for the
/// declaration alone, for a reference and for a text key alike.
#[tokio::test]
async fn re_declaring_a_held_key_as_a_list_reads_no_thing_again() {
    use jojobot_domain::memory::Memory;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    for (name, first, second) in [
        (
            "list-points-held",
            Field::new("weight", ValueType::Reference),
            Field::listing("weight", ValueType::Reference),
        ),
        (
            "list-text-held",
            Field::new("weight", ValueType::Text),
            Field::listing("weight", ValueType::Text),
        ),
    ] {
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store.clone()));
        let folded = Folded::new(counted.clone());
        folded.rebuild().await.expect("the fold loads");
        thing_holding(&folded, "thing:delta", "weight", "3").await;
        folded
            .declare_type(DeclaredType::new(name, vec![first]), None)
            .await
            .expect("declared first");

        counted.reset();
        folded
            .declare_type(DeclaredType::new(name, vec![second]), None)
            .await
            .expect("declared again");
        assert_eq!(
            counted.seen(),
            std::collections::BTreeMap::from([("declare_type", 1)]),
            "{name}: a thing was read again after the key became a list"
        );
        let delta = jojobot_domain::memory::EntityId("thing:delta".into());
        assert_eq!(
            folded.fields(&delta).await.expect("the fold reads"),
            store.fields(&delta).await.expect("the store reads"),
            "{name}: the fold serves the thing as the store does"
        );
    }
}

/// **A read that only looks at the declared types is handed the image's own
/// list, and copies nothing.** Two reads of one image get the same list; a
/// declaration makes a new image, which holds a new list, and a reader that
/// still holds the earlier one keeps reading what it was given.
#[tokio::test]
async fn the_declared_types_are_shared_by_every_read_of_an_image() {
    use jojobot_domain::memory::Memory;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");

    counted.reset();
    let first = folded.declared_types_shared().await.expect("types read");
    let second = folded.declared_types_shared().await.expect("types read");
    assert!(
        Arc::ptr_eq(&first, &second),
        "two reads of one image copied the list"
    );
    assert_eq!(
        counted.calls(),
        0,
        "the store was reached: {:?}",
        counted.seen()
    );
    assert_eq!(
        *first,
        folded.declared_types().await.expect("types read"),
        "the shared list is the list"
    );

    let ranked = DeclaredType::new(
        "fold-shared",
        vec![Field::required("rank", ValueType::Number)],
    );
    folded.declare_type(ranked, None).await.expect("declared");
    let later = folded.declared_types_shared().await.expect("types read");
    assert!(
        later.iter().any(|declared| declared.name == "fold-shared"),
        "a declaration is not in the next image's list"
    );
    assert!(
        !first.iter().any(|declared| declared.name == "fold-shared"),
        "a reader's list moved under it"
    );
}

/// **A merge and a rename with every kind of holder around them leave the image
/// as the store answers.** The child, the edge, the field, the ref and the claim
/// worked out from the folded thing's own claim are all read again.
#[tokio::test]
async fn a_merge_and_a_rename_among_holders_leave_the_image_as_the_store_answers() {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let probe = Probe {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: SWITCHED,
        kept_off_misses_are_empty: false,
        last_signal: Default::default(),
    };
    walk_a_merge_and_a_rename_among_holders(&probe).await;
    walk_a_merge_among_holders_of_one_name(&probe).await;
    let fresh = Folded::new(counted.clone());
    fresh.rebuild().await.expect("the fold loads again");
    agree_after_a_reload(&fresh, &*store).await;
    // Every name an engine record or an ordinary one points at answers as the store does.
    the_fold_answers_who_points_at_every_name_as_the_store_does(&fresh, &*store).await;
}

/// **A kind declared again as it already stands refreshes the kinds no more.** The
/// seed declares every shipped kind on every start, so a fold that read the kinds
/// again for each declaration paid for each of them. The store says
/// whether a declaration changed anything, and the fold reads the kinds again
/// only then, and never a thing: `declared_kinds` is the read a refresh makes once,
/// and `scan` is the read of every thing, which no kind write makes.
///
/// **The positive is in the same case**: the first seed over a store that holds no
/// kind changes every declaration, so it fills the image, at most once for each;
/// and a key a caller adds to a shipped kind changes the store, so that fills it
/// again, where the same declaration repeated does not.
#[tokio::test]
async fn declaring_a_kind_that_changes_nothing_refreshes_the_kinds_no_more() {
    use jojobot_domain::memory::Memory;
    use jojobot_domain::memory::kinds;
    use jojobot_domain::memory::types::{Field, Origin, ValueType};

    let store = Arc::new(InMemoryMemory::new());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    // The kinds are read again by `declared_kinds`; no write reads a thing again.
    let fills = |counted: &Counted| {
        let seen = counted.seen();
        assert_eq!(
            seen.get("scan"),
            None,
            "a kind write read every thing: {seen:?}"
        );
        seen.get("declared_kinds").copied().unwrap_or(0)
    };

    counted.reset();
    kinds::seed(&folded).await.expect("the kinds are seeded");
    let first = fills(&counted);
    assert!(
        (1..=kinds::SHIPPED.len()).contains(&first),
        "the first seed changes each kind, so it fills the image, once for each at most: {first}"
    );

    counted.reset();
    kinds::seed(&folded)
        .await
        .expect("the kinds are seeded again");
    assert_eq!(
        fills(&counted),
        0,
        "a seed that changes nothing filled the image: {:?}",
        counted.seen()
    );

    let addition = || vec![Field::new("packing_note", ValueType::Text)];
    counted.reset();
    folded
        .declare_kind("trip", Origin::Declared, addition())
        .await
        .expect("a caller adds a key to a shipped kind");
    assert_eq!(
        fills(&counted),
        1,
        "a change fills the image once: {:?}",
        counted.seen()
    );
    counted.reset();
    folded
        .declare_kind("trip", Origin::Declared, addition())
        .await
        .expect("the same key again");
    assert_eq!(
        fills(&counted),
        0,
        "the same declaration again filled it: {:?}",
        counted.seen()
    );
}

/// A name no other fixture wears, so a case with hundreds of things meets neither a
/// duplicate name nor the near-miss screen.
fn spelled(n: usize) -> String {
    let mut state = (n as u64 + 1).wrapping_mul(6364136223846793005);
    (0..12)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (b'a' + ((state >> 33) % 26) as u8) as char
        })
        .collect()
}

/// **A scan that lists many things the State declines reads the store once.** Every
/// child of a merged parent declines (the store appends a version of such a child for
/// the move), so a boot scan over a store whose merged parent holds thousands of
/// children made one read of the store for each: minutes, where the image it replaced
/// made none. The declined documents come out of one whole read, in the listing's
/// order, and the thing the State serves is not read again.
#[tokio::test]
async fn a_scan_listing_many_declined_things_reads_the_store_once() {
    use jojobot_domain::memory::{EntityId, Memory, NewEntity};

    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let handle = |kind: &str, n: usize| EntityId(format!("{kind}:{}", spelled(n)));
    let (survivor, merged) = (handle("project", 1), handle("project", 2));
    for parent in [&survivor, &merged] {
        folded
            .add_entity(NewEntity::new(parent.clone(), &parent.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    for n in 10..60 {
        let child = handle("work", n);
        let mut entity = NewEntity::new(child.clone(), &child.0, "fold-image");
        entity.parent = Some(merged.clone());
        folded
            .add_entity(entity)
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let held = handle("project", 3);
    folded
        .add_entity(NewEntity::new(held.clone(), &held.0, "fold-image"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    folded
        .merge(
            &merged,
            &survivor,
            Some("filed twice"),
            jiff::civil::date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("merged");

    counted.reset();
    let scanned = folded.scan().await.expect("the fold scans");
    let seen = counted.seen();
    assert_eq!(seen.get("scan_entity"), None, "{seen:?}");
    assert_eq!(seen.get("scan"), Some(&1), "{seen:?}");
    let listed = folded.list_entities(None).await.expect("lists");
    let in_scan: Vec<String> = scanned
        .iter()
        .filter_map(|doc| doc.entity.as_ref().map(|e| e.id.0.clone()))
        .collect();
    let in_list: Vec<String> = listed.iter().map(|e| e.id.0.clone()).collect();
    assert_eq!(
        in_scan, in_list,
        "the scan lists what the listing lists, in order"
    );
}

/// **A scan that lists a few things the State declines reads each of them on its
/// own, and reads no thing the State serves.** A whole read of the store costs a row
/// for every thing it holds, so it is worth making only when nearly every listed
/// thing is declined (see the case above).
#[tokio::test]
async fn a_scan_listing_a_few_declined_things_reads_only_those_things() {
    use jojobot_domain::memory::{EntityId, Memory, NewEntity};

    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let handle = |kind: &str, n: usize| EntityId(format!("{kind}:{}", spelled(n)));
    for n in 100..140 {
        let plain = handle("project", n);
        folded
            .add_entity(NewEntity::new(plain.clone(), &plain.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let (survivor, merged) = (handle("project", 1), handle("project", 2));
    for parent in [&survivor, &merged] {
        folded
            .add_entity(NewEntity::new(parent.clone(), &parent.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    folded
        .merge(
            &merged,
            &survivor,
            Some("filed twice"),
            jiff::civil::date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("merged");

    counted.reset();
    let scanned = folded.scan().await.expect("the fold scans");
    let seen = counted.seen();
    assert_eq!(seen.get("scan"), None, "{seen:?}");
    let read = seen.get("scan_entity").copied().unwrap_or(0);
    assert!(
        (1..=3).contains(&read),
        "only the merged things are read from the store: {seen:?}"
    );
    assert!(scanned.len() > 40, "the scan lists the rest from the State");
}

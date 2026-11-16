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
    let (store, counted, folded) = a_loaded_fold().await;
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
    let (store, counted, folded) = a_loaded_fold().await;
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

    let (store, counted, folded) = a_loaded_fold().await;
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

/// **A fold over a booted, empty store, loaded**, with the counter between them that says what
/// each call reached the store for.
async fn a_loaded_fold() -> (Arc<InMemoryMemory>, Arc<Counted>, Folded) {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    (store, counted, folded)
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

    let (store, counted, folded) = a_loaded_fold().await;
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
/// asks, so the store is reached for the declaration alone. **Re-declaring a held
/// key as a list reads no thing again either.** The store splits the value of a
/// list of references per item; the State builds the value under the declaration
/// in force when a read asks, so the store is reached for the declaration alone,
/// for a reference and for a text key alike.
#[tokio::test]
async fn declaring_what_changes_how_a_held_key_reads_no_thing_again() {
    use jojobot_domain::memory::Memory;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    // (name, the declaration the key is held under first, the declaration that changes it)
    let rows: [(&str, Option<Field>, Field); 4] = [
        ("fold-sums-held", None, Field::summing("weight")),
        (
            "fold-points-held",
            None,
            Field::new("weight", ValueType::Reference),
        ),
        (
            "list-points-held",
            Some(Field::new("weight", ValueType::Reference)),
            Field::listing("weight", ValueType::Reference),
        ),
        (
            "list-text-held",
            Some(Field::new("weight", ValueType::Text)),
            Field::listing("weight", ValueType::Text),
        ),
    ];
    for (name, first, second) in rows {
        let (store, counted, folded) = a_loaded_fold().await;
        thing_holding(&folded, "thing:delta", "weight", "3").await;
        if let Some(first) = first {
            folded
                .declare_type(DeclaredType::new(name, vec![first]), None)
                .await
                .expect("declared first");
        }

        counted.reset();
        folded
            .declare_type(DeclaredType::new(name, vec![second]), None)
            .await
            .expect("declared");
        assert_eq!(
            counted.seen(),
            std::collections::BTreeMap::from([("declare_type", 1)]),
            "{name}: a thing was read again after the declaration"
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

    let (_, counted, folded) = a_loaded_fold().await;

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
    let (store, counted, folded) = a_loaded_fold().await;
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

    let (_, counted, folded) = a_loaded_fold().await;
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

    let (_, counted, folded) = a_loaded_fold().await;
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

/// **The store, but for a whole read that fails**, as one unreadable row anywhere in
/// it fails the read that builds every document.
struct WholeScanFails(Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl jojobot_domain::memory::Memory for WholeScanFails {
    async fn scan(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::search::DocScan>, jojobot_domain::memory::MemoryError>
    {
        Err(jojobot_domain::memory::MemoryError::Store(
            "a row cannot be read".into(),
        ))
    }
}

/// **A merged parent with `children` children and `served` plain things beside
/// it**: `children + 2` things the State declines, `served` it serves.
async fn declined_and_served(
    store: Arc<dyn jojobot_domain::memory::Memory>,
    children: usize,
    served: usize,
) -> Folded {
    use jojobot_domain::memory::{EntityId, Memory, NewEntity};

    let folded = Folded::new(store);
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
    for n in 0..children {
        let child = handle("work", 10 + n);
        let mut entity = NewEntity::new(child.clone(), &child.0, "fold-image");
        entity.parent = Some(merged.clone());
        folded
            .add_entity(entity)
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    for n in 0..served {
        let plain = handle("project", 100 + n);
        folded
            .add_entity(NewEntity::new(plain.clone(), &plain.0, "fold-image"))
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
    folded
}

/// **The line between reading the whole store and reading each declined thing is
/// five declined in six listed.** Six declined and one served is over it, five and
/// one is not.
#[tokio::test]
async fn a_scan_reads_the_whole_store_past_five_in_six_declined_and_not_at_five() {
    use jojobot_domain::memory::Memory;
    for (children, served, whole) in [(4, 1, true), (3, 1, false)] {
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store));
        let folded = declined_and_served(counted.clone(), children, served).await;
        counted.reset();
        folded.scan().await.expect("the fold scans");
        let seen = counted.seen();
        match whole {
            true => {
                assert_eq!(seen.get("scan"), Some(&1), "{children} declined: {seen:?}");
                assert_eq!(seen.get("scan_entity"), None, "{seen:?}");
            }
            false => {
                assert_eq!(seen.get("scan"), None, "{children} declined: {seen:?}");
                assert_eq!(seen.get("scan_entity"), Some(&(children + 2)), "{seen:?}");
            }
        }
    }
}

/// **A whole read that fails does not fail the scan.** One unreadable row anywhere
/// in the store fails the read that builds every document; the scan then reads the
/// declined things on its own and lists all of them.
#[tokio::test]
async fn a_scan_whose_whole_read_fails_reads_the_declined_things_on_their_own() {
    use jojobot_domain::memory::Memory;
    let store = Arc::new(InMemoryMemory::booted());
    let folded = declined_and_served(Arc::new(WholeScanFails(store)), 4, 1).await;
    let scanned = folded
        .scan()
        .await
        .expect("the declined things are read one by one");
    assert_eq!(scanned.len(), 7, "every thing is listed");
}

/// **The history of a claim that holds no mention reads no roster.** The roster is
/// every thing the store holds, listed for each call, and the search index asks the
/// history of every thing it scans: a boot over thousands of things listed them all
/// thousands of times. Text with no at sign has nothing to render, so the history of
/// such a claim comes back as the store gave it; a claim with a mention still reads
/// the roster, which is the pair.
#[tokio::test]
async fn the_history_of_a_claim_with_no_mention_reads_no_roster() {
    use jojobot_domain::memory::mention::Mentioning;
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let layer = Mentioning::new(counted.clone());
    let handle = |n: usize| EntityId(format!("{}:{}", "person", spelled(n)));
    let (plain, mentioning, other) = (handle(1), handle(2), handle(3));
    let in_details = handle(4);
    for id in [&plain, &mentioning, &other, &in_details] {
        layer
            .add_entity(NewEntity::new(id.clone(), &id.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let claim = |about: &EntityId, words: String| {
        let mut fact = state(NewFact::about(
            about.clone(),
            words,
            jiff::civil::date(2026, 9, 2),
        ));
        fact.session = Some("fold-image".into());
        fact
    };
    let mut written = Vec::new();
    for (about, words, details) in [
        (&plain, "plays the saxophone".to_string(), None),
        (&mentioning, format!("saw @{}", other.0), None),
        (
            &in_details,
            "keeps bees".to_string(),
            Some(format!("met @{}", other.0)),
        ),
    ] {
        let mut fact = claim(about, words);
        fact.details = details;
        written.push(
            layer
                .capture(fact)
                .await
                .expect("captured")
                .written()
                .expect("not blocked"),
        );
    }

    counted.reset();
    let chains = layer.claim_histories(&plain).await.expect("histories");
    let seen = counted.seen();
    assert_eq!(
        seen.get("list_entities"),
        None,
        "a claim with no mention read the roster: {seen:?}"
    );
    // The chain is what the store gave: the claim, its one write, words as written.
    assert_eq!(chains.len(), 1, "{chains:?}");
    let write = &chains[&written[0].id][0];
    assert_eq!(write.content, "plays the saxophone");

    for (about, fact) in [(&mentioning, &written[1]), (&in_details, &written[2])] {
        // A layer of its own holds no roster yet, so the read below is the one that
        // builds it.
        let fresh = Mentioning::new(counted.clone());
        counted.reset();
        let chains = fresh.claim_histories(about).await.expect("histories");
        let seen = counted.seen();
        assert!(
            seen.contains_key("list_entities"),
            "a claim with a mention reads the roster: {seen:?}"
        );
        let rendered = &chains[&fact.id][0];
        let text = format!(
            "{} {}",
            rendered.content,
            rendered.details.clone().unwrap_or_default()
        );
        assert!(
            text.contains(&format!("@{}", other.0)),
            "the mention reads as its handle: {text}"
        );
    }
}

/// **A holder query of a role's lease moment reads the role from the store.** A plain
/// renewal overwrites the `claimed_at` write and keeps the moment in the heartbeat
/// home, with no event, so the State's held value can be the one before. Every other
/// key a role holds is an event's, and so is every key of a thing a merge touched, so
/// the State answers for those: the roster that every write and every mention-bearing
/// read builds asks who holds a key, and a store read for each declined thing made
/// each of them cost seconds.
#[tokio::test]
async fn a_holder_query_reads_the_declined_things_from_the_store_for_the_lease_moment_alone() {
    use jojobot_domain::memory::filter::Test;
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let handle = |kind: &str, n: usize| EntityId(format!("{kind}:{}", spelled(n)));
    let (role, survivor, folded_away, plain) = (
        EntityId("role:kappa".into()),
        handle("person", 1),
        handle("person", 2),
        handle("person", 3),
    );
    for id in [&role, &survivor, &folded_away, &plain] {
        folded
            .add_entity(NewEntity::new(id.clone(), &id.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    for id in [&role, &survivor, &plain] {
        let mut fact = state(NewFact::about(
            id.clone(),
            "holds an agent",
            jiff::civil::date(2026, 9, 2),
        ));
        fact.session = Some("fold-image".into());
        fact.fields.insert("agent".into(), "a1".into());
        fact.sets.insert("agent".into());
        folded
            .capture(fact)
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
    }
    folded
        .merge(
            &folded_away,
            &survivor,
            Some("filed twice"),
            jiff::civil::date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("merged");
    let test = Test::NotEquals(String::new());

    let held = folded
        .keys_holding("agent", &test, None)
        .await
        .expect("who holds");
    for id in [&role, &survivor, &plain] {
        assert!(held.contains(id), "{id:?} holds: {held:?}");
    }

    // The lease moment of a role, and of a thing a merge touched (a role may have been
    // folded into it), is the store's.
    for declined in [&role, &survivor] {
        counted.fail_fields_of(Some(declined.as_str()));
        let refused = folded
            .keys_holding(jojobot_domain::session::ROLE_CLAIMED_AT, &test, None)
            .await;
        assert!(
            refused.is_err(),
            "{declined:?} lease moment was answered from the State: {refused:?}"
        );
    }
    // Every other key, of the role and of a thing a merge touched, is the State's.
    for fault in [&role, &survivor, &plain] {
        counted.fail_fields_of(Some(fault.as_str()));
        let held = folded
            .keys_holding("agent", &test, None)
            .await
            .expect("an event's key is not read from the store");
        for id in [&role, &survivor, &plain] {
            assert!(held.contains(id), "{id:?} holds: {held:?}");
        }
    }
    counted.fail_fields_of(Some(plain.as_str()));
    folded
        .keys_holding(jojobot_domain::session::ROLE_CLAIMED_AT, &test, None)
        .await
        .expect("a thing the State serves is not read from the store");
}

//! **The layer that supplies what the build ships sits over the fold, and the
//! fold knows nothing of it.** The stack is the store, the fold, then the supply
//! layer. The fold answers the stored half at every handle, a supplied one
//! included, from the claims State; the supply layer lays the shipped half on
//! top. The store beneath is counted by method name, so a case names the read
//! that reached it.

use super::*;
use crate::counted::Counted;
use crate::provisioned::Provisioned;
use jojobot_domain::memory::owned::{Provision, Provisions};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{Entity, EntityKind, EntityPatch};

fn loops() -> EntityId {
    EntityId("view:loops".into())
}

/// The record the build ships at `view:loops`, with a default for one key.
fn shipped() -> Provisions {
    let record = Provision::record(
        Entity {
            folded_names: Vec::new(),
            id: loops(),
            kind: EntityKind::VIEW,
            name: "The shipped board".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        },
        std::collections::BTreeMap::from([("selects".to_string(), "work".to_string())]),
    );
    Provisions::new(vec![record])
}

/// The served stack over a counted fake: the fake told what the build supplies,
/// the fold over it, the supply layer over the fold.
async fn the_stack(
    fake: InMemoryMemory,
) -> (Provisioned<Folded>, Arc<Counted>, Arc<InMemoryMemory>) {
    let store = Arc::new(fake.knowing(shipped()));
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold fills");
    (Provisioned::new(folded, shipped()), counted, store)
}

fn named(rows: &[Entity], id: &EntityId) -> Vec<String> {
    rows.iter()
        .filter(|row| row.id == *id)
        .map(|row| row.name.clone())
        .collect()
}

/// **A supplied record the operator named lists and scans with the operator's
/// name, and no store is read.** The State lists the record as the build ships it;
/// the fold leaves it out of the listing it answers, and the supply layer adds it
/// once, with the operator's name over it.
#[tokio::test]
async fn a_supplied_record_the_operator_named_lists_and_scans_by_that_name_with_no_store_read() {
    let (stack, counted, _store) = the_stack(InMemoryMemory::booted()).await;
    stack
        .update_entity(
            &loops(),
            EntityPatch {
                name: Some("Weekly rounds".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the operator names the record")
        .written()
        .expect("not blocked");
    // **A write about another thing lists everything again**, and the listing the
    // fold reads from the State holds the record as the build ships it.
    stack
        .add_entity(NewEntity::new(
            EntityId::person("person:alpha"),
            "Alpha",
            "user-named",
        ))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    counted.reset();
    let listed = stack.list_entities(None).await.expect("lists");
    assert_eq!(named(&listed, &loops()), vec!["Weekly rounds".to_string()]);
    let scanned = stack.scan().await.expect("scans");
    let scanned_names: Vec<String> = scanned
        .iter()
        .filter_map(|doc| doc.entity.as_ref())
        .filter(|entity| entity.id == loops())
        .map(|entity| entity.name.clone())
        .collect();
    assert_eq!(scanned_names, vec!["Weekly rounds".to_string()]);
    assert_eq!(
        counted.calls(),
        0,
        "the store was read: {:?}",
        counted.seen()
    );
}

/// **A per-thing read of a supplied record reads no store.** Its fields come
/// without the overlay's keys and with the shipped default under them, and its
/// claims come from the State.
#[tokio::test]
async fn a_per_thing_read_of_a_supplied_record_reads_no_store() {
    let (stack, counted, _store) = the_stack(InMemoryMemory::booted()).await;
    stack
        .capture(NewFact::about(
            loops(),
            "is pinned",
            jiff::civil::date(2026, 9, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    counted.reset();
    let fields = stack.fields(&loops()).await.expect("fields");
    assert_eq!(fields.get("selects").map(String::as_str), Some("work"));
    assert!(
        fields.keys().all(|key| !key.starts_with("overlay")),
        "{fields:?}"
    );
    let claims = stack.recall(&loops()).await.expect("recall");
    let direct = _store.recall(&loops()).await.expect("direct recall");
    assert!(
        claims.iter().any(|fact| fact.content == "is pinned"),
        "fold {claims:?} direct {direct:?}"
    );
    assert_eq!(
        counted.calls(),
        0,
        "the store was read: {:?}",
        counted.seen()
    );
}

/// **A stored row at a supplied handle lists as the stored row** (rule 508): the
/// operator's own row answers there, once, and what the build ships stays under
/// the keys the row holds.
#[tokio::test]
async fn a_stored_row_at_a_supplied_handle_lists_as_the_stored_row() {
    let fake = InMemoryMemory::booted();
    fake.add_entity(NewEntity::new(loops(), "My rounds", "user-named"))
        .await
        .expect("the row is stored before the build supplies the handle")
        .written()
        .expect("not blocked");
    let (stack, _counted, _store) = the_stack(fake).await;
    let listed = stack.list_entities(None).await.expect("lists");
    assert_eq!(named(&listed, &loops()), vec!["My rounds".to_string()]);
}

/// **A sets-only edit of a supplied record lands through the stack**, as the verb
/// writes it: the supply layer answers the edit with the companion claim the store
/// takes, the fold carries it past the store and reads it back, and the record stays
/// supplied, with no row of its own.
#[tokio::test]
async fn a_sets_only_edit_of_a_supplied_record_lands_through_the_fold_and_reads_back() {
    use jojobot_domain::actor::Actor;
    use jojobot_domain::log::claims::job;
    let (stack, counted, _store) = the_stack(InMemoryMemory::booted()).await;
    let claim = NewFact {
        sets: ["note_9".to_string()].into(),
        fields: jojobot_domain::memory::review::settings_fields(
            [("note_9".to_string(), "kept on top".to_string())].into(),
        ),
        ..NewFact::about(
            loops(),
            "Set note_9 on the shipped view.",
            jiff::civil::date(2026, 11, 9),
        )
    };
    job::within(
        job::test_job(&Actor::Operator),
        stack.update_entity_with_claim(&loops(), EntityPatch::default(), claim),
    )
    .await
    .expect("the edit is a write the gate allows")
    .written()
    .expect("nothing blocks it");
    counted.reset();
    let fields = stack.fields(&loops()).await.expect("fields");
    assert_eq!(
        fields.get("note_9").map(String::as_str),
        Some("kept on top")
    );
    assert_eq!(
        fields.get("selects").map(String::as_str),
        Some("work"),
        "what the build ships is still there under the edit: {fields:?}"
    );
    assert_eq!(
        counted.calls(),
        0,
        "the store was read: {:?}",
        counted.seen()
    );
}

/// **The document of a supplied record is the one the supply layer makes of it,
/// after an edit as before it.** The State holds the record's stored half under its
/// supply identity, and a claim on the record gives that half a document of its own;
/// the supply layer still answers a scan and a one-thing scan with the shipped
/// fields under what the operator wrote, once each.
#[tokio::test]
async fn a_scan_of_an_edited_supplied_record_carries_the_shipped_fields_once() {
    let (stack, _counted, _store) = the_stack(InMemoryMemory::booted()).await;
    stack
        .capture(NewFact::about(
            loops(),
            "is pinned",
            jiff::civil::date(2026, 9, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    let one = stack
        .scan_entity(&loops())
        .await
        .expect("scan_entity")
        .expect("the record has a document");
    assert_eq!(
        one.fields.get("selects").map(String::as_str),
        Some("work"),
        "{one:?}"
    );
    assert!(one.facts.iter().any(|fact| fact.content == "is pinned"));
    let all = stack.scan().await.expect("scan");
    let ours: Vec<_> = all
        .iter()
        .filter(|doc| doc.entity.as_ref().is_some_and(|e| e.id == loops()))
        .collect();
    assert_eq!(ours.len(), 1, "one document for the record: {ours:?}");
    assert_eq!(
        ours[0].fields.get("selects").map(String::as_str),
        Some("work")
    );
}

/// **A search by the operator's name or alias finds a supplied record**, through the
/// whole stack: the fold, the supply layer, mentions and the index over them.
#[tokio::test]
async fn a_search_by_the_operators_alias_finds_a_supplied_record_through_the_stack() {
    use crate::search::{IndexedMemory, Retrieval};
    use jojobot_domain::memory::mention::Mentioning;
    use jojobot_domain::memory::search::{Hit, Search, SearchQuery};
    let store = Arc::new(InMemoryMemory::booted().knowing(shipped()));
    let folded = Arc::new(Folded::new(store.clone()));
    folded.rebuild().await.expect("the fold fills");
    let provisioned = Arc::new(Provisioned::new(SharedFold(folded.clone()), shipped()));
    let indexed = Arc::new(
        IndexedMemory::with_writer(Arc::new(Mentioning::new(provisioned)), folded.writer())
            .expect("the index opens"),
    );
    let retrieval = Retrieval::new(indexed.index(), vec![indexed.clone()]);
    indexed
        .update_entity(
            &loops(),
            EntityPatch {
                name: Some("Weekly rounds".into()),
                aliases: Some(vec!["rounds".into()]),
                ..Default::default()
            },
        )
        .await
        .expect("the operator names the record")
        .written()
        .expect("not blocked");
    for word in ["Weekly", "rounds"] {
        let hits = retrieval
            .search(&SearchQuery::text(word))
            .await
            .expect("search ok");
        assert!(
            hits.iter()
                .any(|hit| matches!(hit, Hit::Entity { entity, .. } if entity.id == loops())),
            "a search for {word} finds the record: {hits:?}"
        );
    }
}

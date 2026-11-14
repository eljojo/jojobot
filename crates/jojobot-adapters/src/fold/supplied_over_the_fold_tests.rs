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

/// A counter the supply layer can own while the case keeps a handle on its counts.
struct SharedCounted(Arc<Counted>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl jojobot_domain::memory::Memory for SharedCounted {}

/// **A stack over the fold with the supply layer's reads counted**: the fake, the
/// fold, a counter, then the supply layer. The counter sees what the supply layer
/// asks the fold.
async fn the_stack_counted_above_the_fold(
    provisions: Provisions,
) -> (Provisioned<SharedCounted>, Arc<Counted>) {
    let store = Arc::new(InMemoryMemory::booted().knowing(provisions.clone()));
    let folded = Folded::new(store);
    folded.rebuild().await.expect("the fold fills");
    let counted = Arc::new(Counted::new(Arc::new(SharedFold(Arc::new(folded)))));
    (
        Provisioned::new(SharedCounted(counted.clone()), provisions),
        counted,
    )
}

fn spelled_thing(n: usize) -> EntityId {
    let mut state = (n as u64 + 1).wrapping_mul(6364136223846793005);
    let slug: String = (0..12)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (b'a' + ((state >> 33) % 26) as u8) as char
        })
        .collect();
    EntityId(format!("{}:{}", "thing", slug))
}

/// **Who holds a key is asked of the fold and not read thing by thing.** The default
/// answer lists every thing and reads the fields of each, and the roster every
/// write and every mention-bearing read builds asks it: over thousands of things
/// that is the cost of each such call. The supply layer forwards the question to the
/// fold and corrects only the handles the build supplies at.
#[tokio::test]
async fn who_holds_a_key_is_asked_of_the_fold_and_not_read_thing_by_thing() {
    let (stack, counted) = the_stack_counted_above_the_fold(shipped()).await;
    for n in 0..40 {
        let id = spelled_thing(n);
        stack
            .add_entity(jojobot_domain::memory::NewEntity::new(
                id.clone(),
                &id.0,
                "fold-image",
            ))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    counted.reset();
    stack
        .keys_holding(
            "selects",
            &jojobot_domain::memory::filter::Test::NotEquals(String::new()),
            None,
        )
        .await
        .expect("who holds");
    let seen = counted.seen();
    assert_eq!(seen.get("keys_holding"), Some(&1), "{seen:?}");
    assert!(
        seen.get("fields").copied().unwrap_or(0) <= 2,
        "the fields of the things were read one by one: {seen:?}"
    );
}

/// **The answer is the one the full read gives, supplied handles included.** A
/// record the build ships whole holds its shipped keys; a stored thing holds the
/// build's default for a key it wrote nothing under, and its own value where it
/// wrote one. The forward must say the same as reading every thing through the
/// layer, case by case.
#[tokio::test]
async fn who_holds_a_key_through_the_supply_layer_is_who_the_full_read_finds() {
    use jojobot_domain::memory::filter::Test;
    use jojobot_domain::memory::holders::holding_by_reading;
    use jojobot_domain::memory::{NewEntity, NewFact};

    let (without_default, with_own, with_default, bare) = (
        spelled_thing(1),
        spelled_thing(2),
        spelled_thing(3),
        spelled_thing(4),
    );
    let mut supplied = shipped();
    supplied.extend(vec![
        Provision::field(with_default.clone(), "rank", "5"),
        Provision::field(with_own.clone(), "rank", "5"),
    ]);
    let (stack, _) = the_stack_counted_above_the_fold(supplied).await;
    // A role and a pair a merge joins: things the fold declines and reads from the store.
    let role = EntityId("role:kappa".into());
    let (survivor, folded_away) = (spelled_thing(5), spelled_thing(6));
    for id in [
        &without_default,
        &with_own,
        &with_default,
        &bare,
        &role,
        &survivor,
        &folded_away,
    ] {
        stack
            .add_entity(NewEntity::new(id.clone(), &id.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    stack
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
    for (id, value) in [
        (&without_default, "5"),
        (&with_own, "9"),
        (&role, "5"),
        (&survivor, "9"),
    ] {
        let mut fact = jojobot_domain::memory::testing::contract::state(NewFact::about(
            id.clone(),
            "holds a rank",
            jiff::civil::date(2026, 9, 2),
        ));
        fact.session = Some("fold-image".into());
        fact.fields.insert("rank".into(), value.into());
        fact.sets.insert("rank".into());
        stack
            .capture(fact)
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
    }
    for (key, test) in [
        ("rank", Test::NotEquals(String::new())),
        ("rank", Test::Equals("5".into())),
        ("rank", Test::Equals("9".into())),
        ("rank", Test::Exists),
        ("selects", Test::NotEquals(String::new())),
        ("selects", Test::Equals("work".into())),
        ("name", Test::Exists),
    ] {
        for kind in [None, Some(EntityKind::ROLE)] {
            let forwarded = stack
                .keys_holding(key, &test, kind)
                .await
                .expect("who holds");
            let read = holding_by_reading(&stack, key, &test, kind)
                .await
                .expect("the full read");
            assert_eq!(forwarded, read, "{key} {test:?} {kind:?}");
        }
    }
    let rank = stack
        .keys_holding("rank", &Test::Exists, None)
        .await
        .expect("who holds");
    assert!(rank.contains(&with_default), "the default holds: {rank:?}");
    assert!(rank.contains(&with_own), "its own value holds: {rank:?}");
    assert!(
        !rank.contains(&bare),
        "nothing holds for a bare thing: {rank:?}"
    );
}

/// **The production stack of the mention layer, with the layer beneath it counted**:
/// the fake, the fold, the supply layer, a counter, then the mention layer.
async fn the_mention_stack() -> (
    jojobot_domain::memory::mention::Mentioning,
    Arc<Counted>,
    Arc<dyn jojobot_domain::memory::Memory>,
) {
    let provisions = shipped();
    let store = Arc::new(InMemoryMemory::booted().knowing(provisions.clone()));
    let folded = Folded::new(store);
    folded.rebuild().await.expect("the fold fills");
    let provisioned: Arc<dyn jojobot_domain::memory::Memory> =
        Arc::new(Provisioned::new(SharedFold(Arc::new(folded)), provisions));
    let counted = Arc::new(Counted::new(provisioned.clone()));
    (
        jojobot_domain::memory::mention::Mentioning::new(Arc::new(SharedCounted(counted.clone()))),
        counted,
        provisioned,
    )
}

/// **An answer whose texts hold no mention asks no store a thing about names.** The
/// roster lists every thing the store holds and asks which collections number; a
/// mention layer that builds one for each answer pays the whole instance for each. A
/// text with no stored mark and no handle makes no names read, no listing and no
/// holder query, on the write path and on every read path. The pair names one thing and
/// reads exactly one names read, and still no listing.
#[tokio::test]
async fn a_text_that_names_nothing_asks_the_store_nothing_about_names() {
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{NewEntity, NewFact};

    let (layer, counted, _) = the_mention_stack().await;
    let (about, named) = (spelled_thing(1), spelled_thing(2));
    for id in [&about, &named] {
        layer
            .add_entity(NewEntity::new(id.clone(), &id.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let claim = |words: String| {
        let mut fact = state(NewFact::about(
            about.clone(),
            words,
            jiff::civil::date(2026, 9, 2),
        ));
        fact.session = Some("fold-image".into());
        fact
    };

    counted.reset();
    let written = layer
        .capture(claim("plays the saxophone".into()))
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    let facts = layer.recall(&about).await.expect("recalled");
    let chain = layer
        .claim_history(&written.address())
        .await
        .expect("history");
    let chains = layer.claim_histories(&about).await.expect("histories");
    layer.scan_entity(&about).await.expect("scanned");
    assert_eq!(facts.len(), 1);
    assert_eq!(chain.len(), 1);
    assert_eq!(chains.len(), 1);
    let seen = counted.seen();
    for read in ["names", "list_entities", "keys_holding"] {
        assert_eq!(seen.get(read), None, "{read}: {seen:?}");
    }

    counted.reset();
    layer
        .capture(claim(format!("saw @{}", named.0)))
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    let seen = counted.seen();
    assert_eq!(seen.get("names"), Some(&1), "{seen:?}");
    assert_eq!(seen.get("list_entities"), None, "{seen:?}");
    assert_eq!(seen.get("keys_holding"), None, "{seen:?}");
}

/// **Every render and every write resolution over the names read equals the same over
/// the whole roster**, over one corpus: a rename, a merge chain, a numbered collection
/// and a number nobody holds, a supplied record, a handle a thing gave up, an archived
/// thing, a badge nothing wears and a handle that was never made a link. The roster
/// built here lists every thing the store holds, and is the answer the scoped read is
/// held to; dropping survivor-following from the read turns it red.
#[tokio::test]
async fn the_names_read_renders_and_resolves_as_the_whole_roster_does() {
    use jojobot_domain::memory::mention::{
        known_among, render_roster, rendered, resolved, roster_for, roster_for_whole,
    };
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{EntityPatch, NewEntity, NewFact};

    let (layer, _, provisioned) = the_mention_stack().await;
    let make = |n: usize| EntityId(format!("{}:{}", "person", spelled_slug(n)));
    let (renamed, merged_a, merged_b, merged_c, archived, plain) =
        (make(1), make(2), make(3), make(4), make(5), make(6));
    for id in [&renamed, &merged_a, &merged_b, &merged_c, &archived, &plain] {
        layer
            .add_entity(NewEntity::new(id.clone(), &id.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let new_name = EntityId(format!("{}:{}", "person", spelled_slug(7)));
    layer
        .rename_entity(
            &renamed,
            &new_name,
            None,
            jiff::civil::date(2026, 9, 3),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("renamed");
    for (gone, kept) in [(&merged_a, &merged_b), (&merged_b, &merged_c)] {
        layer
            .merge(
                gone,
                kept,
                Some("filed twice"),
                jiff::civil::date(2026, 9, 3),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("merged");
    }
    layer
        .archive_entity(&archived, "done", None)
        .await
        .expect("archived");
    let _ = EntityPatch::default();
    // A numbered collection and its members.
    let collection = EntityId(format!("{}:{}", "topic", spelled_slug(8)));
    layer
        .add_entity(NewEntity::new(
            collection.clone(),
            &collection.0,
            "fold-image",
        ))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let mut numbering = state(NewFact::about(
        collection.clone(),
        "numbers its members",
        jiff::civil::date(2026, 9, 2),
    ));
    numbering.session = Some("fold-image".into());
    numbering.fields.insert(
        jojobot_domain::memory::numbered::NUMBERS_BY.into(),
        "number".into(),
    );
    numbering
        .sets
        .insert(jojobot_domain::memory::numbered::NUMBERS_BY.into());
    layer
        .capture(numbering)
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    for (n, number) in [(9, "1"), (10, "2")] {
        let member = EntityId(format!("{}:{}", "topic", spelled_slug(n)));
        layer
            .add_entity(NewEntity {
                parent: Some(collection.clone()),
                ..NewEntity::new(member.clone(), &member.0, "fold-image")
            })
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        let mut fact = state(NewFact::about(
            member.clone(),
            "is a member",
            jiff::civil::date(2026, 9, 2),
        ));
        fact.session = Some("fold-image".into());
        fact.fields.insert("number".into(), number.into());
        fact.sets.insert("number".into());
        layer
            .capture(fact)
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
    }

    // The texts: every kind of name, typed and as a stored mark.
    let whole = render_roster(&*provisioned).await.expect("the roster");
    let badge_of = |id: &EntityId| {
        whole
            .iter()
            .find(|entity| entity.id == *id)
            .and_then(|entity| entity.badge.clone())
            .unwrap_or_else(|| panic!("{id:?} wears a badge"))
    };
    let mut texts: Vec<String> = Vec::new();
    for handle in [
        new_name.0.clone(),
        renamed.0.clone(),
        merged_a.0.clone(),
        merged_b.0.clone(),
        merged_c.0.clone(),
        archived.0.clone(),
        plain.0.clone(),
        format!("{}/2", collection.0),
        format!("{}/9", collection.0),
        "view:loops".to_string(),
        format!("{}:{}", "person", spelled_slug(99)),
    ] {
        texts.push(format!("saw @{handle} again"));
    }
    for id in [
        &new_name, &merged_a, &merged_b, &merged_c, &archived, &plain,
    ] {
        texts.push(format!("kept @#{} here", badge_of(id)));
    }
    texts.push("a mark nothing wears @#zzzzzz here".to_string());
    texts.push(format!("both @{} and @#{}", plain.0, badge_of(&merged_a)));

    for text in &texts {
        let scoped = known_among(&*provisioned, &[text.as_str()])
            .await
            .expect("names");
        let reference = roster_for_whole(&*provisioned, &[text.as_str()])
            .await
            .expect("the whole roster");
        assert_eq!(
            rendered(text, &scoped),
            rendered(text, &reference.known),
            "a render of {text:?}"
        );
        let scoped = roster_for(&*provisioned, &[text.as_str()])
            .await
            .expect("a scoped roster");
        let reference = roster_for_whole(&*provisioned, &[text.as_str()])
            .await
            .expect("the whole roster");
        assert_eq!(
            resolved(text, &scoped.known),
            resolved(text, &reference.known),
            "a write of {text:?}"
        );
    }
}

fn spelled_slug(n: usize) -> String {
    spelled_thing(n)
        .0
        .split_once(':')
        .expect("a handle")
        .1
        .to_string()
}

/// **A write hands back the claim as a reader is served it**, and a claim the write did
/// not re-word still holds the marks it was stored with. An edit of fields alone sends no
/// text, so no name is asked for by the text sent; the claim it returns, and the claim a
/// retraction returns, name their own marks and render them as a recall does.
#[tokio::test]
async fn a_claim_a_write_returns_renders_the_marks_it_was_stored_with() {
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{FactPatch, NewEntity, NewFact};

    let (layer, _, _) = the_mention_stack().await;
    let (about, named) = (spelled_thing(1), spelled_thing(2));
    for id in [&about, &named] {
        layer
            .add_entity(NewEntity::new(id.clone(), &id.0, "fold-image"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let capture = |words: String| {
        let mut fact = state(NewFact::about(
            about.clone(),
            words,
            jiff::civil::date(2026, 9, 2),
        ));
        fact.session = Some("fold-image".into());
        fact
    };
    let stored = layer
        .capture(capture(format!("saw @{}", named.0)))
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    let expected = format!("saw @{}", named.0);
    assert_eq!(stored.content, expected);

    let mut patch = FactPatch::default();
    patch.fields.insert("note".into(), "kept".into());
    let edited = layer
        .update_fact(&stored.address(), patch, &EntityId("bot:sigma".into()))
        .await
        .expect("edited")
        .written()
        .expect("not blocked");
    assert_eq!(edited.content, expected, "a fields-only edit");

    let retraction = layer
        .retract(
            &stored.address(),
            None,
            jiff::civil::date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("retracted");
    assert_eq!(retraction.retracted.content, expected, "a retraction");

    let second = layer
        .capture(capture(format!("saw @{}", named.0)))
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    let (retraction, _) = layer
        .retract_with_claim(
            &second.address(),
            None,
            jiff::civil::date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            None,
            capture("a note that names nothing".into()),
        )
        .await
        .expect("retracted")
        .written()
        .expect("not blocked");
    assert_eq!(
        retraction.retracted.content, expected,
        "a retraction with a claim"
    );
}

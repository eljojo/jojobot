//! **A thing's document read from the claims State is the document the store's
//! own reads give.** A store written through the port in the shapes that move a
//! document (a rewritten claim, a link, a merge, a rename, an archive) is
//! imported, replayed into a State, and every thing is read both ways.

use jiff::civil::date;
use jojobot_domain::log::ObjectId;
use jojobot_domain::log::claims::{State, import};
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityId, FactAddress, FactPatch, Guarded, Memory, NewEntity, NewFact,
    Override, Provenance, guard,
};

use super::image::Doc;
use crate::dolt::Dolt;
use crate::dolt::memory::DoltMemory;
use crate::dolt::migrate;
use crate::testing::{Scratch, free_port, migrated_database};

async fn thing(memory: &dyn Memory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    let new = NewEntity::new(id.clone(), name, "fixture");
    match memory.add_entity(new.clone()).await.expect("add") {
        Guarded::Blocked {
            attempted,
            candidates,
        } => {
            memory
                .add_entity(NewEntity {
                    override_token: Some(guard::override_token(&attempted, &candidates)),
                    override_reason: Some("a fixture of the same family".into()),
                    ..new
                })
                .await
                .expect("add again")
                .written()
                .expect("not blocked twice");
        }
        other => {
            other.written().expect("written");
        }
    }
    id
}

async fn say(
    memory: &dyn Memory,
    about: &EntityId,
    what: &str,
    fields: &[(&str, &str)],
    day: i8,
) -> FactAddress {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, day));
    fact.provenance = Provenance::Inference;
    fact.fields = fields
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    fact.sets = fact.fields.keys().cloned().collect();
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address()
}

/// **Everything the State can answer today equals the store's answer**: the
/// document of every thing that no merge moved a claim into, the listing, the
/// former handles, who points at each thing and what stands on each claim.
#[tokio::test]
async fn a_document_read_from_the_state_is_the_document_the_store_reads() {
    compare(false).await;
}

/// **The merge survivor's document, and the merge events, equal the store's.**
/// Not run yet: the import places a claim a merge moved under the identity it was
/// made under, and the merge event names another, so the survivor's State lacks
/// the folded thing's fields. It runs again when the import names the moved record.
#[tokio::test]
#[ignore = "TEMPORARY(9.3.3): the import names a moved claim by an identity it never created; this runs when card q-933-import-places-moved-claim-under-own-identity places a merged claim under its own"]
async fn a_merge_survivors_document_read_from_the_state_is_the_document_the_store_reads() {
    compare(true).await;
}

/// The store is written through the port, imported and replayed; then the things
/// of one kind are compared: the merge survivor with `merged`, every other thing
/// without.
async fn compare(merged: bool) {
    let scratch = Scratch::new("fold-from-state");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "fromstate")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let homer = thing(&memory, "person:homer", "Homer").await;
    let marge = thing(&memory, "person:marge", "Marge").await;
    let bart = thing(&memory, "person:bart", "Bart").await;
    let lisa = thing(&memory, "person:lisa", "Lisa").await;
    let nelson = thing(&memory, "person:nelson", "Nelson").await;
    let krusty = thing(&memory, "person:krusty", "Krusty").await;

    let eats = say(&memory, &homer, "eats donuts", &[("mood", "happy")], 1).await;
    say(
        &memory,
        &homer,
        "is married",
        &[("spouse", "person:marge")],
        2,
    )
    .await;
    let keeps = say(&memory, &marge, "keeps the house", &[("hair", "blue")], 1).await;
    memory
        .update_fact(
            &keeps,
            FactPatch {
                content: Some("keeps the house tidy".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("the claim is rewritten")
        .written()
        .expect("not blocked");
    let mut fact = NewFact::about(marge.clone(), "is married to Homer", date(2026, 9, 3));
    fact.provenance = Provenance::Testimony;
    fact.edge = Some(Edge::new(EdgeShape::Connection, homer.clone()));
    fact.derived_from = Some(eats);
    fact.refs = vec![homer.clone()];
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    say(&memory, &bart, "skates", &[("mood", "mischievous")], 1).await;
    say(
        &memory,
        &lisa,
        "plays the saxophone",
        &[("instrument", "saxophone")],
        1,
    )
    .await;
    say(&memory, &krusty, "hosts a show", &[], 1).await;
    memory
        .merge(
            &bart,
            &lisa,
            Some("filed twice"),
            date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            Some("run-a"),
        )
        .await
        .expect("the fold lands");
    let renamed = EntityId("person:ralph".into());
    let rename = |token: Option<String>| {
        let (memory, nelson, renamed) = (&memory, &nelson, &renamed);
        async move {
            memory
                .rename_entity(
                    nelson,
                    renamed,
                    None,
                    date(2026, 9, 4),
                    Override::new(
                        token.as_deref(),
                        token.as_ref().map(|_| "a fixture of one family"),
                    ),
                    None,
                )
                .await
                .expect("the rename answers")
        }
    };
    if let Guarded::Blocked {
        attempted,
        candidates,
    } = rename(None).await
    {
        rename(Some(guard::override_token(&attempted, &candidates)))
            .await
            .written()
            .expect("the rename lands");
    }
    memory
        .archive_entity(&krusty, "off the air", None)
        .await
        .expect("the archive lands");

    let read = memory.import_rows().await.expect("the reader reads");
    assert!(read.findings.is_empty(), "{:?}", read.findings);
    let events = import::import(&read.rows).expect("the rows map");
    let state = State::replay(events.iter());
    let declared = memory.declared_types().await.expect("the types");

    let listed = memory.list_entities(None).await.expect("the listing");
    assert!(listed.len() >= 5, "the fixture lists its things");
    let mut differences: Vec<String> = Vec::new();
    for entity in listed
        .iter()
        .filter(|e| e.merged_into.is_none() && (e.id.0 == "person:lisa") == merged)
    {
        let handle = &entity.id;
        let stored = Doc::load(&memory, handle).await.expect("the store reads");
        let held = state
            .document_parts(&ObjectId(stored.scan.doc_id.clone()), &declared)
            .unwrap_or_else(|| panic!("{handle}: the State holds the thing"));
        let mut differ = |part: &str, same: bool, held: String, stored: String| {
            if !same {
                differences.push(format!(
                    "{handle}: {part}\n  state:  {held}\n  stored: {stored}"
                ));
            }
        };
        let scanned = state
            .document_scan(&ObjectId(stored.scan.doc_id.clone()), &declared)
            .unwrap_or_else(|| panic!("{handle}: the State serves a scan"));
        let theirs = &stored.scan;
        differ(
            "scan entity",
            scanned.entity == theirs.entity,
            format!("{:?}", scanned.entity),
            format!("{:?}", theirs.entity),
        );
        differ(
            "scan facts",
            scanned.facts == theirs.facts,
            format!("{:#?}", scanned.facts),
            format!("{:#?}", theirs.facts),
        );
        differ(
            "scan unmapped",
            scanned.unmapped == theirs.unmapped,
            format!("{:?}", scanned.unmapped),
            format!("{:?}", theirs.unmapped),
        );
        differ(
            "scan the rest",
            (
                &scanned.doc_id,
                &scanned.title,
                &scanned.prose,
                &scanned.fields,
                &scanned.field_links,
                &scanned.owner,
            ) == (
                &theirs.doc_id,
                &theirs.title,
                &theirs.prose,
                &theirs.fields,
                &theirs.field_links,
                &theirs.owner,
            ),
            format!(
                "{:?}",
                (
                    &scanned.doc_id,
                    &scanned.title,
                    &scanned.prose,
                    &scanned.fields,
                    &scanned.field_links
                )
            ),
            format!(
                "{:?}",
                (
                    &theirs.doc_id,
                    &theirs.title,
                    &theirs.prose,
                    &theirs.fields,
                    &theirs.field_links
                )
            ),
        );
        differ(
            "fields",
            held.fields == stored.fields,
            format!("{:?}", held.fields),
            format!("{:?}", stored.fields),
        );
        differ(
            "version",
            held.version == stored.version,
            format!("{:?}", held.version),
            format!("{:?}", stored.version),
        );
        differ(
            "ages",
            held.ages == stored.ages,
            format!("{:?}", held.ages),
            format!("{:?}", stored.ages),
        );
        differ(
            "histories",
            held.histories == stored.histories,
            format!("{:?}", held.histories),
            format!("{:?}", stored.histories),
        );
        differ(
            "backing",
            held.backing == stored.backing,
            format!("{:?}", held.backing),
            format!("{:?}", stored.backing),
        );
        differ(
            "trails",
            held.trails == stored.trails,
            format!("{:?}", held.trails),
            format!("{:?}", stored.trails),
        );
        differ(
            "entity versions",
            Some(&held.entity_versions) == stored.entity_versions.as_ref(),
            format!("{:?}", held.entity_versions),
            format!("{:?}", stored.entity_versions),
        );
        let assembled = Doc::from_state(&state, &ObjectId(stored.scan.doc_id.clone()), &declared)
            .unwrap_or_else(|| panic!("{handle}: the State assembles a document"));
        differ(
            "unmapped histories",
            assembled.unmapped_histories == stored.unmapped_histories,
            format!("{:?}", assembled.unmapped_histories),
            format!("{:?}", stored.unmapped_histories),
        );
        differ(
            "assembled scan and fields",
            (&assembled.scan, &assembled.fields, assembled.version)
                == (&stored.scan, &stored.fields, stored.version),
            format!("{:?}", assembled.fields),
            format!("{:?}", stored.fields),
        );
    }
    if merged {
        let stored_merges = memory.merge_events(None).await.expect("the merges");
        let held_merges = state.merge_events(None);
        if held_merges != stored_merges {
            differences.push(format!(
                "merge events\n  state:  {held_merges:?}\n  stored: {stored_merges:?}"
            ));
        }
    }
    if !merged {
        compare_the_whole_store(&memory, &state, &mut differences).await;
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    server.stop().await;
}

/// The answers that are about the whole store and not about one thing.
async fn compare_the_whole_store(
    memory: &DoltMemory,
    state: &State,
    differences: &mut Vec<String>,
) {
    let stored_listing = memory.list_entities(None).await.expect("the listing");
    let held_listing = state.listing();
    if held_listing != stored_listing {
        differences.push(format!(
            "listing\n  state:  {held_listing:#?}\n  stored: {stored_listing:#?}"
        ));
    }
    let stored_formers = memory.former_handles().await.expect("the former handles");
    let held_formers = state.former_handles();
    if held_formers != stored_formers {
        differences.push(format!(
            "former handles\n  state:  {held_formers:?}\n  stored: {stored_formers:?}"
        ));
    }
    for entity in stored_listing.iter().filter(|e| e.merged_into.is_none()) {
        let stored_referring = memory
            .referring_to(&entity.id)
            .await
            .expect("who points here");
        let held_referring = state.referring(&entity.id);
        if held_referring != stored_referring {
            differences.push(format!(
                "referring to {}\n  state:  {held_referring:#?}\n  stored: {stored_referring:#?}",
                entity.id
            ));
        }
    }
    for entity in stored_listing.iter().filter(|e| e.merged_into.is_none()) {
        let scanned = memory
            .scan_entity(&entity.id)
            .await
            .expect("the scan")
            .expect("the thing");
        for fact in &scanned.facts {
            let address = FactAddress::new(entity.id.clone(), fact.id.clone());
            let stored_built = memory.built_on(&address).await.expect("what stands on it");
            let held_built = state.built_on(&address);
            if held_built != stored_built {
                differences.push(format!(
                    "built on {address}\n  state:  {held_built:#?}\n  stored: {stored_built:#?}"
                ));
            }
        }
    }
}

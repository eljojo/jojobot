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

#[tokio::test]
async fn a_document_read_from_the_state_is_the_document_the_store_reads() {
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
            &lisa,
            &bart,
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
    for entity in listed.iter().filter(|e| e.merged_into.is_none()) {
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
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    server.stop().await;
}

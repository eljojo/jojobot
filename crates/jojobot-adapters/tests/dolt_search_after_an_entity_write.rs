//! **What a search costs the real store after an edit of a thing, a rename, an
//! archive, a restore or a page of prose.**
//!
//! Each of these writes re-reads the documents it changes, and the index is then
//! current with the store, so the search that follows reads nothing but the
//! store's signal. The instrument is the store's own statement log, counted per
//! write and per search. A rename also changes how every OTHER thing's stored
//! handle renders, so its holders are re-read: a field, a claim's words, a page
//! of prose and a record the build supplies each name the renamed thing, and the
//! search after the rename finds them by the new handle and not by the old.
//!
//! Run with `--nocapture` to read the counts, one line per verb.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::mention::Mentioning;
use jojobot_domain::memory::owned::{Provision, Provisions};
use jojobot_domain::memory::search::{HeldField, Hit, Search, SearchQuery};
use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, EntityPatch, Memory, NewEntity, NewFact,
};
use tracing_subscriber::layer::{Context, SubscriberExt};

static STATEMENTS: AtomicUsize = AtomicUsize::new(0);

/// Counts the statements the store's driver logs, one event each.
struct CountStatements;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CountStatements {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() == "sqlx::query" {
            STATEMENTS.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// How many statements `run` cost.
async fn counted<T>(run: impl std::future::Future<Output = T>) -> (T, usize) {
    let before = STATEMENTS.load(Ordering::SeqCst);
    let out = run.await;
    (out, STATEMENTS.load(Ordering::SeqCst) - before)
}

fn names(hits: &[Hit], who: &EntityId) -> bool {
    hits.iter().any(|hit| match hit {
        Hit::Entity { entity, .. } => &entity.id == who,
        Hit::Fact { subject, .. } => &subject.id == who,
        _ => false,
    })
}

fn holding_companion(who: &EntityId) -> SearchQuery {
    SearchQuery {
        holding: vec![HeldField {
            key: "companion".to_string(),
            value: Some(who.as_str().to_string()),
        }],
        ..SearchQuery::default()
    }
}

fn the_shipped_view() -> Provisions {
    Provisions::new(vec![Provision::record(
        Entity {
            id: EntityId("view:loops".into()),
            kind: EntityKind::VIEW,
            name: "The Loops".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        },
        std::collections::BTreeMap::new(),
    )])
}

#[tokio::test]
async fn a_search_after_an_edit_a_rename_an_archive_a_restore_or_prose_costs_what_a_search_after_none_costs()
 {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "jojobot-after-an-entity-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
    )));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "afteranentitywrite")
        .await
        .expect("a database of its own");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let _counting =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(CountStatements));

    // The stack the server serves through: the store that knows what the build
    // supplies, the layer that resolves it, the layer that renders a mention as
    // a handle, and the index.
    let supplied = the_shipped_view();
    let provisioned: Arc<dyn Memory> = Arc::new(Provisioned::new(
        DoltMemory::open(pool.clone()).knowing(supplied.clone()),
        supplied,
    ));
    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(Mentioning::new(provisioned))).expect("the search index opens"),
    );
    indexed.rebuild().await.expect("the boot scan reads");
    let retrieval = Retrieval::new(indexed.index(), vec![indexed.clone()]);

    let pet = EntityId("pet:contract-quokka-kept".into());
    let holder = EntityId::person("person:contract-bellringer");
    let pager = EntityId::person("person:contract-lamplighter");
    let view = EntityId("view:loops".into());
    for (id, name) in [
        (&pet, "Kept Pet"),
        (&holder, "Bell Ringer"),
        (&pager, "Lamp Lighter"),
    ] {
        indexed
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .expect("add_entity ok")
            .written()
            .expect("nothing collides with it");
    }
    // Three things name the pet: a person by a field, the build's own view by a
    // field on a claim written about it, and a page of prose by a mention.
    for subject in [&holder, &view] {
        indexed
            .capture(NewFact {
                fields: [("companion".to_string(), pet.as_str().to_string())]
                    .into_iter()
                    .collect(),
                sets: ["companion".to_string()].into_iter().collect(),
                ..NewFact::about(subject.clone(), "keeps the quokka", date(2026, 7, 1))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("lands");
    }
    indexed
        .capture(NewFact::about(
            view.clone(),
            "watches @pet:contract-quokka-kept sleep",
            date(2026, 7, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("lands");
    indexed
        .set_prose(&pager, "walks @pet:contract-quokka-kept at dawn", None)
        .await
        .expect("prose lands");

    // One search to settle, then one with nothing written between: the price of
    // a search that finds the store unchanged.
    retrieval
        .search(&SearchQuery::text("Kept"))
        .await
        .expect("search ok");
    let (idle_hits, idle) = counted(retrieval.search(&SearchQuery::text("Kept"))).await;
    assert!(names(&idle_hits.expect("search ok"), &pet));
    assert!(idle > 0, "the instrument counts statements: {idle}");

    // ── an edit, an edit carrying a claim, a page of prose ───────────────────
    let (edited, write) = counted(indexed.update_entity(
        &pet,
        EntityPatch {
            name: Some("Marmalade Quokka".into()),
            ..Default::default()
        },
    ))
    .await;
    edited.expect("edits").written().expect("lands");
    let (hits, after) = counted(retrieval.search(&SearchQuery::text("Marmalade"))).await;
    assert!(
        names(&hits.expect("search ok"), &pet),
        "the new name is found"
    );
    eprintln!("STATEMENTS update_entity write={write} search={after} idle={idle}");
    assert_eq!(after, idle, "a search after update_entity");

    let (edited, write) = counted(indexed.update_entity_with_claim(
        &pet,
        EntityPatch {
            name: Some("Pistachio Quokka".into()),
            ..Default::default()
        },
        NewFact::about(pet.clone(), "naps in the pantry", date(2026, 7, 2)),
    ))
    .await;
    edited.expect("edits").written().expect("lands");
    let (by_name, after) = counted(retrieval.search(&SearchQuery::text("Pistachio"))).await;
    assert!(
        names(&by_name.expect("search ok"), &pet),
        "the new name is found"
    );
    let by_claim = retrieval
        .search(&SearchQuery::text("pantry"))
        .await
        .expect("search ok");
    assert!(names(&by_claim, &pet), "the claim is found");
    eprintln!("STATEMENTS update_entity_with_claim write={write} search={after} idle={idle}");
    assert_eq!(after, idle, "a search after update_entity_with_claim");

    let (stored, write) =
        counted(indexed.set_prose(&pet, "sleeps under the marmalade tree", None)).await;
    stored.expect("prose lands");
    let (hits, after) = counted(retrieval.search(&SearchQuery::text("tree"))).await;
    assert!(
        hits.expect("search ok").iter().any(|h| matches!(
            h,
            Hit::Prose { entity: Some(e), .. } if e.id == pet
        )),
        "the prose is found"
    );
    eprintln!("STATEMENTS set_prose write={write} search={after} idle={idle}");
    assert_eq!(after, idle, "a search after set_prose");

    // ── an archive and a restore ─────────────────────────────────────────────
    let (archived, write) = counted(indexed.archive_entity(&pet, "no longer kept", None)).await;
    archived.expect("archives");
    let (_, after) = counted(retrieval.search(&holding_companion(&pet))).await;
    eprintln!("STATEMENTS archive_entity write={write} search={after} idle={idle}");
    assert_eq!(after, idle, "a search after archive_entity");
    let (restored, write) = counted(indexed.restore_entity(&pet, None)).await;
    restored.expect("restores");
    let (hits, after) = counted(retrieval.search(&holding_companion(&pet))).await;
    assert!(
        names(&hits.expect("search ok"), &holder),
        "after the restore the holder is found by the pet's handle"
    );
    eprintln!("STATEMENTS restore_entity write={write} search={after} idle={idle}");
    assert_eq!(after, idle, "a search after restore_entity");

    // ── a rename, and everything that names the renamed thing ────────────────
    let renamed = EntityId("pet:contract-wombat-renamed".into());
    let (moved, write) =
        counted(indexed.rename_entity(&pet, &renamed, None, date(2026, 7, 3), None, None)).await;
    moved.expect("the rename lands");
    let (by_new, after) = counted(retrieval.search(&holding_companion(&renamed))).await;
    let by_new = by_new.expect("search ok");
    eprintln!("STATEMENTS rename_entity write={write} search={after} idle={idle}");
    assert_eq!(after, idle, "a search after rename_entity");
    let by_old = retrieval
        .search(&holding_companion(&pet))
        .await
        .expect("search ok");
    for (who, what) in [
        (&holder, "a person's field"),
        (&view, "the supplied view's claim"),
    ] {
        assert!(
            names(&by_new, who),
            "the new handle finds {what}: {by_new:?}"
        );
        assert!(
            !names(&by_old, who),
            "and the old handle does not find {what}: {by_old:?}"
        );
    }
    let prose_by = |word: &'static str| {
        let retrieval = &retrieval;
        let pager = &pager;
        async move {
            retrieval
                .search(&SearchQuery::text(word))
                .await
                .expect("search ok")
                .iter()
                .any(|h| matches!(h, Hit::Prose { entity: Some(e), .. } if &e.id == pager))
        }
    };
    // The claim written about the build's own view, whose words mention the pet.
    let view_claim_by = |word: &'static str, handle: &'static str| {
        let retrieval = &retrieval;
        let view = &view;
        async move {
            retrieval
                .search(&SearchQuery::text(word))
                .await
                .expect("search ok")
                .iter()
                .any(|h| {
                    matches!(h, Hit::Fact { fact, .. }
                        if &fact.subject == view && fact.content.contains(handle))
                })
        }
    };
    assert!(
        view_claim_by("wombat", "contract-wombat-renamed").await,
        "the new handle finds the claim on the supplied view that mentions it"
    );
    assert!(
        !view_claim_by("quokka", "contract-quokka-kept").await,
        "and the old handle does not"
    );
    assert!(
        prose_by("wombat").await,
        "the new handle finds the page that mentions it"
    );
    assert!(
        !prose_by("quokka").await,
        "and the old handle does not find that page"
    );

    // ── a change made past the application is still found by a scan ──────────
    let badge: Option<String> = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(holder.as_str())
        .fetch_one(&pool)
        .await
        .expect("the row is readable");
    assert!(badge.is_some(), "a created entity is given a badge");
    sqlx::query("UPDATE entity SET name = ? WHERE id = ?")
        .bind("Renamed By Hand")
        .bind(holder.as_str())
        .execute(&pool)
        .await
        .expect("the direct edit lands");
    let (hand_hits, after_a_hand_edit) =
        counted(retrieval.search(&SearchQuery::text("Renamed By Hand"))).await;
    assert!(
        names(&hand_hits.expect("search ok"), &holder),
        "an edit made past the application is found"
    );
    assert!(
        after_a_hand_edit > idle,
        "the search that finds a hand edit reads the store: {after_a_hand_edit} vs {idle}"
    );

    store.stop().await;
}

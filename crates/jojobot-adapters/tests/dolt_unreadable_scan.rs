//! **A thing whose document cannot be read is named beside the answer of a walk
//! over the real store, and the things that read are served** (rule 130). The
//! recall verb serves what `take_unreadable` leaves, so this is the real-store
//! half of that case: the store must raise a per-thing failure for the damaged
//! row and must not fail the listing or the walk.
//!
//! The damage is a row with no badge, which the store cannot give a document id
//! and says so (`scan_one_doc`): the state a row holds when the startup fill did
//! not reach it.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity, NewFact, graph};

async fn thing(memory: &dyn Memory, handle: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), handle, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let mut fact = NewFact::about(id.clone(), "where it stands", date(2026, 9, 1));
    fact.fields.insert("status".into(), "next".into());
    fact.sets.insert("status".into());
    memory.capture(fact).await.expect("capture");
    id
}

#[tokio::test]
async fn a_row_the_store_cannot_scan_is_named_and_the_others_are_served() {
    let dir = jojobot_adapters::testing::scratch_path("unreadable-scan");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "unreadable-scan")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    for handle in ["thing:alpha", "thing:gamma", "thing:delta"] {
        thing(&memory, handle).await;
    }
    let query = || graph::GraphQuery {
        select: graph::Selection {
            kind: Some(EntityKind::THING),
            ..graph::Selection::default()
        },
        include: graph::Include {
            counted: false,
            facts: false,
            prose: false,
            stood_for: false,
            status: None,
        },
        ..graph::GraphQuery::default()
    };

    let whole = graph::walk(&memory, &[], &query())
        .await
        .expect("an undamaged store walks");
    assert_eq!(whole.objects.len(), 3);
    assert!(whole.unreadable_scans.is_empty());

    sqlx::query("UPDATE entity SET badge = NULL WHERE id = 'thing:gamma'")
        .execute(&pool)
        .await
        .expect("the row loses its badge");

    let mut selected = graph::walk_in_window(&memory, &[], &query(), None)
        .await
        .expect("a damaged row does not fail the walk");
    assert!(
        selected
            .unreadable_scans
            .contains_key(&EntityId("thing:gamma".into())),
        "the damaged row is named: {:?}",
        selected.unreadable_scans
    );
    let named = graph::take_unreadable(&mut selected.objects, &selected.unreadable_scans);
    let mut served: Vec<String> = selected
        .objects
        .iter()
        .map(|object| object.entity.id.as_str().to_string())
        .collect();
    served.sort();
    assert_eq!(served, ["thing:alpha", "thing:delta"]);
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].0.as_str(), "thing:gamma");
    drop(server);
}

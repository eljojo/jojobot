//! **Every write of an entity keeps what the entity looked like afterwards.**
//!
//! `entity_write` used to log that a write happened and when. It now keeps the
//! whole row as it stood after the write, so an earlier name, alias set, prose or
//! archive state can be read back. The cases write through the port and read the
//! table, because the table is what a history read is built on.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::{EntityId, EntityPatch, Memory, NewEntity, kinds};

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One stored version of the handcart, as the table keeps it.
type Version = (
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

async fn versions(pool: &sqlx::MySqlPool, handle: &str) -> Vec<Version> {
    sqlx::query_as(
        "SELECT ordinal, name, aliases, prose, archived_reason, archived_at IS NOT NULL \
         FROM entity_write WHERE entity = ? ORDER BY ordinal",
    )
    .bind(handle)
    .fetch_all(pool)
    .await
    .expect("the versions are readable")
}

fn version(
    ordinal: i64,
    name: &str,
    aliases: &str,
    prose: &str,
    archived: Option<&str>,
) -> Version {
    (
        ordinal,
        Some(name.into()),
        Some(aliases.into()),
        Some(prose.into()),
        archived.map(str::to_string),
        archived.is_some(),
    )
}

/// **A rename, an alias change, a prose edit, an archive and a restore each
/// leave a version, and the versions before it are not touched.** The current
/// read at the end is the last version: the entity row is still the projection.
#[tokio::test]
async fn each_write_of_an_entity_keeps_its_whole_row_and_the_earlier_ones_stay() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "jojobot-entity-write-snapshot-{}",
        std::process::id()
    )));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "entity_write_snapshot")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    kinds::seed(&memory).await.expect("the kinds are seeded");

    let handcart = EntityId("thing:handcart".into());
    memory
        .add_entity(NewEntity::new(handcart.clone(), "The Handcart", "test"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides");
    memory
        .update_entity(
            &handcart,
            EntityPatch {
                name: Some("The Cart".into()),
                ..EntityPatch::default()
            },
        )
        .await
        .expect("a rename of the display name")
        .written()
        .expect("nothing collides");
    memory
        .update_entity(
            &handcart,
            EntityPatch {
                aliases: Some(vec!["wagon".into()]),
                ..EntityPatch::default()
            },
        )
        .await
        .expect("an alias change")
        .written()
        .expect("nothing collides");
    memory
        .set_prose(&handcart, "A two-wheeled cart.")
        .await
        .expect("a prose edit");
    memory
        .archive_entity(&handcart, "sold")
        .await
        .expect("an archive");
    memory.restore_entity(&handcart).await.expect("a restore");

    let kept = versions(&pool, handcart.as_str()).await;
    let current = memory
        .list_entities(None)
        .await
        .expect("a current read")
        .into_iter()
        .find(|entity| entity.id == handcart)
        .expect("the entity is there");
    store.stop().await;

    let prose = "A two-wheeled cart.";
    assert_eq!(
        kept,
        vec![
            version(1, "The Handcart", "[]", "", None),
            version(2, "The Cart", "[]", "", None),
            version(3, "The Cart", "[\"wagon\"]", "", None),
            version(4, "The Cart", "[\"wagon\"]", prose, None),
            version(5, "The Cart", "[\"wagon\"]", prose, Some("sold")),
            version(6, "The Cart", "[\"wagon\"]", prose, None),
        ]
    );
    // The projection answers as it always did.
    assert_eq!(current.name, "The Cart");
    assert_eq!(current.aliases, vec!["wagon".to_string()]);
    assert!(current.archived.is_none());
}

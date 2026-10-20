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
    Option<String>,
);

async fn versions(pool: &sqlx::MySqlPool, handle: &str) -> Vec<Version> {
    sqlx::query_as(
        "SELECT ordinal, name, aliases, prose, archived_reason, archived_at IS NOT NULL, session \
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
    session: &str,
) -> Version {
    (
        ordinal,
        Some(name.into()),
        Some(aliases.into()),
        Some(prose.into()),
        archived.map(str::to_string),
        archived.is_some(),
        Some(session.into()),
    )
}

/// **A rename, an alias change, a prose edit, an archive and a restore each
/// leave a version, and the versions before it are not touched.** The current
/// read at the end is the last version: the entity row is still the projection.
#[tokio::test]
async fn each_write_of_an_entity_keeps_its_whole_row_and_the_earlier_ones_stay() {
    let scratch = Scratch(jojobot_adapters::testing::scratch_path(
        "jojobot-entity-write-snapshot",
    ));
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
        .add_entity(NewEntity {
            session: Some("mk01".into()),
            ..NewEntity::new(handcart.clone(), "The Handcart", "test")
        })
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides");
    memory
        .update_entity(
            &handcart,
            EntityPatch {
                name: Some("The Cart".into()),
                session: Some("mk02".into()),
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
                session: Some("mk03".into()),
                ..EntityPatch::default()
            },
        )
        .await
        .expect("an alias change")
        .written()
        .expect("nothing collides");
    memory
        .set_prose(&handcart, "A two-wheeled cart.", Some("mk04"))
        .await
        .expect("a prose edit");
    memory
        .archive_entity(&handcart, "sold", Some("mk05"))
        .await
        .expect("an archive");
    memory
        .restore_entity(&handcart, Some("mk06"))
        .await
        .expect("a restore");

    // A rename moves the handle and files its version under the new one.
    let moved = EntityId("thing:shelf".into());
    memory
        .rename_entity(
            &handcart,
            &moved,
            None,
            jiff::civil::date(2026, 6, 1),
            jojobot_domain::memory::Override::none(),
            Some("mk07"),
        )
        .await
        .expect("a rename of the handle")
        .written()
        .expect("nothing collides");

    let kept = versions(&pool, handcart.as_str()).await;
    let after_the_move = versions(&pool, moved.as_str()).await;
    let current = memory
        .list_entities(None)
        .await
        .expect("a current read")
        .into_iter()
        .find(|entity| entity.id == moved)
        .expect("the entity is there");
    store.stop().await;

    let prose = "A two-wheeled cart.";
    assert_eq!(
        kept,
        vec![
            version(1, "The Handcart", "[]", "", None, "mk01"),
            version(2, "The Cart", "[]", "", None, "mk02"),
            version(3, "The Cart", "[\"wagon\"]", "", None, "mk03"),
            version(4, "The Cart", "[\"wagon\"]", prose, None, "mk04"),
            version(5, "The Cart", "[\"wagon\"]", prose, Some("sold"), "mk05"),
            version(6, "The Cart", "[\"wagon\"]", prose, None, "mk06"),
        ]
    );
    assert_eq!(
        after_the_move,
        vec![version(1, "The Cart", "[\"wagon\"]", prose, None, "mk07")]
    );
    // The projection answers as it always did.
    assert_eq!(current.name, "The Cart");
    assert_eq!(current.aliases, vec!["wagon".to_string()]);
    assert!(current.archived.is_none());
}

/// **The history read, against the real store**: the shared case every store
/// answers, then the real store's own shape. A rename moves the handle, and a
/// history asked by either the old or the new handle holds every version; a row
/// an older build appended reads as a version with no values.
#[tokio::test]
async fn the_history_of_an_entity_follows_its_handle_and_marks_what_an_older_build_did_not_keep() {
    use jojobot_domain::memory::testing::contract;

    let scratch = Scratch(jojobot_adapters::testing::scratch_path(
        "jojobot-entity-history",
    ));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "entity_history")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    kinds::seed(&memory).await.expect("the kinds are seeded");

    contract::an_entity_history_returns_each_earlier_value_in_order_naming_its_session(&memory)
        .await;

    let lisa = EntityId("person:lisa".into());
    memory
        .add_entity(NewEntity::new(lisa.clone(), "Lisa", "test"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides");
    // The version an older build would have left first: a write and a moment,
    // and none of the values.
    sqlx::query("UPDATE entity_write SET name = NULL, kind = NULL, source = NULL, boot = NULL, aliases = NULL, prose = NULL, badge = NULL, session = NULL WHERE entity = 'person:lisa'")
        .execute(&pool)
        .await
        .expect("the version is stripped to the old shape");
    let renamed = EntityId("person:homer".into());
    memory
        .rename_entity(
            &lisa,
            &renamed,
            None,
            jiff::civil::date(2026, 6, 1),
            jojobot_domain::memory::Override::none(),
            Some("rn01"),
        )
        .await
        .expect("a rename of the handle")
        .written()
        .expect("nothing collides");

    let by_new = memory.entity_history(&renamed).await.expect("history");
    let by_old = memory
        .entity_history(&lisa)
        .await
        .expect("history by the former handle");
    store.stop().await;

    assert_eq!(by_new.len(), 2, "both handles' versions: {by_new:?}");
    assert_eq!(by_old, by_new, "the former handle asks the same thing");
    assert_eq!(
        by_new.iter().map(|v| v.ordinal).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(
        by_new[0].snapshot.is_none() && by_new[0].written_at.is_some(),
        "the old version says that a write happened and when, and nothing else"
    );
    assert_eq!(by_new[1].session.as_deref(), Some("rn01"));
    assert_eq!(
        by_new[1].snapshot.as_ref().map(|held| held.name.as_str()),
        Some("Lisa"),
        "the version after the rename holds the row"
    );
}

/// **A store with the pre-snapshot shape, a handle freed by a rename and taken by
/// a new thing.** The rows an older build appended name no badge, so the table
/// cannot say which thing they belong to; the former-handle table can.
async fn store_with_old_rows(name: &str) -> (Scratch, Dolt, sqlx::MySqlPool, DoltMemory) {
    let scratch = Scratch(jojobot_adapters::testing::scratch_path(&format!(
        "jojobot-freed-handle-{name}"
    )));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, name)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    kinds::seed(&memory).await.expect("the kinds are seeded");
    (scratch, store, pool, memory)
}

async fn make(memory: &DoltMemory, handle: &str, name: &str) {
    let made = memory
        .add_entity(NewEntity::new(EntityId(handle.into()), name, "test"))
        .await
        .expect("add_entity ok");
    assert!(
        made.clone().written().is_some(),
        "{handle} was blocked: {made:?}"
    );
}

/// **A version an older build filed under a handle that has since been freed and
/// taken by a new thing belongs to the thing that wore the handle then.** The
/// first krusty is written, stripped to the shape an older build left (a write
/// and a moment, no values and no badge), renamed, and a second krusty is made
/// under the freed handle. The renamed thing's history holds the old version; the
/// new thing's holds none of the old writes.
#[tokio::test]
async fn an_old_version_filed_under_a_reused_handle_belongs_to_the_thing_that_wore_it_then() {
    let (_scratch, mut store, pool, memory) = store_with_old_rows("reused").await;
    let krusty = EntityId("person:krusty".into());
    make(&memory, "person:krusty", "Clown").await;
    sqlx::query("UPDATE entity_write SET name = NULL, kind = NULL, source = NULL, boot = NULL, aliases = NULL, prose = NULL, badge = NULL, session = NULL WHERE entity = 'person:krusty'")
        .execute(&pool)
        .await
        .expect("the version is stripped to the old shape");
    let renamed = EntityId("person:barney-gumble".into());
    memory
        .rename_entity(
            &krusty,
            &renamed,
            None,
            jiff::civil::date(2026, 6, 1),
            jojobot_domain::memory::Override::none(),
            Some("rn01"),
        )
        .await
        .expect("a rename of the handle")
        .written()
        .expect("nothing collides");
    make(&memory, "person:krusty", "Showman").await;

    let of_renamed = memory.entity_history(&renamed).await.expect("history");
    let of_new = memory.entity_history(&krusty).await.expect("history");
    store.stop().await;

    assert!(
        of_renamed.iter().any(|v| v.snapshot.is_none()),
        "the renamed thing keeps the version an older build filed under its old handle: {of_renamed:?}"
    );
    assert_eq!(
        of_new.iter().filter(|v| v.snapshot.is_none()).count(),
        0,
        "the new thing wears the handle and owns none of the old writes: {of_new:?}"
    );
    assert!(
        of_new.iter().all(|v| v
            .snapshot
            .as_ref()
            .is_some_and(|held| held.name == "Showman")),
        "and every version it does hold is its own: {of_new:?}"
    );
}

/// **A fold's survivor that was renamed, with its old handle taken by a new thing,
/// is still named by the handle it wears now.** The folded thing's history says
/// where it went; after the survivor's old handle is reused it must not name the
/// thing that wears that handle today.
#[tokio::test]
async fn a_fold_names_its_survivor_by_the_handle_it_wears_after_the_old_handle_is_reused() {
    let (_scratch, mut store, _pool, memory) = store_with_old_rows("survivor").await;
    let folded = EntityId("person:krusty".into());
    let survivor = EntityId("person:homer".into());
    make(&memory, "person:krusty", "Clown").await;
    make(&memory, "person:homer", "Dad").await;
    memory
        .merge(
            &folded,
            &survivor,
            Some("filed twice"),
            jiff::civil::date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            Some("run-a"),
        )
        .await
        .expect("the fold lands");
    let renamed = EntityId("person:barney-gumble".into());
    memory
        .rename_entity(
            &survivor,
            &renamed,
            None,
            jiff::civil::date(2026, 9, 3),
            jojobot_domain::memory::Override::none(),
            Some("rn02"),
        )
        .await
        .expect("a rename of the survivor")
        .written()
        .expect("nothing collides");
    make(&memory, "person:homer", "Father").await;

    let history = memory.entity_history(&folded).await.expect("history");
    store.stop().await;

    let last = history
        .iter()
        .rev()
        .find_map(|v| v.snapshot.as_ref().and_then(|s| s.merged_into.clone()))
        .expect("a version records where the folded thing went");
    assert_eq!(
        last, renamed,
        "the survivor is named by the handle it wears now, not by the one a new thing took: {history:?}"
    );
}

/// **A thing's version counts only the writes it made, after its handle is freed by
/// a rename and taken by a new thing.** The first krusty is renamed and a second
/// krusty is made under the freed handle. The new thing's version is its own one
/// write, and not that write plus the one the renamed thing filed under the
/// handle. Paired: the renamed thing's own version is what it was before the new
/// thing came.
#[tokio::test]
async fn a_version_counts_only_the_writes_of_the_thing_after_its_handle_is_reused() {
    let (_scratch, mut store, _pool, memory) = store_with_old_rows("version-reused").await;
    let krusty = EntityId("person:krusty".into());
    let renamed = EntityId("person:barney-gumble".into());
    make(&memory, "person:krusty", "Clown").await;
    memory
        .rename_entity(
            &krusty,
            &renamed,
            None,
            jiff::civil::date(2026, 6, 1),
            jojobot_domain::memory::Override::none(),
            Some("rn01"),
        )
        .await
        .expect("a rename of the handle")
        .written()
        .expect("nothing collides");
    let before = memory.fields_versioned(&renamed).await.expect("version").1;

    make(&memory, "person:krusty", "Showman").await;
    let of_new = memory.fields_versioned(&krusty).await.expect("version").1;
    let of_renamed = memory.fields_versioned(&renamed).await.expect("version").1;
    store.stop().await;

    assert_eq!(
        of_renamed, before,
        "the renamed thing keeps the version it had: nothing the new thing wrote is its own"
    );
    assert_eq!(
        of_new, 1,
        "the new thing made one write, and the renamed thing's earlier ones are not its own"
    );
}

/// **The same, for versions an older build filed.** Those name no badge, so the
/// former-handle table decides: the renamed thing left the handle and owns them;
/// the thing that wears it now does not.
#[tokio::test]
async fn a_version_counts_the_writes_an_older_build_filed_for_the_thing_that_left_the_handle() {
    let (_scratch, mut store, pool, memory) = store_with_old_rows("version-old").await;
    let krusty = EntityId("person:krusty".into());
    let renamed = EntityId("person:barney-gumble".into());
    make(&memory, "person:krusty", "Clown").await;
    sqlx::query("UPDATE entity_write SET name = NULL, kind = NULL, source = NULL, boot = NULL, aliases = NULL, prose = NULL, badge = NULL, session = NULL WHERE entity = 'person:krusty'")
        .execute(&pool)
        .await
        .expect("the version is stripped to the old shape");
    memory
        .rename_entity(
            &krusty,
            &renamed,
            None,
            jiff::civil::date(2026, 6, 1),
            jojobot_domain::memory::Override::none(),
            Some("rn01"),
        )
        .await
        .expect("a rename of the handle")
        .written()
        .expect("nothing collides");
    let before = memory.fields_versioned(&renamed).await.expect("version").1;
    make(&memory, "person:krusty", "Showman").await;
    let of_new = memory.fields_versioned(&krusty).await.expect("version").1;
    let of_renamed = memory.fields_versioned(&renamed).await.expect("version").1;
    store.stop().await;

    assert_eq!(
        of_renamed, before,
        "the renamed thing still counts the old version, and nothing more"
    );
    assert!(
        before >= 2,
        "the renamed thing counts the old version beside its rename: {before}"
    );
    assert_eq!(
        of_new, 1,
        "the new thing does not count a version an older build filed for the thing that left"
    );
}

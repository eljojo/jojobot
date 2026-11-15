//! **After an entity writer commits, the claims State answers what the store
//! answers for the touched thing.** The oracle is the claims import: it reads the
//! store's rows and says what object each row is, by a path of its own (the full
//! reader, not the single-row read a writer emits from). Each case runs the writer
//! inside a writer job, so its events join the transaction's one batch, and then
//! compares the State the seam holds with the import's reading of the store.

use jiff::civil::date;
use jojobot_domain::log::ObjectId;
use jojobot_domain::log::claims::import::object_body;
use jojobot_domain::memory::{EntityId, EntityPatch, Memory, NewEntity, Override};

use super::DoltMemory;
use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, imported_database, migrated_database};
use crate::writer::Writer;

struct World {
    _scratch: Scratch,
    server: Dolt,
    memory: DoltMemory,
    store: sqlx::MySqlPool,
    writer: Writer,
}

async fn world(name: &str) -> World {
    world_over(name, true).await
}

/// A world over a database with a completed claims import, or without one for a
/// case that runs the import itself.
async fn world_over(name: &str, imported: bool) -> World {
    let scratch = Scratch::new(name);
    let server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = match imported {
        true => imported_database(&server, "alpha").await.unwrap(),
        false => migrated_database(&server, "alpha").await.unwrap(),
    };
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(store.clone()))
        .await
        .expect("the kinds are seeded");
    World {
        _scratch: scratch,
        server,
        memory: DoltMemory::open(store.clone()).journaled(),
        store,
        writer: Writer::default(),
    }
}

impl World {
    /// Run a write as a job of the writer, so it joins the log.
    async fn job<T: Send + 'static>(
        &self,
        work: impl std::future::Future<Output = T> + Send + 'static,
    ) -> T {
        self.writer.run(work).await
    }

    /// **The store's reading of `handle` by the import, and the State's reading of
    /// the same thing**, as `(from the store, from the State)`.
    async fn both(
        &self,
        handle: &str,
    ) -> (
        Option<jojobot_domain::log::claims::ObjectBody>,
        Option<jojobot_domain::log::claims::ObjectBody>,
    ) {
        let rows = self.memory.import_rows().await.expect("the import reads");
        let row = rows.rows.entities.iter().find(|row| row.handle == handle);
        let from_store = row.map(object_body);
        let seam = self
            .writer
            .seam()
            .expect("the seam opened with the first begin");
        let from_state = row.and_then(|row| {
            seam.read_claims(|state| {
                // The State finds the thing by the spelling it answers to now.
                let key = state.resolve(handle)?;
                assert_eq!(
                    key,
                    &ObjectId(row.key.0.clone()),
                    "{handle} answers to its key"
                );
                state.object(key).cloned()
            })
        });
        (from_store, from_state)
    }

    async fn agree(&self, handle: &str) {
        let (store, state) = self.both(handle).await;
        assert!(store.is_some(), "the store holds {handle}");
        assert_eq!(
            store, state,
            "the State answers what the store answers for {handle}"
        );
    }
}

fn id(handle: &str) -> EntityId {
    EntityId(handle.to_string())
}

/// **A creation, a metadata edit, an archive, a restore and a prose write each
/// leave the State agreeing with the store.**
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_entity_writers_leave_the_claims_state_agreeing_with_the_store() {
    let mut world = world("entity-events-writers").await;
    let (memory, homer) = (world.memory.clone(), id("person:homer"));

    let made = {
        let memory = memory.clone();
        let new = NewEntity::new(homer.clone(), "Homer", "fixture");
        world.job(async move { memory.add_entity(new).await }).await
    };
    made.expect("add").written().expect("not blocked");
    world.agree("person:homer").await;

    let patch = EntityPatch {
        name: Some("Homer Simpson".into()),
        aliases: Some(vec!["Homie".into()]),
        ..EntityPatch::default()
    };
    {
        let (memory, homer) = (memory.clone(), homer.clone());
        world
            .job(async move { memory.update_entity(&homer, patch).await })
            .await
            .expect("update")
            .written()
            .expect("not blocked");
    }
    world.agree("person:homer").await;

    {
        let (memory, homer) = (memory.clone(), homer.clone());
        world
            .job(async move { memory.archive_entity(&homer, "done", None).await })
            .await
            .expect("archive");
    }
    world.agree("person:homer").await;
    let (_, archived) = world.both("person:homer").await;
    assert!(
        archived.expect("held").archived,
        "the State holds the archive"
    );

    {
        let (memory, homer) = (memory.clone(), homer.clone());
        world
            .job(async move { memory.restore_entity(&homer, None).await })
            .await
            .expect("restore");
    }
    world.agree("person:homer").await;

    {
        let (memory, homer) = (memory.clone(), homer.clone());
        world
            .job(async move { memory.set_prose(&homer, "Lives next door.", None).await })
            .await
            .expect("prose");
    }
    world.agree("person:homer").await;
    let (_, prose) = world.both("person:homer").await;
    assert_eq!(
        prose
            .expect("held")
            .metadata
            .get("prose")
            .map(String::as_str),
        Some("Lives next door."),
        "the prose is in the State"
    );
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A boot row repair never emits**, even run as a job of the writer. An early
/// emission would make the claims import skip, because it runs only while the log
/// holds no claims event, and the history would be lost. The pass here gives a row
/// to a handle that has claims filed under it and no row, through the same row
/// writer the entity writers use, and the log must still hold no claims member.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_row_repair_emits_no_claims_event() {
    let mut world = world("entity-events-repair").await;
    let handle = "topic:contract-docket";
    sqlx::query("INSERT INTO supplied_handle (handle) VALUES (?)")
        .bind(handle)
        .execute(&world.store)
        .await
        .expect("the handle was once supplied by a build");
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status, \
         recorded_at, edge_shape, edge_object, event_kind, derived_from, derived_from_id, \
         inserted_at, stale_after, happened_at, happened_through) \
         VALUES (?, 'f1', 'filed under a handle with no row', NULL, 'testimony', 'settled', \
         'active', '2026-10-08', NULL, NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL, \
         NULL, NULL)",
    )
    .bind(handle)
    .execute(&world.store)
    .await
    .expect("a claim filed under the handle");
    // **Asked inside the job**, because a pass commits natively and never seals
    // what it emitted: a pass that emitted would leave its events pending in the
    // job's context, where the log cannot show them.
    let (report, pending) = {
        let memory = world.memory.clone();
        world
            .job(async move {
                let report = memory.adopt_handles_without_a_row().await;
                (report, crate::writer::sql::take_emitted())
            })
            .await
    };
    assert_eq!(
        report.expect("the pass runs").adopted,
        vec![id(handle)],
        "the pass did its work"
    );
    assert!(pending.is_empty(), "the pass emitted: {pending:?}");
    let members: usize = world
        .writer
        .shared_log(&world.store)
        .read_from(jojobot_domain::log::Position(1), 100)
        .await
        .expect("the log reads")
        .iter()
        .map(|batch| batch.members().len())
        .sum();
    assert_eq!(members, 0, "the repair put nothing in the log");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A rename retires the old spelling and binds the new one**, and the object is
/// the same one under both.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rename_moves_the_spelling_and_keeps_the_object() {
    let mut world = world("entity-events-rename").await;
    let (memory, from, to) = (
        world.memory.clone(),
        id("person:homer"),
        id("person:homer-simpson"),
    );
    {
        let (memory, new) = (
            memory.clone(),
            NewEntity::new(from.clone(), "Homer", "fixture"),
        );
        world
            .job(async move { memory.add_entity(new).await })
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let key = world
        .writer
        .seam()
        .expect("the seam is open")
        .read_claims(|state| state.resolve("person:homer").cloned())
        .expect("the creation bound its spelling");
    {
        let (memory, from, to) = (memory.clone(), from.clone(), to.clone());
        world
            .job(async move {
                memory
                    .rename_entity(&from, &to, None, date(2026, 11, 5), Override::none(), None)
                    .await
            })
            .await
            .expect("rename")
            .written()
            .expect("not blocked");
    }
    world.agree("person:homer-simpson").await;
    let seam = world.writer.seam().expect("the seam is open");
    assert_eq!(
        seam.read_claims(|state| state.resolve("person:homer-simpson").cloned()),
        Some(key),
        "the new spelling answers to the same object"
    );
    assert_eq!(
        seam.read_claims(|state| state.resolve("person:homer").cloned()),
        None,
        "the old spelling no longer answers"
    );
    // **The retirement carries the day the rename wrote to the former-handle
    // row**, read from the log's own member and from the row.
    let stored: String = sqlx::query_scalar(
        "SELECT changed_at FROM entity_former_handle WHERE former_handle = 'person:homer'",
    )
    .fetch_one(&world.store)
    .await
    .expect("the former handle row");
    let days: Vec<String> = world
        .writer
        .shared_log(&world.store)
        .read_from(jojobot_domain::log::Position(0), 100)
        .await
        .expect("the log reads")
        .iter()
        .flat_map(|batch| batch.members().to_vec())
        .filter_map(|member| crate::claims_log::codec::decode_own(&member).ok().flatten())
        .filter_map(|event| match event.payload {
            jojobot_domain::log::claims::ClaimEvent::AddressRetired { day, .. } => {
                Some(day.to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(days, vec![stored], "the log holds the row's day");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A type declaration is an entity write too: it emits `TypeDeclared`, and only
/// when the declaration differs from the one the State holds.** The first
/// declaration and a changed one are events; declaring the same set again adds
/// none, so a boot that seeds the shipped types over a log that holds them leaves
/// the log as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_type_declaration_emits_only_when_it_differs_from_the_state() {
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    let mut world = world("entity-events-declare").await;
    let declared = |keys: &[&str]| {
        DeclaredType::new(
            "pledge",
            keys.iter()
                .map(|key| Field::new(key, ValueType::Text))
                .collect(),
        )
    };
    let declare = |keys: &'static [&'static str]| {
        let (memory, declaration) = (world.memory.clone(), declared(keys));
        world.job(async move { memory.declare_type(declaration, None).await })
    };
    declare(&["witness"]).await.expect("declared");
    declare(&["witness"]).await.expect("declared again");
    declare(&["witness", "date"]).await.expect("changed");

    let held = world
        .writer
        .seam()
        .expect("the seam is open")
        .read_claims(|state| state.declarations_of(&ObjectId("pledge".into())).to_vec());
    assert_eq!(
        held,
        vec![
            declared(&["witness"]).normalized(),
            declared(&["witness", "date"]).normalized()
        ],
        "one event for the first declaration, none for the repeat, one for the change"
    );
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A type the import already holds is declared again with no event.** The seam is
/// open before the import runs, as the startup passes leave it, so the State only
/// knows the import's type when the import has told the seam to read its batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_type_the_import_holds_is_declared_again_with_no_event() {
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    let mut world = world_over("entity-events-declare-imported", false).await;
    let declaration = DeclaredType::new("pledge", vec![Field::new("witness", ValueType::Text)]);
    let pool = crate::writer::sql::Pool::from(world.store.clone());
    {
        let (opener, pool) = (world.writer.clone(), pool.clone());
        world
            .writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens");
    }
    // A row of the old tables, written by a bare store: it emits no event.
    DoltMemory::open(world.store.clone())
        .declare_type(declaration.clone(), None)
        .await
        .expect("declared on the bare store");
    let imported = crate::claims_import::import_claims_once(
        &world.writer,
        &world.store,
        &jojobot_domain::memory::owned::Provisions::default(),
        jiff::Timestamp::from_second(1_700_000_000).expect("a moment"),
    )
    .await
    .expect("the import runs");
    assert!(
        matches!(imported, crate::claims_import::Imported::Done { .. }),
        "the import ran and did not find an earlier one: {imported:?}"
    );
    let seam = world.writer.seam().expect("the seam is open");
    assert!(
        seam.read_claims(|state| state
            .declared_types()
            .iter()
            .any(|held| held.name == "pledge")),
        "the State holds the imported declaration before the repeat"
    );
    let (memory, again) = (world.memory.clone(), declaration.clone());
    world
        .job(async move { memory.declare_type(again, None).await })
        .await
        .expect("declared again through the served store");

    // Counted in the log itself: the State folds a repeated declaration into one.
    let declared: usize = world
        .writer
        .shared_log(&world.store)
        .read_from(jojobot_domain::log::Position(0), 100)
        .await
        .expect("the log reads")
        .iter()
        .flat_map(|batch| batch.members().to_vec())
        .filter_map(|member| crate::claims_log::codec::decode_own(&member).ok().flatten())
        .filter(|event| {
            matches!(
                &event.payload,
                jojobot_domain::log::claims::ClaimEvent::TypeDeclared { kind, .. }
                    if kind == &ObjectId("pledge".into())
            )
        })
        .count();
    assert_eq!(
        declared, 1,
        "the import's declaration stands alone: the repeat emitted nothing"
    );
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A kind declaration emits `TypeDeclared` like a type declaration does**, and
/// only when it differs from what the State holds: the same kind declared twice
/// emits once. Reclaiming a shipped kind takes its keys away, and the State holds
/// the kind with none.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_kind_declaration_and_its_reclaim_emit_only_what_changes() {
    use jojobot_domain::memory::types::{Field, Origin, ValueType};

    let mut world = world("entity-events-kinds").await;
    let keys = |names: &[&str]| -> Vec<Field> {
        names
            .iter()
            .map(|key| Field::new(key, ValueType::Text))
            .collect()
    };
    for names in [&["witness"][..], &["witness"][..], &["witness", "date"][..]] {
        let (memory, fields) = (world.memory.clone(), keys(names));
        world
            .job(async move {
                memory
                    .declare_kind("pledgekind", Origin::Declared, fields)
                    .await
            })
            .await
            .expect("declared");
    }
    let held = |world: &World| {
        world
            .writer
            .seam()
            .expect("the seam is open")
            .read_claims(|state| {
                state
                    .declarations_of(&ObjectId("pledgekind".into()))
                    .iter()
                    .map(|declared| declared.fields.len())
                    .collect::<Vec<_>>()
            })
    };
    assert_eq!(
        held(&world),
        vec![1, 2],
        "one event for the first declaration, none for the repeat, one for the change"
    );
    {
        let (memory, fields) = (world.memory.clone(), keys(&["shelf"]));
        world
            .job(async move {
                memory
                    .declare_kind("stockkind", Origin::Shipped, fields)
                    .await
            })
            .await
            .expect("a shipped kind");
        let memory = world.memory.clone();
        world
            .job(async move { memory.reclaim_kind("stockkind").await })
            .await
            .expect("reclaimed");
    }
    let stock = world
        .writer
        .seam()
        .expect("the seam is open")
        .read_claims(|state| state.declared(&ObjectId("stockkind".into())).cloned());
    assert_eq!(stock, None, "the reclaimed kind is held no more");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **After a reclaim the State holds the declaration exactly as the store's rows
/// hold it**, and a reclaim that changes nothing appends nothing. The parity is
/// asked of the rows by the read that serves declared types, not by what the
/// writer meant to emit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reclaim_leaves_the_state_as_the_rows_hold_the_type() {
    use jojobot_domain::memory::types::{Field, Origin, ValueType};

    let mut world = world("entity-events-reclaim").await;
    let token = "stockkind";
    {
        let (memory, fields) = (
            world.memory.clone(),
            vec![Field::new("shelf", ValueType::Text)],
        );
        world
            .job(async move { memory.declare_kind(token, Origin::Shipped, fields).await })
            .await
            .expect("a shipped kind");
    }
    for _ in 0..2 {
        let memory = world.memory.clone();
        world
            .job(async move { memory.reclaim_kind(token).await })
            .await
            .expect("reclaimed");
    }
    let rows = world
        .memory
        .declared_types()
        .await
        .expect("the rows read")
        .into_iter()
        .find(|declared| declared.name == token);
    let held = world
        .writer
        .seam()
        .expect("the seam is open")
        .read_claims(|state| state.declared(&ObjectId(token.into())).cloned());
    assert_eq!(held, rows, "the State holds the type as the rows do");
    // **Counted in the log, which the State cannot show**: a withdrawal clears the
    // State, so a repeated reclaim that appended another leaves it unchanged.
    let withdrawn = world
        .writer
        .shared_log(&world.store)
        .read_from(jojobot_domain::log::Position(0), 100)
        .await
        .expect("the log reads")
        .iter()
        .flat_map(|batch| batch.members().to_vec())
        .filter_map(|member| crate::claims_log::codec::decode_own(&member).ok().flatten())
        .filter(|event| {
            matches!(
                &event.payload,
                jojobot_domain::log::claims::ClaimEvent::TypeWithdrawn { kind } if kind.0 == token
            )
        })
        .count();
    assert_eq!(withdrawn, 1, "an unchanged reclaim appends nothing");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **After a caller adds a key to a shipped kind, the State holds the kind's whole
/// key list as the rows do.** The declaration is built from what the rows keep once
/// the plan has landed, not from the keys the caller sent: a caller's key beside
/// the build's own is one declaration of two keys.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_key_added_to_a_shipped_kind_is_held_with_the_keys_the_kind_keeps() {
    use jojobot_domain::memory::types::{Field, Origin, ValueType};

    let mut world = world("entity-events-kind-keys").await;
    let token = "stockkind";
    for (origin, key) in [(Origin::Shipped, "witness"), (Origin::Declared, "date")] {
        let (memory, fields) = (world.memory.clone(), vec![Field::new(key, ValueType::Text)]);
        world
            .job(async move { memory.declare_kind(token, origin, fields).await })
            .await
            .expect("declared");
    }
    let rows = world
        .memory
        .declared_types()
        .await
        .expect("the rows read")
        .into_iter()
        .find(|declared| declared.name == token);
    let held = world
        .writer
        .seam()
        .expect("the seam is open")
        .read_claims(|state| state.declared(&ObjectId(token.into())).cloned());
    assert_eq!(held, rows, "the State holds the kind as the rows do");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A kind declared with no keys emits nothing**: the rows keep no key for it, so
/// the State is to hold no declaration, and the log gets no `TypeDeclared`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_kind_declared_with_no_keys_emits_nothing() {
    use jojobot_domain::memory::types::Origin;

    let mut world = world("entity-events-kind-no-keys").await;
    let token = "emptykind";
    {
        let memory = world.memory.clone();
        world
            .job(async move {
                memory
                    .declare_kind(token, Origin::Declared, Vec::new())
                    .await
            })
            .await
            .expect("declared");
    }
    let held = world
        .writer
        .seam()
        .expect("the seam is open")
        .read_claims(|state| state.declared(&ObjectId(token.into())).cloned());
    assert_eq!(held, None, "the State holds a declaration the rows do not");
    let declared_in_log = world
        .writer
        .shared_log(&world.store)
        .read_from(jojobot_domain::log::Position(0), 100)
        .await
        .expect("the log reads")
        .iter()
        .flat_map(|batch| batch.members().to_vec())
        .filter_map(|member| crate::claims_log::codec::decode_own(&member).ok().flatten())
        .filter(|event| {
            matches!(
                &event.payload,
                jojobot_domain::log::claims::ClaimEvent::TypeDeclared { kind, .. } if kind.0 == token
            )
        })
        .count();
    assert_eq!(declared_in_log, 0, "a declaration without keys was emitted");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A declaration the rows already hold still brings the State to them.** The rows
/// are written outside a writer job, as the boot seed writes them, so the State
/// holds nothing; the same declaration made inside a job changes no row (the plan
/// is the rows) and must still emit what the rows hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_declaration_the_rows_already_hold_still_reaches_the_state() {
    use jojobot_domain::memory::types::{Field, Origin, ValueType};

    let mut world = world("entity-events-kind-held").await;
    let token = "stockkind";
    let keys = || vec![Field::new("witness", ValueType::Text)];
    // **The seed's store is bare**: it writes its rows and emits nothing.
    DoltMemory::open(world.store.clone())
        .declare_kind(token, Origin::Shipped, keys())
        .await
        .expect("seeded outside a job");
    let before = world
        .writer
        .seam()
        .map(|seam| seam.read_claims(|state| state.declared(&ObjectId(token.into())).cloned()));
    assert_ne!(
        before.flatten().map(|declared| declared.fields.len()),
        Some(1),
        "the seed outside a job already reached the State"
    );
    {
        let (memory, fields) = (world.memory.clone(), keys());
        world
            .job(async move { memory.declare_kind(token, Origin::Shipped, fields).await })
            .await
            .expect("declared again");
    }
    let rows = world
        .memory
        .declared_types()
        .await
        .expect("the rows read")
        .into_iter()
        .find(|declared| declared.name == token);
    let held = world
        .writer
        .seam()
        .expect("the seam is open")
        .read_claims(|state| state.declared(&ObjectId(token.into())).cloned());
    assert_eq!(held, rows, "the State holds the kind as the rows do");
    assert!(rows.is_some(), "the rows hold the kind");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A journaled store refuses an entity write that reaches it with no writer
/// job**, and the refused write leaves no row: the transaction is dropped. The
/// store the server serves is journaled, so no write of it is lost without a word.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_journaled_store_refuses_an_entity_write_with_no_writer_job() {
    let mut world = world("entity-events-journaled-refuses").await;
    let handle = id("person:homer");
    let refused = world
        .memory
        .add_entity(NewEntity::new(handle.clone(), "Homer", "fixture"))
        .await;
    assert!(refused.is_err(), "the write went through with no job");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE id = ?")
        .bind(handle.as_str())
        .fetch_one(&world.store)
        .await
        .expect("the count reads");
    assert_eq!(rows, 0, "the refused write left a row");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A bare store writes the same entity and emits nothing**: it is what the
/// boot passes, the seeds and the direct-store cases use, and the log stays as
/// it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bare_store_commits_an_entity_write_and_emits_nothing() {
    let mut world = world("entity-events-bare-commits").await;
    let handle = id("person:homer");
    DoltMemory::open(world.store.clone())
        .add_entity(NewEntity::new(handle.clone(), "Homer", "fixture"))
        .await
        .expect("a bare store takes the write")
        .written()
        .expect("not blocked");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE id = ?")
        .bind(handle.as_str())
        .fetch_one(&world.store)
        .await
        .expect("the count reads");
    assert_eq!(rows, 1, "the bare store did not commit");
    let members: usize = world
        .writer
        .shared_log(&world.store)
        .read_from(jojobot_domain::log::Position(1), 100)
        .await
        .expect("the log reads")
        .iter()
        .map(|batch| batch.members().len())
        .sum();
    assert_eq!(members, 0, "the bare store put something in the log");
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A write made after a merge, through the verbs, leaves what the State holds of each
/// thing what a fold of its writes makes**: a claim written under the folded handle, a
/// claim the merge moved revised under the address it had, and a claim written on the
/// survivor. A thing's newest change is always a moment its own writes make.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_write_after_a_merge_leaves_the_held_fields_what_a_fold_makes() {
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{FactPatch, NewFact};
    use std::collections::BTreeMap;

    let mut world = world("folded-write-newest").await;
    let (memory, alpha, beta) = (world.memory.clone(), id("person:alpha"), id("person:beta"));
    for handle in [&alpha, &beta] {
        let (memory, new) = (
            memory.clone(),
            NewEntity::new(handle.clone(), handle.as_str(), "fixture"),
        );
        world
            .job(async move { memory.add_entity(new).await })
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let mood = |handle: &EntityId, claim: &str, value: &str, day: i8| {
        state(NewFact {
            fields: BTreeMap::from([("mood".to_string(), value.to_string())]),
            ..NewFact::about(handle.clone(), claim, date(2026, 9, day))
        })
    };
    let check = |world: &World, after: &str| {
        let seam = world.writer.seam().expect("the seam opened");
        if let Err(differs) = seam.read_claims(|state| state.held_fields_are_what_a_fold_makes()) {
            panic!("after {after}: {differs}");
        }
    };
    let mut written = Vec::new();
    for (handle, claim, value, day) in [
        (&alpha, "alpha is calm", "calm", 1),
        (&beta, "beta is tense", "tense", 2),
    ] {
        let (memory, fact) = (memory.clone(), mood(handle, claim, value, day));
        written.push(
            world
                .job(async move { memory.capture(fact).await })
                .await
                .expect("capture")
                .written()
                .expect("not blocked"),
        );
    }
    check(&world, "the captures");
    // **What the State holds of the survivor's `mood`**, so a write that never reached the
    // State shows: the state is read for the value, the stream and the newest moment.
    let mood_of_alpha = |world: &World| {
        let seam = world.writer.seam().expect("the seam opened");
        seam.read_claims(|state| {
            let object = state
                .resolve(alpha.as_str())
                .cloned()
                .expect("alpha is held");
            let declared = state.declared_types_shared();
            (
                state.fields(&object, &declared).get("mood").cloned(),
                state
                    .key_trails(&object)
                    .history
                    .get("mood")
                    .map_or(0, Vec::len),
                state.newest_change(&object),
            )
        })
    };
    let (_, writes_before_merge, _) = mood_of_alpha(&world);
    assert_eq!(writes_before_merge, 1, "alpha wrote one mood");

    {
        let (memory, beta, alpha) = (memory.clone(), beta.clone(), alpha.clone());
        world
            .job(async move {
                memory
                    .merge(
                        &beta,
                        &alpha,
                        Some("filed twice"),
                        date(2026, 9, 3),
                        &EntityId(jojobot_domain::memory::ASSISTANT.into()),
                        Some("run-a"),
                    )
                    .await
            })
            .await
            .expect("the merge lands");
    }
    check(&world, "the merge");
    let (held_mood, writes_after_merge, newest_after_merge) = mood_of_alpha(&world);
    assert_eq!(
        writes_after_merge, 2,
        "the folded thing's mood joined alpha's stream"
    );
    assert_eq!(
        held_mood.as_deref(),
        Some("tense"),
        "the folded write is the newer"
    );

    // A claim written under the handle that was folded away.
    let late = {
        let (memory, fact) = (memory.clone(), mood(&beta, "beta late", "late", 4));
        world.job(async move { memory.capture(fact).await }).await
    };
    assert!(
        matches!(late, Ok(jojobot_domain::memory::Guarded::Written(_))),
        "a write under the folded handle lands: {late:?}"
    );
    check(&world, "a write to the folded handle");
    let (held_mood, writes, newest) = mood_of_alpha(&world);
    assert_eq!(
        writes, 3,
        "the write under the folded handle is in alpha's stream"
    );
    assert_eq!(
        held_mood.as_deref(),
        Some("late"),
        "alpha holds the late mood"
    );
    assert!(
        newest > newest_after_merge,
        "the write moved alpha's newest change"
    );

    // The claim the merge moved, revised under the address it had before the merge.
    let moved = written[1].address();
    let revised = {
        let (memory, moved) = (memory.clone(), moved.clone());
        world
            .job(async move {
                memory
                    .update_fact(
                        &moved,
                        FactPatch {
                            fields: BTreeMap::from([("mood".to_string(), "cross".to_string())]),
                            sets: ["mood".to_string()].into(),
                            ..FactPatch::default()
                        },
                        &EntityId(jojobot_domain::memory::ASSISTANT.into()),
                    )
                    .await
            })
            .await
    };
    assert!(
        matches!(revised, Ok(jojobot_domain::memory::Guarded::Written(_))),
        "a revision of the moved claim under its old address lands: {revised:?}"
    );
    check(&world, "a revision of the moved claim");
    let (held_mood, writes, _) = mood_of_alpha(&world);
    assert_eq!(writes, 4, "the revision is a write of alpha's stream");
    assert_eq!(
        held_mood.as_deref(),
        Some("cross"),
        "alpha holds the revised mood"
    );

    // A claim written on the survivor.
    {
        let (memory, fact) = (memory.clone(), mood(&alpha, "alpha again", "steady", 5));
        world
            .job(async move { memory.capture(fact).await })
            .await
            .expect("capture")
            .written()
            .expect("not blocked");
    }
    check(&world, "a write on the survivor");
    let (held_mood, writes, _) = mood_of_alpha(&world);
    assert_eq!(writes, 5, "the survivor's own write is in its stream");
    assert_eq!(
        held_mood.as_deref(),
        Some("steady"),
        "alpha holds its latest mood"
    );
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A claim revised after its thing was folded into another sixty-five times lands under the
/// last survivor.** The emitter followed at most sixty-four merges and stopped at the
/// sixty-fifth thing, so the write named a thing that was itself folded away; the State put the
/// stream under that name and counted the moment on the survivor, and what it held was not what
/// a fold of the writes makes. A chain of merges has no length limit, and the walk has none.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_revised_after_sixty_five_merges_lands_under_the_last_survivor() {
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{FactPatch, Guarded, NewFact};
    use std::collections::BTreeMap;

    const MERGES: usize = 65;
    let mut world = world("folded-write-chain").await;
    let memory = world.memory.clone();
    let letters = |n: usize| {
        format!(
            "{}{}",
            (b'a' + (n / 26) as u8) as char,
            (b'a' + (n % 26) as u8) as char
        )
    };
    let things: Vec<EntityId> = (0..=MERGES)
        .map(|n| id(&format!("person:{}", letters(n))))
        .collect();
    for (n, handle) in things.iter().enumerate() {
        let (memory, mut new) = (
            memory.clone(),
            NewEntity::new(handle.clone(), letters(n), "fixture"),
        );
        let first = {
            let (memory, new) = (memory.clone(), new.clone());
            world.job(async move { memory.add_entity(new).await }).await
        };
        match first.expect("add") {
            Guarded::Written(_) => {}
            Guarded::Blocked {
                attempted,
                candidates,
            } => {
                new.override_token = Some(jojobot_domain::memory::guard::override_token(
                    &attempted,
                    &candidates,
                ));
                new.override_reason = Some("a different thing".into());
                world
                    .job(async move { memory.add_entity(new).await })
                    .await
                    .expect("add")
                    .written()
                    .expect("the override lifts the refusal");
            }
        }
    }
    let held = {
        let (memory, fact) = (
            memory.clone(),
            state(NewFact {
                fields: BTreeMap::from([("mood".to_string(), "calm".to_string())]),
                ..NewFact::about(things[0].clone(), "calm at first", date(2026, 9, 1))
            }),
        );
        world
            .job(async move { memory.capture(fact).await })
            .await
            .expect("capture")
            .written()
            .expect("not blocked")
            .address()
    };
    for pair in things.windows(2) {
        let (memory, folded, survivor) = (memory.clone(), pair[0].clone(), pair[1].clone());
        world
            .job(async move {
                memory
                    .merge(
                        &folded,
                        &survivor,
                        Some("one more"),
                        date(2026, 9, 3),
                        &EntityId(jojobot_domain::memory::ASSISTANT.into()),
                        Some("run-a"),
                    )
                    .await
            })
            .await
            .expect("the merge lands");
    }
    let revised = {
        let memory = memory.clone();
        world
            .job(async move {
                memory
                    .update_fact(
                        &held,
                        FactPatch {
                            fields: BTreeMap::from([("mood".to_string(), "cross".to_string())]),
                            sets: ["mood".to_string()].into(),
                            ..FactPatch::default()
                        },
                        &EntityId(jojobot_domain::memory::ASSISTANT.into()),
                    )
                    .await
            })
            .await
    };
    let Ok(Guarded::Written(revised)) = revised else {
        panic!("the revision lands: {revised:?}");
    };
    // **The claim the State holds is served from the last survivor, under the label the
    // revision names**: the walk of where a claim is served follows the chain of merges to its
    // end, and the revision's claim body carries what it found. A walk that stopped early
    // names a thing that was itself folded away.
    let badge_of = |handle: EntityId| {
        let store = world.store.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT badge FROM entity WHERE id = ?")
                .bind(handle.as_str())
                .fetch_one(&store)
                .await
                .expect("the thing wears a badge")
        }
    };
    let (first_key, last_key) = (
        badge_of(things[0].clone()).await,
        badge_of(things[MERGES].clone()).await,
    );
    let seam = world.writer.seam().expect("the seam opened");
    let held_claim = seam.read_claims(|state| {
        let record = state.record_at_origin(&jojobot_domain::log::ObjectId(first_key), "f1")?;
        state
            .record(&record)
            .map(|held| (held.claim.home.clone(), held.claim.label.clone()))
    });
    assert_eq!(
        held_claim,
        Some((
            jojobot_domain::log::ObjectId(last_key),
            revised.id.as_str().to_string()
        )),
        "the revised claim is served from the last survivor under the label it names"
    );
    if let Err(differs) = seam.read_claims(|state| state.held_fields_are_what_a_fold_makes()) {
        panic!("after a revision past {MERGES} merges: {differs}");
    }
    let last = things[MERGES].clone();
    let mood = seam.read_claims(|state| {
        let object = state.resolve(last.as_str()).cloned()?;
        state
            .fields(&object, &state.declared_types_shared())
            .get("mood")
            .cloned()
    });
    assert_eq!(
        mood.as_deref(),
        Some("cross"),
        "the survivor holds the revised mood"
    );
    world.writer.drain().await;
    world.server.stop().await;
}

/// **A loop among the merges is refused, and nothing is committed.** The walks that find the last
/// survivor of a thing and where a claim is served follow the chain of merges to its end, whatever
/// its length, so a chain that comes back on itself has no end: the write is refused with a
/// storage error before it commits, whether the loop is a thing folded into itself, two things
/// folded into each other, or a claim served from itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_loop_among_the_merges_refuses_the_write_and_commits_nothing() {
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{FactPatch, NewFact};
    use std::collections::BTreeMap;

    let mut world = world("merge-loop").await;
    let memory = world.memory.clone();
    let (alpha, beta) = (id("person:alpha"), id("thing:omega"));
    for (handle, name) in [(&alpha, "Alpha"), (&beta, "Omega")] {
        let (memory, new) = (
            memory.clone(),
            NewEntity::new(handle.clone(), name, "fixture"),
        );
        world
            .job(async move { memory.add_entity(new).await })
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let held = {
        let (memory, fact) = (
            memory.clone(),
            state(NewFact::about(alpha.clone(), "one claim", date(2026, 9, 1))),
        );
        world
            .job(async move { memory.capture(fact).await })
            .await
            .expect("capture")
            .written()
            .expect("not blocked")
            .address()
    };
    let key_of = |handle: &EntityId| {
        let (store, handle) = (world.store.clone(), handle.clone());
        async move {
            sqlx::query_scalar::<_, String>("SELECT badge FROM entity WHERE id = ?")
                .bind(handle.as_str())
                .fetch_one(&store)
                .await
                .expect("the thing wears a badge")
        }
    };
    let (alpha_key, beta_key) = (key_of(&alpha).await, key_of(&beta).await);
    let rows = |store: sqlx::MySqlPool| async move {
        let facts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&store)
            .await
            .expect("count");
        let writes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_write")
            .fetch_one(&store)
            .await
            .expect("count");
        (facts, writes)
    };
    let loop_row = |store: sqlx::MySqlPool,
                    merge_no: i64,
                    ordinal: i64,
                    folded: String,
                    survivor: String,
                    folded_id: String,
                    survivor_id: String| async move {
        sqlx::query(
            "INSERT INTO merge_event \
             (merge_no, ordinal, folded, survivor, merged_at, session, folded_id, survivor_id) \
             VALUES (?, ?, ?, ?, '2026-09-03T00:00:00Z', NULL, ?, ?)",
        )
        .bind(merge_no)
        .bind(ordinal)
        .bind(folded)
        .bind(survivor)
        .bind(folded_id)
        .bind(survivor_id)
        .execute(&store)
        .await
        .expect("the loop is stored");
    };
    let write_on_alpha = |memory: DoltMemory, handle: EntityId| {
        let fact = state(NewFact {
            fields: BTreeMap::from([("mood".to_string(), "calm".to_string())]),
            ..NewFact::about(handle, "a write the loop refuses", date(2026, 9, 4))
        });
        async move { memory.capture(fact).await }
    };

    // A thing folded into itself, and two things folded into each other.
    for (name, pairs) in [
        ("a self-loop", vec![(alpha_key.clone(), alpha_key.clone())]),
        (
            "a loop of two",
            vec![
                (alpha_key.clone(), beta_key.clone()),
                (beta_key.clone(), alpha_key.clone()),
            ],
        ),
    ] {
        sqlx::query("DELETE FROM merge_event")
            .execute(&world.store)
            .await
            .expect("the merges are cleared");
        for (n, (folded, survivor)) in pairs.into_iter().enumerate() {
            loop_row(
                world.store.clone(),
                900 + n as i64,
                0,
                folded,
                survivor,
                String::new(),
                String::new(),
            )
            .await;
        }
        let before = rows(world.store.clone()).await;
        let refused = {
            let (memory, alpha) = (memory.clone(), alpha.clone());
            world.job(write_on_alpha(memory, alpha)).await
        };
        assert!(
            matches!(
                &refused,
                Err(jojobot_domain::memory::MemoryError::StoredDamage { .. })
            ),
            "{name} refuses the write as damage in the store, which no retry mends: {refused:?}"
        );
        // **The walk refuses on its own**, whatever answers first: it follows no loop.
        let mut tx = world.store.begin().await.expect("a transaction");
        let walked = DoltMemory::final_survivor(&mut tx, &alpha_key).await;
        assert!(
            matches!(&walked, Err(jojobot_domain::memory::MemoryError::StoredDamage { what }) if what.contains("loop")),
            "{name}: the walk of the last survivor refuses: {walked:?}"
        );
        let _ = tx.rollback().await;
        assert_eq!(
            rows(world.store.clone()).await,
            before,
            "{name} committed nothing"
        );
    }

    // A claim served from itself.
    sqlx::query("DELETE FROM merge_event")
        .execute(&world.store)
        .await
        .expect("the merges are cleared");
    loop_row(
        world.store.clone(),
        950,
        1,
        alpha_key.clone(),
        alpha_key.clone(),
        held.local.as_str().to_string(),
        held.local.as_str().to_string(),
    )
    .await;
    let before = rows(world.store.clone()).await;
    let refused = {
        let (memory, held) = (memory.clone(), held.clone());
        world
            .job(async move {
                memory
                    .update_fact(
                        &held,
                        FactPatch {
                            content: Some("a revision the loop refuses".into()),
                            ..FactPatch::default()
                        },
                        &EntityId(jojobot_domain::memory::ASSISTANT.into()),
                    )
                    .await
            })
            .await
    };
    assert!(
        matches!(
            &refused,
            Err(jojobot_domain::memory::MemoryError::StoredDamage { .. })
        ),
        "a claim served from itself refuses the revision as damage in the store: {refused:?}"
    );
    let mut tx = world.store.begin().await.expect("a transaction");
    let walked = DoltMemory::served_address(&mut tx, &alpha_key, held.local.as_str()).await;
    assert!(
        matches!(&walked, Err(jojobot_domain::memory::MemoryError::StoredDamage { what }) if what.contains("loop")),
        "the walk of the address refuses: {walked:?}"
    );
    let _ = tx.rollback().await;
    assert_eq!(
        rows(world.store.clone()).await,
        before,
        "the revision committed nothing"
    );

    // A claim served through a loop of two addresses. A walk that refuses only an address that
    // points at itself never ends here, so it is bounded: a walk that does not end is a failure.
    sqlx::query("DELETE FROM merge_event")
        .execute(&world.store)
        .await
        .expect("the merges are cleared");
    loop_row(
        world.store.clone(),
        960,
        1,
        alpha_key.clone(),
        beta_key.clone(),
        held.local.as_str().to_string(),
        "x1".to_string(),
    )
    .await;
    loop_row(
        world.store.clone(),
        961,
        1,
        beta_key.clone(),
        alpha_key.clone(),
        "x1".to_string(),
        held.local.as_str().to_string(),
    )
    .await;
    let mut tx = world.store.begin().await.expect("a transaction");
    let walked = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        DoltMemory::served_address(&mut tx, &alpha_key, held.local.as_str()),
    )
    .await
    .expect("the walk of a loop of two addresses ends");
    assert!(
        matches!(&walked, Err(jojobot_domain::memory::MemoryError::StoredDamage { what }) if what.contains("loop")),
        "the walk of a loop of two addresses refuses: {walked:?}"
    );
    let _ = tx.rollback().await;
    world.writer.drain().await;
    world.server.stop().await;
}

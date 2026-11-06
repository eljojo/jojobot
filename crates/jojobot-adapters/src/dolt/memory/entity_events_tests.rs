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
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;

struct World {
    _scratch: Scratch,
    server: Dolt,
    memory: DoltMemory,
    store: sqlx::MySqlPool,
    writer: Writer,
}

async fn world(name: &str) -> World {
    let scratch = Scratch::new(name);
    let server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(store.clone()))
        .await
        .expect("the kinds are seeded");
    World {
        _scratch: scratch,
        server,
        memory: DoltMemory::open(store.clone()),
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
        .read_from(jojobot_domain::log::Position(0), 100)
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

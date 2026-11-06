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
        memory: DoltMemory::open(store),
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
    world.writer.drain().await;
    world.server.stop().await;
}

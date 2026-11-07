//! **A capture decided over the claims State leaves the rows the old route
//! leaves.** The same facts are captured on two stores, one by the old route and
//! one through a capture job, and every row of the claim tables is compared once
//! the random keys are put back to the handles they stand for. The moments differ
//! by nature and are not compared.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_domain::actor::Actor;
use jojobot_domain::log::claims::ClaimSize;
use jojobot_domain::log::claims::job::{CaptureJob, within};
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityId, FactAddress, Guarded, Memory, NewEntity, NewFact,
};
use sqlx::Row;

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
    jojobot_domain::memory::kinds::load_shipped();
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

fn job() -> CaptureJob {
    job_as(Actor::Operator)
}

fn job_as(actor: Actor) -> CaptureJob {
    CaptureJob::new(
        &actor,
        None,
        Arc::new(|_, _| {
            Some(ClaimSize {
                size: 1,
                room: 100_000,
                row: None,
            })
        }),
        None,
    )
}

fn id(handle: &str) -> EntityId {
    EntityId(handle.to_string())
}

impl World {
    async fn add(&self, handle: &str, name: &str) {
        let (memory, new) = (
            self.memory.clone(),
            NewEntity::new(id(handle), name, "fixture"),
        );
        self.writer
            .run(async move { memory.add_entity(new).await })
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }

    /// Capture `fact`, by the old route or through a capture job.
    async fn capture(&self, fact: NewFact, through_the_job: bool) -> jojobot_domain::memory::Fact {
        self.capture_as(fact, through_the_job, Actor::Operator)
            .await
    }

    /// The same, as `actor`: the engine's own captures run under the system.
    async fn capture_as(
        &self,
        fact: NewFact,
        through_the_job: bool,
        actor: Actor,
    ) -> jojobot_domain::memory::Fact {
        let memory = self.memory.clone();
        let work = self.writer.run(async move { memory.capture(fact).await });
        let written = match through_the_job {
            true => within(job_as(actor), work).await,
            false => work.await,
        };
        match written.expect("capture") {
            Guarded::Written(fact) => fact,
            blocked => panic!("the capture was blocked: {blocked:?}"),
        }
    }

    /// **Every row of the claim tables**, the moments left out, with each random
    /// key put back to the handle it stands for.
    async fn rows(&self) -> Vec<String> {
        let keys: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, badge FROM entity")
            .fetch_all(&self.store)
            .await
            .expect("the keys read");
        let columns = |names: &[&str]| {
            names
                .iter()
                .map(|name| format!("COALESCE(CAST(`{name}` AS CHAR), '~')"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let tables: [(&str, &[&str], &str); 6] = [
            (
                "fact",
                &[
                    "entity",
                    "id",
                    "content",
                    "details",
                    "provenance",
                    "standing",
                    "status",
                    "recorded_at",
                    "happened_at",
                    "happened_through",
                    "edge_shape",
                    "edge_object",
                    "derived_from",
                    "derived_from_id",
                    "stale_after",
                    "receipt",
                    "app_stamp",
                ],
                "entity, id",
            ),
            (
                "fact_write",
                &[
                    "entity",
                    "fact_id",
                    "ordinal",
                    "content",
                    "details",
                    "provenance",
                    "standing",
                    "status",
                    "edge_shape",
                    "edge_object",
                    "derived_from",
                    "derived_from_id",
                    "session",
                    "refs",
                    "stands_for",
                    "receipt",
                    "app_stamp",
                    "bot",
                ],
                "entity, fact_id, ordinal",
            ),
            (
                "fact_event_ref",
                &["fact_home", "fact_id", "ordinal", "entity"],
                "fact_home, fact_id, ordinal",
            ),
            (
                "fact_stands_for",
                &[
                    "fact_home",
                    "fact_id",
                    "ordinal",
                    "source_home",
                    "source_id",
                ],
                "fact_home, fact_id, ordinal",
            ),
            (
                "field_write",
                &[
                    "entity",
                    "key",
                    "ordinal",
                    "value",
                    "fact_id",
                    "write_class",
                    "fact_write_ordinal",
                ],
                "entity, `key`, ordinal",
            ),
            (
                "field_link",
                &["entity", "key", "ordinal", "target"],
                "entity, `key`, ordinal",
            ),
        ];
        let mut out = Vec::new();
        for (table, names, order) in tables {
            let sql = format!(
                "SELECT CONCAT_WS(' | ', {}) FROM {table} ORDER BY {order}",
                columns(names)
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .fetch_all(&self.store)
                .await
                .unwrap_or_else(|error| panic!("{table}: {error}"));
            for row in rows {
                let mut line: String = row.get(0);
                for (handle, badge) in &keys {
                    if let Some(badge) = badge {
                        line = line.replace(badge.as_str(), handle);
                    }
                }
                out.push(format!("{table}: {line}"));
            }
        }
        // The tables are ordered by the random keys; the comparison is of the rows.
        out.sort();
        out
    }
}

/// The scenario both stores run: a claim with an edge, a ref, a setting that links
/// a thing, and a second claim derived from the first.
async fn run(world: &World, through_the_job: bool) {
    for (handle, name) in [
        ("person:homer", "Homer"),
        ("person:marge", "Marge"),
        ("person:bart", "Bart"),
    ] {
        world.add(handle, name).await;
    }
    let mut first = NewFact::about(
        id("person:homer"),
        "a claim with all of it",
        date(2026, 4, 19),
    );
    first.details = Some("and the reason".to_string());
    first.edge = Some(Edge {
        shape: EdgeShape::About,
        object: id("person:marge"),
    });
    first.refs = vec![id("person:bart")];
    first.fields = [
        ("note".to_string(), "kept on the claim".to_string()),
        ("friend".to_string(), "person:marge".to_string()),
    ]
    .into();
    first.sets = ["friend".to_string()].into();
    let first = world.capture(first, through_the_job).await;

    let mut second = NewFact::about(
        id("person:homer"),
        "a claim from the first",
        date(2026, 4, 20),
    );
    second.derived_from = Some(FactAddress::new(id("person:homer"), first.id.clone()));
    second.fields = [("mood".to_string(), "up".to_string())].into();
    second.sets = ["mood".to_string()].into();
    world.capture(second, through_the_job).await;

    // The engine's own record on a bot: a ledger key set on the thing and the
    // engine's mark on the claim.
    world.add("bot:lisa", "Lisa").await;
    let mut ledger = NewFact::about(
        id("bot:lisa"),
        "jojobot keeps here the release notes this bot has been told.",
        date(2026, 4, 21),
    );
    ledger.fields = [
        ("engine_record".to_string(), "release_notes".to_string()),
        (
            "release_notes_told".to_string(),
            "a-note, b-note".to_string(),
        ),
    ]
    .into();
    ledger.sets = ["release_notes_told".to_string()].into();
    world
        .capture_as(ledger, through_the_job, Actor::System("jojobot".into()))
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_capture_through_the_job_leaves_the_rows_the_old_route_leaves() {
    let mut old = world("row-writer-old").await;
    let mut new = world("row-writer-new").await;
    run(&old, false).await;
    run(&new, true).await;
    let (old_rows, new_rows) = (old.rows().await, new.rows().await);
    let folded = |world: &World| {
        let memory = world.memory.clone();
        async move {
            memory
                .fields(&id("bot:lisa"))
                .await
                .expect("the fold reads")
        }
    };
    assert_eq!(
        folded(&new).await,
        folded(&old).await,
        "the bot holds the same fields through either route"
    );
    assert!(old_rows.iter().any(|row| row.starts_with("fact: ")));
    assert_eq!(
        new_rows, old_rows,
        "the rows of the job route are the rows of the old route"
    );
    old.server.stop().await;
    new.server.stop().await;
}

/// **A claim decided over the State is read back by the row readers and edited by
/// `update_fact`**, which finds its claim with a row read until the reads move.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_decided_over_the_state_is_read_back_and_edited_through_the_rows() {
    let mut world = world("row-writer-read-back").await;
    for (handle, name) in [("person:homer", "Homer"), ("person:bart", "Bart")] {
        world.add(handle, name).await;
    }
    let mut fact = NewFact::about(
        id("person:homer"),
        "a claim read by its row",
        date(2026, 4, 19),
    );
    fact.fields = [("mood".to_string(), "up".to_string())].into();
    fact.sets = ["mood".to_string()].into();
    let from_state = world.capture(fact, true).await;

    let read = world
        .memory
        .recall(&id("person:homer"))
        .await
        .expect("the rows read");
    let from_rows = read
        .iter()
        .find(|held| held.id == from_state.id)
        .expect("the row reader finds the claim the State decided");
    assert_eq!(
        (
            &from_rows.content,
            from_rows.provenance,
            from_rows.standing,
            from_rows.status
        ),
        (
            &from_state.content,
            from_state.provenance,
            from_state.standing,
            from_state.status
        ),
    );
    assert_eq!(from_rows.recorded_at, from_state.recorded_at);
    assert_eq!(from_rows.home, from_state.home);

    let patch = jojobot_domain::memory::FactPatch {
        content: Some("edited through the rows".to_string()),
        ..Default::default()
    };
    let (memory, address, caller) = (
        world.memory.clone(),
        from_state.address(),
        id("person:bart"),
    );
    world
        .writer
        .run(async move { memory.update_fact(&address, patch, &caller).await })
        .await
        .expect("the claim is found by its row and edited")
        .written()
        .expect("nothing blocks the edit");
    let after = world
        .memory
        .recall(&id("person:homer"))
        .await
        .expect("the rows read");
    assert!(
        after
            .iter()
            .any(|held| held.id == from_state.id && held.content == "edited through the rows")
    );
    world.server.stop().await;
}

/// **The row writer writes what the decision admitted and takes nothing from a
/// clock**: an event that carries no moment is refused, and one that carries it
/// is written with that moment.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_row_writer_takes_the_moment_from_the_event_and_refuses_an_event_without_one() {
    let mut world = world("row-writer-moment").await;
    world.add("person:homer", "Homer").await;
    let (memory, store) = (world.memory.clone(), world.store.clone());
    let outcome = world
        .writer
        .run(async move {
            let seam = crate::writer::Writer::current()
                .and_then(|writer| writer.seam())
                .expect("the job has a seam");
            let fact = NewFact::about(id("person:homer"), "a claim", date(2026, 4, 19));
            let admitted =
                crate::claims_capture::admit_fact(&seam, &fact, &job(), jiff::Timestamp::now())
                    .await
                    .expect("the job decides")
                    .expect("the capture is admitted");
            let mut without = admitted.events.clone();
            for event in &mut without {
                event.written_at = None;
            }
            let mut tx = store.begin().await.expect("a transaction");
            let refused = memory
                .write_admitted(&mut tx, &seam, &without, &admitted.places)
                .await;
            let written = memory
                .write_admitted(&mut tx, &seam, &admitted.events, &admitted.places)
                .await;
            (refused, written)
        })
        .await;
    assert!(
        outcome.0.is_err(),
        "an event with no moment is refused: {:?}",
        outcome.0
    );
    assert!(
        outcome.1.is_ok(),
        "the same events with their moment are written: {:?}",
        outcome.1
    );
    world.server.stop().await;
}

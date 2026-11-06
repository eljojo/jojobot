//! **The claims State answers what the store answers.** One store is written
//! through the port in the shapes the store keeps (a claim rewritten, a link, a
//! merge, a rename, an archive), read once with the import reader, replayed into
//! a State, and every thing is asked the same question of both. One test per
//! read; a difference is a defect in the State or a quirk of the store that the
//! card reports, never an exception written into a table.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::log::ObjectId;
use jojobot_domain::log::claims::State;
use jojobot_domain::log::claims::import;
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityId, FactAddress, FactPatch, Guarded, Memory, NewEntity, NewFact,
    Override, Provenance, guard,
};

async fn thing(memory: &dyn Memory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    let new = NewEntity::new(id.clone(), name, "fixture");
    match memory.add_entity(new.clone()).await.expect("add") {
        // Things of one family share words, and the guard asks to be told apart.
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
    claim(memory, about, what, fields, true, day).await
}

/// A claim whose keys are its own labels: they stay on the claim and the thing
/// holds none of them.
async fn labelled(
    memory: &dyn Memory,
    about: &EntityId,
    what: &str,
    fields: &[(&str, &str)],
    day: i8,
) -> FactAddress {
    claim(memory, about, what, fields, false, day).await
}

async fn claim(
    memory: &dyn Memory,
    about: &EntityId,
    what: &str,
    fields: &[(&str, &str)],
    sets_them: bool,
    day: i8,
) -> FactAddress {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, day));
    fact.provenance = Provenance::Inference;
    fact.fields = fields
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    if sets_them {
        fact.sets = fact.fields.keys().cloned().collect();
    }
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address()
}

/// **A store written in the shapes it keeps**, read once and replayed into a
/// State. Returns what each read is asked of: the store, the State, and the
/// things still standing (a thing folded into another has no answer of its own).
struct Written {
    server: Dolt,
    memory: DoltMemory,
    pool: sqlx::MySqlPool,
    state: State,
    standing: Vec<EntityId>,
}

async fn written(label: &str) -> Written {
    let scratch = Scratch::new(label);
    let server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, label)
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
    // A key whose value names another thing writes a link.
    say(
        &memory,
        &homer,
        "is married",
        &[("spouse", "person:marge")],
        2,
    )
    .await;
    // A key written twice: the second write holds the value.
    say(&memory, &homer, "is grumpy", &[("mood", "grumpy")], 3).await;
    // A key that stays on its claim: the thing holds none of it.
    labelled(&memory, &homer, "wears a hat", &[("hat", "blue")], 3).await;
    // A claim rewritten has two writes.
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
    // A claim with an edge, a reference and a lineage.
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
    // A key taken off: the later write is blank.
    say(&memory, &bart, "stops skating", &[("mood", "")], 4).await;
    say(
        &memory,
        &lisa,
        "plays the saxophone",
        &[("instrument", "saxophone")],
        1,
    )
    .await;
    say(&memory, &krusty, "hosts a show", &[], 1).await;

    // A merge: Lisa is folded into Bart.
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
    // A rename: Nelson now wears another handle.
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
    // An archive.
    memory
        .archive_entity(&krusty, "off the air", None)
        .await
        .expect("the archive lands");

    let read = memory.import_rows().await.expect("the reader reads");
    let events = import::import(&read.rows).expect("the rows map to events");
    let state = State::replay(&events);
    Written {
        server,
        memory,
        pool,
        state,
        standing: [homer, marge, bart, renamed, krusty].into(),
    }
}

/// The key the State files a thing under: its badge when it has one.
async fn object_of(pool: &sqlx::MySqlPool, handle: &EntityId) -> ObjectId {
    let badge: Option<String> = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(handle.as_str())
        .fetch_one(pool)
        .await
        .expect("the entity row");
    ObjectId(badge.unwrap_or_else(|| handle.as_str().to_string()))
}

/// **The moment of the write that holds each key, and the newest write of any
/// kind, are the store's `field_ages`.**
#[tokio::test]
async fn the_state_gives_the_ages_the_store_gives() {
    let mut world = written("parity-ages").await;
    let declared = world.memory.declared_types().await.expect("declarations");
    for handle in &world.standing {
        let at = object_of(&world.pool, handle).await;
        let stored = world.memory.field_ages(handle).await.expect("the store's");
        assert_eq!(
            world.state.held_since(&at, &declared),
            stored.keys,
            "{handle}: the moment each key's value was taken in"
        );
        assert_eq!(
            world.state.newest_change(&at),
            stored.newest,
            "{handle}: the newest write of any kind"
        );
    }
    world.server.stop().await;
}

/// **Who backs each value is the store's `backing`.**
#[tokio::test]
async fn the_state_gives_the_backing_the_store_gives() {
    let mut world = written("parity-backing").await;
    let declared = world.memory.declared_types().await.expect("declarations");
    for handle in &world.standing {
        let at = object_of(&world.pool, handle).await;
        let stored = world.memory.backing(handle).await.expect("the store's");
        assert_eq!(
            world.state.backing(&at, &declared),
            stored,
            "{handle}: who backs each key"
        );
    }
    world.server.stop().await;
}

/// **The writes of each key, those kept off the thing and those that stay on
/// their record are the store's `key_trails`.**
#[tokio::test]
async fn the_state_gives_the_key_trails_the_store_gives() {
    let mut world = written("parity-trails").await;
    for handle in &world.standing {
        let at = object_of(&world.pool, handle).await;
        let stored = world.memory.key_trails(handle).await.expect("the store's");
        assert_eq!(
            world.state.key_trails(&at),
            stored,
            "{handle}: the writes of each key"
        );
        if handle.as_str() == "person:homer" {
            // The comparison above is not over two empty answers.
            assert!(
                !stored.record_writes.is_empty(),
                "a key stays on its record"
            );
            assert!(!stored.kept_off.is_empty(), "a key is kept off the thing");
        }
    }
    world.server.stop().await;
}

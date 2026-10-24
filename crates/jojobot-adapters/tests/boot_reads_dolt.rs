//! **The boot reader answers what the read of a single thing answers, for every
//! thing it answers for, over the real store.**
//!
//! A store of plain things, a pair a merge joined, a thing that was renamed and a
//! thing that was archived. Each part the reader holds is compared, thing by thing,
//! with the read of that thing on its own. The reader may leave a thing out and the
//! fill then reads it as before, so what is compared is every thing it names; and
//! what it must name is held apart, so a reader that answered for none could not
//! pass.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::boot_reader::BootReader;
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
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, day));
    fact.provenance = Provenance::Testimony;
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

/// A claim that draws a connection at another thing.
async fn say_of(
    memory: &dyn Memory,
    about: &EntityId,
    what: &str,
    toward: &EntityId,
    derived_from: Option<FactAddress>,
    day: i8,
) {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, day));
    fact.provenance = Provenance::Testimony;
    fact.edge = Some(Edge::new(EdgeShape::Connection, toward.clone()));
    fact.derived_from = derived_from;
    fact.refs = vec![toward.clone()];
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
}

/// **A store with every shape the reader meets**, and the handles of the plain
/// ones.
struct World {
    memory: DoltMemory,
    /// Things no merge touched, which the reader must name.
    plain: Vec<EntityId>,
    /// The pair a merge joined, which the reader may leave out.
    joined: Vec<EntityId>,
    /// Plain things with a claim that points at the pair, whose claim writes the
    /// reader leaves to the read of one thing.
    pointing_at_a_merge: Vec<EntityId>,
    /// Plain things holding a field value that names the pair, which the reader
    /// would have to repoint and leaves to the read of one thing.
    naming_a_merge: Vec<EntityId>,
}

async fn world(store: &Dolt, name: &str) -> World {
    let pool = migrated_database(store, name)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool);
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    let homer = thing(&memory, "person:homer", "Homer").await;
    let marge = thing(&memory, "person:marge", "Marge").await;
    let bart = thing(&memory, "person:bart", "Bart").await;
    let lisa = thing(&memory, "person:lisa", "Lisa").await;
    let nelson = thing(&memory, "person:nelson", "Nelson").await;
    let krusty = thing(&memory, "person:krusty", "Krusty").await;
    let empty = thing(&memory, "person:apu", "Apu").await;

    let eats = say(
        &memory,
        &homer,
        "eats donuts",
        &[("mood", "happy"), ("donuts", "3")],
        1,
    )
    .await;
    say(
        &memory,
        &homer,
        "naps on the couch",
        &[("mood", "sleepy")],
        2,
    )
    .await;
    let keeps = say(&memory, &marge, "keeps the house", &[("hair", "blue")], 1).await;
    // A claim rewritten once has two writes, which a history must keep apart.
    memory
        .update_fact(
            &keeps,
            FactPatch {
                content: Some("keeps the house tidy".into()),
                ..Default::default()
            },
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("the claim is rewritten")
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
    say(&memory, &nelson, "has a pacifier", &[("mood", "calm")], 1).await;
    say(&memory, &krusty, "hosts a show", &[], 1).await;
    // Field values that name other things: one a plain thing, one the thing a merge
    // folds below.
    say(
        &memory,
        &homer,
        "is married",
        &[("spouse", "person:marge")],
        6,
    )
    .await;
    say(
        &memory,
        &krusty,
        "has a sibling",
        &[("sibling", "person:lisa")],
        6,
    )
    .await;
    // Pointers: a connection to a plain thing, a claim worked out from another, and
    // a connection to a thing the merge below folds, which the reader cannot place.
    {
        // A claim whose own field stays on the record and never reaches the thing.
        let mut fact = NewFact::about(homer.clone(), "wrote a note", date(2026, 9, 3));
        fact.provenance = Provenance::Testimony;
        fact.fields = [("note".to_string(), "d'oh".to_string())].into();
        memory
            .capture(fact)
            .await
            .expect("capture")
            .written()
            .expect("not blocked");
    }
    say_of(
        &memory,
        &marge,
        "is married to Homer",
        &homer,
        Some(eats),
        3,
    )
    .await;
    say_of(&memory, &krusty, "is a rival of Bart", &bart, None, 3).await;
    say_of(&memory, &homer, "is a father of Lisa", &lisa, None, 4).await;

    // A merge: Lisa is folded into Bart. Both are touched by an event.
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
    // The guard asks to be told apart a name near Marge's; the second call does.
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
    say(&memory, &homer, "naps again", &[("mood", "sleepy")], 5).await;

    World {
        memory,
        plain: vec![homer.clone(), marge, renamed, krusty.clone(), empty],
        pointing_at_a_merge: vec![homer, krusty.clone()],
        naming_a_merge: vec![krusty],
        joined: vec![lisa, bart],
    }
}

/// **The things asked for: every thing the store lists, and one it does not hold.**
async fn asked_of(memory: &DoltMemory) -> Vec<EntityId> {
    let mut asked: Vec<EntityId> = memory
        .list_entities(None)
        .await
        .expect("the things list")
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    asked.push(EntityId("person:skinner".into()));
    asked
}

/// **A store of its own, the world in it, and the check run over both.** One store
/// for each part, so a part that disagrees is named by its own case.
async fn parity(name: &str, check: impl AsyncFnOnce(&World, &dyn BootReader, &[EntityId])) {
    let scratch = Scratch::new(name);
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let world = world(&store, name).await;
    let reader = world.memory.boot_reader();
    let reader: &dyn BootReader = &reader;
    let asked = asked_of(&world.memory).await;
    check(&world, reader, &asked).await;
    store.stop().await;
}

/// Every part a reader holds, for every thing it names, is the part the read of that
/// thing gives; and it names every thing it must.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_backing_is_the_backing_of_each_thing() {
    parity("backing", async |world, reader, asked| {
        let backing = reader.backing_many(asked).await.expect("backing reads");
        for (thing, got) in &backing {
            let alone = world.memory.backing(thing).await.expect("backing of one");
            assert_eq!(got, &alone, "backing of {thing}");
        }
        for thing in &world.plain {
            assert!(backing.contains_key(thing), "backing names {thing}");
        }
        assert!(
            !backing.contains_key(&EntityId("person:skinner".into())),
            "a thing the store does not hold is not answered"
        );
        for thing in &world.joined {
            assert!(
                !backing.contains_key(thing),
                "{thing} was touched by a merge, so the reader leaves it to the read of one thing"
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_claim_histories_are_the_claim_histories_of_each_thing() {
    parity("histories", async |world, reader, asked| {
        let histories = reader
            .claim_histories_many(asked)
            .await
            .expect("claim histories read");
        for (thing, got) in &histories {
            let alone = world.memory.claim_histories(thing).await.expect("of one");
            assert_eq!(got, &alone, "claim histories of {thing}");
        }
        for thing in &world.plain {
            assert_eq!(
                histories.contains_key(thing),
                !world.pointing_at_a_merge.contains(thing),
                "claim histories name {thing} unless a claim of it points at the merge"
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_key_trails_are_the_key_trails_of_each_thing() {
    parity("trails", async |world, reader, asked| {
        let trails = reader
            .key_trails_many(asked)
            .await
            .expect("key trails read");
        for (thing, got) in &trails {
            let alone = world.memory.key_trails(thing).await.expect("of one");
            assert_eq!(got, &alone, "key trails of {thing}");
        }
        for thing in &world.plain {
            assert_eq!(
                trails.contains_key(thing),
                !world.naming_a_merge.contains(thing),
                "key trails name {thing} unless a value of it names the merge"
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_fields_are_the_fields_of_each_thing() {
    parity("fields", async |world, reader, asked| {
        let fields = reader
            .fields_versioned_many(asked)
            .await
            .expect("fields read");
        for (thing, got) in &fields {
            let alone = world.memory.fields_versioned(thing).await.expect("of one");
            assert_eq!(got, &alone, "fields of {thing}");
        }
        for thing in &world.plain {
            assert_eq!(
                fields.contains_key(thing),
                !world.naming_a_merge.contains(thing),
                "fields name {thing} unless a value of it names the merge"
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_field_ages_are_the_field_ages_of_each_thing() {
    parity("ages", async |world, reader, asked| {
        let ages = reader
            .field_ages_many(asked)
            .await
            .expect("field ages read");
        for (thing, got) in &ages {
            let alone = world.memory.field_ages(thing).await.expect("of one");
            assert_eq!(got, &alone, "field ages of {thing}");
        }
        for thing in &world.plain {
            assert!(ages.contains_key(thing), "field ages name {thing}");
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_versions_are_the_entity_history_of_each_thing() {
    parity("versions", async |world, reader, asked| {
        let versions = reader
            .entity_history_many(asked)
            .await
            .expect("entity histories read");
        for (thing, got) in &versions {
            let alone = world.memory.entity_history(thing).await.expect("of one");
            assert_eq!(got, &alone, "entity history of {thing}");
        }
        for thing in &world.plain {
            assert!(
                versions.contains_key(thing),
                "entity histories name {thing}"
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_documents_are_the_documents_of_the_whole_scan() {
    parity("documents", async |world, reader, asked| {
        let scans = reader.scans_many(asked).await.expect("documents read");
        let whole = world.memory.scan().await.expect("the whole scan reads");
        for (thing, got) in &scans {
            let alone = whole
                .iter()
                .find(|doc| doc.entity.as_ref().is_some_and(|e| &e.id == thing))
                .unwrap_or_else(|| panic!("the scan holds a document for {thing}"));
            assert_eq!(got, alone, "document of {thing}");
        }
        for thing in &world.plain {
            assert_eq!(
                scans.contains_key(thing),
                !world.pointing_at_a_merge.contains(thing) && !world.naming_a_merge.contains(thing),
                "documents name {thing} unless a claim or a value of it points at the merge"
            );
        }
    })
    .await;
}

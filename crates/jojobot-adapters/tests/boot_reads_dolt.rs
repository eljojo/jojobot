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
    Edge, EdgeShape, EntityId, FactAddress, Guarded, Memory, NewEntity, NewFact, Override,
    Provenance, guard,
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
    let maggie = thing(&memory, "person:maggie", "Maggie").await;
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
    say(&memory, &marge, "keeps the house", &[("hair", "blue")], 1).await;
    say(&memory, &bart, "skates", &[("mood", "mischievous")], 1).await;
    say(
        &memory,
        &lisa,
        "plays the saxophone",
        &[("instrument", "saxophone")],
        1,
    )
    .await;
    say(&memory, &maggie, "has a pacifier", &[("mood", "calm")], 1).await;
    say(&memory, &krusty, "hosts a show", &[], 1).await;
    // Pointers: a connection to a plain thing, a claim worked out from another, and
    // a connection to a thing the merge below folds, which the reader cannot place.
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
    // A rename: Maggie now wears another handle.
    let renamed = EntityId("person:flanders".into());
    let rename = |token: Option<String>| {
        let (memory, maggie, renamed) = (&memory, &maggie, &renamed);
        async move {
            memory
                .rename_entity(
                    maggie,
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
        pointing_at_a_merge: vec![homer, krusty],
        joined: vec![lisa, bart],
    }
}

/// **Every part the reader holds, for every thing it names, is the part the read
/// of that thing gives.** The things asked for are every thing the store lists
/// and one it does not hold.
async fn assert_every_part_matches(world: &World) {
    let memory = &world.memory;
    let reader = memory.boot_reader();
    let mut asked: Vec<EntityId> = memory
        .list_entities(None)
        .await
        .expect("the things list")
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    asked.push(EntityId("person:nobody-held".into()));

    let backing = reader.backing_many(&asked).await.expect("backing reads");
    for (thing, got) in &backing {
        let alone = memory.backing(thing).await.expect("backing of one reads");
        assert_eq!(got, &alone, "backing of {thing}");
    }
    for thing in &world.plain {
        assert!(backing.contains_key(thing), "backing names {thing}");
    }
    assert!(
        !backing.contains_key(&EntityId("person:nobody-held".into())),
        "a thing the store does not hold is not answered"
    );
    let histories = reader
        .claim_histories_many(&asked)
        .await
        .expect("claim histories read");
    for (thing, got) in &histories {
        let alone = memory
            .claim_histories(thing)
            .await
            .expect("the claim histories of one read");
        assert_eq!(got, &alone, "claim histories of {thing}");
    }
    for thing in &world.plain {
        let pointing = world.pointing_at_a_merge.contains(thing);
        assert_eq!(
            histories.contains_key(thing),
            !pointing,
            "claim histories name {thing} unless a claim of it points at the merge"
        );
    }
    for thing in &world.joined {
        assert!(
            !backing.contains_key(thing),
            "{thing} was touched by a merge, so the reader leaves it to the read of one thing"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_answers_as_the_read_of_a_single_thing_does() {
    let scratch = Scratch::new("boot-reads");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let world = world(&store, "parity").await;
    assert_every_part_matches(&world).await;
    store.stop().await;
}

//! A claim write adds its events to the writer's one batch, so the claims State
//! the seam holds answers what the store answers for the claim it touched.

use jiff::civil::date;
use jojobot_domain::log::claims::import::record_id;
use jojobot_domain::log::{ObjectId, Position};
use jojobot_domain::memory::{EntityId, FactPatch, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::request::{self, Request, RequestId, Scope};

use crate::dolt::Dolt;
use crate::dolt::memory::DoltMemory;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;

fn scope(id: &str) -> std::sync::Arc<Scope> {
    Scope::new(Request {
        bot: EntityId("person:homer".into()),
        id: RequestId::parse(id).expect("a valid request id"),
        verb: "update_fact".into(),
        digest: format!("digest-{id}"),
    })
}

/// A store with one thing, and the claim writes that follow it run as the
/// writer's jobs, one request each.
async fn world(name: &str) -> (Scratch, Dolt, DoltMemory, Writer, sqlx::MySqlPool) {
    let scratch = Scratch::new(name);
    let server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    memory
        .add_entity(NewEntity::new(
            EntityId("person:homer".into()),
            "Homer",
            "fixture",
        ))
        .await
        .expect("add")
        .written()
        .expect("written");
    (scratch, server, memory, Writer::default(), pool)
}

async fn as_request<T: Send + 'static>(
    writer: &Writer,
    id: &str,
    job: impl std::future::Future<Output = T> + Send + 'static,
) -> T {
    let scope = scope(id);
    writer
        .run(async move { request::within(scope, request::armed(job)).await })
        .await
}

/// **A claim captured and then rewritten is one record with two writes in the
/// State**, as it is in the store: the first write creates it and the second
/// revises it.
#[tokio::test]
async fn a_claim_rewritten_is_one_record_revised_in_the_state() {
    let (_scratch, mut server, memory, writer, pool) = world("claim-events-revised").await;
    let homer = EntityId("person:homer".into());

    let made = {
        let (memory, homer) = (memory.clone(), homer.clone());
        as_request(&writer, "alpha", async move {
            let mut fact = NewFact::about(homer, "eats donuts", date(2026, 9, 1));
            fact.provenance = Provenance::Inference;
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("not blocked")
                .address()
        })
        .await
    };
    {
        let (memory, made) = (memory.clone(), made.clone());
        as_request(&writer, "beta", async move {
            memory
                .update_fact(
                    &made,
                    FactPatch {
                        content: Some("eats donuts daily".into()),
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("the claim is rewritten")
                .written()
                .expect("not blocked");
        })
        .await;
    }

    let key = ObjectId(
        sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
            .bind(homer.as_str())
            .fetch_one(&pool)
            .await
            .expect("the entity row")
            .unwrap_or_else(|| homer.to_string()),
    );
    let record = record_id(&key, made.local.as_str());
    let seam = writer.seam().expect("the seam opened with the first begin");
    let (content, revisions) = seam.read_claims(|state| {
        let held = state.record(&record).expect("the State holds the record");
        (held.claim.content.clone(), held.revisions)
    });
    assert_eq!(content, "eats donuts daily");
    assert_eq!(revisions, 1, "the second write revised the record");
    let batches = writer
        .shared_log(&pool)
        .read_from(Position(0), 10)
        .await
        .unwrap();
    assert_eq!(batches.len(), 2, "one batch for each request");
    writer.drain().await;
    server.stop().await;
}

/// **The keys a claim sets are the keys the State holds**: after a capture and
/// a rewrite that set and take off keys, the State gives the values, the ages
/// and the backing the store gives for the thing (the witness of rule
/// 9.3.3-d (b)).
#[tokio::test]
async fn the_keys_a_claim_sets_are_held_by_the_state_as_the_store_holds_them() {
    let (_scratch, mut server, memory, writer, pool) = world("claim-events-keys").await;
    let homer = EntityId("person:homer".into());

    let made = {
        let (memory, homer) = (memory.clone(), homer.clone());
        as_request(&writer, "alpha", async move {
            let mut fact = NewFact::about(homer, "eats donuts", date(2026, 9, 1));
            fact.provenance = Provenance::Inference;
            fact.fields = [("mood", "happy"), ("hair", "none")]
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
        })
        .await
    };
    {
        let (memory, made) = (memory.clone(), made.clone());
        as_request(&writer, "beta", async move {
            memory
                .update_fact(
                    &made,
                    FactPatch {
                        fields: [("mood".to_string(), "grumpy".to_string())]
                            .into_iter()
                            .collect(),
                        sets: ["mood".to_string()].into_iter().collect(),
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("the claim is rewritten")
                .written()
                .expect("not blocked");
        })
        .await;
    }

    let key = ObjectId(
        sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
            .bind(homer.as_str())
            .fetch_one(&pool)
            .await
            .expect("the entity row")
            .unwrap_or_else(|| homer.to_string()),
    );
    let declared = memory.declared_types().await.expect("declarations");
    let seam = writer.seam().expect("the seam opened with the first begin");
    let (held, ages, backing) = seam.read_claims(|state| {
        (
            state.fields(&key, &declared),
            state.held_since(&key, &declared),
            state.backing(&key, &declared),
        )
    });
    let stored = memory.field_ages(&homer).await.expect("the store's ages");
    assert_eq!(held["mood"], "grumpy", "the later write holds the key");
    assert_eq!(held["hair"], "none");
    assert_eq!(
        ages, stored.keys,
        "the moment each key's value was taken in"
    );
    assert_eq!(
        backing,
        memory.backing(&homer).await.expect("the store's backing"),
        "who backs each key"
    );
    writer.drain().await;
    server.stop().await;
}

/// **What the State holds for a thing, after asserting it is what the store
/// holds**: the moment each key's value was taken in and who backs each key.
/// Returns the values the State holds.
async fn agrees(
    memory: &DoltMemory,
    writer: &Writer,
    pool: &sqlx::MySqlPool,
    thing: &EntityId,
) -> std::collections::BTreeMap<String, String> {
    let key = ObjectId(
        sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
            .bind(thing.as_str())
            .fetch_one(pool)
            .await
            .expect("the entity row")
            .unwrap_or_else(|| thing.to_string()),
    );
    let declared = memory.declared_types().await.expect("declarations");
    let seam = writer.seam().expect("the seam opened with the first begin");
    let (held, ages, backing) = seam.read_claims(|state| {
        (
            state.fields(&key, &declared),
            state.held_since(&key, &declared),
            state.backing(&key, &declared),
        )
    });
    let stored = memory.field_ages(thing).await.expect("the store's ages");
    assert_eq!(
        ages, stored.keys,
        "the moment each key's value was taken in"
    );
    assert_eq!(
        backing,
        memory.backing(thing).await.expect("the store's backing"),
        "who backs each key"
    );
    held
}

/// **A merge in the State is the merge in the store**: the folded thing's
/// writes take their places among the survivor's, the survivor's own writes
/// move on, and a write made after the merge, to a claim of either side, lands
/// where the store serves it. The claim the merge moved wears its new id.
#[tokio::test]
async fn a_merge_and_the_writes_after_it_are_held_by_the_state_as_the_store_holds_them() {
    let (_scratch, mut server, memory, writer, pool) = world("claim-events-merge").await;
    let homer = EntityId("person:homer".into());
    let marge = EntityId("person:marge".into());
    memory
        .add_entity(NewEntity::new(marge.clone(), "Marge", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("written");

    // Marge's write falls between Homer's two, so the merge places it between
    // them and moves Homer's later write on.
    let said =
        |about: EntityId, what: &'static str, kept: &'static [(&'static str, &'static str)]| {
            let memory = memory.clone();
            async move {
                let mut fact = NewFact::about(about, what, date(2026, 9, 1));
                fact.provenance = Provenance::Inference;
                fact.fields = kept
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
        };
    let _happy = as_request(
        &writer,
        "alpha",
        said(homer.clone(), "eats donuts", &[("mood", "happy")]),
    )
    .await;
    let calm = as_request(
        &writer,
        "beta",
        said(
            marge.clone(),
            "keeps the house",
            &[("mood", "calm"), ("hat", "red")],
        ),
    )
    .await;
    let grumpy = as_request(
        &writer,
        "gamma",
        said(homer.clone(), "is grumpy", &[("mood", "grumpy")]),
    )
    .await;
    {
        let (memory, marge, homer) = (memory.clone(), marge.clone(), homer.clone());
        as_request(&writer, "delta", async move {
            memory
                .merge(
                    &marge,
                    &homer,
                    Some("filed twice"),
                    date(2026, 9, 3),
                    &EntityId("bot:sigma".into()),
                    Some("run-a"),
                )
                .await
                .expect("the fold lands");
        })
        .await;
    }
    // The merge names the bot that made it as its actor.
    let merged_by: Vec<Option<jojobot_domain::actor::Actor>> = writer
        .shared_log(&pool)
        .read_from(Position(0), 100)
        .await
        .unwrap()
        .iter()
        .flat_map(|batch| batch.members().to_vec())
        .filter_map(|bytes| crate::claims_log::codec::decode_own(&bytes).ok().flatten())
        .filter(|event| {
            matches!(
                event.payload,
                jojobot_domain::log::claims::ClaimEvent::ObjectsMerged { .. }
            )
        })
        .map(|event| event.actor)
        .collect();
    assert_eq!(
        merged_by,
        vec![Some(jojobot_domain::actor::Actor::Bot(EntityId(
            "bot:sigma".into()
        )))],
        "the merge event names its bot"
    );
    let held = agrees(&memory, &writer, &pool, &homer).await;
    assert_eq!(
        held["mood"], "grumpy",
        "the fold's write sits between the survivor's, which keep their order"
    );
    assert_eq!(
        held["hat"], "red",
        "the survivor holds a key only the fold held"
    );
    // Both sides' claims are written again after the merge. The moved claim is
    // named by the id the merge gave it.
    let moved = memory
        .merge_events(None)
        .await
        .expect("the merge events")
        .into_iter()
        .flat_map(|event| event.claims)
        .find(|(was, _)| *was == calm.local)
        .map(|(_, now)| now)
        .expect("the merge mapped the claim");
    let rewrite = |at: jojobot_domain::memory::FactAddress, mood: &'static str| {
        let memory = memory.clone();
        async move {
            memory
                .update_fact(
                    &at,
                    FactPatch {
                        fields: [("mood".to_string(), mood.to_string())].into(),
                        sets: ["mood".to_string()].into(),
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("the claim is rewritten")
                .written()
                .expect("not blocked");
        }
    };
    as_request(&writer, "epsilon", rewrite(grumpy.clone(), "restless")).await;
    as_request(
        &writer,
        "zeta",
        rewrite(
            jojobot_domain::memory::FactAddress::new(homer.clone(), moved.clone()),
            "serene",
        ),
    )
    .await;

    let held = agrees(&memory, &writer, &pool, &homer).await;
    assert_eq!(held["mood"], "serene", "the newest write holds the key");
    writer.drain().await;
    server.stop().await;
}

/// **A claim that cites a merged-in claim by the id the merge gave it names the
/// record that claim is stored in**: the State holds the record under its stored
/// identity, so a pointer to the served id would name a record that is not there.
#[tokio::test]
async fn a_claim_citing_a_merged_claim_names_the_record_it_is_stored_in() {
    let (_scratch, mut server, memory, writer, pool) = world("claim-events-pointer").await;
    let homer = EntityId("person:homer".into());
    let marge = EntityId("person:marge".into());
    memory
        .add_entity(NewEntity::new(marge.clone(), "Marge", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("written");

    let source = {
        let (memory, marge) = (memory.clone(), marge.clone());
        as_request(&writer, "alpha", async move {
            let mut fact = NewFact::about(marge, "keeps the house", date(2026, 9, 1));
            fact.provenance = Provenance::Inference;
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("not blocked")
                .address()
        })
        .await
    };
    {
        let (memory, marge, homer) = (memory.clone(), marge.clone(), homer.clone());
        as_request(&writer, "beta", async move {
            memory
                .merge(
                    &marge,
                    &homer,
                    Some("filed twice"),
                    date(2026, 9, 3),
                    &EntityId("bot:sigma".into()),
                    Some("run-a"),
                )
                .await
                .expect("the fold lands");
        })
        .await;
    }
    // The claim now wears an id on Homer; a new claim cites it by that id.
    let served = memory
        .merge_events(None)
        .await
        .expect("the merge events")
        .into_iter()
        .flat_map(|event| event.claims)
        .find(|(was, _)| *was == source.local)
        .map(|(_, now)| now)
        .expect("the merge mapped the claim");
    let served = jojobot_domain::memory::FactAddress::new(homer.clone(), served);
    let citing = {
        let (memory, homer, served) = (memory.clone(), homer.clone(), served.clone());
        as_request(&writer, "gamma", async move {
            let mut fact = NewFact::about(homer, "is reminded of the house", date(2026, 9, 4));
            fact.provenance = Provenance::Inference;
            fact.derived_from = Some(served);
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("not blocked")
                .address()
        })
        .await
    };
    {
        let (memory, citing, served) = (memory.clone(), citing.clone(), served.clone());
        as_request(&writer, "delta", async move {
            memory
                .update_fact(
                    &citing,
                    FactPatch {
                        stands_for: Some(vec![served]),
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("the mark is set")
                .written()
                .expect("not blocked");
        })
        .await;
    }

    let key_of = |thing: EntityId| {
        let pool = pool.clone();
        async move {
            ObjectId(
                sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
                    .bind(thing.as_str())
                    .fetch_one(&pool)
                    .await
                    .expect("the entity row")
                    .unwrap_or_else(|| thing.to_string()),
            )
        }
    };
    let (homer_key, marge_key) = (key_of(homer).await, key_of(marge).await);
    let stored = record_id(&marge_key, source.local.as_str());
    let seam = writer.seam().expect("the seam opened with the first begin");
    let (derived, stands, source_held) = seam.read_claims(|state| {
        let citing = state
            .record(&record_id(&homer_key, citing.local.as_str()))
            .expect("the citing claim is held");
        (
            citing.claim.derived_from.clone(),
            citing.claim.stands_for.clone(),
            state.record(&stored).is_some(),
        )
    });
    assert!(
        source_held,
        "the State holds the source under its stored record"
    );
    assert_eq!(
        derived,
        Some(stored.clone()),
        "derived_from names the stored record"
    );
    assert_eq!(stands, vec![stored], "stands_for names the stored record");
    writer.drain().await;
    server.stop().await;
}

/// **A clear in the State is the clear in the store**: sent to a claim that does
/// not hold the key it withdraws that claim's write and the thing keeps its
/// value; sent to the claim that holds the key it takes the key off the thing,
/// and the older claim's value does not return. After each, the State gives the
/// store's values, ages and backing.
#[tokio::test]
async fn a_clear_is_held_by_the_state_as_the_store_holds_it() {
    let (_scratch, mut server, memory, writer, pool) = world("claim-events-clear").await;
    let homer = EntityId("person:homer".into());

    let said = |what: &'static str, mood: &'static str| {
        let (memory, homer) = (memory.clone(), homer.clone());
        async move {
            let mut fact = NewFact::about(homer, what, date(2026, 9, 1));
            fact.provenance = Provenance::Inference;
            fact.fields = [("mood".to_string(), mood.to_string())].into();
            fact.sets = fact.fields.keys().cloned().collect();
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("not blocked")
                .address()
        }
    };
    let clear = |at: jojobot_domain::memory::FactAddress| {
        let memory = memory.clone();
        async move {
            memory
                .update_fact(
                    &at,
                    FactPatch {
                        clear_fields: vec!["mood".to_string()],
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("the clear lands")
                .written()
                .expect("not blocked");
        }
    };
    let older = as_request(&writer, "alpha", said("felt content", "content")).await;
    let newer = as_request(&writer, "beta", said("felt queasy", "queasy")).await;

    as_request(&writer, "gamma", clear(older)).await;
    let held = agrees(&memory, &writer, &pool, &homer).await;
    assert_eq!(
        held["mood"], "queasy",
        "the clear on the older claim leaves the value"
    );

    as_request(&writer, "delta", clear(newer)).await;
    let held = agrees(&memory, &writer, &pool, &homer).await;
    assert!(
        !held.contains_key("mood"),
        "the clear on the holder takes the key off, and nothing older returns: {held:?}"
    );
    writer.drain().await;
    server.stop().await;
}

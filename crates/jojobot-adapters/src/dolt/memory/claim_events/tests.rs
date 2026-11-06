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

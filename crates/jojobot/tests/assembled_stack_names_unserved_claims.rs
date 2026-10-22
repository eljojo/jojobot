//! **A claim a merged survivor cannot serve is named, and the address named reads
//! it, through the stack the binary builds.** The bare store names the claim and
//! reads it; this drives the real store under `open_provisioned` and
//! `assemble_memory`, where the image answers every read from memory and a layer
//! that left the default in place would say nothing is unserved.
//!
//! **A test binary of its own, because `open_provisioned` seeds a process-wide
//! kind set and refuses a second call.**

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::{EntityId, FactAddress, FactId, Memory, NewEntity, NewFact};

use jojobot_adapters::testing::Scratch;

async fn created(memory: &dyn Memory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.into());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "assembled-stack-case"))
        .await
        .expect("the entity is created")
        .written()
        .expect("nothing collides on an empty store");
    id
}

#[tokio::test]
async fn the_stack_the_binary_builds_names_the_claim_a_survivor_cannot_serve_and_reads_it() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-assembled-unserved");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let _scratch = Scratch(path.clone());
    let mut server = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");

    let supplied = jojobot_mcp::provisions();
    let bare = DoltMemory::open(server.pool().clone()).knowing(supplied.clone());
    let resolved = jojobot::wiring::open_provisioned(bare, supplied)
        .await
        .expect("the build's provisions do not collide with an empty store");
    let (folded, indexed) =
        jojobot::wiring::assemble_memory(resolved.clone()).expect("the index opens");

    let homer = created(indexed.as_ref(), "person:homer", "Homer").await;
    let apu = created(indexed.as_ref(), "person:apu", "Apu").await;
    for (who, what) in [(&homer, "keeps bees"), (&apu, "runs the store")] {
        indexed
            .capture(NewFact::about(who.clone(), what, date(2026, 9, 1)))
            .await
            .expect("capture ok")
            .written()
            .expect("nothing collides");
    }
    indexed
        .merge(
            &apu,
            &homer,
            Some("filed twice"),
            date(2026, 9, 2),
            &EntityId("bot:assistant".into()),
            None,
        )
        .await
        .expect("the fold lands");

    // A claim stored under the folded key that the merge event does not list,
    // present before the image is read: the store's own rows, as a restart finds
    // them.
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(apu.as_str())
        .fetch_one(server.pool())
        .await
        .expect("the folded thing has a badge");
    for statement in [
        "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
         VALUES (?, 'f9', 'a stray claim', 'inference', 'active', '2026-09-03')",
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, status, recorded_at)
         VALUES (?, 'f9', 1, 'a stray claim', 'inference', 'active', '2026-09-03')",
    ] {
        sqlx::query(statement)
            .bind(&badge)
            .execute(server.pool())
            .await
            .expect("the stray claim lands");
    }
    folded.rebuild().await.expect("the image is read again");

    let stray = FactAddress::new(apu.clone(), FactId("f9".into()));
    let named = indexed
        .unmapped_claims(&homer)
        .await
        .expect("the outermost port answers");
    assert_eq!(
        named.iter().map(|claim| &claim.address).collect::<Vec<_>>(),
        vec![&stray],
        "the claim is named through every layer"
    );
    assert_eq!(
        resolved
            .unmapped_claims(&homer)
            .await
            .expect("the layer beneath the image answers")
            .iter()
            .map(|claim| &claim.address)
            .collect::<Vec<_>>(),
        vec![&stray],
        "the layer the image reads through forwards the question to the store"
    );
    assert!(
        indexed
            .unmapped_claims(&apu)
            .await
            .expect("the outermost port answers")
            .is_empty(),
        "the folded thing names nothing of its own"
    );
    let history = indexed
        .claim_history(&stray)
        .await
        .expect("the address named reads the claim through every layer");
    assert_eq!(history.len(), 1, "{history:?}");
    // The positive half: what the survivor serves is untouched.
    let served = indexed.recall(&homer).await.expect("recall reads");
    assert!(
        served.iter().all(|fact| fact.content != "a stray claim"),
        "the stray claim is not served as a claim of the survivor: {served:?}"
    );
    assert!(
        served.iter().any(|fact| fact.content == "runs the store"),
        "the claim the event maps is served: {served:?}"
    );
    server.stop().await;
}

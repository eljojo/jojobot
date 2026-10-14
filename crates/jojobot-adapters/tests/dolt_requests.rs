//! **A write spends its request id in its own transaction, in the real store.**
//!
//! The fake ledger spends under its own lock, so it cannot show that the id's
//! row and the write land together. This does: the write is refused for a spent
//! id, and nothing of it is stored.

use std::path::PathBuf;
use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::requests::DoltRequests;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::request::{Request, RequestId, Requests, Scope, armed, digest_of, within};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn thing(memory: &dyn Memory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

fn request(bot: &str, id: &str) -> Request {
    Request {
        bot: EntityId(bot.to_string()),
        id: RequestId::parse(id).expect("a good id"),
        verb: "capture".to_string(),
        digest: digest_of("{}"),
    }
}

fn claim_about(subject: &EntityId, what: &str) -> NewFact {
    let mut fact = NewFact::about(subject.clone(), what, date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    fact
}

/// One capture the way the verb makes it: inside the dispatch's scope, and
/// armed. Returns whether the write stored and whether the id was spent.
async fn captured(memory: &dyn Memory, request: Request, fact: NewFact) -> (bool, bool) {
    let scope = Scope::new(request);
    let stored = within(Arc::clone(&scope), armed(memory.capture(fact)))
        .await
        .is_ok();
    (stored, scope.was_spent())
}

async fn rows(pool: &sqlx::MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM request_receipt")
        .fetch_one(pool)
        .await
        .expect("the ledger is readable")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_capture_spends_its_id_with_the_claim_and_a_second_one_stores_nothing() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let ledger = DoltRequests::new(pool.clone());

    let milhouse = thing(&memory, "person:milhouse", "Milhouse").await;
    thing(&memory, "bot:sigma", "Sigma").await;
    thing(&memory, "bot:omega", "Omega").await;

    // ── the first call spends the id, and the claim is stored ───────────────
    let first = request("bot:sigma", "r1");
    assert_eq!(
        captured(
            &memory,
            first.clone(),
            claim_about(&milhouse, "wears glasses")
        )
        .await,
        (true, true),
        "the first call stores the claim and spends the id"
    );
    assert_eq!(memory.recall(&milhouse).await.expect("recall").len(), 1);
    let held = ledger
        .lookup(&first.bot, &first.id)
        .await
        .expect("the ledger is readable")
        .expect("the id is spent");
    assert_eq!(held.verb, "capture");
    assert_eq!(held.digest, first.digest);
    assert!(
        held.landed.contains("person:milhouse#"),
        "the row says what the write returned: {}",
        held.landed
    );
    assert_eq!(held.receipt, None, "the receipt is filled after the commit");
    ledger.fill(&first, "{\"ok\":true}").await.expect("filled");
    assert_eq!(
        ledger
            .lookup(&first.bot, &first.id)
            .await
            .expect("the ledger is readable")
            .and_then(|stored| stored.receipt)
            .as_deref(),
        Some("{\"ok\":true}")
    );

    // ── the same id again: nothing is stored, the id stays spent once ───────
    assert_eq!(
        captured(
            &memory,
            first.clone(),
            claim_about(&milhouse, "wears a hat")
        )
        .await,
        (false, false),
        "a spent id stores nothing"
    );
    assert_eq!(
        memory.recall(&milhouse).await.expect("recall").len(),
        1,
        "the second claim was rolled back with its row"
    );
    assert_eq!(rows(&pool).await, 1);

    // ── the same id from another bot is another row ─────────────────────────
    assert_eq!(
        captured(
            &memory,
            request("bot:omega", "r1"),
            claim_about(&milhouse, "wears a hat")
        )
        .await,
        (true, true)
    );
    assert_eq!(memory.recall(&milhouse).await.expect("recall").len(), 2);
    assert_eq!(rows(&pool).await, 2);

    // ── a capture outside a scope spends nothing ────────────────────────────
    memory
        .capture(claim_about(&milhouse, "owns a radio"))
        .await
        .expect("capture");
    assert_eq!(rows(&pool).await, 2);

    server.stop().await;
}

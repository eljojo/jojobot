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

// ── every covered write, in the real store ──────────────────────────────────

use jiff::Timestamp;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_domain::mailbox::{
    Acting, MailboxError, MailboxName, Mailboxes, NewMessage, OwnerIndex, OwnerLookup,
};
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityPatch, FactPatch};

/// Any well-formed owner resolves.
struct AnyOwner;

#[async_trait::async_trait]
impl OwnerIndex for AnyOwner {
    async fn look_up(&self, _owner: &EntityId) -> Result<OwnerLookup, MailboxError> {
        Ok(OwnerLookup::Known)
    }
}

fn request_on(verb: &str, id: &str) -> Request {
    Request {
        verb: verb.to_string(),
        ..request("bot:sigma", id)
    }
}

/// Run one write the way a verb runs it: in the dispatch's scope, armed.
async fn spending<T>(
    verb: &str,
    id: &str,
    work: impl std::future::Future<Output = T>,
) -> (T, bool) {
    let scope = Scope::new(request_on(verb, id));
    let out = within(Arc::clone(&scope), armed(work)).await;
    (out, scope.was_spent())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_covered_write_spends_its_id_with_its_own_commit() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-all-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_all")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let mail = DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner));
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    let day = date(2026, 9, 1);

    let mut expected = 0_i64;
    // One write of each kind. Each lands once, spends its id, and a second call
    // under the same id is refused and stores nothing.
    macro_rules! covered {
        ($verb:expr, $id:expr, $write:expr, $again:expr) => {{
            let (landed, spent) = spending($verb, $id, $write).await;
            landed.unwrap_or_else(|e| panic!("{} should land: {e:?}", $verb));
            assert!(spent, "{} spends its id", $verb);
            expected += 1;
            assert_eq!(rows(&pool).await, expected, "{} stored one row", $verb);
            let (refused, spent) = spending($verb, $id, $again).await;
            assert!(refused.is_err(), "{} under a spent id is refused", $verb);
            assert!(!spent, "{} does not spend a spent id", $verb);
            assert_eq!(rows(&pool).await, expected, "{} stored nothing more", $verb);
        }};
    }

    let homer = EntityId("person:homer".into());
    let marge = EntityId("person:lisa".into());
    let apu = EntityId("person:apu".into());
    covered!(
        "capture",
        "k-capture",
        memory.capture(claim_about(&me, "writes the code")),
        memory.capture(claim_about(&me, "reads the code"))
    );
    covered!(
        "add_entity",
        "k-add",
        memory.add_entity(NewEntity::new(homer.clone(), "Homer", "fixture")),
        memory.add_entity(NewEntity::new(
            EntityId("person:bart".into()),
            "Bart",
            "fixture"
        ))
    );
    covered!(
        "add_entity",
        "k-add-first",
        memory.add_entity_with_first_claim(
            NewEntity::new(marge.clone(), "Lisa", "fixture"),
            claim_about(&marge, "keeps the house")
        ),
        memory.add_entity_with_first_claim(
            NewEntity::new(apu.clone(), "Apu", "fixture"),
            claim_about(&apu, "runs the store")
        )
    );
    covered!(
        "update_entity",
        "k-update",
        memory.update_entity(
            &homer,
            EntityPatch {
                name: Some("Homer J".into()),
                ..Default::default()
            }
        ),
        memory.update_entity(
            &homer,
            EntityPatch {
                name: Some("Homer Jay".into()),
                ..Default::default()
            }
        )
    );
    covered!(
        "update_entity",
        "k-update-claim",
        memory.update_entity_with_claim(
            &homer,
            EntityPatch {
                name: Some("Homer".into()),
                ..Default::default()
            },
            claim_about(&homer, "works at the plant")
        ),
        memory.update_entity_with_claim(
            &homer,
            EntityPatch::default(),
            claim_about(&homer, "eats donuts")
        )
    );
    // An inference is rewritten in place by any session; testimony only by the one
    // that wrote it.
    let address = memory
        .capture(NewFact::about(homer.clone(), "likes beer", day))
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address();
    covered!(
        "update_fact",
        "k-fact",
        memory.update_fact(
            &address,
            FactPatch {
                content: Some("works at the nuclear plant".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &me
        ),
        memory.update_fact(
            &address,
            FactPatch {
                content: Some("works elsewhere".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &me
        )
    );
    covered!(
        "set_charter",
        "k-prose",
        memory.set_prose(&homer, "A page.", None),
        memory.set_prose(&homer, "Another page.", None)
    );
    covered!(
        "declare_type",
        "k-type",
        memory.declare_type(
            DeclaredType::new("snacking", vec![Field::new("donuts", ValueType::Number)]),
            None
        ),
        memory.declare_type(
            DeclaredType::new("snacking", vec![Field::new("buns", ValueType::Number)]),
            None
        )
    );
    covered!(
        "archive_entity",
        "k-archive",
        memory.archive_entity(&marge, "filed twice", None),
        memory.archive_entity(&homer, "filed twice", None)
    );
    covered!(
        "archive_entity",
        "k-restore",
        memory.restore_entity(&marge, None),
        memory.restore_entity(&marge, None)
    );
    let claim = memory
        .capture(claim_about(&marge, "makes pie"))
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address();
    covered!(
        "retract",
        "k-retract",
        memory.retract(&claim, Some("mistaken"), day, &me, None),
        memory.retract(&claim, Some("mistaken"), day, &me, None)
    );
    thing(&memory, "person:krusty", "Krusty").await;
    let krusty = EntityId("person:krusty".into());
    covered!(
        "merge_entities",
        "k-merge",
        memory.merge(&krusty, &homer, Some("filed twice"), day, &me, None),
        memory.merge(&krusty, &homer, Some("filed twice"), day, &me, None)
    );

    // The mail rail: post, retire, set aside.
    let inbox = MailboxName("inbox".into());
    mail.create_mailbox(&inbox, &EntityId("bot:sigma".into()), None)
        .await
        .expect("a box")
        .written()
        .expect("not blocked");
    let note = |body: &str| NewMessage {
        mailbox: inbox.clone(),
        body: body.to_string(),
        subject: None,
        sender: "omega".to_string(),
        sent_at: Timestamp::now(),
        in_reply_to: None,
        sender_mail_waiting_at_send: None,
        posted_by_session: None,
        supersedes: None,
    };
    covered!(
        "post_message",
        "k-post",
        mail.post_message(note("first")),
        mail.post_message(note("second"))
    );
    let acting = Acting::unnamed(Timestamp::now());
    let first = mail
        .post_message(note("to retire"))
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    let second = mail
        .post_message(note("to set aside"))
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    covered!(
        "mark_processed",
        "k-retire",
        mail.mark_processed(&first.id, Some("done"), acting),
        mail.mark_processed(&second.id, Some("done"), acting)
    );
    covered!(
        "mark_processed",
        "k-aside",
        mail.quarantine(&second.id, &inbox, "not for me", acting),
        mail.quarantine(&first.id, &inbox, "not for me", acting)
    );

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_spent_id_follows_the_bot_through_a_rename_and_is_not_inherited_by_a_new_holder_of_the_name()
 {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-rename-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_rename")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let ledger = DoltRequests::new(pool.clone());
    let milhouse = thing(&memory, "person:milhouse", "Milhouse").await;
    let sigma = thing(&memory, "bot:sigma", "Sigma").await;

    let spent = request("bot:sigma", "r1");
    assert_eq!(
        captured(
            &memory,
            spent.clone(),
            claim_about(&milhouse, "wears glasses")
        )
        .await,
        (true, true)
    );

    // The bot is renamed. The id is found under both handles.
    let renamed = EntityId("bot:omega".into());
    memory
        .rename_entity(
            &sigma,
            &renamed,
            None,
            date(2026, 9, 2),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("renamed")
        .written()
        .expect("not blocked");
    for handle in [&renamed, &sigma] {
        assert!(
            ledger
                .lookup(handle, &spent.id)
                .await
                .expect("readable")
                .is_some(),
            "the id is found under {handle}"
        );
    }

    // A new bot takes the freed name. It has spent nothing, so the id is not
    // its own, and the receipt is not handed to it.
    memory
        .update_entity(
            &renamed,
            EntityPatch {
                name: Some("Omega".into()),
                ..Default::default()
            },
        )
        .await
        .expect("relabelled")
        .written()
        .expect("not blocked");
    thing(&memory, "bot:sigma", "Tau").await;
    assert!(
        ledger
            .lookup(&sigma, &spent.id)
            .await
            .expect("readable")
            .is_none(),
        "a new holder of the name has not spent the id"
    );
    assert!(
        ledger
            .lookup(&renamed, &spent.id)
            .await
            .expect("readable")
            .is_some(),
        "the renamed bot still has"
    );

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_id_older_than_the_retention_goes_with_the_next_spend() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-keep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_keep")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let milhouse = thing(&memory, "person:milhouse", "Milhouse").await;
    thing(&memory, "bot:sigma", "Sigma").await;
    let key: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = 'bot:sigma'")
        .fetch_one(&pool)
        .await
        .expect("the bot's badge");
    // One row from long ago, and one from a moment ago.
    for (id, at) in [
        ("old", "2020-01-01T00:00:00.000000000Z"),
        ("recent", "2999-01-01T00:00:00.000000000Z"),
    ] {
        sqlx::query(
            "INSERT INTO request_receipt (bot_key, request_id, verb, args_digest, landed, written_at) \
             VALUES (?, ?, 'capture', 'd', '{}', ?)",
        )
        .bind(&key)
        .bind(id)
        .bind(at)
        .execute(&pool)
        .await
        .expect("a row");
    }
    assert_eq!(rows(&pool).await, 2);

    captured(
        &memory,
        request("bot:sigma", "now"),
        claim_about(&milhouse, "wears glasses"),
    )
    .await;
    let kept: Vec<String> =
        sqlx::query_scalar("SELECT request_id FROM request_receipt ORDER BY request_id")
            .fetch_all(&pool)
            .await
            .expect("readable");
    assert_eq!(
        kept,
        vec!["now".to_string(), "recent".to_string()],
        "the old row went, the recent one and the new one stayed"
    );

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_calls_under_one_id_at_the_same_moment_store_one_write() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-race-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_race")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = Arc::new(DoltMemory::open(pool.clone()));
    let milhouse = thing(memory.as_ref(), "person:milhouse", "Milhouse").await;
    thing(memory.as_ref(), "bot:sigma", "Sigma").await;

    // Several rounds, so a race that went one way by luck would not pass.
    for round in 0..6 {
        let id = format!("race-{round}");
        let calls = (0..4).map(|n| {
            let memory = Arc::clone(&memory);
            let subject = milhouse.clone();
            let id = id.clone();
            tokio::spawn(async move {
                captured(
                    memory.as_ref(),
                    request("bot:sigma", &id),
                    claim_about(&subject, &format!("said it {round} {n}")),
                )
                .await
            })
        });
        let mut spent = 0;
        for call in calls {
            let (_stored, was_spent) = call.await.expect("the call ran");
            spent += usize::from(was_spent);
        }
        assert_eq!(spent, 1, "round {round}: exactly one call spent the id");
        assert_eq!(
            memory.recall(&milhouse).await.expect("recall").len(),
            round + 1,
            "round {round}: exactly one claim was stored"
        );
    }

    server.stop().await;
}

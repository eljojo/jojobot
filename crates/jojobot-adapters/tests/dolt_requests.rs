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

/// **The calls that lose a race under one id are told to send the call again.**
/// Four calls under one id at the same moment: one spends the id and stores its
/// claim, and each of the others is refused by the store. What a loser is told
/// is read here, because the same words reach the caller (the dispatch replaces
/// them with the winner's receipt only when the ledger already holds it). A
/// loser is told the write collided and to send the same call again, never that
/// the call itself has to change. The send again then finds the winner's row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_calls_that_lose_a_race_under_one_id_are_told_to_send_it_again() {
    use jojobot_domain::memory::MemoryError;

    let dir = std::env::temp_dir().join(format!("jojobot-requests-losers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_losers")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = Arc::new(DoltMemory::open(pool.clone()));
    let ledger = DoltRequests::new(pool.clone());
    let milhouse = thing(memory.as_ref(), "person:milhouse", "Milhouse").await;
    thing(memory.as_ref(), "bot:sigma", "Sigma").await;

    let mut losers = 0;
    // Several rounds, so a race that went one way by luck would not pass.
    for round in 0..6 {
        let id = format!("loser-{round}");
        let calls = (0..4).map(|n| {
            let memory = Arc::clone(&memory);
            let subject = milhouse.clone();
            let id = id.clone();
            tokio::spawn(async move {
                let scope = Scope::new(request("bot:sigma", &id));
                let told = within(
                    Arc::clone(&scope),
                    armed(memory.capture(claim_about(&subject, &format!("said it {round} {n}")))),
                )
                .await;
                (told, scope.was_spent())
            })
        });
        for call in calls {
            let (told, spent) = call.await.expect("the call ran");
            match (spent, told) {
                (true, Ok(_)) => {}
                (false, Err(MemoryError::Conflict)) => losers += 1,
                (spent, told) => panic!(
                    "round {round}: a call that {} the id was told {told:?}, \
                     and a loser is told to send it again",
                    if spent { "spent" } else { "did not spend" }
                ),
            }
        }
        let kept = ledger
            .lookup(
                &EntityId("bot:sigma".into()),
                &RequestId::parse(&id).expect("an id"),
            )
            .await
            .expect("the ledger reads");
        assert!(
            kept.is_some(),
            "round {round}: the send again finds the winner's row"
        );
    }
    assert!(losers > 0, "four calls at once never lost a race: {losers}");

    server.stop().await;
}

/// **Stop a call partway through its transaction.** The call is polled by hand
/// `polls` times, with a moment between polls for the store to answer. Each poll
/// runs it to its next wait on the store, so the call stands between two of its
/// statements. `Some` is the call's answer when it finished first.
async fn stopped_after<F: std::future::Future>(
    call: &mut std::pin::Pin<&mut F>,
    polls: usize,
) -> Option<F::Output> {
    for _ in 0..polls {
        if let std::task::Poll::Ready(done) =
            std::future::poll_fn(|cx| std::task::Poll::Ready(call.as_mut().poll(cx))).await
        {
            return Some(done);
        }
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    }
    None
}

/// **A call that loses at the commit, on the rows it wrote, is told to send it
/// again.** Two identical captures under one id overlap in the store: the first
/// is stopped after some number of its statements, the second runs to its end
/// and commits, and the first then finishes. The stop moves one statement at a
/// time through the whole call, so the first call's transaction began before the
/// second one committed at most of the stops, and it conflicts with the
/// committed one on the claim it wrote and on the id's row when it commits. The
/// store reports that at the commit, not at the insert of the id's row. A loser
/// is told the write collided, never that a key is held twice: the call is good
/// and the same call sent again finds the winner's row. Paired: in every round
/// one call spent the id and stored its claim.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_call_that_loses_at_the_commit_under_one_id_is_told_to_send_it_again() {
    use jojobot_domain::memory::MemoryError;
    use tracing::Instrument;

    const STOPS: usize = 30;
    // **The span this case's calls run in.** The log sink is the process's one
    // sink, so a sibling case that loses a commit writes the same refusal into it.
    // A line a call of this case logs carries the span's name, and no other case's
    // line does.
    //
    // The sink is installed first: a span made before any subscriber exists is
    // cached as disabled, and then no line carries it.
    const CASE_SPAN: &str = "commit_race_case";
    jojobot_adapters::testing::install_store_log_sink();
    let case = tracing::warn_span!("commit_race_case");
    let dir = std::env::temp_dir().join(format!("jojobot-requests-commit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_commit")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = Arc::new(DoltMemory::open(pool.clone()));
    let milhouse = thing(memory.as_ref(), "person:milhouse", "Milhouse").await;
    thing(memory.as_ref(), "bot:sigma", "Sigma").await;

    let mut losses_after_a_stop = 0;
    for stop in 0..STOPS {
        let id = format!("commit-{stop}");
        let capture = |who: &'static str| {
            let memory = Arc::clone(&memory);
            let subject = milhouse.clone();
            let id = id.clone();
            async move {
                let scope = Scope::new(request("bot:sigma", &id));
                let told = within(
                    Arc::clone(&scope),
                    armed(memory.capture(claim_about(&subject, &format!("said it {who} {stop}")))),
                )
                .await;
                (told, scope.was_spent())
            }
            .instrument(case.clone())
        };
        let mut first = std::pin::pin!(capture("first"));
        let finished = stopped_after(&mut first, stop).await;
        let second = capture("second").await;
        let was_stopped = finished.is_none();
        let first = match finished {
            Some(done) => done,
            None => first.await,
        };
        let mut winners = 0;
        for (which, (told, spent)) in [("first", first), ("second", second)] {
            match (spent, told) {
                (true, Ok(_)) => winners += 1,
                (false, Err(MemoryError::Conflict)) => {
                    losses_after_a_stop += usize::from(which == "first" && was_stopped);
                }
                (spent, told) => panic!(
                    "stop {stop}: the {which} call that {} the id was told {told:?}, \
                     and a loser is told to send it again",
                    if spent { "spent" } else { "did not spend" }
                ),
            }
        }
        assert_eq!(winners, 1, "stop {stop}: exactly one call spent the id");
        assert_eq!(
            memory.recall(&milhouse).await.expect("recall").len(),
            stop + 1,
            "stop {stop}: exactly one claim was stored"
        );
    }
    assert!(
        losses_after_a_stop > 0,
        "no stopped call lost to a commit: the case never overlapped two transactions"
    );
    // The store's own account of those losses is in the log: a conflict at the
    // commit, which the insert of a second id row would not say. Only the lines
    // this case's own calls logged count.
    let commit_conflicts = jojobot_adapters::testing::store_log()
        .lines()
        .filter(|line| line.contains(CASE_SPAN) && line.contains("serialization failure"))
        .count();
    assert!(
        commit_conflicts > 0,
        "the store reported no conflict at a commit"
    );

    server.stop().await;
}

/// **A request id answers as a repeat exactly while the ledger keeps it.** A row
/// older than the keep window is not read: the dispatch finds no spent id, and a
/// different write under the same id writes. A resend inside the window still
/// answers, in the same case, so the window check is not a ledger that forgets
/// everything.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_id_is_a_repeat_only_while_it_is_kept() {
    use jojobot_domain::clock::Clock;
    use jojobot_domain::request::KEPT_FOR;
    let dir = std::env::temp_dir().join(format!("jojobot-requests-window-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_window")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let written_on = date(2026, 3, 1);
    let at = |days: i8| {
        Clock::stating(
            written_on
                .checked_add(jiff::Span::new().days(days))
                .unwrap(),
        )
    };
    let memory = DoltMemory::open(pool.clone()).on_clock(at(0));
    let milhouse = thing(&memory, "person:milhouse", "Milhouse").await;
    thing(&memory, "bot:sigma", "Sigma").await;

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

    // ── inside the window: two days on, the id is still a repeat ────────────
    let two_days_on = DoltRequests::new(pool.clone()).on_clock(at(2));
    assert!(
        two_days_on
            .lookup(&first.bot, &first.id)
            .await
            .expect("the ledger is readable")
            .is_some(),
        "an id inside {KEPT_FOR:?} is spent"
    );

    // ── past the window, with no other write: the id is no repeat ───────────
    let four_days_on = DoltRequests::new(pool.clone()).on_clock(at(4));
    assert!(
        four_days_on
            .lookup(&first.bot, &first.id)
            .await
            .expect("the ledger is readable")
            .is_none(),
        "an id older than {KEPT_FOR:?} still answers as a repeat"
    );

    // ── and a different write under the same id writes ──────────────────────
    let later = DoltMemory::open(pool.clone()).on_clock(at(4));
    assert_eq!(
        captured(
            &later,
            first.clone(),
            claim_about(&milhouse, "plays the saxophone")
        )
        .await,
        (true, true),
        "a different write under an id past the window stores and spends it again"
    );
    assert_eq!(later.recall(&milhouse).await.expect("recall").len(), 2);

    server.stop().await;
}

/// **Lead (a): two ids that differ only in case are two ids in the real store.**
/// The ledger's column is a plain `VARCHAR`, and a collation that ignores case
/// would read `Req-A` as `req-a`: the second write would hit the first one's row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_ids_that_differ_only_in_case_are_two_ids_in_the_ledger() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-case-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_case")
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

    let upper = request("bot:sigma", "Req-A");
    let lower = request("bot:sigma", "req-a");
    assert_eq!(
        captured(
            &memory,
            upper.clone(),
            claim_about(&milhouse, "wears glasses")
        )
        .await,
        (true, true)
    );
    assert!(
        ledger
            .lookup(&lower.bot, &lower.id)
            .await
            .expect("readable")
            .is_none(),
        "the lower-case id reads the upper-case id's row"
    );
    assert_eq!(
        captured(
            &memory,
            lower.clone(),
            claim_about(&milhouse, "plays chess")
        )
        .await,
        (true, true),
        "a write under the lower-case id stores and spends it"
    );
    assert_eq!(rows(&pool).await, 2);
    assert!(
        ledger
            .lookup(&upper.bot, &upper.id)
            .await
            .expect("readable")
            .is_some()
    );
    assert!(
        ledger
            .lookup(&lower.bot, &lower.id)
            .await
            .expect("readable")
            .is_some()
    );

    server.stop().await;
}

/// **Lead (c): a handle that two things have worn resolves to the newest.** The
/// ledger finds a bot's key through the former-handle table, and `resolve`
/// reads that table newest first. A read with no order may answer with the
/// older thing, and the id is then spent for the wrong bot.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_handle_two_bots_have_worn_belongs_to_the_newer_one_in_the_ledger() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-former-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_former")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let ledger = DoltRequests::new(pool.clone());
    let milhouse = thing(&memory, "person:milhouse", "Milhouse").await;

    // One bot wears `bot:sigma`, moves off it, and a second bot takes the name
    // and moves off it too: both own it as a former handle.
    thing(&memory, "bot:sigma", "Sigma").await;
    for (from, to) in [("bot:sigma", "bot:older")] {
        memory
            .rename_entity(
                &EntityId(from.into()),
                &EntityId(to.into()),
                None,
                date(2026, 9, 1),
                jojobot_domain::memory::Override::none(),
                None,
            )
            .await
            .expect("rename")
            .written()
            .expect("the rename lands");
    }
    // The freed name meets the near-miss screen; the token is read from the refusal.
    let again = NewEntity::new(
        EntityId("bot:sigma".into()),
        "Sigma again",
        "contract-fixture",
    );
    let jojobot_domain::memory::Guarded::Blocked {
        attempted,
        candidates,
    } = memory.add_entity(again.clone()).await.expect("add ok")
    else {
        panic!("the freed name was expected to meet the screen");
    };
    memory
        .add_entity(NewEntity {
            override_token: Some(jojobot_domain::memory::guard::override_token(
                &attempted,
                &candidates,
            )),
            override_reason: Some("a different bot that took the freed name".to_string()),
            ..again
        })
        .await
        .expect("add ok")
        .written()
        .expect("the freed name can be taken");
    memory
        .rename_entity(
            &EntityId("bot:sigma".into()),
            &EntityId("bot:newer".into()),
            None,
            date(2026, 9, 2),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("rename")
        .written()
        .expect("the rename lands");

    // An id spent under the shared old handle belongs to the bot that wore it last.
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
    let newer = ledger
        .lookup(&EntityId("bot:newer".into()), &spent.id)
        .await
        .expect("readable");
    let older = ledger
        .lookup(&EntityId("bot:older".into()), &spent.id)
        .await
        .expect("readable");
    assert!(
        newer.is_some() && older.is_none(),
        "the id was spent for the wrong bot: newer {newer:?}, older {older:?}"
    );

    server.stop().await;
}

/// **Lead (b): a claim that fails before it inserts hands the id back.** The
/// verb's commit takes the call's request once. If the claim fails on a store
/// error, the request must be free for the next commit of the same call, or that
/// commit would store its write with no row for the id and a resend would write
/// twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_that_fails_before_it_inserts_leaves_the_request_for_the_next_commit() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-claim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_claim")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let milhouse = thing(&memory, "person:milhouse", "Milhouse").await;
    thing(&memory, "bot:sigma", "Sigma").await;

    let scope = Scope::new(request("bot:sigma", "r1"));
    // The ledger is out of reach for the first commit: the claim's first
    // statement on it fails.
    sqlx::query("RENAME TABLE request_receipt TO request_receipt_away")
        .execute(&pool)
        .await
        .expect("the table moves");
    let failed = within(
        Arc::clone(&scope),
        armed(memory.capture(claim_about(&milhouse, "wears glasses"))),
    )
    .await;
    assert!(failed.is_err(), "the commit cannot reach the ledger");
    assert!(!scope.was_spent());

    sqlx::query("RENAME TABLE request_receipt_away TO request_receipt")
        .execute(&pool)
        .await
        .expect("the table comes back");
    // The same call, committing again: it can take the request, and spends it.
    within(
        Arc::clone(&scope),
        armed(memory.capture(claim_about(&milhouse, "wears glasses"))),
    )
    .await
    .expect("the second commit lands");
    assert!(
        scope.was_spent(),
        "the failed claim kept the request, so the write stored with no row for its id"
    );
    assert_eq!(rows(&pool).await, 1);

    server.stop().await;
}

/// **Two merges under one id leave one event in the real store**, as the fake
/// leaves one. The first spends the id in the transaction that writes its event;
/// the second finds it spent and stores nothing, so the thing it would have
/// folded is still a thing. **Paired**: the first merge landed.
#[tokio::test]
async fn two_merges_under_one_id_leave_one_event() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-merge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_merge")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let survivor = thing(&memory, "person:lisa", "Lisa").await;
    let first = thing(&memory, "person:nelson", "Nelson").await;
    let second = thing(&memory, "person:milhouse", "Milhouse").await;
    let caller = thing(&memory, "bot:sigma", "Sigma").await;

    let mut outcomes = Vec::new();
    for folded in [&first, &second] {
        let scope = Scope::new(request_on("merge_entities", "merge-1"));
        let landed = within(
            Arc::clone(&scope),
            armed(memory.merge(folded, &survivor, None, date(2026, 9, 1), &caller, None)),
        )
        .await
        .is_ok();
        outcomes.push((landed, scope.was_spent()));
    }
    assert_eq!(outcomes, [(true, true), (false, false)], "{outcomes:?}");
    let events = memory
        .merge_events(Some(&survivor))
        .await
        .expect("the events read");
    assert_eq!(events.len(), 1, "one event: {events:?}");
    assert_eq!(events[0].folded, first);
    let rows = memory.list_entities(None).await.expect("lists");
    assert!(
        rows.iter()
            .any(|e| e.id == second && e.merged_into.is_none()),
        "the refused merge folded nothing"
    );

    server.stop().await;
}

// ── a write that commits in parts ───────────────────────────────────────────

/// One capture as one named part of a verb, in the dispatch's scope. Returns
/// whether the store stored it and whether the scope holds the part afterwards.
async fn captured_as_part(
    memory: &dyn Memory,
    scope: &Arc<Scope>,
    part: &str,
    note: Option<&str>,
    fact: NewFact,
) -> (bool, bool) {
    let stored = within(
        Arc::clone(scope),
        jojobot_domain::request::armed_part(part, note.map(str::to_string), memory.capture(fact)),
    )
    .await
    .is_ok();
    (stored, scope.recorded(part).is_some())
}

async fn part_rows(pool: &sqlx::MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM request_part")
        .fetch_one(pool)
        .await
        .expect("the part ledger is readable")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_part_is_stored_with_its_own_row_and_the_id_is_spent_when_the_verb_finishes() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-parts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_parts")
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
    let first = request("bot:sigma", "parts-1");

    // ── the first part lands with its row, and spends nothing of the whole ──
    let scope = Scope::new(first.clone());
    assert_eq!(
        captured_as_part(
            &memory,
            &scope,
            "one",
            Some("plan"),
            claim_about(&milhouse, "wears glasses")
        )
        .await,
        (true, true)
    );
    assert_eq!(part_rows(&pool).await, 1);
    assert_eq!(
        rows(&pool).await,
        0,
        "the whole write is not spent by a part"
    );
    assert!(!scope.was_spent());
    let held = ledger
        .parts(&first.bot, &first.id)
        .await
        .expect("the part ledger is readable");
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].part, "one");
    assert_eq!(held[0].verb, "capture");
    assert_eq!(held[0].digest, first.digest);
    assert_eq!(held[0].recorded.note.as_deref(), Some("plan"));
    assert_eq!(
        scope.recorded("one"),
        Some(held[0].recorded.clone()),
        "the call in flight holds what the row holds"
    );
    assert!(
        held[0].recorded.landed.contains("person:milhouse#"),
        "the row says what the store returned: {}",
        held[0].recorded.landed
    );

    // ── the second part, in the same call, lands with a row of its own ──────
    assert_eq!(
        captured_as_part(
            &memory,
            &scope,
            "two",
            None,
            claim_about(&milhouse, "wears a hat")
        )
        .await,
        (true, true)
    );
    assert_eq!(part_rows(&pool).await, 2);

    // ── a part another call wrote stores nothing, and its claim is rolled back
    let again = Scope::new(first.clone());
    assert_eq!(
        captured_as_part(
            &memory,
            &again,
            "one",
            None,
            claim_about(&milhouse, "owns a radio")
        )
        .await,
        (false, false),
        "a part with a row stores nothing"
    );
    assert_eq!(
        memory.recall(&milhouse).await.expect("recall").len(),
        2,
        "the repeated part was rolled back with its row"
    );
    assert_eq!(part_rows(&pool).await, 2);

    // ── another bot's parts under the same id are its own ───────────────────
    assert_eq!(
        ledger
            .parts(&EntityId("bot:omega".into()), &first.id)
            .await
            .expect("readable")
            .len(),
        0
    );

    // ── finishing spends the id, with the receipt ────────────────────────────
    assert!(
        ledger
            .lookup(&first.bot, &first.id)
            .await
            .unwrap()
            .is_none()
    );
    ledger
        .finish(&first, "{\"ok\":true}")
        .await
        .expect("finished");
    let spent = ledger
        .lookup(&first.bot, &first.id)
        .await
        .expect("readable")
        .expect("spent now");
    assert_eq!(spent.receipt.as_deref(), Some("{\"ok\":true}"));
    assert_eq!(spent.verb, "capture");
    assert_eq!(rows(&pool).await, 1);
    ledger
        .finish(&first, "{\"ok\":2}")
        .await
        .expect("finishing again is no error");
    assert_eq!(rows(&pool).await, 1, "one row for one id");

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_note_of_a_charter_change_is_a_part_of_its_own_in_the_real_store() {
    use jojobot_adapters::dolt::sessions::DoltSessions;
    use jojobot_domain::session::{CharterChange, Sessions};

    let dir = std::env::temp_dir().join(format!("jojobot-requests-charter-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_charter")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let sessions = DoltSessions::open(pool.clone());
    let ledger = DoltRequests::new(pool.clone());
    let gamma = thing(&memory, "bot:gamma", "Gamma").await;
    thing(&memory, "bot:sigma", "Sigma").await;
    let first = request_on("set_charter", "charter-part-1");
    let scope = Scope::new(first.clone());

    // The prose is part one, in the memory store.
    within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "charter",
            None,
            memory.set_prose(&gamma, "Holds the plan.", None),
        ),
    )
    .await
    .expect("the charter is set");
    // The note is part two, in the sessions store, in a transaction of its own.
    within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "charter_change",
            None,
            sessions.note_charter_change(
                &gamma,
                CharterChange {
                    by: EntityId("bot:sigma".into()),
                    at: Timestamp::now(),
                },
                None,
            ),
        ),
    )
    .await
    .expect("the change is noted");

    let names: Vec<String> = ledger
        .parts(&first.bot, &first.id)
        .await
        .expect("readable")
        .into_iter()
        .map(|row| row.part)
        .collect();
    assert_eq!(names, ["charter", "charter_change"]);
    let noted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM charter_change_write")
        .fetch_one(&pool)
        .await
        .expect("readable");
    assert_eq!(noted, 1);

    // The same part sent again stores nothing: its row refuses it, and the
    // note it would have written is rolled back with the row.
    let again = Scope::new(first.clone());
    let refused = within(
        Arc::clone(&again),
        jojobot_domain::request::armed_part(
            "charter_change",
            None,
            sessions.note_charter_change(
                &gamma,
                CharterChange {
                    by: EntityId("bot:sigma".into()),
                    at: Timestamp::now(),
                },
                None,
            ),
        ),
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(jojobot_domain::session::SessionError::Conflict)
        ),
        "the part another call wrote is a conflict: {refused:?}"
    );
    let noted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM charter_change_write")
        .fetch_one(&pool)
        .await
        .expect("readable");
    assert_eq!(noted, 1, "the repeated note was rolled back with its row");

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_restore_as_a_part_keeps_what_it_cleared_for_the_claim_that_records_it() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-restore-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_restore")
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
    memory
        .archive_entity(&milhouse, "listed \"twice\"", None)
        .await
        .expect("archived");
    let first = request_on("archive_entity", "restore-part-1");
    let scope = Scope::new(first.clone());

    let (_, was) = within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "restore",
            None,
            memory.restore_entity(&milhouse, None),
        ),
    )
    .await
    .expect("restored");

    let parts = ledger.parts(&first.bot, &first.id).await.expect("readable");
    assert_eq!(parts.len(), 1);
    let landed: serde_json::Value =
        serde_json::from_str(&parts[0].recorded.landed).expect("the row holds JSON");
    assert_eq!(landed["archived_reason"], "listed \"twice\"");
    assert_eq!(landed["archived_reason"], was.reason.as_str());
    assert_eq!(landed["archived_at"], was.at.to_string());

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rename_and_the_move_of_its_box_are_two_parts_in_the_real_store() {
    let dir = std::env::temp_dir().join(format!(
        "jojobot-requests-rename-box-{}",
        std::process::id()
    ));
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
    let mail = DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner));
    let ledger = DoltRequests::new(pool.clone());
    let delta = thing(&memory, "bot:delta", "Delta").await;
    thing(&memory, "bot:sigma", "Sigma").await;
    mail.create_mailbox(&MailboxName("delta".into()), &delta, None)
        .await
        .expect("the box opens");
    let epsilon = EntityId("bot:epsilon".into());
    let first = request_on("rename_entity", "rename-part-1");
    let scope = Scope::new(first.clone());

    let renamed = within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "rename",
            None,
            memory.rename_entity(
                &delta,
                &epsilon,
                None,
                date(2026, 9, 1),
                jojobot_domain::memory::Override::new(None, None),
                None,
            ),
        ),
    )
    .await
    .expect("renamed");
    assert!(renamed.written().is_some());
    let moved = within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part("box", None, mail.repoint_owner(&delta, &epsilon)),
    )
    .await
    .expect("repointed")
    .expect("the box was found");
    assert_eq!(moved.name.as_str(), "epsilon");

    let held = ledger.parts(&first.bot, &first.id).await.expect("readable");
    assert_eq!(
        held.iter().map(|row| row.part.as_str()).collect::<Vec<_>>(),
        ["rename", "box"]
    );
    assert!(
        held[1].recorded.landed.contains("epsilon"),
        "the box part says where the box went: {}",
        held[1].recorded.landed
    );
    assert_eq!(rows(&pool).await, 0, "the whole write waits for the verb");

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bot_and_its_box_are_two_parts_and_a_box_another_call_opened_is_not_opened_twice() {
    let dir = std::env::temp_dir().join(format!("jojobot-requests-bot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_bot")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let mail = DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner));
    let ledger = DoltRequests::new(pool.clone());
    thing(&memory, "bot:sigma", "Sigma").await;
    let delta = EntityId("bot:delta".into());
    let first = request_on("add_entity", "bot-part-1");
    let scope = Scope::new(first.clone());

    within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "entity",
            None,
            memory.add_entity(NewEntity::new(delta.clone(), "Delta", "fixture")),
        ),
    )
    .await
    .expect("the bot is made");
    let name = MailboxName("delta".into());
    within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part("box", None, mail.create_mailbox(&name, &delta, None)),
    )
    .await
    .expect("the box opens");

    let held = ledger.parts(&first.bot, &first.id).await.expect("readable");
    assert_eq!(
        held.iter().map(|row| row.part.as_str()).collect::<Vec<_>>(),
        ["entity", "box"]
    );
    assert!(held[1].recorded.landed.contains("delta"), "{held:?}");

    // A second call under the same id that tries the box part again stores
    // nothing: the part has its row, so the insert of the box is rolled back.
    let again = Scope::new(first.clone());
    let refused = within(
        Arc::clone(&again),
        jojobot_domain::request::armed_part(
            "box",
            None,
            mail.create_mailbox(&MailboxName("omega-box".into()), &delta, None),
        ),
    )
    .await;
    assert!(
        matches!(refused, Err(MailboxError::Conflict)),
        "the part another call wrote is a conflict: {refused:?}"
    );
    let boxes = mail.list_mailboxes().await.expect("readable");
    assert_eq!(
        boxes.iter().filter(|held| held.owner == delta).count(),
        1,
        "the second box was rolled back with its row"
    );

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_mail_a_post_took_for_its_sender_is_kept_with_the_part_that_took_it() {
    use jojobot_domain::mailbox::{Acting, TakenBy};

    let dir = std::env::temp_dir().join(format!("jojobot-requests-mail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_mail")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let mail = DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner));
    let ledger = DoltRequests::new(pool.clone());
    thing(&memory, "bot:sigma", "Sigma").await;
    let inbox = MailboxName("sigma".into());
    mail.create_mailbox(&inbox, &EntityId("bot:sigma".into()), None)
        .await
        .expect("a box")
        .written()
        .expect("not blocked");
    let waiting = mail
        .post_message(NewMessage {
            mailbox: inbox.clone(),
            body: "waiting".to_string(),
            subject: None,
            sender: "omega".to_string(),
            sent_at: Timestamp::now(),
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        })
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    let first = request_on("post_message", "mail-part-1");
    let scope = Scope::new(first.clone());

    let taken = within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "delivery",
            None,
            mail.read_mailbox(&inbox, TakenBy::Posting, Acting::unnamed(Timestamp::now())),
        ),
    )
    .await
    .expect("read")
    .written()
    .expect("not blocked");
    assert_eq!(taken.messages.len(), 1);

    let held = ledger.parts(&first.bot, &first.id).await.expect("readable");
    assert_eq!(held.len(), 1);
    let landed: serde_json::Value =
        serde_json::from_str(&held[0].recorded.landed).expect("the row holds JSON");
    assert_eq!(landed["ids"], serde_json::json!([waiting.id.as_str()]));
    assert_eq!(rows(&pool).await, 0, "the whole write waits for the verb");

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_entry_of_a_beat_and_the_focus_it_moves_are_two_parts_in_the_real_store() {
    use jojobot_adapters::dolt::sessions::DoltSessions;
    use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};

    let dir = std::env::temp_dir().join(format!("jojobot-requests-beat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "requests_beat")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let sessions = DoltSessions::open(pool.clone());
    let ledger = DoltRequests::new(pool.clone());
    thing(&memory, "bot:sigma", "Sigma").await;
    let now = Timestamp::now();
    let run = sessions
        .begin(NewSession {
            bot: EntityId("bot:sigma".into()),
            sid: Sid("brt1".into()),
            focus: "working".into(),
            started_at: now,
            timezone: None,
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("begin")
        .id;
    let first = request_on("journal", "beat-part-1");
    let scope = Scope::new(first.clone());

    let entry = within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "entry",
            None,
            sessions.append(&run, NewEntry::manual("found the cause", now, None)),
        ),
    )
    .await
    .expect("appended");
    within(
        Arc::clone(&scope),
        jojobot_domain::request::armed_part(
            "focus",
            None,
            sessions.set_focus(&run, "writing the fix"),
        ),
    )
    .await
    .expect("focus moved");

    let held = ledger.parts(&first.bot, &first.id).await.expect("readable");
    assert_eq!(
        held.iter().map(|row| row.part.as_str()).collect::<Vec<_>>(),
        ["entry", "focus"]
    );
    let landed: serde_json::Value =
        serde_json::from_str(&held[0].recorded.landed).expect("the row holds JSON");
    assert_eq!(landed["entry"], entry.id.as_str());
    assert!(held[1].recorded.landed.contains("writing the fix"));

    // The entry sent again under the id is a conflict, and nothing is appended.
    let again = Scope::new(first.clone());
    let refused = within(
        Arc::clone(&again),
        jojobot_domain::request::armed_part(
            "entry",
            None,
            sessions.append(&run, NewEntry::manual("found it twice", now, None)),
        ),
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(jojobot_domain::session::SessionError::Conflict)
        ),
        "{refused:?}"
    );
    let read = sessions.read_session(&run).await.expect("readable");
    assert_eq!(read.entries.len(), 1, "the repeated entry was rolled back");

    server.stop().await;
}

/// **No two cases of this file build their data directory under one name.**
///
/// Every case names its directory after the process, and the cases of one file
/// run in one process, so the label is all that tells two cases apart. Two
/// cases with one label start two servers on one data directory and delete it
/// when either ends: one start fails to initialise, or comes up on a directory
/// the other has emptied and waits for a database that is not there.
#[test]
fn every_case_in_this_file_builds_its_data_directory_under_its_own_name() {
    let source = include_str!("dolt_requests.rs");
    let needle = concat!("temp_dir().join(format!", "(");
    let mut labels: Vec<&str> = Vec::new();
    for (at, _) in source.match_indices(needle) {
        // rustfmt may put the label on the next line, so skip the whitespace.
        let rest = source[at + needle.len()..].trim_start();
        let rest = rest
            .strip_prefix('"')
            .expect("the label opens with a quote");
        labels.push(&rest[..rest.find('"').expect("the label ends")]);
    }
    assert!(
        labels.len() >= 19,
        "the scan must reach every case, or it asserts nothing: {}",
        labels.len()
    );
    let mut seen = std::collections::BTreeSet::new();
    let repeated: Vec<&str> = labels
        .iter()
        .copied()
        .filter(|l| !seen.insert(*l))
        .collect();
    assert!(
        repeated.is_empty(),
        "these directory names are used by more than one case: {repeated:?}"
    );
}

//! **The boot's storage check, over the real store.** A store written through the
//! real verbs, including a real merge, reports no damage. Each class is then
//! made by one edit to the rows, which is what a cut-off write leaves, and the
//! check names that class and no other.

use std::path::PathBuf;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::storage_check::DoltStorageCheck;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::storage_check::{StorageCheck, StorageClass};

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

/// A store with two merges, each made by the merge verb, and one claim moved by
/// each. Nothing in it is damaged.
async fn a_store_with_two_real_merges(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool) {
    let dir = jojobot_adapters::testing::scratch_path(label);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let scratch = Scratch(dir.clone());
    let server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, label)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    for (kept, kept_name, gone, gone_name) in [
        ("person:krusty", "Krusty", "person:skinner", "Skinner"),
        ("person:homer", "Homer", "person:apu", "Apu"),
    ] {
        let kept = thing(&memory, kept, kept_name).await;
        let gone = thing(&memory, gone, gone_name).await;
        let mut fact = NewFact::about(gone.clone(), "plays the saxophone", date(2026, 9, 1));
        fact.provenance = Provenance::Testimony;
        memory.capture(fact).await.expect("capture");
        memory
            .merge(
                &gone,
                &kept,
                Some("filed twice"),
                date(2026, 9, 2),
                &EntityId("bot:sigma".into()),
                Some("run-a"),
            )
            .await
            .expect("the merge lands");
    }
    (scratch, server, pool)
}

/// The classes that found anything, with their counts and ids.
async fn found(pool: &sqlx::MySqlPool) -> Vec<(StorageClass, u64, Vec<String>)> {
    let findings = DoltStorageCheck::open(pool.clone(), Clock::default())
        .check_storage()
        .await
        .expect("the check reads");
    assert_eq!(
        findings.iter().map(|f| f.class).collect::<Vec<_>>(),
        StorageClass::ALL,
        "every class is answered, in order"
    );
    findings
        .into_iter()
        .filter(|f| f.count > 0)
        .map(|f| (f.class, f.count, f.ids))
        .collect()
}

async fn run(pool: &sqlx::MySqlPool, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_store_written_through_the_real_verbs_reports_no_damage() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-clean").await;
    let merged: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE merged_into IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(
        merged, 2,
        "the store holds the merges the check must accept"
    );
    assert_eq!(found(&pool).await, vec![]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_class_is_named_by_the_one_edit_that_makes_it() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-classes").await;

    // A1: the merge's event is gone, the thing still forwards. Its claim is no
    // longer listed by any event either, so A4 names it too.
    run(
        &pool,
        "DELETE FROM merge_event WHERE folded = (SELECT badge FROM entity WHERE id = 'person:skinner')",
    )
    .await;
    let classes = |named: &[(StorageClass, u64, Vec<String>)]| -> Vec<StorageClass> {
        named.iter().map(|(class, _, _)| *class).collect()
    };
    let after_a1 = found(&pool).await;
    assert_eq!(
        classes(&after_a1),
        vec![
            StorageClass::MergedWithoutEvent,
            StorageClass::ClaimUnderFoldedKeyUnmapped
        ]
    );
    assert_eq!(after_a1[0].2, vec!["person:skinner".to_string()]);

    // A2: the event is there, the folded thing is not marked merged.
    run(
        &pool,
        "UPDATE entity SET merged_into = NULL WHERE id = 'person:apu'",
    )
    .await;
    let after_a2 = found(&pool).await;
    assert_eq!(
        classes(&after_a2),
        vec![
            StorageClass::MergedWithoutEvent,
            StorageClass::MergeEventNotApplied,
            StorageClass::ClaimUnderFoldedKeyUnmapped
        ],
        "A1 and A4 stand and A2 joins them: {after_a2:?}"
    );
    assert_eq!(after_a2[1].1, 1);

    // Back to whole, then A3 in its two shapes.
    run(
        &pool,
        "UPDATE entity SET merged_into = 'person:homer' WHERE id = 'person:apu'",
    )
    .await;
    run(
        &pool,
        "UPDATE entity SET merged_into = 'person:otto' WHERE id = 'person:skinner'",
    )
    .await;
    assert!(
        found(&pool).await.contains(&(
            StorageClass::MergedIntoNothing,
            1,
            vec!["person:skinner".to_string()]
        )),
        "a merge into nothing is named"
    );
    run(
        &pool,
        "UPDATE entity SET merged_into = 'person:apu' WHERE id = 'person:skinner'",
    )
    .await;
    run(
        &pool,
        "UPDATE entity SET merged_into = 'person:skinner' WHERE id = 'person:apu'",
    )
    .await;
    let looped = found(&pool).await;
    let (_, count, ids) = looped
        .iter()
        .find(|(class, _, _)| *class == StorageClass::MergedIntoNothing)
        .expect("a loop is named");
    assert_eq!(*count, 2, "both ends of the loop: {ids:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_write_cut_before_its_receipt_is_named_and_a_finished_one_is_not() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-requests").await;
    // Two requests with parts. One has its receipt; one does not.
    for request in ["finished", "cut"] {
        run(
            &pool,
            &format!(
                "INSERT INTO request_part (bot_key, request_id, part, verb, args_digest, landed, written_at)
                 VALUES ('bot-key-1', '{request}', 'a', 'capture', 'd', '[]', '2026-10-22T00:00:00.000000000Z'),
                        ('bot-key-1', '{request}', 'b', 'capture', 'd', '[]', '2026-10-22T00:00:01.000000000Z')"
            ),
        )
        .await;
    }
    run(
        &pool,
        "INSERT INTO request_receipt (bot_key, request_id, verb, args_digest, landed, receipt, written_at)
         VALUES ('bot-key-1', 'finished', 'capture', 'd', '[]', '{}', '2026-10-22T00:00:02.000000000Z')",
    )
    .await;
    let named = found(&pool).await;
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].0, StorageClass::HalfDoneWrite);
    assert_eq!(named[0].1, 1);
    assert!(
        named[0].2[0].contains("bot-key-1/cut") && named[0].2[0].contains("2 parts"),
        "the id says whose request, how many parts: {:?}",
        named[0].2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_receipt_never_filled_is_lost_only_after_ten_minutes() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-receipts").await;
    let now = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%S%.9fZ")
        .to_string();
    run(
        &pool,
        &format!(
            "INSERT INTO request_receipt (bot_key, request_id, verb, args_digest, landed, written_at)
             VALUES ('bot-key-1', 'just-now', 'capture', 'd', '[]', '{now}'),
                    ('bot-key-1', 'long-ago', 'update_fact', 'd', '[]', '2026-01-01T00:00:00.000000000Z')"
        ),
    )
    .await;
    let named = found(&pool).await;
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].0, StorageClass::LostReceipt);
    assert_eq!(named[0].1, 1);
    assert!(named[0].2[0].contains("long-ago") && named[0].2[0].contains("update_fact"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_class_with_many_rows_counts_them_all_and_names_twenty() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-many").await;
    for n in 0..25 {
        run(
            &pool,
            &format!(
                "INSERT INTO request_part (bot_key, request_id, part, verb, args_digest, landed, written_at)
                 VALUES ('bot-key-1', 'cut-{n:02}', 'a', 'capture', 'd', '[]', '2026-10-22T00:00:{n:02}.000000000Z')"
            ),
        )
        .await;
    }
    let named = found(&pool).await;
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].1, 25, "the count is whole");
    assert_eq!(
        named[0].2.len(),
        jojobot_domain::storage_check::IDS_SHOWN,
        "the ids are the first twenty"
    );
    assert!(
        named[0].2[0].contains("cut-00"),
        "oldest first: {:?}",
        named[0].2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_under_a_folded_key_that_no_event_lists_is_named() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-unmapped").await;
    assert_eq!(
        found(&pool).await,
        vec![],
        "the merges are whole before the edit"
    );
    run(
        &pool,
        "DELETE FROM merge_event WHERE ordinal > 0
           AND folded = (SELECT badge FROM entity WHERE id = 'person:skinner')",
    )
    .await;
    let named = found(&pool).await;
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].0, StorageClass::ClaimUnderFoldedKeyUnmapped);
    assert_eq!(named[0].1, 1);
    assert!(
        named[0].2[0].starts_with("person:skinner#"),
        "{:?}",
        named[0].2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_column_that_names_a_thing_is_checked_and_a_supplied_handle_is_not_dangling() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-dangling").await;
    for sql in [
        "UPDATE entity SET parent = 'person:milhouse' WHERE id = 'person:krusty'",
        "UPDATE fact SET edge_object = 'person:nelson'
           WHERE entity = (SELECT badge FROM entity WHERE id = 'person:skinner')",
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
           VALUES ('ghosthome', 'k', 1, 'v', 'f1'),
                  ('view:ready', 'k', 1, 'v', 'f2')",
        "INSERT INTO field_link (entity, `key`, ordinal, target)
           VALUES ('person:krusty', 'k', 1, 'person:ralph')",
        "INSERT INTO mailbox (name, owner) VALUES ('orphan-box', 'bot:nelson')",
        "INSERT INTO session (id, bot, focus, started_at, state)
           VALUES ('abcd', 'bot:otto', 'f', '2026-10-22T00:00:00Z', 'active')",
        "INSERT INTO message (id, mailbox, ordinal, body, sender, sent_at, state)
           VALUES ('m1', 'no-such-box', 1, 'b', 'bot:sigma', '2026-10-22T00:00:00Z', 'new')",
        "INSERT INTO message_event (message_id, ordinal, kind, at, prior_state)
           VALUES ('no-such-message', 1, 'read', '2026-10-22T00:00:00Z', 'new')",
        "INSERT INTO entity_former_handle (former_handle, badge, changed_at)
           VALUES ('person:otto', 'zzzzzzzz', '2026-10-22')",
        // A handle the build supplies has no row, and a pointer at it is whole.
        "INSERT INTO supplied_handle (handle) VALUES ('role:dev')",
        "INSERT INTO field_link (entity, `key`, ordinal, target)
           VALUES ('person:krusty', 'k', 2, 'role:dev')",
    ] {
        run(&pool, sql).await;
    }
    let named = found(&pool).await;
    let dangling = named
        .iter()
        .find(|(class, _, _)| *class == StorageClass::DanglingReference)
        .expect("the class is named");
    let mut ids = dangling.2.clone();
    ids.sort();
    let mut want = vec![
        "entity.parent: person:milhouse",
        "entity_former_handle.badge: zzzzzzzz",
        "fact.edge_object: person:nelson",
        "field_link.target: person:ralph",
        "field_write.entity: ghosthome",
        "mailbox.owner: bot:nelson",
        "message.mailbox: no-such-box",
        "message_event.message_id: no-such-message",
        "session.bot: bot:otto",
    ];
    want.sort();
    assert_eq!(
        ids, want,
        "one id for each column, none for the supplied handle and none for a home keyed by a handle"
    );
    assert_eq!(dangling.1, 9);
}

/// A merge made the way the verb makes it under a request id, so the store holds
/// its receipt. Returns the pool's store scratch.
async fn a_store_with_a_merge_under_a_request_id(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool) {
    use jojobot_domain::request::{Request, RequestId, Scope, armed, digest_of, within};
    let dir = jojobot_adapters::testing::scratch_path(label);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let scratch = Scratch(dir.clone());
    let server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, label)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let bot = thing(&memory, "bot:sigma", "Sigma").await;
    let kept = thing(&memory, "person:krusty", "Krusty").await;
    let gone = thing(&memory, "person:skinner", "Skinner").await;
    let scope = Scope::new(Request {
        bot: bot.clone(),
        id: RequestId::parse("merge-1").expect("a good id"),
        verb: "merge_entities".to_string(),
        digest: digest_of("{}"),
    });
    within(
        scope,
        armed(memory.merge(
            &gone,
            &kept,
            Some("filed twice"),
            date(2026, 9, 2),
            &bot,
            Some("run-a"),
        )),
    )
    .await
    .expect("the merge lands");
    (scratch, server, pool)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_receipt_is_whole_while_the_merge_stands_even_after_a_rename() {
    let (_scratch, _server, pool) =
        a_store_with_a_merge_under_a_request_id("storage-check-d2").await;
    let receipts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM request_receipt WHERE verb = 'merge_entities'")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(receipts, 1, "the merge wrote the receipt the check reads");
    assert_eq!(found(&pool).await, vec![]);

    // The survivor is renamed after the merge: the receipt still names the old
    // handle, and the merge still stands.
    let memory = DoltMemory::open(pool.clone());
    memory
        .rename_entity(
            &EntityId("person:krusty".into()),
            &EntityId("person:nelson".into()),
            None,
            date(2026, 9, 3),
            jojobot_domain::memory::Override::none(),
            Some("run-a"),
        )
        .await
        .expect("the rename")
        .written()
        .expect("not blocked");
    assert_eq!(
        found(&pool).await,
        vec![],
        "a renamed survivor is not damage"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_receipt_whose_merge_is_not_in_the_store_is_named() {
    let (_scratch, _server, pool) =
        a_store_with_a_merge_under_a_request_id("storage-check-d2b").await;
    run(
        &pool,
        "UPDATE entity SET merged_into = NULL WHERE id = 'person:skinner'",
    )
    .await;
    let named = found(&pool).await;
    let landed = named
        .iter()
        .find(|(class, _, _)| *class == StorageClass::LandedMergeMissing)
        .expect("the receipt's merge is missing");
    assert_eq!(landed.1, 1);
    assert!(
        landed.2[0].contains("merge-1")
            && landed.2[0].contains("person:skinner into person:krusty"),
        "{:?}",
        landed.2
    );
}

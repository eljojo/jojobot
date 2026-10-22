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

/// A stamp as the ledger writes it, this far before now.
fn ago(hours: i64) -> String {
    (jiff::Timestamp::now() - jiff::SignedDuration::from_hours(hours))
        .strftime("%Y-%m-%dT%H:%M:%S%.9fZ")
        .to_string()
}

/// Two parts of one request, the second a second after the first.
async fn two_parts(pool: &sqlx::MySqlPool, request: &str, hours_ago: i64) {
    let first = ago(hours_ago);
    let second = (jiff::Timestamp::now() - jiff::SignedDuration::from_hours(hours_ago)
        + jiff::SignedDuration::from_secs(1))
    .strftime("%Y-%m-%dT%H:%M:%S%.9fZ")
    .to_string();
    run(
        pool,
        &format!(
            "INSERT INTO request_part (bot_key, request_id, part, verb, args_digest, landed, written_at)
             VALUES ('bot-key-1', '{request}', 'a', 'capture', 'd', '[]', '{first}'),
                    ('bot-key-1', '{request}', 'b', 'capture', 'd', '[]', '{second}')"
        ),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_write_cut_before_its_receipt_is_named_and_a_finished_one_is_not() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-requests").await;
    // Two requests with parts. One has its receipt; one does not.
    for request in ["finished", "cut"] {
        two_parts(&pool, request, 1).await;
    }
    let receipt_at = ago(0);
    run(
        &pool,
        &format!(
            "INSERT INTO request_receipt (bot_key, request_id, verb, args_digest, landed, receipt, written_at)
             VALUES ('bot-key-1', 'finished', 'capture', 'd', '[]', '{{}}', '{receipt_at}')"
        ),
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

/// **A request older than the ledger keeps its ids is not a half-done write.**
/// A finished request leaves its parts and its receipt; a later write purges
/// the receipt after the keep window and leaves the parts, so parts with no
/// receipt are what a finished request looks like once it ages. A request cut
/// part-way and left past the window can no longer be finished by a resend,
/// which the ledger answers as a new call. Only a request still inside the
/// window is damage a resend can repair.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_request_past_the_keep_window_is_not_a_half_done_write() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-aged").await;
    two_parts(&pool, "young-cut", 71).await;
    two_parts(&pool, "aged-finished", 4 * 24).await;
    two_parts(&pool, "aged-cut", 73).await;
    let named = found(&pool).await;
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].0, StorageClass::HalfDoneWrite);
    assert_eq!(named[0].1, 1, "only the young one: {named:?}");
    assert!(
        named[0].2[0].contains("bot-key-1/young-cut"),
        "the request inside the window is the one named: {:?}",
        named[0].2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_receipt_never_filled_is_lost_only_after_ten_minutes() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-receipts").await;
    let now = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%S%.9fZ")
        .to_string();
    let minutes_ago = |minutes: i64| {
        (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(minutes))
            .strftime("%Y-%m-%dT%H:%M:%S%.9fZ")
            .to_string()
    };
    let (nine, eleven) = (minutes_ago(9), minutes_ago(11));
    run(
        &pool,
        &format!(
            "INSERT INTO request_receipt (bot_key, request_id, verb, args_digest, landed, written_at)
             VALUES ('bot-key-1', 'just-now', 'capture', 'd', '[]', '{now}'),
                    ('bot-key-1', 'nine-minutes', 'capture', 'd', '[]', '{nine}'),
                    ('bot-key-1', 'eleven-minutes', 'capture', 'd', '[]', '{eleven}'),
                    ('bot-key-1', 'long-ago', 'update_fact', 'd', '[]', '2026-01-01T00:00:00.000000000Z')"
        ),
    )
    .await;
    let named = found(&pool).await;
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].0, StorageClass::LostReceipt);
    // The grace is ten minutes: nine is inside it, eleven is past it.
    assert_eq!(named[0].1, 2, "{named:?}");
    let ids = named[0].2.join(" | ");
    assert!(
        ids.contains("long-ago") && ids.contains("update_fact") && ids.contains("eleven-minutes"),
        "{ids}"
    );
    assert!(
        !ids.contains("nine-minutes") && !ids.contains("just-now"),
        "a receipt inside the grace is not lost: {ids}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_class_with_many_rows_counts_them_all_and_names_twenty() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-many").await;
    let base = jiff::Timestamp::now() - jiff::SignedDuration::from_hours(1);
    for n in 0..25 {
        let at = (base + jiff::SignedDuration::from_secs(n))
            .strftime("%Y-%m-%dT%H:%M:%S%.9fZ")
            .to_string();
        run(
            &pool,
            &format!(
                "INSERT INTO request_part (bot_key, request_id, part, verb, args_digest, landed, written_at)
                 VALUES ('bot-key-1', 'cut-{n:02}', 'a', 'capture', 'd', '[]', '{at}')"
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
        "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
           VALUES ('ghostbadge', 'f8', 'a claim under a badge no row wears', 'testimony', 'active', '2026-10-22')",
        // A write keeps its bot as an id: one that names a thing is whole, one that
        // names nothing is dangling.
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, status, recorded_at, bot)
           VALUES ('ghostbadge', 'f8', 1, 'c', 'testimony', 'active', '2026-10-22', 'ghostbot'),
                  ('ghostbadge', 'f8', 2, 'c', 'testimony', 'active', '2026-10-22',
                   (SELECT badge FROM entity WHERE id = 'person:skinner'))",
        "INSERT INTO mailbox (name, owner) VALUES ('orphan-box', 'bot:nelson')",
        "INSERT INTO session (id, bot, focus, started_at, state)
           VALUES ('abcd', 'bot:otto', 'f', '2026-10-22T00:00:00Z', 'active')",
        "INSERT INTO message (id, mailbox, ordinal, body, sender, sent_at, state)
           VALUES ('m1', 'no-such-box', 1, 'b', 'bot:sigma', '2026-10-22T00:00:00Z', 'new')",
        "INSERT INTO message_event (message_id, ordinal, kind, at, prior_state)
           VALUES ('no-such-message', 1, 'read', '2026-10-22T00:00:00Z', 'new')",
        "INSERT INTO entity_former_handle (former_handle, badge, changed_at)
           VALUES ('person:otto', 'zzzzzzzz', '2026-10-22')",
        "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
           VALUES ('view:ready', 'f9', 'a claim under a handle with no row', 'testimony', 'active', '2026-10-22')",
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
        "fact.entity: ghostbadge",
        "fact_write.bot: ghostbot",
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
    assert_eq!(dangling.1, 11);

    // The writes homed under a handle are a class of their own, named with the
    // claim's address and the field write's key.
    let homed = named
        .iter()
        .find(|(class, _, _)| *class == StorageClass::HandleHomedWrite)
        .expect("the writes under a handle with no row are named");
    let mut homed_ids = homed.2.clone();
    homed_ids.sort();
    assert_eq!(
        homed_ids,
        vec![
            "fact: view:ready#f9".to_string(),
            "field_write: view:ready/k".to_string()
        ]
    );
    assert_eq!(homed.1, 2);
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

/// **A handle a rename freed and a new thing took is not a merge gone missing.**
/// The receipt names the survivor by the handle it had when the merge landed. The
/// survivor is renamed away and a new thing takes that handle, so the handle is
/// current for one thing and former for another. The merge stands into the thing
/// that held the handle then, and the check must find it there. A folded thing
/// cannot be renamed, so the survivor's handle is the one that can be retaken.
/// Paired with the case above, where a rename frees the handle and nothing takes
/// it, and with the next one, where the merge is really gone and is still named.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_receipt_is_whole_when_the_survivors_handle_was_freed_and_taken_again() {
    let (_scratch, _server, pool) =
        a_store_with_a_merge_under_a_request_id("storage-check-d2c").await;
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
    // A rename leaves the display name as it was, and the write guard refuses
    // a new thing whose handle spells that name; the survivor is called
    // something else first, as an operator's later edit would.
    run(
        &pool,
        "UPDATE entity SET name = 'Principal' WHERE id = 'person:nelson'",
    )
    .await;
    thing(&memory, "person:krusty", "Seymour").await;
    assert_eq!(
        found(&pool).await,
        vec![],
        "a handle freed by a rename and taken by a new thing is not damage"
    );

    // The negative it rests on: with the merge really gone, the same store still
    // names the receipt, so reading both wearers did not make the check blind.
    run(
        &pool,
        "UPDATE entity SET merged_into = NULL WHERE id = 'person:skinner'",
    )
    .await;
    let named = found(&pool).await;
    assert!(
        named
            .iter()
            .any(|(class, count, _)| *class == StorageClass::LandedMergeMissing && *count == 1),
        "a merge that is gone is still named: {named:?}"
    );
}

/// **A handle two different things have left is still the survivor's.** The
/// survivor is renamed away, a new thing takes its handle, and that thing is
/// renamed away in turn, so the former-handle table holds two rows for one
/// handle, one for each thing that wore it. The receipt means the first, and the
/// check must read both rows rather than the one a map happens to keep.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_receipt_is_whole_when_two_things_have_left_the_survivors_handle() {
    let (_scratch, _server, pool) =
        a_store_with_a_merge_under_a_request_id("storage-check-d2d").await;
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
        .expect("the first rename")
        .written()
        .expect("not blocked");
    run(
        &pool,
        "UPDATE entity SET name = 'Principal' WHERE id = 'person:nelson'",
    )
    .await;
    thing(&memory, "person:krusty", "Seymour").await;
    memory
        .rename_entity(
            &EntityId("person:krusty".into()),
            &EntityId("person:vance".into()),
            None,
            date(2026, 9, 4),
            jojobot_domain::memory::Override::none(),
            Some("run-a"),
        )
        .await
        .expect("the second rename")
        .written()
        .expect("not blocked");
    let wearers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entity_former_handle WHERE former_handle = 'person:krusty'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(wearers, 2, "two things have left the handle");
    assert_eq!(
        found(&pool).await,
        vec![],
        "the merge stands into the first wearer of the handle"
    );

    // The negative it rests on: with the merge really gone, both wearers are
    // read and neither holds it, so the receipt is still named.
    run(
        &pool,
        "UPDATE entity SET merged_into = NULL WHERE id = 'person:skinner'",
    )
    .await;
    let named = found(&pool).await;
    assert!(
        named
            .iter()
            .any(|(class, count, _)| *class == StorageClass::LandedMergeMissing && *count == 1),
        "a merge that is gone is still named: {named:?}"
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

/// A word of letters that depends on nothing but `n`, spread far enough apart
/// that the write guard's near-miss screen takes no two for one.
fn word(n: usize) -> String {
    let mut n = n * 7919 + 104_729;
    let mut letters = String::new();
    for _ in 0..8 {
        letters.push((b'a' + (n % 26) as u8) as char);
        n = n / 26 * 31 + 17;
    }
    letters
}

/// **The check's time does not grow with the number of things times the number
/// of rows.** A store of 700 things, each with a claim and two fields written
/// through the verbs, is filled out to about 4,900 claims and 22,000 field writes
/// by copying rows the verbs wrote under new ids, which is the shape a long-used
/// store has. A check that tests every claim and field write against every thing
/// takes tens of seconds here. This one finds no damage and answers inside two
/// seconds, and finds the damage a single edit makes in that same store.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_check_over_a_large_store_answers_inside_two_seconds() {
    let dir = jojobot_adapters::testing::scratch_path("storage-check-large");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "storage-check-large")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    for n in 0..700 {
        let id = EntityId(format!("thing:{}", word(n)));
        memory
            .add_entity(NewEntity::new(id.clone(), word(n + 5000), "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        let mut fact = NewFact::about(id, "a claim", date(2026, 9, 1));
        fact.fields.insert("note".into(), "one".into());
        fact.fields.insert("size".into(), "two".into());
        memory.capture(fact).await.expect("capture");
    }
    // Copy the rows the verbs wrote, under new ids, until the store has the
    // weight of a long-used one. Every copy names a thing that exists.
    let fact_columns: Vec<String> = sqlx::query_scalar(
        "SELECT CAST(column_name AS CHAR) FROM information_schema.columns
         WHERE table_schema = DATABASE() AND table_name = 'fact' ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .expect("the columns of fact");
    let names = fact_columns
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ");
    for copy in 1..=6 {
        let picked = fact_columns
            .iter()
            .map(|c| {
                if c == "id" {
                    format!("CONCAT(`id`, '-c{copy}')")
                } else {
                    format!("`{c}`")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        run(
            &pool,
            &format!(
                "INSERT INTO fact ({names}) SELECT {picked} FROM fact WHERE id NOT LIKE '%-c%'"
            ),
        )
        .await;
    }
    for copy in 1..=15 {
        run(
            &pool,
            &format!(
                "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
                 SELECT entity, `key`, ordinal + {}, value, fact_id FROM field_write
                 WHERE ordinal < 1000",
                1000 * copy
            ),
        )
        .await;
    }
    let writes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_write")
        .fetch_one(&pool)
        .await
        .expect("a count");
    let claims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&pool)
        .await
        .expect("a count");
    assert!(
        writes >= 20_000 && claims >= 4_800,
        "the store is as heavy as the case says: {claims} claims, {writes} field writes"
    );

    let started = std::time::Instant::now();
    let named = found(&pool).await;
    let took = started.elapsed();
    eprintln!("the check took {took:?} over {claims} claims and {writes} field writes");
    assert!(
        named.is_empty(),
        "a store the verbs wrote has no damage: {named:?}"
    );
    assert!(
        took < std::time::Duration::from_secs(2),
        "the check took {took:?} over {claims} claims and {writes} field writes"
    );
    // The positive beside it: the same store, one edit, and the check names it.
    run(
        &pool,
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
         VALUES ('ghosthome', 'k', 1, 'v', 'f1')",
    )
    .await;
    let after = found(&pool).await;
    assert_eq!(after.len(), 1, "{after:?}");
    assert_eq!(after[0].0, StorageClass::DanglingReference);
    assert_eq!(
        after[0].2,
        vec!["field_write.entity: ghosthome".to_string()]
    );
    drop(server);
}

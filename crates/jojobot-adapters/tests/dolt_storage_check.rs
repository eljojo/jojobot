//! **The boot's storage check, over the real store.** A store written through the
//! real verbs, including a real merge, reports no damage. Each class is then
//! made by one edit to the rows, which is what a cut-off write leaves, and the
//! check names that class and no other.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::storage_check::DoltStorageCheck;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::storage_check::{StorageCheck, StorageClass};

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
    thing(&memory, "bot:sigma", "Sigma").await;
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
    let findings = DoltStorageCheck::open(pool.clone())
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

/// Old ledger rows no longer describe writes once the log is authoritative.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_storage_check_does_not_read_the_retired_ledger_tables() {
    let (_scratch, _server, pool) =
        a_store_with_two_real_merges("storage-check-retired-ledgers").await;
    assert!(found(&pool).await.is_empty());
    two_parts(&pool, "legacy-cut", 1).await;
    let at = ago(1);
    sqlx::query(
        "INSERT INTO request_receipt \
         (bot_key, request_id, verb, args_digest, landed, receipt, written_at) \
         VALUES ('bot-key-1', 'legacy-lost', 'capture', 'd', '[]', NULL, ?), \
                ('bot-key-1', 'legacy-merge', 'merge_entities', 'd', '{}', '{}', ?)",
    )
    .bind(&at)
    .bind(&at)
    .execute(&pool)
    .await
    .expect("the old ledger rows stand");
    let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_receipt")
        .fetch_one(&pool)
        .await
        .expect("the old receipts count");
    let parts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_part")
        .fetch_one(&pool)
        .await
        .expect("the old parts count");
    assert_eq!((receipts, parts), (2, 2));
    let named = found(&pool).await;
    assert!(
        named.is_empty(),
        "retired ledger rows are not storage damage: {named:?}"
    );
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
async fn a_class_with_many_rows_counts_them_all_and_names_twenty() {
    let (_scratch, _server, pool) = a_store_with_two_real_merges("storage-check-many").await;
    for n in 0..25 {
        let at = format!("2026-09-{:02}", n + 1);
        run(
            &pool,
            &format!(
                "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
                 VALUES ('cut-{n:02}', 'f1', 'a claim under a badge no row wears', 'testimony', 'active', '{at}')"
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
        // The claims import files a link to a supplied record by its supply identity,
        // the handle behind a prefix, and that pointer is whole too. A prefix on a
        // handle nothing supplies is still dangling.
        "INSERT INTO field_link (entity, `key`, ordinal, target)
           VALUES ('person:krusty', 'k', 3, 'supplied/role:dev')",
        "INSERT INTO field_link (entity, `key`, ordinal, target)
           VALUES ('person:krusty', 'k', 4, 'supplied/bot:otto')",
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
        "field_link.target: supplied/bot:otto",
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
    assert_eq!(dangling.1, 12);

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
/// takes tens of seconds here. This one finds no damage and answers well inside
/// ten seconds (its fastest of three runs), and finds the damage a single edit
/// makes in that same store.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_check_over_a_large_store_does_not_test_every_claim_against_every_thing() {
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

    // **The bound separates a check that reads the store a few times from one that
    // tests every claim against every thing, and not a quiet runner from a busy
    // one.** The check answers in about two seconds here; the quadratic check this
    // case exists to refuse takes tens of seconds, on every run. A runner under load
    // slows one run, for a while, by a factor nobody sets, and a 2 s line reddened a
    // release gate at 2.04 s while a loaded runner took 45 s once. So the check is
    // run up to three times, and the fastest run is judged against ten seconds, five
    // times what it takes quiet: a regression is slow every time and still fails, and
    // one busy moment does not.
    let mut fastest = std::time::Duration::MAX;
    for _ in 0..3 {
        let started = std::time::Instant::now();
        let named = found(&pool).await;
        let took = started.elapsed();
        eprintln!("the check took {took:?} over {claims} claims and {writes} field writes");
        assert!(
            named.is_empty(),
            "a store the verbs wrote has no damage: {named:?}"
        );
        fastest = fastest.min(took);
        if fastest < std::time::Duration::from_secs(2) {
            break;
        }
    }
    assert!(
        fastest < std::time::Duration::from_secs(10),
        "the fastest of three runs of the check took {fastest:?} over {claims} claims and \
         {writes} field writes"
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

//! **The boot storage check reports what is damaged and no other row**, class by
//! class, over the real store: what an id says, how a count relates to its ids,
//! which thing a handle meant when a merge landed, and which sources are checked.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::storage_check::DoltStorageCheck;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Override};
use jojobot_domain::request::{Request, RequestId, Scope, armed, digest_of, within};
use jojobot_domain::storage_check::{StorageCheck, StorageClass};

async fn a_store(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool, DoltMemory) {
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
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    (scratch, server, pool, memory)
}

async fn thing(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

async fn run(pool: &sqlx::MySqlPool, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the thing has a key")
}

/// What the check found for one class: its count and its ids.
async fn found(pool: &sqlx::MySqlPool, class: StorageClass) -> (u64, Vec<String>) {
    let finding = DoltStorageCheck::open(pool.clone(), Clock::default())
        .check_storage()
        .await
        .expect("the check reads")
        .into_iter()
        .find(|finding| finding.class == class)
        .expect("every class is answered");
    (finding.count, finding.ids)
}

/// A merge made as the verb makes it under a request id, so the store holds its
/// receipt: person:skinner folded into person:krusty.
async fn a_merge_under_a_request_id(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool, DoltMemory) {
    let (scratch, server, pool, memory) = a_store(label).await;
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
    (scratch, server, pool, memory)
}

mod precision {
    use super::*;

    /// **An id says which of two things is wrong**, because the way out differs: a
    /// row that is not marked merged can be marked, a missing row cannot.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_merge_event_not_applied_says_whether_the_row_is_unmarked_or_missing() {
        let (_scratch, _server, pool, _memory) = a_merge_under_a_request_id("precision-a2").await;
        let key = badge_of(&pool, "person:skinner").await;
        run(
            &pool,
            "UPDATE entity SET merged_into = NULL WHERE id = 'person:skinner'",
        )
        .await;
        assert_eq!(
            found(&pool, StorageClass::MergeEventNotApplied).await,
            (1, vec![format!("{key}: not marked merged")])
        );
        run(&pool, "DELETE FROM entity WHERE id = 'person:skinner'").await;
        assert_eq!(
            found(&pool, StorageClass::MergeEventNotApplied).await,
            (1, vec![format!("{key}: no row")])
        );
    }

    /// **A dangling reference counts rows and names values**, and the doc of a
    /// finding says so: three link rows that hold one ghost value count three and name
    /// one id.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_dangling_reference_counts_its_rows_and_names_its_value_once() {
        let (_scratch, _server, pool, memory) = a_store("precision-count").await;
        let holder = thing(&memory, "person:krusty", "Krusty").await;
        let key = badge_of(&pool, holder.as_str()).await;
        for ordinal in 1..=3 {
            run(
                &pool,
                &format!(
                    "INSERT INTO field_link (entity, `key`, ordinal, target) \
                     VALUES ('{key}', 'friend', {ordinal}, 'zz9999')"
                ),
            )
            .await;
        }
        assert_eq!(
            found(&pool, StorageClass::DanglingReference).await,
            (3, vec!["field_link.target: zz9999".to_string()])
        );
    }

    /// **A claim derived from a source homed under a handle is left alone**, as a
    /// claim homed under one is: the handle may be one a build once supplied. A source
    /// homed under a bare badge no row wears is damage.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_source_homed_under_a_handle_is_left_alone_and_one_under_a_lost_badge_is_not() {
        let (_scratch, _server, pool, memory) = a_store("precision-derived").await;
        let holder = thing(&memory, "person:krusty", "Krusty").await;
        memory
            .capture(NewFact::about(
                holder.clone(),
                "kept a ledger",
                date(2026, 9, 1),
            ))
            .await
            .expect("capture")
            .written()
            .expect("lands");
        run(
            &pool,
            "UPDATE fact SET derived_from = 'person:marge', derived_from_id = 'f1'",
        )
        .await;
        assert_eq!(
            found(&pool, StorageClass::DanglingReference).await,
            (0, vec![]),
            "a handle-homed source is not damage"
        );
        run(
            &pool,
            "UPDATE fact SET derived_from = 'zz9999', derived_from_id = 'f1'",
        )
        .await;
        assert_eq!(
            found(&pool, StorageClass::DanglingReference).await,
            (1, vec!["fact.derived_from: zz9999".to_string()]),
            "a lost badge is damage"
        );
    }

    /// **A scan of a merged thing reads the pointer the index reads.** The forwarding
    /// row names the survivor by a handle it has since left; the index serves the
    /// handle it wears now, and so must the scan.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_scan_of_a_merged_thing_serves_the_survivor_under_its_current_handle() {
        let (_scratch, _server, pool, memory) = a_merge_under_a_request_id("precision-scan").await;
        memory
            .rename_entity(
                &EntityId("person:krusty".into()),
                &EntityId("person:nelson".into()),
                None,
                date(2026, 9, 3),
                Override::none(),
                Some("run-a"),
            )
            .await
            .expect("the rename")
            .written()
            .expect("not blocked");
        // The forwarding row as a path that did not keep it current left it.
        run(
            &pool,
            "UPDATE entity SET merged_into = 'person:krusty' WHERE id = 'person:skinner'",
        )
        .await;
        let folded = EntityId("person:skinner".into());
        let listed = memory
            .list_entities(None)
            .await
            .expect("the index lists")
            .into_iter()
            .find(|entity| entity.id == folded)
            .expect("the folded thing is listed");
        assert_eq!(
            listed.merged_into.as_ref().map(EntityId::as_str),
            Some("person:nelson"),
            "the index serves the handle the survivor wears now"
        );
        let scanned = memory
            .scan_entity(&folded)
            .await
            .expect("the scan answers")
            .and_then(|doc| doc.entity)
            .expect("the scan names the entity");
        assert_eq!(scanned.merged_into, listed.merged_into);
    }

    /// **A handle a rename freed and a new thing took does not hide a lost merge.**
    /// The receipt names the survivor by the handle it wore when the merge landed. The
    /// survivor is renamed away, a new thing takes the handle, the merge is lost, and
    /// the folded thing is merged into the new thing by another act. Both wore the
    /// handle; the receipt meant the first. The check asks which wore it when the
    /// receipt was written and finds the merge missing; it used to accept any wearer.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_lost_merge_is_named_when_the_retaker_of_the_survivors_handle_holds_the_folded_thing()
    {
        let (_scratch, _server, pool, memory) = a_merge_under_a_request_id("precision-d2").await;
        memory
            .rename_entity(
                &EntityId("person:krusty".into()),
                &EntityId("person:nelson".into()),
                None,
                date(2026, 9, 3),
                Override::none(),
                Some("run-a"),
            )
            .await
            .expect("the rename")
            .written()
            .expect("not blocked");
        run(
            &pool,
            "UPDATE entity SET name = 'Principal' WHERE id = 'person:nelson'",
        )
        .await;
        let retaker = thing(&memory, "person:krusty", "Seymour").await;
        assert_eq!(
            found(&pool, StorageClass::LandedMergeMissing).await.0,
            0,
            "while the merge stands the receipt is whole"
        );
        // The merge is lost, and the folded thing is merged into the retaker.
        run(
            &pool,
            "UPDATE entity SET merged_into = NULL WHERE id = 'person:skinner'",
        )
        .await;
        memory
            .merge(
                &EntityId("person:skinner".into()),
                &retaker,
                Some("filed again"),
                date(2026, 9, 5),
                &EntityId("bot:sigma".into()),
                Some("run-b"),
            )
            .await
            .expect("the second merge lands");
        let (count, ids) = found(&pool, StorageClass::LandedMergeMissing).await;
        assert_eq!(count, 1, "the receipt's merge is not in the store: {ids:?}");
        assert!(
            ids[0].contains("person:skinner into person:krusty"),
            "{ids:?}"
        );
    }
}

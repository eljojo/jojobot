//! **The storage check names a dangling link only while its write still serves.**
//! An edit appends a write beside the old one and leaves the old write and its link
//! rows. A dangling bare name that a served edit has replaced is history, and the
//! check stops naming it; the same name on the current write is damage.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::storage_check::DoltStorageCheck;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{EntityId, EntityKind, FactPatch, Memory, NewEntity, NewFact};
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
    memory
        .declare_type(
            DeclaredType::new(
                "dispatch",
                vec![Field::pointing_at("line", EntityKind::BOT)],
            ),
            None,
        )
        .await
        .expect("declare line");
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

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the thing has a key")
}

/// A work item given a line through the verbs, then damaged the way an old build
/// left it: the stored value and its link hold the bare name. Answers the claim.
async fn bare_line(
    memory: &DoltMemory,
    pool: &sqlx::MySqlPool,
    work: (&str, &str),
    bot: &EntityId,
) -> jojobot_domain::memory::Fact {
    let home = thing(memory, work.0, work.1).await;
    let mut fact = NewFact::about(home.clone(), "assigned a line", date(2026, 9, 1));
    fact.fields.insert("line".into(), bot.as_str().into());
    fact.sets.insert("line".into());
    let claim = memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("the handle lands");
    let key = badge_of(pool, home.as_str()).await;
    for sql in [
        "UPDATE field_write SET value = 'beta' WHERE entity = ? AND `key` = 'line'",
        "UPDATE field_link SET target = 'beta' WHERE entity = ? AND `key` = 'line'",
    ] {
        sqlx::query(sql)
            .bind(&key)
            .execute(pool)
            .await
            .expect("the old damage");
    }
    claim
}

async fn dangling(pool: &sqlx::MySqlPool) -> (u64, Vec<String>) {
    let finding = DoltStorageCheck::open(pool.clone(), Clock::default())
        .check_storage()
        .await
        .expect("the check reads")
        .into_iter()
        .find(|finding| finding.class == StorageClass::DanglingReference)
        .expect("every class is answered");
    (finding.count, finding.ids)
}

fn correcting_to(bot: &EntityId) -> FactPatch {
    FactPatch {
        fields: [("line".into(), bot.as_str().into())].into_iter().collect(),
        sets: ["line".into()].into_iter().collect(),
        ..Default::default()
    }
}

/// **A served correction ends the report, a value left on its write does not.**
/// Two work items hold the bare name beta. One is corrected with update_fact; the
/// other is not. The check names only the one that was not. The old write and its
/// link row of the corrected item are still stored: nothing was deleted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dangling_link_a_served_edit_replaced_is_history_and_one_left_is_damage() {
    let (_scratch, _server, pool, memory) = a_store("check-current-edit").await;
    let bot = thing(&memory, "bot:beta", "Beta").await;
    let corrected = bare_line(&memory, &pool, ("work:alpha-queue", "Alpha queue"), &bot).await;
    let left = bare_line(&memory, &pool, ("work:beta-queue", "Beta queue"), &bot).await;
    assert_eq!(dangling(&pool).await.0, 2, "both rows are damage first");

    memory
        .update_fact(
            &corrected.address(),
            correcting_to(&bot),
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("edit")
        .written()
        .expect("the corrected handle lands");

    let (rows, ids) = dangling(&pool).await;
    assert_eq!(
        (rows, ids),
        (1, vec!["field_link.target: beta".to_string()]),
        "the replaced row is history; the row on the current write is damage"
    );
    let home_key = badge_of(&pool, corrected.subject.as_str()).await;
    let still: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_link WHERE entity = ? AND `key` = 'line' AND target = 'beta'",
    )
    .bind(&home_key)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(still, 1, "the old link row is kept; history is not deleted");
    assert_eq!(
        memory.fields(&corrected.subject).await.expect("fields")["line"],
        bot.as_str(),
        "reads serve the corrected handle"
    );
    let left_key = badge_of(&pool, left.subject.as_str()).await;
    let current: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM field_write WHERE entity = ? AND `key` = 'line'")
            .bind(&left_key)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(current, 1, "the item left alone has only its damaged write");
}

/// **A clear is a later write too**, and a correction by a new claim is not: a
/// new claim has a record of its own, so the old record's write stays current and
/// the check keeps naming it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_clear_replaces_a_dangling_link_and_a_new_claim_does_not() {
    let (_scratch, _server, pool, memory) = a_store("check-current-clear").await;
    let bot = thing(&memory, "bot:beta", "Beta").await;
    let cleared = bare_line(&memory, &pool, ("work:alpha-queue", "Alpha queue"), &bot).await;
    let restated = bare_line(&memory, &pool, ("work:beta-queue", "Beta queue"), &bot).await;
    assert_eq!(dangling(&pool).await.0, 2);

    memory
        .update_fact(
            &cleared.address(),
            FactPatch {
                clear_fields: vec!["line".into()],
                ..Default::default()
            },
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("edit")
        .written()
        .expect("the clear lands");
    let mut again = NewFact::about(
        restated.subject.clone(),
        "assigned a line",
        date(2026, 9, 2),
    );
    again.fields.insert("line".into(), bot.as_str().into());
    again.sets.insert("line".into());
    memory
        .capture(again)
        .await
        .expect("capture")
        .written()
        .expect("the handle lands");

    let (rows, ids) = dangling(&pool).await;
    assert_eq!(
        (rows, ids),
        (1, vec!["field_link.target: beta".to_string()]),
        "the cleared row is history; a new claim leaves the old record's row damage"
    );
}

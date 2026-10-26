//! **The rollback that makes the link rule safe to switch on, proved by real boots
//! of the current binary** on the store the last release filled (rule 646). The
//! rule goes on, then off, then on again. Off, every read serves what the rule
//! before it served, and a write made while it is off keeps its link when the rule
//! comes back.

use super::super::*;
use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{EntityId, EntityKind, Guarded, Memory, NewEntity, NewFact};

const HOLDER: &str = "thing:upgrade-fixture-holder";
const LISTED: &str = "thing:upgrade-fixture-heir";
const GUARDED_BY: &str = "thing:upgrade-fixture-husk";

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the thing has a badge")
}

async fn opened(pool: &sqlx::MySqlPool) -> DoltMemory {
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    memory
}

async fn added(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    let new = NewEntity::new(id.clone(), name, "test");
    // The recorded store holds things with close names, and the screen asks the
    // caller to say these are other things.
    if let Guarded::Blocked {
        attempted,
        candidates,
    } = memory.add_entity(new.clone()).await.expect("add")
    {
        let token = jojobot_domain::memory::guard::override_token(&attempted, &candidates);
        memory
            .add_entity(NewEntity {
                override_token: Some(token),
                override_reason: Some("a thing of this case, not one of the recording".into()),
                ..new
            })
            .await
            .expect("add")
            .written()
            .expect("told apart");
    }
    id
}

async fn claim(memory: &DoltMemory, home: &EntityId, key: &str, value: &str, said: &str) {
    let mut fact = NewFact::about(home.clone(), said, date(2026, 9, 1));
    fact.fields.insert(key.into(), value.into());
    fact.sets.insert(key.into());
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("the claim lands");
}

/// **Every row of the link table**, as `(entity, key, ordinal, target)`, read with
/// no binary over the store.
async fn link_rows(
    pool: &sqlx::MySqlPool,
) -> std::collections::BTreeSet<(String, String, i64, String)> {
    sqlx::query_as::<_, (String, String, i64, String)>(
        "SELECT entity, `key`, ordinal, target FROM field_link",
    )
    .fetch_all(pool)
    .await
    .expect("the link table reads")
    .into_iter()
    .collect()
}

/// **What the holder serves for `words`**, read through the served surface.
async fn recalled(booted: &Booted) -> String {
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let read = surface
        .call("recall", json!({"subject": HOLDER, "facts": true}))
        .await;
    surface.finish().await;
    read
}

/// **On, then log-only, then on again, over the last release's store.** The store
/// holds a list that mixes a word with a handle, and a key declared a reference.
/// The first boot with the rule on marks the list; the boot with it off serves the
/// list as the rule before it served it, with no mark in the answer; a write made
/// while it is off is stored as that rule stores it; and the boot with the rule
/// on again marks that write and keeps its link row.
#[tokio::test]
async fn a_rollback_to_log_only_reads_as_9_2_3_did_and_the_way_back_to_on_keeps_every_link() {
    for restored in deploy_9_1::both_recordings(&fixture_dir(), "rollback-round-trip").await {
        the_round_trip_over(restored).await;
    }
}

async fn the_round_trip_over(restored: Restored) {
    deploy_9_1::as_a_recording_before_the_marking(&restored.state_dir, restored.store_port).await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let pool = store.pool().clone();
    jojobot_adapters::dolt::migrate::run(&pool)
        .await
        .expect("the current migrations run over the recording");
    let memory = opened(&pool).await;
    let holder = added(&memory, HOLDER, "A Holder").await;
    added(&memory, LISTED, "A Listed Heir").await;
    added(&memory, GUARDED_BY, "A Guarding Husk").await;
    memory
        .declare_type(
            DeclaredType::new(
                "upgrade-fixture-guard",
                vec![Field::pointing_at("guarded_by", EntityKind::THING)],
            ),
            None,
        )
        .await
        .expect("declare guarded_by");
    // **The store as the last release left it**: the list is stored as written,
    // because the rule before this one lowers only a value of nothing but handles.
    claim(
        &memory,
        &holder,
        "labels",
        &format!("kitchen, {LISTED}"),
        "lists a word and a thing",
    )
    .await;
    let listed_badge = badge_of(&pool, LISTED).await;
    let guard_badge = badge_of(&pool, GUARDED_BY).await;
    let holder_badge = badge_of(&pool, HOLDER).await;
    store.stop().await;

    let on = deploy_9_1::boot_with_link_rule("on", &restored).await;
    on.stop().await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let marked: String =
        sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = 'labels'")
            .bind(&holder_badge)
            .fetch_one(store.pool())
            .await
            .expect("the list reads");
    let after_on = link_rows(store.pool()).await;
    store.stop().await;
    assert_eq!(
        marked,
        format!("kitchen, @#{listed_badge}"),
        "recorded at {}: the boot with the rule on marks the handle in the list",
        restored.git_ref
    );

    let off = deploy_9_1::boot_with_link_rule(deploy_9_1::LOG_ONLY, &restored).await;
    let read = recalled(&off).await;
    off.stop().await;
    assert!(
        read.contains(&format!("kitchen, {LISTED}")) && !read.contains("@#"),
        "recorded at {}: the boot with the rule off serves the list as written, with no mark: {read}",
        restored.git_ref
    );
    // **The boot with the rule off changes no link row.** The table is read before
    // and after it, so a row the rollback drops cannot come back unseen under the
    // warning the way back to on is allowed to log.
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let after_off = link_rows(store.pool()).await;
    store.stop().await;
    assert!(
        after_off.iter().any(|(entity, key, _, target)| {
            entity == &holder_badge && key == "labels" && target == &listed_badge
        }),
        "recorded at {}: the boot with the rule off dropped the link of the marked list: {after_off:?}",
        restored.git_ref
    );
    assert_eq!(
        after_off, after_on,
        "recorded at {}: the boot with the rule off changed a link row",
        restored.git_ref
    );

    // **A write made while the rule is off**, in the process the binary would run:
    // a key declared a reference is stored as the bare id, with its link row.
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let pool = store.pool().clone();
    let memory = opened(&pool).await;
    claim(
        &memory,
        &holder,
        "guarded_by",
        GUARDED_BY,
        "names what guards it",
    )
    .await;
    let links = |pool: sqlx::MySqlPool, key: &'static str| {
        let holder_badge = holder_badge.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT target FROM field_link WHERE entity = ? AND `key` = ? ORDER BY target",
            )
            .bind(holder_badge)
            .bind(key)
            .fetch_all(&pool)
            .await
            .expect("the link table reads")
        }
    };
    assert_eq!(
        links(pool.clone(), "guarded_by").await,
        vec![guard_badge.clone()],
        "the write made while the rule is off draws its link"
    );
    let with_write = link_rows(&pool).await;
    store.stop().await;
    let drawn: Vec<_> = with_write.difference(&after_off).collect();
    assert!(
        drawn.len() == 1
            && drawn[0].0 == holder_badge
            && drawn[0].1 == "guarded_by"
            && drawn[0].3 == guard_badge,
        "the write made while the rule is off draws exactly its own row: {drawn:?}"
    );

    // The table check finds the row the earlier rule drew for the write made while
    // the rule was off, and says so before it keeps the row: the gate lists that
    // one warning, which every other boot of this case must not log.
    let on_again =
        deploy_9_1::boot_with_link_rule_tolerating("on", &restored, &["LINK TABLE DISAGREED"])
            .await;
    let disagreed: Vec<String> = on_again
        .log
        .iter()
        .filter(|line| line.contains("LINK TABLE DISAGREED"))
        .cloned()
        .collect();
    on_again.stop().await;
    // **The tolerated warning is one line, and it says the table lost and gained
    // nothing.** Tolerating it by a word would also tolerate a boot that dropped
    // rows and put them back, so its own counts are held: the leading space keeps
    // `linked=0` from matching inside `unlinked=0`.
    assert!(
        disagreed.len() == 1
            && disagreed[0].contains(" linked=0")
            && disagreed[0].contains(" unlinked=0"),
        "recorded at {}: the warning the way back to on may log is one line with no row added or removed: {disagreed:?}",
        restored.git_ref
    );
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let pool = store.pool().clone();
    assert_eq!(
        link_rows(&pool).await,
        with_write,
        "recorded at {}: the way back to on changed a link row",
        restored.git_ref
    );
    let guarded: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = ? AND `key` = 'guarded_by'",
    )
    .bind(&holder_badge)
    .fetch_one(&pool)
    .await
    .expect("the reference reads");
    let kept = links(pool.clone(), "guarded_by").await;
    let listed = links(pool, "labels").await;
    store.stop().await;
    assert_eq!(
        kept,
        vec![guard_badge.clone()],
        "recorded at {}: the write made while the rule was off keeps its link",
        restored.git_ref
    );
    assert_eq!(
        guarded,
        format!("@#{guard_badge}"),
        "recorded at {}: the boot with the rule on marks that write",
        restored.git_ref
    );
    assert_eq!(
        listed,
        vec![listed_badge],
        "recorded at {}: the marked list keeps its link after the round trip",
        restored.git_ref
    );
    drop(restored);
}

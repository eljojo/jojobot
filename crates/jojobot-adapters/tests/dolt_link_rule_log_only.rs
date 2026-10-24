//! **The link rule of rule 646 while it is log-only, over the real store.** The
//! process never turns the rule on here. A boot and a write leave every stored
//! byte, link row and pass marker exactly as the rule before it leaves them, and
//! the summary counts what the rule would do without naming a value.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::{DoltMemory, LinkRuleSummary};
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityId, LinkRule, Memory, NewEntity, NewFact, link_rule};

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the entity row is readable")
        .expect("a created entity is given a badge")
}

async fn set_raw(pool: &sqlx::MySqlPool, holder_badge: &str, key: &str, value: &str) {
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ?")
        .bind(value)
        .bind(holder_badge)
        .bind(key)
        .execute(pool)
        .await
        .expect("the stored shape is written");
}

/// **Every row the link rule could touch**, as text: the field writes, the link
/// table and the pass markers.
async fn dump(pool: &sqlx::MySqlPool) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let writes: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT entity, `key`, ordinal, value FROM field_write ORDER BY entity, `key`, ordinal",
    )
    .fetch_all(pool)
    .await
    .expect("the writes read");
    rows.extend(writes.iter().map(|row| format!("write {row:?}")));
    let links: Vec<(String, String, i64, String)> =
        sqlx::query_as("SELECT entity, `key`, ordinal, target FROM field_link ORDER BY 1, 2, 3, 4")
            .fetch_all(pool)
            .await
            .expect("the links read");
    rows.extend(links.iter().map(|row| format!("link {row:?}")));
    let passes: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("the markers read");
    rows.extend(passes.iter().map(|name| format!("pass {name}")));
    rows
}

#[tokio::test]
async fn a_log_only_boot_and_write_change_nothing_and_the_summary_counts_the_rule() {
    assert_eq!(
        link_rule(),
        LinkRule::LogOnly,
        "this process never switches it"
    );
    let scratch = Scratch::new("link-rule-log-only");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "link_rule_log_only")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory_store = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory_store)
        .await
        .expect("the kinds are seeded");
    let [holder, alpha, beta, was, now] = [
        "thing:contract-links-holder",
        "work:contract-links-alpha",
        "work:contract-links-beta",
        "work:contract-links-was",
        "work:contract-links-now",
    ]
    .map(|handle| EntityId(handle.into()));
    for (id, label) in [
        (&holder, "Holder"),
        (&alpha, "Alpha"),
        (&beta, "Beta"),
        (&was, "Was"),
    ] {
        memory_store
            .add_entity(NewEntity::new(id.clone(), label, "contract-fixture"))
            .await
            .expect("add_entity ok")
            .written()
            .expect("nothing collides with it");
    }
    memory_store
        .declare_type(
            DeclaredType::new(
                "contract-links-friended",
                vec![Field::listing("friends", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("declare_type ok");
    // **A write under the rule before this one**: a declared key stores a bare
    // id, a list with a loose item stays plain text.
    memory_store
        .capture(memory::state(NewFact {
            fields: [
                ("friends".to_string(), alpha.to_string()),
                ("labels".to_string(), format!("kitchen, {beta}")),
                ("owner_of".to_string(), alpha.to_string()),
                ("ghost".to_string(), alpha.to_string()),
                ("held_by".to_string(), was.to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(holder.clone(), "worded before the mark", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    let alpha_badge = badge_of(&pool, alpha.as_str()).await;
    let beta_badge = badge_of(&pool, beta.as_str()).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM field_write WHERE entity = ? AND `key` = 'friends'"
        )
        .bind(&holder_badge)
        .fetch_one(&pool)
        .await
        .expect("reads"),
        alpha_badge,
        "a declared key is stored as a bare id, as before"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM field_write WHERE entity = ? AND `key` = 'labels'"
        )
        .bind(&holder_badge)
        .fetch_one(&pool)
        .await
        .expect("reads"),
        format!("kitchen, {beta}"),
        "a list with a loose item stays plain text, as before"
    );

    // The boot passes of the rule before run once and finish.
    memory_store
        .migrate_field_links()
        .await
        .expect("the pass runs");

    // **Left behind by an older pass**: a value of one handle it could not place
    // then and one thing owns today, and a handle two things have answered to.
    memory_store
        .rename_entity(
            &was,
            &now,
            None,
            date(2026, 8, 2),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the new handle");
    memory_store
        .add_entity(NewEntity::new(was.clone(), "Newcomer", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the freed name is free");
    set_raw(&pool, &holder_badge, "owner_of", alpha.as_str()).await;
    set_raw(&pool, &holder_badge, "held_by", was.as_str()).await;
    set_raw(&pool, &holder_badge, "ghost", "thing:contract-moved-first").await;
    sqlx::query(
        "DELETE FROM field_link WHERE entity = ? AND `key` IN ('owner_of', 'held_by', 'ghost')",
    )
    .bind(&holder_badge)
    .execute(&pool)
    .await
    .expect("the stale rows go");

    let before = dump(&pool).await;
    assert!(
        !before
            .iter()
            .any(|row| row.contains("field value links marked")),
        "the marking pass is not finished before: {before:?}"
    );

    // **Two boots**: the passes the boot runs, then the summary the boot logs.
    let mut summaries: Vec<LinkRuleSummary> = Vec::new();
    for _ in 0..2 {
        memory_store
            .migrate_field_links()
            .await
            .expect("the pass settles");
        summaries.push(
            memory_store
                .link_rule_summary()
                .await
                .expect("the summary reads"),
        );
        assert_eq!(
            dump(&pool).await,
            before,
            "a boot changes nothing it did not change before"
        );
    }
    assert_eq!(
        summaries[0], summaries[1],
        "the second boot counts what the first did"
    );
    assert_eq!(
        summaries[0],
        LinkRuleSummary {
            values_read: 5,
            bare_ids: 1,
            mixed_lists: 1,
            handles: 2,
            unresolved: 1,
            ambiguous: 1,
            owned_now: 1,
            links_dropped: 0,
            links_added: 2,
        },
        "the counts of what the rule would do over the seeded data"
    );

    // **A write**: stored as before, nothing marked.
    memory_store
        .capture(memory::state(NewFact {
            fields: [
                ("friends".to_string(), beta.to_string()),
                ("labels_two".to_string(), format!("kitchen, {alpha}")),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                holder.clone(),
                "worded after, still log-only",
                date(2026, 8, 3),
            )
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let stored = |key: &'static str| {
        let pool = pool.clone();
        let badge = holder_badge.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM field_write WHERE entity = ? AND `key` = ? ORDER BY ordinal DESC LIMIT 1",
            )
            .bind(badge)
            .bind(key)
            .fetch_one(&pool)
            .await
            .expect("reads")
        }
    };
    assert_eq!(stored("friends").await, beta_badge, "still a bare id");
    assert_eq!(
        stored("labels_two").await,
        format!("kitchen, {alpha}"),
        "still plain text"
    );
    assert!(
        !dump(&pool)
            .await
            .iter()
            .any(|row| row.contains("field value links marked")),
        "no write finishes the marking pass"
    );
    store.stop().await;
}

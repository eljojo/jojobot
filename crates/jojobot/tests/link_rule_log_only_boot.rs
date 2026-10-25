//! **The boot step of the link rule, run the way the boot runs it.** The rule is
//! chosen by the environment parser the boot uses, both for an unset variable and
//! for an explicit `log-only`, and the step is `wiring::field_links::boot_field_links`,
//! the one the boot calls. Two boots over old-shape data change no write, link row
//! or pass marker, and each logs the counts of what the rule would do.
//!
//! **A test binary of its own**: the switch is process-wide.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{
    EntityId, EntityKind, LinkRule, Memory, NewEntity, NewFact, link_rule,
};

#[derive(Clone, Default)]
struct Kept(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Kept {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Kept {
    type Writer = Kept;
    fn make_writer(&'a self) -> Kept {
        self.clone()
    }
}

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

async fn the_boot_changes_nothing(word: Option<&str>) {
    jojobot::wiring::set_link_rule_from(word).expect("the word is one the boot accepts");
    assert_eq!(link_rule(), LinkRule::LogOnly, "{word:?} selects log-only");
    let scratch = Scratch::new(&format!("link-rule-boot-{}", word.unwrap_or("unset")));
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "link_rule_boot")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory_store = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory_store)
        .await
        .expect("the kinds are seeded");
    let [holder, alpha, beta, was, now, bot] = [
        "thing:contract-links-holder",
        "work:contract-links-alpha",
        "work:contract-links-beta",
        "work:contract-links-was",
        "work:contract-links-now",
        "bot:defrag",
    ]
    .map(|handle| EntityId(handle.into()));
    for (id, label) in [
        (&holder, "Holder"),
        (&alpha, "Alpha"),
        (&beta, "Beta"),
        (&was, "Was"),
        (&bot, "Defrag"),
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
                vec![
                    Field::listing("friends", ValueType::Reference),
                    Field::pointing_at("line", EntityKind::BOT),
                ],
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
                ("line".to_string(), bot.to_string()),
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
    set_raw(&pool, &holder_badge, "line", "defrag").await;
    sqlx::query(
        "DELETE FROM field_link WHERE entity = ? AND `key` IN ('owner_of', 'held_by', 'ghost', 'line')",
    )
    .bind(&holder_badge)
    .execute(&pool)
    .await
    .expect("the stale rows go");
    sqlx::query(
        "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, 'line', 1, 'defrag')",
    )
    .bind(&holder_badge)
    .execute(&pool)
    .await
    .expect("the old bare line row");
    let line: (String, String) = sqlx::query_as(
        "SELECT w.value, l.target FROM field_write w JOIN field_link l
         ON w.entity = l.entity AND w.`key` = l.`key` AND w.ordinal = l.ordinal
         WHERE w.entity = ? AND w.`key` = 'line'",
    )
    .bind(&holder_badge)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(line, ("defrag".into(), "defrag".into()));

    let before = dump(&pool).await;
    assert!(
        !before
            .iter()
            .any(|row| row.contains("field value links marked")),
        "the marking pass is not finished before: {before:?}"
    );

    let before = dump(&pool).await;
    assert!(
        !before
            .iter()
            .any(|row| row.contains("field value links marked")),
        "the marking pass is not finished before: {before:?}"
    );
    for boot in 1..=2 {
        let kept = Kept::default();
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(kept.clone())
                .with_ansi(false)
                .with_max_level(tracing::Level::INFO)
                .finish(),
        );
        jojobot::wiring::field_links::boot_field_links(&memory_store).await;
        drop(guard);
        let log = String::from_utf8_lossy(&kept.0.lock().unwrap()).into_owned();
        assert_eq!(
            dump(&pool).await,
            before,
            "boot {boot} with {word:?} changes nothing"
        );
        for expected in [
            "link rule (log-only)",
            "values_read=6",
            "bare_ids=1",
            "mixed_lists=1",
            "handles=1",
            "unresolved=1",
            "ambiguous=1",
            "r1_owned_now=1",
            "links_dropped=1",
            "links_added=1",
            "inflated_by=",
        ] {
            assert!(
                log.contains(expected),
                "boot {boot} with {word:?} logs {expected}: {log}"
            );
        }
        assert!(!log.contains("kitchen"), "the log names no value: {log}");
    }
    store.stop().await;
}

#[tokio::test]
async fn an_unset_switch_boots_log_only_and_changes_nothing() {
    the_boot_changes_nothing(None).await;
}

#[tokio::test]
async fn an_explicit_log_only_boots_log_only_and_changes_nothing() {
    the_boot_changes_nothing(Some("log-only")).await;
}

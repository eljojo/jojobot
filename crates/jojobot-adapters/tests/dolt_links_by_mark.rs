//! **Whether a field value links is decided when it is written and stored as the
//! mark on the permanent id, over the real store** (rules 355 and 646). A comma
//! item that is a handle is marked under any key and gets a link row; loose text
//! beside it is kept; a type declared afterwards moves no row. Values written
//! before this (plain handle text, a bare id under a declared reference key) are
//! marked once by the boot pass.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

const MARK: &str = "@#";

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the entity row is readable")
        .expect("a created entity is given a badge")
}

async fn stored(pool: &sqlx::MySqlPool, holder_badge: &str, key: &str) -> String {
    sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = ?")
        .bind(holder_badge)
        .bind(key)
        .fetch_one(pool)
        .await
        .expect("the stored value reads")
}

async fn link_rows(pool: &sqlx::MySqlPool) -> Vec<(String, String, i64, String)> {
    sqlx::query_as("SELECT entity, `key`, ordinal, target FROM field_link ORDER BY 1, 2, 3, 4")
        .fetch_all(pool)
        .await
        .expect("the link table reads")
}

/// A store with a holder and two things it can name.
async fn world(name: &str) -> (Scratch, Dolt, sqlx::MySqlPool, DoltMemory, [EntityId; 3]) {
    jojobot_domain::memory::set_link_rule(jojobot_domain::memory::LinkRule::On);
    let scratch = Scratch::new(name);
    let store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, name)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    let things = [
        "thing:contract-links-holder",
        "work:contract-links-alpha",
        "work:contract-links-beta",
    ]
    .map(|handle| EntityId(handle.into()));
    for (id, label) in things.iter().zip(["Holder", "Alpha", "Beta"]) {
        memory
            .add_entity(NewEntity::new(id.clone(), label, "contract-fixture"))
            .await
            .expect("add_entity ok")
            .written()
            .expect("nothing collides with it");
    }
    (scratch, store, pool, memory, things)
}

/// **A list with a loose item among its handles is stored with the handle
/// marked and the loose item as it was, and has one link row; declaring the key
/// a list of references afterwards moves no row.**
#[tokio::test]
async fn a_mixed_list_is_stored_marked_and_a_later_declaration_moves_no_row() {
    let (_scratch, mut store, pool, memory, [holder, alpha, _]) =
        world("links-by-mark-write").await;
    let held = memory
        .capture(memory::state(NewFact {
            fields: [
                ("labels".to_string(), format!("kitchen, {alpha}")),
                ("note".to_string(), "only words".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(holder.clone(), "worded loosely", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let (holder_badge, alpha_badge) = (
        badge_of(&pool, holder.as_str()).await,
        badge_of(&pool, alpha.as_str()).await,
    );
    assert_eq!(
        stored(&pool, &holder_badge, "labels").await,
        format!("kitchen, {MARK}{alpha_badge}"),
        "the handle wears the mark and the loose item is as written"
    );
    let rows = link_rows(&pool).await;
    assert_eq!(
        rows,
        vec![(holder_badge.clone(), "labels".into(), 1, alpha_badge)],
        "one link row, for the one item that is a link"
    );

    memory
        .declare_type(
            DeclaredType::new(
                "contract-links-labelled",
                vec![Field::listing("labels", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("declare_type ok");
    assert_eq!(link_rows(&pool).await, rows, "a declaration moves no row");
    let pointing: Vec<String> = memory
        .referring_to(&alpha)
        .await
        .expect("answers")
        .iter()
        .map(|fact| fact.address().to_string())
        .collect();
    assert_eq!(pointing, vec![held.address().to_string()]);
    store.stop().await;
}

/// **The boot pass marks the two shapes a value had before the mark decided it:
/// a bare id under a declared reference key, and the handles in a list stored as
/// plain text. Loose text is untouched, the link table is built from the marks,
/// and a second run changes nothing.**
#[tokio::test]
async fn the_marking_pass_marks_a_bare_id_and_the_handles_of_a_plain_list() {
    let (_scratch, mut store, pool, memory, [holder, alpha, beta]) =
        world("links-by-mark-pass").await;
    memory
        .declare_type(
            DeclaredType::new(
                "contract-links-friended",
                vec![Field::listing("friends", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("declare_type ok");
    memory
        .capture(memory::state(NewFact {
            fields: [
                ("friends".to_string(), alpha.to_string()),
                ("labels".to_string(), format!("kitchen, {beta}")),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(holder.clone(), "names two things", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    let alpha_badge = badge_of(&pool, alpha.as_str()).await;
    let beta_badge = badge_of(&pool, beta.as_str()).await;

    // **Written back to what a build from before the mark stored.**
    for (key, old) in [
        ("friends", alpha_badge.clone()),
        ("labels", format!("kitchen, {beta}")),
    ] {
        sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ?")
            .bind(&old)
            .bind(&holder_badge)
            .bind(key)
            .execute(&pool)
            .await
            .expect("the old shape is written");
    }
    sqlx::query("DELETE FROM field_link")
        .execute(&pool)
        .await
        .expect("the table starts empty");

    let report = memory
        .migrate_field_links_marked()
        .await
        .expect("the pass runs");
    assert_eq!(report.lowered, 2, "both values are marked: {report:?}");
    assert!(report.left_as_text.is_empty(), "{report:?}");
    assert_eq!(
        stored(&pool, &holder_badge, "friends").await,
        format!("{MARK}{alpha_badge}")
    );
    assert_eq!(
        stored(&pool, &holder_badge, "labels").await,
        format!("kitchen, {MARK}{beta_badge}"),
        "the loose item stays where it was"
    );
    assert_eq!(
        link_rows(&pool).await,
        vec![
            (holder_badge.clone(), "friends".into(), 1, alpha_badge),
            (holder_badge, "labels".into(), 1, beta_badge),
        ],
        "the table is built from the marks"
    );
    let again = memory
        .migrate_field_links_marked()
        .await
        .expect("a second run");
    assert!(again.settled && again.lowered == 0, "{again:?}");
    store.stop().await;
}

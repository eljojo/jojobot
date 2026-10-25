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

#[path = "dolt_links_by_mark/readers.rs"]
mod readers;

#[path = "dolt_links_by_mark/reader_contract.rs"]
mod reader_contract;

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

/// **R1: a handle an older pass left as text is not handed to whoever owns the
/// spelling today.** The older pass finished and left a value of one handle it
/// could not place. A thing wears that spelling now, and the marking pass must
/// keep the older pass's decision: the value stays text, links nothing and is
/// listed.
#[tokio::test]
async fn an_older_passs_unresolved_handle_is_not_given_to_a_later_owner() {
    let (_scratch, mut store, pool, memory, [holder, alpha, _]) = world("links-by-mark-r1").await;
    memory
        .capture(memory::state(NewFact {
            fields: [("owner_of".to_string(), alpha.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "names one thing", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    // The older pass finished, and left this value as plain text with no link.
    sqlx::query("INSERT INTO store_pass (name, finished_at) VALUES ('field value lowering', '2026-06-01T00:00:00Z')")
        .execute(&pool)
        .await
        .expect("the older pass is marked finished");
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'owner_of'")
        .bind(alpha.as_str())
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .expect("the old shape is written");
    sqlx::query("DELETE FROM field_link")
        .execute(&pool)
        .await
        .expect("the table starts empty");

    let report = memory
        .migrate_field_links_marked()
        .await
        .expect("the pass runs");
    assert_eq!(report.lowered, 0, "nothing is marked: {report:?}");
    assert_eq!(
        stored(&pool, &holder_badge, "owner_of").await,
        alpha.to_string(),
        "the value stays the text the older pass left"
    );
    assert!(link_rows(&pool).await.is_empty(), "and links nothing");
    assert_eq!(report.left_as_text.len(), 1, "and is listed: {report:?}");
    store.stop().await;
}

#[tokio::test]
async fn link_rule_leftovers_an_unfinished_older_pass_allows_a_known_handle_to_convert() {
    let (_scratch, mut store, pool, memory, [holder, alpha, beta]) =
        world("links_by_mark_older_open").await;
    memory
        .capture(memory::state(NewFact {
            fields: [
                ("owner_of".into(), alpha.to_string()),
                ("labels".into(), beta.to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(holder.clone(), "written before the pass", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let holder_id = badge_of(&pool, holder.as_str()).await;
    let alpha_id = badge_of(&pool, alpha.as_str()).await;
    let beta_id = badge_of(&pool, beta.as_str()).await;
    for (key, value) in [
        ("owner_of", alpha.to_string()),
        ("labels", format!("kitchen, {beta}")),
    ] {
        sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ?")
            .bind(value)
            .bind(&holder_id)
            .bind(key)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM field_link")
        .execute(&pool)
        .await
        .unwrap();
    let report = memory.migrate_field_links_marked().await.unwrap();
    assert_eq!(report.lowered, 2, "{report:?}");
    assert!(report.left_as_text.is_empty(), "{report:?}");
    assert_eq!(
        stored(&pool, &holder_id, "owner_of").await,
        format!("{MARK}{alpha_id}")
    );
    assert_eq!(
        stored(&pool, &holder_id, "labels").await,
        format!("kitchen, {MARK}{beta_id}")
    );
    assert_eq!(
        link_rows(&pool).await,
        vec![
            (holder_id.clone(), "labels".into(), 1, beta_id),
            (holder_id, "owner_of".into(), 1, alpha_id),
        ]
    );
    assert_eq!(memory.referring_to(&alpha).await.unwrap().len(), 1);
    assert_eq!(memory.referring_to(&beta).await.unwrap().len(), 1);
    store.stop().await;
}

#[tokio::test]
async fn link_rule_leftovers_a_finished_pass_keeps_mixed_text_through_name_reuse_and_restarts() {
    const DATABASE: &str = "links_by_mark_negative_history";
    let (scratch, mut store, mut pool, mut memory, [holder, alpha, _]) = world(DATABASE).await;
    let [was, now, ambiguous, moved] = [
        "work:contract-links-was",
        "work:contract-links-now",
        "thing:contract-moved-first",
        "thing:contract-moved-fourth",
    ]
    .map(|id| EntityId(id.into()));
    memory
        .add_entity(NewEntity::new(ambiguous.clone(), "Earlier", "fixture"))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory
        .rename_entity(
            &ambiguous,
            &moved,
            None,
            date(2026, 8, 2),
            Default::default(),
            None,
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    memory
        .add_entity(NewEntity::new(ambiguous.clone(), "Replacement", "fixture"))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory
        .capture(memory::state(NewFact {
            fields: [("labels".into(), alpha.to_string())].into_iter().collect(),
            ..NewFact::about(
                holder.clone(),
                "keeps old negative decisions",
                date(2026, 8, 1),
            )
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let holder_id = badge_of(&pool, holder.as_str()).await;
    let alpha_id = badge_of(&pool, alpha.as_str()).await;
    let raw = format!("kitchen, {was}, {ambiguous}, {MARK}{alpha_id}");
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'labels'")
        .bind(&raw)
        .bind(&holder_id)
        .execute(&pool)
        .await
        .unwrap();
    let before = link_rows(&pool).await;
    assert_eq!(
        before,
        vec![(holder_id.clone(), "labels".into(), 1, alpha_id)]
    );
    let first = memory.migrate_field_links_marked().await.unwrap();
    assert_eq!(first.left_as_text.len(), 2, "{first:?}");
    assert_eq!(first.lowered, 0, "{first:?}");
    for stage in 0..4 {
        match stage {
            1 => {
                memory
                    .add_entity(NewEntity::new(was.clone(), "Was", "fixture"))
                    .await
                    .unwrap()
                    .written()
                    .unwrap();
            }
            2 => {
                memory
                    .rename_entity(&was, &now, None, date(2026, 8, 3), Default::default(), None)
                    .await
                    .unwrap()
                    .written()
                    .unwrap();
            }
            3 => {
                memory
                    .add_entity(NewEntity::new(was.clone(), "Newcomer", "fixture"))
                    .await
                    .unwrap()
                    .written()
                    .unwrap();
            }
            _ => {}
        }
        if stage >= 2 {
            pool.close().await;
            store.stop().await;
            store = Dolt::start(&scratch.0, free_port()).await.unwrap();
            pool = store.database(DATABASE).await.unwrap();
            memory = DoltMemory::open(pool.clone());
            jojobot_domain::memory::kinds::reload(&memory)
                .await
                .unwrap();
        }
        let report = memory.migrate_field_links_marked().await.unwrap();
        assert_eq!(report.lowered, 0, "stage {stage}: {report:?}");
        assert_eq!(stored(&pool, &holder_id, "labels").await, raw);
        assert_eq!(link_rows(&pool).await, before);
        assert_eq!(
            memory.fields(&holder).await.unwrap()["labels"],
            format!("kitchen, {was}, {ambiguous}, {alpha}")
        );
        let facts = memory.recall(&holder).await.unwrap();
        assert_eq!(
            facts
                .iter()
                .flat_map(|fact| fact.field_handles())
                .collect::<Vec<_>>(),
            vec![alpha.clone()]
        );
        assert_eq!(memory.referring_to(&alpha).await.unwrap().len(), 1);
        for text in [&was, &now, &ambiguous, &moved] {
            assert!(memory.referring_to(text).await.unwrap().is_empty());
        }
        if stage == 3 {
            assert!(report.settled, "{report:?}");
        }
    }
    pool.close().await;
    store.stop().await;
}

/// **R3: a bare id written under a declared reference key is marked at the write.**
/// The id of an existing thing is a link when the key is declared a reference, and
/// the write stores the mark and draws the row.
#[tokio::test]
async fn a_bare_id_under_a_declared_key_is_marked_at_the_write() {
    let (_scratch, mut store, pool, memory, [holder, alpha, _]) = world("links-by-mark-r3").await;
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
    let alpha_badge = badge_of(&pool, alpha.as_str()).await;
    memory
        .capture(memory::state(NewFact {
            fields: [("friends".to_string(), alpha_badge.clone())]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "names a thing by its id", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    assert_eq!(
        stored(&pool, &holder_badge, "friends").await,
        format!("{MARK}{alpha_badge}"),
        "the id wears the mark"
    );
    assert_eq!(
        link_rows(&pool).await,
        vec![(holder_badge, "friends".into(), 1, alpha_badge)],
        "and draws its row"
    );
    store.stop().await;
}

/// **R4: a link set missing one target is repaired, not called settled.** A write
/// holds two marked targets and the table lost the row of one of them.
#[tokio::test]
async fn a_link_set_missing_one_target_is_rebuilt_at_boot() {
    let (_scratch, mut store, pool, memory, [holder, alpha, beta]) =
        world("links-by-mark-r4").await;
    memory
        .capture(memory::state(NewFact {
            fields: [("labels".to_string(), format!("kitchen, {alpha}, {beta}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "worded loosely", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    memory
        .migrate_field_links_marked()
        .await
        .expect("the pass runs");
    let beta_badge = badge_of(&pool, beta.as_str()).await;
    let before = link_rows(&pool).await;
    assert_eq!(before.len(), 2, "two targets, two rows: {before:?}");
    sqlx::query("DELETE FROM field_link WHERE target = ?")
        .bind(&beta_badge)
        .execute(&pool)
        .await
        .expect("one row is lost");

    let report = memory
        .migrate_field_links_marked()
        .await
        .expect("the check runs");
    assert!(report.disagreed && report.linked == 1, "{report:?}");
    assert_eq!(link_rows(&pool).await, before, "the row is back");
    store.stop().await;
}

#[tokio::test]
async fn link_rule_leftovers_an_extra_wrong_target_is_removed_while_valid_rows_survive() {
    let (_scratch, mut store, pool, memory, [holder, alpha, beta]) =
        world("links_by_mark_extra_target").await;
    memory
        .capture(memory::state(NewFact {
            fields: [("labels".into(), format!("kitchen, {alpha}, {beta}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "names two real targets", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory.migrate_field_links_marked().await.unwrap();
    let before = link_rows(&pool).await;
    assert_eq!(before.len(), 2);
    let holder_id = badge_of(&pool, holder.as_str()).await;
    let raw = stored(&pool, &holder_id, "labels").await;
    sqlx::query(
        "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, 'labels', 1, ?)",
    )
    .bind(&holder_id)
    .bind(&holder_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(link_rows(&pool).await.len(), 3);
    let report = memory.migrate_field_links_marked().await.unwrap();
    assert!(
        report.disagreed && report.unlinked == 1 && report.linked == 0,
        "{report:?}"
    );
    assert_eq!(link_rows(&pool).await, before);
    assert_eq!(stored(&pool, &holder_id, "labels").await, raw);
    assert!(memory.referring_to(&holder).await.unwrap().is_empty());
    assert_eq!(memory.referring_to(&alpha).await.unwrap().len(), 1);
    assert_eq!(memory.referring_to(&beta).await.unwrap().len(), 1);
    assert!(memory.migrate_field_links_marked().await.unwrap().settled);
    store.stop().await;
}

#[tokio::test]
async fn link_rule_leftovers_an_unreadable_marker_does_not_remark_old_text() {
    let (_scratch, mut store, pool, memory, [holder, alpha, _]) =
        world("links_by_mark_marker_read").await;
    let later = EntityId("thing:contract-moved-third".into());
    memory
        .capture(memory::state(NewFact {
            fields: [
                ("labels".into(), alpha.to_string()),
                ("owner_of".into(), alpha.to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(holder.clone(), "leaves a name unresolved", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let holder_id = badge_of(&pool, holder.as_str()).await;
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'owner_of'")
        .bind(later.as_str())
        .bind(&holder_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM field_link WHERE `key` = 'owner_of'")
        .execute(&pool)
        .await
        .unwrap();
    let first = memory.migrate_field_links_marked().await.unwrap();
    assert_eq!(first.left_as_text.len(), 1, "{first:?}");
    memory
        .add_entity(NewEntity::new(later.clone(), "Later", "fixture"))
        .await
        .unwrap()
        .written()
        .unwrap();
    let before = stored(&pool, &holder_id, "owner_of").await;
    assert_eq!(before, later.to_string());
    let positive = stored(&pool, &holder_id, "labels").await;
    let rows = link_rows(&pool).await;
    assert_eq!(rows.len(), 1);
    sqlx::query("RENAME TABLE store_pass TO held_store_pass")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        sqlx::query_scalar::<_, String>("SELECT name FROM store_pass")
            .fetch_all(&pool)
            .await
            .is_err(),
        "the marker read itself is unavailable"
    );
    let result = memory.migrate_field_links_marked().await;
    sqlx::query("RENAME TABLE held_store_pass TO store_pass")
        .execute(&pool)
        .await
        .unwrap();
    let after = stored(&pool, &holder_id, "owner_of").await;
    assert!(
        result.is_err(),
        "{result:?}; value after the failed read: {after}"
    );
    assert_eq!(after, before);
    assert_eq!(stored(&pool, &holder_id, "labels").await, positive);
    assert_eq!(link_rows(&pool).await, rows);
    let restored = memory.migrate_field_links_marked().await.unwrap();
    assert!(restored.settled && restored.lowered == 0, "{restored:?}");
    assert_eq!(stored(&pool, &holder_id, "owner_of").await, before);
    assert_eq!(link_rows(&pool).await, rows);
    assert!(memory.referring_to(&later).await.unwrap().is_empty());
    assert_eq!(memory.referring_to(&alpha).await.unwrap().len(), 1);
    pool.close().await;
    store.stop().await;
}

/// **R2: text that wears no mark is served as written, never resolved into a
/// link at read time.** A value of plain handle text that the marking left alone
/// names whoever owns the spelling today if a read resolves it; the read must not.
#[tokio::test]
async fn unmarked_handle_text_is_served_as_written_and_follows_no_rename() {
    let (_scratch, mut store, pool, memory, [holder, alpha, _]) = world("links-by-mark-r2").await;
    memory
        .capture(memory::state(NewFact {
            fields: [("owner_of".to_string(), alpha.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "names one thing", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'owner_of'")
        .bind(alpha.as_str())
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .expect("the older pass's text is written");
    let renamed = EntityId("work:contract-links-now".into());
    memory
        .rename_entity(
            &alpha,
            &renamed,
            None,
            date(2026, 8, 2),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the new handle");
    let served = memory.fields(&holder).await.expect("the fields read");
    assert_eq!(
        served.get("owner_of").map(String::as_str),
        Some(alpha.as_str()),
        "text that was never marked is not followed through a rename"
    );
    store.stop().await;
}

/// **The shared link cases, on the real store**: the mixed list, the rename, the
/// edit, the merge and the bare id each decide their links as the fake does.
#[tokio::test]
async fn the_link_contract_runs_on_the_real_store() {
    let (_scratch, mut store, _pool, store_memory, _) = world("links-by-mark-contract").await;
    memory::run_all_link_at_write(&store_memory).await;
    store.stop().await;
}

#[tokio::test]
async fn link_rule_leftovers_mixed_history_keeps_raw_rows_across_restarts() {
    const DATABASE: &str = "links_by_mark_history";
    let (scratch, mut store, mut pool, mut memory, _) = world(DATABASE).await;
    memory::mixed_rename_reuse_and_merge_preserve_the_history(&memory).await;
    let holder = EntityId("person:contract-listed-bystander".into());
    let survivor = EntityId("work:theta".into());
    let holder_id = badge_of(&pool, holder.as_str()).await;
    let original_id = badge_of(&pool, "work:lambda").await;
    let survivor_id = badge_of(&pool, survivor.as_str()).await;
    let claim = memory
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find(|fact| fact.fields.contains_key("contract_link_history"))
        .unwrap()
        .address();
    for boot in 0..3 {
        if boot > 0 {
            pool.close().await;
            store.stop().await;
            store = Dolt::start(&scratch.0, free_port()).await.unwrap();
            pool = store.database(DATABASE).await.unwrap();
            memory = DoltMemory::open(pool.clone());
            jojobot_domain::memory::kinds::reload(&memory)
                .await
                .unwrap();
        }
        memory.migrate_field_links_marked().await.unwrap();
        let writes: Vec<(i64, String)> = sqlx::query_as(
            "SELECT ordinal, value FROM field_write WHERE entity = ?
             AND `key` = 'contract_link_history' ORDER BY ordinal",
        )
        .bind(&holder_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            writes,
            vec![
                (
                    1,
                    format!("kitchen, {MARK}{original_id}, kitchen, {MARK}{survivor_id}")
                ),
                (
                    2,
                    format!("pantry, {MARK}{original_id}, pantry, {MARK}{survivor_id}")
                ),
            ],
            "boot {boot} keeps both versions and their repeated words"
        );
        let rows: Vec<_> = link_rows(&pool)
            .await
            .into_iter()
            .filter(|(_, key, _, _)| key == "contract_link_history")
            .collect();
        let mut expected_rows: Vec<(String, String, i64, String)> = Vec::new();
        for ordinal in [1, 2] {
            for target in [&original_id, &survivor_id] {
                expected_rows.push((
                    holder_id.clone(),
                    "contract_link_history".into(),
                    ordinal,
                    target.clone(),
                ));
            }
        }
        expected_rows.sort();
        assert_eq!(rows, expected_rows);
        assert_eq!(
            memory.fields(&holder).await.unwrap()["contract_link_history"],
            format!("pantry, {survivor}, pantry")
        );
        let referring: Vec<_> = memory
            .referring_to(&survivor)
            .await
            .unwrap()
            .into_iter()
            .filter(|fact| fact.fields.contains_key("contract_link_history"))
            .map(|fact| fact.address())
            .collect();
        assert_eq!(referring, vec![claim.clone()]);
        assert!(
            memory
                .referring_to(&EntityId("work:omicron".into()))
                .await
                .unwrap()
                .is_empty()
        );
    }
    pool.close().await;
    store.stop().await;
}

#[tokio::test]
async fn link_rule_leftovers_declarations_keep_rows_across_two_store_restarts() {
    const DATABASE: &str = "links_by_mark_declarations";
    let (scratch, mut store, mut pool, mut memory, [holder, alpha, beta]) = world(DATABASE).await;
    let other = EntityId("person:contract-listed-bystander".into());
    memory
        .add_entity(NewEntity::new(
            other.clone(),
            "Bystander",
            "contract-fixture",
        ))
        .await
        .unwrap()
        .written()
        .unwrap();
    let value = format!("kitchen, {beta}, {alpha}, kitchen");
    let mut claims = Vec::new();
    for owner in [&holder, &other] {
        let fact = memory
            .capture(memory::state(NewFact {
                fields: [("labels".into(), value.clone())].into_iter().collect(),
                ..NewFact::about(owner.clone(), "keeps the links", date(2026, 8, 1))
            }))
            .await
            .unwrap()
            .written()
            .unwrap();
        claims.push(fact.address().to_string());
    }
    memory.migrate_field_links_marked().await.unwrap();
    let holder_ids = [
        badge_of(&pool, holder.as_str()).await,
        badge_of(&pool, other.as_str()).await,
    ];
    let alpha_id = badge_of(&pool, alpha.as_str()).await;
    let beta_id = badge_of(&pool, beta.as_str()).await;
    let marked_value = format!("kitchen, {MARK}{beta_id}, {MARK}{alpha_id}, kitchen");
    let rows = link_rows(&pool).await;
    assert_eq!(rows.len(), 4);
    for id in &holder_ids {
        for target in [&alpha_id, &beta_id] {
            assert!(rows.contains(&(id.clone(), "labels".into(), 1, target.clone())));
        }
    }
    let mut original_order = None;
    for stage in 0..6 {
        let fields = match stage {
            1 => Some(vec![Field::listing("labels", ValueType::Reference)]),
            2 => Some(vec![Field::new("labels", ValueType::Text)]),
            3 => Some(vec![Field::new("other_label", ValueType::Text)]),
            _ => None,
        };
        if let Some(fields) = fields {
            memory
                .declare_type(DeclaredType::new("contract-link-changes", fields), None)
                .await
                .unwrap();
        }
        if stage >= 4 {
            pool.close().await;
            store.stop().await;
            store = Dolt::start(&scratch.0, free_port()).await.unwrap();
            pool = store.database(DATABASE).await.unwrap();
            memory = DoltMemory::open(pool.clone());
            jojobot_domain::memory::kinds::reload(&memory)
                .await
                .unwrap();
            let report = memory.migrate_field_links_marked().await.unwrap();
            assert!(report.settled && report.lowered == 0, "{report:?}");
        }
        for id in &holder_ids {
            assert_eq!(stored(&pool, id, "labels").await, marked_value);
        }
        assert_eq!(
            link_rows(&pool).await,
            rows,
            "stage {stage} moves no raw row"
        );
        let folded = jojobot_adapters::fold::Folded::new(std::sync::Arc::new(memory.clone()));
        folded.rebuild().await.unwrap();
        let mut order = Vec::new();
        for port in [&memory as &dyn Memory, &folded as &dyn Memory] {
            for owner in [&holder, &other] {
                assert_eq!(port.fields(owner).await.unwrap()["labels"], value);
                let facts = port.recall(owner).await.unwrap();
                let fact = facts
                    .iter()
                    .find(|fact| claims.contains(&fact.address().to_string()))
                    .unwrap();
                assert_eq!(fact.field_handles(), vec![beta.clone(), alpha.clone()]);
            }
            for target in [&alpha, &beta] {
                let answers: Vec<_> = port
                    .referring_to(target)
                    .await
                    .unwrap()
                    .iter()
                    .map(|fact| fact.address().to_string())
                    .collect();
                assert_eq!(answers.len(), 2);
                for claim in &claims {
                    assert!(answers.contains(claim));
                }
                order.push(answers);
            }
        }
        if let Some(before) = &original_order {
            assert_eq!(&order, before, "the unsorted answers survive stage {stage}");
        } else {
            original_order = Some(order);
        }
    }
    pool.close().await;
    store.stop().await;
}

/// **A pass that died between its commit and its marker finishes on the next
/// boot and changes nothing.** The conversion committed, the marker did not land.
#[tokio::test]
async fn a_marking_pass_killed_before_its_marker_runs_again_and_changes_nothing() {
    const DATABASE: &str = "links-by-mark-death";
    let (scratch, mut store, mut pool, mut store_memory, [holder, alpha, beta]) =
        world(DATABASE).await;
    store_memory
        .capture(memory::state(NewFact {
            fields: [("labels".to_string(), format!("kitchen, {alpha}, {beta}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "worded loosely", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    let alpha_badge = badge_of(&pool, alpha.as_str()).await;
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'labels'")
        .bind(format!("kitchen, {alpha}"))
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .expect("the old shape");
    sqlx::query("DELETE FROM field_link")
        .execute(&pool)
        .await
        .expect("the table starts empty");
    let first = store_memory
        .migrate_field_links_marked()
        .await
        .expect("the pass runs");
    assert_eq!(first.lowered, 1, "{first:?}");
    let converted = stored(&pool, &holder_badge, "labels").await;
    let rows = link_rows(&pool).await;
    assert_eq!(
        rows,
        vec![(
            holder_badge.clone(),
            "labels".into(),
            1,
            alpha_badge.clone()
        )]
    );
    // The process died after the commit and before the marker.
    sqlx::query("DELETE FROM store_pass WHERE name = 'field value links marked'")
        .execute(&pool)
        .await
        .expect("the marker is lost");
    pool.close().await;
    store.stop().await;
    store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    pool = store.database(DATABASE).await.unwrap();
    store_memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::reload(&store_memory)
        .await
        .unwrap();
    let again = store_memory
        .migrate_field_links_marked()
        .await
        .expect("the pass runs again");
    assert_eq!(again.lowered, 0, "nothing is converted twice: {again:?}");
    assert_eq!(stored(&pool, &holder_badge, "labels").await, converted);
    assert_eq!(link_rows(&pool).await, rows);
    assert_eq!(
        converted,
        format!("kitchen, {MARK}{alpha_badge}"),
        "and the conversion is what the first run made"
    );
    let markers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM store_pass WHERE name = 'field value links marked'",
    )
    .fetch_one(&pool)
    .await
    .expect("markers");
    assert_eq!(markers, 1, "the marker is back");
    pool.close().await;
    store.stop().await;
    store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    pool = store.database(DATABASE).await.unwrap();
    store_memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::reload(&store_memory)
        .await
        .unwrap();
    let clean = store_memory.migrate_field_links_marked().await.unwrap();
    assert!(clean.settled && clean.lowered == 0, "{clean:?}");
    assert_eq!(stored(&pool, &holder_badge, "labels").await, converted);
    assert_eq!(link_rows(&pool).await, rows);
    assert_eq!(store_memory.referring_to(&alpha).await.unwrap().len(), 1);
    assert!(store_memory.referring_to(&beta).await.unwrap().is_empty());
    pool.close().await;
    store.stop().await;
}

/// **F1: a reader of the record and the fold link by the marks, not by the words.**
/// A value of plain handle text that wears no mark names a thing that owns the
/// spelling today. The record's own links, the fold's lists of who points here and
/// the store all leave it unlinked, while a mark in a mixed list is linked.
#[tokio::test]
async fn the_record_and_the_fold_link_by_the_marks_and_not_by_the_words() {
    let (_scratch, mut store, pool, store_memory, [holder, alpha, beta]) =
        world("links-by-mark-readers").await;
    store_memory
        .capture(memory::state(NewFact {
            fields: [
                ("owner_of".to_string(), alpha.to_string()),
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
    let own = store_memory
        .capture(NewFact {
            fields: [("legacy".into(), format!("pantry, {beta}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "separate marked claim", date(2026, 8, 1))
        })
        .await
        .unwrap()
        .written()
        .unwrap();
    assert!(!own.sets.contains("legacy"));
    assert!(!own.reaching.contains("legacy"));
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    let beta_badge = badge_of(&pool, beta.as_str()).await;
    let legacy_raw = format!("pantry, {alpha}, {MARK}{beta_badge}");
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'legacy'")
        .bind(&legacy_raw)
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'owner_of'")
        .bind(alpha.as_str())
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .expect("the older pass's text is written");
    sqlx::query("DELETE FROM field_link WHERE entity = ? AND `key` = 'owner_of'")
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .expect("and it drew no row");

    let folded = std::sync::Arc::new(
        jojobot_adapters::fold::Folded::new(std::sync::Arc::new(store_memory.clone()))
            .with_boot_reader(std::sync::Arc::new(store_memory.boot_reader())),
    );
    folded.rebuild().await.expect("the fold loads");
    for (what, claims) in [
        (
            "the store",
            store_memory.recall(&holder).await.expect("recall"),
        ),
        ("the fold", folded.recall(&holder).await.expect("recall")),
    ] {
        let linked: Vec<String> = claims
            .iter()
            .flat_map(|fact| fact.field_handles())
            .map(|id| id.to_string())
            .collect();
        assert_eq!(
            linked,
            vec![beta.to_string(), beta.to_string()],
            "{what}: only the marked item links, never the plain words"
        );
    }
    for (what, pointing) in [
        (
            "the store",
            store_memory.referring_to(&alpha).await.expect("answers"),
        ),
        (
            "the fold",
            folded.referring_to(&alpha).await.expect("answers"),
        ),
    ] {
        assert!(
            pointing.is_empty(),
            "{what}: nothing points at alpha: {pointing:?}"
        );
    }
    readers::assert_links(
        std::sync::Arc::new(store_memory.clone()),
        &holder,
        &alpha,
        &beta,
    )
    .await;
    readers::assert_links(folded.clone(), &holder, &alpha, &beta).await;
    for fields in [
        vec![
            Field::listing("labels", ValueType::Reference),
            Field::listing("legacy", ValueType::Reference),
            Field::listing("owner_of", ValueType::Reference),
        ],
        vec![
            Field::new("labels", ValueType::Reference),
            Field::new("legacy", ValueType::Reference),
            Field::new("owner_of", ValueType::Reference),
        ],
        vec![
            Field::new("labels", ValueType::Text),
            Field::new("legacy", ValueType::Text),
            Field::new("owner_of", ValueType::Text),
        ],
    ] {
        store_memory
            .declare_type(DeclaredType::new("contract-legacy-links", fields), None)
            .await
            .unwrap();
        folded.rebuild().await.unwrap();
        for layer in [
            std::sync::Arc::new(store_memory.clone()) as std::sync::Arc<dyn Memory>,
            folded.clone(),
        ] {
            readers::assert_links(layer, &holder, &alpha, &beta).await;
        }
        folded.rebuild().await.unwrap();
        readers::assert_links(folded.clone(), &holder, &alpha, &beta).await;
        assert_eq!(stored(&pool, &holder_badge, "legacy").await, legacy_raw);
    }
    store.stop().await;
}

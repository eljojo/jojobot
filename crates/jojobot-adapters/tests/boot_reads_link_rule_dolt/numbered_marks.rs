use super::*;
use jojobot_domain::memory::Override;

// A numbered address is derived from live collection state, not an explicit
// rename event. The marking pass must resolve it by the same directory.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_marking_resolves_a_numbered_address_in_a_mixed_value() {
    marking_resolves("direct").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_marking_resolves_a_numbered_chain_in_a_mixed_value() {
    marking_resolves("chain").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_marking_resolves_an_old_collection_prefix_in_a_mixed_value() {
    marking_resolves("former").await;
}

/// **An ambiguous prefix is not marked**: the old handle of a renamed collection
/// that another thing now wears names two things, so the numbered spelling
/// through it stays text. Paired with the three above, which mark.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_marking_leaves_an_ambiguous_collection_prefix_as_text() {
    marking_resolves("ambiguous").await;
}

/// **A prefix that two things have worn blocks marking even when the full
/// spelling names one member**: both holders of the reused handle number a member
/// 1, so the complete numbered alias is unique while its prefix is not. Paired with
/// the positives above, which differ by the second holder alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_marking_leaves_a_numbered_alias_through_an_ambiguous_prefix_as_text() {
    marking_resolves("ambiguous_numbered").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_marking_leaves_a_numbered_chain_through_an_ambiguous_prefix_as_text() {
    marking_resolves("ambiguous_chain").await;
}

async fn marking_resolves(spelling: &str) {
    let ambiguous = spelling.starts_with("ambiguous");
    let scratch = Scratch::new("numbered-link-marks");
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "numbered_link_marks")
        .await
        .unwrap();
    migrate::run(&pool).await.unwrap();
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    let outer = EntityId("topic:alpha".into());
    let collection = EntityId("topic:beta".into());
    let member = EntityId("work:alpha-queue".into());
    let holder = EntityId("thing:contract-links-holder".into());
    for (id, name, parent) in [
        (&outer, "Outer", None),
        (&collection, "Collection", Some(outer.clone())),
        (&member, "Member", Some(collection.clone())),
        (&holder, "Holder", None),
    ] {
        memory
            .add_entity(NewEntity {
                parent,
                ..NewEntity::new(id.clone(), name, "contract-fixture")
            })
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    for (id, key, value) in [
        (&outer, "numbers_by", "group_number"),
        (&collection, "group_number", "2"),
        (&collection, "numbers_by", "number"),
        (&member, "number", "1"),
        (&holder, "mixed", member.as_str()),
        (&holder, "control", member.as_str()),
    ] {
        memory
            .capture(memory::state(NewFact {
                fields: [(key.into(), value.into())].into_iter().collect(),
                ..NewFact::about(id.clone(), "numbered fixture", date(2026, 8, 1))
            }))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    if spelling == "former" || ambiguous {
        memory
            .rename_entity(
                &collection,
                &EntityId("topic:gamma".into()),
                None,
                date(2026, 8, 2),
                Override::none(),
                None,
            )
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    if spelling == "ambiguous" || spelling == "ambiguous_numbered" {
        memory
            .add_entity(NewEntity::new(
                collection.clone(),
                "Newcomer",
                "contract-fixture",
            ))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    if spelling == "ambiguous_numbered" {
        let successor = EntityId("work:beta-queue".into());
        memory
            .add_entity(NewEntity {
                parent: Some(collection.clone()),
                ..NewEntity::new(successor.clone(), "Successor", "contract-fixture")
            })
            .await
            .unwrap()
            .written()
            .unwrap();
        for (id, key, value) in [
            (&collection, "numbers_by", "number"),
            (&successor, "number", "1"),
        ] {
            memory
                .capture(memory::state(NewFact {
                    fields: [(key.into(), value.into())].into_iter().collect(),
                    ..NewFact::about(id.clone(), "numbered fixture", date(2026, 8, 3))
                }))
                .await
                .unwrap()
                .written()
                .unwrap();
        }
    }
    if spelling == "ambiguous_chain" {
        // The outer collection is renamed too, and its handle is reused by a
        // second outer whose own collection numbers a member 1 under group 2.
        memory
            .rename_entity(
                &outer,
                &EntityId("topic:delta".into()),
                None,
                date(2026, 8, 3),
                Override::none(),
                None,
            )
            .await
            .unwrap()
            .written()
            .unwrap();
        let second = EntityId("topic:epsilon".into());
        let successor = EntityId("work:beta-queue".into());
        for (id, name, parent) in [
            (outer.clone(), "Second outer", None),
            (second.clone(), "Second collection", Some(outer.clone())),
            (successor.clone(), "Second member", Some(second.clone())),
        ] {
            let outcome = memory
                .add_entity(NewEntity {
                    parent,
                    ..NewEntity::new(id.clone(), name, "contract-fixture")
                })
                .await
                .unwrap();
            let shown = format!("{outcome:?}");
            assert!(outcome.written().is_some(), "add {id}: {shown}");
        }
        for (id, key, value) in [
            (&outer, "numbers_by", "group_number"),
            (&second, "group_number", "2"),
            (&second, "numbers_by", "number"),
            (&successor, "number", "1"),
        ] {
            memory
                .capture(memory::state(NewFact {
                    fields: [(key.into(), value.into())].into_iter().collect(),
                    ..NewFact::about(id.clone(), "numbered fixture", date(2026, 8, 3))
                }))
                .await
                .unwrap()
                .written()
                .unwrap();
        }
    }
    let alias = if spelling == "chain" || spelling == "ambiguous_chain" {
        format!("{outer}/2/1")
    } else {
        format!("{collection}/1")
    };
    // The ordinary read already supports this spelling; marking must share it.
    if !ambiguous {
        assert_eq!(
            memory.fields(&EntityId(alias.clone())).await.unwrap()["number"],
            "1"
        );
    }
    let holder_badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(holder.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    let member_badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(member.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    for (key, value) in [("mixed", alias.as_str()), ("control", member.as_str())] {
        sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ?")
            .bind(format!("kitchen, {value}"))
            .bind(&holder_badge)
            .bind(key)
            .execute(&pool)
            .await
            .unwrap();
    }
    memory.migrate_field_links_marked().await.unwrap();
    let marked = format!(
        "kitchen, {}",
        jojobot_domain::memory::marked_stored_handle(&member_badge)
    );
    let stored = |key: &'static str| {
        let pool = pool.clone();
        let holder_badge = holder_badge.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM field_write WHERE entity = ? AND `key` = ?",
            )
            .bind(&holder_badge)
            .bind(key)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        stored("control").await,
        marked,
        "a canonical handle is marked as before"
    );
    if ambiguous {
        assert_eq!(
            stored("mixed").await,
            format!("kitchen, {alias}"),
            "an ambiguous spelling stays as written"
        );
    } else {
        assert_eq!(
            stored("mixed").await,
            marked,
            "the numbered spelling did not keep the member's permanent id"
        );
        let served = memory.fields(&holder).await.unwrap();
        assert_eq!(served["mixed"], format!("kitchen, {member}"));
    }
    store.stop().await;
}

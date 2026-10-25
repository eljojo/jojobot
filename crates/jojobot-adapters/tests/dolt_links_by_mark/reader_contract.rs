use std::sync::Arc;

use super::*;
use jojobot_adapters::fold::Folded;

#[tokio::test]
async fn unmarked_person_alpha_links_nowhere_in_the_fake_after_rename_and_reuse() {
    use jojobot_domain::memory::testing::InMemoryMemory;

    jojobot_domain::memory::set_link_rule(jojobot_domain::memory::LinkRule::On);
    let memory = Arc::new(InMemoryMemory::booted());
    let [holder, plain, marked] = [
        "thing:contract-links-holder",
        "person:alpha",
        "work:contract-links-beta",
    ]
    .map(|id| EntityId(id.into()));
    for (id, name) in [(&holder, "Holder"), (&plain, "Earlier"), (&marked, "Beta")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    memory
        .capture(memory::state(NewFact {
            fields: [("labels".into(), format!("kitchen, {marked}"))].into(),
            ..NewFact::about(holder.clone(), "names two things", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let badge = memory
        .list_entities(None)
        .await
        .unwrap()
        .into_iter()
        .find(|entity| entity.id == marked)
        .unwrap()
        .badge
        .unwrap();
    memory.fields_past_the_guard(
        &holder,
        &[
            ("owner_of", plain.as_str()),
            ("legacy", &format!("pantry, {plain}, {MARK}{badge}")),
        ],
    );
    memory
        .declare_type(
            DeclaredType::new(
                "contract-legacy-links",
                ["labels", "legacy", "owner_of"]
                    .map(|key| Field::listing(key, ValueType::Reference))
                    .to_vec(),
            ),
            None,
        )
        .await
        .unwrap();
    readers::assert_links(memory.clone(), &holder, &plain, &marked).await;
    memory
        .rename_entity(
            &plain,
            &EntityId("person:gamma".into()),
            None,
            date(2026, 8, 2),
            Default::default(),
            None,
        )
        .await
        .unwrap()
        .written()
        .expect("the rename fixture has no name collision");
    let before_reuse = memory
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find_map(|fact| fact.fields.get("legacy").cloned());
    assert_eq!(before_reuse, Some(format!("pantry, {plain}, {marked}")));
    memory
        .add_entity(NewEntity::new(
            plain.clone(),
            "Replacement",
            "contract-fixture",
        ))
        .await
        .unwrap()
        .written()
        .unwrap();
    let legacy = memory
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find_map(|fact| fact.fields.get("legacy").cloned());
    assert_eq!(legacy, Some(format!("pantry, {plain}, {marked}")));
    readers::assert_links(memory.clone(), &holder, &plain, &marked).await;
    let folded = Arc::new(Folded::new(memory));
    folded.rebuild().await.unwrap();
    readers::assert_links(folded, &holder, &plain, &marked).await;
}

/// An older negative decision survives declarations, every reader and a real
/// restart. The marked item beside it is the positive control.
#[tokio::test]
async fn unmarked_person_alpha_links_nowhere_before_and_after_a_store_restart() {
    const DATABASE: &str = "links_by_mark_reader_contract";
    jojobot_domain::memory::set_link_rule(jojobot_domain::memory::LinkRule::On);
    let scratch = Scratch::new(DATABASE);
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let mut pool = migrated_database(&store, DATABASE).await.unwrap();
    migrate::run(&pool).await.unwrap();
    let mut memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    let [holder, plain, marked] = [
        "thing:contract-links-holder",
        "person:alpha",
        "work:contract-links-beta",
    ]
    .map(|id| EntityId(id.into()));
    for (id, name) in [&holder, &plain, &marked]
        .into_iter()
        .zip(["Holder", "Alpha", "Beta"])
    {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    memory
        .capture(memory::state(NewFact {
            fields: [
                ("owner_of".into(), plain.to_string()),
                ("labels".into(), format!("kitchen, {marked}")),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(holder.clone(), "names two things", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory
        .capture(NewFact {
            fields: [("legacy".into(), format!("pantry, {marked}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "separate marked claim", date(2026, 8, 1))
        })
        .await
        .unwrap()
        .written()
        .unwrap();
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    let plain_badge = badge_of(&pool, plain.as_str()).await;
    let marked_badge = badge_of(&pool, marked.as_str()).await;
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'owner_of'")
        .bind(plain.as_str())
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM field_link WHERE entity = ? AND `key` = 'owner_of'")
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .unwrap();
    let legacy = format!("pantry, {plain}, {MARK}{marked_badge}");
    sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'legacy'")
        .bind(&legacy)
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .unwrap();
    memory
        .declare_type(
            DeclaredType::new(
                "contract-legacy-links",
                ["labels", "legacy", "owner_of"]
                    .map(|key| Field::listing(key, ValueType::Reference))
                    .to_vec(),
            ),
            None,
        )
        .await
        .unwrap();
    for restarted in [false, true] {
        if restarted {
            pool.close().await;
            store.stop().await;
            store = Dolt::start(&scratch.0, free_port()).await.unwrap();
            pool = store.database(DATABASE).await.unwrap();
            memory = DoltMemory::open(pool.clone());
            jojobot_domain::memory::kinds::reload(&memory)
                .await
                .unwrap();
        }
        let rows = link_rows(&pool).await;
        assert_eq!(rows.iter().filter(|row| row.3 == plain_badge).count(), 0);
        assert_eq!(rows.iter().filter(|row| row.3 == marked_badge).count(), 2);
        assert_eq!(
            stored(&pool, &holder_badge, "owner_of").await,
            "person:alpha"
        );
        assert_eq!(stored(&pool, &holder_badge, "legacy").await, legacy);
        readers::assert_links(Arc::new(memory.clone()), &holder, &plain, &marked).await;
        let folded = Arc::new(
            Folded::new(Arc::new(memory.clone())).with_boot_reader(Arc::new(memory.boot_reader())),
        );
        folded.rebuild().await.unwrap();
        readers::assert_links(folded, &holder, &plain, &marked).await;
    }
    pool.close().await;
    store.stop().await;
}

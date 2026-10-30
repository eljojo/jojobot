use super::*;
use jojobot_domain::memory::Override;
use jojobot_domain::memory::mention::Mentioning;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use std::sync::Arc;

// R2 crosses both production layers: the store composes marks, and Mentioning
// must not resolve the unmarked historical text by a later declaration.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_r2_dolt_mentioning_keeps_unmarked_text_after_a_rename() {
    let scratch = Scratch::new("render-link-rule");
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "render_link_rule").await.unwrap();
    migrate::run(&pool).await.unwrap();
    let bare = Arc::new(DoltMemory::open(pool.clone()));
    jojobot_domain::memory::kinds::seed(bare.as_ref())
        .await
        .unwrap();
    let memory = Mentioning::new(bare.clone());
    let [holder, alpha, renamed] = ["thing:contract-links-holder", "person:alpha", "person:beta"]
        .map(|handle| EntityId(handle.into()));
    for (id, name) in [(&holder, "Holder"), (&alpha, "Alpha")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    for fields in [
        [("marked".into(), alpha.to_string())].into_iter().collect(),
        [
            ("legacy".into(), "kitchen".into()),
            ("mixed".into(), "kitchen".into()),
        ]
        .into_iter()
        .collect(),
    ] {
        memory
            .capture(memory::state(NewFact {
                fields,
                ..NewFact::about(holder.clone(), "R2", date(2026, 8, 1))
            }))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    let holder_badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(holder.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    let alpha_badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(alpha.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    // An On write cannot create these historical unmarked handles. Keep a
    // marked item beside them to prove that only the writer's links follow.
    for (key, value) in [
        ("legacy", alpha.to_string()),
        (
            "mixed",
            format!(
                "kitchen, {alpha}, {}",
                jojobot_domain::memory::marked_stored_handle(&alpha_badge)
            ),
        ),
    ] {
        let updated =
            sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ?")
                .bind(value)
                .bind(&holder_badge)
                .bind(key)
                .execute(&pool)
                .await
                .unwrap();
        assert_eq!(updated.rows_affected(), 1, "the historical {key} is staged");
    }
    memory
        .declare_type(
            DeclaredType::new(
                "contract-r2",
                vec![
                    Field::new("legacy", ValueType::Reference),
                    Field::listing("mixed", ValueType::Reference),
                ],
            ),
            None,
        )
        .await
        .unwrap();
    memory
        .rename_entity(
            &alpha,
            &renamed,
            None,
            date(2026, 8, 2),
            Override::none(),
            None,
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    let facts = memory.recall(&holder).await.unwrap();
    let marked = facts
        .iter()
        .find(|fact| fact.fields.contains_key("marked"))
        .unwrap();
    assert_eq!(marked.fields["marked"], renamed.to_string(), "{marked:?}");
    let legacy = facts
        .iter()
        .find(|fact| fact.fields.contains_key("legacy"))
        .unwrap();
    assert_eq!(legacy.fields["legacy"], alpha.to_string(), "{legacy:?}");
    assert_eq!(
        legacy.fields["mixed"],
        format!("kitchen, {alpha}, {renamed}"),
        "{legacy:?}"
    );
    let fields = memory.fields(&holder).await.unwrap();
    assert_eq!(fields["legacy"], alpha.to_string());
    assert_eq!(fields["marked"], renamed.to_string());
    assert_eq!(fields["mixed"], format!("kitchen, {alpha}, {renamed}"));
    store.stop().await;
}

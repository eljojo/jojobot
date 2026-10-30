use super::*;
use jojobot_domain::memory::mention::Mentioning;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use std::sync::Arc;

// R2 must survive the outer renderer: a Reference declaration cannot turn old
// text into a link after a rename. The marked sibling must still follow it.
#[tokio::test]
async fn link_rule_r2_production_mentioning_keeps_unmarked_text_after_a_rename() {
    let bare = Arc::new(InMemoryMemory::booted().with_own_fields_reaching());
    let memory = Mentioning::new(bare.clone());
    let holder = EntityId("thing:contract-links-holder".into());
    let alpha = EntityId("person:alpha".into());
    let renamed = EntityId("person:beta".into());
    for (id, name) in [(&holder, "Holder"), (&alpha, "Alpha")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    memory
        .capture(contract::state(NewFact {
            fields: [("marked".into(), alpha.to_string())].into_iter().collect(),
            ..NewFact::about(holder.clone(), "R2", jiff::civil::date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    // A write marks every link, so this unmarked value can only come from a store
    // written before the marks existed.
    let badge = bare
        .list_entities(None)
        .await
        .unwrap()
        .into_iter()
        .find(|entity| entity.id == alpha)
        .unwrap()
        .badge
        .unwrap();
    bare.fields_past_the_guard(
        &holder,
        &[
            ("legacy", alpha.as_str()),
            (
                "mixed",
                &format!(
                    "kitchen, {alpha}, {}",
                    jojobot_domain::memory::marked_stored_handle(&badge)
                ),
            ),
        ],
    );
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
            jiff::civil::date(2026, 8, 2),
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
}

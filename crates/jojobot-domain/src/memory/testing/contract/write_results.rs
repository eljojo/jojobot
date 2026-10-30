//! Write results carry the same marked links as the persisted claim.

use super::support::{ensure, other_caller};
use super::*;

async fn world<M: Memory>(store: &M) -> [EntityId; 3] {
    let ids = ["thing:contract-links-holder", "person:alpha", "person:beta"]
        .map(|id| EntityId(id.into()));
    for id in &ids {
        ensure(store, id).await;
    }
    ensure(store, &other_caller()).await;
    ids
}

pub async fn capture_has_the_persisted_links<M: Memory>(store: &M) {
    let [holder, alpha, _] = world(store).await;
    let written = store
        .capture(state(NewFact {
            fields: [("mixed".into(), format!("kitchen, {alpha}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "capture result", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let read = store
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find(|fact| fact.id == written.id)
        .unwrap();
    assert_eq!(read.field_links["mixed"], vec![alpha.clone()]);
    assert_eq!(written.field_links, read.field_links, "{written:?}");
    assert_eq!(written.field_handles(), vec![alpha]);
    assert_eq!(written.fields, read.fields);
}

pub async fn edit_replaces_and_clears_the_persisted_links<M: Memory>(store: &M) {
    let [holder, alpha, beta] = world(store).await;
    let first = store
        .capture(state(NewFact {
            fields: [
                ("mixed".into(), format!("kitchen, {alpha}")),
                ("cleared".into(), alpha.to_string()),
                ("unchanged".into(), alpha.to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(holder.clone(), "edit result", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let written = store
        .update_fact(
            &first.address(),
            state_patch(FactPatch {
                fields: [("mixed".into(), format!("kitchen, {beta}"))]
                    .into_iter()
                    .collect(),
                clear_fields: vec!["cleared".into()],
                ..Default::default()
            }),
            &other_caller(),
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    let read = store
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find(|fact| fact.id == first.id)
        .unwrap();
    assert_eq!(read.field_links["mixed"], vec![beta]);
    assert_eq!(read.field_links["unchanged"], vec![alpha]);
    assert!(!read.field_links.contains_key("cleared"));
    assert_eq!(written.field_links, read.field_links, "{written:?}");
    assert_eq!(written.fields, read.fields);
}

/// **A link to a target that has been renamed is answered under the handle the
/// target wears now.** The result of the write and a later read agree, so the
/// receipt of a write never shows the key the store holds for the target.
pub async fn capture_naming_a_renamed_target_has_the_current_handle<M: Memory>(store: &M) {
    let [holder, alpha, _] = world(store).await;
    let now = EntityId("person:gamma".into());
    store
        .rename_entity(
            &alpha,
            &now,
            None,
            date(2026, 8, 2),
            crate::memory::Override::none(),
            None,
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    let written = store
        .capture(state(NewFact {
            fields: [("mixed".into(), format!("kitchen, {now}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "renamed target", date(2026, 8, 3))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let read = store
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find(|fact| fact.id == written.id)
        .unwrap();
    assert_eq!(read.field_links["mixed"], vec![now.clone()], "{read:?}");
    assert_eq!(written.field_links, read.field_links, "{written:?}");
}

/// **One handle listed twice in a field is one link.** The result of the write
/// and a later read carry the same single link.
pub async fn capture_listing_a_handle_twice_has_one_link<M: Memory>(store: &M) {
    let [holder, alpha, beta] = world(store).await;
    let written = store
        .capture(state(NewFact {
            fields: [("mixed".into(), format!("{alpha}, {beta}, {alpha}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "duplicate items", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let read = store
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find(|fact| fact.id == written.id)
        .unwrap();
    assert_eq!(read.field_links["mixed"], vec![alpha, beta], "{read:?}");
    assert_eq!(written.field_links, read.field_links, "{written:?}");
}

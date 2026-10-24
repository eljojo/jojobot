//! **A field value links to a thing when it is written, and nothing decides it
//! again**, proven against any [`Memory`] adapter (rules 355 and 646). The
//! decision is taken from the value alone: each comma item that is a handle is a
//! link under any key, loose items beside it stay text, and a type declared
//! afterwards neither makes a link nor removes one.

use super::support::{capture, ensure, state};
use super::*;
use std::collections::BTreeMap;

const KEY: &str = "contract_link_tags";
const RENAMED_KEY: &str = "contract_link_renamed";

fn value(target: &EntityId) -> String {
    format!("kitchen, {target}")
}

async fn world<M: Memory>(store: &M) -> (EntityId, EntityId) {
    let target = EntityId("event:contract-listed-first".into());
    let holder = EntityId("person:contract-listed-bystander".into());
    ensure(store, &target).await;
    ensure(store, &holder).await;
    (target, holder)
}

async fn written<M: Memory>(
    store: &M,
    holder: &EntityId,
    key: &str,
    said: &str,
    value: String,
) -> String {
    capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([(key.to_string(), value)]),
            ..NewFact::about(holder.clone(), said, date(2026, 8, 2))
        }),
    )
    .await
    .address()
    .to_string()
}

async fn names<M: Memory>(store: &M, target: &EntityId, claim: &str) -> bool {
    store
        .referring_to(target)
        .await
        .expect("a store answers who points here")
        .iter()
        .any(|fact| fact.address().to_string() == claim)
}

/// **A list with a loose item among its handles links the handles.** Under a key
/// no type declares, the claim is found from the handle, and the value reads
/// back as it was written. A type declared for the key afterwards changes
/// neither the answer nor the value.
pub async fn a_handle_in_a_mixed_list_links_at_the_write_and_a_later_type_changes_nothing<
    M: Memory,
>(
    store: &M,
) {
    let (target, holder) = world(store).await;
    let claim = written(store, &holder, KEY, "worded loosely", value(&target)).await;
    assert!(
        names(store, &target, &claim).await,
        "a handle among loose items is a link under a key nobody declared"
    );
    let served = store.fields(&holder).await.expect("the fields read");
    assert_eq!(
        served.get(KEY),
        Some(&value(&target)),
        "the value reads back with its loose item and its handle"
    );
    store
        .declare_type(
            DeclaredType::new(
                "contract-link-tagged",
                vec![Field::listing(KEY, ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("the declaration lands");
    assert!(
        names(store, &target, &claim).await,
        "declaring the key afterwards removes no link"
    );
}

/// **A renamed target is followed by a link in a mixed list**, and the loose item
/// stays where it was: the mark stored the permanent id, not the spelling.
pub async fn a_link_in_a_mixed_list_follows_a_rename_of_its_target<M: Memory>(store: &M) {
    let (_, holder) = world(store).await;
    let target = EntityId("thing:contract-moved-fifth".into());
    let renamed = EntityId("thing:contract-moved-fourth".into());
    ensure(store, &target).await;
    let claim = written(
        store,
        &holder,
        RENAMED_KEY,
        "worded loosely again",
        value(&target),
    )
    .await;
    store
        .rename_entity(
            &target,
            &renamed,
            None,
            date(2026, 8, 3),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the new handle");
    assert!(
        names(store, &renamed, &claim).await,
        "the claim is found from the target's new handle"
    );
    let served = store.fields(&holder).await.expect("the fields read");
    assert_eq!(
        served.get(RENAMED_KEY),
        Some(&value(&renamed)),
        "the value is served under the new handle, with the loose item kept"
    );
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_link_at_write<M: Memory>(store: &M) {
    a_handle_in_a_mixed_list_links_at_the_write_and_a_later_type_changes_nothing(store).await;
    a_link_in_a_mixed_list_follows_a_rename_of_its_target(store).await;
}

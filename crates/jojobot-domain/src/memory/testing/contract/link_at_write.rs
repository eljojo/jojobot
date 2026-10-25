//! **A field value links to a thing when it is written, and nothing decides it
//! again**, proven against any [`Memory`] adapter (rules 355 and 646). The
//! decision is taken from the value alone: each comma item that is a handle is a
//! link under any key, loose items beside it stay text, and a type declared
//! afterwards neither makes a link nor removes one.

use super::support::{capture, ensure, state, state_patch};
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

async fn badge_of<M: Memory>(store: &M, id: &EntityId) -> String {
    store
        .list_entities(None)
        .await
        .expect("the listing reads")
        .into_iter()
        .find(|entity| &entity.id == id)
        .and_then(|entity| entity.badge)
        .expect("a created thing wears a badge")
}

/// **An edit decides links the way a capture does.** A list edited to name two
/// things links both, in the order written; a handle among words that names
/// nothing refuses the whole edit and changes nothing.
pub async fn an_edit_decides_links_at_the_write_as_a_capture_does<M: Memory>(store: &M) {
    let (first, holder) = world(store).await;
    let second = EntityId("thing:contract-moved-second".into());
    let nobody = "thing:contract-moved-third";
    ensure(store, &second).await;
    for (sets, key) in [
        (false, "contract_link_edited_own"),
        (true, "contract_link_edited_sets"),
    ] {
        let two = format!("kitchen, {second}, {first}, kitchen");
        let fact = NewFact {
            fields: BTreeMap::from([(key.into(), two.clone())]),
            ..NewFact::about(
                holder.clone(),
                format!("mixed links in {key}"),
                date(2026, 8, 2),
            )
        };
        let fact = if sets { state(fact) } else { fact };
        let written = store.capture(fact).await.unwrap().written().unwrap();
        let address = written.address();
        let claim = address.to_string();
        for target in [&first, &second] {
            assert!(
                names(store, target, &claim).await,
                "the capture links {target}"
            );
        }
        let before = store.recall(&holder).await.unwrap();
        let captured = before
            .iter()
            .find(|fact| fact.address() == address)
            .unwrap();
        assert_eq!(captured.fields.get(key), Some(&two));
        assert_eq!(
            captured.field_links.get(key),
            Some(&vec![second.clone(), first.clone()])
        );
        assert_eq!(captured.sets.contains(key), sets);

        let missing = format!("kitchen, {second}, {nobody}, {first}");
        let fact = NewFact {
            fields: BTreeMap::from([(key.into(), missing.clone())]),
            ..NewFact::about(
                holder.clone(),
                "a mixed capture that cannot land",
                date(2026, 8, 2),
            )
        };
        let fact = if sets { state(fact) } else { fact };
        let refused = store.capture(fact).await.unwrap();
        assert!(
            matches!(&refused, Guarded::Blocked { attempted, .. } if attempted.as_str() == nobody),
            "an unknown handle among words refuses the whole capture: {refused:?}"
        );
        assert_eq!(
            store.recall(&holder).await.unwrap(),
            before,
            "no partial capture"
        );

        let edited = format!("pantry, {first}, {second}, pantry");
        let patch = FactPatch {
            fields: BTreeMap::from([(key.into(), edited.clone())]),
            ..Default::default()
        };
        let patch = if sets { state_patch(patch) } else { patch };
        store
            .update_fact(&address, patch, &super::support::other_caller())
            .await
            .unwrap()
            .written()
            .unwrap();
        let before = store.recall(&holder).await.unwrap();
        let changed = before
            .iter()
            .find(|fact| fact.address() == address)
            .unwrap();
        assert_eq!(changed.fields.get(key), Some(&edited));
        assert_eq!(
            changed.field_links.get(key),
            Some(&vec![first.clone(), second.clone()])
        );
        assert_eq!(changed.sets.contains(key), sets);
        if sets {
            assert_eq!(store.fields(&holder).await.unwrap().get(key), Some(&edited));
        }
        let patch = FactPatch {
            fields: BTreeMap::from([(key.into(), missing)]),
            ..Default::default()
        };
        let patch = if sets { state_patch(patch) } else { patch };
        let refused = store
            .update_fact(&address, patch, &super::support::other_caller())
            .await
            .unwrap();
        assert!(
            matches!(&refused, Guarded::Blocked { attempted, .. } if attempted.as_str() == nobody),
            "an unknown handle among words refuses the whole edit: {refused:?}"
        );
        assert_eq!(
            store.recall(&holder).await.unwrap(),
            before,
            "no partial edit"
        );
        for target in [&first, &second] {
            assert!(
                names(store, target, &claim).await,
                "the edit keeps {target}"
            );
        }
    }
}

/// **A merge names a thing once in a mixed list.** The folded thing and the
/// survivor were both listed; after the merge the list names the survivor once
/// and keeps the word.
pub async fn a_merge_names_the_survivor_once_in_a_mixed_list<M: Memory>(store: &M) {
    let (_, holder) = world(store).await;
    let folded = EntityId("thing:contract-moved-first".into());
    let survivor = EntityId("thing:contract-moved-second".into());
    ensure(store, &folded).await;
    ensure(store, &survivor).await;
    let claim = written(
        store,
        &holder,
        "contract_link_merged",
        "lists both",
        format!("kitchen, {folded}, {survivor}"),
    )
    .await;
    store
        .merge(
            &folded,
            &survivor,
            None,
            date(2026, 8, 4),
            &super::support::other_caller(),
            None,
        )
        .await
        .expect("the merge lands");
    let served = store.fields(&holder).await.expect("the fields read");
    assert_eq!(
        served.get("contract_link_merged"),
        Some(&format!("kitchen, {survivor}")),
        "the survivor once, the word kept"
    );
    assert!(
        names(store, &survivor, &claim).await,
        "the survivor is linked"
    );
}

/// Reusing a spelling leaves both versions of a mixed value with their identity.
pub async fn mixed_rename_reuse_and_merge_preserve_the_history<M: Memory>(store: &M) {
    const KEY: &str = "contract_link_history";
    let (_, holder) = world(store).await;
    let old = EntityId("work:omicron".into());
    let renamed = EntityId("work:lambda".into());
    let survivor = EntityId("work:theta".into());
    for (id, name) in [(&old, "History"), (&survivor, "Survivor")] {
        super::support::add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let old_mark = crate::memory::marked_stored_handle(&badge_of(store, &old).await);
    let survivor_mark = crate::memory::marked_stored_handle(&badge_of(store, &survivor).await);
    let claim = written(
        store,
        &holder,
        KEY,
        "keeps a link through changes",
        format!("kitchen, {old}, kitchen, {survivor}"),
    )
    .await;
    let address = store
        .recall(&holder)
        .await
        .unwrap()
        .into_iter()
        .find(|fact| fact.address().to_string() == claim)
        .unwrap()
        .address();
    store
        .update_fact(
            &address,
            state_patch(FactPatch {
                fields: [(KEY.into(), format!("pantry, {old}, pantry, {survivor}"))]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }),
            &super::support::other_caller(),
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    for stage in 0..4 {
        match stage {
            1 => {
                store
                    .rename_entity(
                        &old,
                        &renamed,
                        None,
                        date(2026, 8, 3),
                        crate::memory::Override::none(),
                        None,
                    )
                    .await
                    .unwrap()
                    .written()
                    .unwrap();
            }
            2 => {
                let recreated = store
                    .add_entity(NewEntity::new(old.clone(), "Newcomer", "contract-fixture"))
                    .await
                    .unwrap();
                assert!(
                    matches!(&recreated, Guarded::Written(_)),
                    "the freed spelling can have a new owner: {recreated:?}"
                );
            }
            3 => {
                store
                    .merge(
                        &renamed,
                        &survivor,
                        None,
                        date(2026, 8, 4),
                        &super::support::other_caller(),
                        None,
                    )
                    .await
                    .unwrap();
            }
            _ => {}
        }
        let current = match stage {
            0 => &old,
            3 => &survivor,
            _ => &renamed,
        };
        let expected = if stage == 3 {
            format!("pantry, {survivor}, pantry")
        } else {
            format!("pantry, {current}, pantry, {survivor}")
        };
        assert_eq!(
            store.fields(&holder).await.unwrap().get(KEY),
            Some(&expected)
        );
        assert!(names(store, current, &claim).await);
        assert!(names(store, &survivor, &claim).await);
        if stage >= 2 {
            assert!(
                !names(store, &old, &claim).await,
                "the new spelling owner gets no link"
            );
        }
        let expected_links = if stage == 3 {
            vec![survivor.clone()]
        } else {
            vec![current.clone(), survivor.clone()]
        };
        let fact = store
            .recall(&holder)
            .await
            .unwrap()
            .into_iter()
            .find(|fact| fact.address() == address)
            .unwrap();
        assert_eq!(fact.field_links.get(KEY), Some(&expected_links));
        let history = store.history(&holder, KEY).await.unwrap();
        assert_eq!(history.len(), 2, "the replaced value remains readable");
        assert!(history.iter().all(|write| write.fact.to_string() == claim));
        let expected_history: Vec<_> = ["kitchen", "pantry"]
            .into_iter()
            .map(|word| {
                Some(if stage == 3 {
                    format!("{word}, {survivor_mark}, {word}")
                } else {
                    format!("{word}, {old_mark}, {word}, {survivor_mark}")
                })
            })
            .collect();
        assert_eq!(
            history
                .into_iter()
                .map(|write| write.value)
                .collect::<Vec<_>>(),
            expected_history
        );
    }
}

/// **An id is a link only under a key declared a reference.** Written under a key
/// nobody declared, the badge of a real thing is a word. Under a declared
/// reference key it is marked at the write and names the thing.
pub async fn a_bare_id_links_only_under_a_declared_reference_key<M: Memory>(store: &M) {
    let (target, holder) = world(store).await;
    let badge = badge_of(store, &target).await;
    let word = written(
        store,
        &holder,
        "contract_link_word",
        "an id as a word",
        badge.clone(),
    )
    .await;
    assert!(
        !names(store, &target, &word).await,
        "an id under an undeclared key is a word and links nothing"
    );
    store
        .declare_type(
            DeclaredType::new(
                "contract-link-by-id",
                vec![Field::listing("contract_link_by_id", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("the declaration lands");
    let linked = written(
        store,
        &holder,
        "contract_link_by_id",
        "an id as a link",
        badge.clone(),
    )
    .await;
    assert!(
        names(store, &target, &linked).await,
        "under a declared reference key it links"
    );
    let served = store.fields(&holder).await.expect("the fields read");
    assert_eq!(served.get("contract_link_by_id"), Some(&target.to_string()));
    let words = written(
        store,
        &holder,
        "contract_link_by_id",
        "words stay words",
        "pantry shelves".into(),
    )
    .await;
    let read = store.recall(&holder).await.unwrap();
    let words = read
        .iter()
        .find(|fact| fact.address().to_string() == words)
        .unwrap();
    assert!(words.field_handles().is_empty());
    assert_eq!(words.fields["contract_link_by_id"], "pantry shelves");
    let changed = store
        .update_fact(
            &words.address(),
            state_patch(FactPatch {
                fields: [("contract_link_by_id".into(), badge)]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }),
            &super::support::other_caller(),
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    assert_eq!(changed.field_handles(), vec![target.clone()]);
    assert!(names(store, &target, &changed.address().to_string()).await);

    let archived = EntityId("thing:contract-moved-nowhere".into());
    ensure(store, &archived).await;
    let archived_id = badge_of(store, &archived).await;
    let archived_row = store
        .archive_entity(&archived, "a fixture no longer active", None)
        .await
        .unwrap();
    assert!(archived_row.archived.is_some());
    let claim = written(
        store,
        &holder,
        "contract_link_by_id",
        "names an archived thing by id",
        archived_id,
    )
    .await;
    assert!(names(store, &archived, &claim).await);
    let read = store.recall(&holder).await.unwrap();
    assert_eq!(
        read.iter()
            .find(|fact| fact.address().to_string() == claim)
            .unwrap()
            .field_handles(),
        vec![archived]
    );

    let bot = EntityId("bot:gamma".into());
    ensure(store, &bot).await;
    store
        .declare_type(
            DeclaredType::new(
                "contract-link-by-line",
                vec![Field::pointing_at("line", EntityKind::BOT)],
            ),
            None,
        )
        .await
        .unwrap();
    let valid = written(store, &holder, "line", "names a line", bot.to_string()).await;
    assert!(names(store, &bot, &valid).await);
    let before = store.recall(&holder).await.unwrap();
    let fields = store.fields(&holder).await.unwrap();
    let address = before
        .iter()
        .find(|fact| fact.address().to_string() == valid)
        .unwrap()
        .address();
    let patch = state_patch(FactPatch {
        fields: [("line".into(), "dev7".into())].into_iter().collect(),
        ..Default::default()
    });
    let result = store
        .update_fact(&address, patch, &super::support::other_caller())
        .await;
    assert!(
        matches!(&result, Err(MemoryError::InvalidFact(_))),
        "a lone unknown reference name is refused: {result:?}"
    );
    assert_eq!(store.recall(&holder).await.unwrap(), before);
    assert_eq!(store.fields(&holder).await.unwrap(), fields);
    let result = store
        .capture(state(NewFact {
            fields: [("line".into(), "dev7".into())].into_iter().collect(),
            ..NewFact::about(holder.clone(), "names no line", date(2026, 8, 4))
        }))
        .await;
    assert!(
        matches!(&result, Err(MemoryError::InvalidFact(_))),
        "a lone unknown reference name is refused: {result:?}"
    );
    assert_eq!(store.recall(&holder).await.unwrap(), before);
    assert_eq!(store.fields(&holder).await.unwrap(), fields);
}

/// **A caller cannot send the mark.** The mark is how a store says an item was
/// decided to be a link, so a value that arrives wearing one is refused, on a
/// capture and on an edit; it would otherwise link to an id nobody checked.
pub async fn a_caller_cannot_send_the_mark<M: Memory>(store: &M) {
    let (first, holder) = world(store).await;
    let badge = badge_of(store, &first).await;
    let forged = format!("kitchen, {}", crate::memory::marked_stored_handle(&badge));
    let sent = store
        .capture(state(NewFact {
            fields: BTreeMap::from([("contract_link_forged".to_string(), forged.clone())]),
            ..NewFact::about(holder.clone(), "sends a mark", date(2026, 8, 5))
        }))
        .await;
    assert!(
        matches!(sent, Err(MemoryError::InvalidFact(_))),
        "a capture carrying the mark is refused, got {sent:?}"
    );
    let claim = written(
        store,
        &holder,
        "contract_link_forged",
        "sends none",
        value(&first),
    )
    .await;
    let address = store
        .recall(&holder)
        .await
        .expect("the claims read")
        .into_iter()
        .find(|fact| fact.address().to_string() == claim)
        .expect("the claim reads back")
        .address();
    let edited = store
        .update_fact(
            &address,
            state_patch(FactPatch {
                fields: BTreeMap::from([("contract_link_forged".to_string(), forged)]),
                ..Default::default()
            }),
            &super::support::other_caller(),
        )
        .await;
    assert!(
        matches!(edited, Err(MemoryError::InvalidFact(_))),
        "an edit carrying the mark is refused, got {edited:?}"
    );
}

/// A declaration describes the value without revising the links already stored.
pub async fn a_declaration_replacement_and_key_removal_change_no_link<M: Memory>(store: &M) {
    let (first, holder) = world(store).await;
    let second = EntityId("thing:contract-moved-second".into());
    let other_holder = EntityId("thing:contract-links-holder".into());
    ensure(store, &second).await;
    ensure(store, &other_holder).await;
    let value = format!("kitchen, {second}, {first}, kitchen");
    for (declared_first, key, type_name) in [
        (true, "contract_link_before", "contract-link-before"),
        (false, "contract_link_after", "contract-link-after"),
    ] {
        let reference =
            DeclaredType::new(type_name, vec![Field::listing(key, ValueType::Reference)]);
        if declared_first {
            store.declare_type(reference.clone(), None).await.unwrap();
        }
        let claims = [
            written(
                store,
                &holder,
                key,
                "links before a type changes",
                value.clone(),
            )
            .await,
            written(
                store,
                &other_holder,
                key,
                "another holder of the links",
                value.clone(),
            )
            .await,
        ];
        let mut original_order = None;
        for declared in [
            None,
            Some(reference),
            Some(DeclaredType::new(
                type_name,
                vec![Field::new(key, ValueType::Text)],
            )),
            Some(DeclaredType::new(
                type_name,
                vec![Field::new("contract_link_other", ValueType::Text)],
            )),
        ] {
            if let Some(declared) = declared {
                store.declare_type(declared, None).await.unwrap();
            }
            for owner in [&holder, &other_holder] {
                assert_eq!(store.fields(owner).await.unwrap().get(key), Some(&value));
                let fact = store
                    .recall(owner)
                    .await
                    .unwrap()
                    .into_iter()
                    .find(|fact| claims.contains(&fact.address().to_string()))
                    .expect("the written claim reads");
                assert_eq!(fact.fields.get(key), Some(&value));
                assert_eq!(
                    fact.field_links.get(key),
                    Some(&vec![second.clone(), first.clone()])
                );
            }
            let mut order = Vec::new();
            for target in [&first, &second] {
                let returned: Vec<String> = store
                    .referring_to(target)
                    .await
                    .unwrap()
                    .iter()
                    .map(|fact| fact.address().to_string())
                    .filter(|address| claims.contains(address))
                    .collect();
                assert_eq!(returned.len(), 2, "both holders still point at {target}");
                for claim in &claims {
                    assert!(returned.contains(claim));
                }
                order.push(returned);
            }
            if let Some(before) = &original_order {
                assert_eq!(
                    &order, before,
                    "the returned order is unchanged, without sorting"
                );
            } else {
                original_order = Some(order);
            }
        }
    }
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_link_at_write<M: Memory>(store: &M) {
    a_handle_in_a_mixed_list_links_at_the_write_and_a_later_type_changes_nothing(store).await;
    a_link_in_a_mixed_list_follows_a_rename_of_its_target(store).await;
    an_edit_decides_links_at_the_write_as_a_capture_does(store).await;
    a_merge_names_the_survivor_once_in_a_mixed_list(store).await;
    mixed_rename_reuse_and_merge_preserve_the_history(store).await;
    a_bare_id_links_only_under_a_declared_reference_key(store).await;
    a_caller_cannot_send_the_mark(store).await;
    a_declaration_replacement_and_key_removal_change_no_link(store).await;
}

//! **The link decision of rule 646, switched on, in a process of its own.**
//!
//! The switch is process-wide: the readers that draw links from a served value
//! hold no store to ask. A case that turns it on reaches every other test in its
//! process, so these get a binary to themselves, as the kind set does.

use jojobot_domain::memory::testing::{InMemoryMemory, contract};
use jojobot_domain::memory::{
    EntityId, LinkRule, Memory, NewEntity, NewFact, Override, link_rule, set_link_rule,
};

macro_rules! typed_kind_cases {
    ($($test:ident => $case:ident),+ $(,)?) => {$(
        #[tokio::test]
        async fn $test() {
            set_link_rule(LinkRule::On);
            contract::typed_kinds::$case(&InMemoryMemory::booted()).await;
        }
    )+};
}

typed_kind_cases!(
    b5_on_numbered_location_capture => b5_numbered_location_capture,
    b5_on_numbered_location_capture_refuses_a_person => b5_numbered_location_capture_refuses_a_person,
    b5_on_numbered_location_edit => b5_numbered_location_edit,
    b5_on_numbered_location_edit_refuses_a_person => b5_numbered_location_edit_refuses_a_person,
    b5_on_numbered_reference_capture => b5_numbered_reference_capture,
    b5_on_numbered_reference_edit => b5_numbered_reference_edit,
);

#[tokio::test]
async fn link_rule_carry_keeps_both_existence_gates() {
    set_link_rule(LinkRule::On);
    contract::a_numbered_reference_keeps_both_existence_gates(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn the_fake_links_by_mark_when_the_rule_is_on() {
    set_link_rule(LinkRule::On);
    assert_eq!(link_rule(), LinkRule::On);
    contract::run_all_link_at_write(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn link_rule_leftovers_declarations_do_not_change_stored_links() {
    set_link_rule(LinkRule::On);
    contract::a_declaration_replacement_and_key_removal_change_no_link(&InMemoryMemory::booted())
        .await;
}

#[tokio::test]
async fn link_rule_leftovers_capture_and_edit_are_atomic_in_both_bags() {
    set_link_rule(LinkRule::On);
    contract::an_edit_decides_links_at_the_write_as_a_capture_does(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn link_rule_leftovers_mixed_history_keeps_identity_when_a_spelling_is_reused() {
    set_link_rule(LinkRule::On);
    contract::mixed_rename_reuse_and_merge_preserve_the_history(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn link_rule_leftovers_bare_ids_words_and_lone_names_remain_distinct() {
    set_link_rule(LinkRule::On);
    contract::a_bare_id_links_only_under_a_declared_reference_key(&InMemoryMemory::booted()).await;
}

async fn legacy_is_text(
    memory: &InMemoryMemory,
    holder: &EntityId,
    plain: &EntityId,
    marked: &EntityId,
    unlinked: &[&EntityId],
) {
    let fields = memory.fields(holder).await.unwrap();
    assert_eq!(fields["owner_of"], plain.to_string());
    assert_eq!(fields["legacy"], format!("kitchen, {plain}, {marked}"));
    let claims = memory.recall(holder).await.unwrap();
    let links: Vec<_> = claims
        .iter()
        .flat_map(|fact| fact.field_handles())
        .collect();
    assert_eq!(links, vec![marked.clone(), marked.clone()]);
    let referring = memory.referring_to(marked).await.unwrap();
    assert_eq!(referring.len(), 2);
    let declarations = memory.declared_types().await.unwrap();
    let keys = jojobot_domain::memory::graph::reference_keys(&declarations);
    let held = jojobot_domain::memory::entitlement::ranked(
        &referring,
        marked,
        jiff::civil::date(2026, 8, 3),
        &keys,
        jojobot_domain::memory::entitlement::Widen::WhenEmpty,
    );
    assert_eq!(
        held.len(),
        if keys.iter().any(|key| key == "legacy") {
            2
        } else {
            0
        },
        "held uses the marked link in the mixed value too"
    );
    for target in unlinked {
        assert!(
            memory.referring_to(target).await.unwrap().is_empty(),
            "unmarked text links to neither the former nor the current owner: {target}"
        );
    }
    use jojobot_domain::memory::graph::{self, Along, Direction, Follow, GraphQuery};
    let mut paths = vec![Along::AnyEdge];
    for key in ["legacy", "marked"] {
        if keys.iter().any(|declared| declared == key) {
            paths.push(Along::Relation(key.into()));
        }
    }
    for along in paths {
        let mut asks = vec![
            (holder, Direction::Out, vec![marked.clone()]),
            (marked, Direction::In, vec![holder.clone()]),
        ];
        asks.extend(unlinked.iter().map(|id| (*id, Direction::In, vec![])));
        for (root, direction, expected) in asks {
            let mut query = GraphQuery::subject(root.clone());
            query.include.facts = true;
            query.follow = Some(Follow {
                along: along.clone(),
                direction: Some(direction),
                ..Follow::hop()
            });
            let answer = graph::walk(memory, &[], &query).await.unwrap();
            assert_eq!(answer.objects.len(), 1);
            let reached: Vec<_> = answer.objects[0]
                .connected
                .iter()
                .map(|object| object.entity.id.clone())
                .collect();
            assert_eq!(
                reached, expected,
                "{root} {direction:?} along {along:?} follows marks only"
            );
        }
    }
}

#[tokio::test]
async fn link_rule_leftovers_unmarked_legacy_items_link_nowhere_after_rename_and_reuse() {
    set_link_rule(LinkRule::On);
    let memory = InMemoryMemory::booted().with_own_fields_reaching();
    let [holder, alpha, beta, renamed] = [
        "thing:contract-links-holder",
        "work:contract-links-alpha",
        "work:contract-links-beta",
        "work:contract-links-now",
    ]
    .map(|id| EntityId(id.into()));
    for (id, name) in [(&holder, "Holder"), (&alpha, "Alpha"), (&beta, "Beta")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    memory
        .capture(contract::state(NewFact {
            fields: [("marked".into(), beta.to_string())].into_iter().collect(),
            ..NewFact::about(
                holder.clone(),
                "names a marked thing",
                jiff::civil::date(2026, 8, 1),
            )
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
        .find(|entity| entity.id == beta)
        .unwrap()
        .badge
        .unwrap();
    // This is historical stored text that the write guard cannot create now.
    memory.fields_past_the_guard(
        &holder,
        &[
            ("owner_of", alpha.as_str()),
            (
                "legacy",
                &format!(
                    "kitchen, {alpha}, {}",
                    jojobot_domain::memory::marked_stored_handle(&badge)
                ),
            ),
        ],
    );
    legacy_is_text(&memory, &holder, &alpha, &beta, &[&alpha]).await;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
    for fields in [
        vec![
            Field::listing("marked", ValueType::Reference),
            Field::listing("legacy", ValueType::Reference),
        ],
        vec![
            Field::new("marked", ValueType::Reference),
            Field::new("legacy", ValueType::Reference),
        ],
        vec![Field::new("legacy", ValueType::Text)],
    ] {
        memory
            .declare_type(DeclaredType::new("contract-legacy-links", fields), None)
            .await
            .unwrap();
        legacy_is_text(&memory, &holder, &alpha, &beta, &[&alpha]).await;
    }
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
    legacy_is_text(&memory, &holder, &alpha, &beta, &[&renamed]).await;
    memory
        .add_entity(NewEntity::new(
            alpha.clone(),
            "Newcomer",
            "contract-fixture",
        ))
        .await
        .unwrap()
        .written()
        .unwrap();
    legacy_is_text(&memory, &holder, &alpha, &beta, &[&alpha, &renamed]).await;
}

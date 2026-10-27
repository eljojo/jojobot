use std::sync::Arc;

use super::*;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::memory::graph::{self, Along, Direction, Follow, GraphQuery};
use jojobot_domain::memory::search::{EdgeFilter, Hit, Search, SearchQuery};

pub(super) async fn assert_links(
    memory: Arc<dyn Memory>,
    holder: &EntityId,
    plain: &EntityId,
    marked: &EntityId,
) {
    let claims = memory.recall(holder).await.unwrap();
    let linked: Vec<_> = claims
        .iter()
        .filter(|fact| fact.fields.contains_key("labels") || fact.fields.contains_key("legacy"))
        .collect();
    assert_eq!(linked.len(), 2);
    let expected: Vec<_> = linked.iter().map(|fact| fact.address()).collect();
    assert_eq!(
        claims
            .iter()
            .filter_map(|fact| fact.fields.get("owner_of"))
            .collect::<Vec<_>>(),
        vec![&plain.to_string()]
    );
    assert_eq!(
        memory.fields(holder).await.unwrap()["labels"],
        format!("kitchen, {marked}")
    );
    let doc = memory.scan_entity(holder).await.unwrap().unwrap();
    assert_eq!(doc.field_links.get("labels"), Some(&vec![marked.clone()]));
    assert!(!doc.field_links.contains_key("owner_of"));
    let referring = memory.referring_to(marked).await.unwrap();
    assert_eq!(
        referring
            .iter()
            .map(|fact| fact.address())
            .collect::<Vec<_>>(),
        expected
    );
    assert!(memory.referring_to(plain).await.unwrap().is_empty());
    let declared = memory.declared_types().await.unwrap();
    let keys = graph::reference_keys(&declared);
    let held = jojobot_domain::memory::entitlement::ranked(
        &referring,
        marked,
        date(2026, 8, 3),
        &keys,
        jojobot_domain::memory::entitlement::Widen::WhenEmpty,
    );
    assert_eq!(
        held.iter()
            .map(|held| held.fact.address())
            .collect::<Vec<_>>(),
        if keys.iter().any(|key| key == "labels") {
            expected.clone()
        } else {
            Vec::new()
        }
    );

    let indexed = Arc::new(IndexedMemory::new(memory.clone()).unwrap());
    indexed.rebuild().await.unwrap();
    let search = Retrieval::new(indexed.index(), vec![indexed]);
    for claim in &linked {
        let hits = search
            .search(&SearchQuery {
                text: Some(claim.content.clone()),
                subject: Some(holder.clone()),
                edge: Some(EdgeFilter {
                    shape: None,
                    object: marked.clone(),
                }),
                ..Default::default()
            })
            .await
            .unwrap();
        let facts: Vec<_> = hits
            .iter()
            .filter_map(|hit| match hit {
                Hit::Fact { fact, .. } => Some(fact),
                _ => None,
            })
            .collect();
        assert_eq!(
            facts.iter().map(|fact| fact.address()).collect::<Vec<_>>(),
            vec![claim.address()]
        );
        assert_eq!(facts[0].field_handles(), vec![marked.clone()]);
    }
    let hits = search
        .search(&SearchQuery {
            edge: Some(EdgeFilter {
                shape: None,
                object: plain.clone(),
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        hits.is_empty(),
        "unmarked words draw no search posting: {hits:?}"
    );

    let mut paths = vec![(Along::AnyEdge, true)];
    for key in ["labels", "legacy", "owner_of"] {
        if keys.iter().any(|declared| declared == key) {
            paths.push((Along::Relation(key.into()), key != "owner_of"));
        }
    }
    for (along, positive) in paths {
        for (root, direction, expected) in [
            (
                holder,
                Direction::Out,
                if positive {
                    vec![marked.clone()]
                } else {
                    vec![]
                },
            ),
            (
                marked,
                Direction::In,
                if positive {
                    vec![holder.clone()]
                } else {
                    vec![]
                },
            ),
            (plain, Direction::In, vec![]),
        ] {
            let mut query = GraphQuery::subject(root.clone());
            query.include.facts = true;
            query.follow = Some(Follow {
                along: along.clone(),
                direction: Some(direction),
                ..Follow::hop()
            });
            let answer = graph::walk(memory.as_ref(), &[], &query).await.unwrap();
            assert_eq!(answer.objects.len(), 1);
            assert_eq!(
                answer.objects[0]
                    .connected
                    .iter()
                    .map(|object| object.entity.id.clone())
                    .collect::<Vec<_>>(),
                expected,
                "{root} {direction:?} {along:?}"
            );
        }
    }
}

#[tokio::test]
async fn link_rule_leftovers_search_and_walk_read_marked_own_and_held_fields() {
    use jojobot_domain::memory::testing::InMemoryMemory;
    let memory = Arc::new(InMemoryMemory::booted());
    let [holder, plain, marked] = [
        "thing:contract-links-holder",
        "work:contract-links-alpha",
        "work:contract-links-beta",
    ]
    .map(|id| EntityId(id.into()));
    for (id, name) in [(&holder, "Holder"), (&plain, "Alpha"), (&marked, "Beta")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    memory
        .capture(memory::state(NewFact {
            fields: [("labels".into(), format!("kitchen, {marked}"))]
                .into_iter()
                .collect(),
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
            (
                "legacy",
                &format!(
                    "pantry, {}",
                    jojobot_domain::memory::marked_stored_handle(&badge)
                ),
            ),
        ],
    );
    assert_links(memory.clone(), &holder, &plain, &marked).await;
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
        memory
            .declare_type(DeclaredType::new("contract-legacy-links", fields), None)
            .await
            .unwrap();
        assert_links(memory.clone(), &holder, &plain, &marked).await;
        let folded = Arc::new(jojobot_adapters::fold::Folded::new(memory.clone()));
        folded.rebuild().await.unwrap();
        assert_links(folded.clone(), &holder, &plain, &marked).await;
        folded.rebuild().await.unwrap();
        assert_links(folded, &holder, &plain, &marked).await;
    }
}

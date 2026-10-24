//! **A key an expression asks about is judged against everything the call
//! selects**, never against the part of it the walk happened to read. A caller
//! that hands in candidates (the things an index kept for one term) leaves the
//! other selected things unread, and a key only those hold is still held.

use super::support::{add, capture, state};
use super::*;
use crate::memory::filter::{Filter, Test};
use std::collections::BTreeMap;

const KIND: &str = "contract-where-keys";

/// **The handles of one case.** Three things hold `queue` and are the candidates
/// an index kept for it, one other holds the case's `release` key, and the rest
/// hold neither. The names share no word beyond the prefix, so the near-miss
/// guard lets each one stand, and no two cases meet in one name.
struct Case {
    queued: [&'static str; 3],
    holder: &'static str,
    plain: [&'static str; 2],
    release: &'static str,
}

const HELD_BY_A_THING: Case = Case {
    queued: [
        "thing:contract-where-walnut",
        "thing:contract-where-pecan",
        "thing:contract-where-cashew",
    ],
    holder: "thing:contract-where-hazel",
    plain: ["thing:contract-where-maple", "thing:contract-where-birch"],
    release: "release_a",
};

const MISSPELT: Case = Case {
    queued: [
        "thing:contract-where-cedar",
        "thing:contract-where-willow",
        "thing:contract-where-aspen",
    ],
    holder: "thing:contract-where-alder",
    plain: ["thing:contract-where-spruce", "thing:contract-where-poplar"],
    release: "release_b",
};

/// The holder is of another kind than the selection, so only a walk reaches it.
const HELD_BY_A_TOPIC: Case = Case {
    queued: [
        "thing:contract-where-orchid",
        "thing:contract-where-lilac",
        "thing:contract-where-tulip",
    ],
    holder: "topic:contract-where-daisy",
    plain: ["thing:contract-where-peony", "thing:contract-where-iris"],
    release: "release_c",
};

/// A thing holding these keys, folded onto it by one claim.
async fn holding<M: Memory>(store: &M, id: &EntityId, held: &[(&str, &str)]) {
    add(store, NewEntity::new(id.clone(), id.slug(), KIND)).await;
    let fields: BTreeMap<String, String> = held
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    capture(
        store,
        state(NewFact {
            fields,
            ..NewFact::about(id.clone(), "what it holds", date(2026, 6, 1))
        }),
    )
    .await;
}

/// **The case's things, made.** The candidates are the first read of the call
/// and none of them holds the release key; the holder is beyond them.
async fn a_selection_with_a_holder_beyond_its_candidates<M: Memory>(
    store: &M,
    case: &Case,
) -> Vec<EntityId> {
    let handle = |text: &str| EntityId(text.to_string());
    let mut candidates = Vec::new();
    for queued in case.queued {
        holding(store, &handle(queued), &[("queue", "waiting")]).await;
        candidates.push(handle(queued));
    }
    holding(store, &handle(case.holder), &[(case.release, "nine")]).await;
    for plain in case.plain {
        holding(store, &handle(plain), &[("shade", "deep")]).await;
    }
    candidates
}

fn asking(candidates: &[EntityId], key: &str) -> graph::GraphQuery {
    let kept = Filter::All(vec![
        Filter::leaf(Some("queue"), Test::Equals("waiting".into())),
        Filter::leaf(Some(key), Test::Equals("nine".into())),
    ]);
    graph::GraphQuery {
        select: graph::Selection {
            kind: Some(EntityKind::THING),
            candidates: Some(candidates.to_vec()),
            candidates_exact: false,
            fields: vec![graph::FieldFilter::expression(kept, graph::Scope::Thing)],
            ..graph::Selection::default()
        },
        ..graph::GraphQuery::default()
    }
}

/// **A held key is asked, and the answer is the empty one it earns.** The key is
/// held by a thing the candidates leave out, so no candidate keeps it, and the
/// call answers that none does. Refusing it as held by none would send the
/// caller to a spelling that is right.
pub async fn a_where_on_a_key_held_beyond_the_candidates_is_answered<M: Memory>(store: &M) {
    let candidates = a_selection_with_a_holder_beyond_its_candidates(store, &HELD_BY_A_THING).await;
    let answer = graph::walk(store, &[], &asking(&candidates, HELD_BY_A_THING.release))
        .await
        .expect("a key a selected thing holds is asked, not refused");
    assert!(
        answer.objects.is_empty(),
        "no candidate keeps both terms: {:?}",
        answer
            .objects
            .iter()
            .map(|o| &o.entity.id)
            .collect::<Vec<_>>(),
    );
}

/// **A key nobody holds is still refused, whatever the candidates.** The pair of
/// the case above: the refusal names the keys the whole selection holds, the one
/// that sits outside the candidates among them.
pub async fn a_where_on_a_key_nobody_holds_is_refused_beyond_the_candidates<M: Memory>(store: &M) {
    let candidates = a_selection_with_a_holder_beyond_its_candidates(store, &MISSPELT).await;
    let refused = graph::walk(store, &[], &asking(&candidates, "reelase_b"))
        .await
        .expect_err("a key no selected thing holds is a misspelling");
    let said = refused.to_string();
    assert!(said.contains("reelase_b"), "it names the key: {said}");
    assert!(
        said.contains("release_b") && said.contains("queue"),
        "it lists what the whole selection holds, the holder beyond the candidates too: {said}",
    );
}

/// **A walk's `keeping` is judged against every thing the world holds**, not the
/// candidates alone: the holder of the key is one the candidates leave out.
pub async fn a_keeping_on_a_key_held_beyond_the_candidates_is_answered<M: Memory>(store: &M) {
    let candidates = a_selection_with_a_holder_beyond_its_candidates(store, &HELD_BY_A_TOPIC).await;
    let mut query = asking(&candidates, "queue");
    query.select.fields.clear();
    query.follow = Some(graph::Follow {
        along: graph::Along::Edge(EdgeShape::Connection),
        direction: Some(graph::Direction::Out),
        depth: 1,
        keeping: vec![graph::FieldFilter::expression(
            Filter::leaf(Some("release_c"), Test::Equals("nine".into())),
            graph::Scope::Thing,
        )],
        fits_type: None,
    });
    graph::walk(store, &[], &query)
        .await
        .expect("a key some thing holds is kept by, not refused");
}

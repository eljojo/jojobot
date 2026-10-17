//! **A query costs about what it returns (rule 432), and answers what it did.**
//!
//! `where` leaves and key filters are routed to the index of the keys a thing
//! holds, and the leaves that index cannot hold (`!=`, `contains`, the four
//! orderings, and the order of an answer) to the store, which keeps its values by
//! key. A routed answer reads only the page that fits under the answer ceiling.
//! These cases hold both halves: every routed shape answers exactly as the read
//! of everyone does, page after page, and the Memory-port calls a query makes
//! follow what it returns and not the size of the store.

use std::sync::Arc;

use jojobot_adapters::counted::Counted;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::mention::Mentioning;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity, NewFact};
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::teaching::testing::InMemoryTeachings;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};

use crate::harness::{TEST_SID, handler_indexed_over, json_of, seeded_registry};
use crate::memory::testing::SpySearch;
use crate::{Jojobot, memory::recall::RecallArgs};
use jojobot_domain::memory::search::Coverage;

/// A handle slug that is far from every other, so the near-miss screen has
/// nothing to say about a fixture of hundreds.
fn slug(n: u64) -> String {
    let mut x = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    format!("{:016x}", x ^ (x >> 31))
}

fn handle(kind: &str, n: u64) -> EntityId {
    EntityId(format!("{kind}:{}", slug(n)))
}

/// **A store of `n` pieces of tracked work.** A quarter are `now`, a quarter
/// `next`, a tenth `waiting` and the rest `done`; every seventh is Bea's. Each
/// carries three records, its points, its due day and its tags among the first.
/// Every thirteenth thing that is `now` is archived, and every eleventh that is
/// `waiting` too, so a routed browse and the read of everyone have archived
/// matches to count and shapes with none.
async fn tracked(n: usize) -> Arc<InMemoryMemory> {
    let store = Arc::new(InMemoryMemory::seeded());
    store
        .declare_type(
            DeclaredType::new(
                "ticket",
                vec![
                    Field::new("status", ValueType::Text),
                    Field::new("assignee", ValueType::Text),
                    Field::new("points", ValueType::Number),
                    Field::new("due", ValueType::Date),
                    Field::listing("tags", ValueType::Text),
                ],
            ),
            None,
        )
        .await
        .expect("the type is declared");
    for i in 0..n {
        let id = handle("thing", 1_000 + i as u64);
        store
            .add_entity(NewEntity::new(
                id.clone(),
                format!("Piece {}", slug(5_000 + i as u64)),
                "fixture",
            ))
            .await
            .expect("added")
            .written()
            .expect("not blocked");
        let status = match i % 20 {
            0..=4 => "now",
            5..=9 => "next",
            10 | 11 => "waiting",
            _ => "done",
        };
        let who = if i % 7 == 0 { "bea" } else { "carl" };
        // A value that is no value of its type is there to be left out of every
        // range and every ordering, as the evaluator leaves it.
        let points = if i % 47 == 0 {
            "lots".to_string()
        } else {
            (i % 9).to_string()
        };
        let due = if i % 53 == 0 {
            "soon".to_string()
        } else {
            format!("2026-{:02}-{:02}", 1 + i % 12, 1 + i % 27)
        };
        let tags = match i % 10 {
            0 => "kitchen, garden",
            1 => "garden",
            2 => "kitchen",
            _ => "other",
        };
        let mut first =
            NewFact::about(id.clone(), "where it stands", jiff::civil::date(2026, 2, 1));
        for (key, value) in [
            ("status", status),
            ("assignee", who),
            ("points", points.as_str()),
            ("due", due.as_str()),
            ("tags", tags),
        ] {
            first.fields.insert(key.into(), value.into());
            first.sets.insert(key.into());
        }
        first.session = Some("fixture".into());
        store
            .capture(first)
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
        for note in ["a second record", "a third record"] {
            let mut more = NewFact::about(id.clone(), note, jiff::civil::date(2026, 2, 1));
            more.session = Some("fixture".into());
            store
                .capture(more)
                .await
                .expect("captured")
                .written()
                .expect("not blocked");
        }
        if (status == "now" && i % 13 == 0) || (status == "waiting" && i % 11 == 0) {
            store
                .archive_entity(&id, "finished early", None)
                .await
                .expect("archived");
        }
    }
    store
}

/// A recall, asked as a caller sends it.
async fn asked(jojobot: &Jojobot, mut args: Value) -> Value {
    args["sid"] = json!(TEST_SID);
    let parsed: RecallArgs = serde_json::from_value(args).expect("the arguments read");
    json_of(
        &jojobot
            .recall(Parameters(parsed))
            .await
            .expect("recall answers"),
    )
}

/// The handles an answer lists, in order.
fn listed(answer: &Value) -> Vec<String> {
    answer["objects"]
        .as_array()
        .expect("an answer lists its objects")
        .iter()
        .map(|object| object["id"].as_str().expect("an id").to_string())
        .collect()
}

/// **Every routed shape.** Each is asked with the `where` line or the key filter
/// a caller sends.
fn shapes() -> Vec<(&'static str, Value)> {
    vec![
        ("an equality", json!({"where": "status = now"})),
        (
            "an equality on most of the store",
            json!({"where": "status = done"}),
        ),
        ("a set", json!({"where": "status in (now, waiting)"})),
        ("a bare key", json!({"where": "assignee"})),
        ("an and", json!({"where": "status = now && assignee = bea"})),
        (
            "an and with a conjunct the index cannot serve",
            json!({"where": "status = done && assignee != bea"}),
        ),
        (
            "an and with a conjunct the index cannot serve, over archived matches",
            json!({"where": "status = waiting && assignee != bea"}),
        ),
        (
            "an or",
            json!({"where": "status = now || status = waiting"}),
        ),
        (
            "an or with a branch the index cannot serve",
            json!({"where": "status = now || !(assignee = carl)"}),
        ),
        (
            "an and over an or",
            json!({"where": "(status = now || status = next) && assignee = bea"}),
        ),
        (
            "a key filter",
            json!({"fields": [{"key": "status", "value": "now"}]}),
        ),
        ("a !=", json!({"where": "status != done"})),
        (
            "a != beside an equality",
            json!({"where": "status = now && assignee != bea"}),
        ),
        ("a contains", json!({"where": "tags contains kitchen"})),
        (
            "a range of dates, a word among them",
            json!({"where": "due <= 2026-02-01"}),
        ),
        (
            "a range of numbers, a word among them",
            json!({"where": "points >= 5"}),
        ),
        (
            "a range and an equality",
            json!({"where": "due > 2026-06-01 && status = now"}),
        ),
        (
            "an or of two asks of the store",
            json!({"where": "tags contains kitchen || due < 2026-01-20"}),
        ),
        (
            "an order by a date",
            json!({"kind": "thing", "order": {"key": "due"}}),
        ),
        (
            "an order by a date, largest first",
            json!({"kind": "thing", "order": {"key": "due", "direction": "descending"}}),
        ),
        (
            "an order by a number, a word among them",
            json!({"kind": "thing", "order": {"key": "points"}}),
        ),
        (
            "an order by a key nobody declared a type for",
            json!({"kind": "thing", "order": {"key": "assignee"}}),
        ),
        (
            "an equality and an order",
            json!({"where": "status = now", "order": {"key": "points", "direction": "descending"}}),
        ),
        (
            "a leaf the store answers and an order",
            json!({"where": "tags contains kitchen", "order": {"key": "due"}}),
        ),
        ("a type", json!({"answers_type": "ticket"})),
        (
            "a type and a key filter",
            json!({"answers_type": "ticket", "fields": [{"key": "status", "value": "now"}]}),
        ),
    ]
}

/// **The read of everyone**: the same store behind an index that has not read it,
/// whose candidates are never believed, so every thing is read and asked.
fn reference(store: Arc<InMemoryMemory>) -> Jojobot {
    Jojobot::new(
        store,
        Arc::new(SpySearch::over_memory(Coverage::Unread, Vec::new())),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A routed answer is the answer the read of everyone gives, on every page.**
/// The store is the same; the reference is a handler whose index is never
/// believed, so every candidate is read.
///
/// **What is compared**: the objects both answers show, in order (one answer may
/// show one more than the other, because a routed answer says
/// `candidates_capped` and a few bytes of that move the cut at the ceiling), and
/// how many things the call selected in all: the objects before the page, the
/// objects on it and the objects left. Each page is asked at an offset the
/// reference named.
#[tokio::test]
async fn every_routed_shape_answers_as_the_read_of_everyone_does_page_after_page() {
    let store = tracked(160).await;
    let reference = reference(store.clone());
    let selected_in_all = |answer: &Value, offset: u64| {
        offset
            + answer["count"].as_u64().unwrap_or(0)
            + answer["not_shown"]["count"].as_u64().unwrap_or(0)
    };
    let mut cut_somewhere = false;
    // **Two stacks answer: the index over the store, whose port answers by the
    // full read, and the index over the fold, whose port answers from the values
    // it keeps by key.** Both are held to the read of everyone.
    for (stack, routed) in [
        ("over the store", handler_indexed_over(store.clone())),
        ("over the fold", folded_over(store.clone()).await),
    ] {
        for (name, shape) in shapes() {
            let mut offset = 0;
            for page in 0..2 {
                let mut args = shape.clone();
                args["offset"] = json!(offset);
                let want = asked(&reference, args.clone()).await;
                let got = asked(&routed, args).await;
                let (got_ids, want_ids) = (listed(&got), listed(&want));
                let shared = got_ids.len().min(want_ids.len());
                assert!(
                    shared + 1 >= got_ids.len().max(want_ids.len()),
                    "{stack}, {name}, page {page}: {} objects routed and {} by the read of \
                     everyone",
                    got_ids.len(),
                    want_ids.len()
                );
                assert_eq!(
                    got_ids[..shared],
                    want_ids[..shared],
                    "{stack}, {name}, page {page}: the objects"
                );
                assert_eq!(
                    selected_in_all(&got, offset),
                    selected_in_all(&want, offset),
                    "{stack}, {name}, page {page}: how many things the call selected"
                );
                for count in ["archived_excluded", "merged_excluded", "excluded_note"] {
                    assert_eq!(
                        got[count], want[count],
                        "{stack}, {name}, page {page}: {count}"
                    );
                }
                assert!(
                    !want_ids.is_empty() || page > 0,
                    "{name}: the first page is empty, so this compares nothing"
                );
                match want["not_shown"]["offset"].as_u64() {
                    Some(next) => {
                        cut_somewhere = true;
                        offset = next;
                    }
                    None => break,
                }
            }
        }
    }
    assert!(
        cut_somewhere,
        "no shape was cut at the ceiling, so no page compared"
    );
}

/// **The handler over the index over the mention layer over the fold over the
/// store**, in the order the binary stacks them.
async fn folded_over(store: Arc<InMemoryMemory>) -> Jojobot {
    let below: Arc<dyn Memory> = store;
    let folded = Arc::new(Folded::new(below));
    folded.rebuild().await.expect("the fold loads");
    let mentioning = Arc::new(Mentioning::new(folded));
    let indexed = Arc::new(IndexedMemory::new(mentioning).expect("the index opens"));
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    Jojobot::new(
        indexed,
        search,
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A stack that counts what the query layer asks the Memory port.** The store,
/// the fold over it, a counter above the fold, the real index, and the handler.
async fn counted_stack(n: usize) -> (Jojobot, Arc<Counted>) {
    let store = tracked(n).await;
    let below: Arc<dyn Memory> = store;
    let folded = Arc::new(Folded::new(below));
    folded.rebuild().await.expect("the fold loads");
    let above = Arc::new(Counted::new(folded));
    let indexed = Arc::new(IndexedMemory::new(above.clone()).expect("the index opens"));
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    let jojobot = Jojobot::new(
        indexed,
        search,
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    );
    (jojobot, above)
}

/// **A routed query makes about as many port calls as it shows objects**, and the
/// number does not follow the size of the store or of the candidate set. Read
/// twice: the first read of a fresh stack pays for the index and is not the cost
/// of the query.
#[tokio::test]
async fn a_routed_query_reads_about_what_it_returns() {
    let (jojobot, above) = counted_stack(400).await;
    for (name, shape) in [
        ("an equality", json!({"where": "status = now"})),
        (
            "an equality on most of the store",
            json!({"where": "status = done"}),
        ),
        (
            "an or",
            json!({"where": "status = now || status = waiting"}),
        ),
        (
            "a key filter",
            json!({"fields": [{"key": "status", "value": "done"}]}),
        ),
        ("a !=", json!({"where": "status != done"})),
        ("a contains", json!({"where": "tags contains kitchen"})),
        ("a range of dates", json!({"where": "due <= 2026-04-01"})),
        (
            "a != beside an equality",
            json!({"where": "status = now && assignee != bea"}),
        ),
        (
            "an order by a date",
            json!({"kind": "thing", "order": {"key": "due"}}),
        ),
        (
            "an order by a number, largest first",
            json!({"kind": "thing", "order": {"key": "points", "direction": "descending"}}),
        ),
        (
            "a leaf the store answers and an order",
            json!({"where": "tags contains kitchen", "order": {"key": "due"}}),
        ),
        ("a type", json!({"answers_type": "ticket"})),
    ] {
        asked(&jojobot, shape.clone()).await;
        above.reset();
        let answer = asked(&jojobot, shape).await;
        let shown = listed(&answer).len();
        let calls = above.calls();
        assert!(
            shown >= 20,
            "{name}: only {shown} shown, so the bound is loose"
        );
        // Five reads hydrate an object. A page and the fixed cost of a read, and
        // nothing for the candidates behind it.
        assert!(
            calls <= 8 * shown + 80,
            "{name}: {calls} port calls for {shown} objects shown: {:?}",
            above.seen()
        );
    }
}

/// **A leaf the store answers is still licensed by a declaration.** The shells a
/// decided answer is cut from are asked none of the filters, so the licence is
/// checked before they are, and a `contains` of a key no type calls a list, or an
/// ordering of a key no type calls a date or a number, is refused as it is when
/// every thing is read.
#[tokio::test]
async fn a_leaf_the_store_answers_is_refused_when_no_declaration_licenses_it() {
    let store = tracked(40).await;
    let reference = reference(store.clone());
    for (name, routed) in [
        ("over the store", handler_indexed_over(store.clone())),
        ("over the fold", folded_over(store.clone()).await),
    ] {
        for (leaf, shape) in [
            (
                "a contains of a key that is no list",
                "assignee contains bea",
            ),
            ("an ordering of a key that is text", "status > now"),
            ("an ordering of a key nobody declared", "unknown_key > 3"),
        ] {
            let want = asked(&reference, json!({"where": shape})).await;
            let got = asked(&routed, json!({"where": shape})).await;
            assert_eq!(
                want["status"], "blocked",
                "{leaf}: the read of everyone: {want}"
            );
            assert_eq!(got["status"], "blocked", "{name}, {leaf}: {got}");
            assert_eq!(got["wrote"], false, "{name}, {leaf}: {got}");
        }
    }
}

/// **The two calls reach the fold through every layer the binary puts above it.**
/// The store, the fold, a counter, the mention layer and the index, in that
/// order: the counter sees one call and nothing the default bodies would make
/// (a listing, a read of fields for every thing).
#[tokio::test]
async fn the_layers_above_the_fold_forward_the_two_calls() {
    use jojobot_domain::memory::filter::{Direction, Test};
    let store = tracked(40).await;
    let below: Arc<dyn Memory> = store.clone();
    let folded = Arc::new(Folded::new(below));
    folded.rebuild().await.expect("the fold loads");
    let above = Arc::new(Counted::new(folded));
    let mentioning = Arc::new(Mentioning::new(above.clone()));
    let indexed = IndexedMemory::new(mentioning).expect("the index opens");

    above.reset();
    let kitchen = indexed
        .keys_holding("tags", &Test::Contains("kitchen".into()), None)
        .await
        .expect("keys_holding answers");
    let by_reading = jojobot_domain::memory::holders::holding_by_reading(
        store.as_ref(),
        "tags",
        &Test::Contains("kitchen".into()),
        None,
    )
    .await
    .expect("the full read answers");
    assert!(
        !kitchen.is_empty() && kitchen.len() < 8,
        "some of the 8 that hold the item are archived: {kitchen:?}"
    );
    assert_eq!(
        kitchen, by_reading,
        "the fold answers as the full read does"
    );
    assert_eq!(
        above.seen(),
        [("keys_holding", 1)].into(),
        "{:?}",
        above.seen()
    );

    above.reset();
    let head = indexed
        .ordered_first_k(
            "due",
            ValueType::Date,
            Direction::Ascending,
            Some(EntityKind::THING),
            None,
            5,
        )
        .await
        .expect("ordered_first_k answers");
    assert!(head.head.len() >= 5 && head.cut);
    assert_eq!(
        above.seen(),
        [("ordered_first_k", 1)].into(),
        "{:?}",
        above.seen()
    );
}

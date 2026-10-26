use super::*;
use jiff::civil::date;

async fn thing(store: &InMemoryMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    store
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

async fn say(store: &InMemoryMemory, about: &EntityId, what: &str) -> FactAddress {
    store
        .capture(NewFact::about(about.clone(), what, date(2026, 6, 2)))
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address()
}

/// **Two merges under one request id leave one event.** The first spends the
/// id and lands; the second finds it spent and is refused, and the store holds
/// nothing of it: the event is recorded after the id is spent, as the real
/// store records it in the transaction that spends it. **Paired**: the first
/// merge landed, and without an id a second merge into the same survivor
/// lands too, so the refusal is about the id.
#[tokio::test]
async fn two_merges_under_one_request_id_leave_one_event() {
    use crate::request::{Request, RequestId, Scope, armed, digest_of, within};
    let ledger = crate::request::testing::InMemoryRequests::new();
    let store = InMemoryMemory::booted().with_requests(ledger.clone());
    let survivor = thing(&store, "person:lisa", "Lisa").await;
    let first = thing(&store, "person:nelson", "Nelson").await;
    let second = thing(&store, "person:milhouse", "Milhouse").await;
    let third = thing(&store, "person:ralph", "Ralph").await;
    let caller = EntityId("bot:sigma".into());
    let under_the_id = |folded: EntityId| {
        let store = &store;
        let survivor = survivor.clone();
        let caller = caller.clone();
        async move {
            let scope = Scope::new(Request {
                bot: EntityId("bot:sigma".into()),
                id: RequestId::parse("merge-1").expect("a good id"),
                verb: "merge_entities".into(),
                digest: digest_of("{}"),
            });
            within(
                scope,
                armed(store.merge(&folded, &survivor, None, date(2026, 6, 3), &caller, None)),
            )
            .await
        }
    };

    under_the_id(first.clone())
        .await
        .expect("the first merge lands");
    let refused = under_the_id(second.clone())
        .await
        .expect_err("the id is spent, so the second merge is refused");
    assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");

    let events = store
        .merge_events(Some(&survivor))
        .await
        .expect("the events read");
    assert_eq!(events.len(), 1, "one event: {events:?}");
    assert_eq!(events[0].folded, first);
    let rows = store.list_entities(None).await.expect("lists");
    assert!(
        rows.iter()
            .any(|e| e.id == second && e.merged_into.is_none()),
        "the refused merge folded nothing"
    );
    // The positive: with no id, another merge into the survivor lands.
    store
        .merge(&third, &survivor, None, date(2026, 6, 3), &caller, None)
        .await
        .expect("a merge with no id lands");
    assert_eq!(
        store
            .merge_events(Some(&survivor))
            .await
            .expect("the events read")
            .len(),
        2
    );
}

/// A document of one section: the entity and a record that holds the section
/// under an order, as a put writes it.
async fn document_of_one_section(
    store: &InMemoryMemory,
    handle: &str,
    name: &str,
    text: &str,
) -> EntityId {
    let id = thing(store, handle, name).await;
    let sections = crate::memory::document::split(text).expect("the text splits");
    let orders = crate::memory::order::spread(None, None, sections.len()).expect("room");
    for (section, order) in sections.iter().zip(orders) {
        store
            .capture(NewFact {
                details: (!section.body.is_empty()).then(|| section.body.clone()),
                fields: crate::memory::document::record_fields(section, &order),
                ..NewFact::about(id.clone(), section.first_line.clone(), date(2026, 6, 2))
            })
            .await
            .expect("capture")
            .written()
            .expect("not blocked");
    }
    id
}

/// The order the section with this first line holds, as the thing serves it.
async fn order_of(store: &InMemoryMemory, doc: &EntityId, first_line: &str) -> String {
    store
        .recall(doc)
        .await
        .expect("recall")
        .into_iter()
        .find(|fact| fact.content == first_line)
        .and_then(|fact| fact.fields.get(crate::memory::document::ORDER).cloned())
        .unwrap_or_else(|| panic!("{doc} serves no section {first_line}"))
}

/// **A document merge that finds its request id spent writes nothing**, not
/// even the new order of the sections it was to move: the id is spent before
/// the first section is written, as the real store spends it in the
/// transaction that writes them. **Paired**: the same merge with no id lands
/// and does move the section behind the survivor's, so the order that stayed
/// put under the spent id is the id's doing.
#[tokio::test]
async fn a_document_merge_under_a_spent_request_id_moves_no_section() {
    use crate::request::{Request, RequestId, Scope, armed, digest_of, within};
    let ledger = crate::request::testing::InMemoryRequests::new();
    let store = InMemoryMemory::booted().with_requests(ledger.clone());
    let survivor = document_of_one_section(
        &store,
        "document:alpha",
        "Fryer Manual",
        "# Alpha head\n\nbody\n",
    )
    .await;
    let folded = document_of_one_section(
        &store,
        "document:beta",
        "Oven Guide",
        "# Beta head\n\nbody\n",
    )
    .await;
    let other_survivor = document_of_one_section(
        &store,
        "document:homer",
        "Cellar Log",
        "# Homer head\n\nbody\n",
    )
    .await;
    let other_folded = document_of_one_section(
        &store,
        "document:lisa",
        "Attic Notes",
        "# Lisa head\n\nbody\n",
    )
    .await;
    let caller = EntityId("bot:sigma".into());
    let under_the_id = |folded: EntityId, survivor: EntityId| {
        let store = &store;
        let caller = caller.clone();
        async move {
            let scope = Scope::new(Request {
                bot: EntityId("bot:sigma".into()),
                id: RequestId::parse("merge-1").expect("a good id"),
                verb: "merge_entities".into(),
                digest: digest_of("{}"),
            });
            within(
                scope,
                armed(store.merge(&folded, &survivor, None, date(2026, 6, 3), &caller, None)),
            )
            .await
        }
    };

    under_the_id(folded.clone(), survivor.clone())
        .await
        .expect("the first merge lands");
    let before = order_of(&store, &other_folded, "# Lisa head").await;
    let refused = under_the_id(other_folded.clone(), other_survivor.clone())
        .await
        .expect_err("the id is spent, so the second merge is refused");
    assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");
    assert_eq!(
        order_of(&store, &other_folded, "# Lisa head").await,
        before,
        "the refused merge moved a section"
    );

    store
        .merge(
            &other_folded,
            &other_survivor,
            None,
            date(2026, 6, 3),
            &caller,
            None,
        )
        .await
        .expect("a merge with no id lands");
    assert_ne!(
        order_of(&store, &other_survivor, "# Lisa head").await,
        before,
        "a landed merge places the folded section behind the survivor's"
    );
}

/// **A claim stood up under the folded key after the merge is served to
/// nobody**, as the real store serves it. Paired inside the contract case
/// with the claim the event did map.
#[tokio::test]
async fn a_stray_claim_under_a_folded_key_is_served_to_nobody() {
    let store = InMemoryMemory::booted();
    let survivor = thing(&store, "person:lisa", "Lisa").await;
    let folded = thing(&store, "person:nelson", "Nelson").await;
    say(&store, &folded, "owes a dollar").await;
    store
        .merge(
            &folded,
            &survivor,
            Some("one boy, filed twice"),
            date(2026, 6, 3),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("the fold lands");
    store.stray_claim_under(&folded, "a stray claim");
    contract::a_claim_under_a_folded_key_that_no_event_lists_is_served_to_nobody(
        &store,
        &survivor,
        &folded,
        "a stray claim",
        "owes a dollar",
    )
    .await;
}

/// **Two events for one address: the newest decides, in the rows served as
/// well as in the address a caller is told.** The walk takes the newest, so
/// the world built from the same events must agree with it rather than apply
/// the first.
#[tokio::test]
async fn the_rows_served_follow_the_newest_event_for_an_address() {
    let store = InMemoryMemory::booted();
    let first = thing(&store, "person:lisa", "Lisa").await;
    let folded = thing(&store, "person:nelson", "Nelson").await;
    let second = thing(&store, "person:skinner", "Skinner").await;
    let made = say(&store, &folded, "owes a dollar").await;
    store
        .merge(
            &folded,
            &first,
            Some("one boy, filed twice"),
            date(2026, 6, 3),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("the fold lands");
    // The state a hand-edited store holds: a later event moves the same
    // address somewhere else.
    let (folded_key, _) = store.resolve(&folded).expect("the folded thing");
    let (second_key, _) = store.resolve(&second).expect("the other survivor");
    store
        .merges
        .lock()
        .expect("fake mutex poisoned")
        .push(StoredMerge {
            folded: folded_key,
            survivor: second_key,
            merged_at: jiff::Timestamp::now(),
            session: None,
            claims: vec![(made.local.clone(), FactId("f77".into()))],
        });

    assert_eq!(
        store.moved(&made),
        Some(FactAddress::new(second.clone(), FactId("f77".into()))),
        "the walk takes the newest event"
    );
    let served = store.recall(&second).await.expect("recall reads");
    assert!(
        served
            .iter()
            .any(|fact| fact.id.as_str() == "f77" && fact.content == "owes a dollar"),
        "the rows are served where the walk says: {served:?}"
    );
    let left = store.recall(&first).await.expect("recall reads");
    assert!(
        left.iter().all(|fact| fact.content != "owes a dollar"),
        "and not where the older event put them: {left:?}"
    );
}

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

async fn refused_claimed_edit(drop_thought: bool) {
    use crate::memory::{EdgeShape, THOUGHT_CAPACITY};
    use crate::request::{Request, RequestId, Scope, armed, digest_of, within};
    let ledger = crate::request::testing::InMemoryRequests::new();
    let store = InMemoryMemory::booted().with_requests(ledger);
    let handle = if drop_thought {
        "bot:gamma"
    } else {
        "person:lisa"
    };
    let subject = thing(&store, handle, if drop_thought { "Gamma" } else { "Lisa" }).await;
    let target = thing(&store, "person:ralph", "Ralph").await;
    if drop_thought {
        store
            .capture(NewFact {
                fields: [(THOUGHT_CAPACITY.into(), "1".into())].into(),
                sets: [THOUGHT_CAPACITY.into()].into(),
                ..NewFact::about(subject.clone(), "one thought at a time", date(2026, 6, 2))
            })
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    let claim = |colour: &str| NewFact {
        fields: [("colour".into(), colour.into())].into(),
        sets: ["colour".into()].into(),
        edge: drop_thought.then(|| Edge::new(EdgeShape::Connection, target.clone())),
        ..NewFact::about(subject.clone(), "set with the edit", date(2026, 6, 2))
    };
    let patch = |name: &str| EntityPatch {
        name: Some(name.into()),
        ..Default::default()
    };
    let under_the_id = |patch: EntityPatch, claim: NewFact| {
        let store = &store;
        let subject = &subject;
        async move {
            let scope = Scope::new(Request {
                bot: EntityId("bot:sigma".into()),
                id: RequestId::parse("claimed-edit-1").unwrap(),
                verb: "update_entity".into(),
                digest: digest_of("{}"),
            });
            within(
                scope,
                armed(store.update_entity_with_claim(subject, patch, claim)),
            )
            .await
        }
    };
    let (edited, first) = under_the_id(patch("First edit"), claim("red"))
        .await
        .unwrap()
        .written()
        .unwrap();
    assert_eq!(edited.name, "First edit");
    assert_eq!(store.fields(&subject).await.unwrap()["colour"], "red");
    let entities = store.list_entities(None).await.unwrap();
    let facts = store.recall(&subject).await.unwrap();
    let fields = store.fields(&subject).await.unwrap();
    let history = store.history(&subject, "colour").await.unwrap();
    let entity_history = store.entity_history(&subject).await.unwrap();
    let claim_history = store.claim_history(&first.address()).await.unwrap();
    let mut second = claim("blue");
    if drop_thought {
        second.drop = Some(first.address());
        second.drop_because = Some("replace the old thought".into());
    }
    let refused = under_the_id(patch("Refused edit"), second)
        .await
        .unwrap_err();
    assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");
    assert_eq!(
        store.recall(&subject).await.unwrap(),
        facts,
        "no claim or drop survives"
    );
    assert_eq!(store.fields(&subject).await.unwrap(), fields);
    assert_eq!(store.history(&subject, "colour").await.unwrap(), history);
    assert_eq!(store.list_entities(None).await.unwrap(), entities);
    assert_eq!(
        store.entity_history(&subject).await.unwrap(),
        entity_history
    );
    assert_eq!(
        store.claim_history(&first.address()).await.unwrap(),
        claim_history
    );
    let mut fresh = claim("green");
    if drop_thought {
        fresh.drop = Some(first.address());
        fresh.drop_because = Some("replace the old thought".into());
    }
    let (edited, _) = store
        .update_entity_with_claim(&subject, patch("Final edit"), fresh)
        .await
        .unwrap()
        .written()
        .unwrap();
    assert_eq!(edited.name, "Final edit");
    assert_eq!(store.fields(&subject).await.unwrap()["colour"], "green");
}

/// **A role with a claim and a renewal that fills its heartbeat home**, as
/// `(bot, role, claim moment, renewal moment)`. The claimant is `delta`; the
/// renewal is five minutes after the claim.
async fn a_renewed_role(
    store: &InMemoryMemory,
) -> (EntityId, EntityId, jiff::Timestamp, jiff::Timestamp) {
    use crate::memory::testing::contract::LeaseHeartbeat;
    use crate::session::{ROLE_CLAIMED_AT, ROLE_HOLDER, RoleMove, RoleMoveKind};
    let bot = thing(store, "bot:contract-role-lease-composite", "Composite").await;
    let role = EntityId("role:contract-lease-composite".to_string());
    store
        .add_entity(NewEntity {
            parent: Some(bot.clone()),
            ..NewEntity::new(role.clone(), "contract-lease-composite", "fixture")
        })
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let held = |who: &str, at: jiff::Timestamp| -> std::collections::BTreeMap<String, String> {
        [
            (ROLE_HOLDER.to_string(), who.to_string()),
            (ROLE_CLAIMED_AT.to_string(), at.to_string()),
        ]
        .into()
    };
    let fields = held("delta", t0);
    let claimed = store
        .capture(NewFact {
            derived: fields.keys().cloned().collect(),
            fields,
            ..NewFact::about(role.clone(), "delta claims the role", date(2026, 7, 1))
        })
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    let renewed_at = t0 + jiff::SignedDuration::from_mins(5);
    let fields = held("delta", renewed_at);
    store
        .update_fact(
            &claimed.address(),
            FactPatch {
                derived: fields.keys().cloned().collect(),
                fields,
                role_move: Some(RoleMove {
                    kind: RoleMoveKind::Renew,
                    role: "contract-lease-composite".to_string(),
                    claimant: "delta".to_string(),
                }),
                ..FactPatch::default()
            },
            &bot,
        )
        .await
        .expect("the renewal lands")
        .written()
        .expect("not blocked");
    assert_eq!(
        store.heartbeat_of(&role).await,
        Some(renewed_at.to_string())
    );
    (bot, role, t0, renewed_at)
}

/// The takeover of a lapsed lease by `echo`, as a claim on the role.
fn a_takeover(role: &EntityId, after: jiff::Timestamp) -> NewFact {
    use crate::session::{ROLE_CLAIMED_AT, ROLE_HOLDER};
    let fields: std::collections::BTreeMap<String, String> = [
        (ROLE_HOLDER.to_string(), "echo".to_string()),
        (
            ROLE_CLAIMED_AT.to_string(),
            (after + crate::session::LEASE_FRESHNESS + jiff::SignedDuration::from_mins(1))
                .to_string(),
        ),
    ]
    .into();
    NewFact {
        derived: fields.keys().cloned().collect(),
        fields,
        ..NewFact::about(role.clone(), "echo takes the role", date(2026, 7, 1))
    }
}

/// `act` under the request id every call of this helper shares, so the second
/// call finds the id spent.
async fn under_a_spent_id<T>(act: impl std::future::Future<Output = T>) -> T {
    use crate::request::{Request, RequestId, Scope, armed, digest_of, within};
    let scope = Scope::new(Request {
        bot: EntityId("bot:sigma".into()),
        id: RequestId::parse("lease-composite-1").unwrap(),
        verb: "update_entity".into(),
        digest: digest_of("{}"),
    });
    within(scope, armed(act)).await
}

/// **An edit and a claim whose request id was already spent leave the heartbeat
/// home as it was.** The claim is a takeover of a lapsed lease: the capture
/// empties the home, and the spend that follows is refused, so the act is taken
/// back as the real store's one transaction is.
#[tokio::test]
async fn a_claimed_edit_refused_at_the_spend_leaves_the_heartbeat_home_as_it_was() {
    use crate::memory::testing::contract::LeaseHeartbeat;
    let store =
        InMemoryMemory::booted().with_requests(crate::request::testing::InMemoryRequests::new());
    let (_, role, _, renewed_at) = a_renewed_role(&store).await;
    let patch = |name: &str| EntityPatch {
        name: Some(name.into()),
        ..Default::default()
    };
    let note = NewFact::about(role.clone(), "a note beside the edit", date(2026, 7, 1));
    under_a_spent_id(store.update_entity_with_claim(&role, patch("First"), note))
        .await
        .expect("the first act lands")
        .written()
        .expect("not blocked");
    let folded = store.fields(&role).await.unwrap();
    let refused = under_a_spent_id(store.update_entity_with_claim(
        &role,
        patch("Second"),
        a_takeover(&role, renewed_at),
    ))
    .await
    .unwrap_err();
    assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");
    assert_eq!(store.fields(&role).await.unwrap(), folded);
    assert_eq!(
        store.heartbeat_of(&role).await,
        Some(renewed_at.to_string()),
        "a refused act leaves the home as it was"
    );
}

/// Spend the request id on a plain note about `role`, so the next act under
/// [`under_a_spent_id`] is refused at its spend.
async fn spend_the_id_on_a_note(store: &InMemoryMemory, role: &EntityId) {
    under_a_spent_id(store.update_entity_with_claim(
        role,
        EntityPatch {
            name: Some("Spent".into()),
            ..Default::default()
        },
        NewFact::about(role.clone(), "a note that spends the id", date(2026, 7, 1)),
    ))
    .await
    .expect("the first act lands")
    .written()
    .expect("not blocked");
}

/// **A retraction and a claim whose request id was already spent leave the
/// heartbeat home as it was**, as the edit does.
#[tokio::test]
async fn a_claimed_retraction_refused_at_the_spend_leaves_the_heartbeat_home_as_it_was() {
    use crate::memory::testing::contract::LeaseHeartbeat;
    let store =
        InMemoryMemory::booted().with_requests(crate::request::testing::InMemoryRequests::new());
    let (bot, role, _, renewed_at) = a_renewed_role(&store).await;
    let note = store
        .capture(NewFact::about(
            role.clone(),
            "a note to retract",
            date(2026, 7, 1),
        ))
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    spend_the_id_on_a_note(&store, &role).await;
    let folded = store.fields(&role).await.unwrap();
    let refused = under_a_spent_id(store.retract_with_claim(
        &note.address(),
        Some("taken back"),
        date(2026, 7, 2),
        &bot,
        None,
        a_takeover(&role, renewed_at),
    ))
    .await
    .unwrap_err();
    assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");
    assert_eq!(store.fields(&role).await.unwrap(), folded);
    assert_eq!(
        store.heartbeat_of(&role).await,
        Some(renewed_at.to_string()),
        "a refused act leaves the home as it was"
    );
}

/// **A merge and a claim on the survivor whose request id was already spent
/// leave the survivor's heartbeat home as it was.**
#[tokio::test]
async fn a_claimed_merge_refused_at_the_spend_leaves_the_heartbeat_home_as_it_was() {
    use crate::memory::testing::contract::LeaseHeartbeat;
    let store =
        InMemoryMemory::booted().with_requests(crate::request::testing::InMemoryRequests::new());
    let (bot, role, _, renewed_at) = a_renewed_role(&store).await;
    let other = EntityId("role:theta".to_string());
    store
        .add_entity(NewEntity {
            parent: Some(bot.clone()),
            ..NewEntity::new(other.clone(), "Theta", "fixture")
        })
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    spend_the_id_on_a_note(&store, &role).await;
    let folded = store.fields(&role).await.unwrap();
    let refused = under_a_spent_id(store.merge_with_claim(
        &other,
        &role,
        Some("one role"),
        date(2026, 7, 2),
        &bot,
        a_takeover(&role, renewed_at),
    ))
    .await
    .unwrap_err();
    assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");
    assert_eq!(store.fields(&role).await.unwrap(), folded);
    assert_eq!(
        store.heartbeat_of(&role).await,
        Some(renewed_at.to_string()),
        "a refused act leaves the home as it was"
    );
}

#[tokio::test]
async fn claimed_edit_rollback_leaves_no_claim_after_a_spent_request() {
    refused_claimed_edit(false).await;
}

#[tokio::test]
async fn claimed_edit_rollback_restores_a_dropped_thought() {
    refused_claimed_edit(true).await;
}

/// What a store shows of its own substrates: the write summary, which counts
/// every write of an entity and a claim and hashes the rows, and the entities
/// listed. A creation that was refused must leave both as they were.
async fn shown(store: &InMemoryMemory) -> (Option<crate::memory::WriteSummary>, Vec<Entity>) {
    (
        store.write_summary().await.unwrap(),
        store.list_entities(None).await.unwrap(),
    )
}

fn first_claim_about(subject: &EntityId, what: &str) -> NewFact {
    NewFact::about(subject.clone(), what, date(2026, 6, 2))
}

/// **A creation refused at its first claim leaves nothing.** The first claim is
/// refused for a line break in its text, after the entity was made; the refusal
/// is the claim's own, and the store shows what it showed before the call, the
/// entity absent. **Paired**: the same creation with a clean first claim lands
/// and its claim is readable, so the refusal above is about the claim.
#[tokio::test]
async fn a_creation_refused_at_its_first_claim_leaves_the_store_as_it_was() {
    let store = InMemoryMemory::booted();
    let before = shown(&store).await;
    let homer = EntityId("person:milhouse".to_string());
    let refused = store
        .add_entity_with_first_claim(
            NewEntity::new(homer.clone(), "Milhouse", "fixture"),
            first_claim_about(&homer, "wears glasses\nand a scarf"),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(refused, MemoryError::InvalidFact(_)),
        "{refused:?}"
    );
    assert_eq!(shown(&store).await, before, "nothing of the creation stays");
    let (entity, fact) = store
        .add_entity_with_first_claim(
            NewEntity::new(homer.clone(), "Milhouse", "fixture"),
            first_claim_about(&homer, "wears glasses"),
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    assert_eq!(entity.id, homer);
    assert_eq!(store.recall(&homer).await.unwrap(), vec![fact]);
}

/// **A creation whose companion record is refused after the first claim landed
/// leaves nothing**: not the entity, and not the claim that was written before
/// the refusal. The companion is refused for a line break; the first claim is
/// clean. **Paired**: with a clean companion both records are readable.
#[tokio::test]
async fn a_creation_refused_at_a_companion_record_takes_the_first_claim_back_too() {
    let store = InMemoryMemory::booted();
    let before = shown(&store).await;
    let homer = EntityId("person:milhouse".to_string());
    let with_companion = |companion: &str| NewFact {
        also_written: vec![first_claim_about(&homer, companion)],
        ..first_claim_about(&homer, "wears glasses")
    };
    let refused = store
        .add_entity_with_first_claim(
            NewEntity::new(homer.clone(), "Milhouse", "fixture"),
            with_companion("noted\nover two lines"),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(refused, MemoryError::InvalidFact(_)),
        "{refused:?}"
    );
    assert_eq!(shown(&store).await, before, "no write of the act stays");
    store
        .add_entity_with_first_claim(
            NewEntity::new(homer.clone(), "Milhouse", "fixture"),
            with_companion("noted in one line"),
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    assert_eq!(store.recall(&homer).await.unwrap().len(), 2);
}

/// **A creation whose request id was spent by another call leaves nothing.** The
/// id is spent by a first creation; the second, under the same id, is refused at
/// its final spend, after its entity and claim were written, and the store shows
/// what it showed after the first.
#[tokio::test]
async fn a_creation_refused_at_the_spend_of_its_request_leaves_the_store_as_it_was() {
    use crate::request::{Request, RequestId, Scope, armed, digest_of, within};
    let store =
        InMemoryMemory::booted().with_requests(crate::request::testing::InMemoryRequests::new());
    let creation = |slug: &'static str| {
        let store = &store;
        let id = EntityId(format!("person:{slug}"));
        async move {
            let scope = Scope::new(Request {
                bot: EntityId("bot:sigma".into()),
                id: RequestId::parse("creation-1").unwrap(),
                verb: "add_entity".into(),
                digest: digest_of("{}"),
            });
            within(
                scope,
                armed(store.add_entity_with_first_claim(
                    NewEntity::new(id.clone(), slug, "fixture"),
                    first_claim_about(&id, "was here"),
                )),
            )
            .await
        }
    };
    creation("milhouse").await.unwrap().written().unwrap();
    let after_the_first = shown(&store).await;
    let refused = creation("nelson").await.unwrap_err();
    assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");
    assert_eq!(shown(&store).await, after_the_first);
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

/// **A wrapper that forwards only what the port requires refuses a retraction
/// with a claim, and writes nothing.** The port's default is the refusal, so a
/// layer that forgot to forward the method fails loudly and never retracts and
/// then captures as two writes.
#[tokio::test]
async fn a_wrapper_that_does_not_forward_the_retraction_with_a_claim_refuses_it() {
    let store = std::sync::Arc::new(InMemoryMemory::booted());
    let subject = thing(&store, "person:lisa", "Lisa").await;
    let address = say(&store, &subject, "said in error").await;
    let wrapper = crate::memory::testing::WithoutBuiltOn(store.clone());
    let refused = wrapper
        .retract_with_claim(
            &address,
            None,
            date(2026, 6, 3),
            &subject,
            None,
            NewFact::about(subject.clone(), "said again", date(2026, 6, 3)),
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "the default refuses: {refused:?}"
    );
    let held = store.recall(&subject).await.expect("recall reads");
    assert!(
        held.iter()
            .all(|fact| fact.status == FactStatus::Active && fact.content != "said again"),
        "nothing was retracted and nothing was written: {held:?}"
    );
}

/// **The same for an edit with a claim**: the default refuses and nothing lands.
#[tokio::test]
async fn a_wrapper_that_does_not_forward_the_edit_with_a_claim_refuses_it() {
    let store = std::sync::Arc::new(InMemoryMemory::booted());
    let subject = thing(&store, "person:lisa", "Lisa").await;
    let address = say(&store, &subject, "said in error").await;
    let wrapper = crate::memory::testing::WithoutBuiltOn(store.clone());
    let refused = wrapper
        .update_fact_with_claim(
            &address,
            FactPatch {
                status: Some(FactStatus::Archived),
                ..Default::default()
            },
            &subject,
            NewFact::about(subject.clone(), "said again", date(2026, 6, 3)),
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "the default refuses: {refused:?}"
    );
    let held = store.recall(&subject).await.expect("recall reads");
    assert!(
        held.iter()
            .all(|fact| fact.status == FactStatus::Active && fact.content != "said again"),
        "nothing was archived and nothing was written: {held:?}"
    );
}

/// **The same for a merge with a claim**: the default refuses and nothing lands.
#[tokio::test]
async fn a_wrapper_that_does_not_forward_the_merge_with_a_claim_refuses_it() {
    let store = std::sync::Arc::new(InMemoryMemory::booted());
    let survivor = thing(&store, "person:lisa", "Lisa").await;
    let duplicate = thing(&store, "person:maggie", "Maggie").await;
    say(&store, &duplicate, "a note on the duplicate").await;
    let wrapper = crate::memory::testing::WithoutBuiltOn(store.clone());
    let refused = wrapper
        .merge_with_claim(
            &duplicate,
            &survivor,
            None,
            date(2026, 6, 3),
            &survivor,
            NewFact::about(survivor.clone(), "said again", date(2026, 6, 3)),
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "the default refuses: {refused:?}"
    );
    let held = store.recall(&survivor).await.expect("recall reads");
    assert!(
        held.iter().all(|fact| fact.content != "said again"),
        "nothing was merged and nothing was written: {held:?}"
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("lists")
            .iter()
            .all(|entity| entity.merged_into.is_none()),
        "no thing was merged away"
    );
}

/// **The seam that deletes a thing under the store leaves nothing keyed to it**:
/// not its row, claims or writes, and not its entity writes, versions, prose,
/// badge, merges or former handles. A thing nobody deleted keeps its own.
#[tokio::test]
async fn deleting_a_thing_under_the_store_clears_every_table_keyed_to_it() {
    let store = InMemoryMemory::booted();
    let doomed = thing(&store, "person:nelson", "Nelson").await;
    let kept = thing(&store, "person:milhouse", "Milhouse").await;
    let folded = thing(&store, "person:ralph", "Ralph").await;
    let spare = thing(&store, "person:lisa", "Lisa").await;
    let caller = EntityId("bot:sigma".into());
    // **Renamed first**, so the prose, the merge and the former handle all name
    // the handle the seam is asked to delete.
    let renamed = EntityId("person:delta".into());
    store
        .rename_entity(
            &doomed,
            &renamed,
            None,
            date(2026, 6, 4),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename is answered")
        .written()
        .expect("nothing collides with the new handle");
    for (held, colour) in [(&renamed, "blue"), (&kept, "green")] {
        store
            .capture(NewFact {
                fields: [("colour".into(), colour.into())].into(),
                sets: ["colour".into()].into(),
                ..NewFact::about(held.clone(), "laughs", date(2026, 6, 2))
            })
            .await
            .expect("the claim with a field lands")
            .written()
            .expect("the claim is not blocked");
        store
            .set_prose(held, "a page", None)
            .await
            .expect("prose lands");
    }
    for (from, to) in [(&folded, &renamed), (&renamed, &kept), (&spare, &kept)] {
        store
            .merge(from, to, None, date(2026, 6, 3), &caller, None)
            .await
            .expect("the merge lands");
    }
    let badge = store
        .entities
        .lock()
        .unwrap()
        .iter()
        .find(|e| e.id == renamed)
        .and_then(|e| e.badge.clone())
        .expect("the renamed thing wears a badge");
    let home = EntityId(badge.clone());
    let spare_home = EntityId(store.badges.lock().unwrap()[&spare].clone());
    let kept_home = EntityId(
        store
            .entities
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == kept)
            .and_then(|e| e.badge.clone())
            .expect("the kept thing wears a badge"),
    );
    let entities_before = store.entities.lock().unwrap().clone();
    let facts_before = store.facts.lock().unwrap().clone();
    assert_ne!(
        home, kept_home,
        "the kept control needs another storage key"
    );
    let field_rows = || {
        store
            .writes
            .lock()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    w.entity.clone(),
                    w.key.clone(),
                    w.ordinal,
                    w.value.clone(),
                    w.fact.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    let writes_before = field_rows();
    let claim_writes_before = store.claim_writes.lock().unwrap().clone();
    // **Before**, so each table below is shown to hold something to clear.
    for handle in [&renamed, &kept] {
        assert!(entities_before.iter().any(|e| &e.id == handle));
    }
    for held in [&home, &kept_home] {
        assert!(facts_before.iter().any(|f| &f.home == held));
        assert!(writes_before.iter().any(|(key, ..)| key == held));
        assert!(claim_writes_before.iter().any(|(key, ..)| key == held));
    }
    assert!(
        store
            .entity_writes
            .lock()
            .unwrap()
            .iter()
            .any(|(k, _)| k == &home)
    );
    assert!(
        store
            .entity_versions
            .lock()
            .unwrap()
            .iter()
            .any(|(k, _)| k == &home)
    );
    assert!(store.prose.lock().unwrap().contains_key(&renamed));
    assert!(
        store
            .badges
            .lock()
            .unwrap()
            .values()
            .any(|held| held == &badge)
    );
    assert!(
        store
            .merges
            .lock()
            .unwrap()
            .iter()
            .any(|m| m.survivor == home)
    );
    assert!(
        store
            .merges
            .lock()
            .unwrap()
            .iter()
            .any(|m| m.folded == home)
    );
    assert!(
        store
            .former_handles
            .lock()
            .unwrap()
            .iter()
            .any(|f| f.badge == badge)
    );

    store.delete_entity_past_the_guard(&renamed);

    assert_eq!(
        *store.entities.lock().unwrap(),
        entities_before
            .into_iter()
            .filter(|e| e.id != renamed)
            .collect::<Vec<_>>(),
        "only the deleted entity row must go"
    );
    assert_eq!(
        *store.facts.lock().unwrap(),
        facts_before
            .into_iter()
            .filter(|f| f.home != home)
            .collect::<Vec<_>>(),
        "only claims in the deleted home must go"
    );
    assert_eq!(
        field_rows(),
        writes_before
            .into_iter()
            .filter(|(held, ..)| held != &home)
            .collect::<Vec<_>>(),
        "only key writes in the deleted home must go"
    );
    assert_eq!(
        *store.claim_writes.lock().unwrap(),
        claim_writes_before
            .into_iter()
            .filter(|(held, ..)| held != &home)
            .collect::<Vec<_>>(),
        "only claim writes in the deleted home must go"
    );
    assert!(
        store
            .entity_writes
            .lock()
            .unwrap()
            .iter()
            .all(|(k, _)| k != &home),
        "entity writes of the deleted thing remain"
    );
    assert!(
        store
            .entity_versions
            .lock()
            .unwrap()
            .iter()
            .all(|(k, _)| k != &home),
        "entity versions of the deleted thing remain"
    );
    assert!(
        !store.prose.lock().unwrap().contains_key(&renamed),
        "prose of the deleted thing remains"
    );
    assert!(
        !store
            .badges
            .lock()
            .unwrap()
            .values()
            .any(|held| held == &badge),
        "the badge of the deleted thing remains"
    );
    assert!(
        store
            .merges
            .lock()
            .unwrap()
            .iter()
            .all(|m| m.survivor != home && m.folded != home),
        "a merge naming the deleted thing remains"
    );
    {
        let kept_merges = store.merges.lock().unwrap();
        assert_eq!(kept_merges.len(), 1, "the unrelated merge stays");
        assert_eq!(kept_merges[0].folded, spare_home);
        assert_eq!(kept_merges[0].survivor, kept_home);
    }
    assert!(
        store
            .former_handles
            .lock()
            .unwrap()
            .iter()
            .all(|f| f.badge != badge),
        "a former handle of the deleted thing remains"
    );
    // **The thing nobody deleted keeps its own**, so the clearing is not a wipe.
    assert!(
        store
            .entity_writes
            .lock()
            .unwrap()
            .iter()
            .any(|(k, _)| k == &kept_home)
    );
    assert!(store.prose.lock().unwrap().contains_key(&kept));
    assert!(store.badges.lock().unwrap().contains_key(&kept));
}

/// **A thing renamed after its prose was written is deleted with that prose.**
/// The fake keys prose by the handle it was written under and a rename does not
/// re-key it, so the seam finds the prose through the thing's badge, not through
/// the handle it is asked to delete. A thing nobody deleted keeps its own.
#[tokio::test]
async fn deleting_a_renamed_thing_under_the_store_clears_the_prose_written_before_the_rename() {
    let store = InMemoryMemory::booted();
    let doomed = thing(&store, "person:nelson", "Nelson").await;
    let kept = thing(&store, "person:milhouse", "Milhouse").await;
    for held in [&doomed, &kept] {
        store
            .set_prose(held, "a page", None)
            .await
            .expect("prose lands");
    }
    let renamed = EntityId("person:delta".into());
    store
        .rename_entity(
            &doomed,
            &renamed,
            None,
            date(2026, 6, 4),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename is answered")
        .written()
        .expect("nothing collides with the new handle");
    assert!(
        store.prose.lock().unwrap().contains_key(&doomed),
        "the page still sits under the handle it was written under"
    );

    store.delete_entity_past_the_guard(&renamed);

    let held = store.prose.lock().unwrap();
    assert!(
        !held.contains_key(&doomed) && !held.contains_key(&renamed),
        "prose of the deleted thing remains: {held:?}"
    );
    assert!(
        held.contains_key(&kept),
        "the thing nobody deleted keeps its page"
    );
}

/// **A spelling a surviving thing holds now is not the deleted thing's page**:
/// a thing renamed away from `person:nelson` still lists that spelling as a
/// former handle, and a second thing made at the vacated spelling has written a
/// page there. Erasing the first under the store clears the pages only it wrote,
/// and the second thing keeps its row and its page. Its page sits under the very
/// spelling the deleted thing lists as a former handle.
#[tokio::test]
async fn deleting_a_renamed_thing_keeps_the_page_of_a_thing_at_its_vacated_handle() {
    let store = InMemoryMemory::booted();
    let doomed = thing(&store, "person:nelson", "Nelson").await;
    store
        .set_prose(&doomed, "the first page", None)
        .await
        .expect("prose lands");
    let renamed = EntityId("person:delta".into());
    store
        .rename_entity(
            &doomed,
            &renamed,
            None,
            date(2026, 6, 4),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename is answered")
        .written()
        .expect("nothing collides with the new handle");
    // The vacated handle is screened against the former one, so the creation is
    // refused once and made again with the override its refusal offers.
    let mut second = NewEntity::new(EntityId("person:nelson".into()), "Gamma Prime", "fixture");
    let later = match store.add_entity(second.clone()).await.expect("add") {
        crate::memory::Guarded::Written(entity) => entity.id,
        crate::memory::Guarded::Blocked {
            attempted,
            candidates,
        } => {
            second.override_token = Some(crate::memory::guard::override_token(
                &attempted,
                &candidates,
            ));
            second.override_reason = Some("a different person at the vacated handle".into());
            store
                .add_entity(second)
                .await
                .expect("add")
                .written()
                .expect("the override lifts the refusal")
                .id
        }
    };
    store
        .set_prose(&later, "the second page", None)
        .await
        .expect("prose lands");

    store.delete_entity_past_the_guard(&renamed);

    let second_page = store.prose.lock().unwrap().get(&later).cloned();
    assert_eq!(
        second_page.as_deref(),
        Some("the second page"),
        "the thing at the reused handle keeps its page"
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("the roster reads")
            .iter()
            .any(|entity| entity.id == later),
        "and its row survives"
    );
}

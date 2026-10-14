//! **A merge is an appended event, proven against any [`Memory`] adapter**
//! (card 2248). The fold writes one event naming the folded thing, the survivor,
//! the moment and the run, and the map from each folded claim's old address to
//! the address it has under the survivor. An address handed out before the
//! merge still names its claim afterwards. Each read is paired with its
//! positive: an event for the pair beside none for a thing nobody merged, an
//! address that moved beside one that never was a claim.

use super::support::{add, capture, other_caller};
use super::*;

/// One boy folded into another, with two claims on the folded one and one on
/// the survivor, so both sides own an `f1`. Returns `(survivor, folded, [the
/// folded claims])`.
async fn fold_one_into_the_other<M: Memory>(
    store: &M,
    survivor: &str,
    folded: &str,
    watcher: &str,
    session: Option<&str>,
) -> (EntityId, EntityId, Vec<Fact>) {
    let survivor = EntityId::person(format!("person:{survivor}"));
    let folded = EntityId::person(format!("person:{folded}"));
    add(
        store,
        NewEntity::new(
            survivor.clone(),
            survivor.as_str().replace([':', '-'], " "),
            "contract-fixture",
        ),
    )
    .await;
    add(
        store,
        NewEntity::new(
            folded.clone(),
            folded.as_str().replace([':', '-'], " "),
            "contract-fixture",
        ),
    )
    .await;
    capture(
        store,
        NewFact::about(
            survivor.clone(),
            "keeps a cobalt notebook",
            date(2026, 6, 1),
        ),
    )
    .await;
    let mut moved = Vec::new();
    for content in ["skates to school", "owes a dollar"] {
        moved.push(
            capture(
                store,
                NewFact::about(folded.clone(), content, date(2026, 6, 2)),
            )
            .await,
        );
    }
    // A claim on a third thing resting on the first folded claim, written before
    // the merge: the merge points it at the new address, and a reader holding
    // the old one still finds it.
    let onlooker = EntityId::person(format!("person:{watcher}"));
    add(
        store,
        NewEntity::new(
            onlooker.clone(),
            onlooker.as_str().replace([':', '-'], " "),
            "contract-fixture",
        ),
    )
    .await;
    capture(
        store,
        NewFact {
            derived_from: Some(moved[0].address()),
            ..NewFact::about(onlooker, "so the school bus waits", date(2026, 6, 2))
        },
    )
    .await;
    store
        .merge(
            &folded,
            &survivor,
            Some("one boy, filed twice"),
            date(2026, 6, 3),
            &other_caller(),
            session,
        )
        .await
        .expect("the fold lands");
    (survivor, folded, moved)
}

/// **The merge reads back as an event with its session, and the map says where
/// each claim went.**
pub async fn a_merge_reads_back_as_an_event_with_its_session_and_the_claim_map<M: Memory>(
    store: &M,
) {
    let (survivor, folded, moved) = fold_one_into_the_other(
        store,
        "contract-merge-event-apu",
        "contract-merge-event-moe",
        "contract-merge-event-carl",
        Some("contract-merge-run"),
    )
    .await;
    // A pair nobody merged, so an empty answer is not simply an empty store.
    let loner = EntityId::person("person:contract-merge-event-homer");
    add(
        store,
        NewEntity::new(loner.clone(), "Contract Loner", "contract-fixture"),
    )
    .await;

    let events = store
        .merge_events(Some(&survivor))
        .await
        .expect("the events read");
    assert_eq!(events.len(), 1, "one merge, one event: {events:?}");
    let event = &events[0];
    assert_eq!(event.folded, folded);
    assert_eq!(event.survivor, survivor);
    assert_eq!(
        event.session.as_deref(),
        Some("contract-merge-run"),
        "the event names the run that merged"
    );
    assert_eq!(
        store
            .merge_events(Some(&folded))
            .await
            .expect("the events read"),
        events,
        "the folded side finds the same event"
    );
    assert!(
        store
            .merge_events(Some(&loner))
            .await
            .expect("the events read")
            .is_empty(),
        "a thing nobody merged has no event"
    );
    assert!(
        store
            .merge_events(None)
            .await
            .expect("the events read")
            .contains(event),
        "the whole list holds it"
    );

    // **The map is where the claims are**: each folded claim, read at its new
    // address on the survivor, says what it said.
    assert_eq!(event.claims.len(), moved.len(), "{event:?}");
    let now = store.recall(&survivor).await.expect("recall reads");
    for original in &moved {
        let (_, new_id) = event
            .claims
            .iter()
            .find(|(was, _)| was == &original.id)
            .unwrap_or_else(|| panic!("no entry for {}: {event:?}", original.id));
        let there = now
            .iter()
            .find(|fact| &fact.id == new_id)
            .unwrap_or_else(|| panic!("the survivor holds no {new_id}: {now:?}"));
        assert_eq!(
            there.content, original.content,
            "the map names the claim that moved"
        );
    }
}

/// **An address handed out before the merge still names its claim.** Read,
/// edited and taken back through the old address, the claim answers at its new
/// one; an address that never was a claim is still refused as before.
pub async fn an_address_from_before_a_merge_still_names_its_claim<M: Memory>(store: &M) {
    let (survivor, folded, moved) = fold_one_into_the_other(
        store,
        "contract-merge-address-ned",
        "contract-merge-address-edna",
        "contract-merge-address-lenny",
        None,
    )
    .await;
    let events = store
        .merge_events(Some(&survivor))
        .await
        .expect("the events read");
    let new_of = |was: &Fact| -> FactAddress {
        let (_, id) = events[0]
            .claims
            .iter()
            .find(|(old, _)| old == &was.id)
            .expect("the claim moved");
        FactAddress::new(survivor.clone(), id.clone())
    };
    let old_first = FactAddress::new(folded.clone(), moved[0].id.clone());
    let old_second = FactAddress::new(folded.clone(), moved[1].id.clone());

    // The address moves to where the claim is.
    assert_eq!(
        store.moved_address(&old_first).await.expect("reads"),
        Some(new_of(&moved[0]))
    );
    // And what is not a moved claim does not move: an address that was never a
    // claim, and an address on a thing nobody merged.
    assert_eq!(
        store
            .moved_address(&FactAddress::new(folded.clone(), FactId("f99".into())))
            .await
            .expect("reads"),
        None
    );
    assert_eq!(
        store
            .moved_address(&new_of(&moved[0]))
            .await
            .expect("reads"),
        None,
        "an address under the survivor is already where it is"
    );

    // What rests on the claim is found through either address, and what rests on
    // nothing is found through neither.
    let resting_on_new = store
        .built_on(&new_of(&moved[0]))
        .await
        .expect("built_on reads");
    assert_eq!(resting_on_new.len(), 1, "{resting_on_new:?}");
    assert_eq!(
        store.built_on(&old_first).await.expect("built_on reads"),
        resting_on_new,
        "the old address finds what rests on the claim"
    );
    assert!(
        store
            .built_on(&old_second)
            .await
            .expect("built_on reads")
            .is_empty(),
        "nothing rests on the second claim, by either address"
    );

    // The history of the claim reads the same through either address.
    assert_eq!(
        store.claim_history(&old_first).await.expect("old reads"),
        store
            .claim_history(&new_of(&moved[0]))
            .await
            .expect("new reads"),
        "the old address reads the claim's own history"
    );
    // An address that was never a claim is refused as it always was.
    assert!(
        store
            .claim_history(&FactAddress::new(folded.clone(), FactId("f99".into())))
            .await
            .is_err(),
        "a claim nobody made is still no claim"
    );

    // Edited through the old address, the claim changes where it now lives.
    store
        .update_fact(
            &old_first,
            FactPatch {
                content: Some("skates to the old school".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &other_caller(),
        )
        .await
        .expect("the old address edits")
        .written()
        .expect("not blocked");
    let after = store.recall(&survivor).await.expect("recall reads");
    assert!(
        after
            .iter()
            .any(|f| f.address() == new_of(&moved[0]) && f.content == "skates to the old school"),
        "the edit landed on the claim under the survivor: {after:?}"
    );

    // Taken back through the old address, the claim is taken back there.
    store
        .retract(
            &old_second,
            Some("paid"),
            date(2026, 6, 9),
            &other_caller(),
            None,
        )
        .await
        .expect("the old address retracts");
    let after = store.recall(&survivor).await.expect("recall reads");
    let taken_back = after
        .iter()
        .find(|f| f.address() == new_of(&moved[1]))
        .expect("the claim is still listed, marked");
    assert_eq!(
        taken_back.status,
        crate::memory::FactStatus::Archived,
        "the claim under the survivor is the one taken back: {after:?}"
    );
}

/// **An address from before two merges names its claim after both**, and the
/// address the first merge gave it names the same claim. Every store follows a
/// chain of merges to the same end, which is what the one walk in
/// [`crate::memory::claim_walk`] gives them.
pub async fn an_address_from_before_a_chain_of_merges_names_its_claim_at_the_end<M: Memory>(
    store: &M,
) {
    let (middle, first, moved) = fold_one_into_the_other(
        store,
        "contract-merge-chain-ned",
        "contract-merge-chain-edna",
        "contract-merge-chain-lenny",
        None,
    )
    .await;
    let last = EntityId::person(format!("person:{}", "contract-merge-chain-burns"));
    add(
        store,
        NewEntity::new(
            last.clone(),
            "contract merge chain burns",
            "contract-fixture",
        ),
    )
    .await;
    store
        .merge(
            &middle,
            &last,
            Some("filed twice again"),
            date(2026, 6, 4),
            &other_caller(),
            None,
        )
        .await
        .expect("the second fold lands");

    let first_merge = &store
        .merge_events(Some(&first))
        .await
        .expect("the events read")[0];
    let second_merge = &store
        .merge_events(Some(&last))
        .await
        .expect("the events read")[0];
    let given_by_first = |was: &Fact| -> FactId {
        first_merge
            .claims
            .iter()
            .find(|(old, _)| old == &was.id)
            .expect("the first merge moved the claim")
            .1
            .clone()
    };
    let given_by_second = |at: &FactId| -> FactId {
        second_merge
            .claims
            .iter()
            .find(|(old, _)| old == at)
            .expect("the second merge moved the claim")
            .1
            .clone()
    };
    for claim in &moved {
        let end = FactAddress::new(last.clone(), given_by_second(&given_by_first(claim)));
        assert_eq!(
            store
                .moved_address(&FactAddress::new(first.clone(), claim.id.clone()))
                .await
                .expect("reads"),
            Some(end.clone()),
            "the address from before both merges"
        );
        assert_eq!(
            store
                .moved_address(&FactAddress::new(middle.clone(), given_by_first(claim)))
                .await
                .expect("reads"),
            Some(end),
            "the address the first merge gave it"
        );
    }
    assert_eq!(
        store
            .moved_address(&FactAddress::new(first.clone(), FactId("f99".into())))
            .await
            .expect("reads"),
        None,
        "an address that was never a claim still does not move"
    );
}

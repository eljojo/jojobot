//! **Siblings numbered apart are not refused for their names, proven against
//! any [`Memory`] adapter.** The near-miss screen reads their handles as the
//! caller's own statement of which one each is.

use super::support::add;
use super::*;

/// **A numbered handle under one parent says which one it is**, so a follow-up
/// card filed beside an earlier one is not refused for its name. Two cards on
/// one topic share rare words, and two passes of one review have names that
/// differ by a digit; neither name channel fires between siblings whose
/// handles are numbered apart. **Paired with what is still screened**: the same
/// title under another parent, a sibling whose handle is no numbered entry, and
/// an exact handle, which no token clears.
pub async fn numbered_siblings_are_not_refused_for_their_names<M: Memory>(store: &M) {
    let queue = EntityId("topic:contract-card-queue".into());
    let elsewhere = EntityId("topic:contract-card-shelf".into());
    for (id, name) in [
        (&queue, "Contract Card Queue"),
        (&elsewhere, "Contract Card Shelf"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let card = |parent: &EntityId, handle: &str, name: &str| NewEntity {
        parent: Some(parent.clone()),
        ..NewEntity::new(EntityId(handle.into()), name, "contract-fixture")
    };
    let answer = |new: NewEntity| async move {
        store
            .add_entity(new)
            .await
            .expect("add_entity should succeed")
    };

    add(
        store,
        card(
            &queue,
            "work:contract-q-2700",
            "Card 2700, the stands_for floor on capture",
        ),
    )
    .await;
    add(
        store,
        card(
            &queue,
            "work:contract-d9-review3-gap-code",
            "Review pass 3 gap: the adapter and source nobody read whole",
        ),
    )
    .await;

    // A follow-up card on the same topic, and the next pass of the review.
    for landed in [
        card(
            &queue,
            "work:contract-q-2741",
            "Card 2741, the stands_for floor in both orders",
        ),
        card(
            &queue,
            "work:contract-d9-review4-gap-code",
            "Review pass 4 gap: the adapter and source nobody read whole",
        ),
    ] {
        let id = landed.id.clone();
        match answer(landed).await {
            Guarded::Written(_) => {}
            Guarded::Blocked { candidates, .. } => {
                panic!("{id} is a numbered sibling and was refused: {candidates:?}")
            }
        }
    }

    // The same title under another parent is still screened for its words, and
    // so is a sibling whose handle is no entry of the series.
    for (refused, why) in [
        (
            card(
                &elsewhere,
                "work:contract-shelved-floor-order",
                "Card 2742, the stands_for floor in both orders",
            ),
            "under another parent",
        ),
        (
            card(
                &queue,
                "work:contract-stands-floor-followup",
                "Card follow-up, the stands_for floor on update",
            ),
            "a handle that is no numbered entry",
        ),
    ] {
        let id = refused.id.clone();
        let Guarded::Blocked { candidates, .. } = answer(refused).await else {
            panic!("{id} ({why}) passed the screen");
        };
        assert!(
            candidates
                .iter()
                .any(|m| m.handle.as_str() == "work:contract-q-2700"
                    && m.reason == guard::MatchReason::SharedWord),
            "{id} ({why}) is refused for the words it shares: {candidates:?}"
        );
    }

    // **A name that repeats a sibling's number is the copied-name duplicate the
    // screen exists for**, whatever the handle says: a typo of "Card 2700" filed
    // as the next number is refused on the name, and the same typo under a
    // different number lands.
    let copied = card(
        &queue,
        "work:contract-q-2743",
        "Card 2700, the stands_for flor on capture",
    );
    let Guarded::Blocked { candidates, .. } = answer(copied).await else {
        panic!("a name repeating the number of a sibling passed the screen");
    };
    assert!(
        candidates
            .iter()
            .any(|m| m.handle.as_str() == "work:contract-q-2700"
                && matches!(
                    m.reason,
                    guard::MatchReason::NearName | guard::MatchReason::SharedWord
                )),
        "the copied name is refused on the name: {candidates:?}"
    );
    let renumbered = card(
        &queue,
        "work:contract-q-2744",
        "Card 2744, the stands_for flor on capture",
    );
    assert!(
        matches!(answer(renumbered).await, Guarded::Written(_)),
        "the same typo under a different number is a numbered sibling and lands"
    );

    // An exact handle is never cleared, not even by the token its refusal minted.
    let again = card(&queue, "work:contract-q-2741", "Card 2741, filed again");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = answer(again.clone()).await
    else {
        panic!("an exact handle passed the screen");
    };
    assert!(
        candidates
            .iter()
            .any(|m| m.reason == guard::MatchReason::ExactHandle),
        "{candidates:?}"
    );
    let forced = answer(NewEntity {
        override_token: Some(guard::override_token(&attempted, &candidates)),
        override_reason: Some("a different card".to_string()),
        ..again
    })
    .await;
    assert!(
        matches!(forced, Guarded::Blocked { .. }),
        "the token an exact handle's refusal minted clears nothing"
    );
    numbered_siblings_with_added_parts(store).await;
    numbered_siblings_when_relabelled(store).await;
}

async fn numbered_siblings_with_added_parts<M: Memory>(store: &M) {
    let parent = EntityId("topic:contract-version-series".into());
    add(
        store,
        NewEntity::new(parent.clone(), "Version releases", "contract-fixture"),
    )
    .await;
    for (handle, name) in [
        ("work:contract-deploy-9", "Deploy 9"),
        ("work:contract-deploy-9-3-6", "Deploy 9.3.6: mail"),
        ("work:contract-deploy-9-3", "Deploy 9.3: threads"),
    ] {
        let answer = store
            .add_entity(NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(EntityId(handle.into()), name, "contract-fixture")
            })
            .await
            .expect("the add answers");
        assert!(
            matches!(answer, Guarded::Written(_)),
            "{handle}: {answer:?}"
        );
    }
}

async fn numbered_siblings_when_relabelled<M: Memory>(store: &M) {
    let parent = EntityId("topic:contract-relabel-queue".into());
    let elsewhere = EntityId("topic:contract-relabel-shelf".into());
    for (id, name) in [(&parent, "Labelled items"), (&elsewhere, "Elsewhere names")] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    for (under, handle, name) in [
        (&parent, "work:contract-q-2317", "Queue: card 2317 on dev8"),
        (&parent, "work:contract-q-2318", "The second item"),
        (&elsewhere, "work:contract-relabel-other", "Another item"),
    ] {
        add(
            store,
            NewEntity {
                parent: Some(under.clone()),
                ..NewEntity::new(EntityId(handle.into()), name, "contract-fixture")
            },
        )
        .await;
    }
    let edited = EntityId("work:contract-q-2318".into());
    let answer = store
        .update_entity(
            &edited,
            EntityPatch {
                name: Some("Queue: card 2318 on dev8".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the relabel answers");
    assert!(matches!(answer, Guarded::Written(_)), "{answer:?}");
    for (handle, name) in [
        (&edited, "Queue: card 2317 on dev8"),
        (&edited, "Queue: card 2317 on dev8x"),
        (
            &EntityId("work:contract-relabel-other".into()),
            "Queue: card 2319 on dev8",
        ),
    ] {
        let answer = store
            .update_entity(
                handle,
                EntityPatch {
                    name: Some(name.into()),
                    ..Default::default()
                },
            )
            .await
            .expect("the relabel answers");
        assert!(
            matches!(answer, Guarded::Blocked { .. }),
            "{handle}: {answer:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn numbered_siblings_keep_the_existing_contract() {
        numbered_siblings_are_not_refused_for_their_names(
            &crate::memory::testing::InMemoryMemory::booted(),
        )
        .await;
    }

    #[tokio::test]
    async fn numbered_siblings_with_added_parts_land() {
        numbered_siblings_with_added_parts(&crate::memory::testing::InMemoryMemory::booted()).await;
    }

    #[tokio::test]
    async fn numbered_siblings_can_be_relabelled() {
        numbered_siblings_when_relabelled(&crate::memory::testing::InMemoryMemory::booted()).await;
    }
}

use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// The current and former names must face the same size guard, while a small
/// claim through the former name still lands on the renamed thing.
#[tokio::test]
async fn a_claim_through_a_former_handle_is_measured_before_capture() {
    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;
    let renamed = json_of(
        &jojobot
            .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
                request_id: None,
                handle: "person:alpha".into(),
                to: "person:beta".into(),
                parent: None,
                recorded_at: None,
                override_token: None,
                override_reason: None,
                sid: Some(TEST_SID.into()),
            }))
            .await
            .expect("rename answers"),
    );
    assert_ne!(renamed["status"], "blocked", "{renamed}");

    let small = json_of(
        &jojobot
            .capture(Parameters(capture_args(
                "person:alpha",
                "fits after rename",
            )))
            .await
            .expect("capture answers"),
    );
    assert_ne!(small["status"], "blocked", "{small}");
    let current = EntityId("person:beta".into());
    let before = jojobot.memory.recall(&current).await.expect("claims read");
    assert!(
        before
            .iter()
            .any(|fact| fact.content == "fits after rename")
    );

    // The content alone spends the whole answer ceiling, before the row,
    // stamp and envelope which a whole record must also carry.
    let over = "x".repeat(jojobot_domain::text::ANSWER_CEILING);
    for subject in ["person:beta", "person:alpha"] {
        let refused = json_of(
            &jojobot
                .capture(Parameters(capture_args(subject, &over)))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(
            refused["status"], "blocked",
            "over-ceiling claim through {subject}"
        );
        assert_eq!(refused["wrote"], false, "{subject}: {refused}");
        assert_eq!(
            refused["refusal_code"], "ClaimTooLarge",
            "{subject}: {refused}"
        );
        assert_eq!(refused["what_is_over"], "the claim", "{subject}: {refused}");
    }
    let after = jojobot.memory.recall(&current).await.expect("claims read");
    assert_eq!(after, before, "neither oversized capture wrote a record");
}

/// **A claim through a shorter former handle is measured under the current one.**
/// A read serves the record under the thing's current handle, in its address and
/// its subject, so a record written through a shorter former spelling grows by
/// twice the difference once served. The boundary is found by asking the guard
/// itself, not by guessing a length: the longest content the current handle
/// admits is admitted through the former one, and one character more is refused
/// through both.
#[tokio::test]
async fn a_claim_through_a_shorter_former_handle_meets_the_room_of_the_current_one() {
    let jojobot = handler();
    ensure(&jojobot, "person:mu").await;
    let renamed = json_of(
        &jojobot
            .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
                request_id: None,
                handle: "person:mu".into(),
                to: "person:epsilon".into(),
                parent: None,
                recorded_at: None,
                override_token: None,
                override_reason: None,
                sid: Some(TEST_SID.into()),
            }))
            .await
            .expect("rename answers"),
    );
    assert_ne!(renamed["status"], "blocked", "{renamed}");

    let over = |subject: &'static str, length: usize| {
        let jojobot = &jojobot;
        async move {
            let new = jojobot_domain::memory::NewFact::about(
                EntityId(subject.into()),
                "x".repeat(length),
                jiff::civil::date(2026, 11, 7),
            );
            jojobot
                .refuses_a_new_claim_too_big_to_read(&new, None)
                .await
                .is_some()
        }
    };
    // The longest content the current handle admits, found by halving.
    let (mut fits, mut over_by) = (0_usize, jojobot_domain::text::ANSWER_CEILING + 64);
    assert!(!over("person:epsilon", fits).await, "an empty claim fits");
    assert!(
        over("person:epsilon", over_by).await,
        "a huge claim does not"
    );
    while over_by - fits > 1 {
        let middle = (fits + over_by) / 2;
        if over("person:epsilon", middle).await {
            over_by = middle;
        } else {
            fits = middle;
        }
    }
    assert!(
        !over("person:mu", fits).await,
        "the boundary fits through the former handle"
    );
    assert!(
        over("person:epsilon", fits + 1).await,
        "one more is refused through the current handle"
    );
    assert!(
        over("person:mu", fits + 1).await,
        "and through the former one: both spellings are one thing, served under the current handle"
    );
}

/// **The size of a claim read from the claims State is the size the store route
/// reads**: the same record, the same thing, the same room.
#[tokio::test]
async fn a_claim_is_measured_from_the_state_as_the_store_measures_it() {
    use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody, State};
    use jojobot_domain::log::{Event, ObjectId};

    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;
    let id = EntityId("person:alpha".into());
    let entity = jojobot
        .memory
        .scan_entity(&id)
        .await
        .expect("the thing is read")
        .expect("the thing exists")
        .entity
        .expect("the thing has a row");
    let envelope = |payload| Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: None,
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: None,
        imported_source_time: None,
        payload,
    };
    let state = State::replay(&[
        envelope(ClaimEvent::ObjectCreated {
            object: ObjectId("k1".into()),
            body: ObjectBody {
                kind: KindId(entity.kind.as_token().to_string()),
                parent: None,
                name: entity.name.clone(),
                source: entity.source.clone(),
                aliases: entity.aliases.clone(),
                boot: None,
                metadata: Default::default(),
                archived: false,
                bound_to: None,
            },
        }),
        envelope(ClaimEvent::AddressBound {
            object: ObjectId("k1".into()),
            address: id.0.clone(),
        }),
    ]);

    let now = jojobot.clock().now();
    for content in ["short", &"y".repeat(500)] {
        let new = NewFact::about(id.clone(), content, jiff::civil::date(2026, 4, 19));
        let room = jojobot
            .room_for_records(&id)
            .await
            .expect("the thing is measurable");
        let (size, room_for_one) = jojobot.record_size_and_room(&room, &new, 1);
        let measured =
            Jojobot::measure_new_claim(&state, &new, now).expect("the State holds the thing");
        assert_eq!(
            (measured.size, measured.room),
            (size, room_for_one),
            "the State measures {content:?} as the store route does"
        );
        assert_eq!(measured.row, None);
    }
}

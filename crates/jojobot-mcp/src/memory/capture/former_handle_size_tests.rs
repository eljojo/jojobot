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
            refused["refusal_code"], "TooBigToRead",
            "{subject}: {refused}"
        );
        assert_eq!(refused["what_is_over"], "the claim", "{subject}: {refused}");
    }
    let after = jojobot.memory.recall(&current).await.expect("claims read");
    assert_eq!(after, before, "neither oversized capture wrote a record");
}

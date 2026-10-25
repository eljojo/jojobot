use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// The direct fact door must refuse the same unbound old receipt as the
/// entity door, and finish when the receipt keeps the original owner.
#[tokio::test]
async fn a_cut_fact_rewrite_with_a_legacy_receipt_requires_a_person() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            sets: Some([("runs_out".into(), "2099-12-01".into())].into()),
            ..capture_args("work:handcart", "the cart is due")
        },
    )
    .await;
    let address = captured["address"].as_str().expect("the claim address");
    let args = UpdateFactArgs {
        address: address.to_string(),
        sid: Some(sid),
        ..Default::default()
    };
    let caller = crate::caller::Caller {
        sid: crate::sid::Sid(TEST_RUN.into()),
        bot: EntityId("bot:otto".into()),
        card: None,
        zone: None,
        day: None,
    };
    let note = Some(super::super::due::CLEARS_DUE_ON.to_string());
    let refused = json_of(
        &jojobot
            .finish_the_edit(
                &args,
                &caller,
                jojobot_domain::request::Recorded {
                    landed: serde_json::json!({"address": address}).to_string(),
                    note: note.clone(),
                },
            )
            .await
            .unwrap(),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["fix_by"], "person", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    let way = refused["how_to_proceed"].as_str().unwrap();
    assert!(
        way.contains("operator") && way.contains("identity") && way.contains("request"),
        "{way}"
    );
    assert_eq!(
        fields_of(&jojobot, "work:handcart").await["due_on"],
        "2099-12-01"
    );

    let entity = jojobot
        .entity_now(&EntityId("work:handcart".into()))
        .await
        .unwrap();
    let finished = json_of(
        &jojobot
            .finish_the_edit(
                &args,
                &caller,
                jojobot_domain::request::Recorded {
                    landed: serde_json::json!({"address": address, "entity": entity.badge})
                        .to_string(),
                    note,
                },
            )
            .await
            .unwrap(),
    );
    assert_eq!(finished["address"], address, "{finished}");
    assert_eq!(finished["subject"], "work:handcart", "{finished}");
    assert!(
        fields_of(&jojobot, "work:handcart")
            .await
            .get("due_on")
            .is_none()
    );
    assert!(
        finished.get("entity").is_none(),
        "the private identity is never served"
    );
}

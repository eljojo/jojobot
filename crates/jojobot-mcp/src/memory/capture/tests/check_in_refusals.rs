use super::*;

/// **A check-in that is sent beside a key it computes is refused, and the
/// refusal does not offer to type the arithmetic by hand.** The way forward it
/// names is to send the check-in without the key, because the check-in works the
/// key out. It used to offer sending the fields without a check-in and keeping
/// the arithmetic, which invites writing the basis of a schedule by hand and
/// leaves it unchecked. Beside it the same check-in without the key, which lands.
#[tokio::test]
async fn a_check_in_beside_a_computed_key_is_refused_and_the_way_forward_is_the_check_in() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                sets: Some(
                    [("counts_from".to_string(), "2026-08-10".to_string())]
                        .into_iter()
                        .collect(),
                ),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "did it this morning")
            }))
            .await
            .expect("a refusal is an answer"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    let how = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        how.contains("check-in") && how.contains("counts_from"),
        "the refusal does not name the check-in and the key: {how}"
    );
    assert!(
        !how.contains("arithmetic") && !how.contains("without check_in"),
        "the refusal offers to type the schedule by hand: {how}"
    );

    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "did it this morning")
        },
    )
    .await;
    assert!(landed["address"].is_string(), "{landed}");
    assert_ne!(landed["status"], "blocked", "{landed}");
}

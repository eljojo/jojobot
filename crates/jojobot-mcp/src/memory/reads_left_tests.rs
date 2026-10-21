//! **A read that fails never reads as nothing on a receipt.** Each case writes with
//! a store that cannot read one thing, and the receipt names what it could not
//! work out; the same write over a healthy store carries its note.

use super::testing::*;
use super::*;
use crate::harness::*;

/// A rule starred on a bot, as the bot's own claim.
fn a_starred_rule(content: &str) -> CaptureArgs {
    CaptureArgs {
        fields: Some([("starred".to_string(), "true".to_string())].into()),
        ..capture_args("bot:gamma", content)
    }
}

/// A bot whose own `rule_seats` hold more than the default carries `seats` on a
/// write only when its rules pass THAT number.
async fn gamma_with_twenty_seats(healthy: &Jojobot) {
    make_bot(healthy, "gamma").await;
    capture_ok(
        healthy,
        CaptureArgs {
            sets: Some([("rule_seats".to_string(), "20".to_string())].into()),
            ..capture_args("bot:gamma", "twenty seats")
        },
    )
    .await;
}

/// **A rule starred on a bot whose seat count cannot be read is not told it was
/// pushed off the boot.** The bot holds twenty seats; read as the default, nine
/// starred rules would have four pushed off, and the receipt would say so for a
/// boot that carries all nine. The receipt names the note it could not work out
/// instead. Paired with the healthy read, which says nothing about seats.
#[tokio::test]
async fn a_rule_starred_over_an_unreadable_seat_count_names_the_note_it_left_off() {
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("bot:gamma"));
    gamma_with_twenty_seats(&healthy).await;
    for n in 0..7 {
        let landed = capture_ok(&healthy, a_starred_rule(&format!("rule number {n}"))).await;
        assert!(
            landed.get("seats").is_none(),
            "twenty seats hold them: {landed}"
        );
        assert!(
            landed.get("reads_left").is_none(),
            "a healthy read leaves nothing: {landed}"
        );
    }
    let told = json_of(
        &blind
            .capture(rmcp::handler::server::wrapper::Parameters(a_starred_rule(
                "rule number 8",
            )))
            .await
            .expect("the write lands"),
    );
    assert!(
        told.get("seats").is_none(),
        "no false note about seats: {told}"
    );
    assert_eq!(
        told["reads_left"]["not_worked_out"][0]["what"], "seat_note",
        "the receipt names the note it left off: {told}"
    );
}

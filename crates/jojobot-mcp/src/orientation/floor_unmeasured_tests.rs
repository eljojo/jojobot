//! **A boot floor a write could not measure is said, and a seat count that is
//! no number is refused whether or not the fields read** (rule 130).
//!
//! Each case runs two handlers over one store, the second unable to read the
//! fields of `bot:gamma`, and puts the blind answer beside the healthy one.

use crate::harness::*;
use crate::memory::SetCharterArgs;
use crate::memory::testing::*;
use rmcp::handler::server::wrapper::Parameters;

fn blind_and_healthy() -> (crate::Jojobot, crate::Jojobot) {
    healthy_and_down(Down::FieldsOf("bot:gamma"))
}

async fn charter_of(jojobot: &crate::Jojobot, prose: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "bot:gamma".into(),
                prose: prose.into(),
                sid: Some(writing_as(jojobot)),
                request_id: None,
            }))
            .await
            .expect("set_charter answers"),
    )
}

/// **A charter written while the bot's fields cannot be read lands, and the
/// answer says the boot floor was not measured.** The healthy handler's answer
/// to the same call carries no such block, so the block is the read that failed
/// and nothing a charter always gets.
#[tokio::test]
async fn a_charter_written_over_unreadable_fields_says_the_floor_was_not_measured() {
    let (healthy, blind) = blind_and_healthy();
    make_bot(&healthy, "gamma").await;

    let measured = charter_of(&healthy, "Gamma keeps the ledgers.").await;
    assert_ne!(measured["status"], "blocked", "{measured}");
    assert!(measured.get("reads_left").is_none(), "{measured}");

    let unmeasured = charter_of(&blind, "Gamma keeps the ledgers, and the shelves.").await;
    assert_ne!(unmeasured["status"], "blocked", "{unmeasured}");
    let left = unmeasured["reads_left"]["not_worked_out"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer names no read it left: {unmeasured}"));
    assert!(
        left.iter()
            .any(|read| read["what"] == crate::memory::reads_left::BOOT_FLOOR),
        "{unmeasured}"
    );
}

/// **A seat count that is no number is refused, fields read or not.** The check
/// is asked directly: a capture over unreadable fields stops earlier with a store
/// error to retry, so the verb never reaches this check there. Both handlers
/// refuse the same capture for the number, and the blind one has said nothing
/// about a floor it never measured (the refusal is not a measurement).
#[tokio::test]
async fn a_seat_count_that_is_no_number_is_refused_whether_or_not_the_fields_read() {
    let (healthy, blind) = blind_and_healthy();
    make_bot(&healthy, "gamma").await;
    // The claims unreadable as well as the fields: the check stops at its first
    // read, and the number has to be judged before it.
    let (_, blind_to_claims) = healthy_and_down(Down::Recall);
    let gamma = jojobot_domain::memory::EntityId("bot:gamma".into());
    let writer = jojobot_domain::memory::EntityId("bot:otto".into());
    let today = healthy.clock().today_in(&jiff::tz::TimeZone::UTC);
    let mut new = jojobot_domain::memory::NewFact::about(gamma, "gamma carries abc seats", today);
    new.fields = [("rule_seats".to_string(), "abc".to_string())].into();
    for (who, jojobot) in [
        ("healthy", &healthy),
        ("blind to fields", &blind),
        ("blind to claims", &blind_to_claims),
    ] {
        let reads = crate::memory::reads_left::ReadsLeft::default();
        let refused = jojobot
            .refuses_a_boot_floor_for_capture(&new, &writer, &reads)
            .await;
        match refused {
            Some(jojobot_domain::memory::MemoryError::InvalidFact(words)) => {
                assert!(words.contains("rule_seats"), "{who}: {words}");
                assert!(words.contains("'abc'"), "{who}: {words}");
            }
            other => panic!("{who}: expected the seat count refused, got {other:?}"),
        }
    }
}

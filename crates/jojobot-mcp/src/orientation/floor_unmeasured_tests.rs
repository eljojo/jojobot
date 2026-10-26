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

fn gamma() -> jojobot_domain::memory::EntityId {
    jojobot_domain::memory::EntityId("bot:gamma".into())
}

fn names_gamma_unmeasured(reads: &crate::memory::reads_left::ReadsLeft) -> bool {
    reads.block().is_some_and(|block| {
        block["not_worked_out"].as_array().is_some_and(|left| {
            left.iter().any(|read| {
                read["what"] == crate::memory::reads_left::BOOT_FLOOR
                    && read["read"]
                        .as_str()
                        .is_some_and(|r| r.contains("bot:gamma"))
            })
        })
    })
}

/// **An edit naming a seat count that is no number is refused when the bot's
/// claims cannot be read.** The edit path read the claims before it judged the
/// number, so an unreadable bot let a bad count through. A count that is a number
/// is not refused for the number, and the unread claims are named on the answer:
/// the refusal is the number's, not the measurement's.
#[tokio::test]
async fn an_edit_naming_a_seat_count_that_is_no_number_is_refused_over_unreadable_claims() {
    let (healthy, blind) = healthy_and_blind_to_claims();
    make_bot(&healthy, "gamma").await;
    let address = jojobot_domain::memory::FactAddress {
        home: gamma(),
        local: jojobot_domain::memory::FactId("f1".into()),
    };
    let writer = jojobot_domain::memory::EntityId("bot:otto".into());
    let seats = |written: &str| jojobot_domain::memory::FactPatch {
        fields: [("rule_seats".to_string(), written.to_string())].into(),
        ..Default::default()
    };

    let reads = crate::memory::reads_left::ReadsLeft::default();
    let numeric = blind
        .refuses_a_boot_floor_for_edit(&address, &seats("3"), &writer, &reads)
        .await;
    assert!(numeric.is_none(), "{numeric:?}");
    assert!(names_gamma_unmeasured(&reads), "{:?}", reads.block());

    let reads = crate::memory::reads_left::ReadsLeft::default();
    match blind
        .refuses_a_boot_floor_for_edit(&address, &seats("abc"), &writer, &reads)
        .await
    {
        Some(jojobot_domain::memory::MemoryError::InvalidFact(words)) => {
            assert!(words.contains("'abc'"), "{words}");
        }
        other => panic!("expected the seat count refused, got {other:?}"),
    }
}

/// **A relabelling whose bot cannot be scanned says the floor was not measured.**
/// The scan failure ended the check with nothing on the answer, so the edit
/// landed as if the floor had been measured and found fine. The healthy handler's
/// answer to the same patch carries no such note.
#[tokio::test]
async fn a_relabelling_over_an_unreadable_scan_says_the_floor_was_not_measured() {
    let (healthy, blind) = healthy_and_down(Down::ScanOf("bot:gamma"));
    make_bot(&healthy, "gamma").await;
    let patch = jojobot_domain::memory::EntityPatch {
        name: Some("Gamma, renamed".into()),
        ..Default::default()
    };

    let reads = crate::memory::reads_left::ReadsLeft::default();
    assert!(
        healthy
            .refuses_a_boot_floor_for_relabelling(&gamma(), &patch, &reads)
            .await
            .is_none()
    );
    assert!(reads.block().is_none(), "{:?}", reads.block());

    let reads = crate::memory::reads_left::ReadsLeft::default();
    assert!(
        blind
            .refuses_a_boot_floor_for_relabelling(&gamma(), &patch, &reads)
            .await
            .is_none()
    );
    assert!(names_gamma_unmeasured(&reads), "{:?}", reads.block());
}

/// **Every bot whose floor could not be measured is named**, not left out of the
/// count without a word. The measured bots still come back.
#[tokio::test]
async fn the_floors_of_bots_name_each_bot_they_could_not_measure() {
    let (healthy, blind) = healthy_and_blind_to_claims();
    make_bot(&healthy, "gamma").await;

    let reads = crate::memory::reads_left::ReadsLeft::default();
    let floors = healthy.boot_floors_of_bots(&reads).await;
    assert!(floors.iter().any(|(bot, _)| bot == &gamma()), "{floors:?}");
    assert!(reads.block().is_none(), "{:?}", reads.block());

    let reads = crate::memory::reads_left::ReadsLeft::default();
    let floors = blind.boot_floors_of_bots(&reads).await;
    assert!(!floors.iter().any(|(bot, _)| bot == &gamma()), "{floors:?}");
    assert!(names_gamma_unmeasured(&reads), "{:?}", reads.block());
}

/// **A list of bots that cannot be read is named as the read that failed**, and
/// no bot is measured from a list that was not read. The healthy handler reads the
/// same store, measures `bot:gamma` and names nothing.
#[tokio::test]
async fn the_floors_of_bots_name_the_list_of_bots_when_it_cannot_be_read() {
    let (healthy, blind) = healthy_and_down(Down::EntityIndex);
    make_bot(&healthy, "gamma").await;

    let reads = crate::memory::reads_left::ReadsLeft::default();
    let floors = healthy.boot_floors_of_bots(&reads).await;
    assert!(floors.iter().any(|(bot, _)| bot == &gamma()), "{floors:?}");
    assert!(reads.block().is_none(), "{:?}", reads.block());

    let reads = crate::memory::reads_left::ReadsLeft::default();
    let floors = blind.boot_floors_of_bots(&reads).await;
    assert!(floors.is_empty(), "{floors:?}");
    let named = reads.block().is_some_and(|block| {
        block["not_worked_out"].as_array().is_some_and(|left| {
            left.iter().any(|read| {
                read["what"] == crate::memory::reads_left::BOOT_FLOOR
                    && read["read"] == "the list of bots"
            })
        })
    });
    assert!(named, "{:?}", reads.block());
}

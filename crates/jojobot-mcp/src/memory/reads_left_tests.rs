//! **A read that fails never reads as nothing on a receipt.** Each case writes with
//! a store that cannot read one thing, and the receipt names what it could not
//! work out; the same write over a healthy store carries its note.

use super::testing::*;
use super::update_fact::UpdateFactArgs;
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
    let left: Vec<&str> = told["reads_left"]["not_worked_out"]
        .as_array()
        .map(|left| left.iter().filter_map(|l| l["what"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        left.contains(&"seat_note"),
        "the receipt names the note it left off: {told}"
    );
}

/// A bot with a room of one and a thought in it, in a store whose claims of the bot
/// can be made to stop reading after `reads` reads. Returns the handler that
/// holds the failure and the one that does not, and the borrowing thought.
async fn a_full_room(
    reads: usize,
) -> (
    Jojobot,
    Jojobot,
    CaptureArgs,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (healthy, blind) =
        healthy_and_down(Down::RecallOfAfter("bot:gamma", reads, counter.clone()));
    make_bot(&healthy, "gamma").await;
    capture_ok(
        &healthy,
        CaptureArgs {
            sets: Some([("thought_capacity".to_string(), "1".to_string())].into()),
            ..capture_args("bot:gamma", "capacity is one")
        },
    )
    .await;
    for handle in ["thing:jukebox", "thing:battery"] {
        ensure(&healthy, handle).await;
    }
    let thought = |object: &str, content: &str, sid: String, borrow: bool| CaptureArgs {
        sid: Some(sid),
        shape: Some("connection".into()),
        object: Some(object.into()),
        borrow: Some(borrow),
        ..capture_args("bot:gamma", content)
    };
    // Counting begins now, so the reads of the setup above are not counted.
    capture_ok(
        &healthy,
        thought(
            "thing:jukebox",
            "the jukebox needs a needle",
            writing_as_bot(&healthy, "bot:gamma"),
            false,
        ),
    )
    .await;
    counter.store(0, std::sync::atomic::Ordering::SeqCst);
    let borrowing = thought(
        "thing:battery",
        "the battery needs replacing",
        writing_as_bot(&blind, "bot:gamma"),
        true,
    );
    (healthy, blind, borrowing, counter)
}

/// **A borrowed thought whose claims cannot be read for the receipt is not
/// receipted as one that used nothing.** The claims read for the write itself
/// answer, and the read that builds the receipt does not. The write landed, so
/// it is not refused (rule 130); the receipt names the account count and the
/// reserve note it could not work out. Paired with the same write over a store
/// that reads, which carries the reserve note and no such block.
#[tokio::test]
async fn a_borrow_whose_receipt_cannot_read_the_claims_names_what_it_left_off() {
    // The first count of reads at which the write lands is the one where only the
    // receipt's own read fails.
    let mut landed = None;
    for reads in 0..16 {
        let (_, blind, borrowing, _) = a_full_room(reads).await;
        if let Ok(answered) = blind
            .capture(rmcp::handler::server::wrapper::Parameters(borrowing))
            .await
        {
            landed = Some((reads, json_of(&answered)));
            break;
        }
    }
    let (reads, told) = landed.expect("a count of reads at which only the receipt fails");
    assert_eq!(
        told["reads_left"]["not_worked_out"]
            .as_array()
            .map(|left| left
                .iter()
                .filter_map(|l| l["what"].as_str())
                .collect::<Vec<_>>()),
        Some(vec!["account_count", "reserve_note", "record_reserve_note"]),
        "{told}"
    );
    assert!(
        !told["postcondition"]
            .as_str()
            .unwrap_or_default()
            .contains("emergency reserve"),
        "no reserve note worked out from nothing: {told}"
    );

    // Paired: the healthy handler, over the same world, says the reserve.
    let (healthy, _, borrowing, _) = a_full_room(reads).await;
    let said = json_of(
        &healthy
            .capture(rmcp::handler::server::wrapper::Parameters(CaptureArgs {
                sid: borrowing
                    .sid
                    .clone()
                    .map(|_| writing_as_bot(&healthy, "bot:gamma")),
                ..borrowing
            }))
            .await
            .expect("the write lands"),
    );
    assert!(said.get("reads_left").is_none(), "{said}");
    assert!(
        said["postcondition"]
            .as_str()
            .unwrap_or_default()
            .contains("emergency reserve"),
        "{said}"
    );
}

/// **A record taken off a thing whose fields cannot be read does not report that no
/// key left it.** The archive lands. The receipt of the healthy read lists the key
/// the record alone set; the receipt over an unreadable thing names the account it
/// could not make, and does not say an empty list of keys.
#[tokio::test]
async fn an_archive_over_unreadable_fields_names_the_keys_it_could_not_account_for() {
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("person:homer"));
    ensure(&healthy, "person:homer").await;
    let mut addresses = Vec::new();
    for n in 0..2 {
        let said = capture_ok(
            &healthy,
            CaptureArgs {
                sets: Some([("mood".to_string(), format!("glad {n}"))].into()),
                ..capture_args("person:homer", &format!("homer is glad {n}"))
            },
        )
        .await;
        addresses.push(address_of(&said));
    }
    let archive = |address: &str| UpdateFactArgs {
        status: Some("archived".into()),
        details: Some("no longer so".into()),
        ..update_args(address)
    };

    let blind_told = json_of(
        &blind
            .update_fact(rmcp::handler::server::wrapper::Parameters(archive(
                &addresses[0],
            )))
            .await
            .expect("the archive lands"),
    );
    assert!(blind_told.get("left_the_thing").is_none(), "{blind_told}");
    let left: Vec<&str> = blind_told["reads_left"]["not_worked_out"]
        .as_array()
        .map(|left| left.iter().filter_map(|l| l["what"].as_str()).collect())
        .unwrap_or_default();
    assert!(left.contains(&"keys_left_note"), "{blind_told}");

    let told = json_of(
        &healthy
            .update_fact(rmcp::handler::server::wrapper::Parameters(archive(
                &addresses[1],
            )))
            .await
            .expect("the archive lands"),
    );
    assert!(told.get("reads_left").is_none(), "{told}");
    assert!(told.get("left_the_thing").is_some(), "{told}");
}

/// **A status word the board holds is not called unmapped because the board could
/// not be read.** The project lists `review` among its columns; work filed under it
/// takes the word and the healthy write says nothing. With the project's fields
/// unreadable, the write lands and the receipt names the note it could not work
/// out, and does not claim a word the board holds is outside it.
#[tokio::test]
async fn a_status_word_over_an_unreadable_board_is_not_called_unmapped() {
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("project:atlas"));
    ensure(&healthy, "project:atlas").await;
    capture_ok(
        &healthy,
        CaptureArgs {
            sets: Some(
                [(
                    "columns".to_string(),
                    "someday, next, now, waiting, done, review".to_string(),
                )]
                .into(),
            ),
            ..capture_args("project:atlas", "the board has a review column")
        },
    )
    .await;
    let mut under = jojobot_domain::memory::NewEntity::new(
        jojobot_domain::memory::EntityId("work:phi".into()),
        "Phi",
        "the fixture roster",
    );
    under.parent = Some(jojobot_domain::memory::EntityId("project:atlas".into()));
    healthy.memory.add_entity(under).await.expect("added");
    let in_review = || CaptureArgs {
        sets: Some([("status".to_string(), "review".to_string())].into()),
        ..capture_args("work:phi", "the roof is in review")
    };

    let told = capture_ok(&healthy, in_review()).await;
    assert!(told.get("status_word_note").is_none(), "{told}");
    assert!(told.get("reads_left").is_none(), "{told}");

    let blind_told = json_of(
        &blind
            .capture(rmcp::handler::server::wrapper::Parameters(in_review()))
            .await
            .expect("the write lands"),
    );
    assert!(
        blind_told.get("status_word_note").is_none(),
        "no false note about the word: {blind_told}"
    );
    let left: Vec<&str> = blind_told["reads_left"]["not_worked_out"]
        .as_array()
        .map(|left| left.iter().filter_map(|l| l["what"].as_str()).collect())
        .unwrap_or_default();
    assert!(left.contains(&"status_word_note"), "{blind_told}");
}

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

/// **A rhythm written with a cadence whose fields cannot be read is not told it
/// got no default.** The receipt of a healthy write names the policy jojobot
/// applied because the write named none. When the rhythm's fields cannot be
/// read, the policy cannot be worked out, and the receipt names that note as
/// left off instead of saying nothing. Paired with the healthy write, which
/// names the applied policy and leaves nothing off.
#[tokio::test]
async fn a_rhythm_whose_fields_cannot_be_read_names_the_policy_note_it_left_off() {
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("rhythm:descale"));
    ensure(&healthy, "thing:kettle").await;
    healthy
        .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("the rhythm is added");
    let schedule = |content: &str| CaptureArgs {
        sets: Some(
            [
                ("cadence_days".to_string(), "30".to_string()),
                ("counts_from".to_string(), "2026-08-01".to_string()),
            ]
            .into(),
        ),
        ..capture_args("rhythm:descale", content)
    };

    // The healthy write applies the default for a number of days and says so.
    let told = capture_ok(&healthy, schedule("every thirty days")).await;
    assert!(
        told.to_string().contains("check_in_date"),
        "the receipt names the policy it applied: {told}"
    );
    assert!(
        told.get("reads_left").is_none(),
        "a healthy read leaves nothing: {told}"
    );

    // The same write where the rhythm's fields cannot be read lands, names no
    // policy it did not work out, and names the note it left off.
    let blind_told = json_of(
        &blind
            .capture(rmcp::handler::server::wrapper::Parameters(schedule(
                "every thirty days, again",
            )))
            .await
            .expect("the write lands"),
    );
    assert!(
        !blind_told.to_string().contains("check_in_date"),
        "no policy is named that was not worked out: {blind_told}"
    );
    let left: Vec<&str> = blind_told["reads_left"]["not_worked_out"]
        .as_array()
        .map(|left| left.iter().filter_map(|l| l["what"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        left.contains(&"policy_note"),
        "the receipt names the note it left off: {blind_told}"
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

/// **Defrag's queue over a store whose things cannot be listed says the queue is not
/// known.** The block used to be left off defrag's boot, which reads like a queue of
/// nothing. Paired with the healthy block, which counts and carries no `unread`.
///
/// Asked of the block's own function: the boot builds its identity from one read of
/// the index and degrades whole when that read fails, so the second listing that
/// the queue makes cannot be made to fail alone from the boot's surface.
#[tokio::test]
async fn defrags_queue_over_an_unlistable_store_says_it_is_not_known() {
    let (healthy, blind) = healthy_and_down(Down::EntityIndex);
    make_bot(&healthy, "defrag").await;
    ensure(&healthy, "thread:alpha").await;

    let well = healthy.review_queue_block().await.expect("a queue block");
    assert!(well["count"].is_number(), "{well}");
    assert!(well.get("unread").is_none(), "{well}");

    let told = blind
        .review_queue_block()
        .await
        .expect("a block, not an absence");
    assert!(
        told["unread"].is_string(),
        "the queue is said to be unknown: {told}"
    );
    assert!(
        told.get("count").is_none(),
        "no count worked out from nothing: {told}"
    );
}

/// **A boot that cannot read its bot's fields says what it assumed and what it did
/// not do.** The bot carries twenty seats and a role to claim. Over unreadable
/// fields the boot goes on, because a boot that refused would leave the bot blind,
/// and names the seat count it worked out at the default and the role it could not
/// claim. Paired with the healthy boot, which claims the role and carries no block.
#[tokio::test]
async fn a_boot_over_unreadable_fields_names_the_seats_and_the_role_it_assumed() {
    use crate::orientation::OrientArgs;
    let boot = || OrientArgs {
        request_id: None,
        compacted: None,
        claim: None,
        timezone: None,
        bot: Some("gamma".into()),
        brief: Some(true),
        skill: None,
        section: None,
        resume: None,
        sid: None,
        today: None,
        ask_for_code: None,
        challenge: None,
        code: None,
        offer_code: None,
    };
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("bot:gamma"));
    make_bot(&healthy, "gamma").await;
    capture_ok(
        &healthy,
        CaptureArgs {
            sets: Some(
                [
                    ("rule_seats".to_string(), "20".to_string()),
                    ("claims_role".to_string(), "dev-dispatch".to_string()),
                ]
                .into(),
            ),
            ..capture_args("bot:gamma", "twenty seats and a role")
        },
    )
    .await;

    let well = json_of(
        &healthy
            .start_here(rmcp::handler::server::wrapper::Parameters(boot()))
            .await
            .expect("the boot answers"),
    );
    assert_eq!(well["session"]["claim"]["status"], "taken", "{well}");
    assert!(well.get("reads_left").is_none(), "{well}");
    assert!(well["identity"].get("reads_left").is_none(), "{well}");

    let told = json_of(
        &blind
            .start_here(rmcp::handler::server::wrapper::Parameters(boot()))
            .await
            .expect("the boot answers"),
    );
    let named = |block: &serde_json::Value| -> Vec<String> {
        block["not_worked_out"]
            .as_array()
            .map(|left| {
                left.iter()
                    .filter_map(|l| l["what"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    assert!(
        named(&told["reads_left"]).contains(&"carried_role".to_string()),
        "the boot says it claimed no role for want of a read: {told}"
    );
    assert!(
        named(&told["identity"]["reads_left"]).contains(&"seat_count".to_string()),
        "the identity says its seats are the default for want of a read: {told}"
    );
}

/// **A write is not refused on a boot floor measured at an assumed seat count.**
/// The bot carries one seat, so its boot carries one rule however many are
/// starred. Over unreadable fields the check would measure at the default of five
/// and could refuse a rule the real boot carries. It is skipped instead, and the
/// write lands. Paired with the healthy write, which is measured at one seat and
/// lands for the same reason.
#[tokio::test]
async fn a_write_is_not_refused_on_a_floor_measured_at_an_assumed_seat_count() {
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("bot:gamma"));
    make_bot(&healthy, "gamma").await;
    capture_ok(
        &healthy,
        CaptureArgs {
            sets: Some([("rule_seats".to_string(), "1".to_string())].into()),
            ..capture_args("bot:gamma", "one seat")
        },
    )
    .await;
    let rule = |n: usize| CaptureArgs {
        fields: Some([("starred".to_string(), "true".to_string())].into()),
        ..capture_args("bot:gamma", &format!("{n} {}", "a long rule. ".repeat(500)))
    };
    for n in 0..6 {
        let landed = json_of(
            &healthy
                .capture(rmcp::handler::server::wrapper::Parameters(rule(n)))
                .await
                .expect("the write answers"),
        );
        assert_ne!(
            landed["status"], "blocked",
            "one seat carries one rule: {landed}"
        );
    }
    let told = json_of(
        &blind
            .capture(rmcp::handler::server::wrapper::Parameters(rule(7)))
            .await
            .expect("the write answers"),
    );
    assert_ne!(
        told["status"], "blocked",
        "no refusal on a floor measured at the default: {told}"
    );
}

/// **A boot that names a bot nobody created reads no failed read.** The bot is
/// not there, so it carries no role and the fields of nothing are no failure: the
/// door answers with the roster, and the log says no read failed. Paired with a
/// bot that exists, whose boot reads its role without a failure line either.
#[tokio::test]
async fn a_boot_naming_a_bot_nobody_created_logs_no_failed_read() {
    use crate::orientation::OrientArgs;
    #[derive(Clone, Default)]
    struct Lines(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Lines {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("the log lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let boot = |bot: &str| OrientArgs {
        request_id: None,
        compacted: None,
        claim: None,
        timezone: None,
        bot: Some(bot.into()),
        brief: Some(true),
        skill: None,
        section: None,
        resume: None,
        sid: None,
        today: None,
        ask_for_code: None,
        challenge: None,
        code: None,
        offer_code: None,
    };
    let (healthy, _blind) = healthy_and_down(Down::FieldsOf("bot:gamma"));
    make_bot(&healthy, "gamma").await;
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let (unknown, known) = {
        let _for_this_thread = tracing::subscriber::set_default(subscriber);
        let unknown = json_of(
            &healthy
                .start_here(rmcp::handler::server::wrapper::Parameters(boot(
                    "no-such-bot",
                )))
                .await
                .expect("an unknown bot is an answer"),
        );
        let known = json_of(
            &healthy
                .start_here(rmcp::handler::server::wrapper::Parameters(boot("gamma")))
                .await
                .expect("the boot answers"),
        );
        (unknown, known)
    };
    assert!(unknown.get("identity").is_none(), "{unknown}");
    assert!(known["session"]["sid"].is_string(), "{known}");
    let log = String::from_utf8(lines.0.lock().expect("the log lock").clone()).expect("text");
    assert_eq!(log.matches("a read failed").count(), 0, "{log}");
}

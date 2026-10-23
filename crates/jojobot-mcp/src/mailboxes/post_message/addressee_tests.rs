//! **A post to an addressee that has nowhere to receive is refused with the move
//! that works from where the caller stands**: an alias names its bot, a bot with
//! no box is told to boot it, jojobot is told to tell the operator, and a name
//! nothing answers to keeps its old advice.

use super::*;
use crate::harness::*;
use crate::mailboxes::testing::*;

/// **A name that is a bot's alias is refused honestly** — named as an
/// alias, naming the bot it belongs to, and never advised to boot a bot
/// that cannot exist: the box is already open, under a different handle.
#[tokio::test]
async fn posting_to_a_bots_alias_names_the_bot_and_the_address_that_works() {
    let jojobot = mailbox_handler();
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            aliases: Some(vec!["Dev Two".into()]),
            ..crate::memory::testing::add_args("bot", "gamma", "gamma")
        }))
        .await
        .expect("add ok");

    let refused = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "dev-two".into(),
                sid: as_bot(&jojobot, "otto"),
                body: "reporting in".into(),
                subject: None,
                in_reply_to: None,
                supersedes: None,
            }))
            .await
            .expect("a bad address is an answer, not an error"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false);
    let advice = refused["how_to_proceed"]
        .as_str()
        .expect("advice is a string");
    assert!(
        advice.contains("alias") && advice.contains("bot:gamma"),
        "the refusal has to say this is an alias and name the bot it belongs to: {advice}",
    );
    assert!(
        !advice.contains("start_here"),
        "start_here cannot open a box for a name that was never a bot — the box it means \
         already exists, under a different handle: {advice}",
    );
}

/// **A bot that exists and holds no box is told so.** Its display name
/// spells its own handle's slug, so the alias comparison used to find it and
/// answer that its own handle "is the name of" itself. The refusal says the
/// bot has no mailbox and how one is made, and it does not call the handle
/// anything other than the bot's own.
#[tokio::test]
async fn posting_to_a_bot_that_has_no_box_says_it_has_no_mailbox() {
    let jojobot = mailbox_handler();
    make_box(&jojobot, "otto").await;
    jojobot
        .memory
        .add_entity(NewEntity::new(
            EntityId("bot:moe".into()),
            "Moe",
            "user-named",
        ))
        .await
        .expect("add ok")
        .written()
        .expect("nothing blocks a bot with a new name");

    let refused = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "moe".into(),
                sid: as_bot(&jojobot, "otto"),
                body: "reporting in".into(),
                subject: None,
                in_reply_to: None,
                supersedes: None,
            }))
            .await
            .expect("a bad address is an answer, not an error"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false);
    let advice = refused["how_to_proceed"]
        .as_str()
        .expect("advice is a string");
    assert!(
        advice.contains("bot:moe") && advice.contains("no mailbox"),
        "the refusal has to name the bot and say it has no mailbox: {advice}",
    );
    assert!(
        advice.contains("start_here"),
        "and it has to say how a box is made: {advice}",
    );
    assert!(
        !advice.contains("alias") && !advice.contains("not a bot's own handle"),
        "a bot's own handle is not an alias of itself: {advice}",
    );
}

/// **A reply to jojobot is told that nobody reads it and who to tell.** Mail
/// from jojobot is a release note; no bot named jojobot can be booted, so
/// "boot that bot" is a move that does not exist for the caller. The refusal
/// names the operator's handle as the way to reach a person. The paired
/// positive is the bot that exists without a box, which is still told to
/// boot it.
#[tokio::test]
async fn posting_to_jojobot_names_the_operator_and_not_a_boot() {
    let jojobot = mailbox_handler();
    make_box(&jojobot, "otto").await;

    let refused = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: crate::release_notes::SYSTEM_SENDER.into(),
                sid: as_bot(&jojobot, "otto"),
                body: "thanks".into(),
                subject: None,
                in_reply_to: None,
                supersedes: None,
            }))
            .await
            .expect("a bad address is an answer, not an error"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false);
    let advice = refused["how_to_proceed"]
        .as_str()
        .expect("advice is a string");
    assert!(
        advice.contains("operator"),
        "the refusal has to name who to tell instead: {advice}",
    );
    assert!(
        !advice.contains("start_here"),
        "no start_here can boot jojobot, so the refusal must not send the caller to one: \
         {advice}",
    );
}

/// **The genuinely-missing case reads exactly as it did before.** A name
/// nothing answers to — no bot, no alias — still gets the repair that
/// actually fits it.
#[tokio::test]
async fn posting_to_a_name_nothing_answers_to_still_gets_the_old_advice() {
    let jojobot = mailbox_handler();
    make_box(&jojobot, "otto").await;

    let refused = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "nobody-here".into(),
                sid: as_bot(&jojobot, "otto"),
                body: "reporting in".into(),
                subject: None,
                in_reply_to: None,
                supersedes: None,
            }))
            .await
            .expect("a bad address is an answer, not an error"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    let advice = refused["how_to_proceed"]
        .as_str()
        .expect("advice is a string");
    assert!(
        advice.contains("start_here"),
        "a name nothing answers to still points at the repair that fits it: {advice}",
    );
}

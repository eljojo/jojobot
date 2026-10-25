//! **Closing mail costs what reading it does.** Five ways the surface closes
//! more than one message at a time or without being asked, and the case beside
//! each that shows what it must not do.

use super::testing::*;
use super::*;
use crate::harness::*;
use jojobot_domain::mailbox::MessageState;

/// Post as `sender` to `to`, answering `in_reply_to` when given. The id of the
/// message posted comes back.
async fn post(jojobot: &Jojobot, to: &str, sender: &str, in_reply_to: Option<&str>) -> String {
    let result = post_receipt(jojobot, to, sender, in_reply_to).await;
    result["id"].as_str().expect("an id").to_string()
}

/// [`post`], with the whole receipt.
async fn post_receipt(
    jojobot: &Jojobot,
    to: &str,
    sender: &str,
    in_reply_to: Option<&str>,
) -> serde_json::Value {
    let result = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: to.into(),
                sid: as_bot(jojobot, sender),
                subject: None,
                body: format!("{sender} to {to}"),
                in_reply_to: in_reply_to.map(str::to_string),
                supersedes: None,
            }))
            .await
            .expect("post_message ok"),
    );
    assert_ne!(result["status"], "blocked", "{result}");
    result
}

/// A message as the store holds it now.
async fn held(jojobot: &Jojobot, id: &str) -> Message {
    jojobot
        .mailboxes
        .message_by_id(&MessageId(id.to_string()))
        .await
        .expect("the store reads")
        .expect("the message exists")
}

/// Take delivery of everything in `bot`'s box, so each message is read.
async fn read_everything(jojobot: &Jojobot, bot: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                counts_only: None,
                new_only: None,
                sid: Some(as_bot(jojobot, bot)),
            }))
            .await
            .expect("read_mailbox ok"),
    )
}

async fn close_many(
    jojobot: &Jojobot,
    bot: &str,
    ids: &[&str],
    notes: Option<&str>,
) -> serde_json::Value {
    json_of(
        &jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: String::new(),
                message_ids: Some(ids.iter().map(|id| id.to_string()).collect()),
                notes: notes.map(str::to_string),
                sid: Some(as_bot(jojobot, bot)),
                request_id: None,
                quarantine: None,
            }))
            .await
            .expect("mark_processed ok"),
    )
}

// ── (1) many close in one call ──────────────────────────────────────────────

/// **Three ids close in one call; one that was already processed is counted and
/// left alone.** The receipt names what the call closed and what it found closed.
#[tokio::test]
async fn a_batch_closes_every_message_it_names_and_leaves_a_closed_one_alone() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let first = post(&jojobot, "otto", "gamma", None).await;
    let second = post(&jojobot, "otto", "gamma", None).await;
    let third = post(&jojobot, "otto", "gamma", None).await;
    read_everything(&jojobot, "otto").await;
    close_many(&jojobot, "otto", &[&first], Some("handled alone")).await;

    let closed = close_many(
        &jojobot,
        "otto",
        &[&first, &second, &third],
        Some("handled"),
    )
    .await;
    assert_eq!(closed["processed"], 2, "{closed}");
    assert_eq!(
        closed["ids"],
        serde_json::json!([second, third]),
        "{closed}"
    );
    assert_eq!(
        closed["already_processed"],
        serde_json::json!([first]),
        "{closed}"
    );
    for id in [&first, &second, &third] {
        assert_eq!(held(&jojobot, id).await.state, MessageState::Processed);
    }
    assert_eq!(
        held(&jojobot, &first).await.notes.as_deref(),
        Some("handled alone"),
        "a message already closed keeps the outcome it was closed with"
    );
    assert_eq!(
        held(&jojobot, &second).await.notes.as_deref(),
        Some("handled")
    );
}

/// **One id in another box refuses the whole call, names it, and closes none.**
/// The pair of the case above: a batch that closed what it could and refused the
/// rest would leave the caller not knowing which.
#[tokio::test]
async fn a_batch_with_one_id_that_is_not_the_callers_closes_nothing() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    owning(&jojobot, "epsilon").await;
    let mine = post(&jojobot, "otto", "gamma", None).await;
    let theirs = post(&jojobot, "epsilon", "gamma", None).await;
    read_everything(&jojobot, "otto").await;

    let refused = close_many(&jojobot, "otto", &[&mine, &theirs], None).await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert!(
        refused.to_string().contains(&theirs),
        "the refusal names the id that is not the caller's: {refused}"
    );
    assert_ne!(held(&jojobot, &mine).await.state, MessageState::Processed);
    assert_ne!(held(&jojobot, &theirs).await.state, MessageState::Processed);
}

/// **A batch sent twice under one request_id is answered the second time with
/// the first receipt and writes nothing.** The ledger is attached to the store
/// and the handler, and the batch is sent as the dispatch sends a call that
/// carries an id, so the id is spent as it is on the real store: the second
/// answer is marked `repeat` and holds the first call's count. A batch that
/// never finished its id would close nothing the second time and carry no
/// `repeat`, so this case fails for it.
#[tokio::test]
async fn a_batch_sent_again_under_its_request_id_is_answered_with_its_first_receipt() {
    let ledger = jojobot_domain::request::testing::InMemoryRequests::new();
    let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
    mailboxes.attach_requests(ledger.clone());
    let jojobot = with_mailboxes(mailboxes).with_requests(ledger);
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let first = post(&jojobot, "otto", "gamma", None).await;
    let second = post(&jojobot, "otto", "gamma", None).await;
    read_everything(&jojobot, "otto").await;
    let sid = as_bot(&jojobot, "otto");
    let send = || async {
        let args = MarkProcessedArgs {
            message_ids: Some(vec![first.clone(), second.clone()]),
            notes: Some("handled".into()),
            sid: Some(sid.clone()),
            request_id: Some("close-the-pair".into()),
            ..Default::default()
        };
        let params = rmcp::model::CallToolRequestParams::new("mark_processed").with_arguments(
            serde_json::json!({ "request_id": "close-the-pair" })
                .as_object()
                .expect("an object")
                .clone(),
        );
        let (scope, _flight) = match jojobot.begin_request(&params, Some(&sid)).await {
            crate::request::Begun::Fresh(scope, flight) => (scope, flight),
            crate::request::Begun::Repeat(receipt) => return json_of(&receipt),
            _ => panic!("the id is new or spent"),
        };
        let ran = jojobot_domain::request::within(
            scope.clone(),
            jojobot.mark_processed(Parameters(args)),
        )
        .await
        .expect("mark_processed ok");
        let complete = rmcp::model::CallToolResponse::Complete(ran.clone());
        let _ = jojobot.end_request(&scope, &complete).await;
        json_of(&ran)
    };
    let once = send().await;
    assert_eq!(once["processed"], 2, "{once}");
    assert!(once.get("repeat").is_none(), "{once}");
    let again = send().await;
    assert_eq!(
        again["repeat"], true,
        "the resend is the first receipt: {again}"
    );
    assert_eq!(again["processed"], 2, "{again}");
    for id in [&first, &second] {
        assert_eq!(held(&jojobot, id).await.state, MessageState::Processed);
    }
}

// ── (2) a reply closes what it answers ──────────────────────────────────────

/// **A reply names the message it answers and processes it**, with a note that
/// names the reply. **Paired**: a reply to a message in another box changes
/// nothing there.
#[tokio::test]
async fn a_reply_closes_the_message_it_answers_in_the_repliers_own_box() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    owning(&jojobot, "epsilon").await;
    let asked = post(&jojobot, "otto", "gamma", None).await;
    let elsewhere = post(&jojobot, "epsilon", "gamma", None).await;
    read_everything(&jojobot, "otto").await;

    let answer = post(&jojobot, "gamma", "otto", Some(&asked)).await;
    let closed = held(&jojobot, &asked).await;
    assert_eq!(
        closed.state,
        MessageState::Processed,
        "the answered message closed"
    );
    assert!(
        closed
            .notes
            .as_deref()
            .unwrap_or_default()
            .contains(&answer),
        "its note names the reply: {:?}",
        closed.notes
    );

    // A reply to a message in someone else's box: nothing there moves.
    post(&jojobot, "gamma", "otto", Some(&elsewhere)).await;
    assert_ne!(
        held(&jojobot, &elsewhere).await.state,
        MessageState::Processed,
        "a reply closes only mail in the replier's own box"
    );
}

// ── (3) a sender's newer message closes its older READ ones on the thread ───

/// **A sender's newer message into a thread closes the sender's older messages
/// the reader has read, and leaves the ones still new.** The thread's own first
/// message is among the older ones: the sender started it, the reader read it,
/// and the follow-up closes it. **Paired**: an older message the reader has not
/// read stays new (`supersedes` replaces one by name), another sender's message
/// on the thread stays open, and so does the same sender's message on another
/// thread.
#[tokio::test]
async fn a_newer_message_on_a_thread_closes_the_senders_older_read_ones_and_no_others() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    owning(&jojobot, "epsilon").await;
    // The thread starts with gamma's own message in otto's box, which otto reads.
    let root = post(&jojobot, "otto", "gamma", None).await;
    let other_thread = post(&jojobot, "otto", "gamma", None).await;
    read_everything(&jojobot, "otto").await;

    // A follow-up into the thread closes the root the reader has read.
    let first = post(&jojobot, "otto", "gamma", Some(&root)).await;
    assert_eq!(held(&jojobot, &root).await.state, MessageState::Processed);
    assert_eq!(held(&jojobot, &first).await.state, MessageState::New);

    // otto reads `first`; another sender writes on the thread.
    read_everything(&jojobot, "otto").await;
    let other_sender = post(&jojobot, "otto", "epsilon", Some(&root)).await;
    read_everything(&jojobot, "otto").await;

    // `second` closes `first` (read); `third` finds `second` still new and leaves it.
    let second = post(&jojobot, "otto", "gamma", Some(&root)).await;
    assert_eq!(held(&jojobot, &first).await.state, MessageState::Processed);
    let third = post(&jojobot, "otto", "gamma", Some(&root)).await;
    assert_eq!(
        held(&jojobot, &second).await.state,
        MessageState::New,
        "an older message the reader has not read is not closed by a reply"
    );
    for open in [&other_sender, &other_thread] {
        assert_ne!(
            held(&jojobot, open).await.state,
            MessageState::Processed,
            "{open} is not an older message of the same sender on the thread"
        );
    }
    assert_eq!(held(&jojobot, &third).await.state, MessageState::New);
}

// ── (4) the leftovers block is a count and the oldest few ids ───────────────

/// **A pile of leftovers comes back as a count and its five oldest ids**, and says
/// how many ids it left out. **Paired**: with five or fewer, every id is named and
/// nothing is said to be left out.
#[tokio::test]
async fn the_leftovers_are_a_count_and_the_oldest_five_ids() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let mut ids = Vec::new();
    for _ in 0..8 {
        ids.push(post(&jojobot, "otto", "gamma", None).await);
    }
    read_everything(&jojobot, "otto").await;
    let again = json_of(
        &jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                counts_only: None,
                new_only: Some(true),
                sid: Some(as_bot(&jojobot, "otto")),
            }))
            .await
            .expect("read_mailbox ok"),
    );
    assert_eq!(again["leftovers"]["count"], 8, "{again}");
    assert_eq!(
        again["leftovers"]["ids"],
        serde_json::json!(ids[..5]),
        "{again}"
    );
    assert_eq!(again["leftovers"]["ids_left_out"], 3, "{again}");

    for id in &ids[..3] {
        close_many(&jojobot, "otto", &[id], None).await;
    }
    let fewer = json_of(
        &jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                counts_only: None,
                new_only: Some(true),
                sid: Some(as_bot(&jojobot, "otto")),
            }))
            .await
            .expect("read_mailbox ok"),
    );
    assert_eq!(fewer["leftovers"]["count"], 5, "{fewer}");
    assert!(fewer["leftovers"].get("ids_left_out").is_none(), "{fewer}");
}

// ── (5) a pile of read mail is told once per session ────────────────────────

async fn status_bar_of(jojobot: &Jojobot, sid: &str) -> serde_json::Value {
    let mut answered: CallToolResponse = CallToolResult::success(vec![ContentBlock::text(
        serde_json::json!({ "status": "ok" }).to_string(),
    )])
    .into();
    jojobot.add_status_bar(&mut answered, Some(sid)).await;
    let CallToolResponse::Complete(result) = &answered else {
        panic!("a completed call is what the block attaches to");
    };
    let text = result
        .content
        .first()
        .and_then(|block| block.as_text())
        .expect("one text block")
        .text
        .clone();
    serde_json::from_str::<serde_json::Value>(&text).expect("JSON")["status_bar"].clone()
}

/// **At twenty messages read and not closed the status bar says so, once.**
/// **Paired**: at nineteen it does not, and the second answer past twenty does not
/// say it again.
#[tokio::test]
async fn twenty_read_and_open_messages_are_told_once_and_nineteen_are_not() {
    let jojobot = mailbox_handler();
    let sid = owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    for _ in 0..19 {
        post(&jojobot, "otto", "gamma", None).await;
    }
    read_everything(&jojobot, "otto").await;
    let nineteen = status_bar_of(&jojobot, &sid).await;
    assert!(nineteen.get("mail_pile").is_none(), "{nineteen}");

    post(&jojobot, "otto", "gamma", None).await;
    read_everything(&jojobot, "otto").await;
    let twenty = status_bar_of(&jojobot, &sid).await;
    assert_eq!(twenty["mail_pile"]["read_and_not_closed"], 20, "{twenty}");
    assert!(
        twenty["mail_pile"]["say"]
            .as_str()
            .unwrap_or_default()
            .contains("message_ids"),
        "it names the call that closes them: {twenty}"
    );
    let again = status_bar_of(&jojobot, &sid).await;
    assert!(
        again.get("mail_pile").is_none(),
        "taught once per session: {again}"
    );
}

/// **The pile names the manager the bot reports to**, so the bot knows whom to
/// tell when the pile is real work. The manager adopts the bot, which is the one
/// way a bot with no manager gets one.
#[tokio::test]
async fn the_pile_says_whom_to_tell() {
    let jojobot = mailbox_handler();
    let sid = owning(&jojobot, "otto").await;
    let manager = owning(&jojobot, "gamma").await;
    crate::memory::testing::capture_as(
        &jojobot,
        &manager,
        crate::memory::capture::CaptureArgs {
            sets: Some([("reports_to".to_string(), "bot:gamma".to_string())].into()),
            ..crate::memory::testing::capture_args("bot:otto", "otto reports to gamma")
        },
    )
    .await;
    for _ in 0..20 {
        post(&jojobot, "otto", "gamma", None).await;
    }
    read_everything(&jojobot, "otto").await;
    let bar = status_bar_of(&jojobot, &sid).await;
    assert!(
        bar["mail_pile"]["say"]
            .as_str()
            .unwrap_or_default()
            .contains("bot:gamma"),
        "the pile names the manager: {bar}"
    );
}

// ── a close a post could not make is said, not passed over ──────────────────

/// **A reply whose close fails still lands, and the receipt names what stayed
/// open.** The store fails the retirement of the message the reply answers; the
/// post is posted, the answered message is still read, and `closed.failed` names it.
/// **Paired**: the same reply with no fault closes it and names no failure.
#[tokio::test]
async fn a_reply_whose_close_failed_says_so_in_its_receipt() {
    let (jojobot, board) = counting_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let asked = post(&jojobot, "otto", "gamma", None).await;
    read_everything(&jojobot, "otto").await;
    board.fail_the_next_mark_processed();
    let answer = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "gamma".into(),
                sid: as_bot(&jojobot, "otto"),
                subject: None,
                body: "otto to gamma".into(),
                in_reply_to: Some(asked.clone()),
                supersedes: None,
            }))
            .await
            .expect("post_message ok"),
    );
    assert!(answer["id"].is_string(), "the reply landed: {answer}");
    assert_eq!(
        answer["closed"]["failed"],
        serde_json::json!([asked]),
        "{answer}"
    );
    assert_eq!(held(&jojobot, &asked).await.state, MessageState::Read);

    // The same reply once the store is whole closes it and names no failure.
    let again = post(&jojobot, "gamma", "otto", Some(&asked)).await;
    assert_eq!(held(&jojobot, &asked).await.state, MessageState::Processed);
    let _ = again;
}

// ── a lookup that fails after the post landed is said, not read as "nothing" ─

/// A reply from `otto` to `gamma` answering `asked` (a message in otto own box),
/// sent after `arm` has set the fault on the store. The receipt comes back.
async fn a_reply_with_a_fault(
    arm: impl FnOnce(&crate::mailboxes::testing::CountingMailboxes),
) -> (Jojobot, String, serde_json::Value) {
    let (jojobot, board) = counting_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let asked = post(&jojobot, "otto", "gamma", None).await;
    read_everything(&jojobot, "otto").await;
    arm(&board);
    let answer = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "gamma".into(),
                sid: as_bot(&jojobot, "otto"),
                subject: None,
                body: "otto to gamma".into(),
                in_reply_to: Some(asked.clone()),
                supersedes: None,
            }))
            .await
            .expect("post_message ok"),
    );
    assert!(answer["id"].is_string(), "the reply landed: {answer}");
    (jojobot, asked, answer)
}

/// **A board that cannot be read after the reply landed leaves the answered
/// message open, and the receipt names it.** The first listing after the post is
/// the one that finds the replier own box. Paired with the case above in which the
/// same reply closes it.
#[tokio::test]
async fn a_reply_that_cannot_find_its_own_box_names_the_answered_message_as_still_open() {
    let (jojobot, asked, answer) =
        a_reply_with_a_fault(|board| board.fail_the_nth_listing_after_the_post(1)).await;
    assert_eq!(
        answer["closed"]["failed"],
        serde_json::json!([asked]),
        "{answer}"
    );
    assert_eq!(held(&jojobot, &asked).await.state, MessageState::Read);
}

/// **A close that was made is never also said to have failed**, and the older
/// messages that could not be looked up are said to be unchecked. The answered
/// message closes; the next read of the board, which says whether the box is a
/// person, fails: the receipt holds the answered id once, under `answered`, and
/// says the thread was not checked.
#[tokio::test]
async fn a_reply_that_closed_its_answer_and_could_not_check_the_thread_says_only_that() {
    for arm in [
        (|board: &crate::mailboxes::testing::CountingMailboxes| {
            board.fail_the_nth_listing_after_the_post(2)
        }) as fn(&crate::mailboxes::testing::CountingMailboxes),
        |board| board.fail_the_nth_read_of_a_message_after_the_post(2),
        |board| board.fail_the_read_of_a_senders_mail_after_the_post(),
    ] {
        let (jojobot, asked, answer) = a_reply_with_a_fault(arm).await;
        assert_eq!(
            answer["closed"]["answered"],
            serde_json::json!(asked),
            "{answer}"
        );
        assert!(
            answer["closed"].get("failed").is_none(),
            "the id is not claimed open: {answer}"
        );
        assert_eq!(answer["closed"]["older_unchecked"], true, "{answer}");
        assert_eq!(held(&jojobot, &asked).await.state, MessageState::Processed);
    }
}

/// **An older message whose thread cannot be traced is named as still open.**
/// gamma sends otto a message `root` and a read reply `older`, then a newer reply;
/// the trace of `older` fails. `root` closes, and `older` is named under `failed`.
#[tokio::test]
async fn an_older_message_whose_thread_could_not_be_traced_is_named_as_still_open() {
    let (jojobot, board) = counting_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let root = post(&jojobot, "otto", "gamma", None).await;
    let older = post(&jojobot, "otto", "gamma", Some(&root)).await;
    read_everything(&jojobot, "otto").await;
    // After the post: 1 reads what it answers, 2 finds the root, 3 traces `older`.
    board.fail_the_nth_read_of_a_message_after_the_post(3);
    let newer = post_receipt(&jojobot, "otto", "gamma", Some(&root)).await;
    assert_eq!(held(&jojobot, &older).await.state, MessageState::Read);
    assert_eq!(
        newer["closed"]["failed"],
        serde_json::json!([older]),
        "{newer}"
    );
    assert_eq!(
        newer["closed"]["older_on_the_thread"],
        serde_json::json!([root]),
        "what could be closed was: {newer}"
    );
}

// ── a batch note is the note that was stored ────────────────────────────────

/// **The batch receipt holds the notes as stored, and says when they were cut.**
/// A note one character over the budget is stored cut; the receipt returns the
/// stored text and announces the cut, as the single close does. **Paired**: a
/// note within the budget comes back as sent with no cut announced, and a batch
/// that closes nothing, every id being closed already, claims no note written.
#[tokio::test]
async fn a_batch_receipt_holds_the_notes_as_stored_and_says_when_they_were_cut() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let first = post(&jojobot, "otto", "gamma", None).await;
    let second = post(&jojobot, "otto", "gamma", None).await;
    let third = post(&jojobot, "otto", "gamma", None).await;
    read_everything(&jojobot, "otto").await;

    let over = "n".repeat(jojobot_domain::mailbox::NOTES_BUDGET + 1);
    let cut = close_many(&jojobot, "otto", &[&first, &second], Some(&over)).await;
    for id in [&first, &second] {
        let stored = held(&jojobot, id).await.notes.expect("the note is kept");
        assert_eq!(
            stored.chars().count(),
            jojobot_domain::mailbox::NOTES_BUDGET,
            "cut to the budget"
        );
        assert_eq!(
            cut["notes"],
            serde_json::json!(stored),
            "the receipt is what was stored: {cut}"
        );
    }
    assert!(cut.get("delta").is_some(), "the cut is announced: {cut}");

    let within = close_many(&jojobot, "otto", &[&third], Some("filed")).await;
    assert_eq!(within["notes"], "filed", "{within}");
    assert!(within.get("delta").is_none(), "{within}");

    let nothing = close_many(&jojobot, "otto", &[&first, &second], Some("again")).await;
    assert_eq!(nothing["processed"], 0, "{nothing}");
    assert!(
        nothing.get("notes").is_none(),
        "no note was written by this call: {nothing}"
    );
}

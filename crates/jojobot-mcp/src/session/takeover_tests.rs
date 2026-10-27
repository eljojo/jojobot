//! **A same-bot claim takes a role over from a run that no longer works.**
//! A run that wrapped but could not release its role is such a run: its key is
//! void and its lease is the only thing that holds the role.

use super::*;
use crate::harness::*;
use crate::memory::testing::{Down, healthy_and_down};
use crate::orientation::OrientArgs;
use crate::session::testing::{InMemorySessions, with_sessions};
use jojobot_domain::session::{SessionState, Sessions};
use std::sync::Arc;

/// A new run of `bot` that claims `role` at the door: the whole boot answer.
async fn claiming(jojobot: &Jojobot, bot: &str, role: &str) -> serde_json::Value {
    boot_new(jojobot, bot, Some(role)).await
}

/// A new run of `bot`, claiming `role` when one is named.
async fn boot_new(jojobot: &Jojobot, bot: &str, role: Option<&str>) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: role.map(Into::into),
                timezone: None,
                bot: Some(bot.into()),
                brief: None,
                skill: None,
                section: None,
                resume: Some("new".into()),
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok"),
    )
}

/// **A new run of `bot` that claims `role` at the operator's word**: the code is
/// asked for, read from the operator's box and presented on the boot, as the
/// operator types it into a session. The only way a claim takes a live role.
async fn claiming_at_the_operators_word(
    jojobot: &Jojobot,
    bot: &str,
    role: &str,
) -> serde_json::Value {
    crate::mailboxes::testing::name_the_operator(jojobot, "milhouse").await;
    let today = jojobot.clock().today_in(&jiff::tz::TimeZone::UTC);
    let asked = json_of(
        &jojobot
            .ask_for_code(
                &jojobot_domain::memory::EntityId(format!("bot:{bot}")),
                today,
            )
            .await
            .expect("ask answers"),
    );
    let handle = asked["challenge"]["handle"]
        .as_str()
        .unwrap_or_else(|| panic!("a challenge was made: {asked}"))
        .to_string();
    let mut mail = jojobot.mailboxes.scan_messages().await.expect("scan ok");
    mail.sort_by_key(|message| message.sent_at);
    let code = mail
        .iter()
        .rev()
        .find_map(|message| message.body.split("Code: ").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .expect("the code is in the operator's mail")
        .trim_end_matches('.')
        .to_string();
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: Some(role.into()),
                timezone: None,
                bot: Some(bot.into()),
                brief: None,
                skill: None,
                section: None,
                resume: Some("new".into()),
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: Some(handle),
                code: Some(code),
                offer_code: None,
            }))
            .await
            .expect("start_here ok"),
    )
}

async fn wrapped(jojobot: &Jojobot, sid: String) -> serde_json::Value {
    json_of(
        &jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "done with the dispatch for now".into(),
                sid,
            }))
            .await
            .expect("wrap ok"),
    )
}

/// **A run that wrapped with its role release failing no longer blocks a claim
/// by the next run of its bot**, which takes the role at once and names the
/// wrapped run it took it from. The pair is a wrap whose release landed: the
/// next claim finds nothing to take over.
#[tokio::test]
async fn a_wrapped_run_whose_release_failed_is_taken_over_like_an_open_one() {
    let (healthy, blind) = healthy_and_down(Down::RoleRelease);
    make_bot(&healthy, "gamma").await;

    let first = claiming(&blind, "gamma", "dev-dispatch").await;
    let first_run = run_of(&first).expect("a run");
    let wrap = wrapped(&blind, sid_of(&first).expect("a handle")).await;
    assert_eq!(
        wrap["roles"][0]["released"], false,
        "the release failed: {wrap}"
    );

    let second = claiming(&healthy, "gamma", "dev-dispatch").await;
    assert_eq!(second["session"]["claim"]["status"], "taken", "{second}");
    assert_eq!(
        second["session"]["claim"]["took_over_from"], first_run,
        "{second}"
    );

    let (healthy, _) = healthy_and_down(Down::RoleRelease);
    make_bot(&healthy, "gamma").await;
    let first = claiming(&healthy, "gamma", "dev-dispatch").await;
    let wrap = wrapped(&healthy, sid_of(&first).expect("a handle")).await;
    assert_eq!(wrap["roles"][0]["released"], true, "{wrap}");
    let second = claiming(&healthy, "gamma", "dev-dispatch").await;
    assert_lease_had_lapsed(&second);
}

/// One journal beat by the run behind `sid`.
async fn journalled(jojobot: &Jojobot, sid: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .journal(Parameters(JournalArgs {
                request_id: None,
                working_on: None,
                entry: "read the hand-off".into(),
                focus: None,
                sid: sid.into(),
            }))
            .await
            .expect("an answer"),
    )
}

/// **A run the sweep closed is refused without any displacing run.** A run that
/// the operator's code displaced is refused naming the run that took its role; that
/// half is a story (`role_takeover`), because only the operator's code can displace
/// an active run.
#[tokio::test]
async fn a_swept_run_is_refused_naming_no_displacing_run() {
    let store = Arc::new(InMemorySessions::new());
    let jojobot = with_sessions(store.clone());
    make_bot(&jojobot, "gamma").await;
    // A run the sweep closed: no displacing run, so no run is named.
    let quiet = sid_of(&boot_new(&jojobot, "gamma", None).await).expect("a handle");
    journalled(&jojobot, &quiet).await;
    let card = store
        .sessions_of(&jojobot_domain::memory::EntityId("bot:gamma".into()))
        .await
        .expect("list ok")
        .into_iter()
        .find(|run| {
            run.sid
                .as_ref()
                .is_some_and(|sid| sid.as_str() == run_name(&jojobot, &quiet))
        })
        .expect("the quiet run has a card");
    store
        .close(&card.id, SessionState::Abandoned)
        .await
        .expect("the sweep closes it");
    let swept = journalled(&jojobot, &quiet).await;
    assert_eq!(swept["status"], "blocked", "{swept}");
    assert!(
        !swept["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("was displaced")),
        "a swept run names no displacing run: {swept}"
    );
}

/// **A holder is told it was displaced only when its role moved.** The release of
/// the holder's role fails, so the claimant is refused and the role stays where it
/// was: the holder is not closed, and its next verb lands and is not told another
/// run took its role. The pair is the case above, where the release lands.
#[tokio::test]
async fn a_holder_is_not_displaced_when_the_claimants_release_of_its_role_fails() {
    let (healthy, blind) = healthy_and_down(Down::RoleRelease);
    make_bot(&healthy, "gamma").await;

    let first = claiming(&healthy, "gamma", "dev-dispatch").await;
    let first_sid = sid_of(&first).expect("a handle");
    let second = claiming(&blind, "gamma", "dev-dispatch").await;
    assert_eq!(
        second["session"]["claim"]["status"], "refused",
        "the role did not move, so the claim is refused: {second}"
    );

    let still_working = journalled(&healthy, &first_sid).await;
    assert_ne!(
        still_working["status"], "blocked",
        "the holder was closed as displaced though its role never moved: {still_working}"
    );
}

/// The run behind `sid` claims one more role, by its live handle.
async fn claiming_more(jojobot: &Jojobot, bot: &str, sid: &str, role: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: Some(role.into()),
                timezone: None,
                bot: Some(bot.into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: Some(sid.into()),
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok"),
    )
}

/// **A holder that lost the claimed role is told, though another role of its own
/// stayed held.** The holder has two roles and the release of one of them fails.
/// The claimed role moved, so the claim is taken in the same call and the holder
/// is closed as displaced at once: it is never left running on a role the
/// claimant now owns. The case before it is the pair: no role moved, so the claim
/// is refused and the holder is not told.
#[tokio::test]
async fn a_holder_that_lost_only_the_claimed_role_is_told_at_once() {
    let (healthy, blind) = healthy_and_down(Down::RoleReleaseExcept("dev-dispatch"));
    make_bot(&healthy, "gamma").await;

    let first = claiming(&healthy, "gamma", "dev-dispatch").await;
    let first_sid = sid_of(&first).expect("a handle");
    let more = claiming_more(&healthy, "gamma", &first_sid, "dev-review").await;
    assert_eq!(more["session"]["claim"]["status"], "taken", "{more}");

    let second = claiming_at_the_operators_word(&blind, "gamma", "dev-dispatch").await;
    assert_eq!(
        second["session"]["claim"]["status"], "taken",
        "the claimed role moved, so it is taken: {second}"
    );
    let told = journalled(&healthy, &first_sid).await;
    assert_eq!(
        told["status"], "blocked",
        "the holder lost the role the claimant owns and was not told: {told}"
    );
}

/// **A displaced close that cannot be kept is told to the claimant and does not
/// refuse it.** Every role of the holder was released, so the role is free and
/// the claim lands. The holder could not be marked displaced, and the answer says
/// so, because the holder still runs and has no other way to learn it.
#[tokio::test]
async fn a_holder_whose_close_cannot_be_kept_is_named_to_the_claimant() {
    let store = Arc::new(InMemorySessions::new());
    let jojobot = with_sessions(store.clone());
    make_bot(&jojobot, "gamma").await;

    let first = claiming(&jojobot, "gamma", "dev-dispatch").await;
    let first_run = run_of(&first).expect("a run");
    store.refuse_displaced_close(true);
    let second = claiming_at_the_operators_word(&jojobot, "gamma", "dev-dispatch").await;
    assert_eq!(
        second["session"]["claim"]["status"], "taken",
        "every role was released, so the role is free and the claim lands: {second}"
    );
    assert_eq!(
        second["session"]["claim"]["holder_still_active"], first_run,
        "the answer names the holder that could not be closed: {second}"
    );
    assert!(
        !second["session"]["claim"]["note"]
            .as_str()
            .is_some_and(|note| note.contains("is now abandoned")),
        "the note does not call a holder abandoned that is still active: {second}"
    );
}

/// **A holder keeps the claimed role when only another role of its own was
/// released.** The release of `dev-review` lands and the release of the claimed
/// `dev-dispatch` fails, so the claimant is refused and the holder is not closed
/// as displaced: it still holds what the claimant asked for. The case before it is
/// the pair, where the claimed role is the one that moves.
#[tokio::test]
async fn a_holder_that_kept_the_claimed_role_is_not_closed_when_another_role_moved() {
    let (healthy, blind) = healthy_and_down(Down::RoleReleaseOnly("dev-dispatch"));
    make_bot(&healthy, "gamma").await;

    let first = claiming(&healthy, "gamma", "dev-dispatch").await;
    let first_sid = sid_of(&first).expect("a handle");
    let more = claiming_more(&healthy, "gamma", &first_sid, "dev-review").await;
    assert_eq!(more["session"]["claim"]["status"], "taken", "{more}");

    let second = claiming(&blind, "gamma", "dev-dispatch").await;
    assert_eq!(
        second["session"]["claim"]["status"], "refused",
        "the claimed role did not move, so the claim is refused: {second}"
    );
    let still = journalled(&healthy, &first_sid).await;
    assert_ne!(
        still["status"], "blocked",
        "the holder still holds the claimed role and was closed as displaced: {still}"
    );
}

//! **A same-bot claim takes a role over from a run that no longer works.**
//! A run that wrapped but could not release its role is such a run: its key is
//! void and its lease is the only thing that holds the role.

use super::*;
use crate::harness::*;
use crate::memory::testing::{Down, healthy_and_down};
use crate::orientation::OrientArgs;
use crate::session::testing::*;

/// A new run of `bot` that claims `role` at the door: the whole boot answer.
async fn claiming(jojobot: &Jojobot, bot: &str, role: &str) -> serde_json::Value {
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
                challenge: None,
                code: None,
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
    assert_lease_was_held(&second, &first_run);

    let (healthy, _) = healthy_and_down(Down::RoleRelease);
    make_bot(&healthy, "gamma").await;
    let first = claiming(&healthy, "gamma", "dev-dispatch").await;
    let wrap = wrapped(&healthy, sid_of(&first).expect("a handle")).await;
    assert_eq!(wrap["roles"][0]["released"], true, "{wrap}");
    let second = claiming(&healthy, "gamma", "dev-dispatch").await;
    assert_lease_had_lapsed(&second);
}

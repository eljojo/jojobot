//! The wrap's account of the roles it released, kept apart from the other cases of
//! the verb so the file holds its size.

use super::*;

/// 🚨 **A wrap whose role release fails still lands, and says the role is
/// still held and until when.** The release is written after the close, so
/// it cannot undo the wrap; swallowed, it left the role held for a run whose
/// key is void, and the next claim was refused for the rest of the lease
/// with nobody told why. `held_until` is the claim moment plus the lease,
/// read from the role object the claim wrote.
#[tokio::test]
async fn a_wrap_whose_release_fails_says_the_role_is_still_held_and_until_when() {
    let (healthy, blind) =
        crate::memory::testing::healthy_and_down(crate::memory::testing::Down::RoleRelease);
    make_bot(&healthy, "gamma").await;
    let sid = claiming(&blind, "gamma", "dev-dispatch").await;

    let answer = wrapped(&blind, sid).await;
    assert_eq!(
        answer["session"]["state"], "wrapped",
        "the wrap landed: {answer}"
    );
    let roles = answer["roles"]
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"));
    assert_eq!(roles.len(), 1, "{answer}");
    assert_eq!(roles[0]["role"], "dev-dispatch", "{answer}");
    assert_eq!(roles[0]["released"], false, "{answer}");
    assert!(roles[0]["reason"].is_string(), "{answer}");

    let held = json_of(
        &healthy
            .recall(Parameters(recall_args("role:dev-dispatch")))
            .await
            .expect("recall ok"),
    );
    let fields = &held["objects"][0]["fields"];
    let claimed_at: jiff::Timestamp = fields["claimed_at"]
        .as_str()
        .and_then(|at| at.parse().ok())
        .unwrap_or_else(|| panic!("the claim moment is still held: {held}"));
    let until: jiff::Timestamp = roles[0]["held_until"]
        .as_str()
        .and_then(|at| at.parse().ok())
        .unwrap_or_else(|| panic!("a moment: {answer}"));
    assert_eq!(
        until.duration_since(claimed_at).as_secs(),
        45 * 60,
        "{answer}"
    );
}

/// **The same wrap over a healthy store says the role was released**, and a
/// run that held no role says nothing about roles.
#[tokio::test]
async fn a_wrap_says_it_released_its_role_and_a_run_with_none_says_nothing() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let sid = claiming(&jojobot, "gamma", "dev-dispatch").await;

    let answer = wrapped(&jojobot, sid).await;
    let roles = answer["roles"]
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"));
    assert_eq!(roles.len(), 1, "{answer}");
    assert_eq!(roles[0]["role"], "dev-dispatch", "{answer}");
    assert_eq!(roles[0]["released"], true, "{answer}");
    assert!(roles[0].get("held_until").is_none(), "{answer}");

    let none = wrapped(&jojobot, booted(&jojobot, "gamma").await).await;
    assert_eq!(none["session"]["state"], "wrapped", "{none}");
    assert!(none.get("roles").is_none(), "{none}");
}

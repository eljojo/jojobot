//! **The fixture recorded from the build deploy 8.1 shipped (ed5f0e47)**, the
//! build the operator runs now. It holds two shapes the older recordings lack: a
//! piece of work whose `depends_on` was rewritten over several records, and a
//! type a caller declared with the key `subject`, which a later build ships as a
//! describing key.

use super::*;

/// The fixture recorded from the build deploy 8.1 shipped.
pub(super) const DEPLOY_8_1_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy8_1";

#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_8_1_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_8_1_FIXTURE_DIR, "boots-deploy8-1").await;
}

/// **Boots the head over the recording and returns what a case needs to ask it.**
async fn head_over_the_deploy_8_1_store(label: &str) -> (Scratch, Booted, Surface) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_8_1_FIXTURE_DIR, label).await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    (scratch, booted, surface)
}

/// The handles a walk reached, read out of a `recall` answer.
fn walked(answer: &str) -> Vec<String> {
    let parsed: serde_json::Value = serde_json::from_str(answer).expect("a recall answer is JSON");
    let mut reached: Vec<String> = parsed["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|object| object["connected"].as_array().into_iter().flatten())
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect();
    reached.sort();
    reached
}

/// **A dependency list the deployed build rewrote over three records reads on
/// the head as the newest write holds it.** The recording writes the list of
/// `work:epsilon` three times, from three pieces of work down to one. The walk
/// out reaches the one left, the walk in reaches the holder from it and from no
/// piece the list dropped, and the ready view names the same one blocker.
#[tokio::test]
async fn a_dependency_list_rewritten_over_three_records_walks_as_the_newest_holds_it() {
    let (scratch, booted, surface) = head_over_the_deploy_8_1_store("walk-deploy8-1").await;
    let walk = |subject: &'static str, direction: &'static str| {
        surface.call(
            "recall",
            json!({"subject": subject,
                   "follow": {"relation": "depends_on", "direction": direction}}),
        )
    };
    let out = walk("work:epsilon", "out").await;
    let in_from_delta = walk("work:delta", "in").await;
    let in_from_gamma = walk("work:gamma", "in").await;
    let in_from_prior = walk("work:upgrade-fixture-prior", "in").await;
    let ready = surface
        .call(
            "recall",
            json!({"view": "ready", "subject": "work:epsilon"}),
        )
        .await;
    booted.stop().await;
    assert_eq!(
        walked(&out),
        ["work:delta"],
        "the walk out reaches only what the newest write holds: {out}"
    );
    assert!(
        walked(&in_from_delta).contains(&"work:epsilon".to_string()),
        "the walk in from the one piece left reaches the holder: {in_from_delta}"
    );
    for (dropped, answer) in [("gamma", &in_from_gamma), ("prior", &in_from_prior)] {
        assert!(
            !walked(answer).contains(&"work:epsilon".to_string()),
            "the walk in from {dropped}, which a later write dropped, reaches the holder: {answer}"
        );
    }
    let blocked: serde_json::Value = serde_json::from_str(&ready).expect("the view answers JSON");
    let blockers: Vec<&str> = blocked["objects"][0]["blocked_by"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|blocker| blocker["handle"].as_str())
        .collect();
    assert_eq!(
        blockers,
        ["work:delta"],
        "the ready view names the one blocker the walk reaches: {ready}"
    );
    drop(scratch);
}

/// **A type a caller declared with the key `subject` still answers on the head.**
/// The recording build accepts the key as an ordinary one; the head ships
/// `subject` as a key that describes a claim, and refuses a new declaration of
/// it. A read never judges a type already kept, so the question is answered: no
/// thing carries a key that only labels a claim, so the answer holds none, and it
/// says which keys the type names.
#[tokio::test]
async fn a_stored_type_naming_a_describing_key_answers_on_the_head() {
    let (scratch, booted, surface) = head_over_the_deploy_8_1_store("labelled-deploy8-1").await;
    let asked = surface
        .call(
            "recall",
            json!({"answers_type": "upgrade-fixture-labelled"}),
        )
        .await;
    booted.stop().await;
    let answer: serde_json::Value = serde_json::from_str(&asked).expect("the answer is JSON");
    assert_ne!(answer["status"], "blocked", "{asked}");
    assert_eq!(answer["count"], 0, "{asked}");
    assert!(
        asked.contains("upgrade-fixture-labelled") && asked.contains("\"subject\""),
        "the empty answer names the type and the key it names: {asked}"
    );
    drop(scratch);
}

//! "My agent is still writing and the next run wants its role."
//!
//! A role changes hands only when its holder wraps, when its lease runs out, or
//! at the operator's word. A boot of the same bot is refused while its own earlier
//! run holds the role, and a code the operator typed takes it and says so.

use serde_json::json;

use super::chain_of_trust::{an_instance_with_an_operator, ask_and_read};
use super::dsl::Story;

/// The state a run is listed in.
fn state_in(runs: &serde_json::Value, run: &str) -> Option<String> {
    runs["runs"]
        .as_array()?
        .iter()
        .find(|r| r["run"] == run)
        .and_then(|r| r["state"].as_str().map(str::to_string))
}

/// The holder of `role:carpool`, as the role object reads now. **The role must
/// answer**: a recall that errors, is blocked or names another object panics,
/// so an absent holder means the role holds nobody and not that the read failed.
async fn holder_of(asker: &super::dsl::Session) -> Option<String> {
    let read = asker
        .call("recall", json!({"subject": "role:carpool"}))
        .await
        .json();
    assert_eq!(
        read["objects"][0]["id"], "role:carpool",
        "the role answers the recall: {read}"
    );
    read["objects"][0]["fields"]["holder"]
        .as_str()
        .map(str::to_string)
}

fn claim_args() -> serde_json::Value {
    json!({"bot": "gamma", "brief": true, "claim": "carpool", "resume": "new"})
}

/// **A new boot is refused while its own earlier run holds the role, and the
/// holder's wrap frees it at once.** The refusal names the holder's run, when it
/// last wrote, when the lease runs out and what frees the role. Another bot is
/// refused the same way, so the exemption for the same bot is gone and nothing else
/// changed.
#[tokio::test]
async fn a_new_boot_is_refused_while_its_own_earlier_run_holds_the_role() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;

    let (first, first_run) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "carpool"}),
        )
        .await;
    let first_run = first_run.expect("the first boot hands back a run");
    assert_eq!(first.json()["session"]["claim"]["status"], "taken");
    first_run
        .call(
            "journal",
            json!({"entry": "took the carpool", "focus": "x"}),
        )
        .await;

    // ── a new boot of the same bot is refused, naming the holder ────────────
    let (second, _) = story.call("start_here", claim_args()).await;
    let body = second.json();
    let claim = &body["session"]["claim"];
    assert_eq!(claim["status"], "refused", "{claim}");
    assert_eq!(claim["holder"], first_run.run(), "{claim}");
    assert!(claim.get("took_over_from").is_none(), "{claim}");
    let how = claim["how_to_proceed"].as_str().expect("a way forward");
    for word in ["last wrote", "lease", "wraps", "ask_for_code"] {
        assert!(how.contains(word), "the refusal says {word}: {how}");
    }
    assert_eq!(holder_of(&s).await.as_deref(), Some(first_run.run()));
    let runs = first_run.call("list_runs", json!({})).await.json();
    assert_eq!(
        state_in(&runs, first_run.run()).as_deref(),
        Some("active"),
        "the holder was not touched"
    );

    // ── the holder's wrap frees the role at once ────────────────────────────
    let held = first_run.run().to_string();
    first_run.wrap("done with the carpool").await;
    let (third, third_run) = story.call("start_here", claim_args()).await;
    let body = third.json();
    let claim = &body["session"]["claim"];
    assert_eq!(claim["status"], "taken", "{claim}");
    assert!(claim.get("took_over_from").is_none(), "{claim}");
    assert_ne!(holder_of(&s).await.as_deref(), Some(held.as_str()));
    drop(third_run);
    s.wrap("held a role").await;
    story.finish().await;
}

/// **The operator's code takes a live role and says so.** Gamma's first run holds
/// the role and writes. The operator's code for gamma is typed into a new run,
/// which claims the role: it takes it, the old run reads abandoned by the
/// operator's word with the challenge named, and a live run that held no lease
/// keeps running. The same code, spent, takes nothing.
#[tokio::test]
async fn the_operators_code_takes_a_live_role_and_the_old_run_reads_abandoned_by_the_word() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    an_instance_with_an_operator(&s).await;
    s.add("bot:gamma", "Gamma").await;

    let bystander = story.as_bot("bot:gamma").await;
    bystander
        .call("journal", json!({"entry": "idle", "focus": "x"}))
        .await;
    let (_, first_run) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "carpool", "resume": "new"}),
        )
        .await;
    let first_run = first_run.expect("a run");
    first_run
        .call("journal", json!({"entry": "holding", "focus": "x"}))
        .await;

    let (challenge, code) = ask_and_read(&story, "gamma").await;
    let mut with_code = claim_args();
    with_code["challenge"] = json!(challenge);
    with_code["code"] = json!(code);
    let (taken, taker) = story.call("start_here", with_code.clone()).await;
    let body = taken.json();
    let claim = &body["session"]["claim"];
    assert_eq!(claim["status"], "taken", "{claim}");
    assert_eq!(claim["took_over_from"], first_run.run(), "{claim}");
    assert_eq!(claim["abandoned_by"], "the operator's word", "{claim}");
    assert_eq!(claim["challenge"], challenge, "{claim}");
    let taker = taker.expect("the coded boot hands back a run");
    assert_eq!(holder_of(&s).await.as_deref(), Some(taker.run()));
    // The displaced run is refused naming the run that took its role.
    first_run
        .refused("journal", json!({"entry": "still here", "focus": "x"}))
        .await
        .says(taker.run());
    let runs = bystander.call("list_runs", json!({})).await.json();
    assert_eq!(
        state_in(&runs, first_run.run()).as_deref(),
        Some("abandoned")
    );
    assert_eq!(
        state_in(&runs, bystander.run()).as_deref(),
        Some("active"),
        "a live run that held no lease keeps running"
    );

    // ── the spent code takes nothing ────────────────────────────────────────
    story.refused_at_the_door("start_here", with_code).await;
    assert_eq!(holder_of(&s).await.as_deref(), Some(taker.run()));
    s.wrap("took a role at the operator's word").await;
    story.finish().await;
}

/// **A run the sweep abandons gives up its role.** The lease ran out long ago,
/// so a claim would succeed either way; what the sweep changes is that the role
/// no longer names a holder.
#[tokio::test]
async fn a_swept_run_no_longer_holds_its_role() {
    let story = Story::begin_on_the_real_lease("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let (claimed, run) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "carpool"}),
        )
        .await;
    assert_eq!(claimed.json()["session"]["claim"]["status"], "taken");
    let run = run.expect("a run");
    run.call(
        "journal",
        json!({"entry": "took the carpool", "focus": "x"}),
    )
    .await;
    assert_eq!(holder_of(&s).await.as_deref(), Some(run.run()));

    let two_days_on = jiff::Timestamp::now() + jiff::SignedDuration::from_hours(48);
    let later = story
        .restarted_on_the_real_lease_pretending_it_is(
            &two_days_on
                .to_zoned(jiff::tz::TimeZone::UTC)
                .date()
                .to_string(),
        )
        .await;
    // Booting gamma sweeps its quiet run.
    let reader = later.as_bot("bot:gamma").await;
    assert_eq!(
        holder_of(&reader).await,
        None,
        "the swept run gave the role up"
    );
    later.finish().await;
}

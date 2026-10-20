//! "I archived the agent mid-run and the next run could not take its role."
//!
//! A boot of a bot that claims a role takes it from that bot's own earlier run,
//! which it abandons, instead of waiting out the lease. Another bot is still
//! refused while the lease is fresh, the abandoned run can be resumed, and
//! resuming it does not take the lease back.

use serde_json::json;

use super::dsl::Story;

/// The state a run is listed in.
fn state_in(runs: &serde_json::Value, run: &str) -> Option<String> {
    runs["runs"]
        .as_array()?
        .iter()
        .find(|r| r["run"] == run)
        .and_then(|r| r["state"].as_str().map(str::to_string))
}

/// The holder of `role:carpool`, as the role object reads now.
async fn holder_of(story: &Story, asker: &super::dsl::Session) -> Option<String> {
    let _ = story;
    let read = asker
        .call("recall", json!({"subject": "role:carpool"}))
        .await
        .json();
    read["objects"][0]["fields"]["holder"]
        .as_str()
        .map(str::to_string)
}

#[tokio::test]
async fn a_new_boot_takes_the_role_from_its_own_earlier_run() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;

    // ── the first run claims the role and writes ────────────────────────────
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
    assert_eq!(
        holder_of(&story, &s).await.as_deref(),
        Some(first_run.run())
    );

    // ── a new boot of the same bot claims it and is given it ────────────────
    let (second, second_run) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "carpool", "resume": "new"}),
        )
        .await;
    let second = second.json();
    let second_run = second_run.expect("the second boot hands back a run");
    assert_eq!(second["session"]["claim"]["status"], "taken", "{second}");
    assert_eq!(
        second["session"]["claim"]["took_over_from"],
        first_run.run(),
        "the answer names the run it abandoned: {second}"
    );
    assert_eq!(
        holder_of(&story, &s).await.as_deref(),
        Some(second_run.run()),
        "the lease is the new run's"
    );
    let runs = second_run.call("list_runs", json!({})).await.json();
    let state_of = |run: &str| state_in(&runs, run);
    assert_eq!(state_of(first_run.run()).as_deref(), Some("abandoned"));
    assert_eq!(state_of(second_run.run()).as_deref(), Some("active"));

    // ── another bot is refused while the new lease is fresh ─────────────────
    let (rival, _) = story
        .call(
            "start_here",
            json!({"bot": "delta", "brief": true, "claim": "carpool"}),
        )
        .await;
    assert_eq!(rival.json()["session"]["claim"]["status"], "refused");

    // ── resuming the abandoned run does not take the lease back ─────────────
    let (resumed, _) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": first_run.run()}),
        )
        .await;
    assert_eq!(resumed.json()["session"]["resumed"], true);
    assert_eq!(
        holder_of(&story, &s).await.as_deref(),
        Some(second_run.run()),
        "a resume claims nothing"
    );
    s.wrap("took a role over").await;
    story.finish().await;
}

/// **Only the run that holds the lease is abandoned**: another live run of the
/// same bot, one that holds no role, keeps running.
#[tokio::test]
async fn a_takeover_abandons_only_the_run_that_holds_the_lease() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;

    let holder = story.as_bot("bot:gamma").await;
    holder
        .call("journal", json!({"entry": "idle", "focus": "x"}))
        .await;
    let (first, first_run) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "carpool", "resume": "new"}),
        )
        .await;
    assert_eq!(first.json()["session"]["claim"]["status"], "taken");
    let first_run = first_run.expect("a run");
    first_run
        .call("journal", json!({"entry": "holding", "focus": "x"}))
        .await;

    let (second, _) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "carpool", "resume": "new"}),
        )
        .await;
    assert_eq!(second.json()["session"]["claim"]["status"], "taken");

    let runs = holder.call("list_runs", json!({})).await.json();
    let state_of = |run: &str| state_in(&runs, run);
    assert_eq!(state_of(first_run.run()).as_deref(), Some("abandoned"));
    assert_eq!(
        state_of(holder.run()).as_deref(),
        Some("active"),
        "a live run that held no lease keeps running"
    );
    s.wrap("took a role over").await;
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
    assert_eq!(holder_of(&story, &s).await.as_deref(), Some(run.run()));

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
        holder_of(&later, &reader).await,
        None,
        "the swept run gave the role up"
    );
    later.finish().await;
}

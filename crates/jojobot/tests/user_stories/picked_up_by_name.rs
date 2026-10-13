//! "A different agent took my run over. Does it get taught what I was taught?"
//!
//! **A run picked up by its name is a new agent holding an old run.** The agent
//! that held the key is gone, and whoever names the run after it went quiet has
//! none of its context. What jojobot taught the run and what it showed the run
//! were true of the old agent, so the pick-up forgets both and the next answers
//! teach and show as to a fresh session. A wrap code is the same sitting coming
//! back, and a call with the live key is the same agent: neither forgets.

use std::time::Duration;

use serde_json::json;

use super::dsl::{Session, Story};

/// A lease short enough to wait out, long enough that a run mid-story is not
/// yet quiet.
const LEASE: Duration = Duration::from_secs(1);

/// The lease and a margin: the run has been quiet for a full lease by the
/// server's clock when this has passed.
async fn wait_out_the_lease() {
    tokio::time::sleep(LEASE + Duration::from_millis(600)).await;
}

/// The teaching about claims, whose words are not asserted: it rides a
/// capture's receipt under `teaching`, and what is read is its subject.
const CLAIMS_TEACHING: &str = "does not destroy the one already there";

async fn capture(who: &Session, content: &str) -> super::dsl::Answer {
    who.call(
        "capture",
        json!({"subject": "person:lisa", "content": content, "provenance": "testimony"}),
    )
    .await
}

/// The run's pick-up by name: a boot naming the run, after it went quiet.
async fn pick_up(story: &Story, bot: &str, run: &str) -> (super::dsl::Answer, Session) {
    let (answer, taker) = story
        .call(
            "start_here",
            json!({"bot": bot, "brief": true, "resume": run}),
        )
        .await;
    (
        answer,
        taker.expect("a quiet run is handed to whoever names it, with a new key"),
    )
}

/// A run wrapped and reopened by the code its wrap handed back.
async fn reopen_by_code(story: &Story, who: &Session, bot: &str) -> (super::dsl::Answer, Session) {
    let wrapped = who
        .call("wrap_session", json!({"story": "one sitting, wrapped"}))
        .await
        .json();
    let code = wrapped["wrap_code"]
        .as_str()
        .unwrap_or_else(|| panic!("a wrap hands back a code: {wrapped}"))
        .to_string();
    let (answer, reopened) = story
        .call(
            "start_here",
            json!({"bot": bot, "brief": true, "resume": code}),
        )
        .await;
    (
        answer,
        reopened.expect("the code hands back a key for the wrapped run"),
    )
}

#[tokio::test]
async fn a_pick_up_by_name_is_taught_afresh_and_a_wrap_code_reopen_is_not() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;

    // ── the old sitting is taught once, and the live key is not taught again ──
    let gamma = story.as_bot("bot:gamma").await;
    let (name, key) = (gamma.run().to_string(), gamma.sid().to_string());
    capture(&gamma, "orders the pie")
        .await
        .says(CLAIMS_TEACHING);
    capture(&gamma, "orders the pie again")
        .await
        .never_says(CLAIMS_TEACHING);
    let delta = story.as_bot("bot:delta").await;
    capture(&delta, "orders the donut")
        .await
        .says(CLAIMS_TEACHING);

    // ── the same sitting coming back by its wrap code is not taught again ────
    let (reopened_boot, reopened) = reopen_by_code(&story, &delta, "delta").await;
    assert_eq!(reopened.run(), delta.run(), "the same run");
    assert!(
        reopened_boot.json()["session"]["shown_and_taught"].is_null(),
        "a reopen forgets nothing and does not say it did: {}",
        reopened_boot.json()
    );
    capture(&reopened, "orders the donut once more")
        .await
        .never_says(CLAIMS_TEACHING);

    // ── a different agent taking a quiet run by its name is taught afresh ────
    wait_out_the_lease().await;
    let (taken, taker) = pick_up(&story, "gamma", &name).await;
    assert_eq!(taker.run(), name, "the run keeps its name");
    assert_ne!(taker.sid(), key, "…and gets a new key");
    assert!(
        taken.json()["session"]["shown_and_taught"].is_string(),
        "the pick-up says what it forgot: {}",
        taken.json()
    );
    capture(&taker, "orders the pie, taken over")
        .await
        .says(CLAIMS_TEACHING);
    // The new agent is taught once like any session, not on every call.
    capture(&taker, "and the pie again")
        .await
        .never_says(CLAIMS_TEACHING);

    story.finish().await;
}

#[tokio::test]
async fn a_pick_up_by_name_forgets_what_the_run_was_shown_and_a_wrap_code_reopen_does_not() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("place:wonder-wharf", "Wonder Wharf").await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    let create = async |who: &Session, handle: &str, name: &str| {
        let (kind, slug) = handle.split_once(':').expect("a handle is kind:slug");
        who.call(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named"}),
        )
        .await
    };
    // The question a creation is asked, named by the verb it offers.
    const ASKED: &str = "merge_entities";

    // ── a sitting that was shown a place and has created nothing yet ───────
    let gamma = story.as_bot("bot:gamma").await;
    let name = gamma.run().to_string();
    gamma.call("list_entities", json!({"kind": "place"})).await;
    // A run that never wrote has no card and cannot be picked up by its name.
    gamma.journal("looked at the places").await;

    // ── the same sitting back by its wrap code keeps what it was shown ───────
    let delta = story.as_bot("bot:delta").await;
    delta.call("list_entities", json!({"kind": "place"})).await;
    let (_, reopened) = reopen_by_code(&story, &delta, "delta").await;
    create(&reopened, "place:wharf-road", "Wharf Road")
        .await
        .says("place:wonder-wharf")
        .says(ASKED);

    // ── a different agent taking the quiet run by name was shown nothing ─────
    wait_out_the_lease().await;
    let (_, taker) = pick_up(&story, "gamma", &name).await;
    create(&taker, "place:leftorium", "Wonder Leftorium")
        .await
        .never_says(ASKED);
    // The positive the absence rests on: once it is shown the places, the same
    // creation beside them is asked about like any session's.
    taker.call("list_entities", json!({"kind": "place"})).await;
    create(&taker, "place:moes", "Wonder Moes")
        .await
        .says(ASKED);

    story.finish().await;
}

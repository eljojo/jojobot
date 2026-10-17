//! **A restart keeps what the board alone cannot say.**
//!
//! A run's key, the key a resume or a wrap ended, and the moment another
//! sitting took a run over were all held in the process, so a deploy forgot
//! them: an ended key read as one that never existed, a wrapped run's probe
//! said `gone`, a run taken over a moment ago could be taken again, and a
//! booted run that had not written yet lost its key. Each case below restarts
//! the server over the same stores and asks the question the memory answered.

use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{NewSession, Sessions, Sid};
use serde_json::json;

use super::dsl::Story;

/// A lease of seconds on the real clock, long enough that a restart fits inside
/// it and short enough to wait out.
const LEASE: std::time::Duration = std::time::Duration::from_secs(8);

async fn wait_out_the_lease() {
    tokio::time::sleep(LEASE + std::time::Duration::from_millis(500)).await;
}

/// **A key a resume replaced still says why it stopped, after a restart.** The
/// refusal names the run and lists what is in flight, which only a refusal to a
/// key that once opened a run does; a key that never opened one gets neither.
#[tokio::test]
async fn a_key_a_resume_replaced_is_still_told_why_after_a_restart() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("topic:widgets", "Widgets").await;
    let old = story.as_bot("bot:gamma").await;
    old.call("journal", json!({"entry": "carrying on"})).await;
    wait_out_the_lease().await;
    let (_, taker) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": old.run()}),
        )
        .await;
    let taker = taker.expect("a quiet run is handed to whoever names it");

    let again = story.restarted_on_a_lease_of(LEASE).await;
    let probe = again.as_bot("bot:otto").await;
    let refused = probe
        .refused(
            "recall",
            json!({"subject": "topic:widgets", "sid": old.sid()}),
        )
        .await;
    // The run and the reason: a resume took the run over.
    refused
        .says(old.run())
        .says("was resumed")
        .never_says(old.sid());
    assert!(
        refused.json()["in_flight"].is_object(),
        "the refusal to a key that once opened a run lists what is in flight: {}",
        refused.raw()
    );
    // The positive: the key that replaced it opens the run.
    probe
        .call(
            "recall",
            json!({"subject": "topic:widgets", "sid": taker.sid()}),
        )
        .await
        .says("topic:widgets");
    // The negative control: a key nothing ever minted is not told a history.
    let stranger = probe
        .refused(
            "recall",
            json!({"subject": "topic:widgets", "sid": jojobot_domain::session::mint_key()}),
        )
        .await;
    assert!(
        stranger.json().get("in_flight").is_none(),
        "a key that never opened a run names no run: {}",
        stranger.raw()
    );
    again.finish().await;
    story.finish().await;
}

/// **A run wrapped before a restart still answers `wrapped` to its old key.**
#[tokio::test]
async fn a_run_wrapped_before_a_restart_still_answers_wrapped_to_a_probe() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    let wrapped = story.as_bot("bot:gamma").await;
    wrapped.call("journal", json!({"entry": "almost"})).await;
    wrapped.call("wrap_session", json!({"story": "done"})).await;
    let live = story.as_bot("bot:delta").await;
    live.call("journal", json!({"entry": "still going"})).await;

    let again = story.restarted_on_a_lease_of(LEASE).await;
    let probe = again.as_bot("bot:otto").await;
    let standing = |sid: String| {
        let probe = &probe;
        async move { probe.call("ping", json!({"sid": sid})).await.json()["carried_session"].clone() }
    };
    assert_eq!(standing(wrapped.sid().to_string()).await, "wrapped");
    // The same key sent on a call is refused with the run and the reason: the
    // run wrapped, which ended its key.
    probe
        .refused(
            "recall",
            json!({"subject": "bot:gamma", "sid": wrapped.sid()}),
        )
        .await
        .says(wrapped.run())
        .says("wrapped at")
        .never_says(wrapped.sid());
    // The pairs: a live key is held, and a key nothing minted is gone.
    assert_eq!(standing(live.sid().to_string()).await, "held");
    assert_eq!(standing(jojobot_domain::session::mint_key()).await, "gone");
    again.finish().await;
    story.finish().await;
}

/// **A run taken over inside the lease cannot be taken again, even across a
/// restart.** A pick-up writes a key and no entry, so only the mark of the
/// pick-up says the run is its new sitting's.
#[tokio::test]
async fn a_second_pick_up_inside_the_lease_is_refused_after_a_restart() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let old = story.as_bot("bot:gamma").await;
    old.call("journal", json!({"entry": "carrying on"})).await;
    wait_out_the_lease().await;
    let ask = json!({"bot": "gamma", "brief": true, "resume": old.run()});
    let (_, first) = story.call("start_here", ask.clone()).await;
    first.expect("the first pick-up takes the run");

    let again = story.restarted_on_a_lease_of(LEASE).await;
    again.refused_at_the_door("start_here", ask.clone()).await;
    // The positive: the same name takes the run once the lease has run out.
    wait_out_the_lease().await;
    let (_, second) = again.call("start_here", ask).await;
    second.expect("the name takes a run that has been quiet for the lease");
    again.finish().await;
    story.finish().await;
}

/// **The exchange of a keyless run's name is a pick-up that survives a
/// restart too.**
#[tokio::test]
async fn a_name_exchanged_for_a_key_cannot_take_the_run_inside_the_lease_after_a_restart() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    story
        .sessions_store()
        .begin(NewSession {
            bot: EntityId("bot:gamma".into()),
            sid: Sid("jm7z".into()),
            id: None,
            key_hash: None,
            focus: "a run the older build began".into(),
            started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(1),
            timezone: None,
            started_on: None,
        })
        .await
        .expect("the older build's run is on the board");
    let deployed = story.restarted_on_a_lease_of(LEASE).await;
    let holder = deployed.as_bot("bot:gamma").await;
    holder
        .refused("journal", json!({"entry": "as the old run", "sid": "jm7z"}))
        .await
        .says("\"exchanged\":true");

    let again = deployed.restarted_on_a_lease_of(LEASE).await;
    let ask = json!({"bot": "gamma", "brief": true, "resume": "jm7z"});
    again.refused_at_the_door("start_here", ask.clone()).await;
    wait_out_the_lease().await;
    let (_, taker) = again.call("start_here", ask).await;
    taker.expect("the name takes a run that has been quiet for the lease");
    again.finish().await;
    deployed.finish().await;
    story.finish().await;
}

/// **A run that booted and has not written yet keeps its key across a
/// restart.** No card exists for it, so only a record made at the boot can say
/// the key was ever handed out.
#[tokio::test]
async fn a_booted_run_that_never_wrote_still_holds_its_key_after_a_restart() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("topic:widgets", "Widgets").await;
    let quiet = story.as_bot("bot:gamma").await;

    let again = story.restarted_on_a_lease_of(LEASE).await;
    let probe = again.as_bot("bot:otto").await;
    probe
        .call(
            "recall",
            json!({"subject": "topic:widgets", "sid": quiet.sid()}),
        )
        .await
        .says("topic:widgets");
    // The negative control: a key nothing minted opens nothing.
    probe
        .refused(
            "recall",
            json!({"subject": "topic:widgets", "sid": jojobot_domain::session::mint_key()}),
        )
        .await;
    again.finish().await;
    story.finish().await;
}

/// **A run that answered `new` at the door and has not written yet keeps its key
/// across a restart too.** The answer mints a run by a branch of its own, so the
/// note of the minted key is made there as well.
#[tokio::test]
async fn a_run_begun_by_answering_new_that_never_wrote_still_holds_its_key_after_a_restart() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("topic:widgets", "Widgets").await;
    let (_, begun) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": "new"}),
        )
        .await;
    let begun = begun.expect("answering new hands back a key");

    let again = story.restarted_on_a_lease_of(LEASE).await;
    let probe = again.as_bot("bot:otto").await;
    probe
        .call(
            "recall",
            json!({"subject": "topic:widgets", "sid": begun.sid()}),
        )
        .await
        .says("topic:widgets");
    // The negative control: a key nothing minted opens nothing.
    probe
        .refused(
            "recall",
            json!({"subject": "topic:widgets", "sid": jojobot_domain::session::mint_key()}),
        )
        .await;
    again.finish().await;
    story.finish().await;
}

/// **A note of a run that never wrote is kept for seven days, and a key past
/// that opens nothing at the first restart that finds it.** The same restart
/// finds a second run, minted three days before it, and that key still opens: so
/// the refusal is the window and not a store that forgot every key.
#[tokio::test]
async fn a_note_of_a_run_that_never_wrote_opens_nothing_at_the_first_restart_after_seven_days() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-03-02").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    s.add("topic:widgets", "Widgets").await;
    let old = story.as_bot("bot:gamma").await;

    // A restart six days on: the old key is inside the window. A second run is
    // minted there, and has written nothing either.
    let middle = story.restarted_pretending_it_is("2026-03-08").await;
    let young = middle.as_bot("bot:delta").await;
    let probe = middle.as_bot("bot:otto").await;
    probe
        .call(
            "recall",
            json!({"subject": "topic:widgets", "sid": old.sid()}),
        )
        .await
        .says("topic:widgets");

    // The first restart past the old key's window: ten days after it was minted,
    // four after the young one.
    let late = middle.restarted_pretending_it_is("2026-03-12").await;
    let probe = late.as_bot("bot:otto").await;
    probe
        .call(
            "recall",
            json!({"subject": "topic:widgets", "sid": young.sid()}),
        )
        .await
        .says("topic:widgets");
    probe
        .refused(
            "recall",
            json!({"subject": "topic:widgets", "sid": old.sid()}),
        )
        .await;
    late.finish().await;
    middle.finish().await;
    story.finish().await;
}

/// **…and a note inside the window survives any number of restarts.** The pair
/// of the case above: the same two restarts, three days after the boot.
#[tokio::test]
async fn a_note_of_a_run_that_never_wrote_survives_a_second_restart_inside_seven_days() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-03-02").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("topic:widgets", "Widgets").await;
    let quiet = story.as_bot("bot:gamma").await;

    let soon = story.restarted_pretending_it_is("2026-03-05").await;
    let again = soon.restarted_pretending_it_is("2026-03-05").await;
    let probe = again.as_bot("bot:otto").await;
    probe
        .call(
            "recall",
            json!({"subject": "topic:widgets", "sid": quiet.sid()}),
        )
        .await
        .says("topic:widgets");
    again.finish().await;
    soon.finish().await;
    story.finish().await;
}

/// **A run that has not written keeps its bot through a rename and a restart.**
/// The note of the minted key holds the bot as the permanent identity the card
/// holds it as, so the key opens as the bot's current handle.
#[tokio::test]
async fn a_never_written_run_opens_as_the_renamed_bot_after_a_restart() {
    let story = Story::begin_on_a_lease_of("bot:otto", LEASE).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let quiet = story.as_bot("bot:gamma").await;
    s.call(
        "rename_entity",
        json!({"handle": "bot:gamma", "to": "bot:epsilon"}),
    )
    .await;

    let again = story.restarted_on_a_lease_of(LEASE).await;
    // The key addresses the bot by what it is called now: its own box answers,
    // and a beat written under it belongs to the renamed bot.
    let carried = again.session_carried_from(&quiet).await;
    carried
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .says("epsilon");
    carried.call("journal", json!({"entry": "back"})).await;
    carried.call("list_runs", json!({})).await.says(quiet.run());
    again.finish().await;
    story.finish().await;
}

/// **The newest retired keys survive a restart whichever way the board lists the
/// runs.** The registry keeps a bounded number of retired keys and forgets the
/// oldest first, so the keys that stopped opening a run most recently are the
/// ones that must come back. The two runs whose keys retired last hold the first
/// and the last card on the board, so one of them is read first whichever end
/// the board is listed from.
#[tokio::test]
async fn the_newest_retired_keys_are_the_ones_a_restart_keeps() {
    use jojobot_domain::session::{KeyTrail, RetiredBy, RetiredKey, hash_key, mint_key};

    const RUNS: usize = 1030;
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("topic:widgets", "Widgets").await;
    let began = jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48);
    let at = |seconds: usize| began + jiff::SignedDuration::from_secs(seconds as i64);
    // The runs a earlier server left on the board, each holding a key and the
    // key it replaced. Card `n` began at `n` seconds; the keys of cards 0 and
    // `RUNS - 1` retired last, the others in card order before them.
    let mut retired_keys = Vec::new();
    for n in 0..RUNS {
        let old = mint_key();
        let card = story
            .sessions_store()
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid(jojobot_domain::handle::draw(
                    jojobot_domain::session::RUN_LEN,
                )),
                id: None,
                key_hash: Some(hash_key(&mint_key())),
                focus: "a run an earlier server left".into(),
                started_at: at(n),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("the run is on the board");
        let retired_at = match n {
            0 => at(3 * RUNS),
            last if last == RUNS - 1 => at(3 * RUNS + 1),
            other => at(RUNS + other),
        };
        story
            .sessions_store()
            .set_key_hash(
                &card.id,
                &hash_key(&mint_key()),
                &KeyTrail {
                    retired: Some(RetiredKey {
                        hash: hash_key(&old),
                        at: retired_at,
                        by: RetiredBy::Resumed,
                    }),
                    taken_at: None,
                },
            )
            .await
            .expect("the key is replaced");
        retired_keys.push((old, card.sid.clone().expect("a name")));
    }

    let again = story.restarted().await;
    let probe = again.as_bot("bot:otto").await;
    for (key, name) in [&retired_keys[0], &retired_keys[RUNS - 1]] {
        probe
            .refused("recall", json!({"subject": "topic:widgets", "sid": key}))
            .await
            .says(name.as_str())
            .says("was resumed")
            .never_says(key);
    }
    // …and the registry holds a bounded number, forgetting the OLDEST: of the
    // 1030 retired keys, the ones that retired first (cards 1 to 6) are told
    // nothing, as a key nothing ever minted is, while the next one is still told.
    let (oldest, oldest_name) = &retired_keys[1];
    let forgotten = probe
        .refused("recall", json!({"subject": "topic:widgets", "sid": oldest}))
        .await;
    forgotten.never_says(oldest_name.as_str());
    assert!(
        forgotten.json().get("in_flight").is_none(),
        "the oldest retired key is forgotten, so it names no run: {}",
        forgotten.raw()
    );
    let (kept, kept_name) = &retired_keys[7];
    probe
        .refused("recall", json!({"subject": "topic:widgets", "sid": kept}))
        .await
        .says(kept_name.as_str())
        .says("was resumed");
    again.finish().await;
    story.finish().await;
}

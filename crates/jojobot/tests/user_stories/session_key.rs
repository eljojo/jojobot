//! "A session's name is not its secret."
//!
//! **The boot hands each run two things.** A public NAME, which records may
//! carry and anybody may read, and a KEY only the run holds, which rides every
//! call in `sid`. A name read out of a role's holder names a run and cannot act
//! as it.

use jojobot_domain::memory::{EntityId, Guarded, NewEntity, NewFact};
use jojobot_domain::session::{NewSession, Sessions, Sid};
use serde_json::json;

use super::dsl::Story;

/// **A voucher issued the way the engine issues one**, held by the run called
/// `holder`: through the store, because no verb writes one. Returns its handle.
async fn voucher_held_by(story: &Story, slug: &str, holder: &str) -> EntityId {
    let voucher = EntityId(format!("voucher:{slug}"));
    let today: jiff::civil::Date = "2026-10-09".parse().expect("a day");
    let issued = story
        .memory_store()
        .add_entity_with_first_claim(
            NewEntity::new(voucher.clone(), format!("Grant {slug}"), "jojobot"),
            {
                // **Every key is a setting that reaches the thing**, as the
                // engine writes a voucher.
                let fields: std::collections::BTreeMap<String, String> = [
                    ("right", "boot-as"),
                    ("target", "bot:gamma"),
                    ("holder", holder),
                    ("issuer", "wxyz"),
                    ("state", "held"),
                ]
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
                NewFact {
                    sets: fields.keys().cloned().collect(),
                    fields,
                    ..NewFact::about(voucher.clone(), "a voucher for boot-as on gamma", today)
                }
            },
        )
        .await
        .expect("the engine path writes it");
    assert!(
        matches!(issued, Guarded::Written(_)),
        "the engine path was blocked"
    );
    voucher
}

async fn voucher_state(story: &Story, voucher: &EntityId) -> String {
    story
        .memory_store()
        .fields(voucher)
        .await
        .expect("the voucher reads")["state"]
        .clone()
}

#[tokio::test]
async fn a_name_read_from_a_record_cannot_act_as_the_session_it_names() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;

    // Gamma boots, claiming a role, so its public name lands on the role.
    let (booted, gamma) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "carpool"}),
        )
        .await;
    let gamma = gamma.expect("a boot with nothing to resume hands back a key");
    let booted = booted.json();
    let key = booted["session"]["sid"]
        .as_str()
        .expect("a key")
        .to_string();
    let name = booted["session"]["run"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot names the run apart from its key: {booted}"))
        .to_string();
    assert_ne!(key, name, "the name and the key are two values: {booted}");
    assert!(
        !name.contains(&key) && !key.contains(&name),
        "neither contains the other: {booted}"
    );

    // The positive: Gamma works with its key.
    gamma
        .call(
            "journal",
            json!({"entry": "took the carpool role", "sid": key}),
        )
        .await;

    // Delta reads Gamma's name off the role, as anybody can.
    let delta = story.as_bot("bot:delta").await;
    delta
        .recall("role:carpool")
        .await
        .says(&name)
        .never_says(&key);

    // The negative: the name is not a way in. The refusal names the route back
    // and repeats nothing of what was sent.
    let refusal = delta
        .refused("recall", json!({"subject": "role:carpool", "sid": name}))
        .await;
    refusal.says("start_here").never_says(&key);

    story.finish().await;
}

/// **A run that is still writing belongs to whoever holds its key; one that has
/// been quiet for the lease can be picked up by its name, and the pick-up ends
/// everything the old sitting held.**
///
/// The run's name is public: a role's holder shows it. So a boot that names it
/// is not proof of anything, and the server judges quiet by its OWN clock. The
/// caller's `today` plays no part, which is the case a caller acting out a later
/// day would otherwise use to age a live run into being takeable.
#[tokio::test]
async fn a_run_still_writing_cannot_be_taken_by_its_name_and_a_quiet_one_can() {
    let story = Story::begin_on_the_real_lease("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;

    let gamma = story.as_bot("bot:gamma").await;
    let (name, key) = (gamma.run().to_string(), gamma.sid().to_string());
    gamma
        .call(
            "journal",
            json!({"entry": "took the work", "focus": "the dispatch"}),
        )
        .await;
    let grant = voucher_held_by(&story, "boot-as-gamma", &name).await;

    // ── still writing: the name asks, and is refused, future day or not ─────
    for args in [
        json!({"bot": "gamma", "brief": true, "resume": name}),
        json!({"bot": "gamma", "brief": true, "resume": name, "today": "2099-01-01"}),
    ] {
        story
            .refused_at_the_door("start_here", args)
            .await
            .says(&name)
            .never_says(&key);
    }
    assert_eq!(
        voucher_state(&story, &grant).await,
        "held",
        "a refused pick-up voids nothing"
    );
    // The positive the refusals depend on: the key still continues the run.
    gamma.call("journal", json!({"entry": "still mine"})).await;

    // ── the same world, long after: the run has been quiet past the lease ───
    let later = story
        .restarted_on_the_real_lease_pretending_it_is("2099-01-01")
        .await;
    let (taken, taker) = later
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": name}),
        )
        .await;
    let taker = taker.expect("a quiet run is handed to whoever names it, with a new key");
    assert_eq!(taker.run(), name, "the run keeps its name");
    assert_ne!(taker.sid(), key, "…and gets a new key");
    taken.never_says(&key);
    assert!(
        taken.json()["session"]["last_wrote"].is_string(),
        "the one who took the run is told when it last wrote: {}",
        taken.json()
    );
    assert_eq!(
        voucher_state(&story, &grant).await,
        "void",
        "everything the old sitting held ends with the pick-up"
    );

    // The new key works, and the old one is turned away with the run's name and
    // the moment it was taken over.
    taker
        .call("journal", json!({"entry": "picked it up"}))
        .await;
    let old = later.as_bot("bot:delta").await;
    old.refused(
        "journal",
        json!({"entry": "from the old sitting", "sid": key}),
    )
    .await
    .says(&name)
    .says("resumed")
    .never_says(&key);

    story.finish().await;
}

/// **A caller who reads `list_runs` resumes a stopped run by the `run` it
/// names, and a key sent as `resume` is turned away.** The list carries each
/// run's public name and no key, and `resume` takes that name: the route every
/// description of `resume` now says.
#[tokio::test]
async fn a_caller_resumes_a_stopped_run_by_the_name_list_runs_gave_it() {
    let story = Story::begin_on_the_real_lease("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let stopped = story.as_bot("bot:gamma").await;
    stopped
        .call(
            "journal",
            json!({"entry": "took the work", "focus": "the dispatch"}),
        )
        .await;
    let (name, key) = (stopped.run().to_string(), stopped.sid().to_string());
    let two_days_on = jiff::Timestamp::now() + jiff::SignedDuration::from_hours(48);
    let later = story
        .restarted_on_the_real_lease_pretending_it_is(
            &two_days_on
                .to_zoned(jiff::tz::TimeZone::UTC)
                .date()
                .to_string(),
        )
        .await;
    let reader = later.as_bot("bot:gamma").await;
    let listed = reader.call("list_runs", json!({})).await;
    let runs = listed.json();
    let run = runs["runs"]
        .as_array()
        .and_then(|all| all.iter().find(|r| r["run"] == name))
        .unwrap_or_else(|| panic!("list_runs names the stopped run: {runs}"));
    assert!(
        run.get("sid").is_none(),
        "a list carries a name, never a key: {run}"
    );
    listed.never_says(&key);

    // The key is not the route: sent as `resume`, it is turned away.
    later
        .refused_at_the_door(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": key}),
        )
        .await;
    // The name the list gave is: the run comes back with a key of its own.
    let (taken, taker) = later
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": name}),
        )
        .await;
    let taker = taker.expect("a stopped run is handed to whoever names it, with a new key");
    assert_eq!(taker.run(), name);
    assert_ne!(taker.sid(), key);
    taken.never_says(&key);
    later.finish().await;
    story.finish().await;
}

/// **A live key or an unspent wrap code written into text is refused; a string
/// merely shaped like one is not.** The check reads values, so the same text
/// lands once the secret is not live any more — which is what keeps the refusal
/// from being a blanket on anything that looks long.
#[tokio::test]
async fn a_live_key_or_an_unspent_wrap_code_in_text_is_refused_and_a_digest_lands() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("topic:widgets", "Widgets").await;
    let gamma = story.as_bot("bot:gamma").await;
    let (name, key) = (gamma.run().to_string(), gamma.sid().to_string());
    gamma.call("journal", json!({"entry": "working"})).await;
    let other = story.as_bot("bot:gamma").await;

    // ── a live key, in any text a call stores ───────────────────────────────
    let text = format!("my notes mention {key} in passing");
    for (tool, args) in [
        (
            "capture",
            json!({"subject": "topic:widgets", "content": text, "provenance": "inference"}),
        ),
        ("journal", json!({"entry": text})),
        ("post_message", json!({"to": "gamma", "body": text})),
        (
            "capture",
            json!({"subject": "topic:widgets", "content": "fine",
                   "provenance": "inference", "sets": {"memo": text}}),
        ),
    ] {
        other
            .refused(tool, args)
            .await
            .says("\"wrote\":false")
            .says(&name)
            .never_says(&key)
            // A key belongs in `sid` and nowhere else: sent as `resume` it is
            // refused, so the refusal must not offer that route.
            .says("`sid`")
            .never_says("`resume`");
    }

    // ── the positive: a digest of the same length is only text ──────────────
    let digest = &jojobot_domain::session::hash_key(&key)[..key.len()];
    other
        .call(
            "capture",
            json!({"subject": "topic:widgets", "provenance": "inference",
                   "content": format!("the digest {digest} is only text")}),
        )
        .await
        .says("topic:widgets");

    // ── an unspent wrap code is live too, and a spent one is not ────────────
    let code = gamma
        .call("wrap_session", json!({"story": "done for now"}))
        .await
        .json()["wrap_code"]
        .as_str()
        .expect("a wrap hands back a code")
        .to_string();
    other
        .refused(
            "capture",
            json!({"subject": "topic:widgets", "provenance": "inference",
                   "content": format!("the code is {code}")}),
        )
        .await
        .says(&name)
        // A wrap code belongs in `resume`, never in `sid`.
        .says("`resume`")
        .never_says("`sid`")
        .never_says(&code);
    story.finish().await;
}

/// **Two pick-ups of one quiet run, made at the same moment, hand a key to one
/// of them.** The run's name is public, so whoever reads it can ask for the
/// run. A pick-up writes a key and no entry, so the run must count as written
/// to at the pick-up, or the second ask meets a run that still reads as quiet,
/// is handed a key too, and turns the first taker away from the run it had just
/// been given. **This case pins that moment and does not pin the bot's lock:**
/// with the lock removed it stays green, because the in-memory stores leave no
/// gap between the quiet check and the new key for a second ask to land in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_pick_ups_of_one_quiet_run_at_once_hand_a_key_to_one() {
    // A lease of seconds on the real clock: the run goes quiet by waiting, and
    // is not swept, which a stated far-off day would do to it before the quiet
    // rule ran at all.
    let lease = std::time::Duration::from_secs(3);
    let story = Story::begin_on_a_lease_of("bot:otto", lease).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;
    let (name, key) = (gamma.run().to_string(), gamma.sid().to_string());
    gamma
        .call("journal", json!({"entry": "took the work"}))
        .await;
    tokio::time::sleep(lease + std::time::Duration::from_millis(500)).await;

    let ask = json!({"bot": "gamma", "brief": true, "resume": name});
    // Two tasks, not two futures in one: futures joined in one task are polled
    // by one thread in turn and never interleave, which would leave the lock
    // under test with nothing to do.
    let story = std::sync::Arc::new(story);
    let spawned = |story: std::sync::Arc<Story>, ask: serde_json::Value| {
        tokio::spawn(async move { story.call_either("start_here", ask).await })
    };
    let (first, second) = tokio::join!(
        spawned(story.clone(), ask.clone()),
        spawned(story.clone(), ask.clone()),
    );
    let ((first, first_taker), (second, second_taker)) =
        (first.expect("first ask"), second.expect("second ask"));
    let takers: Vec<_> = [first_taker, second_taker].into_iter().flatten().collect();
    assert_eq!(
        takers.len(),
        1,
        "exactly one pick-up is handed a key: {} / {}",
        first.raw(),
        second.raw()
    );
    // The positive the count depends on: the key that was handed out works.
    let taker = &takers[0];
    assert_ne!(taker.sid(), key, "the winner has a new key");
    taker.call("journal", json!({"entry": "mine"})).await;
    let Ok(story) = std::sync::Arc::try_unwrap(story) else {
        panic!("both asks are over, so nothing else holds the story");
    };
    story.finish().await;
}

/// **The refusal of a live key in text is not a list of verbs.** It stands at
/// the door, before any verb reads its arguments, so it holds for every verb the
/// surface serves — the ones a later build adds included, which is what this
/// table reads from the served list rather than from a list of names.
#[tokio::test]
async fn a_live_key_in_text_is_refused_on_every_verb_the_surface_serves() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;
    let (name, key) = (gamma.run().to_string(), gamma.sid().to_string());
    gamma.call("journal", json!({"entry": "working"})).await;
    let other = story.as_bot("bot:gamma").await;

    let verbs = other.served_verbs().await;
    for known in ["capture", "journal", "post_message", "wrap_session"] {
        assert!(
            verbs.iter().any(|verb| verb == known),
            "the table reads the served list, so it must carry {known:?}: {verbs:?}"
        );
    }
    for verb in &verbs {
        other
            .refused(
                verb,
                json!({"note": format!("a stray {key} in a free argument")}),
            )
            .await
            .says(&name)
            .never_says(&key);
    }
    story.finish().await;
}

/// **A run that began before keys existed exchanges its old name for a key
/// once; every later bare name is turned away.** The keyless run is put on the
/// board the way the older build left it: a name and no hash.
#[tokio::test]
async fn a_keyless_run_exchanges_its_old_name_once_and_a_second_bare_name_is_refused() {
    let story = Story::begin("bot:otto").await;
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
            started_at: jiff::Timestamp::now(),
            timezone: None,
            started_on: None,
        })
        .await
        .expect("the older build's run is on the board");
    let deployed = story.restarted().await;
    let thief = deployed.as_bot("bot:gamma").await;

    // ── the first call with the old name hands out a key ────────────────────
    let exchanged = thief
        .refused("journal", json!({"entry": "as the old run", "sid": "jm7z"}))
        .await;
    exchanged.says("\"exchanged\":true");
    let key = exchanged.json()["sid"]
        .as_str()
        .expect("the exchange hands back a key")
        .to_string();
    assert_ne!(key, "jm7z");

    // The positive: the key works. The negative: the name does not, again, and
    // says when it was exchanged to whoever still sends it.
    thief
        .call("journal", json!({"entry": "as the old run", "sid": key}))
        .await;
    thief
        .refused("journal", json!({"entry": "again", "sid": "jm7z"}))
        .await
        .says("exchanged")
        .never_says(&key)
        .never_says("\"exchanged\":true");

    // The exchange is on the card, so the key outlives a restart.
    let again = deployed.restarted().await;
    let probe = again.as_bot("bot:otto").await;
    probe
        .call("recall", json!({"subject": "bot:gamma", "sid": key}))
        .await
        .says("bot:gamma");

    again.finish().await;
    deployed.finish().await;
}

/// **A key is stored with the run, so a restart keeps what was decided before
/// it**: a live key still opens its run, and neither a key a wrap ended nor one a
/// resume replaced comes back with the server. The registry is in memory and
/// forgets everything; only the card can say which key is the run's.
#[tokio::test]
async fn a_restart_keeps_the_key_a_run_holds_and_brings_back_none_that_ended() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    s.add("topic:widgets", "Widgets").await;

    // Gamma holds a key it keeps; delta's run wraps; a third run of gamma is
    // picked up by its name, which replaces the key it had.
    let kept = story.as_bot("bot:gamma").await;
    kept.call("journal", json!({"entry": "carrying on"})).await;
    let wrapped = story.as_bot("bot:delta").await;
    wrapped.call("journal", json!({"entry": "done soon"})).await;
    wrapped.call("wrap_session", json!({"story": "done"})).await;
    let replaced = story.as_bot("bot:gamma").await;
    replaced
        .call("journal", json!({"entry": "about to be taken"}))
        .await;
    let (_, taker) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": replaced.run()}),
        )
        .await;
    let taker = taker.expect("the pick-up hands back a new key");
    assert_ne!(taker.sid(), replaced.sid());

    let again = story.restarted().await;
    let probe = again.as_bot("bot:otto").await;
    // The positives: both live keys open their runs on the new server. A READ
    // is what asks: a wrapped run refuses every write whatever its key is, so
    // a write cannot tell a key that came back from one that did not.
    for live in [&kept, &taker] {
        probe
            .call(
                "recall",
                json!({"subject": "topic:widgets", "sid": live.sid()}),
            )
            .await
            .says("topic:widgets");
    }
    // The negatives: the two keys that ended stay ended.
    for ended in [&wrapped, &replaced] {
        probe
            .refused(
                "recall",
                json!({"subject": "topic:widgets", "sid": ended.sid()}),
            )
            .await
            .never_says(ended.sid());
    }
    again.finish().await;
    story.finish().await;
}

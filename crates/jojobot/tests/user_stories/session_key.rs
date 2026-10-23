//! "A session's name is not its secret."
//!
//! **The boot hands each run two things.** A public NAME, which records may
//! carry and anybody may read, and a KEY only the run holds, which rides every
//! call in `sid`. A name read out of a role's holder names a run and cannot act
//! as it.

use jojobot_domain::memory::{EntityId, Guarded, NewEntity, NewFact};
use jojobot_domain::session::{NewSession, SessionState, Sessions, Sid};
use serde_json::json;

use super::dsl::{Session, Story};

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
    story.the_store_changed_underneath().await;
    voucher
}

/// **A voucher's state, read the way a caller reads it**: a recall through the
/// served surface, at the field the thing holds. The store is not the oracle.
async fn voucher_state(reader: &Session, voucher: &EntityId) -> String {
    reader
        .recall(voucher.as_str())
        .await
        .text_at("/objects/0/fields/state")
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
        voucher_state(&s, &grant).await,
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
    let reader = later.as_bot("bot:delta").await;
    assert_eq!(
        voucher_state(&reader, &grant).await,
        "void",
        "everything the old sitting held ends with the pick-up"
    );

    // The new key works, and the old one is turned away with the run's name and
    // the moment it was taken over.
    taker
        .call("journal", json!({"entry": "picked it up"}))
        .await;
    let old = &reader;
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

    // **The spent half the doc above promises**, on a bot whose wrapped run is
    // its newest, so its code can be presented. While unspent, text carrying it
    // is refused; presenting it as `resume` spends it, and the same text lands
    // and reads back from the store.
    s.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    delta.call("journal", json!({"entry": "working"})).await;
    let spent_later = delta
        .call("wrap_session", json!({"story": "done for now"}))
        .await
        .json()["wrap_code"]
        .as_str()
        .expect("a wrap hands back a code")
        .to_string();
    let text = json!({"subject": "topic:widgets", "provenance": "inference",
                      "content": format!("the code {spent_later} was spent")});
    other.refused("capture", text.clone()).await;
    let (_, reopened) = story
        .call(
            "start_here",
            json!({"bot": "delta", "brief": true, "resume": spent_later}),
        )
        .await;
    assert!(
        reopened.is_some(),
        "the code opens the wrapped run for one last change"
    );
    other.call("capture", text).await;
    other
        .call("recall", json!({"subject": "topic:widgets", "facts": true}))
        .await
        .says("was spent");
    story.finish().await;
}

/// **A pick-up counts as a write to the run, so a second ask straight after it
/// is turned away.** The run's name is public, so whoever reads it can ask for
/// the run. A pick-up writes a key and no entry, so the run has to count as
/// written to at the pick-up, or the second ask meets a run that still reads as
/// quiet, is handed a key too, and turns the first taker away from the run it
/// had just been given.
///
/// **This case is sequential on purpose and does not pin the bot's lock.** A
/// concurrent form of it stays green with the lock removed, because the
/// in-memory stores leave no gap between the quiet check and the new key for a
/// second ask to land in; no story can pin the lock without a store that has
/// one.
#[tokio::test]
async fn a_second_ask_straight_after_a_pick_up_is_turned_away() {
    // A lease of seconds: the run goes quiet on a clock a story moves a little
    // past the lease, and is not swept, which a stated far-off day would do to it
    // before the quiet rule ran at all. The lease is long enough that a loaded
    // host still makes the second ask inside it.
    let lease = std::time::Duration::from_secs(10);
    let story = Story::begin_on_a_lease_of("bot:otto", lease).await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;
    let (name, key) = (gamma.run().to_string(), gamma.sid().to_string());
    gamma
        .call("journal", json!({"entry": "took the work"}))
        .await;
    let later = story
        .restarted_on_a_lease_of_a_clock_ahead_by(lease, lease * 2)
        .await;

    let ask = json!({"bot": "gamma", "brief": true, "resume": name});
    let (_, first) = later.call("start_here", ask.clone()).await;
    let taker = first.expect("a quiet run is handed to the first who names it");
    assert_ne!(taker.sid(), key, "the first taker has a new key");
    // The second ask, with the run just taken: turned away, and told the run
    // wrote at the pick-up.
    let second = later.refused_at_the_door("start_here", ask).await;
    second.says(&name).never_says(taker.sid()).never_says(&key);
    // The positive both depend on: the key that was handed out still works.
    taker.call("journal", json!({"entry": "mine"})).await;
    later.finish().await;
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

/// **The last moment a refusal names**, read as a timestamp rather than as
/// words: the refusal says when the run can be taken by its name, and that is
/// the last time it writes.
fn last_moment_named(refusal: &str) -> jiff::Timestamp {
    refusal
        .split(|c: char| c.is_whitespace() || c == '"')
        .filter_map(|word| {
            word.trim_end_matches(['.', ','])
                .parse::<jiff::Timestamp>()
                .ok()
        })
        .next_back()
        .unwrap_or_else(|| panic!("the refusal names a moment: {refusal}"))
}

/// **The exchange of a keyless run's name counts as a pick-up.** A run quiet
/// for longer than the lease is exchanged by its holder's first call, which
/// writes no chronology entry; without a mark at the exchange a third party that
/// sends the old name straight after is handed the run the holder was just given
/// a key for. The refusal to the third party and to a holder who lost the answer
/// both name the moment the name takes the run, and after the lease it does.
#[tokio::test]
async fn a_name_cannot_take_a_run_inside_the_lease_after_its_exchange_and_can_after() {
    // Long enough that a loaded host still makes both asks inside the lease.
    let lease = std::time::Duration::from_secs(10);
    let story = Story::begin_on_a_lease_of("bot:otto", lease).await;
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
    let deployed = story.restarted_on_a_lease_of(lease).await;
    let holder = deployed.as_bot("bot:gamma").await;

    let before = jiff::Timestamp::now();
    holder
        .refused("journal", json!({"entry": "as the old run", "sid": "jm7z"}))
        .await
        .says("\"exchanged\":true");
    let after = jiff::Timestamp::now();
    let window = |at: jiff::Timestamp| {
        let lease = jiff::SignedDuration::try_from(lease).expect("a lease is a duration");
        assert!(
            before + lease <= at && at <= after + lease,
            "the name takes the run one lease after the exchange, {before}..{after} + {lease}: {at}"
        );
    };

    // ── a third party sending the old name is turned away, with the moment ───
    let third = deployed
        .refused_at_the_door(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": "jm7z"}),
        )
        .await;
    window(last_moment_named(third.raw()));
    // ── and so is a holder who lost the answer and sends the name again ─────
    let lost = holder
        .refused("journal", json!({"entry": "again", "sid": "jm7z"}))
        .await;
    lost.never_says("\"exchanged\":true");
    window(last_moment_named(lost.raw()));

    // ── after the lease the same name takes the run, once ───────────────────
    let later = deployed
        .restarted_on_a_lease_of_a_clock_ahead_by(lease, lease * 2)
        .await;
    let (_, taker) = later
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": "jm7z"}),
        )
        .await;
    taker.expect("the name takes a run that has been quiet for the lease");

    later.finish().await;
    deployed.finish().await;
    story.finish().await;
}

/// **The older build's run, put on the board the way it left it**: a name and no
/// hash. Returns the card's id.
async fn keyless_run(story: &Story, name: &str) -> jojobot_domain::session::SessionId {
    story
        .sessions_store()
        .begin(NewSession {
            bot: EntityId("bot:gamma".into()),
            sid: Sid(name.into()),
            id: None,
            key_hash: None,
            focus: "a run the older build began".into(),
            started_at: jiff::Timestamp::now(),
            timezone: None,
            started_on: None,
        })
        .await
        .expect("the older build's run is on the board")
        .id
}

/// **A run that ended is never exchanged by its old name.** The older build left
/// runs that wrapped or were abandoned with no key, and an exchange hands a key
/// to whoever sends the name: for a run that ended it would be a key to a run
/// nobody holds. Each way a run ends is refused, and a running keyless run beside
/// them still exchanges, so the refusal is about the run's state and not about
/// the old name having stopped working.
#[tokio::test]
async fn a_keyless_run_that_has_ended_is_not_exchanged_by_its_old_name() {
    for (ended, name) in [
        (SessionState::Wrapped, "jm7z"),
        (SessionState::Abandoned, "kq4n"),
    ] {
        let story = Story::begin("bot:otto").await;
        let s = story.session().await;
        s.add("bot:gamma", "Gamma").await;
        let card = keyless_run(&story, name).await;
        let running = keyless_run(&story, "rt5w").await;
        story
            .sessions_store()
            .close(&card, ended)
            .await
            .expect("the older build's run ended");
        let deployed = story.restarted().await;
        let holder = deployed.as_bot("bot:gamma").await;

        let refused = holder
            .refused("journal", json!({"entry": "as the old run", "sid": name}))
            .await;
        refused.never_says("\"exchanged\":true");
        assert!(
            refused.json().get("sid").is_none(),
            "{ended:?}: a key was handed out for a run that ended: {}",
            refused.raw()
        );

        // The positive the refusal rests on: the run that is still running
        // exchanges once, in the same store, in the same boot.
        let exchanged = holder
            .refused("journal", json!({"entry": "as the old run", "sid": "rt5w"}))
            .await;
        exchanged.says("\"exchanged\":true");
        let _ = running;

        deployed.finish().await;
        story.finish().await;
    }
}

/// **The refusal for the old name of an abandoned keyless run points at a way
/// back that works.** It says to resume the run by its name. Followed, the resume
/// hands back a key, and the key opens the run. Without this the pointer could
/// dead-end at a refusal that says only the key continues the run.
#[tokio::test]
async fn the_refusal_for_an_abandoned_keyless_run_points_at_a_resume_that_works() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let card = keyless_run(&story, "jm7z").await;
    story
        .sessions_store()
        .close(&card, SessionState::Abandoned)
        .await
        .expect("the sweep ended the run");
    let deployed = story.restarted().await;
    let holder = deployed.as_bot("bot:gamma").await;

    let refused = holder
        .refused("journal", json!({"entry": "as the old run", "sid": "jm7z"}))
        .await;
    refused.never_says("\"exchanged\":true");

    // Follow the pointer.
    let (resumed, _) = deployed
        .call("start_here", json!({"bot": "gamma", "resume": "jm7z"}))
        .await;
    let key = resumed.json()["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "the resume the refusal points at hands back a key: {}",
                resumed.raw()
            )
        })
        .to_string();
    assert_ne!(key, "jm7z");
    holder
        .call("journal", json!({"entry": "back on the run", "sid": key}))
        .await;

    deployed.finish().await;
    story.finish().await;
}

/// **The sweep can end a keyless run after the boot read it.** The registry
/// holds the run as keyless from the boot, so the exchange reads the run's state
/// from the board at the moment of the exchange, and a run that has ended since
/// is refused.
#[tokio::test]
async fn a_keyless_run_swept_after_the_boot_is_not_exchanged_by_its_old_name() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let card = keyless_run(&story, "jm7z").await;
    let deployed = story.restarted().await;
    let holder = deployed.as_bot("bot:gamma").await;
    story
        .sessions_store()
        .close(&card, SessionState::Abandoned)
        .await
        .expect("the sweep ended the run");
    deployed.the_store_changed_underneath().await;

    let refused = holder
        .refused("journal", json!({"entry": "as the old run", "sid": "jm7z"}))
        .await;
    refused.never_says("\"exchanged\":true");
    assert!(
        refused.json().get("sid").is_none(),
        "a key was handed out for a run the sweep ended: {}",
        refused.raw()
    );
    deployed.finish().await;
    story.finish().await;
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

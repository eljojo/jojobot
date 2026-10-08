//! "What did I do last time?"
//!
//! A bot's own past sittings are records of kind `session`, and `recall`
//! already selects by kind — so a bot asks about its own history the way it
//! asks about anything else. `list_runs` answers too, with each run's state and
//! a focus line, and it is exercised in `catching_up.rs`; only `recall` of kind
//! `session` with prose reads a run's chronology, which is why this story reads
//! it that way. **The part that needs a story is the boundary.** A bot finds ITS OWN runs and never another bot's,
//! and what it is not shown is COUNTED rather than dropped, because "nothing
//! of mine" and "something exists that is not mine" are different facts and a
//! bot acts on each differently: the first sends it to start fresh, the
//! second sends it to ask a colleague.
//!
//! ⛔️ **Every negative here sits beside the positive it depends on, in the
//! same read.** A bot that is shown nothing passes "never leaks a colleague's
//! words" on a build where no run is indexed at all.
//!
//! **Every story's own bot is stood up by the identity each instance arrives
//! holding**, and that write leaves a run behind. So a colleague's run already
//! exists here before any story step, and the counts below are read as what a
//! step ADDED to them.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The browse a bot makes to ask what it did before: kind `session`, prose on
/// so a run reads whole. No handle, no second verb.
async fn my_past_runs(who: &Session) -> Value {
    who.call("recall", json!({ "kind": "session", "prose": true }))
        .await
        .json()
}

fn runs_found(answered: &Value) -> &Vec<Value> {
    answered["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a kind browse answers with a list: {answered}"))
}

#[tokio::test]
async fn a_bot_finds_its_own_past_work_and_is_told_a_colleagues_exists() {
    let story = Story::begin("bot:otto").await;

    // ── ① otto does some work ───────────────────────────────────────────────
    let otto = story.session().await;
    otto.add("bot:epsilon", "Epsilon").await;
    otto.journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    let before = my_past_runs(&otto).await;
    let withheld_before = before["withheld"]
        .as_u64()
        .unwrap_or_else(|| panic!("a kind browse says how many runs it withheld: {before}"));

    // ── ② a colleague does its own, and the two are never mixed ─────────────
    let epsilon = story.as_bot("bot:epsilon").await;
    epsilon
        .journal("epsilon priced the replacement wheels, a private errand")
        .await;
    let answered = my_past_runs(&otto).await;

    // The positive: its own run is there, whole, and the only one shown.
    let runs = runs_found(&answered);
    assert_eq!(
        runs.len(),
        1,
        "otto must find exactly its own run: {answered}"
    );
    assert!(
        runs[0]["id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("session:"),
        "the hit must be a session record: {answered}"
    );
    assert!(
        runs[0]["prose"]
            .as_str()
            .unwrap_or_default()
            .contains("east gorge"),
        "otto's own run must read back what it did: {answered}"
    );

    // The negative that depends on it: the colleague's run is counted, and
    // its words are nowhere in the answer.
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(withheld_before + 1),
        "the colleague's run must be counted as withheld, not dropped: {answered}"
    );
    assert!(
        !answered.to_string().contains("replacement wheels"),
        "a colleague's words reached a bot that does not own them: {answered}"
    );

    // ── ③ "what did I do last time": a NEW sitting reads the earlier one ────
    // Ownership is the bot's, never the session's. Every read above came from
    // the session that wrote the run, so a build scoping runs by session would
    // pass all of it.
    let later = story.session_on("2026-10-12", Some("new")).await;
    assert_ne!(
        later.sid(),
        otto.sid(),
        "a second sitting, not the first again"
    );
    let last_time = my_past_runs(&later).await;
    let found = runs_found(&last_time);
    assert_eq!(
        found.len(),
        1,
        "the later sitting must find the earlier one's run, and only it: {last_time}"
    );
    assert!(
        found[0]["prose"]
            .as_str()
            .unwrap_or_default()
            .contains("east gorge"),
        "the earlier sitting's run must read back whole: {last_time}"
    );
    assert_eq!(
        last_time["withheld"].as_u64(),
        Some(withheld_before + 1),
        "the colleague's run is still counted, from a later sitting too: {last_time}"
    );
    assert!(
        !last_time.to_string().contains("replacement wheels"),
        "a colleague's words reached a later sitting of a different bot: {last_time}"
    );

    story.finish().await;
}

#[tokio::test]
async fn nothing_of_mine_reads_differently_from_nothing_at_all() {
    // ── ① a bot that has done nothing, beside a run that is not its own ─────
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let answered = my_past_runs(&otto).await;
    assert!(
        runs_found(&answered).is_empty(),
        "otto has written nothing, so it has no run to find: {answered}"
    );
    assert!(
        answered["withheld"].as_u64().is_some_and(|n| n >= 1),
        "an empty list that counts a run withheld says somebody else's exists: {answered}"
    );
    story.finish().await;

    // ── ② an instance where nobody has written anything ─────────────────────
    let empty = Story::begin_with_nothing_written().await;
    let nobody = empty.session().await;
    let answered = my_past_runs(&nobody).await;
    assert!(
        runs_found(&answered).is_empty(),
        "no run exists on an instance nobody has written to: {answered}"
    );
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(0),
        "an index with nothing in it withholds none, which is not the answer in ①: {answered}"
    );
    empty.finish().await;
}

#[tokio::test]
async fn a_caller_with_no_identity_is_told_runs_exist_and_shown_none() {
    // ── ① an instance with a run on it, asked by a caller carrying no sid ───
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    // What an identified caller sees: its own run shown, the rest counted.
    let identified = my_past_runs(&otto).await;
    let own_runs = runs_found(&identified).len() as u64;
    let others = identified["withheld"]
        .as_u64()
        .unwrap_or_else(|| panic!("a kind browse says how many runs it withheld: {identified}"));
    assert!(
        own_runs >= 1,
        "otto's run is indexed, or the count below proves nothing: {identified}"
    );
    let (answered, no_handle) = story
        .call("recall", json!({ "kind": "session", "prose": true }))
        .await;
    assert!(
        no_handle.is_none(),
        "a recall mints no session, so the caller still has no identity"
    );
    let answered = answered.json();
    assert!(
        runs_found(&answered).is_empty(),
        "a caller with no identity owns no run, so it is shown none: {answered}"
    );
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(own_runs + others),
        "every run on the instance, otto's included, is counted for a caller that owns none: \
         {answered}"
    );
    assert!(
        !answered.to_string().contains("east gorge"),
        "a run's words reached a caller that owns none: {answered}"
    );
    let how = answered["withheld_note"].as_str().unwrap_or_else(|| {
        panic!("an anonymous read of runs says how to read its own: {answered}")
    });
    assert!(
        how.contains("start_here") && how.contains("sid"),
        "the note must name the door and the handle it returns: {how}"
    );
    story.finish().await;

    // ── ② an instance where nobody has written anything ─────────────────────
    let empty = Story::begin_with_nothing_written().await;
    let (answered, _) = empty
        .call("recall", json!({ "kind": "session", "prose": true }))
        .await;
    let answered = answered.json();
    assert!(
        runs_found(&answered).is_empty(),
        "no run exists on an instance nobody has written to: {answered}"
    );
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(0),
        "an index with nothing in it withholds none, which is not the answer in ①: {answered}"
    );
    empty.finish().await;
}

#[tokio::test]
async fn a_colleagues_run_is_never_named_slugged_or_attributed_to_another_bot() {
    const FOCUS: &str = "pricing the replacement wheels";
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("bot:epsilon", "Epsilon").await;
    let epsilon = story.as_bot("bot:epsilon").await;
    epsilon
        .call(
            "journal",
            json!({ "entry": "started on the wheels", "focus": FOCUS }),
        )
        .await;

    // The run's own id, read by its owner.
    let own = my_past_runs(&epsilon).await;
    let run_id = runs_found(&own)
        .iter()
        .find(|run| run["name"] == FOCUS)
        .unwrap_or_else(|| panic!("epsilon's run is named for its focus: {own}"))["id"]
        .as_str()
        .expect("a run carries its id")
        .to_string();
    // Three ways of naming that run: a near miss of its id, the slug of its
    // focus, and the exact id.
    let mut near_miss = run_id.clone();
    let last = near_miss.pop().expect("an id is not empty");
    near_miss.push(if last == 'a' { 'b' } else { 'a' });
    let by_focus = format!("session:{}", FOCUS.replace(' ', "-"));
    let subjects = [near_miss, by_focus, run_id.clone()];

    // The other bot names each of them and is told neither whose it is nor
    // what it was about, only that it is not its own or does not exist.
    for subject in &subjects {
        let answered = otto
            .refused("recall", json!({ "kind": "session", "subject": subject }))
            .await;
        let text = answered.json().to_string().replace(subject.as_str(), "");
        assert!(
            !text.contains("wheels"),
            "naming {subject} gave another bot a run's focus text: {text}"
        );
        assert!(
            !text.contains("bot:epsilon"),
            "naming {subject} told another bot whose run it is: {text}"
        );
    }

    // The positive each negative depends on: the owner is shown its own run
    // for the exact id, and offered it by name for the other two, so the
    // checks above can see what they claim is absent.
    for subject in &subjects {
        let asked = json!({ "kind": "session", "subject": subject });
        if *subject == run_id {
            let answered = epsilon.call("recall", asked).await.json();
            assert_eq!(
                answered["objects"][0]["name"], FOCUS,
                "the owner reads its own run by its exact id: {answered}"
            );
        } else {
            let answered = epsilon.refused("recall", asked).await.json();
            assert!(
                answered.to_string().contains("wheels"),
                "the owner is offered its own run for {subject}: {answered}"
            );
        }
    }
    story.finish().await;
}

/// "I wrapped up yesterday. Can I read what I did?" A run that was wrapped is a
/// run like any other: the next sitting of the same bot finds it with one
/// ordinary browse of kind `session`, reads its chronology whole, and a run of
/// another bot is still counted and not shown. The negative sits beside the
/// positive it depends on, in the same read.
#[tokio::test]
async fn a_wrapped_run_is_read_by_the_next_run_of_its_bot_and_not_by_another_bots() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    first.add("bot:epsilon", "Epsilon").await;
    first
        .journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    first
        .wrap("surveyed the east gorge; the bridge is closed")
        .await;

    let epsilon = story.as_bot("bot:epsilon").await;
    epsilon
        .journal("epsilon priced the replacement wheels, a private errand")
        .await;
    epsilon
        .call("wrap_session", json!({"story": "priced the wheels"}))
        .await;

    // ── the next sitting of otto reads the wrapped one ──────────────────────
    let next = story.session_on("2026-10-12", Some("new")).await;
    let answered = my_past_runs(&next).await;
    let runs = runs_found(&answered);
    assert_eq!(
        runs.len(),
        1,
        "the next sitting must find the run it wrapped, and only it: {answered}"
    );
    let prose = runs[0]["prose"].as_str().unwrap_or_default();
    assert!(
        prose.contains("east gorge") && prose.contains("bridge is closed"),
        "the wrapped run reads back its chronology and its closing story: {answered}"
    );
    assert!(
        answered["withheld"].as_u64().is_some_and(|n| n >= 1),
        "the other bot's wrapped run is counted: {answered}"
    );
    assert!(
        !answered.to_string().contains("replacement wheels"),
        "another bot's words reached a sitting that does not own them: {answered}"
    );
    story.finish().await;
}

/// "My name changed. Are my old runs still mine?" A run belongs to the bot, not
/// to the handle the bot wore when the run was written, so after the bot is
/// renamed the next run under the new handle reads the old run, and a run of
/// another bot is still counted and not shown.
#[tokio::test]
async fn a_runs_owner_follows_its_bot_through_a_rename() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    first.add("bot:epsilon", "Epsilon").await;
    first
        .journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    let epsilon = story.as_bot("bot:epsilon").await;
    epsilon
        .journal("epsilon priced the replacement wheels, a private errand")
        .await;
    epsilon
        .call(
            "rename_entity",
            json!({"handle": "bot:otto", "to": "bot:otto-prime"}),
        )
        .await;

    let prime = story.as_bot("bot:otto-prime").await;
    let answered = my_past_runs(&prime).await;
    let runs = runs_found(&answered);
    assert_eq!(
        runs.len(),
        1,
        "the renamed bot must find the run it wrote under its old handle: {answered}"
    );
    assert!(
        runs[0]["prose"]
            .as_str()
            .unwrap_or_default()
            .contains("east gorge"),
        "the old run reads back whole: {answered}"
    );
    assert_eq!(
        runs[0]["parent"], "bot:otto-prime",
        "the run is filed under the handle the bot wears now: {answered}"
    );
    assert!(
        answered["withheld"].as_u64().is_some_and(|n| n >= 1),
        "the other bot's run is counted: {answered}"
    );
    assert!(
        !answered.to_string().contains("replacement wheels"),
        "another bot's words reached the renamed bot: {answered}"
    );
    story.finish().await;
}

/// "I filtered my runs and got nothing. Did somebody take them?" No: a filter no
/// run of mine can match leaves my own runs out, and the answer says how many of
/// mine it left out, apart from the runs withheld as another identity's. The
/// count of runs withheld is the same with and without the filter, and a browse
/// that left none of mine out carries no such count.
#[tokio::test]
async fn a_filter_that_leaves_out_my_own_runs_says_how_many_apart_from_those_withheld() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.journal("otto surveyed the east gorge and found the bridge closed")
        .await;

    // The positive: the plain browse finds my run, and excluded none of mine.
    let plain = my_past_runs(&otto).await;
    assert_eq!(runs_found(&plain).len(), 1, "my run is there: {plain}");
    assert!(
        plain.get("own_runs_excluded").is_none(),
        "a browse that left none of mine out says nothing of it: {plain}"
    );
    let withheld = plain["withheld"].clone();

    // The filter that matches no run of mine.
    let narrowed = otto
        .call(
            "recall",
            json!({"kind": "session", "fields": [{"key": "no_such_key"}]}),
        )
        .await
        .json();
    assert!(
        runs_found(&narrowed).is_empty(),
        "nothing carries that key: {narrowed}"
    );
    assert_eq!(
        narrowed["own_runs_excluded"], 1,
        "the answer counts my own run as left out by the filter: {narrowed}"
    );
    assert_eq!(
        narrowed["withheld"], withheld,
        "my run is not counted among those another identity owns: {narrowed}"
    );
    story.finish().await;
}

/// "Which of my runs was that one?" A run's sid is a field of the run, so its
/// owner filters on it and reads it back, and the same value is what
/// `start_here` takes as `resume`. Another bot filtering on the same value finds
/// nothing and is never shown the value.
#[tokio::test]
async fn a_runs_sid_is_a_field_its_owner_can_filter_on_read_back_and_resume() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("bot:epsilon", "Epsilon").await;
    otto.journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    let sid = otto.sid().to_string();

    // Read back: the run carries the sid it was written under.
    let plain = my_past_runs(&otto).await;
    assert_eq!(
        runs_found(&plain)[0]["fields"]["sid"],
        sid,
        "the run carries its sid: {plain}"
    );

    // Filter on it: exactly that run.
    let by_sid = otto
        .call(
            "recall",
            json!({"kind": "session", "fields": [{"key": "sid", "value": sid}]}),
        )
        .await
        .json();
    assert_eq!(
        runs_found(&by_sid).len(),
        1,
        "the run answers its sid: {by_sid}"
    );
    assert!(
        by_sid.get("own_runs_excluded").is_none(),
        "nothing of mine was left out: {by_sid}"
    );
    let other = otto
        .call(
            "recall",
            json!({"kind": "session", "fields": [{"key": "sid", "value": "nosuchsid"}]}),
        )
        .await
        .json();
    assert!(
        runs_found(&other).is_empty(),
        "another value matches no run: {other}"
    );

    // Another bot filtering on the same value finds nothing, and the value is
    // nowhere in what it is told.
    let epsilon = story.as_bot("bot:epsilon").await;
    let theirs = epsilon
        .call(
            "recall",
            json!({"kind": "session", "fields": [{"key": "sid", "value": sid}]}),
        )
        .await
        .json();
    assert!(
        runs_found(&theirs).is_empty(),
        "a colleague's filter reached my run: {theirs}"
    );
    assert!(
        !theirs.to_string().contains(&sid),
        "my sid reached another bot: {theirs}"
    );

    // And the value answers which run to resume.
    let resumed = story.session_on("2026-10-12", Some(&sid)).await;
    assert_eq!(resumed.sid(), sid, "the sid resumes the run it names");
    story.finish().await;
}

/// "What was I doing around the 8th?" A run is placed on the day it started, so
/// `near` finds it inside the window and not outside it.
#[tokio::test]
async fn near_places_a_run_on_the_day_it_started() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date();
    let far = today
        .checked_add(jiff::Span::new().days(60))
        .expect("a day");

    // The positive: my run is inside a window around the day it started.
    let around = otto
        .call(
            "recall",
            json!({"kind": "session", "near": {"day": today.to_string(), "within_days": 2}}),
        )
        .await
        .json();
    assert_eq!(
        runs_found(&around).len(),
        1,
        "the run is near its day: {around}"
    );

    // The negative that depends on it: a window two months away does not hold it.
    let away = otto
        .call(
            "recall",
            json!({"kind": "session", "near": {"day": far.to_string(), "within_days": 2}}),
        )
        .await
        .json();
    assert!(
        runs_found(&away).is_empty(),
        "the run is not near a day two months on: {away}"
    );
    story.finish().await;
}

/// "I tried the sid as the run's handle." The handle of a run is not its sid, and
/// the refusal says which read finds a run by the sid held, rather than sending
/// the caller to create an entity.
#[tokio::test]
async fn naming_a_run_by_its_sid_is_refused_with_the_read_that_finds_it() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.journal("otto surveyed the east gorge").await;
    let refused = otto
        .refused(
            "recall",
            json!({"subject": format!("session:{}", otto.sid())}),
        )
        .await
        .json();
    let way = refused["how_to_proceed"].as_str().unwrap_or_default();
    for named in ["kind", "session", "fields", "sid"] {
        assert!(way.contains(named), "the way on names {named}: {refused}");
    }
    assert!(
        !way.contains("add_entity"),
        "a run is not created by hand: {refused}"
    );
    story.finish().await;
}

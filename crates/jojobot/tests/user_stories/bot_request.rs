//! **A manager asks for a bot, and only the app bound to the request reports on
//! it.** A manager writes one record naming the bot, what should run and the app
//! that watches it; a bot that is not above the target is refused, and the
//! refusal names who may. The bound app finds its request by listing the
//! requests bound to it, writes what it observed, and no bot writes that.

use jojobot_domain::app::operations::OperationSpec;
use serde_json::json;

use super::dsl::Story;

const APP: &str = "beta";
const OTHER_APP: &str = "gamma";

fn status_keys() -> Vec<String> {
    [
        "observed_state",
        "observed_generation",
        "runtime_ref",
        "last_result",
    ]
    .map(String::from)
    .to_vec()
}

fn spec(target: &str, machine: &str) -> serde_json::Value {
    json!({
        "target_bot": target, "desired": "running", "machine": machine,
        "model": "model-a", "bound_app": APP,
    })
}

fn creating(spec: serde_json::Value) -> serde_json::Value {
    json!({"kind": "bot-request", "handle": "alpha", "name": "Homer runs",
           "source": "user-named", "sets": spec})
}

fn bound_to_the_app() -> serde_json::Value {
    json!({"kind": "bot-request", "fields": [{"key": "bound_app", "value": APP}]})
}

#[tokio::test]
async fn a_manager_asks_for_a_bot_and_only_its_app_reports_on_it() {
    let (story, token) = Story::begin_serving_an_app(
        "bot:marge",
        APP,
        vec![
            OperationSpec {
                operation: "recall".into(),
                kinds: vec![],
                keys: vec![],
            },
            OperationSpec {
                operation: "capture".into(),
                kinds: vec!["bot-request".into()],
                keys: status_keys(),
            },
        ],
    )
    .await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("bot:bart", "Bart").await;
    marge.add("machine:theta", "Theta").await;
    let homer = story.as_bot("bot:homer").await;
    let bart = story.as_bot("bot:bart").await;

    // ── 1. a bot that is not above the target is refused, and told who may ──
    bart.refused("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await
        .says("\"wrote\":false")
        .says("bot:marge");

    // ── 2. an unknown bot or machine in the spec is refused at the write ────
    marge
        .refused("add_entity", creating(spec("bot:omega", "machine:theta")))
        .await
        .says("\"wrote\":false");
    marge
        .refused("add_entity", creating(spec("bot:homer", "machine:omega")))
        .await
        .says("\"wrote\":false");

    // ── 2b. a target with no manager on the chart has no writer, whoever asks ─
    marge
        .refused("add_entity", creating(spec("bot:marge", "machine:theta")))
        .await
        .says("\"wrote\":false")
        .says("KeyNotYours");

    // ── 3. the manager above the target asks for the bot ───────────────────
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;

    // ── 4. the bound app lists the requests bound to it and finds it ───────
    let (status, listed) = story.app_calls(&token, "recall", bound_to_the_app()).await;
    assert_eq!(status, 200, "{listed}");
    let found = &listed["objects"][0];
    assert_eq!(found["handle"], "bot-request:alpha", "{listed}");
    assert_eq!(found["fields"]["desired"], "running", "{listed}");
    assert_eq!(found["fields"]["generation"], "1", "{listed}");

    // ── 5. a spec edit is the manager's alone, and it raises the generation ─
    bart.refused(
        "capture",
        json!({"subject": "bot-request:alpha", "content": "stop it",
               "sets": {"desired": "stopped"}}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:marge");
    homer
        .refused(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "stop me",
                   "sets": {"desired": "stopped"}}),
        )
        .await
        .says("\"wrote\":false");
    marge
        .call(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Stop the bot.",
                   "sets": {"desired": "stopped"}}),
        )
        .await;
    let (_, edited) = story.app_calls(&token, "recall", bound_to_the_app()).await;
    assert_eq!(
        edited["objects"][0]["fields"]["desired"], "stopped",
        "{edited}"
    );
    assert_eq!(
        edited["objects"][0]["fields"]["generation"], "2",
        "{edited}"
    );

    // ── 6. no bot sends the generation, and the target never moves ─────────
    marge
        .refused(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Skip ahead.",
                   "sets": {"generation": "9"}}),
        )
        .await
        .says("\"wrote\":false");
    marge
        .refused(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Another bot.",
                   "sets": {"target_bot": "bot:bart"}}),
        )
        .await
        .says("\"wrote\":false");

    // ── 7. only the bound app writes status; no bot does, whoever it is ─────
    let status_of = json!({"subject": "bot-request:alpha", "content": "Seen.",
                           "sets": {"observed_state": "stopped", "observed_generation": "2"}});
    marge
        .refused("capture", status_of.clone())
        .await
        .says("\"wrote\":false");
    homer
        .refused("capture", status_of.clone())
        .await
        .says("\"wrote\":false");
    let (written, body) = story.app_calls(&token, "capture", status_of).await;
    assert_eq!(written, 200, "{body}");
    let (_, seen) = story.app_calls(&token, "recall", bound_to_the_app()).await;
    assert_eq!(
        seen["objects"][0]["fields"]["observed_generation"], "2",
        "{seen}"
    );
    assert_eq!(seen["objects"][0]["fields"]["generation"], "2", "{seen}");

    // ── 8. the app writes no spec, and anyone adds a note ──────────────────
    let (refused, body) = story
        .app_calls(
            &token,
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Run it.",
                   "sets": {"desired": "running"}}),
        )
        .await;
    assert_ne!(refused, 200, "{body}");
    // The refused write left the request as it was: the spec the manager set,
    // the generation it raised, read back.
    let (_, unchanged) = story.app_calls(&token, "recall", bound_to_the_app()).await;
    assert_eq!(
        unchanged["objects"][0]["fields"]["desired"], "stopped",
        "{unchanged}"
    );
    assert_eq!(
        unchanged["objects"][0]["fields"]["generation"], "2",
        "{unchanged}"
    );
    bart.call(
        "capture",
        json!({"subject": "bot-request:alpha", "content": "A note from a bystander."}),
    )
    .await;
    story.finish().await;
}

/// **Only the app a request names writes its status, whichever other app holds
/// the same grant.** Two apps hold the same grant on the same kind and keys. The
/// request names the first; a second request names the second app; a third names
/// an app that is not served; a fourth names none. The named app writes each of
/// the first two. The other app, the app on the unserved name and both apps on
/// the bare request are refused for the bound app, and each refused write leaves
/// the request as it was.
#[tokio::test]
async fn only_the_app_a_request_names_writes_its_status() {
    let grant = || {
        vec![
            OperationSpec {
                operation: "recall".into(),
                kinds: vec![],
                keys: vec![],
            },
            OperationSpec {
                operation: "capture".into(),
                kinds: vec!["bot-request".into()],
                keys: status_keys(),
            },
        ]
    };
    let (story, tokens) =
        Story::begin_serving_apps("bot:marge", vec![(APP, grant()), (OTHER_APP, grant())]).await;
    let (first, second) = (&tokens[0], &tokens[1]);
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("machine:theta", "Theta").await;
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    let mut unserved = spec("bot:homer", "machine:theta");
    unserved["bound_app"] = json!("omega");
    marge
        .call(
            "add_entity",
            json!({"kind": "bot-request", "handle": "gamma", "name": "Homer elsewhere",
                   "source": "user-named", "sets": unserved}),
        )
        .await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot-request", "handle": "beta", "name": "Homer unbound",
                   "source": "user-named"}),
        )
        .await;
    let mut names_second = spec("bot:homer", "machine:theta");
    names_second["bound_app"] = json!(OTHER_APP);
    marge
        .call(
            "add_entity",
            json!({"kind": "bot-request", "handle": "epsilon", "name": "Homer on the second app",
                   "source": "user-named", "sets": names_second}),
        )
        .await;
    let reports = |handle: &str, state: &str| json!({"subject": handle, "content": "Seen.", "sets": {"observed_state": state}});
    let seen = |handle: &str| reports(handle, "running");
    // A refused write says a different state from the one held, so a read-back
    // after it can tell the write was kept out.
    let stopped = |handle: &str| reports(handle, "stopped");

    // ── the app the request names writes it ─────────────────────────────────
    let (written, body) = story
        .app_calls(first, "capture", seen("bot-request:alpha"))
        .await;
    assert_eq!(written, 200, "{body}");
    assert_eq!(
        observed(&story, first, "bot-request:alpha").await,
        "running"
    );

    // ── the second app writes the request that names it, with the same grant ──
    let (written, body) = story
        .app_calls(second, "capture", seen("bot-request:epsilon"))
        .await;
    assert_eq!(written, 200, "{body}");
    assert_eq!(
        observed(&story, second, "bot-request:epsilon").await,
        "running"
    );

    // ── another app with the same grant does not, on a request that names the
    //    other, one that names an app nobody serves, or one that names none ────
    for (token, handle) in [
        (second, "bot-request:alpha"),
        (first, "bot-request:gamma"),
        (second, "bot-request:gamma"),
        (first, "bot-request:beta"),
        (second, "bot-request:beta"),
        (first, "bot-request:epsilon"),
    ] {
        let (status, body) = story.app_calls(token, "capture", stopped(handle)).await;
        assert_ne!(status, 200, "{handle}: {body}");
        // The cause, not any failure: a bad token is refused too.
        assert!(
            body.to_string().contains("bound_app"),
            "{handle} is refused for another cause: {body}"
        );
    }
    for (handle, held) in [
        ("bot-request:alpha", json!("running")),
        ("bot-request:gamma", serde_json::Value::Null),
        ("bot-request:beta", serde_json::Value::Null),
        ("bot-request:epsilon", json!("running")),
    ] {
        assert_eq!(
            observed(&story, first, handle).await,
            held,
            "{handle} is as it was"
        );
    }
    story.finish().await;
}

/// The status a request holds, as an app reads it with the token it holds.
async fn observed(story: &Story, token: &str, handle: &str) -> serde_json::Value {
    let (status, read) = story
        .app_calls(token, "recall", json!({"subject": handle}))
        .await;
    assert_eq!(status, 200, "{read}");
    read["objects"][0]["fields"]["observed_state"].clone()
}

/// **Taking a spec key off a request changes the spec, so it raises the
/// generation, and no one takes the generation off.** The app reconciles on the
/// generation, so a change it cannot see would leave the bot unchanged.
#[tokio::test]
async fn clearing_a_spec_key_raises_the_generation_and_the_generation_stays() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("machine:theta", "Theta").await;
    let created = marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    let address = created.field("address");
    let generation = || async {
        let read = marge
            .call("recall", json!({"subject": "bot-request:alpha"}))
            .await;
        read.json()["objects"][0]["fields"]["generation"].clone()
    };
    assert_eq!(generation().await, "1");

    marge
        .call(
            "update_fact",
            json!({"address": &address, "clear_fields": ["machine"]}),
        )
        .await;
    assert_eq!(generation().await, "2", "a cleared spec key is a change");

    marge
        .refused(
            "update_fact",
            json!({"address": &address, "clear_fields": ["generation"]}),
        )
        .await
        .says("\"wrote\":false");
    assert_eq!(generation().await, "2", "the generation is still held");
    story.finish().await;
}

/// **An entity update that changes the spec raises the generation too.** The
/// same change through `capture` and `update_fact` is held by other cases; this
/// is the third verb that writes the keys.
#[tokio::test]
async fn updating_the_entity_raises_the_generation_of_a_request() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("machine:theta", "Theta").await;
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    let read = || async {
        let read = marge
            .call("recall", json!({"subject": "bot-request:alpha"}))
            .await;
        let fields = read.json()["objects"][0]["fields"].clone();
        (fields["generation"].clone(), fields["desired"].clone())
    };
    assert_eq!(read().await, ("1".into(), "running".into()));

    marge
        .call(
            "update_entity",
            json!({"handle": "bot-request:alpha", "sets": {"desired": "stopped"}}),
        )
        .await;
    assert_eq!(read().await, ("2".into(), "stopped".into()));
    story.finish().await;
}

/// **A request is not merged away, and the refusal does not send the caller to a
/// ceiling it has nothing to do with.** The merge is refused for the manager of
/// the target and for a bystander alike, into a topic and into another request.
/// The refusal names the request. A bare request, which carries no spec, still
/// folds.
#[tokio::test]
async fn a_request_is_not_merged_away_and_the_refusal_names_it() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("bot:bart", "Bart").await;
    marge.add("machine:theta", "Theta").await;
    marge.add("topic:scrapbook", "Scrapbook").await;
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot-request", "handle": "beta", "name": "Homer waits",
                   "source": "user-named"}),
        )
        .await;
    let bart = story.as_bot("bot:bart").await;

    for (who, survivor) in [
        (&marge, "topic:scrapbook"),
        (&bart, "topic:scrapbook"),
        (&marge, "bot-request:beta"),
    ] {
        who.refused(
            "merge_entities",
            json!({"duplicate": "bot-request:alpha", "survivor": survivor}),
        )
        .await
        .says("bot-request:alpha")
        .never_says("ceiling");
    }
    // The control: a request with no spec carries nothing to move.
    marge
        .call(
            "merge_entities",
            json!({"duplicate": "bot-request:beta", "survivor": "topic:scrapbook"}),
        )
        .await;
    story.finish().await;
}

/// **An ordinary thing that carries a status key is not a request, and the
/// refusal that keeps it out of a request tells the caller to archive the claim
/// that sets the key.** The request is the survivor, so the key cannot land on
/// it. The merge judges every key the thing wrote, and clearing a key is a write
/// of it, so the merge is still refused after a clear. Archiving the claim takes
/// its writes out of that history: the merge then lands, and the request is left
/// without the key.
#[tokio::test]
async fn an_ordinary_thing_merged_into_a_request_is_told_to_archive_the_claim_that_sets_its_status_key()
 {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot-request", "handle": "beta", "name": "Homer waits",
                   "source": "user-named"}),
        )
        .await;
    let created = marge
        .call(
            "add_entity",
            json!({"kind": "topic", "handle": "scrapbook", "name": "Scrapbook",
                   "source": "user-named", "sets": {"observed_state": "running"}}),
        )
        .await;
    let address = created.field("address");

    marge
        .refused(
            "merge_entities",
            json!({"duplicate": "topic:scrapbook", "survivor": "bot-request:beta"}),
        )
        .await
        .says("topic:scrapbook")
        .says("bot-request:beta")
        .says("archived")
        .never_says("clear_fields");
    marge
        .call(
            "update_fact",
            json!({"address": &address, "clear_fields": ["observed_state"]}),
        )
        .await;
    marge
        .refused(
            "merge_entities",
            json!({"duplicate": "topic:scrapbook", "survivor": "bot-request:beta"}),
        )
        .await
        .says("observed_state")
        .says("archived");
    marge
        .call(
            "update_fact",
            json!({"address": &address, "status": "archived"}),
        )
        .await;
    marge
        .call(
            "merge_entities",
            json!({"duplicate": "topic:scrapbook", "survivor": "bot-request:beta"}),
        )
        .await;
    let request = marge
        .call("recall", json!({"subject": "bot-request:beta"}))
        .await
        .json();
    assert!(
        request["objects"][0]["fields"]
            .get("observed_state")
            .is_none(),
        "the merge put the archived status key on the request: {request}"
    );
    story.finish().await;
}

/// **A manager sends a `reports_to` beside the target, and the target's own
/// chain still judges the request.** Marge manages homer and not lisa. She names
/// homer as the manager and lisa as the target, so the request is asked for a
/// bot nobody above her authorised. Bart manages lisa and makes the same write.
#[tokio::test]
async fn naming_a_manager_beside_the_target_does_not_change_who_judges_it() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge.add("bot:bart", "Bart").await;
    marge.add("machine:theta", "Theta").await;
    let bart = story.as_bot("bot:bart").await;
    for (by, handle, name, boss) in [
        (&marge, "homer", "Homer", "bot:marge"),
        (&bart, "lisa", "Lisa", "bot:bart"),
    ] {
        by.call(
            "add_entity",
            json!({"kind": "bot", "handle": handle, "name": name,
                   "source": "user-named", "sets": {"reports_to": boss}}),
        )
        .await;
    }
    let mut asked = spec("bot:lisa", "machine:theta");
    asked["reports_to"] = json!("bot:homer");

    marge
        .refused("add_entity", creating(asked.clone()))
        .await
        .says("\"wrote\":false")
        .says("bot:bart");
    bart.call("add_entity", creating(asked)).await;
    story.finish().await;
}

/// **Taking back an older partial spec changes the folded spec, so the
/// retraction carries the next generation.** A names the whole spec (1), B only
/// `desired` (2), C only `model` (3); B taken back leaves a spec no number
/// named, so the retraction carries 4. A withdrawal that uncovers nothing
/// carries none: D is wholly overridden by the newer E, and taking D back
/// changes no key.
#[tokio::test]
async fn taking_back_an_older_partial_spec_carries_the_next_generation() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("machine:theta", "Theta").await;
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    let read = || async {
        let read = marge
            .call("recall", json!({"subject": "bot-request:alpha"}))
            .await;
        let fields = read.json()["objects"][0]["fields"].clone();
        (
            fields["generation"].clone(),
            fields["desired"].clone(),
            fields["model"].clone(),
        )
    };
    let capture = |content: &'static str, sets: serde_json::Value| {
        let marge = &marge;
        async move {
            marge
                .call(
                    "capture",
                    json!({"subject": "bot-request:alpha", "content": content, "sets": sets}),
                )
                .await
                .field("address")
        }
    };
    assert_eq!(
        read().await,
        ("1".into(), "running".into(), "model-a".into())
    );
    let older = capture("stop it", json!({"desired": "stopped"})).await;
    capture("another model", json!({"model": "model-b"})).await;
    assert_eq!(
        read().await,
        ("3".into(), "stopped".into(), "model-b".into())
    );

    marge
        .call(
            "retract",
            json!({"address": &older, "reason": "written in error"}),
        )
        .await;
    assert_eq!(
        read().await,
        ("4".into(), "running".into(), "model-b".into()),
        "the spec is a new one, and the number names it",
    );

    let overridden = capture("pause it", json!({"desired": "paused"})).await;
    capture("stop it for good", json!({"desired": "halted"})).await;
    assert_eq!(read().await.0, "6");
    marge
        .call(
            "retract",
            json!({"address": &overridden, "reason": "written in error"}),
        )
        .await;
    assert_eq!(
        read().await,
        ("6".into(), "halted".into(), "model-b".into()),
        "a withdrawal that uncovers nothing carries no number",
    );
    story.finish().await;
}

/// **Archiving a claim of a request changes its folded spec the way a retraction
/// does, so the edit carries the next generation on a live claim.** The sequence
/// of the retraction story: B archived gives 4. The claim that carries the
/// current number is archived too: C holds 3, the archive leaves a spec no number
/// named, so the number goes up to 5 and never falls back to an older one.
#[tokio::test]
async fn archiving_a_claim_of_a_request_carries_the_next_generation() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("machine:theta", "Theta").await;
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    let read = || async {
        let read = marge
            .call("recall", json!({"subject": "bot-request:alpha"}))
            .await;
        let fields = read.json()["objects"][0]["fields"].clone();
        (
            fields["generation"].clone(),
            fields["desired"].clone(),
            fields["model"].clone(),
        )
    };
    let capture = |content: &'static str, sets: serde_json::Value| {
        let marge = &marge;
        async move {
            marge
                .call(
                    "capture",
                    json!({"subject": "bot-request:alpha", "content": content, "sets": sets}),
                )
                .await
                .field("address")
        }
    };
    let archive = |address: String| {
        let marge = &marge;
        async move {
            marge
                .call(
                    "update_fact",
                    json!({"address": address, "status": "archived",
                           "details": "written in error"}),
                )
                .await;
        }
    };
    let older = capture("stop it", json!({"desired": "stopped"})).await;
    let newest = capture("another model", json!({"model": "model-b"})).await;
    assert_eq!(
        read().await,
        ("3".into(), "stopped".into(), "model-b".into())
    );

    archive(older).await;
    assert_eq!(
        read().await,
        ("4".into(), "running".into(), "model-b".into()),
        "the spec is a new one, and the number names it",
    );

    archive(newest).await;
    assert_eq!(
        read().await,
        ("5".into(), "running".into(), "model-a".into()),
        "the claim that held the number is archived, and the number goes up",
    );
    story.finish().await;
}

/// **A merge that brings a spec key onto a request changes its folded spec, so
/// the merge carries the next generation on a live claim.** The duplicate was
/// written after the request, so its `desired` and `model` win; the merge leaves
/// a spec no number named and the number goes from 1 to 2. A merge that brings no
/// spec key leaves the spec as it was and carries none.
#[tokio::test]
async fn merging_a_spec_onto_a_request_carries_the_next_generation() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("machine:theta", "Theta").await;
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    let read = || async {
        let read = marge
            .call("recall", json!({"subject": "bot-request:alpha"}))
            .await;
        let fields = read.json()["objects"][0]["fields"].clone();
        (
            fields["generation"].clone(),
            fields["desired"].clone(),
            fields["model"].clone(),
        )
    };
    assert_eq!(
        read().await,
        ("1".into(), "running".into(), "model-a".into())
    );
    marge
        .call(
            "add_entity",
            json!({"kind": "topic", "handle": "tarps", "name": "Tarps",
                   "source": "user-named",
                   "sets": {"desired": "stopped", "model": "model-b"}}),
        )
        .await;
    marge.add("topic:pegs", "Pegs").await;

    marge
        .call(
            "merge_entities",
            json!({"duplicate": "topic:tarps", "survivor": "bot-request:alpha"}),
        )
        .await;
    assert_eq!(
        read().await,
        ("2".into(), "stopped".into(), "model-b".into()),
        "the merge made a new spec, and the number names it",
    );

    marge
        .call(
            "merge_entities",
            json!({"duplicate": "topic:pegs", "survivor": "bot-request:alpha"}),
        )
        .await;
    assert_eq!(
        read().await,
        ("2".into(), "stopped".into(), "model-b".into()),
        "a merge that brings no spec key carries no number",
    );
    story.finish().await;
}

/// **Taking back the claim that carries the newest number, under a later resend
/// that covers its spec, still carries the next number.** The spec is the same
/// before and after, but the fold of the generation would fall to an older number
/// that named another spec. A reads 1; D sets `desired` paused and reads 2; E
/// sends paused again, a resend that carries no number. Taking D back leaves the
/// spec as it was, and the number goes to 3 and never down to 1. An archive of
/// such a carrier does the same.
#[tokio::test]
async fn taking_back_a_covered_carrier_carries_the_next_generation() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("machine:theta", "Theta").await;
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;
    let read = || async {
        let read = marge
            .call("recall", json!({"subject": "bot-request:alpha"}))
            .await;
        let fields = read.json()["objects"][0]["fields"].clone();
        (fields["generation"].clone(), fields["desired"].clone())
    };
    let capture = |content: &'static str, sets: serde_json::Value| {
        let marge = &marge;
        async move {
            marge
                .call(
                    "capture",
                    json!({"subject": "bot-request:alpha", "content": content, "sets": sets}),
                )
                .await
                .field("address")
        }
    };
    let carrier = capture("pause it", json!({"desired": "paused"})).await;
    capture("pause it again", json!({"desired": "paused"})).await;
    assert_eq!(read().await, ("2".into(), "paused".into()));

    marge
        .call(
            "retract",
            json!({"address": &carrier, "reason": "written in error"}),
        )
        .await;
    assert_eq!(
        read().await,
        ("3".into(), "paused".into()),
        "the spec is as it was, and the number never falls back to 1",
    );

    let carrier = capture("halt it", json!({"desired": "halted"})).await;
    capture("halt it again", json!({"desired": "halted"})).await;
    assert_eq!(read().await, ("4".into(), "halted".into()));
    marge
        .call(
            "update_fact",
            json!({"address": &carrier, "status": "archived",
                   "details": "written in error"}),
        )
        .await;
    assert_eq!(
        read().await,
        ("5".into(), "halted".into()),
        "an archive of the covered carrier carries the next number too",
    );
    story.finish().await;
}

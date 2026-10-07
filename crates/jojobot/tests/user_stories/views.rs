//! "Who are my colleagues, and what has gone quiet?"
//!
//! **Two questions a session asks by name.** It declared neither and it does
//! not build a query: it names a view and reads the answer. The software ships
//! these two, so a session that was told nothing can still ask them.
//!
//! **The point of the design is what this story CANNOT show.** A shipped view
//! and a view the operator declared come back through the same read, and
//! nothing in the answer says which was which — because there is nothing to
//! say. Both are records of the same shape and the read that runs one cannot
//! tell where it came from.
//!
//! **The bot directory is the first real use** (rule 139): a bot asking who
//! else is here, answered as a question over the graph rather than as a verb of
//! its own.
//!
//! **Every half is paired**: a shipped view beside a declared one in one read,
//! and a view that exists beside a name that is not one.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_session_asks_a_shipped_view_and_its_own_by_name_through_one_read() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    // ── the directory, which nobody declared ────────────────────────────────
    //
    // The smoke test: a bot asking who its colleagues are. There is no verb
    // for this and there does not need to be.
    let colleagues = s.call("recall", json!({"view": "colleagues"})).await;
    colleagues.says("bot:gamma");
    colleagues.says("bot:assistant");
    // **The small list by default.** A charter can run to thousands of
    // characters, so the view stops short of shipping every one of them
    // unasked — the caller's own `charter: true` is what reaches it. Without
    // this half the case passes on a build that ships every charter whether
    // anybody asked or not.
    assert!(
        !colleagues.raw().contains("THEIR WORD IS GROUND TRUTH"),
        "the default answer does not carry a charter: {}",
        colleagues.raw(),
    );

    // The positive the assertion above rests on: asking for the charter still
    // reaches it, because the caller's own arguments win over what the view
    // fills in.
    let with_charters = s
        .call("recall", json!({"view": "colleagues", "charter": true}))
        .await;
    with_charters.says("THEIR WORD IS GROUND TRUTH");

    // ── a view the operator declares, through the ordinary surface ──────────
    //
    // `add_entity` of kind view, and the keys are the query. No new verb.
    s.add("view:my-people", "My People").await;
    s.event_with(
        "view:my-people",
        "the people I keep an eye on",
        json!({"selects": "person"}),
        &[],
    )
    .await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("person:ralph", "Ralph").await;

    // **The same read, and it does not care which half supplied the view.**
    let mine = s.call("recall", json!({"view": "my-people"})).await;
    mine.says("person:milhouse");
    mine.says("person:ralph");
    assert!(
        !mine.raw().contains("bot:gamma"),
        "the view selects people, so the bots are not in it: {}",
        mine.raw(),
    );

    // ── a name that is no view is blocked, and says what there is ───────────
    //
    // The pairing the two answers above rest on: without it, a build that
    // answered everything with everything would pass.
    let refused = s
        .refused("recall", json!({"view": "whatever-i-guessed"}))
        .await;
    refused.says("colleagues");

    // ── a shipped view's name is not the operator's to take ────────────────
    //
    // **What the build supplies behaves like a stored row** (rule 234), so the
    // handle guard has to see it — a guard that reads only the store sees no
    // collision here and lets a second thing answer to one name.
    let taken = s
        .refused(
            "add_entity",
            json!({
                "kind": "view", "handle": "colleagues", "name": "My Colleagues",
                "source": "user-named",
            }),
        )
        .await;
    // The way forward is a name of their own, as it is for a shipped kind.
    taken.says("colleagues");

    // …and the pairing it rests on: a name of the operator's own still lands —
    // once confirmed over the near-miss the guard surfaces, because "loops"
    // inside "my-loops" collides with the shipped view exactly as it would
    // with a stored one (rule 234). Without this the case passes on a build
    // that refuses every view there is.
    s.add_over_the_screen("view:my-loops", "My Loops").await;

    story.finish().await;
}

/// **The colleagues view says who each bot reports to, and who reports to a bot
/// is a second call.** A bot carries `reports_to` holding its manager's handle.
/// The view reads that out of each bot's own fields and does not walk inward,
/// because a walk repeats every report in full under its manager and makes the
/// directory larger; the inbound side is `recall` of the manager with that
/// relation followed inward.
#[tokio::test]
async fn the_colleagues_view_says_who_each_bot_reports_to_and_a_second_call_says_who_reports_to_it()
{
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.add("bot:sigma", "Sigma").await;
    s.add("bot:psi", "Psi").await;
    for report in ["bot:sigma", "bot:psi"] {
        s.event_with(
            report,
            "answers to omega",
            json!({"reports_to": "bot:omega"}),
            &[],
        )
        .await;
    }

    let colleagues = s.call("recall", json!({"view": "colleagues"})).await.json();
    let objects = colleagues["objects"].as_array().expect("a list of bots");
    let bot = |handle: &str| {
        objects
            .iter()
            .find(|object| object["id"] == handle)
            .unwrap_or_else(|| panic!("the view lists {handle}: {colleagues}"))
    };
    let ids = |list: &serde_json::Value| -> Vec<String> {
        let mut found: Vec<String> = list
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        found.sort();
        found
    };

    // Out: a report names its manager, in its own fields.
    assert_eq!(bot("bot:sigma")["fields"]["reports_to"], "bot:omega");
    assert_eq!(bot("bot:psi")["fields"]["reports_to"], "bot:omega");
    // In: the view does not list who reports to a manager. It reaches nobody.
    assert!(
        ids(&bot("bot:omega")["connected"]).is_empty(),
        "the view walks nothing inward: {colleagues}"
    );
    // The second call answers it: the manager, with `reports_to` followed
    // inward, lists the bots that report to it. Paired with the line above, so
    // the view's silence is the view's and not an answer nobody can give.
    let reports = s
        .call(
            "recall",
            json!({
                "subject": "bot:omega",
                "follow": {"relation": "reports_to", "direction": "in"},
            }),
        )
        .await
        .json();
    assert_eq!(
        ids(&reports["objects"][0]["connected"]),
        ["bot:psi", "bot:sigma"],
        "{reports}"
    );
    // The pairing: a bot nobody reports to has nobody under it.
    let none = s
        .call(
            "recall",
            json!({
                "subject": "bot:gamma",
                "follow": {"relation": "reports_to", "direction": "in"},
            }),
        )
        .await
        .json();
    assert!(
        ids(&none["objects"][0]["connected"]).is_empty(),
        "nobody reports to gamma: {none}"
    );
    story.finish().await;
}

/// **The colleagues view shows each bot's one-liner and who it reports to, and
/// leaves the operational keys out, naming them.** A bot holds keys that run it
/// and that nobody reading the directory asked for; the view says which it
/// kept, says which it left out, and the caller's own `keys` and a plain recall
/// both reach them.
#[tokio::test]
async fn the_colleagues_view_leaves_operational_keys_out_and_names_what_it_left_out() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.add("bot:sigma", "Sigma").await;
    s.event_with(
        "bot:omega",
        "what omega is for",
        json!({"one_liner": "runs the office", "probe/last": "2026-10-01"}),
        &[],
    )
    .await;
    s.event_with(
        "bot:sigma",
        "what sigma is for",
        json!({
            "one_liner": "keeps the books", "reports_to": "bot:omega",
            "probe/last": "2026-10-02", "nudge/quiet": "on",
        }),
        &[],
    )
    .await;

    let colleagues = s.call("recall", json!({"view": "colleagues"})).await.json();
    let objects = colleagues["objects"].as_array().expect("a list of bots");
    let bot = |handle: &str| {
        objects
            .iter()
            .find(|object| object["id"] == handle)
            .unwrap_or_else(|| panic!("the view lists {handle}: {colleagues}"))
    };

    // Up front: the one-liner and the manager, and nothing else.
    assert_eq!(
        bot("bot:sigma")["fields"],
        json!({"one_liner": "keeps the books", "reports_to": "bot:omega"}),
        "{colleagues}"
    );
    // What it left out is named, key by key.
    let left_out = bot("bot:sigma")["fields_left_out"]
        .as_str()
        .unwrap_or_else(|| panic!("sigma names what it left out: {colleagues}"));
    assert!(
        left_out.contains("probe/last") && left_out.contains("nudge/quiet"),
        "{left_out}"
    );
    // A walk narrows the objects it reaches the same way: the second call that
    // lists who reports to omega, given the same `keys`, returns sigma without
    // its operational keys and says which it left out.
    let walked = s
        .call(
            "recall",
            json!({
                "subject": "bot:omega",
                "follow": {"relation": "reports_to", "direction": "in"},
                "keys": ["one_liner", "reports_to"],
            }),
        )
        .await
        .json();
    let reached = &walked["objects"][0]["connected"][0];
    assert_eq!(reached["id"], "bot:sigma", "{walked}");
    assert_eq!(
        reached["fields"],
        json!({"one_liner": "keeps the books", "reports_to": "bot:omega"}),
        "{walked}"
    );
    assert!(
        reached["fields_left_out"]
            .as_str()
            .is_some_and(|note| note.contains("nudge/quiet")),
        "{walked}"
    );
    // The pairing: a bot that holds no operational key has nothing to name.
    assert!(
        bot("bot:gamma").get("fields_left_out").is_none(),
        "{colleagues}"
    );

    // The caller's own `keys` wins over the view's, and reaches what it kept out.
    let mine = s
        .call(
            "recall",
            json!({"view": "colleagues", "keys": ["probe/last"]}),
        )
        .await
        .json();
    let sigma = mine["objects"]
        .as_array()
        .and_then(|list| list.iter().find(|object| object["id"] == "bot:sigma"))
        .unwrap_or_else(|| panic!("the view lists sigma: {mine}"));
    assert_eq!(
        sigma["fields"],
        json!({"probe/last": "2026-10-02"}),
        "{mine}"
    );

    // And the call the note points at returns every key.
    s.call("recall", json!({"subject": "bot:sigma"}))
        .await
        .says("nudge/quiet");
    story.finish().await;
}

/// **The note a view's narrowing leaves tells the truth about the way back.**
/// When `keys` came from the view, asking the view again without `keys` applies
/// the view's keys again and loops, so the note names the two calls that do
/// return every key: leave the view out, or pass `keys` naming the ones wanted.
/// Both are followed here as the note gives them, and a direct `keys` call, where
/// "ask again without keys" is true, says nothing of a view.
#[tokio::test]
async fn the_note_a_view_leaves_names_calls_that_return_the_keys_it_left_out() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.event_with(
        "bot:omega",
        "what omega is for",
        json!({"one_liner": "runs the office", "probe/last": "2026-10-01", "nudge/quiet": "on"}),
        &[],
    )
    .await;
    let field_of = |answer: &serde_json::Value, handle: &str| -> serde_json::Value {
        answer["objects"]
            .as_array()
            .and_then(|list| list.iter().find(|object| object["id"] == handle))
            .unwrap_or_else(|| panic!("the answer lists {handle}: {answer}"))
            .clone()
    };

    // The view names its keys, so the note is about a view.
    let colleagues = s.call("recall", json!({"view": "colleagues"})).await.json();
    let omega = field_of(&colleagues, "bot:omega");
    let note = omega["fields_left_out"]
        .as_str()
        .unwrap_or_else(|| panic!("omega names what it left out: {colleagues}"));
    for word in ["view", "keys", "probe/last", "nudge/quiet"] {
        assert!(
            note.contains(word),
            "the note does not name `{word}`, so it does not say how to get the keys back: {note}"
        );
    }

    // Advice one, as given: leave the view out and send its kind.
    let without_the_view = s.call("recall", json!({"kind": "bot"})).await.json();
    let whole = field_of(&without_the_view, "bot:omega");
    assert_eq!(whole["fields"]["probe/last"], "2026-10-01", "{whole}");
    assert_eq!(whole["fields"]["nudge/quiet"], "on", "{whole}");
    // Advice two, as given: the view again, with `keys` naming the keys left out.
    let named = s
        .call(
            "recall",
            json!({"view": "colleagues", "keys": ["probe/last", "nudge/quiet"]}),
        )
        .await
        .json();
    let wanted = field_of(&named, "bot:omega");
    assert_eq!(wanted["fields"]["probe/last"], "2026-10-01", "{wanted}");
    assert_eq!(wanted["fields"]["nudge/quiet"], "on", "{wanted}");

    // The pairing: narrowed by the caller's own `keys`, the note says nothing of
    // a view, because "ask again without keys" is exactly right there.
    let direct = s
        .call("recall", json!({"kind": "bot", "keys": ["one_liner"]}))
        .await
        .json();
    let direct_omega = field_of(&direct, "bot:omega");
    let direct_note = direct_omega["fields_left_out"]
        .as_str()
        .unwrap_or_else(|| panic!("the direct call names what it left out: {direct}"));
    assert!(direct_note.contains("keys"), "{direct_note}");
    assert!(!direct_note.contains("view"), "{direct_note}");
    story.finish().await;
}

/// **A session that was told nothing finds the capability and uses it.**
///
/// The bar a view has to clear is not that it exists — it is that an agent
/// carrying no knowledge of it ends up asking one. So this walks the route a
/// cold session actually takes: the boot names the skill, the skill is fetched
/// by name, and **what comes out of it is a COMPOSED CALL that lands** rather
/// than a recital of what the axes are.
#[tokio::test]
async fn a_session_told_nothing_finds_the_view_path_and_composes_a_real_question() {
    let story = Story::begin("bot:gamma").await;
    let (booted, s) = story.full_boot().await;

    // ① The boot names the skill and what it is for, and ships no body — which
    // is what makes it free to know it exists.
    booted_names_the_skill(&booted);

    // ② Fetched by name, it names the view path and the shipped views.
    let skill = story.skill("asking").await.to_string();
    assert!(
        skill.contains("colleagues") && skill.contains("loops"),
        "the skill's worked examples are the views that really ship: {skill}",
    );
    // …and the sentence that stops an agent concluding a capability is absent.
    assert!(
        skill.contains("ARGUMENT on a verb you already know"),
        "the skill says where capabilities live on this surface: {skill}",
    );

    // ③ **The composed call.** Not the axes recited — a real question, built
    // from what the skill just said, that comes back with the answer.
    s.add("person:milhouse", "Milhouse").await;
    let composed = s
        .call("recall", json!({"view": "colleagues", "facts": true}))
        .await;
    composed.says("bot:assistant");
    // The argument sent beside the view won, which is what the skill promised.
    assert!(
        composed.raw().contains("\"facts\""),
        "what the caller sent beside the view name took: {}",
        composed.raw(),
    );

    story.finish().await;
}

/// 🚨 **A claim may point at a view the software ships.**
///
/// "Ask for the loops one again — that is the second time this week."
///
/// A session recording that draws a claim on the person with an edge at the
/// view, exactly as it would at any other record. **The read answered for the
/// view and the write guard refused it**, because the guard consulted the
/// stored rows while the read resolved what the build supplies — the two halves
/// disagreeing about what exists.
///
/// ⚠️ **And the refusal's advice made it worse**: a caller that did what it
/// said would create a stored record for a handle the build already owns.
///
/// ⭐ **The pair is that the edge LANDS and comes back pointing where it was
/// sent.** A build that waved the write through and dropped the edge would
/// satisfy "the refusal is gone" and lose the link.
#[tokio::test]
async fn a_claim_can_point_at_a_view_the_software_ships() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:milhouse", "Milhouse").await;

    // The read answers for the shipped view — the half that always worked.
    s.recall("view:loops").await.says("view:loops");

    let asked = s
        .fact_about(
            "person:milhouse",
            "asked for the loops view again, second time this week",
            "about",
            "view:loops",
        )
        .await;

    // **The edge landed and it points where it was sent.**
    s.recall("person:milhouse")
        .await
        .claim(&asked)
        .says("\"object\":\"view:loops\"")
        .says("second time this week");

    // ⭐ **And the link is walkable**, which is what an edge is for: what points
    // at the shipped view comes back by asking the view.
    s.shape(
        "what points at the loops view",
        json!({
            "subject": "view:loops",
            "follow": {"shape": "about", "direction": "in"},
        }),
    )
    .await
    .says("person:milhouse");

    s.wrap("recorded that the operator keeps asking for one view")
        .await;

    story.finish().await;
}

/// The boot's skill index carries this skill by name, with no body.
fn booted_names_the_skill(booted: &serde_json::Value) {
    let skills = booted["skills"].as_array().expect("the boot lists skills");
    let asking = skills
        .iter()
        .find(|s| s["name"] == "asking")
        .unwrap_or_else(|| panic!("a cold session is told this skill exists: {booted}"));
    assert!(
        asking["when_to_use"].is_string(),
        "…and what it is for, which is what a session compares against its job",
    );
    assert!(
        asking.get("body").is_none(),
        "…and it costs nothing: the index ships no bodies",
    );
}

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

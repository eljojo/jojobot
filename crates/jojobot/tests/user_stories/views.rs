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
    // …and it carries what each identity is FOR, because that is what the view
    // says to show. Without this the case passes on a build that answered with
    // bare handles.
    colleagues.says("THEIR WORD IS GROUND TRUTH");

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

    // …and the pairing it rests on: a name of the operator's own still lands.
    // Without this the case passes on a build that refuses every view there is.
    s.add("view:my-loops", "My Loops").await;

    story.finish().await;
}

//! "What did the last session leave me?"
//!
//! 🚨 **A run that closed properly used to disappear.** The boot door offers
//! runs still in flight and one that stopped without being wrapped up. A
//! wrapped run was in neither list, so the one sitting that told its story
//! vanished from the next boot while every sitting that merely stopped stayed
//! on the offer. **Closing cleanly was how you became invisible**, and across a
//! measured year the behaviour followed: after the first clean close, nobody
//! wrapped a session again.
//!
//! ⛔️ **The fix is not that a wrapped run becomes resumable.** `wrapped` is
//! terminal and stays terminal — nothing appends to it. It is handed over to
//! **read**, on its own key, and it is never among the runs a caller may pick
//! up. Conflating the two would break the never-auto-wrap rule and the resume
//! rule at once.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_session_that_closed_properly_is_what_the_next_one_is_handed() {
    let story = Story::begin("bot:otto").await;

    // ── ① a sitting that does its work and closes properly ──────────────────
    let first = story.session().await;
    first.add("person:milhouse", "Milhouse").await;
    first
        .fact("person:milhouse", "keeps the club's spare wheels")
        .await;
    let sid = first.sid().to_string();
    first
        .wrap(
            "the spare wheels are Milhouse's, not the club's. Whoever picks this up should ask \
             him before the next survey rather than assuming the club has any.",
        )
        .await;

    // ── ② the next sitting, which knows nothing ─────────────────────────────
    //
    // ⭐ It does not search and it is not told to look. It opens the one door
    // every sitting opens first, and the story is there.
    let next = story.session().await;
    let door = next
        .call("start_here", json!({ "bot": "otto", "brief": true }))
        .await;
    let answered = door.json();
    let handover = &answered["session"]["handover"];

    let told = handover["story"]
        .as_str()
        .unwrap_or_else(|| panic!("the door handed over no story: {answered}"));
    assert!(
        told.contains("spare wheels"),
        "the closing story never reached the next sitting: {told}",
    );

    // ⛔️ **Terminal stays terminal.** Readable, and not on the list of runs
    // this caller may pick up.
    let resumable: Vec<&str> = answered["session"]["choices"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["sid"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        !resumable.contains(&sid.as_str()),
        "a wrapped run is being offered as something to resume: {answered}",
    );

    story.finish().await;
}

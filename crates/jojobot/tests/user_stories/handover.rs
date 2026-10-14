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
        .fact("person:milhouse", "keeps the council's spare barriers")
        .await;
    let sid = first.run().to_string();
    first
        .wrap(
            "the spare barriers are Milhouse's, not the council's. Whoever picks this up should ask \
             him before the next inspection rather than assuming the council has any.",
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
        told.contains("spare barriers"),
        "the closing story never reached the next sitting: {told}",
    );

    // ⛔️ **Terminal stays terminal.** Readable, and not on the list of runs
    // this caller may pick up.
    let resumable: Vec<&str> = answered["session"]["choices"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["run"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        !resumable.contains(&sid.as_str()),
        "a wrapped run is being offered as something to resume: {answered}",
    );

    story.finish().await;
}

/// "I stopped twice without wrapping. Which one do I pick up?" The offer a boot
/// hands back names every run it offers, each by the name that `resume` takes,
/// beside what the run was working on. Resuming by one of those names lands on
/// that run and no other, and the offer hands out no key until one is picked.
#[tokio::test]
async fn the_resume_offer_names_each_run_and_resuming_by_one_lands() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    first
        .call(
            "journal",
            json!({"entry": "inspected the east gorge", "focus": "the east gorge inspection"}),
        )
        .await;
    let second = story.as_bot("bot:otto").await;
    second
        .call(
            "journal",
            json!({"entry": "priced the barriers", "focus": "pricing the replacement barriers"}),
        )
        .await;
    let (first_sid, second_sid) = (first.run().to_string(), second.run().to_string());
    assert_ne!(first_sid, second_sid, "two runs, not the same one twice");

    // The boot offers both, each by its sid and beside its focus.
    let (door, minted) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    assert!(minted.is_none(), "an offer mints no handle");
    let offer = door.json();
    assert!(
        offer["session"]["sid"].is_null(),
        "the offer hands out no sid of its own until one is picked: {offer}"
    );
    let choices = offer["session"]["choices"]
        .as_array()
        .unwrap_or_else(|| panic!("two runs in flight are offered back: {offer}"));
    let working_on = |sid: &str| {
        choices
            .iter()
            .find(|c| c["run"] == sid)
            .unwrap_or_else(|| panic!("{sid} is not named in the offer: {offer}"))["working_on"]
            .clone()
    };
    assert_eq!(
        working_on(&first_sid),
        "the east gorge inspection",
        "{offer}"
    );
    assert_eq!(
        working_on(&second_sid),
        "pricing the replacement barriers",
        "{offer}"
    );

    // Resuming by one of those sids lands on that run.
    let (resumed, _) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": first_sid}),
        )
        .await;
    let resumed = resumed.json();
    assert_eq!(
        resumed["session"]["run"], first_sid,
        "resuming by a name from the offer lands on that run: {resumed}"
    );
    assert_ne!(
        resumed["session"]["run"], second_sid,
        "and not on the other: {resumed}"
    );
    story.finish().await;
}

/// "The offer is the second time I have read the essay. What do I send back?" Send
/// nothing extra: the answer to the offer carries the run and its key, leaves out
/// what the offer shipped, and names the call that returns it. The offer carries
/// the essay, the snapshot and the handover story, so the saving is real.
#[tokio::test]
async fn the_answer_to_the_offer_carries_only_what_is_new_and_names_the_call_for_the_rest() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    first
        .call(
            "journal",
            json!({"entry": "inspected the east gorge", "focus": "the east gorge inspection"}),
        )
        .await;
    let first_sid = first.run().to_string();

    // The positive: the offer carries every part the answer then leaves out.
    let (door, minted) = story.call("start_here", json!({"bot": "otto"})).await;
    assert!(minted.is_none(), "an offer mints no handle");
    let offer = door.json();
    assert!(
        offer["orientation"]
            .as_str()
            .is_some_and(|essay| !essay.is_empty()),
        "the offer carries the essay: {offer}"
    );
    assert!(offer["snapshot"]["entities"].is_object(), "{offer}");
    assert!(offer["session"]["handover"].is_object(), "{offer}");
    assert!(offer.get("left_out").is_none(), "{offer}");
    let offer_code = offer["session"]["offer_code"]
        .as_str()
        .unwrap_or_else(|| panic!("the offer carries a code to answer it with: {offer}"))
        .to_string();

    // The negative: `resume` with no code is not an answer to the offer, so the
    // caller is handed the whole boot and nothing is named as left out.
    let (bare, _) = story
        .call("start_here", json!({"bot": "otto", "resume": first_sid}))
        .await;
    let bare = bare.json();
    assert!(bare["orientation"].is_string(), "{bare}");
    assert!(bare["snapshot"]["entities"].is_object(), "{bare}");
    assert!(bare["session"]["handover"].is_object(), "{bare}");
    assert!(bare.get("left_out").is_none(), "{bare}");

    // The answer, with the offer's code beside `resume`: no essay, no snapshot,
    // no handover, and the run is still handed back with the key to write with.
    let (answered, _) = story
        .call(
            "start_here",
            json!({"bot": "otto", "resume": first_sid, "offer_code": offer_code}),
        )
        .await;
    let answered = answered.json();
    assert!(answered["orientation"].is_null(), "{answered}");
    assert!(answered["snapshot"].is_null(), "{answered}");
    assert!(answered["session"].get("handover").is_none(), "{answered}");
    assert_eq!(answered["session"]["run"], first_sid, "{answered}");
    assert!(
        answered["session"]["sid"].is_string(),
        "and the answer still carries the key to write with: {answered}"
    );

    // Each left-out part is named, with the call that returns it.
    let left_out = &answered["left_out"];
    let parts: Vec<&str> = left_out["parts"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer names what it left out: {answered}"))
        .iter()
        .filter_map(|part| part.as_str())
        .collect();
    for part in ["orientation", "snapshot", "handover"] {
        assert!(parts.contains(&part), "{part} is not named: {answered}");
    }
    assert_eq!(left_out["returned_by"]["tool"], "start_here", "{answered}");

    // The call it names returns them.
    let (again, _) = story
        .call("start_here", left_out["returned_by"]["arguments"].clone())
        .await;
    let again = again.json();
    assert!(again["orientation"].is_string(), "{again}");
    assert!(again["snapshot"]["entities"].is_object(), "{again}");
    assert!(again["session"]["handover"].is_object(), "{again}");
    story.finish().await;
}

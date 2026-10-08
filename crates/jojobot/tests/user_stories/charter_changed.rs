//! "Nobody told me my charter had changed."
//!
//! A session works to the charter it was handed when it booted. Another
//! identity rewrites that charter while the session runs, and the session keeps
//! working to the old text until its next boot, because nothing it does would
//! make it read the new one.
//!
//! So the status bar carries one entrant for it: that the charter changed, when,
//! by whom, and the call that reads it. Reading the charter is what ends it. A
//! bot rewriting its own charter needs no notice, and a session that booted
//! after the change was handed the new text.

use serde_json::{Value, json};

use super::dsl::{Answer, Story};

/// The entrant, or `Null` when the answer carries none.
fn entrant(answer: &Answer) -> Value {
    answer.json()["status_bar"]["charter_changed"].clone()
}

#[tokio::test]
async fn a_charter_rewritten_by_another_identity_is_told_to_the_running_session_until_it_reads_it()
{
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    // A run that has written nothing has no card yet, and it is told all the same.
    let idle = story.as_bot("bot:gamma").await;

    // ── the quiet case first, and everything below rests on it ──────────────
    let quiet = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&quiet), Value::Null, "{}", quiet.raw());

    // ── another identity rewrites gamma's charter ───────────────────────────
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Holds the plan and does not implement." }),
        )
        .await;

    // ── gamma's next answer, of any verb, says so ───────────────────────────
    for (verb, args) in [
        ("list_entities", json!({ "kind": "bot" })),
        ("search", json!({ "query": "delta" })),
    ] {
        let told = gamma.call(verb, args).await;
        let said = entrant(&told);
        assert_eq!(said["by"], "bot:delta", "{verb}: {}", told.raw());
        assert!(
            said["at"].as_str().is_some_and(|at| !at.is_empty()),
            "it says when: {}",
            told.raw()
        );
        assert_eq!(
            said["read_with"],
            json!({
                "verb": "recall",
                "arguments": { "subject": "bot:gamma", "charter": true },
            }),
            "it names the call that reads the charter: {}",
            told.raw()
        );
    }

    let told = idle.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&told)["by"], "bot:delta", "{}", told.raw());

    // ── a refusal carries it too ────────────────────────────────────────────
    let turned_down = gamma
        .refused("recall", json!({ "subject": "person:zzz-nobody" }))
        .await;
    assert_eq!(
        entrant(&turned_down)["by"],
        "bot:delta",
        "{}",
        turned_down.raw()
    );

    // ── something else in the bar does not end it ───────────────────────────
    gamma.drain().await;
    let still = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&still)["by"], "bot:delta", "{}", still.raw());

    // ── reading the charter is what ends it ─────────────────────────────────
    let read = gamma
        .call("recall", json!({ "subject": "bot:gamma", "charter": true }))
        .await;
    read.says("Holds the plan");
    assert_eq!(entrant(&read), Value::Null, "{}", read.raw());
    let after = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&after), Value::Null, "{}", after.raw());

    // ── reading ends it for the session that read, and for that one only ────
    let still = idle.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&still)["by"], "bot:delta", "{}", still.raw());

    // ── and it comes back for the next change ───────────────────────────────
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Holds the plan. Reviews slices." }),
        )
        .await;
    let again = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&again)["by"], "bot:delta", "{}", again.raw());

    // ── a rename of the identity that wrote it moves what the entrant says ──
    gamma
        .call(
            "rename_entity",
            json!({ "handle": "bot:delta", "to": "bot:epsilon" }),
        )
        .await;
    let renamed = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&renamed)["by"], "bot:epsilon", "{}", renamed.raw());

    story.finish().await;
}

#[tokio::test]
async fn a_bot_rewriting_its_own_charter_and_a_session_booted_after_a_change_are_told_nothing() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    // Another run of gamma, written nothing yet, which never reads its charter.
    let idle = story.as_bot("bot:gamma").await;

    // ── gamma rewrites its own charter: nothing to tell it ──────────────────
    gamma
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Written by gamma itself." }),
        )
        .await;
    let own = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&own), Value::Null, "{}", own.raw());

    // ── delta rewrites it; gamma is told, positive for the pair below ───────
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Written by delta." }),
        )
        .await;
    let told = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&told)["by"], "bot:delta", "{}", told.raw());

    // ── gamma rewriting it again does not hide delta's change from another run
    // ── of gamma that has not read it ───────────────────────────────────────
    gamma
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Written by gamma, again." }),
        )
        .await;
    let idle_told = idle.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(
        entrant(&idle_told)["by"],
        "bot:delta",
        "{}",
        idle_told.raw()
    );

    // ── a gamma booted after the change was handed the new text ─────────────
    let later = story.as_bot("bot:gamma").await;
    let quiet = later.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&quiet), Value::Null, "{}", quiet.raw());

    story.finish().await;
}

/// The `sid` of the one run of gamma a second boot offers back.
fn offered(door: &Answer) -> String {
    let choices = door.json()["session"]["choices"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(choices.len(), 1, "one run to pick up: {}", door.raw());
    choices[0]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the offer names no run: {}", door.raw()))
        .to_string()
}

/// **A resume does not count as being told.** A resume withholds the charter, so
/// a run picked up after another identity rewrote it has not been handed the new
/// text, and it is told until it reads it. The same holds when the server
/// restarted between the change and the resume.
#[tokio::test]
async fn a_resumed_run_is_told_until_it_reads_the_charter_the_same_after_a_restart() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    // A write makes the run a card, which is what a later boot can offer back.
    gamma.journal("working to the first charter").await;
    let delta = story.as_bot("bot:delta").await;
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "The second charter." }),
        )
        .await;

    // ── resumed on the same server ──────────────────────────────────────────
    let (door, none) = story
        .call("start_here", json!({ "bot": "gamma", "brief": true }))
        .await;
    assert!(none.is_none(), "an offer, not a handle: {}", door.raw());
    let sid = offered(&door);
    let (_, resumed) = story
        .call(
            "start_here",
            json!({ "bot": "gamma", "brief": true, "resume": sid }),
        )
        .await;
    let resumed = resumed.expect("the run is picked up");
    let told = resumed
        .call("list_entities", json!({ "kind": "bot" }))
        .await;
    assert_eq!(entrant(&told)["by"], "bot:delta", "{}", told.raw());
    resumed
        .call("recall", json!({ "subject": "bot:gamma", "charter": true }))
        .await;
    let after = resumed
        .call("list_entities", json!({ "kind": "bot" }))
        .await;
    assert_eq!(entrant(&after), Value::Null, "{}", after.raw());

    // ── a restart forgets nothing: the read is on the card, not in the process
    let again = story.restarted().await;
    let (door, none) = again
        .call("start_here", json!({ "bot": "gamma", "brief": true }))
        .await;
    assert!(none.is_none(), "an offer, not a handle: {}", door.raw());
    let sid = offered(&door);
    let (_, resumed) = again
        .call(
            "start_here",
            json!({ "bot": "gamma", "brief": true, "resume": sid }),
        )
        .await;
    let resumed = resumed.expect("the run is picked up after a restart");
    let quiet = resumed
        .call("list_entities", json!({ "kind": "bot" }))
        .await;
    assert_eq!(entrant(&quiet), Value::Null, "{}", quiet.raw());

    // ── a change after that read, and a restart before the run learns of it ─
    let delta = again.as_bot("bot:delta").await;
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "The third charter." }),
        )
        .await;
    let later = again.restarted().await;
    let (door, none) = later
        .call("start_here", json!({ "bot": "gamma", "brief": true }))
        .await;
    assert!(none.is_none(), "an offer, not a handle: {}", door.raw());
    let sid = offered(&door);
    let (_, resumed) = later
        .call(
            "start_here",
            json!({ "bot": "gamma", "brief": true, "resume": sid }),
        )
        .await;
    let resumed = resumed.expect("the run is picked up after a second restart");
    let told = resumed
        .call("list_entities", json!({ "kind": "bot" }))
        .await;
    assert_eq!(entrant(&told)["by"], "bot:delta", "{}", told.raw());
    resumed
        .call("recall", json!({ "subject": "bot:gamma", "charter": true }))
        .await;
    let after = resumed
        .call("list_entities", json!({ "kind": "bot" }))
        .await;
    assert_eq!(entrant(&after), Value::Null, "{}", after.raw());

    later.finish().await;
}

/// **A run that read the charter before it had a card keeps that when the card
/// is made.** The run has written nothing when the first change lands and when
/// it reads; another change lands before its first write makes the card, and a
/// restart after that still tells it about that change.
#[tokio::test]
async fn a_charter_read_before_the_run_had_a_card_is_remembered_by_the_card_it_makes() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    // This run has written nothing, so it has no card yet.
    let idle = story.as_bot("bot:gamma").await;
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "The second charter." }),
        )
        .await;
    let told = idle.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&told)["by"], "bot:delta", "{}", told.raw());
    idle.call("recall", json!({ "subject": "bot:gamma", "charter": true }))
        .await;
    // **A change after the read, before the run has a card**, and then the first
    // write makes the card. The card must remember the read, or it would start
    // after the change and a restart would hide it.
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "The third charter." }),
        )
        .await;
    idle.journal("read the second charter").await;

    let again = story.restarted().await;
    let (door, none) = again
        .call("start_here", json!({ "bot": "gamma", "brief": true }))
        .await;
    assert!(none.is_none(), "an offer, not a handle: {}", door.raw());
    let choices = door.json()["session"]["choices"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let sid = choices
        .iter()
        .find(|choice| {
            choice["working_on"]
                .as_str()
                .is_some_and(|f| f.contains("second charter"))
        })
        .and_then(|choice| choice["sid"].as_str())
        .unwrap_or_else(|| panic!("the run that journalled is offered back: {}", door.raw()))
        .to_string();
    let (_, resumed) = again
        .call(
            "start_here",
            json!({ "bot": "gamma", "brief": true, "resume": sid }),
        )
        .await;
    let resumed = resumed.expect("the run is picked up");
    let told = resumed
        .call("list_entities", json!({ "kind": "bot" }))
        .await;
    assert_eq!(entrant(&told)["by"], "bot:delta", "{}", told.raw());

    again.finish().await;
}

/// **Reading the instance's charter text through `prose` ends the notice too.**
/// `recall` with `prose: true` on one's own bot returns the same text a charter
/// read returns, so a session that read it that way has it. Reading another
/// bot's page does not end a notice about one's own.
#[tokio::test]
async fn reading_ones_own_bots_prose_ends_the_notice_and_another_bots_does_not() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Written by delta." }),
        )
        .await;
    let told = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&told)["by"], "bot:delta", "{}", told.raw());

    // ── another bot's page is not gamma's charter ───────────────────────────
    let other = gamma
        .call("recall", json!({ "subject": "bot:delta", "prose": true }))
        .await;
    assert_eq!(
        entrant(&other)["by"],
        "bot:delta",
        "reading another bot's page ends nothing: {}",
        other.raw()
    );

    // ── without the prose the page is not read either ───────────────────────
    let bare = gamma
        .call("recall", json!({ "subject": "bot:gamma" }))
        .await;
    assert_eq!(entrant(&bare)["by"], "bot:delta", "{}", bare.raw());

    // ── gamma's own page, prose asked for, is the charter text ──────────────
    let read = gamma
        .call("recall", json!({ "subject": "bot:gamma", "prose": true }))
        .await;
    read.says("Written by delta.");
    assert_eq!(entrant(&read), Value::Null, "{}", read.raw());
    let after = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(entrant(&after), Value::Null, "{}", after.raw());

    story.finish().await;
}

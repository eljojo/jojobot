//! "I never wrote a charter for the assistant. Why does it already know what
//! it is for — and where did my own line go?"
//!
//! **Because the software supplies data, not just behaviour.** The text lives
//! in the build and a read resolves it in; the operator's own line is the only
//! half the store holds. A session reads one charter and cannot tell which
//! words came from where, which is the point: **no verb here knows that shipped
//! data exists.**
//!
//! **The trap this story exists to pin.** A session reads the charter, adds a
//! line and sends the whole thing back — the most ordinary edit there is. If
//! that landed, the build's own words would become this instance's and would
//! stop moving when the software does. It comes back blocked, and **the refusal
//! has to be actionable by a caller who has never heard of any of this**, so
//! the story reads it the way a session would rather than asserting an error
//! type.
//!
//! **Every half is paired with its opposite in the same story**: the refusal
//! beside the write that lands, and the shipped text beside a bot that has none.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_session_reads_one_charter_and_never_learns_which_half_shipped() {
    // The story's own bot is somebody the operator stood up; the session boots
    // as the identity the software ships, which is the one it supplies for.
    let story = Story::begin("bot:gamma").await;
    let s = story.as_bot("assistant").await;

    // ── it already knows what it is for, and nobody wrote that ──────────────
    let booted = s.call("start_here", json!({"bot": "assistant"})).await;
    booted.says("THEIR WORD IS GROUND TRUTH");

    // ── the operator adds their own line ────────────────────────────────────
    s.call(
        "set_charter",
        json!({"bot": "assistant", "prose": "This instance keeps the workshop rota."}),
    )
    .await;

    // One charter, both halves, and the build's half first — a reader meeting
    // the narrowing before the rule it narrows has to hold it in the air.
    let whole = s.call("start_here", json!({"bot": "assistant"})).await;
    whole.says("THEIR WORD IS GROUND TRUTH");
    whole.says("keeps the workshop rota");

    // ── THE ROUND TRIP, and it is refused ───────────────────────────────────
    //
    // Exactly what a session does next: take what it just read, add a line,
    // send it back.
    let read_back = serde_json::from_str::<serde_json::Value>(whole.raw())
        .expect("the boot answer is json")["identity"]["charter"]
        .as_str()
        .expect("the identity answers with a charter")
        .to_string();
    let refused = s
        .refused(
            "set_charter",
            json!({
                "bot": "assistant",
                "prose": format!("{read_back}\n\nAnd it waters the fern."),
            }),
        )
        .await;
    // **Read as a caller would.** The way forward has to be in the words, for
    // somebody who does not know a core exists — so this pins the instruction
    // rather than an error type.
    refused.says("only the part you are adding");

    // ── …and the same intent, sent as the addition, lands ───────────────────
    //
    // The positive the refusal rests on. Without it the story passes on a build
    // that refuses every charter there is.
    s.call(
        "set_charter",
        json!({"bot": "assistant", "prose": "This instance keeps the workshop rota, and waters the fern."}),
    )
    .await;
    let after = s.call("start_here", json!({"bot": "assistant"})).await;
    after.says("waters the fern");
    after.says("THEIR WORD IS GROUND TRUTH");

    story.finish().await;
}

#[tokio::test]
async fn a_bot_the_build_supplies_nothing_for_answers_with_its_own_words_alone() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    // A bot the operator stood up. Nothing is supplied for it, so what it
    // answers with is what somebody wrote — and the round trip is not a trap
    // there, because there is no other half to send back.
    s.call(
        "set_charter",
        json!({"bot": "gamma", "prose": "Gamma reviews the week and files a note."}),
    )
    .await;

    let theirs = s
        .call("recall", json!({"subject": "bot:gamma", "charter": true}))
        .await;
    theirs.says("reviews the week");
    assert!(
        !theirs.raw().contains("THEIR WORD IS GROUND TRUTH"),
        "nothing is supplied for a bot the operator stood up: {}",
        theirs.raw(),
    );

    story.finish().await;
}

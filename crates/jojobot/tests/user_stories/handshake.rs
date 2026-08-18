//! "My client is a year old. Why can't it see any of my tools?"
//!
//! **The first thing that happens between a client and jojobot is that the two
//! of them agree what they are speaking**, and it is the one exchange no other
//! story reaches: every story here starts life already connected. That makes
//! the handshake the most caller-visible thing in the product with the least
//! standing behind it — and a client that cannot open a session is the worst
//! failure there is, because nothing after it happens at all.
//!
//! **The failure this rules out is silent agreement.** Agreeing to a revision
//! and then answering it with a payload that revision does not allow leaves a
//! client that connects, asks for the tools, throws the answer away for being
//! malformed, and shows the operator an assistant with no jojobot in it. There
//! is nothing to read and nothing to act on.
//!
//! So the refusal has to arrive at the door, and it has to carry the way
//! forward: **the revisions jojobot does serve, by name.** This story is that
//! round trip, ending where an operator cares about it ending — a session that
//! opens and does the work.

use serde_json::json;

use super::dsl::Story;
use crate::support::{event_stream_error, event_stream_result, open_with};

/// A revision this server does not serve. It is real rather than invented: it
/// makes fields required on a tool list that jojobot does not send, so agreeing
/// to it would mean promising a payload jojobot cannot write.
const TOO_NEW: &str = "2026-07-28";

#[tokio::test]
async fn a_client_speaking_a_revision_jojobot_cannot_serve_is_told_what_it_can() {
    let story = Story::begin("bot:otto").await;
    let http = reqwest::Client::new();
    let door = story.door();

    // ── the old client knocks ───────────────────────────────────────────────
    let refused = event_stream_error(&open_with(&http, &door, TOO_NEW).await.1);
    let served: Vec<String> = refused["data"]["supported"]
        .as_array()
        .expect("the refusal names what jojobot does speak")
        .iter()
        .map(|v| v.as_str().expect("a revision is a string").to_string())
        .collect();

    // **The way forward, and it is the whole reason this is a refusal rather
    // than a silence** (rule 68). A client told only "no" has nowhere to go;
    // this one is handed the list it can open on.
    assert!(
        !served.is_empty(),
        "a refusal with no revisions named leaves the client nowhere to go: {refused}"
    );
    // …and it must not offer back the one it just turned down, which is the
    // way this refusal would be worse than useless.
    assert!(
        !served.iter().any(|revision| revision.as_str() >= TOO_NEW),
        "a revision jojobot cannot serve was offered back: {served:?}"
    );

    // ── the client opens again on what it was offered ───────────────────────
    let newest = served.iter().max().expect("at least one revision").clone();
    let (session, opened) = open_with(&http, &door, &newest).await;
    assert_eq!(
        event_stream_result(&opened)["protocolVersion"]
            .as_str()
            .expect("the handshake says what the two of us will speak"),
        newest,
        "the second handshake must agree to what the refusal offered"
    );

    // ── and the work behind the door actually happens ───────────────────────
    //
    // **This is where the story stops being about a protocol.** A handshake
    // that agrees and then serves nothing is the same outage wearing a
    // different error, so the recovered connection is asked for the one thing
    // every session starts with, over the revision the two of them settled on.
    let mut booting = http
        .post(&door)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", newest.clone())
        .body(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "start_here", "arguments": { "bot": "otto", "brief": true } },
            })
            .to_string(),
        );
    if let Some(session) = &session {
        booting = booting.header("mcp-session-id", session.clone());
    }
    let booted = booting.send().await.expect("the boot reaches jojobot");
    assert!(
        booted.status().is_success(),
        "a session must open over the revision jojobot itself offered: {}",
        booted.status()
    );
    let text = event_stream_result(&booted.text().await.expect("a body"))["content"][0]["text"]
        .as_str()
        .expect("the boot answers with a body")
        .to_string();
    let door_said: serde_json::Value = serde_json::from_str(&text).expect("the boot answers json");
    assert_eq!(
        door_said["identity"]["bot"]["id"], "bot:otto",
        "the client that could not connect is now booted as itself: {door_said}"
    );
    assert!(
        door_said["session"]["sid"].is_string(),
        "…and it holds the handle every later call rides on: {door_said}"
    );
}

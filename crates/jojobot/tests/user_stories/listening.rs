//! "Will my client be told when jojobot's tools change?"
//!
//! A client on the newest revision asks to hear about changes by opening a
//! listen, one request that the server holds open. jojobot says at the
//! handshake that its tool list can change, and the one category it serves a
//! listen for is that tool list. The acknowledgement names that category, the
//! listen is held open, and the server goes on answering while it is.

use serde_json::{Value, json};

use super::dsl::Story;
use crate::support::{event_stream_result, open_with, request_meta};

/// The revision that defines `subscriptions/listen`.
const NEWEST: &str = "2026-07-28";

/// The next JSON-RPC message on an open event stream, or `None` when it ends.
async fn next_message(stream: &mut reqwest::Response, pending: &mut String) -> Option<Value> {
    loop {
        while let Some(end) = pending.find("\n\n") {
            let event: String = pending.drain(..end + 2).collect();
            if let Some(message) = event
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .find_map(|json| serde_json::from_str::<Value>(json).ok())
            {
                return Some(message);
            }
        }
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(5), stream.chunk())
            .await
            .unwrap_or_else(|_| {
                panic!("the stream neither carried nor ended within five seconds: {pending:?}")
            })
            .ok()??;
        pending.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
    }
}

#[tokio::test]
async fn a_client_that_listens_is_told_it_will_hear_about_the_tool_list() {
    let story = Story::begin("bot:otto").await;
    let door = story.door();
    let http = reqwest::Client::new();

    // ── the server says its tool list can change ────────────────────────────
    let (_session, opened) = open_with(&http, &door, NEWEST).await;
    let handshake = event_stream_result(&opened);
    assert_eq!(
        handshake["capabilities"]["tools"]["listChanged"], true,
        "the handshake says the tool list can move: {handshake}"
    );

    // ── the client asks to hear about the tool list ─────────────────────────
    let mut stream = http
        .post(&door)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", NEWEST)
        .header("mcp-method", "subscriptions/listen")
        .body(
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "subscriptions/listen",
                "params": {
                    "notifications": {"toolsListChanged": true},
                    "_meta": request_meta(NEWEST),
                },
            })
            .to_string(),
        )
        .send()
        .await
        .expect("the listen reaches jojobot");
    assert!(
        stream.status().is_success(),
        "a listen on the newest revision is not refused: {}",
        stream.status()
    );
    let mut pending = String::new();
    let first = next_message(&mut stream, &mut pending)
        .await
        .expect("the listen is answered with an acknowledgement");
    assert_eq!(
        first["method"], "notifications/subscriptions/acknowledged",
        "{first}"
    );
    assert_eq!(
        first["params"]["notifications"]["toolsListChanged"], true,
        "the tool list is what the client will hear about: {first}"
    );

    // ── it is held open, and the server still does its work ─────────────────
    let held = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        next_message(&mut stream, &mut pending),
    )
    .await;
    assert!(held.is_err(), "a listen is held open: {held:?}");
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    assert_eq!(
        s.recall("person:milhouse").await.json()["objects"][0]["id"],
        "person:milhouse",
        "the server answers while a listen is open"
    );
    drop(stream);
    story.finish().await;
}

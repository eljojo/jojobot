//! **This file is a contract, not a story.** It pins what the story harness does
//! when a story ends. Nothing here is a use case in a person's words: a story
//! has no call that ends it, so these cases hold that a story which simply goes
//! out of scope stops its server, and that ending a story never hides a failure.

use std::time::Duration;

use super::dsl::Story;

/// Whether a connection to the server's address is accepted.
async fn answers(door: &str) -> bool {
    let address = door
        .trim_start_matches("http://")
        .trim_end_matches("/mcp")
        .to_string();
    tokio::net::TcpStream::connect(address).await.is_ok()
}

/// **A story that goes out of scope stops its server.** No call ends a story:
/// the server it started is cancelled when the story is dropped, so a case never
/// leaves one listening for the rest of the run.
#[tokio::test]
async fn a_story_that_goes_out_of_scope_stops_its_server() {
    let story = Story::begin("bot:alpha").await;
    let door = story.door();
    assert!(
        answers(&door).await,
        "the server answers while the story lives"
    );
    drop(story);
    let mut stopped = false;
    for _ in 0..100 {
        if !answers(&door).await {
            stopped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        stopped,
        "the server still answers after its story was dropped"
    );
}

/// **A story that fails still fails.** The drop that ends a story runs while the
/// failure unwinds, and it must neither swallow the panic nor turn it into a pass.
#[tokio::test]
#[should_panic(expected = "the failure inside the story")]
async fn a_story_that_panics_inside_still_fails() {
    let _story = Story::begin("bot:alpha").await;
    panic!("the failure inside the story");
}

//! **A recorded stream, replayed through a real room.**
//!
//! The free cases in `replay` drive the reader, the classifier and the mapper
//! with a script for a door. These send a stream down the path the replay
//! binary takes: a room on the shipped server, the calls through the served
//! surface, the ids a run mints mapped into the calls that follow.
//!
//! The stream here is made by calling one room and writing down what it said, in
//! the shape the CLI keeps. A second room, empty and of its own, then gets the
//! same calls. Nothing is committed from a real run.

use jojobot_exercise::replay::{self, Class};
use jojobot_exercise::room::{Room, server_binary};
use serde_json::{Value, json};

/// **A small sitting, called on a room and written down as the CLI keeps it.**
/// The sid the boot hands back goes into the calls after it, and the address the
/// capture hands back goes into the last one.
async fn record(surface: &jojobot_exercise::surface::Surface) -> String {
    let mut lines = vec![json!({"jojobot_sitting": "one"}).to_string()];
    let mut step = 0;
    let mut say = |verb: &str, input: Value, body: &str| {
        step += 1;
        lines.extend(event_pair(&format!("t{step}"), verb, input, body));
    };

    let boot = json!({"bot": "assistant", "brief": true});
    let body = surface.call("start_here", boot.clone()).await;
    say("start_here", boot, &body);
    let sid = answer_of(&body)["session"]["sid"]
        .as_str()
        .expect("a sid")
        .to_string();

    let add = json!({
        "sid": sid, "kind": "thing", "handle": "handcart",
        "name": "The Handcart", "source": "user-named",
    });
    let body = surface.call("add_entity", add.clone()).await;
    say("add_entity", add, &body);

    let capture = json!({
        "sid": sid, "subject": "thing:handcart", "content": "it squeaks",
        "provenance": "testimony",
    });
    let body = surface.call("capture", capture.clone()).await;
    say("capture", capture, &body);
    let address = answer_of(&body)["address"]
        .as_str()
        .expect("the capture handed back an address")
        .to_string();

    let fix = json!({"sid": sid, "address": address, "content": "it squeaks, loudly"});
    let body = surface.call("update_fact", fix.clone()).await;
    say("update_fact", fix, &body);
    lines.join("\n")
}

fn answer_of(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"))
}

fn event_pair(id: &str, verb: &str, input: Value, body: &str) -> [String; 2] {
    [
        json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": id,
             "name": format!("{}{verb}", replay::VERB_PREFIX), "input": input}
        ]}})
        .to_string(),
        json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": id, "content": body}
        ]}})
        .to_string(),
    ]
}

/// **Calls recorded on one room and sent to another come back in the same
/// classes, with the sid the second room minted in the calls that carry it.**
/// The sid differs between the two rooms, so a replay that sent it as recorded
/// would be refused from the second call on.
#[tokio::test]
async fn a_stream_replayed_on_a_second_room_changes_no_class() {
    let binary = server_binary().expect("a built server");
    let (first, surface) = Room::open_with_client(&binary).await.expect("a room");
    let stream = record(&surface).await;
    surface.finish().await;
    drop(first);

    let (second, surface) = Room::open_with_client(&binary)
        .await
        .expect("a second room");
    let sittings = replay::read_stream(&stream);
    let results = replay::replay(&sittings, &surface).await;
    surface.finish().await;
    drop(second);

    let summary = replay::summarise(&results[0]);
    assert_eq!(summary.calls, 4);
    assert_eq!(summary.differing, 0, "{results:?}");
    // The address goes as recorded, which the second room accepts because it
    // numbers its records the same way. The report says so rather than reading
    // as complete.
    assert_eq!(
        replay::gaps(&stream, &sittings, &results),
        vec![replay::Gap::Unmapped(vec![
            "capture.address (1 use)".to_string()
        ])]
    );
    assert!(
        results[0]
            .calls
            .iter()
            .all(|call| call.recorded == Class::Accepted),
        "the recorded calls were all accepted: {results:?}"
    );
}

/// **A recording that says a call was accepted, replayed on a room that refuses
/// it, names that call as the first divergence.** The recorded answer of the
/// second call is edited to claim a class the room will not give.
#[tokio::test]
async fn a_recorded_class_the_room_does_not_give_is_named_as_the_first_divergence() {
    let binary = server_binary().expect("a built server");
    let (first, surface) = Room::open_with_client(&binary).await.expect("a room");
    let stream = record(&surface).await;
    surface.finish().await;
    drop(first);

    // The second call (add_entity) was accepted when recorded; the recording now
    // says the room refused it, and the replay's room accepts it.
    let mut sittings = replay::read_stream(&stream);
    sittings[0].calls[1].answer = Some(r#"{"status":"blocked","fix_by":"change"}"#.to_string());

    let (second, surface) = Room::open_with_client(&binary)
        .await
        .expect("a second room");
    let results = replay::replay(&sittings, &surface).await;
    surface.finish().await;
    drop(second);

    let summary = replay::summarise(&results[0]);
    let first_divergence = summary.first.expect("a divergence");
    assert_eq!(first_divergence.verb, "add_entity");
    assert_eq!(first_divergence.index, 1);
    assert_eq!(first_divergence.recorded, Class::Refused("change".into()));
    assert_eq!(first_divergence.replayed, Class::Accepted);
    assert_eq!(summary.differing, 1);
}

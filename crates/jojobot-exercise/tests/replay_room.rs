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
    assert_eq!(replay::gaps(&stream, &sittings, &results), vec![]);
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

/// A run recorded on a server, and the stream the CLI would have kept of it.
async fn a_run_recorded_on(stating: Option<&str>) -> String {
    let binary = server_binary().expect("a built server");
    let (room, surface) = match stating {
        Some(day) => Room::open_with_client_stating(&binary, day).await,
        None => Room::open_with_client(&binary).await,
    }
    .expect("a room");
    let stream = record(&surface).await;
    surface.finish().await;
    drop(room);
    stream
}

/// **A replay of a run made under a stated day opens its room on that day.**
/// The file's name stamps a different day (1970-01-01), so a replay that read
/// the name before the run's own record would fail here.
#[tokio::test]
async fn a_replay_of_a_run_made_under_a_stated_day_opens_on_that_day() {
    let stream = a_run_recorded_on(Some("2031-05-06")).await;
    let (_room, surface, day) = replay::open_for_run(
        &server_binary().expect("a built server"),
        &stream,
        std::path::Path::new("year-1.jsonl"),
    )
    .await
    .expect("the log records a day");
    assert_eq!(day.day, "2031-05-06");
    assert_eq!(day.source, replay::DaySource::Announced);
    assert_eq!(
        surface.stated_day().await.as_deref(),
        Some("2031-05-06"),
        "the replay's server did not act out the day the run was made on",
    );
}

/// **A run made on the wall clock replays on the day its file name stamps.**
#[tokio::test]
async fn a_replay_of_a_wall_clock_run_opens_on_the_day_of_its_file_name() {
    let stream = a_run_recorded_on(None).await;
    let (_room, surface, day) = replay::open_for_run(
        &server_binary().expect("a built server"),
        &stream,
        std::path::Path::new("year-1893499200.jsonl"),
    )
    .await
    .expect("the file name records a day");
    assert_eq!(day.source, replay::DaySource::FileName);
    assert_eq!(surface.stated_day().await.as_deref(), Some("2030-01-01"));
}

/// **A run whose log records no day is refused with the run named**, paired
/// with the two cases above: the same recorded wall-clock run, given a stamp,
/// opens.
#[tokio::test]
async fn a_replay_of_a_run_whose_log_records_no_day_is_refused_naming_it() {
    let stream = a_run_recorded_on(None).await;
    let refused = replay::open_for_run(
        &server_binary().expect("a built server"),
        &stream,
        std::path::Path::new("year-latest.jsonl"),
    )
    .await
    .err()
    .expect("a run with no day on record is not replayed");
    let said = format!("{refused:#}");
    assert!(
        said.contains("year-latest.jsonl") && said.contains("no day"),
        "the refusal does not name the run and say no day is on record: {said}",
    );
}

/// A directory this case owns alone, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Scratch {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "jojobot-exercise-replay-day-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run the `replay` binary over one stream file kept under `name`, and say
/// whether it succeeded with what it printed.
fn replayed_by_the_binary(stream: &str, name: &str) -> (bool, String) {
    let scratch = Scratch::new();
    let file = scratch.0.join(name);
    std::fs::write(&file, stream).expect("the stream is written");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_replay"))
        .args(["--stream"])
        .arg(&file)
        .args(["--room", "year", "--candidate", "test", "--reports"])
        .arg(scratch.0.join("reports"))
        .env("JOJOBOT_BIN", server_binary().expect("a built server"))
        .output()
        .expect("the replay binary runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr),
    )
}

/// **The replay binary opens on the replayed run's day and says so**, by its
/// report: through the binary a person runs, not through the function it calls.
/// A run that records no day is refused by the same binary, naming the run.
#[tokio::test]
async fn the_replay_binary_acts_out_the_runs_day_and_names_it_or_refuses_the_run() {
    let stated = a_run_recorded_on(Some("2031-05-06")).await;
    let (worked, said) = replayed_by_the_binary(&stated, "year-1.jsonl");
    assert!(worked, "the replay of a stated-day run ran: {said}");
    assert!(
        said.contains("server acted out 2031-05-06") && said.contains("announced"),
        "the report does not name the day and where it came from: {said}",
    );

    let wall = a_run_recorded_on(None).await;
    let (worked, said) = replayed_by_the_binary(&wall, "year-1893499200.jsonl");
    assert!(worked, "the replay of a stamped wall-clock run ran: {said}");
    assert!(
        said.contains("server acted out 2030-01-01") && said.contains("file name"),
        "the report does not name the day and where it came from: {said}",
    );

    let (worked, said) = replayed_by_the_binary(&wall, "year-latest.jsonl");
    assert!(!worked, "a run with no day on record was replayed: {said}");
    assert!(
        said.contains("year-latest.jsonl") && said.contains("no day"),
        "the refusal does not name the run: {said}",
    );
}

//! **The deploy's pre-build, at the binary.** `jojobot snapshot-only` connects to a
//! store that is already serving, writes one snapshot of the log and exits. It
//! exits non-zero when it cannot, so the deploy step that runs it stops before the
//! old process does.

use std::process::{Command, Stdio};

use jojobot_adapters::dolt::{Dolt, migrate};
use jojobot_adapters::teaching_log::codec as teaching;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_adapters::writer::Writer;
use jojobot_adapters::writer_log::WriterLog;
use jojobot_domain::log::{BatchId, LogStore, Position, SealedBatch};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};

/// Run the binary's snapshot-only mode against the store on `port`.
fn pre_build(port: u16) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").expect("a PATH for the binary"),
        )
        .env("JOJOBOT_STORE_PORT", port.to_string())
        .arg("snapshot-only")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the binary runs")
}

/// **A pre-build that cannot reach the store fails loudly.** Nothing answers on the
/// port, so the mode has no log to read: it exits non-zero and says which port.
#[test]
fn a_pre_build_that_cannot_reach_the_store_exits_non_zero() {
    let port = free_port();
    let out = pre_build(port);
    assert!(
        !out.status.success(),
        "a pre-build with no store succeeded: {out:?}"
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains(&port.to_string()), "it named no port: {said}");
}

/// **A boot after a pre-build that failed says it has no snapshot, and counts it.**
/// The pre-build finds no store and exits non-zero, so it wrote nothing; the next
/// boot, over a store that holds no snapshot, replays the log. That boot names the
/// reason on its boot line, reports that it did not start from a snapshot, and
/// serves the count on the scrape, so a deploy that lost its snapshot is seen by
/// whoever reads the log or the numbers and not only by the step that ran the
/// pre-build.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_after_a_failed_pre_build_replays_the_log_and_says_so() {
    let failed = tokio::task::spawn_blocking(|| pre_build(free_port()))
        .await
        .expect("the task ends");
    assert!(
        !failed.status.success(),
        "a pre-build with no store succeeded: {failed:?}"
    );

    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-failed-pre-build");
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let metrics_port = free_port();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", free_port().to_string())
        .env("JOJOBOT_BIND", "127.0.0.1:0")
        .env("JOJOBOT_METRICS_BIND", format!("127.0.0.1:{metrics_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");
    let serving = jojobot_adapters::testing::wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    .unwrap_or_else(|failure| panic!("the boot after a failed pre-build {failure}"));

    // **The boot says so once, as a warning, naming the reason.**
    let said: Vec<_> = serving
        .seen
        .iter()
        .filter(|line| line.contains("SNAPSHOT NOT USED"))
        .collect();
    assert_eq!(
        said.len(),
        1,
        "the boot did not warn exactly once that no snapshot was used:\n{:#?}",
        serving.seen
    );
    assert!(
        said[0].contains("WARN"),
        "it was not a warning: {}",
        said[0]
    );
    assert!(
        said[0].contains("none_held"),
        "it named no reason: {}",
        said[0]
    );
    let booted = serving
        .seen
        .iter()
        .find(|line| line.contains("event log boot"))
        .unwrap_or_else(|| panic!("no boot line:\n{:#?}", serving.seen));
    assert!(
        booted.contains("from_snapshot=false"),
        "the boot says it started from a snapshot: {booted}"
    );

    let scraped = reqwest::get(format!("http://127.0.0.1:{metrics_port}/metrics"))
        .await
        .expect("the metrics listener answers")
        .text()
        .await
        .expect("a body");
    // One boot with no snapshot counts once, and no other reason moved.
    let skips: std::collections::BTreeMap<&str, u64> = scraped
        .lines()
        .filter_map(|line| line.strip_prefix("jojobot_snapshot_skips{reason=\""))
        .filter_map(|rest| {
            let (reason, value) = rest.split_once("\"} ")?;
            Some((reason, value.trim().parse().ok()?))
        })
        .collect();
    assert!(
        skips.get("none_held") == Some(&1),
        "the scrape does not count the skipped snapshot once:\n{scraped}"
    );
    let others: Vec<_> = skips
        .iter()
        .filter(|(reason, count)| **reason != "none_held" && **count != 0)
        .collect();
    assert!(
        others.is_empty(),
        "a reason other than none_held moved: {others:?}"
    );

    let _ = child.start_kill();
    let _ = child.wait().await;
    drop(scratch);
}

/// **A pre-build against a serving store writes the snapshot a boot then loads.**
/// The store is started and filled by the test, the way the old process leaves it;
/// the binary connects to it and writes one snapshot at the head.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_build_writes_a_snapshot_of_the_serving_log() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-snapshot-only");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();
    let mut server = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");
    let writer = Writer::default();
    let head = {
        let log = WriterLog::open(server.pool().clone(), writer.clone())
            .await
            .expect("the log opens");
        let members = (0..4)
            .map(|n| {
                teaching::encode(&TeachingEvent::Recorded {
                    run: RunId(format!("r{n}")),
                    domain: "claims".into(),
                    taught_at: jiff::Timestamp::UNIX_EPOCH,
                })
            })
            .collect();
        let batch = SealedBatch::seal(
            BatchId("alpha".into()),
            Position(0),
            jiff::Timestamp::UNIX_EPOCH,
            1,
            members,
        )
        .expect("a batch");
        log.append(Position(0), &batch).await.expect("it lands");
        assert!(
            log.load_snapshot().await.expect("readable").is_none(),
            "the serving process left no snapshot"
        );
        batch.range().last
    };

    let out = tokio::task::spawn_blocking(move || pre_build(port))
        .await
        .expect("the task ends");
    assert!(out.status.success(), "the pre-build failed: {out:?}");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.contains(&format!("covers position {}", head.0)),
        "it did not say where the snapshot covers: {said}"
    );

    let log = WriterLog::open(server.pool().clone(), writer.clone())
        .await
        .expect("the log opens again");
    let stored = log
        .load_snapshot()
        .await
        .expect("readable")
        .expect("the pre-build wrote a snapshot");
    assert_eq!(stored.covered_position, head);

    drop(log);
    writer.drain().await;
    server.stop().await;
    drop(scratch);
}

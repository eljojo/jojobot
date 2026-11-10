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

/// What the real binary's boot printed up to its serving line, and the count of each
/// reason a boot skipped a snapshot, as the scrape served it.
struct Booted {
    seen: Vec<String>,
    skips: std::collections::BTreeMap<String, u64>,
}

/// **Boot the real binary over the store in `state_dir`**, read what it logs and
/// what its scrape counts, and stop it.
async fn boot_and_scrape(state_dir: &std::path::Path) -> Booted {
    let metrics_port = free_port();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env("STATE_DIRECTORY", state_dir)
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
    .unwrap_or_else(|failure| panic!("the boot {failure}"));
    let scraped = reqwest::get(format!("http://127.0.0.1:{metrics_port}/metrics"))
        .await
        .expect("the metrics listener answers")
        .text()
        .await
        .expect("a body");
    let skips = scraped
        .lines()
        .filter_map(|line| line.strip_prefix("jojobot_snapshot_skips{reason=\""))
        .filter_map(|rest| {
            let (reason, value) = rest.split_once("\"} ")?;
            Some((reason.to_string(), value.trim().parse().ok()?))
        })
        .collect();
    let _ = child.start_kill();
    let _ = child.wait().await;
    Booted {
        seen: serving.seen,
        skips,
    }
}

/// A boot is counted once under `reason` and under no other.
fn counted_once_under(booted: &Booted, reason: &str) {
    assert_eq!(
        booted.skips.get(reason),
        Some(&1),
        "the scrape does not count the skipped snapshot once under {reason}: {:?}",
        booted.skips
    );
    let others: Vec<_> = booted
        .skips
        .iter()
        .filter(|(each, count)| each.as_str() != reason && **count != 0)
        .collect();
    assert!(others.is_empty(), "another reason moved: {others:?}");
}

/// **A boot over an empty history says so at info and counts it once.** A first boot
/// has no snapshot because there is nothing to snapshot, which says nothing about a
/// pre-build, so it does not warn. It is still a boot that used no snapshot, and the
/// scrape counts it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_over_an_empty_history_says_so_at_info_and_counts_it_once() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-empty-history");
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());

    let booted = boot_and_scrape(&state_dir).await;

    let said: Vec<_> = booted
        .seen
        .iter()
        .filter(|line| line.contains("no snapshot held and no history"))
        .collect();
    assert_eq!(
        said.len(),
        1,
        "the boot did not say once that it held no snapshot and no history:\n{:#?}",
        booted.seen
    );
    assert!(said[0].contains("INFO"), "it was not at info: {}", said[0]);
    assert!(said[0].contains("none_held"), "no reason: {}", said[0]);
    assert!(
        !booted
            .seen
            .iter()
            .any(|line| line.contains("SNAPSHOT NOT USED")),
        "a boot over an empty history warned:\n{:#?}",
        booted.seen
    );
    counted_once_under(&booted, "none_held");
    drop(scratch);
}

/// **A boot after a pre-build that failed, over a history, warns and counts it once.**
/// The pre-build finds no store and exits non-zero, so it wrote nothing; the store
/// the old process left holds history and no snapshot, so the next boot replays the
/// whole log. That is the case the warning is for: it names the reason on its boot
/// line, reports that it did not start from a snapshot and had a tail to replay, and
/// serves the count on the scrape, so a deploy that lost its snapshot is seen by
/// whoever reads the log or the numbers and not only by the step that ran the
/// pre-build.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_over_a_history_after_a_failed_pre_build_warns_and_counts_it_once() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-failed-pre-build");
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());

    // The store as the old process left it: history, and no snapshot.
    let writer = Writer::default();
    let mut server = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the old process's store comes up");
    migrate::run(server.pool()).await.expect("the schema");
    {
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
            "the old process left a snapshot"
        );
    }
    writer.drain().await;
    server.stop().await;

    // The pre-build finds no store and writes nothing.
    let failed = tokio::task::spawn_blocking(|| pre_build(free_port()))
        .await
        .expect("the task ends");
    assert!(
        !failed.status.success(),
        "a pre-build with no store succeeded: {failed:?}"
    );

    let booted = boot_and_scrape(&state_dir).await;

    // **The boot says so once, as a warning, naming the reason.**
    let said: Vec<_> = booted
        .seen
        .iter()
        .filter(|line| line.contains("SNAPSHOT NOT USED"))
        .collect();
    assert_eq!(
        said.len(),
        1,
        "the boot did not warn exactly once that no snapshot was used:\n{:#?}",
        booted.seen
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
    let boot_line = booted
        .seen
        .iter()
        .find(|line| line.contains("event log boot"))
        .unwrap_or_else(|| panic!("no boot line:\n{:#?}", booted.seen));
    assert!(
        boot_line.contains("from_snapshot=false") && !boot_line.contains("tail_batches=0"),
        "the boot did not replay a history without a snapshot: {boot_line}"
    );
    counted_once_under(&booted, "none_held");
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

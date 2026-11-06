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

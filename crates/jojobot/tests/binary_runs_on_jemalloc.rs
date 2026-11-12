//! **The server binary allocates through jemalloc.** The memory it holds after a
//! day of writes is the number rule 800 bounds, and the system allocator kept more
//! than half of it as freed pages (the memory job measured 2119.9 MiB after the
//! day under glibc and 741.9 MiB under jemalloc). jemalloc reads its own
//! configuration variable and prints its statistics when the process ends, and the
//! system allocator does neither, so the printed statistics say which allocator ran.

use std::process::Stdio;

use jojobot_adapters::testing::{Scratch, free_port};
use tokio::io::{AsyncBufReadExt, AsyncReadExt};

/// How long the binary has to stop after the signal, and its stderr to end.
const STOP_PATIENCE: std::time::Duration = std::time::Duration::from_secs(60);

#[tokio::test]
async fn the_server_binary_ends_by_printing_jemallocs_statistics() {
    let scratch = Scratch::new("runs-on-jemalloc");
    let mut child = tokio::process::Command::new(
        jojobot_exercise::room::server_binary().expect("a jojobot binary to run"),
    )
    .env("STATE_DIRECTORY", &scratch.0)
    .env("JOJOBOT_STORE_PORT", free_port().to_string())
    .env("JOJOBOT_BIND", "127.0.0.1:0")
    .env("JOJOBOT_ALLOW_NO_AUTH", "1")
    .env_remove("JOJOBOT_ISSUER")
    .env("RUST_LOG", "info")
    // The crate prefixes jemalloc's symbols and its variable, so a jemalloc some
    // other library holds does not read this one.
    .env("_RJEM_MALLOC_CONF", "stats_print:true")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .spawn()
    .expect("the binary runs");
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let kept = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text).await;
        text
    });
    let mut lines =
        tokio::io::BufReader::new(child.stdout.take().expect("stdout was piped")).lines();
    let reached = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            if line.contains("serving http://") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(reached, "the binary never reached its serving line");
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    let pid = child.id().expect("a running process has an id");
    let stopped = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(stopped.success(), "the stop signal was not delivered");
    // A server that does not stop ends the case instead of hanging the suite.
    let ended = tokio::time::timeout(STOP_PATIENCE, child.wait()).await;
    if ended.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
        panic!("the binary did not stop within {STOP_PATIENCE:?} of the stop signal");
    }
    let text = tokio::time::timeout(STOP_PATIENCE, kept)
        .await
        .expect("stderr reached its end")
        .expect("stderr was read");
    assert!(
        text.contains("jemalloc statistics"),
        "the binary ended without jemalloc's statistics, so it ran on another allocator:\n{text}"
    );
}

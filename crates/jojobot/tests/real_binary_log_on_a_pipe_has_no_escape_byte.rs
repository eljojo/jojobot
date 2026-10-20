//! **The log of the real binary, with its stdout on a pipe, holds no escape byte.**
//!
//! `colour_for` decides whether the log is coloured, and a unit case proves the
//! rule on a stream that is not a terminal. That does not prove `main` asks it
//! about the stream the subscriber writes to. This case starts the binary with
//! stdout on a pipe, sends one request through the HTTP transport and reads what
//! the process logged. Colour puts escape codes inside a field's text, so
//! `tool="recall"` would not be found in a log that carried it.

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jojobot_adapters::testing::free_port;
use tokio::io::AsyncReadExt;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn the_log_of_the_real_binary_on_a_pipe_names_the_tool_and_holds_no_escape_byte() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-pipe-log");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let http_port = free_port();
    let store_port = free_port();

    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        // An inherited filter can hide the request line, and the package build
        // exports an empty one.
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");

    // Raw bytes, not lines: the byte under test must not be able to hide in a
    // line break.
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let log = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader = {
        let log = Arc::clone(&log);
        tokio::spawn(async move {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stdout.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                log.lock().unwrap().extend_from_slice(&chunk[..n]);
            }
        })
    };
    let said = |needle: &str| String::from_utf8_lossy(&log.lock().unwrap()).contains(needle);

    let boot_began = Instant::now();
    let deadline = boot_began + jojobot_exercise::room::READY_TIMEOUT;
    while !said("serving http://") && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        said("serving http://"),
        "the binary never reached its serving line after {:?} (a boot is given {:?}), so there is \
         no request to log",
        boot_began.elapsed(),
        jojobot_exercise::room::READY_TIMEOUT
    );

    let surface =
        jojobot_exercise::surface::Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
            .await
            .expect("connecting to the real binary");
    let answer = surface
        .call("recall", serde_json::json!({"subject": "topic:instance"}))
        .await;
    surface.finish().await;
    assert!(
        answer.contains("topic:instance"),
        "the request was not answered: {answer}"
    );

    // The request line is written when the request ends, which can be after the
    // client has its answer.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !said("tool=\"recall\"") && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
    let _ = reader.await;

    let bytes = log.lock().unwrap().clone();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        !bytes.contains(&0x1b),
        "the log of a process writing to a pipe carries an escape byte:\n{text:?}"
    );
    assert!(
        text.contains("tool=\"recall\""),
        "the request line with the tool's name is missing from the log:\n{text}"
    );
}

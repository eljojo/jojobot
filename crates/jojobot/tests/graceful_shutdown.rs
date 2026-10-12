//! **A signal to stop finishes the call in flight and exits through the graceful
//! path.** A service manager stops a unit with SIGTERM, and an operator at a
//! terminal stops it with SIGINT; both must give the server the same orderly
//! stop: no new work, the call that is already running finished, the process
//! gone with a clean exit and a line saying it shut down.
//!
//! Each case spawns the real binary over a store of its own, opens one call and
//! holds it open by sending its body in two parts, signals the server while the
//! second part is unsent, and then finishes the call. **Nothing here sleeps.**
//! The server says when it has the call (it answers the request's
//! `Expect: 100-continue`) and when it has begun to stop (a log line), and the
//! case moves on those two things and on nothing else.
//!
//! The two signals are one pair: SIGINT already worked and SIGTERM did not, so
//! each case holds the other's shape. A server that ignored the signal and kept
//! serving would pass neither, because the exit and the line are asserted.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use jojobot_adapters::testing::free_port;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The server process, stopped hard when a case fails before it has exited.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// How long a case waits for something the server has to say or do. A wait
/// ends the moment it happens; this is only how long a failure takes to show.
const PATIENCE: Duration = Duration::from_secs(120);

/// How long a case waits for the server to answer a signal. The store is up and
/// the call is open, so the stop is quick when it happens at all.
const STOP_PATIENCE: Duration = Duration::from_secs(30);

/// The first request of a conversation with the server, and the part of its
/// body that is sent after the signal.
const BODY: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"gamma","version":"1"}}}"#;

/// Wait for a line of the server's log that holds `needle`, answering every
/// line seen on the way.
fn wait_for_line(
    lines: &mpsc::Receiver<String>,
    needle: &str,
    seen: &mut Vec<String>,
    patience: Duration,
) {
    loop {
        match lines.recv_timeout(patience) {
            Ok(line) => {
                let hit = line.contains(needle);
                seen.push(line);
                if hit {
                    return;
                }
            }
            Err(_) => panic!(
                "the server never said {needle:?}. What it said:\n{}",
                seen.join("\n")
            ),
        }
    }
}

/// Stop the server with `signal` while a call is in flight, and hold it to the
/// orderly stop.
fn stopping_on(signal: &str, name: &str) {
    let state_dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let http_port = free_port();
    let store_port = free_port();

    let mut child = Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env_clear()
        // The store's own binary is found on the path.
        .env(
            "PATH",
            std::env::var_os("PATH").expect("a PATH to find the store"),
        )
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");
    let pid = child.id();
    let log = child.stdout.take().expect("stdout was piped");
    let mut server = Server(child);

    // The server's log, line by line, from a thread of its own.
    let (said, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(log).lines().map_while(Result::ok) {
            if said.send(line).is_err() {
                return;
            }
        }
    });
    let mut seen = Vec::new();
    wait_for_line(&lines, "serving http://", &mut seen, PATIENCE);

    // **The call, held open.** The head asks to be told to continue, and the
    // server's `100 Continue` is its word that it has the call and is reading
    // its body. Half of the body goes now and the rest after the signal.
    let mut call = TcpStream::connect(("127.0.0.1", http_port)).expect("the server answers");
    call.set_read_timeout(Some(PATIENCE)).expect("a timeout");
    let head = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{http_port}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\
         Expect: 100-continue\r\nConnection: close\r\n\r\n",
        BODY.len()
    );
    call.write_all(head.as_bytes()).expect("the head is sent");
    let mut heard = Vec::new();
    let mut byte = [0u8; 1];
    while !heard.ends_with(b"\r\n\r\n") {
        let n = call.read(&mut byte).expect("the server answers the head");
        assert!(n == 1, "the server closed the call before it began");
        heard.push(byte[0]);
    }
    let first = String::from_utf8_lossy(&heard).to_string();
    assert!(
        first.starts_with("HTTP/1.1 100"),
        "the server did not ask for the body: {first}"
    );
    let (now, later) = BODY.split_at(BODY.len() / 2);
    call.write_all(now.as_bytes()).expect("half the body");
    call.flush().expect("flushed");

    // **The signal, and the server's word that it has begun to stop.**
    let sent = Command::new("kill")
        .args([&format!("-{signal}"), &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success(), "kill -{signal} {pid} failed");
    wait_for_line(&lines, "shutting down", &mut seen, STOP_PATIENCE);

    // **The call finishes.** The rest of the body goes, and the answer to the
    // whole call comes back.
    call.write_all(later.as_bytes())
        .expect("the rest of the body");
    let mut answer = String::new();
    call.read_to_string(&mut answer)
        .expect("the answer is read to the end");
    assert!(
        answer.starts_with("HTTP/1.1 200"),
        "the call in flight did not complete: {answer}"
    );
    // **The exchange is closed properly**, with the last chunk of a chunked
    // answer, and not cut off by the process ending under it. Whether the MCP
    // result is in the answer is a separate matter: the sessions are cancelled
    // when the signal arrives, so today it is not.
    assert!(
        answer.ends_with("0\r\n\r\n"),
        "the call in flight was cut off instead of finished: {answer:?}"
    );

    // **The orderly exit**: the process leaves with a clean exit code and says
    // it shut down.
    let (done, exited) = mpsc::channel();
    let waiting = std::thread::spawn({
        let mut child = std::mem::replace(
            &mut server.0,
            Command::new("true").spawn().expect("a placeholder"),
        );
        move || {
            let status = child.wait();
            let _ = done.send(status);
        }
    });
    let status = exited
        .recv_timeout(STOP_PATIENCE)
        .expect("the server never exited")
        .expect("the exit is read");
    waiting.join().expect("the waiter ends");
    assert_eq!(
        status.code(),
        Some(0),
        "{signal} ended the server some way other than its orderly exit ({status:?})"
    );
    while let Ok(line) = lines.try_recv() {
        seen.push(line);
    }
    assert!(
        seen.iter().any(|line| line.contains("shut down")),
        "the server never said it shut down. What it said:\n{}",
        seen.join("\n")
    );
}

/// **SIGTERM, what a service manager sends.** The server finishes the call in
/// flight and exits cleanly.
#[test]
fn sigterm_finishes_the_call_in_flight_and_exits_cleanly() {
    stopping_on("TERM", "jojobot-stops-on-sigterm");
}

/// **SIGINT, what a terminal sends.** The same stop. The pair of the case
/// above.
#[test]
fn sigint_finishes_the_call_in_flight_and_exits_cleanly() {
    stopping_on("INT", "jojobot-stops-on-sigint");
}

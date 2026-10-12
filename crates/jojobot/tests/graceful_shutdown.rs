//! **A signal to stop lets the calls in flight finish, cuts what is still open
//! when the drain window ends, and exits cleanly either way.** A service manager
//! stops a unit with SIGTERM and an operator at a terminal stops it with SIGINT;
//! both must give the same orderly stop.
//!
//! Each case spawns the real binary over a store of its own. **Nothing here
//! sleeps.** The server says when it has a call (it answers the request's
//! `Expect: 100-continue`) and when it has begun to stop (a log line), and a case
//! moves on those two things and on the server's exit, each waited for with a
//! limit that only a failure reaches.
//!
//! * A call held open by sending its body in two parts, with the signal sent
//!   between them, is answered in full (the server's own result is in the answer)
//!   and the process exits with code 0 well before the window ends. One case per
//!   signal, because SIGINT already worked and SIGTERM did not.
//! * A stream the client holds open is cut when the window ends, not before and
//!   not after, and the process still exits with code 0.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jojobot::SHUTDOWN_DRAIN;
use jojobot_adapters::testing::free_port;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The server process, stopped hard when a case fails before it has exited.
struct Server(Option<Child>);

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// How long a case waits for the server to start. The store is built from
/// nothing, so this is long; a wait ends the moment the line comes.
const PATIENCE: Duration = Duration::from_secs(120);

/// How long a case waits for the server to answer a signal, beyond the drain
/// window. Only a failure reaches it.
const STOP_PATIENCE: Duration = Duration::from_secs(30);

/// The first request of a conversation with the server.
const BODY: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"gamma","version":"1"}}}"#;

/// A running server, and what a case needs to talk to it and watch it.
struct Running {
    server: Server,
    pid: u32,
    http_port: u16,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    _scratch: Scratch,
}

/// Start the real binary and wait for its serving line.
fn start(name: &str) -> Running {
    let state_dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
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
    let server = Server(Some(child));
    // The server's log, line by line, from a thread of its own.
    let (said, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(log).lines().map_while(Result::ok) {
            if said.send(line).is_err() {
                return;
            }
        }
    });
    let mut running = Running {
        server,
        pid,
        http_port,
        lines,
        seen: Vec::new(),
        _scratch: scratch,
    };
    running.wait_for_line("serving http://", PATIENCE);
    running
}

impl Running {
    /// Wait for a line of the server's log that holds `needle`.
    fn wait_for_line(&mut self, needle: &str, patience: Duration) {
        loop {
            match self.lines.recv_timeout(patience) {
                Ok(line) => {
                    let hit = line.contains(needle);
                    self.seen.push(line);
                    if hit {
                        return;
                    }
                }
                Err(_) => panic!(
                    "the server never said {needle:?}. What it said:\n{}",
                    self.seen.join("\n")
                ),
            }
        }
    }

    /// Send `signal` to the server.
    fn signal(&self, signal: &str) {
        let sent = Command::new("kill")
            .args([&format!("-{signal}"), &self.pid.to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success(), "kill -{signal} {} failed", self.pid);
    }

    /// Wait for the process to exit and say how, within `patience`.
    fn exit_within(&mut self, patience: Duration) -> std::process::ExitStatus {
        let mut child = self.server.0.take().expect("the server is running");
        let (done, exited) = mpsc::channel();
        let waiting = std::thread::spawn(move || {
            let status = child.wait();
            let _ = done.send(status);
        });
        let status = exited
            .recv_timeout(patience)
            .unwrap_or_else(|_| {
                panic!(
                    "the server never exited. What it said:\n{}",
                    self.seen.join("\n")
                )
            })
            .expect("the exit is read");
        waiting.join().expect("the waiter ends");
        // Whatever it said last.
        while let Ok(line) = self.lines.try_recv() {
            self.seen.push(line);
        }
        status
    }

    /// Open a connection and read the head of the answer to a request that asked
    /// to be told to continue.
    fn open_a_call(&self, path: &str, method: &str, extra: &str, length: usize) -> TcpStream {
        let mut call =
            TcpStream::connect(("127.0.0.1", self.http_port)).expect("the server answers");
        call.set_read_timeout(Some(PATIENCE)).expect("a timeout");
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{extra}Content-Length: {length}\r\n\
             Expect: 100-continue\r\nConnection: close\r\n\r\n",
            self.http_port
        );
        call.write_all(head.as_bytes()).expect("the head is sent");
        let first = read_head(&mut call);
        assert!(
            first.starts_with("HTTP/1.1 100"),
            "the server did not ask for the body: {first}"
        );
        call
    }
}

/// Read up to the end of a response head, a byte at a time so nothing past it
/// is taken.
fn read_head(stream: &mut TcpStream) -> String {
    let mut heard = Vec::new();
    let mut byte = [0u8; 1];
    while !heard.ends_with(b"\r\n\r\n") {
        let n = stream.read(&mut byte).expect("the server answers the head");
        assert!(n == 1, "the server closed the call before it answered");
        heard.push(byte[0]);
    }
    String::from_utf8_lossy(&heard).to_string()
}

const JSON_HEADERS: &str =
    "Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n";

/// Stop the server with `signal` while a call is in flight, and hold it to the
/// orderly stop.
fn a_call_in_flight_finishes(signal: &str, name: &str) {
    let mut running = start(name);
    // **The call, held open.** The server's `100 Continue` is its word that it
    // has the call and is reading its body. Half of the body goes now.
    let mut call = running.open_a_call("/mcp", "POST", JSON_HEADERS, BODY.len());
    let (now, later) = BODY.split_at(BODY.len() / 2);
    call.write_all(now.as_bytes()).expect("half the body");
    call.flush().expect("flushed");

    // **The signal, and the server's word that it has begun to stop.**
    let signalled = Instant::now();
    running.signal(signal);
    running.wait_for_line("shutting down", STOP_PATIENCE);

    // **The call finishes**, with the server's own result.
    call.write_all(later.as_bytes())
        .expect("the rest of the body");
    let mut answer = String::new();
    call.read_to_string(&mut answer)
        .expect("the answer is read to the end");
    assert!(
        answer.starts_with("HTTP/1.1 200"),
        "the call in flight did not complete: {answer}"
    );
    assert!(
        answer.contains("protocolVersion"),
        "the call in flight was answered without its result: {answer:?}"
    );

    // **The orderly exit**, before the drain window would have ended anything.
    let status = running.exit_within(STOP_PATIENCE);
    assert_eq!(
        status.code(),
        Some(0),
        "{signal} ended the server some way other than its orderly exit ({status:?})"
    );
    assert!(
        signalled.elapsed() < SHUTDOWN_DRAIN,
        "the stop waited out the drain window though nothing was left open"
    );
    assert!(
        running.seen.iter().any(|line| line.contains("shut down")),
        "the server never said it shut down. What it said:\n{}",
        running.seen.join("\n")
    );
}

/// **SIGTERM, what a service manager sends.** The call in flight is answered in
/// full and the server exits cleanly.
#[test]
fn sigterm_lets_the_call_in_flight_finish_and_exits_cleanly() {
    a_call_in_flight_finishes("TERM", "jojobot-stops-on-sigterm");
}

/// **SIGINT, what a terminal sends.** The same stop. The pair of the case above.
#[test]
fn sigint_lets_the_call_in_flight_finish_and_exits_cleanly() {
    a_call_in_flight_finishes("INT", "jojobot-stops-on-sigint");
}

/// **A stream still open when the drain window ends is cut at the window, and
/// the process exits cleanly.** The client opens a session and holds its
/// notification stream, which nothing ends by itself. The signal comes, the
/// server waits the window out, cuts the stream and exits 0 within the limit.
/// The pair of the cases above: a server that cut at once would have ended the
/// call in flight there, and one that waited without end would never exit here.
#[test]
fn a_stream_held_open_is_cut_when_the_drain_window_ends() {
    let mut running = start("jojobot-cuts-a-held-stream");
    // The session the stream belongs to, opened by a call that finishes.
    let mut hello = running.open_a_call("/mcp", "POST", JSON_HEADERS, BODY.len());
    hello.write_all(BODY.as_bytes()).expect("the body");
    let mut answer = String::new();
    hello
        .read_to_string(&mut answer)
        .expect("the answer is read to the end");
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let session = answer
        .lines()
        .find_map(|line| line.strip_prefix("mcp-session-id: "))
        .unwrap_or_else(|| panic!("the server named no session: {answer}"))
        .trim()
        .to_string();

    // **The stream, held open.** Its head is the server's word that the stream
    // is open.
    let mut stream =
        TcpStream::connect(("127.0.0.1", running.http_port)).expect("the server answers");
    stream.set_read_timeout(Some(PATIENCE)).expect("a timeout");
    let get = format!(
        "GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAccept: text/event-stream\r\n\
         mcp-session-id: {session}\r\nConnection: close\r\n\r\n",
        running.http_port
    );
    stream.write_all(get.as_bytes()).expect("the request");
    let head = read_head(&mut stream);
    assert!(
        head.starts_with("HTTP/1.1 200"),
        "the stream did not open: {head}"
    );

    let signalled = Instant::now();
    running.signal("TERM");
    running.wait_for_line("shutting down", STOP_PATIENCE);

    // **The server ends the stream, and not before the window is out.**
    let mut rest = Vec::new();
    let _ = stream.read_to_end(&mut rest);
    let cut_after = signalled.elapsed();
    assert!(
        cut_after >= SHUTDOWN_DRAIN,
        "the stream was cut after {cut_after:?}, before the {SHUTDOWN_DRAIN:?} window ended"
    );
    let status = running.exit_within(STOP_PATIENCE);
    assert_eq!(
        status.code(),
        Some(0),
        "a stream held open turned the stop into something other than a clean exit ({status:?})"
    );
    assert!(
        signalled.elapsed() < SHUTDOWN_DRAIN + STOP_PATIENCE,
        "the stop took longer than the window and its grace"
    );
    assert!(
        running.seen.iter().any(|line| line.contains("shut down")),
        "the server never said it shut down. What it said:\n{}",
        running.seen.join("\n")
    );
}

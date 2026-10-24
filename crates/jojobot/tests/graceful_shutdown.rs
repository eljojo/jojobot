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
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jojobot::SHUTDOWN_DRAIN;
use jojobot_adapters::testing::free_port;

use jojobot_adapters::testing::Scratch;

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

/// How long a case waits for the server to answer a request it has sent, after
/// the server is up. This is no boot: a boot is given
/// [`jojobot_exercise::room::READY_TIMEOUT`], the value every test that boots the
/// real binary uses.
const READ_PATIENCE: Duration = Duration::from_secs(120);

/// How long a closed log waits for the process to be gone before the failure
/// calls it running. A process that closed its log is on its way out; a server
/// under load can take seconds to finish. Only a failure reaches it.
const EXIT_GRACE: Duration = Duration::from_secs(10);

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
    /// Where the server's standard error went. A server that exits while it
    /// starts says why there and not in its log.
    stderr_path: std::path::PathBuf,
    _scratch: Scratch,
}

/// Start the real binary and wait for its serving line.
fn start(name: &str) -> Running {
    let state_dir = jojobot_adapters::testing::scratch_path(name);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let http_port = free_port();
    let store_port = free_port();
    let stderr_path = state_dir.join("server.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_jojobot"))
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
        // **The server leads a process group of its own**, so a signal to the
        // group reaches the server and everything it started, as a service
        // manager's stop reaches every process of the unit.
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr_path).expect("a file for the server's stderr"))
        .spawn()
        .expect("the binary runs");
    let mut running = watch(child, http_port, stderr_path, scratch);
    running.wait_for_line("serving http://", jojobot_exercise::room::READY_TIMEOUT);
    running
}

/// Watch a spawned server's log from a thread of its own.
///
/// **The log is read as bytes and a line that is not UTF-8 is read lossily.**
/// `lines()` ends the loop at the first line it cannot decode, and a loop that
/// ends closes the channel, which a case reads as a server that stopped
/// talking.
fn watch(
    mut child: Child,
    http_port: u16,
    stderr_path: std::path::PathBuf,
    scratch: Scratch,
) -> Running {
    let pid = child.id();
    let mut log = std::io::BufReader::new(child.stdout.take().expect("stdout was piped"));
    let server = Server(Some(child));
    let (said, lines) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = Vec::new();
        while matches!(log.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            let text = String::from_utf8_lossy(&line).trim_end().to_string();
            line.clear();
            if said.send(text).is_err() {
                return;
            }
        }
    });
    Running {
        server,
        pid,
        http_port,
        lines,
        seen: Vec::new(),
        stderr_path,
        _scratch: scratch,
    }
}

impl Running {
    /// Wait for a line of the server's log that holds `needle`.
    fn wait_for_line(&mut self, needle: &str, patience: Duration) {
        let began = Instant::now();
        loop {
            match self.lines.recv_timeout(patience) {
                Ok(line) => {
                    let hit = line.contains(needle);
                    self.seen.push(line);
                    if hit {
                        return;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => panic!(
                    "the server said nothing for {patience:?} and never said {needle:?}; it had \
                     been {:?} since the wait began. What it said:\n{}",
                    began.elapsed(),
                    self.seen.join("\n")
                ),
                Err(mpsc::RecvTimeoutError::Disconnected) => panic!(
                    "the server closed its log before it said {needle:?}. {}\nWhat it said:\n{}",
                    self.how_it_ended(EXIT_GRACE),
                    self.seen.join("\n")
                ),
            }
        }
    }

    /// What is known of a server whose log has closed: how it exited and what it
    /// wrote to its standard error.
    ///
    /// **The log closes when the process closes its standard output, which can
    /// come before the process is gone**, and a status read at that instant says
    /// "still running" about a server that exits a moment later. So this waits for
    /// the exit up to `within`, polling, and only a server still alive at the end
    /// of that time is reported as running.
    fn how_it_ended(&mut self, within: Duration) -> String {
        let deadline = Instant::now() + within;
        let status = loop {
            match self.server.0.as_mut().map(Child::try_wait) {
                Some(Ok(Some(status))) => break format!("it exited: {status}"),
                Some(Ok(None)) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Some(Ok(None)) => break format!("it is still running after {within:?}"),
                Some(Err(e)) => break format!("its exit could not be read: {e}"),
                None => break "it was already collected".to_string(),
            }
        };
        let stderr = std::fs::read_to_string(&self.stderr_path)
            .unwrap_or_else(|e| format!("(its stderr could not be read: {e})"));
        format!("{status}. Its stderr:\n{stderr}")
    }

    /// Send `signal` to the server.
    fn signal(&self, signal: &str) {
        let sent = Command::new("kill")
            .args([&format!("-{signal}"), &self.pid.to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success(), "kill -{signal} {} failed", self.pid);
    }

    /// Send `signal` to every process of the server's group: the server and the
    /// store it started.
    fn signal_group(&self, signal: &str) {
        let sent = Command::new("kill")
            .args(["-s", signal, "--", &format!("-{}", self.pid)])
            .status()
            .expect("kill runs");
        assert!(sent.success(), "kill -s {signal} -{} failed", self.pid);
    }

    /// One whole call: the request, then the answer read to the end.
    fn call(&self, extra: &str, body: &str) -> String {
        let mut call = self.open_a_call("/mcp", "POST", extra, body.len());
        call.write_all(body.as_bytes()).expect("the body");
        let mut answer = String::new();
        call.read_to_string(&mut answer)
            .expect("the answer is read to the end");
        // A call is answered 200 and a notification is accepted with 202.
        assert!(answer.starts_with("HTTP/1.1 20"), "{answer}");
        answer
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
        call.set_read_timeout(Some(READ_PATIENCE))
            .expect("a timeout");
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

/// **A call that needs the store is answered when the whole group is stopped.**
/// A service manager's stop sends SIGTERM to every process of the unit, the store
/// included, so the store must not be one that dies of it: the drain window is
/// for calls that still need it. The call is a write held open between two parts
/// of its body, and the signal goes to the server's whole group between them.
/// Paired with the cases above, which signal the server alone and hold a call
/// that never reaches the store.
#[test]
fn a_write_in_flight_is_answered_when_the_whole_group_is_stopped() {
    let mut running = start("jojobot-stops-its-whole-group");
    // **The start's own writes are done first.** The release notes are sent from a
    // task of their own after the serving line, and a stop that came during them
    // would put their store errors in the log this case reads.
    running.wait_for_line("release notes pass finished", STOP_PATIENCE);
    // **A run to write with**, from a boot that finishes before the stop.
    let init = running.call(JSON_HEADERS, BODY);
    let session = init
        .lines()
        .find_map(|line| line.strip_prefix("mcp-session-id: "))
        .unwrap_or_else(|| panic!("the server named no session: {init}"))
        .trim()
        .to_string();
    let with_session = format!("{JSON_HEADERS}mcp-session-id: {session}\r\n");
    running.call(
        &with_session,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    let boot = running.call(
        &with_session,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"start_here","arguments":{"bot":"assistant"}}}"#,
    );
    let sid = boot
        .find("sk-")
        .map(|at| boot[at..at + 29].to_string())
        .unwrap_or_else(|| panic!("the boot named no sid: {boot}"));

    // **The write, held open.** Half of its body goes now.
    let write = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"journal","arguments":{{"sid":"{sid}","entry":"a beat written while the group is stopped"}}}}}}"#
    );
    let mut call = running.open_a_call("/mcp", "POST", &with_session, write.len());
    let (now, later) = write.split_at(write.len() / 2);
    call.write_all(now.as_bytes()).expect("half the body");
    call.flush().expect("flushed");

    // **The signal, to the group, and the server's word that it has begun to stop.**
    let signalled = Instant::now();
    running.signal_group("TERM");
    running.wait_for_line("shutting down", STOP_PATIENCE);

    // **The write finishes against the store, and is answered with its receipt.**
    call.write_all(later.as_bytes())
        .expect("the rest of the body");
    let mut answer = String::new();
    call.read_to_string(&mut answer)
        .expect("the answer is read to the end");
    assert!(
        answer.starts_with("HTTP/1.1 200"),
        "the write in flight did not complete: {answer}"
    );
    assert!(
        answer.contains("changed") && !answer.contains("isError\":true"),
        "the write in flight was not answered with a receipt: {answer:?}"
    );

    let status = running.exit_within(STOP_PATIENCE);
    assert_eq!(
        status.code(),
        Some(0),
        "stopping the group ended the server some way other than its orderly exit ({status:?})"
    );
    assert!(
        signalled.elapsed() < SHUTDOWN_DRAIN,
        "the stop waited out the drain window though the write had finished"
    );
    assert!(
        !running
            .seen
            .iter()
            .any(|line| line.contains("the memory store failed")),
        "the store failed under the drain. What it said:\n{}",
        running.seen.join("\n")
    );
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
    stream
        .set_read_timeout(Some(READ_PATIENCE))
        .expect("a timeout");
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

/// **A server that exits while it starts is reported with how it exited and
/// what it wrote to its standard error.** The log closes, the wait ends at once
/// rather than at its limit, and the failure names the exit status and the
/// stderr: that is the only place a server that cannot start says why.
#[test]
fn a_server_that_exits_while_starting_is_reported_with_its_exit_and_stderr() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-exits-early");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let stderr_path = state_dir.join("server.stderr");
    let child = Command::new("sh")
        .args([
            "-c",
            "echo 'starting up'; echo 'the store would not open' >&2; exit 3",
        ])
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr_path).expect("a file for stderr"))
        .spawn()
        .expect("sh runs");
    let mut running = watch(child, 0, stderr_path, scratch);

    let started = Instant::now();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        running.wait_for_line("serving http://", jojobot_exercise::room::READY_TIMEOUT);
    }))
    .expect_err("a server that never serves is a failure");
    let said = panic
        .downcast_ref::<String>()
        .cloned()
        .expect("the failure is a message");

    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the wait ended at its limit and not when the log closed: {:?}",
        started.elapsed()
    );
    assert!(said.contains("exit status: 3"), "{said}");
    assert!(said.contains("the store would not open"), "{said}");
    assert!(said.contains("starting up"), "{said}");
}

/// **A server whose log has closed but which has not finished exiting is reported
/// by how it exits.** The log closes when the process closes its standard output,
/// which can come a moment before the process is gone. The failure waits for the
/// exit and names it; reading the status once at the instant the log closed says
/// "still running" about a server that exits a moment later.
#[test]
fn a_server_that_closes_its_log_and_exits_a_moment_later_is_reported_with_its_exit() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-exits-late");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let stderr_path = state_dir.join("server.stderr");
    let child = Command::new("sh")
        .args([
            "-c",
            "echo 'starting up'; echo 'the store would not open' >&2; exec >&-; sleep 1; exit 3",
        ])
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr_path).expect("a file for stderr"))
        .spawn()
        .expect("sh runs");
    let mut running = watch(child, 0, stderr_path, scratch);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        running.wait_for_line("serving http://", jojobot_exercise::room::READY_TIMEOUT);
    }))
    .expect_err("a server that never serves is a failure");
    let said = panic
        .downcast_ref::<String>()
        .cloned()
        .expect("the failure is a message");

    assert!(said.contains("exit status: 3"), "{said}");
    assert!(said.contains("the store would not open"), "{said}");
    assert!(!said.contains("still running"), "{said}");
}

/// **A server that closes its log and stays alive is still reported as running**,
/// after the wait for its exit, so the wait does not turn every closed log into
/// an exit.
#[test]
fn a_server_that_closes_its_log_and_stays_alive_is_reported_as_running() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-stays-alive");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let stderr_path = state_dir.join("server.stderr");
    let child = Command::new("sh")
        .args(["-c", "exec >&-; sleep 60"])
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr_path).expect("a file for stderr"))
        .spawn()
        .expect("sh runs");
    let mut running = watch(child, 0, stderr_path, scratch);

    let started = Instant::now();
    let said = running.how_it_ended(Duration::from_millis(300));

    assert!(said.contains("still running"), "{said}");
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "it was judged before the deadline: {:?}",
        started.elapsed()
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the wait did not end at the deadline: {:?}",
        started.elapsed()
    );
}

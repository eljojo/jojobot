//! **What a boot costs over a log with a tail, in each state its snapshot can be
//! in, and that an orderly stop leaves a snapshot the next boot loads.** Each case
//! spawns the real binary over a store it prepared, times process start to the
//! first answer of `/healthz`, reads the process's peak resident memory, and
//! prints one line. The release promises a boot under [`BOOT_BOUND`] and a process
//! under [`MEMORY_BOUND_KIB`] (`MemoryMax=1G`); a case fails with the measured
//! number when it is over.
//!
//! The store is a fresh one the binary created, with [`EVENTS`] teaching events
//! appended to its log beside their legacy rows, so a boot that replays the log
//! has real work to do. It is built here and never committed (rule 661). The
//! counts are printed, so a reader of the run knows what size was measured.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::teaching::RowChange;
use jojobot_adapters::log_snapshot::take_snapshot;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_adapters::writer::Writer;
use jojobot_adapters::writer_log::WriterLog;
use jojobot_domain::log::{BatchId, Position, SealedBatch};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};

/// The release's boot promise.
const BOOT_BOUND: Duration = Duration::from_secs(20);
/// `MemoryMax=1G`.
const MEMORY_BOUND_KIB: u64 = 1024 * 1024;
/// Teaching events in the log's tail, in batches of [`PER_BATCH`].
const EVENTS: u64 = 20_000;
const PER_BATCH: u64 = 500;
const STOP_PATIENCE: Duration = Duration::from_secs(60);

struct Running {
    child: Option<Child>,
    pid: u32,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    started: Instant,
    http_port: u16,
    state: Scratch,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Start the real binary over `state` and return at once.
fn spawn(state: Scratch) -> Running {
    let stderr = state.0.join("server.stderr");
    let mut child = Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").expect("a PATH to find the store"),
        )
        .env("STATE_DIRECTORY", &state.0)
        .env("JOJOBOT_STORE_PORT", free_port().to_string())
        .env("JOJOBOT_BIND", "127.0.0.1:0")
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("RUST_LOG", "info")
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(stderr).expect("a file for the server's stderr"))
        .spawn()
        .expect("the binary runs");
    let started = Instant::now();
    let pid = child.id();
    let mut log = std::io::BufReader::new(child.stdout.take().expect("stdout was piped"));
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
        child: Some(child),
        pid,
        lines,
        seen: Vec::new(),
        started,
        http_port: 0,
        state,
    }
}

impl Running {
    fn wait_for_line(&mut self, needle: &str) {
        let patience = jojobot_exercise::room::READY_TIMEOUT;
        loop {
            match self.lines.recv_timeout(patience) {
                Ok(line) => {
                    let hit = line.contains(needle);
                    self.seen.push(line);
                    if hit {
                        return;
                    }
                }
                Err(error) => panic!(
                    "{error}: the server never said {needle:?}. What it said:\n{}",
                    self.seen.join("\n")
                ),
            }
        }
    }

    /// Wait for the serving line, then ask `/healthz`. Returns the time from the
    /// spawn to the answer.
    fn first_answer(&mut self) -> Duration {
        self.wait_for_line("serving http://");
        self.http_port = self
            .seen
            .last()
            .and_then(|line| jojobot_adapters::testing::served_port(line))
            .expect("the serving line names a port");
        let mut call = TcpStream::connect(("127.0.0.1", self.http_port)).expect("connects");
        call.write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            .expect("the request");
        let mut answer = String::new();
        call.read_to_string(&mut answer).expect("the answer");
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        self.started.elapsed()
    }

    /// Peak resident memory of the process, from the kernel's own account.
    fn peak_kib(&self) -> u64 {
        std::fs::read_to_string(format!("/proc/{}/status", self.pid))
            .expect("the process status")
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))
            .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
            .expect("VmHWM is in the status")
    }

    fn signal(&self, signal: &str) {
        let sent = Command::new("kill")
            .args([&format!("-{signal}"), &self.pid.to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success(), "kill -{signal} {} failed", self.pid);
    }

    fn wait_exit(&mut self) -> std::process::ExitStatus {
        let began = Instant::now();
        loop {
            let child = self.child.as_mut().expect("not yet collected");
            if let Some(status) = child.try_wait().expect("status") {
                self.child = None;
                return status;
            }
            assert!(
                began.elapsed() < STOP_PATIENCE,
                "the server did not exit in {STOP_PATIENCE:?}. What it said:\n{}",
                self.seen.join("\n")
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// An orderly stop: SIGTERM, then the exit must be clean. The rest of the
    /// server's log is read into `seen`.
    fn stopped(mut self) -> (Scratch, Vec<String>) {
        self.signal("TERM");
        let status = self.wait_exit();
        assert_eq!(status.code(), Some(0), "the stop was not orderly");
        while let Ok(line) = self.lines.recv_timeout(Duration::from_secs(2)) {
            self.seen.push(line);
        }
        let seen = std::mem::take(&mut self.seen);
        let state = std::mem::replace(&mut self.state, Scratch("/nonexistent".into()));
        (state, seen)
    }

    /// A crash: SIGKILL to the server and the store it started, with no stop. The
    /// server leads a group of its own, so the store cannot outlive it and hold
    /// the directory against the next start.
    fn killed(mut self) -> Scratch {
        let sent = Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{}", self.pid)])
            .status()
            .expect("kill runs");
        assert!(sent.success(), "kill -s KILL -{} failed", self.pid);
        let _ = self.wait_exit();
        std::mem::replace(&mut self.state, Scratch("/nonexistent".into()))
    }

    /// The event log boot line, as the server wrote it.
    fn log_boot(&self) -> String {
        self.seen
            .iter()
            .find(|line| line.contains("event log boot"))
            .unwrap_or_else(|| panic!("no event log boot line:\n{}", self.seen.join("\n")))
            .clone()
    }
}

/// A store the binary created and stopped, with nothing in its log.
fn fresh_store(name: &str) -> Scratch {
    let dir = jojobot_adapters::testing::scratch_path(name);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let first = spawn(Scratch(dir));
    let mut first = first;
    first.first_answer();
    first.stopped().0
}

fn batch(id: &str, head: Position, first: u64, count: u64) -> (SealedBatch, Vec<RowChange>) {
    let at = jiff::Timestamp::from_second(1_700_000_000).unwrap();
    let members = (first..first + count)
        .map(|n| {
            jojobot_adapters::teaching_log::codec::encode(&TeachingEvent::Recorded {
                run: RunId(format!("r{n:06}")),
                domain: "claims".into(),
                taught_at: at,
            })
        })
        .collect();
    let rows = (first..first + count)
        .map(|n| RowChange::Record {
            sid: format!("r{n:06}"),
            domain: "claims".into(),
            taught_at: at,
        })
        .collect();
    (
        SealedBatch::seal(BatchId(id.into()), head, at, 1, members).unwrap(),
        rows,
    )
}

/// What a case does to the log's snapshot.
#[derive(Clone, Copy, Debug)]
enum Plan {
    /// No snapshot: a full replay.
    None,
    /// A snapshot at the head.
    Warm,
    /// A snapshot at half the tail; the other half follows it.
    Stale,
    /// A snapshot at the head, then made unusable by a change of the log's
    /// representation epoch.
    Invalidated,
}

/// Append [`EVENTS`] events to the log of the store in `state`, then leave the
/// snapshot as `plan` says.
async fn prepare(state: &Scratch, plan: Plan) {
    let mut store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("the store comes up with no binary over it");
    let pool = store.pool().clone();
    let writer = Writer::default();
    let log = WriterLog::open(pool.clone(), writer.clone())
        .await
        .expect("the log opens");
    let half = (EVENTS / PER_BATCH) / 2;
    let mut head = Position(0);
    for n in 0..EVENTS / PER_BATCH {
        let (sealed, rows) = batch(&format!("measure-{n}"), head, n * PER_BATCH, PER_BATCH);
        log.append_with_rows(head, &sealed, rows)
            .await
            .expect("the batch lands");
        head = sealed.range().last;
        if matches!(plan, Plan::Stale) && n + 1 == half {
            take_snapshot(&log, jiff::Timestamp::now())
                .await
                .expect("the half snapshot");
        }
    }
    if matches!(plan, Plan::Warm | Plan::Invalidated) {
        take_snapshot(&log, jiff::Timestamp::now())
            .await
            .expect("the snapshot");
    }
    if matches!(plan, Plan::Invalidated) {
        let payload: Vec<u8> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot = 'validity'")
                .fetch_one(&pool)
                .await
                .expect("the validity slot");
        let mut validity: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let epoch = validity["representation_epoch"].as_u64().unwrap_or(0);
        validity["representation_epoch"] = (epoch + 1).into();
        sqlx::query("UPDATE event_log SET payload = ? WHERE slot = 'validity'")
            .bind(serde_json::to_vec(&validity).unwrap())
            .execute(&pool)
            .await
            .expect("the validity changes");
    }
    drop(log);
    writer.drain().await;
    drop(writer);
    store.stop().await;
}

/// One measured boot: print its line and assert the bounds.
fn measured(case: &str, run: &mut Running) -> Duration {
    let took = run.first_answer();
    let peak = run.peak_kib();
    println!(
        "boot-measure {case}: events={EVENTS} batches={} start_to_first_answer_ms={} \
         peak_rss_kib={peak} | {}",
        EVENTS / PER_BATCH,
        took.as_millis(),
        run.log_boot()
    );
    assert!(
        took < BOOT_BOUND,
        "{case}: the first answer took {took:?}, over {BOOT_BOUND:?}"
    );
    assert!(
        peak < MEMORY_BOUND_KIB,
        "{case}: peak memory was {peak} kB, over {MEMORY_BOUND_KIB} kB (MemoryMax=1G)"
    );
    took
}

async fn case(name: &str, plan: Plan, from_snapshot: bool) {
    let state = fresh_store(&format!("jojobot-measure-{name}"));
    prepare(&state, plan).await;
    let mut run = spawn(state);
    measured(name, &mut run);
    let line = run.log_boot();
    assert!(
        line.contains(&format!("from_snapshot={from_snapshot}")),
        "{name}: the boot took the other path: {line}"
    );
    let _ = run.stopped();
}

/// No snapshot: the boot replays the whole tail.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_with_no_snapshot_replays_the_log() {
    case("none", Plan::None, false).await;
}

/// A snapshot at the head: the boot loads it and replays nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_with_a_warm_snapshot_loads_it() {
    case("warm", Plan::Warm, true).await;
}

/// A snapshot behind the head: the boot loads it and replays the rest.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_with_a_stale_snapshot_loads_it_and_replays_the_tail() {
    case("stale", Plan::Stale, true).await;
}

/// A snapshot the log's epoch no longer accepts: the boot replays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_with_an_invalidated_snapshot_replays_the_log() {
    case("invalidated", Plan::Invalidated, false).await;
}

/// **A server killed after writing boots from the snapshot it had.** The first
/// process is stopped orderly over a store with a tail, so a stop snapshot covers
/// it; a second boots, is killed with no stop, and a third boots from the same
/// snapshot with the tail it had.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_after_a_crash_loads_the_snapshot_the_stop_left() {
    let state = fresh_store("jojobot-measure-crash");
    prepare(&state, Plan::None).await;
    let mut first = spawn(state);
    first.first_answer();
    let (state, seen) = first.stopped();
    // The tick can publish it just before the stop does; either way the log holds
    // a snapshot by the time the process has exited.
    assert!(
        seen.iter().any(|line| line.contains("snapshot taken")),
        "the first process took no snapshot. What it said:\n{}",
        seen.join("\n")
    );
    let mut second = spawn(state);
    second.first_answer();
    assert!(second.log_boot().contains("from_snapshot=true"));
    let state = second.killed();
    let mut third = spawn(state);
    measured("crash", &mut third);
    assert!(
        third.log_boot().contains("from_snapshot=true"),
        "{}",
        third.log_boot()
    );
    let _ = third.stopped();
}

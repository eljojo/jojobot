//! **What a boot costs over a log with a tail, in each state its snapshot can be
//! in, and that an orderly stop leaves a snapshot the next boot loads.** Each case
//! spawns the real binary over a store it prepared, times process start to the
//! first answer of `/healthz`, reads the peak resident memory of the server and its store, and
//! prints one line. The release promises a boot under [`BOOT_BOUND`] and a process
//! under [`memory_bound_kib`] (the `memoryMax` of the NixOS module). **Only the memory
//! bound fails a case**, with the measured number, on both boots. The wall time and the
//! cpu time of each boot are reported against [`BOOT_BOUND`] on every run and never fail
//! it, because wall time moves with the runner.
//!
//! The store is the one the last deployed build recorded (`UPGRADE_FIXTURE_DIR`,
//! as for the upgrade gate), booted once by the binary under test, with [`EVENTS`]
//! teaching events appended to its log, so a boot that
//! replays the log has real work to do. It is built here and never committed
//! (rule 661). Each case appends its numbers and the server's own timed boot steps
//! to `target/bar/boot-measures.log`, with the counts of things, messages and log
//! events, so a reader of the run knows what size was measured.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::log_snapshot::take_snapshot;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_adapters::writer::Writer;
use jojobot_adapters::writer_log::WriterLog;
use jojobot_domain::log::{BatchId, LogStore, Position, SealedBatch};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};

/// The release's boot promise.
const BOOT_BOUND: Duration = Duration::from_secs(20);

/// **The memory the unit may use**, read from the NixOS module's `memoryMax`
/// default, which the unit's `MemoryMax` takes, so the bound is the one the
/// deployment sets and not a copy of it. It covers the server and the Dolt store it
/// starts, so the cases compare it with their sum.
fn memory_bound_kib() -> u64 {
    let module =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nix/modules/jojobot.nix");
    let text = std::fs::read_to_string(&module)
        .unwrap_or_else(|e| panic!("reading {}: {e}", module.display()));
    let option = text
        .split("memoryMax = mkOption {")
        .nth(1)
        .expect("the module declares the memoryMax option");
    let value = option
        .split("default = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the memoryMax option has a default");
    systemd_size_kib(value)
}

/// A systemd size (`4G`, `512M`, `1048576K`, bytes when bare) in kB.
fn systemd_size_kib(size: &str) -> u64 {
    let digits = size.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let number: u64 = digits
        .parse()
        .unwrap_or_else(|_| panic!("{size:?} is not a systemd size"));
    match &size[digits.len()..] {
        "" => number / 1024,
        "K" => number,
        "M" => number * 1024,
        "G" => number * 1024 * 1024,
        "T" => number * 1024 * 1024 * 1024,
        other => panic!("{size:?}: the unit {other:?} is not one the bound reads"),
    }
}

#[test]
fn the_memory_bound_is_read_from_the_module_and_sizes_are_read_with_their_unit() {
    assert_eq!(systemd_size_kib("4G"), 4 * 1024 * 1024);
    assert_eq!(systemd_size_kib("512M"), 512 * 1024);
    assert_eq!(systemd_size_kib("1048576K"), 1024 * 1024);
    assert_eq!(systemd_size_kib("2097152"), 2048);
    assert!(
        memory_bound_kib() >= 1024 * 1024,
        "the module bounds the unit below a gigabyte"
    );
}
/// Teaching events in the log's tail, in batches of [`PER_BATCH`].
const EVENTS: u64 = 20_000;
const PER_BATCH: u64 = 500;
/// **The live instance's size, read from its store** (read only, 2026-11-06): the
/// release is measured at these. Entities, claims, fact writes, field writes,
/// messages, sessions (runs), and the call counters the server keeps.
const PRODUCTION_THINGS: i64 = 3_208;
const PRODUCTION_CLAIMS: i64 = 25_955;
const PRODUCTION_FACT_WRITES: i64 = 45_294;
const PRODUCTION_FIELD_WRITES: i64 = 108_942;
const PRODUCTION_MESSAGES: i64 = 24_134;
const PRODUCTION_SESSIONS: i64 = 1_560;
const PRODUCTION_RUN_CALLS: i64 = 94_905;
const PRODUCTION_TOOL_CALLS: i64 = 116_258;
/// **What the server writes through its tools**: every entity, one claim under
/// each with [`SETS_PER_CLAIM`] keys, [`TOOL_MESSAGES`] messages and the sessions.
/// Writing the instance's whole count that way takes more than an hour, so
/// [`scale_to_production`] reaches the counts above by copying rows the server
/// wrote. A copy shares its parent's shape: the row counts and the bytes are
/// production's, the variety of content is not. Copied messages stay in the state
/// they were posted in (new), where production's are nearly all processed.
const SETS_PER_CLAIM: usize = 4;
const TOOL_MESSAGES: i64 = 1_000;
/// Connections that write the grown store at once.
const GROW_CLIENTS: i64 = 8;
/// A tail under the snapshot cadence of 10,000 events, for the case that needs the
/// stop to be the only moment a snapshot is due.
const SMALL_TAIL: u64 = 1_000;
const STOP_PATIENCE: Duration = Duration::from_secs(60);

/// **One boot at a time.** The cases run side by side in one binary on one runner,
/// and a boot timed while five others start is timed against their contention: the
/// same case moved from 23.1 s to 18.6 s between two runs. A process holds this from
/// before it starts until its first answer, so every boot is timed alone; the rest
/// of a case (preparing its store, stopping the process) runs side by side.
static ONE_BOOT: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The memory report: the server's resident memory across a first boot over the
/// production-sized store and a replayed day of calls.
#[path = "boot_measures/memory_report.rs"]
mod memory_report;

struct Running {
    /// Held from before the process starts until it has answered.
    boot_turn: Option<std::sync::MutexGuard<'static, ()>>,
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
    spawn_binary(state, env!("CARGO_BIN_EXE_jojobot"), 0)
}

fn recording(state: Scratch) -> Running {
    let binary = std::env::var("UPGRADE_FIXTURE_BINARY")
        .expect("the recording runs while the deployed binary is available");
    // Older builds log the configured port instead of the bound port.
    spawn_binary(state, &binary, free_port())
}

fn spawn_binary(state: Scratch, binary: &str, http_port: u16) -> Running {
    let boot_turn = ONE_BOOT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stderr = state.0.join("server.stderr");
    let mut child = Command::new(binary)
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").expect("a PATH to find the store"),
        )
        .env("STATE_DIRECTORY", &state.0)
        .env("JOJOBOT_STORE_PORT", free_port().to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
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
        boot_turn: Some(boot_turn),
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
        let took = self.started.elapsed();
        self.boot_turn = None;
        took
    }

    /// **Peak resident memory of the server and the store it started**, from the
    /// kernel's own account, as `(server, store)`. The unit's `MemoryMax` covers the
    /// service's whole cgroup, so the bound is on their sum. The store is every
    /// descendant of the server's pid.
    fn peaks_kib(&self) -> (u64, u64) {
        fn field(status: &str, name: &str) -> Option<u64> {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        }
        let mut parents = std::collections::HashMap::new();
        let mut peaks = std::collections::HashMap::new();
        for entry in std::fs::read_dir("/proc").expect("/proc").flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            // A process can exit between the listing and the read.
            let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
                continue;
            };
            parents.insert(pid, field(&status, "PPid:").unwrap_or(0) as u32);
            peaks.insert(pid, field(&status, "VmHWM:").unwrap_or(0));
        }
        let server = *peaks.get(&self.pid).expect("the server's VmHWM");
        let mut below = std::collections::HashSet::from([self.pid]);
        loop {
            let found: Vec<u32> = parents
                .iter()
                .filter(|(pid, parent)| below.contains(parent) && !below.contains(pid))
                .map(|(pid, _)| *pid)
                .collect();
            if found.is_empty() {
                break;
            }
            below.extend(found);
        }
        let store = below
            .iter()
            .filter(|pid| **pid != self.pid)
            .map(|pid| peaks[pid])
            .sum();
        (server, store)
    }

    /// **The processor time the server and the store it started have spent**, in
    /// milliseconds, from the kernel's own account. A boot's wall time moves with the
    /// other jobs the runner is running; this does not, so it says whether a slow
    /// boot did more work or waited for the machine.
    fn cpu_ms(&self) -> u64 {
        let ticks: u64 = Command::new("getconf")
            .arg("CLK_TCK")
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(100);
        let mut parents = std::collections::HashMap::new();
        let mut spent = std::collections::HashMap::new();
        for entry in std::fs::read_dir("/proc").expect("/proc").flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            // The command name is in parentheses and may hold spaces; the fields
            // after it are counted from the state (field 3): ppid is the second
            // after it, utime and stime the twelfth and thirteenth.
            let Some((_, rest)) = stat.rsplit_once(") ") else {
                continue;
            };
            let fields: Vec<&str> = rest.split_whitespace().collect();
            let number = |at: usize| fields.get(at).and_then(|f| f.parse::<u64>().ok());
            if let (Some(parent), Some(user), Some(system)) = (number(1), number(11), number(12)) {
                parents.insert(pid, parent as u32);
                spent.insert(pid, user + system);
            }
        }
        let mut below = std::collections::HashSet::from([self.pid]);
        loop {
            let found: Vec<u32> = parents
                .iter()
                .filter(|(pid, parent)| below.contains(parent) && !below.contains(pid))
                .map(|(pid, _)| *pid)
                .collect();
            if found.is_empty() {
                break;
            }
            below.extend(found);
        }
        below.iter().filter_map(|pid| spent.get(pid)).sum::<u64>() * 1000 / ticks
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

    /// A crash: SIGKILL to the server's process group, with no stop. The store
    /// starts its own group and is killed when its parent dies, so it does not
    /// hold the directory against the next start either.
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
    /// A number from the event log boot line, such as `tail_batches=2`.
    fn boot_field(&self, name: &str) -> u64 {
        let line = self.log_boot();
        let from = line
            .find(&format!("{name}="))
            .unwrap_or_else(|| panic!("no {name} in {line}"))
            + name.len()
            + 1;
        line[from..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or_else(|_| panic!("{name} is no number in {line}"))
    }

    fn log_boot(&self) -> String {
        self.seen
            .iter()
            .find(|line| line.contains("event log boot"))
            .unwrap_or_else(|| panic!("no event log boot line:\n{}", self.seen.join("\n")))
            .clone()
    }
}

/// **The statements of a dump, split at the semicolons that end one.** A
/// semicolon inside a quoted value ends nothing. The same reader as the upgrade
/// gate's.
fn split_sql_statements(dump: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_value = false;
    let mut chars = dump.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if in_value {
            match c {
                '\\' => current.extend(chars.next()),
                '\'' if chars.peek() == Some(&'\'') => current.extend(chars.next()),
                '\'' => in_value = false,
                _ => {}
            }
        } else if c == '\'' {
            in_value = true;
        } else if c == ';' {
            statements.push(std::mem::take(&mut current).trim().to_string());
        }
    }
    let rest = current.trim();
    if !rest.is_empty() {
        statements.push(format!("{rest};"));
    }
    statements.retain(|statement| statement != ";");
    statements
}

/// **A recorded store** replayed into a store of this case's own and then booted
/// once by the binary under test before a measurement. When growing, the deployed
/// binary records rows so the head's import follows scaling. `source` names its directory:
/// the cases read the grown store CI cached (`BOOT_STORE_DIR`), and the recorder
/// that makes that store starts from the one the last deployed build recorded
/// (`UPGRADE_FIXTURE_DIR`, as for the upgrade gate) and grows it when `growing`.
/// The log's tail is added on top by [`prepare`].
async fn restore_into(name: &str, source: &str, growing: bool) -> Scratch {
    let fixture = std::env::var(source)
        .unwrap_or_else(|_| panic!("CI supplies {source}, the store this case starts from"));
    restore_dump(name, &fixture, growing).await
}

async fn restore_dump(name: &str, fixture: &str, growing: bool) -> Scratch {
    let state = replay_dump(name, fixture).await;
    let mut first = if growing {
        recording(state)
    } else {
        spawn(state)
    };
    first.first_answer();
    if growing {
        grow(first.http_port).await;
    }
    first.stopped().0
}

/// **The store a dump holds, replayed into a directory of its own and stopped**, so
/// no binary has booted over it yet: the next boot is a first boot over the rows the
/// dump holds.
async fn replay_dump(name: &str, fixture: &str) -> Scratch {
    // **A relative path is the workspace root's**, as the upgrade gate reads it: a
    // test runs from its own crate's directory, and CI names the directory
    // `target/upgrade-fixture`.
    let fixture = std::path::PathBuf::from(fixture);
    let fixture = if fixture.is_absolute() {
        fixture
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(fixture)
    };
    let fixture = fixture
        .to_str()
        .expect("the fixture path is UTF-8")
        .to_string();
    let dir = jojobot_adapters::testing::scratch_path(name);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let state = Scratch(dir);
    let mut store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("a store comes up to receive the recording");
    let dump = std::fs::read_to_string(format!("{fixture}/doltdump.sql"))
        .unwrap_or_else(|e| panic!("reading {fixture}/doltdump.sql: {e}"));
    for statement in split_sql_statements(&dump) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(store.pool())
            .await
            .unwrap_or_else(|e| panic!("replaying the recording failed on:\n{statement}\n{e}"));
    }
    store.stop().await;
    state
}

/// The cached grown store, restored once for the whole test binary: every case
/// needs the same one.
static TEMPLATE: tokio::sync::OnceCell<Scratch> = tokio::sync::OnceCell::const_new();

/// **A copy of the grown store for one case**, so a case changes its own and no
/// other.
async fn restored_store(name: &str) -> Scratch {
    let template = TEMPLATE
        .get_or_init(|| restore_into("jojobot-measure-template", "BOOT_STORE_DIR", false))
        .await;
    let dir = jojobot_adapters::testing::scratch_path(name);
    let _ = std::fs::remove_dir_all(&dir);
    let copied = Command::new("cp")
        .arg("-a")
        .arg(&template.0)
        .arg(&dir)
        .status()
        .expect("the copy runs");
    assert!(copied.success(), "the grown store copies");
    Scratch(dir)
}

/// **The recorder of the cached grown store.** `scripts/prepare-boot-store` runs it
/// on a cache miss, with `BOOT_STORE_DIR` naming an absolute directory outside git
/// for the result. It grows the store the last deployed build recorded, stops the
/// server and dumps the store the way the upgrade fixture is dumped. The bar never
/// runs it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run through scripts/prepare-boot-store, never by `make check`"]
async fn record_the_grown_store() {
    let out = std::path::PathBuf::from(
        std::env::var("BOOT_STORE_DIR").expect("the script names BOOT_STORE_DIR"),
    );
    assert!(
        out.is_absolute(),
        "the recorder writes to an absolute directory"
    );
    let grown = restore_into("jojobot-measure-recorder", "UPGRADE_FIXTURE_DIR", true).await;
    let mut store = scaling_store(&grown).await;
    jojobot_domain::memory::kinds::load_shipped();
    remove_the_recorders_log(store.pool()).await;
    // Opening the log ensures its validity row, which is kept in the cached dump.
    let log = WriterLog::open(store.pool().clone(), Writer::default())
        .await
        .expect("the recorder log opens before scaling");
    assert_eq!(
        claims_counts(&log.read_from(Position(0), 100_000).await.unwrap()),
        (0, 0),
        "the recorder's log holds no claims events before the first head boot"
    );
    drop(log);
    let counts = scale_to_production(store.pool()).await;
    store.stop().await;
    dump_grown_store(&grown, &out, &counts);
}

/// **Leave the log of a store the claims import has not run on**, which is what a
/// recorder that predates the claims log leaves. The recorder is the last deployed
/// build, and since 9.3.3 that build imports claims on its own first boot, so its log
/// already holds them. The batches and the snapshots are removed and the log's
/// identity row stays: what is left is the log the head boot imports into.
async fn remove_the_recorders_log(pool: &sqlx::MySqlPool) {
    sqlx::query("DELETE FROM event_log WHERE slot <> 'validity'")
        .execute(pool)
        .await
        .expect("the recorder's events and snapshots are removed");
}

async fn scaling_store(state: &Scratch) -> Dolt {
    let store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("the store comes up with no binary over it");
    // Bring rows to the head schema without booting its claims import.
    jojobot_adapters::dolt::migrate::run(store.pool())
        .await
        .expect("the current migrations run before scaling");
    store
}

fn dump_grown_store(grown: &Scratch, out: &std::path::Path, counts: &str) {
    let database = grown.0.join("db").join("jojobot");
    assert!(
        database.join(".dolt").is_dir(),
        "the binary's database is not where the recorder expects: {}",
        database.display()
    );
    // The dump tool keeps its configuration under a home directory of its own.
    let home = grown.0.join("dolt-home");
    std::fs::create_dir_all(&home).expect("a directory for the dump tool's configuration");
    let dump = Command::new("dolt")
        .env("DOLT_ROOT_PATH", &home)
        .args(["dump", "-r", "sql", "-f", "-fn", "doltdump.sql"])
        .current_dir(&database)
        .output()
        .expect("dolt dump runs");
    assert!(
        dump.status.success(),
        "dolt dump failed: {}",
        String::from_utf8_lossy(&dump.stderr)
    );
    std::fs::create_dir_all(out).expect("the output directory");
    std::fs::copy(database.join("doltdump.sql"), out.join("doltdump.sql"))
        .expect("the dump is copied");
    std::fs::write(out.join("grown.txt"), format!("{counts}\n"))
        .expect("the record of what was grown");
}

/// Copy the rows of `table` that `filter` selects, once, with the columns in
/// `replace` computed instead of copied. The column list is read from the store,
/// so a column a later migration adds is copied too.
async fn copy_rows(pool: &sqlx::MySqlPool, table: &str, replace: &[(&str, String)], filter: &str) {
    use sqlx::Row;
    let columns: Vec<String> =
        sqlx::query(sqlx::AssertSqlSafe(format!("SHOW COLUMNS FROM {table}")))
            .fetch_all(pool)
            .await
            .expect("the columns read")
            .iter()
            .map(|row| row.get::<String, _>("Field"))
            .collect();
    let names = columns
        .iter()
        .map(|column| format!("`{column}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let values = columns
        .iter()
        .map(|column| {
            replace.iter().find(|(name, _)| name == column).map_or_else(
                || format!("`{column}`"),
                |(_, expression)| expression.clone(),
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let statement =
        format!("INSERT INTO {table} ({names}) SELECT {values} FROM {table} WHERE {filter}");
    sqlx::query(sqlx::AssertSqlSafe(statement.clone()))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("copying rows failed on:\n{statement}\n{e}"));
}

async fn count_of(pool: &sqlx::MySqlPool, what: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
        "SELECT CAST({what} AS SIGNED)"
    )))
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("counting {what} failed: {e}"))
}

/// **Bring the tool-written store to the live instance's counts** by copying rows
/// the server wrote (see [`SETS_PER_CLAIM`]). The copies are whole: a copied claim
/// brings its writes and its field writes, a longer history brings later ordinals
/// of the same claim, and the counters are scaled. Returns what the store holds.
async fn scale_to_production(pool: &sqlx::MySqlPool) -> String {
    scale_to(
        pool,
        &Scale {
            things: PRODUCTION_THINGS,
            messages: PRODUCTION_MESSAGES,
            claims: PRODUCTION_CLAIMS,
            fact_writes: PRODUCTION_FACT_WRITES,
            field_writes: PRODUCTION_FIELD_WRITES,
            tool_calls: PRODUCTION_TOOL_CALLS,
            run_calls: PRODUCTION_RUN_CALLS,
        },
    )
    .await
}

/// Counts are inputs so the recorder's SQL path can also be checked with a small seed.
struct Scale {
    things: i64,
    messages: i64,
    claims: i64,
    fact_writes: i64,
    field_writes: i64,
    tool_calls: i64,
    run_calls: i64,
}

async fn scale_to(pool: &sqlx::MySqlPool, target: &Scale) -> String {
    // **The claims this recorder wrote are found by their content**, which does not
    // depend on how the store spells an entity or a claim id. A copy gets an `x`
    // and its round after the id, and no id the server makes holds an `x`, so a
    // copy is never copied again.
    let grown = "content LIKE 'Load % sits on shelf%'";
    let fact_original = format!("{grown} AND id NOT LIKE '%x%'");
    let write_original = format!("{grown} AND fact_id NOT LIKE '%x%' AND ordinal = 1");
    let field_original = "fact_id NOT LIKE '%x%' AND EXISTS (SELECT 1 FROM fact f \
         WHERE f.entity = field_write.entity AND f.id = field_write.fact_id \
         AND f.content LIKE 'Load % sits on shelf%')";

    // **Messages**: copied whole, with their events, in the ordinal order the
    // original gave, after the last one.
    let length = count_of(pool, "(SELECT MAX(CHAR_LENGTH(id)) FROM message)").await;
    let held = count_of(
        pool,
        &format!("(SELECT COUNT(*) FROM message WHERE CHAR_LENGTH(id) = {length})"),
    )
    .await;
    let message_rounds =
        (target.messages - count_of(pool, "(SELECT COUNT(*) FROM message)").await + held - 1)
            / held;
    for round in 1..=message_rounds {
        let by = round * 1_000_000;
        copy_rows(
            pool,
            "message_event",
            &[("message_id", format!("CONCAT(`message_id`, 'c{round}')"))],
            &format!("CHAR_LENGTH(message_id) = {length}"),
        )
        .await;
        copy_rows(
            pool,
            "message",
            &[
                ("id", format!("CONCAT(`id`, 'c{round}')")),
                ("ordinal", format!("`ordinal` + {by}")),
            ],
            &format!("CHAR_LENGTH(id) = {length}"),
        )
        .await;
    }

    // **Claims**: each copy brings its fact, its writes and its field writes, and
    // the field writes take the next ordinal of their key.
    let claims = count_of(
        pool,
        &format!("(SELECT COUNT(*) FROM fact WHERE {})", fact_original),
    )
    .await;
    // **A store that holds none of the claims this recorder wrote is told apart
    // from a miscounted one by what it does hold**: the recorder stops and names a
    // few rows, so the next run does not have to guess at their spelling.
    if claims == 0 {
        use sqlx::Row;
        let seen: Vec<String> = sqlx::query("SELECT entity, id, content FROM fact LIMIT 8")
            .fetch_all(pool)
            .await
            .expect("a sample of the claims")
            .iter()
            .map(|row| {
                format!(
                    "{:?} {:?} {:?}",
                    row.get::<String, _>("entity"),
                    row.get::<String, _>("id"),
                    row.get::<String, _>("content")
                )
            })
            .collect();
        panic!("the store holds none of the claims the recorder wrote; some it holds: {seen:?}");
    }
    let claim_rounds = (target.claims + claims / 2) / claims - 1;
    for round in 1..=claim_rounds {
        copy_rows(
            pool,
            "fact",
            &[("id", format!("CONCAT(`id`, 'x{round}')"))],
            &fact_original,
        )
        .await;
        copy_rows(
            pool,
            "fact_write",
            &[("fact_id", format!("CONCAT(`fact_id`, 'x{round}')"))],
            &write_original,
        )
        .await;
        copy_rows(
            pool,
            "field_write",
            &[
                ("fact_id", format!("CONCAT(`fact_id`, 'x{round}')")),
                ("ordinal", format!("`ordinal` + {round}")),
            ],
            field_original,
        )
        .await;
    }

    // **Longer histories**: later writes of the same claims, until the writes
    // number what the instance holds.
    let writes = count_of(pool, "(SELECT COUNT(*) FROM fact_write)").await;
    let originals = count_of(
        pool,
        &format!("(SELECT COUNT(*) FROM fact_write WHERE {})", write_original),
    )
    .await;
    let history_rounds = ((target.fact_writes - writes + originals - 1) / originals).max(0);
    for round in 1..=history_rounds {
        copy_rows(
            pool,
            "fact_write",
            &[("ordinal", format!("`ordinal` + {round}"))],
            &write_original,
        )
        .await;
    }

    // **Counters** are scaled, not copied: what they hold is a sum.
    for (table, target) in [
        ("tool_call_count", target.tool_calls),
        ("run_call_count", target.run_calls),
    ] {
        let sum = count_of(
            pool,
            &format!("(SELECT COALESCE(SUM(calls), 0) FROM {table})"),
        )
        .await
        .max(1);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET calls = calls * {}",
            (target + sum - 1) / sum
        )))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("scaling {table} failed: {e}"));
    }

    let held = |table: &str| format!("(SELECT COUNT(*) FROM {table})");
    let (things, messages, claims, fact_writes, field_writes, sessions) = (
        count_of(pool, &held("entity")).await,
        count_of(pool, &held("message")).await,
        count_of(pool, &held("fact")).await,
        count_of(pool, &held("fact_write")).await,
        count_of(pool, &held("field_write")).await,
        count_of(pool, &held("session")).await,
    );
    let tool_calls = count_of(
        pool,
        "(SELECT COALESCE(SUM(calls), 0) FROM tool_call_count)",
    )
    .await;
    let run_calls = count_of(pool, "(SELECT COALESCE(SUM(calls), 0) FROM run_call_count)").await;
    // A tenth under the instance's count is a store that was not scaled.
    for (what, have, want) in [
        ("entities", things, target.things),
        ("messages", messages, target.messages),
        ("claims", claims, target.claims),
        ("fact writes", fact_writes, target.fact_writes),
        ("field writes", field_writes, target.field_writes),
        ("tool calls", tool_calls, target.tool_calls),
        ("run calls", run_calls, target.run_calls),
    ] {
        assert!(
            have * 10 >= want * 9,
            "{what}: the store holds {have}, the instance {want}"
        );
    }
    format!(
        "entities={things} claims={claims} fact_writes={fact_writes} field_writes={field_writes} messages={messages} sessions={sessions} tool_calls={tool_calls} run_calls={run_calls}"
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run through scripts/prepare-boot-store with the deployed binary"]
async fn scaled_claim_rows_and_the_restored_claims_state_hold_the_same_history() {
    compare_grown_claims(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run through scripts/prepare-boot-store with the deployed binary"]
async fn the_unscaled_seed_and_its_restored_claims_state_hold_the_same_history() {
    compare_grown_claims(false).await;
}

fn claims_counts(batches: &[SealedBatch]) -> (usize, usize) {
    use jojobot_domain::log::claims::ClaimEvent;

    jojobot_domain::memory::kinds::load_shipped();
    let mut records = 0;
    let mut imports = 0;
    for member in batches.iter().flat_map(|batch| batch.members()) {
        if let Some(event) = jojobot_adapters::claims_log::codec::decode_own(member)
            .expect("the claims event decodes")
        {
            records += usize::from(matches!(event.payload, ClaimEvent::RecordCreated { .. }));
            imports += usize::from(matches!(event.payload, ClaimEvent::ImportCompleted { .. }));
        }
    }
    (records, imports)
}

async fn compare_grown_claims(scaled: bool) {
    use jojobot_adapters::claims_log::LoggedClaims;
    use jojobot_adapters::dolt::memory::DoltMemory;
    use jojobot_domain::memory::{EntityId, Memory};
    use jojobot_exercise::surface::Surface;
    use serde_json::json;
    use std::sync::Arc;

    let mut running = recording(Scratch::new("grown-claims-seed"));
    running.first_answer();
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", running.http_port))
        .await
        .expect("the seed connects");
    let boot = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "resume": "new", "brief": true}),
        )
        .await
        .expect("the seed boots");
    let sid = boot["session"]["sid"]
        .as_str()
        .expect("the seed has a session");
    surface
        .must(
            "add_entity",
            json!({
                "kind": "thing", "handle": "kettle", "name": "Kettle",
                "source": "user-named", "sid": sid,
            }),
        )
        .await
        .expect("the seed thing is written");
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:kettle",
                "content": "Load kettle sits on shelf 1 of aisle 1",
                "sets": {"load_key_0": "shelf 1"}, "sid": sid,
            }),
        )
        .await
        .expect("the seed claim is written");
    surface
        .must(
            "post_message",
            json!({"to": "assistant", "body": "The kettle sits on shelf 1", "sid": sid}),
        )
        .await
        .expect("the recorder has a message seed");
    surface.finish().await;
    let seed = running.stopped().0;
    let mut store = scaling_store(&seed).await;
    remove_the_recorders_log(store.pool()).await;
    let writer = Writer::default();
    let seed_log = WriterLog::open(store.pool().clone(), writer.clone())
        .await
        .expect("the seed log opens");
    let head = seed_log.head().expect("the seed log holds its head");
    assert_eq!(
        claims_counts(&seed_log.read_from(Position(0), 100_000).await.unwrap()),
        (0, 0),
        "the seed log holds no claims events before the first head boot"
    );
    let other_family = batch("grown-claims-control", head, 1_000_000, 1);
    seed_log
        .append(head, &other_family)
        .await
        .expect("the other family has a marker");
    drop(seed_log);
    writer.drain().await;
    drop(writer);
    let counts = if scaled {
        let target = Scale {
            things: 1,
            messages: count_of(store.pool(), "(SELECT COUNT(*) FROM message)").await,
            claims: 2,
            fact_writes: count_of(store.pool(), "(SELECT COUNT(*) FROM fact_write)").await + 2,
            field_writes: 2,
            tool_calls: 1,
            run_calls: 1,
        };
        scale_to(store.pool(), &target).await
    } else {
        "unscaled tool-written seed".to_string()
    };
    store.stop().await;
    let recording = seed.0.join("recording");
    dump_grown_store(&seed, &recording, &counts);
    let restored = restore_dump("grown-claims-restored", recording.to_str().unwrap(), false).await;
    let mut store = Dolt::start(&restored.0.join("db"), free_port())
        .await
        .expect("the restored store opens");
    let pool = store.pool().clone();
    let memory = DoltMemory::open(pool.clone()).knowing(jojobot_mcp::provisions());
    let entity = EntityId("thing:kettle".into());
    let facts = memory.recall(&entity).await.expect("the row claims read");
    let mut facts: Vec<_> = facts
        .iter()
        .filter(|fact| fact.content.starts_with("Load kettle sits on shelf"))
        .collect();
    assert_eq!(
        facts.len(),
        if scaled { 2 } else { 1 },
        "the SQL path made the seed's copies"
    );
    assert_eq!(facts.iter().any(|fact| fact.id.0.ends_with("x1")), scaled);
    facts.sort_by_key(|fact| !fact.id.0.ends_with("x1"));
    jojobot_domain::memory::kinds::load_shipped();
    let log = WriterLog::open(pool.clone(), Writer::default())
        .await
        .expect("the restored log opens");
    let batches = log
        .read_from(Position(0), 100_000)
        .await
        .expect("the restored events read");
    let imported = claims_counts(&batches);
    assert_eq!(
        imported,
        (
            count_of(&pool, "(SELECT COUNT(*) FROM fact)").await as usize,
            1
        ),
        "the first head boot imports the complete grown population once"
    );
    for member in other_family.members() {
        assert_eq!(
            batches
                .iter()
                .flat_map(|batch| batch.members())
                .filter(|held| *held == member)
                .count(),
            1,
            "scaling claims preserves the other family's event exactly once"
        );
    }
    let held = LoggedClaims::open(Arc::new(log))
        .await
        .expect("the claims family opens")
        .state()
        .await;
    let addresses: Vec<_> = facts
        .iter()
        .map(|fact| fact.address().to_string())
        .collect();
    for fact in facts {
        let address = fact.address();
        let located = held
            .claim_at(&address.to_string())
            .unwrap_or_else(|| panic!("the restored claims State omitted copied row {address}"));
        assert_eq!(held.fact(&located.record).unwrap().content, fact.content);
        let history = memory
            .claim_history(&address)
            .await
            .expect("the row history reads");
        assert!(!history.is_empty());
        assert_eq!(
            held.claim_history(&located.record).len(),
            history.len(),
            "{address}: every revision reaches the held State"
        );
        if !fact.id.0.ends_with("x1") {
            assert_eq!(
                history.len(),
                if scaled { 2 } else { 1 },
                "the scaler extends the original claim's history"
            );
        }
    }
    let object = held
        .resolve("thing:kettle")
        .expect("the held thing resolves");
    let rows = memory
        .history(&entity, "load_key_0")
        .await
        .expect("the row key history reads");
    assert_eq!(rows.len(), if scaled { 2 } else { 1 });
    let mut state_rows = held
        .key_trails(object)
        .history
        .remove("load_key_0")
        .expect("the held key has its history");
    // State trails keep the permanent identity of the home; the row port serves its
    // current handle. The writer is already served as the handle its bot answers to.
    for write in &mut state_rows {
        write.fact.home = held
            .handle(&jojobot_domain::log::ObjectId(write.fact.home.0.clone()))
            .expect("a history home has its served handle");
    }
    assert_eq!(
        state_rows, rows,
        "the copied key writes reach the held State"
    );
    store.stop().await;
    let mut second = spawn(restored);
    second.first_answer();
    let restored = second.stopped().0;
    let mut store = Dolt::start(&restored.0.join("db"), free_port())
        .await
        .expect("the replayed store opens");
    let log = WriterLog::open(store.pool().clone(), Writer::default())
        .await
        .expect("the replayed log opens");
    assert_eq!(
        claims_counts(&log.read_from(Position(0), 100_000).await.unwrap()),
        imported,
        "the second head boot replays without importing again"
    );
    let replayed = LoggedClaims::open(Arc::new(log))
        .await
        .unwrap()
        .state()
        .await;
    for address in addresses {
        assert!(replayed.claim_at(&address).is_some(), "{address}");
    }
    store.stop().await;
}

/// **One tool call to a running server**, answered as the client received it. An
/// answer that is an error fails the case.
async fn call(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    name: &'static str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    use rmcp::model::CallToolRequestParams;
    let result = peer
        .call_tool(
            CallToolRequestParams::new(name)
                .with_arguments(arguments.as_object().expect("an object").clone()),
        )
        .await
        .unwrap_or_else(|e| panic!("{name} failed: {e}"));
    let text = serde_json::to_value(&result).expect("a result serializes");
    assert!(text["isError"] != true, "{name} was an error: {text}");
    text
}

/// **The JSON a tool answered with**, from the text of its first content block.
fn answer_of(result: &serde_json::Value) -> serde_json::Value {
    result["content"][0]["text"]
        .as_str()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .unwrap_or_default()
}

/// **A write**, answered. A name that resembles a known one comes back blocked with
/// a token that lets the same call through, which a bulk writer takes. A refused
/// write is a successful call, so the answer is read: a write that did not land
/// would leave the store smaller than the case says. Returns the answer of the call
/// that wrote.
async fn writing(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    name: &'static str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let first = answer_of(&call(peer, name, arguments.clone()).await);
    assert!(
        first["wrote"] != false || first["status"] == "blocked",
        "{name} wrote nothing: {first}"
    );
    if first["status"] != "blocked" {
        return first;
    }
    let token = first["override_token"]
        .as_str()
        .unwrap_or_else(|| panic!("{name} was blocked with no token: {first}"))
        .to_string();
    let mut again = arguments;
    again["override_token"] = token.into();
    let second = answer_of(&call(peer, name, again).await);
    assert!(
        second["status"] != "blocked",
        "{name} was blocked after its token: {second}"
    );
    second
}

/// **Write the rows of the grown store through the server's own front door.** The
/// recorded store holds a few dozen things and a handful of messages; the fold
/// fill and the search index cost what the instance holds, so a boot measured
/// over that store says little. Each thing is written by `add_entity` and given
/// one claim with [`SETS_PER_CLAIM`] keys by `capture`; [`TOOL_MESSAGES`] messages
/// are posted by `post_message` and the sessions started by `start_here`, so
/// every table gets rows the server really writes. [`scale_to_production`] then
/// copies them up to the instance's counts. Only the recorder runs this, and CI
/// caches the result. Nothing here is committed (rule 661), and [`prepare`]
/// refuses a store smaller than [`PRODUCTION_THINGS`] and
/// [`PRODUCTION_MESSAGES`].
async fn grow(port: u16) {
    use rmcp::ServiceExt;
    use rmcp::model::{ClientCapabilities, ClientInfo, Implementation};
    use rmcp::transport::StreamableHttpClientTransport;

    let client = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("boot-measures-grow", "0.0.1"),
    )
    .serve(StreamableHttpClientTransport::from_uri(format!(
        "http://127.0.0.1:{port}/mcp"
    )))
    .await
    .expect("the client connects to the server");
    let started = Instant::now();
    let peer = client.peer().clone();

    // **The recorded store may hold a run of this bot**, and a boot then offers
    // resume-or-new and hands back no sid until it is answered.
    let mut sid = None;
    let mut answered = String::new();
    for arguments in [
        serde_json::json!({ "bot": "assistant", "brief": true }),
        serde_json::json!({ "bot": "assistant", "brief": true, "resume": "new" }),
    ] {
        let booted = call(&peer, "start_here", arguments).await;
        let text = booted["content"][0]["text"].as_str().unwrap_or_default();
        answered = text.chars().take(2_000).collect();
        sid = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|json| json["session"]["sid"].as_str().map(str::to_string));
        if sid.is_some() {
            break;
        }
    }
    let sid = sid.unwrap_or_else(|| panic!("the default identity boots; it answered: {answered}"));

    let mut shards = tokio::task::JoinSet::new();
    for shard in 0..GROW_CLIENTS {
        let (peer, sid) = (peer.clone(), sid.clone());
        shards.spawn(async move {
            for n in (shard..PRODUCTION_THINGS).step_by(GROW_CLIENTS as usize) {
                let word = scrambled(n);
                let handle = format!("load-{word}");
                writing(
                    &peer,
                    "add_entity",
                    serde_json::json!({
                        "kind": "thing", "handle": handle, "name": format!("Load {word}"),
                        "source": "user-named", "sid": sid,
                    }),
                )
                .await;
                let sets: serde_json::Map<String, serde_json::Value> = (0..SETS_PER_CLAIM)
                    .map(|key| {
                        (
                            format!("load_key_{key}"),
                            format!("value {key} of {word}").into(),
                        )
                    })
                    .collect();
                writing(
                    &peer,
                    "capture",
                    serde_json::json!({
                        "subject": format!("thing:{handle}"),
                        "content": format!(
                            "Load {word} sits on shelf {} of aisle {}", n % 97, n % 31
                        ),
                        "sets": sets,
                        "provenance": "testimony", "recorded_at": "2026-07-01", "sid": sid,
                    }),
                )
                .await;
                if shard == 0 && n % 400 == 0 {
                    record(&format!(
                        "grow: things at {n} after {} s",
                        started.elapsed().as_secs()
                    ));
                }
            }
            for n in (shard..TOOL_MESSAGES).step_by(GROW_CLIENTS as usize) {
                writing(
                    &peer,
                    "post_message",
                    serde_json::json!({
                        "to": "assistant",
                        "subject": format!("load message {n}"),
                        "body": format!(
                            "Load message {n} about shelf {} and aisle {}", n % 97, n % 31
                        ),
                        "sid": sid,
                    }),
                )
                .await;
                if shard == 0 && n % 1600 == 0 {
                    record(&format!(
                        "grow: messages at {n} after {} s",
                        started.elapsed().as_secs()
                    ));
                }
            }
        });
    }
    while let Some(done) = shards.join_next().await {
        done.expect("a grow client finished");
    }
    // **Sessions**, one at a time: `new` starts a run of the default identity and
    // a wrap ends it with its story, as a real session does, so the next one
    // starts. The first run left alone allowed seven; the cause of the stop is
    // unknown, so the first answer without a sid ends the loop and its words go to
    // the measure log.
    let mut sessions = 0;
    for _ in 0..PRODUCTION_SESSIONS {
        let booted = call(
            &peer,
            "start_here",
            serde_json::json!({ "bot": "assistant", "brief": true, "resume": "new" }),
        )
        .await;
        let text = booted["content"][0]["text"].as_str().unwrap_or_default();
        let started_sid = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|json| json["session"]["sid"].as_str().map(str::to_string));
        let Some(started_sid) = started_sid else {
            record(&format!(
                "grow: sessions stopped after {sessions}; start_here answered: {}",
                text.chars().take(1_500).collect::<String>()
            ));
            break;
        };
        call(
            &peer,
            "wrap_session",
            serde_json::json!({ "sid": started_sid, "story": "A load session: started to give the store a run, and wrapped at once." }),
        )
        .await;
        sessions += 1;
    }
    record(&format!(
        "grow: {PRODUCTION_THINGS} things, {TOOL_MESSAGES} messages and {sessions} sessions written by {GROW_CLIENTS} clients in {} s",
        started.elapsed().as_secs()
    ));
    client.cancel().await.expect("the client closes");
}

/// **A name that resembles no other**, so the server's near-miss screen on names
/// lets each written thing through: ten letters from a mix of the number.
fn scrambled(n: i64) -> String {
    let mut x = (n as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (0..10)
        .map(|i| char::from(b'a' + ((x >> (i * 5)) % 26) as u8))
        .collect()
}

/// What the store held when a case was measured.
struct Size {
    things: i64,
    messages: i64,
    events: u64,
}

fn batch(id: &str, head: Position, first: u64, count: u64) -> SealedBatch {
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
    SealedBatch::seal(BatchId(id.into()), head, at, 1, members).unwrap()
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
    /// A snapshot of the log as it was before the tail, so the tail alone is
    /// uncovered whatever the recorded log held.
    Behind,
}

/// Append `events` events to the log of the store in `state`, then leave the
/// snapshot as `plan` says. Returns what the store held.
async fn prepare(state: &Scratch, plan: Plan, events: u64) -> Size {
    let mut store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("the store comes up with no binary over it");
    let pool = store.pool().clone();
    let writer = Writer::default();
    // **The log of the grown store holds the claims the boot imported**, and the
    // snapshot replay decodes a declared type that points at a kind only in a
    // process that holds the kind set. The binary loads it at boot; this process
    // loads the 25 shipped kinds here. A kind the store declares itself would
    // need `kinds::seed` over the store, which this case does not do.
    jojobot_domain::memory::kinds::load_shipped();
    let log = WriterLog::open(pool.clone(), writer.clone())
        .await
        .expect("the log opens");
    // **The binary's first boot and stop left a snapshot of the restored log**,
    // so the case starts from none unless it takes one.
    sqlx::query("DELETE FROM event_log WHERE slot LIKE 'snapshot%'")
        .execute(&pool)
        .await
        .expect("the snapshot slot is cleared");
    if matches!(plan, Plan::Behind) {
        take_snapshot(&log, jiff::Timestamp::now())
            .await
            .expect("the baseline snapshot");
    }
    let half = (events / PER_BATCH) / 2;
    let mut head = log.head().expect("the log holds a head");
    for n in 0..events / PER_BATCH {
        let sealed = batch(
            &format!("measure-{n}"),
            head,
            1_000_000 + n * PER_BATCH,
            PER_BATCH,
        );
        log.append(head, &sealed).await.expect("the batch lands");
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
    let count = |table: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM {table}"
            )))
            .fetch_one(&pool)
            .await
            .expect("a count")
        }
    };
    let size = Size {
        things: count("entity").await,
        messages: count("message").await,
        events: head.0,
    };
    assert!(
        size.things >= PRODUCTION_THINGS && size.messages >= PRODUCTION_MESSAGES,
        "the measured store is smaller than production: {} things, {} messages",
        size.things,
        size.messages
    );
    drop(log);
    writer.drain().await;
    drop(writer);
    store.stop().await;
    size
}

/// **Where the numbers are kept.** A passing test's output is captured and the
/// job log shows none of it, so each measured boot appends to a file under
/// `target/bar`, which CI uploads with the run's other logs.
fn record(text: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bar");
    std::fs::create_dir_all(&dir).expect("target/bar");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("boot-measures.log"))
        .expect("the measures file");
    writeln!(file, "{text}").expect("the measures file takes the line");
}

/// **A copy of a stopped store**, made before anything boots it, so the same boot
/// can be run over the same bytes again.
fn copy_of(state: &Scratch, name: &str) -> Scratch {
    let dir = jojobot_adapters::testing::scratch_path(name);
    let _ = std::fs::remove_dir_all(&dir);
    let copied = Command::new("cp")
        .arg("-a")
        .arg(&state.0)
        .arg(&dir)
        .status()
        .expect("the copy runs");
    assert!(copied.success(), "the store copies");
    Scratch(dir)
}

/// **The boot of `run`, then the same boot again over `copy`**, both recorded,
/// before either is judged. The first starts as soon as the template was copied;
/// the second starts once the first has answered, while the first process is still
/// resident (the caller stops it), on a copy made before the first, so the files
/// the second reads are the bytes the first read, and the page cache holds them. **Both
/// boots are reported against the 20 s** (wall and cpu), and **the memory bound is
/// asserted on both**. Neither boot fails on time: the second is not faster than the
/// first on the shared runner, so no boot stands for a restart on the same host.
fn measured_twice(case: &str, size: &Size, run: &mut Running, copy: Scratch) -> Duration {
    let first = measure(case, size, run);
    let mut again = spawn(copy);
    let second = measure(&format!("{case} second boot"), size, &mut again);
    let _ = again.stopped();
    record(&format!(
        "boot-pair {case}: first {} ms (cpu {} ms), second {} ms (cpu {} ms); peak first {} kB, second {} kB",
        first.took.as_millis(),
        first.cpu_ms,
        second.took.as_millis(),
        second.cpu_ms,
        first.peak,
        second.peak
    ));
    for (label, boot) in [("first", &first), ("second", &second)] {
        let verdict = |ms: u128| {
            if ms < BOOT_BOUND.as_millis() {
                "within"
            } else {
                "OVER"
            }
        };
        record(&format!(
            "boot-bar {case} {label}: wall {} ms {} {} ms; cpu {} ms {} {} ms",
            boot.took.as_millis(),
            verdict(boot.took.as_millis()),
            BOOT_BOUND.as_millis(),
            boot.cpu_ms,
            verdict(u128::from(boot.cpu_ms)),
            BOOT_BOUND.as_millis()
        ));
    }
    assert_memory(case, &first);
    assert_memory(&format!("{case} second boot"), &second);
    second.took
}

#[derive(Clone, Copy)]
struct Boot {
    took: Duration,
    cpu_ms: u64,
    peak: u64,
    server: u64,
    store: u64,
}

/// Only memory fails a case. Wall time and cpu time are reported against
/// `BOOT_BOUND` on every run and never fail it: wall time moves with the runner.
fn assert_memory(case: &str, boot: &Boot) {
    let Boot {
        took: _,
        cpu_ms: _,
        peak,
        server,
        store,
    } = *boot;
    let bound = memory_bound_kib();
    assert!(
        peak < bound,
        "{case}: peak memory was {peak} kB (server {server} + store {store}), over \
         {bound} kB (the module's memoryMax)"
    );
}

/// One measured boot: wait for its first answer, read its peak memory, and record
/// the numbers with the steps the server timed. It asserts nothing.
fn measure(case: &str, size: &Size, run: &mut Running) -> Boot {
    let took = run.first_answer();
    let (server, store) = run.peaks_kib();
    let cpu = run.cpu_ms();
    let peak = server + store;
    // The boot steps the server timed itself: the snapshot load and replay, the
    // fold fill, the search index and the rest.
    let steps: Vec<&str> = run
        .seen
        .iter()
        .filter(|line| line.contains("elapsed_ms") || line.contains("event log boot"))
        .map(String::as_str)
        .collect();
    // **Who owns the bytes**: the server's process and the store it started.
    let share = |part: u64| part * 100 / peak.max(1);
    let (owner, owned) = if server >= store {
        ("the server", server)
    } else {
        ("the store", store)
    };
    record(&format!(
        "boot-measure {case}: things={} messages={} log_events={} \
         start_to_first_answer_ms={} cpu_ms={cpu} peak_rss_kib={peak} (server {server} + store {store}); \
         {owner} owns {} per cent of the peak, {owned} kB (server {} per cent, store {} per cent) \
         against a bound of {} kB\n  {}",
        size.things,
        size.messages,
        size.events,
        took.as_millis(),
        share(owned),
        share(server),
        share(store),
        memory_bound_kib(),
        steps.join("\n  ")
    ));
    Boot {
        took,
        cpu_ms: cpu,
        peak,
        server,
        store,
    }
}

async fn case(name: &str, plan: Plan, from_snapshot: bool) {
    let state = restored_store(&format!("jojobot-measure-{name}")).await;
    let size = prepare(&state, plan, EVENTS).await;
    let copy = copy_of(&state, &format!("jojobot-measure-{name}-again"));
    let mut run = spawn(state);
    measured_twice(name, &size, &mut run, copy);
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
    let state = restored_store("jojobot-measure-crash").await;
    let size = prepare(&state, Plan::None, EVENTS).await;
    let mut first = spawn(state);
    first.first_answer();
    let (state, seen) = first.stopped();
    // The tick can publish it just before the stop does (the tail is past the
    // cadence); either way the log holds a snapshot once the process has exited.
    assert!(
        seen.iter().any(|line| line.contains("snapshot taken")),
        "the first process took no snapshot. What it said:\n{}",
        seen.join("\n")
    );
    let mut second = spawn(state);
    second.first_answer();
    assert!(second.log_boot().contains("from_snapshot=true"));
    let state = second.killed();
    let copy = copy_of(&state, "jojobot-measure-crash-again");
    let mut third = spawn(state);
    measured_twice("crash", &size, &mut third, copy);
    assert!(
        third.log_boot().contains("from_snapshot=true"),
        "{}",
        third.log_boot()
    );
    let _ = third.stopped();
}

/// **An orderly stop publishes a snapshot the next boot loads, and the stop is
/// what published it.** The tail is far under the cadence and the process runs
/// for seconds, so no tick finds a snapshot due: the only moment that can publish
/// it is the stop, and the log line says that is the one that did.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_orderly_stop_publishes_a_snapshot_the_next_boot_loads() {
    let state = restored_store("jojobot-measure-stop").await;
    let size = prepare(&state, Plan::Behind, SMALL_TAIL).await;
    let mut first = spawn(state);
    first.first_answer();
    // The snapshot is of the log before the tail, so exactly the tail is replayed
    // and exactly the tail is uncovered, whatever size the recorded log has.
    assert!(
        first.log_boot().contains("from_snapshot=true"),
        "the case starts from the baseline snapshot: {}",
        first.log_boot()
    );
    assert_eq!(
        first.boot_field("tail_batches"),
        SMALL_TAIL / PER_BATCH,
        "{}",
        first.log_boot()
    );
    let (state, seen) = first.stopped();
    let covered = seen
        .iter()
        .find(|line| line.contains("snapshot taken at shutdown"))
        .unwrap_or_else(|| {
            panic!(
                "the stop took no snapshot. What it said:\n{}",
                seen.join("\n")
            )
        })
        .split_whitespace()
        .find_map(|field| {
            field
                .strip_prefix("covered=")
                .and_then(|n| n.parse::<u64>().ok())
        })
        .expect("the shutdown snapshot names its covered position");
    // Release notes can append after the first answer; the final snapshot must
    // cover those too, rather than pinning the head from before the boot.
    assert!(covered >= size.events, "the prepared tail is covered");
    assert!(
        !seen
            .iter()
            .any(|line| line.contains("snapshot taken covered")),
        "a tick published it before the stop"
    );
    let copy = copy_of(&state, "jojobot-measure-stop-again");
    let mut second = spawn(state);
    measured_twice("stop", &size, &mut second, copy);
    // The stop's snapshot covers the head: loaded, with nothing left to replay.
    assert!(
        second.log_boot().contains("from_snapshot=true"),
        "{}",
        second.log_boot()
    );
    assert_eq!(
        second.boot_field("tail_batches"),
        0,
        "{}",
        second.log_boot()
    );
    assert_eq!(
        second.boot_field("restored_at"),
        second.boot_field("head"),
        "{}",
        second.log_boot()
    );
    assert_eq!(second.boot_field("head"), covered, "{}", second.log_boot());
    let _ = second.stopped();
}

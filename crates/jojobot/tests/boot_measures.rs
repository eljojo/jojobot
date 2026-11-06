//! **What a boot costs over a log with a tail, in each state its snapshot can be
//! in, and that an orderly stop leaves a snapshot the next boot loads.** Each case
//! spawns the real binary over a store it prepared, times process start to the
//! first answer of `/healthz`, reads the peak resident memory of the server and its store, and
//! prints one line. The release promises a boot under [`BOOT_BOUND`] and a process
//! under [`MEMORY_BOUND_KIB`] (`MemoryMax=1G`); a case fails with the measured
//! number when it is over.
//!
//! The store is the one the last deployed build recorded (`UPGRADE_FIXTURE_DIR`,
//! as for the upgrade gate), booted once by the binary under test, with [`EVENTS`]
//! teaching events appended to its log beside their legacy rows, so a boot that
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
/// The instance's size the release is measured at: its entity count today, and
/// the floor of its message count until the instance's own is read.
const PRODUCTION_THINGS: i64 = 3_144;
const PRODUCTION_MESSAGES: i64 = 11_852;
/// Facts written under each grown thing. **A stand-in until the live instance's
/// own count is known**: the number of facts per thing there is read from the
/// instance and replaces this.
const FACTS_PER_THING: i64 = 3;
/// Connections that write the grown store at once.
const GROW_CLIENTS: i64 = 8;
/// A tail under the snapshot cadence of 10,000 events, for the case that needs the
/// stop to be the only moment a snapshot is due.
const SMALL_TAIL: u64 = 1_000;
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

    /// **Peak resident memory of the server and the store it started**, from the
    /// kernel's own account, as `(server, store)`. `MemoryMax=1G` covers the
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
/// once by the binary under test, so its migrations and imports are done before
/// anything is measured. `source` names the variable that holds its directory:
/// the cases read the grown store CI cached (`BOOT_STORE_DIR`), and the recorder
/// that makes that store starts from the one the last deployed build recorded
/// (`UPGRADE_FIXTURE_DIR`, as for the upgrade gate) and grows it when `growing`.
/// The log's tail is added on top by [`prepare`].
async fn restore_into(name: &str, source: &str, growing: bool) -> Scratch {
    let fixture = std::env::var(source)
        .unwrap_or_else(|_| panic!("CI supplies {source}, the store this case starts from"));
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
    let mut first = spawn(state);
    first.first_answer();
    if growing {
        grow(first.http_port).await;
    }
    first.stopped().0
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
    std::fs::create_dir_all(&out).expect("the output directory");
    std::fs::copy(database.join("doltdump.sql"), out.join("doltdump.sql"))
        .expect("the dump is copied");
    std::fs::write(
        out.join("grown.txt"),
        format!(
            "things={PRODUCTION_THINGS} messages={PRODUCTION_MESSAGES} facts_per_thing={FACTS_PER_THING}\n"
        ),
    )
    .expect("the record of what was grown");
}

/// **Bring the store to production's size through the server's own front door.**
/// The recorded store holds a few dozen things and a handful of messages; the
/// fold fill and the search index cost what the instance holds, so a boot
/// measured over that store says little. Each thing is written by `add_entity`
/// and given [`FACTS_PER_THING`] facts by `capture`, and each message by
/// `post_message`, so every table gets the rows the server really writes. This
/// takes about half an hour, which is why only the recorder runs it and CI caches
/// the result. Nothing here is committed (rule 661), and [`prepare`] refuses a
/// store smaller than [`PRODUCTION_THINGS`] and [`PRODUCTION_MESSAGES`].
async fn grow(port: u16) {
    use rmcp::ServiceExt;
    use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
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

    async fn call(
        peer: &rmcp::Peer<rmcp::RoleClient>,
        name: &'static str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
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
    // A name that resembles a known one comes back blocked with a token that
    // lets the same call through, which a bulk writer takes.
    async fn writing(
        peer: &rmcp::Peer<rmcp::RoleClient>,
        name: &'static str,
        arguments: serde_json::Value,
    ) {
        // **A refused write is a successful call**, so the answer is read: a write
        // that did not land would leave the store smaller than the case says.
        let answer = |result: &serde_json::Value| {
            result["content"][0]["text"]
                .as_str()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                .unwrap_or_default()
        };
        let first = answer(&call(peer, name, arguments.clone()).await);
        assert!(
            first["wrote"] != false || first["status"] == "blocked",
            "{name} wrote nothing: {first}"
        );
        if first["status"] != "blocked" {
            return;
        }
        let token = first["override_token"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} was blocked with no token: {first}"))
            .to_string();
        let mut again = arguments;
        again["override_token"] = token.into();
        let second = answer(&call(peer, name, again).await);
        assert!(
            second["status"] != "blocked",
            "{name} was blocked after its token: {second}"
        );
    }

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
                for fact in 0..FACTS_PER_THING {
                    writing(
                        &peer,
                        "capture",
                        serde_json::json!({
                            "subject": format!("thing:{handle}"),
                            "content": format!(
                                "Load {word} holds item {fact} on shelf {} of aisle {}",
                                n % 97,
                                n % 31
                            ),
                            "provenance": "testimony", "recorded_at": "2026-07-01", "sid": sid,
                        }),
                    )
                    .await;
                }
                if shard == 0 && n % 400 == 0 {
                    record(&format!(
                        "grow: things at {n} after {} s",
                        started.elapsed().as_secs()
                    ));
                }
            }
            for n in (shard..PRODUCTION_MESSAGES).step_by(GROW_CLIENTS as usize) {
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
    record(&format!(
        "grow: {PRODUCTION_THINGS} things and {PRODUCTION_MESSAGES} messages written by {GROW_CLIENTS} clients in {} s",
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
    let log = WriterLog::open(pool.clone(), writer.clone())
        .await
        .expect("the log opens");
    // **The binary's first boot and stop left a snapshot of the restored log**,
    // so the case starts from none unless it takes one.
    sqlx::query("DELETE FROM event_log WHERE slot = 'snapshot'")
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
        let (sealed, rows) = batch(
            &format!("measure-{n}"),
            head,
            1_000_000 + n * PER_BATCH,
            PER_BATCH,
        );
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

/// One measured boot: record its numbers and assert the bounds.
fn measured(case: &str, size: &Size, run: &mut Running) -> Duration {
    let took = run.first_answer();
    let (server, store) = run.peaks_kib();
    let peak = server + store;
    // The boot steps the server timed itself: the snapshot load and replay, the
    // fold fill, the search index and the rest.
    let steps: Vec<&str> = run
        .seen
        .iter()
        .filter(|line| line.contains("elapsed_ms") || line.contains("event log boot"))
        .map(String::as_str)
        .collect();
    record(&format!(
        "boot-measure {case}: things={} messages={} log_events={} \
         start_to_first_answer_ms={} peak_rss_kib={peak} (server {server} + store {store})\n  {}",
        size.things,
        size.messages,
        size.events,
        took.as_millis(),
        steps.join("\n  ")
    ));
    assert!(
        took < BOOT_BOUND,
        "{case}: the first answer took {took:?}, over {BOOT_BOUND:?}"
    );
    assert!(
        peak < MEMORY_BOUND_KIB,
        "{case}: peak memory was {peak} kB (server {server} + store {store}), over \
         {MEMORY_BOUND_KIB} kB (MemoryMax=1G)"
    );
    took
}

async fn case(name: &str, plan: Plan, from_snapshot: bool) {
    let state = restored_store(&format!("jojobot-measure-{name}")).await;
    let size = prepare(&state, plan, EVENTS).await;
    let mut run = spawn(state);
    measured(name, &size, &mut run);
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
    let mut third = spawn(state);
    measured("crash", &size, &mut third);
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
    assert!(
        seen.iter()
            .any(|line| line.contains("snapshot taken at shutdown")),
        "the stop took no snapshot. What it said:\n{}",
        seen.join("\n")
    );
    assert!(
        !seen
            .iter()
            .any(|line| line.contains("snapshot taken covered")),
        "a tick published it before the stop"
    );
    let mut second = spawn(state);
    measured("stop", &size, &mut second);
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
    assert_eq!(
        second.boot_field("head"),
        size.events,
        "{}",
        second.log_boot()
    );
    let _ = second.stopped();
}

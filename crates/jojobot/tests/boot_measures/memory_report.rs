//! **The memory report: how much of the machine the server's own process holds,
//! through a first boot over the production-sized store and then a replayed day of
//! calls** (rule 800's ship proof). It reads the server's resident memory, never
//! the unit's cgroup, which carries the store as well.
//!
//! **Two numbers, both the server alone.** The first is the peak of the process
//! through its first boot: the fold fills, the search indexes, the claims import
//! and the first snapshot. The second is its resident memory after a replayed day.
//! **Beside them, what each projection cost**, read as how much the resident set
//! grew between the boot line that ends one projection and the line that ends the
//! last. That is growth of the process, and it is not the bytes the projection
//! owns: the allocator keeps what a step freed, so a step that built and dropped a
//! large buffer shows it. Each is divided by the thing it holds (claims, runs,
//! messages) so a year of the operator's data can be projected from it.
//!
//! **Nothing here fails on a number.** The report says where it stands against the
//! unit's `memoryMax` and the case asserts only that the instrument saw what it
//! reads: every boot line and every call of the day.

use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use super::{
    Dolt, PRODUCTION_THINGS, Running, Scratch, call, count_of, free_port, record, replay_dump,
    scrambled, spawn, writing,
};

/// One reading of a process's memory, in kB, from `/proc/<pid>/status`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Memory {
    /// What the process holds in memory now (`VmRSS`).
    resident: u64,
    /// The part of it no file backs (`RssAnon`): the heap and the stacks.
    anonymous: u64,
    /// The most it has held (`VmHWM`).
    peak: u64,
}

/// The memory a `/proc/<pid>/status` text states. A field the text lacks reads 0.
fn memory_of(status: &str) -> Memory {
    let field = |name: &str| -> u64 {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
            .unwrap_or(0)
    };
    Memory {
        resident: field("VmRSS:"),
        anonymous: field("RssAnon:"),
        peak: field("VmHWM:"),
    }
}

/// The memory of one process now, or `None` when it has gone.
fn memory_of_pid(pid: u32) -> Option<Memory> {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .map(|status| memory_of(&status))
}

/// Every process below `pid`, the store the server started among them.
fn descendants_of(pid: u32) -> Vec<u32> {
    let mut parents = std::collections::HashMap::new();
    for entry in std::fs::read_dir("/proc").expect("/proc").flatten() {
        let Some(child) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        // A process can exit between the listing and the read.
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let parent = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|rest| rest.trim().parse::<u32>().ok());
        if let Some(parent) = parent {
            parents.insert(child, parent);
        }
    }
    let mut below = std::collections::HashSet::from([pid]);
    loop {
        let found: Vec<u32> = parents
            .iter()
            .filter(|(child, parent)| below.contains(parent) && !below.contains(child))
            .map(|(child, _)| *child)
            .collect();
        if found.is_empty() {
            break;
        }
        below.extend(found);
    }
    below.remove(&pid);
    below.into_iter().collect()
}

/// **The projections the boot builds, each named by the line that ends it.** The
/// server logs one line when a fill finishes, so the line says when to read the
/// process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Projection {
    /// The fold's full read of memory: every thing, claim and history.
    MemoryImage,
    /// The search index over that memory.
    MemorySearch,
    /// The image of every run.
    RunImage,
    /// The image of every box and message.
    MailImage,
    /// The search index over mail.
    MailSearch,
    /// The search index over runs.
    RunSearch,
    /// The teaching ledger, opened over the event log.
    Teaching,
    /// Today's claims, fields and identities, put into the event log and the claims
    /// State at the first boot.
    ClaimsImport,
    /// The snapshot the boot takes once the import has landed.
    FirstSnapshot,
}

impl Projection {
    const ALL: [Projection; 9] = [
        Projection::MemoryImage,
        Projection::MemorySearch,
        Projection::RunImage,
        Projection::MailImage,
        Projection::MailSearch,
        Projection::RunSearch,
        Projection::Teaching,
        Projection::ClaimsImport,
        Projection::FirstSnapshot,
    ];

    fn label(self) -> &'static str {
        match self {
            // The first reading is the process at its spawn, so this step holds all
            // that the boot does before the fold's fill ends: the start-up, the store
            // coming up and the repairs of stored rows, with the fill itself.
            Projection::MemoryImage => "the start-up and the image of memory",
            Projection::MemorySearch => "the search index over memory",
            Projection::RunImage => "the image of runs",
            Projection::MailImage => "the image of mail",
            Projection::MailSearch => "the search index over mail",
            Projection::RunSearch => "the search index over runs",
            Projection::Teaching => "the teaching ledger",
            Projection::ClaimsImport => "the claims import and its State",
            Projection::FirstSnapshot => "the first snapshot",
        }
    }
}

/// **The words each ending line holds**, as the server logs them
/// (`crates/jojobot/src/wiring.rs` and the snapshot keeper). A changed line stops the
/// report here, because the case asserts every projection was seen.
const ENDINGS: [(&str, Projection); 9] = [
    (
        "fold: everything the memory verbs read is held in memory",
        Projection::MemoryImage,
    ),
    (
        "search: index built from a full scan",
        Projection::MemorySearch,
    ),
    ("fold: every run is held in memory", Projection::RunImage),
    (
        "fold: every box and message is held in memory",
        Projection::MailImage,
    ),
    (
        "search: mail indexed from a full board read",
        Projection::MailSearch,
    ),
    (
        "search: sessions indexed from a full read",
        Projection::RunSearch,
    ),
    (
        "teaching ledger: open over the event log",
        Projection::Teaching,
    ),
    (
        "imported today's claims into the event log",
        Projection::ClaimsImport,
    ),
    ("snapshot taken after import", Projection::FirstSnapshot),
];

/// The projection a server log line says it has finished building, if it does.
fn projection_ended_by(line: &str) -> Option<Projection> {
    ENDINGS
        .iter()
        .find(|(words, _)| line.contains(words))
        .map(|(_, projection)| *projection)
}

/// One look at the server: when, what it held, and the projection the line just
/// read from its log ended, if one did.
#[derive(Clone, Copy, Debug)]
struct Reading {
    at: Duration,
    memory: Memory,
    ended: Option<Projection>,
}

/// How much the resident set grew across one projection, in kB, when the line that
/// ended it arrived, and what the process held after.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Growth {
    projection: Projection,
    kib: i64,
    resident_after: u64,
    at: Duration,
}

/// The growth of each projection, in the order its line arrived: its reading minus
/// the reading before it, the first reading of all standing for the start.
fn growth(readings: &[Reading]) -> Vec<Growth> {
    let mut before = readings
        .first()
        .map_or(0, |reading| reading.memory.resident);
    let mut steps = Vec::new();
    for reading in readings {
        if let Some(projection) = reading.ended {
            steps.push(Growth {
                projection,
                kib: reading.memory.resident as i64 - before as i64,
                resident_after: reading.memory.resident,
                at: reading.at,
            });
            before = reading.memory.resident;
        }
    }
    steps
}

/// One kind of call in a day of the live instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Call {
    Recall,
    Capture,
    UpdateFact,
    /// A message posted and left.
    Post,
    /// A message posted, read and marked processed.
    PostAndClose,
    /// The count of what waits in a box.
    Mailbox,
    Search,
    /// A run started and wrapped.
    Session,
    AddEntity,
}

/// **A day of the live instance, by kind of call**, read from its own metrics on
/// 2026-11-10: the calls of the last seven days by tool (41,198 in all), a seventh
/// of each. A tool the replay makes in a different count is named: messages read
/// are fewer than posted in the live count (23 a day) and a closed message here
/// reads its message first; runs wrapped are fewer (46 a day) than started (167),
/// and here each run is wrapped. The rest of the live day (documents read, entities
/// updated, the journal, about 5 per cent) is not replayed. Numbers only.
const DAY: [(Call, usize); 9] = [
    (Call::Recall, 1_861),
    (Call::Capture, 948),
    (Call::UpdateFact, 382),
    (Call::Post, 516),
    (Call::PostAndClose, 356),
    (Call::Mailbox, 814),
    (Call::Search, 170),
    (Call::Session, 167),
    (Call::AddEntity, 86),
];

/// A number mixed from `n`, the same on every run.
fn mixed(n: u64) -> u64 {
    let mut x = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// The calls of one day, in an order that mixes the kinds the way a day does and is
/// the same on every run.
fn day() -> Vec<Call> {
    let mut keyed: Vec<(u64, Call)> = DAY
        .iter()
        .flat_map(|(call, count)| std::iter::repeat_n(*call, *count))
        .enumerate()
        .map(|(at, call)| (mixed(at as u64), call))
        .collect();
    keyed.sort_by_key(|(key, _)| *key);
    keyed.into_iter().map(|(_, call)| call).collect()
}

/// What the report needs of the log of a first boot.
struct Boot {
    readings: Vec<Reading>,
    /// The peak of the server and of the store it started, in kB.
    peak: (u64, u64),
}

/// **Read the server's memory through its boot**, at every line it logs and at
/// least every 50 ms, until it serves. The boot's lines are read here and kept in
/// `seen`, so the case that waits for the serving line would never see it: this
/// asks `/healthz` as `first_answer` does.
fn watch_boot(run: &mut Running) -> Boot {
    let started = Instant::now();
    let patience = jojobot_exercise::room::READY_TIMEOUT;
    let mut readings = vec![Reading {
        at: Duration::ZERO,
        memory: memory_of_pid(run.pid).unwrap_or_default(),
        ended: None,
    }];
    loop {
        let line = match run.lines.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => Some(line),
            Err(RecvTimeoutError::Timeout) if started.elapsed() < patience => None,
            Err(error) => panic!(
                "{error}: the server never served. What it said:\n{}",
                run.seen.join("\n")
            ),
        };
        let ended = line.as_deref().and_then(projection_ended_by);
        let serving = line
            .as_deref()
            .is_some_and(|line| line.contains("serving http://"));
        if let Some(memory) = memory_of_pid(run.pid) {
            readings.push(Reading {
                at: started.elapsed(),
                memory,
                ended,
            });
        }
        if let Some(line) = line {
            if serving {
                run.http_port = jojobot_adapters::testing::served_port(&line)
                    .expect("the serving line names a port");
            }
            run.seen.push(line);
        }
        if serving {
            break;
        }
    }
    use std::io::{Read, Write};
    let mut ask = std::net::TcpStream::connect(("127.0.0.1", run.http_port)).expect("connects");
    ask.write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .expect("the request");
    let mut answer = String::new();
    ask.read_to_string(&mut answer).expect("the answer");
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    run.boot_turn = None;
    Boot {
        readings,
        peak: run.peaks_kib(),
    }
}

/// The number after `name=` in a server log line, such as `runs=1560`.
fn field_of(lines: &[String], name: &str) -> Option<u64> {
    lines.iter().find_map(|line| {
        let from = line.find(&format!("{name}="))? + name.len() + 1;
        line[from..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()
    })
}

/// **A session of the default identity**, started as a first call does: the store
/// may hold a run of this bot, and a boot then offers resume-or-new and hands back
/// no sid until it is answered.
async fn a_session(peer: &rmcp::Peer<rmcp::RoleClient>) -> String {
    let mut answered = String::new();
    for arguments in [
        serde_json::json!({ "bot": "assistant", "brief": true }),
        serde_json::json!({ "bot": "assistant", "brief": true, "resume": "new" }),
    ] {
        let booted = call(peer, "start_here", arguments).await;
        let text = booted["content"][0]["text"].as_str().unwrap_or_default();
        answered = text.chars().take(2_000).collect();
        let sid = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|json| json["session"]["sid"].as_str().map(str::to_string));
        if let Some(sid) = sid {
            return sid;
        }
    }
    panic!("the default identity boots; it answered: {answered}");
}

/// **One call of the day**, made as a session makes it. `mine` holds the claims
/// this client wrote, so an edit has one to edit.
async fn make(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    sid: &str,
    kind: Call,
    n: usize,
    mine: &mut Vec<String>,
) {
    // **An entity the grown store holds**, whatever the call: the grown things are
    // `load-` and a scrambled word, one for each of the instance's things.
    let handle = format!("load-{}", scrambled((n as i64 * 7_919) % PRODUCTION_THINGS));
    let thing = format!("thing:{handle}");
    let shelf = n % 97;
    match kind {
        Call::Recall => {
            call(
                peer,
                "recall",
                serde_json::json!({ "subject": thing, "facts": true, "sid": sid }),
            )
            .await;
        }
        Call::Capture => {
            let wrote = writing(
                peer,
                "capture",
                serde_json::json!({
                    "subject": thing,
                    "content": format!("A day call {n} about shelf {shelf}"),
                    "sets": { "day_key": format!("value {n}") },
                    "provenance": "testimony", "recorded_at": "2026-11-10", "sid": sid,
                }),
            )
            .await;
            if let Some(address) = wrote["address"].as_str() {
                mine.push(address.to_string());
            }
        }
        Call::UpdateFact => match mine.pop() {
            Some(address) => {
                call(
                    peer,
                    "update_fact",
                    serde_json::json!({
                        "address": address,
                        "content": format!("A day call {n} about shelf {shelf}, edited"),
                        "sid": sid,
                    }),
                )
                .await;
            }
            None => {
                let wrote = writing(
                    peer,
                    "capture",
                    serde_json::json!({
                        "subject": thing,
                        "content": format!("A day call {n} about aisle {shelf}"),
                        "provenance": "testimony", "recorded_at": "2026-11-10", "sid": sid,
                    }),
                )
                .await;
                if let Some(address) = wrote["address"].as_str() {
                    mine.push(address.to_string());
                }
            }
        },
        Call::Post | Call::PostAndClose => {
            let posted = writing(
                peer,
                "post_message",
                serde_json::json!({
                    "to": "assistant",
                    "subject": format!("day message {n}"),
                    "body": format!("Day message {n} about shelf {shelf}"),
                    "sid": sid,
                }),
            )
            .await;
            if kind == Call::PostAndClose {
                let id = posted["id"].as_str().expect("a posted message has an id");
                call(
                    peer,
                    "read_message",
                    serde_json::json!({ "message_id": id, "sid": sid }),
                )
                .await;
                call(
                    peer,
                    "mark_processed",
                    serde_json::json!({ "message_id": id, "notes": "a day call", "sid": sid }),
                )
                .await;
            }
        }
        Call::Mailbox => {
            call(
                peer,
                "read_mailbox",
                serde_json::json!({ "counts_only": true, "sid": sid }),
            )
            .await;
        }
        Call::Search => {
            call(
                peer,
                "search",
                serde_json::json!({
                    "query": format!("shelf {shelf} aisle {}", n % 31), "sid": sid,
                }),
            )
            .await;
        }
        Call::Session => {
            let started = a_session(peer).await;
            call(
                peer,
                "wrap_session",
                serde_json::json!({ "sid": started, "story": "A day session, wrapped at once." }),
            )
            .await;
        }
        Call::AddEntity => {
            let word = scrambled(1_000_000 + n as i64);
            writing(
                peer,
                "add_entity",
                serde_json::json!({
                    "kind": "thing", "handle": format!("day-{word}"),
                    "name": format!("Day {word}"), "source": "user-named", "sid": sid,
                }),
            )
            .await;
        }
    }
}

/// Clients that make the day's calls at once. The grow step writes through eight.
const CLIENTS: usize = 8;

/// **How long the replay may run in all.** The lane's test binary is killed at 1,200
/// seconds, and every other case in it takes under six minutes; a day of calls
/// through the tools is slower than that (the first run of this case, with four
/// clients and no limit, was still on its first day when the binary was killed). The
/// replay makes as much of the days as the budget allows and reports how much.
const REPLAY_BUDGET: Duration = Duration::from_secs(420);

/// What a replay of one day made, and the server's memory while it did.
struct Replayed {
    /// `(elapsed, calls made, memory)`, about every half second.
    samples: Vec<(Duration, usize, Memory)>,
    /// The entries of the day it made before the budget ended it.
    made: usize,
}

/// **Replay one day through a running server**, for no longer than `budget`,
/// sampling the server's memory about every half second and writing a line of
/// progress about every ten, so a run that is stopped still leaves its numbers.
async fn replay_day(pid: u32, port: u16, first_call: usize, budget: Duration) -> Replayed {
    use rmcp::ServiceExt;
    use rmcp::model::{ClientCapabilities, ClientInfo, Implementation};
    use rmcp::transport::StreamableHttpClientTransport;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let client = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("boot-measures-memory-report", "0.0.1"),
    )
    .serve(StreamableHttpClientTransport::from_uri(format!(
        "http://127.0.0.1:{port}/mcp"
    )))
    .await
    .expect("the client connects to the server");
    let peer = client.peer().clone();
    let sid = a_session(&peer).await;
    // **The sessions of the day are started after the others**, one at a time as the
    // grow step starts its own: a run started for the same bot while four clients
    // call under the first run's key could end that run, and a call under an ended
    // run is refused.
    let (sessions, others): (Vec<Call>, Vec<Call>) =
        day().into_iter().partition(|call| *call == Call::Session);
    let calls = Arc::new(others);
    let made = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let samples = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sampler = {
        let (made, finished, samples) = (made.clone(), finished.clone(), samples.clone());
        tokio::spawn(async move {
            let started = Instant::now();
            let mut taken = 0_usize;
            while !finished.load(Ordering::SeqCst) {
                if let Some(memory) = memory_of_pid(pid) {
                    let calls_made = made.load(Ordering::SeqCst);
                    samples.lock().expect("the samples").push((
                        started.elapsed(),
                        calls_made,
                        memory,
                    ));
                    if taken.is_multiple_of(20) {
                        record(&format!(
                            "memory-report day-sample: {} s into the replay, {calls_made} entries \
                             made, server resident {}",
                            started.elapsed().as_secs(),
                            mib(memory.resident),
                        ));
                    }
                    taken += 1;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
    };
    let started = Instant::now();
    let mut shards = tokio::task::JoinSet::new();
    for shard in 0..CLIENTS {
        let (peer, sid, calls, made) = (peer.clone(), sid.clone(), calls.clone(), made.clone());
        shards.spawn(async move {
            let mut mine = Vec::new();
            for at in (shard..calls.len()).step_by(CLIENTS) {
                if started.elapsed() >= budget {
                    break;
                }
                make(&peer, &sid, calls[at], first_call + at, &mut mine).await;
                made.fetch_add(1, Ordering::SeqCst);
            }
        });
    }
    while let Some(done) = shards.join_next().await {
        done.expect("a client finished its calls");
    }
    let mut mine = Vec::new();
    for (at, call) in sessions.iter().enumerate() {
        if started.elapsed() >= budget {
            break;
        }
        make(&peer, &sid, *call, first_call + calls.len() + at, &mut mine).await;
        made.fetch_add(1, Ordering::SeqCst);
    }
    finished.store(true, Ordering::SeqCst);
    sampler.await.expect("the sampler stopped");
    client.cancel().await.expect("the client closes");
    let samples = samples.lock().expect("the samples").clone();
    Replayed {
        samples,
        made: made.load(Ordering::SeqCst),
    }
}

/// How many claims a stopped store holds, counted by a store of its own that the
/// count starts and stops.
async fn claims_in(state: &Scratch) -> i64 {
    let mut store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("the store comes up to be counted");
    let claims = count_of(store.pool(), "(SELECT COUNT(*) FROM fact)").await;
    store.stop().await;
    claims
}

/// How many tool calls one entry of the day makes, before any retry of a write the
/// server blocked.
fn tool_calls_of(call: Call) -> usize {
    match call {
        Call::PostAndClose => 3,
        Call::Session => 2,
        _ => 1,
    }
}

/// The tool calls of a day, which are more than its entries: a closed message is
/// posted, read and marked, and a session is started and wrapped.
fn day_tool_calls() -> usize {
    DAY.iter()
        .map(|(call, count)| tool_calls_of(*call) * count)
        .sum()
}

fn mib(kib: u64) -> String {
    format!("{:.1} MiB", kib as f64 / 1024.0)
}

/// **The report.** A first boot over the production-sized store, then days of calls
/// through the running server, each read as the server's own memory.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_servers_memory_across_a_first_boot_and_a_replayed_day() {
    let fixture = std::env::var("BOOT_STORE_DIR")
        .expect("CI supplies BOOT_STORE_DIR, the store this case starts from");
    // **A first boot, as production's is on a new build**: the dump the last deployed
    // build recorded and the case grew to the instance's counts, with no binary
    // booted over it yet, so the boot imports the claims and takes the first snapshot.
    let state = replay_dump("jojobot-memory-report", &fixture).await;
    // **The claims the boot reads are counted before it**, so a growth at boot is
    // divided by what the boot held and not by what the replay made of it.
    let claims = claims_in(&state).await;
    let mut run = spawn(state);
    let boot = watch_boot(&mut run);
    let (server_peak, store_peak) = boot.peak;
    let seen = run.seen.clone();
    let steps = growth(&boot.readings);
    let missing: Vec<&str> = Projection::ALL
        .iter()
        .filter(|projection| !steps.iter().any(|step| step.projection == **projection))
        .map(|projection| projection.label())
        .collect();
    assert!(
        missing.is_empty(),
        "the boot never said it had built: {missing:?}. What it said:\n{}",
        seen.join("\n")
    );
    let after_boot = memory_of_pid(run.pid).expect("the server runs");
    record(&format!(
        "memory-report boot: server peak {} ({server_peak} kB), store peak {} ({store_peak} kB), \
         server resident after boot {}, of which anonymous {}",
        mib(server_peak),
        mib(store_peak),
        mib(after_boot.resident),
        mib(after_boot.anonymous),
    ));
    let things = field_of(&seen, "things");
    let runs = field_of(&seen, "runs");
    let messages = field_of(&seen, "messages");
    let events = field_of(&seen, "events");
    for step in &steps {
        record(&format!(
            "memory-report step: {} ended at {} ms and grew the server by {} kB ({}{}), to {} \
             resident",
            step.projection.label(),
            step.at.as_millis(),
            step.kib,
            if step.kib < 0 { "-" } else { "" },
            mib(step.kib.unsigned_abs()),
            mib(step.resident_after),
        ));
    }

    // **A replay of up to three days**, in the time the budget allows, so the climb
    // of a process that keeps what it freed is a series and not one reading.
    let (pid, port) = (run.pid, run.http_port);
    let mut after_day = Vec::new();
    let mut first_call = 0;
    let replay_started = Instant::now();
    for day_number in 1..=3 {
        let left = REPLAY_BUDGET.saturating_sub(replay_started.elapsed());
        if left < Duration::from_secs(10) {
            break;
        }
        let replayed = replay_day(pid, port, first_call, left).await;
        first_call += replayed.made;
        let samples = replayed.samples;
        let last = samples.last().expect("a replay has samples").2;
        let top = samples.iter().map(|s| s.2.resident).max().unwrap_or(0);
        record(&format!(
            "memory-report day {day_number}: {} of the {} entries of a day made, which is {} of \
             its {} tool calls before any retry, in {} s; server resident {} at the end, {} at \
             the most; peak so far {}",
            replayed.made,
            day().len(),
            replayed.made * day_tool_calls() / day().len(),
            day_tool_calls(),
            samples.last().map_or(0, |s| s.0.as_secs()),
            mib(last.resident),
            mib(top),
            mib(run.peaks_kib().0),
        ));
        after_day.push(last);
    }
    assert!(
        first_call > 0,
        "the replay made no call in its budget of {REPLAY_BUDGET:?}"
    );
    let (server_peak_after, store_peak_after) = run.peaks_kib();
    let finally = *after_day.last().expect("the days ran");
    let store_resident: u64 = descendants_of(run.pid)
        .into_iter()
        .filter_map(memory_of_pid)
        .map(|memory| memory.resident)
        .sum();
    let (state, _) = run.stopped();

    // **What the store held after the replay**, counted after the stop.
    let claims_after = claims_in(&state).await;
    let held = |kind: Projection| {
        steps
            .iter()
            .find(|step| step.projection == kind)
            .map_or(0, |step| step.kib)
    };
    let per = |kib: i64, count: Option<u64>| match count {
        Some(count) if count > 0 => format!("{} bytes each", kib * 1024 / count as i64),
        _ => "no count".to_string(),
    };
    record(&format!(
        "memory-report per thing held at the first boot (claims {claims}, and {claims_after} \
         after the replay; things {things:?}, runs {runs:?}, messages {messages:?}, import \
         events {events:?}): the start-up and the image of memory {} per claim; memory search \
         {} per claim; run image {} per run; mail image {} per message; claims import {} per \
         claim",
        per(held(Projection::MemoryImage), Some(claims as u64)),
        per(held(Projection::MemorySearch), Some(claims as u64)),
        per(held(Projection::RunImage), runs),
        per(held(Projection::MailImage), messages),
        per(held(Projection::ClaimsImport), Some(claims as u64)),
    ));
    // **The unit's own bound**, read from the module as the boot cases read it.
    let bound = super::memory_bound_kib();
    let verdict = |kib: u64| {
        if kib < bound { "under" } else { "OVER" }
    };
    let beside = finally.resident + store_resident;
    record(&format!(
        "memory-report summary: the server's peak through the first boot {} ({} {}); its \
         peak after the replay {} ({} {}); its resident memory after them {} ({} \
         {}), anonymous {}; the store beside it {} resident and {} at its peak; the unit's \
         cgroup holds both, so {} resident against its memoryMax of {} ({})",
        mib(server_peak),
        verdict(server_peak),
        mib(bound),
        mib(server_peak_after),
        verdict(server_peak_after),
        mib(bound),
        mib(finally.resident),
        verdict(finally.resident),
        mib(bound),
        mib(finally.anonymous),
        mib(store_resident),
        mib(store_peak_after.max(store_peak)),
        mib(beside),
        mib(bound),
        verdict(beside),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A `/proc/<pid>/status` as the kernel writes it** (tabs after the colon, sizes
    /// in kB), the fields the report reads among the ones it does not.
    const STATUS: &str = "Name:\tjojobot\nUmask:\t0022\nState:\tS (sleeping)\nPid:\t4242\nPPid:\t1\n\
VmPeak:\t 3314144 kB\nVmSize:\t 3262704 kB\nVmLck:\t       0 kB\nVmPin:\t       0 kB\n\
VmHWM:\t 2951028 kB\nVmRSS:\t 2760236 kB\nRssAnon:\t 2700120 kB\nRssFile:\t   60116 kB\n\
RssShmem:\t       0 kB\nVmData:\t 2900000 kB\nThreads:\t24\n";

    #[test]
    fn memory_report_reads_the_resident_anonymous_and_peak_memory_of_a_status() {
        assert_eq!(
            memory_of(STATUS),
            Memory {
                resident: 2_760_236,
                anonymous: 2_700_120,
                peak: 2_951_028,
            },
            "VmRSS is not RssAnon, and VmHWM is the peak, not VmPeak"
        );
        assert_eq!(
            memory_of("Name:\tkthreadd\nPid:\t2\n"),
            Memory::default(),
            "a process with no memory fields reads zero"
        );
    }

    /// The lines the real server logs when each fill ends, as its own source words
    /// them (crates/jojobot/src/wiring.rs), behind the prefix the log writes.
    const LINES: [(&str, Option<Projection>); 11] = [
        (
            "2026-11-10T20:00:01.100Z  INFO jojobot::wiring: fold: everything the memory verbs \
             read is held in memory, from a full read things=3208 elapsed_ms=900",
            Some(Projection::MemoryImage),
        ),
        (
            "2026-11-10T20:00:02.100Z  INFO jojobot::wiring: search: index built from a full \
             scan docs=29100",
            Some(Projection::MemorySearch),
        ),
        (
            "2026-11-10T20:00:03.100Z  INFO jojobot::wiring: fold: every run is held in memory, \
             from a full read runs=1560 elapsed_ms=300",
            Some(Projection::RunImage),
        ),
        (
            "2026-11-10T20:00:04.100Z  INFO jojobot::wiring: fold: every box and message is held \
             in memory, from a full read messages=24134 elapsed_ms=400",
            Some(Projection::MailImage),
        ),
        (
            "2026-11-10T20:00:05.100Z  INFO jojobot::wiring: search: mail indexed from a full \
             board read messages=24134",
            Some(Projection::MailSearch),
        ),
        (
            "2026-11-10T20:00:06.100Z  INFO jojobot::wiring: search: sessions indexed from a full \
             read sessions=1560",
            Some(Projection::RunSearch),
        ),
        (
            "2026-11-10T20:00:07.100Z  INFO jojobot::wiring: teaching ledger: open over the event \
             log",
            Some(Projection::Teaching),
        ),
        (
            "2026-11-10T20:00:08.100Z  INFO jojobot::wiring: imported today's claims into the \
             event log events=190000",
            Some(Projection::ClaimsImport),
        ),
        (
            "2026-11-10T20:00:09.100Z  INFO jojobot_adapters::snapshot_keeper: snapshot taken \
             after import covered=190000",
            Some(Projection::FirstSnapshot),
        ),
        // A line that names a step and is no fill.
        (
            "2026-11-10T20:00:00.500Z  INFO jojobot::wiring: boot step finished \
             step=\"search-memory\" elapsed_ms=900",
            None,
        ),
        (
            "2026-11-10T20:00:10.100Z  INFO jojobot: serving http://127.0.0.1:41000",
            None,
        ),
    ];

    #[test]
    fn memory_report_names_the_projection_a_boot_line_ends() {
        for (line, ended) in LINES {
            assert_eq!(projection_ended_by(line), ended, "{line}");
        }
    }

    fn at(secs: u64, resident: u64, ended: Option<Projection>) -> Reading {
        Reading {
            at: Duration::from_secs(secs),
            memory: Memory {
                resident,
                anonymous: resident,
                peak: resident,
            },
            ended,
        }
    }

    #[test]
    fn memory_report_grows_each_projection_by_what_the_resident_set_gained_across_it() {
        let readings = [
            at(0, 100, None),
            at(1, 100, None),
            at(2, 400, Some(Projection::MemoryImage)),
            at(3, 380, None),
            // A projection after a step that gave memory back reads as a loss.
            at(4, 350, Some(Projection::MemorySearch)),
            at(5, 900, Some(Projection::MailImage)),
        ];
        assert_eq!(
            growth(&readings),
            [
                Growth {
                    projection: Projection::MemoryImage,
                    kib: 300,
                    resident_after: 400,
                    at: Duration::from_secs(2),
                },
                Growth {
                    projection: Projection::MemorySearch,
                    kib: -50,
                    resident_after: 350,
                    at: Duration::from_secs(4),
                },
                Growth {
                    projection: Projection::MailImage,
                    kib: 550,
                    resident_after: 900,
                    at: Duration::from_secs(5),
                },
            ]
        );
        assert_eq!(
            growth(&[at(0, 100, None), at(1, 150, None)]),
            [],
            "a boot that ended no projection grew none"
        );
    }

    #[test]
    fn memory_report_replays_each_kind_of_call_as_often_as_a_live_day_holds_it() {
        let calls = day();
        assert_eq!(calls.len(), DAY.iter().map(|(_, n)| n).sum::<usize>());
        for (kind, count) in DAY {
            assert_eq!(
                calls.iter().filter(|call| **call == kind).count(),
                count,
                "{kind:?}"
            );
        }
        // **A day is mixed, not a run of each kind**: the first hundred calls hold
        // most of the kinds.
        let first: std::collections::HashSet<Call> = calls.iter().take(100).copied().collect();
        assert!(first.len() >= 6, "{first:?}");
        assert_eq!(day(), calls, "the same day on every run");
    }

    #[test]
    fn memory_report_counts_the_tool_calls_of_a_day_not_only_its_entries() {
        let count = |kind: Call| {
            DAY.iter()
                .find(|(call, _)| *call == kind)
                .map_or(0, |d| d.1)
        };
        assert_eq!(
            day_tool_calls(),
            day().len() + 2 * count(Call::PostAndClose) + count(Call::Session),
            "a closed message is three calls and a session two; the rest are one"
        );
    }

    #[test]
    fn memory_report_reads_a_number_after_its_name_in_the_log() {
        let lines = vec![
            "INFO fold: every run is held in memory, from a full read runs=1560 elapsed_ms=300"
                .to_string(),
        ];
        assert_eq!(field_of(&lines, "runs"), Some(1_560));
        assert_eq!(field_of(&lines, "elapsed_ms"), Some(300));
        assert_eq!(field_of(&lines, "messages"), None);
    }
}

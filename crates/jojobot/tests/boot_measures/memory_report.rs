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

/// What the server logs when the log it boots over already holds the import
/// (`crates/jojobot/src/wiring.rs`); the real-binary suite pins that spelling against
/// the server, so a change there fails there.
const ALREADY_IMPORTED: &str = "claims import: already done";

/// **The projections a boot builds, read from what it said.** The import and the
/// snapshot after it happen only when the log holds no claims event, and the store
/// the report boots over is recorded by a build, so a later recording can hold one.
/// A boot that says the import is already done is not asked for them; a boot that
/// says nothing of the import still is.
fn projections_a_boot_builds(seen: &[String]) -> Vec<Projection> {
    let imported_before = seen.iter().any(|line| line.contains(ALREADY_IMPORTED));
    Projection::ALL
        .into_iter()
        .filter(|projection| {
            !imported_before
                || !matches!(
                    projection,
                    Projection::ClaimsImport | Projection::FirstSnapshot
                )
        })
        .collect()
}

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
    let patience = BOOT_PATIENCE;
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

/// How long a first boot over a replayed dump may take before the report gives up.
/// The claims import of that boot writes the whole dump's claims into the event
/// log, and on a CI host running many binaries side by side it took longer than
/// the 120 seconds a room allows a server.
const BOOT_PATIENCE: Duration = Duration::from_secs(600);

/// Clients that make the day's calls at once. The grow step writes through eight.
const CLIENTS: usize = 8;

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

/// **What a store holds, counted and weighed**, so a reader can set CI's store beside
/// production's. The memory the claims State takes follows the bytes of every write of
/// every claim and how deep each claim's writes go, not only how many things exist.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Shape {
    claims: i64,
    /// Every write of every claim, the history the claims State keeps whole.
    claim_writes: i64,
    claim_write_bytes: i64,
    /// The most writes any one claim has.
    deepest_claim: i64,
    field_writes: i64,
    field_write_bytes: i64,
    /// Batches of the event log. The validity row is not a batch.
    event_batches: i64,
    event_bytes: i64,
    messages: i64,
    message_bytes: i64,
    runs: i64,
    journal_entries: i64,
    journal_bytes: i64,
    /// What a write of a claim carries beside its text, which the claims State keeps
    /// with every write and the images do not: the receipt it was served, the
    /// application stamp, and the references and claims it stands for.
    receipt_bytes: i64,
    stamp_bytes: i64,
    link_bytes: i64,
}

async fn shape_in(pool: &sqlx::MySqlPool) -> Shape {
    Shape {
        claims: count_of(pool, "(SELECT COUNT(*) FROM fact)").await,
        claim_writes: count_of(pool, "(SELECT COUNT(*) FROM fact_write)").await,
        claim_write_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(LENGTH(content) + COALESCE(LENGTH(details), 0)), 0) FROM fact_write)",
        )
        .await,
        deepest_claim: count_of(
            pool,
            "(SELECT COALESCE(MAX(n), 0) FROM (SELECT COUNT(*) AS n FROM fact_write GROUP BY entity, fact_id) AS depth)",
        )
        .await,
        field_writes: count_of(pool, "(SELECT COUNT(*) FROM field_write)").await,
        field_write_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(LENGTH(value)), 0) FROM field_write)",
        )
        .await,
        event_batches: count_of(
            pool,
            "(SELECT COUNT(*) FROM event_log WHERE first_position IS NOT NULL)",
        )
        .await,
        event_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(LENGTH(payload)), 0) FROM event_log WHERE first_position IS NOT NULL)",
        )
        .await,
        messages: count_of(pool, "(SELECT COUNT(*) FROM message)").await,
        message_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(LENGTH(body)), 0) FROM message)",
        )
        .await,
        runs: count_of(pool, "(SELECT COUNT(*) FROM session)").await,
        journal_entries: count_of(pool, "(SELECT COUNT(*) FROM journal_entry)").await,
        journal_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(LENGTH(text)), 0) FROM journal_entry)",
        )
        .await,
        receipt_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(LENGTH(receipt)), 0) FROM fact_write)",
        )
        .await,
        stamp_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(LENGTH(app_stamp)), 0) FROM fact_write)",
        )
        .await,
        link_bytes: count_of(
            pool,
            "(SELECT COALESCE(SUM(COALESCE(LENGTH(refs), 0) + COALESCE(LENGTH(stands_for), 0)), 0) FROM fact_write)",
        )
        .await,
    }
}

/// The shape of a stopped store, read by a store of its own that the read starts and
/// stops.
async fn shape_of(state: &Scratch) -> Shape {
    let mut store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("the store comes up to be counted");
    let shape = shape_in(store.pool()).await;
    store.stop().await;
    shape
}

/// The report line for a shape. Every number is named, so a line set beside
/// production's totals needs no key.
fn shape_line(when: &str, shape: &Shape) -> String {
    format!(
        "memory-report store: {when}: {} claims, {} claim writes ({} bytes, deepest {} writes), \
         {} field writes ({} bytes), {} event batches ({} bytes), {} messages ({} bytes), \
         {} runs, {} journal entries ({} bytes); claim writes also carry {} receipt bytes, {} \
         stamp bytes and {} link bytes",
        shape.claims,
        shape.claim_writes,
        shape.claim_write_bytes,
        shape.deepest_claim,
        shape.field_writes,
        shape.field_write_bytes,
        shape.event_batches,
        shape.event_bytes,
        shape.messages,
        shape.message_bytes,
        shape.runs,
        shape.journal_entries,
        shape.journal_bytes,
        shape.receipt_bytes,
        shape.stamp_bytes,
        shape.link_bytes,
    )
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

/// What a first boot over a replayed dump left: the running server and what the
/// boot's log and memory showed.
struct FirstBoot {
    run: Running,
    /// The claims the store held before the boot read them.
    claims: i64,
    server_peak: u64,
    store_peak: u64,
    steps: Vec<Growth>,
    /// Whether this boot ran the claims import, which a store already holding it skips.
    imported: bool,
    things: Option<u64>,
    runs: Option<u64>,
    messages: Option<u64>,
    events: Option<u64>,
}

/// **Make the replayed store as long and as heavy as production's**, on this case's
/// own copy: production's log before the claims import (its events and batches,
/// read from its boot lines) and the copied text padded by the fitted factor. Both
/// are applied here and not in the shared recorder, so the store the check lane
/// boots over is unchanged and a factor change costs one memory job and no regrow.
/// The line names what was done, so a reading is read beside its factor.
async fn calibrate_the_store(state: &Scratch) {
    jojobot_domain::memory::kinds::load_shipped();
    let mut store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("the store comes up to be calibrated");
    let earlier = super::add_the_pre_import_log(
        store.pool(),
        super::PRODUCTION_PRE_IMPORT_EVENTS,
        super::PRODUCTION_PRE_IMPORT_BATCHES,
    )
    .await;
    let padded = super::pad_the_text(store.pool(), super::FITTED_PAD_FACTOR).await;
    store.stop().await;
    record(&calibration_line(&earlier, &padded));
}

/// **A first boot, as production's is on a new build**: the dump the last deployed
/// build recorded and the case grew to the instance's counts, with no binary
/// booted over it yet, so the boot imports the claims and takes the first
/// snapshot. The server's memory is recorded at every line the boot logs.
async fn first_boot() -> FirstBoot {
    let fixture = std::env::var("BOOT_STORE_DIR")
        .expect("CI supplies BOOT_STORE_DIR, the store this case starts from");
    let state = replay_dump("jojobot-memory-report", &fixture).await;
    calibrate_the_store(&state).await;
    // **The claims the boot reads are counted before it**, so a growth at boot is
    // divided by what the boot held and not by what a replay made of it.
    let shape = shape_of(&state).await;
    record(&shape_line("before the first boot", &shape));
    let claims = shape.claims;
    let mut run = spawn(state);
    let boot = watch_boot(&mut run);
    let (server_peak, store_peak) = boot.peak;
    let seen = run.seen.clone();
    let steps = growth(&boot.readings);
    let imported = boot_imported(&seen);
    let missing = missing_projections(&seen, &steps);
    assert!(
        missing.is_empty(),
        "the boot never said it had built: {missing:?}. What it said:\n{}",
        seen.join("\n")
    );
    let after_boot = memory_of_pid(run.pid).expect("the server runs");
    record(&boot_line(imported, server_peak, store_peak, after_boot));
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
    FirstBoot {
        things: field_of(&seen, "things"),
        runs: field_of(&seen, "runs"),
        messages: field_of(&seen, "messages"),
        events: field_of(&seen, "events"),
        run,
        claims,
        server_peak,
        store_peak,
        steps,
        imported,
    }
}

/// Whether the boot ran the claims import: it did unless it said the import was
/// already done.
fn boot_imported(seen: &[String]) -> bool {
    projections_a_boot_builds(seen).contains(&Projection::ClaimsImport)
}

/// **The projections the boot was asked to build and never said it had.** The case
/// fails on any, so a log line the server stops writing, or one it never wrote for
/// this kind of store, is named and not skipped.
fn missing_projections(seen: &[String], steps: &[Growth]) -> Vec<&'static str> {
    projections_a_boot_builds(seen)
        .iter()
        .filter(|projection| !steps.iter().any(|step| step.projection == **projection))
        .map(|projection| projection.label())
        .collect()
}

/// The line that says what the first boot peaked at, and which kind of store it was.
fn boot_line(imported: bool, server_peak: u64, store_peak: u64, after_boot: Memory) -> String {
    format!(
        "memory-report boot: the store {}; server peak {} ({server_peak} kB), store peak {} \
         ({store_peak} kB), server resident after boot {}, of which anonymous {}",
        if imported {
            "needed the claims import at this boot"
        } else {
            "already held the claims import"
        },
        mib(server_peak),
        mib(store_peak),
        mib(after_boot.resident),
        mib(after_boot.anonymous),
    )
}

/// **The line for the boot after the first**: the claims import is done and the
/// first boot's orderly stop left a snapshot, which is the boot production pays at
/// its next release. It is its own line so the first-boot reader does not take it
/// for the first boot.
fn second_boot_line(server_peak: u64, store_peak: u64, after_boot: Memory) -> String {
    format!(
        "memory-report second boot: the store held the claims import and a snapshot; server \
         peak {} ({server_peak} kB), store peak {} ({store_peak} kB), server resident after \
         boot {}, of which anonymous {}",
        mib(server_peak),
        mib(store_peak),
        mib(after_boot.resident),
        mib(after_boot.anonymous),
    )
}

/// **Boot the server again over the store the first boot left**, as production
/// boots at its next release, and record what the boot held. The stop is orderly,
/// so the store it hands back holds a snapshot again.
fn second_boot(state: Scratch) -> Scratch {
    let mut run = spawn(state);
    let boot = watch_boot(&mut run);
    let (server_peak, store_peak) = boot.peak;
    let after_boot = memory_of_pid(run.pid).expect("the second boot's server runs");
    record(&second_boot_line(server_peak, store_peak, after_boot));
    run.stopped().0
}

/// **The line for the boot with no usable snapshot**: the import is done and the
/// snapshot rows are gone, as when the next release is deployed over the build that
/// imported. It is its own line, so the verdict reads it as the boot it is.
fn no_snapshot_boot_line(server_peak: u64, store_peak: u64, after_boot: Memory) -> String {
    format!(
        "memory-report no-snapshot boot: the store held the claims import and no snapshot; \
         server peak {} ({server_peak} kB), store peak {} ({store_peak} kB), server resident \
         after boot {}, of which anonymous {}",
        mib(server_peak),
        mib(store_peak),
        mib(after_boot.resident),
        mib(after_boot.anonymous),
    )
}

/// **Remove the snapshot the last stop took**, so the next boot has none to load.
async fn without_the_snapshot(state: &Scratch) {
    let mut store = Dolt::start(&state.0.join("db"), free_port())
        .await
        .expect("the store comes up to lose its snapshot");
    sqlx::query("DELETE FROM event_log WHERE slot LIKE 'snapshot%'")
        .execute(store.pool())
        .await
        .expect("the snapshot rows are removed");
    store.stop().await;
}

/// **Boot over the import-done store with no snapshot**, record what the boot held
/// and hand back the running server: the replayed day runs on it.
async fn no_snapshot_boot(state: Scratch) -> Running {
    without_the_snapshot(&state).await;
    let mut run = spawn(state);
    let boot = watch_boot(&mut run);
    let (server_peak, store_peak) = boot.peak;
    let after_boot = memory_of_pid(run.pid).expect("the no-snapshot boot's server runs");
    record(&no_snapshot_boot_line(server_peak, store_peak, after_boot));
    run
}

/// **What the calibration did, and whose bytes they are.** Production's per-table
/// byte totals are unread while `PRODUCTION_BYTES_READ_ON` is none, so the line says
/// the bytes are fitted and the comparison it supports does not discharge the card.
fn calibration_line(earlier: &str, padded: &str) -> String {
    calibration_line_for(super::PRODUCTION_BYTES_READ_ON, earlier, padded)
}

fn calibration_line_for(read_on: Option<&str>, earlier: &str, padded: &str) -> String {
    let bytes = match read_on {
        Some(day) => format!("production's totals read on {day}"),
        None => "bytes fitted, production's totals unread".to_string(),
    };
    format!("memory-report calibration: {earlier} {padded}; {bytes}")
}

/// What the boot's log said it held, for the per-thing division.
struct Counts {
    things: Option<u64>,
    runs: Option<u64>,
    messages: Option<u64>,
    events: Option<u64>,
}

/// **What each projection held at the first boot**, divided by the claims, runs
/// or messages it holds. An import that did not run has no figure.
fn per_thing_line(steps: &[Growth], claims: i64, counts: &Counts, imported: bool) -> String {
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
    let claims = claims as u64;
    format!(
        "memory-report per thing held at the first boot (claims {claims}; things {:?}, runs \
         {:?}, messages {:?}, import events {:?}): the start-up and the image of memory {} per \
         claim; memory search {} per claim; run image {} per run; mail image {} per message; \
         claims import {}",
        counts.things,
        counts.runs,
        counts.messages,
        counts.events,
        per(held(Projection::MemoryImage), Some(claims)),
        per(held(Projection::MemorySearch), Some(claims)),
        per(held(Projection::RunImage), counts.runs),
        per(held(Projection::MailImage), counts.messages),
        if imported {
            per(held(Projection::ClaimsImport), Some(claims))
        } else {
            "no bytes: the import did not run at this boot".to_string()
        },
    )
}

fn record_per_thing(boot: &FirstBoot) {
    let counts = Counts {
        things: boot.things,
        runs: boot.runs,
        messages: boot.messages,
        events: boot.events,
    };
    record(&per_thing_line(
        &boot.steps,
        boot.claims,
        &counts,
        boot.imported,
    ));
}

/// **Under or over the unit's own bound**, read from the module as the boot cases
/// read it.
fn verdict(kib: u64) -> &'static str {
    if kib < super::memory_bound_kib() {
        "under"
    } else {
        "OVER"
    }
}

/// **The replay half of the report: a whole day of calls through the server
/// that has just booted.** The day is the kinds of call the live instance makes in
/// the proportions its metrics show, made with no time budget, so the server's
/// resident memory afterward is the memory of a server that has served a day. It
/// takes longer than the lane's test binary is allowed, so it never runs there:
/// the memory lane runs it, with a deadline of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "runs in the memory lane, which has its own deadline, never in `make check`"]
async fn the_servers_memory_after_a_replayed_day() {
    let began = Instant::now();
    let boot = first_boot().await;
    record_per_thing(&boot);
    // **The import boot prepares the store and is not the server of the day.** Its
    // peaks are kept for the summary. Its orderly stop takes a snapshot, so the next
    // boot loads it: no import, the boot over the store's own snapshot. Its stop takes
    // another, which the last boot removes: a boot with none to load is the one the
    // deploy of the next release makes, and the day runs on it. These two boots and the
    // day, not the import boot, are the bound's.
    let (server_peak, store_peak) = (boot.server_peak, boot.store_peak);
    let (prepared, _) = boot.run.stopped();
    let prepared = second_boot(prepared);
    let served = no_snapshot_boot(prepared).await;
    let (pid, port) = (served.pid, served.http_port);
    let replayed = replay_day(pid, port, 0, Duration::MAX).await;
    let samples = replayed.samples;
    let last = samples.last().expect("a replay has samples").2;
    let top = samples.iter().map(|s| s.2.resident).max().unwrap_or(0);
    assert_eq!(
        replayed.made,
        day().len(),
        "the replay stopped before it made the whole day"
    );
    let (server_peak_after, store_peak_after) = served.peaks_kib();
    record(&format!(
        "memory-report day: all {} entries of a day made, which is {} tool calls before any \
         retry, in {} s; server resident {} at the end, {} at the most; peak {}",
        replayed.made,
        day_tool_calls(),
        samples.last().map_or(0, |s| s.0.as_secs()),
        mib(last.resident),
        mib(top),
        mib(server_peak_after),
    ));
    let store_resident: u64 = descendants_of(served.pid)
        .into_iter()
        .filter_map(memory_of_pid)
        .map(|memory| memory.resident)
        .sum();
    let (state, _) = served.stopped();
    let shape_after = shape_of(&state).await;
    record(&shape_line("after the day", &shape_after));
    let claims_after = shape_after.claims;
    let bound = super::memory_bound_kib();
    let beside = last.resident + store_resident;
    record(&format!(
        "memory-report summary: the server's peak through the first boot {} ({} {}); its peak \
         after the day {} ({} {}); its resident memory after the day {} ({} {}), anonymous {}; \
         the store beside it {} resident and {} at its peak; the unit's cgroup holds both, so \
         {} resident against its memoryMax of {} ({}); the store held {claims_after} claims \
         after the day",
        mib(server_peak),
        verdict(server_peak),
        mib(bound),
        mib(server_peak_after),
        verdict(server_peak_after),
        mib(bound),
        mib(last.resident),
        verdict(last.resident),
        mib(bound),
        mib(last.anonymous),
        mib(store_resident),
        mib(store_peak_after.max(store_peak)),
        mib(beside),
        mib(bound),
        verdict(beside),
    ));
    record(&format!(
        "memory-report wall: {} s from the start of the case to its end",
        began.elapsed().as_secs()
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

    /// **A boot over a store that holds the import builds seven projections, and the
    /// report does not ask for the other two.** The server logs the import and the
    /// snapshot after it only when the log holds no claims event; the next recording
    /// of the boot store is made by a build that has the import, so its boot says
    /// `already done` instead. A boot that imported, and a boot that said nothing of
    /// the import, are still asked for all nine.
    #[test]
    fn memory_report_asks_for_the_import_only_when_the_boot_imported() {
        let said = |line: &str| format!("2026-11-10T20:00:00.100Z  INFO jojobot::wiring: {line}");
        let imported = vec![
            said("teaching ledger: open over the event log"),
            said("imported today's claims into the event log events=190000"),
        ];
        let already = vec![
            said("teaching ledger: open over the event log"),
            said("claims import: already done"),
        ];
        let silent = vec![said("teaching ledger: open over the event log")];
        let asked = |seen: &[String]| projections_a_boot_builds(seen).len();
        assert_eq!(asked(&imported), 9, "a boot that imported builds all nine");
        assert_eq!(asked(&silent), 9, "no word of the import is no excuse");
        let skipped: Vec<Projection> = Projection::ALL
            .into_iter()
            .filter(|projection| !projections_a_boot_builds(&already).contains(projection))
            .collect();
        assert_eq!(
            skipped,
            [Projection::ClaimsImport, Projection::FirstSnapshot],
            "the import and the snapshot after it are what an imported store lacks"
        );
    }

    fn grown(projection: Projection, kib: i64) -> Growth {
        Growth {
            projection,
            kib,
            resident_after: 2000,
            at: Duration::from_secs(1),
        }
    }

    fn said(line: &str) -> String {
        format!("2026-11-10T20:00:00.100Z  INFO jojobot::wiring: {line}")
    }

    /// **The case's use of the detection**: a boot is asked for the projections its own
    /// words call for, and is failed for each it never said it built. A boot that said
    /// the import was done is not failed for the two it skips; a boot that said nothing
    /// is failed for them.
    #[test]
    fn memory_report_names_each_projection_the_boot_was_asked_for_and_never_built() {
        let without_import: Vec<Growth> = Projection::ALL
            .into_iter()
            .filter(|p| !matches!(p, Projection::ClaimsImport | Projection::FirstSnapshot))
            .map(|p| grown(p, 1000))
            .collect();
        let with_import: Vec<Growth> = Projection::ALL
            .into_iter()
            .map(|p| grown(p, 1000))
            .collect();
        let already = vec![said("claims import: already done")];
        let silent = vec![said("teaching ledger: open over the event log")];
        let imported = vec![said(
            "imported today's claims into the event log events=190000",
        )];

        assert!(missing_projections(&already, &without_import).is_empty());
        assert!(!boot_imported(&already));
        assert_eq!(
            missing_projections(&silent, &without_import),
            [
                Projection::ClaimsImport.label(),
                Projection::FirstSnapshot.label()
            ],
            "a boot that said nothing of the import is failed for the two it skipped"
        );
        assert!(boot_imported(&silent));
        assert!(missing_projections(&imported, &with_import).is_empty());
        assert!(boot_imported(&imported));
        assert_eq!(
            missing_projections(&already, &without_import[1..]),
            [Projection::MemoryImage.label()],
            "a projection every boot builds is still named when it is missing"
        );
    }

    /// **The boot line says which kind of store booted and keeps the peaks it was
    /// given**, in kB as the log gives them.
    #[test]
    fn memory_report_says_in_its_boot_line_whether_the_store_needed_the_import() {
        let after = Memory {
            resident: 1_100_000,
            anonymous: 1_000_000,
            peak: 0,
        };
        let imported = boot_line(true, 1_201_580, 1_521_188, after);
        let already = boot_line(false, 1_201_580, 1_521_188, after);
        assert_ne!(imported, already, "the two kinds of store read alike");
        for line in [&imported, &already] {
            assert!(line.starts_with("memory-report boot: "), "{line}");
            assert!(line.contains("(1201580 kB)"), "{line}");
            assert!(line.contains("(1521188 kB)"), "{line}");
        }
    }

    /// **The second boot's line is its own line.** It is the boot production pays at
    /// the next release: the import is done and the stop left a snapshot. It keeps
    /// the peaks it was given and is not read by the verdict as the first boot's, so
    /// it must not begin as the first boot's line does.
    #[test]
    fn memory_report_gives_the_second_boot_a_line_the_first_boots_reader_skips() {
        let after = Memory {
            resident: 900_000,
            anonymous: 800_000,
            peak: 0,
        };
        let line = second_boot_line(1_001_580, 1_221_188, after);
        assert!(line.starts_with("memory-report second boot: "), "{line}");
        assert!(!line.starts_with("memory-report boot: "), "{line}");
        assert!(line.contains("(1001580 kB)"), "{line}");
        assert!(line.contains("(1221188 kB)"), "{line}");
    }

    /// **The no-snapshot boot has a line of its own**: the import is done and the
    /// snapshot rows are gone, as at the deploy of the next release. Its prefix is
    /// neither the import boot's nor the second boot's, so the verdict reads each as
    /// what it is.
    #[test]
    fn memory_report_gives_the_no_snapshot_boot_its_own_line() {
        let after = Memory {
            resident: 950_000,
            anonymous: 850_000,
            peak: 0,
        };
        let line = no_snapshot_boot_line(1_101_580, 1_321_188, after);
        assert!(
            line.starts_with("memory-report no-snapshot boot: "),
            "{line}"
        );
        assert!(!line.starts_with("memory-report boot: "), "{line}");
        assert!(!line.starts_with("memory-report second boot: "), "{line}");
        assert!(line.contains("(1101580 kB)"), "{line}");
    }

    /// **The calibration line says whether the bytes are production's or fitted.** The
    /// grown store's bytes are fitted until production's per-table totals are read, and
    /// a reader of the line must not take the store for production's.
    #[test]
    fn memory_report_says_in_its_calibration_line_that_the_bytes_are_fitted() {
        let line = calibration_line("pre_import_events=7 pre_import_batches=3", "pad_factor=20");
        assert!(line.starts_with("memory-report calibration: "), "{line}");
        assert!(line.contains("fitted"), "{line}");
        assert!(line.contains("pad_factor=20"), "{line}");
        let read = calibration_line_for(Some("2026-12-01"), "x", "y");
        assert!(
            read.contains("production's totals read on 2026-12-01"),
            "{read}"
        );
        assert!(!read.contains("fitted"), "{read}");
    }

    /// **The per-thing line divides each projection's growth by what it holds, and
    /// gives the import no figure when it did not run.**
    #[test]
    fn memory_report_gives_an_import_that_did_not_run_no_bytes_per_claim() {
        let steps = vec![
            grown(Projection::MemoryImage, 1000),
            grown(Projection::ClaimsImport, 2500),
        ];
        let counts = Counts {
            things: Some(7),
            runs: Some(5),
            messages: Some(11),
            events: Some(30),
        };
        let ran = per_thing_line(&steps, 25, &counts, true);
        let skipped = per_thing_line(&steps, 25, &counts, false);
        // 1000 kB over 25 claims is 40,960 bytes; 2,500 kB over 25 claims is 102,400.
        assert!(ran.contains("40960 bytes each per claim"), "{ran}");
        assert!(ran.contains("102400 bytes each"), "{ran}");
        assert!(skipped.contains("40960 bytes each per claim"), "{skipped}");
        assert!(
            !skipped.contains("102400"),
            "an import that did not run was given a figure: {skipped}"
        );
        assert!(ran.contains("Some(7)") && ran.contains("Some(30)"), "{ran}");
    }

    /// **The store's line names every count and every byte total it holds**, each with its
    /// own number, so a reader can set production's totals beside CI's and see which
    /// differs. The numbers are all different here so that one read for another shows.
    #[test]
    fn memory_report_names_each_count_and_byte_total_of_a_store() {
        let shape = Shape {
            claims: 11,
            claim_writes: 12,
            claim_write_bytes: 13,
            deepest_claim: 14,
            field_writes: 15,
            field_write_bytes: 16,
            event_batches: 17,
            event_bytes: 18,
            messages: 19,
            message_bytes: 20,
            runs: 21,
            journal_entries: 22,
            journal_bytes: 23,
            receipt_bytes: 24,
            stamp_bytes: 25,
            link_bytes: 26,
        };
        let line = shape_line("before the boot", &shape);
        assert!(
            line.starts_with("memory-report store: before the boot: "),
            "{line}"
        );
        for number in 11..=26 {
            let words: Vec<&str> = line.split(|c: char| !c.is_ascii_digit()).collect();
            assert_eq!(
                words
                    .iter()
                    .filter(|word| **word == number.to_string())
                    .count(),
                1,
                "{number} appears once: {line}"
            );
        }
    }

    /// **The counts and byte totals are read out of the store's own tables.** A real store
    /// holds two claims, one of them written twice, two mails, one run with a journal entry
    /// and one event batch, and the shape says exactly that.
    #[tokio::test]
    async fn memory_report_counts_what_a_real_store_holds() {
        let scratch = Scratch::new("memory-report-shape");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = jojobot_adapters::testing::migrated_database(&store, "shapecase")
            .await
            .expect("a database of its own");
        for id in ["f1", "f2"] {
            sqlx::query(
                "INSERT INTO fact (entity, id, content, provenance, status, recorded_at) \
                 VALUES ('person:homer', ?, 'now', 'inference', 'active', '2026-09-01')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("a claim");
        }
        for (fact, ordinal, content) in [
            ("f1", 1, "eats donuts"),
            ("f1", 2, "eats donuts daily"),
            ("f2", 1, "wears a hat"),
        ] {
            sqlx::query(
                "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, status, recorded_at) \
                 VALUES ('person:homer', ?, ?, ?, 'inference', 'active', '2026-09-01')",
            )
            .bind(fact)
            .bind(ordinal)
            .bind(content)
            .execute(&pool)
            .await
            .expect("a write of a claim");
        }
        // The metadata a write carries beside its text: the receipt it was served, the
        // application stamp, and the links to other claims.
        sqlx::query(
            "UPDATE fact_write SET receipt = 'rrrr', app_stamp = 'ss', refs = 'lll', \
             stands_for = 'mm' WHERE fact_id = 'f1' AND ordinal = 2",
        )
        .execute(&pool)
        .await
        .expect("a write carries its metadata");
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id) \
             VALUES ('person:homer', 'diet', 1, 'donuts', 'f1')",
        )
        .execute(&pool)
        .await
        .expect("a field write");
        for (id, body) in [("m1", "hello"), ("m2", "hello again")] {
            sqlx::query(
                "INSERT INTO message (id, mailbox, ordinal, body, sender, sent_at, state) \
                 VALUES (?, 'inbox', 1, ?, 'omega', '2026-09-01T00:00:00Z', 'new')",
            )
            .bind(id)
            .bind(body)
            .execute(&pool)
            .await
            .expect("a message");
        }
        sqlx::query(
            "INSERT INTO session (id, bot, focus, started_at, state) \
             VALUES ('s1', 'bot:omega', 'work', '2026-09-01T00:00:00Z', 'open')",
        )
        .execute(&pool)
        .await
        .expect("a run");
        sqlx::query(
            "INSERT INTO journal_entry (session, id, ordinal, at, text) \
             VALUES ('s1', 'j1', 1, '2026-09-01T00:00:00Z', 'a beat')",
        )
        .execute(&pool)
        .await
        .expect("a journal entry");
        sqlx::query(
            "INSERT INTO event_log (slot, first_position, last_position, payload) \
             VALUES ('test-slot', 1, 1, 'payload!')",
        )
        .execute(&pool)
        .await
        .expect("an event batch");

        let shape = shape_in(&pool).await;
        store.stop().await;
        assert_eq!(shape.claims, 2, "{shape:?}");
        assert_eq!(
            shape.claim_writes, 3,
            "two claims, one written twice: {shape:?}"
        );
        assert_eq!(shape.deepest_claim, 2, "{shape:?}");
        assert!(
            shape.claim_write_bytes >= ("eats donuts".len() + "eats donuts daily".len()) as i64,
            "{shape:?}"
        );
        assert_eq!(shape.receipt_bytes, 4, "{shape:?}");
        assert_eq!(shape.stamp_bytes, 2, "{shape:?}");
        assert_eq!(
            shape.link_bytes, 5,
            "the references and the claims it stands for: {shape:?}"
        );
        assert_eq!(shape.field_writes, 1, "{shape:?}");
        assert_eq!(shape.field_write_bytes, "donuts".len() as i64, "{shape:?}");
        assert_eq!(shape.messages, 2, "{shape:?}");
        assert_eq!(
            shape.message_bytes,
            ("hello".len() + "hello again".len()) as i64
        );
        assert_eq!(shape.runs, 1, "{shape:?}");
        assert_eq!(shape.journal_entries, 1, "{shape:?}");
        assert_eq!(shape.journal_bytes, "a beat".len() as i64);
        assert_eq!(shape.event_batches, 1, "{shape:?}");
        assert_eq!(shape.event_bytes, "payload!".len() as i64);
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

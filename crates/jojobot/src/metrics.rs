//! **The numbers a scraper reads**, served on a listener of their own.
//!
//! The main listener sits behind the public proxy, so a route there would be
//! reachable from outside. These numbers are bound to a second address that the
//! operator turns on (`JOJOBOT_METRICS_BIND`), and the main router has no
//! `/metrics` route at all.
//!
//! **Every label value comes from a closed set**: a route's template, a verb the
//! router serves, a word a call ends with, a kind, a bot's box name, an app's
//! name, a status code, the name of a read. Never a handle of a stored thing, a
//! message id, a session id or text a caller sent, so the series stay bounded
//! and no life data reaches the scraper.
//!
//! What it counts is kept in memory or read when scraped. A scrape writes
//! nothing to the store; a gauge that needs a store read reads it once, with a
//! timeout, and a read that fails or runs out of time is a metric saying so.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{MatchedPath, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
use prometheus_client::metrics::info::Info;
use prometheus_client::registry::Registry;

use jojobot_domain::app::AppPrincipal;
use jojobot_domain::mailbox::{Mailbox, MailboxName, Mailboxes};
use jojobot_domain::memory::{EntityId, Memory, kinds};
use jojobot_domain::session::{SessionState, Sessions};
use jojobot_mcp::observe::{CallEnd, CallObserver};

/// The one path the metrics listener serves.
pub const PATH: &str = "/metrics";

/// **How long a scrape waits for one store read.** A scraper times out on its
/// own clock, and a scrape that hangs on the store would read as jojobot being
/// down when only one gauge is.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// The text format the encoder writes, which Prometheus and VictoriaMetrics
/// both read.
const CONTENT_TYPE: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

/// **The label a route carries when the router matched none.** Such a path is
/// text a caller chose.
const UNMATCHED: &str = "unmatched";

/// **The box name a person's box is counted under.** A person's box is named
/// for the person, and a person's name never reaches the scraper.
const PERSON_BOX: &str = "operator";

/// **The label of everything a caller made**: a kind a caller declared, or the
/// box of a bot a caller created. Their names are text a caller chose, and a
/// label per name would grow with every one.
const OTHER: &str = "other";

/// **How often a scrape reads the store for its gauges.** A scraper that asks
/// more often is answered with the gauges of the last read, so no scrape
/// interval can drive the store harder than this.
const REFRESH_EVERY: Duration = Duration::from_secs(30);

/// RSS is the resident-page column of this process's statm, in bytes.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn resident_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    // SAFETY: sysconf reads the process's fixed page-size setting.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (size > 0).then(|| pages.saturating_mul(size as u64))
}

/// **What the allocator holds, as its own statistics say it**, read by the binary that
/// installs the allocator and handed to the registry. `None` when the allocator cannot
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocatorBytes {
    /// Bytes the program has allocated and not freed (jemalloc's `stats.allocated`).
    pub allocated: u64,
    /// Bytes in the pages the allocator holds resident (jemalloc's `stats.resident`),
    /// which count the free space inside them.
    pub resident: u64,
}

/// A reader of the allocator's statistics. A scrape calls it once, under the scrape lock.
/// jemalloc's reader advances the statistics epoch, which merges every arena's counters: a
/// global operation, cheap beside the store reads of the same scrape and not free.
pub type AllocatorReader = fn() -> Option<AllocatorBytes>;

/// The kind a thing is counted under: its own when the build ships it.
fn kind_label(token: &str) -> String {
    if kinds::SHIPPED.contains(&token) {
        token.to_string()
    } else {
        OTHER.to_string()
    }
}

/// The box a box's mail is counted under: a shipped bot's own name, the
/// operator's for a person's box, and [`OTHER`] for the rest.
fn box_label(held: &Mailbox) -> String {
    if held.is_private() {
        PERSON_BOX.to_string()
    } else if let Some((handle, _)) = jojobot_mcp::seed::SHIPPED_BOTS
        .iter()
        .find(|(handle, _)| held.owner.as_str() == *handle)
    {
        MailboxName::named_for(&EntityId((*handle).to_string())).0
    } else {
        OTHER.to_string()
    }
}

/// The reads a scrape makes, each named on [`READ_OK`]'s label.
const READ_SESSIONS: &str = "sessions";
const READ_MAIL: &str = "mail";
const READ_THINGS: &str = "things";
const READ_PAYLOAD: &str = "payload";
const READ_ALLOCATOR: &str = "allocator";

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct RouteStatus {
    route: String,
    status: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Route {
    route: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct VerbEnd {
    verb: String,
    outcome: &'static str,
    refusal_code: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Verb {
    verb: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StoreEnd {
    rail: &'static str,
    operation: &'static str,
    outcome: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct AppStatus {
    app: String,
    status: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct BoxState {
    mailbox: String,
    state: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Kind {
    kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Shadowed {
    family: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct SnapshotSkip {
    reason: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Read {
    read: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Build {
    version: &'static str,
    build: &'static str,
}

/// The name of the gauge that says whether a scrape's read answered.
const READ_OK: &str = "jojobot_scrape_read_ok";

fn latency() -> Histogram {
    // 1 ms to about 33 s: a read answered from memory sits at the bottom, and a
    // write that waits on the store sits in the middle.
    Histogram::new(exponential_buckets(0.001, 2.0, 16))
}

/// **What this process counts**, and the registry the scrape encodes.
pub struct Metrics {
    registry: Registry,
    started: Instant,
    http_requests: Family<RouteStatus, Counter>,
    http_duration: Family<Route, Histogram, fn() -> Histogram>,
    calls: Family<VerbEnd, Counter>,
    call_duration: Family<Verb, Histogram, fn() -> Histogram>,
    store_errors: Family<StoreEnd, Counter>,
    store_duration: Family<StoreEnd, Histogram, fn() -> Histogram>,
    app_writes: Family<AppStatus, Counter>,
    uptime: Gauge<f64, AtomicU64>,
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    process_resident: Gauge,
    /// The bytes of text the claims State holds for the data it serves, and the
    /// records that hold it: the State's own count, read with the store's gauges.
    payload_bytes: Gauge,
    payload_records: Gauge,
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    resident_to_payload: Gauge<f64, AtomicU64>,
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    resident_per_record: Gauge<f64, AtomicU64>,
    /// The two heap series and the reader that fills them, present only when the
    /// binary that installs an allocator handed one ([`Metrics::with_allocator`]).
    allocator: Option<(Gauge, Gauge, AllocatorReader)>,
    sessions_live: Family<[(&'static str, &'static str); 0], Gauge>,
    mail: Family<BoxState, Gauge>,
    things: Family<Kind, Gauge>,
    snapshot_skips: Family<SnapshotSkip, Gauge>,
    read_ok: Family<Read, Gauge>,
    shadow_comparisons: Family<Shadowed, Gauge>,
    shadow_disagreements: Family<Shadowed, Gauge>,
    /// The shadow whose counts a scrape reports, set once the store is wired.
    shadow: std::sync::OnceLock<Arc<jojobot_adapters::fold::shadow::Shadow>>,
    /// The claims State a scrape counts the payload of, set once the store is wired. A
    /// served store lends none of its own, so the process says where its State is.
    claims: std::sync::OnceLock<Arc<dyn jojobot_domain::log::claims::ClaimsLender>>,
    /// **One scrape at a time**, holding when the gauges were last read: a
    /// scrape clears and refills them, and two at once would encode each
    /// other's half-filled families.
    scraping: tokio::sync::Mutex<Option<Instant>>,
    /// How long the gauges of one read are served.
    refresh_every: Duration,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let http_requests = Family::<RouteStatus, Counter>::default();
        registry.register(
            "jojobot_http_requests",
            "HTTP requests on the main listener, by route template and status",
            http_requests.clone(),
        );
        let http_duration: Family<Route, Histogram, fn() -> Histogram> =
            Family::new_with_constructor(latency);
        registry.register(
            "jojobot_http_request_duration_seconds",
            "Time to the response head on the main listener, by route template",
            http_duration.clone(),
        );
        let calls = Family::<VerbEnd, Counter>::default();
        registry.register(
            "jojobot_mcp_calls",
            "MCP tool calls by verb and how they ended: answered, or refused with who fixes it",
            calls.clone(),
        );
        let call_duration: Family<Verb, Histogram, fn() -> Histogram> =
            Family::new_with_constructor(latency);
        registry.register(
            "jojobot_mcp_call_duration_seconds",
            "Time an MCP tool call took to answer, by verb",
            call_duration.clone(),
        );
        let app_writes = Family::<AppStatus, Counter>::default();
        let store_errors = Family::<StoreEnd, Counter>::default();
        registry.register(
            "jojobot_store_operation_errors",
            "Store errors by persisted interface and operation; domain refusals are excluded",
            store_errors.clone(),
        );
        let store_duration: Family<StoreEnd, Histogram, fn() -> Histogram> =
            Family::new_with_constructor(latency);
        registry.register(
            "jojobot_store_operation_duration_seconds",
            "Time a persisted store operation took to answer, by interface, operation and outcome",
            store_duration.clone(),
        );
        registry.register(
            "jojobot_app_writes",
            "Writes an app sent, by app and the status it was answered with",
            app_writes.clone(),
        );
        let uptime = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "jojobot_uptime_seconds",
            "Seconds since this process started",
            uptime.clone(),
        );
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let process_resident = {
            let resident = Gauge::default();
            registry.register(
                "process_resident_memory_bytes",
                "Resident bytes of this process",
                resident.clone(),
            );
            resident
        };
        let payload_bytes = Gauge::default();
        registry.register(
            "jojobot_payload_bytes",
            "Bytes of text the claims State serves now: the name, source, aliases, boot text and metadata of each object, and the content, details and current key and value writes of each claim, at any status. A claim's earlier wording and replaced writes are history and are not counted",
            payload_bytes.clone(),
        );
        let payload_records = Gauge::default();
        registry.register(
            "jojobot_payload_records",
            "Objects and claims that hold the payload, each counted once and alike",
            payload_records.clone(),
        );
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let resident_to_payload = Gauge::<f64, AtomicU64>::default();
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        registry.register(
            "jojobot_resident_to_payload_ratio",
            "Resident bytes of this scrape over the payload bytes of the last read, which can be one read interval older; 0 while the payload is unread or empty",
            resident_to_payload.clone(),
        );
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let resident_per_record = Gauge::<f64, AtomicU64>::default();
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        registry.register(
            "jojobot_resident_bytes_per_record",
            "Resident bytes of this scrape over the payload records of the last read, which can be one read interval older; 0 while the payload is unread or empty",
            resident_per_record.clone(),
        );
        let sessions_live = Family::<[(&'static str, &'static str); 0], Gauge>::default();
        registry.register(
            "jojobot_sessions_live",
            "Runs that are active: begun and neither wrapped nor swept",
            sessions_live.clone(),
        );
        let mail = Family::<BoxState, Gauge>::default();
        registry.register(
            "jojobot_mail_messages",
            "Messages in each box by state: a shipped bot's box by name, a person's as operator, every other as other",
            mail.clone(),
        );
        let things = Family::<Kind, Gauge>::default();
        registry.register(
            "jojobot_things",
            "Things that are not archived or folded into another, by kind: a shipped kind by name, every other as other",
            things.clone(),
        );
        let snapshot_skips = Family::<SnapshotSkip, Gauge>::default();
        registry.register(
            "jojobot_snapshot_skips",
            "Boots of this process that did not use the snapshot the log held, by reason; a boot that skipped one replayed the log and was not a fast boot",
            snapshot_skips.clone(),
        );
        let read_ok = Family::<Read, Gauge>::default();
        registry.register(
            READ_OK,
            "1 when the store read a scrape makes for its gauges answered in time, else 0",
            read_ok.clone(),
        );
        let shadow_comparisons = Family::<Shadowed, Gauge>::default();
        registry.register(
            "jojobot_shadow_comparisons",
            "Reads of a shadowed projection family that were answered both ways and compared, since boot",
            shadow_comparisons.clone(),
        );
        let shadow_disagreements = Family::<Shadowed, Gauge>::default();
        registry.register(
            "jojobot_shadow_disagreements",
            "Of those comparisons, reads whose two answers differed, since boot",
            shadow_disagreements.clone(),
        );
        registry.register(
            "jojobot_build",
            "The build this process runs",
            Info::new(Build {
                version: jojobot_mcp::observe::VERSION,
                build: jojobot_mcp::observe::BUILD,
            }),
        );
        Metrics {
            registry,
            started: Instant::now(),
            http_requests,
            http_duration,
            calls,
            call_duration,
            store_errors,
            store_duration,
            app_writes,
            uptime,
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            process_resident,
            payload_bytes,
            payload_records,
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            resident_to_payload,
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            resident_per_record,
            allocator: None,
            sessions_live,
            mail,
            things,
            snapshot_skips,
            read_ok,
            shadow_comparisons,
            shadow_disagreements,
            shadow: std::sync::OnceLock::new(),
            claims: std::sync::OnceLock::new(),
            scraping: tokio::sync::Mutex::new(None),
            refresh_every: REFRESH_EVERY,
        }
    }

    /// **Report the allocator's own heap statistics.** The binary that installs an
    /// allocator hands its reader here, and two series appear: the bytes in use
    /// (`stats.allocated`) and the bytes the allocator holds beyond them
    /// (`stats.resident` less it). A process that hands none, such as a test binary or a
    /// build on another allocator, registers neither: a library that read the statistics
    /// of an allocator the process does not run would report a near-zero heap as fact.
    #[must_use]
    pub fn with_allocator(mut self, reader: AllocatorReader) -> Self {
        let in_use = Gauge::default();
        self.registry.register(
            "process_heap_in_use_bytes",
            "Bytes the program has allocated and not freed, from the allocator's own statistics",
            in_use.clone(),
        );
        let retained = Gauge::default();
        self.registry.register(
            "process_allocator_retained_bytes",
            "Resident bytes the allocator holds beyond what is allocated, from its own statistics",
            retained.clone(),
        );
        self.allocator = Some((in_use, retained, reader));
        self
    }

    /// **The same, reading the store at most once per `every`**, for a case
    /// that has to see a read happen or not happen.
    #[must_use]
    pub fn refreshing_every(self, every: Duration) -> Self {
        Self {
            refresh_every: every,
            ..self
        }
    }

    /// **Count the payload of this State on every read of the gauges.** Set once, when
    /// the store is wired; a second call changes nothing.
    pub fn watch_claims(&self, lender: Arc<dyn jojobot_domain::log::claims::ClaimsLender>) {
        let _ = self.claims.set(lender);
    }

    /// **Report this shadow's counts on every scrape**, per family. Set once,
    /// when the store is wired; a second call changes nothing.
    pub fn watch_shadow(&self, shadow: Arc<jojobot_adapters::fold::shadow::Shadow>) {
        let _ = self.shadow.set(shadow);
    }

    fn http(&self, route: String, status: StatusCode, elapsed: Duration) {
        self.http_requests
            .get_or_create(&RouteStatus {
                route: route.clone(),
                status: status.as_u16().to_string(),
            })
            .inc();
        self.http_duration
            .get_or_create(&Route { route })
            .observe(elapsed.as_secs_f64());
    }

    fn app_write(&self, app: String, status: StatusCode) {
        self.app_writes
            .get_or_create(&AppStatus {
                app,
                status: status.as_u16().to_string(),
            })
            .inc();
    }

    /// **Fill the gauges a scrape reads, then encode everything.** Each read is
    /// made once, under [`READ_TIMEOUT`]; a read that fails leaves its gauges
    /// empty and says so on [`READ_OK`].
    async fn scrape(&self, ports: &Ports) -> String {
        let mut last_read = self.scraping.lock().await;
        self.uptime.set(self.started.elapsed().as_secs_f64());
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            self.process_resident.set(
                resident_bytes()
                    .and_then(|bytes| i64::try_from(bytes).ok())
                    .unwrap_or_default(),
            );
        }
        if let Some((in_use, retained, reader)) = &self.allocator {
            // A failed read leaves both series at their last value and says so on the
            // read gauge: a zero here would read as a healthy heap.
            let held = reader();
            if let Some(held) = held {
                in_use.set(i64::try_from(held.allocated).unwrap_or(i64::MAX));
                retained.set(
                    i64::try_from(held.resident.saturating_sub(held.allocated)).unwrap_or(i64::MAX),
                );
            }
            self.answered(READ_ALLOCATOR, held.is_some());
        }
        // Read from the process, not the store: every scrape serves them.
        for (reason, count) in jojobot_adapters::log_snapshot::skip_counts() {
            self.snapshot_skips
                .get_or_create(&SnapshotSkip { reason })
                .set(i64::try_from(count).unwrap_or(i64::MAX));
        }
        if last_read.is_none_or(|at| at.elapsed() >= self.refresh_every) {
            self.read_gauges(ports).await;
            *last_read = Some(Instant::now());
        }
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            let resident = self.process_resident.get() as f64;
            let over = |of: i64| if of > 0 { resident / of as f64 } else { 0.0 };
            self.resident_to_payload.set(over(self.payload_bytes.get()));
            self.resident_per_record
                .set(over(self.payload_records.get()));
        }
        // **The shadow's counts are in memory**, so they are read on every scrape
        // and not on the store's interval.
        if let Some(shadow) = self.shadow.get() {
            for counted in shadow.counts() {
                let family = Shadowed {
                    family: counted.family.name(),
                };
                self.shadow_comparisons
                    .get_or_create(&family)
                    .set(i64::try_from(counted.reads).unwrap_or(i64::MAX));
                self.shadow_disagreements
                    .get_or_create(&family)
                    .set(i64::try_from(counted.disagreements).unwrap_or(i64::MAX));
            }
        }
        let mut text = String::new();
        // Writing into a String cannot fail.
        let _ = prometheus_client::encoding::text::encode(&mut text, &self.registry);
        text
    }

    /// **Read the store once for every gauge a scrape serves.**
    async fn read_gauges(&self, ports: &Ports) {
        self.sessions_live.clear();
        let sessions = tokio::time::timeout(READ_TIMEOUT, ports.sessions.run_starts(None)).await;
        let read = match sessions {
            Ok(Ok(runs)) => {
                let live = runs
                    .iter()
                    .filter(|run| run.state == SessionState::Active)
                    .count();
                self.sessions_live
                    .get_or_create(&[])
                    .set(i64::try_from(live).unwrap_or(i64::MAX));
                true
            }
            _ => false,
        };
        self.answered(READ_SESSIONS, read);

        self.mail.clear();
        let boxes = tokio::time::timeout(READ_TIMEOUT, ports.mailboxes.list_mailboxes()).await;
        let read = match boxes {
            Ok(Ok(boxes)) => {
                for held in &boxes {
                    let mailbox = box_label(held);
                    let counts = held.counts;
                    for (state, count) in [
                        ("new", counts.new),
                        ("read", counts.read),
                        ("processed", counts.processed),
                    ] {
                        self.mail
                            .get_or_create(&BoxState {
                                mailbox: mailbox.clone(),
                                state,
                            })
                            .inc_by(i64::try_from(count).unwrap_or(i64::MAX));
                    }
                }
                true
            }
            _ => false,
        };
        self.answered(READ_MAIL, read);

        self.things.clear();
        let things = tokio::time::timeout(READ_TIMEOUT, ports.memory.list_entities(None)).await;
        let read = match things {
            Ok(Ok(things)) => {
                for thing in things.iter().filter(|thing| thing.browsable()) {
                    self.things
                        .get_or_create(&Kind {
                            kind: kind_label(thing.id.kind_token()),
                        })
                        .inc();
                }
                true
            }
            _ => false,
        };
        self.answered(READ_THINGS, read);

        let payload = self
            .claims
            .get()
            .cloned()
            .or_else(|| ports.memory.claims_lender())
            .and_then(|lender| Self::payload_of(lender.as_ref()));
        self.payload_bytes
            .set(payload.map_or(0, |held| i64::try_from(held.bytes).unwrap_or(i64::MAX)));
        self.payload_records
            .set(payload.map_or(0, |held| i64::try_from(held.records).unwrap_or(i64::MAX)));
        self.answered(READ_PAYLOAD, payload.is_some());
    }

    /// **The State's own count of what it holds**, or `None` when it cannot answer:
    /// a store with no State to lend, or one that is behind the log.
    fn payload_of(
        lender: &dyn jojobot_domain::log::claims::ClaimsLender,
    ) -> Option<jojobot_domain::log::claims::Payload> {
        let mut payload = None;
        lender
            .lend(&mut |state| payload = Some(state.payload()))
            .then_some(payload)
            .flatten()
    }

    fn answered(&self, read: &'static str, ok: bool) {
        self.read_ok
            .get_or_create(&Read { read })
            .set(i64::from(ok));
    }
}

impl CallObserver for Metrics {
    fn call_answered(&self, verb: &str, end: CallEnd, refusal_code: &str, elapsed: Duration) {
        self.calls
            .get_or_create(&VerbEnd {
                verb: verb.to_string(),
                outcome: end.as_str(),
                refusal_code: if end == CallEnd::Answered {
                    ""
                } else {
                    jojobot_mcp::observe::bounded_refusal_code(refusal_code)
                }
                .to_string(),
            })
            .inc();
        self.call_duration
            .get_or_create(&Verb {
                verb: verb.to_string(),
            })
            .observe(elapsed.as_secs_f64());
    }
}

impl jojobot_adapters::observe::Observer for Metrics {
    fn finished(
        &self,
        operation: jojobot_adapters::observe::Operation,
        outcome: jojobot_adapters::observe::Outcome,
        elapsed: Duration,
    ) {
        let labels = StoreEnd {
            rail: operation.rail(),
            operation: operation.name(),
            outcome: outcome.as_str(),
        };
        if outcome == jojobot_adapters::observe::Outcome::StoreError {
            self.store_errors.get_or_create(&labels).inc();
        }
        self.store_duration
            .get_or_create(&labels)
            .observe(elapsed.as_secs_f64());
    }
}

/// **Count a request on the main listener**, under the route the router matched.
/// Mounted as a route layer, so it runs after routing and the template is known.
pub async fn track_request(
    State(metrics): State<Arc<Metrics>>,
    req: Request,
    next: Next,
) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| UNMATCHED.to_string(), |path| path.as_str().to_string());
    let started = Instant::now();
    let response = next.run(req).await;
    metrics.http(route, response.status(), started.elapsed());
    response
}

/// **Count an app's write under the app's name**, once the app is known.
/// Mounted inside the app gate, which is what puts the app in the request.
pub async fn track_app_write(
    State(metrics): State<Arc<Metrics>>,
    req: Request,
    next: Next,
) -> Response {
    let app = req
        .extensions()
        .get::<AppPrincipal>()
        .map(|app| app.id.as_str().to_string());
    let response = next.run(req).await;
    if let Some(app) = app {
        metrics.app_write(app, response.status());
    }
    response
}

/// The ports a scrape reads its gauges from.
#[derive(Clone)]
pub struct Ports {
    pub sessions: Arc<dyn Sessions>,
    pub mailboxes: Arc<dyn Mailboxes>,
    pub memory: Arc<dyn Memory>,
}

#[derive(Clone)]
struct Scrape {
    metrics: Arc<Metrics>,
    ports: Ports,
}

async fn serve_metrics(State(scrape): State<Scrape>) -> Response {
    let text = scrape.metrics.scrape(&scrape.ports).await;
    ([(header::CONTENT_TYPE, CONTENT_TYPE)], text).into_response()
}

/// **The metrics listener's router**: `/metrics`, and nothing else.
pub fn router(metrics: Arc<Metrics>, ports: Ports) -> Router {
    Router::new()
        .route(PATH, get(serve_metrics))
        .with_state(Scrape { metrics, ports })
}

/// **What the process holds beside what the State must hold**, read once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overhead {
    pub payload: jojobot_domain::log::claims::Payload,
    /// Zero where the platform gives no resident figure.
    pub resident_bytes: u64,
}

/// **The overhead now**, or `None` when the State cannot answer.
pub fn overhead(lender: &dyn jojobot_domain::log::claims::ClaimsLender) -> Option<Overhead> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    let resident_bytes = resident_bytes().unwrap_or_default();
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    let resident_bytes = 0;
    let payload = Metrics::payload_of(lender)?;
    Some(Overhead {
        payload,
        resident_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::mailbox::{MailboxName, StateCounts};
    use jojobot_domain::memory::{EntityId, NewEntity};

    #[tokio::test]
    async fn refusal_statistics_scrape_labels_the_reason_without_unbounded_values() {
        let metrics = Metrics::new();
        metrics.call_answered("capture", CallEnd::Change, "InvalidFact", Duration::ZERO);
        metrics.call_answered(
            "capture",
            CallEnd::Change,
            "caller-chose-this",
            Duration::ZERO,
        );
        metrics.call_answered("recall", CallEnd::Answered, "", Duration::ZERO);
        let ports = Ports {
            memory: Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
            sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            mailboxes: Arc::new(
                jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
            ),
        };
        let text = metrics.scrape(&ports).await;
        let lines: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with("jojobot_mcp_calls_total{"))
            .collect();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("refusal_code=\"InvalidFact\"")),
            "{text}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("refusal_code=\"unlisted\"")),
            "{text}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("outcome=\"answered\"")
                    && line.contains("refusal_code=\"\"")),
            "{text}"
        );
        assert!(!text.contains("caller-chose-this"), "{text}");
    }

    /// A reader that answers once and then fails, as an allocator whose statistics
    /// cannot be read does.
    fn answers_once() -> Option<AllocatorBytes> {
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        (CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0).then_some(AllocatorBytes {
            allocated: 5000,
            resident: 9000,
        })
    }

    #[tokio::test]
    async fn a_failed_allocator_read_keeps_the_last_heap_and_says_so_on_the_read_gauge() {
        let metrics = Metrics::new().with_allocator(answers_once);
        let ports = Ports {
            memory: Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
            sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            mailboxes: Arc::new(
                jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
            ),
        };
        let line = |text: &str, name: &str| -> String {
            text.lines()
                .find(|line| line.starts_with(name))
                .unwrap_or_else(|| panic!("no {name} on the scrape:\n{text}"))
                .to_string()
        };
        let first = metrics.scrape(&ports).await;
        assert!(
            line(&first, "process_heap_in_use_bytes").ends_with(" 5000"),
            "{first}"
        );
        assert!(
            line(&first, "process_allocator_retained_bytes").ends_with(" 4000"),
            "{first}"
        );
        assert!(
            line(&first, "jojobot_scrape_read_ok{read=\"allocator\"}").ends_with(" 1"),
            "{first}"
        );
        let second = metrics.scrape(&ports).await;
        assert!(
            line(&second, "process_heap_in_use_bytes").ends_with(" 5000"),
            "a failed read left a zero as the heap: {second}"
        );
        assert!(
            line(&second, "process_allocator_retained_bytes").ends_with(" 4000"),
            "a failed read left a zero as the retained bytes: {second}"
        );
        assert!(
            line(&second, "jojobot_scrape_read_ok{read=\"allocator\"}").ends_with(" 0"),
            "{second}"
        );
    }

    #[test]
    fn a_kind_or_a_bot_a_caller_made_is_counted_as_other() {
        // A person's box is told by its owner's kind, which the process reads
        // from the kinds it loaded, as a booted process holds them.
        kinds::load(kinds::SHIPPED);
        assert_eq!(kind_label("person"), "person");
        assert_eq!(kind_label("donut"), OTHER);
        let held = |owner: &str| Mailbox {
            name: MailboxName::named_for(&EntityId(owner.to_string())),
            owner: EntityId(owner.to_string()),
            counts: StateCounts::default(),
            quarantined: Vec::new(),
        };
        assert_eq!(box_label(&held("bot:assistant")), "assistant");
        assert_eq!(box_label(&held("bot:milhouse")), OTHER);
        assert_eq!(box_label(&held("person:milhouse")), PERSON_BOX);
    }

    /// **A shadow's counts are on the scrape, per family, and rise with use.** A
    /// compared read moves the comparison count; a read whose two answers
    /// differ moves the disagreement count as well.
    #[tokio::test]
    async fn a_shadows_counts_are_on_the_scrape() {
        let shadow = Arc::new(
            jojobot_adapters::fold::shadow::Shadow::named("referring_to")
                .expect("referring_to is a family"),
        );
        let metrics = Metrics::new().refreshing_every(Duration::ZERO);
        metrics.watch_shadow(shadow.clone());
        let ports = Ports {
            memory: Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
            sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            mailboxes: Arc::new(
                jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
            ),
        };
        let value = |text: &str, name: &str| -> Option<String> {
            text.lines()
                .find(|line| line.starts_with(&format!("{name}{{family=\"referring_to\"}}")))
                .and_then(|line| line.rsplit(' ').next())
                .map(str::to_string)
        };

        let before = metrics.scrape(&ports).await;
        assert_eq!(
            value(&before, "jojobot_shadow_comparisons").as_deref(),
            Some("0"),
            "{before}"
        );
        assert_eq!(
            value(&before, "jojobot_shadow_disagreements").as_deref(),
            Some("0"),
            "{before}"
        );

        shadow.compared_referring_to(
            &jojobot_domain::memory::EntityId("person:homer".into()),
            &[],
            &[],
        );
        let agreed = metrics.scrape(&ports).await;
        assert_eq!(
            value(&agreed, "jojobot_shadow_comparisons").as_deref(),
            Some("1"),
            "{agreed}"
        );
        assert_eq!(
            value(&agreed, "jojobot_shadow_disagreements").as_deref(),
            Some("0"),
            "{agreed}"
        );

        let subject = jojobot_domain::memory::EntityId("person:marge".into());
        ports
            .memory
            .add_entity(jojobot_domain::memory::NewEntity::new(
                subject.clone(),
                "Marge",
                "fixture",
            ))
            .await
            .unwrap()
            .written()
            .unwrap();
        let fact = ports
            .memory
            .capture(jojobot_domain::memory::NewFact::about(
                subject.clone(),
                "alpha",
                jiff::civil::date(2026, 1, 1),
            ))
            .await
            .unwrap()
            .written()
            .unwrap();
        shadow.compared_referring_to(&subject, &[], &[fact]);
        let differed = metrics.scrape(&ports).await;
        assert_eq!(
            value(&differed, "jojobot_shadow_comparisons").as_deref(),
            Some("2"),
            "{differed}"
        );
        assert_eq!(
            value(&differed, "jojobot_shadow_disagreements").as_deref(),
            Some("1"),
            "{differed}"
        );
    }

    /// **The scrape says what the process holds against what the State must hold.**
    /// The payload and its records are the State's own count; the ratio and the
    /// bytes per record are the resident bytes of the same scrape over them.
    #[tokio::test]
    async fn a_scrape_says_the_payload_beside_the_resident_bytes() {
        let memory = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
        let ports = Ports {
            memory: memory.clone(),
            sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            mailboxes: Arc::new(
                jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
            ),
        };
        let subject = EntityId("person:milhouse".into());
        memory
            .add_entity(NewEntity::new(subject.clone(), "Milhouse", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        memory
            .capture(jojobot_domain::memory::NewFact::about(
                subject,
                "plays the recorder",
                jiff::civil::date(2026, 1, 1),
            ))
            .await
            .expect("capture")
            .written()
            .expect("not blocked");
        let mut held = jojobot_domain::log::claims::Payload::default();
        memory
            .claims_lender()
            .expect("the fake lends its State")
            .lend(&mut |state| held = state.payload());
        assert!(held.bytes > 0 && held.records >= 2, "{held:?}");

        let text = Metrics::new().scrape(&ports).await;
        let value = |name: &str| -> f64 {
            text.lines()
                .find(|line| line.starts_with(&format!("{name} ")))
                .and_then(|line| line.rsplit(' ').next())
                .and_then(|number| number.parse().ok())
                .unwrap_or_else(|| panic!("no {name} on the scrape:\n{text}"))
        };
        assert_eq!(value("jojobot_payload_bytes"), held.bytes as f64, "{text}");
        assert_eq!(
            value("jojobot_payload_records"),
            held.records as f64,
            "{text}"
        );
        assert!(
            text.lines()
                .any(|line| line == "jojobot_scrape_read_ok{read=\"payload\"} 1"),
            "{text}"
        );
        let resident = value("process_resident_memory_bytes");
        let close = |got: f64, want: f64| (got - want).abs() <= want.abs() * 1e-9;
        assert!(
            close(
                value("jojobot_resident_to_payload_ratio"),
                resident / held.bytes as f64
            ),
            "{text}"
        );
        assert!(
            close(
                value("jojobot_resident_bytes_per_record"),
                resident / held.records as f64
            ),
            "{text}"
        );
    }

    /// **A State the process says it serves from is the one a scrape counts**, whatever
    /// the store lends: a served store lends none of its own, and its State is the
    /// writer's.
    #[tokio::test]
    async fn a_scrape_counts_the_state_the_process_watches() {
        let served = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
        served
            .add_entity(NewEntity::new(
                EntityId("person:milhouse".into()),
                "Milhouse",
                "fixture",
            ))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        let ports = Ports {
            memory: Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
            sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            mailboxes: Arc::new(
                jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
            ),
        };
        let count = |lender: Arc<dyn jojobot_domain::log::claims::ClaimsLender>| {
            let mut held = jojobot_domain::log::claims::Payload::default();
            lender.lend(&mut |state| held = state.payload());
            held
        };
        let watched = count(served.claims_lender().expect("the fake lends"));
        let ports_own = count(ports.memory.claims_lender().expect("the fake lends"));
        assert_ne!(watched.records, ports_own.records, "the two States differ");

        let metrics = Metrics::new();
        metrics.watch_claims(served.claims_lender().expect("the fake lends"));
        let text = metrics.scrape(&ports).await;
        assert!(
            text.lines()
                .any(|line| line == format!("jojobot_payload_records {}", watched.records)),
            "{text}"
        );
    }

    /// **The ready line carries the same measure the scrape does**: the State's
    /// payload and the resident bytes of the process, read when the server is ready.
    #[tokio::test]
    async fn memory_overhead_at_ready_is_the_states_payload_beside_the_resident_bytes() {
        let memory = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
        memory
            .add_entity(NewEntity::new(
                EntityId("person:milhouse".into()),
                "Milhouse",
                "fixture",
            ))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        let mut held = jojobot_domain::log::claims::Payload::default();
        let lender = memory.claims_lender().expect("the fake lends its State");
        lender.lend(&mut |state| held = state.payload());
        let overhead = overhead(lender.as_ref()).expect("the State answers");
        assert_eq!(overhead.payload, held);
        assert!(held.bytes > 0, "{held:?}");
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        assert!(overhead.resident_bytes > 0, "{overhead:?}");
    }

    /// **A scrape inside the interval reads nothing**: it serves the gauges of
    /// the last read, so a thing made since is not counted until the next one.
    #[tokio::test]
    async fn a_scrape_inside_the_interval_serves_the_last_read() {
        let memory: Arc<dyn Memory> =
            Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
        let ports = Ports {
            sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            mailboxes: Arc::new(
                jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
            ),
            memory: memory.clone(),
        };
        let people = |text: &str| {
            text.lines()
                .any(|line| line.starts_with(r#"jojobot_things{kind="person"}"#))
        };
        let held = Metrics::new().refreshing_every(Duration::from_secs(3600));
        let fresh = Metrics::new().refreshing_every(Duration::ZERO);
        assert!(!people(&held.scrape(&ports).await));
        assert!(!people(&fresh.scrape(&ports).await));
        memory
            .add_entity(NewEntity::new(
                EntityId("person:milhouse".into()),
                "Milhouse",
                "fixture",
            ))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        assert!(
            !people(&held.scrape(&ports).await),
            "a scrape inside the interval read the store"
        );
        assert!(
            people(&fresh.scrape(&ports).await),
            "a scrape past the interval reads the store"
        );
    }
}

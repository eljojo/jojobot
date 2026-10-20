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
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::memory::Memory;
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

/// The reads a scrape makes, each named on [`READ_OK`]'s label.
const READ_SESSIONS: &str = "sessions";
const READ_MAIL: &str = "mail";
const READ_THINGS: &str = "things";

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
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Verb {
    verb: String,
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
    app_writes: Family<AppStatus, Counter>,
    uptime: Gauge<f64, AtomicU64>,
    sessions_live: Gauge,
    mail: Family<BoxState, Gauge>,
    things: Family<Kind, Gauge>,
    read_ok: Family<Read, Gauge>,
    /// **One scrape at a time**: a scrape clears and refills the gauges it
    /// reads, and two at once would encode each other's half-filled families.
    scraping: tokio::sync::Mutex<()>,
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
        let sessions_live = Gauge::default();
        registry.register(
            "jojobot_sessions_live",
            "Runs that are active: begun and neither wrapped nor swept",
            sessions_live.clone(),
        );
        let mail = Family::<BoxState, Gauge>::default();
        registry.register(
            "jojobot_mail_messages",
            "Messages in each box by state; a person's box is counted as operator",
            mail.clone(),
        );
        let things = Family::<Kind, Gauge>::default();
        registry.register(
            "jojobot_things",
            "Things that are not archived or folded into another, by kind",
            things.clone(),
        );
        let read_ok = Family::<Read, Gauge>::default();
        registry.register(
            READ_OK,
            "1 when the store read a scrape makes for its gauges answered in time, else 0",
            read_ok.clone(),
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
            app_writes,
            uptime,
            sessions_live,
            mail,
            things,
            read_ok,
            scraping: tokio::sync::Mutex::new(()),
        }
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
        let _one_at_a_time = self.scraping.lock().await;
        self.uptime.set(self.started.elapsed().as_secs_f64());

        let sessions = tokio::time::timeout(READ_TIMEOUT, ports.sessions.all_summaries()).await;
        let read = match sessions {
            Ok(Ok(runs)) => {
                let live = runs
                    .iter()
                    .filter(|run| run.state == SessionState::Active)
                    .count();
                self.sessions_live
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
                    let mailbox = if held.is_private() {
                        PERSON_BOX.to_string()
                    } else {
                        held.name.as_str().to_string()
                    };
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
                            kind: thing.id.kind_token().to_string(),
                        })
                        .inc();
                }
                true
            }
            _ => false,
        };
        self.answered(READ_THINGS, read);

        let mut text = String::new();
        // Writing into a String cannot fail.
        let _ = prometheus_client::encoding::text::encode(&mut text, &self.registry);
        text
    }

    fn answered(&self, read: &'static str, ok: bool) {
        self.read_ok
            .get_or_create(&Read { read })
            .set(i64::from(ok));
    }
}

impl CallObserver for Metrics {
    fn call_answered(&self, verb: &str, end: CallEnd, elapsed: Duration) {
        self.calls
            .get_or_create(&VerbEnd {
                verb: verb.to_string(),
                outcome: end.as_str(),
            })
            .inc();
        self.call_duration
            .get_or_create(&Verb {
                verb: verb.to_string(),
            })
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

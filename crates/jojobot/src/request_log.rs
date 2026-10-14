//! **One log line for every request to `/mcp`, when it ends.**
//!
//! The transport logs a request with the client library's name and nothing
//! else, so a connection that is cut cannot be tied to a caller, a verb or a
//! standing stream. This layer names them: the HTTP method, the MCP method and
//! tool, the bot and run behind the key, the time it was open and how it ended.
//!
//! **Names, ids and timings only.** No body, claim text, field key or key ever
//! reaches a line: the run is named by its public name, never by the key the
//! caller sent.
//!
//! **Two layers, because the line and the naming cost different things.** The
//! outer one, [`log_request`], opens the line and writes it, so a request the
//! bearer check refuses is logged too. It reads no body. The inner one,
//! [`name_the_request`], sits inside the bearer check: reading a request's body
//! to name its verb is work done for the caller, so it is done only for a caller
//! who was let in, and it tells the outer layer what it found.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jojobot_mcp::sid::SessionRegistry;

/// How long a request may stay open before one warning says so.
pub const SLOW_AFTER: Duration = Duration::from_secs(60);

/// The largest request body this layer reads to name the verb. A request over it
/// is refused rather than passed on half read.
const READ_AT_MOST: usize = 16 * 1024 * 1024;

/// What the layer needs to name a caller.
#[derive(Clone)]
pub struct RequestLog {
    registry: Arc<SessionRegistry>,
    slow_after: Duration,
}

impl RequestLog {
    pub fn new(registry: Arc<SessionRegistry>) -> Self {
        Self {
            registry,
            slow_after: SLOW_AFTER,
        }
    }

    /// A shorter warning threshold, so a case need not wait a minute.
    pub fn slow_after(mut self, after: Duration) -> Self {
        self.slow_after = after;
        self
    }
}

/// **What a request names**, kept as plain words that are safe to print: the
/// MCP method and the tool, and the bot and run behind the key.
#[derive(Clone, Default)]
struct Named {
    http_method: String,
    mcp_method: Option<String>,
    tool: Option<String>,
    bot: Option<String>,
    run: Option<String>,
}

/// **The names of one request, shared by the two layers.** The outer layer puts
/// it in the request's extensions and reads it when the line is written; the
/// inner layer fills in what the body says once the caller is let in.
#[derive(Clone)]
struct Shared(Arc<Mutex<Named>>);

impl Shared {
    fn read(&self) -> Named {
        self.0.lock().expect("the names of a request").clone()
    }
}

/// **A name from the wire, made safe to print**: only the characters a method or
/// tool name is made of, and no more than 64 of them.
fn printable(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':'))
        .take(64)
        .collect()
}

/// Read the verb out of a JSON-RPC body. A body that is not one request names
/// nothing, and is still served.
fn name_the_call(bytes: &[u8], registry: &SessionRegistry, named: &mut Named) {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return;
    };
    let Some(method) = value.get("method").and_then(|m| m.as_str()) else {
        return;
    };
    named.mcp_method = Some(printable(method));
    if method != "tools/call" {
        return;
    }
    let params = &value["params"];
    named.tool = params["name"].as_str().map(printable);
    if let Some((bot, run)) = params["arguments"]["sid"]
        .as_str()
        .and_then(|key| registry.whose_key(key))
    {
        named.bot = Some(bot);
        named.run = Some(run);
    }
}

/// **The line for one request, written when it is dropped**, whichever way it
/// ends: the body was read to its end, the client or the proxy closed it, or the
/// request was refused.
struct Ticket {
    named: Shared,
    started: Instant,
    status: Option<StatusCode>,
    finished: bool,
    slow: Option<tokio::task::JoinHandle<()>>,
}

impl Ticket {
    fn open(named: Shared, slow_after: Duration) -> Self {
        let for_warning = named.clone();
        let started = Instant::now();
        // **A standing GET stream is open by design** and is not a call that
        // has hung, so it never warns. Its line is still written when it ends.
        let standing = named.read().http_method == Method::GET.as_str();
        let slow = (!standing).then(|| {
            tokio::spawn(async move {
                tokio::time::sleep(slow_after).await;
                let for_warning = for_warning.read();
                tracing::warn!(
                http_method = %for_warning.http_method,
                mcp_method = for_warning.mcp_method.as_deref(),
                tool = for_warning.tool.as_deref(),
                bot = for_warning.bot.as_deref(),
                run = for_warning.run.as_deref(),
                open_ms = started.elapsed().as_millis() as u64,
                "mcp request still open"
                );
            })
        });
        Self {
            named,
            started,
            status: None,
            finished: false,
            slow,
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(slow) = self.slow.take() {
            slow.abort();
        }
        let named = &self.named.read();
        let duration_ms = self.started.elapsed().as_millis() as u64;
        let refused = self.status.is_some_and(|s| s.as_u16() >= 400);
        let stream = named.http_method == Method::GET.as_str();
        let outcome = match (refused, self.finished) {
            (true, _) => "refused",
            (false, true) => "answered",
            (false, false) => "cancelled",
        };
        macro_rules! line {
            ($level:ident, $message:expr) => {
                tracing::$level!(
                    http_method = %named.http_method,
                    mcp_method = named.mcp_method.as_deref(),
                    tool = named.tool.as_deref(),
                    bot = named.bot.as_deref(),
                    run = named.run.as_deref(),
                    status = self.status.map(|s| s.as_u16()),
                    duration_ms,
                    outcome,
                    $message
                )
            };
        }
        match (stream, outcome, named.tool.is_some()) {
            (true, "cancelled", _) => {
                line!(info, "mcp stream closed by the client or the proxy")
            }
            (true, _, _) => line!(info, "mcp stream ended"),
            (false, "cancelled", true) => {
                line!(error, "mcp tool call cancelled before its answer was sent")
            }
            (false, _, _) => line!(info, "mcp request ended"),
        }
    }
}

/// A response body that ends the request's line when it is read to the end, and
/// when it is dropped before that. Size and end hints pass through, so the
/// framing on the wire does not change.
struct Watched {
    inner: Body,
    ticket: Ticket,
}

impl http_body::Body for Watched {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let polled = Pin::new(&mut self.inner).poll_frame(cx);
        if let Poll::Ready(None) = polled {
            self.ticket.finished = true;
        }
        polled
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

/// **The outer layer: opens the line and writes it when the request ends.** It
/// reads no body. A request the bearer check refuses gets its line, with the
/// status it was refused with.
pub async fn log_request(State(log): State<RequestLog>, mut req: Request, next: Next) -> Response {
    let named = Shared(Arc::new(Mutex::new(Named {
        http_method: req.method().as_str().to_string(),
        ..Named::default()
    })));
    req.extensions_mut().insert(named.clone());

    let mut ticket = Ticket::open(named, log.slow_after);
    let response = next.run(req).await;
    ticket.status = Some(response.status());
    let (parts, inner) = response.into_parts();
    ticket.finished = http_body::Body::is_end_stream(&inner);
    Response::from_parts(parts, Body::new(Watched { inner, ticket }))
}

/// **The inner layer: reads a POST body to name the verb, after the bearer check
/// has passed.** The body is the caller's to send and ours to read only once the
/// caller is allowed in; a request over [`READ_AT_MOST`] is refused rather than
/// passed on half read. What it finds goes to the outer layer's line.
pub async fn name_the_request(State(log): State<RequestLog>, req: Request, next: Next) -> Response {
    let Some(shared) = req.extensions().get::<Shared>().cloned() else {
        return next.run(req).await;
    };
    if req.method() != Method::POST {
        return next.run(req).await;
    }
    let (parts, body) = req.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, READ_AT_MOST).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let mut found = shared.read();
    name_the_call(&bytes, &log.registry, &mut found);
    *shared.0.lock().expect("the names of a request") = found;
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::{get, post};
    use axum::{Router, middleware};
    use jojobot_domain::memory::EntityId;
    use std::sync::Mutex;
    use tower::ServiceExt;

    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Kept {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("log buffer").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Kept {
        type Writer = Kept;
        fn make_writer(&'a self) -> Kept {
            self.clone()
        }
    }

    impl Kept {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().expect("log buffer").clone()).expect("utf8")
        }
        fn lines(&self) -> Vec<String> {
            self.text().lines().map(str::to_string).collect()
        }
    }

    /// The subscriber for one case, held until the case ends.
    fn capture() -> (Kept, tracing::subscriber::DefaultGuard) {
        let kept = Kept::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(kept.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .finish();
        (kept, tracing::subscriber::set_default(subscriber))
    }

    /// The two layers in the order the served app mounts them: the naming layer
    /// innermost, the line outermost.
    fn app(log: RequestLog, answer: axum::routing::MethodRouter) -> Router {
        Router::new()
            .route("/mcp", answer)
            .layer(middleware::from_fn_with_state(
                log.clone(),
                name_the_request,
            ))
            .layer(middleware::from_fn_with_state(log, log_request))
    }

    /// **A stand-in for the bearer check**, between the two layers: a request
    /// with no `authorization` header is refused with 401.
    async fn refuse_without_a_bearer(req: Request, next: Next) -> Response {
        if req.headers().get("authorization").is_none() {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        next.run(req).await
    }

    fn app_behind_a_bearer(log: RequestLog, answer: axum::routing::MethodRouter) -> Router {
        Router::new()
            .route("/mcp", answer)
            .layer(middleware::from_fn_with_state(
                log.clone(),
                name_the_request,
            ))
            .layer(middleware::from_fn(refuse_without_a_bearer))
            .layer(middleware::from_fn_with_state(log, log_request))
    }

    /// A request body that records that it was polled, and errors when it is.
    struct Poisoned(Arc<std::sync::atomic::AtomicBool>);

    impl http_body::Body for Poisoned {
        type Data = axum::body::Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            std::task::Poll::Ready(Some(Err(std::io::Error::other("the body was read"))))
        }
    }

    fn tool_call(tool: &str, key: &str) -> Request {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": tool, "arguments": {"sid": key, "query": "a claim's words"}},
        });
        Request::post("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("a request")
    }

    fn registry_with_a_run() -> (Arc<SessionRegistry>, String, String) {
        let registry = Arc::new(SessionRegistry::new());
        let minted = registry
            .mint(&EntityId("bot:gamma".into()))
            .expect("a free name");
        (registry, minted.key, minted.name.0)
    }

    /// **A recall through the transport logs one line naming recall, the bot,
    /// the run and a duration.** The key the caller sent and the words of its
    /// query appear nowhere in the line.
    #[tokio::test]
    async fn a_recall_logs_one_line_naming_the_tool_the_bot_the_run_and_a_duration() {
        let (kept, _guard) = capture();
        let (registry, key, run) = registry_with_a_run();
        let router = app(RequestLog::new(registry), post(|| async { "answer" }));

        let response = router
            .oneshot(tool_call("recall", &key))
            .await
            .expect("served");
        let _ = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("the answer ends");

        let lines = kept.lines();
        assert_eq!(lines.len(), 1, "one line per request: {lines:?}");
        let line = &lines[0];
        for named in [
            "tool=\"recall\"",
            "bot=\"bot:gamma\"",
            &format!("run=\"{run}\""),
            "http_method=POST",
            "mcp_method=\"tools/call\"",
            "outcome=\"answered\"",
            "duration_ms=",
        ] {
            assert!(line.contains(named), "{named:?} is missing from {line}");
        }
        assert!(!line.contains(&key), "the key was logged: {line}");
        assert!(
            !line.contains("a claim's words"),
            "a body was logged: {line}"
        );
    }

    /// **A call held past the threshold logs one warning naming it**, and the
    /// line for its end still follows.
    #[tokio::test(start_paused = true)]
    async fn a_call_held_past_the_threshold_logs_one_warning_naming_it() {
        let (kept, _guard) = capture();
        let (registry, key, _run) = registry_with_a_run();
        let log = RequestLog::new(registry).slow_after(Duration::from_secs(60));
        let router = app(
            log,
            post(|| async {
                tokio::time::sleep(Duration::from_secs(90)).await;
                "late"
            }),
        );

        let response = router
            .oneshot(tool_call("recall", &key))
            .await
            .expect("served");
        let _ = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("the answer ends");

        let warnings: Vec<String> = kept
            .lines()
            .into_iter()
            .filter(|l| l.contains("WARN"))
            .collect();
        assert_eq!(warnings.len(), 1, "one warning: {:?}", kept.lines());
        assert!(warnings[0].contains("tool=\"recall\""), "{}", warnings[0]);
    }

    /// **A client's GET stream dropped mid-way logs at INFO as a stream, with no
    /// ERROR.**
    #[tokio::test]
    async fn a_get_stream_dropped_by_the_client_logs_info_naming_a_stream() {
        let (kept, _guard) = capture();
        let (registry, _key, _run) = registry_with_a_run();
        let router = app(RequestLog::new(registry), get(|| async { pending_body() }));

        let response = router
            .oneshot(Request::get("/mcp").body(Body::empty()).expect("a request"))
            .await
            .expect("served");
        drop(response);

        let text = kept.text();
        assert!(text.contains("INFO"), "{text}");
        assert!(text.contains("stream"), "{text}");
        assert!(!text.contains("ERROR"), "{text}");
        assert_eq!(kept.lines().len(), 1, "{text}");
    }

    /// **A tool call whose answer is never sent logs an ERROR naming the tool.**
    #[tokio::test]
    async fn a_tool_call_dropped_before_its_answer_logs_an_error_naming_the_tool() {
        let (kept, _guard) = capture();
        let (registry, key, _run) = registry_with_a_run();
        let router = app(RequestLog::new(registry), post(|| async { pending_body() }));

        let response = router
            .oneshot(tool_call("search", &key))
            .await
            .expect("served");
        drop(response);

        let lines = kept.lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("ERROR"), "{}", lines[0]);
        assert!(lines[0].contains("tool=\"search\""), "{}", lines[0]);
        assert!(lines[0].contains("outcome=\"cancelled\""), "{}", lines[0]);
    }

    /// **A request the bearer check refuses is logged and its body is never
    /// read.** The caller is not allowed in, so reading what it sent is a cost
    /// it was not owed. **Paired**: with a bearer the same call is read and its
    /// verb is named, so the refusal is what kept the read from happening.
    #[tokio::test]
    async fn a_refused_post_is_logged_without_its_body_being_read_and_an_admitted_one_is_named() {
        let (kept, _guard) = capture();
        let (registry, key, _run) = registry_with_a_run();
        let router = app_behind_a_bearer(RequestLog::new(registry), post(|| async { "answer" }));

        // ── no bearer: refused, one line, the body untouched ────────────────
        let polled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let refused = router
            .clone()
            .oneshot(
                Request::post("/mcp")
                    .body(Body::new(Poisoned(polled.clone())))
                    .expect("a request"),
            )
            .await
            .expect("served");
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        let _ = axum::body::to_bytes(refused.into_body(), 1 << 20).await;
        assert!(
            !polled.load(std::sync::atomic::Ordering::SeqCst),
            "the body of a refused request was read"
        );
        let lines = kept.lines();
        assert_eq!(
            lines.len(),
            1,
            "a refused request still gets its line: {lines:?}"
        );
        for named in ["http_method=POST", "status=401", "outcome=\"refused\""] {
            assert!(
                lines[0].contains(named),
                "{named:?} is missing from {}",
                lines[0]
            );
        }
        assert!(
            !lines[0].contains("tool="),
            "a refused call names no verb: {}",
            lines[0]
        );

        // ── with a bearer: read, and the verb is named ──────────────────────
        let mut admitted = tool_call("recall", &key);
        admitted
            .headers_mut()
            .insert("authorization", "Bearer a-token".parse().expect("a header"));
        let answered = router.oneshot(admitted).await.expect("served");
        let _ = axum::body::to_bytes(answered.into_body(), 1 << 20).await;
        let lines = kept.lines();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[1].contains("tool=\"recall\""), "{}", lines[1]);
    }

    /// **A standing GET stream that is simply open warns nothing**, however long
    /// it stays. **Paired in the same router**: a POST held as long warns once, so
    /// the silence is the stream's and not a layer that never warns.
    #[tokio::test(start_paused = true)]
    async fn a_standing_get_stream_open_past_the_threshold_warns_nothing_and_a_held_post_does() {
        let (kept, _guard) = capture();
        let (registry, key, _run) = registry_with_a_run();
        let log = RequestLog::new(registry).slow_after(Duration::from_secs(60));
        let router = app(
            log,
            get(|| async { pending_body() }).post(|| async {
                tokio::time::sleep(Duration::from_secs(90)).await;
                "late"
            }),
        );

        let stream = router
            .clone()
            .oneshot(Request::get("/mcp").body(Body::empty()).expect("a request"))
            .await
            .expect("served");
        tokio::time::sleep(Duration::from_secs(90)).await;
        assert!(
            kept.lines().iter().all(|l| !l.contains("WARN")),
            "an open stream warned: {:?}",
            kept.lines()
        );

        let held = router
            .oneshot(tool_call("recall", &key))
            .await
            .expect("served");
        let _ = axum::body::to_bytes(held.into_body(), 1 << 20).await;
        drop(stream);

        let warnings: Vec<String> = kept
            .lines()
            .into_iter()
            .filter(|l| l.contains("WARN"))
            .collect();
        assert_eq!(
            warnings.len(),
            1,
            "one warning, the POST's: {:?}",
            kept.lines()
        );
        assert!(warnings[0].contains("http_method=POST"), "{}", warnings[0]);
    }

    /// A body that never yields and never ends: a call whose answer is still
    /// being made, or a stream nothing is writing to.
    fn pending_body() -> Body {
        struct Pending;
        impl http_body::Body for Pending {
            type Data = axum::body::Bytes;
            type Error = std::io::Error;
            fn poll_frame(
                self: std::pin::Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>>
            {
                std::task::Poll::Pending
            }
        }
        Body::new(Pending)
    }
}

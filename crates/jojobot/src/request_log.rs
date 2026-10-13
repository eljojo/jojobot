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

use std::pin::Pin;
use std::sync::Arc;
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
    named: Named,
    started: Instant,
    status: Option<StatusCode>,
    finished: bool,
    slow: Option<tokio::task::JoinHandle<()>>,
}

impl Ticket {
    fn open(named: Named, slow_after: Duration) -> Self {
        let for_warning = named.clone();
        let started = Instant::now();
        let slow = tokio::spawn(async move {
            tokio::time::sleep(slow_after).await;
            tracing::warn!(
                http_method = %for_warning.http_method,
                mcp_method = for_warning.mcp_method.as_deref(),
                tool = for_warning.tool.as_deref(),
                bot = for_warning.bot.as_deref(),
                run = for_warning.run.as_deref(),
                open_ms = started.elapsed().as_millis() as u64,
                "mcp request still open"
            );
        });
        Self {
            named,
            started,
            status: None,
            finished: false,
            slow: Some(slow),
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(slow) = self.slow.take() {
            slow.abort();
        }
        let named = &self.named;
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

pub async fn log_request(State(log): State<RequestLog>, req: Request, next: Next) -> Response {
    let mut named = Named {
        http_method: req.method().as_str().to_string(),
        ..Named::default()
    };
    let req = if req.method() == Method::POST {
        let (parts, body) = req.into_parts();
        let Ok(bytes) = axum::body::to_bytes(body, READ_AT_MOST).await else {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        };
        name_the_call(&bytes, &log.registry, &mut named);
        Request::from_parts(parts, Body::from(bytes))
    } else {
        req
    };

    let mut ticket = Ticket::open(named, log.slow_after);
    let response = next.run(req).await;
    ticket.status = Some(response.status());
    let (parts, inner) = response.into_parts();
    ticket.finished = http_body::Body::is_end_stream(&inner);
    Response::from_parts(parts, Body::new(Watched { inner, ticket }))
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

    fn app(log: RequestLog, answer: axum::routing::MethodRouter) -> Router {
        Router::new()
            .route("/mcp", answer)
            .layer(middleware::from_fn_with_state(log, log_request))
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

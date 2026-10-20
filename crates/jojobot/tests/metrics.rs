//! **The metrics listener, as a scraper meets it.**
//!
//! These cases start the real binary, because the second listener is bound in
//! `main` and nowhere else: a router built in the test would prove the router
//! and not that the binary serves it. Each scrape is read with a strict
//! OpenMetrics parser, so a line a scraper would drop fails the case.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;

use jojobot_adapters::testing::free_port;
use jojobot_mcp::observe::CallEnd;
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A running binary and what it printed up to its serving line.
struct Running {
    child: tokio::process::Child,
    http_port: u16,
    printed: Vec<String>,
    _scratch: Scratch,
}

/// **The real binary over a store of its own**, with `metrics_port` set as its
/// metrics address when there is one. Returns once the serving line is printed.
async fn running(name: &str, metrics_port: Option<u16>) -> Running {
    let state_dir = jojobot_adapters::testing::scratch_path(name);
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut command = tokio::process::Command::new(&binary);
    command
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", free_port().to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env_remove("JOJOBOT_METRICS_BIND")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(port) = metrics_port {
        command.env("JOJOBOT_METRICS_BIND", format!("127.0.0.1:{port}"));
    }
    let mut child = command.spawn().expect("the binary runs");
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut printed = Vec::new();
    let served = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            let serving = line.contains("serving http://");
            printed.push(line);
            if serving {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        served,
        "the binary never reached its serving line: {printed:#?}"
    );
    // The rest of what it prints is not read; draining it keeps a full pipe
    // from stopping the server.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Running {
        child,
        http_port,
        printed,
        _scratch: scratch,
    }
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("an answer in JSON ({e}): {text}"))
}

/// Every sample of a scrape, as its family's name and its labels.
fn samples(text: &str) -> Vec<(String, BTreeMap<String, String>)> {
    let exposition = openmetrics_parser::openmetrics::parse_openmetrics(text)
        .unwrap_or_else(|e| panic!("a scrape a strict parser reads ({e}):\n{text}"));
    let mut all = Vec::new();
    for (name, family) in &exposition.families {
        for sample in family.iter_samples() {
            let labels = sample
                .get_labelset()
                .unwrap_or_else(|e| panic!("labels that fit their family ({e})"))
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            all.push((name.clone(), labels));
        }
    }
    all
}

fn has(
    samples: &[(String, BTreeMap<String, String>)],
    family: &str,
    labels: &[(&str, &str)],
) -> bool {
    samples.iter().any(|(name, held)| {
        name == family
            && labels
                .iter()
                .all(|(k, v)| held.get(*k).map(String::as_str) == Some(*v))
    })
}

#[tokio::test]
async fn a_scrape_reads_the_numbers_of_the_calls_made_and_the_main_listener_has_no_metrics() {
    let metrics_port = free_port();
    let server = running("jojobot-metrics-on", Some(metrics_port)).await;
    assert!(
        server.printed.iter().any(|line| line.contains(&format!(
            "serving metrics http://127.0.0.1:{metrics_port}/metrics"
        ))),
        "the metrics listener is bound before the serving line: {:#?}",
        server.printed
    );

    let surface = jojobot_exercise::surface::Surface::connect(&format!(
        "http://127.0.0.1:{}/mcp",
        server.http_port
    ))
    .await
    .expect("connecting to the real binary");
    let booted = parse(
        &surface
            .call("start_here", json!({"bot": "assistant", "brief": true}))
            .await,
    );
    let sid = booted
        .pointer("/session/sid")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("a boot hands back a sid: {booted}"))
        .to_string();
    let run = booted
        .pointer("/session/run")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("a boot names its run: {booted}"))
        .to_string();
    let made = parse(
        &surface
            .call(
                "add_entity",
                json!({
                    "kind": "person", "handle": "milhouse", "name": "Milhouse",
                    "source": "user-named", "sid": sid,
                }),
            )
            .await,
    );
    assert_ne!(made["status"], "blocked", "{made}");
    // A bot a caller made: its box is counted, and never under its name.
    let colleague = parse(
        &surface
            .call(
                "add_entity",
                json!({
                    "kind": "bot", "handle": "nelson", "name": "Nelson",
                    "source": "user-named", "sid": sid,
                }),
            )
            .await,
    );
    assert_ne!(colleague["status"], "blocked", "{colleague}");
    let captured = parse(
        &surface
            .call(
                "capture",
                json!({
                    "subject": "person:milhouse", "content": "lends Bart a quartzite marble",
                    "provenance": "testimony", "sid": sid,
                }),
            )
            .await,
    );
    assert_ne!(captured["status"], "blocked", "{captured}");
    // A capture on a thing nobody made is refused, and the call is counted with
    // the word that says who fixes it.
    let refused = parse(
        &surface
            .call(
                "capture",
                json!({
                    "subject": "person:nelson", "content": "says haw-haw",
                    "provenance": "testimony", "sid": sid,
                }),
            )
            .await,
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    let refused_with = refused["fix_by"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal names who fixes it: {refused}"))
        .to_string();
    let posted = parse(
        &surface
            .call(
                "post_message",
                json!({"to": "assistant", "subject": "a note", "body": "the marble is back", "sid": sid}),
            )
            .await,
    );
    let message = posted["id"]
        .as_str()
        .unwrap_or_else(|| panic!("a post names its message: {posted}"))
        .to_string();
    let verbs: BTreeSet<String> = surface
        .tools_for_the_model()
        .await
        .expect("the verb list")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect();

    let client = reqwest::Client::new();
    let main = client
        .get(format!("http://127.0.0.1:{}/metrics", server.http_port))
        .send()
        .await
        .expect("the main listener answers");
    assert_eq!(main.status(), 404, "the main listener serves no metrics");

    let scraped = client
        .get(format!("http://127.0.0.1:{metrics_port}/metrics"))
        .send()
        .await
        .expect("the metrics listener answers");
    assert_eq!(scraped.status(), 200);
    let content_type = scraped
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("application/openmetrics-text"),
        "{content_type}"
    );
    let text = scraped.text().await.expect("a body");
    let all = samples(&text);

    // **Every label value is from a closed set**, and nothing a caller wrote
    // or the store holds rides on one.
    let ends: BTreeSet<&str> = CallEnd::ALL.iter().map(|end| end.as_str()).collect();
    let words_written = [
        "milhouse",
        "nelson",
        "quartzite",
        "marble",
        sid.as_str(),
        run.as_str(),
        message.as_str(),
    ];
    for (family, labels) in &all {
        for (key, value) in labels {
            assert!(
                !words_written.iter().any(|word| value.contains(word)),
                "{family}{{{key}={value:?}}} carries something a caller wrote or the store holds"
            );
            let closed = match key.as_str() {
                "route" => {
                    value == "/healthz"
                        || value == "/.well-known/oauth-protected-resource"
                        || value.starts_with("/mcp")
                }
                "status" => value.len() == 3 && value.chars().all(|c| c.is_ascii_digit()),
                "verb" => verbs.contains(value) || value == "unserved",
                "outcome" => ends.contains(value.as_str()),
                "kind" => {
                    jojobot_domain::memory::kinds::SHIPPED.contains(&value.as_str())
                        || value == "other"
                }
                "mailbox" => {
                    jojobot_mcp::seed::SHIPPED_BOTS
                        .iter()
                        .any(|(handle, _)| handle.strip_prefix("bot:") == Some(value.as_str()))
                        || value == "operator"
                        || value == "other"
                }
                "state" => matches!(value.as_str(), "new" | "read" | "processed"),
                "read" => matches!(value.as_str(), "sessions" | "mail" | "things"),
                "version" => value == jojobot_mcp::observe::VERSION,
                "build" => value == jojobot_mcp::observe::BUILD,
                "le" => value == "+Inf" || value.parse::<f64>().is_ok(),
                _ => false,
            };
            assert!(
                closed,
                "{family}{{{key}={value:?}}} is not from a closed set:\n{text}"
            );
        }
    }

    assert!(has(&all, "jojobot_build", &[]), "{text}");
    assert!(has(&all, "jojobot_uptime_seconds", &[]), "{text}");
    assert!(
        has(
            &all,
            "jojobot_mcp_calls",
            &[("verb", "add_entity"), ("outcome", "answered")]
        ),
        "{text}"
    );
    assert!(
        has(
            &all,
            "jojobot_mcp_calls",
            &[("verb", "capture"), ("outcome", "answered")]
        ),
        "{text}"
    );
    assert!(
        has(
            &all,
            "jojobot_mcp_calls",
            &[("verb", "capture"), ("outcome", refused_with.as_str())]
        ),
        "{text}"
    );
    assert!(
        has(
            &all,
            "jojobot_mcp_call_duration_seconds",
            &[("verb", "capture")]
        ),
        "{text}"
    );
    assert!(
        has(&all, "jojobot_http_requests", &[("status", "200")]),
        "{text}"
    );
    assert!(
        has(&all, "jojobot_http_request_duration_seconds", &[]),
        "{text}"
    );
    assert!(has(&all, "jojobot_things", &[("kind", "person")]), "{text}");
    assert!(
        has(
            &all,
            "jojobot_mail_messages",
            &[("mailbox", "assistant"), ("state", "new")]
        ),
        "{text}"
    );
    assert!(
        has(
            &all,
            "jojobot_mail_messages",
            &[("mailbox", "other"), ("state", "new")]
        ),
        "a box a caller made is counted as other: {text}"
    );
    assert!(has(&all, "jojobot_sessions_live", &[]), "{text}");
    for read in ["sessions", "mail", "things"] {
        assert!(
            has(&all, "jojobot_scrape_read_ok", &[("read", read)]),
            "{text}"
        );
    }
    assert!(
        text.lines()
            .filter(|line| line.starts_with("jojobot_scrape_read_ok"))
            .all(|line| line.ends_with(" 1")),
        "every read a scrape made answered: {text}"
    );

    surface.finish().await;
    let mut child = server.child;
    let _ = child.start_kill();
    let _ = child.wait().await;
}

#[tokio::test]
async fn with_no_metrics_address_the_binary_binds_no_metrics_listener() {
    let server = running("jojobot-metrics-off", None).await;
    assert!(
        !server
            .printed
            .iter()
            .any(|line| line.contains("serving metrics")),
        "no metrics listener is bound: {:#?}",
        server.printed
    );
    let main = reqwest::get(format!("http://127.0.0.1:{}/metrics", server.http_port))
        .await
        .expect("the main listener answers");
    assert_eq!(main.status(), 404);
    let mut child = server.child;
    let _ = child.start_kill();
    let _ = child.wait().await;
}

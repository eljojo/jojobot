//! **The app API through the served router**: an app the operator registered
//! records a deploy event with its own token, and nothing else.
//!
//! Every case sends HTTP to a listening server built by `build_app`, so a route
//! that was never mounted, a layer that was never added and a gate in the wrong
//! order all fail here and nowhere a function-level test could see.

use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::app_api::Apps;
use jojobot::{AppState, build_app};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::app::{AppPrincipal, AppSpec, hash_token, mint_token};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity};
use tokio_util::sync::CancellationToken;

const THREAD: &str = "thread:deploys-alpha";

fn spec(name: &str, revoked: bool) -> AppSpec {
    AppSpec {
        name: name.into(),
        operation: "record_deploy_event".into(),
        thread: THREAD.into(),
        kind: "event".into(),
        keys: vec!["commit".into(), "host".into(), "unit".into()],
        revoked,
    }
}

struct Served {
    addr: SocketAddr,
    memory: Arc<dyn Memory>,
    /// The store under the served stack, to switch a fault on.
    store: Arc<InMemoryMemory>,
    _stop: CancellationToken,
}

/// Serve the router with the given apps. `validator` turns the `/mcp` token
/// check on, with a validator that holds no keys.
async fn serve(apps: Vec<AppPrincipal>, auth_on: bool) -> Served {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store = Arc::new(InMemoryMemory::booted());
    let indexed = Arc::new(IndexedMemory::new(store.clone()).expect("the search index opens"));
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    let memory: Arc<dyn Memory> = indexed.clone();
    let mailboxes: Arc<dyn jojobot_domain::mailbox::Mailboxes> =
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner());
    let validator = auth_on.then(|| {
        Arc::new(jojobot::auth::Validator::from_keys(
            "https://issuer.example",
            "https://resource.example/mcp",
            std::collections::HashMap::new(),
        ))
    });
    let state = AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: auth_on.then(|| "https://issuer.example".to_string()),
        validator,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: memory.clone(),
        search,
        mailboxes,
        sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        requests: None,
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
        clock: jojobot_domain::clock::Clock::default(),
        push: None,
        quiet_after: None,
        apps: Apps::new(apps),
    };
    jojobot_mcp::seed::ensure_default_identity(&state.memory, &state.mailboxes).await;
    let _ = jojobot_mcp::seed::ensure_shipped_types(&state.memory).await;
    let _ = jojobot_mcp::seed::ensure_kinds(&state.memory).await;
    memory
        .add_entity(NewEntity::new(
            EntityId(THREAD.into()),
            "Deploys Alpha",
            "fixture",
        ))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());
    let shutdown = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await
            .unwrap();
    });
    Served {
        addr,
        memory,
        store,
        _stop: ct,
    }
}

/// An app with a fresh token: the principal the server holds and the token the
/// app sends.
fn enrolled(name: &str, revoked: bool) -> (AppPrincipal, String) {
    let token = mint_token();
    let app = AppPrincipal::from_spec(&spec(name, revoked), &hash_token(&token)).expect("a grant");
    (app, token)
}

async fn post_event(
    served: &Served,
    token: &str,
    body: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let response = reqwest::Client::new()
        .post(format!("http://{}/app/v1/deploy-events", served.addr))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("the server answers");
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)),
    )
}

fn deploy(attempt: &str, host: &str) -> serde_json::Value {
    serde_json::json!({
        "attempt_id": attempt,
        "fields": { "commit": "abc1234", "host": host },
    })
}

async fn events(served: &Served) -> Vec<jojobot_domain::memory::Entity> {
    served
        .memory
        .list_entities(Some(EntityKind::EVENT))
        .await
        .expect("lists")
}

#[tokio::test]
async fn an_app_records_one_event_and_a_fresh_read_finds_it() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;

    let (status, receipt) = post_event(&served, &token, deploy("run-1", "host-alpha")).await;
    assert_eq!(status, 201, "{receipt}");
    assert_eq!(receipt["repeat"], false);

    let found = events(&served).await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].parent.as_ref().map(|p| p.as_str()), Some(THREAD));
    assert_eq!(receipt["handle"], found[0].id.to_string());

    let facts = served.memory.recall(&found[0].id).await.expect("recall");
    let stamp = facts[0].app_stamp.as_ref().expect("the app's mark");
    assert_eq!(stamp.app, "springfield-ci");
    assert_eq!(stamp.grant, "record_deploy_event");
    let fields = served.memory.fields(&found[0].id).await.expect("fields");
    assert_eq!(fields.get("host").map(String::as_str), Some("host-alpha"));
    assert_eq!(fields.get("commit").map(String::as_str), Some("abc1234"));
}

#[tokio::test]
async fn a_retry_answers_the_first_receipt_and_another_body_is_a_conflict() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;

    let (_, first) = post_event(&served, &token, deploy("run-1", "host-alpha")).await;
    let (status, again) = post_event(&served, &token, deploy("run-1", "host-alpha")).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["repeat"], true);
    assert_eq!(again["handle"], first["handle"]);
    assert_eq!(again["address"], first["address"]);

    let (status, _) = post_event(&served, &token, deploy("run-1", "host-beta")).await;
    assert_eq!(status, 409);
    assert_eq!(
        events(&served).await.len(),
        1,
        "neither wrote a second event"
    );
}

/// **A write that landed is never reported as failed** (rule 130). The fold behind
/// the store can fail to refresh after the event is stored; the app is told the
/// event landed, with the receipt an ordinary success gives, and a retry of the
/// attempt answers that receipt again. A body the grant refuses still answers 422.
#[tokio::test]
async fn an_event_that_landed_is_answered_as_landed_when_the_fold_refresh_fails() {
    let (app, token) = enrolled("alpha-deployer", false);
    let served = serve(vec![app], false).await;

    let (status, ordinary) = post_event(&served, &token, deploy("run-0", "host-alpha")).await;
    assert_eq!(status, 201, "{ordinary}");

    served.store.land_app_events_behind();
    let (status, behind) = post_event(&served, &token, deploy("run-1", "host-beta")).await;
    assert_eq!(status, 201, "{behind}");
    assert_eq!(behind["repeat"], false, "{behind}");
    assert_eq!(
        behind.as_object().map(|o| o.keys().collect::<Vec<_>>()),
        ordinary.as_object().map(|o| o.keys().collect::<Vec<_>>()),
        "the same receipt shape as an ordinary success: {behind}"
    );
    let stored = events(&served).await;
    assert_eq!(stored.len(), 2, "the event is stored");
    assert!(
        stored.iter().any(|e| e.id.to_string() == behind["handle"]),
        "the receipt names the stored event: {behind}"
    );

    let (status, again) = post_event(&served, &token, deploy("run-1", "host-beta")).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["repeat"], true);
    assert_eq!(again["handle"], behind["handle"]);
    assert_eq!(again["address"], behind["address"]);

    let (status, refused) = post_event(
        &served,
        &token,
        serde_json::json!({"attempt_id": "run-2", "fields": {"commit": "not hex"}}),
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        events(&served).await.len(),
        2,
        "the refused body wrote nothing"
    );
}

#[tokio::test]
async fn a_redeploy_of_the_same_commit_to_the_same_host_is_a_second_event() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;

    for attempt in ["run-1", "run-2"] {
        let (status, body) = post_event(&served, &token, deploy(attempt, "host-alpha")).await;
        assert_eq!(status, 201, "{body}");
    }
    assert_eq!(events(&served).await.len(), 2);
}

#[tokio::test]
async fn four_events_under_one_thread_all_land() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;

    for n in 0..4 {
        let (status, body) =
            post_event(&served, &token, deploy(&format!("run-{n}"), "host-alpha")).await;
        assert_eq!(status, 201, "{body}");
    }
    assert_eq!(events(&served).await.len(), 4);
}

#[tokio::test]
async fn a_revoked_an_unknown_and_a_missing_token_write_nothing() {
    let (revoked, revoked_token) = enrolled("shut-off", true);
    let (live, live_token) = enrolled("springfield-ci", false);
    let served = serve(vec![revoked, live], false).await;

    // The positive the refusals rest on: the live app writes.
    let (status, body) = post_event(&served, &live_token, deploy("run-1", "host-alpha")).await;
    assert_eq!(status, 201, "{body}");

    for token in [revoked_token.as_str(), mint_token().as_str(), "not-a-token"] {
        let (status, _) = post_event(&served, token, deploy("run-2", "host-alpha")).await;
        assert_eq!(status, 401, "{token}");
    }
    let response = reqwest::Client::new()
        .post(format!("http://{}/app/v1/deploy-events", served.addr))
        .body(deploy("run-3", "host-alpha").to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401, "no bearer at all");
    assert_eq!(events(&served).await.len(), 1);
}

#[tokio::test]
async fn a_write_outside_the_grant_writes_nothing() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;

    // **All three are the one branch: a key the grant does not list.** A guarded
    // key and a key that makes a thing fall due can never be listed, because the
    // apps file that lists one is refused when the server reads it (the config
    // cases hold that), so a request that sends one is outside the grant for
    // that reason and no request-time branch of its own judges it.
    let outside = [
        // A key nobody has said anything about.
        serde_json::json!({"attempt_id": "a", "fields": {"commit": "abc1234", "note": "x"}}),
        // A guarded key, which no grant lists.
        serde_json::json!({"attempt_id": "b", "fields": {"commit": "abc1234", "reports_to": "bot:assistant"}}),
        // A key that makes a thing fall due, which no grant lists.
        serde_json::json!({"attempt_id": "c", "fields": {"commit": "abc1234", "runs_out": "2026-12-01"}}),
    ];
    for body in outside {
        let (status, answer) = post_event(&served, &token, body.clone()).await;
        assert_eq!(status, 403, "{body}: {answer}");
    }
    // A shape the server does not name: another thread, another kind.
    for body in [
        serde_json::json!({"attempt_id": "d", "thread": "thread:contract-deploys", "fields": {"commit": "abc1234"}}),
        serde_json::json!({"attempt_id": "e", "kind": "person", "fields": {"commit": "abc1234"}}),
    ] {
        let (status, _) = post_event(&served, &token, body.clone()).await;
        assert_eq!(status, 422, "{body}");
    }
    // A value that is not its shape.
    let (status, _) = post_event(
        &served,
        &token,
        serde_json::json!({"attempt_id": "f", "fields": {"commit": "not hex"}}),
    )
    .await;
    assert_eq!(status, 422);
    // **The two checks after the grant**, each reached by a request that passes
    // every check before it: no keys at all, and a key that is listed and holds
    // its shape under an attempt id that is no id.
    for (what, body) in [
        (
            "no keys",
            serde_json::json!({"attempt_id": "g", "fields": {}}),
        ),
        (
            "an empty attempt id",
            serde_json::json!({"attempt_id": "", "fields": {"commit": "abc1234"}}),
        ),
    ] {
        let (status, answer) = post_event(&served, &token, body.clone()).await;
        assert_eq!(status, 422, "{what}: {answer}");
    }
    assert!(events(&served).await.is_empty(), "nothing was written");
    // The positive that makes the nothing above mean something: the same grant
    // takes a request with a key it lists, a shape it holds and an id.
    let (status, answer) = post_event(&served, &token, deploy("h", "web-1")).await;
    assert_eq!(status, 201, "{answer}");
    assert_eq!(events(&served).await.len(), 1, "that one was written");
}

async fn mcp_status(served: &Served, bearer: Option<&str>) -> reqwest::StatusCode {
    let mut request = reqwest::Client::new()
        .post(format!("http://{}/mcp", served.addr))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body("{}");
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
    request.send().await.unwrap().status()
}

#[tokio::test]
async fn an_app_token_is_refused_at_mcp_with_auth_off() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;

    // The positive: with auth off the surface is reached by a caller with no
    // app token, so the refusal below is the app layer's and nothing else's.
    assert_ne!(mcp_status(&served, None).await, 401);
    assert_ne!(
        mcp_status(&served, Some("a-bearer-no-one-checks")).await,
        401
    );
    assert_eq!(mcp_status(&served, Some(&token)).await, 401);
}

#[tokio::test]
async fn an_app_token_is_refused_at_mcp_with_auth_on() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], true).await;
    assert_eq!(mcp_status(&served, Some(&token)).await, 401);
}

#[tokio::test]
async fn the_app_route_is_not_mounted_when_no_app_is_registered() {
    let served = serve(Vec::new(), false).await;
    let (status, _) = post_event(&served, &mint_token(), deploy("run-1", "host-alpha")).await;
    assert_eq!(status, 404);
}

/// **An app's event is dated in the instance's zone**, the day the operator's pages
/// read, and not in UTC. The widest zones are on another day than UTC for part of every
/// day, so the one that is now falls on one day there and another in UTC. The instance
/// that names no zone reads UTC, and its event carries the UTC day, which is the pair.
#[tokio::test]
async fn an_apps_event_is_dated_in_the_instances_zone() {
    let clock = jojobot_domain::clock::Clock::default();
    let utc = clock.today_in(&jiff::tz::TimeZone::UTC);
    let (zone, there) = ["Etc/GMT+12", "Pacific/Kiritimati"]
        .into_iter()
        .map(|zone| {
            let tz = jiff::tz::TimeZone::get(zone).expect("a zone");
            (zone, clock.today_in(&tz))
        })
        .find(|(_, there)| *there != utc)
        .expect("one of the widest zones is on another day than UTC");

    // The instance names its zone.
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;
    let instance = EntityId("topic:instance".into());
    served
        .memory
        .add_entity(NewEntity::new(
            instance.clone(),
            "This instance",
            "the operator",
        ))
        .await
        .expect("the instance's record is written")
        .written()
        .expect("nothing collides with it");
    let mut zoned = jojobot_domain::memory::NewFact::about(
        instance,
        "where the operator lives",
        jiff::civil::Date::constant(2026, 3, 5),
    );
    zoned.fields = [("timezone".to_string(), zone.to_string())]
        .into_iter()
        .collect();
    zoned.sets = zoned.fields.keys().cloned().collect();
    served
        .memory
        .capture(zoned)
        .await
        .expect("the zone is written");

    let (status, receipt) = post_event(&served, &token, deploy("run-1", "host-alpha")).await;
    assert_eq!(status, 201, "{receipt}");
    let found = events(&served).await;
    let facts = served.memory.recall(&found[0].id).await.expect("recall");
    assert_eq!(
        facts[0].recorded_at.to_string(),
        there.to_string(),
        "the event is dated in the instance's zone ({zone})"
    );
    assert_ne!(facts[0].recorded_at, utc, "and not in UTC");

    // An instance that names no zone is dated in UTC.
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;
    let (status, receipt) = post_event(&served, &token, deploy("run-1", "host-alpha")).await;
    assert_eq!(status, 201, "{receipt}");
    let found = events(&served).await;
    let facts = served.memory.recall(&found[0].id).await.expect("recall");
    assert_eq!(facts[0].recorded_at, utc, "no zone, so the UTC day");
}

/// Send raw bytes to the app route with the given headers, so a case can shape
/// what `post_event` always gets right.
async fn raw_post(served: &Served, headers: &[(&str, String)], body: &str) -> reqwest::StatusCode {
    let mut request = reqwest::Client::new()
        .post(format!("http://{}/app/v1/deploy-events", served.addr))
        .body(body.to_string());
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    request.send().await.expect("the server answers").status()
}

#[tokio::test]
async fn a_body_that_is_empty_or_not_json_is_refused_with_a_stated_status_and_writes_nothing() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;
    let auth = ("authorization", format!("Bearer {token}"));
    let json = ("content-type", "application/json".to_string());

    // The positive the refusals rest on: the same headers carry a good body.
    let good = deploy("run-1", "host-alpha").to_string();
    assert_eq!(
        raw_post(&served, &[auth.clone(), json.clone()], &good).await,
        201
    );

    assert_eq!(
        raw_post(&served, &[auth.clone(), json.clone()], "").await,
        400,
        "empty body"
    );
    assert_eq!(
        raw_post(&served, &[auth.clone(), json.clone()], "not json at all").await,
        400,
        "a body that is not JSON"
    );
    assert_eq!(
        raw_post(&served, &[auth.clone(), json.clone()], "[1, 2]").await,
        422,
        "JSON of the wrong shape"
    );
    assert_eq!(
        raw_post(&served, std::slice::from_ref(&auth), &good).await,
        415,
        "a good body without a JSON content type"
    );
    assert_eq!(events(&served).await.len(), 1, "only the good body wrote");
}

#[tokio::test]
async fn a_request_with_two_authorization_headers_is_refused_whatever_their_order() {
    let (app, token) = enrolled("springfield-ci", false);
    let served = serve(vec![app], false).await;
    let good = ("authorization", format!("Bearer {token}"));
    let junk = ("authorization", "Bearer not-a-token".to_string());
    let json = ("content-type", "application/json".to_string());
    let body = deploy("run-1", "host-alpha").to_string();

    // The positive the refusals rest on: one valid header lands.
    assert_eq!(
        raw_post(&served, &[good.clone(), json.clone()], &body).await,
        201
    );
    let first_good = raw_post(&served, &[good.clone(), junk.clone(), json.clone()], &body).await;
    let first_junk = raw_post(&served, &[junk.clone(), good.clone(), json.clone()], &body).await;
    let twice_good = raw_post(&served, &[good.clone(), good, json.clone()], &body).await;
    assert_eq!(first_good, 400, "a valid token first does not decide");
    assert_eq!(
        first_junk, 400,
        "a valid token behind a bad one does not rescue it"
    );
    assert_eq!(
        twice_good, 400,
        "two copies of one valid header are two headers"
    );
    assert_eq!(
        events(&served).await.len(),
        1,
        "only the single header wrote"
    );

    // A bad single header and no header stay what they were.
    assert_eq!(raw_post(&served, &[junk, json.clone()], &body).await, 401);
    assert_eq!(raw_post(&served, &[json], &body).await, 401);
}

async fn mcp_status_with(served: &Served, headers: Vec<(&str, String)>) -> reqwest::StatusCode {
    let mut request = reqwest::Client::new()
        .post(format!("http://{}/mcp", served.addr))
        .header("accept", "application/json, text/event-stream")
        .body("{}");
    for (name, value) in headers {
        request = request.header(name, value);
    }
    request.send().await.unwrap().status()
}

/// The tool surface reads the same credential, so two headers are refused there
/// as well, with auth off and with it on.
#[tokio::test]
async fn a_request_with_two_authorization_headers_is_refused_at_mcp() {
    for auth_on in [false, true] {
        let (app, _) = enrolled("springfield-ci", false);
        let served = serve(vec![app], auth_on).await;
        let json = ("content-type", "application/json".to_string());
        let junk = || ("authorization", "Bearer not-a-token".to_string());
        assert_eq!(
            mcp_status_with(&served, vec![json.clone(), junk(), junk()]).await,
            400,
            "auth_on={auth_on}"
        );
        // The same request with one header is not a 400: it is judged as before.
        assert_ne!(
            mcp_status_with(&served, vec![json.clone(), junk()]).await,
            400,
            "auth_on={auth_on}"
        );
    }
}

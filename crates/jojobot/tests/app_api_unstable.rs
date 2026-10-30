//! **The unstable tier of the app API through the served router**: an app calls an
//! operation of the registry by name, as itself, under its grant.
//!
//! Every case sends HTTP to a listening server built by `build_app_operating`, so
//! a route that was never mounted or a call that skips the grant fails here.
//! This is the proof table of the route: a value rule is a row here, and the
//! journey of watchbot's calls is a story.

use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::app_api::Apps;
use jojobot::{AppState, build_app_operating};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::app::operations::OperationSpec;
use jojobot_domain::app::{AppPrincipal, AppSpec, hash_token, mint_token};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_mcp::app_ops::{Answer, Entry, Registry};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn op(operation: &str, kinds: &[&str], keys: &[&str]) -> OperationSpec {
    OperationSpec {
        operation: operation.into(),
        kinds: kinds.iter().map(|s| s.to_string()).collect(),
        keys: keys.iter().map(|s| s.to_string()).collect(),
    }
}

/// An app that holds only unstable-tier operations.
fn enrolled(name: &str, operations: Vec<OperationSpec>) -> (AppPrincipal, String) {
    jojobot_domain::memory::kinds::load_shipped();
    let token = mint_token();
    let spec = AppSpec {
        name: name.into(),
        operation: String::new(),
        thread: String::new(),
        kind: String::new(),
        keys: vec![],
        operations,
        revoked: false,
    };
    let app = AppPrincipal::from_spec(&spec, &hash_token(&token)).expect("a grant");
    (app, token)
}

/// The grant the watcher of the line is given.
fn watcher() -> (AppPrincipal, String) {
    enrolled(
        "springfield-watch",
        vec![
            op("capture", &["role", "work"], &["watch_state", "watch_seen"]),
            op("post_message", &[], &[]),
        ],
    )
}

struct Served {
    addr: SocketAddr,
    memory: Arc<dyn Memory>,
    mailboxes: Arc<dyn jojobot_domain::mailbox::Mailboxes>,
    _stop: CancellationToken,
}

async fn serve(apps: Vec<AppPrincipal>, registry: Registry) -> Served {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store = Arc::new(InMemoryMemory::booted());
    let indexed = Arc::new(IndexedMemory::new(store).expect("the search index opens"));
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    let memory: Arc<dyn Memory> = indexed.clone();
    let mailboxes: Arc<dyn jojobot_domain::mailbox::Mailboxes> =
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner());
    let state = AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: None,
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: memory.clone(),
        search,
        mailboxes: mailboxes.clone(),
        sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        requests: None,
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
        clock: jojobot_domain::clock::Clock::default(),
        push: None,
        quiet_after: None,
        apps: Apps::new(apps),
        metrics: None,
    };
    jojobot_mcp::seed::ensure_default_identity(&state.memory, &state.mailboxes).await;
    let _ = jojobot_mcp::seed::ensure_shipped_types(&state.memory).await;
    let _ = jojobot_mcp::seed::ensure_kinds(&state.memory).await;
    for (handle, name) in [
        ("role:alpha", "Alpha"),
        ("work:beta", "Beta"),
        ("thread:gamma", "Gamma"),
        ("person:homer", "Homer"),
    ] {
        memory
            .add_entity(NewEntity::new(EntityId(handle.into()), name, "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    // **The operator is the person the instance's record names**, the one box an
    // app may post to.
    let instance = EntityId("topic:instance".into());
    if memory
        .recall(&instance)
        .await
        .map(|f| f.is_empty())
        .unwrap_or(true)
    {
        memory
            .add_entity(NewEntity::new(instance.clone(), "This instance", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let mut naming = NewFact::about(
        instance,
        "who the operator is",
        jiff::civil::Date::constant(2026, 3, 4),
    );
    naming.provenance = Provenance::Testimony;
    naming
        .fields
        .insert("operator".to_string(), "person:homer".to_string());
    naming.sets.insert("operator".to_string());
    memory.capture(naming).await.expect("the operator is named");
    let ct = CancellationToken::new();
    let app = build_app_operating(state, ct.child_token(), registry);
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
        mailboxes,
        _stop: ct,
    }
}

async fn call(served: &Served, token: &str, operation: &str, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(format!("http://{}/app/unstable/{operation}", served.addr))
        .header("authorization", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .expect("the server answers");
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

#[tokio::test]
async fn unstable_a_granted_capture_lands_with_the_apps_mark_and_no_bot_writer() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;

    let (status, view) = call(
        &served,
        &token,
        "capture",
        json!({"subject": "role:alpha", "content": "Seen at the gate.", "sets": {"watch_state": "ok"}}),
    )
    .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["subject"], "role:alpha");

    let facts = served
        .memory
        .recall(&EntityId("role:alpha".into()))
        .await
        .expect("recall");
    let landed = facts
        .iter()
        .find(|f| f.address().to_string() == view["address"])
        .expect("the claim the route named");
    let stamp = landed.app_stamp.as_ref().expect("the app's mark");
    assert_eq!(stamp.app, "springfield-watch");
    assert_eq!(stamp.grant, "capture");
    assert_eq!(landed.content, "Seen at the gate.");
    let fields = served
        .memory
        .fields(&EntityId("role:alpha".into()))
        .await
        .expect("fields");
    assert_eq!(fields.get("watch_state").map(String::as_str), Some("ok"));
}

#[tokio::test]
async fn unstable_an_operation_outside_the_grant_is_refused_naming_it_and_writes_nothing() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;

    let (status, refused) = call(
        &served,
        &token,
        "add_entity",
        json!({"kind": "thread", "handle": "beta", "name": "Beta", "source": "the watcher"}),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("add_entity"),
        "{refused}"
    );
    assert!(
        served
            .memory
            .recall(&EntityId("thread:beta".into()))
            .await
            .map(|f| f.is_empty())
            .unwrap_or(true),
        "nothing was made"
    );
}

#[tokio::test]
async fn unstable_a_key_outside_the_grant_or_one_no_app_may_set_is_refused_naming_it() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;
    for key in ["holder", "claimed_at", "agent", "runs_out"] {
        let (status, refused) = call(
            &served,
            &token,
            "capture",
            json!({"subject": "role:alpha", "content": "Taken.", "sets": {key: "x"}}),
        )
        .await;
        assert_eq!(status, 403, "{key}: {refused}");
        assert!(
            refused["error"].as_str().unwrap().contains(key),
            "{refused}"
        );
    }
    let facts = served
        .memory
        .recall(&EntityId("role:alpha".into()))
        .await
        .expect("recall");
    assert!(
        facts.iter().all(|f| f.content != "Taken."),
        "nothing was written"
    );
}

#[tokio::test]
async fn unstable_a_kind_outside_the_grant_and_an_argument_nobody_listed_are_refused() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, refused) = call(
        &served,
        &token,
        "capture",
        json!({"subject": "thread:gamma", "content": "Nope."}),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("thread"),
        "{refused}"
    );
    let (status, refused) = call(
        &served,
        &token,
        "capture",
        json!({"subject": "role:alpha", "content": "Nope.", "check_in": "2026-11-01"}),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("check_in"),
        "{refused}"
    );
}

#[tokio::test]
async fn unstable_an_app_posts_to_the_operators_box_from_itself_and_to_no_other() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;

    let (status, posted) = call(
        &served,
        &token,
        "post_message",
        json!({"to": "person:homer", "body": "The gate is open.", "subject": "gate"}),
    )
    .await;
    assert_eq!(status, 200, "{posted}");
    let message = served
        .mailboxes
        .read_message(
            &jojobot_domain::mailbox::MessageId(posted["id"].as_str().unwrap().into()),
            jojobot_domain::mailbox::Acting::unnamed(jiff::Timestamp::UNIX_EPOCH),
        )
        .await
        .expect("reads")
        .message;
    assert_eq!(message.sender, "app/springfield-watch");
    assert_eq!(message.sender_mail_waiting_at_send, None);
    assert_eq!(message.posted_by_session, None);

    let (status, refused) = call(
        &served,
        &token,
        "post_message",
        json!({"to": "bot:alpha", "body": "Hello."}),
    )
    .await;
    assert!(status == 403 || status == 409, "{status} {refused}");
}

#[tokio::test]
async fn unstable_set_charter_is_in_the_registry_and_no_grant_reaches_it() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, refused) = call(
        &served,
        &token,
        "set_charter",
        json!({"bot": "bot:alpha", "prose": "Mine."}),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("set_charter"),
        "{refused}"
    );
}

#[tokio::test]
async fn unstable_an_entry_added_to_the_registry_is_served_with_no_other_edit() {
    let (app, token) = enrolled("springfield-watch", vec![op("echo", &[], &[])]);
    let registry =
        Registry::shipped().with(Entry::new(
            "echo",
            |call| async move { Answer::Done(call.body) },
        ));
    let served = serve(vec![app], registry).await;

    let (status, answered) = call(&served, &token, "echo", json!({"said": "hello"})).await;
    assert_eq!(status, 200, "{answered}");
    assert_eq!(answered, json!({"said": "hello"}));

    // The same app with the same grant, on a registry that lacks the entry.
    let (app, token) = enrolled("springfield-watch", vec![op("echo", &[], &[])]);
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, _) = call(&served, &token, "echo", json!({"said": "hello"})).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn unstable_a_token_that_names_no_app_opens_nothing() {
    let (app, _) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, _) = call(&served, &mint_token(), "capture", json!({})).await;
    assert_eq!(status, 401);
}

/// The grant of an app that may create roles and works and set one key on them.
fn creator() -> (AppPrincipal, String) {
    enrolled(
        "springfield-maker",
        vec![op("add_entity", &["role", "work"], &["watch_state"])],
    )
}

#[tokio::test]
async fn unstable_a_creation_sets_a_granted_key_and_lands_with_the_apps_mark() {
    let (app, token) = creator();
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, view) = call(
        &served,
        &token,
        "add_entity",
        json!({"kind": "role", "handle": "delta", "name": "Delta", "source": "the maker",
               "sets": {"watch_state": "ok"}}),
    )
    .await;
    assert_eq!(status, 200, "{view}");
    let fields = served
        .memory
        .fields(&EntityId("role:delta".into()))
        .await
        .expect("fields");
    assert_eq!(fields.get("watch_state").map(String::as_str), Some("ok"));
}

#[tokio::test]
async fn unstable_a_creation_with_a_key_outside_the_grant_or_one_no_app_may_set_is_refused() {
    let (app, token) = creator();
    let served = serve(vec![app], Registry::shipped()).await;
    for (kind, handle, key) in [
        ("role", "epsilon", "holder"),
        ("role", "zeta", "agent"),
        ("work", "eta", "status"),
        ("work", "theta", "runs_out"),
        ("work", "iota", "promised_by"),
    ] {
        let (status, refused) = call(
            &served,
            &token,
            "add_entity",
            json!({"kind": kind, "handle": handle, "name": "Fixture", "source": "the maker",
                   "sets": {key: "2026-12-01"}}),
        )
        .await;
        assert_eq!(status, 403, "{key}: {refused}");
        assert!(
            refused["error"].as_str().unwrap().contains(key),
            "{refused}"
        );
        let made = served
            .memory
            .recall(&EntityId(format!("{kind}:{handle}")))
            .await
            .map(|f| !f.is_empty())
            .unwrap_or(false);
        assert!(!made, "{key}: nothing was made");
    }
}

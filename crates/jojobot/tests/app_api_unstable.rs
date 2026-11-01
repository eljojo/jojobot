//! **The unstable tier of the app API through the served router**: an app calls an
//! operation of the registry by name, as itself, under its grant.
//!
//! Every case sends HTTP to a listening server built by `build_app_operating`, so
//! a route that was never mounted or a call that skips the grant fails here.
//! This is the proof table of the route: a value rule is a row here, and the
//! journey of watchbot's calls is a story.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use jojobot::app_api::Apps;
use jojobot::{AppState, build_app_operating};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::app::operations::OperationSpec;
use jojobot_domain::app::{AppPrincipal, AppSpec, hash_token, mint_token};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::session::Sessions;
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
    sessions: Arc<jojobot_domain::session::testing::InMemorySessions>,
    _stop: CancellationToken,
}

/// **A store whose age read fails once told to**, every other read the fake's.
struct AgesFail {
    inner: InMemoryMemory,
    failing: Arc<AtomicBool>,
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for AgesFail {
    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(MemoryError::Store("ages unavailable".into()));
        }
        self.inner.field_ages(entity).await
    }
}

async fn serve(apps: Vec<AppPrincipal>, registry: Registry) -> Served {
    serve_over(apps, registry, Arc::new(AtomicBool::new(false))).await
}

async fn serve_over(
    apps: Vec<AppPrincipal>,
    registry: Registry,
    ages_failing: Arc<AtomicBool>,
) -> Served {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // **The views the build ships are served by the store, as in the binary**, so
    // an app that asks the view `ready` meets the one a bot meets.
    let supplied = jojobot_mcp::provisions();
    let store = Arc::new(jojobot_adapters::provisioned::Provisioned::new(
        AgesFail {
            inner: InMemoryMemory::booted().knowing(supplied.clone()),
            failing: ages_failing,
        },
        supplied,
    ));
    let indexed = Arc::new(IndexedMemory::new(store).expect("the search index opens"));
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
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
        sessions: sessions.clone(),
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
        sessions,
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

    // **A box that exists**, so the refusal below can only be the grant: to a
    // name no box answers to, the verb refuses first and the guard is never met.
    served
        .mailboxes
        .create_mailbox(
            &jojobot_domain::mailbox::MailboxName("alpha".into()),
            &EntityId("bot:alpha".into()),
            None,
        )
        .await
        .expect("a box for the bot");
    let (status, refused) = call(
        &served,
        &token,
        "post_message",
        json!({"to": "bot:alpha", "body": "Hello."}),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("operator"),
        "{refused}"
    );
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
async fn unstable_an_operation_nobody_has_is_a_404_naming_it_and_the_operations_the_app_was_granted()
 {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, refused) = call(&served, &token, "nonesuch", json!({})).await;
    assert_eq!(status, 404, "{refused}");
    let said = refused["error"].as_str().expect("an error text");
    assert!(said.contains("nonesuch"), "{refused}");
    assert!(
        said.contains("capture") && said.contains("post_message"),
        "the granted operations ride beside it: {refused}"
    );
    // The set is the grant's, not the registry's: add_entity is served and was not granted.
    assert!(
        !said.contains("add_entity"),
        "an operation the registry holds and the app was not granted stays off the list: {refused}"
    );
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

/// An app granted `recall` and nothing else.
fn reader() -> (AppPrincipal, String) {
    enrolled("springfield-reader", vec![op("recall", &[], &[])])
}

/// A work item with a status the reader can ask for.
async fn work_with_status(served: &Served, handle: &str, name: &str, status: &str) {
    served
        .memory
        .add_entity(NewEntity::new(EntityId(handle.into()), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let mut fact = NewFact::about(
        EntityId(handle.into()),
        "where it stands",
        jiff::civil::Date::constant(2026, 3, 5),
    );
    fact.fields.insert("status".to_string(), status.to_string());
    fact.sets.insert("status".to_string());
    served.memory.capture(fact).await.expect("status lands");
}

#[tokio::test]
async fn unstable_a_granted_recall_reads_a_card_by_handle_and_by_status() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "now").await;
    work_with_status(&served, "work:sigma", "Paint the fence", "next").await;

    let (status, by_handle) =
        call(&served, &token, "recall", json!({"subject": "work:omega"})).await;
    assert_eq!(status, 200, "{by_handle}");
    assert_eq!(
        by_handle["objects"][0]["handle"], "work:omega",
        "{by_handle}"
    );
    assert_eq!(
        by_handle["objects"][0]["fields"]["status"], "now",
        "the card's fields come back: {by_handle}"
    );

    let (status, by_status) = call(
        &served,
        &token,
        "recall",
        json!({"kind": "work", "where": "status = now"}),
    )
    .await;
    assert_eq!(status, 200, "{by_status}");
    let handles: Vec<&str> = by_status["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|o| o["handle"].as_str())
        .collect();
    assert!(handles.contains(&"work:omega"), "{by_status}");
    assert!(
        !handles.contains(&"work:sigma"),
        "a card at another status came back: {by_status}"
    );
}

#[tokio::test]
async fn unstable_a_recall_outside_the_grant_is_refused_naming_it() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, refused) = call(&served, &token, "recall", json!({"subject": "work:beta"})).await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("recall"),
        "{refused}"
    );
}

#[tokio::test]
async fn unstable_a_recall_argument_outside_the_input_is_refused_naming_it() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    for argument in ["follow", "charter", "history"] {
        let (status, refused) = call(
            &served,
            &token,
            "recall",
            json!({"subject": "work:beta", argument: "x"}),
        )
        .await;
        assert_eq!(status, 422, "{argument}: {refused}");
        assert!(
            refused.to_string().contains(argument),
            "the refusal does not name {argument}: {refused}"
        );
    }
}

#[tokio::test]
async fn unstable_a_recall_never_returns_a_run_and_still_reads_a_card_beside_it() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "now").await;
    let run = served
        .sessions
        .begin(jojobot_domain::session::NewSession {
            bot: EntityId("bot:assistant".into()),
            sid: jojobot_domain::session::Sid("rd01".into()),
            focus: "mowing".into(),
            started_at: jiff::Timestamp::now(),
            timezone: None,
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("a run is seeded");

    // The positive, in the same case: an unowned card reads, so an empty answer
    // below is not an app that reads nothing.
    let (status, card) = call(&served, &token, "recall", json!({"subject": "work:omega"})).await;
    assert_eq!(status, 200, "{card}");
    assert_eq!(card["objects"][0]["handle"], "work:omega", "{card}");

    for body in [
        json!({"kind": "session"}),
        json!({"subject": format!("session:{}", run.id.as_str())}),
    ] {
        let (status, runs) = call(&served, &token, "recall", body.clone()).await;
        assert!(
            status == 200 || status == 409,
            "{body}: neither an answer nor a refusal by the verb: {status} {runs}"
        );
        assert!(
            !runs.to_string().contains("mowing"),
            "{body}: a run came back to an app: {runs}"
        );
        assert!(
            runs["objects"]
                .as_array()
                .is_none_or(|objects| objects.is_empty()),
            "{body}: an object came back with a run asked for: {runs}"
        );
    }
}

/// A key set on a work item in a claim of its own, after the status.
async fn set_key(served: &Served, handle: &str, key: &str, value: &str) {
    let mut fact = NewFact::about(
        EntityId(handle.into()),
        "a later write",
        jiff::civil::Date::constant(2026, 3, 6),
    );
    fact.fields.insert(key.to_string(), value.to_string());
    fact.sets.insert(key.to_string());
    served.memory.capture(fact).await.expect("the key lands");
}

/// **An app asks the ready view on work and gets the queued card that waits on
/// nothing, and not the one that waits on it.** Both halves in one read, so the
/// negative cannot pass on an empty answer.
#[tokio::test]
async fn unstable_the_ready_view_lists_the_unblocked_queued_card_and_not_the_blocked_one() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "queued").await;
    work_with_status(&served, "work:sigma", "Paint the fence", "queued").await;
    set_key(&served, "work:sigma", "depends_on", "work:omega").await;

    let (status, ready) = call(
        &served,
        &token,
        "recall",
        json!({"view": "ready", "where": "status = queued"}),
    )
    .await;
    assert_eq!(status, 200, "{ready}");
    let handles: Vec<&str> = ready["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|o| o["handle"].as_str())
        .collect();
    assert!(handles.contains(&"work:omega"), "{ready}");
    assert!(
        !handles.contains(&"work:sigma"),
        "the blocked card came back: {ready}"
    );
}

/// **Every card of a ready answer carries its verdict, and a card the call
/// named that is blocked says so.** Naming the blocked card brings it back, so
/// the answer must not read as a card to start: it comes back `ready: false`
/// with what stops it, and the unblocked card beside it comes back `ready: true`.
#[tokio::test]
async fn unstable_a_named_blocked_card_comes_back_not_ready_with_its_blockers() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "queued").await;
    work_with_status(&served, "work:sigma", "Paint the fence", "queued").await;
    set_key(&served, "work:sigma", "depends_on", "work:omega").await;

    let (status, blocked) = call(
        &served,
        &token,
        "recall",
        json!({"view": "ready", "subject": "work:sigma"}),
    )
    .await;
    assert_eq!(status, 200, "{blocked}");
    let sigma = &blocked["objects"][0];
    assert_eq!(sigma["handle"], "work:sigma", "{blocked}");
    assert_eq!(sigma["ready"], false, "the named blocked card: {blocked}");
    assert_eq!(
        sigma["blocked_by"][0]["handle"], "work:omega",
        "the blocker is named: {blocked}"
    );

    let (status, unblocked) = call(
        &served,
        &token,
        "recall",
        json!({"view": "ready", "subject": "work:omega"}),
    )
    .await;
    assert_eq!(status, 200, "{unblocked}");
    assert_eq!(
        unblocked["objects"][0]["handle"], "work:omega",
        "{unblocked}"
    );
    assert_eq!(unblocked["objects"][0]["ready"], true, "{unblocked}");
}

/// **A ready list says how many cards the ready question left out.** The blocked
/// card is not listed, and the count is the only way an app learns it exists.
#[tokio::test]
async fn unstable_a_ready_list_serves_how_many_cards_the_question_left_out() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "queued").await;
    work_with_status(&served, "work:sigma", "Paint the fence", "queued").await;
    set_key(&served, "work:sigma", "depends_on", "work:omega").await;

    let (status, ready) = call(
        &served,
        &token,
        "recall",
        json!({"view": "ready", "where": "status = queued"}),
    )
    .await;
    assert_eq!(status, 200, "{ready}");
    assert_eq!(ready["objects"][0]["handle"], "work:omega", "{ready}");
    assert_eq!(ready["objects"][0]["ready"], true, "{ready}");
    assert_eq!(
        ready["ready_left_out"]["count"], 1,
        "the blocked card is counted: {ready}"
    );
}

/// **Any other view is refused, naming the one an app may ask.**
#[tokio::test]
async fn unstable_a_view_other_than_ready_is_refused_naming_ready() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    let (status, refused) = call(&served, &token, "recall", json!({"view": "colleagues"})).await;
    assert_eq!(status, 422, "{refused}");
    assert!(refused.to_string().contains("ready"), "{refused}");
}

/// **Backing names when the claim that set a key was written.** The status is
/// written first and another key after it: the instant under `status` is the
/// status claim's own, earlier than the other key's, and never the object's
/// newest write.
#[tokio::test]
async fn unstable_backing_names_the_instant_of_the_claim_that_set_the_key() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "waiting").await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    set_key(&served, "work:omega", "waiting_on", "person:homer").await;

    let (status, read) = call(
        &served,
        &token,
        "recall",
        json!({"subject": "work:omega", "backing": true}),
    )
    .await;
    assert_eq!(status, 200, "{read}");
    let backing = &read["objects"][0]["fields_backing"];
    let at = |key: &str| -> jiff::Timestamp {
        backing[key]["written_at"]
            .as_str()
            .unwrap_or_else(|| panic!("no instant under {key}: {read}"))
            .parse()
            .expect("an instant")
    };
    assert!(
        backing["status"]["claim"].is_string(),
        "the claim is named beside its instant: {read}"
    );
    assert!(
        at("status") < at("waiting_on"),
        "the status instant is the status claim's, not the newest write's: {read}"
    );
}

/// **A backing whose instants cannot be read is not served as a backing without
/// them.** The same read answers with the instant while the store reads ages, so
/// the failure cannot pass for a card that has none; once the age read fails the
/// call is unavailable and names no entry.
#[tokio::test]
async fn unstable_backing_is_unavailable_when_the_age_read_fails() {
    let (app, token) = reader();
    let failing = Arc::new(AtomicBool::new(false));
    let served = serve_over(vec![app], Registry::shipped(), failing.clone()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "waiting").await;
    let ask = json!({"subject": "work:omega", "backing": true});

    let (status, read) = call(&served, &token, "recall", ask.clone()).await;
    assert_eq!(status, 200, "{read}");
    assert!(
        read["objects"][0]["fields_backing"]["status"]["written_at"].is_string(),
        "the instant is served while ages are readable: {read}"
    );

    failing.store(true, Ordering::SeqCst);
    let (status, down) = call(&served, &token, "recall", ask).await;
    assert_eq!(status, 503, "{down}");
    assert!(down.get("objects").is_none(), "{down}");
}

/// **A page cut is served and counted, and a key narrowing keeps only the keys
/// named.** Three cards at one status are read three ways: with no limit (all
/// three, nothing left out), with `limit` 2 (two, and the one left out counted)
/// and with `keys` naming one key of a card that holds two. **Each refusal to
/// show is read beside what it hides**: the unlimited read names the card the
/// page leaves out, and the whole read of the card names the key the narrowing
/// drops.
#[tokio::test]
async fn unstable_a_recall_page_cut_is_counted_and_a_key_narrowing_keeps_only_those_keys() {
    let (app, token) = reader();
    let served = serve(vec![app], Registry::shipped()).await;
    work_with_status(&served, "work:omega", "Mow the lawn", "now").await;
    work_with_status(&served, "work:sigma", "Paint the fence", "now").await;
    work_with_status(&served, "work:tau", "Rake the leaves", "now").await;
    set_key(&served, "work:omega", "waiting_on", "person:homer").await;
    let ask = json!({"kind": "work", "where": "status = now"});

    let (status, whole) = call(&served, &token, "recall", ask.clone()).await;
    assert_eq!(status, 200, "{whole}");
    assert_eq!(whole["objects"].as_array().expect("objects").len(), 3);
    assert_eq!(whole["left_out"], 0, "{whole}");

    let (status, page) = call(
        &served,
        &token,
        "recall",
        json!({"kind": "work", "where": "status = now", "limit": 2}),
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["objects"].as_array().expect("objects").len(), 2);
    assert_eq!(page["left_out"], 1, "{page}");

    let (status, full) = call(&served, &token, "recall", json!({"subject": "work:omega"})).await;
    assert_eq!(status, 200, "{full}");
    assert!(
        full["objects"][0]["fields"].get("waiting_on").is_some(),
        "the card holds the key the narrowing will drop: {full}"
    );
    let (status, narrowed) = call(
        &served,
        &token,
        "recall",
        json!({"subject": "work:omega", "keys": ["status"]}),
    )
    .await;
    assert_eq!(status, 200, "{narrowed}");
    let kept: Vec<&str> = narrowed["objects"][0]["fields"]
        .as_object()
        .expect("fields")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(kept, ["status"], "{narrowed}");
}

/// **A capture is judged by the kind the store holds, not the spelling the call
/// sent.** A role is retyped to a thread, which the app's grant does not cover; the
/// old spelling still answers to it. Paired: before the retype the same call lands
/// on the role, so the refusal below is the retype and nothing else.
#[tokio::test]
async fn unstable_a_capture_is_judged_by_the_kind_the_store_holds_and_not_the_spelling_sent() {
    let (app, token) = watcher();
    let served = serve(vec![app], Registry::shipped()).await;
    let body = json!({"subject": "role:alpha", "content": "Seen at the gate."});

    let (status, landed) = call(&served, &token, "capture", body.clone()).await;
    assert_eq!(status, 200, "{landed}");

    served
        .memory
        .rename_entity(
            &EntityId("role:alpha".into()),
            &EntityId("thread:alpha".into()),
            None,
            jiff::civil::Date::constant(2026, 3, 7),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("the retype lands")
        .written()
        .expect("the retype is not blocked");

    let (status, refused) = call(
        &served,
        &token,
        "capture",
        json!({"subject": "role:alpha", "content": "Seen again."}),
    )
    .await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("thread"),
        "{refused}"
    );
    let facts = served
        .memory
        .recall(&EntityId("thread:alpha".into()))
        .await
        .expect("recall");
    assert!(
        facts.iter().all(|f| f.content != "Seen again."),
        "nothing was written under the old spelling"
    );
}

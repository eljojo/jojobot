//! Contract: served reads and mention writes use the filled State, not per-entity store reads.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use jiff::civil::date;
use jojobot_adapters::observe::{Observation, Observer, Operation, Outcome};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Guarded, Memory, NewEntity, NewFact, Wanted, guard};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

struct CountFields {
    inner: Arc<InMemoryMemory>,
    counting: AtomicBool,
    reads: Mutex<Vec<EntityId>>,
}

impl CountFields {
    fn start(&self) {
        self.reads.lock().unwrap().clear();
        self.counting.store(true, Ordering::SeqCst);
    }

    fn finish(&self, phase: &str) -> usize {
        self.counting.store(false, Ordering::SeqCst);
        let reads = self.reads.lock().unwrap();
        let mut ids = BTreeMap::new();
        for read in reads.iter() {
            *ids.entry(read.as_str()).or_insert(0usize) += 1;
        }
        eprintln!("{phase}: fields={}, ids={ids:?}", reads.len());
        reads.len()
    }
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for CountFields {
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        if self.counting.load(Ordering::SeqCst) {
            self.reads.lock().unwrap().push(entity.clone());
        }
        self.inner.fields(entity).await
    }
}

struct Observe;
impl Observer for Observe {
    fn finished(&self, _: Operation, _: Outcome, _: std::time::Duration) {}
}

async fn add(store: &dyn Memory, id: &str, parent: Option<&str>) {
    let new = NewEntity {
        parent: parent.map(|id| EntityId(id.into())),
        ..NewEntity::new(EntityId(id.into()), id, "fixture")
    };
    match store
        .add_entity(new.clone())
        .await
        .expect("the entity lands")
    {
        Guarded::Written(_) => {}
        Guarded::Blocked {
            attempted,
            candidates,
        } if candidates.iter().all(|m| {
            matches!(
                m.reason,
                guard::MatchReason::SharedWord | guard::MatchReason::NearSlug
            )
        }) =>
        {
            store
                .add_entity(NewEntity {
                    override_token: Some(guard::override_token(&attempted, &candidates)),
                    override_reason: Some("distinct Greek-letter fixture identities".into()),
                    ..new
                })
                .await
                .expect("the fixture collision is lifted")
                .written()
                .expect("the declared fixture identities are distinct");
        }
        blocked => panic!("fixture {id}: {blocked:?}"),
    }
}

async fn write(store: &dyn Memory, subject: &str, content: &str, pairs: &[(&str, &str)]) {
    let fields = pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<BTreeMap<_, _>>();
    store
        .capture(NewFact {
            sets: fields.keys().cloned().collect(),
            fields,
            ..NewFact::about(EntityId(subject.into()), content, date(2026, 10, 1))
        })
        .await
        .expect("the claim lands")
        .written()
        .expect("the claim is permitted");
}

async fn fixture() -> (
    Arc<CountFields>,
    Arc<jojobot_adapters::fold::Folded>,
    Arc<jojobot_adapters::search::IndexedMemory>,
) {
    let supplied = jojobot_mcp::provisions();
    let store = Arc::new(InMemoryMemory::booted().knowing(supplied.clone()));
    let greek = [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
        "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi",
        "chi", "psi", "omega",
    ];
    for kind in [
        "topic", "person", "thing", "org", "place", "event", "project", "work", "document", "list",
    ] {
        for slug in greek {
            let id = format!("{kind}:{slug}");
            add(
                store.as_ref(),
                &id,
                (kind == "thing").then_some("topic:alpha"),
            )
            .await;
        }
    }
    add(store.as_ref(), "bot:gamma", None).await;
    for slug in greek {
        add(store.as_ref(), &format!("role:{slug}"), Some("bot:gamma")).await;
    }
    write(
        store.as_ref(),
        "topic:alpha",
        "the collection numbers its members",
        &[("numbers_by", "number")],
    )
    .await;
    for (at, slug) in greek.iter().enumerate() {
        write(
            store.as_ref(),
            &format!("thing:{slug}"),
            "a numbered member",
            &[("number", &(at + 1).to_string())],
        )
        .await;
    }
    write(store.as_ref(), "thing:beta", "see @topic:alpha/1", &[]).await;
    store
        .merge(
            &EntityId("person:alpha".into()),
            &EntityId("person:beta".into()),
            Some("duplicate entry"),
            date(2026, 10, 1),
            &EntityId("bot:gamma".into()),
            None,
        )
        .await
        .expect("the merge lands");
    let counted = Arc::new(CountFields {
        inner: store,
        counting: AtomicBool::new(false),
        reads: Mutex::new(Vec::new()),
    });
    let observed = Observation::new(Arc::new(Observe)).memory(counted.clone());
    let (folded, indexed) = jojobot::wiring::assemble_memory(observed, supplied)
        .await
        .expect("the production assembly opens");
    jojobot::wiring::fill_memory(&folded, &indexed).await;
    assert!(
        folded.names(&Wanted::default()).await.unwrap().is_some(),
        "the fold is filled"
    );
    (counted, folded, indexed)
}

mod support;

#[tokio::test]
async fn filled_fold_serves_recall_and_capture_without_store_fields() {
    let (counted, _folded, indexed) = fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = support::app_state::app_state(addr, &indexed);
    jojobot::wiring::run_startup_passes(
        &state.sessions,
        &state.memory,
        &state.mailboxes,
        &state.clock,
    )
    .await;
    let memory = state.memory.clone();
    let stop = CancellationToken::new();
    let app = jojobot::build_app(state, stop.clone());
    let shutdown = stop.clone();
    let serving = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap()
    });
    counted.start();
    let listed = memory
        .list_entities(None)
        .await
        .expect("the scrape's listing reads");
    let listing = counted.finish("list_entities");
    assert!(listed.len() >= 240, "the listing holds the seeded world");
    let surface = jojobot_exercise::surface::Surface::connect(&format!("http://{addr}/mcp"))
        .await
        .unwrap();
    let boot: Value =
        serde_json::from_str(&surface.call("start_here", json!({"bot":"gamma"})).await).unwrap();
    let sid = boot["session"]["sid"].as_str().expect("boot returns a key");
    counted.start();
    let read = surface
        .call(
            "recall",
            json!({"subject":"thing:beta", "facts":true, "sid":sid}),
        )
        .await;
    let recall = counted.finish("recall");
    assert!(read.contains("thing:beta"), "{read}");
    counted.start();
    let captured = surface
        .call(
            "capture",
            json!({"subject":"thing:beta", "content":"see @topic:alpha/1", "sid":sid}),
        )
        .await;
    let capture = counted.finish("capture");
    let captured: Value = serde_json::from_str(&captured).unwrap();
    assert_eq!(captured["success"], true, "{captured}");
    let claimed: Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({
                    "bot":"gamma", "sid":sid, "claim":"gamma", "agent":"gamma-agent", "brief":true
                }),
            )
            .await,
    )
    .unwrap();
    assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
    let role = EntityId("role:gamma".into());
    assert_eq!(
        memory
            .image_field(&role, jojobot_domain::session::ROLE_HOLDER)
            .flatten()
            .as_deref(),
        claimed["session"]["run"].as_str(),
        "a claimed role remains visible to the renewal"
    );
    assert_eq!(
        memory.image_field(&role, jojobot_domain::session::ROLE_CLAIMED_AT),
        None
    );
    counted.start();
    let another = surface
        .call(
            "capture",
            json!({
                "subject":"thing:beta", "content":"see @topic:alpha/1 again", "sid":sid
            }),
        )
        .await;
    let held_capture = counted.finish("capture-with-role");
    assert_eq!(
        serde_json::from_str::<Value>(&another).unwrap()["success"],
        true,
        "{another}"
    );
    assert_eq!(held_capture, 1, "only the held role needs its live lease");
    assert!(
        counted
            .reads
            .lock()
            .unwrap()
            .iter()
            .all(|read| read == &role)
    );
    let wrapped = surface
        .call(
            "wrap_session",
            json!({"sid":sid, "story":"The fixture run finished."}),
        )
        .await;
    let holder = memory
        .image_field(&role, jojobot_domain::session::ROLE_HOLDER)
        .expect("the release remains visible in the State");
    assert!(
        holder.as_deref().is_none_or(|held| held.is_empty()),
        "{wrapped}"
    );
    surface.finish().await;
    stop.cancel();
    serving.await.unwrap();
    assert_eq!(
        (listing, recall, capture),
        (0, 0, 0),
        "a filled fold should answer without per-record store reads"
    );
}

#[tokio::test]
async fn filled_fold_filters_role_holders_but_leaves_lease_moments_to_the_store() {
    let (counted, folded, indexed) = fixture().await;
    let role = EntityId("role:alpha".into());
    let cold = jojobot_adapters::fold::Folded::new(counted.clone());
    assert_eq!(
        cold.image_field(&role, jojobot_domain::session::ROLE_HOLDER),
        None
    );
    counted.start();
    folded
        .fields(&role)
        .await
        .expect("a full role read answers");
    assert_eq!(counted.finish("role-lease-control"), 1);
    assert_eq!(
        indexed
            .memory()
            .image_field(&role, jojobot_domain::session::ROLE_HOLDER),
        Some(None)
    );
    assert_eq!(
        indexed
            .memory()
            .image_field(&role, jojobot_domain::session::ROLE_CLAIMED_AT),
        None
    );
    assert_eq!(
        folded.image_field(
            &EntityId("person:beta".into()),
            jojobot_domain::session::ROLE_HOLDER
        ),
        None,
        "a merged thing still falls through"
    );
}

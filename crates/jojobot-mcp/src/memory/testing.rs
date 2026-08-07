//! **Memory's test fixtures** — the arguments a verb takes, the entities a test
//! needs standing, and the doubles that make a store behave badly on purpose.
//!
//! Named for the context they belong to, mirroring `jojobot_domain::memory::
//! testing`. What builds a handler lives in [`crate::harness`]; what is ABOUT
//! Memory lives here.

use super::*;
use crate::harness::*;
use async_trait::async_trait;
pub(crate) use jojobot_domain::memory::testing::InMemoryMemory;
use std::sync::Mutex;

impl Default for SpySearch {
    fn default() -> Self {
        SpySearch {
            seen: Mutex::new(None),
            hits: Mutex::new(Vec::new()),
            coverage: Coverage::Loaded,
            memory: Coverage::Loaded,
        }
    }
}

impl SpySearch {
    pub(crate) fn answering(hits: Vec<Hit>) -> Self {
        SpySearch {
            hits: Mutex::new(hits),
            ..Default::default()
        }
    }

    /// A search port at a given mail coverage — the states a degraded index
    /// reports.
    pub(crate) fn covering(coverage: Coverage, hits: Vec<Hit>) -> Self {
        SpySearch {
            hits: Mutex::new(hits),
            coverage,
            ..Default::default()
        }
    }

    /// A search port whose MEMORY half is degraded — an index that could not
    /// re-read a document it wrote, or whose boot scan failed.
    pub(crate) fn over_memory(memory: Coverage, hits: Vec<Hit>) -> Self {
        SpySearch {
            hits: Mutex::new(hits),
            memory,
            ..Default::default()
        }
    }

    /// A search port whose mailbox world was never readable — the state an
    /// index is in when the boot scan of the board failed and nothing has
    /// indexed a message since.
    pub(crate) fn with_no_mail_indexed() -> Self {
        Self::covering(Coverage::Unread, Vec::new())
    }

    pub(crate) fn query(&self) -> SearchQuery {
        self.seen
            .lock()
            .unwrap()
            .clone()
            .expect("search must have reached the port")
    }
}

#[async_trait::async_trait]
impl Search for SpySearch {
    async fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        *self.seen.lock().unwrap() = Some(query.clone());
        Ok(self.hits.lock().unwrap().clone())
    }

    fn mail_coverage(&self) -> Coverage {
        self.coverage
    }

    fn memory_coverage(&self) -> Coverage {
        self.memory
    }
}

/// A [`Search`] double: it records the query it was handed and answers with
/// canned hits. On this path the MCP layer's whole job is translating
/// arguments into a query and hits into JSON, and that is exactly what this
/// pins — the ranking and matching are the index's tests, not these.
pub(crate) struct SpySearch {
    seen: Mutex<Option<SearchQuery>>,
    hits: Mutex<Vec<Hit>>,
    /// How much of the mail board this double claims to hold. Default
    /// loaded: an index that has read the board is the ordinary case, and
    /// the degraded ones are worth writing down at a call site.
    coverage: Coverage,
    /// The same, for the memory half.
    memory: Coverage,
}

pub(crate) fn capture_args(subject: &str, content: &str) -> CaptureArgs {
    CaptureArgs {
        subject: subject.into(),
        content: content.into(),
        details: None,
        provenance: None,
        standing: None,
        date: None,
        shape: None,
        object: None,
        derived_from: None,
        event_type: None,
        metadata: None,
        refs: None,
        sid: Some(crate::harness::TEST_SID.into()),
    }
}

pub(crate) fn update_args(address: &str) -> UpdateFactArgs {
    UpdateFactArgs {
        address: address.into(),
        content: None,
        details: None,
        status: None,
        standing: None,
        provenance: None,
        confirmed_by_user: None,
        shape: None,
        object: None,
        sid: Some(crate::harness::TEST_SID.into()),
    }
}

/// Make sure a handle names an entity, so the write guard's **existence
/// gate** is not what a spec about something else trips over. Idempotent —
/// an add that comes back blocked means it is already there.
pub(crate) async fn ensure(jojobot: &Jojobot, handle: &str) {
    let id = EntityId::person(handle);
    let kind = id.kind().expect("test handles are well-formed");
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            kind: kind.as_token().into(),
            handle: id.slug().into(),
            name: id.slug().into(),
            aliases: None,
            source: "test-fixture".into(),
            crm: None,
            boot: None,
            override_token: None,
            // The handler's own registry, for the same reason `make_bot` uses
            // it: a bare-registry test must be able to provision a subject.
            sid: Some(crate::harness::writing_as(jojobot)),
        }))
        .await
        .expect("add_entity call ok");
}

/// [`ensure`], attributed to a session. Beats are written for whoever the
/// handle names, so a spec about the tally has to say who is calling.
pub(crate) async fn ensure_as(jojobot: &Jojobot, sid: &str, handle: &str) {
    let id = EntityId::person(handle);
    let kind = id.kind().expect("test handles are well-formed");
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            sid: Some(sid.to_string()),
            ..add_args(kind.as_token(), id.slug(), id.slug())
        }))
        .await
        .expect("add_entity call ok");
}

/// [`capture_ok`], attributed to a session — same reason.
pub(crate) async fn capture_as(
    jojobot: &Jojobot,
    sid: &str,
    args: CaptureArgs,
) -> serde_json::Value {
    capture_ok(
        jojobot,
        CaptureArgs {
            sid: Some(sid.to_string()),
            ..args
        },
    )
    .await
}

/// Capture through the handler, expecting the guard to wave it through —
/// provisioning the subject and any edge object first, because every write
/// that names an entity now requires one that exists.
pub(crate) async fn capture_ok(jojobot: &Jojobot, args: CaptureArgs) -> serde_json::Value {
    ensure(jojobot, &args.subject).await;
    if let Some(object) = args.object.as_deref() {
        ensure(jojobot, object).await;
    }
    let result = jojobot.capture(Parameters(args)).await.expect("capture ok");
    let body = json_of(&result);
    assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
    body
}

/// The `address` field of a rendered fact — every read carries one.
pub(crate) fn address_of(fact: &serde_json::Value) -> String {
    fact["address"]
        .as_str()
        .expect("every fact on the wire carries its address")
        .to_string()
}

pub(crate) fn add_args(kind: &str, handle: &str, name: &str) -> AddEntityArgs {
    AddEntityArgs {
        kind: kind.into(),
        handle: handle.into(),
        name: name.into(),
        aliases: None,
        source: "user-named".into(),
        crm: None,
        boot: None,
        override_token: None,
        sid: Some(crate::harness::TEST_SID.into()),
    }
}

pub(crate) fn search_args() -> SearchArgs {
    SearchArgs {
        query: None,
        kind: None,
        status: None,
        provenance: None,
        subject: None,
        edge: None,
        include_mail: None,
        limit: None,
        sid: None,
    }
}

/// A handler whose mailbox world answers nothing, over a memory the caller
/// may already have populated — a bot has to be stood up while the world is
/// up, since a claim that cannot be screened is refused.
/// A Memory whose ENTITY INDEX cannot be read, everything else working —
/// the shape an Outline outage takes for the one read ownership depends on.
pub(crate) struct UnindexedMemory(pub(crate) Arc<InMemoryMemory>);

#[async_trait]
impl Memory for UnindexedMemory {
    async fn list_entities(&self, _: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        Err(MemoryError::Store("the entity index cannot be read".into()))
    }
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.0.add_entity(new).await
    }
    async fn update_entity(
        &self,
        id: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.0.update_entity(id, patch).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.0.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.0.recall(subject).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.0.update_fact(address, patch).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
    ) -> Result<jojobot_domain::memory::Retraction, MemoryError> {
        self.0.retract(address, reason, date).await
    }
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        self.0.set_prose(entity, prose).await
    }
    async fn scan(&self) -> Result<Vec<jojobot_domain::memory::search::DocScan>, MemoryError> {
        self.0.scan().await
    }
}

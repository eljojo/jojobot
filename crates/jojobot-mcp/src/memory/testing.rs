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
        fields: None,
        refs: None,
        check_in: None,
        sid: Some(crate::harness::TEST_SID.into()),
        stale_after: None,
    }
}

/// **The whole-page query**: one handle, its records, no walk. What `recall`
/// answered before it could walk, and still the commonest thing asked of it.
///
/// **It asks for the records explicitly**, since they are off by default: a
/// case that asserts on what a claim SAYS has to ask for the claim.
pub(crate) fn recall_args(subject: &str) -> RecallArgs {
    RecallArgs {
        subject: Some(subject.into()),
        kind: None,
        answers_type: None,
        fields: None,
        facts: Some(true),
        prose: None,
        follow: None,
        overdue: None,
        sid: None,
        history: None,
        history_most: None,
        values: None,
        values_most: None,
        built_on: None,
    }
}

pub(crate) fn update_args(address: &str) -> UpdateFactArgs {
    UpdateFactArgs {
        address: address.into(),
        fields: None,
        clear_fields: None,
        content: None,
        details: None,
        status: None,
        standing: None,
        provenance: None,
        confirmed_by_user: None,
        shape: None,
        object: None,
        stale_after: None,
        clear_stale_after: None,
        derived_from: None,
        clear_derived_from: None,
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
            parent: None,
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
        parent: None,
        override_token: None,
        sid: Some(crate::harness::TEST_SID.into()),
    }
}

pub(crate) fn search_args() -> SearchArgs {
    SearchArgs {
        answers_type: None,
        query: None,
        kind: None,
        status: None,
        provenance: None,
        subject: None,
        edge: None,
        include_mail: None,
        limit: None,
        sid: None,
        fits_type: None,
    }
}

/// **Which ONE read a [`DownMemory`] cannot answer.** One at a time, because a
/// port that failed wholesale could not say which read the surface under test
/// actually depends on.
pub(crate) enum Down {
    /// The entity index — the shape an Outline outage takes for the one read
    /// ownership depends on.
    EntityIndex,
    /// The type roster — the one fallible step behind every `answers_type`
    /// argument.
    TypeRoster,
}

/// A handler whose mailbox world answers nothing, over a memory the caller
/// may already have populated — a bot has to be stood up while the world is
/// up, since a claim that cannot be screened is refused.
/// A Memory with [`Down`]'s one read failing and everything else working.
pub(crate) struct DownMemory(pub(crate) Down, pub(crate) Arc<InMemoryMemory>);

#[async_trait]
impl Memory for DownMemory {
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        match self.0 {
            Down::EntityIndex => Err(MemoryError::Store("the entity index cannot be read".into())),
            Down::TypeRoster => self.1.list_entities(kind).await,
        }
    }
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.1.add_entity(new).await
    }
    async fn declare_type(
        &self,
        declared: jojobot_domain::memory::types::DeclaredType,
    ) -> Result<jojobot_domain::memory::types::DeclaredType, MemoryError> {
        self.1.declare_type(declared).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: jojobot_domain::memory::types::Origin,
        fields: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        self.1.declare_kind(token, origin, fields).await
    }

    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        self.1.declared_kinds().await
    }

    async fn declared_types(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::types::DeclaredType>, MemoryError> {
        match self.0 {
            Down::TypeRoster => Err(MemoryError::Store("the type roster cannot be read".into())),
            Down::EntityIndex => self.1.declared_types().await,
        }
    }
    async fn update_entity(
        &self,
        id: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.1.update_entity(id, patch).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.1.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.1.recall(subject).await
    }
    async fn history(
        &self,
        entity: &EntityId,
        key: &str,
    ) -> Result<Vec<jojobot_domain::memory::FieldWrite>, MemoryError> {
        self.1.history(entity, key).await
    }
    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        self.1.fields(entity).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.1.update_fact(address, patch).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
    ) -> Result<jojobot_domain::memory::Retraction, MemoryError> {
        self.1.retract(address, reason, date).await
    }
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        self.1.set_prose(entity, prose).await
    }
    async fn scan(&self) -> Result<Vec<jojobot_domain::memory::search::DocScan>, MemoryError> {
        self.1.scan().await
    }
}

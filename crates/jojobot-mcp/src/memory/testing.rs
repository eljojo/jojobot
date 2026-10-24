//! **Memory's test fixtures** — the arguments a verb takes, the entities a test
//! needs standing, and the doubles that make a store behave badly on purpose.
//!
//! Named for the context they belong to, mirroring `jojobot_domain::memory::
//! testing`. What builds a handler lives in [`crate::harness`]; what is ABOUT
//! Memory lives here.

use super::*;
use crate::harness::*;
use jojobot_domain::memory::search::RankedPage;
pub(crate) use jojobot_domain::memory::testing::InMemoryMemory;
use std::sync::Mutex;

impl Default for SpySearch {
    fn default() -> Self {
        SpySearch {
            seen: Mutex::new(None),
            hits: Mutex::new(Vec::new()),
            coverage: Coverage::Loaded,
            memory: Coverage::Loaded,
            session: Coverage::Loaded,
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

    /// A search port whose SESSIONS half is degraded — a read that could not
    /// reach the store, with the last good index left standing.
    pub(crate) fn over_sessions(session: Coverage, hits: Vec<Hit>) -> Self {
        SpySearch {
            hits: Mutex::new(hits),
            session,
            ..Default::default()
        }
    }

    /// The hits this port answers with from now on, for a case whose hits are
    /// records the case itself has just written.
    pub(crate) fn answer_with(&self, hits: Vec<Hit>) {
        *self.hits.lock().unwrap() = hits;
    }

    pub(crate) fn query(&self) -> SearchQuery {
        self.seen
            .lock()
            .unwrap()
            .clone()
            .expect("search must have reached the port")
    }

    /// **Whether anything ever asked this port a question.** The same signal
    /// [`SpySearch::query`] leans on, without the panic — for a case whose
    /// whole point is that the port was never reached.
    pub(crate) fn reached(&self) -> bool {
        self.seen.lock().unwrap().is_some()
    }
}

#[async_trait::async_trait]
impl Search for SpySearch {
    async fn search_page(&self, query: &SearchQuery) -> Result<RankedPage, MemoryError> {
        *self.seen.lock().unwrap() = Some(query.clone());
        // The count describes every match; the page carries only its best hits.
        let hits = self.hits.lock().unwrap();
        Ok(RankedPage {
            total: hits.len(),
            hits: hits.iter().take(query.limit).cloned().collect(),
        })
    }

    fn mail_coverage(&self) -> Coverage {
        self.coverage
    }

    fn memory_coverage(&self) -> Coverage {
        self.memory
    }

    fn session_coverage(&self) -> Coverage {
        self.session
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
    /// The same, for the sessions half.
    session: Coverage,
}

pub(crate) fn capture_args(subject: &str, content: &str) -> CaptureArgs {
    CaptureArgs {
        override_token: None,
        override_reason: None,
        tick: None,
        entries: None,
        ticks: None,
        subject: subject.into(),
        content: content.into(),
        details: None,
        provenance: None,
        standing: None,
        recorded_at: None,
        happened_at: None,
        happened_through: None,
        shape: None,
        object: None,
        derived_from: None,
        fields: None,
        sets: None,
        sets_add: None,
        sets_remove: None,
        refs: None,
        check_in: None,
        sid: Some(crate::harness::TEST_SID.into()),
        stale_after: None,
        receipt: None,
        drop: None,
        drop_because: None,
        after: None,
        borrow: None,
        request_id: None,
    }
}

/// **The whole-page query**: one handle, its records, no walk. What `recall`
/// answered before it could walk, and still the commonest thing asked of it.
///
/// **It asks for the records explicitly**, since they are off by default: a
/// case that asserts on what a claim SAYS has to ask for the claim.
pub(crate) fn recall_args(subject: &str) -> RecallArgs {
    RecallArgs {
        ticks: None,
        ticks_offset: None,
        unreviewed: None,
        details: None,
        view: None,
        inputs: None,
        subject: Some(subject.into()),
        kind: None,
        parent: None,
        answers_type: None,
        fields: None,
        expression: None,
        scope: None,
        order: None,
        facts: Some(true),
        stood_for: None,
        review_marks: None,
        status: None,
        prose: None,
        charter: None,
        keys: None,
        record_keys: None,
        follow: None,
        overdue: None,
        near: None,
        sid: None,
        history: None,
        history_record: None,
        records: None,
        history_most: None,
        offset: None,
        limit: None,
        values: None,
        values_most: None,
        built_on: None,
        backing: None,
        held: None,
    }
}

pub(crate) fn update_args(address: &str) -> UpdateFactArgs {
    UpdateFactArgs {
        override_token: None,
        override_reason: None,
        request_id: None,
        address: address.into(),
        fields: None,
        sets: None,
        sets_add: None,
        sets_remove: None,
        clear_fields: None,
        content: None,
        details: None,
        recorded_at: None,
        happened_at: None,
        clear_happened_at: None,
        happened_through: None,
        clear_happened_through: None,
        status: None,
        standing: None,
        provenance: None,
        confirmed_by_user: None,
        shape: None,
        object: None,
        stale_after: None,
        receipt: None,
        clear_stale_after: None,
        derived_from: None,
        clear_derived_from: None,
        stands_for: None,
        clear_stands_for: None,
        clear_edge: None,
        keep: None,
        find: None,
        replace: None,
        field: None,
        after: None,
        expected_revision: None,
        sid: Some(crate::harness::TEST_SID.into()),
    }
}

/// Make sure a handle names an entity, so the write guard's **existence
/// gate** is not what a spec about something else trips over. Idempotent —
/// an add that comes back blocked means it is already there.
pub(crate) async fn ensure(jojobot: &Jojobot, handle: &str) {
    let id = EntityId::person(handle);
    let kind = id.kind().expect("test handles are well-formed");
    add_fixture(
        jojobot,
        AddEntityArgs {
            request_id: None,
            kind: kind.as_token().into(),
            handle: id.slug().into(),
            name: id.slug().into(),
            aliases: None,
            source: "test-fixture".into(),
            crm: None,
            boot: None,
            parent: None,
            sets: None,
            override_token: None,
            override_reason: None,
            // The handler's own registry, for the same reason `make_bot` uses
            // it: a bare-registry test must be able to provision a subject.
            sid: Some(crate::harness::writing_as(jojobot)),
        },
    )
    .await;
}

/// [`ensure`], attributed to a session. Beats are written for whoever the
/// session names, so a spec that reads them back must provision as that session.
pub(crate) async fn ensure_as(jojobot: &Jojobot, sid: &str, handle: &str) {
    let id = EntityId::person(handle);
    let kind = id.kind().expect("test handles are well-formed");
    add_fixture(
        jojobot,
        AddEntityArgs {
            sid: Some(sid.to_string()),
            ..add_args(kind.as_token(), id.slug(), id.slug())
        },
    )
    .await;
}

/// **Create a fixture the way a bot that insists does.** A fixture's name is its
/// slug, so two of one family (`contract-view-filter-routine`,
/// `contract-view-filter-owner`) share rare words and the shared-word reason
/// refuses the second. No case that provisions this way is about that refusal,
/// so the refusal is lifted with the token it minted and a reason. Any other
/// refusal is left as it came: an add that comes back blocked means the thing
/// is already there.
async fn add_fixture(jojobot: &Jojobot, args: AddEntityArgs) {
    let result = jojobot
        .add_entity(Parameters(AddEntityArgs {
            override_token: args.override_token.clone(),
            override_reason: args.override_reason.clone(),
            request_id: args.request_id.clone(),
            kind: args.kind.clone(),
            handle: args.handle.clone(),
            name: args.name.clone(),
            aliases: args.aliases.clone(),
            source: args.source.clone(),
            crm: args.crm.clone(),
            boot: args.boot.clone(),
            parent: args.parent.clone(),
            sets: args.sets.clone(),
            sid: args.sid.clone(),
        }))
        .await
        .expect("add_entity call ok");
    let body = json_of(&result);
    let by_shared_words = body["status"] == "blocked"
        && body["candidates"].as_array().is_some_and(|found| {
            !found.is_empty() && found.iter().all(|c| c["reason"] == "shared-word")
        });
    if !by_shared_words {
        return;
    }
    let advice = body["how_to_proceed"].as_str().unwrap_or_default();
    let marker = "override_token: \"";
    let from = advice.find(marker).expect("the refusal names its token") + marker.len();
    let token = &advice[from..from + advice[from..].find('"').expect("the token ends")];
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            override_token: Some(token.to_string()),
            override_reason: Some("a fixture of the same family".into()),
            ..args
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

/// What a thing holds, folded, as `recall` renders it.
pub(crate) async fn fields_of(jojobot: &Jojobot, subject: &str) -> serde_json::Value {
    let body = json_of(
        &jojobot
            .recall(Parameters(recall_args(subject)))
            .await
            .expect("recall ok"),
    );
    body["objects"][0]["fields"].clone()
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
        request_id: None,
        kind: kind.into(),
        handle: handle.into(),
        name: name.into(),
        aliases: None,
        source: "user-named".into(),
        crm: None,
        boot: None,
        parent: None,
        sets: None,
        override_token: None,
        override_reason: None,
        sid: Some(crate::harness::TEST_SID.into()),
    }
}

pub(crate) fn search_args() -> SearchArgs {
    SearchArgs {
        details: None,
        review_marks: None,
        answers_type: None,
        query: None,
        kind: None,
        status: None,
        provenance: None,
        standing: None,
        subject: None,
        edge: None,
        include_mail: None,
        include_links: None,
        include_history: None,
        limit: None,
        offset: None,
        clock: None,
        sid: None,
        fits_type: None,
    }
}

/// **Why a store write fails**, for [`Down::WritesAfter`]: a store that could not
/// be reached, stored damage a person must repair, or a write the store's
/// constraints refused. Each wears a different word.
#[derive(Clone, Copy)]
pub(crate) enum Cause {
    Outage,
    Damage(&'static str),
    Refusal(&'static str),
}

/// **Which ONE read a [`DownMemory`] cannot answer.** One at a time, because a
/// port that failed wholesale could not say which read the surface under test
/// actually depends on.
pub(crate) enum Down {
    /// The entity index — the shape a failed entity-index read takes, for the
    /// one read ownership depends on.
    EntityIndex,
    /// The type roster — the one fallible step behind every `answers_type`
    /// argument.
    TypeRoster,
    /// The kind vocabulary — the read the boot door names the shipped kinds
    /// from, and the one whose failure must not read as a build shipping none.
    Vocabulary,
    /// A claim read, the one a guard on the record's own fields depends on
    /// before it lets a retraction or an archive through.
    Recall,
    /// A claim read that fails with something other than a store error: the
    /// kind set was never loaded, so no handle can be read. The guard above
    /// has to refuse on this too, because it cannot say a record carries no
    /// role field when it could not read the record.
    RecallNeverLoaded,
    /// Every write of a claim: a capture and an edit both fail with a store
    /// error, while every read answers.
    Writes,
    /// **The instance's own record is not to be read.** A read of its fields
    /// panics, and every other read answers: the stand-in for a call that must
    /// not spend a read on the zone it has no use for. A failure would not do,
    /// because the zone read swallows one and answers as if no zone were set.
    InstanceRecord,
    /// **A thing's fields cannot be read**: the read of the fields of any
    /// `thing:` handle fails with a store error, and every other read answers.
    ThingFields,
    /// **A piece of work's fields cannot be read**: the read of the fields of any
    /// `work:` handle fails with a store error, and every other read answers. The
    /// read a list makes of the work its entries link.
    WorkFields,
    /// **An app's event cannot be recorded**: the write fails with a store error
    /// and every read answers.
    AppEvent,
    /// **The type roster reads until a declaration lands, and not after it**: the
    /// flag is set by the write, and every read of the roster once it is set fails
    /// with a store error. The stand-in for a read that fails right after a write.
    TypeRosterAfterDeclare(Arc<std::sync::atomic::AtomicBool>),
    /// **One claim-history read fails, counted from the value set**: the counter
    /// holds how many of the next claim-history reads to let through before the
    /// one that fails, once; every later read and every other read answers. `2`
    /// lets one read through and fails the next. The stand-in for a read that
    /// fails between two reads of the same answer.
    ClaimHistoryFailsAt(Arc<std::sync::atomic::AtomicUsize>),
    /// **The fields of ONE handle cannot be read**: that handle's read fails with
    /// a store error, and the fields of every other handle answer.
    FieldsOf(&'static str),
    /// **The scan of ONE handle cannot be read**: that handle's entity scan fails
    /// with a store error, and every other read answers.
    ScanOf(&'static str),
    /// **The claims of ONE handle stop being readable after a number of reads**:
    /// the first `allowed` reads of that handle answer and every later one fails
    /// with a store error, which is what a store that goes down between two reads
    /// of one call does. Every other handle answers.
    RecallOfAfter(&'static str, usize, Arc<std::sync::atomic::AtomicUsize>),
    /// **Claim writes stop landing after a number of them**: the first `allowed`
    /// captures answer and every later one fails with a store error, while every
    /// read answers. A batch cut partway through is this. The third part is
    /// the cause of the failure.
    WritesAfter(usize, Arc<std::sync::atomic::AtomicUsize>, Cause),
    /// **The listing of ONE kind cannot be read**: a listing that names this kind
    /// token fails with a store error, and a listing of any other kind or of all
    /// of them answers.
    ListingKind(&'static str),
    /// **A role cannot be released**: an edit that carries a release move fails
    /// with a store error, and every other read and write answers.
    RoleRelease,
    /// **Every role but the one named cannot be released**: an edit that carries
    /// a release move for any other role fails with a store error, so a holder of
    /// two roles loses only the named one.
    RoleReleaseExcept(&'static str),
    /// **Only the role named cannot be released**: the release of any other role
    /// lands, so a holder of two roles keeps just the named one.
    RoleReleaseOnly(&'static str),
    /// RAM fields have no readable image; asynchronous reads and writes still answer.
    ImageFields,
    /// **Nothing fails.** The healthy half of a pair that needs both halves to
    /// be the same kind of store.
    Nothing,
    /// **What was built on a record is not to be read.** The read panics, and
    /// every other read answers: the stand-in for a call that must not spend a
    /// read on a block it leaves out. A failure would not do, because a list
    /// read swallows one.
    BuiltOn,
    /// **A second writer lands a resembling thing in the instant before a
    /// creation with its first claim**: the thing named here is created over the
    /// screen, so the creation meets a candidate that the refusal a caller got
    /// earlier did not list, and the token that refusal minted no longer fits.
    ResemblerBeforeFirstClaim {
        handle: &'static str,
        name: &'static str,
    },
}

/// **Two handlers over ONE store, the second unable to read a claim.** They
/// share the store, the mail, the sessions and the registry, so a record the
/// first writes is the record the second is asked to take back — and a write
/// the blind one lets through is visible through the healthy one.
pub(crate) fn healthy_and_blind_to_claims() -> (Jojobot, Jojobot) {
    healthy_and_down(Down::Recall)
}

/// **Two handlers over one store, the second with `down` failing.** See
/// [`healthy_and_blind_to_claims`].
pub(crate) fn healthy_and_down(down: Down) -> (Jojobot, Jojobot) {
    let memory = Arc::new(InMemoryMemory::booted());
    let pair = pair_of_handlers();
    (
        pair(memory.clone()),
        pair(Arc::new(DownMemory(down, memory))),
    )
}

/// [`healthy_and_down`] over the records the build supplies, its views among
/// them, as [`crate::harness::handler_shipped`] reads them.
pub(crate) fn healthy_and_down_shipped(down: Down) -> (Jojobot, Jojobot) {
    use jojobot_adapters::provisioned::Provisioned;
    let supplied = crate::provisions();
    let memory = Arc::new(InMemoryMemory::seeded().knowing(supplied.clone()));
    let pair = pair_of_handlers();
    (
        pair(Arc::new(Provisioned::new(
            DownMemory(Down::Nothing, memory.clone()),
            supplied.clone(),
        ))),
        pair(Arc::new(Provisioned::new(
            DownMemory(down, memory),
            supplied,
        ))),
    )
}

/// **What two handlers over one store share**: the mail, the sessions, the
/// teachings and the registry.
fn pair_of_handlers() -> impl Fn(Arc<dyn Memory>) -> Jojobot {
    let boxes = Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner());
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let teachings = Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new());
    let registry = seeded_registry();
    move |store: Arc<dyn Memory>| {
        Jojobot::new(
            store,
            Arc::new(SpySearch::default()),
            boxes.clone(),
            sessions.clone(),
            teachings.clone(),
            registry.clone(),
        )
    }
}

/// **[`healthy_and_down`] over a store that answers for what the build
/// supplies**, so the shipped views (`ready`, `board`) are there to be asked.
/// The healthy handler's store fails nothing it is asked.
pub(crate) fn shipped_healthy_and_down(down: Down) -> (Jojobot, Jojobot) {
    let supplied = crate::provisions();
    let memory = Arc::new(InMemoryMemory::seeded().knowing(supplied.clone()));
    let boxes = Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner());
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let teachings = Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new());
    let registry = seeded_registry();
    let build = |down: Down| {
        Jojobot::new(
            Arc::new(jojobot_adapters::provisioned::Provisioned::new(
                DownMemory(down, memory.clone()),
                supplied.clone(),
            )),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            sessions.clone(),
            teachings.clone(),
            registry.clone(),
        )
    };
    (build(Down::FieldsOf("machine:phi")), build(down))
}

/// **A role claim taken at the boot door**, and the address of the record that
/// carries its holder and claim moment — the record whose retraction or archive
/// would take both off the fold.
pub(crate) async fn a_claimed_role(jojobot: &Jojobot) -> String {
    make_bot(jojobot, "gamma").await;
    let booted = json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: Some("dev-dispatch".into()),
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok"),
    );
    assert_eq!(booted["session"]["claim"]["status"], "taken", "{booted}");
    jojobot
        .memory
        .recall(&EntityId("role:dev-dispatch".into()))
        .await
        .expect("recall ok")
        .into_iter()
        .find(|fact| fact.fields.contains_key("holder"))
        .expect("the claim left a fact carrying its own fields")
        .address()
        .to_string()
}

/// **A role object that holds a claim and an ordinary claim, renamed since.**
/// Returns the address of the record that carries the role's holder and claim
/// moment and the address of the ordinary claim, both typed under the handle the
/// role object wore before the rename.
pub(crate) async fn a_role_and_an_ordinary_claim_under_a_former_handle(
    jojobot: &Jojobot,
) -> (String, String) {
    let claim = a_claimed_role(jojobot).await;
    let ordinary = address_of(
        &capture_ok(
            jojobot,
            capture_args("role:dev-dispatch", "an ordinary claim"),
        )
        .await,
    );
    // **Renamed by the bot that owns the role**: renaming a role object is the
    // owner's or the chart head's to do, whatever the rename changes.
    let owner = crate::harness::as_bot(jojobot, "gamma");
    let renamed = json_of(
        &jojobot
            .rename_entity(Parameters(RenameEntityArgs {
                request_id: None,
                handle: "role:dev-dispatch".into(),
                to: "role:dev-desk".into(),
                parent: None,
                recorded_at: None,
                override_token: None,
                override_reason: None,
                sid: Some(owner),
            }))
            .await
            .expect("rename ok"),
    );
    assert_ne!(renamed["status"], "blocked", "{renamed}");
    (claim, ordinary)
}

mod down_memory;
pub(crate) use down_memory::DownMemory;

mod fold_behind;
pub(crate) use fold_behind::FoldBehindMemory;

/// **A store whose `racer` gains a ceiling in the instant before a merge
/// runs** — a second session of the same bot writing between whatever a caller
/// checked and the merge the check was for. Every read answers as it did
/// before the ceiling landed, exactly what a checking caller saw.
pub(crate) struct RacingMemory {
    pub(crate) inner: Arc<InMemoryMemory>,
    pub(crate) racer: EntityId,
    pub(crate) armed: std::sync::atomic::AtomicBool,
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for RacingMemory {
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<jojobot_domain::memory::Merge, MemoryError> {
        if self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.inner
                .capture(NewFact {
                    fields: std::collections::BTreeMap::from([(
                        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                        "5".to_string(),
                    )]),
                    sets: [jojobot_domain::memory::THOUGHT_CAPACITY.to_string()]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(
                        self.racer.clone(),
                        "a second session raises the ceiling",
                        jiff::civil::date(2026, 10, 6),
                    )
                })
                .await?;
        }
        self.inner
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }
}

/// **A store where the claim is written again right after a caller READS it**
/// — a second session editing between the read an edit was computed from and
/// the write that carries it. Every read answers as it did before the write
/// landed, which is exactly what the reading caller saw.
pub(crate) struct MovesAfterTheRead {
    pub(crate) inner: Arc<InMemoryMemory>,
    pub(crate) armed: std::sync::atomic::AtomicBool,
    /// Change the claim's shape after recall but before history is read.
    pub(crate) before_history: bool,
}

impl MovesAfterTheRead {
    pub(crate) fn new(inner: Arc<InMemoryMemory>) -> Self {
        Self {
            inner,
            armed: std::sync::atomic::AtomicBool::new(true),
            before_history: false,
        }
    }
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for MovesAfterTheRead {
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let facts = self.inner.recall(subject).await?;
        if self.before_history && self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            let fact = facts
                .iter()
                .find(|fact| fact.content.starts_with("Set "))
                .unwrap();
            let chain = self.inner.claim_history(&fact.address()).await?;
            self.inner
                .update_fact(
                    &fact.address(),
                    FactPatch {
                        fields: [
                            ("agent".into(), "a2".into()),
                            ("seat".into(), "front".into()),
                        ]
                        .into(),
                        sets: ["agent".into(), "seat".into()].into(),
                        session: chain.last().and_then(|write| write.session.clone()),
                        ..Default::default()
                    },
                    &EntityId("bot:gamma".into()),
                )
                .await?;
        }
        Ok(facts)
    }
    async fn claim_history(
        &self,
        address: &jojobot_domain::memory::FactAddress,
    ) -> Result<Vec<jojobot_domain::memory::ClaimWrite>, MemoryError> {
        let chain = self.inner.claim_history(address).await?;
        if !self.before_history && self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.inner
                .update_fact(
                    address,
                    FactPatch {
                        content: Some("moved on while the text was read".into()),
                        provenance: Some(jojobot_domain::memory::Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:assistant".into()),
                )
                .await?;
        }
        Ok(chain)
    }
}

/// **A store that gives up the thread where a real store does**: after a read
/// has answered, and either side of a write. An in-memory double answers at once,
/// so two callers on one thread never interleave and a race between a read and
/// the write built on it cannot happen. This one lets them: the second caller
/// reads before the first has written.
pub(crate) struct YieldsLikeAStore {
    pub(crate) inner: Arc<InMemoryMemory>,
    /// A listing of the entities that stops once, after it has answered, until
    /// a case lets it go. `None` for a store that never stops.
    pub(crate) pause: Option<Arc<ListingPause>>,
    /// A claim write that stops once, before it reaches the store, until a case
    /// lets it go. The caller has read, and minted whatever it will store from
    /// that read, when it waits here. `None` for a store that never stops.
    pub(crate) write_pause: Option<Arc<ListingPause>>,
}

/// **One listing of the entities that is held after it has read**, so a case can
/// land a write between that read and whatever the caller does with its answer.
/// Armed, it stops the next listing exactly once.
#[derive(Default)]
pub(crate) struct ListingPause {
    armed: std::sync::atomic::AtomicBool,
    reached: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

impl ListingPause {
    /// Stop the next listing, once.
    pub(crate) fn arm(&self) {
        self.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Wait until a listing has read and is held.
    pub(crate) async fn reached(&self) {
        self.reached.notified().await;
    }

    /// Let the held listing answer.
    pub(crate) fn release(&self) {
        self.release.notify_one();
    }
}

/// Hand the thread to every other task that is ready, a few times over.
async fn yield_like_a_store() {
    for _ in 0..4 {
        tokio::task::yield_now().await;
    }
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for YieldsLikeAStore {
    async fn former_handles(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::FormerHandle>, MemoryError> {
        let read = self.inner.former_handles().await;
        yield_like_a_store().await;
        read
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        let read = self.inner.list_entities(kind).await;
        if let Some(pause) = &self.pause
            && pause.armed.swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            pause.reached.notify_one();
            pause.release.notified().await;
        }
        yield_like_a_store().await;
        read
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        yield_like_a_store().await;
        if let Some(pause) = &self.write_pause
            && pause.armed.swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            pause.reached.notify_one();
            pause.release.notified().await;
        }
        let written = self.inner.capture(fact).await;
        yield_like_a_store().await;
        written
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let read = self.inner.recall(subject).await;
        yield_like_a_store().await;
        read
    }
}

/// **Every `capture` and `update_fact` collides — the shape a real store's
/// optimistic concurrency takes** (SQLSTATE `40001`), for a caller that
/// needs `MemoryError::Conflict` without spinning up a real store to race
/// against itself.
pub(crate) struct ConflictingMemory(pub(crate) Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for ConflictingMemory {
    async fn capture(&self, _fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        Err(MemoryError::Conflict)
    }
    async fn update_fact(
        &self,
        _address: &FactAddress,
        _patch: FactPatch,
        _caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        Err(MemoryError::Conflict)
    }
}

/// **A store that stops taking writes partway through a run of them**: the
/// captures before `fails_from` land and every one from it on fails with a store
/// error, and so do the edits from `edits_fail_from`, while every read answers.
pub(crate) struct FailsOnCapture {
    pub(crate) inner: Arc<dyn Memory>,
    /// The capture, counting from 1, from which every capture fails.
    pub(crate) fails_from: usize,
    pub(crate) seen: std::sync::atomic::AtomicUsize,
    /// The edit, counting from 1, from which every edit fails.
    pub(crate) edits_fail_from: usize,
    pub(crate) edits_seen: std::sync::atomic::AtomicUsize,
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for FailsOnCapture {
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let nth = self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if nth >= self.fails_from {
            return Err(MemoryError::Store("the store stopped taking writes".into()));
        }
        self.inner.capture(fact).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let nth = self
            .edits_seen
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if nth >= self.edits_fail_from {
            return Err(MemoryError::Store("the store stopped taking edits".into()));
        }
        self.inner.update_fact(address, patch, caller).await
    }
    /// **An edit of a thing with a claim is an edit**: it counts toward
    /// `edits_fail_from` and fails with the store error the other edits fail with,
    /// not with the refusal the trait's default gives a store that cannot do it.
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        let nth = self
            .edits_seen
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if nth >= self.edits_fail_from {
            return Err(MemoryError::Store("the store stopped taking edits".into()));
        }
        self.inner
            .update_entity_with_claim(handle, patch, claim)
            .await
    }
}

#[cfg(test)]
mod down_memory_tests {
    use super::*;
    use jojobot_domain::app::{AppEvent, AppOutcome};
    use jojobot_domain::memory::NewFact;

    /// A deploy event an app would send, under a thread the store holds.
    async fn a_deploy_event(memory: &InMemoryMemory) -> AppEvent {
        let thread = EntityId("thread:deploys-alpha".into());
        memory
            .add_entity(NewEntity::new(thread.clone(), "Deploys Alpha", "fixture"))
            .await
            .expect("the thread is written")
            .written()
            .expect("nothing collides with it");
        let handle = EntityId("event:goodbye-party".into());
        let mut entity = NewEntity::new(handle.clone(), "Goodbye party", "an app");
        entity.parent = Some(thread);
        let first = NewFact::about(handle, "deployed", jiff::civil::date(2026, 10, 20));
        AppEvent {
            entity,
            first,
            app: "springfield-ci".into(),
            attempt: "attempt-1".into(),
            digest: "digest-1".into(),
        }
    }

    /// **The double that cannot record an app's event fails that write and nothing
    /// else.** Every other variant records it, which they did not before the
    /// double forwarded the write at all: it answered the trait's refusal for a
    /// store that cannot record one. **Paired**: the failing variant leaves nothing
    /// behind, and the healthy ones leave the event.
    #[tokio::test]
    async fn the_app_event_double_fails_the_record_and_every_other_variant_records_it() {
        let failing = Arc::new(InMemoryMemory::booted());
        let event = a_deploy_event(&failing).await;
        let refused = DownMemory(Down::AppEvent, failing.clone())
            .record_app_event(event)
            .await;
        assert!(
            matches!(refused, Err(MemoryError::Store(_))),
            "the failing variant fails with a store error: {refused:?}"
        );
        assert!(
            failing
                .list_entities(Some(EntityKind::EVENT))
                .await
                .expect("the events read")
                .is_empty(),
            "a failed record leaves nothing behind"
        );
        // The reads of the failing variant still answer.
        assert!(
            DownMemory(Down::AppEvent, failing.clone())
                .list_entities(None)
                .await
                .is_ok()
        );

        for down in [
            Down::Recall,
            Down::Writes,
            Down::WorkFields,
            Down::ThingFields,
        ] {
            let healthy = Arc::new(InMemoryMemory::booted());
            let event = a_deploy_event(&healthy).await;
            let landed = DownMemory(down, healthy.clone())
                .record_app_event(event)
                .await
                .expect("a variant that fails another read records the event");
            assert!(matches!(landed, AppOutcome::Landed(_)), "{landed:?}");
            assert_eq!(
                healthy
                    .list_entities(Some(EntityKind::EVENT))
                    .await
                    .expect("the events read")
                    .len(),
                1,
                "the event is there"
            );
        }
    }

    /// **The double that cannot read a work's fields fails the fields of a `work:`
    /// and of nothing else.** Paired with the double that cannot read a thing's:
    /// each fails its own kind and answers the other's.
    #[tokio::test]
    async fn the_work_fields_double_fails_a_works_fields_and_no_other_thing() {
        let memory = Arc::new(InMemoryMemory::booted());
        for handle in ["work:pour-the-slab", "thing:gravel-bike"] {
            memory
                .add_entity(NewEntity::new(EntityId(handle.into()), handle, "fixture"))
                .await
                .expect("written")
                .written()
                .expect("nothing collides");
        }
        let work = EntityId("work:pour-the-slab".into());
        let thing = EntityId("thing:gravel-bike".into());
        let blind_to_work = DownMemory(Down::WorkFields, memory.clone());
        assert!(
            matches!(
                blind_to_work.fields(&work).await,
                Err(MemoryError::Store(_))
            ),
            "a work's fields cannot be read"
        );
        assert!(
            blind_to_work.fields(&thing).await.is_ok(),
            "a thing's fields still can"
        );
        let blind_to_thing = DownMemory(Down::ThingFields, memory);
        assert!(
            blind_to_thing.fields(&work).await.is_ok(),
            "the thing double leaves a work's fields readable"
        );
        assert!(blind_to_thing.fields(&thing).await.is_err());
    }
}

/// A same-bot edit adding another key between recall and history must
/// invalidate the selection, just as a write after selection does.
#[tokio::test]
async fn a_repeated_set_does_not_rewrite_a_shape_changed_between_its_reads() {
    let memory = Arc::new(InMemoryMemory::booted());
    let moving = Arc::new(MovesAfterTheRead {
        inner: memory.clone(),
        armed: std::sync::atomic::AtomicBool::new(false),
        before_history: true,
    });
    let jojobot = handler_over_mentions(moving.clone());
    make_bot(&jojobot, "gamma").await;
    let sid = sid_of(&boot(&jojobot, "gamma").await).expect("gamma boots");
    ensure(&jojobot, "person:alpha").await;
    let setting = |agent: &str| {
        serde_json::from_value::<UpdateEntityArgs>(serde_json::json!({
            "handle": "person:alpha", "sets": {"agent": agent}, "sid": sid
        }))
        .unwrap()
    };
    for agent in ["a0", "a1"] {
        let receipt = json_of(
            &jojobot
                .update_entity(Parameters(setting(agent)))
                .await
                .unwrap(),
        );
        assert_ne!(receipt["status"], "blocked", "{receipt}");
    }
    let before = memory
        .recall(&EntityId("person:alpha".into()))
        .await
        .unwrap();
    assert_eq!(before.len(), 1, "an unchanged own claim rewrites");
    let address = before[0].address();

    moving
        .armed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let receipt = json_of(
        &jojobot
            .update_entity(Parameters(setting("a3")))
            .await
            .unwrap(),
    );
    assert_ne!(receipt["status"], "blocked", "{receipt}");
    assert!(!moving.armed.load(std::sync::atomic::Ordering::SeqCst));
    let after = memory.recall(&address.home).await.unwrap();
    assert_eq!(after.len(), 2, "the changed shape must append: {after:?}");
    let original = after.iter().find(|fact| fact.address() == address).unwrap();
    assert_eq!(original.fields.get("agent").map(String::as_str), Some("a2"));
    assert_eq!(
        original.fields.get("seat").map(String::as_str),
        Some("front")
    );
    let appended = after.iter().find(|fact| fact.address() != address).unwrap();
    assert_eq!(appended.fields.get("agent").map(String::as_str), Some("a3"));
}

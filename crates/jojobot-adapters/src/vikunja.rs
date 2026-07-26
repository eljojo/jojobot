//! The Vikunja store — the real [`Mailboxes`] adapter.
//!
//! **Convention over configuration.** The adapter is never handed a Vikunja id.
//! Its only config is credentials. It discovers its own project *by name* (a
//! software constant) and by an ownership marker, so it never adopts a
//! same-named project the operator made; it self-provisions the project, its
//! kanban view's three columns, and each mailbox label; and a concurrent
//! double-create self-heals to one canonical (the oldest) rather than forking.
//!
//! **The write-scope invariant.** The operator's own task boards live on this
//! same Vikunja. Every project-scoped call goes through a [`Scope`], which is
//! minted only by [`VikunjaStore::resolve_scope`] and refuses any other project
//! id **before a request leaves the process** — a mis-scoped write to a real
//! board is not something a read-back can undo. The sharp edge is a card id,
//! which is global: no card is ever written unless it was read out of jojobot's
//! own board *and* still declares that project.
//!
//! The HTTP surface is behind the [`api::VikunjaApi`] port, so all of that logic
//! runs under fast tests against an in-memory double.
//!
//! **Known quirk — no retry on 500.** Vikunja over SQLite answers concurrent
//! writes with 500s (database-lock class); this adapter surfaces them as
//! [`MailboxError::Store`] and does not retry. Every write path is
//! read-back-verified and rolls back on failure, so a 500 is loud and clean —
//! but a caller running beside another writer should expect occasional
//! transient failures and re-issue the verb itself.

mod api;
mod codec;

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;

use jojobot_domain::mailbox::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, MessageState, NewMessage, StateCounts, guard::{self, Decision},
    message_title, normalize_body, normalize_notes, validate_body, validate_mailbox_name,
    validate_message_id, validate_notes, validate_sender,
};

use api::{BoardBucket, HttpVikunja, LabelRec, ProjectRec, TaskRec, Unconfigured, VikunjaApi};
use codec::{Envelope, parse_description, render_description};

/// Vikunja's page size for list endpoints. The store pages until a short page,
/// so a match past the first page is never missed — a stop-at-one-page bug here
/// forks the project or hides half a mailbox.
const PAGE: u64 = 50;

/// The board endpoint paginates the cards **inside** each column, so it is
/// paged too: `processed` is an archive that never drains, so a mailbox project
/// outgrows one page as a matter of course.
///
/// **Never larger than [`PAGE`].** Vikunja clamps `per_page` server-side to its
/// `maxitemsperpage` setting, and asking for more does not get more — it gets
/// the cap, with nothing in the body to say the column was cut short.
const BOARD_PAGE: u64 = PAGE;

/// The marker jojobot stamps into the description of everything it creates —
/// the project and every mailbox label — and checks on match, so it only ever
/// adopts something it created itself.
const OWNER_TAG: &str = "[jojobot:owned]";

/// The separator between a mailbox label's namespace and the mailbox's name.
///
/// **Vikunja labels are global, not per-project.** A mailbox label therefore
/// shares one namespace with every facet the operator keeps on their own boards
/// — and with every other jojobot store pointed at the same Vikunja. So a label
/// is titled `<project>/<mailbox>`: the project half keeps jojobot's labels out
/// of the operator's namespace, *and* keeps a throwaway store's boxes out of the
/// real one's. It is presentation only; the mailbox's name is what follows it.
const LABEL_SEPARATOR: char = '/';

// --- secret -----------------------------------------------------------------

/// An API token that never prints itself. `Debug` redacts, so the token can't
/// leak through a `#[derive(Debug)]`, a `dbg!`, or a `tracing` field.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// Borrow the raw value — only at the point it's actually used (the bearer
    /// header). Deliberately named so call sites are greppable.
    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(\"***\")")
    }
}

// --- config -----------------------------------------------------------------

/// The store's only configuration: **credentials**. No project id, no view id,
/// no label ids — those are discovered by convention. `Debug` is safe: the token
/// redacts.
#[derive(Debug, Clone)]
pub struct VikunjaConfig {
    /// Vikunja's root URL, e.g. `https://tasks.example.org` — without the
    /// `/api/v1` suffix, which the client appends.
    pub base_url: String,
    /// API token (bearer). Redacted in `Debug`.
    pub token: Secret,
}

// --- the write scope --------------------------------------------------------

/// The one project jojobot may touch, and the kanban view its columns live in.
///
/// Minted only by [`VikunjaStore::resolve_scope`], which means no call path can
/// name a project without having discovered it as jojobot's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scope {
    project: u64,
    view: u64,
}

impl Scope {
    /// The project id, for a call that is scoped to it by construction.
    fn project(&self) -> u64 {
        self.project
    }

    /// Confirm a card belongs to jojobot's project.
    ///
    /// **This is the sharp edge of the invariant.** A card id is global in
    /// Vikunja: `POST /tasks/{id}` reaches any card the token can see, the
    /// operator's boards included, and the update carries a `project_id` — so
    /// writing to a card jojobot does not own does not merely edit it, it
    /// *moves* it onto jojobot's board. A card that turns up in jojobot's
    /// columns while declaring another project is an integrity violation, not
    /// routine noise, and every verb refuses rather than degrading around it.
    fn verify(&self, task: &TaskRec) -> Result<(), MailboxError> {
        if task.project_id == self.project {
            Ok(())
        } else {
            Err(MailboxError::ForeignProject(format!(
                "card {} declares project {}, not jojobot's mailbox project {}",
                task.id, task.project_id, self.project
            )))
        }
    }
}

/// What one pass over the board yields: the readable messages, and the cards
/// that could not be read as messages — quarantined, acted on by nothing.
struct BoardRead {
    messages: Vec<(TaskRec, Message)>,
    /// `(box, card id)` per unreadable card. The box is **`None` when the card
    /// carries no mailbox label**: a card jojobot created and could not label
    /// is still outside the funnel and still must not be lost, but there is
    /// nothing on it that says which box it belongs to, so `list_mailboxes`
    /// cannot file it under one. That residue is said at error level, with the
    /// card id, where it happens.
    quarantined: Vec<(Option<MailboxName>, u64)>,
}

// --- the store --------------------------------------------------------------

/// The real Mailboxes adapter, fronting a Vikunja project it manages by name.
/// Stateless as far as Vikunja goes: it holds an API client and the project
/// *name*, never an id. The one thing it does keep is [the lock](#the-lock).
///
/// # The lock
///
/// Every verb is a read-modify-verify sequence over the board, and two of them
/// running at once interleave: two reads both see a message in `new` and both
/// hand it over as fresh mail; a post's read-back lands after a delivery moved
/// its card. **jojobot is the only machine writer to this project** (the
/// write-scope invariant) and runs as one process, so holding a per-store async
/// mutex across each verb's whole body removes every jojobot-vs-jojobot race by
/// construction — and, incidentally, stops jojobot handing real Vikunja the
/// concurrent writes it answers with 500s.
///
/// **Accepted constraint: one server instance.** The lock is in-process, so two
/// jojobot processes pointed at one project would race again — the same posture
/// the Outline adapter takes on its read-modify-write (no compare-and-set,
/// accepted for a single-session assistant, not forgotten). What the lock
/// cannot cover is what the read-back verification is still for: the operator
/// hand-editing the board in the UI, and the store normalizing what it stored.
#[derive(Clone)]
pub struct VikunjaStore {
    api: Arc<dyn VikunjaApi>,
    project: String,
    /// One store = one project = one lock. Shared across clones, so a cloned
    /// handle is the same writer, not a second one.
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl VikunjaStore {
    /// The project jojobot manages by default. A software constant — jojobot
    /// creates and owns this project; it never touches the operator's own.
    pub const DEFAULT_PROJECT: &'static str = "jojobot-mailboxes";

    /// A store pointed at Vikunja via credentials, managing the default project.
    pub fn new(http: reqwest::Client, config: VikunjaConfig) -> Self {
        Self::with_project(http, config, Self::DEFAULT_PROJECT)
    }

    /// A store managing a named project (e.g. a throwaway one for the gated
    /// integration test). jojobot only ever creates/manages its own projects.
    pub fn with_project(
        http: reqwest::Client,
        config: VikunjaConfig,
        project: impl Into<String>,
    ) -> Self {
        let api = Arc::new(HttpVikunja::new(http, config.base_url, config.token));
        Self::from_api(api, project)
    }

    /// A store with no credentials yet — every verb returns
    /// [`MailboxError::NotConfigured`]. Lets the server boot before Vikunja is
    /// wired, without shipping a toy store.
    pub fn unconfigured() -> Self {
        Self::from_api(Arc::new(Unconfigured), Self::DEFAULT_PROJECT)
    }

    fn from_api(api: Arc<dyn VikunjaApi>, project: impl Into<String>) -> Self {
        Self {
            api,
            project: project.into(),
            lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// The description jojobot stamps on everything it creates.
    fn owner_description(&self) -> String {
        format!("Managed by jojobot — do not edit by hand. {OWNER_TAG}")
    }

    /// The namespace every mailbox label of *this* store's project is titled
    /// under. See [`LABEL_SEPARATOR`] for why a global label namespace needs one.
    fn label_prefix(&self) -> String {
        format!("{}{LABEL_SEPARATOR}", self.project)
    }

    /// Every project that is both named ours AND carries the ownership tag —
    /// paged in full.
    ///
    /// **Stop on an empty page, never on a short one.** Vikunja serves the
    /// page size *it* decides (`maxitemsperpage`), not the one requested — so
    /// "fewer than I asked for" is true on every page when the server's cap is
    /// below the request, and a loop that stops there reads page one and
    /// concludes the project is absent. Same rule as [`Self::board`].
    async fn owned_projects(&self) -> Result<Vec<ProjectRec>, MailboxError> {
        let mut owned = Vec::new();
        let mut page = 1;
        loop {
            let batch = self.api.list_projects(page, PAGE).await?;
            if batch.is_empty() {
                break;
            }
            owned.extend(
                batch
                    .into_iter()
                    .filter(|p| p.title == self.project && p.description.contains(OWNER_TAG)),
            );
            page += 1;
        }
        Ok(owned)
    }

    /// The mailbox project's id, creating it if absent. After a create it
    /// re-lists and picks the canonical (oldest) owned project, so a concurrent
    /// double-create converges on one rather than forking.
    async fn resolve_project(&self) -> Result<u64, MailboxError> {
        if let Some(p) = oldest(self.owned_projects().await?, |p| (p.created.as_str(), p.id)) {
            return Ok(p.id);
        }
        self.api
            .create_project(&self.project, &self.owner_description())
            .await?;
        oldest(self.owned_projects().await?, |p| (p.created.as_str(), p.id))
            .map(|p| p.id)
            .ok_or_else(|| MailboxError::Store("mailbox project missing after create".into()))
    }

    /// The scope every other call runs inside: jojobot's project, its kanban
    /// view, and the three columns present. Idempotent — it provisions whatever
    /// is missing and adopts whatever is already there.
    async fn resolve_scope(&self) -> Result<Scope, MailboxError> {
        let project = self.resolve_project().await?;
        let view = self
            .api
            .list_views(project)
            .await?
            .into_iter()
            .filter(|v| v.kind == "kanban")
            .map(|v| v.id)
            .min()
            .ok_or_else(|| {
                MailboxError::Store(format!(
                    "project {project} has no kanban view — columns are where state lives"
                ))
            })?;
        let scope = Scope { project, view };
        self.ensure_columns(&scope).await?;
        Ok(scope)
    }

    /// Make sure the board carries one column per state. Missing ones are
    /// created in funnel order; anything else on the board is left alone.
    async fn ensure_columns(&self, scope: &Scope) -> Result<(), MailboxError> {
        let existing = self.api.list_buckets(scope.project(), scope.view).await?;
        for state in MessageState::ALL {
            if !existing.iter().any(|b| b.title == state.as_token()) {
                self.api
                    .create_bucket(scope.project(), scope.view, state.as_token())
                    .await?;
            }
        }
        Ok(())
    }

    /// The bucket id for a state, on this board.
    async fn column(&self, scope: &Scope, state: MessageState) -> Result<u64, MailboxError> {
        self.api
            .list_buckets(scope.project(), scope.view)
            .await?
            .into_iter()
            .find(|b| b.title == state.as_token())
            .map(|b| b.id)
            .ok_or_else(|| {
                MailboxError::Store(format!("the board has no '{state}' column"))
            })
    }

    /// Every mailbox label jojobot owns — paged in full. Labels are global in
    /// Vikunja, so both halves of the marker matter: the title prefix keeps the
    /// namespace disjoint from the operator's own facets, and the owner tag is
    /// what proves jojobot created it.
    async fn mailbox_labels(&self) -> Result<Vec<(MailboxName, u64)>, MailboxError> {
        let prefix = self.label_prefix();
        let mut owned: Vec<LabelRec> = Vec::new();
        let mut page = 1;
        // Stop on an empty page, never on a short one — see `owned_projects`.
        loop {
            let batch = self.api.list_labels(page, PAGE).await?;
            if batch.is_empty() {
                break;
            }
            owned.extend(batch.into_iter().filter(|l| {
                l.title.starts_with(&prefix) && l.description.contains(OWNER_TAG)
            }));
            page += 1;
        }
        // Oldest wins, so a concurrent double-create of one mailbox converges
        // rather than leaving two labels answering to one name.
        owned.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
        let mut seen = std::collections::HashSet::new();
        Ok(owned
            .into_iter()
            .filter_map(|l| {
                let name = MailboxName(l.title.strip_prefix(&prefix)?.to_string());
                seen.insert(name.clone()).then_some((name, l.id))
            })
            .collect())
    }

    /// Just the names — what the guard screens against.
    async fn mailbox_names(&self) -> Result<Vec<MailboxName>, MailboxError> {
        Ok(self.mailbox_labels().await?.into_iter().map(|(n, _)| n).collect())
    }

    /// The whole board, paged until every column returns a short page. A column
    /// that filled its page is why this loops: truncating there would silently
    /// under-report a mailbox's counts and hide the oldest messages in it.
    async fn board(&self, scope: &Scope) -> Result<Vec<BoardBucket>, MailboxError> {
        let mut merged: Vec<BoardBucket> = Vec::new();
        let mut page = 1;
        loop {
            let batch = self.api.board(scope.project(), scope.view, page, BOARD_PAGE).await?;
            if batch.is_empty() {
                break;
            }
            // **Stop on an empty page, never on a short one.** The obvious test
            // — "no column filled the page I asked for" — compares against the
            // size *requested*, and Vikunja serves the size it decides: a
            // requested page larger than the server's cap is never filled, so
            // that test reads page one and calls it the board. Asking until
            // every column has nothing left needs no agreement about page size
            // at all.
            let mut any = false;
            for bucket in batch {
                any |= !bucket.tasks.is_empty();
                match merged.iter_mut().find(|b| b.id == bucket.id) {
                    Some(existing) => existing.tasks.extend(bucket.tasks),
                    None => merged.push(bucket),
                }
            }
            if !any {
                break;
            }
            page += 1;
        }
        Ok(merged)
    }

    /// Every message on the board, with the card it came from — the one read
    /// path, so counts, deliveries and lookups can never disagree about what is
    /// where.
    ///
    /// A card that cannot be read as a message is **quarantined**: never
    /// delivered, never invented a reading for, but surfaced as unreadable,
    /// because a real message silently skipped is invisible to every verb at
    /// once. Two things put a card there — a description that no longer parses,
    /// and a column that is no state.
    ///
    /// **Quarantine is decided by the column, not by the label.** An unlabelled
    /// card in one of the three state columns is somebody else's — a human's
    /// note on the board — and jojobot neither delivers it nor counts it.
    /// Outside those columns there is no such thing as somebody else's card on
    /// this board: it is jojobot's own project, and the one way a card gets
    /// there unlabelled is a create whose labelling failed. Reading that card as
    /// "not mine" is how a message jojobot itself wrote becomes invisible to
    /// every verb, quarantine included.
    async fn board_read(&self, scope: &Scope) -> Result<BoardRead, MailboxError> {
        let prefix = self.label_prefix();
        let mut found = Vec::new();
        let mut quarantined = Vec::new();
        for bucket in self.board(scope).await? {
            let state = MessageState::from_token(&bucket.title);
            for task in bucket.tasks {
                let mailbox = task
                    .labels
                    .iter()
                    .find_map(|l| l.strip_prefix(&prefix))
                    .map(|n| MailboxName(n.to_string()));
                // A card with no mailbox label sitting in one of jojobot's three
                // state columns is somebody else's: it is not a message and not
                // jojobot's to touch.
                if state.is_some() && mailbox.is_none() {
                    continue;
                }
                // The one choke point for the write-scope invariant: every card
                // jojobot ever writes to arrives either from here or from a
                // `create_task` in its own project, so checking here covers
                // every verb at once. **Above the quarantine branches, not
                // below them** — a foreign card must be refused outright, never
                // classified, or its id gets published under one of jojobot's
                // mailboxes as if jojobot had a claim on it.
                scope.verify(&task)?;
                let (Some(state), Some(box_of)) = (state, mailbox.clone()) else {
                    // The card id only: a non-state column title is operator- or
                    // Vikunja-authored text, and no log prints operator text.
                    tracing::warn!(
                        card = task.id,
                        attributable = mailbox.is_some(),
                        "a card sits in a column that is no state — quarantined, not delivered"
                    );
                    quarantined.push((mailbox, task.id));
                    continue;
                };
                let Some((body, envelope)) = parse_description(&task.description) else {
                    tracing::warn!(
                        card = task.id,
                        "a card wearing a mailbox label carries no readable machine block — \
                         quarantined, not delivered"
                    );
                    quarantined.push((Some(box_of), task.id));
                    continue;
                };
                let mailbox = box_of;
                let message = Message {
                    id: MessageId(task.id.to_string()),
                    mailbox,
                    body,
                    sender: envelope.sender,
                    sent_at: envelope.sent_at,
                    state,
                    notes: envelope.notes,
                };
                found.push((task, message));
            }
        }
        // Oldest first, by the instant the sender declared; the card id breaks a
        // tie, so the order is total and two reads agree.
        found.sort_by(|a, b| {
            a.1.sent_at
                .cmp(&b.1.sent_at)
                .then_with(|| a.0.id.cmp(&b.0.id))
        });
        quarantined.sort_by_key(|(_, id)| *id);
        Ok(BoardRead {
            messages: found,
            quarantined,
        })
    }

    /// The readable messages alone — what every verb but `list_mailboxes`
    /// consumes. Quarantined cards are deliberately absent: they are surfaced,
    /// never acted on.
    async fn messages(&self, scope: &Scope) -> Result<Vec<(TaskRec, Message)>, MailboxError> {
        Ok(self.board_read(scope).await?.messages)
    }

    /// Read one message back through the read path — the verification half of
    /// every write.
    async fn read_back(&self, scope: &Scope, id: &MessageId) -> Result<Message, MailboxError> {
        self.messages(scope)
            .await?
            .into_iter()
            .find(|(_, m)| &m.id == id)
            .map(|(_, m)| m)
            .ok_or_else(|| MailboxError::Store(format!("message {id} did not read back")))
    }

    /// A card ready to be written back: exactly what Vikunja handed over, with
    /// the two fields jojobot owns replaced.
    ///
    /// **Everything else has to ride along.** Vikunja's task update writes the
    /// whole model, so a field left out of the payload is written back as its
    /// zero value — a due date, a priority, an assignee, the kanban position,
    /// all blanked by an edit that only meant to record an outcome.
    fn card_with(card: &TaskRec, title: &str, description: &str) -> serde_json::Value {
        let mut payload = card.raw.clone();
        if !payload.is_object() {
            payload = serde_json::json!({ "project_id": card.project_id });
        }
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("id".into(), card.id.into());
            obj.insert("title".into(), title.into());
            obj.insert("description".into(), description.into());
        }
        payload
    }

    /// Put a whole batch of cards back in the columns they were in. A delivery
    /// moves every message before it verifies any of them, so a failure has to
    /// undo **all** of them: restoring only the one that failed leaves the rest
    /// marked delivered, to a caller that was told the call failed and never
    /// received them.
    ///
    /// With one exception — **the same later-state-wins rule the read-back
    /// applies**: a card that advanced PAST the state this call wrote
    /// (`written`) was taken and consumed by someone in the meantime, and that
    /// consumer's `mark_processed` was already told success. Restoring it would
    /// erase their recorded outcome and redeliver a handled message, so it is
    /// skipped. If the board cannot be re-read, everything is restored: a card
    /// wrongly put back is redelivered flagged as a leftover, never lost.
    async fn restore_all(
        &self,
        scope: &Scope,
        cards: &[(TaskRec, MessageState)],
        written: MessageState,
        verb: &str,
    ) -> String {
        let advanced: std::collections::HashSet<u64> = match self.board_read(scope).await {
            Ok(board) => board
                .messages
                .iter()
                .filter(|(_, m)| m.state > written)
                .map(|(task, _)| task.id)
                .collect(),
            Err(_) => Default::default(),
        };
        let mut failures = Vec::new();
        for (card, state) in cards {
            if advanced.contains(&card.id) {
                continue;
            }
            if let Err(failure) = self.restore(scope, card, *state).await {
                failures.push(format!("card {}: {failure}", card.id));
            }
        }
        if failures.is_empty() {
            format!("every card this {verb} moved was put back")
        } else {
            format!("AND restoring part of this {verb} failed ({})", failures.join("; "))
        }
    }

    /// Put a card back the way a failed write found it. A read-back mismatch
    /// means the store transformed what was written; leaving the transformed
    /// card behind hands a garbled message to the next consumer. Best-effort:
    /// the outcome is typed — a caller reports it, never string-matches it —
    /// and an `Err` carries the first thing that failed, so the error can say
    /// which state the card is actually in.
    async fn restore(
        &self,
        scope: &Scope,
        task: &TaskRec,
        state: MessageState,
    ) -> Result<(), MailboxError> {
        let description = self
            .api
            .update_task(
                scope.project(),
                &Self::card_with(task, &task.title, &task.description),
            )
            .await;
        let column = match self.column(scope, state).await {
            Ok(bucket) => self
                .api
                .move_task(scope.project(), scope.view, bucket, task.id)
                .await,
            Err(e) => Err(e),
        };
        match (description, column) {
            (Ok(()), Ok(())) => Ok(()),
            (d, c) => Err(d.err().or(c.err()).expect("at least one side failed")),
        }
    }

    /// The clause a failed verb appends about its restore attempt.
    fn restore_clause(outcome: Result<(), MailboxError>, verb: &str) -> String {
        match outcome {
            Ok(()) => format!("the card was restored to its state before this {verb}"),
            Err(e) => format!("AND restoring the card failed ({e}) — it may be left mid-{verb}"),
        }
    }

    /// The `parked` column's id, creating the column on demand. Deliberately
    /// **not a state token**: a card here has no place in the funnel, so the
    /// board read quarantines it — surfaced by `list_mailboxes`, counted as
    /// nothing, delivered to nobody.
    async fn parking_column(&self, scope: &Scope) -> Result<u64, MailboxError> {
        if let Some(bucket) = self
            .api
            .list_buckets(scope.project(), scope.view)
            .await?
            .into_iter()
            .find(|b| b.title == PARKED_COLUMN)
        {
            return Ok(bucket.id);
        }
        Ok(self
            .api
            .create_bucket(scope.project(), scope.view, PARKED_COLUMN)
            .await?
            .id)
    }

    /// Park a card jojobot created seconds ago that a failed write cannot vouch
    /// for: move it to the [`PARKED_COLUMN`], where it is quarantined — never
    /// delivered as mail, and never counted either. Parking it in `processed`
    /// instead would fabricate handled mail nobody ever received; leaving it in
    /// `new` would deliver a message the caller was told failed. **Nothing is
    /// ever deleted**: jojobot has no delete capability at all, so a rollback's
    /// only moves are restore and park. A create has no prior state to restore
    /// to — its prior state is absence — which is why parking is what stands in.
    ///
    /// **It re-issues nothing.** This used to relabel the card first, so that
    /// the board read could see it — which meant that when the label call was
    /// what failed, the rescue's first act was the call that had just failed,
    /// and a Vikunja refusing it refused the retry too. Visibility no longer
    /// depends on this working at all: quarantine is decided by the column, so
    /// a created card is outside the funnel and surfaced whether this move
    /// lands or not. This only tidies it into the column that says so.
    async fn park_create(&self, scope: &Scope, task_id: u64, verb: &str) -> String {
        let parked = async {
            let bucket = self.parking_column(scope).await?;
            self.api
                .move_task(scope.project(), scope.view, bucket, task_id)
                .await
        }
        .await;
        match parked {
            Ok(()) => format!(
                "the card this {verb} created was parked in '{PARKED_COLUMN}' — quarantined and \
                 surfaced by list_mailboxes; nothing is deleted"
            ),
            Err(e) => {
                tracing::error!(
                    card = task_id,
                    "a card this {verb} created could not be parked — it is quarantined where the \
                     failure left it (outside the funnel, delivered to nobody), but it is not in \
                     the column that says so"
                );
                format!(
                    "AND parking the card this {verb} created failed ({e}) — card {task_id} is \
                     left where the failure found it"
                )
            }
        }
    }
}

/// Where a failed write's card is parked. **Not a state on purpose** — a card
/// here is quarantined, outside the funnel, and the board read surfaces it as
/// unreadable instead of counting it as mail.
const PARKED_COLUMN: &str = "parked";

/// Whether a read-back vouches for a write: it returned exactly what was
/// written — or the same message **further down the funnel**. A card that
/// advanced past the written state was received and consumed by someone between
/// the write and its verification; that is delivery working, not corruption,
/// and rolling it back would destroy a message a consumer already has. Notes
/// are not compared once the state advanced: they belong to whichever consumer
/// moved the card.
fn read_back_confirms(expected: &Message, seen: &Message) -> bool {
    if seen == expected {
        return true;
    }
    seen.state > expected.state
        && seen.id == expected.id
        && seen.mailbox == expected.mailbox
        && seen.body == expected.body
        && seen.sender == expected.sender
        && seen.sent_at == expected.sent_at
}

/// The deterministic canonical winner: oldest by the record's own creation
/// stamp, ties broken by id. Both are stable across list calls, so every session
/// agrees on which one is canonical.
fn oldest<T>(mut items: Vec<T>, key: impl Fn(&T) -> (&str, u64)) -> Option<T> {
    items.sort_by(|a, b| key(a).cmp(&key(b)));
    items.into_iter().next()
}

#[async_trait]
impl Mailboxes for VikunjaStore {
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        create_new: bool,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        let _serialized = self.lock.lock().await;
        validate_mailbox_name(name)?;
        // Resolving the scope first is what makes the very first call to a bare
        // Vikunja work: the project and its columns are provisioned before the
        // box that will live on them.
        self.resolve_scope().await?;

        let existing = self.mailbox_names().await?;
        if let Decision::Block(candidates) = guard::decide_create(name, &existing, create_new) {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }

        self.api
            .create_label(
                &format!("{}{name}", self.label_prefix()),
                &self.owner_description(),
            )
            .await?;

        // Read-back: the box exists once the read path returns it. A create has
        // no prior state to restore to, so — as in the Memory adapter — a
        // mismatch errors rather than rolling anything back.
        if !self.mailbox_names().await?.contains(name) {
            return Err(MailboxError::Store(format!(
                "mailbox '{name}' did not read back after its label was created"
            )));
        }
        Ok(Guarded::Written(Mailbox {
            name: name.clone(),
            counts: StateCounts::default(),
            quarantined: Vec::new(),
        }))
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        let _serialized = self.lock.lock().await;
        let scope = self.resolve_scope().await?;
        let board = self.board_read(&scope).await?;
        Ok(self
            .mailbox_names()
            .await?
            .into_iter()
            .map(|name| {
                let mut counts = StateCounts::default();
                for (_, message) in board.messages.iter().filter(|(_, m)| m.mailbox == name) {
                    counts.add(message.state);
                }
                let quarantined = board
                    .quarantined
                    .iter()
                    .filter(|(mailbox, _)| mailbox.as_ref() == Some(&name))
                    .map(|(_, id)| MessageId(id.to_string()))
                    .collect();
                Mailbox {
                    name,
                    counts,
                    quarantined,
                }
            })
            .collect())
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        let _serialized = self.lock.lock().await;
        validate_mailbox_name(&message.mailbox)?;
        validate_sender(&message.sender)?;
        validate_body(&message.body)?;
        let scope = self.resolve_scope().await?;

        // The mailbox must already exist — this verb never provisions one. A
        // typo that mints a box is a message posted where nobody is listening,
        // and it looks exactly like success.
        let labels = self.mailbox_labels().await?;
        let names: Vec<MailboxName> = labels.iter().map(|(n, _)| n.clone()).collect();
        if let Decision::Block(candidates) = guard::decide_existing(&message.mailbox, &names) {
            return Ok(Guarded::Blocked {
                attempted: message.mailbox,
                candidates,
            });
        }
        let label = labels
            .iter()
            .find(|(n, _)| n == &message.mailbox)
            .map(|(_, id)| *id)
            .ok_or_else(|| {
                MailboxError::Store(format!(
                    "mailbox '{}' lost its label mid-write",
                    message.mailbox
                ))
            })?;

        let body = normalize_body(&message.body);
        let sender = message.sender.trim().to_string();
        let envelope = Envelope {
            sender: sender.clone(),
            sent_at: message.sent_at,
            notes: None,
        };
        let card = self
            .api
            .create_task(
                scope.project(),
                &message_title(&sender, &body),
                &render_description(&body, &envelope),
            )
            .await?;

        // A fresh card carries neither its mailbox nor its state: Vikunja drops
        // it in the view's default column and gives it no labels, and neither
        // can be set in the create call — the task model's `labels` field is
        // not a column (`xorm:"-"`), and the create handler echoes back the
        // payload it was given, so a create carrying labels answers 201 with
        // the labels in the response and none of them stored.
        //
        // **Every step from here on rolls the card back.** A `?` that returned
        // early left the created card stranded in the default column.
        let placed = async {
            self.api
                .set_task_labels(card.id, &[label])
                .await
                .inspect_err(|_| {
                    // **The residue class, and the only one left.** The card is
                    // outside the funnel and quarantined by its column, so no
                    // verb will deliver or count it — but with no mailbox label
                    // there is nothing on it saying which box it belongs to, so
                    // `list_mailboxes` cannot file it under one. The card id is
                    // said here instead, at error level, because a card only a
                    // log knows about is one a human has to be told about.
                    tracing::error!(
                        card = card.id,
                        "a card this post_message created could not be labelled — it is \
                         quarantined, but no box can claim it and list_mailboxes cannot show it"
                    );
                })?;
            let new_column = self.column(&scope, MessageState::New).await?;
            self.api
                .move_task(scope.project(), scope.view, new_column, card.id)
                .await
        }
        .await;
        if let Err(e) = placed {
            let parked = self.park_create(&scope, card.id, "post_message").await;
            return Err(MailboxError::Store(format!("{e}; {parked}")));
        }

        let expected = Message {
            id: MessageId(card.id.to_string()),
            mailbox: message.mailbox,
            body,
            sender,
            sent_at: message.sent_at,
            state: MessageState::New,
            notes: None,
        };
        match self.read_back(&scope, &expected.id).await {
            Ok(seen) if read_back_confirms(&expected, &seen) => Ok(Guarded::Written(seen)),
            outcome => {
                let parked = self.park_create(&scope, card.id, "post_message").await;
                Err(MailboxError::Store(match outcome {
                    Ok(seen) => format!(
                        "message {} read back changed: wrote {expected:?}, read {seen:?}; {parked}",
                        expected.id
                    ),
                    Err(e) => format!("{e}; {parked}"),
                }))
            }
        }
    }

    async fn read_mailbox(&self, name: &MailboxName) -> Result<Guarded<Delivery>, MailboxError> {
        let _serialized = self.lock.lock().await;
        validate_mailbox_name(name)?;
        let scope = self.resolve_scope().await?;

        let existing = self.mailbox_names().await?;
        if let Decision::Block(candidates) = guard::decide_existing(name, &existing) {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }

        let owed: Vec<(TaskRec, Message)> = self
            .messages(&scope)
            .await?
            .into_iter()
            .filter(|(_, m)| &m.mailbox == name && m.state.is_unprocessed())
            .collect();

        let read_column = self.column(&scope, MessageState::Read).await?;
        let mut delivered = Vec::with_capacity(owed.len());
        // Every card this call actually moved, with the column it came from —
        // so a failure anywhere in the batch can put all of them back.
        let mut moved: Vec<(TaskRec, MessageState)> = Vec::new();
        for (card, message) in owed {
            let seen_before = message.state == MessageState::Read;
            if !seen_before {
                if let Err(e) = self
                    .api
                    .move_task(scope.project(), scope.view, read_column, card.id)
                    .await
                {
                    let restored = self.restore_all(&scope, &moved, MessageState::Read, "read_mailbox").await;
                    return Err(MailboxError::Store(format!("{e}; {restored}")));
                }
                moved.push((card.clone(), message.state));
            }
            delivered.push((card, message, seen_before));
        }

        // Read-back: a delivery is only a delivery once the column moved. A
        // message reported as delivered but still sitting in `new` would be
        // handed to the next consumer as fresh mail — the duplicate-delivery
        // bug, reported as success. A message that moved the *other* way, into
        // `processed`, is dropped from the batch: see the arm below.
        let after = self.messages(&scope).await?;
        let mut messages = Vec::with_capacity(delivered.len());
        for (card, expected, seen_before) in delivered {
            let seen = after
                .iter()
                .find(|(_, m)| m.id == expected.id)
                .map(|(_, m)| m.clone());
            let expected_read = Message {
                state: MessageState::Read,
                ..expected.clone()
            };
            match seen {
                Some(seen) if seen == expected_read => messages.push(Delivered {
                    message: seen,
                    seen_before,
                }),
                // **Advanced past `read` — dropped, not delivered.** The
                // read-back's later-state tolerance is right for a post: a
                // message that was consumed still exists, so the post
                // succeeded. Here it is the opposite. Under the verb lock a
                // card can only reach `processed` between this delivery's move
                // and its verification by a hand on the board, which means
                // somebody handled it — and handing it to a consumer as fresh
                // mail is exactly the double-processing this whole context
                // exists to prevent. It is not rolled back either: the outcome
                // that handler recorded stands.
                Some(seen) if read_back_confirms(&expected_read, &seen) => {
                    tracing::warn!(
                        card = %expected.id,
                        state = %seen.state,
                        "a message advanced past `read` while this delivery was in flight — \
                         somebody handled it, so it is dropped from the delivery, not handed over"
                    );
                }
                seen => {
                    // The whole batch goes back, not just this card: the caller
                    // is being told the call failed, so nothing in it may stay
                    // marked delivered.
                    let _ = &card;
                    let restored = self.restore_all(&scope, &moved, MessageState::Read, "read_mailbox").await;
                    return Err(MailboxError::Store(format!(
                        "message {} did not read back as delivered: expected {expected_read:?}, \
                         read {seen:?}; {restored}",
                        expected.id
                    )));
                }
            }
        }
        Ok(Guarded::Written(Delivery {
            mailbox: name.clone(),
            messages,
        }))
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        let _serialized = self.lock.lock().await;
        validate_message_id(id)?;
        validate_notes(notes)?;
        let scope = self.resolve_scope().await?;

        // The card is found by walking jojobot's OWN board. An id that is not on
        // it is a miss — never a lookup by raw id, which in Vikunja would reach
        // any card the token can see, the operator's boards included.
        let (card, message) = self
            .messages(&scope)
            .await?
            .into_iter()
            .find(|(_, m)| &m.id == id)
            .ok_or_else(|| MailboxError::UnknownMessage {
                attempted: id.to_string(),
            })?;

        let notes = normalize_notes(notes).or(message.notes.clone());
        let envelope = Envelope {
            sender: message.sender.clone(),
            sent_at: message.sent_at,
            notes: notes.clone(),
        };
        // The outcome is written first, then the column moves. A `?` between the
        // two left the message carrying a recorded outcome while still sitting
        // in a column a read delivers — so the next consumer would act a second
        // time on a message that already says it was handled.
        let retired = async {
            self.api
                .update_task(
                    scope.project(),
                    &Self::card_with(
                        &card,
                        &card.title,
                        &render_description(&message.body, &envelope),
                    ),
                )
                .await?;
            let processed_column = self.column(&scope, MessageState::Processed).await?;
            self.api
                .move_task(scope.project(), scope.view, processed_column, card.id)
                .await
        }
        .await;
        if let Err(e) = retired {
            let restored = Self::restore_clause(
                self.restore(&scope, &card, message.state).await,
                "mark_processed",
            );
            return Err(MailboxError::Store(format!("{e}; {restored}")));
        }

        let expected = Message {
            state: MessageState::Processed,
            notes,
            ..message.clone()
        };
        match self.read_back(&scope, id).await {
            Ok(seen) if seen == expected => Ok(seen),
            outcome => {
                let restored = Self::restore_clause(
                    self.restore(&scope, &card, message.state).await,
                    "mark_processed",
                );
                Err(MailboxError::Store(match outcome {
                    Ok(seen) => format!(
                        "message {id} read back changed: wrote {expected:?}, read {seen:?}; \
                         {restored}"
                    ),
                    Err(e) => format!("{e}; {restored}"),
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use jiff::Timestamp;
    use jojobot_domain::mailbox::testing::contract;

    use super::api::{BucketRec, ViewRec};
    use super::*;

    /// In-memory [`VikunjaApi`] double. Ids and creation stamps are a monotonic
    /// counter (zero-padded, so lexicographic order is chronological) — no
    /// clock, fully deterministic.
    ///
    /// **Honest about the Vikunja behaviours that matter here.** A fake that
    /// simply does what the store hopes proves nothing; each of these is a
    /// behaviour the real store has, and each one hides a bug when it is absent:
    ///
    /// * a created project comes with its four default views, and the kanban
    ///   view starts with **one default column of Vikunja's own naming** — so a
    ///   created card lands somewhere that is not `new`, and a store that forgot
    ///   to move it would post messages nobody can read;
    /// * a created card goes to the view's **default (first) column**, not to
    ///   the one the caller wanted;
    /// * `update_task` **replaces** title and description rather than merging;
    /// * lists **paginate**, including the cards inside each column on the board
    ///   endpoint.
    #[derive(Default)]
    struct FakeVikunja {
        seq: AtomicU64,
        projects: Mutex<Vec<ProjectRec>>,
        views: Mutex<Vec<(u64, ViewRec)>>,
        buckets: Mutex<Vec<(u64, u64, BucketRec)>>,
        tasks: Mutex<Vec<TaskRec>>,
        /// task id → bucket id.
        placement: Mutex<HashMap<u64, u64>>,
        labels: Mutex<Vec<LabelRec>>,
        /// task id → label ids.
        task_labels: Mutex<HashMap<u64, Vec<u64>>>,
        /// Arms a mangled description for the next `update_task`/`create_task` —
        /// the induced fault behind the restore contract.
        poison: AtomicBool,
        /// Every project id any call named. The write-scope invariant is
        /// asserted against this: it must only ever hold jojobot's own.
        named_projects: Mutex<std::collections::HashSet<u64>>,
        /// Every card any call wrote to. A card id is global in Vikunja, so
        /// this is the half of the invariant a project id cannot cover.
        written_tasks: Mutex<std::collections::HashSet<u64>>,
        /// Arms a transport failure for the next call to the named method — the
        /// induced fault behind the rollback contracts. A write path has more
        /// damage on every other. `None` remaining means *every* call to that
        /// method fails, for good — a rescue path cannot dodge it by retrying.
        fail_next: Mutex<Option<(&'static str, Option<u64>)>>,
        /// The server-side `maxitemsperpage` this fake enforces. See
        /// [`DEFAULT_PAGE_CAP`].
        page_cap: AtomicU64,
    }

    /// The column Vikunja gives a fresh kanban view. Deliberately not one of
    /// jojobot's three: a card that lands here is a card the store must move.
    const DEFAULT_COLUMN: &str = "Backlog";

    /// **Vikunja clamps `per_page` server-side** to its `maxitemsperpage`
    /// setting — 50 by default — in the read-all handler every list route
    /// shares, and the cap reaches the board endpoint as a limit on the cards
    /// returned *per column*. Asking for more does not get more: it gets the
    /// cap, with nothing in the body to say there was more.
    ///
    /// The fake enforces it, and the cap is **configurable per test**: with the
    /// cap equal to the store's own page size, a paging loop that compares the
    /// count it got against the size it *requested* can never take its broken
    /// branch — the tests that could not fail. Paging tests drop the cap below
    /// the requested size so the broken stop condition actually stops early.
    const DEFAULT_PAGE_CAP: u64 = 50;

    impl FakeVikunja {
        /// Clamp list responses to `cap` items per page (per column, on the
        /// board endpoint), as a Vikunja with `maxitemsperpage = cap` would.
        fn cap_pages_at(&self, cap: u64) {
            self.page_cap.store(cap, Ordering::SeqCst);
        }

        /// The page size the server will actually serve for a requested one.
        fn served(&self, per_page: u64) -> usize {
            per_page.min(self.page_cap.load(Ordering::SeqCst)) as usize
        }
    }

    impl FakeVikunja {
        /// Record a project id a call named — the write-scope evidence.
        fn named(&self, project: u64) {
            self.named_projects.lock().unwrap().insert(project);
        }

        /// Record a card a call wrote to.
        fn wrote(&self, task: u64) {
            self.written_tasks.lock().unwrap().insert(task);
        }

        fn new() -> Arc<Self> {
            let fake = Self::default();
            fake.cap_pages_at(DEFAULT_PAGE_CAP);
            Arc::new(fake)
        }

        fn stamp(&self) -> String {
            format!("{:020}", self.seq.fetch_add(1, Ordering::SeqCst))
        }

        fn next_id(&self) -> u64 {
            self.seq.fetch_add(1, Ordering::SeqCst) + 1
        }

        /// Mangle the description of the next write before it lands.
        fn poison_next_write(&self) {
            self.poison.store(true, Ordering::SeqCst);
        }

        /// Make the next call to `method` fail as a transport error would.
        fn fail_next(&self, method: &'static str) {
            self.fail_nth(method, 1);
        }

        /// Make the **nth** call to `method` from now on fail. A write path with
        /// several steps only shows its rollback gaps when the failure lands
        /// part-way through it, not on the first step — which is exactly the
        /// case a fail-the-next-call injector cannot reach.
        fn fail_nth(&self, method: &'static str, nth: u64) {
            *self.fail_next.lock().unwrap() = Some((method, Some(nth)));
        }

        /// Make **every** call to `method` fail, for good. A one-shot injector
        /// cannot reach a rescue path that re-issues the call that just failed:
        /// the retry succeeds, and the hole it was covering stays invisible.
        /// A method Vikunja is refusing outright refuses the retry too.
        fn fail_all(&self, method: &'static str) {
            *self.fail_next.lock().unwrap() = Some((method, None));
        }

        /// Fail here if this call is the armed one.
        fn maybe_fail(&self, method: &'static str) -> Result<(), MailboxError> {
            let mut armed = self.fail_next.lock().unwrap();
            let Some((target, remaining)) = armed.as_mut() else {
                return Ok(());
            };
            if *target != method {
                return Ok(());
            }
            let Some(remaining) = remaining.as_mut() else {
                return Err(MailboxError::Store(format!("induced failure in {method}")));
            };
            *remaining -= 1;
            if *remaining == 0 {
                *armed = None;
                return Err(MailboxError::Store(format!("induced failure in {method}")));
            }
            Ok(())
        }

        fn mangle(&self, description: &str) -> String {
            if self.poison.swap(false, Ordering::SeqCst) {
                // The machine block does not survive — exactly the shape of a
                // store that reformats what it was handed.
                description.replace("sent-at:", "sent~at:")
            } else {
                description.to_string()
            }
        }

        /// Pre-seed a project; returns its id. `owned` stamps jojobot's marker.
        fn seed_project(&self, title: &str, description: &str) -> u64 {
            let id = self.next_id();
            self.projects.lock().unwrap().push(ProjectRec {
                id,
                title: title.into(),
                description: description.into(),
                created: self.stamp(),
            });
            self.seed_views(id);
            id
        }

        /// A new project comes with Vikunja's default views, and its kanban view
        /// comes with one default column.
        fn seed_views(&self, project: u64) {
            let mut views = self.views.lock().unwrap();
            for kind in ["list", "gantt", "table", "kanban"] {
                let id = self.next_id();
                views.push((project, ViewRec { id, kind: kind.into() }));
                if kind == "kanban" {
                    let bucket = self.next_id();
                    self.buckets.lock().unwrap().push((
                        project,
                        id,
                        BucketRec { id: bucket, title: DEFAULT_COLUMN.into() },
                    ));
                }
            }
        }

        fn kanban_view(&self, project: u64) -> u64 {
            self.views
                .lock()
                .unwrap()
                .iter()
                .find(|(p, v)| *p == project && v.kind == "kanban")
                .map(|(_, v)| v.id)
                .expect("a project has a kanban view")
        }

        /// The first column of a view — where Vikunja puts a newly created card.
        fn default_bucket(&self, project: u64, view: u64) -> Option<u64> {
            self.buckets
                .lock()
                .unwrap()
                .iter()
                .find(|(p, v, _)| *p == project && *v == view)
                .map(|(_, _, b)| b.id)
        }

        fn projects_titled(&self, title: &str) -> Vec<ProjectRec> {
            self.projects
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.title == title)
                .cloned()
                .collect()
        }

        fn owned_titled(&self, title: &str) -> usize {
            self.projects_titled(title)
                .iter()
                .filter(|p| p.description.contains(OWNER_TAG))
                .count()
        }

        fn tasks_in(&self, project: u64) -> Vec<TaskRec> {
            self.tasks
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.project_id == project)
                .cloned()
                .collect()
        }

        /// Set a field on a card that jojobot's own model knows nothing about —
        /// the operator reaching into Vikunja and setting a due date or a
        /// priority on a message card.
        fn set_field(&self, task: u64, key: &str, value: serde_json::Value) {
            let mut tasks = self.tasks.lock().unwrap();
            let card = tasks.iter_mut().find(|t| t.id == task).expect("card exists");
            card.raw[key] = value;
        }

        /// Read such a field back.
        fn field(&self, task: u64, key: &str) -> serde_json::Value {
            self.tasks
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.id == task)
                .map(|t| t.raw[key].clone())
                .unwrap_or(serde_json::Value::Null)
        }

        /// The title of the column a card sits in.
        fn column_of(&self, task: u64) -> Option<String> {
            let bucket = *self.placement.lock().unwrap().get(&task)?;
            self.buckets
                .lock()
                .unwrap()
                .iter()
                .find(|(_, _, b)| b.id == bucket)
                .map(|(_, _, b)| b.title.clone())
        }

        /// Attach labels to a card directly, without going through the store —
        /// for seeding a board the way a hand edit would leave it.
        fn seed_task(&self, project: u64, title: &str, description: &str, labels: &[u64]) -> u64 {
            let id = self.next_id();
            self.tasks.lock().unwrap().push(TaskRec {
                id,
                project_id: project,
                title: title.into(),
                description: description.into(),
                labels: Vec::new(),
                raw: serde_json::json!({
                    "id": id,
                    "project_id": project,
                    "title": title,
                    "description": description,
                }),
            });
            self.task_labels.lock().unwrap().insert(id, labels.to_vec());
            if let Some(bucket) = self.default_bucket(project, self.kanban_view(project)) {
                self.placement.lock().unwrap().insert(id, bucket);
            }
            id
        }

        /// The bucket id of a named column — for a test hook reaching into the
        /// board the way a concurrent session would.
        fn bucket_titled(&self, project: u64, title: &str) -> u64 {
            let view = self.kanban_view(project);
            self.buckets
                .lock()
                .unwrap()
                .iter()
                .find(|(p, v, b)| *p == project && *v == view && b.title == title)
                .map(|(_, _, b)| b.id)
                .expect("the column exists")
        }

        /// Move a seeded card into a named column, as the store would.
        fn seed_placement(&self, project: u64, task: u64, column: &str) {
            let view = self.kanban_view(project);
            let bucket = self
                .buckets
                .lock()
                .unwrap()
                .iter()
                .find(|(p, v, b)| *p == project && *v == view && b.title == column)
                .map(|(_, _, b)| b.id)
                .expect("the column exists");
            self.placement.lock().unwrap().insert(task, bucket);
        }

        /// A card as the API hands it back: its stored fields plus the label
        /// titles its relations resolve to.
        fn rendered(&self, task: &TaskRec) -> TaskRec {
            let labels = self.labels.lock().unwrap();
            let titles = self
                .task_labels
                .lock()
                .unwrap()
                .get(&task.id)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| labels.iter().find(|l| l.id == *id))
                        .map(|l| l.title.clone())
                        .collect()
                })
                .unwrap_or_default();
            TaskRec { labels: titles, ..task.clone() }
        }
    }

    #[async_trait]
    impl VikunjaApi for FakeVikunja {
        async fn list_projects(
            &self,
            page: u64,
            per_page: u64,
        ) -> Result<Vec<ProjectRec>, MailboxError> {
            let all = self.projects.lock().unwrap();
            Ok(all
                .iter()
                .skip((page as usize - 1) * self.served(per_page))
                .take(self.served(per_page))
                .cloned()
                .collect())
        }

        async fn create_project(
            &self,
            title: &str,
            description: &str,
        ) -> Result<ProjectRec, MailboxError> {
            let id = self.seed_project(title, description);
            Ok(self
                .projects
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .expect("just created"))
        }

        async fn list_views(&self, project_id: u64) -> Result<Vec<ViewRec>, MailboxError> {
            self.named(project_id);
            Ok(self
                .views
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, _)| *p == project_id)
                .map(|(_, v)| v.clone())
                .collect())
        }

        async fn list_buckets(
            &self,
            project_id: u64,
            view_id: u64,
        ) -> Result<Vec<BucketRec>, MailboxError> {
            self.maybe_fail("list_buckets")?;
            self.named(project_id);
            Ok(self
                .buckets
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, v, _)| *p == project_id && *v == view_id)
                .map(|(_, _, b)| b.clone())
                .collect())
        }

        async fn create_bucket(
            &self,
            project_id: u64,
            view_id: u64,
            title: &str,
        ) -> Result<BucketRec, MailboxError> {
            self.maybe_fail("create_bucket")?;
            self.named(project_id);
            let bucket = BucketRec { id: self.next_id(), title: title.into() };
            self.buckets
                .lock()
                .unwrap()
                .push((project_id, view_id, bucket.clone()));
            Ok(bucket)
        }

        async fn board(
            &self,
            project_id: u64,
            view_id: u64,
            page: u64,
            per_page: u64,
        ) -> Result<Vec<BoardBucket>, MailboxError> {
            self.named(project_id);
            let buckets: Vec<BucketRec> = self
                .buckets
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, v, _)| *p == project_id && *v == view_id)
                .map(|(_, _, b)| b.clone())
                .collect();
            let placement = self.placement.lock().unwrap().clone();
            let tasks = self.tasks.lock().unwrap().clone();
            Ok(buckets
                .into_iter()
                .map(|b| {
                    let in_bucket: Vec<TaskRec> = tasks
                        .iter()
                        .filter(|t| placement.get(&t.id) == Some(&b.id))
                        .map(|t| self.rendered(t))
                        .skip((page as usize - 1) * self.served(per_page))
                        .take(self.served(per_page))
                        .collect();
                    BoardBucket { id: b.id, title: b.title, tasks: in_bucket }
                })
                .collect())
        }

        async fn create_task(
            &self,
            project_id: u64,
            title: &str,
            description: &str,
        ) -> Result<TaskRec, MailboxError> {
            self.maybe_fail("create_task")?;
            self.named(project_id);
            let id = self.next_id();
            let description = self.mangle(description);
            let task = TaskRec {
                id,
                project_id,
                title: title.into(),
                description: description.clone(),
                labels: Vec::new(),
                raw: serde_json::json!({
                    "id": id,
                    "project_id": project_id,
                    "title": title,
                    "description": description,
                }),
            };
            self.tasks.lock().unwrap().push(task.clone());
            // Vikunja drops a fresh card in the view's default column, not in
            // whichever column the caller had in mind.
            let view = self.kanban_view(project_id);
            if let Some(bucket) = self.default_bucket(project_id, view) {
                self.placement.lock().unwrap().insert(id, bucket);
            }
            Ok(task)
        }

        /// **The whole model is what gets written.** Vikunja's task update
        /// persists the task it is handed, so any writable field the payload
        /// omits comes back as its Go zero value. The fake stores exactly the
        /// payload, which is what makes "a field jojobot does not model
        /// survives" a thing a test can fail on.
        async fn update_task(
            &self,
            project_id: u64,
            task: &serde_json::Value,
        ) -> Result<(), MailboxError> {
            self.maybe_fail("update_task")?;
            self.named(project_id);
            let task_id = task["id"]
                .as_u64()
                .ok_or_else(|| MailboxError::Store("update_task: no id".into()))?;
            self.wrote(task_id);

            let mut payload = task.clone();
            let description = self.mangle(payload["description"].as_str().unwrap_or_default());
            payload["description"] = description.clone().into();

            let mut tasks = self.tasks.lock().unwrap();
            match tasks.iter_mut().find(|t| t.id == task_id) {
                Some(stored) => {
                    stored.title = payload["title"].as_str().unwrap_or_default().to_string();
                    stored.description = description;
                    stored.raw = payload;
                    Ok(())
                }
                None => Err(MailboxError::Store(format!("update_task: no card {task_id}"))),
            }
        }

        async fn move_task(
            &self,
            project_id: u64,
            view_id: u64,
            bucket_id: u64,
            task_id: u64,
        ) -> Result<(), MailboxError> {
            self.maybe_fail("move_task")?;
            self.named(project_id);
            let _ = view_id;
            self.wrote(task_id);
            self.placement.lock().unwrap().insert(task_id, bucket_id);
            Ok(())
        }

        async fn list_labels(
            &self,
            page: u64,
            per_page: u64,
        ) -> Result<Vec<LabelRec>, MailboxError> {
            let all = self.labels.lock().unwrap();
            Ok(all
                .iter()
                .skip((page as usize - 1) * self.served(per_page))
                .take(self.served(per_page))
                .cloned()
                .collect())
        }

        async fn create_label(
            &self,
            title: &str,
            description: &str,
        ) -> Result<LabelRec, MailboxError> {
            self.maybe_fail("create_label")?;
            let label = LabelRec {
                id: self.next_id(),
                title: title.into(),
                description: description.into(),
                created: self.stamp(),
            };
            self.labels.lock().unwrap().push(label.clone());
            Ok(label)
        }

        async fn set_task_labels(&self, task_id: u64, labels: &[u64]) -> Result<(), MailboxError> {
            self.maybe_fail("set_task_labels")?;
            self.wrote(task_id);
            self.task_labels.lock().unwrap().insert(task_id, labels.to_vec());
            Ok(())
        }
    }

    /// A decorator over the fake that opens two seams the bare fake has not
    /// got:
    ///
    /// * it runs a hook right before the **nth** `board` read from now — where
    ///   a hand edit in the Vikunja UI lands, between a verb's writes and its
    ///   read-back;
    /// * **every call is a real await point.** The fake awaits nothing, so two
    ///   verbs joined on one task would run to completion one after the other
    ///   whatever the store did — a concurrency test over it could not fail.
    ///   Yielding at each call is what a network round trip does, and it is
    ///   what makes the serialization tests below able to observe an
    ///   interleaving.
    ///
    /// Every call otherwise delegates untouched.
    /// What an armed interleave runs, handed the fake to reach into.
    type BoardHook = Box<dyn FnOnce(&FakeVikunja) + Send>;

    struct Interleaved {
        inner: Arc<FakeVikunja>,
        on_board: Mutex<Option<(u64, BoardHook)>>,
    }

    impl Interleaved {
        fn new(inner: Arc<FakeVikunja>) -> Arc<Self> {
            Arc::new(Self {
                inner,
                on_board: Mutex::new(None),
            })
        }

        /// Hand the runtime a chance to run the other verb — what an HTTP call
        /// does at every one of these points.
        async fn pause(&self) {
            tokio::task::yield_now().await;
        }

        /// Arm `hook` to run right before the nth `board` call from now.
        fn before_board(&self, nth: u64, hook: impl FnOnce(&FakeVikunja) + Send + 'static) {
            *self.on_board.lock().unwrap() = Some((nth, Box::new(hook)));
        }

        fn maybe_interleave(&self) {
            let mut armed = self.on_board.lock().unwrap();
            let Some((remaining, _)) = armed.as_mut() else {
                return;
            };
            *remaining -= 1;
            if *remaining == 0 {
                let (_, hook) = armed.take().expect("just matched");
                hook(&self.inner);
            }
        }
    }

    #[async_trait]
    impl VikunjaApi for Interleaved {
        async fn list_projects(
            &self,
            page: u64,
            per_page: u64,
        ) -> Result<Vec<ProjectRec>, MailboxError> {
            self.pause().await;
            self.inner.list_projects(page, per_page).await
        }
        async fn create_project(
            &self,
            title: &str,
            description: &str,
        ) -> Result<ProjectRec, MailboxError> {
            self.pause().await;
            self.inner.create_project(title, description).await
        }
        async fn list_views(&self, project_id: u64) -> Result<Vec<ViewRec>, MailboxError> {
            self.pause().await;
            self.inner.list_views(project_id).await
        }
        async fn list_buckets(
            &self,
            project_id: u64,
            view_id: u64,
        ) -> Result<Vec<BucketRec>, MailboxError> {
            self.pause().await;
            self.inner.list_buckets(project_id, view_id).await
        }
        async fn create_bucket(
            &self,
            project_id: u64,
            view_id: u64,
            title: &str,
        ) -> Result<BucketRec, MailboxError> {
            self.pause().await;
            self.inner.create_bucket(project_id, view_id, title).await
        }
        async fn board(
            &self,
            project_id: u64,
            view_id: u64,
            page: u64,
            per_page: u64,
        ) -> Result<Vec<BoardBucket>, MailboxError> {
            self.pause().await;
            self.maybe_interleave();
            self.inner.board(project_id, view_id, page, per_page).await
        }
        async fn create_task(
            &self,
            project_id: u64,
            title: &str,
            description: &str,
        ) -> Result<TaskRec, MailboxError> {
            self.pause().await;
            self.inner.create_task(project_id, title, description).await
        }
        async fn update_task(
            &self,
            project_id: u64,
            task: &serde_json::Value,
        ) -> Result<(), MailboxError> {
            self.pause().await;
            self.inner.update_task(project_id, task).await
        }
        async fn move_task(
            &self,
            project_id: u64,
            view_id: u64,
            bucket_id: u64,
            task_id: u64,
        ) -> Result<(), MailboxError> {
            self.pause().await;
            self.inner.move_task(project_id, view_id, bucket_id, task_id).await
        }
        async fn list_labels(
            &self,
            page: u64,
            per_page: u64,
        ) -> Result<Vec<LabelRec>, MailboxError> {
            self.pause().await;
            self.inner.list_labels(page, per_page).await
        }
        async fn create_label(
            &self,
            title: &str,
            description: &str,
        ) -> Result<LabelRec, MailboxError> {
            self.pause().await;
            self.inner.create_label(title, description).await
        }
        async fn set_task_labels(&self, task_id: u64, labels: &[u64]) -> Result<(), MailboxError> {
            self.pause().await;
            self.inner.set_task_labels(task_id, labels).await
        }
    }

    const PROJECT: &str = "jojobot-mailboxes-test";

    fn store(fake: Arc<FakeVikunja>) -> VikunjaStore {
        VikunjaStore::from_api(fake, PROJECT)
    }

    /// A store with one mailbox already created, so a spec about placement or
    /// ownership is not what the existence gate trips over.
    async fn store_with_box(fake: Arc<FakeVikunja>, name: &str) -> VikunjaStore {
        let store = store(fake);
        contract::create(&store, name).await;
        store
    }

    fn owned_desc() -> String {
        format!("Managed by jojobot. {OWNER_TAG}")
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(secs).expect("a valid instant")
    }

    /// The whole real store logic — provisioning, screening, placement, codec —
    /// against a fake transport. The same suite the in-memory fake satisfies and
    /// the gated integration test runs against real Vikunja.
    #[tokio::test]
    async fn the_vikunja_store_satisfies_the_contract() {
        contract::run_all(|| store(FakeVikunja::new())).await;
    }

    // --- provisioning, ownership, and the operator's own boards ---------------

    #[tokio::test]
    async fn self_provisions_the_project_and_its_three_columns() {
        let fake = FakeVikunja::new();
        contract::create(&store(fake.clone()), "inbox").await;

        assert_eq!(fake.owned_titled(PROJECT), 1, "exactly one owned project");
        let project = fake.projects_titled(PROJECT)[0].id;
        let view = fake.kanban_view(project);
        let columns: Vec<String> = fake
            .buckets
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, v, _)| *p == project && *v == view)
            .map(|(_, _, b)| b.title.clone())
            .collect();
        for state in MessageState::ALL {
            assert!(
                columns.contains(&state.as_token().to_string()),
                "the board must carry a '{state}' column: {columns:?}"
            );
        }
    }

    /// **A board whose columns cannot be provisioned refuses, rather than
    /// filing mail into a funnel that does not exist.** The columns ARE the
    /// state here, so a store that shrugged off a failed `create_bucket` would
    /// post messages into whatever column Vikunja happened to hand it and read
    /// them back as no state at all.
    #[tokio::test]
    async fn a_board_whose_columns_cannot_be_created_refuses_every_verb() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());

        fake.fail_all("create_bucket");
        let outcome = store
            .create_mailbox(&MailboxName("inbox".into()), false)
            .await;
        assert!(
            matches!(outcome, Err(MailboxError::Store(_))),
            "a board that cannot be provisioned is a store failure: {outcome:?}"
        );
        assert!(
            store.list_mailboxes().await.is_err(),
            "…and every other verb resolves the same scope, so none of them proceeds either"
        );
    }

    /// **A mailbox jojobot could not label into existence is not a mailbox.**
    /// The label IS the box, so a `create_label` that fails must come back as a
    /// failure — never as a created box a later post would be blocked from.
    #[tokio::test]
    async fn a_mailbox_whose_label_cannot_be_created_is_not_reported_as_created() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());

        fake.fail_all("create_label");
        let outcome = store
            .create_mailbox(&MailboxName("inbox".into()), false)
            .await;
        assert!(outcome.is_err(), "a failed label creation must not report a box: {outcome:?}");
        assert!(
            store.list_mailboxes().await.expect("list ok").is_empty(),
            "…and no box exists afterwards"
        );
    }

    /// **The operator's own boards live on this Vikunja.** A project that
    /// happens to share jojobot's name but carries no ownership marker is
    /// somebody else's, and jojobot makes its own rather than adopting it.
    #[tokio::test]
    async fn never_adopts_the_operators_same_named_project() {
        let fake = FakeVikunja::new();
        let theirs = fake.seed_project(PROJECT, "my own board");
        fake.seed_task(theirs, "buy stamps", "", &[]);

        contract::post(&store_with_box(fake.clone(), "inbox").await, "inbox", "alpha", "hello", 0)
            .await;

        assert_eq!(fake.owned_titled(PROJECT), 1, "jojobot made its own project");
        assert_eq!(
            fake.tasks_in(theirs).len(),
            1,
            "the operator's project keeps exactly the card it had"
        );
        assert_eq!(fake.tasks_in(theirs)[0].title, "buy stamps");
    }

    #[tokio::test]
    async fn reconciles_duplicate_owned_projects_to_the_oldest() {
        let fake = FakeVikunja::new();
        let older = fake.seed_project(PROJECT, &owned_desc());
        let newer = fake.seed_project(PROJECT, &owned_desc());

        contract::post(&store_with_box(fake.clone(), "inbox").await, "inbox", "alpha", "hello", 0)
            .await;

        assert_eq!(fake.owned_titled(PROJECT), 2, "no third project created");
        assert_eq!(fake.tasks_in(older).len(), 1, "the message went to the oldest");
        assert!(fake.tasks_in(newer).is_empty());
    }

    #[tokio::test]
    async fn pages_beyond_the_first_page_of_projects_before_concluding_absent() {
        let fake = FakeVikunja::new();
        // The server's cap sits BELOW the page size the store requests. With
        // the two equal, a loop that stops on "fewer than I requested" can
        // never take its broken branch — so this test used to pass against
        // exactly the stop-at-one-page bug it describes.
        fake.cap_pages_at(25);
        for i in 0..(PAGE + 20) {
            fake.seed_project(&format!("other-{i}"), "unrelated");
        }
        // The one owned match sits past the first page.
        let owned = fake.seed_project(PROJECT, &owned_desc());

        contract::post(&store_with_box(fake.clone(), "inbox").await, "inbox", "alpha", "hello", 0)
            .await;

        assert_eq!(fake.owned_titled(PROJECT), 1, "must find the paged-past project, not fork");
        assert_eq!(fake.tasks_in(owned).len(), 1);
    }

    /// A mailbox label past the first page of a Vikunja carrying many labels is
    /// still found — otherwise posting into it would come back blocked, and
    /// creating it again would be refused as an exact collision nobody can see.
    #[tokio::test]
    async fn pages_beyond_the_first_page_of_labels() {
        let fake = FakeVikunja::new();
        // Below the requested page size — see the projects sibling above.
        fake.cap_pages_at(25);
        let store = store(fake.clone());
        for i in 0..(PAGE + 20) {
            fake.create_label(&format!("facet-{i}"), "the operator's own")
                .await
                .expect("seed label");
        }
        contract::create(&store, "inbox").await;

        let names = store.mailbox_names().await.expect("names");
        assert_eq!(names.len(), 1, "the operator's own labels are not mailboxes");
        assert_eq!(names[0].as_str(), "inbox");
    }

    // --- placement, and the cards jojobot did not write ------------------------

    /// Vikunja drops a fresh card in the view's default column, which is not one
    /// of jojobot's three. The card is moved, and it is the **read-back** that
    /// says so — a message left in the default column is mail nobody can read.
    #[tokio::test]
    async fn a_posted_card_leaves_the_default_column_for_new() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let posted = contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;

        let card: u64 = posted.id.as_str().parse().expect("a numeric card id");
        assert_eq!(fake.column_of(card).as_deref(), Some("new"));
    }

    /// **A card a human put on the board is not a message.** It carries no
    /// declared sender and no instant, so it is neither delivered nor counted —
    /// inventing provenance for it would put an unattributable message into a
    /// consumer's batch.
    #[tokio::test]
    async fn a_card_without_a_machine_block_is_never_delivered() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let project = fake.projects_titled(PROJECT)[0].id;
        let label = fake.labels.lock().unwrap()[0].id;
        let stray = fake.seed_task(project, "a note someone typed", "no machine block here", &[label]);
        fake.seed_placement(project, stray, "new");

        let delivery = contract::read(&store, "inbox").await;
        assert!(
            delivery.messages.is_empty(),
            "a hand-written card must not be delivered as mail: {:?}",
            delivery.messages
        );
        assert_eq!(
            contract::counts(&store, "inbox").await.expect("inbox").total(),
            0,
            "…and it is not counted as mail either"
        );
        assert_eq!(
            fake.tasks_in(project).len(),
            1,
            "…and it is left exactly where the human put it"
        );
    }

    /// **An unreadable message is surfaced, never silently skipped.** A card
    /// wearing jojobot's mailbox label whose description was hand-edited past
    /// parsing is invisible to every verb — not counted, not delivered, not
    /// processable — so `list_mailboxes` must say "1 unreadable" with the card
    /// id, instead of nothing.
    #[tokio::test]
    async fn an_unreadable_card_wearing_the_label_is_quarantined_and_surfaced() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let project = fake.projects_titled(PROJECT)[0].id;
        let label = fake.labels.lock().unwrap()[0].id;
        // A real message whose description someone garbled in the UI.
        let garbled = fake.seed_task(project, "alpha: hello", "hand-edited past parsing", &[label]);
        fake.seed_placement(project, garbled, "new");

        let boxes = store.list_mailboxes().await.expect("list ok");
        let inbox = boxes.iter().find(|m| m.name.as_str() == "inbox").expect("inbox");
        assert_eq!(
            inbox.quarantined,
            vec![MessageId(garbled.to_string())],
            "the unreadable card is surfaced with its id"
        );
        assert_eq!(inbox.counts.total(), 0, "…but never counted as a readable message");
        assert!(
            contract::read(&store, "inbox").await.messages.is_empty(),
            "…and never delivered"
        );
    }

    /// The sibling path: a card wearing the label but sitting in a column whose
    /// title is no state token. It has no state to read, so it is quarantined —
    /// and surfaced — rather than silently skipped with not even a log line.
    #[tokio::test]
    async fn a_labelled_card_in_an_unknown_column_is_quarantined_and_surfaced() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let posted = contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;
        let card: u64 = posted.id.as_str().parse().expect("a numeric card id");
        let project = fake.projects_titled(PROJECT)[0].id;
        // The operator drags the card into Vikunja's own default column.
        fake.seed_placement(project, card, DEFAULT_COLUMN);

        let boxes = store.list_mailboxes().await.expect("list ok");
        let inbox = boxes.iter().find(|m| m.name.as_str() == "inbox").expect("inbox");
        assert_eq!(
            inbox.quarantined,
            vec![posted.id.clone()],
            "a message stranded outside the funnel is surfaced, not lost"
        );
        assert_eq!(inbox.counts.total(), 0);
        assert!(contract::read(&store, "inbox").await.messages.is_empty());
        // Every verb still refuses to guess a state for it.
        let err = store.mark_processed(&posted.id, None).await;
        assert!(
            matches!(err, Err(MailboxError::UnknownMessage { .. })),
            "a quarantined card is not processable: {err:?}"
        );
    }

    // --- read-back, and what a failed write leaves behind ----------------------

    /// A post whose read-back mismatches leaves **no deliverable card** — but
    /// deletes nothing, and fabricates nothing. A half-written message card is
    /// not inert: left in `new`, the next read would hand it to a consumer as
    /// mail; parked in `processed`, it would read back as handled mail nobody
    /// ever received. So the rollback parks it in the `parked` column — not a
    /// state — where the board read quarantines it: surfaced, counted as
    /// nothing, delivered to nobody.
    #[tokio::test]
    async fn a_failed_post_parks_the_card_into_quarantine_without_deleting_it() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let project = fake.projects_titled(PROJECT)[0].id;

        fake.poison_next_write();
        let outcome = store
            .post_message(NewMessage {
                mailbox: MailboxName("inbox".into()),
                body: "the shipment landed".into(),
                sender: "alpha".into(),
                sent_at: at(1_780_000_000),
            })
            .await;
        assert!(outcome.is_err(), "a mangled write must not report success: {outcome:?}");

        assert_eq!(
            MessageState::from_token(PARKED_COLUMN),
            None,
            "the whole design rests on 'parked' never being a state"
        );
        let left = fake.tasks_in(project);
        assert_eq!(left.len(), 1, "nothing is deleted, ever: {left:?}");
        assert_eq!(
            fake.column_of(left[0].id).as_deref(),
            Some("parked"),
            "the card is parked outside the funnel, not in a state column"
        );
        let delivery = contract::read(&store, "inbox").await;
        assert!(
            delivery.messages.is_empty(),
            "a parked card is never handed to a consumer as mail: {:?}",
            delivery.messages
        );
        let inbox = store
            .list_mailboxes()
            .await
            .expect("list ok")
            .into_iter()
            .find(|m| m.name.as_str() == "inbox")
            .expect("inbox");
        assert_eq!(
            inbox.counts.total(),
            0,
            "a failed post leaves NOTHING readable — especially not phantom processed mail"
        );
        assert_eq!(
            inbox.quarantined,
            vec![MessageId(left[0].id.to_string())],
            "…and the parked card is surfaced, not lost"
        );
    }

    /// A processing whose read-back mismatches puts the card back where it was —
    /// still deliverable, rather than stranded half-processed where no consumer
    /// would ever see it again.
    #[tokio::test]
    async fn a_failed_processing_restores_the_card() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let posted = contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;
        contract::read(&store, "inbox").await;
        let card: u64 = posted.id.as_str().parse().expect("a numeric card id");

        fake.poison_next_write();
        let outcome = store.mark_processed(&posted.id, Some("filed")).await;
        assert!(outcome.is_err(), "a mangled write must not report success: {outcome:?}");

        assert_eq!(
            fake.column_of(card).as_deref(),
            Some("read"),
            "the card goes back to the column it was in"
        );
        let again = contract::read(&store, "inbox").await;
        assert_eq!(
            again.messages.len(),
            1,
            "the message is still deliverable, not stranded: {:?}",
            again.messages
        );
        assert_eq!(again.messages[0].message.body, "the shipment landed");
        assert_eq!(again.messages[0].message.notes, None, "no half-written outcome remains");
    }

    // --- the write-scope invariant --------------------------------------------

    /// **No verb reaches another project.** The operator's own boards live on
    /// this Vikunja; a mis-scoped write to one of them is not something a
    /// read-back can undo.
    ///
    /// The fake records every project id any call named and every card any call
    /// wrote to, so this fails the moment a verb can address something outside
    /// jojobot's own board — whether by project id or by the global card id that
    /// would sidestep it.
    #[tokio::test]
    async fn no_verb_ever_reaches_a_project_other_than_jojobots() {
        let fake = FakeVikunja::new();
        // The operator's board, with real cards on it.
        let theirs = fake.seed_project("their-own-board", "not jojobot's");
        let their_card = fake.seed_task(theirs, "renew the passport", "due in March", &[]);

        let store = store_with_box(fake.clone(), "inbox").await;
        let ours = fake.projects_titled(PROJECT)[0].id;

        // Every verb, once.
        let posted = contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;
        store.list_mailboxes().await.expect("list ok");
        contract::read(&store, "inbox").await;
        store.mark_processed(&posted.id, Some("filed")).await.expect("processed");

        let named = fake.named_projects.lock().unwrap().clone();
        assert_eq!(
            named,
            std::collections::HashSet::from([ours]),
            "a verb named a project that is not jojobot's: {named:?} (jojobot's is {ours})"
        );

        let written: Vec<u64> = fake.written_tasks.lock().unwrap().iter().copied().collect();
        assert!(
            !written.contains(&their_card),
            "a verb wrote to a card on the operator's board: {written:?}"
        );
        let theirs_after = fake.tasks_in(theirs);
        assert_eq!(theirs_after.len(), 1, "no card was added to their board");
        assert_eq!(theirs_after[0].id, their_card);
        assert_eq!(
            theirs_after[0].project_id, theirs,
            "…and none was moved off it either"
        );
        assert_eq!(
            (theirs_after[0].title.as_str(), theirs_after[0].description.as_str()),
            ("renew the passport", "due in March"),
            "the operator's card is exactly as they left it"
        );
    }

    /// **The sharp edge: a card id is global.** A card that turns up on
    /// jojobot's board while declaring another project — a view shared across
    /// projects, a store that answers with more than it was asked — is refused
    /// **before anything is written to it**, rather than being rewritten with
    /// jojobot's project id and quietly moved off the operator's board.
    #[tokio::test]
    async fn a_card_declaring_another_project_is_refused_before_any_write() {
        let fake = FakeVikunja::new();
        let theirs = fake.seed_project("their-own-board", "not jojobot's");
        let store = store_with_box(fake.clone(), "inbox").await;
        let ours = fake.projects_titled(PROJECT)[0].id;
        let label = fake.labels.lock().unwrap()[0].id;

        // Their card, wearing jojobot's mailbox label and sitting in jojobot's
        // `new` column — everything a message looks like, except whose it is.
        let intruder = fake.seed_task(
            theirs,
            "alpha: looks like a message",
            &render_description(
                "looks like a message",
                &Envelope { sender: "alpha".into(), sent_at: at(1_780_000_000), notes: None },
            ),
            &[label],
        );
        fake.seed_placement(ours, intruder, "new");
        fake.written_tasks.lock().unwrap().clear();

        let err = store
            .mark_processed(&MessageId(intruder.to_string()), Some("filed"))
            .await
            .expect_err("a card from another project must not be written");
        assert!(
            matches!(err, MailboxError::ForeignProject(_)),
            "got {err:?}"
        );
        assert!(
            fake.written_tasks.lock().unwrap().is_empty(),
            "the refusal must come before any write reaches the card"
        );
        assert_eq!(
            fake.tasks_in(theirs)[0].description,
            render_description(
                "looks like a message",
                &Envelope { sender: "alpha".into(), sent_at: at(1_780_000_000), notes: None },
            ),
            "…and the card is untouched"
        );
    }

    /// **Mailbox labels are namespaced by the project that owns them.** Vikunja
    /// labels are global, so without this a throwaway store — the gated
    /// integration test's, say — would see, screen against, and be blocked by
    /// the boxes of the real one running beside it.
    #[tokio::test]
    async fn two_stores_on_different_projects_do_not_see_each_others_boxes() {
        let fake = FakeVikunja::new();
        let mine = VikunjaStore::from_api(fake.clone(), "jojobot-mailboxes-a");
        let theirs = VikunjaStore::from_api(fake.clone(), "jojobot-mailboxes-b");

        contract::create(&mine, "inbox").await;

        assert!(
            theirs.list_mailboxes().await.expect("list ok").is_empty(),
            "another project's boxes are not this one's"
        );
        // …and the name is therefore free here, rather than blocked as taken.
        contract::create(&theirs, "inbox").await;
        assert_eq!(mine.list_mailboxes().await.expect("list ok").len(), 1);
        assert_eq!(theirs.list_mailboxes().await.expect("list ok").len(), 1);
    }

    /// **Vikunja clamps a page server-side.** `per_page` is capped at the
    /// instance's `maxitemsperpage` (50 by default) by the shared read-all
    /// handler, and the cap reaches the board endpoint as a limit on the cards
    /// returned *per column*. A store that asks for more and treats "fewer than
    /// I asked for" as "that was everything" reads the first page of each column
    /// and calls it the board.
    ///
    /// That is silent, and it is unbounded: `processed` is an archive that never
    /// drains, so every mailbox project reaches this. Past the cap, counts
    /// under-report, messages stop being delivered, and — because read-back goes
    /// through the same read — a post whose card lands past the cap is deleted
    /// again by its own rollback.
    #[tokio::test]
    async fn the_board_is_read_whole_even_when_a_column_outruns_a_page() {
        let fake = FakeVikunja::new();
        // The cap sits BELOW the page size the store requests (BOARD_PAGE):
        // with the two equal, "fewer than I requested" is only ever true on the
        // genuinely-last page and the broken stop condition is unreachable.
        // Below it, every page is short of the request, and the broken loop
        // reads page one of each column and calls it the board.
        let cap = 25;
        fake.cap_pages_at(cap);
        let store = store_with_box(fake.clone(), "inbox").await;

        // Comfortably past the server's cap, in one column.
        let posted = cap + 7;
        for i in 0..posted {
            contract::post(&store, "inbox", "alpha", &format!("message {i}"), i as i64).await;
        }

        let counts = contract::counts(&store, "inbox").await.expect("inbox exists");
        assert_eq!(
            counts.new, posted as usize,
            "every card in the column must be counted, not just the first page"
        );

        let delivery = contract::read(&store, "inbox").await;
        assert_eq!(
            delivery.messages.len(),
            posted as usize,
            "every unprocessed message must be delivered, not just the first page"
        );

        // …and the read-back path agrees, so a message past the cap can still be
        // retired rather than being unreachable forever.
        let last = delivery.messages.last().expect("a message").message.id.clone();
        let processed = store.mark_processed(&last, None).await.expect("mark_processed ok");
        assert_eq!(processed.state, MessageState::Processed);
    }

    // --- a failed write leaves no damage, at EVERY step -----------------------

    /// **The rollback has to cover the whole write, not just its last step.**
    /// A post is four calls — create the card, label it, find the column, move
    /// it — and a failure at any of them must leave nothing a consumer could
    /// receive and nothing that reads back as handled: the created card is
    /// parked into quarantine — in the `parked` column, which is no state, so
    /// it counts as nothing and is delivered to nobody — and never deleted.
    ///
    /// The `set_task_labels` step is the one whose card cannot be attributed to
    /// a box afterwards; what that costs is pinned separately below.
    #[tokio::test]
    async fn a_post_that_fails_midway_parks_the_card_into_quarantine() {
        for step in ["set_task_labels", "move_task"] {
            let fake = FakeVikunja::new();
            let store = store_with_box(fake.clone(), "inbox").await;
            let project = fake.projects_titled(PROJECT)[0].id;

            fake.fail_next(step);
            let outcome = store
                .post_message(NewMessage {
                    mailbox: MailboxName("inbox".into()),
                    body: "the shipment landed".into(),
                    sender: "alpha".into(),
                    sent_at: at(1_780_000_000),
                })
                .await;
            assert!(outcome.is_err(), "a failed {step} must not report success");
            let left = fake.tasks_in(project);
            assert_eq!(left.len(), 1, "a failed {step} deletes nothing: {left:?}");
            assert_eq!(
                fake.column_of(left[0].id).as_deref(),
                Some("parked"),
                "a failed {step} parks the card outside the funnel"
            );
            let delivery = contract::read(&store, "inbox").await;
            assert!(
                delivery.messages.is_empty(),
                "a card stranded by a failed {step} must never be delivered as mail: {:?}",
                delivery.messages
            );
            let inbox = store
                .list_mailboxes()
                .await
                .expect("list ok")
                .into_iter()
                .find(|m| m.name.as_str() == "inbox")
                .expect("inbox");
            assert_eq!(
                inbox.counts.total(),
                0,
                "nothing readable is left behind by a failed {step} — no phantom archive"
            );
            // Whichever step failed, the card is in quarantine — attributed to
            // its box when the label landed, unattributed when it did not.
            let scope = store.resolve_scope().await.expect("scope resolves");
            let board = store.board_read(&scope).await.expect("the board reads");
            assert_eq!(
                board.quarantined.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
                vec![left[0].id],
                "the parked card is quarantined even when {step} was what failed"
            );
        }
    }

    /// **The one residue class, stated exactly.** When the label call is what
    /// fails, the card jojobot created wears no mailbox label — and nothing
    /// else on a card says which box it belongs to (the codec deliberately does
    /// not repeat what the board already holds, which is what keeps a card from
    /// having two answers about its own mailbox). So the card is quarantined by
    /// its column — outside the funnel, counted as nothing, delivered to nobody
    /// — but no box can claim it, and `list_mailboxes` cannot show it under
    /// one. That is the whole reason it is said at error level with the card id:
    /// a card only a log knows about is one a human has to be told about.
    #[tokio::test]
    async fn a_card_that_could_not_be_labelled_is_quarantined_but_names_no_box() {
        let logged = crate::log_capture::log_sink();
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let project = fake.projects_titled(PROJECT)[0].id;

        fake.fail_all("set_task_labels");
        let outcome = store
            .post_message(NewMessage {
                mailbox: MailboxName("inbox".into()),
                body: "the shipment landed".into(),
                sender: "alpha".into(),
                sent_at: at(1_780_000_000),
            })
            .await;
        assert!(outcome.is_err(), "a failed labelling must not report success: {outcome:?}");
        let card = fake.tasks_in(project)[0].id;

        *fake.fail_next.lock().unwrap() = None;
        let scope = store.resolve_scope().await.expect("scope resolves");
        let board = store.board_read(&scope).await.expect("the board reads");
        assert_eq!(
            board.quarantined,
            vec![(None, card)],
            "the card is quarantined, and no box may be invented for it"
        );
        assert!(
            board.messages.is_empty(),
            "…and it is certainly not a deliverable message: {:?}",
            board.messages
        );
        let inbox = store
            .list_mailboxes()
            .await
            .expect("list ok")
            .into_iter()
            .find(|m| m.name.as_str() == "inbox")
            .expect("inbox");
        assert!(
            inbox.quarantined.is_empty(),
            "a box must not be made to claim a card that carries nothing saying it is theirs"
        );

        let line = logged
            .line_with("could not be labelled")
            .expect("the residue is reported, not swallowed");
        assert!(line.contains("ERROR"), "…at error level: {line}");
        assert!(line.contains(&format!("card={card}")), "…naming the card: {line}");
    }

    /// **A card jojobot created is never invisible to every verb.** The
    /// invariant, stated whole: no failure of any single API method may leave a
    /// created card outside BOTH the funnel (a state column, where it is
    /// counted and delivered) and quarantine (surfaced as unreadable, acted on
    /// by nothing). A card in neither is a message that exists, is owed to
    /// somebody, and cannot be seen by counts, delivery, `mark_processed` or
    /// quarantine — the one outcome this context has no way to recover from.
    ///
    /// The failures are armed **for good**, not once: the hole this closes was
    /// a rescue path whose first act re-issued the very call that had just
    /// failed, which a one-shot injector papers over.
    #[tokio::test]
    async fn no_single_api_failure_leaves_a_created_card_outside_the_funnel_and_quarantine() {
        // Every API method a post can reach. `create_bucket` is not among them
        // — a provisioned board has its columns, so a healthy post never calls
        // it; it is armed in the provisioning test instead, where it fires.
        for step in ["create_task", "set_task_labels", "list_buckets", "move_task"] {
            let fake = FakeVikunja::new();
            let store = store_with_box(fake.clone(), "inbox").await;
            let project = fake.projects_titled(PROJECT)[0].id;

            fake.fail_all(step);
            let outcome = store
                .post_message(NewMessage {
                    mailbox: MailboxName("inbox".into()),
                    body: "the shipment landed".into(),
                    sender: "alpha".into(),
                    sent_at: at(1_780_000_000),
                })
                .await;
            assert!(outcome.is_err(), "a failed {step} must not report success: {outcome:?}");

            // Read the board back through jojobot's own eyes, with the failure
            // disarmed — what the next session would see.
            *fake.fail_next.lock().unwrap() = None;
            let scope = store.resolve_scope().await.expect("scope resolves");
            let board = store.board_read(&scope).await.expect("the board reads");
            for card in fake.tasks_in(project) {
                let in_funnel = board.messages.iter().any(|(task, _)| task.id == card.id);
                let quarantined = board.quarantined.iter().any(|(_, id)| *id == card.id);
                assert!(
                    in_funnel || quarantined,
                    "a failed {step} left card {} where no verb can see it: column {:?}, \
                     labels {:?}",
                    card.id,
                    fake.column_of(card.id),
                    card.labels,
                );
            }
        }
    }

    /// A post whose very first step — creating the card — fails propagates a
    /// clean error. Deliberately a single assertion: before `create_task`
    /// succeeds there is nothing to roll back or park, and asserting an empty
    /// board would only restate what the fake guarantees on its own error
    /// path. What this pins is that the injector reaches the first step and
    /// the failure is not swallowed.
    #[tokio::test]
    async fn a_post_whose_card_creation_fails_propagates_the_error() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;

        fake.fail_next("create_task");
        let outcome = store
            .post_message(NewMessage {
                mailbox: MailboxName("inbox".into()),
                body: "the shipment landed".into(),
                sender: "alpha".into(),
                sent_at: at(1_780_000_000),
            })
            .await;
        assert!(outcome.is_err(), "a failed create_task must not report success: {outcome:?}");
    }

    /// A processing whose **first** step — writing the outcome — fails leaves
    /// the message exactly as it was: still in its column, still deliverable,
    /// no half-written outcome. The suite covered the second step (the column
    /// move) and never the first.
    #[tokio::test]
    async fn a_processing_whose_first_step_fails_leaves_the_message_untouched() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let posted = contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;
        contract::read(&store, "inbox").await;
        let card: u64 = posted.id.as_str().parse().expect("a numeric card id");

        fake.fail_next("update_task");
        let outcome = store.mark_processed(&posted.id, Some("filed")).await;
        assert!(outcome.is_err(), "a failed outcome write must not report success: {outcome:?}");

        assert_eq!(fake.column_of(card).as_deref(), Some("read"), "the column is unchanged");
        let again = contract::read(&store, "inbox").await;
        assert_eq!(again.messages.len(), 1, "still deliverable");
        assert_eq!(
            again.messages[0].message.notes, None,
            "no outcome is left on a message that was not processed"
        );
    }

    /// A processing that fails after the outcome was written must put the card
    /// back — otherwise the message is redelivered as unprocessed while already
    /// carrying a recorded outcome, and the consumer acts on it twice.
    #[tokio::test]
    async fn a_processing_that_fails_after_writing_the_outcome_restores_the_card() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let posted = contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;
        contract::read(&store, "inbox").await;
        let card: u64 = posted.id.as_str().parse().expect("a numeric card id");

        // The description write lands; the column move is what fails.
        fake.fail_next("move_task");
        let outcome = store.mark_processed(&posted.id, Some("filed")).await;
        assert!(outcome.is_err(), "a failed move must not report success: {outcome:?}");

        assert_eq!(fake.column_of(card).as_deref(), Some("read"), "the column is unchanged");
        let again = contract::read(&store, "inbox").await;
        assert_eq!(again.messages.len(), 1, "still deliverable");
        assert_eq!(
            again.messages[0].message.notes, None,
            "no outcome is left on a message that was not processed"
        );
    }

    /// **A batch is delivered or it is not.** A read moves every message before
    /// verifying any of them; restoring only the one that failed left the rest
    /// moved to `read` while the caller was told the call failed — messages
    /// nobody received, silently marked as received.
    #[tokio::test]
    async fn a_delivery_that_fails_partway_restores_the_whole_batch() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let mut cards = Vec::new();
        for i in 0..3 {
            let posted = contract::post(&store, "inbox", "alpha", &format!("message {i}"), i).await;
            cards.push(posted.id.as_str().parse::<u64>().expect("a numeric card id"));
        }

        // The last move in the batch fails, after the first two have landed.
        fake.fail_nth("move_task", 3);
        let outcome = store.read_mailbox(&MailboxName("inbox".into())).await;
        assert!(outcome.is_err(), "a partial delivery must not report success: {outcome:?}");

        for card in &cards {
            assert_eq!(
                fake.column_of(*card).as_deref(),
                Some("new"),
                "every message in the batch goes back to new, not just the failing one"
            );
        }
    }

    /// **The read-back mismatch branch of a delivery is live code.** Nothing in
    /// the suite ever corrupted a card between a delivery's moves and its
    /// verification read, so `restore_all` on a changed batch was dead code. A
    /// card garbled mid-delivery is a genuine corruption — unlike a state
    /// advance, which the read-back rightly tolerates — and the whole batch
    /// must go back, the garbled description restored from the card the first
    /// read handed over.
    #[tokio::test]
    async fn a_delivery_whose_read_back_finds_a_garbled_card_restores_the_whole_batch() {
        let fake = FakeVikunja::new();
        let api = Interleaved::new(fake.clone());
        let store = VikunjaStore::from_api(api.clone(), PROJECT);
        contract::create(&store, "inbox").await;
        let first = contract::post(&store, "inbox", "alpha", "message one", 0).await;
        let second = contract::post(&store, "inbox", "milhouse", "message two", 60).await;
        let garbled: u64 = second.id.as_str().parse().expect("a numeric card id");

        // A delivery reads the board (two paged calls) to find what is owed,
        // moves the batch, and reads again to verify. The corruption lands
        // right before the verification read — board call 3.
        api.before_board(3, move |fake| {
            let mut tasks = fake.tasks.lock().unwrap();
            let card = tasks.iter_mut().find(|t| t.id == garbled).expect("the card exists");
            card.description = "hand-garbled mid-delivery".into();
            card.raw["description"] = "hand-garbled mid-delivery".into();
        });

        let outcome = store.read_mailbox(&MailboxName("inbox".into())).await;
        assert!(outcome.is_err(), "a garbled batch must not report success: {outcome:?}");

        // The whole batch goes back to `new` — including the card that was fine.
        for id in [&first.id, &second.id] {
            let card: u64 = id.as_str().parse().expect("a numeric card id");
            assert_eq!(
                fake.column_of(card).as_deref(),
                Some("new"),
                "card {id} goes back with the rest of its batch"
            );
        }
        // The restore rewrote the garbled description from the card the first
        // read handed over, so both messages are deliverable again.
        let again = contract::read(&store, "inbox").await;
        let bodies: Vec<&str> = again.messages.iter().map(|d| d.message.body.as_str()).collect();
        assert_eq!(bodies, vec!["message one", "message two"], "nothing stays garbled");
    }

    /// **A batch rollback must not undo another consumer's confirmed work.**
    /// The consumer here is a human on the board, not a second jojobot session
    /// — the verb lock rules that one out — but a card can still advance
    /// under a delivery's feet, and the read-back accepts it when it does.
    /// When a different card in the batch genuinely fails, `restore_all` used
    /// to put back EVERY moved card,
    /// including the processed one: its recorded outcome erased, the handled
    /// message moved back to `new` and delivered again. The rollback now
    /// applies the same later-state-wins rule the read-back does, and skips
    /// cards that advanced past the state this delivery wrote.
    #[tokio::test]
    async fn a_batch_rollback_skips_a_card_a_concurrent_consumer_already_processed() {
        let fake = FakeVikunja::new();
        let api = Interleaved::new(fake.clone());
        let store = VikunjaStore::from_api(api.clone(), PROJECT);
        contract::create(&store, "inbox").await;
        let handled = contract::post(&store, "inbox", "alpha", "message one", 0).await;
        let garbled = contract::post(&store, "inbox", "milhouse", "message two", 60).await;
        let project = fake.projects_titled(PROJECT)[0].id;
        let handled_card: u64 = handled.id.as_str().parse().expect("a numeric card id");
        let garbled_card: u64 = garbled.id.as_str().parse().expect("a numeric card id");

        // Between the delivery's moves and its verification read: a concurrent
        // consumer finishes mark_processed on the first card (and was told
        // success), while the second card is garbled.
        api.before_board(3, move |fake| {
            let processed = fake.bucket_titled(project, "processed");
            fake.placement.lock().unwrap().insert(handled_card, processed);
            let mut tasks = fake.tasks.lock().unwrap();
            let card = tasks.iter_mut().find(|t| t.id == garbled_card).expect("the card");
            card.description = "hand-garbled mid-delivery".into();
            card.raw["description"] = "hand-garbled mid-delivery".into();
        });

        let outcome = store.read_mailbox(&MailboxName("inbox".into())).await;
        assert!(outcome.is_err(), "a garbled batch must not report success: {outcome:?}");

        assert_eq!(
            fake.column_of(handled_card).as_deref(),
            Some("processed"),
            "the card a consumer already processed must NOT be dragged back into delivery"
        );
        assert_eq!(
            fake.column_of(garbled_card).as_deref(),
            Some("new"),
            "…while the genuinely failed card goes back with its description restored"
        );
        let again = contract::read(&store, "inbox").await;
        let bodies: Vec<&str> = again.messages.iter().map(|d| d.message.body.as_str()).collect();
        assert_eq!(
            bodies,
            vec!["message two"],
            "only the restored message is owed delivery; the processed one is done"
        );
    }

    /// **A delivery must never hand over a message somebody already handled.**
    /// The read-back tolerates a card that advanced past the state the write
    /// wrote — which is right for a post (a message that was consumed still
    /// exists). For a delivery it is not: a card found in `processed` at
    /// verification time was handled between the move and the read, and
    /// handing it to a consumer as `seen_before: false` is the
    /// double-processing bug, reported as success. It is dropped from the
    /// delivery instead — never handed over, and never rolled back either: the
    /// outcome the handler recorded on it stands.
    #[tokio::test]
    async fn a_message_handled_under_a_delivery_is_dropped_not_handed_over() {
        let fake = FakeVikunja::new();
        let api = Interleaved::new(fake.clone());
        let store = VikunjaStore::from_api(api.clone(), PROJECT);
        contract::create(&store, "inbox").await;
        let handled = contract::post(&store, "inbox", "alpha", "message one", 0).await;
        let owed = contract::post(&store, "inbox", "milhouse", "message two", 60).await;
        let project = fake.projects_titled(PROJECT)[0].id;
        let handled_card: u64 = handled.id.as_str().parse().expect("a numeric card id");

        // A delivery reads the board to find what it is owed, moves the batch,
        // and reads again to verify — board call 3. Right before that, the
        // operator handles the first message and drags it to `processed`.
        api.before_board(3, move |fake| {
            let processed = fake.bucket_titled(project, "processed");
            fake.placement.lock().unwrap().insert(handled_card, processed);
        });

        let delivery = contract::read(&store, "inbox").await;
        let ids: Vec<&str> = delivery.messages.iter().map(|d| d.message.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![owed.id.as_str()],
            "a message someone already handled must not be delivered as mail: {:?}",
            delivery.messages
        );
        assert_eq!(
            fake.column_of(handled_card).as_deref(),
            Some("processed"),
            "…and it is left where the handler put it, not rolled back into the funnel"
        );
    }

    // --- rollback never deletes -----------------------------------------------

    /// **A consumed message is a delivered message, not a failed post.** The
    /// operator working the board can move the card `new → read` between the
    /// post's placement and its read-back (a second jojobot verb cannot — the
    /// lock rules that out, which is exactly why the read-back's remaining job
    /// is the human); the read-back then finds the card in
    /// a *later* state than it wrote. That is delivery working — the message
    /// exists and someone received it. The rollback used to call this a
    /// mismatch and delete the card, destroying a message a consumer had
    /// already been handed.
    #[tokio::test]
    async fn a_post_racing_a_concurrent_delivery_is_success_not_a_rollback() {
        let fake = FakeVikunja::new();
        let api = Interleaved::new(fake.clone());
        let store = VikunjaStore::from_api(api.clone(), PROJECT);
        contract::create(&store, "inbox").await;
        let project = fake.projects_titled(PROJECT)[0].id;

        // The post's read-back is its first board read; right before it, a
        // concurrent consumer takes delivery of the box: everything in `new`
        // moves to `read`.
        api.before_board(1, move |fake| {
            let new = fake.bucket_titled(project, "new");
            let read = fake.bucket_titled(project, "read");
            for bucket in fake.placement.lock().unwrap().values_mut() {
                if *bucket == new {
                    *bucket = read;
                }
            }
        });

        let posted = store
            .post_message(NewMessage {
                mailbox: MailboxName("inbox".into()),
                body: "the shipment landed".into(),
                sender: "alpha".into(),
                sent_at: at(1_780_000_000),
            })
            .await
            .expect("a consumed message is a delivered message, not a failure")
            .written()
            .expect("…and not a blocked write either");

        let card: u64 = posted.id.as_str().parse().expect("a numeric card id");
        assert_eq!(
            fake.tasks_in(project).len(),
            1,
            "the card a consumer already received must never be deleted"
        );
        assert_eq!(
            fake.column_of(card).as_deref(),
            Some("read"),
            "…and it stays exactly where the consumer's delivery moved it"
        );
        assert_eq!(posted.body, "the shipment landed");
        assert_eq!(
            posted.state,
            MessageState::Read,
            "the post reports the state the card is actually in"
        );
    }

    // --- jojobot never races jojobot -----------------------------------------

    /// **Two deliveries of one message must not both call it fresh mail.**
    /// Every verb is a read-modify-verify sequence, so two of them running at
    /// once over one board interleave: both reads see the message in `new`,
    /// both move it to `read`, and both hand it over with `seen_before: false`
    /// — the same message processed twice by two consumers, each told it was
    /// the first. The per-store lock makes that history unreachable: one read
    /// finishes before the other starts, so the second sees the column the
    /// first moved and flags its leftover.
    #[tokio::test]
    async fn two_concurrent_reads_never_both_call_one_message_fresh() {
        let fake = FakeVikunja::new();
        let store = VikunjaStore::from_api(Interleaved::new(fake.clone()), PROJECT);
        // Provisioning runs first, alone: the race under test is over one
        // message, not over two verbs each creating the board's columns.
        contract::create(&store, "inbox").await;
        contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;

        let name = MailboxName("inbox".into());
        let (first, second) = tokio::join!(store.read_mailbox(&name), store.read_mailbox(&name));
        let delivered = |outcome: Result<Guarded<Delivery>, MailboxError>| -> Vec<bool> {
            outcome
                .expect("read_mailbox ok")
                .written()
                .expect("the guard must not block a box that exists")
                .messages
                .iter()
                .map(|d| d.seen_before)
                .collect()
        };
        let mut flags = delivered(first);
        flags.extend(delivered(second));

        assert_eq!(flags.len(), 2, "both reads still hand the message over: {flags:?}");
        assert_eq!(
            flags.iter().filter(|fresh| !**fresh).count(),
            1,
            "exactly one delivery may call it fresh mail; the other is a leftover: {flags:?}"
        );
    }

    /// **A card update must not blank the rest of the card.** Vikunja's task
    /// update writes the whole model, so any writable field missing from the
    /// payload is written back as its zero value — a due date, a priority, an
    /// assignee, the kanban position. jojobot rewrites two fields and has to
    /// send back everything else exactly as it found it.
    #[tokio::test]
    async fn processing_a_message_preserves_every_other_field_on_the_card() {
        let fake = FakeVikunja::new();
        let store = store_with_box(fake.clone(), "inbox").await;
        let posted = contract::post(&store, "inbox", "alpha", "the shipment landed", 0).await;
        let card: u64 = posted.id.as_str().parse().expect("a numeric card id");

        // Something the operator set by hand on the card, that jojobot's own
        // model knows nothing about.
        fake.set_field(card, "priority", serde_json::json!(3));
        fake.set_field(card, "due_date", serde_json::json!("2026-08-01T00:00:00Z"));

        store.mark_processed(&posted.id, Some("filed")).await.expect("mark_processed ok");

        assert_eq!(fake.field(card, "priority"), serde_json::json!(3));
        assert_eq!(
            fake.field(card, "due_date"),
            serde_json::json!("2026-08-01T00:00:00Z"),
            "a field jojobot does not model must survive a field jojobot does write"
        );
    }
}

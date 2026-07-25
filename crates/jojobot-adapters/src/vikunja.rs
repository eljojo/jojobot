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

/// The board endpoint paginates the cards **inside** each column, so it gets a
/// wider page: a mailbox with a few hundred archived messages is ordinary, and a
/// truncated read would silently under-report every count.
const BOARD_PAGE: u64 = 250;

/// The marker jojobot stamps into the description of everything it creates —
/// the project and every mailbox label — and checks on match, so it only ever
/// adopts something it created itself.
const OWNER_TAG: &str = "[jojobot:owned]";

/// The prefix on every mailbox label's title.
///
/// **Vikunja labels are global, not per-project**, so a mailbox label shares one
/// namespace with every facet the operator uses on their own boards. Without a
/// prefix a mailbox named for an ordinary word would collide with one of theirs
/// in the UI and in every label list. The prefix is presentation only: the
/// mailbox's name is what follows it.
const LABEL_PREFIX: &str = "jojobot-mailbox/";

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
}

// --- the store --------------------------------------------------------------

/// The real Mailboxes adapter, fronting a Vikunja project it manages by name.
/// Stateless: it holds an API client and the project *name*, never an id.
#[derive(Clone)]
pub struct VikunjaStore {
    api: Arc<dyn VikunjaApi>,
    project: String,
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
        }
    }

    /// The description jojobot stamps on everything it creates.
    fn owner_description(&self) -> String {
        format!("Managed by jojobot — do not edit by hand. {OWNER_TAG}")
    }

    /// Every project that is both named ours AND carries the ownership tag —
    /// paged in full.
    async fn owned_projects(&self) -> Result<Vec<ProjectRec>, MailboxError> {
        let mut owned = Vec::new();
        let mut page = 1;
        loop {
            let batch = self.api.list_projects(page, PAGE).await?;
            let count = batch.len() as u64;
            owned.extend(
                batch
                    .into_iter()
                    .filter(|p| p.title == self.project && p.description.contains(OWNER_TAG)),
            );
            if count < PAGE {
                break;
            }
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
        let mut owned: Vec<LabelRec> = Vec::new();
        let mut page = 1;
        loop {
            let batch = self.api.list_labels(page, PAGE).await?;
            let count = batch.len() as u64;
            owned.extend(batch.into_iter().filter(|l| {
                l.title.starts_with(LABEL_PREFIX) && l.description.contains(OWNER_TAG)
            }));
            if count < PAGE {
                break;
            }
            page += 1;
        }
        // Oldest wins, so a concurrent double-create of one mailbox converges
        // rather than leaving two labels answering to one name.
        owned.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
        let mut seen = std::collections::HashSet::new();
        Ok(owned
            .into_iter()
            .filter_map(|l| {
                let name = MailboxName(l.title.strip_prefix(LABEL_PREFIX)?.to_string());
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
            let mut more = false;
            for bucket in batch {
                more |= bucket.tasks.len() as u64 >= BOARD_PAGE;
                match merged.iter_mut().find(|b| b.id == bucket.id) {
                    Some(existing) => existing.tasks.extend(bucket.tasks),
                    None => merged.push(bucket),
                }
            }
            if !more {
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
    /// A card that carries no machine block is **not a message**: it is
    /// something a human put on the board, and jojobot neither delivers it nor
    /// counts it as mail. A card whose label is not a mailbox label is skipped
    /// for the same reason.
    async fn messages(&self, scope: &Scope) -> Result<Vec<(TaskRec, Message)>, MailboxError> {
        let mut found = Vec::new();
        for bucket in self.board(scope).await? {
            let Some(state) = MessageState::from_token(&bucket.title) else {
                continue;
            };
            for task in bucket.tasks {
                let Some(mailbox) = task
                    .labels
                    .iter()
                    .find_map(|l| l.strip_prefix(LABEL_PREFIX))
                    .map(|n| MailboxName(n.to_string()))
                else {
                    continue;
                };
                let Some((body, envelope)) = parse_description(&task.description) else {
                    tracing::warn!(
                        card = task.id,
                        "a card in jojobot's mailbox project carries no machine block — \
                         not delivering it as a message"
                    );
                    continue;
                };
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
        Ok(found)
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

    /// Put a card back the way a failed write found it. A read-back mismatch
    /// means the store transformed what was written; leaving the transformed
    /// card behind hands a garbled message to the next consumer. Best-effort:
    /// the returned clause lands in the error, so the caller knows which state
    /// the card is actually in.
    async fn restore(
        &self,
        scope: &Scope,
        task: &TaskRec,
        state: MessageState,
        verb: &str,
    ) -> String {
        let description = self
            .api
            .update_task(scope.project(), task.id, &task.title, &task.description)
            .await;
        let column = match self.column(scope, state).await {
            Ok(bucket) => self
                .api
                .move_task(scope.project(), scope.view, bucket, task.id)
                .await,
            Err(e) => Err(e),
        };
        match (description, column) {
            (Ok(()), Ok(())) => format!("the card was restored to its state before this {verb}"),
            (d, c) => {
                let failure = d.err().map(|e| e.to_string()).or(c.err().map(|e| e.to_string()));
                format!(
                    "AND restoring the card failed ({}) — it may be left mid-{verb}",
                    failure.unwrap_or_default()
                )
            }
        }
    }

    /// Delete a card jojobot created seconds ago whose read-back did not match.
    ///
    /// This is the one place a card is removed, and it is a **rollback, not a
    /// lifecycle step**: `processed` is the terminal state and nothing ever
    /// leaves it. A create has no prior state to restore to — its prior state is
    /// absence — and unlike a Memory doc, a half-written message card is not
    /// inert: left in `new`, the next `read_mailbox` would deliver it.
    async fn undo_create(&self, task_id: u64, verb: &str) -> String {
        match self.api.delete_task(task_id).await {
            Ok(()) => format!("the card this {verb} created was removed"),
            Err(e) => format!(
                "AND removing the card this {verb} created failed ({e}) — card {task_id} may \
                 remain on the board"
            ),
        }
    }
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
    async fn create_mailbox(&self, name: &MailboxName) -> Result<Guarded<Mailbox>, MailboxError> {
        validate_mailbox_name(name)?;
        // Resolving the scope first is what makes the very first call to a bare
        // Vikunja work: the project and its columns are provisioned before the
        // box that will live on them.
        self.resolve_scope().await?;

        let existing = self.mailbox_names().await?;
        if let Decision::Block(candidates) = guard::decide_create(name, &existing) {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }

        self.api
            .create_label(
                &format!("{LABEL_PREFIX}{name}"),
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
        }))
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        let scope = self.resolve_scope().await?;
        let messages = self.messages(&scope).await?;
        Ok(self
            .mailbox_names()
            .await?
            .into_iter()
            .map(|name| {
                let mut counts = StateCounts::default();
                for (_, message) in messages.iter().filter(|(_, m)| m.mailbox == name) {
                    counts.add(message.state);
                }
                Mailbox { name, counts }
            })
            .collect())
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
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
        // it in the view's default column and gives it no labels. Both are set
        // here, and both are checked by the read-back below.
        self.api.set_task_labels(card.id, &[label]).await?;
        let new_column = self.column(&scope, MessageState::New).await?;
        self.api
            .move_task(scope.project(), scope.view, new_column, card.id)
            .await?;

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
            Ok(seen) if seen == expected => Ok(Guarded::Written(seen)),
            outcome => {
                let undone = self.undo_create(card.id, "post_message").await;
                Err(MailboxError::Store(match outcome {
                    Ok(seen) => format!(
                        "message {} read back changed: wrote {expected:?}, read {seen:?}; {undone}",
                        expected.id
                    ),
                    Err(e) => format!("{e}; {undone}"),
                }))
            }
        }
    }

    async fn read_mailbox(&self, name: &MailboxName) -> Result<Guarded<Delivery>, MailboxError> {
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
        for (card, message) in owed {
            let seen_before = message.state == MessageState::Read;
            if !seen_before {
                self.api
                    .move_task(scope.project(), scope.view, read_column, card.id)
                    .await?;
            }
            delivered.push((card, message, seen_before));
        }

        // Read-back: a delivery is only a delivery once the column moved. A
        // message reported as delivered but still sitting in `new` would be
        // handed to the next consumer as fresh mail — the duplicate-delivery
        // bug, reported as success.
        let after = self.messages(&scope).await?;
        let mut messages = Vec::with_capacity(delivered.len());
        for (card, expected, seen_before) in delivered {
            let seen = after
                .iter()
                .find(|(_, m)| m.id == expected.id)
                .map(|(_, m)| m.clone());
            let moved = Message {
                state: MessageState::Read,
                ..expected.clone()
            };
            match seen {
                Some(seen) if seen == moved => messages.push(Delivered {
                    message: seen,
                    seen_before,
                }),
                seen => {
                    let restored = self
                        .restore(&scope, &card, expected.state, "read_mailbox")
                        .await;
                    return Err(MailboxError::Store(format!(
                        "message {} did not read back as delivered: expected {moved:?}, \
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
        self.api
            .update_task(
                scope.project(),
                card.id,
                &card.title,
                &render_description(&message.body, &envelope),
            )
            .await?;
        let processed_column = self.column(&scope, MessageState::Processed).await?;
        self.api
            .move_task(scope.project(), scope.view, processed_column, card.id)
            .await?;

        let expected = Message {
            state: MessageState::Processed,
            notes,
            ..message.clone()
        };
        match self.read_back(&scope, id).await {
            Ok(seen) if seen == expected => Ok(seen),
            outcome => {
                let restored = self
                    .restore(&scope, &card, message.state, "mark_processed")
                    .await;
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
    }

    /// The column Vikunja gives a fresh kanban view. Deliberately not one of
    /// jojobot's three: a card that lands here is a card the store must move.
    const DEFAULT_COLUMN: &str = "Backlog";

    impl FakeVikunja {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
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
            });
            self.task_labels.lock().unwrap().insert(id, labels.to_vec());
            if let Some(bucket) = self.default_bucket(project, self.kanban_view(project)) {
                self.placement.lock().unwrap().insert(id, bucket);
            }
            id
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
                .skip(((page - 1) * per_page) as usize)
                .take(per_page as usize)
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
                        .skip(((page - 1) * per_page) as usize)
                        .take(per_page as usize)
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
            let id = self.next_id();
            let task = TaskRec {
                id,
                project_id,
                title: title.into(),
                description: self.mangle(description),
                labels: Vec::new(),
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

        async fn update_task(
            &self,
            _project_id: u64,
            task_id: u64,
            title: &str,
            description: &str,
        ) -> Result<(), MailboxError> {
            let description = self.mangle(description);
            let mut tasks = self.tasks.lock().unwrap();
            match tasks.iter_mut().find(|t| t.id == task_id) {
                Some(task) => {
                    task.title = title.into();
                    task.description = description;
                    Ok(())
                }
                None => Err(MailboxError::Store(format!("update_task: no card {task_id}"))),
            }
        }

        async fn delete_task(&self, task_id: u64) -> Result<(), MailboxError> {
            self.tasks.lock().unwrap().retain(|t| t.id != task_id);
            self.placement.lock().unwrap().remove(&task_id);
            self.task_labels.lock().unwrap().remove(&task_id);
            Ok(())
        }

        async fn move_task(
            &self,
            _project_id: u64,
            _view_id: u64,
            bucket_id: u64,
            task_id: u64,
        ) -> Result<(), MailboxError> {
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
                .skip(((page - 1) * per_page) as usize)
                .take(per_page as usize)
                .cloned()
                .collect())
        }

        async fn create_label(
            &self,
            title: &str,
            description: &str,
        ) -> Result<LabelRec, MailboxError> {
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
            self.task_labels.lock().unwrap().insert(task_id, labels.to_vec());
            Ok(())
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

    // --- read-back, and what a failed write leaves behind ----------------------

    /// A post whose read-back mismatches leaves **no card on the board**. Unlike
    /// a half-written Memory doc, a half-written message card is not inert: left
    /// in `new`, the next read would hand it to a consumer as mail.
    #[tokio::test]
    async fn a_failed_post_leaves_no_card_on_the_board() {
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

        assert!(
            fake.tasks_in(project).is_empty(),
            "no card is left for the next read to deliver: {:?}",
            fake.tasks_in(project)
        );
        assert_eq!(contract::counts(&store, "inbox").await.expect("inbox").total(), 0);
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
}

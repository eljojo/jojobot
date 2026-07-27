//! The Sessions adapter — jojobot's session board, in **its own Vikunja
//! project**.
//!
//! One session is one card. The **column is the state**; the **description is
//! current truth** (what the session is working on, plus the machine block that
//! says which bot and when it began); the **comments are the chronology**,
//! oldest first.
//!
//! Comments carry the chronology because they are the one Vikunja surface that
//! is genuinely append-shaped: each keeps its own id, editing one leaves the
//! rest alone, and nothing rewrites the set. Entries as more lines in the
//! description would be rewritten whole on every write, which is how an
//! append-only record quietly stops being one.
//!
//! **A different project from Mailboxes, and the write scope says so.** Both
//! contexts front the same Vikunja and the operator's real boards live there
//! too; this store discovers and provisions exactly one project of its own and
//! refuses to touch a card that declares any other.

use std::sync::Arc;

use async_trait::async_trait;
use jiff::Timestamp;

use jojobot_domain::mailbox::MailboxError;
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, SessionState,
    Sessions, normalize_entry, validate_entry, validate_focus, validate_session_id,
};

use super::api::{CommentRec, HttpVikunja, TaskRec, Unconfigured, VikunjaApi};
use super::board::{PAGE, Provisioner, Scope};
use super::codec::{field, render_block, split_description};
use super::VikunjaConfig;

/// The board endpoint pages the cards inside each column, so the board read
/// pages too — `wrapped` is an archive that never drains.
const BOARD_PAGE: u64 = PAGE;

/// The machine-block field naming the bot a session is one run of.
const BOT: &str = "bot";
/// The machine-block field carrying the instant a session began.
const STARTED_AT: &str = "started-at";
/// The machine-block field carrying the instant an entry was recorded.
const AT: &str = "at";
/// The machine-block field naming the verb class an automatic beat is about.
/// Absent on an entry the session wrote itself.
const BEAT: &str = "beat";

/// How much of the focus rides in the card's title.
const TITLE_BUDGET: usize = 60;

/// The real Sessions adapter. Stateless as far as Vikunja goes: it holds an API
/// client and the project *name*, never an id.
///
/// # The lock
///
/// Every verb is a read-modify-verify sequence over the board, so two running at
/// once interleave — the same reason the mailbox store serializes, and the same
/// remedy. One store = one project = one lock, shared across clones so a cloned
/// handle is the same writer rather than a second one.
pub struct VikunjaSessions {
    api: Arc<dyn VikunjaApi>,
    project: String,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl VikunjaSessions {
    /// The project this store manages when nobody says otherwise. **Not the
    /// mailbox project**: two contexts, two boards, and a card on one is never
    /// visible to the other.
    pub const DEFAULT_PROJECT: &'static str = "jojobot-sessions";

    /// A store pointed at Vikunja via credentials, managing the default project.
    pub fn new(http: reqwest::Client, config: VikunjaConfig) -> Self {
        Self::with_project(http, config, Self::DEFAULT_PROJECT)
    }

    /// A store managing a named project (e.g. a throwaway one for the gated
    /// integration test).
    pub fn with_project(
        http: reqwest::Client,
        config: VikunjaConfig,
        project: impl Into<String>,
    ) -> Self {
        Self::from_api(
            Arc::new(HttpVikunja::new(http, config.base_url, config.token)),
            project,
        )
    }

    /// A store with no credentials — every verb refuses, loudly.
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

    /// The shared adopt-or-create, told which columns this board carries.
    ///
    /// **`wrapped` IS done**, mirroring `processed` on the mailbox board: it is
    /// the end a session reaches by finishing the way it was meant to, so the
    /// operator's UI and jojobot's archive agree about it. `abandoned` is
    /// deliberately not the done column — a session that stopped without telling
    /// its story is not a finished one.
    fn provisioner(&self) -> Provisioner<'_> {
        Provisioner {
            api: self.api.as_ref(),
            project: &self.project,
            columns: &["active", "wrapped", "abandoned"],
            done: Some(SessionState::Wrapped.as_token()),
        }
    }

    async fn scope(&self) -> Result<Scope, SessionError> {
        self.provisioner().resolve().await.map_err(store)
    }

    async fn column(&self, scope: &Scope, state: SessionState) -> Result<u64, SessionError> {
        self.provisioner()
            .column(scope, state.as_token())
            .await
            .map_err(store)
    }

    /// The whole board, paged until every column returns nothing new.
    async fn board(&self, scope: &Scope) -> Result<Vec<(TaskRec, SessionState)>, SessionError> {
        let mut found: Vec<(TaskRec, SessionState)> = Vec::new();
        let mut page = 1;
        loop {
            let batch = self
                .api
                .board(scope.project(), scope.view, page, BOARD_PAGE)
                .await
                .map_err(store)?;
            if batch.is_empty() {
                break;
            }
            let mut any = false;
            for bucket in batch {
                let state = SessionState::from_token(&bucket.title);
                for task in bucket.tasks {
                    any = true;
                    // The one choke point for the write-scope invariant: every
                    // card this store ever writes to arrives either from here or
                    // from its own `create_task`.
                    if !scope.owns(&task) {
                        return Err(SessionError::ForeignProject(format!(
                            "card {} declares project {}, not jojobot's session project {}",
                            task.id,
                            task.project_id,
                            scope.project()
                        )));
                    }
                    // A card in a column that is no state is somebody's note, or
                    // a session card a person dragged out of the funnel. Either
                    // way jojobot cannot say what state it is in, so it acts on
                    // none of them — the mailbox board's quarantine rule, in this
                    // context's vocabulary.
                    let Some(state) = state else {
                        tracing::warn!(
                            card = task.id,
                            "a card on the session board sits in a column that is no state — \
                             left alone, not read as a session"
                        );
                        continue;
                    };
                    found.push((task, state));
                }
            }
            if !any {
                break;
            }
            page += 1;
        }
        Ok(found)
    }

    /// One card read as a session, chronology and all. `None` when the card
    /// carries no readable machine block — a card a person added by hand is not
    /// a session, and inventing a bot for it would put a run on the record that
    /// nobody started.
    async fn read_card(
        &self,
        card: &TaskRec,
        state: SessionState,
    ) -> Result<Option<Session>, SessionError> {
        let Some((focus, fields)) = parse_session(&card.description) else {
            tracing::warn!(
                card = card.id,
                "a card on the session board carries no readable machine block — left alone, \
                 not read as a session"
            );
            return Ok(None);
        };
        let (Some(bot), Some(started_at)) = (field(&fields, BOT), field(&fields, STARTED_AT))
        else {
            return Ok(None);
        };
        let Ok(started_at) = started_at.parse::<Timestamp>() else {
            return Ok(None);
        };
        Ok(Some(Session {
            id: SessionId(card.id.to_string()),
            bot: EntityId(bot),
            focus,
            started_at,
            state,
            entries: self.entries(card.id).await?,
        }))
    }

    /// A card's chronology, oldest first.
    ///
    /// **Ordered by the instant in each entry's own block, not by the order the
    /// store returned them** — an id breaks a tie, so the order is total and two
    /// reads agree. A comment jojobot cannot read is left out rather than given
    /// an invented time: it is somebody's note on the card, not a beat.
    async fn entries(&self, card: u64) -> Result<Vec<JournalEntry>, SessionError> {
        let mut found: Vec<(u64, JournalEntry)> = self
            .api
            .list_comments(card)
            .await
            .map_err(store)?
            .into_iter()
            .filter_map(|c| parse_entry(&c).map(|e| (c.id, e)))
            .collect();
        found.sort_by(|a, b| a.1.at.cmp(&b.1.at).then_with(|| a.0.cmp(&b.0)));
        Ok(found.into_iter().map(|(_, e)| e).collect())
    }

    /// Every readable session on the board.
    async fn sessions(&self, scope: &Scope) -> Result<Vec<(TaskRec, Session)>, SessionError> {
        let mut found = Vec::new();
        for (card, state) in self.board(scope).await? {
            if let Some(session) = self.read_card(&card, state).await? {
                found.push((card, session));
            }
        }
        Ok(found)
    }

    /// The card and session an id addresses, or the miss it earns.
    async fn addressed(
        &self,
        scope: &Scope,
        id: &SessionId,
    ) -> Result<(TaskRec, Session), SessionError> {
        self.sessions(scope)
            .await?
            .into_iter()
            .find(|(_, s)| &s.id == id)
            .ok_or_else(|| SessionError::UnknownSession {
                attempted: id.to_string(),
            })
    }

    /// The card and session an id addresses, refusing if it is already closed —
    /// **terminal both ways**, checked in one place so the write verbs cannot
    /// come to disagree about what closed means.
    async fn writable(
        &self,
        scope: &Scope,
        id: &SessionId,
    ) -> Result<(TaskRec, Session), SessionError> {
        let (card, session) = self.addressed(scope, id).await?;
        if session.state.is_terminal() {
            return Err(SessionError::Closed {
                attempted: id.to_string(),
                state: session.state,
            });
        }
        Ok((card, session))
    }

    /// Read one session back through the read path — the verification half of
    /// every write.
    async fn read_back(&self, scope: &Scope, id: &SessionId) -> Result<Session, SessionError> {
        self.addressed(scope, id).await.map(|(_, s)| s)
    }

    /// A card ready to be written back: exactly what Vikunja handed over, with
    /// the two fields jojobot owns replaced. Vikunja's task update writes the
    /// whole model, so a field left out is written back as its zero value.
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

    /// The cards a rollback must not touch, read as late as the rollback runs:
    /// one that reached a terminal column **that is not the one this write put
    /// it in**, because somebody else ended that session and their ending
    /// stands. `Err` is the blind case — with no board there is no way to tell a
    /// card somebody closed from one this verb owes a restore, so **nothing is
    /// put back**.
    ///
    /// The mailbox store's rule in this context's vocabulary. Where that one
    /// compares "further down the funnel", this compares against the state the
    /// write itself produced: a session's two ends are siblings, not stages, so
    /// there is no "past" to be further along — but a card sitting in exactly
    /// the column this verb just moved it to is this verb's own move to undo,
    /// not somebody else's ending to respect.
    ///
    /// The mailbox store's quarantine half has no analogue here: a card this
    /// store cannot read is already left out of every read it makes.
    async fn untouchable(
        &self,
        scope: &Scope,
        written: SessionState,
    ) -> Result<std::collections::HashSet<u64>, String> {
        match self.board(scope).await {
            Ok(board) => Ok(board
                .into_iter()
                .filter(|(_, state)| state.is_terminal() && *state != written)
                .map(|(card, _)| card.id)
                .collect()),
            Err(blind) => Err(format!(
                "the board could not be re-read, so nothing was put back rather than risk \
                 restoring a card somebody else has since closed: {blind}"
            )),
        }
    }

    /// Undo a column move, and **only** the column move — the rollback a verb
    /// that wrote no description is entitled to. See the mailbox store's
    /// `put_back_column` for why the two are not the same rollback.
    async fn restore_move(
        &self,
        scope: &Scope,
        card: &TaskRec,
        to: SessionState,
        written: SessionState,
    ) -> Result<(), Vec<(u64, String)>> {
        let untouchable = match self.untouchable(scope, written).await {
            Ok(untouchable) => untouchable,
            Err(blind) => return Err(vec![(card.id, blind)]),
        };
        if untouchable.contains(&card.id) {
            tracing::warn!(
                card = card.id,
                "the session this write moved has since been closed by somebody else — left \
                 exactly where it is rather than rolled back over their ending"
            );
            return Ok(());
        }
        let bucket = self
            .column(scope, to)
            .await
            .map_err(|e| vec![(card.id, e.to_string())])?;
        self.api
            .move_task(scope.project(), scope.view, bucket, card.id)
            .await
            .map_err(|e| vec![(card.id, e.to_string())])
    }

    /// Put a card's description back the way a failed write found it — the
    /// rollback a focus change earns by having written that description itself,
    /// exactly as `mark_processed` earns the mailbox store's.
    async fn restore_description(
        &self,
        scope: &Scope,
        card: &TaskRec,
    ) -> Result<(), Vec<(u64, String)>> {
        self.api
            .update_task(
                scope.project(),
                &Self::card_with(card, &card.title, &card.description),
            )
            .await
            .map_err(|e| vec![(card.id, e.to_string())])
    }
}

/// The error a failed verb answers with: the cause on its own when the rollback
/// held, and a [`SessionError::Stranded`] naming the cards when it did not.
fn stranded(verb: &str, cause: String, rollback: Result<(), Vec<(u64, String)>>) -> SessionError {
    match rollback {
        Ok(()) => SessionError::Store(format!(
            "{cause}; this {verb} left nothing mid-write — every card it moved is back where it \
             was, bar any that had since been closed, which are deliberately left alone"
        )),
        Err(failures) => SessionError::Stranded {
            verb: verb.to_string(),
            cards: failures.iter().map(|(card, _)| card.to_string()).collect(),
            cause,
            rollback: failures
                .iter()
                .map(|(card, why)| format!("card {card}: {why}"))
                .collect::<Vec<_>>()
                .join("; "),
        },
    }
}

/// Transport failures arrive in the mailbox context's error type, because the
/// Vikunja API port is shared infrastructure and predates this context. Mapped
/// here, at the one seam, rather than leaking a second context's vocabulary
/// through this store's surface.
fn store(e: MailboxError) -> SessionError {
    match e {
        MailboxError::NotConfigured(why) => SessionError::NotConfigured(why),
        other => SessionError::Store(other.to_string()),
    }
}

/// Split a session card's description into its focus and its block.
fn parse_session(description: &str) -> Option<(String, Vec<String>)> {
    split_description(description, |inner| {
        let has_bot = inner.iter().any(|l| field_line(l, BOT).is_some());
        let has_start = inner
            .iter()
            .any(|l| field_line(l, STARTED_AT).is_some_and(|v| v.parse::<Timestamp>().is_ok()));
        has_bot && has_start
    })
}

/// One comment read as a chronology entry, or `None` if it is not one of
/// jojobot's.
fn parse_entry(comment: &CommentRec) -> Option<JournalEntry> {
    let (text, fields) = split_description(&comment.text, |inner| {
        inner
            .iter()
            .any(|l| field_line(l, AT).is_some_and(|v| v.parse::<Timestamp>().is_ok()))
    })?;
    Some(JournalEntry {
        id: EntryId(comment.id.to_string()),
        at: field(&fields, AT)?.parse().ok()?,
        text,
        beat: field(&fields, BEAT),
    })
}

/// Render one chronology entry as a comment body.
fn render_entry(text: &str, at: Timestamp, beat: Option<&str>) -> String {
    render_block(
        text,
        &[
            (AT, at.to_string()),
            (BEAT, beat.unwrap_or_default().to_string()),
        ],
    )
}

/// Render a session card's description: the focus a human reads, then the block.
fn render_session(focus: &str, bot: &EntityId, started_at: Timestamp) -> String {
    render_block(
        focus,
        &[
            (BOT, bot.as_str().to_string()),
            (STARTED_AT, started_at.to_string()),
        ],
    )
}

/// The human-visible half of a session card: the bot, then its focus, cut on a
/// word boundary the way a message title is.
fn session_title(bot: &EntityId, focus: &str) -> String {
    let flat = focus.split_whitespace().collect::<Vec<_>>().join(" ");
    let head = if flat.chars().count() <= TITLE_BUDGET {
        flat
    } else {
        let mut kept = String::new();
        for word in flat.split(' ') {
            if kept.chars().count() + word.chars().count() + 1 > TITLE_BUDGET {
                break;
            }
            if !kept.is_empty() {
                kept.push(' ');
            }
            kept.push_str(word);
        }
        if kept.is_empty() {
            kept = flat.chars().take(TITLE_BUDGET).collect();
        }
        format!("{kept}…")
    };
    format!("{bot}: {head}")
}

/// The value of a `key: value` line, if this line is one.
fn field_line(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.strip_prefix(':')?.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

/// A minted id as the number it is, for tie-breaking. Ids are card ids rendered
/// decimal, so comparing them as text would put `10` before `2`.
fn numeric(id: &SessionId) -> u64 {
    id.as_str().parse().unwrap_or(u64::MAX)
}


#[async_trait]
impl Sessions for VikunjaSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        let _serialized = self.lock.lock().await;
        let scope = self.scope().await?;
        let mut mine: Vec<Session> = self
            .sessions(&scope)
            .await?
            .into_iter()
            .map(|(_, s)| s)
            .filter(|s| &s.bot == bot)
            .collect();
        mine.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| numeric(&b.id).cmp(&numeric(&a.id)))
        });
        Ok(mine)
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        let _serialized = self.lock.lock().await;
        validate_session_id(id)?;
        let scope = self.scope().await?;
        self.addressed(&scope, id).await.map(|(_, s)| s)
    }

    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        let _serialized = self.lock.lock().await;
        validate_focus(&new.focus)?;
        let scope = self.scope().await?;

        let focus = new.focus.trim().to_string();
        let card = self
            .api
            .create_task(
                scope.project(),
                &session_title(&new.bot, &focus),
                &render_session(&focus, &new.bot, new.started_at),
            )
            .await
            .map_err(store)?;

        // A fresh card lands in the view's default column, which is not
        // `active`. Nothing deletes it if the placement fails — this port has no
        // delete at all — so the failure says where the card is and that a
        // person has to look, rather than pretending it was never created.
        let placed = async {
            let active = self.column(&scope, SessionState::Active).await?;
            self.api
                .move_task(scope.project(), scope.view, active, card.id)
                .await
                .map_err(store)
        }
        .await;
        if let Err(e) = placed {
            return Err(SessionError::Store(format!(
                "{e} — card {} was created but never placed in `active`. It sits outside the \
                 funnel, where no verb reads it as a session, and a person has to look",
                card.id
            )));
        }

        let id = SessionId(card.id.to_string());
        let expected = Session {
            id: id.clone(),
            bot: new.bot,
            focus,
            started_at: new.started_at,
            state: SessionState::Active,
            entries: Vec::new(),
        };
        match self.read_back(&scope, &id).await {
            Ok(seen) if seen == expected => Ok(seen),
            outcome => Err(SessionError::Store(format!(
                "session {id} did not read back ({}) — card {} is on the board and a person has \
                 to look",
                match outcome {
                    Ok(seen) => format!("wrote {expected:?}, read {seen:?}"),
                    Err(e) => e.to_string(),
                },
                card.id
            ))),
        }
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        let _serialized = self.lock.lock().await;
        validate_session_id(id)?;
        validate_entry(&entry.text)?;
        let scope = self.scope().await?;
        let (card, _) = self.writable(&scope, id).await?;

        let text = normalize_entry(&entry.text);
        let written = self
            .api
            .create_comment(card.id, &render_entry(&text, entry.at, entry.beat.as_deref()))
            .await
            .map_err(store)?;

        // **No rollback, and that is the honest shape.** This port has no delete
        // — production jojobot never deletes anything — so a comment that does
        // not read back cannot be taken away. It is said plainly instead: the
        // entry may be sitting on the card unreadable, which a person can see
        // and repair, and the caller must not treat it as recorded.
        let expected = EntryId(written.id.to_string());
        self.entries(card.id)
            .await?
            .into_iter()
            .find(|e| e.id == expected)
            .filter(|e| e.text == text && e.beat == entry.beat)
            .ok_or_else(|| {
                SessionError::Store(format!(
                    "entry {expected} did not read back on session {id} — comment {} was written \
                     and cannot be read back as a chronology entry. Nothing deletes it; treat the \
                     entry as NOT recorded and look at card {}",
                    written.id, card.id
                ))
            })
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        let _serialized = self.lock.lock().await;
        validate_session_id(id)?;
        validate_entry(text)?;
        let scope = self.scope().await?;
        let (card, session) = self.writable(&scope, id).await?;

        let last = session
            .entries
            .last()
            .cloned()
            .ok_or_else(|| SessionError::NoEntries {
                attempted: id.to_string(),
            })?;
        let comment: u64 = last
            .id
            .as_str()
            .parse()
            .map_err(|_| SessionError::Store(format!("entry id {} is not a comment", last.id)))?;

        let text = normalize_entry(text);
        self.api
            .update_comment(
                card.id,
                comment,
                &render_entry(&text, last.at, last.beat.as_deref()),
            )
            .await
            .map_err(store)?;

        // The rollback this verb earns: it wrote this comment's text moments
        // ago, so a mismatch is most likely its own write coming back mangled,
        // and putting the previous text back is the repair.
        let seen = self
            .entries(card.id)
            .await?
            .into_iter()
            .find(|e| e.id == last.id);
        match seen {
            Some(seen) if seen.text == text && seen.at == last.at && seen.beat == last.beat => {
                Ok(seen)
            }
            other => {
                let restored = self
                    .api
                    .update_comment(
                        card.id,
                        comment,
                        &render_entry(&last.text, last.at, last.beat.as_deref()),
                    )
                    .await
                    .map_err(|e| vec![(card.id, e.to_string())]);
                Err(stranded(
                    "amend_journal",
                    format!("entry {} did not read back amended: read {other:?}", last.id),
                    restored,
                ))
            }
        }
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        let _serialized = self.lock.lock().await;
        validate_session_id(id)?;
        validate_focus(focus)?;
        let scope = self.scope().await?;
        let (card, session) = self.writable(&scope, id).await?;

        let focus = focus.trim().to_string();
        if let Err(e) = self
            .api
            .update_task(
                scope.project(),
                &Self::card_with(
                    &card,
                    &session_title(&session.bot, &focus),
                    &render_session(&focus, &session.bot, session.started_at),
                ),
            )
            .await
            .map_err(store)
        {
            let restored = self.restore_description(&scope, &card).await;
            return Err(stranded("journal", e.to_string(), restored));
        }

        let expected = Session { focus, ..session };
        match self.read_back(&scope, id).await {
            Ok(seen) if seen == expected => Ok(seen),
            outcome => {
                let restored = self.restore_description(&scope, &card).await;
                Err(stranded(
                    "journal",
                    match outcome {
                        Ok(seen) => {
                            format!("session {id} did not read back: wrote {expected:?}, read {seen:?}")
                        }
                        Err(e) => e.to_string(),
                    },
                    restored,
                ))
            }
        }
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        let _serialized = self.lock.lock().await;
        validate_session_id(id)?;
        let scope = self.scope().await?;
        let (card, session) = self.writable(&scope, id).await?;

        let bucket = self.column(&scope, to).await?;
        if let Err(e) = self
            .api
            .move_task(scope.project(), scope.view, bucket, card.id)
            .await
        {
            let restored = self.restore_move(&scope, &card, session.state, to).await;
            return Err(stranded("wrap_session", e.to_string(), restored));
        }

        let expected = Session { state: to, ..session.clone() };
        match self.read_back(&scope, id).await {
            Ok(seen) if seen == expected => Ok(seen),
            outcome => {
                let restored = self.restore_move(&scope, &card, session.state, to).await;
                Err(stranded(
                    "wrap_session",
                    match outcome {
                        Ok(seen) => format!(
                            "session {id} did not read back closed: wrote {expected:?}, read \
                             {seen:?}"
                        ),
                        Err(e) => e.to_string(),
                    },
                    restored,
                ))
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use jojobot_domain::session::testing::contract;

    use super::super::tests::{FakeVikunja, Interleaved};
    use jojobot_domain::mailbox::Mailboxes;
    use super::*;

    /// A throwaway board title, deliberately not the default one.
    const PROJECT: &str = "jojobot-sessions-test";

    fn store(fake: Arc<FakeVikunja>) -> VikunjaSessions {
        VikunjaSessions::from_api(fake, PROJECT)
    }

    fn bot(slug: &str) -> EntityId {
        EntityId(format!("bot:{slug}"))
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(secs).expect("a valid instant")
    }

    /// **The whole shared contract, against the real store over an API double.**
    /// The same suite the fake runs and the same one real Vikunja runs — which
    /// is what stops this adapter from satisfying its own idea of the spec.
    #[tokio::test]
    async fn the_vikunja_store_satisfies_the_contract() {
        contract::run_all(|| VikunjaSessions::from_api(FakeVikunja::new(), PROJECT)).await;
    }

    /// The board provisions itself: its own project — **not the mailbox one** —
    /// nested under jojobot's home, with the three columns and the done flag on
    /// `wrapped`.
    #[tokio::test]
    async fn self_provisions_its_own_project_and_its_three_columns() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());
        contract::begin(&store, "gamma", "reading the hand-off", 0).await;

        assert_eq!(fake.owned_titled(PROJECT), 1, "one board, tagged as jojobot's");
        let project = fake.projects_titled(PROJECT)[0].id;
        let home = fake.projects_titled("jojobot");
        assert_eq!(home.len(), 1, "jojobot's home is created if absent");
        assert_eq!(
            fake.projects_titled(PROJECT)[0].parent,
            home[0].id,
            "a new board is born inside the home"
        );

        let buckets = fake.buckets.lock().unwrap().clone();
        let columns: Vec<String> = buckets
            .iter()
            .filter(|(p, _, _)| *p == project)
            .map(|(_, _, b)| b.title.clone())
            .collect();
        for state in SessionState::ALL {
            assert!(
                columns.iter().any(|t| t == state.as_token()),
                "the board must carry a `{state}` column: {columns:?}"
            );
        }

        // `wrapped` is done; `abandoned` deliberately is not.
        let wrapped = fake.bucket_titled(project, "wrapped");
        let views = fake.views.lock().unwrap().clone();
        let view = views
            .iter()
            .find(|(p, v)| *p == project && v.kind == "kanban")
            .expect("a kanban view");
        assert_eq!(
            view.1.done_bucket_id, wrapped,
            "a session that wrapped reads as done in the operator's UI"
        );
    }

    /// **Two boards, and neither store can see the other's cards.** The session
    /// board is a different project from the mailbox board, so a message card is
    /// not a session and a session card is not mail.
    #[tokio::test]
    async fn the_session_board_is_a_different_project_from_the_mailbox_board() {
        let fake = FakeVikunja::new();
        let sessions = store(fake.clone());
        let mailboxes = super::super::VikunjaStore::from_api(fake.clone(), "jojobot-mailboxes-test");
        contract::begin(&sessions, "gamma", "reading the hand-off", 0).await;
        jojobot_domain::mailbox::testing::contract::create(&mailboxes, "inbox").await;
        jojobot_domain::mailbox::testing::contract::post(
            &mailboxes,
            "inbox",
            "alpha",
            "the shipment landed",
            0,
        )
        .await;

        assert_eq!(fake.owned_titled(PROJECT), 1);
        assert_eq!(fake.owned_titled("jojobot-mailboxes-test"), 1);
        assert_ne!(
            fake.projects_titled(PROJECT)[0].id,
            fake.projects_titled("jojobot-mailboxes-test")[0].id,
            "two contexts, two boards"
        );

        // Neither store reads the other's cards.
        let mine = sessions.sessions_of(&bot("gamma")).await.expect("list ok");
        assert_eq!(mine.len(), 1, "a message card is not a session: {mine:?}");
        let listed = mailboxes.list_mailboxes().await.expect("list ok");
        assert_eq!(
            listed.iter().map(|m| m.counts.total()).sum::<usize>(),
            1,
            "…and a session card is not mail: {listed:?}"
        );
    }

    /// A card a person added to the session board by hand is **not a session**.
    /// It has no declared bot and no start, and jojobot invents neither — a run
    /// on the record that nobody started is worse than a card nobody reads.
    #[tokio::test]
    async fn a_card_without_a_machine_block_is_never_read_as_a_session() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());
        let begun = contract::begin(&store, "gamma", "reading the hand-off", 0).await;
        let project = fake.projects_titled(PROJECT)[0].id;

        let stray = fake.seed_task(project, "a note to self", "just a note someone typed", &[]);
        let active = fake.bucket_titled(project, "active");
        fake.placement.lock().unwrap().insert(stray, active);

        let listed = store.sessions_of(&bot("gamma")).await.expect("list ok");
        let ids: Vec<&str> = listed.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec![begun.id.as_str()], "the hand-written card is not a session");
    }

    /// **A comment jojobot cannot read is not a beat.** An operator commenting
    /// on a session card is commenting, not journalling, and giving that comment
    /// an invented instant would put it in the chronology in an order nobody
    /// chose.
    #[tokio::test]
    async fn a_comment_that_is_not_an_entry_stays_out_of_the_chronology() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());
        let session = contract::begin(&store, "gamma", "reading the hand-off", 0).await;
        contract::journal(&store, &session.id, "read the task", 60).await;

        let card: u64 = session.id.as_str().parse().expect("a numeric card id");
        fake.create_comment(card, "looks good to me")
            .await
            .expect("a person can comment on a card");

        let read = store.read_session(&session.id).await.expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["read the task"], "only jojobot's own entries");
    }

    /// **The chronology is ordered by the instant in each entry, not by the
    /// order the store hands them back.** A store that returned comments in any
    /// order would otherwise ship a journal that reads differently every time.
    #[tokio::test]
    async fn the_chronology_is_ordered_by_its_own_instants() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());
        let session = contract::begin(&store, "gamma", "reading the hand-off", 0).await;
        // Written out of order on purpose: insertion order and instant disagree.
        contract::journal(&store, &session.id, "second", 120).await;
        contract::journal(&store, &session.id, "first", 60).await;

        let read = store.read_session(&session.id).await.expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["first", "second"], "oldest first, by the instant");
    }

    /// **A store that mangles a comment must not report the entry as recorded.**
    /// Nothing deletes a comment — this port has no delete at all — so the write
    /// says plainly that the entry is on the card and unreadable, rather than
    /// answering success for a beat no chronology will ever show.
    #[tokio::test]
    async fn an_entry_that_does_not_read_back_is_not_reported_as_recorded() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());
        let session = contract::begin(&store, "gamma", "reading the hand-off", 0).await;

        fake.poison_next_write();
        let err = store
            .append(&session.id, NewEntry::manual("read the task", at(60)))
            .await
            .expect_err("a mangled entry must not report success");
        assert!(
            err.to_string().contains("NOT recorded"),
            "the caller has to be told not to treat it as written: {err}"
        );

        let read = store.read_session(&session.id).await.expect("read ok");
        assert!(
            read.entries.is_empty(),
            "…and it is genuinely not in the chronology: {:?}",
            read.entries
        );
    }

    /// **Closing rolls back like a delivery, not like a retirement.** The verb
    /// writes no description, so its rollback puts back the column and nothing
    /// else — and a session somebody else closed inside the window is left where
    /// they closed it.
    #[tokio::test]
    async fn a_failed_close_puts_the_column_back_without_touching_the_card() {
        let fake = FakeVikunja::new();
        let api = Interleaved::new(fake.clone());
        let store = VikunjaSessions::from_api(api.clone(), PROJECT);
        let session = contract::begin(&store, "gamma", "reading the hand-off", 0).await;
        let project = fake.projects_titled(PROJECT)[0].id;
        let card: u64 = session.id.as_str().parse().expect("a numeric card id");

        // Right before the verification read, the operator retitles the card and
        // rewrites its focus — still a readable session, so the verification
        // fails on content and the rollback is this verb's to make.
        api.before_board(3, move |fake| {
            let rewritten = render_session("something else entirely", &bot("gamma"), at(0));
            let mut tasks = fake.tasks.lock().unwrap();
            let held = tasks.iter_mut().find(|t| t.id == card).expect("the card");
            held.title = "a title the operator typed".into();
            held.raw["title"] = "a title the operator typed".into();
            held.description = rewritten.clone();
            held.raw["description"] = rewritten.into();
        });

        let err = store
            .close(&session.id, SessionState::Wrapped)
            .await
            .expect_err("a close that could not verify must not report success");
        assert!(!matches!(err, SessionError::Stranded { .. }), "got {err:?}");

        assert_eq!(
            fake.column_of(card).as_deref(),
            Some("active"),
            "the column move is undone — the caller was told the session did not close"
        );
        let stored = fake
            .tasks_in(project)
            .into_iter()
            .find(|t| t.id == card)
            .expect("the card is still there");
        assert_eq!(
            stored.title, "a title the operator typed",
            "a verb that writes no title puts none back"
        );
        assert!(
            stored.description.contains("something else entirely"),
            "…and none of the description either: {:?}",
            stored.description
        );
    }

    /// **A rollback that cannot see the board puts NOTHING back**, and one that
    /// can leaves a session somebody else has since closed exactly where they
    /// closed it. Both halves of the rule the mailbox context paid for, held
    /// here from birth rather than after the same incident.
    #[tokio::test]
    async fn a_close_rollback_never_reopens_a_session_somebody_else_ended() {
        for blind in [false, true] {
            let fake = FakeVikunja::new();
            let api = Interleaved::new(fake.clone());
            let store = VikunjaSessions::from_api(api.clone(), PROJECT);
            let session = contract::begin(&store, "gamma", "reading the hand-off", 0).await;
            let project = fake.projects_titled(PROJECT)[0].id;
            let card: u64 = session.id.as_str().parse().expect("a numeric card id");

            // The operator wraps the session by hand right before this verb's
            // verification read, and the read fails.
            api.before_board(3, move |fake| {
                let wrapped = fake.bucket_titled(project, "wrapped");
                fake.placement.lock().unwrap().insert(card, wrapped);
                fake.fail_all("board");
            });
            if !blind {
                // …or the read fails for another reason and the board is
                // readable again by the time the rollback looks.
                api.before_board(3, move |fake| {
                    let wrapped = fake.bucket_titled(project, "wrapped");
                    fake.placement.lock().unwrap().insert(card, wrapped);
                    let mut tasks = fake.tasks.lock().unwrap();
                    let held = tasks.iter_mut().find(|t| t.id == card).expect("the card");
                    held.description = "hand-garbled".into();
                    held.raw["description"] = "hand-garbled".into();
                });
            }

            let outcome = store.close(&session.id, SessionState::Abandoned).await;
            assert!(outcome.is_err(), "blind={blind}: an unverifiable close must not succeed");
            assert_ne!(
                fake.column_of(card).as_deref(),
                Some("active"),
                "blind={blind}: a session somebody else ended must never be reopened"
            );
        }
    }

    /// The write-scope invariant, extended to this board: **no call ever names a
    /// project other than this store's own**, and no card outside it is ever
    /// written to. The fake records every project any call named and every card
    /// any call wrote.
    #[tokio::test]
    async fn no_verb_ever_reaches_a_project_other_than_this_stores() {
        let fake = FakeVikunja::new();
        let store = store(fake.clone());
        let session = contract::begin(&store, "gamma", "reading the hand-off", 0).await;
        contract::journal(&store, &session.id, "read the task", 60).await;
        store
            .amend_last(&session.id, "read the task properly")
            .await
            .expect("amend ok");
        store
            .set_focus(&session.id, "building the session context")
            .await
            .expect("focus ok");
        store
            .close(&session.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let project = fake.projects_titled(PROJECT)[0].id;
        let home = fake.projects_titled("jojobot")[0].id;
        let named = fake.named_projects.lock().unwrap().clone();
        let stray: Vec<u64> = named
            .iter()
            .copied()
            .filter(|p| *p != project && *p != home)
            .collect();
        assert!(stray.is_empty(), "a call named a project that is not ours: {stray:?}");

        let card: u64 = session.id.as_str().parse().expect("a numeric card id");
        let written = fake.written_tasks.lock().unwrap().clone();
        let strays: Vec<u64> = written.iter().copied().filter(|t| *t != card).collect();
        assert!(strays.is_empty(), "a call wrote to a card that is not ours: {strays:?}");
    }
}

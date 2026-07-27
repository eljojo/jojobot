//! Sessions — one mortal instance of a bot, on the record.
//!
//! A **bot** (Memory's ninth kind) is a role: durable, reusable, the same
//! identity next month. A **session is one run of it** — the unit of work, not
//! the unit of connection. It spans MCP connections and survives a disconnect or
//! a device hop, because what makes two connections the same session is the
//! identity that booted them, not the socket underneath.
//!
//! A **bounded context beside Memory and Mailboxes**, with its own store, and
//! the same shape as Mailboxes because it is the same kind of thing: a card is a
//! record, **the column IS the state**, and the terminal states are terminal
//! both ways.
//!
//! Three parts to a session, and they answer different questions:
//!
//! * **focus — what it is working on NOW.** Current truth, rewritten in place.
//!   A later reader asking "what is this session doing" gets one line, not a
//!   history to infer it from.
//! * **the chronology — what happened.** Append-only, oldest first. Only the
//!   most recent entry may be amended, and only in place; everything older is
//!   what it was. A journal that can be rewritten is a journal nobody can trust
//!   as evidence.
//! * **the lifecycle — `active` → `wrapped` | `abandoned`.** `wrapped` is a
//!   session whose story was told; `abandoned` is one that stopped without
//!   telling it. Both are terminal, and nothing walks back out of either.
//!
//! **This is user-agnostic software: no user PII, fixtures included.** Bots and
//! entries in tests and examples are openly fictional.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::memory::EntityId;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// A session's id — minted by the store, opaque to the domain. Validated as the
/// same narrow token a message id is, and for the same reason: it selects a card
/// to rewrite, so it never carries a path segment, a quote or a newline.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    /// Borrow the underlying id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One chronology entry's id — minted by the store (a comment id).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntryId(pub String);

impl EntryId {
    /// Borrow the underlying id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a session sits. **The column is the state**, exactly as it is for a
/// message — no second field to disagree with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// Running, or at least never closed.
    Active,
    /// Closed by the bot itself, story told. Terminal.
    Wrapped,
    /// Closed by the sweep: it stopped without telling its story. Terminal, and
    /// deliberately not the same word as `wrapped` — the difference between them
    /// is the whole point of having two.
    Abandoned,
}

impl SessionState {
    /// Every state, in funnel order — which is also the column order on the
    /// board. What reads a board's columns walks this, so a column title that is
    /// no state is never mistaken for one; **an adapter names its own columns**
    /// rather than deriving them here, because the board is that adapter's to
    /// provision and this is the domain.
    pub const ALL: [SessionState; 3] = [
        SessionState::Active,
        SessionState::Wrapped,
        SessionState::Abandoned,
    ];

    /// The wire token — and the column's title on the board.
    pub fn as_token(self) -> &'static str {
        match self {
            SessionState::Active => "active",
            SessionState::Wrapped => "wrapped",
            SessionState::Abandoned => "abandoned",
        }
    }

    /// Parse a state token. Strict, for the reason a message state is: a column
    /// title jojobot doesn't recognize is not a state, and guessing one would
    /// file a session in a lifecycle stage nobody chose.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_token() == token.trim())
    }

    /// Whether this state is an end. **A closed session takes no more entries**,
    /// whichever end it reached — no append, no amend, no focus change, no
    /// second close.
    pub fn is_terminal(self) -> bool {
        !matches!(self, SessionState::Active)
    }

    /// Whether this end is the last word — **the one place the two ends stop
    /// being the same.**
    ///
    /// **`wrapped` is final because wrapping PUBLISHES.** The story goes into
    /// the operator's Journal as one dated entry, and reopening the run would
    /// make an already-published entry retroactively false — an account of a
    /// thing that turns out not to have finished. That is what terminal-both-
    /// ways is protecting, and it is a rationale about *this state*.
    ///
    /// `abandoned` published nothing, which is the entire content of "it wasn't
    /// wrapped up": a disconnect, a closed laptop, an agent that moved on. So
    /// [`Sessions::reopen`] takes it back to `active` and the record continues
    /// where it stopped. Picking one back up is ordinary rather than recovery.
    ///
    /// **The corollary is why this walk-back has to exist at all.** Without it,
    /// no run that was ever interrupted could ever be wrapped properly — its
    /// story would be lost by construction, permanently, because the only verb
    /// that tells it refuses to run on a closed session. Reopening from
    /// `abandoned` is precisely what lets an interrupted run eventually tell its
    /// story.
    pub fn is_final(self) -> bool {
        matches!(self, SessionState::Wrapped)
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_token())
    }
}

/// How long a session may go without a beat before the next boot of its bot
/// sweeps it to `abandoned`.
///
/// **Named, not inlined.** It is a judgement about how long a plausible pause
/// lasts — a session that goes quiet overnight is still that night's work — and
/// a judgement written as a literal at its one call site is one nobody can find
/// to argue with.
///
/// Measured from the newest thing the session has to show for itself: its last
/// entry, or when it started if it never wrote one.
pub const ABANDONED_AFTER: jiff::SignedDuration = jiff::SignedDuration::from_hours(24);

/// How recently a run must have stopped for a boot to **offer** it back.
///
/// **Its own number, deliberately not [`ABANDONED_AFTER`].** They answer
/// different questions — one is "when does an unattended run stop being active",
/// the other is "how long do we keep bringing it up" — and fusing them means
/// changing one silently changes the other.
///
/// A week, from the operator's own session granularity: a run covers a milestone
/// or a few, so a week covers coming back after a weekend and stops offering
/// month-old runs nobody remembers.
///
/// **This bounds attention, never reachability.** A handle a caller still holds
/// addresses its session at any age — resuming an eight-month-old run works
/// perfectly well. The bound governs only what jojobot volunteers unprompted,
/// because an offer nobody wants is noise in front of the one they do.
///
/// Measured from the last beat, like staleness, because that is the only instant
/// the record carries: a card knows when it was last worked in, not when the
/// sweep got round to marking it.
pub const OFFER_ABANDONED_WITHIN: jiff::SignedDuration = jiff::SignedDuration::from_hours(24 * 7);

/// The id charset, `[a-z0-9-]` — the mailbox context's, for the same reasons.
fn is_id_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
}

/// Validate a session id arriving from a client.
pub fn validate_session_id(id: &SessionId) -> Result<(), SessionError> {
    let i = id.as_str();
    let ok = !i.is_empty() && i.len() <= 64 && i.bytes().all(is_id_byte);
    if ok {
        Ok(())
    } else {
        Err(SessionError::InvalidId(i.to_string()))
    }
}

/// Validate one chronology entry. Multi-line is ordinary — an entry is prose —
/// so only emptiness is refused, exactly as a message body is.
///
/// **The journal discipline is not enforced here.** "High-level beats, never a
/// firehose" is taught in the orientation a session reads, because it is a
/// judgement about what is worth recording and no length check can make it: a
/// two-line entry can be noise and a paragraph can be the one thing that
/// mattered.
pub fn validate_entry(entry: &str) -> Result<(), SessionError> {
    if entry.trim().is_empty() {
        return Err(SessionError::InvalidEntry("an entry is empty".into()));
    }
    Ok(())
}

/// Validate a focus line — what the session is working on now. One plain line:
/// it rides in the card's description above the machine block, and it is meant
/// to be read at a glance.
pub fn validate_focus(focus: &str) -> Result<(), SessionError> {
    let f = focus.trim();
    if f.is_empty() {
        return Err(SessionError::InvalidEntry("a focus is empty".into()));
    }
    if f.contains('\n') || f.contains('\r') || f.contains('`') || f.chars().any(char::is_control) {
        return Err(SessionError::InvalidEntry(
            "a focus must be one plain line (no newline, no backtick)".into(),
        ));
    }
    if f.chars().count() > 200 {
        return Err(SessionError::InvalidEntry("a focus is too long".into()));
    }
    Ok(())
}

/// Normalize an entry the way a message body is normalized: edge whitespace is
/// not significant and no store preserves it, and CRLF becomes `\n` so a
/// line-reconstructing store and a byte-preserving one give one answer.
pub fn normalize_entry(entry: &str) -> String {
    let mut entry = entry.to_string();
    while entry.contains("\r\n") {
        entry = entry.replace("\r\n", "\n");
    }
    entry.trim().to_string()
}

/// One entry in a session's chronology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The store-minted id — what an amend targets.
    pub id: EntryId,
    /// When it was recorded.
    pub at: Timestamp,
    /// The entry itself.
    pub text: String,
    /// When this entry was last rewritten, for one that has been.
    ///
    /// **`at` is when it happened; this is when it was last touched.** They are
    /// different questions and a beat needs both: `at` keeps the chronology in
    /// the order things occurred, and a tally corrected an hour later must not
    /// jump to the end of the record — while the sweep, which asks whether this
    /// session is still working, has to see that hour.
    ///
    /// Without it a session that had already used every verb class once went
    /// quiet as far as the sweep was concerned: each further call amended an
    /// existing beat, no instant moved, and a session working steadily became
    /// sweepable while it worked.
    #[serde(default)]
    pub touched: Option<Timestamp>,
    /// The verb class this beat summarizes, for an entry jojobot wrote itself;
    /// `None` for one the session wrote.
    ///
    /// **Marked apart on purpose.** A reader weighing a chronology has to be
    /// able to tell "the session said it was doing this" from "jojobot noticed
    /// it doing this" — the first is testimony about intent, the second is a
    /// record of calls. Collapsing them would make a machine's tally read as a
    /// session's account of itself.
    #[serde(default)]
    pub beat: Option<String>,
}

impl JournalEntry {
    /// Whether jojobot wrote this entry rather than the session.
    pub fn is_auto(&self) -> bool {
        self.beat.is_some()
    }
}

/// An entry about to be appended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntry {
    /// The entry text.
    pub text: String,
    /// When — passed in rather than read off a clock, so the domain stays
    /// deterministic.
    pub at: Timestamp,
    /// The verb class, for an automatic beat; `None` for a session's own entry.
    pub beat: Option<String>,
}

impl NewEntry {
    /// An entry the session wrote.
    pub fn manual(text: impl Into<String>, at: Timestamp) -> Self {
        NewEntry {
            text: text.into(),
            at,
            beat: None,
        }
    }

    /// A beat jojobot wrote about a verb class.
    pub fn beat(class: impl Into<String>, text: impl Into<String>, at: Timestamp) -> Self {
        NewEntry {
            text: text.into(),
            at,
            beat: Some(class.into()),
        }
    }
}

/// A session about to begin — everything but the id, which the store mints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    /// The bot this is one run of.
    pub bot: EntityId,
    /// What it is working on, at the moment it begins.
    pub focus: String,
    /// When it began.
    pub started_at: Timestamp,
}

/// One session on the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The store-minted id.
    pub id: SessionId,
    /// The bot this is one run of.
    pub bot: EntityId,
    /// What it is working on now — current truth, rewritten in place.
    pub focus: String,
    /// When it began.
    pub started_at: Timestamp,
    /// Which column it sits in.
    pub state: SessionState,
    /// The chronology, oldest first.
    pub entries: Vec<JournalEntry>,
}

impl Session {
    /// The instant this session last had anything to show for itself: its
    /// newest entry, or the moment it began if it never wrote one.
    ///
    /// **What the sweep measures.** Not "when did it last call something" — a
    /// session that boots and never journals leaves nothing to measure but its
    /// own start, and that is exactly the session the sweep exists for.
    pub fn last_beat(&self) -> Timestamp {
        self.entries
            .iter()
            .flat_map(|e| [e.at, e.touched.unwrap_or(e.at)])
            .max()
            .unwrap_or(self.started_at)
    }

    /// Whether `now` is far enough past this session's last beat to sweep it.
    /// Only an `active` session is ever swept: the other two are already closed.
    pub fn is_stale(&self, now: Timestamp) -> bool {
        !self.state.is_terminal() && now.duration_since(self.last_beat()) >= ABANDONED_AFTER
    }

    /// Whether a boot should **offer** this run back — an `abandoned` one that
    /// stopped inside [`OFFER_ABANDONED_WITHIN`].
    ///
    /// `wrapped` is never offered: its story was told and it does not reopen.
    /// An older `abandoned` run is not offered either, and stays resumable by
    /// anyone holding its handle — see the constant.
    pub fn is_offerable(&self, now: Timestamp) -> bool {
        self.state == SessionState::Abandoned
            && now.duration_since(self.last_beat()) < OFFER_ABANDONED_WITHIN
    }
}

/// Why a session operation failed. Adapters map their transport/parse errors
/// into these; the domain and the MCP layer speak only this vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The session id is not a well-formed token.
    #[error("invalid session id '{0}': ids are [a-z0-9-]+")]
    InvalidId(String),
    /// The entry or focus is malformed for storage.
    #[error("invalid entry: {0}")]
    InvalidEntry(String),
    /// The addressed session doesn't exist. Never created, never guessed at.
    #[error("no session '{attempted}' on jojobot's session board")]
    UnknownSession {
        /// The id that missed.
        attempted: String,
    },
    /// **The terminal-both-ways rule.** The session is closed, so it takes no
    /// more entries and cannot be closed again. Its own variant, because it is
    /// the answer a caller most needs told apart from "no such session": the id
    /// is real, the record is right there and readable, and the only thing that
    /// is over is writing to it.
    #[error(
        "session '{attempted}' is {state} — closed, and closed is terminal both ways. Its \
         chronology stands as the record of what happened, and nothing appends to it, amends it \
         or reopens it. If there is more to say, it belongs to a new session"
    )]
    Closed {
        /// The id that was addressed.
        attempted: String,
        /// Which end it reached.
        state: SessionState,
    },
    /// An amend that had nothing to amend. Refused rather than turned into an
    /// append: a caller who meant to correct a beat and silently wrote a new one
    /// has a chronology saying something they did not mean.
    #[error(
        "session '{attempted}' has no entries yet, so there is no most-recent one to amend — \
         journal it instead"
    )]
    NoEntries {
        /// The id that was addressed.
        attempted: String,
    },
    /// An out-of-order amend reached for an entry the session wrote itself.
    /// Only jojobot's own beats may be rewritten where they sit; a session's
    /// account of what it was doing is append-only, and the newest entry is the
    /// only one [`Sessions::amend_last`] will touch.
    #[error(
        "entry '{attempted}' on session '{session}' is not an automatic beat — it is what the \
         session itself recorded, and those are append-only. Only the most recent entry can be \
         amended, through amend_journal"
    )]
    NotABeat {
        /// The entry that was addressed.
        attempted: String,
        /// The session it is on.
        session: String,
    },
    /// **The write-scope invariant**, extended to this context's own project.
    /// The operator's boards live on the same Vikunja, and this store may touch
    /// exactly one project — a different one from the mailbox store's.
    #[error("refusing to touch a project other than jojobot's session project: {0}")]
    ForeignProject(String),
    /// A write failed, and putting the card back failed too. Its own variant for
    /// the reason the mailbox context's is: whether the rollback worked is the
    /// one thing a caller cannot infer from anything else in the answer.
    #[error(
        "{verb} failed ({cause}) AND putting it back failed ({rollback}) — card(s) {} are left \
         mid-{verb}, and a person has to look at the board",
        .cards.join(", ")
    )]
    Stranded {
        /// The verb that failed.
        verb: String,
        /// The cards left mid-write.
        cards: Vec<String>,
        /// What failed first.
        cause: String,
        /// Why the rollback could not undo it.
        rollback: String,
    },
    /// The underlying store failed.
    #[error("store error: {0}")]
    Store(String),
    /// The store isn't configured (no credentials).
    #[error("session store not configured: {0}")]
    NotConfigured(String),
}

/// The Sessions port. One real adapter stands behind it in production (Vikunja,
/// in its own project); a fake stands behind it in tests. The invariants every
/// adapter holds:
///
/// * **read-back** — a write succeeds only if reading it back through the read
///   path returns it, and a mismatch restores the prior state before erroring.
/// * **terminal both ways** — nothing appends to, amends, or reopens a closed
///   session, and no rollback moves a card out of a terminal column.
/// * **nothing is created as a side effect** — [`begin`](Sessions::begin) is the
///   only thing that brings a session card into being, and the caller decides
///   when. A boot that never works leaves no card.
#[async_trait::async_trait]
pub trait Sessions: Send + Sync {
    /// Every session of one bot, whatever its state, newest start first. What
    /// attaching reads, and what the sweep walks.
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError>;

    /// One session by id, chronology and all.
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError>;

    /// Bring a session card into being. **The only mint on this port** — see the
    /// trait's invariants.
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError>;

    /// Append one entry to the chronology. Refused on a closed session.
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError>;

    /// Rewrite the **most recent** entry in place. Refused on a closed session,
    /// and refused with [`SessionError::NoEntries`] when there is nothing to
    /// amend. Only the last one: everything older is append-only.
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError>;

    /// Rewrite one **automatic beat** in place, wherever it sits in the
    /// chronology — and **only** an automatic beat.
    ///
    /// A beat is jojobot's running tally of one verb class ("captured facts:
    /// …"), so a second capture does not deserve a second entry; it deserves the
    /// first one to say two. That is one fact getting more accurate, not a
    /// record being rewritten.
    ///
    /// The append-only rule is untouched by this, which is why the restriction
    /// is on the port rather than left to callers: an entry the session wrote is
    /// its own account of what it was doing, and
    /// [`SessionError::NotABeat`] is what reaching for one gets — only
    /// [`amend_last`](Sessions::amend_last) touches those, and only the newest.
    ///
    /// `at` records when the correction was made, and lands on the entry's
    /// `touched` rather than its `at` — the beat keeps its place in the
    /// chronology, and the session stops looking idle to the sweep.
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError>;

    /// Rewrite what the session is working on now. Refused on a closed session.
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError>;

    /// Move a session to a terminal state. Refused if it is already in one —
    /// terminal both ways.
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError>;

    /// Take an `abandoned` session back to `active`, so the run continues where
    /// it stopped rather than starting again beside it.
    ///
    /// **The one walk-back, and only from the end that told no story.** A
    /// `wrapped` session is refused with [`SessionError::Closed`] — see
    /// [`SessionState::is_final`]. An `active` one comes back unchanged, because
    /// a caller resuming the run they are already in has made no mistake.
    ///
    /// The chronology is untouched by this: reopening adds nothing, rewrites
    /// nothing, and leaves no mark saying the session was ever away. What it
    /// changes is what the store will accept next.
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError>;
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemorySessions, contract};
    use super::*;

    /// The full behavioural contract holds for the fake — the same suite the
    /// Vikunja adapter runs against its API double, and against real Vikunja.
    #[tokio::test]
    async fn the_fake_satisfies_the_contract() {
        contract::run_all(InMemorySessions::new).await;
    }

    #[test]
    fn the_three_states_round_trip_and_the_set_is_closed() {
        for state in SessionState::ALL {
            assert_eq!(SessionState::from_token(state.as_token()), Some(state));
        }
        assert_eq!(SessionState::ALL.len(), 3, "three columns, no more");
        for unknown in ["done", "Active", "", "open", "closed"] {
            assert_eq!(SessionState::from_token(unknown), None, "{unknown:?} is no state");
        }
        // The funnel order is the column order a board carries, in the order it
        // carries them — each adapter names its own, so this is what they are
        // named after rather than what walks them.
        assert_eq!(
            SessionState::ALL.map(|s| s.as_token()),
            ["active", "wrapped", "abandoned"]
        );
    }

    /// Both ends are ends. `wrapped` and `abandoned` differ in what they say
    /// about the session, never in whether it is over.
    #[test]
    fn only_active_is_not_terminal() {
        assert!(!SessionState::Active.is_terminal());
        assert!(SessionState::Wrapped.is_terminal());
        assert!(SessionState::Abandoned.is_terminal());
    }

    #[test]
    fn a_session_id_is_a_narrow_token() {
        assert!(validate_session_id(&SessionId("4212".into())).is_ok());
        for bad in ["", "../42", "4 2", "42;drop", "4\n2"] {
            assert!(
                validate_session_id(&SessionId(bad.into())).is_err(),
                "must reject {bad:?}"
            );
        }
    }

    /// An entry is prose; a focus is one line, because it rides above the
    /// machine block and is meant to be read at a glance.
    #[test]
    fn an_entry_is_prose_and_a_focus_is_one_line() {
        assert!(validate_entry("read the task, started the domain module").is_ok());
        assert!(validate_entry("two\n\nparagraphs").is_ok());
        assert!(validate_entry("   ").is_err());

        assert!(validate_focus("building the session context").is_ok());
        for bad in ["", "  ", "two\nlines", "carriage\rreturn", "back`tick"] {
            assert!(validate_focus(bad).is_err(), "must refuse the focus {bad:?}");
        }
        assert!(validate_focus(&"x".repeat(200)).is_ok());
        assert!(validate_focus(&"x".repeat(201)).is_err(), "and it is capped");
    }

    /// The sweep measures the newest thing a session has to show for itself —
    /// and for one that never journalled, that is when it began. A session that
    /// boots and does nothing is exactly the case the sweep exists for, so
    /// measuring only entries would leave it active forever.
    #[test]
    fn staleness_is_measured_from_the_last_beat_or_the_start() {
        let start = Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        let bare = Session {
            id: SessionId("1".into()),
            bot: EntityId("bot:gamma".into()),
            focus: "nothing yet".into(),
            started_at: start,
            state: SessionState::Active,
            entries: Vec::new(),
        };
        assert_eq!(bare.last_beat(), start, "no entries: the start is the last beat");
        assert!(!bare.is_stale(start + jiff::SignedDuration::from_hours(23)));
        assert!(bare.is_stale(start + ABANDONED_AFTER), "the threshold is inclusive");

        let hour = jiff::SignedDuration::from_hours(1);
        let busy = Session {
            entries: vec![JournalEntry {
                id: EntryId("e1".into()),
                at: start + hour,
                text: "did a thing".into(),
                touched: None,
                beat: None,
            }],
            ..bare.clone()
        };
        assert_eq!(busy.last_beat(), start + hour, "an entry moves the clock forward");
        assert!(
            !busy.is_stale(start + ABANDONED_AFTER),
            "…so the same instant that swept the bare one leaves this one alone"
        );

        // A closed session is never swept: it is already at an end.
        let closed = Session { state: SessionState::Wrapped, ..bare };
        assert!(!closed.is_stale(start + ABANDONED_AFTER + hour));
    }

    /// **What a boot volunteers is bounded; what a handle reaches is not.** An
    /// abandoned run inside the window is offered back, an older one is not, and
    /// a wrapped one never is — its story was told.
    #[test]
    fn only_a_recently_abandoned_run_is_offered_back() {
        let start = Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        let run = Session {
            id: SessionId("1".into()),
            bot: EntityId("bot:gamma".into()),
            focus: "reading the hand-off".into(),
            started_at: start,
            state: SessionState::Abandoned,
            entries: Vec::new(),
        };

        let day = jiff::SignedDuration::from_hours(24);
        assert!(run.is_offerable(start + day), "yesterday's run is the one to offer");
        assert!(
            run.is_offerable(start + OFFER_ABANDONED_WITHIN - day),
            "…and so is one from inside the window"
        );
        assert!(
            !run.is_offerable(start + OFFER_ABANDONED_WITHIN),
            "the bound is exclusive at the edge"
        );
        assert!(
            !run.is_offerable(start + OFFER_ABANDONED_WITHIN + day * 60),
            "a run from two months ago is not something to bring up"
        );

        // The other two states are never offered, whatever their age.
        for state in [SessionState::Active, SessionState::Wrapped] {
            let other = Session { state, ..run.clone() };
            assert!(
                !other.is_offerable(start + day),
                "{state} is not an abandoned run to offer back"
            );
        }

        // **The two thresholds are not the same number**, and fusing them would
        // make changing one silently change the other.
        assert!(
            OFFER_ABANDONED_WITHIN > ABANDONED_AFTER,
            "a run must be abandoned before it can be offered back as abandoned"
        );
    }

    /// An automatic beat is marked apart from a session's own words, so a
    /// reader can tell an account of intent from a tally of calls.
    #[test]
    fn an_automatic_beat_is_distinguishable_from_a_session_s_own_entry() {
        let at = Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        let manual = NewEntry::manual("read the task", at);
        let auto = NewEntry::beat("capture", "captured facts: person:milhouse", at);
        assert_eq!(manual.beat, None);
        assert_eq!(auto.beat.as_deref(), Some("capture"));

        let entry = |new: NewEntry| JournalEntry {
            id: EntryId("e1".into()),
            at: new.at,
            text: new.text,
            touched: None,
            beat: new.beat,
        };
        assert!(!entry(manual).is_auto());
        assert!(entry(auto).is_auto());
    }

    /// Normalization matches the mailbox body's, to a fixpoint — a store that
    /// rebuilds text line by line and one that keeps bytes must agree.
    #[test]
    fn an_entry_normalizes_its_line_endings_to_a_fixpoint() {
        assert_eq!(normalize_entry("  line one\r\nline two  "), "line one\nline two");
        assert_eq!(normalize_entry("line one\r\r\nline two"), "line one\nline two");
    }
}

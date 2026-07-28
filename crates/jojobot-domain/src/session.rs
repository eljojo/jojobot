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
//!   telling it. Both are closed — neither takes a write — but only `wrapped`
//!   is the last word. An `abandoned` run reopens, because it published
//!   nothing and because otherwise no interrupted run could ever be wrapped at
//!   all. See [`SessionState::is_final`].
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

/// A session's **handle** — the address a caller is given and carries back.
///
/// Distinct from [`SessionId`], which is the store's card id, and the two must
/// not be mixed: the card id is the store's key and the trace's anchor; the sid
/// is what an agent holds. This type is here rather than in the MCP layer
/// because a session's handle is part of what a session IS — the store persists
/// it on the card, so the vocabulary has to reach the port.
///
/// **What one looks like is a domain rule; DRAWING one is not.** The charset and
/// the length live here, next to the other validators; the entropy that produces
/// a fresh one lives in the layer that mints, because reading the OS entropy
/// source is I/O and this crate does none.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Sid(pub String);

impl Sid {
    /// Borrow the handle.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Sid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The handle's alphabet: **Crockford's base32, lowercased** — the digits and
/// the letters, minus `i`, `l`, `o` and `u`.
///
/// Those are the glyphs a reader mistakes for one another (`i`/`l`/`1`, `o`/`0`,
/// `u`/`v`), and a mistaken handle is one jojobot must refuse rather than
/// correct — correcting it means guessing which session somebody meant. Every
/// symbol is inside the `[a-z0-9-]` a [`SessionId`] accepts, so a handle is
/// always storable wherever an id is.
pub const SID_ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// How many characters a handle is.
///
/// **Four, not three.** Three would be 32³ = 32,768 — enough for one operator's
/// live sessions, but the space is what makes a handle hard to forge, and a
/// fourth character buys 32× of it for one keystroke. It also keeps the handle
/// space clear of the word a caller may answer the boot's offer with: three
/// characters could mint `new` itself.
pub const SID_LEN: usize = 4;

/// Whether this string has the shape of a handle jojobot mints.
///
/// **Shape only** — a readable handle may still address nothing, and the two are
/// told apart where they are answered, because "you mistyped it" and "that
/// session is gone" send a caller to different places.
pub fn is_readable_sid(sid: &str) -> bool {
    sid.len() == SID_LEN && sid.bytes().all(|b| SID_ALPHABET.contains(&b))
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
    /// **`wrapped` is final because the run TOLD ITS STORY.** It used to be
    /// because wrapping published that story into a shared Journal, and
    /// reopening the run would have made an already-published entry
    /// retroactively false. Nothing is published now — the journal is dark until
    /// events land — and the asymmetry survives it intact, because it never
    /// really rested on the audience: a run that said "here is what happened and
    /// I am done" has ended, and a run that merely stopped has not.
    ///
    /// `abandoned` told no story, which is the entire content of "it wasn't
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

/// A session about to begin — everything but the card id, which the store mints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    /// The bot this is one run of.
    pub bot: EntityId,
    /// **The handle, minted before the card and stored on it.** Required, not
    /// optional: a card born without one would be a session the registry could
    /// never rebuild an address for, and the whole point of persisting it is
    /// that a restart does not orphan handles.
    pub sid: Sid,
    /// What it is working on, at the moment it begins.
    pub focus: String,
    /// When it began.
    pub started_at: Timestamp,
}

/// One session on the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The store-minted id — the card, and the anchor a Journal entry's
    /// `[session …]` mark refers to.
    pub id: SessionId,
    /// The handle this run answers to, as stored on the card.
    ///
    /// **`None` only for a card written before handles were persisted.** Every
    /// session created from now on is born with one, which is what lets the
    /// registry be rebuilt from the board instead of dying with the process. A
    /// legacy card is handed a handle the first time a boot offers it, and that
    /// one is process-local — the migration is a no-op precisely because
    /// minting-on-offer already exists.
    #[serde(default)]
    pub sid: Option<Sid>,
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
    #[error("no session '{attempted}': no bot's sessions page carries a run with that id")]
    UnknownSession {
        /// The id that missed.
        attempted: String,
    },
    /// **The closed-session rule.** The session takes no more entries and cannot
    /// be closed again. Its own variant, because it is the answer a caller most
    /// needs told apart from "no such session": the id is real, the record is
    /// right there and readable, and the only thing that is over is writing to
    /// it.
    ///
    /// Also what [`Sessions::reopen`] returns for a `wrapped` session — the one
    /// end that is the last word. The message stays true of both states by
    /// saying only what closed means for writes; which end this is, and whether
    /// it can be picked back up, is `state`, and the layer talking to a caller
    /// is where that difference gets spelled out.
    #[error(
        "session '{attempted}' is {state} — closed, so it takes no entry, no amend and no focus \
         change. Its chronology stands as the record of what happened. A wrapped session is the \
         last word; an abandoned one can be picked back up by resuming it"
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

/// The Sessions port. One real adapter stands behind it in production (Outline,
/// a page per bot); a fake stands behind it in tests. The invariants every
/// adapter holds:
///
/// * **read-back** — a write succeeds only if reading it back through the read
///   path returns it, and a mismatch restores the prior state before erroring.
/// * **closed is closed, for writes** — nothing appends to, amends, or changes
///   the focus of a closed session, whichever end it reached, and no rollback
///   moves a card out of a terminal column.
/// * **only `wrapped` is the last word** — [`reopen`](Sessions::reopen) takes an
///   `abandoned` session back to `active` and is refused on a `wrapped` one. It
///   is the single walk-back on this port; see [`SessionState::is_final`].
/// * **nothing is created as a side effect** — [`begin`](Sessions::begin) is the
///   only thing that brings a session card into being, and the caller decides
///   when. A boot that never works leaves no card.
#[async_trait::async_trait]
pub trait Sessions: Send + Sync {
    /// Every session of one bot, whatever its state, newest start first. What
    /// attaching reads, and what the sweep walks.
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError>;

    /// **Every session on the board, whosever it is** — what the handle registry
    /// is rebuilt from at startup.
    ///
    /// Read once, eagerly, rather than lazily on a miss: a lazy rebuild would
    /// make the first caller after a restart get a different answer from the
    /// second, which is the kind of difference nobody can reproduce.
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError>;

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

/// **What a boot found on this bot's board**, after the sweep has run.
///
/// Named rather than a tuple because it grew a third thing the day an
/// `abandoned` run became something a boot could offer back, and a
/// `(Vec, Option, Vec)` at five call sites is a shape nobody can read.
#[derive(Debug, Default)]
pub struct Board {
    /// Every run still working — **all of them, not the newest.** A bot may
    /// have several at once (two devices, two pieces of work), so the offer
    /// needs them all.
    pub live: Vec<Session>,
    /// The one stopped run worth bringing up, if there is one.
    pub offerable: Option<Session>,
    /// The ids this sweep closed.
    pub swept: Vec<String>,
    /// The stale runs the store **refused to close**, with why.
    ///
    /// Reported rather than logged, because the domain does not log: it says
    /// what happened and the caller — which owns the log — decides what that is
    /// worth saying. Each of these is left `active` for the next boot to try
    /// again; a sweep that cannot close one session must not stop a boot.
    pub unswept: Vec<(SessionId, SessionError)>,
}

/// Sweep this bot's stale sessions and hand back what is on its board.
///
/// **One caller: the boot.** Binding is the caller's job — this reads and
/// writes the store, and returns what it found.
///
/// **`now` is an argument, not a reading.** The domain is clock-free: a date is
/// stamped at the edge (`capture`), and so is an instant. That keeps the whole
/// board decision — what to close, what is live, what to offer — a function of
/// its arguments, and decidable at a chosen instant with no handler in the way.
pub async fn sweep_and_find(
    sessions: &dyn Sessions,
    bot: &EntityId,
    now: Timestamp,
) -> Result<Board, SessionError> {
    let existing = sessions.sessions_of(bot).await?;

    let mut swept = Vec::new();
    let mut unswept = Vec::new();
    for stale in existing.iter().filter(|s| s.is_stale(now)) {
        match sessions.close(&stale.id, SessionState::Abandoned).await {
            Ok(_) => swept.push(stale.id.to_string()),
            Err(e) => unswept.push((stale.id.clone(), e)),
        }
    }

    // Newest first already, so the first live one is the newest.
    let live: Vec<Session> = existing
        .iter()
        .filter(|s| !s.state.is_terminal() && !s.is_stale(now))
        .cloned()
        .collect();
    // **Read AFTER the sweep, and through it.** The run this boot just marked
    // `abandoned` is the archetypal "resume last session" — it is the one that
    // stopped yesterday — so it has to be a candidate here, and the list read
    // above still says `active` for it.
    let offerable = existing
        .into_iter()
        .map(|s| match swept.contains(&s.id.to_string()) {
            true => Session {
                state: SessionState::Abandoned,
                ..s
            },
            false => s,
        })
        .find(|s| s.is_offerable(now));
    Ok(Board {
        live,
        offerable,
        swept,
        unswept,
    })
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemorySessions, contract};
    use super::*;

    /// The full behavioural contract holds for the fake — the same suite the
    /// Outline adapter runs against its API double, and against real Outline.
    #[tokio::test]
    async fn the_fake_satisfies_the_contract() {
        contract::run_all(InMemorySessions::new).await;
    }

    /// **The sweep reads a clock it is handed, so its answer is a function of
    /// its arguments.** This is what the descent bought: the boot's whole
    /// board decision — what to close, what is live, what to offer — is now
    /// decidable at a chosen instant, with no handler and no wall clock.
    /// A run of `bot:gamma` that last had something to show for itself
    /// `hours_ago` before [`contract::epoch`].
    async fn run(store: &dyn Sessions, nth: u8, focus: &str, hours_ago: i64) -> Session {
        store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid(format!("s{nth:03}")),
                focus: focus.to_string(),
                started_at: contract::epoch() - jiff::SignedDuration::from_hours(hours_ago),
            })
            .await
            .expect("begin should succeed")
    }

    #[tokio::test]
    async fn the_board_is_swept_and_read_at_the_instant_it_is_handed() {
        let store = InMemorySessions::new();
        let gamma = EntityId("bot:gamma".into());
        let at = contract::epoch();

        // Three runs of one bot, at three ages: a day and a half, an hour, and
        // a fortnight. Only the middle one is still working.
        let old = run(&store, 1, "the day before yesterday", 36).await;
        let warm = run(&store, 2, "an hour ago", 1).await;
        let ancient = run(&store, 3, "a fortnight ago", 24 * 15).await;

        let board = sweep_and_find(&store, &gamma, at).await.expect("a board");

        assert_eq!(
            board.swept,
            vec![old.id.to_string(), ancient.id.to_string()],
            "both runs past ABANDONED_AFTER are closed, newest first"
        );
        assert_eq!(
            board
                .live
                .iter()
                .map(|s| s.id.to_string())
                .collect::<Vec<_>>(),
            vec![warm.id.to_string()],
            "only the run that is still working stays live"
        );
        assert_eq!(
            board.offerable.as_ref().map(|s| s.id.to_string()),
            Some(old.id.to_string()),
            "the run this very sweep closed is the one worth offering back — the \
             fortnight-old one is past OFFER_ABANDONED_WITHIN"
        );
        assert_eq!(
            board.offerable.as_ref().map(|s| s.state),
            Some(SessionState::Abandoned),
            "…and it is offered as what the sweep just made it, not as the \
             `active` the pre-sweep read still says"
        );
        assert!(board.unswept.is_empty(), "nothing refused to close");

        // The clock is the argument, not the wall: ask the same store at an
        // earlier instant and nothing is stale yet.
        let earlier = sweep_and_find(&store, &gamma, at - ABANDONED_AFTER)
            .await
            .expect("a board");
        assert!(
            earlier.swept.is_empty(),
            "nothing is stale before it happens"
        );
    }

    /// A session the store refuses to close does not stop a boot: it is left
    /// active for the next one and reported, so the caller — which owns the log
    /// — can say so. The domain names what happened; it does not log.
    #[tokio::test]
    async fn a_session_that_will_not_close_is_reported_and_left_active() {
        struct RefusesToClose(InMemorySessions);

        #[async_trait::async_trait]
        impl Sessions for RefusesToClose {
            async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
                self.0.sessions_of(bot).await
            }
            async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
                self.0.all_sessions().await
            }
            async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
                self.0.read_session(id).await
            }
            async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
                self.0.begin(new).await
            }
            async fn append(
                &self,
                id: &SessionId,
                entry: NewEntry,
            ) -> Result<JournalEntry, SessionError> {
                self.0.append(id, entry).await
            }
            async fn amend_last(
                &self,
                id: &SessionId,
                text: &str,
            ) -> Result<JournalEntry, SessionError> {
                self.0.amend_last(id, text).await
            }
            async fn amend_beat(
                &self,
                id: &SessionId,
                entry: &EntryId,
                text: &str,
                touched: Timestamp,
            ) -> Result<JournalEntry, SessionError> {
                self.0.amend_beat(id, entry, text, touched).await
            }
            async fn set_focus(
                &self,
                id: &SessionId,
                focus: &str,
            ) -> Result<Session, SessionError> {
                self.0.set_focus(id, focus).await
            }
            async fn close(&self, _: &SessionId, _: SessionState) -> Result<Session, SessionError> {
                Err(SessionError::Store("the board said no".into()))
            }
            async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
                self.0.reopen(id).await
            }
        }

        let store = RefusesToClose(InMemorySessions::new());
        let gamma = EntityId("bot:gamma".into());
        let stale = run(&store.0, 1, "yesterday's work", 36).await;

        let board = sweep_and_find(&store, &gamma, contract::epoch())
            .await
            .expect("a refused close is not a failed boot");

        assert!(board.swept.is_empty(), "nothing was actually closed");
        assert_eq!(
            board
                .unswept
                .iter()
                .map(|(id, _)| id.to_string())
                .collect::<Vec<_>>(),
            vec![stale.id.to_string()],
            "and the boot is told which one, rather than the sweep going quiet"
        );
        assert_eq!(
            store.0.read_session(&stale.id).await.expect("read").state,
            SessionState::Active,
            "left active for the next boot to try again"
        );
    }

    #[test]
    fn the_three_states_round_trip_and_the_set_is_closed() {
        for state in SessionState::ALL {
            assert_eq!(SessionState::from_token(state.as_token()), Some(state));
        }
        assert_eq!(SessionState::ALL.len(), 3, "three columns, no more");
        for unknown in ["done", "Active", "", "open", "closed"] {
            assert_eq!(
                SessionState::from_token(unknown),
                None,
                "{unknown:?} is no state"
            );
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
            assert!(
                validate_focus(bad).is_err(),
                "must refuse the focus {bad:?}"
            );
        }
        assert!(validate_focus(&"x".repeat(200)).is_ok());
        assert!(
            validate_focus(&"x".repeat(201)).is_err(),
            "and it is capped"
        );
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
            sid: Some(Sid("s001".into())),
            bot: EntityId("bot:gamma".into()),
            focus: "nothing yet".into(),
            started_at: start,
            state: SessionState::Active,
            entries: Vec::new(),
        };
        assert_eq!(
            bare.last_beat(),
            start,
            "no entries: the start is the last beat"
        );
        assert!(!bare.is_stale(start + jiff::SignedDuration::from_hours(23)));
        assert!(
            bare.is_stale(start + ABANDONED_AFTER),
            "the threshold is inclusive"
        );

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
        assert_eq!(
            busy.last_beat(),
            start + hour,
            "an entry moves the clock forward"
        );
        assert!(
            !busy.is_stale(start + ABANDONED_AFTER),
            "…so the same instant that swept the bare one leaves this one alone"
        );

        // A closed session is never swept: it is already at an end.
        let closed = Session {
            state: SessionState::Wrapped,
            ..bare
        };
        assert!(!closed.is_stale(start + ABANDONED_AFTER + hour));
    }

    /// **The glyphs a reader confuses are not in the alphabet**, and everything
    /// in it is something the card's id type accepts — a handle jojobot cannot
    /// store is a handle jojobot cannot hand out.
    #[test]
    fn the_handle_alphabet_excludes_the_confusable_glyphs() {
        assert_eq!(
            SID_ALPHABET.len(),
            32,
            "Crockford's base32, minus nothing else"
        );
        for confusable in [b'i', b'l', b'o', b'u'] {
            assert!(
                !SID_ALPHABET.contains(&confusable),
                "{} reads as another glyph and must not be mintable",
                confusable as char
            );
        }
        let whole = String::from_utf8(SID_ALPHABET.to_vec()).expect("ascii");
        assert!(
            validate_session_id(&SessionId(whole.clone())).is_ok(),
            "every symbol must be a legal session id byte: {whole}"
        );
    }

    /// Shape is checked, and a near-miss is refused rather than repaired:
    /// correcting `1` to `l` is guessing which session somebody meant.
    #[test]
    fn an_unreadable_handle_is_refused_rather_than_corrected() {
        assert!(is_readable_sid("k3f9"));
        for bad in [
            "", "k3f", "k3f9a", "k3fo", "k3fi", "k3fl", "k3fu", "K3F9", "k3f-", "k3f ",
        ] {
            assert!(!is_readable_sid(bad), "{bad:?} must not read as a handle");
        }
    }

    /// **What a boot volunteers is bounded; what a handle reaches is not.** An
    /// abandoned run inside the window is offered back, an older one is not, and
    /// a wrapped one never is — its story was told.
    #[test]
    fn only_a_recently_abandoned_run_is_offered_back() {
        let start = Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        let run = Session {
            id: SessionId("1".into()),
            sid: Some(Sid("s001".into())),
            bot: EntityId("bot:gamma".into()),
            focus: "reading the hand-off".into(),
            started_at: start,
            state: SessionState::Abandoned,
            entries: Vec::new(),
        };

        let day = jiff::SignedDuration::from_hours(24);
        assert!(
            run.is_offerable(start + day),
            "yesterday's run is the one to offer"
        );
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
            let other = Session {
                state,
                ..run.clone()
            };
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
        assert_eq!(
            normalize_entry("  line one\r\nline two  "),
            "line one\nline two"
        );
        assert_eq!(
            normalize_entry("line one\r\r\nline two"),
            "line one\nline two"
        );
    }
}

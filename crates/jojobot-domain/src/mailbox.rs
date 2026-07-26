//! Mailboxes — leave a message in any box; see what's new, what's seen, what's
//! handled.
//!
//! A **bounded context beside Memory**, not an extension of it: nothing here is
//! an entity or a fact, and no type crosses between the two. A mailbox is a
//! named box; a message is one card in it; and **the column a message sits in IS
//! its state** — `new → read → processed`, with `processed` terminal (archive
//! semantics; nothing is ever deleted).
//!
//! Two invariants shape the whole context:
//!
//! * **a typo must never mint a box.** `post_message` into an unknown mailbox
//!   comes back [`Guarded::Blocked`] with candidates; only `create_mailbox`
//!   brings one into existence — the same detection-without-inference the Memory
//!   write guard runs, in this context's vocabulary.
//! * **delivery is not consumption.** `read_mailbox` hands over everything
//!   unprocessed and moves `new → read`; a consumer marks a message processed
//!   only *after* acting on it, so a crash leaves the message visible as
//!   already-seen rather than silently dropped.
//!
//! **This is user-agnostic software: no user PII, fixtures included.** Mailbox
//! names and senders in tests and examples are openly fictional or generic.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

pub mod guard;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// The name of a mailbox — the box a message is left in.
///
/// The grammar is the Memory handle's slug charset, `[a-z0-9-]+`, for the same
/// three reasons: a name has exactly one spelling (so two sessions can't create
/// `Inbox` and `inbox`), it cannot forge markup in a card, and it is what makes
/// near-miss screening meaningful — an edit distance between free-form titles
/// with mixed case and punctuation measures typography, not intent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MailboxName(pub String);

impl MailboxName {
    /// Borrow the underlying name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MailboxName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A message's id — minted by the store, opaque to the domain. Digits in the
/// Vikunja adapter; validated as a narrow token here so an id arriving from a
/// client can never carry a path segment, a quote, or a newline into a URL or a
/// card.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    /// Borrow the underlying id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a message sits in its box. **The column is the state** — there is no
/// separate status field to disagree with it, exactly as a fact's home doc, not
/// a subject cell, decides whose page it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageState {
    /// Left, never delivered.
    New,
    /// Delivered to a consumer, not yet acted on.
    Read,
    /// Acted on. Terminal — archive, never deletion.
    Processed,
}

impl MessageState {
    /// Every state, in funnel order — which is also the column order on the
    /// board. Provisioning walks this, so the board can never grow a column the
    /// domain doesn't know or lose one it does.
    pub const ALL: [MessageState; 3] = [
        MessageState::New,
        MessageState::Read,
        MessageState::Processed,
    ];

    /// The wire token — and the column's title on the board.
    pub fn as_token(self) -> &'static str {
        match self {
            MessageState::New => "new",
            MessageState::Read => "read",
            MessageState::Processed => "processed",
        }
    }

    /// Parse a state token. Strict: a column title jojobot doesn't recognize is
    /// not a state, and guessing one would file a message in a lifecycle stage
    /// nobody chose.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_token() == token.trim())
    }

    /// Whether this state means "still owed work" — the set `read_mailbox`
    /// delivers. `processed` is terminal, so it is the one state that isn't.
    pub fn is_unprocessed(self) -> bool {
        !matches!(self, MessageState::Processed)
    }
}

impl std::fmt::Display for MessageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_token())
    }
}

/// The slug charset: `[a-z0-9-]`. Deliberately narrow — no newline (forge a line
/// in a machine block), no backtick (close a fence), no space, no uppercase (so
/// a name has exactly one spelling).
fn is_slug_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
}

/// Validate a mailbox name before it is written or looked up anywhere.
pub fn validate_mailbox_name(name: &MailboxName) -> Result<(), MailboxError> {
    let n = name.as_str();
    let ok = !n.is_empty()
        && n.len() <= 64
        && n.bytes().all(is_slug_byte)
        && n.starts_with(|c: char| c.is_ascii_alphanumeric())
        && n.ends_with(|c: char| c.is_ascii_alphanumeric());
    if ok {
        Ok(())
    } else {
        Err(MailboxError::InvalidName(n.to_string()))
    }
}

/// Validate a message id arriving from a client. Same narrow token as a mailbox
/// name: an id is used to select a card to rewrite, so it never carries free
/// text.
pub fn validate_message_id(id: &MessageId) -> Result<(), MailboxError> {
    let i = id.as_str();
    let ok = !i.is_empty() && i.len() <= 64 && i.bytes().all(is_slug_byte);
    if ok {
        Ok(())
    } else {
        Err(MailboxError::InvalidMessageId(i.to_string()))
    }
}

/// A line break, either byte — refused as hard in a machine-block field as the
/// Memory codec refuses it in a table cell, and for the same reason: a field is
/// one line inside a fenced block, so a newline forges a second field.
fn breaks_the_line(value: &str) -> bool {
    value.contains('\n') || value.contains('\r')
}

/// Validate a caller-declared sender. **Provenance is required on every
/// message** — a message with no attributable origin is one nobody can reply to
/// or hold to account — so it is non-empty, and one plain line because it rides
/// in the machine block *and* in the card's title.
pub fn validate_sender(sender: &str) -> Result<(), MailboxError> {
    let s = sender.trim();
    if s.is_empty() {
        return Err(MailboxError::InvalidMessage("sender is empty".into()));
    }
    if s.chars().count() > 120 {
        return Err(MailboxError::InvalidMessage("sender is too long".into()));
    }
    if breaks_the_line(s) || s.contains('`') || s.chars().any(char::is_control) {
        return Err(MailboxError::InvalidMessage(
            "sender must be one plain line (no newline, no backtick)".into(),
        ));
    }
    Ok(())
}

/// Validate a message body. Multi-line is the ordinary case here — a message is
/// prose, not a table cell — so only emptiness is refused.
pub fn validate_body(body: &str) -> Result<(), MailboxError> {
    if body.trim().is_empty() {
        return Err(MailboxError::InvalidMessage("body is empty".into()));
    }
    Ok(())
}

/// Validate the outcome notes a consumer records when it marks a message
/// processed. One plain line: notes ride in the machine block beside `sender`.
pub fn validate_notes(notes: Option<&str>) -> Result<(), MailboxError> {
    let Some(notes) = notes else { return Ok(()) };
    if notes.trim().is_empty() {
        return Ok(());
    }
    if breaks_the_line(notes) || notes.contains('`') || notes.chars().any(char::is_control) {
        return Err(MailboxError::InvalidMessage(
            "notes must be one plain line (no newline, no backtick)".into(),
        ));
    }
    if notes.chars().count() > 500 {
        return Err(MailboxError::InvalidMessage("notes are too long".into()));
    }
    Ok(())
}

/// How much of the body rides in the card's title, in characters.
const TITLE_BODY_BUDGET: usize = 60;

/// The human-visible half of a message card: `"<sender>: <first words of body>"`.
///
/// Truncation is on a **word** boundary with an ellipsis, so a title never ends
/// mid-word and never implies the message says less than it does. Newlines in
/// the body collapse to spaces — a title is one line.
pub fn message_title(sender: &str, body: &str) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let head = if flat.chars().count() <= TITLE_BODY_BUDGET {
        flat
    } else {
        let mut kept = String::new();
        for word in flat.split(' ') {
            if kept.chars().count() + word.chars().count() + 1 > TITLE_BODY_BUDGET {
                break;
            }
            if !kept.is_empty() {
                kept.push(' ');
            }
            kept.push_str(word);
        }
        // A single word longer than the whole budget has no boundary to cut on.
        if kept.is_empty() {
            kept = flat.chars().take(TITLE_BODY_BUDGET).collect();
        }
        format!("{kept}…")
    };
    format!("{}: {head}", sender.trim())
}

/// Normalize a body to the form that survives a round-trip through the store:
/// edge whitespace is not significant, and no store preserves it; CRLF line
/// endings become plain `\n`, because a store that reconstructs text
/// line-by-line strips the `\r`s — normalizing here is what keeps one contract
/// from getting two answers.
///
/// Folded to a **fixpoint**: a single non-overlapping replace turns `\r\r\n`
/// into exactly the `\r\n` it was meant to remove. A lone `\r` with no `\n`
/// after it is left alone — it is body text, and line-based stores keep it.
pub fn normalize_body(body: &str) -> String {
    let mut body = body.to_string();
    while body.contains("\r\n") {
        body = body.replace("\r\n", "\n");
    }
    body.trim().to_string()
}

/// Normalize optional notes — blank notes are no notes, not empty ones.
pub fn normalize_notes(notes: Option<&str>) -> Option<String> {
    notes
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
}

/// A mailbox and what is in it. The counts are the whole point of
/// `list_mailboxes`: what's new, what's seen, what's handled, per box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mailbox {
    /// The box's name.
    pub name: MailboxName,
    /// How many messages sit in each state.
    pub counts: StateCounts,
    /// Cards wearing this box's label that could not be read as messages — a
    /// description hand-edited past parsing, or a card sitting in a column that
    /// is no state. Such a card is invisible to every other verb (not counted,
    /// not delivered, not processable), so this is where its existence is
    /// surfaced: "N unreadable" instead of nothing.
    #[serde(default)]
    pub quarantined: Vec<MessageId>,
}

/// Per-state message counts for one mailbox.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateCounts {
    /// Left, never delivered.
    pub new: usize,
    /// Delivered, not yet acted on.
    pub read: usize,
    /// Acted on — terminal.
    pub processed: usize,
}

impl StateCounts {
    /// Count one message into its state's tally.
    pub fn add(&mut self, state: MessageState) {
        match state {
            MessageState::New => self.new += 1,
            MessageState::Read => self.read += 1,
            MessageState::Processed => self.processed += 1,
        }
    }

    /// Every message in the box, whatever its state.
    pub fn total(&self) -> usize {
        self.new + self.read + self.processed
    }
}

/// A message about to be posted — everything but the id, which the store mints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    /// The box to leave it in. **It must already exist**; this verb never
    /// creates one.
    pub mailbox: MailboxName,
    /// The message itself.
    pub body: String,
    /// Who is sending — caller-declared. Persona resolution is a later
    /// milestone; jojobot records what the caller claims, and says so.
    pub sender: String,
    /// When it was sent. Passed in rather than read off a clock, so the domain
    /// stays deterministic.
    pub sent_at: Timestamp,
}

/// A message on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// The store-minted id — what `mark_processed` targets.
    pub id: MessageId,
    /// The box it is in.
    pub mailbox: MailboxName,
    /// The message body.
    pub body: String,
    /// Who sent it, as the poster declared.
    pub sender: String,
    /// When it was sent.
    pub sent_at: Timestamp,
    /// Which column it sits in.
    pub state: MessageState,
    /// The outcome a consumer recorded when it marked this processed —
    /// including a failure. **A failure is data plus a reply message, never a
    /// state**: there is no `failed` column, because a message whose handling
    /// failed has still been handled, and the retry is a new message.
    pub notes: Option<String>,
}

/// One message as `read_mailbox` delivers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivered {
    /// The message, in the state it is in **after** delivery (always `read`).
    pub message: Message,
    /// Whether a previous `read_mailbox` had already handed this over. True
    /// means a consumer took it and never marked it processed — a crash, a
    /// dropped batch, a job that died mid-flight. Delivering it again silently
    /// would make that invisible, which is how leftovers turn into ghosts.
    pub seen_before: bool,
}

/// What `read_mailbox` hands back: every unprocessed message in the box, in the
/// order it was posted, with the leftovers flagged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    /// The box that was read.
    pub mailbox: MailboxName,
    /// Everything unprocessed, oldest first.
    pub messages: Vec<Delivered>,
}

impl Delivery {
    /// The messages a previous read had already delivered — the crashed
    /// consumer's leftovers.
    pub fn leftovers(&self) -> impl Iterator<Item = &Delivered> {
        self.messages.iter().filter(|d| d.seen_before)
    }
}

/// The result of a write that names a mailbox: it either happened, or the guard
/// stopped it and is asking. Modelled as a value rather than an error for the
/// same reason Memory's is — a blocked write is a decision the caller owes, not
/// a failure to log and move past.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guarded<T> {
    /// No suspicion, or the caller had already resolved it: this is the record.
    Written(T),
    /// **Nothing was written.** A creation takes a different name; a write that
    /// only *names* a box takes an existing name or a `create_mailbox` first,
    /// because it cannot create one. `candidates` may be empty — an
    /// unrecognized name is blocked whether or not anything resembles it.
    Blocked {
        /// The name the caller tried to write.
        attempted: MailboxName,
        /// What the guard found, strongest first.
        candidates: Vec<guard::MailboxMatch>,
    },
}

impl<T> Guarded<T> {
    /// The written record, or `None` if the guard blocked the write.
    pub fn written(self) -> Option<T> {
        match self {
            Guarded::Written(v) => Some(v),
            Guarded::Blocked { .. } => None,
        }
    }
}

/// Why a mailbox operation failed. Adapters map their transport/parse errors
/// into these; the domain and the MCP layer speak only this vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    /// The mailbox name is not well-formed.
    #[error("invalid mailbox name '{0}': names are [a-z0-9-]+, starting and ending alphanumeric")]
    InvalidName(String),
    /// The message id is not a well-formed token.
    #[error("invalid message id '{0}': ids are [a-z0-9-]+")]
    InvalidMessageId(String),
    /// The message is malformed for storage.
    #[error("invalid message: {0}")]
    InvalidMessage(String),
    /// The addressed message doesn't exist in any mailbox jojobot manages.
    /// Never created, never guessed at.
    #[error("no message '{attempted}' in any mailbox jojobot manages")]
    UnknownMessage {
        /// The id that missed.
        attempted: String,
    },
    /// The addressed id is a **quarantined card**: it is on a jojobot mailbox
    /// board — `list_mailboxes` publishes this very id — but it cannot be read
    /// as a message, so no verb may act on it.
    ///
    /// Distinct from [`MailboxError::UnknownMessage`] on purpose. Answering
    /// "no such message" here is a false statement about an id jojobot itself
    /// handed out, and it sends the caller looking for a lost message instead
    /// of at the card that is sitting right there.
    #[error(
        "message '{attempted}' is a quarantined card: {reason}. jojobot will not act on a card it \
         cannot read — a person needs to open card {attempted} on the mailbox board and either \
         restore what was edited out of it or move it back into one of the funnel's columns"
    )]
    Quarantined {
        /// The id that was addressed — the same one `list_mailboxes` published.
        attempted: String,
        /// Why the card cannot be read.
        reason: String,
    },
    /// **A write failed, and putting the card back failed too.** The card is
    /// left mid-verb: not written, not restored, and not something the caller
    /// can retry its way out of.
    ///
    /// Its own variant on purpose. Whether a rollback worked is the one thing a
    /// caller cannot infer from anything else in the answer, and the last time
    /// it was carried as a sentence inside a general store error, detecting it
    /// meant string-matching that sentence — so rewording it silently broke the
    /// detection with every test green.
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
    /// **The write-scope invariant.** A call path reached for a project that is
    /// not the discovered mailbox project. Refused before any request leaves the
    /// process: the operator's own boards live on the same Vikunja, and a
    /// mis-scoped write there is not something a read-back can undo.
    #[error("refusing to touch a project other than jojobot's mailbox project: {0}")]
    ForeignProject(String),
    /// The underlying store (Vikunja, or its network/parse layer) failed.
    #[error("store error: {0}")]
    Store(String),
    /// The store isn't configured (no credentials).
    #[error("mailbox store not configured: {0}")]
    NotConfigured(String),
}

/// The Mailboxes port — five verbs over boxes and the messages in them. One real
/// adapter stands behind it in production (Vikunja); a fake stands behind it in
/// tests. Three invariants bind every adapter:
///
/// * **read-back** — a write succeeds only if reading it back through the read
///   path returns it. A read-back mismatch restores the prior state before
///   erroring, so a retry can trust what it finds.
/// * **the guard is on the write path** — a write that names a mailbox screens
///   against the live list first, so it cannot be skipped by a caller who forgot.
/// * **never create on a miss** — an unknown mailbox comes back blocked with the
///   nearest candidates. Only [`create_mailbox`](Mailboxes::create_mailbox)
///   brings a box into existence, because a typo that mints a box is a message
///   posted where nobody is listening.
#[async_trait::async_trait]
pub trait Mailboxes: Send + Sync {
    /// Create a mailbox. Screened against the existing names, so one that looks
    /// like a box already there comes back [`Guarded::Blocked`] with candidates.
    /// `create_new` is the caller's explicit "I know, it's a sibling" signal:
    /// it overrides the near/containment screen (so `worker-2` is creatable
    /// beside `worker-1`), and never an exact name — that box already exists.
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        create_new: bool,
    ) -> Result<Guarded<Mailbox>, MailboxError>;

    /// Every mailbox jojobot manages, with per-state counts.
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError>;

    /// Leave a message in a box. It lands in `new`. **The mailbox must already
    /// exist** — an unknown name comes back [`Guarded::Blocked`], never a new box.
    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError>;

    /// Deliver everything unprocessed in a box — messages in `new` and messages
    /// a previous read already handed over — and move `new → read`. There is no
    /// peek/take split: reading IS taking delivery, and the state moves with it.
    ///
    /// **A message that reaches `processed` while the delivery is in flight is
    /// dropped from it.** Somebody handled it; handing it over anyway would put
    /// an already-processed message into a consumer's batch flagged as fresh
    /// mail, which is the double-processing this context exists to prevent.
    async fn read_mailbox(&self, name: &MailboxName) -> Result<Guarded<Delivery>, MailboxError>;

    /// Move a message to `processed`, optionally recording the outcome.
    /// Terminal: nothing is deleted, and nothing moves out of `processed`.
    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError>;
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemoryMailboxes, contract};
    use super::*;

    /// The full behavioural contract holds for the fake — the same suite the
    /// Vikunja adapter runs against its API double, and against real Vikunja.
    #[tokio::test]
    async fn the_fake_satisfies_the_contract() {
        contract::run_all(InMemoryMailboxes::new).await;
    }

    #[test]
    fn a_mailbox_name_has_exactly_one_spelling() {
        for good in ["inbox", "errands", "box-2", "a"] {
            assert!(
                validate_mailbox_name(&MailboxName(good.into())).is_ok(),
                "must accept {good:?}"
            );
        }
        for bad in [
            "",           // empty
            "Inbox",      // uppercase would give one box two spellings
            "in box",     // space
            "in_box",     // underscore is out of the charset
            "-inbox",     // leading separator
            "inbox-",     // trailing separator
            "in`box",     // could close a fence in a card
            "in\nbox",    // could forge a machine-block field
        ] {
            assert!(
                validate_mailbox_name(&MailboxName(bad.into())).is_err(),
                "must reject {bad:?}"
            );
        }
    }

    #[test]
    fn the_three_states_round_trip_and_the_set_is_closed() {
        for state in MessageState::ALL {
            assert_eq!(MessageState::from_token(state.as_token()), Some(state));
        }
        assert_eq!(MessageState::ALL.len(), 3, "three columns, no more");
        for unknown in ["done", "New", "", "backlog", "unread"] {
            assert_eq!(
                MessageState::from_token(unknown),
                None,
                "{unknown:?} is not a state"
            );
        }
        // The funnel order is the column order — provisioning walks it.
        assert_eq!(
            MessageState::ALL.map(|s| s.as_token()),
            ["new", "read", "processed"]
        );
    }

    /// `processed` is the one terminal state, so it is the one a read does not
    /// deliver. Everything else is still owed work.
    #[test]
    fn only_processed_is_out_of_the_delivery_set() {
        assert!(MessageState::New.is_unprocessed());
        assert!(MessageState::Read.is_unprocessed());
        assert!(!MessageState::Processed.is_unprocessed());
    }

    /// The title is the human-visible half of the card: who it is from, then the
    /// opening of what they said — cut on a word boundary, never mid-word, and
    /// always one line.
    #[test]
    fn a_title_is_the_sender_and_the_opening_of_the_body() {
        assert_eq!(
            message_title("alpha", "the shipment landed"),
            "alpha: the shipment landed"
        );
        assert_eq!(
            message_title("  alpha  ", "  the shipment\n  landed  "),
            "alpha: the shipment landed",
            "a title is one line, whatever the body's shape"
        );

        let long = "the shipment landed this morning and the crates are stacked by the north door";
        let title = message_title("alpha", long);
        assert!(title.starts_with("alpha: the shipment landed this morning"));
        assert!(title.ends_with('…'), "a cut title says it was cut: {title:?}");
        assert!(
            !title.trim_end_matches('…').ends_with(' '),
            "the cut lands on a word, not on the space after it: {title:?}"
        );
        assert!(
            long.starts_with(
                title
                    .trim_start_matches("alpha: ")
                    .trim_end_matches('…')
            ),
            "the kept part is a prefix of the body, never a mangled word: {title:?}"
        );
    }

    /// A single word longer than the whole budget has no boundary to cut on —
    /// it is still cut, because a title is a title.
    #[test]
    fn a_title_cuts_an_unbroken_word_rather_than_running_forever() {
        let title = message_title("alpha", &"x".repeat(200));
        assert!(title.chars().count() < 100, "got {}", title.chars().count());
        assert!(title.ends_with('…'));
    }

    /// Every field that rides in the machine block is one plain line, for the
    /// reason the Memory codec refuses a bare CR in a table cell: a newline
    /// forges a field, and a backtick closes the fence around it. A **body** is
    /// the exception — a message is prose, and prose has paragraphs.
    #[test]
    fn machine_block_fields_are_one_line_but_a_body_is_prose() {
        assert!(validate_sender("alpha").is_ok());
        for bad in ["", "   ", "two\nlines", "carriage\rreturn", "back`tick"] {
            assert!(validate_sender(bad).is_err(), "must refuse the sender {bad:?}");
        }

        assert!(validate_body("a message\n\nwith paragraphs").is_ok());
        assert!(validate_body("   ").is_err(), "an empty body is not a message");

        assert!(validate_notes(None).is_ok());
        assert!(validate_notes(Some("drained into the journal")).is_ok());
        assert!(validate_notes(Some("")).is_ok(), "blank notes are no notes");
        assert!(validate_notes(Some("two\nlines")).is_err());
    }

    #[test]
    fn a_message_id_is_a_narrow_token() {
        assert!(validate_message_id(&MessageId("4212".into())).is_ok());
        for bad in ["", "../42", "4 2", "42;drop", "4\n2"] {
            assert!(
                validate_message_id(&MessageId(bad.into())).is_err(),
                "must reject {bad:?}"
            );
        }
    }

    #[test]
    fn counts_tally_per_state() {
        let mut counts = StateCounts::default();
        counts.add(MessageState::New);
        counts.add(MessageState::New);
        counts.add(MessageState::Read);
        counts.add(MessageState::Processed);
        assert_eq!(counts.new, 2);
        assert_eq!(counts.read, 1);
        assert_eq!(counts.processed, 1);
        assert_eq!(counts.total(), 4);
    }
}

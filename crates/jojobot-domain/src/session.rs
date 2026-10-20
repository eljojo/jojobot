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
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::memory::EntityId;

pub mod history;
pub mod mention;
pub mod role_keys;

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

/// **Which bot a run belongs to**, from the run's public name. A store that
/// reads who first wrote a claim asks it, so a claim another bot wrote can be told
/// from the bot's own ([`crate::memory::Fact::written_by_another`]). `None` is a
/// name no run answers to.
pub trait RunBots: Send + Sync {
    /// The bot the run named `run` is a run of.
    fn bot_of_run(&self, run: &str) -> Option<EntityId>;
}

/// The handle's alphabet — [`crate::handle::ALPHABET`], which every drawn id on
/// every rail comes from. Named here as well because a handle's shape is part
/// of what a session answers to, and a second spelling of the same bytes is how
/// the two drift.
pub use crate::handle::ALPHABET as SID_ALPHABET;

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
    crate::handle::is_drawn(sid, SID_LEN) || is_run_name(sid)
}

/// **How many characters a run's public name is.** The name is minted at the
/// boot and is also the id of the run's record once the run writes, so it is
/// the same length as every other drawn record id.
pub const RUN_LEN: usize = 6;

/// Whether this string has the shape of a run name minted at a boot. A run
/// that began before names were minted keeps its four-character handle as its
/// name, and [`is_readable_sid`] accepts both.
pub fn is_run_name(name: &str) -> bool {
    crate::handle::is_drawn(name, RUN_LEN)
}

/// **Every run key starts with this**, so a caller's value that is a key is
/// told from a name without looking anything up, in the way a wrap code is
/// told by its own prefix.
pub const KEY_PREFIX: &str = "sk-";

/// How many characters follow the prefix on a drawn key: 26 characters of a
/// 32-letter alphabet, 130 bits.
const KEY_DRAWN: usize = 26;

/// **Draw a run key**: the prefix and 26 characters of OS entropy. The key is
/// the run's secret and its only credential; it is never a name.
pub fn mint_key() -> String {
    format!("{KEY_PREFIX}{}", crate::handle::draw(KEY_DRAWN))
}

/// Whether this string has the shape of a run key. Shape only: a well-shaped
/// key may still open nothing.
pub fn is_key(value: &str) -> bool {
    value
        .strip_prefix(KEY_PREFIX)
        .is_some_and(|rest| crate::handle::is_drawn(rest, KEY_DRAWN))
}

/// **The hash a run's key is stored and looked up by**: SHA-256 of the key,
/// written as lowercase hex. No process key goes into it, because a key has to
/// keep working across a restart.
pub fn hash_key(key: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(key.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// **What a run's stored key hash says when its key was taken away.** A hash
/// is 64 hex characters, so this word cannot be one. A run that never held a
/// key (a run that began before keys existed) stores no hash at all, which is
/// how it is told from a run whose key was voided.
pub const KEY_VOID: &str = "void";

/// **Why a key stopped opening its run.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetiredBy {
    /// Another session resumed the run and was handed a new key.
    Resumed,
    /// The run wrapped, which took its key away.
    Wrapped,
}

impl RetiredBy {
    /// The word the store keeps for this reason.
    pub fn as_token(self) -> &'static str {
        match self {
            RetiredBy::Resumed => "resumed",
            RetiredBy::Wrapped => "wrapped",
        }
    }

    /// The reason a stored word names, or nothing for a word this build did not
    /// write.
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "resumed" => Some(RetiredBy::Resumed),
            "wrapped" => Some(RetiredBy::Wrapped),
            _ => None,
        }
    }
}

/// **The key a run held before its newest key**, kept so the sitting that still
/// sends it is told why it stopped opening the run. One per run: the newest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetiredKey {
    /// The hash of the key that stopped opening the run.
    pub hash: String,
    /// When it stopped, on the server's clock.
    pub at: Timestamp,
    /// Why it stopped.
    pub by: RetiredBy,
}

/// **What a run's key history says that its newest hash cannot**, written in
/// the same statement as the hash so a restart finds the two together.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyTrail {
    /// The newest key that stopped opening this run, if one did.
    #[serde(default)]
    pub retired: Option<RetiredKey>,
    /// When another sitting last took this run over, by a resume, on the
    /// server's clock. A pick-up writes
    /// no chronology entry, so without this mark the run reads as quiet.
    #[serde(default)]
    pub taken_at: Option<Timestamp>,
    /// **The run that took this run's role over**, when that is why the run was
    /// closed. Written in the act that closes the run, and cleared by any later
    /// key write, which a resume makes. A run closed by the sweep has none.
    #[serde(default)]
    pub displaced_by: Option<Sid>,
}

/// **A run's key as the boot handed it out, for a run with no card yet.** The
/// run's card holds all of this once the run writes; until then this is the
/// only record that the key was ever given, so a restart can bring it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MintedRun {
    /// The run's public name.
    pub name: Sid,
    /// The hash of its key. Never the key.
    pub key_hash: String,
    /// The bot it is one run of.
    pub bot: EntityId,
    /// The zone it stated at the boot, if it stated one.
    pub zone: Option<String>,
    /// The day it stated at the boot, if it stated one.
    pub day: Option<Date>,
    /// When the boot minted it, on the server's clock.
    pub minted_at: Timestamp,
}

/// **How long a note of a minted run is kept when the run never writes.** Seven
/// days is the window in which a run that stopped is still offered back at a
/// boot; a run that never wrote has nothing to offer past it, and a caller that
/// comes back later boots again.
pub const MINTED_NOTE_KEPT: jiff::SignedDuration = jiff::SignedDuration::from_hours(7 * 24);

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
    /// `wrapped` is final because the run TOLD ITS STORY — not because
    /// wrapping publishes anything (it does not). The asymmetry never really
    /// rested on an audience: a run that said "here is what happened and I am
    /// done" has ended; a run that merely stopped has not.
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

/// **The window a wrap leaves open for one last change.** A wrap hands back a
/// code; the run it closed stays `wrapped` the whole time, and the code is what
/// lets that run take writes once more. Stored as one text on the session's own
/// record, and absent on every run that never wrapped or whose window ended.
///
/// **Three things end it**: the second wrap, a newer run of the same bot, and the
/// clock. It lasts [`WRAP_WINDOW_HOURS`] from the wrap's own moment
/// ([`Session::wrapped_at`]), however many entries the run writes inside it, so
/// no record of it is stored beyond the entries the run already holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WrapWindow {
    /// The wrap handed the code back and nobody has used it yet. The run is
    /// closed to writes.
    Offered(String),
    /// The code was used. The run takes writes, and is still `wrapped`.
    Open(String),
}

/// **How many hours after its wrap a run takes writes on a wrap code.** One
/// number, read where the window is judged ([`Session::window_lapsed_at`]).
pub const WRAP_WINDOW_HOURS: i64 = 2;

/// **Every wrap code starts with this**, so a caller's `resume` that is a code
/// is told from a handle without looking anything up, and a spent code is
/// refused in its own words rather than as a handle nobody holds.
pub const WRAP_CODE_PREFIX: &str = "wc-";

/// **A stored window keeps this prefix and then the hash of its code**, never the
/// code. The code is handed back once; a read of the store must not produce one
/// that reopens a run, as a read of the store produces no key.
pub const WRAP_HASH_PREFIX: &str = "wh-";

/// **What a window stores in place of the code it was handed out for**: the
/// prefix and the same hash a key is stored by.
pub fn hash_wrap_code(code: &str) -> String {
    format!("{WRAP_HASH_PREFIX}{}", hash_key(code))
}

/// How many characters follow the prefix on a drawn wrap code.
const WRAP_CODE_DRAWN: usize = 12;

/// **Draw a wrap code**: the prefix and twelve characters of OS entropy. It is
/// server-minted, opaque, and never the sid of the run it reopens.
pub fn mint_wrap_code() -> String {
    format!("{WRAP_CODE_PREFIX}{}", crate::handle::draw(WRAP_CODE_DRAWN))
}

/// Whether a `resume` answer is a wrap code rather than a handle.
pub fn is_wrap_code(answer: &str) -> bool {
    answer.starts_with(WRAP_CODE_PREFIX)
}

impl WrapWindow {
    /// The code this window answers to.
    pub fn code(&self) -> &str {
        match self {
            WrapWindow::Offered(code) | WrapWindow::Open(code) => code,
        }
    }

    /// **Whether this window answers to `code`.** A window stores the hash of
    /// its code. One an older build stored holds the code itself, and answers to
    /// it the same way until it is used and written back hashed. A hash is never
    /// accepted as the code: only a value shaped like a code is compared plain.
    pub fn answers_to(&self, code: &str) -> bool {
        let stored = self.code();
        stored == hash_wrap_code(code)
            || (is_wrap_code(code) && stored.starts_with(WRAP_CODE_PREFIX) && stored == code)
    }

    /// Whether the run takes writes.
    pub fn is_open(&self) -> bool {
        matches!(self, WrapWindow::Open(_))
    }

    /// The text this is stored as: the state, a space, the code.
    pub fn to_stored(&self) -> String {
        match self {
            WrapWindow::Offered(code) => format!("offered {code}"),
            WrapWindow::Open(code) => format!("open {code}"),
        }
    }

    /// Read a stored window back. `None` for text this did not write: a value
    /// that does not parse is no window, never a guess at one.
    pub fn from_stored(stored: &str) -> Option<WrapWindow> {
        let (state, code) = stored.split_once(' ')?;
        if !is_wrap_code(code) && !code.starts_with(WRAP_HASH_PREFIX) {
            return None;
        }
        match state {
            "offered" => Some(WrapWindow::Offered(code.to_string())),
            "open" => Some(WrapWindow::Open(code.to_string())),
            _ => None,
        }
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

/// **The last day a run is still working**, from the newest day it states.
///
/// The judgement is [`ABANDONED_AFTER`]'s and it is read off it rather than
/// written again: a session that goes quiet overnight is still that night's
/// work, so a run whose newest stated day is yesterday is live and one older is
/// not. Two constants for one judgement is how they come to disagree.
fn keeps_working_until(last: Date) -> Date {
    last.checked_add(jiff::Span::new().hours(ABANDONED_AFTER.as_hours()))
        .unwrap_or(Date::MAX)
}

/// **The last day a stopped run is still offered back**, on the same rule.
fn still_offered_until(last: Date) -> Date {
    last.checked_add(jiff::Span::new().hours(OFFER_ABANDONED_WITHIN.as_hours()))
        .unwrap_or(Date::MAX)
}

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

/// How fresh a claim on a role must be to still count as held, before it is
/// treated as free for the taking.
///
/// **Its own value, deliberately not [`ABANDONED_AFTER`].** The sweep asks
/// "has this run gone quiet for a whole day"; a lease asks "is somebody
/// plausibly still at this right now" — minutes, not a day. Fusing the two
/// would mean a run that is genuinely still working, mid-beat, reads as
/// having lost its lease long before the sweep would ever call it abandoned.
///
/// **45 minutes, not 5.** Nothing releases a role when its holder's session
/// ends on its own (a crash, a lost connection) rather than through an
/// explicit wrap — see the release-on-wrap path for the case where it does.
/// Five minutes meant a dev that merely stopped answering, mid-task, was
/// refused its own role again within minutes of picking the work back up.
pub const LEASE_FRESHNESS: jiff::SignedDuration = jiff::SignedDuration::from_secs(45 * 60);

/// **How long a lease has to be old before a write renews it**: a tenth of
/// [`LEASE_FRESHNESS`], 4 minutes 30 seconds.
///
/// Every write by a holder used to rewrite the claim's moment, so the claim
/// gained one stored write per write, for as long as the role was held. A
/// renewal that moves the expiry by seconds buys nothing, so a write inside
/// this age renews nothing.
///
/// **What it costs, and why it is safe:** the stored moment can trail the
/// holder's last write by less than this. The lease then lapses no sooner than
/// 40 minutes 30 seconds after the last write, instead of 45. A tenth keeps a
/// holder writing every few minutes to about 11 renewals an hour at most, and
/// leaves the lease within a tenth of the one decided for it.
pub const LEASE_RENEWS_AFTER: jiff::SignedDuration = jiff::SignedDuration::from_secs(45 * 60 / 10);

/// **Whether a write at `at` should renew a lease last stamped at
/// `claimed_at`.** No stamp, or a stamp at least [`LEASE_RENEWS_AFTER`] old, is
/// due. A stamp in the future of `at` is not: a clock that stepped back must
/// not rewrite a lease that is still fresh.
pub fn renewal_is_due(claimed_at: Option<Timestamp>, at: Timestamp) -> bool {
    claimed_at.is_none_or(|stamped| at.duration_since(stamped) >= LEASE_RENEWS_AFTER)
}

/// A verdict on a claimed role, decided against a threshold — never handed to
/// the caller as a timestamp to judge for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseVerdict {
    /// Nobody has ever claimed it.
    Free,
    /// Claimed, and the claim is still inside the freshness threshold.
    Held,
    /// Claimed once, but the claim is older than the threshold — claimable.
    Stale,
}

/// Decide the [`LeaseVerdict`] for a role last claimed at `claimed_at`, if
/// ever, as of `now`, against `threshold`.
pub fn lease_verdict(
    claimed_at: Option<Timestamp>,
    now: Timestamp,
    threshold: jiff::SignedDuration,
) -> LeaseVerdict {
    match claimed_at {
        None => LeaseVerdict::Free,
        Some(at) if now.duration_since(at) < threshold => LeaseVerdict::Held,
        Some(_) => LeaseVerdict::Stale,
    }
}

/// **What a write to a role's own fields is, when it is not a claim.**
///
/// A renewal and a release both name the holder they act for, and the store
/// applies them ONLY WHILE that sid still holds the role. A claim may take a
/// role nobody holds, so a renewal that was decided as a claim would lease
/// the role again to a session whose release landed an instant earlier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleMoveKind {
    /// Keeps the holder's lease fresh: writes the holder and a new moment.
    Renew,
    /// Gives the role up: clears the holder and writes a moment that reads as
    /// stale.
    Release,
}

/// **A renewal or a release, naming whose it is.** Set only by this crate's
/// own code, never by a caller of a verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleMove {
    /// Which of the two this is.
    pub kind: RoleMoveKind,
    /// The role it acts on.
    pub role: String,
    /// The session id it acts for.
    pub claimant: String,
}

/// One claim's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseClaim {
    /// The claimant now holds the role.
    Taken,
    /// A second claimant, told no while the lease is fresh — and told who
    /// holds it and until when the hold goes stale on its own.
    Refused {
        /// Who holds it.
        holder: String,
        /// When the hold goes stale, absent a renewal.
        until: Timestamp,
    },
}

/// Decide whether `claimant` may take a role currently held by
/// `current_holder` (if any), last claimed at `claimed_at` (if ever).
///
/// **The same claimant renewing is never refused.** A lease is a claim
/// against everyone else, not a lock its own holder must fight its way back
/// into — [`LeaseClaim::Refused`] only ever names somebody else.
pub fn claim_role(
    claimant: &str,
    current_holder: Option<&str>,
    claimed_at: Option<Timestamp>,
    now: Timestamp,
    threshold: jiff::SignedDuration,
) -> LeaseClaim {
    match (current_holder, claimed_at) {
        (Some(holder), Some(at))
            if holder != claimant
                && lease_verdict(Some(at), now, threshold) == LeaseVerdict::Held =>
        {
            LeaseClaim::Refused {
                holder: holder.to_string(),
                until: at + threshold,
            }
        }
        _ => LeaseClaim::Taken,
    }
}

/// **The key a role object holds its current claimant under.** The role is its
/// own object, a child of the bot that holds it, so the key carries no role name:
/// the object is the role.
pub const ROLE_HOLDER: &str = "holder";

/// **The key a role object holds the moment of its last claim under.** A release
/// keeps it and writes the epoch.
pub const ROLE_CLAIMED_AT: &str = "claimed_at";

/// **What a role's lease says right now.**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleState {
    /// The session id that holds the role, if any session does.
    pub holder: Option<String>,
    /// When the role was last claimed or renewed, if it ever was.
    pub claimed_at: Option<Timestamp>,
}

/// **A role's state, read off the role object.**
///
/// `child` is the role object's folded fields, `None` when no such object
/// exists yet. **A role object that carries a claim moment is the whole truth**,
/// with or without a holder: a release clears the holder and keeps the moment. A
/// role object with no moment says nothing about the lease, and so does no
/// object.
pub fn role_state(child: Option<&std::collections::BTreeMap<String, String>>) -> RoleState {
    match child {
        Some(child) if child.contains_key(ROLE_CLAIMED_AT) => RoleState {
            holder: child.get(ROLE_HOLDER).cloned(),
            claimed_at: child.get(ROLE_CLAIMED_AT).and_then(|s| s.parse().ok()),
        },
        _ => RoleState::default(),
    }
}

/// **What a write to a role object's own fields says about a claim.** `None`
/// when it names no holder and a moment that reads as one: a write of anything
/// else on a role object is not this write's business.
pub fn role_claim_in(
    fields: &std::collections::BTreeMap<String, String>,
) -> Option<(String, Timestamp)> {
    let claimant = fields.get(ROLE_HOLDER)?.clone();
    let now: Timestamp = fields.get(ROLE_CLAIMED_AT)?.parse().ok()?;
    Some((claimant, now))
}

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
        return Err(SessionError::InvalidEntry(format!(
            "a focus may be 200 characters and this one is {}",
            f.chars().count()
        )));
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
    /// **The day the caller says this beat happened on**, when the run stated
    /// one. `at` is when the store took it in; this is the day it belongs to in
    /// the caller's own frame, and they are different questions for any run
    /// that is not happening now — a session catching up on last week, or one
    /// acting out months in minutes.
    ///
    /// `None` is a beat from a run that stated no day, and the sweep answers
    /// those on the clock exactly as it always did.
    #[serde(default)]
    pub on: Option<Date>,
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
    /// **The run's own focus, exactly as it stood when this entry closed the
    /// session** — `wrap_session`'s own field, and nothing else ever sets it.
    ///
    /// It used to be glued onto the front of the story as text; a focus is a
    /// machine label set as a side effect of several verbs, so a story that
    /// opened on one read as a fragment to a reader with no context. The
    /// story is the text exactly as written now, and this carries what the
    /// run was doing beside it, never inside it.
    #[serde(default)]
    pub closing_focus: Option<String>,
    /// **Whether a wrap closed the run with this entry.** `wrap_session`'s own
    /// mark, and nothing else sets it. It is a mark of its own because
    /// [`JournalEntry::closing_focus`] cannot be it: that field is empty on a
    /// closing entry whenever the run had no focus, or a focus the story
    /// repeats. A run reopened for one last change takes entries after its
    /// closing one, so the newest entry is not the story, and this is how the
    /// story is found. `false` on every entry written before the mark existed.
    #[serde(default)]
    pub closing: bool,
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
    /// **The day the caller's run says this happened on**, or nothing when the
    /// run stated none. Passed in for the reason `at` is: the domain reads no
    /// clock and invents no frame.
    pub on: Option<Date>,
    /// **The run's own focus, when this entry is closing the session.**
    /// `None` for every ordinary entry — only `wrap_session` ever sets it, via
    /// [`NewEntry::closing`].
    pub closing_focus: Option<String>,
    /// **Whether this entry closes the session** — see
    /// [`JournalEntry::closing`]. Set by [`NewEntry::closing`] and by nothing
    /// else.
    pub closing: bool,
}

impl NewEntry {
    /// An entry the session wrote.
    pub fn manual(text: impl Into<String>, at: Timestamp, on: Option<Date>) -> Self {
        NewEntry {
            text: text.into(),
            at,
            on,
            beat: None,
            closing_focus: None,
            closing: false,
        }
    }

    /// A beat jojobot wrote about a verb class.
    pub fn beat(
        class: impl Into<String>,
        text: impl Into<String>,
        at: Timestamp,
        on: Option<Date>,
    ) -> Self {
        NewEntry {
            text: text.into(),
            at,
            on,
            beat: Some(class.into()),
            closing_focus: None,
            closing: false,
        }
    }

    /// **Mark this entry as the one closing the session**, carrying the run's
    /// own focus at that moment. `None` when the run had none, which adds no
    /// field rather than an empty one. **The entry is marked closing either
    /// way**: the mark is what finds the story, and the focus is only a label.
    #[must_use]
    pub fn closing(mut self, focus: Option<String>) -> Self {
        self.closing_focus = focus;
        self.closing = true;
        self
    }
}

/// A session about to begin. The card id is the run's public name when the boot
/// minted one ([`NewSession::id`]) and is minted by the store when it did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    /// The bot this is one run of.
    pub bot: EntityId,
    /// **The run's public name, minted before the card and stored on it.**
    /// Required, not optional: a card born without one would be a session the
    /// registry could never rebuild an address for, and the whole point of
    /// persisting it is that a restart does not orphan runs.
    pub sid: Sid,
    /// **The id the card is born with**, when the boot minted the run's name
    /// and the name is the card's id. `None` leaves the id to the store, as a
    /// run of the older shape had it.
    pub id: Option<SessionId>,
    /// **The hash of the run's key**, written with the card so a restart does
    /// not bring a run back without it. `None` is a run that holds no key.
    pub key_hash: Option<String>,
    /// What it is working on, at the moment it begins.
    pub focus: String,
    /// When it began.
    pub started_at: Timestamp,
    /// **The zone this run resolves days against**, as the IANA name the caller
    /// supplied at the door, or nothing when it supplied none.
    ///
    /// It is carried as a NAME and never resolved here: what a name means comes
    /// from a database on the machine serving the call, and this model stays
    /// clock-free and reads nothing.
    pub timezone: Option<String>,
    /// **The day this run says it begins on**, as the caller stated it at the
    /// door, or nothing when it stated none. Carried beside the zone because it
    /// answers the other half of the same question: the zone says how to name a
    /// day, and this says which day the run is in.
    pub started_on: Option<Date>,
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
    /// **The hash of the key this run holds**, or [`KEY_VOID`] once its key was
    /// taken away, or `None` for a run that never held one. It is the lookup a
    /// caller's key is matched by and it is never served: no read returns it.
    #[serde(default)]
    pub key_hash: Option<String>,
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
    /// **The zone this run resolves days against**, as an IANA name.
    ///
    /// **A property of the RUN, not of the server.** Two runs of one bot in two
    /// zones legitimately disagree about what today is for the same stored row,
    /// and that is the caller's frame working rather than a fault.
    ///
    /// `None` is a run that supplied none, and every surface that takes one says
    /// which zone answers it then. Absent on every card written before runs
    /// carried one.
    #[serde(default)]
    pub timezone: Option<String>,
    /// **The day this run says it began on**, when it stated one. It is what
    /// the sweep measures a session that has journalled nothing against, the
    /// same way `started_at` is on the clock.
    #[serde(default)]
    pub started_on: Option<Date>,
    /// **A running total of characters this session has been handed** —
    /// every answer's own body, added on the way out (rule 264). Zero for a
    /// session nobody has answered yet, and absent on no card: a legacy row
    /// reads zero rather than nothing, because it has been handed nothing
    /// since this existed to count either way.
    #[serde(default)]
    pub served_chars: u64,
    /// **The day a resume most recently set this run to**, distinct from
    /// [`Session::started_on`] — which stays the creation day forever, and
    /// never moves. `None` until a resume sets one; a reader that wants the
    /// day this run is CURRENTLY in reads this with `started_on` as the
    /// fallback, never the other way round.
    #[serde(default)]
    pub stated_day: Option<Date>,
    /// **The window a wrap left open for one last change**, if one is open or
    /// offered. See [`WrapWindow`]. `None` on a run that never wrapped, on a
    /// run whose window ended, and on every card written before this existed.
    #[serde(default)]
    pub wrap_window: Option<WrapWindow>,
    /// **When this run was last handed its bot's charter**, if the store has
    /// been told. A charter another identity rewrote after this moment has not
    /// been read by the run. [`NEVER_TOLD`] on a run that no call ever handed it
    /// to. `None` on a run that was handed it at its start and has not read it
    /// since, and on every card written before this existed: the run's start is
    /// then the moment to compare from.
    #[serde(default)]
    pub charter_told_at: Option<Timestamp>,
    /// **The key history the newest hash cannot say**: the last key that stopped
    /// opening this run and when another sitting took it over. Empty on a run
    /// that was never resumed or wrapped, and on every card written before this
    /// existed.
    #[serde(default)]
    pub key_trail: KeyTrail,
}

/// **The moment a run that was never handed its charter is held to have been
/// told it**: before every rewrite there can be. It is a value of its own, apart
/// from the `None` of a card written before the column existed, because the two
/// mean different things: a run of an older build is compared from its start, and
/// a run never told is news-bearing for every rewrite by another identity.
pub const NEVER_TOLD: Timestamp = Timestamp::UNIX_EPOCH;

/// **A moment a run was handed its charter, as a reader is told it.** The stored
/// moment of a run never handed its charter is [`NEVER_TOLD`], an instant in 1970
/// that no run lived. A reader gets the words *never told* for it, and every other
/// moment as it was kept. This is the one place that translation is made.
pub fn told_in_words(stored: &str) -> String {
    match stored.parse::<Timestamp>() {
        Ok(moment) if moment == NEVER_TOLD => "never told".to_string(),
        _ => stored.to_string(),
    }
}

impl Session {
    /// **Whether a write to this run is allowed.** An open run takes writes. A
    /// closed one does not, except a `wrapped` run whose window is open: it
    /// stays wrapped and takes writes until the window ends.
    pub fn takes_writes(&self) -> bool {
        !self.state.is_terminal()
            || (self.state == SessionState::Wrapped
                && self.wrap_window.as_ref().is_some_and(WrapWindow::is_open))
    }

    /// **When a wrap's window ends**: [`WRAP_WINDOW_HOURS`] after the wrap. `None`
    /// on a run that holds no window.
    pub fn window_ends_at(&self) -> Option<Timestamp> {
        self.wrap_window.as_ref()?;
        self.wrapped_at()
            .checked_add(jiff::SignedDuration::from_hours(WRAP_WINDOW_HOURS))
            .ok()
    }

    /// **Whether the clock has ended this run's window by `now`.** A window that an
    /// older build opened is judged by the same wrap moment, so one older than
    /// the limit is over.
    pub fn window_lapsed_at(&self, now: Timestamp) -> bool {
        self.state == SessionState::Wrapped && self.window_ends_at().is_some_and(|ends| now >= ends)
    }
}

/// **What one run has been served and every compaction it reported.** The
/// totals the usage records add up; see [`crate::usage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTotals {
    /// The bot the run belongs to.
    pub bot: EntityId,
    /// When the run began.
    pub started_at: Timestamp,
    /// The day the run said it began on, when it stated one.
    pub started_on: Option<Date>,
    /// What the run has been served since its last compaction.
    pub served_chars: u64,
    /// Each compaction the run reported, oldest first.
    pub resets: Vec<crate::usage::Reset>,
    /// The earliest moment the store dated any change of the run.
    pub dated_from: Option<Timestamp>,
}

/// **The compactions a run reported, oldest first**, read off the changes kept
/// beside it: a `served_reset` change holding the total it set back and the
/// moment it was recorded. A change with neither is not a compaction this
/// build can place, so it is not one. The one reading every caller shares, so
/// the usage weeks, the run totals and a run's summary cannot come to disagree
/// about what a compaction is.
///
/// **Each compaction also carries the day the run was in when it was reported**,
/// the day a served total is counted on: the day the run stated when it began,
/// moved by each change of that day before the compaction. A run that states no
/// day has none, and its compactions are counted on the day they were recorded.
/// `changes` is oldest first.
pub fn resets_of(
    changes: &[history::SessionChange],
    started_on: Option<Date>,
) -> Vec<crate::usage::Reset> {
    let mut day = started_on;
    let mut resets = Vec::new();
    for change in changes {
        match change.changed {
            Some(history::ChangeKind::StatedDay) => {
                day = change.value.as_deref().and_then(|text| text.parse().ok());
            }
            Some(history::ChangeKind::ServedReset) => {
                if let (Some(total), Some(at)) = (
                    change.value.as_deref().and_then(|text| text.parse().ok()),
                    change.written_at,
                ) {
                    resets.push(crate::usage::Reset { total, at, day });
                }
            }
            _ => {}
        }
    }
    resets
}

/// **The totals of these runs**, read off the runs and the changes kept beside
/// them. The one reading every store's [`Sessions::run_totals`] comes from, so
/// the stores cannot come to disagree about what a compaction is: a
/// `served_reset` change holding the total it set back.
pub fn run_totals_of<'a>(
    runs: impl IntoIterator<Item = &'a Session>,
    changes_of: impl Fn(&SessionId) -> Option<&'a [history::SessionChange]>,
) -> Vec<RunTotals> {
    runs.into_iter()
        .map(|run| {
            let kept: &[history::SessionChange] = changes_of(&run.id).unwrap_or(&[]);
            RunTotals {
                bot: run.bot.clone(),
                started_at: run.started_at,
                started_on: run.started_on,
                served_chars: run.served_chars,
                resets: resets_of(kept, run.started_on),
                dated_from: kept.iter().filter_map(|change| change.written_at).min(),
            }
        })
        .collect()
}

/// **The summaries of these runs**, each with the changes `histories` keeps for
/// it. A run with none has reported no compaction.
pub fn summaries_with(
    runs: &[Session],
    histories: &history::SessionHistories,
) -> Vec<SessionSummary> {
    let by_run: std::collections::BTreeMap<&SessionId, &[history::SessionChange]> = histories
        .changes
        .iter()
        .map(|(id, kept)| (id, kept.as_slice()))
        .collect();
    runs.iter()
        .map(|run| SessionSummary::of(run, by_run.get(&run.id).copied().unwrap_or(&[])))
        .collect()
}

/// **A run, without its chronology** — the fields [`list_runs`](crate) and
/// anything like it actually renders: which run, what it was doing, in what
/// state, and the two numbers about its beats that today cost reading every
/// one of them to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// The store-minted id.
    pub id: SessionId,
    /// The handle this run answers to, `None` only for a card predating
    /// stored handles.
    pub sid: Option<Sid>,
    /// The bot this is one run of.
    pub bot: EntityId,
    /// What it was working on, last.
    pub focus: String,
    /// When it began.
    pub started_at: Timestamp,
    /// Which column it sits in.
    pub state: SessionState,
    /// How many entries the chronology holds — never the entries.
    pub entry_count: usize,
    /// The instant this session last had anything to show for itself — see
    /// [`Session::last_beat`], the same computation, answered without the
    /// text.
    pub last_beat: Timestamp,
    /// The running total of characters this session has been handed.
    pub served_chars: u64,
    /// **The day this run said it began on**, when it stated one. A reader that
    /// places a run on a day uses this before the clock: a run played on a stated
    /// day began on that day whatever the clock said when it did.
    pub started_on: Option<Date>,
    /// How many compactions the run reported.
    pub compactions: u64,
    /// The sum of the totals those compactions set back. With `served_chars` it
    /// is the whole of what the run was served: see [`SessionSummary::served_lifetime`].
    pub served_set_back: u64,
    /// **Why this run could not be read**, when its stored row holds a moment, a
    /// day or a state jojobot cannot read (rule 130). The run is still named, with
    /// the fields that could be read, so one damaged row never fails the read of
    /// the others; its moments are not to be trusted. `None` for every run that
    /// read whole.
    pub unreadable: Option<String>,
}

impl SessionSummary {
    /// **The name the run answers to**: its public name, or its own id for a card
    /// written before names were kept. The name its calls are counted under.
    pub fn run_name(&self) -> &str {
        self.sid
            .as_ref()
            .map_or(self.id.as_str(), |sid| sid.as_str())
    }

    /// **Everything the run was served over its life**: what it has been served
    /// since its last compaction, plus the totals its compactions set back. The
    /// one sum, [`crate::usage::lifetime_served`], which the usage weeks use too.
    pub fn served_lifetime(&self) -> u64 {
        crate::usage::lifetime_served(self.served_chars, self.served_set_back)
    }

    /// A summary derived from a whole [`Session`] already in hand and the
    /// changes kept beside it — the fallback every store gets for free through
    /// [`Sessions::summaries_of`]'s own default, and what a reader holding whole
    /// runs in memory answers with. `changes` is where the compactions are: a
    /// run's own record holds none.
    pub fn of(session: &Session, changes: &[history::SessionChange]) -> Self {
        let resets = resets_of(changes, session.started_on);
        SessionSummary {
            id: session.id.clone(),
            sid: session.sid.clone(),
            bot: session.bot.clone(),
            focus: session.focus.clone(),
            started_at: session.started_at,
            state: session.state,
            entry_count: session.entries.len(),
            last_beat: session.last_beat(),
            served_chars: session.served_chars,
            started_on: session.started_on,
            compactions: 0,
            served_set_back: 0,
            unreadable: None,
        }
        .counting(&resets)
    }

    /// **This summary with its compactions taken from `resets`**: how many there
    /// are and the total they set back. Every store's summary ends here, with
    /// the resets [`resets_of`] kept, so a store cannot count a compaction by a
    /// rule of its own.
    pub fn counting(mut self, resets: &[crate::usage::Reset]) -> Self {
        self.compactions = resets.len() as u64;
        self.served_set_back = crate::usage::set_back_total(resets);
        self
    }
}

impl Session {
    /// The instant this session last had anything to show for itself: its
    /// newest entry, or the moment it began if it never wrote one.
    ///
    /// **What the sweep measures.** Not "when did it last call something" — a
    /// session that boots and never journals leaves nothing to measure but its
    /// own start, and that is exactly the session the sweep exists for.
    pub fn last_beat(&self) -> Timestamp {
        history::last_beat(
            self.started_at,
            self.entries.iter().map(|e| (e.at, e.touched)),
        )
    }

    /// **The entry a wrap closed this run with**: the newest entry marked
    /// closing. A run wrapped a second time holds two, and the second is the
    /// story it ended on.
    ///
    /// **A run wrapped before the mark existed is read by what the wrap did
    /// leave.** It could already be reopened by its wrap code, and its closing
    /// entry carries the focus the run had at the close, which no other entry
    /// does. With no marked entry, the newest entry that carries one is the
    /// closing entry. `None` for a run that was never wrapped, and for one wrapped
    /// before the mark with no focus to carry, whose last entry is its story.
    pub fn closing_entry(&self) -> Option<&JournalEntry> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.closing)
            .or_else(|| {
                self.entries
                    .iter()
                    .rev()
                    .find(|entry| entry.closing_focus.is_some())
            })
    }

    /// **When the run was wrapped**: the moment of its closing entry. A run
    /// reopened for one last change takes entries after it, so
    /// [`Session::last_beat`] moves and this does not. A run with no marked
    /// entry answers `last_beat`, which is its wrap for a run from before the
    /// mark.
    pub fn wrapped_at(&self) -> Timestamp {
        self.closing_entry()
            .map_or_else(|| self.last_beat(), |entry| entry.at)
    }

    /// **The newest day this session has to show for itself in the caller's own
    /// frame**: the newest day any beat states, or the day it says it began.
    ///
    /// `None` when the run stated no day anywhere, which is the only case the
    /// clock answers.
    pub fn last_beat_on(&self) -> Option<Date> {
        self.entries
            .iter()
            .filter_map(|e| e.on)
            .max()
            .or(self.started_on)
    }

    /// Whether this session is far enough past its last beat to sweep it. Only
    /// an `active` session is ever swept: the other two are already closed.
    ///
    /// 🚨 **`today` is the day the caller states, and it wins when both sides
    /// have one.** The clock answers only when nobody stated a frame — a run
    /// acting out months finishes in real minutes, so measured on the clock
    /// nothing it leaves is ever stale and every sitting after the first meets
    /// a choice it should never see.
    pub fn is_stale(&self, now: Timestamp, today: Option<Date>) -> bool {
        if self.state.is_terminal() {
            return false;
        }
        match (today, self.last_beat_on()) {
            (Some(today), Some(last)) => today > keeps_working_until(last),
            _ => now.duration_since(self.last_beat()) >= ABANDONED_AFTER,
        }
    }

    /// Whether a boot should **offer** this run back — an `abandoned` one that
    /// stopped inside [`OFFER_ABANDONED_WITHIN`].
    ///
    /// `wrapped` is never offered: its story was told and it does not reopen.
    /// An older `abandoned` run is not offered either, and stays resumable by
    /// anyone holding its handle — see the constant.
    pub fn is_offerable(&self, now: Timestamp, today: Option<Date>) -> bool {
        if self.state != SessionState::Abandoned {
            return false;
        }
        match (today, self.last_beat_on()) {
            (Some(today), Some(last)) => today <= still_offered_until(last),
            _ => now.duration_since(self.last_beat()) < OFFER_ABANDONED_WITHIN,
        }
    }
}

/// Why a session operation failed. Adapters map their transport/parse errors
/// into these; the domain and the MCP layer speak only this vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The commit may have landed, but no result can be reconstructed.
    #[error("{}", crate::request::INDETERMINATE_WRITE)]
    Indeterminate,
    /// The session id is not a well-formed token.
    #[error("invalid session id '{0}': ids are [a-z0-9-]+")]
    InvalidId(String),
    /// The entry or focus is malformed for storage.
    #[error("invalid entry: {0}")]
    InvalidEntry(String),
    /// The addressed session doesn't exist. Never created, never guessed at.
    #[error("no session '{attempted}': no bot's sessions carry a run with that id")]
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
         change. Its chronology is the account of what happened. A wrapped session is the \
         last word; an abandoned one can be picked back up by resuming it"
    )]
    Closed {
        /// The id that was addressed.
        attempted: String,
        /// Which end it reached.
        state: SessionState,
        /// The run that took this run's role over, when that closed it.
        displaced_by: Option<Sid>,
    },
    /// A wrap window was set on a run that is not `wrapped`. Only a wrapped run
    /// has one.
    #[error("session '{attempted}' is {state}, so it has no wrap window: only a wrapped run does")]
    NotWrapped {
        /// The id that was addressed.
        attempted: String,
        /// The state it is in.
        state: SessionState,
    },
    /// An amend that had nothing to amend. Refused rather than turned into an
    /// append: a caller who meant to correct a beat and silently wrote a new one
    /// has a chronology saying something they did not mean.
    #[error(
        "session '{attempted}' has no entry of its own yet, so there is no most-recent one to \
         amend — journal it instead. Only an entry the session wrote can be amended, \
         never a line jojobot wrote about its calls"
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
         session itself recorded, and those are append-only. Only the session's newest own \
         entry can be amended, through amend_journal"
    )]
    NotABeat {
        /// The entry that was addressed.
        attempted: String,
        /// The session it is on.
        session: String,
    },
    /// The underlying store failed.
    #[error("store error: {0}")]
    Store(String),
    /// **The store answered and what it holds cannot be read.** A stored moment,
    /// day or state that does not parse: damage a person repairs, which sending
    /// the same call again meets again. Told apart from [`SessionError::Store`], a
    /// store that could not be reached, which a retry can outlast. The words name
    /// the field and never carry the damaged cell.
    #[error("a run's stored record cannot be read: {what}")]
    Unreadable {
        /// Which field of the record, and why it does not read.
        what: String,
    },
    /// **The store was reached and refused the write** on a rule it enforces
    /// itself: see [`crate::memory::MemoryError::Refused`], the same distinction
    /// on this rail. The word is the rule's kind and nothing of the server's own
    /// account.
    #[error(
        "the store refused this write on a rule it enforces ({0}); nothing was written, and \
         sending the same call again will meet the same refusal"
    )]
    Refused(String),
    /// **A write collided with another that landed the same instant** — see
    /// [`crate::memory::MemoryError::Conflict`], the same distinction on
    /// this rail: the store answered correctly and promptly, so this is not
    /// the escalate-to-a-person failure [`SessionError::Store`] is.
    #[error(
        "this write collided with another that landed the same instant; nothing was written \
         here — the store is working, and retrying the same call is the right response to a \
         transient conflict rather than a mistake in what was sent"
    )]
    Conflict,
    /// **A mention this process cannot tell from an ordinary word**, because
    /// it has loaded no kinds at all. The same failure
    /// [`crate::memory::MemoryError::KindsNeverLoaded`] names on the memory
    /// rail, carried here because a session's focus and its chronology hold
    /// mentions on the same terms a claim's words do.
    #[error("{}", crate::memory::kinds::NotAKind::SetNeverLoaded)]
    KindsNeverLoaded,
}

/// The Sessions port. A store-backed adapter stands behind it in production; a
/// fake stands behind it in tests. The invariants every adapter holds:
///
/// * **a write is reported successful only once it has landed** — read back
///   through the read path, or carried by a transaction that either commits or
///   does not.
/// * **closed is closed, for writes** — nothing appends to, amends, or changes
///   the focus of a closed session, whichever end it reached, and no rollback
///   moves a card out of a terminal column.
/// * **only `wrapped` is the last word** — [`reopen`](Sessions::reopen) takes an
///   `abandoned` session back to `active` and is refused on a `wrapped` one. It
///   is the single walk-back on this port; see [`SessionState::is_final`].
/// * **nothing is created as a side effect** — [`begin`](Sessions::begin) is the
///   only thing that brings a session card into being, and the caller decides
///   when. A boot that never works leaves no card.
#[jojobot_forward::port(
    callback = forward_sessions,
    scope = "session",
    uses(
        crate::memory::EntityId,
        ::jiff::Timestamp,
        ::jiff::civil::Date,
    )
)]
pub trait Sessions: Send + Sync {
    /// Every session of one bot, whatever its state, newest start first. What
    /// attaching reads, and what the sweep walks.
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError>;

    /// **The same runs, without their chronology** — what a caller asking
    /// "which runs, in what state, when" needs, which is never the text of
    /// a single beat.
    ///
    /// **Defaulted off [`sessions_of`](Sessions::sessions_of).** A store that
    /// cannot answer the count and the last beat any cheaper than by reading
    /// every entry is still correct read through here — it just does not
    /// save anything. [`DoltSessions`] overrides this with a query that
    /// never asks the store for a beat's own text at all, which is the
    /// point: `list_runs` renders two numbers per run and today pays for
    /// every word to get them.
    async fn summaries_of(&self, bot: &EntityId) -> Result<Vec<SessionSummary>, SessionError> {
        let runs = self.sessions_of(bot).await?;
        let histories = self.scan_session_history().await?;
        Ok(summaries_with(&runs, &histories))
    }

    /// **Every run on the board, without its chronology** — the same
    /// widening [`all_sessions`](Sessions::all_sessions) is to
    /// [`sessions_of`], for the same reason [`summaries_of`] exists: a
    /// caller asking which runs exist, whosever they are, never needs a
    /// beat's own text to answer it.
    ///
    /// **Defaulted off [`all_sessions`](Sessions::all_sessions).** A store
    /// with no cheaper query is still correct through here; [`DoltSessions`]
    /// overrides it with the same aggregate [`summaries_of`] uses, minus
    /// the one clause that scopes it to a bot.
    async fn all_summaries(&self) -> Result<Vec<SessionSummary>, SessionError> {
        let runs = self.all_sessions().await?;
        let histories = self.scan_session_history().await?;
        Ok(summaries_with(&runs, &histories))
    }

    /// **One run, without its chronology** — `None` when no run carries the id,
    /// or the id could never be one. What a caller that names a single run needs
    /// to know whose it is and what state it is in, which is never the text of
    /// a beat and never the other runs on the board.
    ///
    /// **Defaulted off [`read_session`](Sessions::read_session).** A store with
    /// no cheaper query is still correct through here; [`DoltSessions`] overrides
    /// it with the aggregate [`summaries_of`](Sessions::summaries_of) uses,
    /// narrowed to the one run. A layer over a store forwards this, or the read
    /// it stands for goes back to costing what the board holds.
    async fn summary_of(&self, id: &SessionId) -> Result<Option<SessionSummary>, SessionError> {
        match self.read_session(id).await {
            Ok(session) => {
                let changes = self.session_history(id).await?;
                Ok(Some(SessionSummary::of(&session, &changes)))
            }
            Err(SessionError::UnknownSession { .. } | SessionError::InvalidId(_)) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// **What every run has been served and every compaction it reported**, in
    /// one read and without a chronology: the input of the usage records.
    ///
    /// **Defaulted off [`all_sessions`](Sessions::all_sessions) and
    /// [`scan_session_history`](Sessions::scan_session_history)**, so a store
    /// with nothing cheaper is still correct through here; the fold answers from
    /// its image. A layer over a store forwards this, for the reason
    /// [`summary_of`](Sessions::summary_of) names.
    async fn run_totals(&self) -> Result<Vec<RunTotals>, SessionError> {
        let runs = self.all_sessions().await?;
        let histories = self.scan_session_history().await?;
        let by_run: std::collections::BTreeMap<&SessionId, &[history::SessionChange]> = histories
            .changes
            .iter()
            .map(|(id, kept)| (id, kept.as_slice()))
            .collect();
        Ok(run_totals_of(&runs, |id| by_run.get(id).copied()))
    }

    /// **Count one call that reached a verb**: add one to the count held under
    /// `key`. A store that keeps counts adds in one step, so two calls counted
    /// at once are both counted.
    ///
    /// **A key that names a run is counted twice, in one step**: under the bot and
    /// the day, as a key with no run is, and under the run. The two always agree.
    ///
    /// **The default refuses.** A layer over a store forwards this, and one that
    /// forgot to would otherwise lose every count without a sound; the refusal
    /// is logged by the caller on each call, which is how a missing forward is
    /// found.
    async fn count_call(&self, key: &crate::usage::CallKey) -> Result<(), SessionError> {
        let _ = key;
        Err(SessionError::Store(
            "this store keeps no counts of calls".to_string(),
        ))
    }

    /// **What the given runs called and were refused**, one entry per run, verb
    /// and ending; `None` asks for every run's. A run is named by its public name.
    /// Kept by [`count_call`](Sessions::count_call) for a key that names a run.
    /// The default is a store that keeps none.
    async fn run_calls(
        &self,
        runs: Option<&[String]>,
    ) -> Result<Vec<crate::usage::RunCall>, SessionError> {
        let _ = runs;
        Ok(Vec::new())
    }

    /// **Every count of calls the store holds**, one entry per key. The input of
    /// the calls a usage record shows. The default is a store that keeps none.
    async fn call_counts(&self) -> Result<Vec<crate::usage::CallCount>, SessionError> {
        Ok(Vec::new())
    }

    /// **What every run was served on every day**, counted by [`add_served`]:
    /// the input of the days a usage record shows. Nothing is counted for a day
    /// before the store began to keep days. The default is a store that keeps
    /// none.
    ///
    /// [`add_served`]: Sessions::add_served
    async fn served_days(&self) -> Result<Vec<crate::usage::ServedDay>, SessionError> {
        Ok(Vec::new())
    }

    /// **How many runs each bot has**, whatever their states: the numbers a
    /// caller needs to say how many runs exist, or how many belong to others,
    /// without reading any of them. A bot with no run is absent.
    ///
    /// **Defaulted off [`all_summaries`](Sessions::all_summaries)**, as the
    /// others are, and overridden by [`DoltSessions`] with one grouped count. A
    /// layer over a store forwards this, for the reason
    /// [`summary_of`](Sessions::summary_of) names.
    async fn run_counts(
        &self,
    ) -> Result<std::collections::BTreeMap<EntityId, usize>, SessionError> {
        let mut counts = std::collections::BTreeMap::new();
        for run in self.all_summaries().await? {
            *counts.entry(run.bot).or_insert(0) += 1;
        }
        Ok(counts)
    }

    /// **Every session on the board, whosever it is** — what the handle registry
    /// is rebuilt from at startup.
    ///
    /// Read once, eagerly, rather than lazily on a miss: a lazy rebuild would
    /// make the first caller after a restart get a different answer from the
    /// second, which is the kind of difference nobody can reproduce.
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError>;

    /// A cheap signal for whether any run has been written since a caller
    /// last looked, so a refresh can skip paying for
    /// [`all_sessions`](Sessions::all_sessions) when nothing changed —
    /// [`jojobot_domain::memory::Memory::write_summary`]'s own shape, one
    /// count and one moment, carried over to this port rather than answered
    /// by a second mechanism.
    ///
    /// **`None` means this store offers no such signal**, exactly as the
    /// memory port's own default: a caller that cannot tell "unchanged"
    /// from "I don't know" must treat every refresh as if something
    /// changed.
    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        Ok(None)
    }

    /// One session by id, chronology and all.
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError>;

    /// Bring a session card into being. **The only mint on this port** — see the
    /// trait's invariants.
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError>;

    /// Append one entry to the chronology. Refused on a closed session.
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError>;

    /// Rewrite the **most recent entry the session wrote itself** in place, never
    /// a beat jojobot wrote (a beat is one with [`JournalEntry::is_auto`]), which
    /// may be newer than it. Refused on a closed session, and refused with
    /// [`SessionError::NoEntries`] when the session has none of its own to amend.
    /// Only the last one: everything older is append-only.
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

    /// **Record the zone this run resolves days against**, replacing whatever
    /// it carried.
    ///
    /// A run outlives a disconnect and a device hop, so the zone it was born in
    /// is not always the zone it is being worked in. The door writes this when
    /// a boot supplies one, so the card and the live run never say two
    /// different things about the same run.
    ///
    /// The name is stored as given and is never resolved here — see
    /// [`Session::timezone`].
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError>;

    /// **Record the day a resume most recently set this run to**, replacing
    /// whatever it carried — [`Session::stated_day`], persisted exactly the
    /// way [`set_timezone`](Sessions::set_timezone) already persists the
    /// zone.
    ///
    /// A run outlives a restart, so a day set only in the process registry is
    /// lost the moment the process is. [`Session::started_on`] is untouched
    /// by this: it stays the day the run began, and this is the day a later
    /// resume moved it to.
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError>;

    /// **Set or clear the window a wrap left open.** Never refused on a closed
    /// run, because it is the one write that is about a closed run: it works
    /// on a `wrapped` session and refuses any other state, and `None` clears
    /// whatever was there. Writes nothing to the chronology.
    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError>;

    /// Move a session to a terminal state. Refused if it is already in one —
    /// terminal both ways.
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError>;

    /// **Abandon an open session because the run `by` took its role over**, and
    /// record `by` on the session in the same act. The close is the same as
    /// [`close`](Sessions::close) to `abandoned`, with the displacing run kept
    /// beside it so a later refusal can say why the run stopped.
    async fn close_displaced(&self, id: &SessionId, by: &Sid) -> Result<Session, SessionError>;

    /// **Add to the running total of what this session has been handed**
    /// (rule 264) — characters, on every answer this run receives — and to the
    /// count of what it was handed on `day`, the day the run was in when the
    /// answer was served.
    ///
    /// **The two are one write**, so the days always add up to the lifetime the
    /// run was counted over, and a store that cannot keep both keeps neither.
    ///
    /// **Never refused on a closed session.** [`SessionError::Closed`] is
    /// about writes that change what a session SAID — an entry, a focus, a
    /// chronology — and this changes none of that: the answer that closes a
    /// run is itself something the run was handed, so refusing here would
    /// undercount by exactly the one answer that matters most.
    ///
    /// [`SessionError::UnknownSession`] on a card that does not exist,
    /// exactly as every other addressed verb here.
    async fn add_served(
        &self,
        id: &SessionId,
        chars: u64,
        day: jiff::civil::Date,
    ) -> Result<(), SessionError>;

    /// **Start the running total of what this session has been handed over
    /// again.** The one verb that lowers it: a run that reports its context was
    /// compacted holds nothing of what it was handed, so what it has been handed
    /// is counted from here.
    ///
    /// Refused exactly as [`Sessions::add_served`] is: never on a closed
    /// session, and [`SessionError::UnknownSession`] on a card that does not
    /// exist. **The reset is recorded as one change of the run carrying the
    /// total it set back**; an answer served adds to the total and records
    /// nothing.
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError>;

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

    /// **Keep when this run was handed its bot's charter.** Set at a read of
    /// the charter, so the notice that it changed ends and stays ended across a
    /// restart.
    ///
    /// **Never refused on a closed session**, for the reason
    /// [`add_served`](Sessions::add_served) is not: it changes nothing the run
    /// said. [`SessionError::UnknownSession`] on a card that does not exist.
    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError>;

    /// **Every change a run went through, oldest first**, each with the moment
    /// the store made its record and what was kept of it. A read: nothing moves
    /// and every current read answers as it did.
    ///
    /// **A change made before the store kept this has a record that says only
    /// that the run was written**, with no kind and no moment, and a run changed
    /// only before has nothing more. An empty answer for a run says nothing was
    /// recorded, never that nothing happened. The key a run works through is
    /// recorded as changed and its hash is never in an answer, and neither is the
    /// code of a wrap window. [`SessionError::UnknownSession`] on a card that
    /// does not exist.
    async fn session_history(
        &self,
        id: &SessionId,
    ) -> Result<Vec<history::SessionChange>, SessionError>;

    /// **Every record of every run in one read**, for the image that answers
    /// [`session_history`](Sessions::session_history) and
    /// [`journal_history`](Sessions::journal_history) from memory. A run with
    /// none is not listed. A read: nothing moves.
    async fn scan_session_history(&self) -> Result<history::SessionHistories, SessionError>;

    /// **Every wording of one journal entry, oldest first**: the text it was
    /// appended with and the text of each rewrite after it. The entry itself
    /// stays the newest wording, and every current read answers as it did. An
    /// entry written before the store kept this has no wordings. An entry the
    /// run does not hold is [`SessionError::NoEntries`], and a run that does not
    /// exist [`SessionError::UnknownSession`].
    async fn journal_history(
        &self,
        id: &SessionId,
        entry: &EntryId,
    ) -> Result<Vec<history::Wording>, SessionError>;

    /// **Write the hash of the key this run holds, or [`KEY_VOID`] when its key
    /// is taken away.** The row is written before the call that rotated or
    /// removed the key returns, so a restart cannot bring an old key back.
    ///
    /// Allowed on a closed run, because a wrap removes the key of a run that is
    /// already closing, and a wrap code returns one to a run that is `wrapped`.
    /// [`SessionError::UnknownSession`] on a card that does not exist.
    ///
    /// **The `trail` is written in the same statement** and replaces what the
    /// card held: the caller sends the whole picture, because a restart reads it
    /// back as the picture and a half written would be a run that lost the other
    /// half.
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &KeyTrail,
    ) -> Result<(), SessionError>;

    /// **Keep the key a boot handed a run that has not written yet.** One row per
    /// run, written once; a second note of the same name changes nothing. Read
    /// back by [`minted_without_a_card`](Sessions::minted_without_a_card) so a
    /// restart brings the key back before the run's first write makes a card.
    async fn note_minted(&self, run: &MintedRun) -> Result<(), SessionError>;

    /// **The minted runs that have no card**, oldest first. A run whose card
    /// exists is not here: the card holds its key.
    async fn minted_without_a_card(&self) -> Result<Vec<MintedRun>, SessionError>;

    /// **Delete the spent and the abandoned notes.** A note whose run has a card
    /// is spent. A note with no card minted before `before` is abandoned: its
    /// caller never wrote and did not come back inside the window. Returns how
    /// many it deleted.
    async fn prune_minted(&self, before: Timestamp) -> Result<usize, SessionError>;

    /// **Keep that `bot`'s charter was rewritten**, by whom, from which run
    /// (`by_session`, the run's public name) and when. Every change is kept, in
    /// the order it was made, the bot's own included.
    ///
    /// What [`Self::charter_change_of`] answers is one record per bot, the newest
    /// change by ANOTHER identity, rewritten in place by the next such change:
    /// the question it answers is whether the charter changed since a session
    /// was last handed it, and only the newest change can answer that.
    ///
    /// Kept by the store and not by the process: a session outlives a restart,
    /// so a change recorded only in memory would be lost on the deploy that
    /// follows it, and the session would work to the old text with nothing to
    /// say so.
    ///
    /// A change by the bot's own identity is kept with the rest and replaces
    /// nothing the reads answer with: its own rewrite needs no notice.
    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError>;

    /// The newest charter change another identity made to `bot`, or `None` when
    /// none has been recorded.
    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError>;

    /// **Every charter change on record, by bot** — what a reader that holds the
    /// whole ledger in memory fills itself from. `None`, the default, says this
    /// port keeps no list it can hand over, so a reader must ask
    /// [`charter_change_of`](Self::charter_change_of) bot by bot.
    async fn charter_changes(
        &self,
    ) -> Result<Option<std::collections::BTreeMap<EntityId, CharterChange>>, SessionError> {
        Ok(None)
    }
}

/// **A charter rewritten by another identity**: who, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharterChange {
    /// The identity that rewrote the charter.
    pub by: EntityId,
    /// The moment of the rewrite, on the server's clock.
    pub at: Timestamp,
}

/// A running tally of one verb class, as one chronology entry.
///
/// **jojobot's own account of what a session did**, kept apart from what the
/// session said about itself: one beat per class per session, its count and its
/// examples corrected in place as the class repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beat {
    /// The entry the tally lives in.
    pub entry: EntryId,
    /// How many calls of this class this session has made.
    pub count: usize,
    /// The first few things it named, so the beat says what it touched and not
    /// only how often. Capped — a beat is a beat, not a log.
    pub examples: Vec<String>,
}

/// How many examples a beat carries before it stops naming them.
pub const BEAT_EXAMPLES: usize = 5;

/// Every verb class jojobot beats, and the phrase its tally is written with.
///
/// **One table, because the phrase is half the parse.** A beat is rendered from
/// it and read back through it, so a class whose phrase lived only at its call
/// site would render fine and come back unparseable on the next reconnect.
///
/// **Writes only.** A read is attributed and never journalled, so no read verb
/// appears here.
pub const BEAT_CLASSES: &[(&str, &str)] = &[
    ("add_entity", "brought entities into being"),
    ("update_entity", "edited entities"),
    ("capture", "captured facts about"),
    ("put_document", "put markdown onto"),
    ("update_fact", "edited facts"),
    ("set_charter", "wrote charters for"),
    ("post_message", "posted to mailboxes"),
    ("mark_processed", "retired messages"),
    ("quarantine", "quarantined messages"),
    ("rename_entity", "renamed entities"),
    ("retract", "took back records"),
    ("merge_entities", "merged entities"),
    ("archive_entity", "archived entities"),
];

/// A running tally, as one line of chronology.
///
/// **One shape, always, including at a count of one** — because this line is
/// where the tally LIVES. A session outlives the connections that write to it,
/// so a resumed session's counts are read back out of the entries by
/// [`parse_beat`], and a rendering that dropped the count for the first
/// occurrence would make the two disagree the moment somebody reconnects.
pub fn beat_text(phrase: &str, beat: &Beat) -> String {
    let mut named = beat.examples.join(", ");
    // Said out loud when the examples stop naming everything, so the line does
    // not read as a complete list that happens to be short.
    if beat.examples.len() < beat.count {
        named.push_str(", …");
    }
    format!("{phrase}: {named} ({})", beat.count)
}

/// Read a tally back out of the line it was rendered as — the inverse of
/// [`beat_text`], and the reason a resumed session keeps counting rather than
/// starting over.
///
/// `None` for a line this did not write: a beat whose text a person edited by
/// hand is left exactly as they left it, and the class starts a fresh tally
/// rather than jojobot rewriting their words into its own format.
pub fn parse_beat(phrase: &str, entry: &JournalEntry) -> Option<Beat> {
    let rest = entry.text.strip_prefix(phrase)?.strip_prefix(": ")?;
    let (named, count) = rest.rsplit_once(" (")?;
    let count: usize = count.strip_suffix(')')?.parse().ok()?;
    let examples: Vec<String> = named
        .trim_end_matches(", …")
        .split(", ")
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect();
    Some(Beat {
        entry: entry.id.clone(),
        count,
        examples,
    })
}

/// The tally this session already has, read off its chronology — what makes the
/// one-beat-per-class rule belong to the SESSION rather than to whichever
/// connection happens to be holding it.
pub fn beats_of(session: &Session) -> std::collections::HashMap<&'static str, Beat> {
    let mut found = std::collections::HashMap::new();
    for entry in &session.entries {
        let Some(class) = entry.beat.as_deref() else {
            continue;
        };
        let Some((class, phrase)) = BEAT_CLASSES.iter().find(|(known, _)| *known == class) else {
            continue;
        };
        if let Some(beat) = parse_beat(phrase, entry) {
            found.insert(*class, beat);
        }
    }
    found
}

/// **What a boot found on this bot's board**, after the sweep has run.
///
/// Named rather than a tuple: a `(Vec, Option, Vec)` at five call sites is a
/// shape nobody can read.
#[derive(Debug, Default)]
pub struct Board {
    /// Every run still working — **all of them, not the newest.** A bot may
    /// have several at once (two devices, two pieces of work), so the offer
    /// needs them all.
    pub live: Vec<Session>,
    /// The one stopped run worth bringing up, if there is one.
    pub offerable: Option<Session>,
    /// **The newest run that was wrapped up, if there is one — the handover.**
    ///
    /// ⛔️ **Not a resume candidate, and it must never be offered as one.**
    /// `wrapped` is terminal: nothing appends to it. This is here because
    /// wrapping is the one act that requires somebody to say what happened, so
    /// a wrapped run carries a story by construction — and a run that closed
    /// properly used to vanish from the next boot while every run that merely
    /// stopped stayed on the offer. Closing cleanly was how you became
    /// invisible.
    pub handover: Option<Session>,
    /// The ids this sweep closed.
    pub swept: Vec<String>,
    /// The stale runs the store **refused to close**, with why.
    ///
    /// Reported rather than logged, because the domain does not log: it says
    /// what happened and the caller — which owns the log — decides what that is
    /// worth saying. Each of these is left `active` for the next boot to try
    /// again; a sweep that cannot close one session must not stop a boot.
    pub unswept: Vec<(SessionId, SessionError)>,
    /// **Every run of this bot that carries a wrap window**, as the read the
    /// sweep started from saw it. A boot ends them when a newer run begins, and
    /// handing the ids down is what lets it do so without asking the store for
    /// the bot's runs a second time.
    pub windows_open: Vec<SessionId>,
}

/// The ids of the runs among these that carry a wrap window, open or offered.
pub fn windows_open(runs: &[Session]) -> Vec<SessionId> {
    runs.iter()
        .filter(|run| run.wrap_window.is_some())
        .map(|run| run.id.clone())
        .collect()
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
    today: Option<Date>,
) -> Result<Board, SessionError> {
    let existing = sessions.sessions_of(bot).await?;

    let mut swept = Vec::new();
    let mut unswept = Vec::new();
    for stale in existing.iter().filter(|s| s.is_stale(now, today)) {
        match sessions.close(&stale.id, SessionState::Abandoned).await {
            Ok(_) => swept.push(stale.id.to_string()),
            Err(e) => unswept.push((stale.id.clone(), e)),
        }
    }
    Ok(Board::of(existing, swept, unswept, now, today))
}

/// **The board as a boot would find it, with nothing written.** Every run a
/// sweep would close is read as already closed, and none is closed. It is the
/// same decision [`sweep_and_find`] makes, from the same function, so what a
/// refusal lists as in flight is what the door would offer.
pub async fn find_without_sweeping(
    sessions: &dyn Sessions,
    bot: &EntityId,
    now: Timestamp,
    today: Option<Date>,
) -> Result<Board, SessionError> {
    let existing = sessions.sessions_of(bot).await?;
    let would_sweep = existing
        .iter()
        .filter(|s| s.is_stale(now, today))
        .map(|s| s.id.to_string())
        .collect();
    Ok(Board::of(existing, would_sweep, Vec::new(), now, today))
}

impl Board {
    /// **What is on a bot's board, given the runs it has and the ones that were
    /// swept.** Pure: it reads nothing and writes nothing, so a boot that swept
    /// and a read that did not decide on one definition.
    pub fn of(
        existing: Vec<Session>,
        swept: Vec<String>,
        unswept: Vec<(SessionId, SessionError)>,
        now: Timestamp,
        today: Option<Date>,
    ) -> Board {
        // Newest first already, so the first live one is the newest.
        let live: Vec<Session> = existing
            .iter()
            .filter(|s| !s.state.is_terminal() && !s.is_stale(now, today))
            .cloned()
            .collect();
        // **The run somebody closed properly MOST RECENTLY**, read before the
        // list is consumed. `existing` sorts newest-START-first, which is not
        // the order a WRAP happened in: a run started long ago and wrapped just
        // now still needs to win over one started more recently and wrapped
        // earlier. `wrapped_at` is the wrap's own moment: the moment of the entry
        // `wrap_session` closed the run with, which a later entry written by a run
        // reopened for one last change does not move.
        let handover = existing
            .iter()
            .filter(|s| s.state == SessionState::Wrapped)
            .max_by_key(|s| s.wrapped_at())
            .cloned();
        // **Read AFTER the sweep, and through it.** The run this boot just marked
        // `abandoned` is the archetypal "resume last session" — it is the one that
        // stopped yesterday — so it has to be a candidate here, and the list read
        // above still says `active` for it.
        let windows_open = windows_open(&existing);
        let offerable = existing
            .into_iter()
            .map(|s| match swept.contains(&s.id.to_string()) {
                true => Session {
                    state: SessionState::Abandoned,
                    ..s
                },
                false => s,
            })
            .find(|s| s.is_offerable(now, today));
        Board {
            live,
            offerable,
            handover,
            swept,
            unswept,
            windows_open,
        }
    }
}

/// **The key a run's start day rides under** on the projected run. The walk
/// places a run by it when a read asks what was near a day, because a run holds
/// no claim for any clock to read.
pub const STARTED_ON: &str = "started_on";

mod projection;
pub use projection::{
    COMPACTIONS, SERVED_LIFETIME, SERVED_SINCE_COMPACTION, projected, projected_summary,
    served_on_keys,
};

#[cfg(test)]
mod reset_day_tests;

#[cfg(test)]
mod projection_tests {
    use super::*;
    use crate::memory::EntityKind;

    /// **A run projects into an object the query can select, owned by its bot.**
    ///
    /// ⭐ **This is the whole of what a session lacked.** Every axis the query
    /// has works on it once it has a handle: the kind selects it, the owner
    /// keeps it private, the parent is the walk to the bot, and the chronology
    /// is its prose.
    ///
    /// **The owner is the property and not a field**, asserted here because a
    /// caller can write a key called `owner` on anything, and visibility read
    /// out of the fields would be visibility a caller controls.
    #[test]
    fn a_run_projects_as_an_object_its_own_bot_owns() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let bot = EntityId("bot:gamma".into());
        let session = Session {
            key_hash: None,
            id: SessionId("contract-gamma-run".into()),
            sid: None,
            bot: bot.clone(),
            focus: "reading the roster gate".to_string(),
            started_at: "2026-07-24T09:00:00Z".parse().expect("a timestamp"),
            state: SessionState::Active,
            timezone: None,
            started_on: None,
            served_chars: 0,
            stated_day: None,
            wrap_window: None,
            charter_told_at: None,
            key_trail: Default::default(),
            entries: vec![
                JournalEntry {
                    id: EntryId("e1".into()),
                    at: "2026-07-24T09:05:00Z".parse().expect("a timestamp"),
                    on: None,
                    text: "set out to read the gate".to_string(),
                    touched: None,
                    beat: None,
                    closing_focus: None,
                    closing: false,
                },
                JournalEntry {
                    id: EntryId("e2".into()),
                    at: "2026-07-24T09:30:00Z".parse().expect("a timestamp"),
                    on: None,
                    text: "found the scan reads handles only".to_string(),
                    touched: None,
                    beat: None,
                    closing_focus: None,
                    closing: false,
                },
            ],
        };

        let doc = projected(&session);
        let entity = doc.entity.as_ref().expect("a run is an object");

        assert_eq!(entity.id, EntityId("session:contract-gamma-run".into()));
        assert_eq!(entity.kind, EntityKind::SESSION);
        assert_eq!(
            doc.owner.as_ref(),
            Some(&bot),
            "the run is its bot's, and the owner is a property of the document",
        );
        assert_eq!(
            entity.parent.as_ref(),
            Some(&bot),
            "…and the tree is the walk from a bot to its runs",
        );
        assert!(
            !doc.fields.contains_key("owner"),
            "the owner must not be a field, or a caller could write one and change who reads",
        );
        assert_eq!(doc.fields.get("beats").map(String::as_str), Some("2"));
        assert!(
            doc.prose.contains("set out to read the gate")
                && doc.prose.contains("found the scan reads handles only"),
            "the chronology is the run's prose, whole and in order: {}",
            doc.prose,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemorySessions, contract};
    use super::*;

    /// The full behavioural contract holds for the fake — the same suite the
    /// store's own adapter runs against, so the two answer alike.
    #[tokio::test]
    async fn the_fake_satisfies_the_contract() {
        contract::run_all(InMemorySessions::new).await;
    }

    /// **A change that cannot be read as a compaction counts as none** in every
    /// summary and in the run totals.
    #[tokio::test]
    async fn the_fake_counts_an_unreadable_compaction_as_none() {
        let store = InMemorySessions::new();
        let run = contract::a_run_with_one_compaction(&store).await;
        store.plant_unreadable_reset(&run.id, "not-a-number");
        contract::an_unreadable_compaction_counts_for_nothing(&store, &run).await;
    }

    /// **A second claimant is refused while the lease is fresh** — told no,
    /// and told who holds it and until when.
    #[test]
    fn a_second_claimant_is_refused_while_the_lease_is_fresh() {
        let claimed_at = contract::epoch();
        let now = claimed_at + jiff::SignedDuration::from_secs(30);
        let threshold = jiff::SignedDuration::from_secs(300);
        let decision = claim_role("delta", Some("gamma"), Some(claimed_at), now, threshold);
        assert_eq!(
            decision,
            LeaseClaim::Refused {
                holder: "gamma".into(),
                until: claimed_at + threshold,
            },
            "a fresh holder must not lose the role to a second claimant"
        );
    }

    /// **The same claimant renewing is not refused.** A lease is a claim
    /// against everyone else, not a lock its own holder must fight back into.
    #[test]
    fn the_same_claimant_renewing_is_not_refused() {
        let claimed_at = contract::epoch();
        let now = claimed_at + jiff::SignedDuration::from_secs(30);
        let threshold = jiff::SignedDuration::from_secs(300);
        let decision = claim_role("gamma", Some("gamma"), Some(claimed_at), now, threshold);
        assert_eq!(
            decision,
            LeaseClaim::Taken,
            "the holder renewing its own claim"
        );
    }

    /// **A lease past the threshold reads `stale` and is claimable.**
    #[test]
    fn a_lease_past_the_threshold_reads_stale_and_is_claimable() {
        let claimed_at = contract::epoch();
        let threshold = jiff::SignedDuration::from_secs(300);
        let now = claimed_at + jiff::SignedDuration::from_secs(301);
        assert_eq!(
            lease_verdict(Some(claimed_at), now, threshold),
            LeaseVerdict::Stale
        );
        assert_eq!(
            claim_role("delta", Some("gamma"), Some(claimed_at), now, threshold),
            LeaseClaim::Taken,
            "a stale lease is claimable by anyone"
        );
    }

    /// **A renewal is due at the renewal age and not before it.** The edges
    /// are the case: one second short is not due, the age itself is, and no
    /// stamp at all is.
    #[test]
    fn a_renewal_is_due_at_the_renewal_age_and_not_before() {
        let stamped = contract::epoch();
        let short = LEASE_RENEWS_AFTER - jiff::SignedDuration::from_secs(1);
        assert!(
            !renewal_is_due(Some(stamped), stamped + short),
            "a write one second short of the renewal age rewrote the lease"
        );
        assert!(
            renewal_is_due(Some(stamped), stamped + LEASE_RENEWS_AFTER),
            "a write at the renewal age left the lease alone"
        );
        assert!(
            renewal_is_due(None, stamped),
            "a lease with no stamp is always due"
        );
    }

    /// **A stamp in the future of the write is never due**, so a clock that
    /// stepped back does not rewrite a lease that is still fresh.
    #[test]
    fn a_stamp_in_the_future_of_the_write_is_not_due() {
        let stamped = contract::epoch() + jiff::SignedDuration::from_secs(3600);
        assert!(!renewal_is_due(Some(stamped), contract::epoch()));
    }

    /// **The renewal age is a fraction of the lease, so the lease keeps most
    /// of its length after the last write.** Read off both constants, so a
    /// change to the lease moves the age with it.
    #[test]
    fn the_renewal_age_leaves_most_of_the_lease_after_the_last_write() {
        assert!(LEASE_RENEWS_AFTER.as_secs() > 0);
        assert_eq!(
            LEASE_RENEWS_AFTER.as_secs() * 10,
            LEASE_FRESHNESS.as_secs(),
            "the renewal age is a tenth of the lease"
        );
    }

    /// **A lease inside the threshold reads `held`.**
    #[test]
    fn a_lease_inside_the_threshold_reads_held() {
        let claimed_at = contract::epoch();
        let threshold = jiff::SignedDuration::from_secs(300);
        let now = claimed_at + jiff::SignedDuration::from_secs(299);
        assert_eq!(
            lease_verdict(Some(claimed_at), now, threshold),
            LeaseVerdict::Held
        );
    }

    /// **Nobody has ever claimed it reads `free`, and is claimable.**
    #[test]
    fn an_unclaimed_lease_reads_free_and_is_claimable() {
        let now = contract::epoch();
        let threshold = jiff::SignedDuration::from_secs(300);
        assert_eq!(lease_verdict(None, now, threshold), LeaseVerdict::Free);
        assert_eq!(
            claim_role("gamma", None, None, now, threshold),
            LeaseClaim::Taken
        );
    }

    /// **A key is told from a name by its shape alone, and the hash it is stored
    /// by is pinned by its literal.** A key is the prefix and 26 drawn
    /// characters; a name is 6 drawn characters (or the 4 an older build
    /// minted). Neither is the other. The stored hash is SHA-256 in lowercase
    /// hex with no process key, because a key has to keep working across a
    /// restart: nothing outside this process declares that, and a record
    /// written under another digest could not fail a test.
    #[test]
    fn a_key_is_told_from_a_name_and_its_hash_is_pinned() {
        let key = mint_key();
        assert!(is_key(&key), "{key}");
        assert_eq!(key.len(), 3 + 26, "the prefix and 26 drawn characters");
        assert!(key.starts_with("sk-"));
        assert_ne!(key, mint_key(), "two keys are never the same");
        assert!(!is_run_name(&key) && !is_key("k3m9qz") && !is_key("jm7z"));
        assert!(is_run_name("k3m9qz") && !is_run_name(&key));
        assert!(!is_key(&format!("{key}x")), "a longer value is not a key");
        assert!(!is_key(&key.to_uppercase()), "a key is not corrected");

        assert_eq!(
            hash_key("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hash_key(&key), hash_key(&key), "the hash is deterministic");
        assert!(
            KEY_VOID.len() != 64 && !KEY_VOID.bytes().all(|b| b.is_ascii_hexdigit()),
            "the void sentinel cannot be a hash"
        );
    }

    /// **A second claimant, decided against the CURRENT stored state a
    /// caller supplies — never against anything this write's own fields
    /// say.** This is the shape an atomic check inside a real write has to
    /// hold: `role_claim_in` only ever reads the write's own intent, and
    /// `claim_role` is what a caller feeds the store's live answer into.
    #[test]
    fn a_second_claimant_is_refused_against_the_current_stored_holder() {
        let claimed_at = contract::epoch();
        let now = claimed_at + jiff::SignedDuration::from_secs(10);
        let fields: std::collections::BTreeMap<String, String> = [
            (ROLE_HOLDER.to_string(), "delta".to_string()),
            (ROLE_CLAIMED_AT.to_string(), now.to_string()),
        ]
        .into_iter()
        .collect();
        let (claimant, now) = role_claim_in(&fields).expect("names a holder and a moment");
        let verdict = claim_role(
            &claimant,
            Some("gamma"),
            Some(claimed_at),
            now,
            LEASE_FRESHNESS,
        );
        assert_eq!(
            verdict,
            LeaseClaim::Refused {
                holder: "gamma".to_string(),
                until: claimed_at + LEASE_FRESHNESS,
            }
        );
    }

    /// **A claim that succeeds must be READ BACK as held by the claimant.**
    /// An assertion that the wrong claimant is refused passes identically
    /// when nothing was actually taken — so this chains the successful
    /// claim's own outcome into the next decision, the way a caller who
    /// wrote the claim and then re-read it would.
    #[test]
    fn a_successful_claim_reads_back_as_held_by_the_claimant() {
        let now = contract::epoch();
        let threshold = jiff::SignedDuration::from_secs(300);
        assert_eq!(
            claim_role("gamma", None, None, now, threshold),
            LeaseClaim::Taken,
            "the first claimant takes a free role"
        );
        // What gamma's claim wrote, read back: `gamma` now holds it, claimed
        // at `now`.
        let later = now + jiff::SignedDuration::from_secs(30);
        assert_eq!(
            lease_verdict(Some(now), later, threshold),
            LeaseVerdict::Held,
            "the claim must read back as held, not merely as accepted"
        );
        assert_eq!(
            claim_role("delta", Some("gamma"), Some(now), later, threshold),
            LeaseClaim::Refused {
                holder: "gamma".into(),
                until: now + threshold,
            },
            "and held BY GAMMA specifically — a second claimant is told so"
        );
    }

    /// **The sweep reads a clock it is handed, so its answer is a function of
    /// its arguments.** The boot's whole board decision — what to close, what
    /// is live, what to offer — is decidable at a chosen instant, with no
    /// handler and no wall clock.
    /// A full beat's worth of examples, all from the fictional roster — exactly
    /// [`BEAT_EXAMPLES`] of them, so the cap is exercised by the constant
    /// rather than by a number written twice.
    const ROSTER_EXAMPLES: [&str; BEAT_EXAMPLES] = [
        "person:milhouse",
        "person:alpha",
        "person:beta",
        "person:kappa",
        "person:zenith",
    ];

    /// **Render and parse are inverses, for every class in the table.** That is
    /// the claim `BEAT_CLASSES` is written for — "the phrase is half the parse"
    /// — and until now nothing asserted it directly: it was exercised through a
    /// handler, one class at a time, which is a test of the two classes
    /// somebody happened to use.
    #[test]
    fn every_class_renders_a_tally_that_reads_back_as_itself() {
        for (class, phrase) in BEAT_CLASSES {
            for beat in [
                // A count of one still carries its count — this line is where
                // the tally LIVES, so a reconnect has to be able to read it.
                Beat {
                    entry: EntryId("e1".into()),
                    count: 1,
                    examples: vec!["person:milhouse".into()],
                },
                // Examples capped below the count: the line says so, and the
                // ellipsis must not survive as a phantom example.
                Beat {
                    entry: EntryId("e2".into()),
                    count: 9,
                    examples: ROSTER_EXAMPLES.iter().map(|e| (*e).to_string()).collect(),
                },
            ] {
                let text = beat_text(phrase, &beat);
                let entry = JournalEntry {
                    id: beat.entry.clone(),
                    at: contract::epoch(),
                    on: None,
                    touched: None,
                    beat: Some((*class).to_string()),
                    text: text.clone(),
                    closing_focus: None,
                    closing: false,
                };
                let read = parse_beat(phrase, &entry)
                    .unwrap_or_else(|| panic!("{class} must read back its own line: {text:?}"));
                assert_eq!(read.count, beat.count, "{class}: {text:?}");
                assert_eq!(read.examples, beat.examples, "{class}: {text:?}");
                assert_eq!(read.entry, beat.entry, "{class}: {text:?}");
            }
        }
    }

    /// **Every phrase, written out as a literal.**
    ///
    /// 🚨 **The case above cannot hold these.** It takes the phrase from
    /// [`BEAT_CLASSES`], renders with it and parses with it, so it compares the
    /// table against itself and stays green under any rename. It proves render
    /// and parse are inverses. It proves nothing about the VALUE.
    ///
    /// ⚠️ **And the value is stored text.** A tally is written into a chronology
    /// entry and read back out of one by [`beats_of`] on every reconnect. Rename
    /// a phrase and every line already written under the old one stops parsing:
    /// the class opens a fresh tally, the count restarts at one, and the session
    /// carries two tally lines for one class. **Nothing inside the process can
    /// see that happen** — the store accepts the new spelling, and only records
    /// written before the change disagree.
    ///
    /// So the spelling is pinned here, where a rename has to move a literal.
    /// **A class removed or added moves it too, and that is the case working**:
    /// the table is what the store holds, so a change to it is a decision, not
    /// an edit.
    #[test]
    fn every_stored_phrase_is_pinned_by_its_own_literal() {
        assert_eq!(
            BEAT_CLASSES,
            [
                ("add_entity", "brought entities into being"),
                ("update_entity", "edited entities"),
                ("capture", "captured facts about"),
                ("put_document", "put markdown onto"),
                ("update_fact", "edited facts"),
                ("set_charter", "wrote charters for"),
                ("post_message", "posted to mailboxes"),
                ("mark_processed", "retired messages"),
                ("quarantine", "quarantined messages"),
                ("rename_entity", "renamed entities"),
                ("retract", "took back records"),
                ("merge_entities", "merged entities"),
                ("archive_entity", "archived entities"),
            ],
            "a phrase here is the spelling of text the store already holds, so a \
             change to this table orphans every tally line written under the old one",
        );
    }

    /// A line jojobot did not write is not a tally. Somebody's own words stay
    /// theirs, and the class opens a fresh tally beside them rather than jojobot
    /// rewriting a person's entry into its own format.
    #[test]
    fn a_line_this_did_not_write_is_not_read_as_a_tally() {
        let entry = |text: &str| JournalEntry {
            id: EntryId("e1".into()),
            at: contract::epoch(),
            on: None,
            touched: None,
            beat: Some("capture".into()),
            text: text.to_string(),
            closing_focus: None,
            closing: false,
        };
        for hand_edited in [
            "captured facts about milhouse and a few others",
            "captured facts about: person:milhouse",
            "captured facts about: person:milhouse (lots)",
            "wrote charters for: bot:gamma (1)",
            "",
        ] {
            assert!(
                parse_beat("captured facts about", &entry(hand_edited)).is_none(),
                "must not read {hand_edited:?} as a tally"
            );
        }
    }

    /// **The marker for a run never handed its charter is served as words, and
    /// every real moment is served as it was kept.** Both halves in one case: a
    /// translation of everything, or of nothing, fails one of them.
    #[test]
    fn a_run_never_handed_its_charter_is_told_in_words_and_a_real_moment_is_not() {
        assert_eq!(told_in_words(&NEVER_TOLD.to_string()), "never told");
        let real = Timestamp::from_second(1_790_000_000).expect("a valid instant");
        assert_eq!(told_in_words(&real.to_string()), real.to_string());
        assert_eq!(
            told_in_words("not a moment"),
            "not a moment",
            "a stored value that is no moment is served as kept"
        );
    }

    /// The tally belongs to the SESSION, not to whichever connection is holding
    /// it — so it is read back off the chronology. An entry that is not an
    /// automatic beat is skipped, and so is one whose class is not in the table.
    #[test]
    fn the_tally_is_read_back_off_the_chronology() {
        let entry = |id: &str, beat: Option<&str>, text: &str| JournalEntry {
            id: EntryId(id.into()),
            at: contract::epoch(),
            on: None,
            touched: None,
            beat: beat.map(str::to_string),
            text: text.to_string(),
            closing_focus: None,
            closing: false,
        };
        let session = Session {
            key_hash: None,
            timezone: None,
            started_on: None,
            served_chars: 0,
            stated_day: None,
            wrap_window: None,
            charter_told_at: None,
            key_trail: Default::default(),
            id: SessionId("1".into()),
            sid: Some(Sid("s001".into())),
            bot: EntityId("bot:gamma".into()),
            focus: "working".into(),
            started_at: contract::epoch(),
            state: SessionState::Active,
            entries: vec![
                entry("e1", None, "read the hand-off"),
                entry(
                    "e2",
                    Some("capture"),
                    "captured facts about: person:milhouse (2)",
                ),
                entry("e3", Some("no-such-class"), "did a thing: x (1)"),
                entry("e4", Some("post_message"), "posted to mailboxes: pm (1)"),
            ],
        };

        let found = beats_of(&session);
        assert_eq!(found.len(), 2, "two tallies, not four: {found:?}");
        assert_eq!(found["capture"].count, 2);
        assert_eq!(found["capture"].entry, EntryId("e2".into()));
        assert_eq!(found["post_message"].examples, vec!["pm".to_string()]);
        assert!(
            !found.contains_key("no-such-class"),
            "a class with no phrase renders nothing readable, so it is no tally"
        );
    }

    /// A run of `bot:gamma` that last had something to show for itself
    /// `hours_ago` before [`contract::epoch`].
    async fn run(store: &dyn Sessions, nth: u8, focus: &str, hours_ago: i64) -> Session {
        store
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid(format!("s{nth:03}")),
                focus: focus.to_string(),
                started_at: contract::epoch() - jiff::SignedDuration::from_hours(hours_ago),
                id: None,
                key_hash: None,
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

        let board = sweep_and_find(&store, &gamma, at, None)
            .await
            .expect("a board");

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
        let earlier = sweep_and_find(&store, &gamma, at - ABANDONED_AFTER, None)
            .await
            .expect("a board");
        assert!(
            earlier.swept.is_empty(),
            "nothing is stale before it happens"
        );
    }

    /// **The board read without a sweep decides what a sweeping boot decides,
    /// and closes nothing.** Same three runs as the boot's case: reading the board
    /// leaves the stale ones `active` in the store, while the live, offerable and
    /// handover decisions equal the sweeping boot's. A refusal lists runs from
    /// this read, so a refused call never closes a run.
    #[tokio::test]
    async fn the_board_read_without_a_sweep_decides_as_a_boot_does_and_closes_nothing() {
        let store = InMemorySessions::new();
        let gamma = EntityId("bot:gamma".into());
        let at = contract::epoch();
        let old = run(&store, 1, "the day before yesterday", 36).await;
        let warm = run(&store, 2, "an hour ago", 1).await;
        let _ancient = run(&store, 3, "a fortnight ago", 24 * 15).await;

        let looked = find_without_sweeping(&store, &gamma, at, None)
            .await
            .expect("a board");
        for id in [&old.id, &warm.id] {
            let kept = store.read_session(id).await.expect("the run is there");
            assert_ne!(
                kept.state,
                SessionState::Abandoned,
                "reading the board closed run {id}"
            );
        }
        let swept = sweep_and_find(&store, &gamma, at, None)
            .await
            .expect("a board");
        let names = |runs: &[Session]| runs.iter().map(|s| s.id.to_string()).collect::<Vec<_>>();
        assert_eq!(names(&looked.live), names(&swept.live));
        // The expectation is the fixture's own, not the sweeping read's: the run a
        // day and a half old is the one offered, as the abandoned run a sweep would
        // make it, though nothing closed it.
        assert_eq!(
            looked
                .offerable
                .as_ref()
                .map(|s| (s.id.to_string(), s.state)),
            Some((old.id.to_string(), SessionState::Abandoned)),
        );
        assert_eq!(
            looked
                .offerable
                .as_ref()
                .map(|s| (s.id.to_string(), s.state)),
            swept
                .offerable
                .as_ref()
                .map(|s| (s.id.to_string(), s.state)),
            "the stale run is offered as the abandoned one it would be"
        );
        assert_eq!(looked.windows_open, swept.windows_open);
    }

    /// A session the store refuses to close does not stop a boot: it is left
    /// active for the next one and reported, so the caller — which owns the log
    /// — can say so. The domain names what happened; it does not log.
    #[tokio::test]
    async fn a_session_that_will_not_close_is_reported_and_left_active() {
        struct RefusesToClose(InMemorySessions);

        #[crate::forward(forward_sessions, via = 0)]
        impl Sessions for RefusesToClose {
            async fn close(&self, _: &SessionId, _: SessionState) -> Result<Session, SessionError> {
                Err(SessionError::Store("the board said no".into()))
            }
            async fn close_displaced(
                &self,
                _: &SessionId,
                _: &Sid,
            ) -> Result<Session, SessionError> {
                Err(SessionError::Store("the board said no".into()))
            }
        }

        let store = RefusesToClose(InMemorySessions::new());
        let gamma = EntityId("bot:gamma".into());
        let stale = run(&store.0, 1, "yesterday's work", 36).await;

        let board = sweep_and_find(&store, &gamma, contract::epoch(), None)
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
        let refused = validate_focus(&"x".repeat(201)).expect_err("and it is capped");
        let said = refused.to_string();
        assert!(
            said.contains("200"),
            "the refusal must name the limit a caller has to write under: {said}"
        );
    }

    /// 🚨 **The sweep answers in the day the caller states, and only falls back
    /// to the clock when nobody stated one.**
    ///
    /// A run acting out months finishes in real minutes, so measured against a
    /// clock nothing it left is ever stale and every sitting after the first
    /// meets a resume-or-new choice it should never see. A session catching up
    /// on last week, and an instance restored from a backup, meet the same
    /// wrong answer — the frame belongs to the caller here exactly as it does
    /// for the day a capture gets.
    ///
    /// **Both halves in one case.** A run whose newest stated day is months
    /// back is stale; one whose newest stated day is yesterday is not, because
    /// a session that goes quiet overnight is still that night's work. Without
    /// the second half this passes identically against a sweep that closes
    /// everything.
    #[test]
    fn staleness_is_measured_in_the_day_the_caller_states() {
        let start = Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        let day = |text: &str| text.parse::<Date>().expect("a date");
        let run = |on: Date| Session {
            key_hash: None,
            timezone: None,
            started_on: Some(on),
            served_chars: 0,
            stated_day: None,
            wrap_window: None,
            charter_told_at: None,
            key_trail: Default::default(),
            id: SessionId("1".into()),
            sid: Some(Sid("s001".into())),
            bot: EntityId("bot:gamma".into()),
            focus: "nothing yet".into(),
            started_at: start,
            state: SessionState::Active,
            entries: vec![JournalEntry {
                id: EntryId("e1".into()),
                at: start,
                on: Some(on),
                text: "did a thing".into(),
                touched: None,
                beat: None,
                closing_focus: None,
                closing: false,
            }],
        };

        let today = day("2026-09-13");
        assert!(
            run(day("2026-03-15")).is_stale(start, Some(today)),
            "a run whose newest stated day is months back is not being swept, so a year acted \
             out in minutes offers every sitting it ever had",
        );
        assert!(
            !run(day("2026-09-12")).is_stale(start, Some(today)),
            "a run that went quiet overnight was swept — that night's work is still the work, \
             and this case would pass on a sweep that closed everything",
        );
        assert!(
            !run(today).is_stale(start, Some(today)),
            "a run from the caller's own day was swept",
        );

        // **The caller who states nothing gets exactly what it got before.**
        let quiet = run(day("2026-03-15"));
        assert!(
            !quiet.is_stale(start, None),
            "a caller that stated no day had the clock answer differently from before",
        );
        assert!(
            quiet.is_stale(start + ABANDONED_AFTER, None),
            "…and the clock still sweeps when nobody states a frame",
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
            key_hash: None,
            timezone: None,
            started_on: None,
            served_chars: 0,
            stated_day: None,
            wrap_window: None,
            charter_told_at: None,
            key_trail: Default::default(),
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
        assert!(!bare.is_stale(start + jiff::SignedDuration::from_hours(23), None));
        assert!(
            bare.is_stale(start + ABANDONED_AFTER, None),
            "the threshold is inclusive"
        );

        let hour = jiff::SignedDuration::from_hours(1);
        let busy = Session {
            entries: vec![JournalEntry {
                id: EntryId("e1".into()),
                at: start + hour,
                on: None,
                text: "did a thing".into(),
                touched: None,
                beat: None,
                closing_focus: None,
                closing: false,
            }],
            ..bare.clone()
        };
        assert_eq!(
            busy.last_beat(),
            start + hour,
            "an entry moves the clock forward"
        );
        assert!(
            !busy.is_stale(start + ABANDONED_AFTER, None),
            "…so the same instant that swept the bare one leaves this one alone"
        );

        // A closed session is never swept: it is already at an end.
        let closed = Session {
            state: SessionState::Wrapped,
            ..bare
        };
        assert!(!closed.is_stale(start + ABANDONED_AFTER + hour, None));
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

    /// **A window answers to the code it was made for, whether it stores the
    /// hash or, from an older build, the code itself.** A stored hash is not a
    /// code: sent as one it opens nothing, and a window never answers to a code
    /// of another window.
    #[test]
    fn a_window_answers_to_its_code_stored_hashed_or_plain_and_to_nothing_else() {
        let code = mint_wrap_code();
        let other = mint_wrap_code();
        let hashed = WrapWindow::Offered(hash_wrap_code(&code));
        let plain = WrapWindow::Offered(code.clone());
        for window in [&hashed, &plain] {
            assert!(window.answers_to(&code), "{window:?}");
            assert!(!window.answers_to(&other), "{window:?}");
        }
        assert!(
            !hashed.answers_to(&hash_wrap_code(&code)),
            "a stored hash was accepted as the code"
        );
        // Both shapes survive being stored and read back.
        for window in [&hashed, &plain] {
            assert_eq!(
                WrapWindow::from_stored(&window.to_stored()).as_ref(),
                Some(window)
            );
        }
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
            key_hash: None,
            timezone: None,
            started_on: None,
            served_chars: 0,
            stated_day: None,
            wrap_window: None,
            charter_told_at: None,
            key_trail: Default::default(),
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
            run.is_offerable(start + day, None),
            "yesterday's run is the one to offer"
        );
        assert!(
            run.is_offerable(start + OFFER_ABANDONED_WITHIN - day, None),
            "…and so is one from inside the window"
        );
        assert!(
            !run.is_offerable(start + OFFER_ABANDONED_WITHIN, None),
            "the bound is exclusive at the edge"
        );
        assert!(
            !run.is_offerable(start + OFFER_ABANDONED_WITHIN + day * 60, None),
            "a run from two months ago is not something to bring up"
        );

        // The other two states are never offered, whatever their age.
        for state in [SessionState::Active, SessionState::Wrapped] {
            let other = Session {
                state,
                ..run.clone()
            };
            assert!(
                !other.is_offerable(start + day, None),
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

    /// **The clock ends a window `WRAP_WINDOW_HOURS` after the wrap, and the edge
    /// is exclusive of the last second.** A window an older build left, hashed or
    /// plain, open or offered, is judged by the same moment. A run with no window,
    /// and a run that is not wrapped, are never lapsed.
    #[test]
    fn a_window_lapses_two_hours_after_the_wrap_and_not_a_second_before() {
        let wrapped = Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        let run = Session {
            key_hash: None,
            timezone: None,
            started_on: None,
            served_chars: 0,
            stated_day: None,
            wrap_window: Some(WrapWindow::Offered(hash_wrap_code("wc-fixture"))),
            charter_told_at: None,
            key_trail: Default::default(),
            id: SessionId("1".into()),
            sid: Some(Sid("s001".into())),
            bot: EntityId("bot:gamma".into()),
            focus: "told its story".into(),
            started_at: wrapped,
            state: SessionState::Wrapped,
            entries: Vec::new(),
        };
        let two_hours = jiff::SignedDuration::from_hours(WRAP_WINDOW_HOURS);
        let second = jiff::SignedDuration::from_secs(1);

        assert_eq!(run.window_ends_at(), Some(wrapped + two_hours));
        assert!(!run.window_lapsed_at(wrapped + two_hours - second));
        assert!(run.window_lapsed_at(wrapped + two_hours));
        for window in [
            WrapWindow::Open(hash_wrap_code("wc-fixture")),
            WrapWindow::Offered("wc-fixture".into()),
            WrapWindow::Open("wc-fixture".into()),
        ] {
            let other = Session {
                wrap_window: Some(window),
                ..run.clone()
            };
            assert!(other.window_lapsed_at(wrapped + two_hours), "{other:?}");
            assert!(!other.window_lapsed_at(wrapped + two_hours - second));
        }
        let none = Session {
            wrap_window: None,
            ..run.clone()
        };
        assert_eq!(none.window_ends_at(), None);
        assert!(!none.window_lapsed_at(wrapped + two_hours * 50));
        let live = Session {
            state: SessionState::Active,
            ..run
        };
        assert!(!live.window_lapsed_at(wrapped + two_hours * 50));
    }

    /// An automatic beat is marked apart from a session's own words, so a
    /// reader can tell an account of intent from a tally of calls.
    #[test]
    fn an_automatic_beat_is_distinguishable_from_a_session_s_own_entry() {
        let at = Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        let manual = NewEntry::manual("read the task", at, None);
        let auto = NewEntry::beat("capture", "captured facts: person:milhouse", at, None);
        assert_eq!(manual.beat, None);
        assert_eq!(auto.beat.as_deref(), Some("capture"));

        let entry = |new: NewEntry| JournalEntry {
            id: EntryId("e1".into()),
            at: new.at,
            on: None,
            text: new.text,
            touched: None,
            beat: new.beat,
            closing_focus: new.closing_focus,
            closing: new.closing,
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

    fn fields_of(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// **A role object that carries a claim moment is the whole truth about the
    /// role, even when it carries no holder.** A released role keeps its moment
    /// and loses its holder.
    #[test]
    fn a_role_object_with_a_claim_moment_answers_for_the_lease() {
        let at = contract::epoch();
        let released = fields_of(&[("claimed_at", &at.to_string())]);
        let state = role_state(Some(&released));
        assert_eq!(state.holder, None, "a release is a role without a holder");
        assert_eq!(state.claimed_at, Some(at));

        let held = fields_of(&[("holder", "delta"), ("claimed_at", &at.to_string())]);
        let state = role_state(Some(&held));
        assert_eq!(state.holder.as_deref(), Some("delta"), "{state:?}");
        assert_eq!(state.claimed_at, Some(at));
    }

    /// **A role with no object, or an object with no moment, says nothing about
    /// the lease.** The keys a bot once held for the role are nobody's answer.
    #[test]
    fn a_role_with_no_claim_moment_is_held_by_nobody() {
        let momentless = fields_of(&[("holder", "gamma"), ("agent", "an-agent-id")]);
        for child in [None, Some(&momentless), Some(&fields_of(&[]))] {
            assert_eq!(role_state(child), RoleState::default(), "{child:?}");
        }
    }

    /// **A claim on a role object is read off its own two keys**, and a write
    /// that names only one of them is not a claim.
    #[test]
    fn a_claim_on_a_role_object_names_a_holder_and_a_moment() {
        let now = contract::epoch();
        let both = fields_of(&[("holder", "gamma"), ("claimed_at", &now.to_string())]);
        assert_eq!(role_claim_in(&both), Some(("gamma".to_string(), now)));
        assert_eq!(role_claim_in(&fields_of(&[("holder", "gamma")])), None);
        assert_eq!(
            role_claim_in(&fields_of(&[("claimed_at", &now.to_string())])),
            None
        );
        assert_eq!(
            role_claim_in(&fields_of(&[
                ("holder", "gamma"),
                ("claimed_at", "yesterday")
            ])),
            None,
            "a moment that is no moment is not a claim"
        );
    }
}

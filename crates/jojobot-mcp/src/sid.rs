//! The **run registry** — who holds which key, and what each run is called.
//!
//! **A run has two values and they are not interchangeable.** Its public NAME is
//! six characters, minted at the boot, and it is the only thing records, mail
//! and answers ever carry about the run. Its KEY is the secret the boot hands
//! the run: 26 drawn characters behind a fixed prefix, sent in `sid` on every
//! call, and the only credential. A name read off a role's holder names a run
//! and cannot act as it.
//!
//! Four properties, each load-bearing:
//!
//! * **opaque** — neither value says anything about what the run is working on;
//! * **hard to guess** — the key is 130 bits of OS entropy, and is stored only
//!   as its SHA-256, so a read of the store cannot produce one;
//! * **hard to confuse** — the alphabet excludes the glyphs that read as one
//!   another, so a name copied by eye survives the trip;
//! * **checked before anything is allocated** — a call carrying a key finds the
//!   run by the key's hash, and a value that is no key allocates nothing.
//!
//! **The registry is process-wide, never per connection.** The transport builds
//! one handler per MCP session and most clients open a fresh one per tool call,
//! so a map living on the handler would forget every run it ever issued.
//!
//! **A restart empties it, and the board fills it back**: every run's name and
//! key hash are written on the card the run created, so
//! [`SessionRegistry::rebuild_from`] recovers them at startup. What does NOT
//! come back is a run that never wrote a card — there is nothing on the board
//! carrying it — and that one is *gone*: blocked, never silently swapped for a
//! new session.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{
    KEY_VOID, RUN_LEN, Session, SessionId, hash_key, is_readable_sid, mint_key,
};

// The name type, its alphabet and its shape rule are the domain's — a session
// answers to it, and the store persists it. Re-exported so callers reach one
// vocabulary rather than two names for one thing.
pub use jojobot_domain::session::{Sid, is_readable_sid as is_readable};

/// The answer that chooses a fresh session over any of the resumable ones.
/// Deliberately a word rather than a name: it is not a session, and giving it
/// a name would mean minting one for a session that may never be written.
pub const NEW: &str = "new";

/// How many collisions minting will ride out before giving up.
///
/// Reaching it means the registry holds a large fraction of a billion live
/// names, which is not a state this server gets into — but "unreachable" is
/// not a thing to write an `expect` about when the alternative is one honest
/// error the door can report.
const MINT_ATTEMPTS: usize = 64;

/// Draw one candidate name — [`jojobot_domain::handle::draw`] at the length of
/// a run's name. The draw itself is shared with every other drawn id, so there
/// is one alphabet, one entropy source and one place either can change.
pub fn draw() -> String {
    jojobot_domain::handle::draw(RUN_LEN)
}

/// **Whether a run holds a key**, and which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyState {
    /// The SHA-256 of the key the run holds.
    Held(String),
    /// The run's key was taken away (it wrapped) and nothing replaced it.
    Void,
    /// A run that began before keys existed: it holds none, and its old
    /// four-character name is exchanged for a key once.
    Legacy,
}

impl KeyState {
    /// The state a stored `key_hash` says: a hash, the word that voids one, or
    /// nothing at all.
    pub fn from_stored(stored: Option<&str>) -> Self {
        match stored {
            None => KeyState::Legacy,
            Some(KEY_VOID) => KeyState::Void,
            Some(hash) => KeyState::Held(hash.to_string()),
        }
    }

    /// The text the card stores this as.
    pub fn to_stored(&self) -> Option<String> {
        match self {
            KeyState::Held(hash) => Some(hash.clone()),
            KeyState::Void => Some(KEY_VOID.to_string()),
            KeyState::Legacy => None,
        }
    }
}

/// What a run is: whose it is, what it is called, and the card it landed on —
/// `None` until the first real write materializes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handle {
    /// The identity this session is one run of. **A session is bound to it at
    /// boot and never switches**, so this is what refuses somebody else's run.
    pub(crate) bot: EntityId,
    /// **The run's public name.** What every record, answer and refusal calls
    /// the run. It is never a credential.
    pub(crate) name: Sid,
    /// The store's card, once there is one. For a run begun with a name minted
    /// at its boot the card's id is the name; for a run of the older shape it
    /// is an id the store minted.
    pub(crate) card: Option<SessionId>,
    /// Whether the run holds a key, and which.
    pub(crate) key: KeyState,
    /// **The zone this run resolves days against**, as the IANA name the caller
    /// supplied at the door, or nothing when it supplied none.
    ///
    /// Held here as well as on the card because the card is lazy: a run that has
    /// only read has no row yet, and its reads are day-grained too.
    pub(crate) zone: Option<String>,
    /// **The day this run says it is in**, when it stated one. Carried beside
    /// the zone for the same reason: a run outlives a restart, and the sweep
    /// reads this frame rather than the server's clock.
    pub(crate) day: Option<jiff::civil::Date>,
    /// **When this run's old name was exchanged for a key**, if it was, on the
    /// server's clock. In memory only: it exists so the sitting that still
    /// sends the old name is told why it is refused.
    pub(crate) exchanged_at: Option<jiff::Timestamp>,
    /// **When another sitting last took this run over**, on the server's clock.
    /// A pick-up writes a key and no chronology entry, so without this the run
    /// would still read as quiet to the next asker, and two asks made together
    /// would both be handed a key. In memory only: after a restart the run is
    /// judged by its last entry, which the sitting that took it writes next.
    pub(crate) taken_at: Option<jiff::Timestamp>,
}

/// What [`SessionRegistry::rebuild_from`] found on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rebuilt {
    /// How many runs were put back.
    pub recovered: usize,
    /// **How many cards carried a name that failed the shape check.** Held
    /// apart from `recovered` rather than folded into the gap a missing name
    /// leaves, because the two say different things: a card with no name is
    /// the ordinary pre-handle case, and a card with an unreadable one is
    /// damage a restart should be able to say out loud.
    pub unreadable: usize,
}

/// Minting found no free name. See [`MINT_ATTEMPTS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoFreeHandle;

impl std::fmt::Display for NoFreeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no free session name could be minted")
    }
}

/// **Why a key stopped opening its run.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetiredBy {
    /// Another session resumed the run and was handed a new key.
    Resumed,
    /// The run wrapped, which took its key away.
    Wrapped,
    /// The run's old name was exchanged for a key, once.
    Exchanged,
}

/// **What the registry remembers about a key that no longer opens its run**, so
/// the sitting that held it is told why rather than that it never existed. Held
/// in memory only: a restart forgets it, and the key then reads as unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retired {
    /// The run it was the key of, by its public name.
    pub name: Sid,
    /// The identity that run belongs to.
    pub bot: EntityId,
    /// When it stopped opening the run, on the server's clock.
    pub at: jiff::Timestamp,
    /// Why it stopped.
    pub by: RetiredBy,
}

/// How many retired keys the registry remembers. The oldest are forgotten
/// first, so a loop of resumes cannot grow it without bound.
const RETIRED_KEPT: usize = 1024;

/// **A run that has just been minted**: its public name and the key only it
/// holds. The key is handed to the caller once and kept nowhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minted {
    /// The run's public name.
    pub name: Sid,
    /// The run's key. Shown once, in the boot's answer.
    pub key: String,
}

#[cfg(test)]
impl Minted {
    /// **The key**, for the tests that call a verb as the run they minted.
    pub(crate) fn as_str(&self) -> &str {
        &self.key
    }
}

#[cfg(test)]
impl std::fmt::Display for Minted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key)
    }
}

/// The two maps the registry holds runs in. **One writer lock covers both**, so
/// a name and the hash that finds it can never disagree.
#[derive(Debug, Default)]
struct Held {
    /// Every run this process knows, by its public name.
    by_name: HashMap<String, Handle>,
    /// The name of the run each key hash opens.
    by_hash: HashMap<String, String>,
    /// Keys that stopped opening their run, by hash, and the order they did.
    retired: HashMap<String, Retired>,
    retired_order: std::collections::VecDeque<String>,
}

impl Held {
    /// Set a run's key state, keeping the hash index in step with it. The key
    /// it replaces is remembered as retired when a reason is given.
    fn set_key(&mut self, name: &str, key: KeyState, retire: Option<(jiff::Timestamp, RetiredBy)>) {
        let Some(handle) = self.by_name.get_mut(name) else {
            return;
        };
        if let KeyState::Held(old) = &handle.key {
            self.by_hash.remove(old);
            if let Some((at, by)) = retire {
                if self.retired_order.len() >= RETIRED_KEPT
                    && let Some(oldest) = self.retired_order.pop_front()
                {
                    self.retired.remove(&oldest);
                }
                self.retired_order.push_back(old.clone());
                self.retired.insert(
                    old.clone(),
                    Retired {
                        name: handle.name.clone(),
                        bot: handle.bot.clone(),
                        at,
                        by,
                    },
                );
            }
        }
        if let (KeyState::Held(_), Some((at, RetiredBy::Resumed))) = (&handle.key, &retire) {
            handle.taken_at = Some(*at);
        }
        if let KeyState::Held(new) = &key {
            self.by_hash.insert(new.clone(), name.to_string());
        }
        handle.key = key;
    }
}

/// Every run this process holds — **and the locks that serialize the callers
/// using them.**
///
/// The two live together because they answer to the same key. A run is what two
/// callers writing to one session have in common, so a lock anywhere else (on
/// the handler, on the store) either excludes too much or, as the per-handler
/// gate did, excludes nothing at all.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    held: RwLock<Held>,
    /// One mutex per key in flight, **keyed by the bot's handle**: a boot
    /// resolves a whole identity's board and a write resolves one run of it, and
    /// the identity is the only name both of them hold. Keyed any more narrowly
    /// the pair that must agree ends up in two queues.
    ///
    /// **Entries are never removed, and none is ever made for a value that has
    /// not proved itself.** A key here is a bot id, or the empty string for a
    /// call whose key opens nothing, so the map is bounded by how many
    /// identities this operator has and a stranger cannot grow it.
    gates: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// **What each session was shown by the read verbs**, kept here because this
    /// is the one thing the process already shares across connections, keyed by
    /// the run's public name. Nothing in it is stored: a restart empties it. See
    /// [`crate::seen`].
    seen: std::sync::Mutex<crate::seen::Ledger>,
    /// **How many starred rules each session has unstarred, per bot**, kept here
    /// beside the runs it is keyed by: both are process-local and both are
    /// gone when the process is. Nothing in it is stored. See
    /// [`SessionRegistry::note_unstar`].
    unstars: std::sync::Mutex<HashMap<(String, String), usize>>,
    /// **When each session was last handed its bot's charter**, on the server's
    /// clock: at the boot that minted it and at every read of the charter. It is
    /// what a charter change is compared against, so the notice ends when the
    /// session has read the text. Process-local: a restart empties it, and a
    /// session this process did not mint is compared from its card's start. A
    /// session that already read the change is then told once more, which is a
    /// repeat and never a miss.
    charter_told: std::sync::Mutex<HashMap<String, jiff::Timestamp>>,
    /// **The key a challenge's code is hashed with**, drawn when this registry is
    /// made and never stored, so a restart voids every outstanding challenge.
    /// Here because the server builds a handler per connection and this is the
    /// one thing they share. See [`jojobot_domain::challenge`].
    challenge_key: ChallengeKey,
    /// **One code is redeemed at a time.** A coded boot holds it from the check
    /// of the code to the spending of it, so two boots presenting one code cannot
    /// both be given a voucher.
    redeem_lock: tokio::sync::Mutex<()>,
}

/// The key a code is hashed with. Drawn from OS entropy when made, and its
/// `Debug` says nothing of it, so a log line holding a registry holds no key.
struct ChallengeKey(Vec<u8>);

impl Default for ChallengeKey {
    fn default() -> Self {
        ChallengeKey(jojobot_domain::handle::draw(32).into_bytes())
    }
}

impl std::fmt::Debug for ChallengeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChallengeKey(..)")
    }
}

impl SessionRegistry {
    /// The key a code is hashed with — see [`SessionRegistry::challenge_key`]'s field.
    pub(crate) fn challenge_key(&self) -> &[u8] {
        &self.challenge_key.0
    }

    /// The lock a coded boot holds from checking the code to spending it.
    pub(crate) fn redeem_lock(&self) -> &tokio::sync::Mutex<()> {
        &self.redeem_lock
    }

    /// An empty registry — one per process.
    pub fn new() -> Self {
        Self::default()
    }

    /// Count one call this run made, so a thing shown can say how many calls
    /// ago it was. **Keyed by the run's public name**, and a name the registry
    /// does not hold is not counted: nothing is allocated for a stranger.
    pub(crate) fn note_call(&self, name: &str) {
        if !self.holds(name) {
            return;
        }
        self.seen
            .lock()
            .expect("the seen ledger is poisoned")
            .tick(name);
    }

    /// Record that this run was handed its charter at `at`.
    pub(crate) fn note_charter_told(&self, name: &str, at: jiff::Timestamp) {
        self.charter_told
            .lock()
            .expect("the charter ledger is poisoned")
            .insert(name.to_string(), at);
    }

    /// When this process last handed the run its charter, if it did.
    pub(crate) fn charter_told_at(&self, name: &str) -> Option<jiff::Timestamp> {
        self.charter_told
            .lock()
            .expect("the charter ledger is poisoned")
            .get(name)
            .copied()
    }

    /// Count one starred rule of `bot` that this run unstarred.
    pub(crate) fn note_unstar(&self, name: &str, bot: &str) {
        *self
            .unstars
            .lock()
            .expect("the unstar ledger is poisoned")
            .entry((name.to_string(), bot.to_string()))
            .or_default() += 1;
    }

    /// How many starred rules of `bot` this run has unstarred.
    pub(crate) fn unstarred_by(&self, name: &str, bot: &str) -> usize {
        self.unstars
            .lock()
            .expect("the unstar ledger is poisoned")
            .get(&(name.to_string(), bot.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Remember the things a read answer returned.
    pub(crate) fn note_shown(&self, name: &str, body: &serde_json::Value) {
        let returned = crate::seen::entities_in(body);
        if returned.is_empty() || !self.holds(name) {
            return;
        }
        self.seen
            .lock()
            .expect("the seen ledger is poisoned")
            .record(name, returned);
    }

    /// Drop a merged-away handle from what every session was shown.
    pub(crate) fn note_merged(&self, handle: &str) {
        self.seen
            .lock()
            .expect("the seen ledger is poisoned")
            .forget(handle);
    }

    /// Ask, once, about the thing a creation sits beside. See [`crate::seen`].
    pub(crate) fn consult_creation(
        &self,
        name: &str,
        handle: &str,
        kind: &str,
        entity_name: &str,
        aliases: &[String],
        parent: Option<&str>,
    ) -> Option<crate::seen::Noticed> {
        self.seen
            .lock()
            .expect("the seen ledger is poisoned")
            .consult(name, handle, kind, entity_name, aliases, parent)
    }

    /// **The lock a caller takes before resolving and writing a session**, keyed
    /// by whatever names that session for this call.
    ///
    /// Returns the mutex rather than a guard, because the caller has to hold it
    /// across awaits and `.lock().await` on a returned guard would drop it at
    /// the end of this expression.
    ///
    /// The span it protects is read-the-board → decide → write-the-card. Two
    /// callers inside it at once both see no live run and both begin one, and
    /// the loser's chronology is orphaned on a card nothing will ever resolve to
    /// again — a session whose story cannot be told, by construction.
    pub fn gate(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.gates
            .lock()
            .expect("the gate map is poisoned")
            .entry(key.to_string())
            .or_default()
            .clone()
    }

    /// How many gate slots the registry holds. Test only: it is what shows that a
    /// value which opens nothing leaves no slot behind.
    #[cfg(test)]
    pub(crate) fn gates_held(&self) -> usize {
        self.gates.lock().expect("the gate map is poisoned").len()
    }

    /// Whether the registry holds a run called `name`.
    pub fn holds(&self, name: &str) -> bool {
        self.held
            .read()
            .expect("the registry is poisoned")
            .by_name
            .contains_key(name)
    }

    /// **Mint a run for `bot`**: its public name and the key only it holds. The
    /// name is drawn at the boot, before any card exists, and is also the id of
    /// the run's card once the run writes.
    pub fn mint(&self, bot: &EntityId) -> Result<Minted, NoFreeHandle> {
        self.mint_with(bot, draw, mint_key)
    }

    /// The same, over caller-supplied draws — **so the collision path is
    /// testable.** A retry nobody has watched happen is a retry nobody knows
    /// works, and it is not a path entropy will produce on demand.
    pub fn mint_with(
        &self,
        bot: &EntityId,
        mut draw_name: impl FnMut() -> String,
        mut draw_key: impl FnMut() -> String,
    ) -> Result<Minted, NoFreeHandle> {
        let mut held = self.held.write().expect("the registry is poisoned");
        for _ in 0..MINT_ATTEMPTS {
            let candidate = draw_name();
            // **Against every run this process has issued, not only the live
            // ones.** A name whose session was wrapped an hour ago is still an
            // answer a caller may be holding, and re-issuing it would point a
            // record's citation at somebody else's session.
            if held.by_name.contains_key(&candidate) {
                continue;
            }
            let key = draw_key();
            let hash = hash_key(&key);
            if held.by_hash.contains_key(&hash) {
                continue;
            }
            let name = Sid(candidate.clone());
            held.by_name.insert(
                candidate.clone(),
                Handle {
                    bot: bot.clone(),
                    name: name.clone(),
                    card: None,
                    key: KeyState::Held(hash.clone()),
                    zone: None,
                    day: None,
                    exchanged_at: None,
                    taken_at: None,
                },
            );
            held.by_hash.insert(hash, candidate);
            return Ok(Minted { name, key });
        }
        Err(NoFreeHandle)
    }

    /// A run already on a card, for the tests that need to name one particular
    /// run: what a restart rebuilds off the board.
    #[cfg(test)]
    pub fn mint_on_card(&self, bot: &EntityId, card: SessionId) -> Minted {
        let run = self.mint(bot).expect("a free name");
        self.attach_card(&run.name, card);
        run
    }

    /// **The run a key opens**, or `None` for a key the registry is not holding.
    /// The key is hashed and never compared as itself.
    pub fn lookup_key(&self, key: &str) -> Option<Handle> {
        let hash = hash_key(key);
        let held = self.held.read().expect("the registry is poisoned");
        let name = held.by_hash.get(&hash)?;
        held.by_name.get(name).cloned()
    }

    /// What this name addresses, or `None` for one this registry is not holding
    /// — a typo, or a run that never wrote a card for a restart to rebuild it
    /// from. **A name is not a credential**: this is for the callers that
    /// already know which run they mean.
    pub fn lookup_name(&self, name: &str) -> Option<Handle> {
        self.held
            .read()
            .expect("the registry is poisoned")
            .by_name
            .get(name)
            .cloned()
    }

    /// **The name the run on this card goes by**, if the registry holds it.
    pub fn name_of_card(&self, card: &SessionId) -> Option<Sid> {
        self.held
            .read()
            .expect("the registry is poisoned")
            .by_name
            .values()
            .find(|h| h.card.as_ref() == Some(card))
            .map(|h| h.name.clone())
    }

    /// **Exchange a run's old name for a key, once.** Only a run that holds
    /// none (`Legacy`) is exchanged, under the registry's own lock, so two
    /// callers sending the same old name cannot both be handed a key. Returns
    /// the run, or `None` when the run is not held or already holds one.
    pub fn take_legacy(&self, name: &str, hash: String, at: jiff::Timestamp) -> Option<Handle> {
        let mut held = self.held.write().expect("the registry is poisoned");
        let handle = held.by_name.get_mut(name)?;
        if handle.key != KeyState::Legacy {
            return None;
        }
        handle.key = KeyState::Held(hash.clone());
        handle.exchanged_at = Some(at);
        let taken = handle.clone();
        held.by_hash.insert(hash, name.to_string());
        Some(taken)
    }

    /// **Undo an exchange the store refused**, so the run can be exchanged
    /// again. Only the moment `take_legacy` took is undone.
    pub fn put_back_legacy(&self, name: &str, hash: &str) {
        let mut held = self.held.write().expect("the registry is poisoned");
        if let Some(handle) = held.by_name.get_mut(name) {
            handle.key = KeyState::Legacy;
            handle.exchanged_at = None;
        }
        held.by_hash.remove(hash);
    }

    /// **Set what key a run holds**, replacing whatever it held. The caller has
    /// already written the card, so a restart cannot bring the old key back.
    /// `retire` is when and why the key it replaces stopped opening the run, so
    /// the sitting that still holds it is told.
    pub fn set_key(&self, name: &Sid, key: KeyState, retire: Option<(jiff::Timestamp, RetiredBy)>) {
        self.held
            .write()
            .expect("the registry is poisoned")
            .set_key(name.as_str(), key, retire);
    }

    /// **What is known of a key that no longer opens its run**, if it ever did
    /// in this process.
    pub fn retired(&self, key: &str) -> Option<Retired> {
        self.held
            .read()
            .expect("the registry is poisoned")
            .retired
            .get(&hash_key(key))
            .cloned()
    }

    /// **Take one run off the board into the registry**, unless the registry
    /// already holds a run by that name. What a restart does for every card,
    /// done for the one card a boot is looking at: a card some other writer
    /// began is still a run of this bot, and its name is public.
    pub fn adopt(&self, session: &Session) {
        let name = session
            .sid
            .clone()
            .unwrap_or_else(|| Sid(session.id.as_str().to_string()));
        if self.holds(name.as_str()) {
            return;
        }
        self.rebuild_from(std::slice::from_ref(session));
    }

    /// **Rebuild from the board** — every run a card carries, put back.
    ///
    /// Called once at startup, eagerly, before the first request: a registry
    /// filled lazily on the first miss would answer differently for the first
    /// caller after a restart than for the second, which is the kind of
    /// difference nobody can reproduce.
    ///
    /// A card written before names were persisted carries none and simply
    /// contributes nothing here. **A card whose name fails the shape check is a
    /// different thing and is counted separately** — see [`Rebuilt::unreadable`].
    /// Nothing this process writes produces one:
    /// `jojobot_adapters::dolt::sessions` proves the column round-trips
    /// unvalidated, so its only source is damage on the board, never the
    /// ordinary pre-handle gap.
    pub fn rebuild_from(&self, sessions: &[Session]) -> Rebuilt {
        let mut held = self.held.write().expect("the registry is poisoned");
        let mut unreadable = 0;
        for session in sessions {
            // **A card written before names were kept goes by its own id**, which
            // is a drawn id of the same shape, and holds no key.
            let name = session
                .sid
                .clone()
                .unwrap_or_else(|| Sid(session.id.as_str().to_string()));
            if !is_readable_sid(name.as_str()) {
                unreadable += 1;
                continue;
            }
            let key = KeyState::from_stored(session.key_hash.as_deref());
            if let KeyState::Held(hash) = &key {
                held.by_hash.insert(hash.clone(), name.0.clone());
            }
            held.by_name.insert(
                name.0.clone(),
                Handle {
                    bot: session.bot.clone(),
                    name,
                    card: Some(session.id.clone()),
                    key,
                    // **Off the card, so a restart does not silently reframe a
                    // run that survived it.** The registry is rebuilt from the
                    // board; a zone kept only in the process would be gone
                    // while the run it belongs to is still being worked.
                    zone: session.timezone.clone(),
                    // **`stated_day` first, `started_on` the fallback** — a
                    // resume that moved the day writes the former and leaves
                    // the latter as the creation day it always was.
                    day: session.stated_day.or(session.started_on),
                    exchanged_at: None,
                    taken_at: None,
                },
            );
        }
        Rebuilt {
            recovered: held.by_name.len(),
            unreadable,
        }
    }

    /// **Record the zone this run resolves days against**, replacing whatever it
    /// carried.
    ///
    /// The door calls this on every boot that supplies one, resume included: a
    /// run outlives a device hop, so the zone it was born in is not always the
    /// zone it is being worked in.
    pub fn set_zone(&self, name: &Sid, zone: Option<String>) {
        if let Some(handle) = self
            .held
            .write()
            .expect("the registry is poisoned")
            .by_name
            .get_mut(name.as_str())
        {
            handle.zone = zone;
        }
    }

    /// **Record the day this run says it is in**, replacing whatever it held.
    ///
    /// Called on every boot that supplies one, resume included, for the reason
    /// the zone is: a run outlives a device hop and a night, so the day it was
    /// born in is not always the day it is being worked in.
    pub fn set_day(&self, name: &Sid, day: Option<jiff::civil::Date>) {
        if let Some(handle) = self
            .held
            .write()
            .expect("the registry is poisoned")
            .by_name
            .get_mut(name.as_str())
        {
            handle.day = day;
        }
    }

    /// Record the card a run landed on, once the first write materializes one.
    pub fn attach_card(&self, name: &Sid, card: SessionId) {
        if let Some(handle) = self
            .held
            .write()
            .expect("the registry is poisoned")
            .by_name
            .get_mut(name.as_str())
        {
            handle.card = Some(card);
        }
    }

    /// **Move every run bound to `from` onto `to`, in place.**
    ///
    /// A rename that lands on the board does not touch this process's own
    /// cache of who a run answers to — nothing in the write path knows this
    /// registry exists — so a run minted before the rename kept acting as the
    /// OLD identity for the rest of the process: every write it made, every
    /// box it read, attributed to a bot that had just stopped answering to that
    /// name. This is the other half of a rename, called right after the entity
    /// write commits, so a caller who kept the key the door gave them is
    /// renamed too, without minting a new one or losing the card it already
    /// carries.
    ///
    /// Returns how many runs moved, so a rename can say so.
    pub fn rebind_bot(&self, from: &EntityId, to: &EntityId) -> usize {
        let mut held = self.held.write().expect("the registry is poisoned");
        let mut moved = 0;
        for handle in held.by_name.values_mut() {
            if &handle.bot == from {
                handle.bot = to.clone();
                moved += 1;
            }
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::session::{SessionState, is_key};

    fn bot(slug: &str) -> EntityId {
        EntityId(format!("bot:{slug}"))
    }

    fn card(name: &str, key_hash: Option<&str>) -> Session {
        Session {
            id: SessionId(name.to_string()),
            sid: Some(Sid(name.to_string())),
            key_hash: key_hash.map(str::to_string),
            bot: bot("gamma"),
            focus: "reading".into(),
            started_at: "2026-10-09T10:00:00Z".parse().expect("a moment"),
            state: SessionState::Active,
            entries: Vec::new(),
            timezone: None,
            started_on: None,
            served_chars: 0,
            stated_day: None,
            wrap_window: None,
            charter_told_at: None,
        }
    }

    /// **The collision retry, watched.** Entropy will not produce a collision
    /// on demand, so the draw is supplied: the first candidate is a name the
    /// registry already holds, and minting must ride past it rather than
    /// re-issuing a name somebody else is already using.
    #[test]
    fn minting_rides_past_a_collision_rather_than_re_issuing_a_name() {
        let registry = SessionRegistry::new();
        let taken = registry
            .mint_with(&bot("gamma"), || "abc234".to_string(), mint_key)
            .expect("the first mint takes it");
        assert_eq!(taken.name.as_str(), "abc234");

        let mut drawn = ["abc234", "abc234", "bcd345"].into_iter();
        let mut calls = 0;
        let next = registry
            .mint_with(
                &bot("delta"),
                || {
                    calls += 1;
                    drawn.next().expect("a candidate").to_string()
                },
                mint_key,
            )
            .expect("a free name is found");
        assert_eq!(
            next.name.as_str(),
            "bcd345",
            "the taken name is not re-issued"
        );
        assert_eq!(calls, 3, "…and it drew again for each collision");
        assert_eq!(
            registry.lookup_name("abc234").expect("still held").bot,
            bot("gamma"),
            "the first holder keeps its name"
        );
    }

    /// A draw that never comes up free is an error the door can report, not a
    /// hang and not a duplicate.
    #[test]
    fn minting_gives_up_rather_than_looping_or_duplicating() {
        let registry = SessionRegistry::new();
        registry
            .mint_with(&bot("gamma"), || "abc234".to_string(), mint_key)
            .expect("taken");
        assert_eq!(
            registry.mint_with(&bot("delta"), || "abc234".to_string(), mint_key),
            Err(NoFreeHandle)
        );
    }

    /// **A key opens its run and a name does not.** The two values are told
    /// apart by what the registry will do with them: the key is hashed and finds
    /// the run, and the name finds nothing when it is offered as a key.
    /// **A run that holds no key is taken once.** The first taker gets the
    /// run and the moment; the second gets nothing, whatever it sends, because
    /// the run holds a key now. The check and the change are one step under the
    /// registry's own lock, which is what keeps two callers sending one old name
    /// from both leaving with a key.
    #[test]
    fn a_keyless_run_is_taken_once_and_not_again() {
        let registry = SessionRegistry::new();
        let bot = EntityId("bot:gamma".into());
        registry
            .mint_with(&bot, || "jm7z".to_string(), mint_key)
            .expect("minted");
        registry.set_key(&Sid("jm7z".into()), KeyState::Legacy, None);
        let at = jiff::Timestamp::now();
        let first = registry.take_legacy("jm7z", hash_key("sk-first"), at);
        assert!(first.is_some(), "the first taker gets the run");
        assert!(
            registry
                .take_legacy("jm7z", hash_key("sk-second"), at)
                .is_none(),
            "the second gets nothing"
        );
        assert_eq!(
            registry
                .lookup_name("jm7z")
                .and_then(|held| held.exchanged_at),
            Some(at),
            "the run says when it was exchanged"
        );
        assert!(
            registry.lookup_key("sk-first").is_some() && registry.lookup_key("sk-second").is_none(),
            "only the first key opens it"
        );
    }

    #[test]
    fn a_key_opens_its_run_and_its_name_opens_nothing() {
        let registry = SessionRegistry::new();
        let run = registry.mint(&bot("gamma")).expect("minted");
        assert!(is_key(&run.key), "{}", run.key);
        assert_ne!(run.key, run.name.as_str());

        let opened = registry.lookup_key(&run.key).expect("the key opens it");
        assert_eq!(opened.name, run.name);
        assert_eq!(opened.bot, bot("gamma"));
        assert!(
            registry.lookup_key(run.name.as_str()).is_none(),
            "a name offered as a key opens nothing"
        );
    }

    /// **Replacing a key closes the old one.** The index follows the key state,
    /// so a rotated or voided key stops opening the run at once.
    #[test]
    fn a_replaced_or_voided_key_stops_opening_its_run() {
        let registry = SessionRegistry::new();
        let run = registry.mint(&bot("gamma")).expect("minted");
        let next = mint_key();
        registry.set_key(&run.name, KeyState::Held(hash_key(&next)), None);
        assert!(
            registry.lookup_key(&run.key).is_none(),
            "the old key is closed"
        );
        assert_eq!(
            registry
                .lookup_key(&next)
                .expect("the new key opens it")
                .name,
            run.name
        );

        registry.set_key(&run.name, KeyState::Void, None);
        assert!(
            registry.lookup_key(&next).is_none(),
            "a voided key opens nothing"
        );
        assert!(
            registry.holds(run.name.as_str()),
            "the run is still known by its name"
        );
    }

    /// **A restart brings back the runs the board carries, with the keys they
    /// held.** A card with a hash opens by it, a card with the void word holds
    /// none, and a card from before keys existed is told apart from both.
    #[test]
    fn the_board_puts_back_each_runs_name_and_the_key_it_held() {
        let key = mint_key();
        let board = vec![
            card("abc234", Some(&hash_key(&key))),
            card("bcd345", Some(KEY_VOID)),
            card("cdef", None),
        ];
        let registry = SessionRegistry::new();
        let rebuilt = registry.rebuild_from(&board);
        assert_eq!(
            rebuilt,
            Rebuilt {
                recovered: 3,
                unreadable: 0
            }
        );

        assert_eq!(
            registry.lookup_key(&key).expect("opens").name.as_str(),
            "abc234"
        );
        assert_eq!(
            registry.lookup_name("bcd345").expect("held").key,
            KeyState::Void
        );
        assert_eq!(
            registry.lookup_name("cdef").expect("held").key,
            KeyState::Legacy
        );
    }

    /// **A name that fails the shape check is counted, not restored.**
    #[test]
    fn a_card_with_a_name_this_build_never_minted_is_counted_apart() {
        let mut broken = card("abc234", None);
        broken.sid = Some(Sid("NOPE".into()));
        let rebuilt = SessionRegistry::new().rebuild_from(&[broken]);
        assert_eq!(
            rebuilt,
            Rebuilt {
                recovered: 0,
                unreadable: 1
            }
        );
    }

    /// **A stranger allocates nothing.** The ledgers are keyed by name and
    /// ignore a name the registry does not hold, so a call that never proved
    /// itself leaves no entry behind.
    #[test]
    fn a_name_the_registry_does_not_hold_leaves_no_ledger_entry() {
        let registry = SessionRegistry::new();
        let shown = serde_json::json!({"id": "person:alpha", "name": "Alpha"});
        registry.note_call("zzzzzz");
        registry.note_shown("zzzzzz", &shown);
        assert!(
            !registry
                .seen
                .lock()
                .expect("the seen ledger")
                .knows("zzzzzz"),
            "a name nobody holds was given a ledger entry"
        );

        // The positive: a run the registry holds is remembered.
        let run = registry.mint(&bot("gamma")).expect("minted");
        registry.note_shown(run.name.as_str(), &shown);
        assert!(
            registry
                .seen
                .lock()
                .expect("the seen ledger")
                .knows(run.name.as_str()),
            "a held run was not remembered, so the negative above proves nothing"
        );
    }
}

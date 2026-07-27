//! The **session handle** — `sid`: server-minted, opaque, four characters.
//!
//! A session's id used to be the store's card id, which meant a caller could
//! only address a session that had already been written. The handle is minted
//! by jojobot instead, at the door, before any card exists — and the registry
//! here is what maps it to a card once one does.
//!
//! Three properties, each load-bearing:
//!
//! * **short** — it rides on calls a person reads, so it is four characters,
//!   not a UUID;
//! * **hard to confuse** — the alphabet excludes the glyphs that read as one
//!   another, so a handle copied by eye survives the trip;
//! * **opaque** — it says NOTHING about what the session is working on. No slug
//!   derived from a focus, no content of any kind: a handle that describes its
//!   session is one that leaks what the session is about to anybody who sees an
//!   id, and one that has to be re-minted when the focus moves.
//!
//! Minted from OS entropy rather than a counter or a clock, because a handle a
//! caller can predict is a handle a caller can guess their way into.
//!
//! **The registry is process-wide, never per connection.** The transport builds
//! one handler per MCP session and most clients open a fresh one per tool call,
//! so a map living on the handler would forget every sid it ever issued. The
//! flip side is stated rather than papered over: a restart empties it, and a
//! handle from before one is *gone* — blocked, never silently swapped for a new
//! session.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{SID_ALPHABET, SID_LEN, Session, SessionId, is_readable_sid};

// The handle type, its alphabet and its shape rule are the domain's — a session
// answers to it, and the store persists it. Re-exported so callers reach one
// vocabulary rather than two names for one thing.
pub use jojobot_domain::session::{Sid, is_readable_sid as is_readable};

/// The answer that chooses a fresh session over any of the resumable ones.
/// Deliberately a word rather than a handle: it is not a session, and giving it
/// a handle would mean minting one for a session that may never be written.
pub const NEW: &str = "new";

/// How many collisions minting will ride out before giving up.
///
/// Reaching it means the registry holds a large fraction of a million live
/// handles, which is not a state this server gets into — but "unreachable" is
/// not a thing to write an `expect` about when the alternative is one honest
/// error the door can report.
const MINT_ATTEMPTS: usize = 64;

/// Draw one candidate handle from OS entropy.
///
/// `256 % 32 == 0`, so the fold is uniform — no rejection sampling and no bias
/// toward the front of the alphabet.
pub fn draw() -> String {
    let mut bytes = [0u8; SID_LEN];
    // The OS entropy source failing is not a condition this server can serve
    // through: every handle it mints after that would be one it cannot promise
    // is unguessable.
    getrandom::fill(&mut bytes).expect("the OS entropy source is readable");
    bytes
        .iter()
        .map(|b| SID_ALPHABET[*b as usize % SID_ALPHABET.len()] as char)
        .collect()
}

/// What a handle addresses: whose session it is, and the card it landed on —
/// `None` until the first real write materializes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handle {
    /// The identity this session is one run of. **A session is bound to it at
    /// boot and never switches**, so this is what refuses somebody else's sid.
    pub bot: EntityId,
    /// The store's card, once there is one.
    pub card: Option<SessionId>,
}

/// Minting found no free handle. See [`MINT_ATTEMPTS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoFreeHandle;

impl std::fmt::Display for NoFreeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no free session handle could be minted")
    }
}

/// Every handle this process has issued — **and the locks that serialize the
/// callers using them.**
///
/// The two live together because they answer to the same key. A handle is what
/// names a session across connections, so it is also the only thing two callers
/// writing to one session have in common: a lock anywhere else (on the handler,
/// on the store) either excludes too much or, as the per-handler gate did,
/// excludes nothing at all.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    held: RwLock<HashMap<String, Handle>>,
    /// One mutex per key in flight, **keyed by the bot's handle**: a boot
    /// resolves a whole identity's board and a write resolves one run of it, and
    /// the identity is the only name both of them hold. Keyed any more narrowly
    /// the pair that must agree ends up in two queues.
    ///
    /// **Entries are never removed.** A key is a bot id — or, for a handle this
    /// process is not holding, the handle itself — and both are bounded by how
    /// many identities and runs this operator has. Reaping one would mean
    /// proving nobody is about to take it, and a lock whose removal races is
    /// worse than a map that keeps a few dozen mutexes.
    gates: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl SessionRegistry {
    /// An empty registry — one per process.
    pub fn new() -> Self {
        Self::default()
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

    /// Mint a handle for a session of `bot`, on `card` if one already exists.
    pub fn mint(&self, bot: &EntityId, card: Option<SessionId>) -> Result<Sid, NoFreeHandle> {
        self.mint_with(bot, card, draw)
    }

    /// The same, over a caller-supplied draw — **so the collision path is
    /// testable.** A retry nobody has watched happen is a retry nobody knows
    /// works, and it is not a path entropy will produce on demand.
    pub fn mint_with(
        &self,
        bot: &EntityId,
        card: Option<SessionId>,
        mut draw: impl FnMut() -> String,
    ) -> Result<Sid, NoFreeHandle> {
        let mut held = self.held.write().expect("the registry is poisoned");
        for _ in 0..MINT_ATTEMPTS {
            let candidate = draw();
            // **Against every handle this process has issued, not only the live
            // ones.** A handle whose session was wrapped an hour ago is still
            // an answer a caller may be holding, and re-issuing it would send
            // their next call into somebody else's session.
            if held.contains_key(&candidate) {
                continue;
            }
            held.insert(
                candidate.clone(),
                Handle {
                    bot: bot.clone(),
                    card,
                },
            );
            return Ok(Sid(candidate));
        }
        Err(NoFreeHandle)
    }

    /// What this handle addresses, or `None` for one this process never issued
    /// — a typo, or a handle from before a restart.
    pub fn lookup(&self, sid: &str) -> Option<Handle> {
        self.held
            .read()
            .expect("the registry is poisoned")
            .get(sid)
            .cloned()
    }

    /// **One handle per card.** A session that is offered back on two
    /// consecutive boots is one session, and handing it a second address would
    /// make the same run answer to two names.
    pub fn addressing(&self, card: &SessionId) -> Option<Sid> {
        self.held
            .read()
            .expect("the registry is poisoned")
            .iter()
            .find(|(_, h)| h.card.as_ref() == Some(card))
            .map(|(sid, _)| Sid(sid.clone()))
    }

    /// The handle for a card that exists: the one already issued for it, or a
    /// fresh one.
    pub fn for_card(&self, bot: &EntityId, card: &SessionId) -> Result<Sid, NoFreeHandle> {
        match self.addressing(card) {
            Some(sid) => Ok(sid),
            None => self.mint(bot, Some(card.clone())),
        }
    }

    /// **Rebuild from the board** — every handle a card carries, put back.
    ///
    /// Called once at startup, eagerly, before the first request: a registry
    /// filled lazily on the first miss would answer differently for the first
    /// caller after a restart than for the second, which is the kind of
    /// difference nobody can reproduce.
    ///
    /// A card written before handles were persisted carries none and simply
    /// contributes nothing here — the boot that offers it mints one on the spot,
    /// which is the whole of the migration.
    ///
    /// Returns how many it recovered, so a restart can say so out loud.
    pub fn rebuild_from(&self, sessions: &[Session]) -> usize {
        let mut held = self.held.write().expect("the registry is poisoned");
        for session in sessions {
            let Some(sid) = session.sid.clone() else {
                continue;
            };
            if !is_readable_sid(sid.as_str()) {
                continue;
            }
            held.insert(
                sid.0,
                Handle {
                    bot: session.bot.clone(),
                    card: Some(session.id.clone()),
                },
            );
        }
        held.len()
    }

    /// Record the card a handle's session landed on, once the first write
    /// materializes one.
    pub fn attach_card(&self, sid: &Sid, card: SessionId) {
        if let Some(handle) = self
            .held
            .write()
            .expect("the registry is poisoned")
            .get_mut(sid.as_str())
        {
            handle.card = Some(card);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(slug: &str) -> EntityId {
        EntityId(format!("bot:{slug}"))
    }

    /// **The collision retry, watched.** Entropy will not produce a collision
    /// on demand, so the draw is supplied: the first candidate is one the
    /// registry already holds, and minting must ride past it rather than
    /// re-issuing a handle somebody else is already using.
    #[test]
    fn minting_rides_past_a_collision_rather_than_re_issuing_a_handle() {
        let registry = SessionRegistry::new();
        let taken = registry
            .mint_with(&bot("gamma"), None, || "aaaa".to_string())
            .expect("the first mint takes it");
        assert_eq!(taken.as_str(), "aaaa");

        let mut drawn = ["aaaa", "aaaa", "bbbb"].into_iter();
        let mut calls = 0;
        let next = registry
            .mint_with(&bot("delta"), None, || {
                calls += 1;
                drawn.next().expect("a candidate").to_string()
            })
            .expect("a free handle is found");
        assert_eq!(
            next.as_str(),
            "bbbb",
            "the taken handle must not be re-issued"
        );
        assert_eq!(calls, 3, "…and it drew again for each collision");

        assert_eq!(
            registry.lookup("aaaa").expect("still held").bot,
            bot("gamma"),
            "the first holder keeps its handle"
        );
    }

    /// A draw that never comes up free is an error the door can report, not a
    /// hang and not a duplicate.
    #[test]
    fn minting_gives_up_rather_than_looping_or_duplicating() {
        let registry = SessionRegistry::new();
        registry
            .mint_with(&bot("gamma"), None, || "aaaa".to_string())
            .expect("taken");
        assert_eq!(
            registry.mint_with(&bot("delta"), None, || "aaaa".to_string()),
            Err(NoFreeHandle)
        );
    }

    /// **One handle per card.** A session offered back on two boots is one
    /// session; a second address for it would make the same run answer to two
    /// names.
    #[test]
    fn a_card_keeps_the_one_handle_it_was_first_given() {
        let registry = SessionRegistry::new();
        let card = SessionId("4212".into());
        let first = registry.for_card(&bot("gamma"), &card).expect("minted");
        let again = registry.for_card(&bot("gamma"), &card).expect("found");
        assert_eq!(first, again);
        assert_eq!(registry.addressing(&card), Some(first.clone()));

        let other = registry
            .for_card(&bot("gamma"), &SessionId("4213".into()))
            .expect("minted");
        assert_ne!(other, first, "a different card is a different session");
    }

    /// The lazy half: a handle exists before any card does, and the card is
    /// recorded on it when the first write makes one.
    #[test]
    fn a_handle_outruns_its_card_and_is_joined_to_it_later() {
        let registry = SessionRegistry::new();
        let sid = registry.mint(&bot("gamma"), None).expect("minted");
        assert_eq!(registry.lookup(sid.as_str()).expect("held").card, None);

        registry.attach_card(&sid, SessionId("4212".into()));
        assert_eq!(
            registry.lookup(sid.as_str()).expect("held").card,
            Some(SessionId("4212".into()))
        );
        assert_eq!(registry.addressing(&SessionId("4212".into())), Some(sid));
    }

    /// A handle this process never issued is simply not there — and it is the
    /// registry's silence, not a guess, that the door turns into a block.
    #[test]
    fn a_handle_from_before_a_restart_is_not_in_a_fresh_registry() {
        let old = SessionRegistry::new();
        let sid = old.mint(&bot("gamma"), None).expect("minted");
        assert!(SessionRegistry::new().lookup(sid.as_str()).is_none());
    }
}

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
use std::sync::RwLock;

use jojobot_domain::memory::EntityId;
use jojobot_domain::session::SessionId;

/// The handle's alphabet: **Crockford's base32, lowercased** — the digits and
/// the letters, minus `i`, `l`, `o` and `u`.
///
/// `i`/`l`/`1`, `o`/`0` and `u`/`v` are the pairs a reader mistakes for one
/// another, and a mistaken handle is one jojobot must refuse rather than
/// correct — correcting it means guessing which session somebody meant. Thirty
/// two symbols, every one of them inside the `[a-z0-9-]` a
/// [`SessionId`](jojobot_domain::session::SessionId) accepts.
pub const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// How many characters a handle is.
///
/// **Four, not three.** Three would be 32³ = 32,768 — enough for the live
/// sessions of one operator, but the space is what makes a handle hard to
/// forge, and a fourth character buys 32× of it for one keystroke. It also
/// keeps the handle space clear of [`NEW`], the one word a caller may answer
/// the boot's offer with: three characters could mint `new` itself.
pub const SID_LEN: usize = 4;

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

/// A session handle. Its own type, because the thing it is NOT — the store's
/// card id, also a `SessionId` — is exactly what it would otherwise be mixed up
/// with while this migration is half done.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// Whether this string could be a handle jojobot minted.
///
/// **Shape only** — a readable handle may still be unknown, and the two are
/// told apart where they are answered, because "you mistyped it" and "that
/// session is gone" send a caller to different places.
pub fn is_readable(sid: &str) -> bool {
    sid.len() == SID_LEN && sid.bytes().all(|b| ALPHABET.contains(&b))
}

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
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
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

/// Every handle this process has issued.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    held: RwLock<HashMap<String, Handle>>,
}

impl SessionRegistry {
    /// An empty registry — one per process.
    pub fn new() -> Self {
        Self::default()
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
    use jojobot_domain::session::validate_session_id;

    fn bot(slug: &str) -> EntityId {
        EntityId(format!("bot:{slug}"))
    }

    /// **The glyphs a reader confuses are not in the alphabet**, and everything
    /// in it is something the domain's id type accepts — a handle jojobot
    /// cannot store is a handle jojobot cannot hand out.
    #[test]
    fn the_alphabet_excludes_the_confusable_glyphs() {
        assert_eq!(ALPHABET.len(), 32, "Crockford's base32, minus nothing else");
        for confusable in [b'i', b'l', b'o', b'u'] {
            assert!(
                !ALPHABET.contains(&confusable),
                "{} reads as another glyph and must not be mintable",
                confusable as char
            );
        }
        let whole = String::from_utf8(ALPHABET.to_vec()).expect("ascii");
        assert!(
            validate_session_id(&SessionId(whole.clone())).is_ok(),
            "every symbol must be a legal session id byte: {whole}"
        );
    }

    /// A handle is four characters of that alphabet, and it is drawn rather
    /// than derived — two draws in a row must not be the same thing.
    #[test]
    fn a_drawn_handle_is_four_readable_characters() {
        let drawn: Vec<String> = (0..200).map(|_| draw()).collect();
        for sid in &drawn {
            assert!(is_readable(sid), "{sid} is not a readable handle");
        }
        let distinct: std::collections::HashSet<&String> = drawn.iter().collect();
        assert!(
            distinct.len() > 100,
            "handles must come from entropy, not a pattern: {} distinct of 200",
            distinct.len()
        );
    }

    /// Shape is checked, and a near-miss is refused rather than repaired:
    /// correcting `1` to `l` is guessing which session somebody meant.
    #[test]
    fn an_unreadable_handle_is_refused_rather_than_corrected() {
        assert!(is_readable("k3f9"));
        for bad in [
            "", "k3f", "k3f9a", "k3fo", "k3fi", "k3fl", "k3fu", "K3F9", "k3f-", "k3f ",
        ] {
            assert!(!is_readable(bad), "{bad:?} must not read as a handle");
        }
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

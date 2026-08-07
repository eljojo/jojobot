//! **The token a refusal mints, and the only thing that lifts it** (rule 75).
//!
//! **A boolean cannot do this job.** A flag the caller sets asserts nothing: it
//! can be sent on a first call, by a caller that has seen no refusal at all,
//! and the guard cannot tell that apart from a caller that read one and
//! decided. An override like that is available to exactly the callers it is
//! meant to slow down.
//!
//! A token fixes that by being **unguessable and specific**. The refusal mints
//! it, the answer carries it, and the guard lifts only for the token it minted
//! **for that same collision** — so a token from one refusal does not open a
//! different one, and a made-up token opens nothing.
//!
//! **One mechanism, both gates** (rule 51). The entity screen and the mailbox
//! screen take the same token type and answer with the same shape; a second
//! mechanism beside the first is the thing this deletes rather than doubles.
//!
//! **No expiry, no counter, no audit trail, no store.** The token is derived
//! from what the refusal said, keyed by a secret this process makes at startup,
//! so there is nothing to evict and nothing to clean up. A restart invalidates
//! outstanding tokens, which costs a caller one re-read and is honest: the
//! process that made the promise is gone.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::OnceLock;

/// The process's key. Made once, on first use, and never written down.
///
/// `RandomState` is a randomly-seeded SipHash — the standard library's own
/// defence against an adversary who chooses inputs. One instance for the
/// process is what makes a token unguessable rather than merely a hash of
/// something a caller can see.
fn key() -> &'static RandomState {
    static KEY: OnceLock<RandomState> = OnceLock::new();
    KEY.get_or_init(RandomState::new)
}

/// **What a refusal was about** — the thing a token is minted for and checked
/// against.
///
/// Two refusals with the same fingerprint are the same refusal, and a token
/// for one lifts the other. That is correct: the caller was told the same
/// thing and answered it. Two refusals that differ anywhere here are different
/// refusals, and a token does not carry across.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Collision {
    /// Which gate refused — so a mailbox token cannot lift an entity refusal
    /// even if the names happen to match.
    pub gate: &'static str,
    /// What the caller tried to write.
    pub attempted: String,
    /// What the guard found, in the order it reported them.
    pub candidates: Vec<String>,
}

impl Collision {
    /// The token for this refusal. Stable for one process and one collision.
    pub fn token(&self) -> String {
        // Hex of a keyed 64-bit hash. Long enough that guessing is not a
        // strategy, short enough to read back in a tool call.
        format!("{:016x}", key().hash_one(self))
    }

    /// Whether `offered` is the token THIS refusal minted.
    ///
    /// **The whole mechanism is that this is false for anything else** — a
    /// token invented by the caller, a token from a different collision, a
    /// token from a previous run of the server.
    pub fn honours(&self, offered: Option<&str>) -> bool {
        offered.is_some_and(|offered| offered.trim() == self.token())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collision(gate: &'static str, attempted: &str, candidates: &[&str]) -> Collision {
        Collision {
            gate,
            attempted: attempted.to_string(),
            candidates: candidates.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    /// **The pair the mechanism is.** Its own token lifts it; anything else
    /// does not.
    ///
    /// The first half alone is satisfied by a guard that accepts every token,
    /// which is the boolean again wearing a longer name.
    #[test]
    fn a_refusal_is_lifted_by_its_own_token_and_by_nothing_else() {
        let refusal = collision("entity", "person:alpha", &["person:alphonse"]);

        assert!(
            refusal.honours(Some(&refusal.token())),
            "the token a refusal minted lifts that refusal"
        );

        for wrong in ["", "   ", "0000000000000000", "true", "yes"] {
            assert!(
                !refusal.honours(Some(wrong)),
                "a token nobody minted lifts nothing: {wrong:?}"
            );
        }
        assert!(!refusal.honours(None), "no token at all lifts nothing");
    }

    /// **A token does not carry to another refusal.** This is what makes it a
    /// token rather than a password: reading one refusal does not buy a way
    /// past the next one.
    #[test]
    fn a_token_minted_for_one_collision_does_not_lift_another() {
        let alpha = collision("entity", "person:alpha", &["person:alphonse"]);
        let others = [
            // A different thing attempted.
            collision("entity", "person:beta", &["person:alphonse"]),
            // The same attempt against different candidates — the guard found
            // something else, so the caller was told something else.
            collision("entity", "person:alpha", &["person:alpha-two"]),
            // More candidates than the caller was shown.
            collision(
                "entity",
                "person:alpha",
                &["person:alphonse", "person:alpha-two"],
            ),
            // The other gate. A name can collide in both worlds and they are
            // not one refusal.
            collision("mailbox", "person:alpha", &["person:alphonse"]),
        ];

        for other in others {
            assert!(
                !other.honours(Some(&alpha.token())),
                "a token for {alpha:?} must not lift {other:?}"
            );
        }
    }

    /// The same refusal, asked twice, mints the same token — or a caller
    /// re-reading its own refusal would be handed a token that no longer works.
    #[test]
    fn one_refusal_mints_one_token() {
        let once = collision("entity", "person:alpha", &["person:alphonse"]);
        let again = collision("entity", "person:alpha", &["person:alphonse"]);
        assert_eq!(once.token(), again.token());
    }
}

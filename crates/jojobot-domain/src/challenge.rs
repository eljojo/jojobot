//! **The challenge: a code the operator holds, waiting to be presented on a
//! boot.**
//!
//! A session the operator starts by hand asks for a code for the bot it names.
//! jojobot writes the code into the operator's own mailbox and keeps a
//! challenge: a keyed hash of the code, the bot it is for, when it was issued
//! and where it stands. The session receives the challenge's handle and never
//! the code. A challenge is not a voucher: it grants nothing until a boot
//! presents the right code, and then the boot is given a voucher.
//!
//! **Only the engine writes one**, like a voucher: see
//! [`crate::voucher::refuses_engine_only_write`]. A challenge holds a hash any
//! bot could read, so the hash is keyed with a key the process draws at startup
//! and never stores. A restart voids every outstanding challenge, which costs a
//! fresh request inside the window.
//!
//! **This module decides; it does not write.** [`decide_redemption`] is the one
//! decision, taken on what the challenge holds, the clock and whether the code
//! matched.

use std::collections::BTreeMap;

use jiff::{SignedDuration, Timestamp};

use crate::memory::EntityId;

/// The key a challenge holds the bot it is for under.
pub const BOT: &str = "bot";
/// The key a challenge holds the keyed hash of its code under.
pub const CODE_HASH: &str = "code_hash";
/// The key a challenge holds its state under.
pub const STATE: &str = "state";
/// The key a challenge holds the moment it was issued under.
pub const ISSUED_AT: &str = "issued_at";
/// The key a challenge holds the number of wrong codes presented under.
pub const WRONG_TRIES: &str = "wrong_tries";

/// **How long a code stands unredeemed.** The one expiry in the design: a held
/// voucher has none.
pub const WINDOW: SignedDuration = SignedDuration::from_mins(10);

/// **How many wrong codes void a challenge.** The third wrong code voids it, so
/// guessing online stops at three.
pub const MAX_WRONG: u32 = 3;

/// How many characters a code has, drawn from the handle alphabet.
pub const CODE_LEN: usize = 8;

/// **Where a challenge stands.** A challenge past [`WINDOW`] is expired by the
/// clock and stays `issued` in the store: nothing writes at the moment a window
/// ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeState {
    /// Waiting for its code.
    Issued,
    /// Its code was presented and a voucher was given. It never grants again.
    Redeemed,
    /// A newer request for the same bot took its place.
    Replaced,
    /// Too many wrong codes were presented.
    Void,
}

impl ChallengeState {
    /// Every state.
    pub const ALL: [ChallengeState; 4] = [
        ChallengeState::Issued,
        ChallengeState::Redeemed,
        ChallengeState::Replaced,
        ChallengeState::Void,
    ];

    /// The word the state is stored and read back as.
    pub fn as_token(self) -> &'static str {
        match self {
            ChallengeState::Issued => "issued",
            ChallengeState::Redeemed => "redeemed",
            ChallengeState::Replaced => "replaced",
            ChallengeState::Void => "void",
        }
    }

    /// The state a stored word names, exactly.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_token() == token)
    }
}

/// **One challenge, whole.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    /// The bot the code is for.
    pub bot: EntityId,
    /// The keyed hash of the code, in hex.
    pub code_hash: String,
    /// Where it stands.
    pub state: ChallengeState,
    /// When it was issued.
    pub issued_at: Timestamp,
    /// How many wrong codes have been presented.
    pub wrong_tries: u32,
}

impl Challenge {
    /// **The fields the engine writes**, under the five keys.
    pub fn fields(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            (BOT.to_string(), self.bot.to_string()),
            (CODE_HASH.to_string(), self.code_hash.clone()),
            (STATE.to_string(), self.state.as_token().to_string()),
            (ISSUED_AT.to_string(), self.issued_at.to_string()),
            (WRONG_TRIES.to_string(), self.wrong_tries.to_string()),
        ])
    }

    /// **A challenge read back from what a store holds.** `None` when a key is
    /// missing or does not read, so a damaged record is never taken for a live
    /// one.
    pub fn from_fields(fields: &BTreeMap<String, String>) -> Option<Self> {
        Some(Challenge {
            bot: EntityId(fields.get(BOT)?.clone()),
            code_hash: fields.get(CODE_HASH)?.clone(),
            state: ChallengeState::from_token(fields.get(STATE)?)?,
            issued_at: fields.get(ISSUED_AT)?.parse().ok()?,
            wrong_tries: fields.get(WRONG_TRIES)?.parse().ok()?,
        })
    }
}

/// **What presenting a code comes to.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redemption {
    /// The code matched a live challenge.
    Granted,
    /// The code did not match. `void` says this was the wrong code that voided
    /// the challenge.
    WrongCode {
        /// Whether this wrong code was the last one allowed.
        void: bool,
    },
    /// The challenge was already redeemed.
    Spent,
    /// A newer request took its place.
    Replaced,
    /// The window ended before the code was presented.
    Expired,
    /// Too many wrong codes were presented earlier.
    Void,
}

/// **The one decision on presenting a code.** A challenge that is not `issued`
/// answers for its state whatever the code is, so a spent or replaced challenge
/// cannot be told from a wrong guess by timing or by text. The window is read
/// before the code is, so an expired challenge never counts a try.
pub fn decide_redemption(challenge: &Challenge, now: Timestamp, code_matches: bool) -> Redemption {
    match challenge.state {
        ChallengeState::Redeemed => return Redemption::Spent,
        ChallengeState::Replaced => return Redemption::Replaced,
        ChallengeState::Void => return Redemption::Void,
        ChallengeState::Issued => {}
    }
    if now.duration_since(challenge.issued_at) > WINDOW {
        return Redemption::Expired;
    }
    if code_matches {
        return Redemption::Granted;
    }
    Redemption::WrongCode {
        void: challenge.wrong_tries + 1 >= MAX_WRONG,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        s.parse().expect("a fixed instant")
    }

    fn live() -> Challenge {
        Challenge {
            bot: EntityId("bot:marge".into()),
            code_hash: "ab".repeat(32),
            state: ChallengeState::Issued,
            issued_at: at("2026-10-09T12:00:00Z"),
            wrong_tries: 0,
        }
    }

    /// **A live challenge grants on the right code and counts a wrong one.**
    #[test]
    fn a_live_challenge_grants_on_the_right_code_and_refuses_a_wrong_one() {
        let now = at("2026-10-09T12:05:00Z");
        assert_eq!(decide_redemption(&live(), now, true), Redemption::Granted);
        assert_eq!(
            decide_redemption(&live(), now, false),
            Redemption::WrongCode { void: false }
        );
    }

    /// **The third wrong code voids, and the second does not.**
    #[test]
    fn the_third_wrong_code_voids_the_challenge() {
        let now = at("2026-10-09T12:01:00Z");
        let second = Challenge {
            wrong_tries: 1,
            ..live()
        };
        assert_eq!(
            decide_redemption(&second, now, false),
            Redemption::WrongCode { void: false }
        );
        let third = Challenge {
            wrong_tries: MAX_WRONG - 1,
            ..live()
        };
        assert_eq!(
            decide_redemption(&third, now, false),
            Redemption::WrongCode { void: true }
        );
    }

    /// **The window is ten minutes, the last second included, and an expired
    /// challenge refuses even the right code.**
    #[test]
    fn the_window_ends_ten_minutes_after_issue() {
        let edge = at("2026-10-09T12:10:00Z");
        assert_eq!(decide_redemption(&live(), edge, true), Redemption::Granted);
        let past = at("2026-10-09T12:10:01Z");
        assert_eq!(decide_redemption(&live(), past, true), Redemption::Expired);
        assert_eq!(decide_redemption(&live(), past, false), Redemption::Expired);
    }

    /// **A challenge that is not issued answers for its state**, with the right
    /// code and with a wrong one alike.
    #[test]
    fn a_challenge_that_is_not_issued_answers_for_its_state() {
        let now = at("2026-10-09T12:01:00Z");
        for (state, expected) in [
            (ChallengeState::Redeemed, Redemption::Spent),
            (ChallengeState::Replaced, Redemption::Replaced),
            (ChallengeState::Void, Redemption::Void),
        ] {
            let challenge = Challenge { state, ..live() };
            for matches in [true, false] {
                assert_eq!(
                    decide_redemption(&challenge, now, matches),
                    expected,
                    "{state:?} {matches}"
                );
            }
        }
    }

    /// **The five stored keys are pinned by value, and a challenge reads back
    /// from what it wrote.** A damaged record reads as none.
    #[test]
    fn a_challenge_writes_its_five_keys_and_reads_back() {
        let challenge = Challenge {
            wrong_tries: 2,
            ..live()
        };
        let fields = challenge.fields();
        let keys: Vec<&str> = fields.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["bot", "code_hash", "issued_at", "state", "wrong_tries"]
        );
        assert_eq!(fields["wrong_tries"], "2");
        assert_eq!(Challenge::from_fields(&fields), Some(challenge));
        let mut damaged = fields.clone();
        damaged.insert("state".into(), "expired".into());
        assert_eq!(Challenge::from_fields(&damaged), None);
        let mut gone = fields;
        gone.remove("issued_at");
        assert_eq!(Challenge::from_fields(&gone), None);
    }

    /// **Each state reads back as itself, and four is all there are.**
    #[test]
    fn every_state_reads_back_and_a_stray_word_is_none() {
        let words: Vec<&str> = ChallengeState::ALL.map(ChallengeState::as_token).to_vec();
        assert_eq!(words, ["issued", "redeemed", "replaced", "void"]);
        for state in ChallengeState::ALL {
            assert_eq!(ChallengeState::from_token(state.as_token()), Some(state));
        }
        assert_eq!(ChallengeState::from_token("expired"), None);
    }
}

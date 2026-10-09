//! **The voucher: a grant to do one gated act, issued by the engine and never
//! written by a caller.**
//!
//! A voucher is its own shipped kind. It is separate from a role because a role
//! answers *is this seat occupied*, and a voucher answers *may this session do
//! this act on this target*. It carries six keys: the right, the target, the
//! holder and the issuer (both sessions), the parent voucher (its link back
//! towards the root) and a state.
//!
//! **Only the engine writes one**, the way [`crate::session`] writes a role's
//! holder: the caller-facing verbs call [`refuses_engine_only_write`] and turn the
//! write back, and the engine path writes through the `Memory` trait directly.
//! A well-shaped record that a caller captures is never a grant, because a
//! hand-written grant is a right taken without being given.
//!
//! **The behaviour of a right lives in the build, never in a record.** The
//! rights are a shipped named set, [`SHIPPED_RIGHTS`]. An instance may add a
//! word, and the word grants nothing until code gives it behaviour: a voucher
//! naming a right no code knows is refused by [`refuses_unknown_right`], not
//! accepted and ignored.
//!
//! Nothing here gates an act. A gate reads these types in a later slice.

use std::collections::BTreeMap;

use crate::memory::{EntityId, EntityKind, MemoryError};

/// The key a voucher holds its right under: which act it permits.
pub const RIGHT: &str = "right";
/// The key a voucher holds its target under: the bot or role the act is on.
pub const TARGET: &str = "target";
/// The key a voucher holds its holder under: the sid of the session that may
/// use it.
pub const HOLDER: &str = "holder";
/// The key a voucher holds its issuer under: the sid of the session that gave
/// it.
pub const ISSUER: &str = "issuer";
/// The key a voucher holds its parent under: the voucher it descends from, whose
/// own parent leads on towards the root. A root has none.
pub const PARENT: &str = "parent";
/// The key a voucher holds its state under.
pub const STATE: &str = "state";

/// **Where a voucher stands.** A held voucher has no expiry: it ends by being
/// spent, voided or left in escrow past the pickup window, never by a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoucherState {
    /// Written by the engine and not yet taken up by its holder.
    Issued,
    /// Taken up: its holder may present it.
    Held,
    /// Surrendered to jojobot by a graceful wrap, waiting for a successor.
    Escrow,
    /// Presented for its act. It never grants again.
    Spent,
    /// Taken back, or replaced. It never grants again.
    Void,
}

impl VoucherState {
    /// Every state, in the order a voucher moves through them.
    pub const ALL: [VoucherState; 5] = [
        VoucherState::Issued,
        VoucherState::Held,
        VoucherState::Escrow,
        VoucherState::Spent,
        VoucherState::Void,
    ];

    /// The word the state is stored and read back as.
    pub fn as_token(self) -> &'static str {
        match self {
            VoucherState::Issued => "issued",
            VoucherState::Held => "held",
            VoucherState::Escrow => "escrow",
            VoucherState::Spent => "spent",
            VoucherState::Void => "void",
        }
    }

    /// The state a stored word names, exactly. A word that is none is no state.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_token() == token)
    }
}

/// The right of the root voucher: the one the operator's code grants to a
/// session that boots as the assistant. It names no act. Its presence is what
/// turns the gate on.
pub const ROOT: &str = "root";

/// **The rights the software ships**: one per gated act (booting a session as a
/// bot, claiming a role, changing a subordinate's charter and creating a bot)
/// and the root, which the operator's code alone grants.
pub const SHIPPED_RIGHTS: [&str; 5] = [
    "boot-as",
    "claim-role",
    "change-charter",
    "create-bot",
    ROOT,
];

/// **One voucher, whole.** The engine builds it and writes all of it in one
/// claim, so a stored voucher never lacks a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voucher {
    /// Which act it permits.
    pub right: String,
    /// The bot or role the act is on.
    pub target: String,
    /// The sid of the session that may use it.
    pub holder: String,
    /// The sid of the session that gave it.
    pub issuer: String,
    /// The voucher it descends from; `None` for a root.
    pub parent: Option<EntityId>,
    /// Where it stands.
    pub state: VoucherState,
}

impl Voucher {
    /// **The fields the engine writes for this voucher**, under the six keys.
    pub fn fields(&self) -> BTreeMap<String, String> {
        let mut fields = BTreeMap::from([
            (RIGHT.to_string(), self.right.clone()),
            (TARGET.to_string(), self.target.clone()),
            (HOLDER.to_string(), self.holder.clone()),
            (ISSUER.to_string(), self.issuer.clone()),
            (STATE.to_string(), self.state.as_token().to_string()),
        ]);
        if let Some(parent) = &self.parent {
            fields.insert(PARENT.to_string(), parent.to_string());
        }
        fields
    }
}

/// **A caller's write to a voucher or a challenge is refused**, whatever the
/// key: the handle names the kind, so the answer does not depend on what the
/// write carries. The engine path writes through the `Memory` trait and never
/// reaches this.
pub fn refuses_engine_only_write(subject: &EntityId) -> Option<MemoryError> {
    // The prefix is read off the handle rather than through `kind()`, which
    // answers from the loaded kind set: a guard must not depend on a load.
    let (kind, _) = subject.as_str().split_once(':')?;
    if kind == EntityKind::VOUCHER.as_token() {
        return Some(MemoryError::VoucherIssued {
            subject: subject.to_string(),
        });
    }
    (kind == EntityKind::CHALLENGE.as_token()).then(|| MemoryError::ChallengeIssued {
        subject: subject.to_string(),
    })
}

/// **A right no code knows is refused**, not accepted and ignored. `instance`
/// is the words the instance added; one of them is known, and grants nothing
/// until code gives it behaviour.
pub fn refuses_unknown_right(right: &str, instance: &[String]) -> Option<MemoryError> {
    let known = |word: &str| SHIPPED_RIGHTS.contains(&word) || instance.iter().any(|w| w == word);
    (!known(right)).then(|| MemoryError::UnknownRight {
        right: right.to_string(),
        known: SHIPPED_RIGHTS
            .iter()
            .map(|word| word.to_string())
            .chain(instance.iter().cloned())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(handle: &str) -> EntityId {
        EntityId(handle.to_string())
    }

    /// **The guard is on the kind, and only on the kind.** The positive and the
    /// negatives in one read: a voucher is refused whatever else is true, and the
    /// kinds that carry the same keys, a role and a bot, are not.
    #[test]
    fn a_write_to_a_voucher_is_refused_and_to_anything_else_is_not() {
        let refused = refuses_engine_only_write(&id("voucher:boot-as-patty"));
        assert!(
            matches!(&refused, Some(MemoryError::VoucherIssued { subject }) if subject == "voucher:boot-as-patty"),
            "{refused:?}",
        );
        let refused = refuses_engine_only_write(&id("challenge:abcd"));
        assert!(
            matches!(&refused, Some(MemoryError::ChallengeIssued { subject }) if subject == "challenge:abcd"),
            "{refused:?}",
        );
        for other in ["role:dev-dispatch", "bot:marge", "topic:widgets"] {
            assert!(refuses_engine_only_write(&id(other)).is_none(), "{other}");
        }
    }

    /// **Each state is a word that reads back as itself, and nothing else is a
    /// state.** Five states, no more: a sixth is a decision.
    #[test]
    fn every_state_reads_back_and_a_stray_word_is_none() {
        assert_eq!(VoucherState::ALL.len(), 5);
        for state in VoucherState::ALL {
            assert_eq!(VoucherState::from_token(state.as_token()), Some(state));
        }
        let words: Vec<&str> = VoucherState::ALL.map(VoucherState::as_token).to_vec();
        assert_eq!(words, ["issued", "held", "escrow", "spent", "void"]);
        assert_eq!(VoucherState::from_token("Held"), None);
        assert_eq!(VoucherState::from_token("expired"), None);
    }

    /// **The four shipped rights are known, an instance's word is known beside
    /// them, and any other word is refused with the words that are known.**
    #[test]
    fn a_right_no_code_knows_is_refused_and_a_known_one_is_not() {
        for right in SHIPPED_RIGHTS {
            assert!(refuses_unknown_right(right, &[]).is_none(), "{right}");
        }
        assert_eq!(
            SHIPPED_RIGHTS,
            [
                "boot-as",
                "claim-role",
                "change-charter",
                "create-bot",
                "root"
            ],
        );
        let instance = vec!["read-ledger".to_string()];
        assert!(refuses_unknown_right("read-ledger", &instance).is_none());
        let refused = refuses_unknown_right("read-ledger", &[]);
        assert!(
            matches!(&refused, Some(MemoryError::UnknownRight { right, known }) if right == "read-ledger" && known.len() == 5),
            "{refused:?}",
        );
        let refused = refuses_unknown_right("fly", &instance);
        assert!(
            matches!(&refused, Some(MemoryError::UnknownRight { known, .. }) if known.contains(&"read-ledger".to_string()) && known.contains(&"boot-as".to_string())),
            "{refused:?}",
        );
    }

    /// **A voucher is written whole, under the six keys, and the root has no
    /// parent.** The keys are the stored spelling, so they are pinned by value.
    #[test]
    fn a_voucher_writes_its_six_keys_and_a_root_writes_five() {
        let root = Voucher {
            right: "boot-as".into(),
            target: "bot:marge".into(),
            holder: "abcd".into(),
            issuer: "wxyz".into(),
            parent: None,
            state: VoucherState::Held,
        };
        let fields = root.fields();
        let keys: Vec<&str> = fields.keys().map(String::as_str).collect();
        assert_eq!(keys, ["holder", "issuer", "right", "state", "target"]);
        assert_eq!(fields["state"], "held");
        let child = Voucher {
            parent: Some(id("voucher:one")),
            ..root
        };
        let fields = child.fields();
        assert_eq!(fields["parent"], "voucher:one");
        assert_eq!(fields.len(), 6);
    }
}

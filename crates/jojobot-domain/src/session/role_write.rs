//! The keys a claim, renewal or release writes together.

use super::{ROLE_CLAIMED_AT, ROLE_HOLDER, RoleMove, RoleMoveKind};
use crate::memory::FactPatch;
use jiff::Timestamp;

/// A role's pointer to the agent hosting its holder.
pub const ROLE_AGENT: &str = "agent";

/// The three writes to a role's binding and lease.
pub enum RoleWrite<'a> {
    /// Take the role with the caller's runtime agent, when supplied.
    Claim {
        claimant: &'a str,
        now: Timestamp,
        agent: Option<&'a str>,
    },
    /// Keep the current holder's lease fresh without changing its agent.
    Renew { claimant: &'a str, at: Timestamp },
    /// Give up the holder and its agent together.
    Release { claimant: &'a str },
}

impl RoleWrite<'_> {
    /// The whole binding change, applied under the store's lease check.
    pub fn patch(&self, role: &str) -> FactPatch {
        let (claimant, moment, kind) = match self {
            Self::Claim { claimant, now, .. } => (*claimant, *now, None),
            Self::Renew { claimant, at } => (*claimant, *at, Some(RoleMoveKind::Renew)),
            Self::Release { claimant } => (
                *claimant,
                Timestamp::UNIX_EPOCH,
                Some(RoleMoveKind::Release),
            ),
        };
        let mut fields =
            std::collections::BTreeMap::from([(ROLE_CLAIMED_AT.to_string(), moment.to_string())]);
        fields.insert(
            ROLE_HOLDER.to_string(),
            if kind == Some(RoleMoveKind::Release) {
                ""
            } else {
                claimant
            }
            .to_string(),
        );
        let agent = match self {
            Self::Claim { agent, .. } => Some(agent.unwrap_or_default()),
            Self::Release { .. } => Some(""),
            Self::Renew { .. } => None,
        };
        if let Some(agent) = agent {
            // A blank wins over a legacy agent written on a different claim.
            fields.insert(ROLE_AGENT.to_string(), agent.to_string());
        }
        FactPatch {
            derived: fields.keys().cloned().collect(),
            fields,
            role_move: kind.map(|kind| RoleMove {
                kind,
                role: role.to_string(),
                claimant: claimant.to_string(),
            }),
            // A plain renewal names no session: a session on the patch makes
            // it an ordinary edit, rather than an overwritten heartbeat with
            // nothing appended (see is_a_plain_renewal).
            session: (kind != Some(RoleMoveKind::Renew)).then(|| claimant.to_string()),
            ..FactPatch::default()
        }
    }
}

#[cfg(any(test, feature = "testing"))]
pub mod testing {
    use super::*;

    #[cfg_attr(test, test)]
    pub fn p1_role_pointer_value_table() {
        let now = Timestamp::UNIX_EPOCH;
        let rows = [
            (
                RoleWrite::Claim {
                    claimant: "gamma",
                    now,
                    agent: Some("gamma-agent"),
                },
                Some("gamma-agent"),
                Vec::<String>::new(),
                Some("gamma"),
            ),
            (
                RoleWrite::Claim {
                    claimant: "gamma",
                    now,
                    agent: None,
                },
                Some(""),
                vec![],
                Some("gamma"),
            ),
            (
                RoleWrite::Renew {
                    claimant: "gamma",
                    at: now,
                },
                None,
                vec![],
                Some("gamma"),
            ),
            (
                RoleWrite::Release { claimant: "gamma" },
                Some(""),
                vec![],
                Some(""),
            ),
        ];
        let mut failures = Vec::new();
        for (row, (write, agent, clear, holder)) in rows.into_iter().enumerate() {
            let patch = write.patch("dev-dispatch");
            if patch.fields.get(ROLE_AGENT).map(String::as_str) != agent
                || patch.fields.get(ROLE_HOLDER).map(String::as_str) != holder
                || patch.clear_fields != clear
            {
                failures.push(format!("row {row}: {patch:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

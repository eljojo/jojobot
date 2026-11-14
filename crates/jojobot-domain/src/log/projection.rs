//! The log's one projection. Its members are the manifest of log-owned families;
//! a family still served from old rows is never included in a snapshot.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::request::events::{LedgerEvent, MissingPart};
use crate::teaching::events::TeachingEvent;

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct State {
    pub teaching: crate::teaching::events::State,
    pub ledger: crate::request::events::State,
    /// **The claims family's State.** Claim, field and identity events apply to it through
    /// the claims interpreter, which the adapter wraps to warn on a repaired duplicate handle,
    /// so it is not a member of [`State::apply`].
    pub claims: crate::log::claims::State,
    /// **The log holds a ledger event.** The ledger State evicts, so it cannot say, and the
    /// ledgers are filled once from the old tables only into a log that holds none. A State
    /// read back from a snapshot has to carry it.
    pub ledger_seen: bool,
}

pub enum Member {
    Teaching(TeachingEvent),
    Ledger(LedgerEvent),
}

impl State {
    /// Boot and live writes use the same family interpreters.
    pub fn apply(&mut self, member: &Member) -> Result<(), MissingPart> {
        match member {
            Member::Teaching(event) => self.teaching.apply(event),
            Member::Ledger(event) => {
                self.ledger.apply(event)?;
                self.ledger_seen = true;
            }
        }
        Ok(())
    }

    /// **A State read back from a snapshot, finished.** The one door a loaded State comes
    /// through: what each family leaves out of its serialized form is made again here.
    pub fn restored(mut self) -> Self {
        self.claims = self.claims.restored();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The ledger State evicts, so the State says on its own that the log holds a ledger
    /// event**, and says it again after it is serialized and read back.
    #[test]
    fn a_ledger_member_marks_the_log_as_holding_one_through_a_restore() {
        use crate::request::events::{Commit, LedgerEvent, Request, RequestKey};
        let mut state = State::default();
        assert!(!state.ledger_seen, "an empty State holds no ledger event");
        let at = jiff::Timestamp::from_second(1_700_000_000).expect("a moment");
        let spent = LedgerEvent::RequestCommitted(Commit {
            request: Request {
                key: RequestKey {
                    actor: crate::log::ObjectId("bot:homer".into()),
                    id: crate::log::RequestId("alpha".into()),
                    generation: 1,
                },
                verb: "capture".into(),
                digest: "digest-alpha".into(),
            },
            landed: "{}".into(),
            receipt: None,
            written_at: at,
            expires_at: at + jiff::SignedDuration::from_secs(10),
        });
        state
            .apply(&Member::Ledger(spent))
            .expect("a committed request applies");
        assert!(state.ledger_seen);
        let restored: State =
            serde_json::from_slice(&serde_json::to_vec(&state).expect("the State serializes"))
                .expect("the State reads back");
        assert!(
            restored.restored().ledger_seen,
            "a restored State forgot that the log holds a ledger event"
        );
    }
}

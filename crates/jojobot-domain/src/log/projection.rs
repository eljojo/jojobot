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
            Member::Ledger(event) => self.ledger.apply(event)?,
        }
        Ok(())
    }
}

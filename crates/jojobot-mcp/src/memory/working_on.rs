//! **What a bot is working on: one key on the bot, written by a session, shown
//! only while that session is live.**
//!
//! `journal` declares or clears it, and `capture` or `update_fact` fill it when
//! the acting bot moves work it owns to `now`. All three come through
//! [`Jojobot::set_working_on`]. The value lives on one record of the bot,
//! rewritten in place, so every declaration is a write of that record and
//! carries the session that made it.
//!
//! **A reader never gets the stored value.** [`Jojobot::working_on_of`] is the
//! one read: it looks up the session behind the write that holds the value and
//! answers only while that session is active.

use super::*;

use jojobot_domain::memory::Memory;
use jojobot_domain::memory::graph::Object;
use jojobot_domain::memory::kinds::WORKING_ON;
use jojobot_domain::memory::{FactAddress, FactPatch, Guarded, NewFact};
use jojobot_domain::session::{SessionId, SessionState, Sessions};
use std::collections::{BTreeMap, BTreeSet};

/// The words of the bot's one working-on record. The link is in its fields, so
/// the words never go stale when the link moves.
const RECORD: &str = "What this bot is working on now";

/// What a write to `working_on` does.
pub(crate) enum Declared {
    /// Point the bot at this thing.
    At(EntityId),
    /// Take the link off.
    Nothing,
}

/// **What a reader is told about a bot's `working_on`.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingOn {
    /// Nobody declared it, or it was cleared.
    Unset,
    /// A live session declared it.
    Live(String),
    /// A session declared it and has since wrapped or been abandoned.
    Stale,
}

/// **The one read of a bot's `working_on`.** The value is the one the fold
/// holds; whether it is shown depends on the session behind the write that
/// holds it. The web page asks this too, so no surface serves the stored value
/// on its own.
pub async fn working_on_of(
    memory: &dyn Memory,
    sessions: &dyn Sessions,
    bot: &EntityId,
) -> Result<WorkingOn, MemoryError> {
    let Some(value) = memory.fields(bot).await?.remove(WORKING_ON) else {
        return Ok(WorkingOn::Unset);
    };
    let Some(held) = memory.backing(bot).await?.remove(WORKING_ON) else {
        return Ok(WorkingOn::Stale);
    };
    let chain = memory
        .claim_history(&FactAddress::new(bot.clone(), held.fact))
        .await?;
    let Some(run) = chain.last().and_then(|write| write.session.clone()) else {
        return Ok(WorkingOn::Stale);
    };
    match sessions.summary_of(&SessionId(run)).await {
        Ok(Some(summary)) if summary.state == SessionState::Active => Ok(WorkingOn::Live(value)),
        Ok(_) => Ok(WorkingOn::Stale),
        Err(e) => {
            tracing::warn!(error = %e, %bot, "could not read the session behind working_on");
            Ok(WorkingOn::Stale)
        }
    }
}

/// **Whether a write moves a piece of work to `now`**: the call set `status`
/// (not a field of the claim alone) and the word it set is `now`.
pub(crate) fn moves_to_now(fields: &BTreeMap<String, String>, sets: &BTreeSet<String>) -> bool {
    sets.contains("status")
        && fields
            .get("status")
            .is_some_and(|word| word.trim() == "now")
}

impl Jojobot {
    /// **Declare or clear what the caller's bot is working on.** The write is
    /// the caller's session's: the record keeps every earlier declaration and
    /// the session that made each.
    pub(crate) async fn set_working_on(
        &self,
        caller: &crate::caller::Caller,
        what: Declared,
        today: jiff::civil::Date,
    ) -> Result<(), MemoryError> {
        let session = Some(caller.sid.as_str().to_string());
        let record = self.working_on_record(&caller.bot).await?;
        match (record, what) {
            (Some(address), Declared::At(thing)) => {
                self.memory
                    .update_fact(
                        &address,
                        FactPatch {
                            fields: [(WORKING_ON.to_string(), thing.as_str().to_string())].into(),
                            sets: [WORKING_ON.to_string()].into(),
                            session,
                            ..FactPatch::default()
                        },
                        &caller.bot,
                    )
                    .await?;
            }
            (Some(address), Declared::Nothing) => {
                self.memory
                    .update_fact(
                        &address,
                        FactPatch {
                            clear_fields: vec![WORKING_ON.to_string()],
                            session,
                            ..FactPatch::default()
                        },
                        &caller.bot,
                    )
                    .await?;
            }
            (None, Declared::At(thing)) => {
                let mut fact = NewFact::about(caller.bot.clone(), RECORD, today);
                fact.fields = [(WORKING_ON.to_string(), thing.as_str().to_string())].into();
                fact.sets = [WORKING_ON.to_string()].into();
                fact.session = session;
                match self.memory.capture(fact).await? {
                    Guarded::Written(_) => {}
                    Guarded::Blocked { attempted, .. } => {
                        return Err(MemoryError::InvalidFact(format!(
                            "{attempted} is not a thing jojobot knows, so the bot cannot be \
                             pointed at it"
                        )));
                    }
                }
            }
            // Nothing was declared, so there is nothing to clear.
            (None, Declared::Nothing) => {}
        }
        Ok(())
    }

    /// **Point the acting bot at a piece of work it owns that this write moved
    /// to `now`.** Best-effort: the write that moved the work has landed, and a
    /// failure here is logged and never turns it into a failed call.
    pub(crate) async fn fill_working_on(
        &self,
        caller: &crate::caller::Caller,
        thing: &EntityId,
        moved_to_now: bool,
    ) {
        if !moved_to_now || thing.kind() != Some(EntityKind::WORK) {
            return;
        }
        let owner = match self.memory.fields(thing).await {
            Ok(held) => held.get("owner").cloned(),
            Err(e) => {
                tracing::warn!(error = %e, %thing, "could not read the owner of moved work");
                return;
            }
        };
        if owner.as_deref() != Some(caller.bot.as_str()) {
            return;
        }
        let today = match self.dated(None, Some(caller.sid.as_str())).await {
            Ok(day) => day,
            Err(_) => return,
        };
        if let Err(e) = self
            .set_working_on(caller, Declared::At(thing.clone()), today)
            .await
        {
            tracing::warn!(error = %e, %thing, "the bot's working_on was not filled");
        }
    }

    /// **The one read of a bot's `working_on`**, for the verbs of this server.
    pub(crate) async fn working_on_of(&self, bot: &EntityId) -> Result<WorkingOn, MemoryError> {
        working_on_of(&*self.memory, &*self.sessions, bot).await
    }

    /// The bot's one working-on record, if it has one.
    async fn working_on_record(&self, bot: &EntityId) -> Result<Option<FactAddress>, MemoryError> {
        let facts = match self.memory.recall(bot).await {
            Ok(facts) => facts,
            Err(MemoryError::UnknownEntity { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(facts
            .iter()
            .find(|f| f.content == RECORD && f.status == FactStatus::Active)
            .map(|f| f.address()))
    }

    /// **Take `working_on` out of every bot in an answer whose writing session
    /// is no longer live**, the bots a walk reached included. Answers with the
    /// bots it hid, so the caller can say so once.
    ///
    /// A bot whose key a live session wrote keeps it, and so does one nobody
    /// declared. A read that cannot decide hides the key: the stored value is
    /// never the fallback.
    pub(crate) async fn hide_stale_working_on(&self, objects: &mut [Object]) -> BTreeSet<EntityId> {
        fn bots_holding(objects: &[Object], into: &mut BTreeSet<EntityId>) {
            for object in objects {
                if object.fields.contains_key(WORKING_ON) {
                    into.insert(object.entity.id.clone());
                }
                bots_holding(&object.connected, into);
            }
        }
        fn strip(objects: &mut [Object], hidden: &BTreeSet<EntityId>) {
            for object in objects {
                if hidden.contains(&object.entity.id) {
                    object.fields.remove(WORKING_ON);
                }
                strip(&mut object.connected, hidden);
            }
        }
        let mut holding = BTreeSet::new();
        bots_holding(objects, &mut holding);
        let mut hidden = BTreeSet::new();
        for bot in holding {
            match self.working_on_of(&bot).await {
                Ok(WorkingOn::Live(_)) => {}
                Ok(WorkingOn::Unset) | Ok(WorkingOn::Stale) => {
                    hidden.insert(bot);
                }
                Err(e) => {
                    tracing::warn!(error = %e, %bot, "could not decide working_on, so it is not shown");
                    hidden.insert(bot);
                }
            }
        }
        strip(objects, &hidden);
        hidden
    }
}

/// **The one sentence an answer carries when it hid a stored `working_on`.**
pub(crate) fn hidden_note(bots: &BTreeSet<EntityId>) -> String {
    format!(
        "working_on of {} is not shown: the session that wrote it is no longer live, so what \
         the bot is doing now is unknown",
        bots.iter()
            .map(EntityId::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

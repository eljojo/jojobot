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

impl Jojobot {
    /// **Whether a write moves a piece of work to `now`**: the call set `status`
    /// (not a field of the claim alone) and the word it set means `now` on this
    /// instance, by the one rule that says what a status word means. The
    /// instance's record is read only when the call sets a status. `Err` is the
    /// read of that record that failed: whether the word means `now` is not
    /// known, and [`Self::fill_working_on`] says so on the receipt.
    pub(crate) async fn moves_to_now(
        &self,
        fields: &BTreeMap<String, String>,
        sets: &BTreeSet<String>,
    ) -> Result<bool, MemoryError> {
        if !sets.contains(jojobot_domain::memory::kinds::STATUS) {
            return Ok(false);
        }
        let Some(word) = fields.get(jojobot_domain::memory::kinds::STATUS) else {
            return Ok(false);
        };
        // A shipped word means itself whatever the instance maps, so the record
        // is read only for a word of the instance's own.
        if let Some(shipped) = jojobot_domain::memory::kinds::meaning_of(word, &BTreeMap::new()) {
            return Ok(shipped == "now");
        }
        // A write is never refused for a record it could not read; the caller
        // names what it could not work out.
        let instance = self.instance_fields().await?;
        Ok(jojobot_domain::memory::kinds::meaning_of(word, &instance) == Some("now"))
    }

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
        moved_to_now: &Result<bool, MemoryError>,
        unread: &super::instance_unread::InstanceUnread,
    ) {
        if matches!(moved_to_now, Ok(false)) || thing.kind() != Some(EntityKind::WORK) {
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
        // **Owned work whose status could not be read as `now` or not**: the
        // write landed and `working_on` is not filled; the receipt says so.
        if let Err(e) = moved_to_now {
            unread.left(super::instance_unread::WORKING_ON, e);
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

    /// The bot's one working-on record, if it has one.
    async fn working_on_record(&self, bot: &EntityId) -> Result<Option<FactAddress>, MemoryError> {
        working_on_record_of(&*self.memory, bot).await
    }

    /// **The bots whose stored `working_on` is not shown**: the session that
    /// wrote it is gone. Read from the one gate, [`working_on_of`]; a bot whose
    /// session cannot be decided is counted as gone, so the stored value is never
    /// the fallback.
    pub(crate) async fn stale_working_on_bots(&self) -> BTreeSet<EntityId> {
        stale_working_on_bots(&*self.memory, &*self.sessions).await
    }
}

/// The bot's one working-on record, if it has one.
async fn working_on_record_of(
    memory: &dyn Memory,
    bot: &EntityId,
) -> Result<Option<FactAddress>, MemoryError> {
    let facts = match memory.recall(bot).await {
        Ok(facts) => facts,
        Err(MemoryError::UnknownEntity { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(facts
        .iter()
        .find(|f| f.content == RECORD && f.status == FactStatus::Active)
        .map(|f| f.address()))
}

/// **One time a session declared what its bot is working on**, read from the
/// history of the bot's one working-on record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The bot that declared it.
    pub bot: EntityId,
    /// The thing it declared.
    pub on: EntityId,
    /// The public name of the run that declared it, when that can be read.
    pub run: Option<String>,
    /// When the store took the declaration in, when it recorded one.
    pub at: Option<jiff::Timestamp>,
    /// Whether this is the declaration the bot is working on NOW: the one
    /// [`working_on_of`] shows. Every other is a past one.
    pub live: bool,
}

/// **Every declaration by `bots` that points at a thing in `of`**, live or past,
/// oldest first for each bot, and whether the history of any bot could not be
/// read whole (then what is returned is what could be).
///
/// Whether a declaration is live is [`working_on_of`]'s answer and nothing else
/// (rule 498): the bot's newest declaration is live when that gate shows it.
/// Nothing here looks at a session's state.
pub async fn declarations_on(
    memory: &dyn Memory,
    sessions: &dyn Sessions,
    bots: &[EntityId],
    of: &BTreeSet<EntityId>,
) -> (Vec<Declaration>, bool) {
    let mut found = Vec::new();
    let mut incomplete = false;
    // **A write of a key that names a thing is kept as that thing's permanent id**,
    // so a stored value is turned back into the handle the thing wears now.
    let known = match memory.list_entities(None).await {
        Ok(known) => known,
        Err(_) => return (found, true),
    };
    for bot in bots {
        let Ok(Some(record)) = working_on_record_of(memory, bot).await else {
            continue;
        };
        let (Ok(values), Ok(chain)) = (
            memory.history(bot, WORKING_ON).await,
            memory.claim_history(&record).await,
        ) else {
            incomplete = true;
            continue;
        };
        // Every write of this record sets or clears the key, so the two
        // histories run side by side; when they do not, the pairing is not known
        // and nothing is claimed from it.
        if values.len() != chain.len() {
            incomplete = true;
            continue;
        }
        let live_on = match working_on_of(memory, sessions, bot).await {
            Ok(WorkingOn::Live(value)) => Some(value),
            _ => None,
        };
        let newest = values.len().saturating_sub(1);
        for (at, (write, claim)) in values.iter().zip(&chain).enumerate() {
            let Some(on) = write.value.as_deref().map(|stored| {
                jojobot_domain::memory::entity_wearing(stored, &known)
                    .map_or_else(|| EntityId(stored.to_string()), |thing| thing.id.clone())
            }) else {
                continue;
            };
            if !of.contains(&on) {
                continue;
            }
            let run = match &claim.session {
                Some(id) => match sessions.summary_of(&SessionId(id.clone())).await {
                    Ok(Some(summary)) => summary.sid.map(|sid| sid.as_str().to_string()),
                    _ => None,
                },
                None => None,
            };
            found.push(Declaration {
                bot: bot.clone(),
                live: at == newest && live_on.as_deref() == Some(on.as_str()),
                on,
                run,
                at: claim.written_at,
            });
        }
    }
    (found, incomplete)
}

/// **The bots whose stored `working_on` is not shown**, for any surface that
/// holds the two ports.
pub async fn stale_working_on_bots(
    memory: &dyn Memory,
    sessions: &dyn Sessions,
) -> BTreeSet<EntityId> {
    let mut stale = BTreeSet::new();
    let bots = match memory.list_entities(Some(EntityKind::BOT)).await {
        Ok(bots) => bots,
        Err(e) => {
            tracing::warn!(error = %e, "could not list the bots to decide working_on");
            return stale;
        }
    };
    for bot in bots {
        match memory.fields(&bot.id).await {
            Ok(held) if held.contains_key(WORKING_ON) => {}
            _ => continue,
        }
        match working_on_of(memory, sessions, &bot.id).await {
            Ok(WorkingOn::Live(_)) => {}
            Ok(WorkingOn::Unset) => {}
            Ok(WorkingOn::Stale) => {
                stale.insert(bot.id);
            }
            Err(e) => {
                tracing::warn!(error = %e, bot = %bot.id, "could not decide working_on, so it is not shown");
                stale.insert(bot.id);
            }
        }
    }
    stale
}

/// **The keys a walk reads as not held**, for these bots: their `working_on`.
pub fn masked_working_on(
    stale: &BTreeSet<EntityId>,
) -> std::collections::BTreeMap<EntityId, Vec<String>> {
    stale
        .iter()
        .map(|bot| (bot.clone(), vec![WORKING_ON.to_string()]))
        .collect()
}

/// **The stale bots an answer holds**, the bots a walk reached included, so the
/// answer names only the ones it left a value out of.
pub fn stale_in(objects: &[Object], stale: &BTreeSet<EntityId>) -> BTreeSet<EntityId> {
    let mut present = BTreeSet::new();
    fn visit(objects: &[Object], stale: &BTreeSet<EntityId>, into: &mut BTreeSet<EntityId>) {
        for object in objects {
            if stale.contains(&object.entity.id) {
                into.insert(object.entity.id.clone());
            }
            visit(&object.connected, stale, into);
        }
    }
    visit(objects, stale, &mut present);
    present
}

/// **The one sentence an answer carries when it hid a stored `working_on`.**
pub(crate) fn hidden_note(bots: &BTreeSet<EntityId>) -> String {
    format!(
        "working_on of {} is not shown: the session that wrote it is no longer live, so what \
         the bot is doing now is unknown. recall the bot with facts: true reads the last value \
         it set, marked working_on_stale",
        bots.iter()
            .map(EntityId::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

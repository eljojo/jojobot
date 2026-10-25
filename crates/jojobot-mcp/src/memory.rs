//! **Memory** — what jojobot knows: typed entities, and dated claims about them.
//!
//! One file per verb. Each holds that verb's arguments, the description a
//! caller reads, and an entrypoint thin enough to read in one go: it parses the
//! call, chains the systems below it, and renders what came back. The logic
//! those bodies call is next door — [`parse`] the input grammar, [`wire`] the
//! response vocabulary, [`declined`] the refusals.

use rmcp::{
    ErrorData as McpError, handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
};

use crate::*;

pub mod add_entity;
pub mod archive_entity;
mod archive_mark;
#[cfg(test)]
mod archive_mark_tests;
pub mod capture;
mod capture_batch;
mod capture_schedule;
mod capture_standing;
pub mod claim_room;
pub(crate) mod clauses;
pub mod declare_type;
#[cfg(test)]
mod declare_type_describing_keys;
pub mod declined;
pub mod documents;
pub mod due;
#[cfg(test)]
mod due_unread;
mod fillers;
#[cfg(test)]
mod fillers_tests;
pub(crate) mod instance_unread;
#[cfg(test)]
mod keys_kept_tests;
pub mod left_out;
pub mod list_delta;
pub mod list_entities;
pub mod list_linked;
pub mod list_move;
pub mod list_plan;
pub mod list_ticks;
pub mod merge_entities;
pub(crate) mod must_override;
pub mod parse;
mod placement_gate;
pub mod possible_subject;
pub mod put_document;
mod reach_missed;
#[cfg(test)]
mod reach_missed_tests;
pub mod read_document;
pub(crate) mod reads_left;
#[cfg(test)]
mod reads_left_tests;
pub mod ready;
pub mod recall;
mod recall_members;
#[cfg(test)]
mod recall_stored_types;
pub mod receipt;
pub mod rename_entity;
pub mod retract;
mod review;
pub(crate) mod room;
pub mod search;
pub mod set_charter;
pub mod starred;
pub mod status_words;
#[cfg(test)]
pub mod testing;
pub mod unlocks;
pub mod update_entity;
pub mod update_fact;
pub mod view_run;
pub mod wire;
pub mod working_on;

// **Re-exported for the verb files, which reach them through `use super::*`.**
// Named modules, so a reader asking "where does `blocked_result` live" gets an
// answer from the import list; the glob is a convenience at ONE hop, not a
// second home for the items.
pub use add_entity::AddEntityArgs;
pub use capture::CaptureArgs;
pub use declare_type::{DeclareTypeArgs, FieldArgs};
pub use list_entities::ListEntitiesArgs;
pub use put_document::PutDocumentArgs;
pub use read_document::ReadDocumentArgs;
pub use recall::RecallArgs;
pub use rename_entity::RenameEntityArgs;
pub use retract::RetractArgs;
pub use search::{EdgeFilterArgs, SearchArgs};
pub use set_charter::SetCharterArgs;
pub use update_entity::UpdateEntityArgs;
pub use update_fact::UpdateFactArgs;

pub(crate) use declined::*;
pub(crate) use parse::*;
pub(crate) use wire::*;

/// This context's half of the surface — one router per verb file, summed.
///
/// **A verb is added by adding a file**, and it reaches the surface by being
/// named here. Nothing scans a directory: an unlisted verb is invisible, which
/// is the same deliberate friction `the_tool_surface_is_exactly_this_list` puts
/// in front of a new tool.
pub(crate) fn router() -> ToolRouter<Jojobot> {
    Jojobot::add_entity_router()
        + Jojobot::archive_entity_router()
        + Jojobot::capture_router()
        + Jojobot::declare_type_router()
        + Jojobot::list_entities_router()
        + Jojobot::merge_entities_router()
        + Jojobot::put_document_router()
        + Jojobot::read_document_router()
        + Jojobot::recall_router()
        + Jojobot::rename_entity_router()
        + Jojobot::retract_router()
        + Jojobot::search_router()
        + Jojobot::set_charter_router()
        + Jojobot::update_entity_router()
        + Jojobot::update_fact_router()
}

impl Jojobot {
    /// **How many runs a thought of this bot may go untouched before it ages
    /// out of the count**: the bot's own setting, else the default.
    ///
    /// Read by the two verbs that compute a cutoff before they write, so the
    /// number a write is judged on is the one a later read reports.
    pub(crate) async fn ages_after_runs_of(&self, bot: &EntityId) -> Result<usize, MemoryError> {
        Ok(jojobot_domain::memory::ages_after_runs_of(
            &self.memory.fields(bot).await?,
        ))
    }

    /// **The handle a thing answers to now, for a name it may have worn once.**
    ///
    /// A guard that compares who a write is about with who is making it must
    /// compare current handles: a session is rebound to its bot's new handle
    /// on a rename, while the subject of a write is whatever the caller typed.
    /// A name nothing ever answered to comes back unchanged, so the write's
    /// own refusal for an unknown entity still speaks.
    pub(crate) async fn current_handle(&self, id: &EntityId) -> Result<EntityId, MemoryError> {
        let known = self.memory.list_entities(None).await?;
        let former = self.memory.former_handles().await?;
        Ok(jojobot_domain::memory::resolve_handle(id, &known, &former)
            .map_or_else(|| id.clone(), |entity| entity.id.clone()))
    }

    /// **The handle of the thing a write or a read of one thing starts from**: a
    /// member's number is the member that holds it, and every other name is kept
    /// as sent, a former handle included, which these writes refuse. The one
    /// place a verb that took a handle from its caller canonicalises it before it
    /// measures or reads the thing as it stands; a gate that judges its kind asks
    /// [`Self::kind_now`], which is built on this.
    pub(crate) async fn row_handle(&self, id: &EntityId) -> Result<EntityId, MemoryError> {
        match jojobot_domain::memory::numbered::numbered_handle(id) {
            Some(_) => self.current_handle(id).await,
            None => Ok(id.clone()),
        }
    }

    /// **The kind the store holds for the thing a caller named**, for a gate that
    /// judges kind. A caller's spelling can carry the kind the thing had before a
    /// retype. A handle no rename ever left behind is its own current handle, so
    /// its kind is its prefix and the store is not listed; only a former handle is
    /// read through [`Self::current_handle`], and a number is already resolved by
    /// [`Self::row_handle`].
    pub(crate) async fn kind_now(&self, id: &EntityId) -> Result<Option<EntityKind>, MemoryError> {
        let sent = self.row_handle(id).await?;
        let former = self.memory.former_handles().await?;
        if former.iter().any(|left| left.former == sent) {
            return Ok(self.current_handle(&sent).await?.kind());
        }
        Ok(sent.kind())
    }

    /// **Wait for the right to place sections on `thing`**, and hold it until the
    /// guard drops. **The lock is on the thing and not on any name for it**: the
    /// key is the permanent id the handle resolves to, so a handle the thing used
    /// to wear, the one it wears now and the one it will wear after a rename that
    /// lands while a caller is on its way to the lock all take one lock. A thing
    /// that wears no permanent id is keyed by its current handle, as the store
    /// keys its rows, and a name that answers to nothing by the name as sent.
    ///
    /// This is the one place that decides the key; every caller that places
    /// sections passes the name it has and takes what this returns.
    pub(crate) async fn hold_placing(
        &self,
        thing: &EntityId,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, MemoryError> {
        let key = self.placing_key(thing).await?;
        Ok(self.registry.hold_placing(&key).await)
    }

    /// **The placement locks of two things, taken in one fixed order.** A merge
    /// reads the entries of both lists and mints keys on the survivor from them,
    /// so a capture on either side must not land between that read and the
    /// merge's write. The locks are taken in the order of their keys, and once
    /// when both names answer to one thing, so two merges that name the same two
    /// lists the other way round cannot each hold one and wait for the other.
    pub(crate) async fn hold_placing_both(
        &self,
        a: &EntityId,
        b: &EntityId,
    ) -> Result<Vec<tokio::sync::OwnedMutexGuard<()>>, MemoryError> {
        let mut keys = vec![self.placing_key(a).await?, self.placing_key(b).await?];
        keys.sort();
        keys.dedup();
        let mut held = Vec::with_capacity(keys.len());
        for key in keys {
            held.push(self.registry.hold_placing(&key).await);
        }
        Ok(held)
    }

    /// The key a thing's placement lock is held under.
    pub(crate) async fn placing_key(&self, thing: &EntityId) -> Result<String, MemoryError> {
        let known = self.memory.list_entities(None).await?;
        let former = self.memory.former_handles().await?;
        Ok(
            match jojobot_domain::memory::resolve_handle(thing, &known, &former) {
                Some(entity) => match &entity.badge {
                    Some(badge) => format!("badge:{badge}"),
                    None => format!("handle:{}", entity.id.as_str()),
                },
                None => format!("handle:{}", thing.as_str()),
            },
        )
    }

    /// **A type name, resolved to the declaration it stands for — once, here.**
    ///
    /// Everything below this point takes the KEYS rather than the name, so
    /// nothing deeper needs a store to answer a type question. A name nobody
    /// declared is answered here too, where the roster to offer instead is in
    /// reach.
    ///
    /// Shared by the two verbs that take a type, so the refusal a caller reads
    /// cannot come to differ between them. `did` names what the call was doing
    /// when it stopped, because "nothing was searched" and "nothing was
    /// recalled" are the same sentence about different work.
    ///
    /// **Two nested answers, because there are two channels and this decides
    /// which.** The outer `Err` is a failure the verb hands straight on with
    /// `?`; the inner `Err` is a blocked body — a successful result the caller
    /// acts on. A single error type here would have let each call site pick,
    /// and both of them picked the blocked one for an outage.
    pub(crate) async fn declared(
        &self,
        wanted: &str,
        did: &'static str,
    ) -> Result<Result<(DeclaredType, Option<Displaced>), CallToolResult>, McpError> {
        let known = match self.memory.declared_types().await {
            Ok(known) => known,
            // **Not a name mistake — the store is down.** `declared_types` has
            // exactly one fallible step, so nothing else is reachable here, and
            // a blocked body would tell a client branching on `status`, as the
            // published instructions tell it to, that the outage was its own
            // call to fix. Which faults are the caller's is `memory_declined`'s
            // one answer; asking it is what keeps a second copy of that line
            // from drifting away from it.
            Err(e) => return memory_declined(did, e).map(Err),
        };
        if let Some(found) = DeclaredType::named(&known, wanted) {
            // **A name that resolves may still have displaced a caller's own
            // declaration** (rule 68): the query is about to match against
            // the type this answers with, and if that used to be a caller's,
            // with different keys, silence here is the same silence
            // `declare_type`'s own refusal used to carry.
            let displaced = match self.memory.displaced_type(&found.name).await {
                Ok(displaced) => displaced,
                Err(e) => return memory_declined(did, e).map(Err),
            };
            return Ok(Ok((found.clone(), displaced)));
        }
        let names: Vec<&str> = known.iter().map(|t| t.name.as_str()).collect();
        Ok(Err(blocked_body(
            "Declared",
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was {did} for a type: no type is called '{wanted}'. Declaring a type is \
                 write-time help and never a precondition — a record is found by the keys it \
                 carries — so if you know the keys, ask for them another way rather than \
                 declaring a type to reach them. Types that do exist: {}.",
                if names.is_empty() {
                    "none yet".to_string()
                } else {
                    names.join(", ")
                }
            ),
        )))
    }
}

//! **A container's number is raised by someone above the bots that filled it**
//! (containers.md: a session cannot widen the container it is filling).
//!
//! The licence of `record_capacity` is the domain's `WritersSuperior` rule: not
//! a bot that writes into the thing, and, when there are any, a bot above one of
//! them. What the domain cannot read is who the writers are. A thread's thought
//! writers are the chart read already; the bots that wrote the records of a
//! thing are found here, from the run behind the first write of each live
//! record, and read into the lineage the licence is asked with.
//!
//! **The assistant is the one exemption**, named by the domain's
//! `is_the_assistant`, which the domain's licence asks too for a thread's thought
//! writers.
//!
//! **One helper for every verb that sets or clears the number** (rule 498):
//! `capture`, `add_entity` and `update_entity` reach it through
//! [`Jojobot::refuses_an_unlicensed_key`], and `update_fact` calls it itself.
//!
//! **The lineage is read here, before the write, and not inside it.** A raise
//! that races the first write of a bot that has written nothing yet is judged
//! against the writers there were a moment earlier, and it slips through. The
//! window is the same one the chart read in [`Jojobot::chart_around`] accepts.

use super::*;
use jojobot_domain::memory::{Lineage, capacity};
use jojobot_domain::session::SessionId;
use std::collections::BTreeMap;

impl Jojobot {
    /// **The refusal for a caller that fills a thing and sets or clears its
    /// record number**, or none when the write does not move the number or the
    /// caller may move it. `sent` is what the CALLER sent and `cleared` the keys
    /// it asked to take off.
    pub(crate) async fn refuses_a_filler_widening(
        &self,
        verb: &'static str,
        subject: &EntityId,
        caller: &EntityId,
        sent: &BTreeMap<String, String>,
        cleared: &[String],
    ) -> Result<Option<CallToolResult>, McpError> {
        let key = capacity::RECORD_CAPACITY;
        let written: BTreeMap<String, String> = sent
            .iter()
            .filter(|(sent_key, _)| sent_key.trim() == key)
            .map(|(sent_key, value)| (sent_key.clone(), value.clone()))
            .collect();
        let clears = cleared.iter().any(|cleared_key| cleared_key.trim() == key);
        if written.is_empty() && !clears {
            return Ok(None);
        }
        // **The assistant is exempt**, by the operator's ruling: it fills the
        // containers it works in in the operator's name, and raises their numbers
        // as the operator would. It is named by the identity the build ships, the
        // one the instance boots and the one the root voucher is for. Every other
        // writer is held to the rule below.
        if jojobot_domain::memory::is_the_assistant(caller) {
            return Ok(None);
        }
        // **A number that is no number is refused for that**, by the check that
        // knows what the key holds, before it is asked who may write it.
        if written
            .values()
            .any(|value| capacity::ceiling_value_refusal(key, value).is_some())
        {
            return Ok(None);
        }
        let subject = match self.current_handle(subject).await {
            Ok(current) => current,
            Err(e) => return memory_declined(verb, e).map(Some),
        };
        // **A resend of the number the thing holds is not a change**, and a clear
        // of a number it does not hold is not one either.
        let moving = if written.is_empty() {
            match self.memory.fields(&subject).await {
                Ok(held) if held.contains_key(key) => {
                    BTreeMap::from([(key.to_string(), String::new())])
                }
                Ok(_) => BTreeMap::new(),
                Err(e) => return memory_declined(verb, e).map(Some),
            }
        } else {
            match self.fields_that_would_change(&subject, &written).await {
                Ok(changing) => changing,
                Err(e) => return memory_declined(verb, e).map(Some),
            }
        };
        if moving.is_empty() {
            return Ok(None);
        }
        let lineage = match self.chart_with_record_writers(&subject).await {
            Ok(lineage) => lineage,
            Err(e) => return memory_declined(verb, e).map(Some),
        };
        match jojobot_domain::memory::refuses_unlicensed_write(
            &subject,
            caller,
            &moving,
            Some(&lineage),
        ) {
            Some(refused) => memory_declined(verb, refused).map(Some),
            None => Ok(None),
        }
    }

    /// **The chart around a thing, with the bots that wrote its records among its
    /// writers.** A thread's thought writers are there already; a bot that wrote a
    /// record and no thought is added, with the bots above it.
    async fn chart_with_record_writers(&self, subject: &EntityId) -> Result<Lineage, MemoryError> {
        let mut lineage = self.chart_around(subject, None).await?;
        for writer in self.record_writers(subject).await? {
            for bot in self.chain_above(&writer).await? {
                if !lineage.above_writers.contains(&bot) {
                    lineage.above_writers.push(bot);
                }
            }
            if !lineage.writers.contains(&writer) {
                lineage.writers.push(writer);
            }
        }
        Ok(lineage)
    }

    /// **The bots that wrote the live records of a thing**: the bot behind the
    /// run that made the first write of each. A record that sets the number is
    /// the setting of it and not a fill, so its writer is not one; a record that
    /// takes no slot is not a fill either. A write with no run behind it names
    /// nobody. A read that fails refuses the write rather than waving it through.
    async fn record_writers(&self, subject: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        let mut writers: Vec<EntityId> = Vec::new();
        for fact in self.memory.recall(subject).await?.iter().filter(|fact| {
            fact.status == FactStatus::Active
                && !capacity::takes_no_slot(&fact.fields)
                && !fact.sets.contains(capacity::RECORD_CAPACITY)
        }) {
            let chain = self.memory.claim_history(&fact.address()).await?;
            let Some(run) = chain.first().and_then(|write| write.session.clone()) else {
                continue;
            };
            let summary = self
                .sessions
                .summary_of(&SessionId(run))
                .await
                .map_err(|e| MemoryError::Store(format!("the run behind a record: {e}")))?;
            if let Some(summary) = summary
                && !writers.contains(&summary.bot)
            {
                writers.push(summary.bot);
            }
        }
        Ok(writers)
    }
}

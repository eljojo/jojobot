//! **A bot's room, counted the one way.**
//!
//! The receipt of a write, the room read and the check that admits a thought
//! all say how full a room is. They take the answer from here, which takes it
//! from [`count_room`], so none of them can count a quiet thought as live while
//! another leaves it out.

use super::*;
use jojobot_domain::memory::{RoomCount, THOUGHT_CAPACITY, count_room, thought_room};

/// A counted room, and the threshold that decided what is quiet.
pub(crate) struct RoomView {
    pub(crate) count: RoomCount,
    /// The runs a thought may go untouched, and whether the bot's own setting
    /// said so (else the default did).
    pub(crate) ages_after: usize,
    pub(crate) from_setting: bool,
    /// The bot's runs could not be read, so no thought was counted quiet: `live`
    /// may hold thoughts a read of the runs would have left out.
    pub(crate) ages_unread: bool,
}

impl RoomView {
    /// **The block a room read carries.** Sent when a thought has gone quiet or
    /// the room is over its ceiling, so a session reads either from the answer it
    /// already has. When one is quiet, it also says the call that brings it back.
    pub(crate) fn block(&self) -> serde_json::Value {
        let mut block = serde_json::json!({
            "capacity": self.count.capacity,
            "live": self.count.live.len(),
            "aged_out": self.count.aged_out.len(),
            "ages_after_runs": self.ages_after,
            "ages_after_from": if self.from_setting {
                jojobot_domain::memory::THOUGHT_AGES_AFTER_RUNS
            } else {
                "default"
            },
        });
        if self.count.over() {
            block["over"] = true.into();
        }
        if !self.count.aged_out.is_empty() {
            block["bring_back"] = "update_fact with keep: true on the address of a quiet thought \
                                   brings it back. A room already at its ceiling refuses it \
                                   until a live thought is archived."
                .into();
        }
        block
    }
}

impl Jojobot {
    /// **Count the room `bot` holds**, from its claims and its fields. `None`
    /// for a bot with no capacity or no thought in it: the count is an addition
    /// to an answer and never a condition of one. **Runs that cannot be read do
    /// not make it `None`**: the room is counted with nothing quiet, and the
    /// view says so in `ages_unread`, so a caller that has a debt to report
    /// still reports it and says how the count was made.
    pub(crate) async fn count_room_of(
        &self,
        bot: &EntityId,
        facts: &[Fact],
        fields: &std::collections::BTreeMap<String, String>,
    ) -> Option<RoomView> {
        let capacity = fields
            .get(THOUGHT_CAPACITY)
            .and_then(|v| v.trim().parse::<usize>().ok())?;
        let nominal = thought_room(facts);
        if nominal.is_empty() {
            return None;
        }
        let mut touched = std::collections::HashMap::new();
        for fact in &nominal {
            if let Ok(chain) = self.memory.claim_history(&fact.address()).await
                && let Some(at) = chain.last().and_then(|w| w.written_at)
            {
                touched.insert(fact.id.clone(), at);
            }
        }
        let runs = self.sessions.summaries_of(bot).await.ok();
        let setting = jojobot_domain::memory::ages_after_runs_setting(fields);
        let ages_after = setting.unwrap_or(jojobot_domain::memory::AGES_AFTER_RUNS);
        let cutoff = runs.as_ref().and_then(|runs| {
            jojobot_domain::memory::aging_cutoff(
                &jojobot_domain::session::history::started_ats(runs),
                ages_after,
            )
        });
        Some(RoomView {
            count: count_room(nominal, &touched, cutoff, capacity),
            ages_after,
            from_setting: setting.is_some(),
            ages_unread: runs.is_none(),
        })
    }
}

impl Jojobot {
    /// **Whether an edit to a bot's claim could put a thought into its room:** it
    /// draws a connection edge or brings the record back to active, or the record
    /// already is a thought and the edit touches it. A record that cannot be read
    /// counts as one that could, so a doubt costs a read and never a missed check.
    pub(crate) async fn could_join_a_room(&self, address: &FactAddress, patch: &FactPatch) -> bool {
        if jojobot_domain::memory::authorship::is_connection(patch.edge.as_ref())
            || patch.status == Some(FactStatus::Active)
        {
            return true;
        }
        match self.memory.recall(&address.home).await {
            Ok(held) => held
                .iter()
                .find(|fact| fact.id == address.local)
                .is_none_or(|fact| !thought_room(std::slice::from_ref(fact)).is_empty()),
            Err(_) => true,
        }
    }
}

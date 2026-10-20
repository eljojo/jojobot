//! **A star into full seats is refused before it lands (rule 591)**, and the
//! unstars a session makes are counted.
//!
//! `capture` and `update_fact` both ask here before a write that stars a rule
//! on a bot: when the bot's seats are full, the write is refused, naming the
//! rules that hold the seats, and the writer unstars one first. Unstarring and
//! editing a starred rule always land.

use super::*;

/// **What a record is told when it lands beside three or more unstars and
/// stands for nothing.** An unstarred rule is kept but no longer rides the boot,
/// and nothing leads back to it from a record that does not stand for it.
pub(crate) const UNSTARS_WITHOUT_A_SUMMARY: &str = "The rules you unstarred this session are kept but \
    do not ride the boot, and nothing leads to them from this record: mark it with stands_for \
    naming them, and a read of it can return its sources.";

/// How many unstars a session makes before a record beside them is taught.
const UNSTARS_BEFORE_TEACHING: usize = 3;

impl Jojobot {
    /// **Count this write if it unstarred a rule**: a bot's own claim that was
    /// starred and no longer is. `was_starred` is read before the write; the
    /// claim's fields after it say whether the star is still there.
    pub(crate) fn note_unstarred(&self, fact: &Fact, was_starred: bool, sid: &str) {
        if was_starred
            && fact.subject.kind() == Some(EntityKind::BOT)
            && fact.fields.get("starred").is_none_or(|v| v != "true")
        {
            self.registry.note_unstar(sid, fact.subject.as_str());
        }
    }

    /// **Teach, on the receipt of a record that is not itself an unstar, that
    /// the rules this session unstarred are not reachable from it.** Only for a
    /// bot's own record that stands for nothing, once the session has unstarred
    /// three or more rules of that bot.
    pub(crate) fn note_unstars_without_a_summary(
        &self,
        fact: &Fact,
        unstarred_this_write: bool,
        sid: &str,
        body: &mut serde_json::Value,
    ) {
        if unstarred_this_write
            || fact.subject.kind() != Some(EntityKind::BOT)
            || !fact.stands_for.is_empty()
            || self.registry.unstarred_by(sid, fact.subject.as_str()) < UNSTARS_BEFORE_TEACHING
        {
            return;
        }
        crate::answer::note_teaching(body, UNSTARS_WITHOUT_A_SUMMARY);
    }

    /// **The refusal for a capture that would star a rule into full seats**,
    /// or `None` (rule 591). Asked before the write, beside the boot-size
    /// refusal, and with the same race window: two stars sent at the same
    /// moment both read the seats before either lands, so both can land.
    pub(crate) async fn refuses_a_star_for_capture(&self, new: &NewFact) -> Option<MemoryError> {
        if new.subject.kind() != Some(EntityKind::BOT)
            || new.fields.get("starred").is_none_or(|v| v != "true")
        {
            return None;
        }
        self.refuses_a_star(&new.subject, None).await
    }

    /// **The same refusal for an edit that leaves a rule starred.** A rule
    /// already starred takes no new seat, so an edit of its words, or an edit
    /// that takes the star off, always lands. The same race window as the
    /// capture's.
    pub(crate) async fn refuses_a_star_for_edit(
        &self,
        address: &FactAddress,
        patch: &FactPatch,
    ) -> Option<MemoryError> {
        if address.home.kind() != Some(EntityKind::BOT) {
            return None;
        }
        let held = self.memory.recall(&address.home).await.ok()?;
        let mut edited = held.iter().find(|fact| fact.id == address.local)?.clone();
        jojobot_domain::memory::apply_fact_patch(&mut edited, patch).ok()?;
        if edited.fields.get("starred").is_none_or(|v| v != "true") {
            return None;
        }
        self.refuses_a_star_over(&address.home, held, Some(address))
            .await
    }

    /// The seats of `bot` as they stand, asked of the one admission.
    async fn refuses_a_star(
        &self,
        bot: &EntityId,
        written: Option<&FactAddress>,
    ) -> Option<MemoryError> {
        let held = self.memory.recall(bot).await.ok()?;
        self.refuses_a_star_over(bot, held, written).await
    }

    async fn refuses_a_star_over(
        &self,
        bot: &EntityId,
        held: Vec<Fact>,
        written: Option<&FactAddress>,
    ) -> Option<MemoryError> {
        let in_force = jojobot_domain::memory::rules_in_force(&held);
        // A count that cannot be read is no reason to refuse: the default would
        // refuse a star the bot has seats for.
        let seats = jojobot_domain::memory::rule_seats_of(&self.memory.fields(bot).await.ok()?);
        jojobot_domain::memory::seats::refuses_a_star(bot, &in_force, written, seats)
    }
}

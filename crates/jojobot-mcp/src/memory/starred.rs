//! **A starred rule that no longer fits the boot's seats — named on the write
//! that pushed it off, not only at the next boot.**
//!
//! `capture` and `update_fact` both call through here after a write lands on
//! a bot's own claim: if that write is what set `fields.starred == "true"`
//! and the bot's starred, in-force rules now exceed the boot's seats, this
//! names which one no longer rides — the oldest — and carries the same
//! sentence the boot itself shows once its seats are full. The write is
//! never refused for this.

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

    /// **Note it on the receipt when this write just pushed a starred rule
    /// off the boot.** A no-op unless `fact`'s subject is a bot and `fact`
    /// itself carries `fields.starred == "true"` — the one write shape that
    /// can grow the count at all.
    ///
    /// A read failure here adds nothing, silently: this is jojobot's own
    /// bookkeeping riding along on somebody else's write, and it must never
    /// be the reason that write fails or reads as one.
    pub(crate) async fn note_seat_pushed_off(&self, fact: &Fact, body: &mut serde_json::Value) {
        if fact.subject.kind() != Some(EntityKind::BOT)
            || fact.fields.get("starred").is_none_or(|v| v != "true")
        {
            return;
        }
        let Ok(all) = self.memory.recall(&fact.subject).await else {
            return;
        };
        let in_force = jojobot_domain::memory::rules_in_force(&all);
        let seat_count = jojobot_domain::memory::rule_seats_of(
            &self.memory.fields(&fact.subject).await.unwrap_or_default(),
        );
        let Some(seats) = jojobot_domain::memory::carried_seats_status(&in_force, seat_count)
        else {
            return;
        };
        let Some(dropped) = &seats.dropped else {
            return;
        };
        crate::answer::note_seat_dropped(body, &dropped.to_string(), &seats.sentence);
    }
}

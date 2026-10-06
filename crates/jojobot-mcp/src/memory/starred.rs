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

impl Jojobot {
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

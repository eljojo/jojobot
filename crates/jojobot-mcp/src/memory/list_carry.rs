//! **What a tick receipt teaches about carrying an entry to the next plan.**
//!
//! A list that covers a day or an ISO week is a plan. Carrying one of its open
//! entries to the next plan is a capture there with `derived_from` naming the
//! old entry. The single tick and the batch of ticks both carry this sentence
//! as the receipt's `hint`, the one teaching key a single-write receipt keeps,
//! from here (rule 498), so the two cannot teach it differently. Nothing is stored: the open state is read from the entries
//! after the ticks landed.

use jojobot_domain::memory::entries::{self, Setting};

use super::*;

/// The ledger key of the carry teaching: a session is told it once, by whichever
/// receipt reaches it first, a single tick or a batch of ticks.
pub(crate) const PLAN_CARRY_DOMAIN: &str = "plan-carry";

/// The sentence a tick receipt adds when an entry of a plan is still open.
const CARRY_LINE: &str = "To carry an open entry to the next plan, capture it there with \
    derived_from set to the old entry's address.";

impl Jojobot {
    /// Prepare the ordinary receipt's hint without spending it. The existing
    /// deferred-spend scope checks the same ledger and small-receipt gates as a
    /// batch; the receipt owner commits only the hint its composer actually kept.
    pub(crate) async fn pending_carry_teaching_after_ticks(
        &self,
        list: &EntityId,
        caller: &Caller,
    ) -> Option<&'static str> {
        crate::teaching::BATCH_SPENDS
            .scope(
                std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
                self.carry_teaching_after_ticks(list, caller),
            )
            .await
    }

    /// **The carry sentence for a list just ticked, or none.** None when the list
    /// covers no day or week, when no entry reads open after the ticks, or when
    /// the list could not be read: the tick landed, so a sentence that cannot be
    /// worked out is left off and logged, never a failure of the write.
    pub(crate) async fn carry_line_after_ticks(&self, list: &EntityId) -> Option<&'static str> {
        let unread = |error: &MemoryError| {
            tracing::warn!(%error, %list, "a tick receipt could not read its plan");
        };
        let fields = self.memory.fields(list).await.inspect_err(unread).ok()?;
        jojobot_domain::memory::types::period_span(fields.get("covers")?.trim())?;
        let held = self.memory.recall(list).await.inspect_err(unread).ok()?;
        let seen = entries::view(&held, Setting::of(&fields));
        seen.states
            .values()
            .any(|state| *state == entries::OPEN)
            .then_some(CARRY_LINE)
    }

    /// **The carry sentence a tick receipt delivers, or none.** The plan must read
    /// open first and the session's one teaching is spent after that, never
    /// before: a tick that leaves nothing open must not use it up.
    pub(crate) async fn carry_teaching_after_ticks(
        &self,
        list: &EntityId,
        caller: &Caller,
    ) -> Option<&'static str> {
        let carry = self.carry_line_after_ticks(list).await?;
        self.first_contact(PLAN_CARRY_DOMAIN, Some(caller))
            .await
            .then_some(carry)
    }
}

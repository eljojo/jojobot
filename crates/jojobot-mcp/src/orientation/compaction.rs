//! **A run reports that its context was compacted.**
//!
//! A compaction takes from a bot what jojobot taught it and showed it, and the
//! bot has no way to say so, so jojobot keeps eliding what the run no longer
//! holds. `start_here` with `compacted: true` and the run's live `sid` is that
//! way.
//!
//! **It reuses what a pick-up by name forgets and nothing else of it.** A
//! pick-up is a different agent taking an old run, so it voids vouchers, moves
//! the key and may reopen the run. A compaction is the same agent without its
//! context: the key, the vouchers, the chronology and the role claim stay as
//! they are, and no other run of the bot is touched.

use super::*;
use jojobot_domain::session::NewEntry;

/// The class of the chronology entry a report writes. It marks the entry as
/// jojobot's own and apart from anything the run said about itself.
const COMPACTION_BEAT: &str = "compaction";

impl Jojobot {
    /// Make the run forget what it was taught and shown, and write the mark.
    /// Answers the bot the handle belongs to, or the refusal.
    ///
    /// **Refused when the handle opens no run**, with the way forward: the
    /// report is about one run, and a guess at whose it is would make another
    /// run forget. `bot`, when named, has to be the handle's own bot.
    ///
    /// Done under the bot's gate, and in an order that can be sent again: what
    /// can fail comes before what cannot, so a refusal leaves the run as it was
    /// or forgotten once more.
    pub(crate) async fn report_compaction(
        &self,
        sid: Option<&str>,
        named: Option<&EntityId>,
    ) -> Result<EntityId, CallToolResult> {
        let gate = self.registry.gate(&self.gate_key(sid));
        let serialized = gate.lock().await;
        let caller = self.identified(sid)?;
        if let Some(named) = named
            && named != &caller.bot
        {
            return Err(misused(format!(
                "this sid belongs to {}, not to {named}. A compaction report is about the run \
                 the sid addresses: drop `bot`, or send the sid of a run of {named}",
                caller.bot
            )));
        }
        let run = caller.sid.as_str();
        if let Err(e) = self.teachings.forget(&caller.sid).await {
            tracing::warn!(error = %e, %run, "a compaction report could not forget what the run was taught");
            return Err(handle_declined(
                "",
                format!(
                    "Nothing was forgotten and run {run} is as it was: what it was taught \
                     could not be cleared. Send the same call again, and if it persists a \
                     person has to look at the store."
                ),
            ));
        }
        let card = match self
            .session_for(&serialized, &caller, None, Some("reported a compaction"))
            .await
        {
            Ok(card) => card,
            Err(e) => {
                tracing::warn!(error = ?e, %run, "a compaction report could not reach the run's record");
                return Err(handle_declined(
                    "",
                    format!(
                        "The run's record could not be reached, so the report was not written \
                         and run {run} is otherwise as it was. Send the same call again."
                    ),
                ));
            }
        };
        let mark = NewEntry::beat(
            COMPACTION_BEAT,
            "The run reported that its context was compacted: what it was taught and shown \
             before is forgotten, and the answers from here teach and show as to a fresh \
             session.",
            self.clock().now(),
            caller.day,
        );
        if let Err(e) = self.sessions.append(&card, mark).await {
            tracing::warn!(error = %e, %run, "a compaction report could not write its mark");
            return Err(handle_declined(
                "",
                format!(
                    "The mark could not be written, so the report is not complete: run {run} \
                     was made to forget what it was taught, and nothing else moved. Send the \
                     same call again."
                ),
            ));
        }
        // **The running total restarts after the mark**, the last step that can
        // fail. The answer to this call is counted by `finish` after this
        // returns, so the new total is what the run is handed from here on,
        // this answer included.
        if let Err(e) = self.sessions.reset_served(&card).await {
            tracing::warn!(error = %e, %run, "a compaction report could not restart the running total");
            return Err(handle_declined(
                "",
                format!(
                    "The running total of what run {run} has been handed could not be \
                     restarted, so the report is not complete. Send the same call again."
                ),
            ));
        }
        self.registry.forget_shown(run);
        Ok(caller.bot)
    }
}

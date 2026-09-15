//! **The stored due moment, kept current by any write that could move it.**
//!
//! `capture` and `update_fact` both call through here after assembling the
//! fields they are about to write and before committing: if those fields (or
//! a clear) touch a key any carrier's interface names, this reads the
//! thing's current fields, projects what they will read once the write
//! lands, and hands back what should happen to [`attention::DUE_ON`] — set
//! to a new date, cleared, or left alone.

use super::*;
use jojobot_domain::attention;

impl Jojobot {
    /// **What this write should do to the stored due moment.**
    /// [`attention::DueMove::Unchanged`] covers both "this write cannot move
    /// anything" (skipped before any read) and "it would not move it",
    /// since neither has anything for a caller to add or take off.
    ///
    /// A read failure here answers `Unchanged` too, silently: this is
    /// jojobot's own bookkeeping riding along on somebody else's write, and
    /// it must never be the reason that write fails.
    pub(crate) async fn moved_due_moment(
        &self,
        subject: &EntityId,
        incoming: &std::collections::BTreeMap<String, String>,
        cleared: &[String],
    ) -> attention::DueMove {
        let carriers = self.carriers();
        let touches_incoming = carriers.iter().any(|c| {
            c.interface()
                .fields
                .iter()
                .any(|f| incoming.contains_key(&f.key))
        });
        let touches_cleared = carriers.iter().any(|c| {
            c.interface()
                .fields
                .iter()
                .any(|f| cleared.iter().any(|k| k == &f.key))
        });
        if !touches_incoming && !touches_cleared {
            return attention::DueMove::Unchanged;
        }
        let mut projected = match self.memory.fields(subject).await {
            Ok(fields) => fields,
            Err(_) => return attention::DueMove::Unchanged,
        };
        let existing_due_on = projected
            .get(attention::DUE_ON)
            .and_then(|v| v.trim().parse().ok());
        for key in cleared {
            projected.remove(key);
        }
        projected.extend(incoming.iter().map(|(k, v)| (k.clone(), v.clone())));
        attention::moved_due_moment(&carriers, existing_due_on, &projected)
    }
}

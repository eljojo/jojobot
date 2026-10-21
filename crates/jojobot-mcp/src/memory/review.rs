//! **The review stamp, set by jojobot when defrag writes** (rules 458 and 4).
//!
//! Every write by defrag carries one computed key on the thing it is about:
//! [`review::DEFRAGGED_AT`], the moment of the review. It rides the write that
//! carries it, beside the other keys jojobot computes, so the stamp is never a
//! record of its own and no second write is made. The one record defrag's
//! mark writes is labelled [`review::REVIEW_MARK`] and takes no slot under a
//! record ceiling. A caller never writes it: the
//! refusal is the one a hand-written due moment gets.

use super::*;
use jojobot_domain::memory::review;

impl Jojobot {
    /// **The stamp a write by `writer` carries**, or none when the writer is not
    /// defrag. The value is the moment, read from the server's clock.
    pub(crate) fn review_stamp(&self, writer: &EntityId) -> Option<(String, String)> {
        review::is_defrag(writer).then(|| {
            (
                review::DEFRAGGED_AT.to_string(),
                self.clock().now().to_string(),
            )
        })
    }

    /// **The refusal for a caller that writes or clears a key that is jojobot's
    /// own**: the review stamp and mark, the label on a record jojobot wrote
    /// about an act of its own, and the release notes a bot was told. `sent` and
    /// `cleared` are what the CALLER sent,
    /// taken before jojobot adds its own copy.
    pub(crate) fn refuses_a_hand_written_review_stamp(
        &self,
        subject: &EntityId,
        sent: &std::collections::BTreeMap<String, String>,
        cleared: &[String],
    ) -> Option<CallToolResult> {
        let keys = [
            review::DEFRAGGED_AT,
            review::REVIEW_MARK,
            jojobot_domain::memory::capacity::ENGINE_RECORD,
            jojobot_domain::memory::RELEASE_NOTES_TOLD,
        ];
        let (key, writes) = keys.iter().find_map(|key| {
            let writes = sent.contains_key(*key);
            (writes || cleared.iter().any(|k| k == key)).then_some((*key, writes))
        })?;
        let how = if key == jojobot_domain::memory::RELEASE_NOTES_TOLD {
            "jojobot sets it on a bot when a release note reaches the bot's box, and on a \
             bot it makes, which is told every note its build ships."
        } else if key == jojobot_domain::memory::capacity::ENGINE_RECORD {
            "jojobot sets it on the record it writes about an act of its own, such as a \
             restore, and that record takes no slot under a record ceiling."
        } else {
            "jojobot sets it on a thing when the bot `defrag` writes to it, and defrag \
             marks a thing reviewed with `update_entity` and `reviewed: true`."
        };
        Some(blocked_body(
            subject,
            &[],
            format!(
                "Nothing was written. '{key}' is jojobot's own key and a caller does not {}. \
                 {how} Send the write without '{key}'.",
                if writes { "write it" } else { "clear it" },
            ),
        ))
    }

    /// **How many things wait for defrag, and the first of them**, for defrag's
    /// own boot. The question is the domain's one, and the order is the queue's,
    /// so the boot and `recall` with `unreviewed` cannot disagree. A page, and the
    /// call that reads the rest.
    pub(crate) async fn review_queue_block(&self) -> Option<serde_json::Value> {
        /// How many of the queue the boot names.
        const PAGE: usize = 10;
        let things = self.memory.list_entities(None).await.ok()?;
        let mut waiting = Vec::new();
        for thing in things.iter().filter(|thing| thing.browsable()) {
            let Ok(ages) = self.memory.field_ages(&thing.id).await else {
                continue;
            };
            if review::is_unreviewed(&ages) {
                waiting.push((review::oldest_change_first(Some(&ages)), ages.newest, thing));
            }
        }
        // Ties (several things with no known newest write) break by handle, as the
        // recall pass leaves them.
        waiting.sort_by(|(left, _, a), (right, _, b)| {
            left.cmp(right)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        let count = waiting.len();
        let first: Vec<serde_json::Value> = waiting
            .iter()
            .take(PAGE)
            .map(|(_, newest, thing)| {
                serde_json::json!({
                    "id": thing.id.as_str(),
                    "name": thing.name,
                    "changed": newest.map(|at| at.to_string()),
                })
            })
            .collect();
        let mut block = serde_json::json!({"count": count, "first": first});
        if count > PAGE {
            block["more"] =
                "recall with unreviewed: true reads the rest, oldest change first".into();
        }
        Some(block)
    }
}

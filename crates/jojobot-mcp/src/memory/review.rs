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
    /// about an act of its own, the release notes a bot was told, and the reason an
    /// archive gave. `sent` and
    /// `cleared` are what the CALLER sent,
    /// taken before jojobot adds its own copy.
    pub(crate) fn refuses_a_hand_written_review_stamp(
        &self,
        subject: &EntityId,
        sent: &std::collections::BTreeMap<String, String>,
        cleared: &[String],
    ) -> Option<CallToolResult> {
        let keys = review::SERVER_WRITTEN_KEYS;
        let (key, writes) = keys.iter().find_map(|key| {
            let writes = sent.keys().any(|sent| sent.trim() == *key);
            (writes || cleared.iter().any(|sent| sent.trim() == *key)).then_some((*key, writes))
        })?;
        let how = if key == review::SETTINGS_RECORD {
            "jojobot labels the settings records it generates with add_entity or update_entity; a caller's own claim is written with capture."
        } else if key == jojobot_domain::memory::ARCHIVE_REASON {
            "jojobot sets it on a claim when the claim is archived, from the details the \
             archive carries. Give a reason by archiving with `update_fact`, `status: \
             archived` and `details`."
        } else if key == jojobot_domain::memory::RELEASE_NOTES_TOLD {
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
            "RefusesAHandWrittenReviewStamp",
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
        // **A queue that cannot be read is said, never an absent queue**: a boot that
        // left `review_queue` off would read like a queue of nothing.
        let things = match self.memory.list_entities(None).await {
            Ok(things) => things,
            Err(e) => {
                tracing::warn!(error = %e, "the review queue could not be read for defrag's boot");
                return Some(serde_json::json!({
                    "unread": "the things defrag has not read could not be listed just now, so \
                               the queue is not known: boot again to read it"
                }));
            }
        };
        let mut waiting = Vec::new();
        // **Things whose age could not be read**, counted, so the queue never shrinks
        // by a thing that was left out without a word.
        let mut unread = 0usize;
        for thing in things.iter().filter(|thing| thing.browsable()) {
            let Ok(ages) = self.memory.field_ages(&thing.id).await else {
                unread += 1;
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
        if unread > 0 {
            block["unread"] = format!(
                "{unread} things could not be read for their age, so they are not in this \
                 count: the queue may hold more than {count}"
            )
            .into();
        }
        if count > PAGE {
            block["more"] =
                "recall with unreviewed: true reads the rest, oldest change first".into();
        }
        Some(block)
    }
}

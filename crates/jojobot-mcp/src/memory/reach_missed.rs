//! **A write whose keys landed on its subject says so when a thing it names got
//! none of them** (rule 456, beside the receipt's `reached_the_thing` line).
//!
//! A fact can be filed on the company that services an object, with the object
//! named in the refs or the words. The keys the caller sent in `sets` landed on
//! the subject, and the named thing holds none: the receipt says where they
//! went, so the writer can move them. It is silent when the caller sent no key
//! in `sets`, when no other thing is named in full, and when the named thing
//! holds one of the keys.

use super::possible_subject::{claim_text, points_at};
use super::*;

/// **How many things one sentence names.**
const NAMED_AT_MOST: usize = 3;

impl Jojobot {
    /// **Say which named things received none of the keys the caller sent in
    /// `sets` that reached the subject.** `sent` is what the caller put in `sets`,
    /// so a key jojobot computed (a check-in's outcome and dates, the review
    /// stamp, a due moment) is never reported as a landing. A read that fails adds
    /// nothing: the write has landed either way.
    pub(crate) async fn note_keys_missed_a_named_thing(
        &self,
        body: &mut serde_json::Value,
        fact: &Fact,
        sent: &[String],
        reached: &[String],
    ) {
        let landed: Vec<&String> = sent.iter().filter(|key| reached.contains(*key)).collect();
        if landed.is_empty() {
            return;
        }
        let Ok(known) = self.memory.list_entities(None).await else {
            return;
        };
        let parent = known
            .iter()
            .find(|entity| entity.id == fact.subject)
            .and_then(|entity| entity.parent.as_ref());
        let text = claim_text(fact);
        let mut named: Vec<(&EntityId, &str)> = fact.refs.iter().map(|id| (id, "refs")).collect();
        for entity in jojobot_domain::memory::words::named_in_full(&text, &known) {
            // A thing the build supplies is part of the software and wears no
            // permanent id; a claim's key never belonged on it.
            if entity.badge.is_some() && !points_at(fact, parent, &text, &entity.id) {
                named.push((&entity.id, "words"));
            }
        }
        let mut missed: Vec<(&EntityId, &str)> = Vec::new();
        for (id, how) in named {
            if id == &fact.subject || missed.iter().any(|(seen, _)| *seen == id) {
                continue;
            }
            let Ok(held) = self.memory.fields(id).await else {
                continue;
            };
            if landed.iter().any(|key| held.contains_key(*key)) {
                continue;
            }
            missed.push((id, how));
            if missed.len() == NAMED_AT_MOST {
                break;
            }
        }
        if missed.is_empty() {
            return;
        }
        let keys = landed
            .iter()
            .map(|key| key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let things = missed
            .iter()
            .map(|(id, how)| format!("{id}, named in your {how},"))
            .collect::<Vec<_>>()
            .join(" and ");
        let it = if landed.len() == 1 { "it" } else { "them" };
        crate::answer::note_keys_missed_a_named_thing(
            body,
            format!(
                "{keys} landed on {}; {things} did not receive {it}",
                fact.subject
            ),
        );
    }
}

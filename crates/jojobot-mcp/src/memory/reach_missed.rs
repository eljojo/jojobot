//! **A write whose keys landed on its subject says so when a thing it names got
//! none of them** (rule 456, the receipt's `reached_the_thing` line).
//!
//! A fact about an object can be filed on the company that services it, with the
//! object named in the refs or the words. The keys the caller set landed on the
//! subject, and the named thing holds none: the receipt says where they went, so
//! the writer can move them. It is silent when no key was set, when no other
//! thing is named in full, and when the named thing holds one of the keys.

use super::*;

/// **The key the sentence rides under** on a receipt.
pub(crate) const SENTENCE_KEY: &str = "a_named_thing_did_not_receive_them";

/// **How many things one sentence names.**
const NAMED_AT_MOST: usize = 3;

impl Jojobot {
    /// **Say which named things received none of the keys that landed on the
    /// subject.** `reached` is the keys of the claim that reached the thing; only
    /// the ones the caller sent in `sets` count, so a date jojobot computed is
    /// never reported as a landing. A read that fails adds nothing: the write has
    /// landed either way.
    pub(crate) async fn note_keys_missed_a_named_thing(
        &self,
        body: &mut serde_json::Value,
        fact: &Fact,
        reached: &[String],
    ) {
        // **The three dates jojobot computes are never the caller's**: a write that
        // sets a day to fall due on gets `due_on` beside it, and a check-in gets
        // the other two. Left out, so the sentence names what the caller sent.
        let computed = [
            jojobot_domain::attention::DUE_ON,
            jojobot_domain::attention::LAST_CHECK_IN,
            jojobot_domain::attention::COUNTS_FROM,
        ];
        let sent: Vec<&String> = reached
            .iter()
            .filter(|key| fact.sets.contains(*key) && !computed.contains(&key.as_str()))
            .collect();
        if sent.is_empty() {
            return;
        }
        let Ok(known) = self.memory.list_entities(None).await else {
            return;
        };
        let mut named: Vec<(&EntityId, &str)> = Vec::new();
        for id in &fact.refs {
            named.push((id, "refs"));
        }
        let mut text = fact.content.clone();
        let describing = crate::seed::describing_keys();
        for (key, value) in &fact.fields {
            if !describing.contains(key) {
                text.push('\n');
                text.push_str(value);
            }
        }
        for entity in jojobot_domain::memory::words::named_in_full(&text, &known) {
            // A thing the build supplies is part of the software and wears no
            // permanent id; a claim's key never belonged on it.
            if entity.badge.is_some() && !named.iter().any(|(id, _)| **id == entity.id) {
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
            if sent.iter().any(|key| held.contains_key(*key)) {
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
        let keys = sent
            .iter()
            .map(|key| key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let things = missed
            .iter()
            .map(|(id, how)| format!("{id}, named in your {how},"))
            .collect::<Vec<_>>()
            .join(" and ");
        let it = if sent.len() == 1 { "it" } else { "them" };
        let said = format!(
            "{keys} landed on {}; {things} did not receive {it}",
            fact.subject
        );
        if let Some(object) = body.as_object_mut() {
            object.insert(SENTENCE_KEY.into(), said.into());
        }
    }
}

//! **What a boot cannot cut, measured when a write would grow it.**
//!
//! A boot ships the essay's core, the snapshot, the charter and every carried
//! rule's own words whatever the ranking decides. The boot never declines, so
//! a bot whose floor is over the ceiling would ship an oversized answer
//! forever. A star or a seat count that would take a bot over is refused where
//! it is written, naming the size and the overage.

use super::*;
use crate::orientation::identity::carried_rules;
use crate::orientation::orient::{elide_rule_details, entity_summary, floor_len};
use jojobot_domain::memory::{FactStatus, RULE_SEATS, rule_seats_of, rules_in_force};

impl Jojobot {
    /// **The boot's floor for `bot` if it held `in_force` rules and `seats`
    /// seats**, in characters — the same sum `orient` measures, with every
    /// carried rule's `details` already elided. `None` when a store cannot
    /// say, which is never a reason to refuse a write.
    ///
    /// Not counted: the session blocks, which a write has none of, and the
    /// bot's own box counts, which a read must not repair.
    async fn boot_floor_if(
        &self,
        bot: &EntityId,
        in_force: &[Fact],
        seats: usize,
    ) -> Option<usize> {
        let index = self.memory.list_entities(None).await;
        let entity = index.as_ref().ok()?.iter().find(|e| &e.id == bot)?.clone();
        let charter = self
            .memory
            .scan_entity(bot)
            .await
            .ok()?
            .map(|doc| doc.prose)
            .filter(|prose| !prose.trim().is_empty());
        let as_of = self.clock().today_in(&jiff::tz::TimeZone::UTC);
        let rules: Vec<serde_json::Value> = carried_rules(in_force, seats)
            .into_iter()
            .map(|rule| {
                let mut rule = fact_json(rule, as_of, None);
                elide_rule_details(&mut rule);
                rule
            })
            .collect();
        let identity = serde_json::json!({
            "bot": entity_json(&entity),
            "charter": charter,
            "charter_elided": false,
            "rules": rules,
        });
        let snapshot = self.snapshot_block(Some(bot), entity_summary(&index)).await;
        Some(floor_len(
            false,
            &snapshot,
            &identity,
            &serde_json::Value::Null,
            &serde_json::Value::Null,
            self.stated_clock(),
        ))
    }

    /// **The refusal for a write that would leave `bot`'s boot floor over the
    /// ceiling**, or `None`. `would_be` is the claims the write leaves the bot
    /// holding, `seats_written` the seat count the write names, if it names one.
    ///
    /// Refused only when the write makes the floor bigger and the result is
    /// over: a bot already over before the write must stay repairable by every
    /// write that does not grow it.
    pub(crate) async fn refuses_a_boot_floor_over(
        &self,
        bot: &EntityId,
        would_be: &[Fact],
        seats_written: Option<&str>,
    ) -> Option<MemoryError> {
        let current = self.memory.recall(bot).await.ok()?;
        let fields = self.memory.fields(bot).await.unwrap_or_default();
        let seats_now = rule_seats_of(&fields);
        let seats_after = match seats_written {
            None => seats_now,
            Some(written) => match written.trim().parse::<usize>() {
                Ok(seats) if seats > 0 => seats,
                _ => {
                    return Some(MemoryError::InvalidFact(format!(
                        "rule_seats holds a whole number above zero, and '{written}' is not one"
                    )));
                }
            },
        };
        let in_force_after = rules_in_force(would_be);
        let after = self
            .boot_floor_if(bot, &in_force_after, seats_after)
            .await?;
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        if after <= budget {
            return None;
        }
        let before = self
            .boot_floor_if(bot, &rules_in_force(&current), seats_now)
            .await?;
        (after > before).then(|| MemoryError::BootTooHeavy {
            subject: bot.to_string(),
            floor: after,
            budget,
        })
    }

    /// **The floor check for a capture**, run before it lands. Asked only of a
    /// write that could grow a bot's floor: a star on a bot, or a seat count.
    ///
    /// A bot naming its own seat count is left to the self-ceiling refusal,
    /// which speaks for that write; a bot starring its own rule is exactly the
    /// write this exists for.
    pub(crate) async fn refuses_a_boot_floor_for_capture(
        &self,
        new: &NewFact,
        caller: &EntityId,
    ) -> Option<MemoryError> {
        if new.subject.kind() != Some(EntityKind::BOT) {
            return None;
        }
        let starred = new.fields.get("starred").is_some_and(|v| v == "true");
        let seats = new.fields.get(RULE_SEATS);
        if !starred && seats.is_none() {
            return None;
        }
        if seats.is_some() && &new.subject == caller {
            return None;
        }
        let mut held = self.memory.recall(&new.subject).await.ok()?;
        held.push(Fact {
            id: jojobot_domain::memory::FactId(format!("f{}", held.len() + 1)),
            home: new.subject.clone(),
            subject: new.subject.clone(),
            content: new.content.clone(),
            details: new.details.clone(),
            provenance: new.provenance,
            standing: new.standing.unwrap_or(match new.provenance {
                Provenance::Testimony => jojobot_domain::memory::Standing::Settled,
                _ => jojobot_domain::memory::Standing::Open,
            }),
            status: FactStatus::Active,
            recorded_at: new.recorded_at,
            happened_at: new.happened_at,
            happened_through: new.happened_through,
            edge: new.edge.clone(),
            fields: new.fields.clone(),
            refs: new.refs.clone(),
            derived_from: new.derived_from.clone(),
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: new.stale_after,
        });
        self.refuses_a_boot_floor_over(&new.subject, &held, seats.map(String::as_str))
            .await
    }

    /// **The floor check for an edit**, run before it lands. Asked of an edit
    /// that leaves a bot's own record starred, or that names a seat count —
    /// an edit to the words of a starred rule grows the floor as surely as a
    /// new star does.
    pub(crate) async fn refuses_a_boot_floor_for_edit(
        &self,
        address: &FactAddress,
        patch: &FactPatch,
        caller: &EntityId,
    ) -> Option<MemoryError> {
        if address.home.kind() != Some(EntityKind::BOT) {
            return None;
        }
        let mut held = self.memory.recall(&address.home).await.ok()?;
        let at = held.iter().position(|fact| fact.id == address.local)?;
        let mut edited = held[at].clone();
        jojobot_domain::memory::apply_fact_patch(&mut edited, patch).ok()?;
        let starred = edited.fields.get("starred").is_some_and(|v| v == "true");
        let seats = patch.fields.get(RULE_SEATS);
        if !starred && seats.is_none() {
            return None;
        }
        if seats.is_some() && &address.home == caller {
            return None;
        }
        held[at] = edited;
        self.refuses_a_boot_floor_over(&address.home, &held, seats.map(String::as_str))
            .await
    }
}

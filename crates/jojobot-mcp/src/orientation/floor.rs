//! **What a boot cannot cut, measured when a write would grow it.**
//!
//! A boot ships the essay's core, the snapshot, the charter and every carried
//! rule's own words whatever the ranking decides. The boot never declines, so
//! a bot whose floor is over the ceiling would ship an oversized answer
//! forever. A star or a seat count that would take a bot over is refused where
//! it is written, naming the size and the overage. A charter is held to the
//! same ceiling. Creating a bot grows every other bot's snapshot and is never
//! refused, so it is measured instead, and the answer names the bots it took
//! over the ceiling.

use super::*;
use crate::orientation::identity::carried_rules;
use crate::orientation::instance_zone::{ZoneFrom, zone_answer};
use crate::orientation::orient::{InstanceLines, elide_rule_details, entity_summary, floor_len};
use jojobot_domain::memory::{FactStatus, RULE_SEATS, rule_seats_of, rules_in_force};

/// **What a boot cannot cut, measured, and what it is made of.**
pub(crate) struct Floor {
    /// The size in characters, the sum `orient` measures.
    pub(crate) total: usize,
    /// Each part of the total with its own size, largest first. They add up to
    /// `total`: the last is whatever the three named parts leave.
    pub(crate) parts: Vec<(String, usize)>,
}

fn json_len(value: &serde_json::Value) -> usize {
    value.to_string().chars().count()
}

/// **What a floor of `total` is made of**, largest first. The three named
/// parts are measured and the last is whatever they leave, so the parts add up
/// to `total`. Shared by the refusals and by the boot, so the two cannot name
/// different sizes for one floor.
pub(crate) fn floor_parts(
    total: usize,
    snapshot: &serde_json::Value,
    identity: &serde_json::Value,
) -> Vec<(String, usize)> {
    let named = [
        ("charter", json_len(&identity["charter"])),
        ("snapshot", json_len(snapshot)),
        ("carried rules", json_len(&identity["rules"])),
    ];
    let rest = total.saturating_sub(named.iter().map(|(_, n)| n).sum::<usize>());
    let mut parts: Vec<(String, usize)> = named
        .into_iter()
        .chain([("orientation, skills and bot record", rest)])
        .map(|(name, n)| (name.to_string(), n))
        .collect();
    parts.sort_by_key(|part| std::cmp::Reverse(part.1));
    parts
}

/// **The ways a bot's floor comes down**, each named by the verb that does it.
/// One sentence for the refusal that stops a write and the notice a boot
/// carries, so what a caller is told to do cannot differ between them.
pub(crate) fn ways_down(subject: &str) -> String {
    format!(
        "Unstar a rule on {subject} with update_fact, shorten the rules that are starred or its \
         charter with set_charter, or have a different identity lower rule_seats on {subject} — \
         a bot cannot write rule_seats about itself — and a rule that binds at one moment is \
         better carried by a skill than by a seat."
    )
}

/// **The notice a boot carries when what it cannot cut is over the ceiling.**
/// The ceiling is a property of the answer, so the answer says so: any entity
/// grows every bot's snapshot, and no write that cannot be refused should be
/// refused for it. The sizes are the refusals' own.
pub(crate) fn over_the_ceiling(
    bot: &EntityId,
    total: usize,
    parts: &[(String, usize)],
) -> serde_json::Value {
    let budget = jojobot_domain::text::BOOT_ANSWER.budget;
    let largest = parts
        .first()
        .map(|(part, n)| format!("{part} ({n} characters)"))
        .unwrap_or_default();
    serde_json::json!({
        "floor": total,
        "budget": budget,
        "over": total.saturating_sub(budget),
        "floor_parts": parts
            .iter()
            .map(|(part, n)| serde_json::json!({"part": part, "characters": n}))
            .collect::<Vec<_>>(),
        "how_to_proceed": format!(
            "What this boot cannot cut is over its ceiling, and the boot ships anyway. The \
             largest part is {largest}; floor_parts lists every part. {} Entities count toward \
             the snapshot: archive_entity takes one that is no longer wanted out of its counts.",
            ways_down(bot.as_str()),
        ),
    })
}

impl Jojobot {
    /// **The boot's floor for `bot` if it held `in_force` rules and `seats`
    /// seats**, in characters — the same sum `orient` measures, with every
    /// carried rule's `details` already elided. `charter` is the charter the
    /// boot would carry; `None` reads the one the bot holds now. `None` back
    /// when a store cannot say, which is never a reason to refuse a write.
    ///
    /// Not counted: the session blocks, which a write has none of, and the
    /// bot's own box counts, which a read must not repair.
    async fn boot_floor_if(
        &self,
        bot: &EntityId,
        in_force: &[Fact],
        seats: usize,
        charter: Option<&str>,
    ) -> Option<Floor> {
        let index = self.memory.list_entities(None).await;
        let entity = index.as_ref().ok()?.iter().find(|e| &e.id == bot)?.clone();
        let charter = match charter {
            // **The charter as the boot composes it**: the build's layer joined
            // to what the instance wrote, through the same store the boot reads.
            // The proposed text alone is what a caller sends and not what a
            // boot serves, and counting it alone leaves a bot with a shipped
            // layer thousands of characters over a ceiling the check passed.
            Some(proposed) => Some(self.memory.composed_prose(bot, proposed).await.ok()?),
            None => self
                .memory
                .scan_entity(bot)
                .await
                .ok()?
                .map(|doc| doc.prose),
        }
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
        let mut identity = serde_json::json!({
            "bot": entity_json(&entity),
            "charter": charter,
            "charter_elided": false,
            "rules": rules,
        });
        // The map of the rules that have no seat rides every boot, so a floor
        // that left it out would undercount by exactly what a bot with many
        // rules adds.
        if let (Some(map), Some(obj)) = (
            crate::orientation::identity::unseated_rules(bot, in_force, seats),
            identity.as_object_mut(),
        ) {
            obj.insert("unseated_rules".into(), map);
        }
        let snapshot = self.snapshot_block(Some(bot), entity_summary(&index)).await;
        let total = floor_len(
            false,
            &snapshot,
            &identity,
            &serde_json::Value::Null,
            &serde_json::Value::Null,
            self.stated_clock(),
            InstanceLines {
                timezone: &{
                    let instance = self.instance_zone().await;
                    match instance.name() {
                        Some(name) => zone_answer(Some(name), ZoneFrom::Instance, &instance),
                        None => zone_answer(None, ZoneFrom::Default, &instance),
                    }
                },
                operator: &self.operator_answer().await,
            },
        );
        let parts = floor_parts(total, &snapshot, &identity);
        Some(Floor { total, parts })
    }

    /// **The refusal for a floor of `after`**, when it is over the ceiling and
    /// larger than `before`, or `None`. A bot already over before the write
    /// must stay repairable by every write that does not grow it.
    fn over_the_ceiling(bot: &EntityId, after: Floor, before: &Floor) -> Option<MemoryError> {
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        (after.total > budget && after.total > before.total).then(|| MemoryError::BootTooHeavy {
            subject: bot.to_string(),
            floor: after.total,
            budget,
            parts: after.parts,
        })
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
            .boot_floor_if(bot, &in_force_after, seats_after, None)
            .await?;
        if after.total <= jojobot_domain::text::BOOT_ANSWER.budget {
            return None;
        }
        let before = self
            .boot_floor_if(bot, &rules_in_force(&current), seats_now, None)
            .await?;
        Self::over_the_ceiling(bot, after, &before)
    }

    /// **The floor check for a charter**, run before it is written. A charter
    /// replaces the one the bot has, so it is measured as the charter the boot
    /// would carry, against the one it carries now.
    pub(crate) async fn refuses_a_boot_floor_for_charter(
        &self,
        bot: &EntityId,
        prose: &str,
    ) -> Option<MemoryError> {
        let held = self.memory.recall(bot).await.ok()?;
        let seats = rule_seats_of(&self.memory.fields(bot).await.unwrap_or_default());
        let in_force = rules_in_force(&held);
        let after = self
            .boot_floor_if(bot, &in_force, seats, Some(prose.trim()))
            .await?;
        if after.total <= jojobot_domain::text::BOOT_ANSWER.budget {
            return None;
        }
        let before = self.boot_floor_if(bot, &in_force, seats, None).await?;
        Self::over_the_ceiling(bot, after, &before)
    }

    /// **Every bot's floor as things stand**, for a write that grows them all
    /// and cannot be refused. A bot whose floor a store cannot measure is left
    /// out: an unmeasured bot is not a bot known to be over.
    pub(crate) async fn boot_floors_of_bots(&self) -> Vec<(EntityId, usize)> {
        let Ok(index) = self.memory.list_entities(None).await else {
            return Vec::new();
        };
        let mut floors = Vec::new();
        for bot in index
            .iter()
            .filter(|e| e.kind == EntityKind::BOT && e.browsable())
        {
            let Ok(held) = self.memory.recall(&bot.id).await else {
                continue;
            };
            let seats = rule_seats_of(&self.memory.fields(&bot.id).await.unwrap_or_default());
            if let Some(floor) = self
                .boot_floor_if(&bot.id, &rules_in_force(&held), seats, None)
                .await
            {
                floors.push((bot.id.clone(), floor.total));
            }
        }
        floors
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

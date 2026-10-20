//! **What a boot cannot cut, measured when a write would grow it.**
//!
//! A boot ships the essay's core, the snapshot, the charter and every carried
//! rule's own words whatever the ranking decides. The boot never declines, so
//! a bot whose floor is over the ceiling would ship an oversized answer
//! forever. A star or a seat count that would take a bot over is refused where
//! it is written, naming the size and the overage. A charter is held to the
//! same ceiling. Creating a bot grows every other bot's snapshot and is never
//! refused for that, so it is measured instead, and the answer names the bots
//! it took over the ceiling. A bot created with fields that star a rule is
//! held to the same check as a capture of those fields, measured as the bot
//! will exist once the creation commits.

use super::*;
use crate::memory::reads_left::{BOOT_FLOOR, ReadsLeft};
use crate::orientation::instance_zone::{ZoneFrom, zone_answer};
use crate::orientation::orient::{InstanceLines, elide_rule_details, entity_summary, floor_len};
use jojobot_domain::memory::{FactStatus, RULE_SEATS, rule_seats_of, rules_in_force};

mod fold;
mod rename;

/// **A seat count a write names, as a number or as the refusal it earns.** Judged
/// before any read, so a count that is no number is refused whether or not the
/// bot's fields can be read.
fn seat_count_written(written: Option<&str>) -> Result<Option<usize>, MemoryError> {
    match written {
        None => Ok(None),
        Some(written) => match written.trim().parse::<usize>() {
            Ok(seats) if seats > 0 => Ok(Some(seats)),
            _ => Err(MemoryError::InvalidFact(format!(
                "rule_seats holds a whole number above zero, and '{written}' is not one"
            ))),
        },
    }
}

/// **What a boot cannot cut, measured, and what it is made of.**
pub(crate) struct Floor {
    /// The size in characters, the sum `orient` measures.
    pub(crate) total: usize,
    /// Each part of the total with its own size, largest first. They add up to
    /// `total`: the last is whatever the three named parts leave.
    pub(crate) parts: Vec<(String, usize)>,
}

/// **The claim a write would add, as the boot would serve it once stored.**
/// `inserted_at` is the moment the store stamps at the append; the boot serializes
/// it beside the claim, so a claim measured without it is measured short by the
/// width of a timestamp.
pub(crate) fn prospective_fact(
    new: &NewFact,
    ordinal: usize,
    inserted_at: Option<jiff::Timestamp>,
) -> Fact {
    Fact {
        app_stamp: None,
        written_by_another: false,
        sets: Default::default(),
        reaching: Default::default(),
        field_links: Default::default(),
        id: jojobot_domain::memory::FactId(format!("f{ordinal}")),
        home: new.subject.clone(),
        subject: new.subject.clone(),
        content: new.content.clone(),
        details: new.details.clone(),
        provenance: new.provenance,
        standing: new
            .standing
            .unwrap_or(jojobot_domain::memory::Standing::default_for(
                new.provenance,
            )),
        status: FactStatus::Active,
        recorded_at: new.recorded_at,
        happened_at: new.happened_at,
        happened_through: new.happened_through,
        edge: new.edge.clone(),
        fields: new.fields.clone(),
        refs: new.refs.clone(),
        derived_from: new.derived_from.clone(),
        stands_for: Vec::new(),
        inserted_at,
        stale_after: new.stale_after,
        receipt: None,
    }
}

/// **The widest moment the store can stamp a record with**, on the second after
/// `now`. A timestamp drops its trailing zeros, so a stamp renders anywhere from
/// 20 to 30 characters, and the store takes its moment later than any check
/// does: the check cannot know which width it will get. A fraction with a
/// nonzero last digit renders the most, so a rule that fits with this stamp fits
/// with any stamp the store can give it. **The second after, and not the second
/// of**, so a record not yet written is newer than every record already stored,
/// which is what the boot's seats order by age.
pub(crate) fn widest_stamp(now: jiff::Timestamp) -> jiff::Timestamp {
    jiff::Timestamp::new(now.as_second() + 1, 123_456_789).unwrap_or(now)
}

/// **The characters [`widest_stamp`] holds back** over the narrowest stamp: the
/// fraction a whole-second stamp does not carry. A refusal for a new rule names
/// it, so a writer that short of the ceiling knows why.
fn stamp_margin() -> usize {
    let whole_second = jiff::Timestamp::UNIX_EPOCH;
    widest_stamp(whole_second).to_string().len() - whole_second.to_string().len()
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
        // `get`, not an index: an identity without `rules` ships no key, and an
        // index would read it as the four characters of `null`.
        ("carried rules", identity.get("rules").map_or(0, json_len)),
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
        "{} Unstar a rule on {subject} that way, shorten the rules that are starred or its \
         charter with set_charter, or have a different identity lower rule_seats on {subject} — \
         a bot cannot write rule_seats about itself. A rule that binds at one moment is \
         better filed under a purpose that an index rule names with its moment (start_here \
         skill: organizing) than carried in a seat, and a group of rules that all bind is \
         better carried by one claim that stands for the others: write it, mark it with \
         stands_for naming them, keep the others unstarred, and read the boot back.",
        jojobot_domain::memory::STAR_ROUTE,
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
    /// **The seats a bot's boot carries, read for the floor check.** A count that
    /// cannot be read is no count: the floor is not measured at the default, because
    /// a floor measured on an assumed count could refuse a write the real boot would
    /// carry, and a refusal that does not say it assumed is a default answer
    /// (rule 130). The check is skipped, the read is logged and the write's answer
    /// names the floor among the reads it left (`reads`); a boot that is over its
    /// ceiling says so itself.
    async fn seats_read(&self, bot: &EntityId, reads: &ReadsLeft) -> Option<usize> {
        match self.memory.fields(bot).await {
            Ok(fields) => Some(rule_seats_of(&fields)),
            Err(e) => {
                reads.left(BOOT_FLOOR, bot, &e);
                None
            }
        }
    }

    /// **The claims a bot holds now, read for the floor check.** A read that fails
    /// is named on the answer with the floor, as the seats are.
    async fn held_for_the_floor(&self, bot: &EntityId, reads: &ReadsLeft) -> Option<Vec<Fact>> {
        match self.memory.recall(bot).await {
            Ok(held) => Some(held),
            Err(e) => {
                reads.left(BOOT_FLOOR, bot, &e);
                None
            }
        }
    }

    /// **The bot's own entity, read for the floor check.** A read that fails is
    /// named on the answer with the floor, as the claims and the seats are; a bot
    /// the store does not hold names nothing, because there is no boot to measure.
    async fn entity_for_the_floor(&self, bot: &EntityId, reads: &ReadsLeft) -> Option<Entity> {
        match self.memory.scan_entity(bot).await {
            Ok(scan) => scan?.entity,
            Err(e) => {
                reads.left(BOOT_FLOOR, bot, &e);
                None
            }
        }
    }

    /// **A floor the boot's own parts could not be read for**: the check is skipped
    /// and the answer says so.
    fn floor_unmeasured(bot: &EntityId, reads: &ReadsLeft) {
        reads.left_for(
            BOOT_FLOOR,
            format!("{bot}: a part of its boot could not be read"),
        );
    }

    /// **The boot's floor for `bot` if it held `in_force` rules and `seats`
    /// seats**, in characters — the same sum `orient` measures, with every
    /// carried rule's `details` already elided. `charter` is the charter the
    /// boot would carry; `None` reads the one the bot holds now. `None` back
    /// when a store cannot say, which is never a reason to refuse a write.
    ///
    /// Not counted: the session blocks, which a write has none of. The box block
    /// is counted as the boot carries it, read without repairing the box.
    async fn boot_floor_if(
        &self,
        bot: &EntityId,
        in_force: &[Fact],
        seats: usize,
        charter: Option<&str>,
        creating: Option<&Entity>,
        relabelled: Option<&Entity>,
    ) -> Option<Floor> {
        let mut index = self.memory.list_entities(None).await;
        // **A bot that does not exist yet is measured as it will exist once its
        // creation commits**: it is the entity the boot would name, and it joins
        // the snapshot's counts, because the boot that follows the creation
        // counts it.
        if let (Some(creating), Ok(listed)) = (creating, index.as_mut()) {
            listed.push(creating.clone());
        }
        // **A bot being renamed or re-aliased is measured as the entity the edit
        // leaves**: its handle, name and aliases ride in its own boot. The entity
        // is found under the handle it wears now, which a rename changes.
        let standing = index.as_ref().ok()?.iter().find(|e| &e.id == bot)?.clone();
        // **The roster the snapshot counts is the one the store holds**, with the
        // bot under the handle it wears now; the moved handle is spelled into it
        // below, so the bot is still the one the snapshot calls its own.
        let roster = entity_summary(&index);
        // **A rename changes the handle and nothing else of the row the roster
        // holds**, so the entity is the one the boot reads with its id moved; an
        // edit of the name or the aliases is the entity as the edit leaves it.
        let leaves = relabelled.map(|relabelled| match relabelled.id == standing.id {
            true => relabelled.clone(),
            false => Entity {
                id: relabelled.id.clone(),
                ..standing.clone()
            },
        });
        if let (Some(leaves), Ok(listed)) = (&leaves, index.as_mut())
            && let Some(slot) = listed.iter_mut().find(|e| &e.id == bot)
        {
            *slot = leaves.clone();
        }
        let entity = leaves.unwrap_or_else(|| standing.clone());
        let charter = match charter {
            // **The charter as the boot composes it**: the build's layer joined
            // to what the instance wrote, through the same store the boot reads.
            // The proposed text alone is what a caller sends and not what a
            // boot serves, and counting it alone leaves a bot with a shipped
            // layer thousands of characters over a ceiling the check passed.
            Some(proposed) => Some(self.memory.composed_prose(bot, proposed).await.ok()?),
            None if creating.is_some() => None,
            None => self
                .memory
                .scan_entity(bot)
                .await
                .ok()?
                .map(|doc| doc.prose),
        }
        .filter(|prose| !prose.trim().is_empty());
        // **A rule's staleness is read on the day the boot serves it.** The boot
        // reads it in the run's zone, else the instance's; a write guard has no
        // run to ask, so it takes the instance's, then UTC, the frame every
        // unzoned read shares.
        let as_of = self.clock().today_in(&self.unzoned_frame().await);
        // **The identity the boot carries, from the function the boot builds it
        // with** (rule 498), the box block read without repairing it. A part the
        // boot carries and a write left out is a part the write measured short.
        // **The box is read under the handle it stands under**; a rename moves it
        // with the bot, so the box the boot will carry is named for the new handle.
        let mut owned_mailbox = self.owned_mailbox_read(&standing).await.ok()?;
        if entity.id != standing.id
            && let Some(name) = owned_mailbox.get_mut("name")
            && name.as_str() == Some(standing.id.slug())
        {
            *name = entity.id.slug().into();
        }
        let mut identity = self
            .identity_block(crate::orientation::identity::IdentityParts {
                entity: &entity,
                charter,
                charter_elided: false,
                in_force,
                seat_count: seats,
                as_of,
                owned_mailbox,
            })
            .await;
        if let Some(rules) = identity.get_mut("rules").and_then(|r| r.as_array_mut()) {
            for rule in rules.iter_mut() {
                elide_rule_details(rule);
            }
        }
        // The stored handle finds its box and counts; the proposed floor uses
        // the handle and box name the rename leaves behind.
        let mut snapshot = self.snapshot_block(Some(bot), roster).await;
        if entity.id != standing.id
            && let Some(bots) = snapshot["entities"]["bots"].as_array_mut()
        {
            for entry in bots {
                if entry["handle"].as_str() == Some(standing.id.as_str()) {
                    entry["handle"] = entity.id.as_str().into();
                    if let Some(name) = entry["mail"].get_mut("name") {
                        *name = entity.id.slug().into();
                    }
                }
            }
        }
        let skills_listed = crate::orientation::skills::index(self.skills, Some(bot));
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
                skills: &skills_listed,
            },
        );
        let parts = floor_parts(total, &snapshot, &identity);
        Some(Floor { total, parts })
    }

    /// **The refusal for a floor of `after`**, when it is over the ceiling and
    /// larger than `before`, or `None`. A bot already over before the write
    /// must stay repairable by every write that does not grow it.
    fn over_the_ceiling(
        bot: &EntityId,
        after: Floor,
        before: &Floor,
        stamp_margin: usize,
    ) -> Option<MemoryError> {
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        (after.total > budget && after.total > before.total).then(|| MemoryError::BootTooHeavy {
            subject: bot.to_string(),
            floor: after.total,
            budget,
            parts: after.parts,
            stamp_margin,
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
        creating: Option<&Entity>,
        stamp_margin: usize,
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        let seats_written = match seat_count_written(seats_written) {
            Ok(seats) => seats,
            Err(refused) => return Some(refused),
        };
        // **A bot being created holds nothing yet**: no claims, no fields, and a
        // floor of nothing before the write, so any floor over the ceiling is
        // growth.
        let current = match creating {
            Some(_) => Vec::new(),
            None => self.held_for_the_floor(bot, reads).await?,
        };
        let seats_now = match creating {
            Some(_) => rule_seats_of(&Default::default()),
            None => self.seats_read(bot, reads).await?,
        };
        let seats_after = seats_written.unwrap_or(seats_now);
        let in_force_after = rules_in_force(would_be);
        let Some(after) = self
            .boot_floor_if(bot, &in_force_after, seats_after, None, creating, None)
            .await
        else {
            Self::floor_unmeasured(bot, reads);
            return None;
        };
        if after.total <= jojobot_domain::text::BOOT_ANSWER.budget {
            return None;
        }
        let before = match creating {
            Some(_) => Floor {
                total: 0,
                parts: Vec::new(),
            },
            None => {
                let Some(before) = self
                    .boot_floor_if(bot, &rules_in_force(&current), seats_now, None, None, None)
                    .await
                else {
                    Self::floor_unmeasured(bot, reads);
                    return None;
                };
                before
            }
        };
        Self::over_the_ceiling(bot, after, &before, stamp_margin)
    }

    /// **The floor check for a charter**, run before it is written. A charter
    /// replaces the one the bot has, so it is measured as the charter the boot
    /// would carry, against the one it carries now.
    pub(crate) async fn refuses_a_boot_floor_for_charter(
        &self,
        bot: &EntityId,
        prose: &str,
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        let held = self.held_for_the_floor(bot, reads).await?;
        let seats = self.seats_read(bot, reads).await?;
        let in_force = rules_in_force(&held);
        let Some(after) = self
            .boot_floor_if(bot, &in_force, seats, Some(prose.trim()), None, None)
            .await
        else {
            Self::floor_unmeasured(bot, reads);
            return None;
        };
        if after.total <= jojobot_domain::text::BOOT_ANSWER.budget {
            return None;
        }
        let Some(before) = self
            .boot_floor_if(bot, &in_force, seats, None, None, None)
            .await
        else {
            Self::floor_unmeasured(bot, reads);
            return None;
        };
        Self::over_the_ceiling(bot, after, &before, 0)
    }

    /// **The floor check for a bot's name or aliases**, run before an edit
    /// lands. They ride in the bot's own boot, so the edit is measured as the
    /// entity it leaves, against the entity as it stands; only an edit that
    /// grows the floor past the ceiling is refused.
    pub(crate) async fn refuses_a_boot_floor_for_relabelling(
        &self,
        bot: &EntityId,
        patch: &jojobot_domain::memory::EntityPatch,
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        if bot.kind() != Some(EntityKind::BOT) || (patch.name.is_none() && patch.aliases.is_none())
        {
            return None;
        }
        let scanned = match self.memory.scan_entity(bot).await {
            Ok(scanned) => scanned,
            Err(e) => {
                reads.left(BOOT_FLOOR, bot, &e);
                return None;
            }
        };
        let mut relabelled = scanned?.entity?;
        if let Some(name) = &patch.name {
            relabelled.name = name.clone();
        }
        if let Some(aliases) = &patch.aliases {
            relabelled.aliases = aliases.clone();
        }
        let held = self.held_for_the_floor(bot, reads).await?;
        let seats = self.seats_read(bot, reads).await?;
        let in_force = rules_in_force(&held);
        let Some(after) = self
            .boot_floor_if(bot, &in_force, seats, None, None, Some(&relabelled))
            .await
        else {
            Self::floor_unmeasured(bot, reads);
            return None;
        };
        if after.total <= jojobot_domain::text::BOOT_ANSWER.budget {
            return None;
        }
        let Some(before) = self
            .boot_floor_if(bot, &in_force, seats, None, None, None)
            .await
        else {
            Self::floor_unmeasured(bot, reads);
            return None;
        };
        Self::over_the_ceiling(bot, after, &before, 0)
    }

    /// **Every bot's floor as things stand**, for a write that grows them all
    /// and cannot be refused. A bot whose floor a store cannot measure is left
    /// out: an unmeasured bot is not a bot known to be over.
    pub(crate) async fn boot_floors_of_bots(&self, reads: &ReadsLeft) -> Vec<(EntityId, usize)> {
        let index = match self.memory.list_entities(None).await {
            Ok(index) => index,
            Err(e) => {
                reads.left(BOOT_FLOOR, "the list of bots", &e);
                return Vec::new();
            }
        };
        let mut floors = Vec::new();
        for bot in index
            .iter()
            .filter(|e| e.kind == EntityKind::BOT && e.browsable())
        {
            let Some(held) = self.held_for_the_floor(&bot.id, reads).await else {
                continue;
            };
            let Some(seats) = self.seats_read(&bot.id, reads).await else {
                continue;
            };
            match self
                .boot_floor_if(&bot.id, &rules_in_force(&held), seats, None, None, None)
                .await
            {
                Some(floor) => floors.push((bot.id.clone(), floor.total)),
                None => Self::floor_unmeasured(&bot.id, reads),
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
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        self.refuses_a_boot_floor_for_first_or_later(new, caller, None, reads)
            .await
    }

    /// **The same check for a bot being created**, run before the creation
    /// lands, on every creation of a bot. `first` is the first claim the call
    /// writes, or a bare one when it writes none. `creating` is the bot as it will
    /// exist; there is one check and a capture and a creation both reach it.
    pub(crate) async fn refuses_a_boot_floor_for_creation(
        &self,
        first: &NewFact,
        creating: &Entity,
        caller: &EntityId,
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        self.refuses_a_boot_floor_for_first_or_later(first, caller, Some(creating), reads)
            .await
    }

    async fn refuses_a_boot_floor_for_first_or_later(
        &self,
        new: &NewFact,
        caller: &EntityId,
        creating: Option<&Entity>,
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        if new.subject.kind() != Some(EntityKind::BOT) {
            return None;
        }
        let starred = new.fields.get("starred").is_some_and(|v| v == "true");
        let seats = new.fields.get(RULE_SEATS);
        // **A claim on a bot that already exists grows its floor only by a star or
        // a seat count**, so any other claim is none of this check's business. A
        // bot being created is different: its name, aliases and source are in the
        // boot it will have, whatever its first claim says.
        if creating.is_none() && !starred && seats.is_none() {
            return None;
        }
        if seats.is_some() && &new.subject == caller {
            return None;
        }
        // **A seat count that is no number is refused before any read**, so it is
        // refused whether or not the bot's claims can be read.
        if let Err(refused) = seat_count_written(seats.map(String::as_str)) {
            return Some(refused);
        }
        let mut held = match creating {
            Some(_) => Vec::new(),
            None => self.held_for_the_floor(&new.subject, reads).await?,
        };
        // **Stamped as the widest the store can stamp it**: the boot serializes
        // the moment beside the rule, and a rule measured without it fits by the
        // width of a timestamp it will not have room for. The store takes its own
        // moment later, so the check measures the widest rendering instead of a
        // moment of its own.
        held.push(prospective_fact(
            new,
            held.len() + 1,
            Some(widest_stamp(self.clock().now())),
        ));
        // **A starred rule is measured with a stamp, and a refusal says how much
        // room that held back.** An unstarred claim rides no boot, so no stamp of
        // its own is counted against the floor and none is said to be held.
        self.refuses_a_boot_floor_over(
            &new.subject,
            &held,
            seats.map(String::as_str),
            creating,
            if starred { stamp_margin() } else { 0 },
            reads,
        )
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
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        if address.home.kind() != Some(EntityKind::BOT) {
            return None;
        }
        let seats = patch.fields.get(RULE_SEATS);
        if seats.is_some() && &address.home == caller {
            return None;
        }
        // **A seat count that is no number is refused before any read**, so it is
        // refused whether or not the bot's claims can be read.
        if let Err(refused) = seat_count_written(seats.map(String::as_str)) {
            return Some(refused);
        }
        let mut held = self.held_for_the_floor(&address.home, reads).await?;
        let at = held.iter().position(|fact| fact.id == address.local)?;
        let mut edited = held[at].clone();
        jojobot_domain::memory::apply_fact_patch(&mut edited, patch).ok()?;
        let starred = edited.fields.get("starred").is_some_and(|v| v == "true");
        if !starred && seats.is_none() {
            return None;
        }
        held[at] = edited;
        // An edit keeps the stamp the record already has, so it holds none back.
        self.refuses_a_boot_floor_over(
            &address.home,
            &held,
            seats.map(String::as_str),
            None,
            0,
            reads,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::mailbox_handler;
    use crate::memory::testing::*;

    #[tokio::test]
    async fn a_rename_at_its_measured_ceiling_keeps_the_real_boot_within_it() {
        let jojobot = mailbox_handler();
        let mut length = super::super::floor_merge_tests::a_bot_at_the_brink(&jojobot, "mu").await;
        let from = EntityId("bot:mu".into());
        let to = EntityId("bot:epsilon".into());
        let mut renamed = jojobot
            .memory
            .scan_entity(&from)
            .await
            .unwrap()
            .unwrap()
            .entity
            .unwrap();
        renamed.id = to.clone();
        let reads = ReadsLeft::default();
        let seats = jojobot.seats_read(&from, &reads).await.unwrap();
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        let mut predicted = None;
        for _ in 0..4 {
            let held = jojobot.memory.recall(&from).await.unwrap();
            let mut rehomed = held.clone();
            for claim in &mut rehomed {
                claim.home = to.clone();
                claim.subject = to.clone();
            }
            let floor = jojobot
                .boot_floor_if(
                    &from,
                    &rules_in_force(&rehomed),
                    seats,
                    None,
                    None,
                    Some(&renamed),
                )
                .await
                .unwrap();
            if floor.total <= budget {
                predicted = Some(floor.total);
                break;
            }
            let excess = floor.total - budget;
            assert!(length > excess);
            length -= excess;
            let claim = held
                .iter()
                .find(|f| f.fields.get("starred").is_some_and(|v| v == "true"))
                .unwrap();
            jojobot
                .memory
                .update_fact(
                    &claim.address(),
                    FactPatch {
                        content: Some("x".repeat(length)),
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:otto".into()),
                )
                .await
                .unwrap()
                .written()
                .unwrap();
        }
        let predicted = predicted.expect("shrinking the rule leaves the proposed rename room");
        assert!(budget - predicted < 2 * (to.as_str().len() - from.as_str().len()));
        let answer = json_of(
            &jojobot
                .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
                    request_id: None,
                    handle: from.to_string(),
                    to: to.to_string(),
                    parent: None,
                    recorded_at: None,
                    override_token: None,
                    override_reason: None,
                    sid: Some(writing_as(&jojobot)),
                }))
                .await
                .unwrap(),
        );
        assert_ne!(answer["status"], "blocked", "{answer}");
        let actual = jojobot
            .boot_floors_of_bots(&reads)
            .await
            .into_iter()
            .find(|(bot, _)| bot == &to)
            .unwrap()
            .1;
        assert!(
            actual <= budget,
            "accepted rename left floor {actual} over {budget}"
        );
        assert_eq!(
            predicted, actual,
            "the estimate must count the boot it leaves"
        );
    }

    /// **A bot's floor counts the skills listed for it.** Two bots whose boots
    /// differ only by a skill scoped to one of them have floors that differ by at
    /// least that skill's index entry, so a write that would push the bot over
    /// the ceiling is measured with the list its boot carries.
    #[tokio::test]
    async fn a_bots_floor_counts_the_skills_scoped_to_it() {
        use crate::orientation::skills::PROBES;
        let jojobot = handler().with_skills(PROBES);
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        let measured = jojobot.boot_floors_of_bots(&ReadsLeft::default()).await;
        let floors = |slug: &str| {
            measured
                .iter()
                .find(|(bot, _)| bot.as_str() == format!("bot:{slug}"))
                .map(|(_, floor)| *floor)
                .unwrap_or_else(|| panic!("no floor was measured for {slug}: {measured:?}"))
        };
        let entry = serde_json::json!({
            "name": PROBES[1].name,
            "when_to_use": PROBES[1].when_to_use,
        })
        .to_string()
        .chars()
        .count();
        assert!(
            floors("gamma") >= floors("delta") + entry,
            "gamma {} delta {} entry {entry}",
            floors("gamma"),
            floors("delta")
        );
    }

    /// **A write is measured with the box block the boot carries** (rule 498). A
    /// bot that holds one message it has read and not finished boots with a
    /// longer box block than a bot whose box is empty. The case finds the longest
    /// starred rule the check lets the empty-box bot write; the same rule on the
    /// bot with the unfinished message is refused at the write, and on the
    /// empty-box bot it lands.
    #[tokio::test]
    async fn a_write_is_measured_with_the_box_block_the_boot_carries() {
        let jojobot = mailbox_handler();
        for slug in ["gamma", "delta", "sigma"] {
            make_bot(&jojobot, slug).await;
        }
        crate::mailboxes::testing::send(&jojobot, "gamma", "sigma", "the rack needs a look").await;
        jojobot
            .read_mailbox(Parameters(crate::mailboxes::ReadMailboxArgs {
                counts_only: None,
                new_only: None,
                sid: Some(as_bot(&jojobot, "gamma")),
            }))
            .await
            .expect("gamma takes delivery and leaves the message unfinished");
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        let starred = |bot: &str, length: usize| {
            let mut new = NewFact::about(
                EntityId(bot.into()),
                "x".repeat(length),
                jojobot.clock().today_in(&jiff::tz::TimeZone::UTC),
            );
            new.fields = [("starred".to_string(), "true".to_string())].into();
            new
        };
        let delta = EntityId("bot:delta".into());
        let (mut fits, mut over) = (1usize, budget);
        while over - fits > 1 {
            let middle = (fits + over) / 2;
            match jojobot
                .refuses_a_boot_floor_for_capture(
                    &starred("bot:delta", middle),
                    &delta,
                    &ReadsLeft::default(),
                )
                .await
            {
                None => fits = middle,
                Some(_) => over = middle,
            }
        }

        let args = |bot: &str| CaptureArgs {
            fields: Some([("starred".to_string(), "true".to_string())].into()),
            ..capture_args(bot, &"x".repeat(fits))
        };
        let told = json_of(
            &jojobot
                .capture(Parameters(args("bot:gamma")))
                .await
                .expect("capture answers"),
        );
        assert_eq!(
            told["status"], "blocked",
            "the rule takes gamma's boot, box block included, over the ceiling: {told}"
        );
        assert!(
            told["floor"].as_u64() > told["budget"].as_u64(),
            "the refusal is the boot floor's: {told}"
        );
        let landed = capture_ok(&jojobot, args("bot:delta")).await;
        assert_ne!(
            landed["status"], "blocked",
            "delta's empty box leaves room: {landed}"
        );
    }

    /// **A part that is not in the answer weighs nothing.** The answer to the
    /// offer has no `rules` key, and indexing an object for a key it lacks reads
    /// `null`, which is four characters the answer never carried. **Paired**: an
    /// identity that holds its rules is counted by what they serialize to.
    #[test]
    fn carried_rules_that_are_not_in_the_identity_weigh_nothing() {
        let size_of = |parts: &[(String, usize)], name: &str| {
            parts
                .iter()
                .find(|(part, _)| part == name)
                .map(|(_, n)| *n)
                .unwrap_or_else(|| panic!("{name} is named: {parts:?}"))
        };
        let without = serde_json::json!({"bot": {"id": "bot:gamma"}, "charter": null});
        let parts = floor_parts(100, &serde_json::Value::Null, &without);
        assert_eq!(size_of(&parts, "carried rules"), 0, "{parts:?}");
        assert_eq!(
            parts.iter().map(|(_, n)| n).sum::<usize>(),
            100,
            "{parts:?}"
        );

        let rules = serde_json::json!([{"address": "bot:gamma#f1"}]);
        let with = serde_json::json!({"bot": {"id": "bot:gamma"}, "charter": null, "rules": rules});
        let parts = floor_parts(100, &serde_json::Value::Null, &with);
        assert_eq!(
            size_of(&parts, "carried rules"),
            rules.to_string().chars().count(),
            "{parts:?}"
        );
    }

    /// **The floor reads a rule's staleness on the day the boot serves.** The
    /// boot reads it in the run's zone, else the instance's, so a floor read in
    /// UTC disagrees with the boot for the hours the two zones are on different
    /// days: a rule the boot flags as stale, with the note that comes with it,
    /// is measured without it, or the other way round.
    ///
    /// One of the two widest zones is on another day than UTC at any instant,
    /// and the instance lives in it. A rule whose last good day is the earlier
    /// of the two days is stale in exactly the frame that stands on the later
    /// one; the same rule with a last good day far ahead is stale in neither.
    /// The floor must differ between the two exactly when the INSTANCE's frame
    /// is the later one. Both worlds hold the same instance record and a last
    /// good day of the same length, so only staleness separates them.
    #[tokio::test]
    async fn the_floor_reads_staleness_on_the_day_the_instances_zone_is_on() {
        let zone = ["Etc/GMT+12", "Pacific/Kiritimati"]
            .into_iter()
            .find(|zone| {
                let probe = mailbox_handler();
                let tz = jiff::tz::TimeZone::get(zone).expect("a zone");
                probe.clock().today_in(&tz) != probe.clock().today_in(&jiff::tz::TimeZone::UTC)
            })
            .expect("one of the widest zones is on another day than UTC");
        let measured = |far_ahead: bool| async move {
            let jojobot = mailbox_handler();
            make_bot(&jojobot, "gamma").await;
            let utc = jojobot.clock().today_in(&jiff::tz::TimeZone::UTC);
            let there = jojobot
                .clock()
                .today_in(&jiff::tz::TimeZone::get(zone).expect("a zone"));
            let last_good = match far_ahead {
                true => "2999-12-31".to_string(),
                false => utc.min(there).to_string(),
            };
            capture_ok(
                &jojobot,
                CaptureArgs {
                    fields: Some([("starred".to_string(), "true".to_string())].into()),
                    stale_after: Some(last_good),
                    ..capture_args("bot:gamma", "a rule with a last good day")
                },
            )
            .await;
            capture_ok(
                &jojobot,
                CaptureArgs {
                    sets: Some([("timezone".to_string(), zone.to_string())].into()),
                    ..capture_args("topic:instance", "where the operator lives")
                },
            )
            .await;
            let held = jojobot
                .memory
                .recall(&EntityId("bot:gamma".into()))
                .await
                .expect("the bot's claims");
            let rules = jojobot_domain::memory::rules_in_force(&held);
            let total = jojobot
                .boot_floor_if(&EntityId("bot:gamma".into()), &rules, 5, None, None, None)
                .await
                .expect("the floor")
                .total;
            (total, utc, there)
        };
        let (stale_somewhere, utc, there) = measured(false).await;
        let (never_stale, _, _) = measured(true).await;
        // **The note is far wider than the noise.** The two rules render a
        // character apart for reasons that are not staleness, so "carries the
        // note" is a difference of dozens of characters and "does not" is a
        // difference of a few; the case cannot flip on the hour it runs.
        const NOTE: usize = 40;
        let carries_the_note = stale_somewhere > never_stale + NOTE;
        let does_not = stale_somewhere.abs_diff(never_stale) < NOTE;
        assert!(
            carries_the_note || does_not,
            "the floors differ by something that is neither the note nor noise: \
             {stale_somewhere} against {never_stale}"
        );
        assert_eq!(
            carries_the_note,
            there > utc,
            "the floor carries the stale note exactly when the instance's day is the later \
             one: {stale_somewhere} against {never_stale}, the zone on {there} and UTC on {utc}"
        );
    }

    /// **A rule is measured with the widest stamp the store can write.** The
    /// store stamps a record at its own moment, later than the check's, and a
    /// timestamp drops its trailing zeros, so the two renderings can differ by up
    /// to ten characters. A check that measured with its own moment let a rule
    /// pass by that difference and then land over the ceiling.
    ///
    /// The check's own moment here is cut to the whole second: the narrowest
    /// rendering, since a stated clock keeps the fraction of a second and would
    /// otherwise render as wide as the store's. The case reads the store's width
    /// off a record the store stamped. It finds the length at which a starred rule fits the ceiling with
    /// the check's own stamp and is over with the store's, and asks the check
    /// about that rule: it must refuse. A rule shorter by the whole difference in
    /// width is accepted, so a check that refused every rule would not pass.
    #[tokio::test]
    async fn a_rule_is_measured_with_the_widest_stamp_the_store_can_write() {
        let jojobot = mailbox_handler().on_clock(jojobot_domain::clock::Clock::stating(
            "2026-06-01".parse().expect("a day"),
        ));
        make_bot(&jojobot, "gamma").await;
        let bot = EntityId("bot:gamma".into());
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        let starred = |content: String| {
            let mut new = NewFact::about(
                bot.clone(),
                content,
                jojobot.clock().today_in(&jiff::tz::TimeZone::UTC),
            );
            new.fields = [("starred".to_string(), "true".to_string())].into();
            new
        };
        let checks = jiff::Timestamp::from_second(jojobot.clock().now().as_second())
            .expect("a moment cut to the second");
        assert_eq!(
            checks.to_string().len(),
            "2026-06-01T00:00:00Z".len(),
            "the check's moment is cut to the whole second: {checks}"
        );
        // **Stamped on another bot**, so the bot being measured holds no record
        // the floor would also list.
        make_bot(&jojobot, "delta").await;
        let stored = jojobot
            .memory
            .capture(NewFact::about(
                EntityId("bot:delta".into()),
                "a record the store stamps".to_string(),
                jojobot.clock().today_in(&jiff::tz::TimeZone::UTC),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked")
            .inserted_at
            .expect("the store stamps it");
        assert!(
            stored.to_string().len() > checks.to_string().len(),
            "the store's stamp renders wider than the check's: {stored} against {checks}"
        );
        let floor_of = |content: usize, stamp: jiff::Timestamp| {
            let new = starred("x".repeat(content));
            let fact = prospective_fact(&new, 1, Some(stamp));
            let jojobot = &jojobot;
            let bot = &bot;
            async move {
                jojobot
                    .boot_floor_if(bot, &[fact], 5, None, None, None)
                    .await
                    .expect("the floor")
                    .total
            }
        };
        assert!(
            widest_stamp(checks).to_string().len() >= stored.to_string().len(),
            "the widest stamp is at least as wide as one the store wrote"
        );
        let narrow_at_one = floor_of(1, checks).await;
        let fits_with_the_checks_stamp = 1 + (budget - narrow_at_one);
        assert_eq!(floor_of(fits_with_the_checks_stamp, checks).await, budget);
        assert!(
            floor_of(fits_with_the_checks_stamp, stored).await > budget,
            "stored with its own, wider stamp, the rule is over the ceiling"
        );

        let refused = jojobot
            .refuses_a_boot_floor_for_capture(
                &starred("x".repeat(fits_with_the_checks_stamp)),
                &bot,
                &ReadsLeft::default(),
            )
            .await;
        assert!(
            refused.is_some(),
            "a rule that fits only with the check's own, narrower stamp is refused"
        );
        // The positive: a rule short enough to fit with the widest stamp is not.
        let widest = widest_stamp(checks);
        let difference = floor_of(1, widest).await - narrow_at_one;
        assert!(
            jojobot
                .refuses_a_boot_floor_for_capture(
                    &starred("x".repeat(fits_with_the_checks_stamp - difference)),
                    &bot,
                    &ReadsLeft::default(),
                )
                .await
                .is_none(),
            "a rule that fits with the widest stamp is accepted"
        );
    }

    /// **A refusal for a new rule says how much of the margin is kept for the
    /// stamp.** The check measures a new rule with the widest stamp the store can
    /// write, so a writer a few characters short of the ceiling is refused for
    /// room the record may not use. The refusal carries `stamp_margin`, the
    /// characters held back over the narrowest rendering, so that writer is not
    /// left guessing.
    ///
    /// Asserted on the identifier and its number, never on the prose. The number
    /// is worked out here from two renderings of a timestamp, not from the
    /// check's own constants. Paired with the two refusals that measure no new
    /// stamp, a charter and an edit, which carry no margin: a refusal that always
    /// carried one would say something false about them.
    #[tokio::test]
    async fn a_refusal_for_a_new_rule_names_the_stamp_margin() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        let bot = EntityId("bot:gamma".into());
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        let today = jojobot.clock().today_in(&jiff::tz::TimeZone::UTC);
        let starred = |content: String| {
            let mut new = NewFact::about(bot.clone(), content, today);
            new.fields = [("starred".to_string(), "true".to_string())].into();
            new
        };
        let margin = |what: &str, refused: &Option<MemoryError>| match refused {
            Some(MemoryError::BootTooHeavy { stamp_margin, .. }) => *stamp_margin,
            other => panic!("{what}: expected the boot too heavy, got {other:?}"),
        };
        let served = |refused: MemoryError| {
            json_of(
                &crate::memory::declined::memory_declined("capture", refused).expect("an answer"),
            )
        };
        let with_fraction = jiff::Timestamp::new(0, 123_456_789).expect("a moment");
        let whole_second = jiff::Timestamp::new(0, 0).expect("a moment");
        let kept = with_fraction.to_string().len() - whole_second.to_string().len();

        // A new rule: measured with a stamp, so it names the margin.
        let refused = jojobot
            .refuses_a_boot_floor_for_capture(
                &starred("x".repeat(budget)),
                &bot,
                &ReadsLeft::default(),
            )
            .await;
        assert_eq!(margin("a new rule", &refused), kept);
        let body = served(refused.expect("refused"));
        assert_eq!(body["stamp_margin"], kept, "{body}");

        // A charter: no new record, so no stamp and no margin.
        let refused = jojobot
            .refuses_a_boot_floor_for_charter(&bot, &"x".repeat(budget * 2), &ReadsLeft::default())
            .await;
        assert_eq!(margin("a charter", &refused), 0);
        let body = served(refused.expect("refused"));
        assert!(body.get("stamp_margin").is_none(), "{body}");

        // An edit of a rule already stored: it keeps its own stamp, so no margin.
        let stored = jojobot
            .memory
            .capture(starred("a short rule".to_string()))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        let refused = jojobot
            .refuses_a_boot_floor_for_edit(
                &stored.address(),
                &FactPatch {
                    content: Some("x".repeat(budget)),
                    provenance: Some(Provenance::Inference),
                    ..FactPatch::default()
                },
                &bot,
                &ReadsLeft::default(),
            )
            .await;
        assert_eq!(margin("an edit", &refused), 0);
        let body = served(refused.expect("refused"));
        assert!(body.get("stamp_margin").is_none(), "{body}");
    }

    /// **A claim near the ceiling is measured with the timestamp the store will
    /// give it.** The boot serializes a rule's `inserted_at` beside the rule, and
    /// a rule about to be written has none yet, so it was measured with `null`
    /// where the boot will carry a full timestamp. A starred rule that fit the
    /// check by the width of a timestamp was then stored over the ceiling.
    ///
    /// The case finds the length at which a starred rule's floor is exactly the
    /// ceiling without the stamp, and over it with the stamp, and asks the check
    /// about a rule of that length: it must refuse. A rule one character shorter
    /// than the first that overflows is accepted, so a check that refused every
    /// rule would not pass.
    #[tokio::test]
    async fn a_rule_is_measured_with_the_timestamp_it_will_be_stored_with() {
        // A stated clock stamps whole seconds, so every stamp renders at one
        // width. The wall clock's fraction drops trailing zeros, which made the
        // rule miss the ceiling by a character on some runs.
        let jojobot = mailbox_handler().on_clock(jojobot_domain::clock::Clock::stating(
            "2026-06-01".parse().expect("a day"),
        ));
        make_bot(&jojobot, "gamma").await;
        let bot = EntityId("bot:gamma".into());
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        let starred = |content: String| {
            let mut new = NewFact::about(
                bot.clone(),
                content,
                jojobot.clock().today_in(&jiff::tz::TimeZone::UTC),
            );
            new.fields = [("starred".to_string(), "true".to_string())].into();
            new
        };
        let stamp = widest_stamp(jojobot.clock().now());
        let floor_of = |content: usize, stamped: bool| {
            let new = starred("x".repeat(content));
            let fact = prospective_fact(&new, 1, stamped.then_some(stamp));
            let jojobot = &jojobot;
            let bot = &bot;
            async move {
                jojobot
                    .boot_floor_if(bot, &[fact], 5, None, None, None)
                    .await
                    .expect("the floor")
                    .total
            }
        };
        // Floors grow one character per character of content, so the length at
        // which the unstamped floor sits exactly on the ceiling follows from one
        // measurement. The stamped floor is a timestamp wider.
        let unstamped_at_one = floor_of(1, false).await;
        let fits_unstamped = 1 + (budget - unstamped_at_one);
        assert_eq!(floor_of(fits_unstamped, false).await, budget);
        assert!(
            floor_of(fits_unstamped, true).await > budget,
            "the stamp is wider than null, so the stamped rule is over"
        );

        let refused = jojobot
            .refuses_a_boot_floor_for_capture(
                &starred("x".repeat(fits_unstamped)),
                &bot,
                &ReadsLeft::default(),
            )
            .await;
        assert!(
            refused.is_some(),
            "a rule that only fits without its timestamp is refused"
        );
        // The positive: a rule short enough to fit with its timestamp is not.
        let stamp_width = floor_of(1, true).await - unstamped_at_one;
        let fits_stamped = fits_unstamped - stamp_width;
        assert!(
            jojobot
                .refuses_a_boot_floor_for_capture(
                    &starred("x".repeat(fits_stamped)),
                    &bot,
                    &ReadsLeft::default()
                )
                .await
                .is_none(),
            "a rule that fits with its timestamp is accepted"
        );
    }

    /// **A claim written with no standing is measured with the standing the
    /// store gives it**, for each of the three provenances. The floor builds the
    /// claim a write would add before the write happens; a standing of its own
    /// choosing measures a claim the store will not serve. The standing is read
    /// off a claim the store really wrote, not off the function the fix calls,
    /// and the sizes are the floors of the two claims at the same stamp.
    /// **Paired** inside the loop: the provenances that already agreed pass
    /// whichever way the floor decides, so the one that did not is the case.
    #[tokio::test]
    async fn a_claim_with_no_standing_is_measured_with_the_standing_it_is_stored_with() {
        use jojobot_domain::memory::{Provenance, Standing};
        let jojobot = mailbox_handler().on_clock(jojobot_domain::clock::Clock::stating(
            "2026-06-01".parse().expect("a day"),
        ));
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        let day = jojobot.clock().today_in(&jiff::tz::TimeZone::UTC);
        for (provenance, standing) in [
            (Provenance::Testimony, Standing::Settled),
            (Provenance::Observation, Standing::Settled),
            (Provenance::Inference, Standing::Open),
        ] {
            let mut new = NewFact::about(EntityId("bot:delta".into()), "a claim".to_string(), day);
            new.provenance = provenance;
            // An observation has to name where it was read.
            new.fields = [("read_from".to_string(), "a ledger".to_string())].into();
            assert!(
                new.standing.is_none(),
                "this case is about no standing given"
            );

            let stored = jojobot
                .memory
                .capture(new.clone())
                .await
                .expect("capture ok")
                .written()
                .expect("not blocked");
            assert_eq!(
                stored.standing, standing,
                "the store's own answer for {provenance:?}"
            );

            let ordinal: usize = stored
                .id
                .0
                .trim_start_matches('f')
                .parse()
                .expect("f<number>");
            let measured = prospective_fact(&new, ordinal, stored.inserted_at);
            assert_eq!(
                measured.standing, stored.standing,
                "the floor measures a {provenance:?} claim with the wrong standing"
            );

            let bot = EntityId("bot:gamma".into());
            let floor_of = |fact: jojobot_domain::memory::Fact| {
                let jojobot = &jojobot;
                let bot = &bot;
                async move {
                    jojobot
                        .boot_floor_if(bot, &[fact], 5, None, None, None)
                        .await
                        .expect("the floor")
                        .total
                }
            };
            assert_eq!(
                floor_of(measured).await,
                floor_of(stored).await,
                "the floor's size for a {provenance:?} claim is not the stored claim's size"
            );
        }
    }
}

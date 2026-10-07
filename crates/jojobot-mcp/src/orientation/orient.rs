//! **The one orientation, anonymous or identified.**
//!
//! Naming a bot adds the identity half to an answer that is otherwise the same
//! text and the same snapshot; it does not open a second way in. The one call
//! site is the point, and `there_is_exactly_one_orientation_verb` counts it.

use super::instance_zone::{InstanceZone, ZoneFrom, zone_answer};
use super::*;

/// **A bot's own box, in the boot's snapshot** — [`mailbox_json`] with its
/// `quarantined` report dropped when there is nothing in it. A zero count is
/// not a fact worth a field: an agent reading `quarantined.count`
/// unconditionally already treats an absent report as zero, so nothing here
/// is a silent elision — see [`quarantined_json`] for what the field holds
/// when there is something to say.
fn own_mail_json(mailbox: &jojobot_domain::mailbox::Mailbox) -> serde_json::Value {
    let mut mail = mailbox_json(mailbox);
    if mail["quarantined"]["count"] == 0 {
        if let Some(object) = mail.as_object_mut() {
            object.remove("quarantined");
        }
    }
    mail
}

/// **What a piece of text costs in the answer a caller receives**, in
/// characters: serialized, where a newline or a quote is two. The boot's
/// ceiling bounds that answer, so everything the boot ranks against it is
/// sized this way. Escaping is per character, so costs add up the way text
/// does.
fn serialized_len(text: &str) -> usize {
    serde_json::to_string(text)
        .map(|quoted| quoted.chars().count().saturating_sub(2))
        .unwrap_or_else(|_| text.chars().count())
}

/// **Null out one rule's `details`, marked** — the same shape whichever
/// caller reaches for it: a ranked cut, or the floor measurement below that
/// has to know the answer's size with every rule's reasoning already gone.
pub(super) fn elide_rule_details(rule: &mut serde_json::Value) {
    let Some(details) = rule["details"].as_str().map(str::to_string) else {
        return;
    };
    let Some(fields) = rule.as_object_mut() else {
        return;
    };
    fields.insert("details".into(), serde_json::Value::Null);
    fields.insert("details_elided".into(), true.into());
    fields.insert("details_bytes".into(), details.len().into());
    let subject = fields
        .get("subject")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    fields.insert(
        "details_note".into(),
        format!(
            "this rule's own reasoning is not in this boot — recall {subject} with facts: true \
             to read it whole"
        )
        .into(),
    );
}

/// **Rank the rules' `details`, newest first, against a budget already
/// computed to be what is left of the ceiling** — see `orient` for where
/// that number comes from. `content` and `charter` are never here:
/// `content` is never cut at all, and `charter` is always served whole
/// regardless of size (rule 138's own dispatch: "the interesting case...
/// cutting it silently is worse than shipping it"), so it costs nothing to
/// rank and is counted in the floor instead, once, rather than a second time
/// here.
///
/// **The essay is not ranked here at all, for either shape of boot** — see
/// `orient` for what each gets and why. `head`'s always-take-the-first
/// guarantee is right for this COLLECTION: a rules-heavy identity returning
/// no reasoning at all is worse than one oversized entry.
///
/// `identity`'s own `rules` are mutated in place for whichever `details` the
/// budget reached. A `Value::Null` identity (anonymous) carries no rules and
/// this is a no-op.
fn rank_rule_details(identity: &mut serde_json::Value, budget: usize) {
    let mut rule_slots: Vec<(usize, usize)> = Vec::new();
    if let Some(rules) = identity["rules"].as_array() {
        for i in (0..rules.len()).rev() {
            if let Some(details) = rules[i]["details"].as_str() {
                // **Sized serialized**, for the reason `essay_for_boot` sizes
                // its units that way: the ceiling bounds what a caller
                // receives, where a quote or a newline costs two characters.
                rule_slots.push((i, serialized_len(details)));
            }
        }
    }
    let kept_rules: std::collections::HashSet<usize> = (text::Capped { budget })
        .head_or_none(&rule_slots, |(_, len)| *len)
        .kept()
        .iter()
        .map(|(i, _)| *i)
        .collect();

    // **An anonymous boot's `identity` is `Value::Null`, and it stays that
    // way.** Indexing a `Value` with `IndexMut` (to reach `rules` for the
    // elision) auto-vivifies `Null` into an empty object on the first write
    // — so this is read-only above and this guard is what keeps it that way
    // when there is nothing to rank.
    if identity.is_null() {
        return;
    }
    if let Some(rules) = identity["rules"].as_array_mut() {
        for (i, rule) in rules.iter_mut().enumerate() {
            if !kept_rules.contains(&i) {
                elide_rule_details(rule);
            }
        }
    }
}

/// **What the essay's own text is for this boot, and whether the ceiling had
/// a say in it — the two shapes of boot are not the same question.**
///
/// **A named boot never gets the remainder — it is not a ranking candidate,
/// it is a rule.** The core already carries the kinds, what a claim carries,
/// the structural type questions and the call that reaches the rest, which
/// is the teaching a session needs before it can write anything; the
/// remainder waits behind an anonymous boot, always, whatever the ceiling
/// has room for. Resting this on arithmetic instead — ranking the remainder
/// against whatever budget happened to be left — is the shape that broke:
/// the answer depended on a margin (206 characters, on the lightest real
/// identity measured) that a single new sentence in the essay could flip
/// with nobody noticing.
///
/// **An anonymous boot gets the whole essay, core and remainder joined, and
/// that answer still goes through the cap** — [`text::Capped::head_or_none`]
/// — rather than being assumed to fit because there is no identity to pay
/// for. Today's headroom is real (an anonymous floor measured lighter than
/// the lightest named one by over a thousand characters, purely from the
/// identity-shaped JSON a named boot always carries) but it is a fact about
/// today's essay, not a guarantee, and the one candidate here is the one
/// this exists to catch if that ever changes.
///
/// What `essay_for_boot` ships, and how it says what it left out.
pub(crate) struct EssayForBoot {
    /// The text to ship. `None` only when `brief` dropped it outright —
    /// every other shape, named or anonymous, always carries the core at
    /// least.
    pub(crate) text: Option<String>,
    /// Whether anything was left out relative to the whole essay: always
    /// true for `brief` and for a named boot (core-only is a rule, not a
    /// ranking outcome), and true for an anonymous boot only when the
    /// ceiling actually cut something.
    pub(crate) elided: bool,
    /// The note explaining the cut, or `None` when nothing was cut. Built
    /// here rather than at the call site, because only this function knows
    /// which unit stopped the prefix.
    pub(crate) note: Option<String>,
}

/// **What the essay's own text is for this boot, and whether the ceiling had
/// a say in it — the two shapes of boot are not the same question.**
///
/// **A named boot never gets the remainder — it is not a ranking candidate,
/// it is a rule.** The core already carries the kinds, what a claim carries,
/// the structural type questions and the call that reaches the rest, which
/// is the teaching a session needs before it can write anything; the
/// remainder waits behind an anonymous boot, always, whatever the ceiling
/// has room for.
///
/// **An anonymous boot always gets the core, plus as many whole units of the
/// remainder as `budget` allows, taken strictly in order** — the un-headed
/// preamble first (`essay::remainder_units`'s own first unit), then each
/// `##` section whole, a `###` subheading riding with its parent rather than
/// standing on its own. `budget` is the remainder's own allowance: the
/// caller (`orient`) has already paid for the core in the floor, so this
/// never subtracts it and always prepends it. **Never cut inside a unit,
/// and never skip one to fit a later one** — `text::Capped::head_or_none`'s
/// own strict-prefix behaviour is exactly this rule.
///
/// A unit left out is named by its own `##` heading in `note`; the un-headed
/// preamble, when it is the one left out, is named in plain words instead,
/// because it has no heading to be named by.
fn essay_for_boot(brief: bool, identity: &serde_json::Value, budget: usize) -> EssayForBoot {
    if brief {
        return EssayForBoot {
            text: None,
            elided: true,
            note: None,
        };
    }
    if !identity.is_null() {
        return EssayForBoot {
            text: Some(essay::ORIENTATION_CORE.to_string()),
            elided: true,
            note: Some(
                "a named boot's orientation is the essay's core; call start_here again naming \
                 no bot to read the whole essay"
                    .to_string(),
            ),
        };
    }

    let (preamble, sections) = essay::remainder();
    let mut candidates: Vec<(Option<&str>, &str)> = Vec::with_capacity(sections.len() + 1);
    candidates.push((None, preamble));
    candidates.extend(sections.iter().map(|s| (Some(s.heading), s.body)));

    // **Sized by what a unit costs serialized, not as text.** The budget bounds
    // the answer a caller receives, where a newline or a quote is two
    // characters, so a unit that fits as text can take the whole answer over
    // the ceiling while the boot says nothing was left out. Escaping is per
    // character, so the cost of the units adds up the way their text does.
    let kept =
        (text::Capped { budget }).head_or_none(&candidates, |(_, body)| serialized_len(body));
    let kept_items = kept.kept();

    let mut text = essay::ORIENTATION_CORE.to_string();
    for (_, body) in kept_items {
        text.push_str(body);
    }

    if kept.omitted() == 0 {
        return EssayForBoot {
            text: Some(text),
            elided: false,
            note: None,
        };
    }

    let preamble_shipped = !kept_items.is_empty();
    let shipped_sections = kept_items.len() - usize::from(preamble_shipped);
    let left_out: Vec<&str> = sections[shipped_sections..]
        .iter()
        .map(|s| s.heading)
        .collect();

    let note = if preamble_shipped {
        format!(
            "this boot's ceiling could not fit the whole essay. Left out: {}. Call start_here \
             again with section set to one of those exact headings to read it whole.",
            left_out.join(", ")
        )
    } else {
        format!(
            "this boot's ceiling could not fit any of the essay's remainder — not even its \
             opening paragraphs. Left out: the remainder's opening paragraphs, plus every \
             section: {}. Call start_here again with section: \"opening\" for the paragraphs, \
             or with section set to one of those exact headings for a section, to read either \
             whole.",
            left_out.join(", ")
        )
    };

    EssayForBoot {
        text: Some(text),
        elided: true,
        note: Some(note),
    }
}

/// **The entity half of the snapshot**, summarised from the one index read a
/// whole answer shares. A free function so the boot and a write that measures
/// the boot's floor summarise the same way.
pub(super) fn entity_summary(
    index: &Result<Vec<Entity>, jojobot_domain::memory::MemoryError>,
) -> serde_json::Value {
    match index {
        Ok(entities) => {
            // **The snapshot is a BROWSE, and a browse excludes an
            // archived entity** — the same broad-door/direct-door split
            // a claim's own archived state already has (`list_entities`
            // holds it too). Offering an archived bot as one to boot as
            // is the harm the operator's ruling is about; booting AS it
            // by name is the direct door and is untouched by this — see
            // `identity`, which reads `index` rather than this filtered
            // view.
            let browsable = || entities.iter().filter(|e| e.browsable());
            let mut by_kind = std::collections::BTreeMap::<&str, usize>::new();
            for e in browsable() {
                let kind = e.id.as_str().split(':').next().unwrap_or("unknown");
                *by_kind.entry(kind).or_default() += 1;
            }
            // **The one kind that is NAMED and not merely counted**, because
            // it is the only one this door asks you to pick from. An
            // anonymous boot could see that five identities exist and could
            // not learn one to boot as — while the refusal for an unknown
            // bot lists every real one, so the only route to a usable name
            // was to guess a wrong one and read it off the complaint. The
            // door's own suggested next step was unreachable from the door.
            //
            // **Names only, and the refusal's own spelling.** Counts and
            // charters belong to a caller weighing its own work; this one
            // owns nothing and is choosing an identity. `bots`, full
            // handles, so one record has one shape wherever it appears.
            let mut bots: Vec<&str> = browsable()
                .filter(|e| e.kind == EntityKind::BOT)
                .map(|e| e.id.as_str())
                .collect();
            bots.sort_unstable();
            serde_json::json!({
                "available": true,
                "count": entities.len(),
                "by_kind": by_kind,
                "bots": bots,
            })
        }
        Err(_) => serde_json::json!({
            "available": false,
            "note": "the memory world is not reachable right now — its tools will say why",
        }),
    }
}

/// **The two lines a boot reads off the instance's own record**, bundled so the
/// floor measures both and a third line is one field rather than one more
/// argument.
pub(super) struct InstanceLines<'a> {
    pub(super) timezone: &'a serde_json::Value,
    pub(super) operator: &'a serde_json::Value,
}

/// **What a boot cannot cut, in characters** — the sum `orient` measures and a
/// write that would grow it measures too, so the two cannot disagree about it.
/// The essay's core rides in it for every boot that is not `brief`, exactly
/// like the charter: it always ships and is never ranked, so its true cost is
/// counted here, once.
pub(super) fn floor_len(
    brief: bool,
    snapshot: &serde_json::Value,
    identity_floor: &serde_json::Value,
    session: &serde_json::Value,
    carried: &serde_json::Value,
    clock: Option<serde_json::Value>,
    instance: InstanceLines<'_>,
) -> usize {
    let core = (!brief).then_some(essay::ORIENTATION_CORE);
    serde_json::json!({
        "orientation": core,
        "orientation_elided": true,
        "skills": skills::index(),
        "snapshot": snapshot,
        "identity": identity_floor,
        "session": session,
        "carried_session": carried,
        "clock": clock,
        "timezone": instance.timezone,
        "operator": instance.operator,
    })
    .to_string()
    .chars()
    .count()
}

/// The door's own arguments, already validated — bundled so `orient` takes
/// one argument rather than growing a new parameter every time the door
/// learns a new one.
pub(crate) struct OrientRequest<'a> {
    pub(crate) bot: Option<&'a EntityId>,
    pub(crate) brief: bool,
    pub(crate) resume: Option<&'a str>,
    /// The IANA zone this run resolves days in, validated at the door.
    pub(crate) timezone: Option<&'a str>,
    /// What the instance's record holds for its zone: what a run that sends
    /// none is answered in.
    pub(crate) instance_zone: InstanceZone,
    pub(crate) today: Option<jiff::civil::Date>,
    /// What the handle this caller arrived with is worth — from
    /// [`Jojobot::standing`], and `Null` when they arrived with none.
    pub(crate) carried: serde_json::Value,
    /// **A role to claim, by the name the caller chose.** `None` is the
    /// ordinary boot, unchanged. Decided only once this call's own sid is
    /// known — see where `session` is built below.
    pub(crate) claim: Option<&'a str>,
}

impl Jojobot {
    /// The one orientation, anonymous or identified — **the one call site is
    /// the point.** Naming a bot adds the identity half to an answer that is
    /// otherwise the same text and the same snapshot; it does not open a second
    /// way in.
    pub(crate) async fn orient(&self, req: OrientRequest<'_>) -> Result<CallToolResult, McpError> {
        let OrientRequest {
            bot,
            brief,
            resume,
            timezone,
            instance_zone,
            today,
            carried,
            claim,
        } = req;
        // **The zone this call resolves days in:** the one the session sent,
        // else the zone a resumed run already holds, else the instance's, else
        // UTC. A run that holds no zone of its own is answered in the instance's
        // on every later call too (see [`Jojobot::dated`]); a resume that sends
        // none keeps the zone it holds, and the boot's own days (a rule's
        // staleness, the day a claim is dated) are read in it too.
        let held_zone = match (timezone, resume) {
            (None, Some(sid)) => self.registry.lookup(sid).and_then(|handle| handle.zone),
            _ => None,
        };
        let frame = timezone.or(held_zone.as_deref()).or(instance_zone.name());
        let frame_zone = || {
            frame
                .and_then(|name| jiff::tz::TimeZone::get(name).ok())
                .unwrap_or(jiff::tz::TimeZone::UTC)
        };
        // **A bot that carries a role claims it at the door when the caller
        // names none.** An explicit `claim` always wins, and a bot with no key
        // boots exactly as it did. The claim that follows is the ordinary one,
        // so a refusal is said at the door the same way.
        //
        // **A wrap code takes no role.** It reopens a wrapped run for one last
        // change, so neither the named claim nor the bot's own is decided.
        let by_wrap_code = resume.is_some_and(jojobot_domain::session::is_wrap_code);
        let claim = claim.filter(|_| !by_wrap_code);
        let carried_role: Option<String> = match (claim, bot) {
            (None, Some(bot)) if !by_wrap_code => {
                self.memory.fields(bot).await.ok().and_then(|fields| {
                    fields
                        .get(jojobot_domain::memory::CLAIMS_ROLE)
                        .map(|role| role.trim().to_string())
                        .filter(|role| !role.is_empty())
                })
            }
            _ => None,
        };
        let claimed_by_the_bot = carried_role.is_some();
        let claim = claim.or(carried_role.as_deref());
        // The entity index is read ONCE for the whole answer. Three parts of
        // a boot need it — the counts by kind, which boxes the caller drains,
        // and the identity itself — and reading it three times would mean
        // three remote round trips per boot, and three reads that can
        // disagree with one another inside a single payload.
        //
        // Best-effort per world: orientation must land even when one world is
        // down — a fresh agent on a half-configured server still gets the map.
        let index = self.memory.list_entities(None).await;
        let entities = entity_summary(&index);
        // A memory world that is down cannot answer who anybody is; the
        // snapshot below already says so, and this stays null rather than
        // claiming the identity is missing.
        let identity = match (bot, &index) {
            (None, _) | (_, Err(_)) => serde_json::Value::Null,
            (Some(bot), Ok(index)) => match self
                .identity(
                    index,
                    bot,
                    resume.is_some(),
                    // **The day this run states, and the clock in its own zone
                    // when it states none.** The boot is where a run declares
                    // the day it is working in, so a rule's staleness read on
                    // the server's clock would tell a run working through March
                    // that its own rules expired months ago — in the very call
                    // that accepted the day (rule 222).
                    match today {
                        Some(stated) => stated,
                        None => self.clock().today_in(&frame_zone()),
                    },
                )
                .await?
            {
                Ok(identity) => identity,
                // A name that is no bot: the guards' own shape, so one
                // client-side branch handles every "jojobot declined" answer —
                // but with the door's own body, not the generic absence one.
                Err(candidates) => {
                    return Ok(booting_unknown(bot, &candidates, index));
                }
            },
        };
        // **The mailbox world is read AFTER the identity, and that ordering is
        // load-bearing.** Resolving the identity is what heals this bot's box
        // when it is missing. Read the board first and one payload says, in the
        // snapshot, that this bot has no box, and says in the identity beside it
        // that the box was missing and has just been opened — two halves of one
        // answer disagreeing about the world, with nothing to tell a session
        // which to believe. It is still exactly ONE read; it just happens once
        // the repair this boot performs has landed.
        //
        // **The snapshot is scoped the same way the listing is.** Per-state
        // counts for every box on the server pose the question the own-box rule
        // then has to answer in prose: is that unread one mine? An anonymous
        // `start_here` owns nothing, which is exactly right for a caller that
        // only posts.
        //
        // **Mail hangs off the bot that owns it rather than being a population
        // of its own.** A box belongs to exactly one bot and is not a peer of
        // it, so a boot that listed boxes beside bots would ask a caller to
        // hold two directories and the correspondence between them. Addressing
        // is by handle; a box name is not something anybody needs.
        let snapshot = self.snapshot_block(bot, entities).await;
        // **Only after the identity resolved.** A name that is no bot boots
        // nothing, so it starts no session and sweeps nothing either — binding
        // a connection to an identity jojobot just refused would be a session
        // belonging to nobody.
        let mut session = match bot {
            None => serde_json::Value::Null,
            Some(bot) => match self.attach(bot, resume, timezone, today).await {
                Ok(session) => session,
                // A handle that addresses nothing stops the whole answer.
                // Handing back orientation around it would bury the one thing
                // the caller has to act on.
                Err(refused) => return Ok(refused),
            },
        };
        // **The lease's own half: decided only once this call's sid is
        // known.** A role named without a resulting handle (the resume-or-new
        // choice came back instead) decides nothing — there is no claimant
        // yet, and the caller answers the choice, then names the role again.
        if let (Some(bot), Some(role)) = (bot, claim) {
            let sid = session
                .get("sid")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(sid) = sid {
                let now = self.clock().now();
                let today_or_clock = today.unwrap_or_else(|| self.clock().today_in(&frame_zone()));
                // **Its own gate, taken fresh.** `attach`'s gate is already
                // released by the time control reaches here, and
                // materializing a session record below needs the same proof
                // of serialization `session_for` requires everywhere else it
                // is called.
                let gate = self.registry.gate(bot.as_str());
                let _serialized = gate.lock().await;
                let outcome = self
                    .decide_role_claim(bot, role, &sid, now, today_or_clock)
                    .await;
                // **A GRANTED claim has done something, so its session record
                // is written now, through the same lazy path any first write
                // uses.** A refused claim writes nothing new — the caller
                // that lost the race gets no card for a run that never did
                // anything.
                if outcome.get("status").and_then(|s| s.as_str()) == Some("taken") {
                    match self.identified(Some(&sid)) {
                        Ok(caller) => {
                            let derived = format!("claimed the {role} role");
                            if let Err(e) = self
                                .session_for(&_serialized, &caller, None, Some(&derived))
                                .await
                            {
                                tracing::warn!(
                                    error = ?e, %sid, role,
                                    "a granted role claim could not materialize its session \
                                     record"
                                );
                            }
                        }
                        Err(_) => tracing::warn!(
                            %sid, role,
                            "a granted role claim's own sid did not resolve to a caller"
                        ),
                    }
                }
                let mut outcome = outcome;
                if claimed_by_the_bot && let Some(obj) = outcome.as_object_mut() {
                    // **Said, because a caller that named no claim did not ask
                    // for one** and would otherwise find a lease it never made.
                    obj.insert("claimed_because".into(), "this bot's claims_role".into());
                }
                if let Some(obj) = session.as_object_mut() {
                    obj.insert("claim".into(), outcome);
                }
            }
        }
        // **Which zone this boot used, and where it came from**, read from the
        // run once it is bound: a resume that sent none keeps its own.
        let timezone_answer = {
            let resumed = session
                .get("resumed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let run_zone = session
                .get("sid")
                .and_then(|v| v.as_str())
                .and_then(|sid| self.registry.lookup(sid))
                .and_then(|handle| handle.zone);
            match (timezone, resumed, run_zone.as_deref()) {
                (Some(sent), _, _) => zone_answer(Some(sent), ZoneFrom::Session, &instance_zone),
                (None, true, Some(own)) => zone_answer(Some(own), ZoneFrom::Run, &instance_zone),
                (None, _, _) => match instance_zone.name() {
                    Some(name) => zone_answer(Some(name), ZoneFrom::Instance, &instance_zone),
                    None => zone_answer(None, ZoneFrom::Default, &instance_zone),
                },
            }
        };
        // **Who the operator is, in one line**, read from the instance's record
        // beside the zone. It is measured in the floor below like the zone is.
        let operator_answer = self.operator_answer().await;
        // **ONE declared ceiling for the WHOLE answer** — [`text::BOOT_ANSWER`]
        // — not for its prose alone (rule 138's own bar: a payload the client
        // cannot read, not a field inside it). Measure the FLOOR first:
        // everything that ships whatever the ranking below decides — bot
        // metadata, charter whole (it is never cut, see `rank_rule_details`),
        // the essay's own core (never cut, every boot, see `essay_for_boot`),
        // every rule's own structural fields (address, dates, provenance,
        // standing, status, fields, refs), session, snapshot, skills — with
        // every rule's `details` already gone. What is LEFT of the ceiling
        // after that floor is what the rules' `details` compete for, and —
        // for an anonymous boot only — what the remainder's own units are
        // ranked against; see `essay_for_boot`.
        let mut floor_identity = identity.clone();
        if let Some(rules) = floor_identity
            .get_mut("rules")
            .and_then(|r| r.as_array_mut())
        {
            for rule in rules.iter_mut() {
                elide_rule_details(rule);
            }
        }
        // **The essay's core rides in the floor for every boot, named or
        // anonymous, exactly like the charter — it always ships and is
        // never ranked, so its true cost is counted here, once, rather than
        // competing with the rules' `details` or the remainder's own units.**
        let floor_len = floor_len(
            brief,
            &snapshot,
            &floor_identity,
            &session,
            &carried,
            self.stated_clock(),
            InstanceLines {
                timezone: &timezone_answer,
                operator: &operator_answer,
            },
        );
        let remaining_for_prose = text::BOOT_ANSWER.budget.saturating_sub(floor_len);
        // **A floor over the ceiling is said, not refused.** The boot never
        // declines, and any entity grows the snapshot every bot's boot carries,
        // so a bot can be pushed over by a write that nobody could refuse. The
        // answer carries the sizes and the ways down instead.
        let over_the_ceiling = bot
            .filter(|_| floor_len > text::BOOT_ANSWER.budget)
            .map(|bot| {
                super::floor::over_the_ceiling(
                    bot,
                    floor_len,
                    &super::floor::floor_parts(floor_len, &snapshot, &floor_identity),
                )
            });

        let mut identity = identity;
        rank_rule_details(&mut identity, remaining_for_prose);
        let essay = essay_for_boot(brief, &identity, remaining_for_prose);
        let mut answer = serde_json::json!({
            "orientation": essay.text,
            // **The elision is marked, and that is all it is.** The essay used
            // to arrive stamped with a version so a returning session could ask
            // whether the copy it held was current; the stamp is gone, and no
            // staleness check replaces it. What is left is the marker every
            // elision on this surface owes — less came back, and the caller is
            // told so rather than left to infer withheld from empty.
            //
            // **False only when an anonymous boot got the whole essay** — the
            // only shape that is ever NOT missing something; every other
            // shape — `brief`, a named boot's core-only, or an anonymous
            // boot the ceiling cut — is elided, and `essay_for_boot` is
            // where that is decided.
            "orientation_elided": essay.elided,
            // **Names and when-to-use lines, never bodies.** A session that
            // needs a procedure fetches it by name; a boot that shipped every
            // one would spend a session's attention on the jobs it is not
            // doing, and get worse with each skill added. `brief` does not
            // narrow this — the index is what CHANGES between builds, so it is
            // exactly what a returning caller still needs.
            "skills": skills::index(),
            "snapshot": snapshot,
            "identity": identity,
            "session": session,
            // **The handle you arrived with, and the one this call hands you,
            // are two different things** — so they are two fields. `session` is
            // the run this door just started or picked up; this says what the
            // handle you were already carrying is worth, which is what a caller
            // that came back to a server it does not recognise is really asking.
            "carried_session": carried,
            // **A server acting out a day says so at the door.** Absent is the
            // ordinary answer and means the real clock; present is the
            // exception, and a session reads it before it writes anything.
            "clock": self.stated_clock(),
            // **The zone this boot answered days in, and who named it** — the
            // session, the instance, the run it resumed, or neither.
            "timezone": timezone_answer,
            // **The operator, by the handle a `waiting_on` holds**, or one
            // sentence saying there is no entity yet and how to make one.
            "operator": operator_answer,
        });
        // **`brief` needs no note — the caller set that flag and already
        // knows why.** Every other reason an answer carries less than the
        // whole essay is `essay_for_boot`'s own note, already built there:
        // a named boot's core-only is a rule, and an anonymous boot's cut
        // names exactly which units were left out.
        if !brief && let Some(obj) = answer.as_object_mut() {
            if let Some(note) = essay.note {
                obj.insert("orientation_note".into(), note.into());
            }
        }
        if let (Some(notice), Some(obj)) = (over_the_ceiling, answer.as_object_mut()) {
            obj.insert("over_the_ceiling".into(), notice);
        }
        json_result(&answer)
    }
}

impl Jojobot {
    /// **The snapshot block of a boot** — the world's counts, the mail board
    /// and the vocabulary — read from the stores as they stand. `entities` is
    /// the entity half, already summarised from the one index read the whole
    /// answer shares.
    ///
    /// It is a method of its own so a write that must measure the boot's floor
    /// reads the same snapshot the boot serves, never a copy of it.
    pub(crate) async fn snapshot_block(
        &self,
        bot: Option<&EntityId>,
        entities: serde_json::Value,
    ) -> serde_json::Value {
        let listed = self.mailboxes.list_mailboxes().await;
        let mail = match &listed {
            // **When there is no roster to hang mail on, mail answers for
            // itself.** The bots come from Memory, so a Memory that cannot be
            // read would otherwise take the whole mail board down with it —
            // and whose box is whose is the mail world's own fact, never
            // Memory's. So an unreadable entity index costs the roster and
            // nothing else.
            Ok(boxes) if entities["available"] == serde_json::Value::Bool(false) => {
                let mine = self.ownership_of(boxes, bot);
                serde_json::json!({
                    "available": true,
                    "note": "the entity roster is unreadable, so mail is listed by owner here \
                             rather than beside the bots. Another bot's mail counts are never \
                             shown here, whichever way this boot lists them.",
                    "by_owner": boxes
                        .iter()
                        .map(|b| serde_json::json!({
                            "owner": b.owner.as_str(),
                            "yours": mine.drains(b.name.as_str()),
                            "mail": match mine.drains(b.name.as_str()) {
                                true => own_mail_json(b),
                                false => serde_json::json!({}),
                            },
                        }))
                        .collect::<Vec<_>>(),
                })
            }
            Ok(_) => serde_json::json!({
                "available": true,
                "note": "another bot's mail counts are never shown here — only its own boot or \
                         its own poll can see them",
            }),
            Err(_) => serde_json::json!({
                "available": false,
                "note": "the mailbox world is not reachable right now — its tools will say why",
            }),
        };
        let mut entities = entities;
        if let (Ok(boxes), Some(object)) = (&listed, entities.as_object_mut()) {
            let mine = self.ownership_of(boxes, bot);
            let by_owner = |handle: &str| {
                let owned: Vec<_> = boxes
                    .iter()
                    .filter(|b| b.owner.as_str() == handle)
                    .collect();
                match owned.as_slice() {
                    [] => None,
                    [b] => Some(match mine.drains(b.name.as_str()) {
                        true => own_mail_json(b),
                        // **Somebody else's queue is not yours to weigh, and
                        // neither is what jojobot could not read on it.** An
                        // agent reading this boot cannot act on a stranger
                        // box's fault — the caller who can is a sender, and
                        // `list_sent` is where they read it.
                        false => serde_json::json!({}),
                    }),
                    // **Two boxes is damage, and no count over one of them is
                    // an answer.** One box per bot is settled, so weighing the
                    // first match would tell a session its mail was measured
                    // whole while a second box sat beside it, unnamed.
                    several => Some(several_boxes_json(several.iter().copied())),
                }
            };
            if let Some(serde_json::Value::Array(bots)) = object.get_mut("bots") {
                for entry in bots.iter_mut() {
                    let Some(handle) = entry.as_str().map(str::to_string) else {
                        continue;
                    };
                    // Yours is a fact about identity, not about the board: the
                    // bot you booted as is the one whose mail is yours.
                    let yours = bot.is_some_and(|booted| booted.as_str() == handle);
                    *entry = serde_json::json!({
                        "handle": handle,
                        "yours": yours,
                        // **Absent rather than empty when a bot has no box.**
                        // A box opens with the bot that owns it, so this is
                        // damage, and it is named where a boot repairs it
                        // rather than rendered as a bot with a quiet inbox.
                        "mail": by_owner(&handle),
                    });
                }
            }
        }
        // **The vocabulary the software arrived holding, named at the door.**
        //
        // A session that has just booted has to write something, and which
        // kinds a handle may carry is the first thing it needs. It used to
        // find out by declaring one and reading a refusal, or by asking a
        // question it had no reason to ask: the distinction was invisible
        // until it tripped, which is the safe-default rule upside down.
        //
        // **Names only, never bodies.** What a kind means and which keys it
        // asks for is bigger than the list and is a deliberate second read.
        // Origin is left off too: an agent booting does not act differently
        // for knowing which name is closed to redeclaration, and it stays
        // exactly where that question is actually asked — declaring a kind
        // or a type, or reading one directly.
        //
        // **Read from the store rather than from the list this build ships**,
        // for the same reason the boot's other reads are: a kind a caller
        // declared is part of the vocabulary, and a shipped kind the store
        // somehow lost is not.
        let vocabulary = match self.memory.declared_kinds().await {
            Ok(kinds) => {
                let mut named: Vec<serde_json::Value> = kinds
                    .iter()
                    .map(|(token, _origin)| serde_json::json!({ "kind": token }))
                    .collect();
                named.sort_by_key(|k| k["kind"].as_str().unwrap_or("").to_string());
                serde_json::json!({ "available": true, "kinds": named })
            }
            // **Best-effort, like every other half of this answer.** A boot
            // that could not read the vocabulary says so rather than naming
            // none: "there are no kinds" and "jojobot could not look" are
            // different claims and a caller acts on both.
            Err(_) => serde_json::json!({
                "available": false,
                "kinds": [],
                "note": "the vocabulary is not readable right now, so this is not a claim that \
                         the software ships none",
            }),
        };
        // **A declared type, named beside the kinds — the same block, because
        // it is the one place that answers what vocabulary this instance
        // has.** A declared type was invisible from where a cold agent
        // stands: the boot named every shipped kind and not one declared
        // type, and every phase of a long-running session is deliberately
        // fresh, so an earlier session's own catalogue fetch buys a later one
        // nothing. The only path back was guessing a type name that does not
        // exist and reading the refusal's own candidate list — which
        // presupposes already suspecting one exists. Read independently of
        // `kinds`: a type roster outage must not read as a kind vocabulary
        // outage, or the other way round.
        let mut vocabulary = vocabulary;
        match self.memory.declared_types().await {
            Ok(types) => {
                let mut named: Vec<serde_json::Value> = types
                    .iter()
                    .map(|declared| serde_json::json!({ "type": declared.name }))
                    .collect();
                named.sort_by_key(|t| t["type"].as_str().unwrap_or("").to_string());
                vocabulary["types_available"] = serde_json::json!(true);
                vocabulary["types"] = serde_json::json!(named);
            }
            Err(_) => {
                vocabulary["types_available"] = serde_json::json!(false);
                vocabulary["types"] = serde_json::json!([]);
                vocabulary["types_note"] = serde_json::json!(
                    "the declared types are not readable right now, so this is not a claim \
                     that the software declares none"
                );
            }
        }
        serde_json::json!({ "entities": entities, "mail": mail, "vocabulary": vocabulary })
    }
}

impl Jojobot {
    /// **Decide and, if granted, write a claim on a named role — atomically,
    /// inside the write itself.**
    ///
    /// The lease's own half of the boot door. [`jojobot_domain::session::claim_role`]
    /// is not called here: it is called by the store, against whatever it
    /// holds at the moment it writes — see `role_write_in` and its call
    /// sites in `Memory::capture`/`Memory::update_fact`. Calling it here
    /// first, against a separate read, would decide against a value that
    /// could already be stale by the time the write lands; the refusal this
    /// function reports is the store's own, taken in the same act as the
    /// write it gates.
    ///
    /// **A claim never multiplies records.** An existing claim record for
    /// this role is patched in place — the same record renews or changes
    /// hands, and a bot's rules stay whatever they were before anybody
    /// claimed anything. Only the first claim ever made for a role captures
    /// a new one.
    ///
    /// Naming no role never reaches this — see the call site in
    /// [`Jojobot::orient`].
    async fn decide_role_claim(
        &self,
        bot: &EntityId,
        role: &str,
        claimant: &str,
        now: jiff::Timestamp,
        today: jiff::civil::Date,
    ) -> serde_json::Value {
        let holder_key = jojobot_domain::session::role_holder_key(role);
        let claimed_at_key = jojobot_domain::session::role_claimed_at_key(role);
        let existing = match self.memory.recall(bot).await {
            Ok(facts) => facts
                .into_iter()
                // **Either key names the claim record**: a release clears the
                // holder and keeps the moment, and the next claim patches that
                // record rather than writing a second one.
                .find(|f| {
                    f.fields.contains_key(&holder_key) || f.fields.contains_key(&claimed_at_key)
                }),
            Err(e) => {
                tracing::warn!(
                    error = %e, %bot, role,
                    "could not read the bot's claims to decide a role claim"
                );
                return serde_json::json!({
                    "role": role,
                    "status": "unavailable",
                    "note": "the memory world could not be read, so this claim was neither \
                             granted nor refused. Nothing was written.",
                    "how_to_proceed": claim_unavailable_way_forward(role),
                });
            }
        };
        let written = match &existing {
            Some(found) => {
                self.memory
                    .update_fact(
                        &found.address(),
                        FactPatch {
                            fields: [
                                (holder_key, claimant.to_string()),
                                (claimed_at_key, now.to_string()),
                            ]
                            .into_iter()
                            .collect(),
                            ..FactPatch::default()
                        },
                        bot,
                    )
                    .await
            }
            None => {
                let mut fact =
                    NewFact::about(bot.clone(), format!("claimed the {role} role"), today);
                fact.fields.insert(holder_key, claimant.to_string());
                fact.fields.insert(claimed_at_key, now.to_string());
                self.memory.capture(fact).await
            }
        };
        match written {
            Ok(Guarded::Written(_)) => serde_json::json!({
                "role": role,
                "status": "taken",
            }),
            Err(MemoryError::RoleTaken { holder, until, .. }) => serde_json::json!({
                "role": role,
                "status": "refused",
                "how_to_proceed": crate::memory::role_taken_way_forward(
                    role,
                    &holder,
                    &until.to_string(),
                ),
                "holder": holder,
                "until": until.to_string(),
            }),
            // **The store's own optimistic concurrency, not a defect.** Two
            // writes landed on the same instant; nothing here was written,
            // and the store answered promptly and correctly. Retrying is the
            // right response to this one specifically, which is why it gets
            // its own status rather than folding into `unavailable` below.
            Err(MemoryError::Conflict) => {
                tracing::info!(%bot, role, "a role claim collided with another write; retry");
                serde_json::json!({
                    "role": role,
                    "status": "conflict",
                    "note": "the claim collided with another write landing the same instant. \
                             Nothing is held. Retry the same claim: it is a transient \
                             collision on a store that is working correctly, not a mistake in \
                             what you sent.",
                })
            }
            other => {
                if let Err(e) = &other {
                    tracing::warn!(error = %e, %bot, role, "a role claim could not be written");
                } else {
                    tracing::warn!(%bot, role, "a role claim's own write was blocked unexpectedly");
                }
                serde_json::json!({
                    "role": role,
                    "status": "unavailable",
                    "note": "the claim was decided but could not be written. Nothing is \
                             held.",
                    "how_to_proceed": claim_unavailable_way_forward(role),
                })
            }
        }
    }
}

/// **What to do about a claim the store could not decide.** Said once for both
/// places it happens, the read that precedes the claim and the write that makes
/// it, so the two cannot come to say different things.
fn claim_unavailable_way_forward(role: &str) -> String {
    format!(
        "Nothing is held. Call start_here again with the same claim for '{role}' in a \
         moment. If it still comes back unavailable, the store cannot decide the claim now: \
         carry on without holding '{role}', and tell the operator, because the store needs a \
         person."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::DeclareTypeArgs;
    use crate::memory::testing::*;
    use crate::session::testing::*;

    /// **A claim that collides with a real store's own optimistic
    /// concurrency tells the caller to retry, not that the role is
    /// `unavailable`.** `MemoryError::Conflict` is a documented, correct
    /// outcome — a write that landed on the same instant as another, on a
    /// store working correctly — and folding it into the generic "could not
    /// be written" branch loses that: a caller reading `unavailable` has no
    /// reason to believe trying again would do anything different.
    #[tokio::test]
    async fn a_claim_that_conflicts_tells_the_caller_to_retry() {
        let jojobot = Jojobot::new(
            Arc::new(ConflictingMemory(Arc::new(InMemoryMemory::booted()))),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            Arc::new(crate::sid::SessionRegistry::new()),
        );
        make_bot(&jojobot, "gamma").await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let claim = &booted["session"]["claim"];
        assert_ne!(
            claim["status"], "unavailable",
            "a Conflict must read as its own outcome, not the generic write failure: {booted}"
        );
        let note = claim["note"]
            .as_str()
            .unwrap_or_else(|| panic!("a Conflict answer must say what to do: {booted}"));
        assert!(
            note.to_lowercase().contains("retry") || note.to_lowercase().contains("try again"),
            "the answer must tell the caller a retry is the right move: {note}"
        );
    }

    /// **A claim the store could not decide comes back `unavailable` and says
    /// what to do next.** Nothing was written and nothing is held, which is
    /// what the outcome says; what a caller does about it is the other half,
    /// and `conflict` and `refused` both already carry theirs.
    #[tokio::test]
    async fn an_unavailable_claim_says_what_to_do_next() {
        let (healthy, blind) = healthy_and_down(Down::Writes);
        make_bot(&healthy, "gamma").await;

        let booted = json_of(
            &blind
                .start_here(Parameters(OrientArgs {
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let claim = &booted["session"]["claim"];
        assert_eq!(
            claim["status"], "unavailable",
            "the store cannot write the claim, so it is undecided: {booted}"
        );
        let how = claim["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("an unavailable claim must say what to do next: {booted}"));
        assert!(
            how.contains("dev-dispatch"),
            "the way forward names the role it is about: {how}"
        );
    }

    /// 🚨 **The bar the correction asked for: the WHOLE answer against the
    /// declared ceiling, not one field against one constant** (rule 106,
    /// decision log 297/298) — a rules-heavy identity. Five rules — every
    /// one this boot's cap can ever carry (rule 306) — each carrying
    /// details real enough that no single one is trivially small next to
    /// the others, so only a genuine rank-and-cut over every one of them —
    /// never a bound on the collection alone — keeps the newest end while
    /// the essay competes for what is left, exactly as a real identity with
    /// a long history of rules would.
    #[tokio::test]
    async fn a_boot_ranked_heavy_in_rules_cuts_the_oldest_details_and_still_fits_one_ceiling() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Small charter.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");

        for n in 0..jojobot_domain::text::CARRIED_RULES {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    details: Some(format!("rule {n} reasoning: {}", "x".repeat(3_000))),
                    // A boot only carries a marked rule (rule 306) — every
                    // one of these has to be marked, or the cap would drop
                    // some before the details-ranking below ever runs.
                    fields: Some(
                        [("starred".to_string(), "true".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("bot:gamma", &format!("rule {n}"))
                },
            )
            .await;
        }

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            booted["identity"]["charter"], "Small charter.",
            "charter is never dropped by this cut: {booted}"
        );
        let rules = booted["identity"]["rules"]
            .as_array()
            .expect("rules is an array");
        assert_eq!(
            rules.len(),
            jojobot_domain::text::CARRIED_RULES,
            "every marked rule is still listed: {rules:?}"
        );

        // Content never drops — it is the short, curated half.
        for (n, rule) in rules.iter().enumerate() {
            assert_eq!(rule["content"], format!("rule {n}"), "{rule}");
        }

        // **The bar itself: the WHOLE serialized answer against the WHOLE
        // declared ceiling** — never a field or a collection measured
        // against a constant of its own (rule 106). This is what the caller
        // actually receives, structural JSON included.
        let whole = booted.to_string().chars().count();
        assert!(
            whole <= jojobot_domain::text::BOOT_ANSWER.budget,
            "the whole answer must fit the one declared ceiling: {whole} chars"
        );

        // The oldest rule is what a tight remainder drops first.
        let oldest = &rules[0];
        assert!(oldest["details"].is_null(), "{oldest}");
        assert_eq!(oldest["details_elided"], true, "{oldest}");
        assert!(
            oldest["details_bytes"].as_u64().expect("a byte count") > 0,
            "{oldest}"
        );
        let note = oldest["details_note"].as_str().expect("a note");
        assert!(
            note.contains("recall") && note.contains("facts"),
            "the way back is named: {note}"
        );

        // The newest rule is the positive the drop above depends on.
        let newest = rules.last().expect("at least one rule");
        assert!(newest["details"].is_string(), "{newest}");
        assert!(newest["details_elided"].is_null(), "{newest}");
    }

    /// 🚨 **`stale_after` is omitted from a rule's rendering when nothing set
    /// it, never spelled out as `null`** (rule 300's first mechanical
    /// instance: every field earns its place). Paired against the positive
    /// on purpose: a rendering that always dropped the key would pass the
    /// negative half alone, so a rule that actually carries it must still
    /// render it whole.
    ///
    /// **`edge`, `derived_from` and `happened_at` are deliberately NOT
    /// touched**, and this asserts they still are not: real user stories
    /// (`unprompted`, `bikes`, `challenge`, `unsourced`) pin the literal
    /// `"edge":null` / `"derived_from":null` / `"happened_at":null` on the
    /// wire, so omitting them would break a reader that already exists.
    /// `fields`, `refs` and `stands_for` carry the same always-present
    /// requirement for the same reason (see
    /// `a_capture_with_no_fields_answers_with_an_empty_bag` and the
    /// `stands_for` ordinary-field case) and are out of scope here too.
    #[tokio::test]
    async fn a_rules_stale_after_is_omitted_when_empty_and_other_null_keys_stay_present() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        ensure(&jojobot, "milhouse").await;

        // A boot only carries a marked rule (rule 306) — this test is about
        // null-key rendering, not about the cap, so both rules are marked.
        let starred = || {
            Some(
                [("starred".to_string(), "true".to_string())]
                    .into_iter()
                    .collect(),
            )
        };
        let empty = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: starred(),
                ..capture_args("bot:gamma", "an ordinary rule")
            },
        )
        .await;
        let empty_address = address_of(&empty);

        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("about".into()),
                object: Some("person:milhouse".into()),
                derived_from: Some(empty_address.clone()),
                happened_at: Some("2026-01-05".into()),
                stale_after: Some("2026-02-01".into()),
                fields: starred(),
                ..capture_args("bot:gamma", "a rule with every optional key set")
            },
        )
        .await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let rules = booted["identity"]["rules"].as_array().expect("rules");

        let empty_rule = rules
            .iter()
            .find(|r| r["content"] == "an ordinary rule")
            .expect("the empty rule is there");
        assert!(
            empty_rule.get("stale_after").is_none(),
            "a rule with no expiry still spelled stale_after out as null: {empty_rule}"
        );
        // The positive control: edge/derived_from/happened_at stay present
        // and null, exactly as the pinned user stories require. Checked on
        // the object's own key set, not on equality — indexing a MISSING key
        // also reads back as `Null`, so an equality check alone cannot tell
        // "present and null" from "absent" apart.
        let empty_rule_object = empty_rule.as_object().expect("a rule is a JSON object");
        for key in ["edge", "derived_from", "happened_at"] {
            assert!(
                empty_rule_object.contains_key(key) && empty_rule[key].is_null(),
                "a story pins `\"{key}\":null` on the wire; this rendering must still say so: \
                 {empty_rule}"
            );
        }

        let filled_rule = rules
            .iter()
            .find(|r| r["content"] == "a rule with every optional key set")
            .expect("the filled rule is there");
        assert_eq!(
            filled_rule["edge"]["object"], "person:milhouse",
            "{filled_rule}"
        );
        assert_eq!(filled_rule["derived_from"], empty_address, "{filled_rule}");
        assert_eq!(filled_rule["happened_at"], "2026-01-05", "{filled_rule}");
        assert_eq!(filled_rule["stale_after"], "2026-02-01", "{filled_rule}");
    }

    /// 🚨 **A boot serves a bot's rules that are IN FORCE** (rule 301 landing
    /// at the boot): a rule whose status is not active is not served at
    /// all — not its statement, not its reasoning, not a marker beside it.
    /// Paired against the positive on purpose: a boot that served no rules
    /// at all would pass the negative half alone.
    ///
    /// **No elision marker for what was left out, deliberately.** A retired
    /// instruction is not withheld information a session might want; naming
    /// a count would invite fetching rules that do not bind this session.
    #[tokio::test]
    async fn a_boot_serves_only_the_rules_that_are_active() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let retiring = capture_ok(&jojobot, capture_args("bot:gamma", "an old instruction")).await;
        let retiring_address = address_of(&retiring);
        jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                details: Some("superseded, kept for history".into()),
                ..update_args(&retiring_address)
            }))
            .await
            .expect("update ok");

        capture_ok(
            &jojobot,
            CaptureArgs {
                // A boot only carries a marked rule (rule 306) — this test is
                // about status filtering, not about the cap.
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("bot:gamma", "the rule that still binds")
            },
        )
        .await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let rules = booted["identity"]["rules"].as_array().expect("rules");

        assert!(
            rules.iter().all(|r| r["address"] != retiring_address),
            "a retired rule is still served: {rules:?}"
        );
        let active = rules
            .iter()
            .find(|r| r["content"] == "the rule that still binds")
            .expect("the rule still in force is served whole");
        assert_eq!(active["status"], "active", "{active}");
    }

    /// 🚨 **A charter big enough, alone, to leave no room for anything else** —
    /// proving `charter` is still served whole regardless of what that costs
    /// the rest of the answer, and that a rule's own `details` is honestly
    /// elided rather than forced through when there is no room left: a
    /// claim's reasoning is what the backpack model leaves at home — reachable
    /// by digging (`recall … facts: true`), never carried at a cost to the
    /// ceiling every other answer holds to.
    ///
    /// **13,000, not a rounder or larger number, because today's core alone is
    /// 10,218 characters** — this charter plus the core plus the fixed cost of
    /// everything else this boot always carries (snapshot, skills, session,
    /// identity metadata) is chosen to clear the 28,000 ceiling with room to
    /// spare, once the rule's reasoning is honestly elided rather than forced
    /// through. **This is not the same claim as "any charter this size fits"**:
    /// a real identity's own charter and rules can still exceed the ceiling
    /// once the core competes with them too — measured against a real 25-rule
    /// charter at 35,508 characters against this same 28,000 budget — and that
    /// is an open question this test does not answer and does not hide.
    ///
    /// **The core is never null, whatever the charter costs** — a named
    /// boot's orientation is the core by rule, not by a ranking this charter
    /// could have won or lost, so a heavy charter proves nothing different
    /// about the essay than a light one does. `a_light_named_boot_still_gets_only_the_core`
    /// is the same claim, cheaper to construct; this one exists for the
    /// charter and rule-ranking behaviour beside it.
    #[tokio::test]
    async fn a_boot_ranked_heavy_in_charter_still_serves_charter_and_core_whole() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "x".repeat(13_000),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");
        capture_ok(
            &jojobot,
            CaptureArgs {
                details: Some(format!("rule 0 reasoning: {}", "x".repeat(2_000))),
                // A boot only carries a marked rule (rule 306) — this test is
                // about charter/details ranking against the ceiling, not
                // about the cap.
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("bot:gamma", "rule 0")
            },
        )
        .await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );

        // **Charter ranks first and is always served whole**, whatever it
        // costs — the positive this whole mechanism rests on.
        let charter = booted["identity"]["charter"]
            .as_str()
            .expect("charter is never dropped by this cut");
        assert_eq!(charter.chars().count(), 13_000, "{charter:?}");

        // **The rule's own reasoning has no room left beside a 13,000-character
        // charter and the essay's own core, and it is left at home rather
        // than forced through: elided, marked, and pointed at the dig that
        // reaches it whole.**
        let rules = booted["identity"]["rules"].as_array().expect("rules");
        assert!(rules[0]["details"].is_null(), "{rules:?}");
        assert_eq!(rules[0]["details_elided"], true, "{rules:?}");
        assert_eq!(
            rules[0]["details_bytes"],
            "rule 0 reasoning: ".len() + 2_000,
            "{rules:?}"
        );
        assert!(
            rules[0]["details_note"]
                .as_str()
                .is_some_and(|n| n.contains("recall") && n.contains("bot:gamma")),
            "{rules:?}"
        );

        // **The core is never null, and it is the whole of what a named boot
        // gets — not a ranking outcome this charter's weight could tip.**
        let orientation = booted["orientation"]
            .as_str()
            .expect("the core always ships when brief is false: {booted}");
        assert_eq!(
            orientation,
            crate::orientation::essay::ORIENTATION_CORE,
            "a named boot must carry exactly the core and nothing of the remainder, \
             heavy charter or not: {orientation:?}",
        );
        // **The remainder is genuinely gone, not merely truncated** — content
        // that only the remainder carries must not appear.
        assert!(
            !orientation.contains("CLEAR AND RESUME"),
            "the remainder leaked into what was supposed to be core-only: {orientation:?}",
        );
        assert_eq!(booted["orientation_elided"], true, "{booted}");
        let note = booted["orientation_note"]
            .as_str()
            .expect("a named boot's core-only answer names why: {booted}");
        assert!(
            note.contains("core"),
            "the reason names what a named boot actually gets, not a ceiling this charter's \
             weight never touched: {note}"
        );

        // **The bar itself: the WHOLE serialized answer against the WHOLE
        // declared ceiling.**
        let whole = booted.to_string().chars().count();
        assert!(
            whole <= jojobot_domain::text::BOOT_ANSWER.budget,
            "the whole answer must fit the one declared ceiling: {whole} chars"
        );
    }

    /// 🚨 **What the code does today when the FLOOR ALONE — charter plus the
    /// essay's core plus the fixed cost of everything else a named boot always
    /// carries, with every rankable thing already elided — will not fit the
    /// declared ceiling.** Nothing here endorses this as correct: it is a
    /// pin on OBSERVED behaviour, established by running the call and reading
    /// what it did rather than by assuming.
    ///
    /// **It answers `Ok`, whole, and over the ceiling — not a decline, not a
    /// panic, not a truncation.** `remaining_for_prose` saturates to zero and
    /// every rankable candidate (a rule's own reasoning, an anonymous boot's
    /// essay) is elided as far as that can go, but the floor itself — charter
    /// and core, both unconditional by existing rule — still ships whole,
    /// and nothing downstream compares the final serialized size against
    /// [`text::BOOT_ANSWER`] at all. A client that cannot read a payload this
    /// large sees a boot that silently exceeded the one number the product
    /// declares it holds to.
    ///
    /// **This is not hypothetical.** A real identity's own charter and rules
    /// measure 33,992 characters against this same 28,000 budget — the
    /// operator's own bot is in the condition this case pins today. Whether
    /// that should keep shipping oversized, decline, or something else is a
    /// design question this case does not answer — see the backpack model's
    /// cap, in flight on another line. This case exists so that question is
    /// answered on purpose rather than discovered by a client failing to
    /// render an answer.
    #[tokio::test]
    async fn a_floor_that_alone_exceeds_the_ceiling_ships_oversized_rather_than_declining() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        // Large enough that charter plus today's ~10,218-character core plus
        // the fixed cost of everything else already exceeds the ceiling with
        // no rule and no reasoning competing for room at all — the floor
        // alone is what overflows here, not a ranking outcome.
        // **Written to the store, not through `set_charter`**, which refuses a
        // charter that takes the boot over the ceiling: this is the state a
        // boot meets when a write the gate never saw has already left it there.
        jojobot
            .memory
            .set_prose(&EntityId("bot:gamma".into()), &"x".repeat(20_000))
            .await
            .expect("the store keeps the prose");

        let result = jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: None,
                bot: Some("gamma".into()),
                brief: Some(false),
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
            }))
            .await;

        let booted = json_of(&result.expect("a floor that cannot fit still answers Ok today"));
        assert!(
            booted.get("status").is_none(),
            "today's answer is an ordinary boot, not a decline: {booted}"
        );
        let charter = booted["identity"]["charter"]
            .as_str()
            .expect("the charter is still served whole even though nothing else fits beside it");
        assert_eq!(charter.chars().count(), 20_000, "{charter:?}");

        let whole = booted.to_string().chars().count();
        assert!(
            whole > jojobot_domain::text::BOOT_ANSWER.budget,
            "this case exists because the floor alone does NOT fit today — a whole of {whole} \
             at or under the {}-character budget means the condition this pins no longer \
             reproduces, and the fix that closed it belongs in this case's own doc comment, not \
             a silent pass: {booted}",
            jojobot_domain::text::BOOT_ANSWER.budget,
        );
    }

    /// 🚨 **The positive half of the core/remainder bar, re-pointed at the
    /// boot that actually has this behaviour.**
    ///
    /// **An anonymous boot ships the whole essay when the whole answer fits
    /// the ceiling** — core and remainder joined, byte for byte. The ceiling
    /// decides: a store whose snapshot leaves no room for every unit ships the
    /// units that fit and names the rest (see
    /// `an_anonymous_boots_whole_answer_is_under_the_ceiling_or_has_dropped_everything_it_can`).
    ///
    /// A named boot never gets the whole essay — the remainder is not a
    /// ranking candidate for one, it is a rule (`essay_for_boot`'s own doc).
    /// **An anonymous boot is where "whole" is the load-bearing behaviour**: no
    /// identity to pay for, so it is what a caller who asked to be taught the
    /// surface gets whenever the answer fits. This store is the bare double,
    /// whose snapshot is small; a real instance's is larger, and the case
    /// above grows the snapshot to find where the essay stops fitting.
    #[tokio::test]
    async fn an_anonymous_boot_that_fits_the_ceiling_ships_the_essay_whole() {
        let jojobot = handler();
        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );

        let orientation = booted["orientation"]
            .as_str()
            .expect("an anonymous boot ships the essay: {booted}");
        assert_eq!(
            orientation,
            crate::orientation::essay::orientation(),
            "an anonymous boot that fits must ship the essay whole, core and remainder joined",
        );
        assert_eq!(booted["orientation_elided"], false, "{booted}");
        assert!(
            booted["orientation_note"].is_null(),
            "nothing was cut, so there is nothing to explain: {booted}"
        );
        // **The answer that shipped the whole essay is within the ceiling**,
        // which is what makes "whole" the true answer and not an overshoot.
        let whole = booted.to_string().chars().count();
        assert!(
            whole <= jojobot_domain::text::BOOT_ANSWER.budget,
            "the whole essay shipped in {whole} characters, over the ceiling"
        );
    }

    /// **A fresh instance's anonymous boot ships the whole essay inside the
    /// ceiling.** The boot trims itself to the ceiling and says what it cut, so
    /// an essay that grew past it would still answer and nothing would fail:
    /// the next boot would carry less and say so. This holds the whole answer,
    /// uncut, to the ceiling on an instance with nothing in it but what the
    /// software ships, which is the case a first reader meets.
    #[tokio::test]
    async fn a_fresh_instances_anonymous_boot_ships_the_whole_essay_inside_the_ceiling() {
        let jojobot = handler();
        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let whole = booted.to_string().chars().count();
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        assert_eq!(
            booted["orientation_elided"], false,
            "the essay no longer fits whole: the answer is {whole} of {budget} characters"
        );
        assert!(whole <= budget, "{whole} characters against {budget}");
    }

    /// **Whatever the store's size, the serialized whole answer is at or under
    /// the ceiling, or nothing that could drop is left** (rule 306). The
    /// ceiling bounds what a caller RECEIVES, so a unit of the essay is sized
    /// by what it costs once serialized: a newline or a quote is two
    /// characters there and one in the text.
    ///
    /// A snapshot grows with the store, so the case grows it a bot at a time
    /// and reads every boot along the way, including the ones near the edge
    /// where the text fits and its serialization does not. **Both halves must
    /// occur**, or a ceiling that shipped everything, or dropped everything,
    /// would pass.
    #[tokio::test]
    async fn an_anonymous_boots_whole_answer_is_under_the_ceiling_or_has_dropped_everything_it_can()
    {
        let jojobot = handler();
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;
        let core = crate::orientation::essay::ORIENTATION_CORE;
        let (mut shipped_some, mut dropped_some) = (false, false);
        for n in 0..24 {
            // **Three of one letter each**, so no slug contains another and the
            // creation screen takes every one as its own bot.
            let letter = (b'a' + n as u8) as char;
            make_bot(&jojobot, &format!("{letter}{letter}{letter}")).await;
            let booted = json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        claim: None,
                        timezone: None,
                        bot: None,
                        brief: Some(false),
                        skill: None,
                        section: None,
                        resume: None,
                        sid: None,
                        today: None,
                    }))
                    .await
                    .expect("start_here ok"),
            );
            let whole = booted.to_string().chars().count();
            let orientation = booted["orientation"].as_str().expect("the essay");
            assert!(orientation.starts_with(core), "the core always ships");
            let shipped_remainder = orientation.len() > core.len();
            shipped_some |= shipped_remainder;
            dropped_some |= booted["orientation_elided"] == true;
            if booted["orientation_elided"] == true {
                assert!(
                    booted["orientation_note"].is_string(),
                    "what was left out is named: {booted}"
                );
            }
            assert!(
                whole <= budget || !shipped_remainder,
                "with {n} more bots the whole answer is {whole} characters, over the {budget} \
                 ceiling, and the essay still ships part of its remainder"
            );
        }
        assert!(shipped_some, "no boot shipped any of the remainder");
        assert!(
            dropped_some,
            "no boot dropped anything, so the edge was never crossed"
        );
    }

    /// **A rule's reasoning is sized by what it costs serialized too.** Four
    /// rules whose `details` are full of quotes fit the ceiling
    /// as text and would take the answer past it as the caller receives them.
    /// The newest reasoning ships and the older is left at home, marked.
    ///
    /// **Both halves**, or a boot that dropped every reasoning would pass.
    #[tokio::test]
    async fn rule_reasoning_full_of_quotes_is_ranked_by_what_it_costs_serialized() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let heavy = "\"x\" ".repeat(750);
        for n in 0..4 {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    details: Some(heavy.clone()),
                    fields: Some(
                        [("starred".to_string(), "true".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("bot:gamma", &format!("rule number {n}"))
                },
            )
            .await;
        }
        let booted = boot(&jojobot, "gamma").await;
        let rules = booted["identity"]["rules"].as_array().expect("rules");
        assert_eq!(rules.len(), 4, "{rules:?}");
        let kept = rules.iter().filter(|r| r["details"].is_string()).count();
        let elided = rules.iter().filter(|r| r["details_elided"] == true).count();
        assert!(kept >= 1, "the newest reasoning ships: {rules:?}");
        assert!(
            elided >= 1,
            "the older reasoning is left at home: {rules:?}"
        );
        let whole = booted.to_string().chars().count();
        assert!(
            whole <= jojobot_domain::text::BOOT_ANSWER.budget,
            "the whole answer is {whole} characters, over the ceiling"
        );
    }

    /// **The negative this pairs with: a named boot never gets the
    /// remainder, whatever room the ceiling has.** A "Small charter." bot
    /// with no rules is about as light as a named identity gets — if the
    /// remainder were still a candidate for anyone, it would fit here — and
    /// it still gets the core alone, because the rule is that a named boot
    /// gets the core, not that it sometimes wins a ranking.
    #[tokio::test]
    async fn a_light_named_boot_still_gets_only_the_core() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Small charter.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );

        let orientation = booted["orientation"]
            .as_str()
            .expect("a named boot still gets the core: {booted}");
        assert_eq!(
            orientation,
            essay::ORIENTATION_CORE,
            "a named boot must never carry the remainder, light identity or not",
        );
        assert_eq!(booted["orientation_elided"], true, "{booted}");
        let note = booted["orientation_note"]
            .as_str()
            .expect("a named boot's core-only answer names why: {booted}");
        assert!(
            note.contains("core"),
            "the reason names what a named boot actually gets: {note}"
        );
    }

    /// The two worlds are apart, and this is the test that says so. Ownership
    /// is stated on the box, so an unreadable entity index takes the
    /// charter, the rules and the roster with it, and leaves the mail
    /// scoping standing. If this assertion's polarity ever flips back, that
    /// means ownership is being read off the entity record again — a
    /// regression.
    /// **A boot that could not read the vocabulary says so rather than naming
    /// none.**
    ///
    /// *There are no kinds* and *jojobot could not look* are different claims,
    /// and a caller acts on both: the first says invent your own vocabulary,
    /// and the second says come back. **Both halves**, because a marker that
    /// was always false would satisfy the first assertion on its own.
    #[tokio::test]
    async fn a_vocabulary_that_cannot_be_read_says_so_rather_than_shipping_none() {
        let memory = Arc::new(InMemoryMemory::booted());
        crate::seed::ensure_kinds(&(memory.clone() as Arc<dyn Memory>))
            .await
            .expect("the build's kinds are written");
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let reading = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        make_box(&reading, "dev").await;
        let read = boot(&reading, "dev").await;
        assert_eq!(
            read["snapshot"]["vocabulary"]["available"], true,
            "a store that answers did not read as available: {read}"
        );
        assert!(
            read["snapshot"]["vocabulary"]["kinds"]
                .as_array()
                .is_some_and(|kinds| !kinds.is_empty()),
            "a store that answers named no kinds, so the case below proves nothing: {read}"
        );

        let blind = Jojobot::new(
            Arc::new(DownMemory(Down::Vocabulary, memory)),
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        let unread = boot(&blind, "dev").await;
        assert_eq!(
            unread["snapshot"]["vocabulary"]["available"], false,
            "a boot that could not read the vocabulary claimed it could: {unread}"
        );
        assert!(
            unread["snapshot"]["vocabulary"]["kinds"]
                .as_array()
                .is_some_and(|kinds| kinds.is_empty()),
            "an unreadable vocabulary named kinds it never read: {unread}"
        );
        // **That a note is THERE, never what it says.** See the roster case for
        // why the wording stays unpinned.
        assert!(
            unread["snapshot"]["vocabulary"]["note"]
                .as_str()
                .is_some_and(|note| !note.trim().is_empty()),
            "an unreadable vocabulary came back as a bare marker, so nothing tells a caller \
             this is not a build that ships none: {unread}"
        );
    }

    /// **A declared type is named beside the kinds, at a cold boot.** Before
    /// this, a declared type was invisible from where a cold agent stands:
    /// the boot named every shipped kind and not one declared type, so a
    /// session with no memory of another session's own catalogue fetch had
    /// no proactive path back to a name like `trip` — only a reactive one,
    /// guessing a type name that does not exist and reading the refusal's
    /// own candidate list.
    #[tokio::test]
    async fn a_cold_boot_names_every_declared_type_beside_the_kinds() {
        let memory = Arc::new(InMemoryMemory::booted());
        crate::seed::ensure_kinds(&(memory.clone() as Arc<dyn Memory>))
            .await
            .expect("the build's kinds are written");
        crate::seed::ensure_shipped_types(&(memory.clone() as Arc<dyn Memory>))
            .await
            .expect("the build's shipped types are written");
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        make_box(&jojobot, "dev").await;
        let read = boot(&jojobot, "dev").await;
        assert_eq!(
            read["snapshot"]["vocabulary"]["types_available"], true,
            "a store that answers did not read its declared types as available: {read}"
        );
        let types = read["snapshot"]["vocabulary"]["types"]
            .as_array()
            .expect("a types array");
        assert!(
            types.iter().any(|t| t["type"] == "trip"),
            "the shipped trip type is not named among the boot's declared types: {read}"
        );
    }

    /// **The two vocabularies are read independently, and a sabotage of one
    /// must not redden the other.** `Down::Vocabulary` fails only
    /// `declared_kinds`; `Down::TypeRoster` fails only `declared_types` — a
    /// shared flag would have one outage silently pass for the other.
    #[tokio::test]
    async fn a_type_roster_outage_does_not_read_as_a_kind_vocabulary_outage_or_the_reverse() {
        let memory = Arc::new(InMemoryMemory::booted());
        crate::seed::ensure_kinds(&(memory.clone() as Arc<dyn Memory>))
            .await
            .expect("the build's kinds and types are written");
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());

        let types_down = Jojobot::new(
            Arc::new(DownMemory(Down::TypeRoster, memory.clone())),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        make_box(&types_down, "dev").await;
        let read = boot(&types_down, "dev").await;
        assert_eq!(
            read["snapshot"]["vocabulary"]["types_available"], false,
            "a down type roster did not read as unavailable: {read}"
        );
        assert_eq!(
            read["snapshot"]["vocabulary"]["available"], true,
            "a down type roster took the kind vocabulary down with it: {read}"
        );

        let kinds_down = Jojobot::new(
            Arc::new(DownMemory(Down::Vocabulary, memory)),
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        let read = boot(&kinds_down, "dev").await;
        assert_eq!(
            read["snapshot"]["vocabulary"]["available"], false,
            "a down kind vocabulary did not read as unavailable: {read}"
        );
        assert_eq!(
            read["snapshot"]["vocabulary"]["types_available"], true,
            "a down kind vocabulary took the declared types down with it: {read}"
        );
    }

    #[tokio::test]
    async fn an_unreadable_entity_index_no_longer_hides_who_drains_what() {
        let memory = Arc::new(InMemoryMemory::booted());
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let seeded = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        make_box(&seeded, "dev").await;
        send(&seeded, "dev", "delta", "your hand-off").await;

        let blind = Jojobot::new(
            Arc::new(DownMemory(Down::EntityIndex, memory)),
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        // **Read through the identity, which is where a caller's own mail now
        // lives.** The snapshot hangs mail off the bots, and the bots come from
        // Memory — so when Memory cannot be read there is no roster to hang
        // anything on. The caller's own box does not depend on that: it is the
        // mail world's own answer, and this is the half that must not go quiet.
        let booted = boot(&blind, "dev").await;
        // **Mail answers for itself when there is no roster to hang it on.**
        // The bots come from Memory, so folding mail onto them would let an
        // unreadable entity index take the mail board down with it — and whose
        // box is whose is the mail world's own fact.
        let mine = booted["snapshot"]["mail"]["by_owner"]
            .as_array()
            .unwrap_or_else(|| panic!("mail answers by owner here: {booted}"))
            .iter()
            .find(|b| b["yours"] == true)
            .unwrap_or_else(|| panic!("the caller's own box is named: {booted}"))
            .clone();

        assert_eq!(
            mine["mail"]["counts"]["new"], 1,
            "the mail world knows whose box this is without asking Memory: {booted}"
        );
        // The positive it rests on: Memory really is unreadable in this boot,
        // so the counts above cannot have come from a roster.
        assert_eq!(
            booted["snapshot"]["entities"]["available"], false,
            "the entity index must be down, or this proves nothing: {booted}"
        );
        // **Mail reports itself AVAILABLE while the roster is down**, because
        // an unreadable roster costs the roster and nothing else. Paired with
        // the shape that says this branch was really taken: without it, a build
        // where the degraded path never runs satisfies the marker by never
        // reaching it.
        assert_eq!(
            booted["snapshot"]["mail"]["available"], true,
            "mail reads as down when only the roster is: {booted}"
        );
        assert!(
            booted["snapshot"]["mail"]["by_owner"].is_array(),
            "the degraded shape was never reached, so the marker above says nothing: {booted}"
        );
        // **That a note is THERE, never what it says.** A marker alone tells a
        // caller something is down and nothing about whether to retry, wait or
        // go elsewhere. The wording is deliberately unpinned: an assertion
        // quoting a sentence breaks when the sentence improves and proves
        // nothing about behaviour.
        assert!(
            booted["snapshot"]["entities"]["note"]
                .as_str()
                .is_some_and(|note| !note.trim().is_empty()),
            "an unreadable roster came back as a bare marker with nothing a caller can act \
             on: {booted}"
        );
        // **The note beside it is prose somebody reads, and what it SAYS is not
        // asserted.** That this answer is in the degraded mode is the marker's
        // claim, and that an explanation came with it is the presence check
        // above; a third assertion quoting the sentence would break the day
        // somebody improves it and prove nothing about behaviour either way.
        //
        // What is asserted is not the wording: a run of spaces is source
        // indentation that escaped a wrapped literal — the file's own layout
        // arriving in a sentence, which is a defect in any wording.
        let note = booted["snapshot"]["mail"]["note"]
            .as_str()
            .expect("the degraded shape says why it is shaped that way");
        assert!(
            !note.contains("  "),
            "…as one run of prose, with none of the source's indentation in it: {note:?}"
        );
    }

    /// One bot's entry out of a boot's snapshot, by bare name.
    fn bot_entry(booted: &serde_json::Value, name: &str) -> serde_json::Value {
        booted["snapshot"]["entities"]["bots"]
            .as_array()
            .expect("the bots")
            .iter()
            .find(|b| b["handle"] == format!("bot:{name}"))
            .unwrap_or_else(|| panic!("{name} is on the board: {booted}"))
            .clone()
    }

    /// **Another bot's mail is not this boot's to weigh, quarantine included.**
    /// An agent reading the snapshot cannot act differently for knowing a
    /// stranger box's fault exists — that is what a sender's own
    /// `list_sent` is for — so neither the counts nor the quarantine report
    /// on somebody else's box rides in a boot that is not theirs. The
    /// per-bot elision marker (`counts_elided`) is gone too: the snapshot
    /// says once, in `mail.note`, that another bot's counts are never
    /// shown here, rather than repeating a flag on every bot in the list.
    #[tokio::test]
    async fn a_boot_shows_nothing_at_all_about_a_box_that_is_not_its_own() {
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(boxes.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        boxes.quarantine_by_damage(
            &MailboxName("delta".into()),
            &MessageId("4212".into()),
            "its row cannot be read — a state or a sender has been edited past parsing",
        );

        let booted = boot(&jojobot, "gamma").await;
        let theirs = bot_entry(&booted, "delta");
        assert_eq!(theirs["yours"], false);
        assert!(
            theirs["mail"].get("counts").is_none(),
            "somebody else's queue is not this boot's to render: {theirs}"
        );
        assert!(
            theirs["mail"].get("counts_elided").is_none(),
            "the per-bot elision flag must be gone — the snapshot says it once, not per bot: \
             {theirs}"
        );
        assert!(
            theirs["mail"].get("quarantined").is_none(),
            "a stranger box's fault is not this boot's to report either: {theirs}"
        );
        assert!(
            booted["snapshot"]["mail"]["note"]
                .as_str()
                .is_some_and(|note| !note.trim().is_empty()),
            "the snapshot must say once that another bot's mail counts are not shown here: \
             {booted}"
        );
    }

    /// **The caller's own quarantine is the pair's other half — present when
    /// there is something to act on, and absent otherwise.** A zero count is
    /// not a fact worth a field: an agent that just reads `quarantined.count`
    /// unconditionally would treat an absent report as zero already, so
    /// nothing is lost by leaving the key out when there is nothing to say.
    #[tokio::test]
    async fn a_boots_own_quarantine_is_present_only_when_it_is_not_zero() {
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(boxes.clone());
        make_bot(&jojobot, "gamma").await;

        let clean = boot(&jojobot, "gamma").await;
        let mine = bot_entry(&clean, "gamma");
        assert!(
            mine["mail"].get("quarantined").is_none(),
            "a clean box must not carry an empty quarantine report: {mine}"
        );

        boxes.quarantine_by_damage(
            &MailboxName("gamma".into()),
            &MessageId("4212".into()),
            "its row cannot be read — a state or a sender has been edited past parsing",
        );
        let dirty = boot(&jojobot, "gamma").await;
        let mine = bot_entry(&dirty, "gamma");
        assert_eq!(mine["mail"]["quarantined"]["count"], 1, "{mine}");
        assert_eq!(mine["mail"]["quarantined"]["ids"][0], "4212");
    }

    /// **A kind's or a type's origin is not something a boot's own agent acts
    /// on** — it matters when declaring or reading one directly, which is
    /// where it stays. The pair: gone from the boot's vocabulary listing,
    /// still there wherever a kind or type is declared or read on its own.
    #[tokio::test]
    async fn the_boots_vocabulary_names_no_origin_though_declaring_one_still_does() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let field = || crate::memory::FieldArgs {
            key: "cost".into(),
            holds: None,
            folds: None,
            required: false,
            one_of: None,
        };
        jojobot
            .declare_type(Parameters(DeclareTypeArgs {
                name: "own-vocab-test".into(),
                fields: vec![field()],
                sid: Some(writing_as(&jojobot)),
            }))
            .await
            .expect("declare_type call ok");

        let booted = boot(&jojobot, "gamma").await;
        let vocabulary = &booted["snapshot"]["vocabulary"];
        for kind in vocabulary["kinds"].as_array().expect("kinds") {
            assert!(
                kind.get("origin").is_none(),
                "a kind must not carry its origin in the boot's own vocabulary: {kind}"
            );
        }
        for declared_type in vocabulary["types"].as_array().expect("types") {
            assert!(
                declared_type.get("origin").is_none(),
                "a type must not carry its origin in the boot's own vocabulary: {declared_type}"
            );
        }

        let declared = json_of(
            &jojobot
                .declare_type(Parameters(DeclareTypeArgs {
                    name: "own-vocab-test".into(),
                    fields: vec![field()],
                    sid: Some(writing_as(&jojobot)),
                }))
                .await
                .expect("declare_type call ok"),
        );
        assert_eq!(
            declared["type"]["origin"], "declared",
            "origin still rides declare_type's own answer: {declared}"
        );
    }

    /// A boot sees its own box's counts in the snapshot, and names only for the
    /// rest — the same rule, in the other place a session meets this listing.
    #[tokio::test]
    async fn a_boot_snapshot_counts_only_the_bot_s_own_box() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        send(&jojobot, "gamma", "delta", "your hand-off").await;
        send(&jojobot, "delta", "sigma", "not your business").await;

        let booted = boot(&jojobot, "gamma").await;
        let find = |name: &str| bot_entry(&booted, name);

        assert_eq!(
            find("gamma")["mail"]["counts"]["new"],
            1,
            "my mail, counted: {booted}"
        );
        assert_eq!(find("gamma")["yours"], true);
        assert!(
            find("delta")["mail"]["counts"].is_null(),
            "somebody else's queue is not mine to weigh: {booted}"
        );
        assert_eq!(find("delta")["yours"], false);
        // No `ownership_known` flag: it could only ever say `true` where it
        // appeared, since it would be rendered inside the `Ok` arm of the
        // very read whose `Err` arm is the only thing that would set it
        // false. A field that cannot vary is a question a reader branches on
        // and learns nothing from.
        assert!(
            find("gamma")["mail"].get("ownership_known").is_none(),
            "a flag that cannot be false is not an answer: {booted}"
        );

        // The bot's own box still comes back in full under `identity`, which is
        // the whole point of booting as somebody.
        assert_eq!(booted["identity"]["owned_mailbox"]["counts"]["new"], 1);
    }

    /// **A bot holding two boxes is damage on the board too, not a bot with a
    /// tidy inbox.** Rendering the first match's counts would tell a session
    /// its mail is weighed and whole while a second box sits beside it,
    /// unnamed and uncounted — the same half-answer a read of one of them
    /// would be.
    #[tokio::test]
    async fn a_boot_reports_a_bot_holding_two_boxes_as_damage() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        send(&jojobot, "gamma", "delta", "your hand-off").await;
        a_second_box(&jojobot, "gamma", "sigma").await;

        let booted = boot(&jojobot, "gamma").await;
        let mine = bot_entry(&booted, "gamma");
        assert_eq!(
            mine["yours"], true,
            "this is the caller's own entry, or the rest proves nothing: {booted}"
        );
        assert!(
            mine["mail"]["counts"].is_null(),
            "a count over one of two boxes is a number nobody can use: {mine}"
        );
        let damage = mine["mail"]["damage"]
            .as_str()
            .unwrap_or_else(|| panic!("the entry says what is wrong with it: {mine}"));
        assert!(
            damage.contains("gamma") && damage.contains("sigma"),
            "…and names both boxes, or nobody can go and look: {damage}"
        );
    }

    /// **The two halves of one boot say the same thing about a bot's own
    /// box.** `identity.owned_mailbox` is the field a session is pointed at,
    /// and the snapshot beside it is the field it can cross-check against — so
    /// a payload that weighs one of two boxes in the first and refuses to weigh
    /// them in the second hands a session half its mail as all of it, as
    /// settled truth, next to a sentence saying that cannot be done. Neither
    /// half is checkable on its own; this reads both out of one answer.
    #[tokio::test]
    async fn both_halves_of_a_boot_refuse_to_weigh_a_bot_holding_two_boxes() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        // Mail first: a bot already holding two boxes cannot be posted to, so
        // the fixture's mail has to land while it holds one.
        send(&jojobot, "gamma", "delta", "your hand-off").await;
        a_second_box(&jojobot, "gamma", "sigma").await;

        let booted = boot(&jojobot, "gamma").await;
        let identity = booted["identity"]["owned_mailbox"].clone();
        let snapshot = bot_entry(&booted, "gamma")["mail"].clone();

        // **The positives both refusals rest on.** Both halves are in this one
        // payload and both are about a readable world — an answer missing
        // either half would satisfy every "is null" below without refusing
        // anything.
        assert_eq!(
            identity["available"], true,
            "the mailbox world answered, so this is a ruling and not an outage: {booted}"
        );
        let refused = snapshot["damage"]
            .as_str()
            .unwrap_or_else(|| panic!("the snapshot half calls it damage: {booted}"));
        let owned = identity["damage"]
            .as_str()
            .unwrap_or_else(|| panic!("the identity half calls it damage too: {booted}"));
        assert_eq!(
            owned, refused,
            "one payload, one account of what is wrong: {booted}"
        );
        assert!(
            owned.contains("gamma") && owned.contains("sigma"),
            "…naming both boxes, or nobody can go and look: {owned}"
        );
        assert!(
            owned.contains("person"),
            "…and saying it takes a person, because no verb of the caller's repairs it: {owned}"
        );

        // Neither half weighs a box, and neither picks one as the answer.
        assert!(
            identity["counts"].is_null() && snapshot["counts"].is_null(),
            "a count over one of two boxes is a number nobody can use: {booted}"
        );
        assert!(
            identity["name"].is_null(),
            "naming one of the two is the same choice as counting it: {identity}"
        );

        // …and there is mail in one of those boxes, still waiting — so this
        // refused an answer it could have given, rather than passing because
        // the board is empty.
        let held: Vec<_> = jojobot
            .mailboxes
            .list_mailboxes()
            .await
            .expect("list ok")
            .into_iter()
            .filter(|b| b.owner == EntityId("bot:gamma".into()))
            .collect();
        assert_eq!(held.len(), 2, "the fixture holds two boxes for one bot");
        assert_eq!(
            held.iter().map(|b| b.counts.new).sum::<usize>(),
            1,
            "and the message is there to be counted: {held:?}"
        );
    }

    /// **The door's own next step was unreachable from the door.**
    ///
    /// An anonymous boot counted the bots and named none of them, so a caller
    /// with no identity — which is exactly the caller this answer is for — could
    /// see that five identities exist and could not pick one. Meanwhile the
    /// refusal for an unknown bot lists every real one. So the only way to learn
    /// a name you could boot with was to guess a name that does not exist and
    /// read it off the complaint, and a context-free runner did precisely that.
    ///
    /// The surface was inconsistent with itself rather than minimal. Naming them
    /// here adds no tool and no second call to the first call anybody makes.
    ///
    /// **Names only.** Not counts, not charters: an anonymous caller owns
    /// nothing and is weighing nothing, and the same spelling the refusal uses
    /// (`bots`, full handles) so one record has one shape across the surface.
    #[tokio::test]
    async fn the_snapshot_names_the_bots_a_caller_could_boot_as() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        ensure(&jojobot, "alpha").await;

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let entities = &anonymous["snapshot"]["entities"];
        let handles: Vec<&str> = entities["bots"]
            .as_array()
            .expect("a roster")
            .iter()
            .map(|b| b["handle"].as_str().expect("a handle"))
            .collect();
        assert_eq!(
            handles,
            ["bot:delta", "bot:gamma"],
            "the door has to name what it counts: {anonymous}"
        );
        // A person is not an identity you can boot as, so the roster is not
        // simply the index with a different key on it.
        assert_eq!(entities["by_kind"]["person"], 1, "{anonymous}");

        // **Named, never weighed.** The counts belong to whoever drains the
        // box; an anonymous caller drains none and is choosing an identity, not
        // grading one.
        let listed = entities["bots"].as_array().expect("a roster");
        assert!(
            listed.iter().all(|b| b["mail"]["counts"].is_null()),
            "no queue is weighed for a caller that drains none: {anonymous}"
        );
        assert!(
            listed.iter().all(|b| b.get("charter").is_none()),
            "and no charter rides out with the roster: {anonymous}"
        );

        // …and the name it hands over actually boots, which is the whole claim.
        let booted = boot(&jojobot, "delta").await;
        assert_eq!(booted["identity"]["bot"]["id"], "bot:delta", "{booted}");
    }

    /// 🚨 **The snapshot is a BROWSE and excludes an archived bot; booting AS
    /// it is the DIRECT door and still works, carrying why and when** (rule
    /// 301: the same broad-door/direct-door split a claim's archived state
    /// already has). Paired against the positive on purpose: the negative
    /// alone would pass on a snapshot that named nobody, and the positive
    /// alone would pass on a snapshot that never filtered anything.
    #[tokio::test]
    async fn an_archived_bot_is_absent_from_the_snapshot_and_still_boots_direct() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

        jojobot
            .memory
            .archive_entity(
                &EntityId("bot:delta".into()),
                "retired, superseded by gamma",
            )
            .await
            .expect("archive ok");

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let bots = anonymous["snapshot"]["entities"]["bots"]
            .as_array()
            .expect("a roster");
        assert!(
            bots.iter().all(|b| b["handle"] != "bot:delta"),
            "an archived bot is still offered to boot as: {bots:?}"
        );
        assert!(
            bots.iter().any(|b| b["handle"] == "bot:gamma"),
            "the positive this depends on — a live bot is still listed: {bots:?}"
        );
        assert_eq!(
            anonymous["snapshot"]["entities"]["by_kind"]["bot"], 1,
            "the count excludes the archived one too: {anonymous}"
        );

        // The direct door: booting AS the archived bot still works, and says
        // why and when.
        let booted = boot(&jojobot, "delta").await;
        assert_eq!(booted["identity"]["bot"]["id"], "bot:delta", "{booted}");
        assert_eq!(
            booted["identity"]["bot"]["archived"]["reason"], "retired, superseded by gamma",
            "{booted}"
        );
        assert!(
            booted["identity"]["bot"]["archived"]["at"].is_string(),
            "{booted}"
        );
    }

    /// 🚨 **A boot carries at most `CARRIED_RULES` of a bot's own records —
    /// the ones the writer marked, never more, never chosen for it** (rule
    /// 306, the backpack's other axis). Paired against the positive on
    /// purpose: a boot that dropped every rule would pass the negative half
    /// alone, so a marked rule must still survive whole.
    ///
    /// **Not truncate-then-hunt**: the answer itself says how many of a
    /// bot's rules in force are carried and names the way to read the rest,
    /// because some of what stayed home may still bind this bot.
    #[tokio::test]
    async fn a_boot_carries_only_marked_rules_and_says_what_it_left_home() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("bot:gamma", "a rule the writer chose to carry")
            },
        )
        .await;

        capture_ok(
            &jojobot,
            capture_args("bot:gamma", "an ordinary rule nobody marked"),
        )
        .await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let rules = booted["identity"]["rules"].as_array().expect("rules");

        assert!(
            rules
                .iter()
                .any(|r| r["content"] == "a rule the writer chose to carry"),
            "the marked rule did not survive the cap: {rules:?}"
        );
        assert!(
            rules
                .iter()
                .all(|r| r["content"] != "an ordinary rule nobody marked"),
            "an unmarked rule was carried anyway: {rules:?}"
        );
        assert_eq!(booted["identity"]["rules_elided"], true, "{booted}");
        let note = booted["identity"]["rules_note"]
            .as_str()
            .expect("the boot names what it left home: {booted}");
        assert!(
            note.contains("1 of 2") && note.contains("bot:gamma") && note.contains("facts"),
            "{note}"
        );
        assert!(
            note.contains("starred"),
            "the note that says rules were left home never names the key that keeps one, so the \
             only path to it is the diff: {note}"
        );
    }

    /// **Seats can be full with nothing elided at all** — exactly
    /// `CARRIED_RULES` starred and nothing else in force, so `rules_elided`
    /// never fires — and the boot still has to say the seats are full,
    /// because a session that stars a sixth would otherwise learn the cap
    /// exists only by watching one quietly fall off.
    ///
    /// **Paired with a boot one rule short of the cap**, which must say
    /// nothing: a build that always added the sentence would pass the
    /// positive half alone.
    #[tokio::test]
    async fn a_boot_with_every_seat_taken_says_so_and_one_spare_says_nothing() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        for n in 0..jojobot_domain::text::CARRIED_RULES {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    fields: Some(
                        [("starred".to_string(), "true".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("bot:gamma", &format!("starred rule {n}"))
                },
            )
            .await;
        }

        let full = boot(&jojobot, "gamma").await;
        assert_eq!(
            full["identity"]["rules_elided"],
            serde_json::Value::Null,
            "every starred rule fit, so nothing was elided: {full}"
        );
        let note = full["identity"]["rules_note"]
            .as_str()
            .unwrap_or_else(|| panic!("a boot with every seat taken has to say so: {full}"));
        assert!(
            note.contains("skill"),
            "the seats-full sentence names where a one-off rule belongs instead: {note}"
        );

        make_bot(&jojobot, "delta").await;
        for n in 0..jojobot_domain::text::CARRIED_RULES - 1 {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    fields: Some(
                        [("starred".to_string(), "true".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("bot:delta", &format!("starred rule {n}"))
                },
            )
            .await;
        }
        let spare = boot(&jojobot, "delta").await;
        assert_eq!(
            spare["identity"]["rules_note"],
            serde_json::Value::Null,
            "one seat short of the cap must say nothing about it being full: {spare}"
        );
    }

    /// **One response never contradicts itself about which boxes exist.**
    ///
    /// It could before: booting minted the declared box *between* taking the
    /// snapshot and reporting the identity, so a single payload said in one
    /// half that no such box was on the board and in the other that it was
    /// there with counts — and a session had no way to tell which half to
    /// believe. Both halves are reads of the same world now; this holds them to
    /// agreeing.
    ///
    /// There is no such thing as a bot whose box nobody has opened — a box
    /// opens with its owner — so the reachable disagreement is the opposite
    /// one: an identity naming a box the snapshot beside it does not list.
    #[tokio::test]
    async fn a_boot_never_disagrees_with_its_own_snapshot_about_a_box() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma").await;
        make_bot(&jojobot, "delta").await;

        let booted = boot(&jojobot, "sigma").await;
        let named = booted["identity"]["owned_mailbox"]["name"]
            .as_str()
            .expect("the identity names its box")
            .to_string();
        let on_the_board: Vec<String> = booted["snapshot"]["entities"]["bots"]
            .as_array()
            .expect("the bots")
            .iter()
            .filter(|b| !b["mail"].is_null())
            .map(|b| b["handle"].as_str().expect("a handle").to_string())
            .collect();
        assert!(
            on_the_board.contains(&format!("bot:{named}")),
            "the identity named {named:?}, and the snapshot beside it gives it no mail: {booted}"
        );
        // …and the other bot's mail is on the same board, so this is not
        // passing because the board holds exactly one thing.
        assert!(on_the_board.contains(&"bot:delta".to_string()), "{booted}");
    }

    /// **One orientation, one door — but not one answer any more, by
    /// design.** Naming a bot is `start_here` plus an identity, never a
    /// second world-model to drift out of step with the first; that used to
    /// mean the two doors' `orientation` text was byte-identical, which
    /// stopped being true the day a named boot stopped being offered the
    /// remainder at all (`essay_for_boot`'s own doc). Equality would now be
    /// asserting the split does not exist. **The relationship that survives
    /// is what this proves instead**: both carry the core, whole and
    /// identical; only the anonymous one also carries the remainder; and the
    /// named one names the call that reaches what it withheld — so a session
    /// that boots as an identity still learns there is more, and how to get
    /// it, rather than reading a shorter text with no sign anything is
    /// missing.
    #[tokio::test]
    async fn a_named_boot_and_an_anonymous_one_share_the_core_and_differ_by_design() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let identified = boot(&jojobot, "gamma").await;

        let anonymous_orientation = anonymous["orientation"]
            .as_str()
            .expect("an anonymous boot ships the essay: {anonymous}");
        let named_orientation = identified["orientation"]
            .as_str()
            .expect("a named boot ships the core: {identified}");
        assert_eq!(
            anonymous_orientation,
            crate::orientation::essay::orientation(),
            "the anonymous boot carries the core and the remainder, joined: {anonymous}",
        );
        assert_eq!(
            named_orientation,
            essay::ORIENTATION_CORE,
            "the named boot carries the core alone: {identified}",
        );
        assert!(
            anonymous_orientation.starts_with(named_orientation),
            "the named boot's text must be a genuine prefix of the anonymous one's, not merely \
             equal in length or independently worded",
        );
        let note = identified["orientation_note"]
            .as_str()
            .expect("the named boot names the call that reaches what it withheld: {identified}");
        assert!(
            note.contains("start_here"),
            "the note must name the way back to the rest: {note}"
        );
        assert!(
            anonymous["orientation_note"].is_null(),
            "the anonymous boot withheld nothing, so it has nothing to explain: {anonymous}"
        );
        // **What EXISTS is one answer; whose queue it is, is not.** The bots
        // themselves are the shared invariant — their mail is scoped to the
        // caller, which is the whole point of scoping, so a full-equality
        // assertion here would be asserting the scoping does not happen.
        let handles = |body: &serde_json::Value| -> Vec<String> {
            body["snapshot"]["entities"]["bots"]
                .as_array()
                .expect("the bots")
                .iter()
                .map(|b| b["handle"].as_str().expect("a handle").to_string())
                .collect()
        };
        assert_eq!(
            handles(&anonymous),
            handles(&identified),
            "what exists is one answer, whoever asks"
        );
        assert_eq!(
            anonymous["snapshot"]["entities"]["by_kind"],
            identified["snapshot"]["entities"]["by_kind"],
        );
        // The mailbox half is deliberately NOT equal once a bot drains a
        // box — that is the whole point of scoping counts to the caller — so
        // the shared invariant is the set of boxes, not their contents. Make
        // sure the fixture actually exercises this (give gamma a mailbox), or
        // a full-equality assertion could pass for a reason that has nothing
        // to do with the invariant being claimed.
        let names = |body: &serde_json::Value| -> Vec<String> {
            body["snapshot"]["entities"]["bots"]
                .as_array()
                .expect("the bots")
                .iter()
                .map(|b| b["handle"].as_str().expect("a handle").to_string())
                .collect()
        };
        assert_eq!(
            names(&anonymous),
            names(&identified),
            "both doors see the same board; they differ only in whose queue is theirs to read"
        );
        assert!(
            anonymous["identity"].is_null(),
            "an anonymous session claims no identity"
        );
    }

    /// …and the difference the previous test carves out, asserted directly: the
    /// booted door counts the box its identity drains, the anonymous one does
    /// not.
    #[tokio::test]
    async fn the_two_doors_differ_only_in_whose_queue_is_theirs_to_read() {
        let jojobot = handler();
        make_box(&jojobot, "dev").await;
        send(&jojobot, "dev", "delta", "your hand-off").await;

        let counts_for = |body: &serde_json::Value| bot_entry(body, "dev");

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(
            counts_for(&anonymous)["mail"].get("counts").is_none(),
            "{anonymous}"
        );
        assert_eq!(counts_for(&anonymous)["yours"], false);
        // **The snapshot says once, not per bot.** The same rule the whole
        // surface keeps — a reader must not have to infer withheld from
        // empty — kept here at the one place it is true instead of on
        // every bot's own entry.
        assert!(
            anonymous["snapshot"]["mail"]["note"]
                .as_str()
                .is_some_and(|note| !note.trim().is_empty()),
            "{anonymous}"
        );

        let identified = boot(&jojobot, "dev").await;
        assert_eq!(
            counts_for(&identified)["mail"]["counts"]["new"],
            1,
            "{identified}"
        );
        assert_eq!(counts_for(&identified)["yours"], true);
    }

    /// **Both halves of the door make the same promise, so both keep it.**
    /// Orientation lands even when a world is down. An identified half that
    /// hard-errors the moment a bot owns a box makes every box-owning identity
    /// unbootable over an outage in the *other* world, while the charter and
    /// the rules sit in Memory and are right there.
    ///
    /// So the mailbox half degrades on its own, the same way the snapshot's
    /// does: the boot lands, the identity is whole, and the one thing jojobot
    /// cannot answer says so instead of guessing.
    #[tokio::test]
    async fn a_boot_survives_a_world_that_is_down_exactly_as_an_anonymous_one_does() {
        // Stood up while both worlds are up — a claim that cannot be screened
        // is refused, so this bot could not have been created below.
        let memory = Arc::new(InMemoryMemory::booted());
        let healthy = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        make_bot(&healthy, "gamma").await;
        healthy
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Holds the plan.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");

        let jojobot = handler_with_mailboxes_down(memory);
        let body = boot(&jojobot, "gamma").await;
        assert_ne!(body["status"], "blocked", "a boot must still land: {body}");

        let me = &body["identity"];
        assert_eq!(me["bot"]["id"], "bot:gamma");
        assert_eq!(
            me["charter"], "Holds the plan.",
            "the half that is up arrives whole"
        );

        // Ownership is stated on the box, not the bot's own record, so an
        // unreachable mailbox world means jojobot cannot say which box is
        // yours, or whether you have one — and it must say exactly that
        // instead of naming a box it cannot see.
        // **That a note is THERE, never what it says.** See the roster case
        // above for why the wording stays unpinned.
        assert!(
            body["snapshot"]["mail"]["note"]
                .as_str()
                .is_some_and(|note| !note.trim().is_empty()),
            "an unreachable mail world came back as a bare marker with nothing a caller can act \
             on: {body}"
        );
        let owned = &me["owned_mailbox"];
        assert_eq!(owned["available"], false, "got {owned}");
        assert!(
            owned["name"].is_null(),
            "a box it cannot read is not a box it can name: {owned}"
        );
        // **The FIELD stays null and the NOTE stops pretending.** Existence is
        // what an outage hides; the name is derived from the handle the caller
        // is already holding, and reporting it as equally unknown made a
        // mystery of the half jojobot can work out from first principles. The
        // note says which is which — expect this name, do not conclude it is
        // there.
        let note = owned["note"].as_str().expect("a note");
        assert!(
            note.contains("'gamma'"),
            "the derivable half is named: {note}"
        );
        assert!(
            note.contains("not as confirmation"),
            "…and is fenced off from the half that is not: {note}"
        );

        // …and the snapshot degrades beside it, exactly as it does anonymously.
        assert_eq!(body["snapshot"]["mail"]["available"], false);
    }
}

/// **Skills are listed, never shipped, and fetched by name through the door
/// that already orients.**
///
/// `start_here` is skill zero: it is where a session learns the model, so it is
/// where a session learns which procedures exist. Packing the fetch onto it too
/// is rule 66 — the surface grows by giving an existing verb more to do, not by
/// growing a verb count — and rule 51, because the index and the fetch read one
/// list rather than two.
#[cfg(test)]
mod skills_are_indexed_not_shipped {
    use super::*;
    use crate::harness::*;
    use crate::orientation::skills;

    /// The boot names every skill and hands over no procedure.
    ///
    /// The negative here is the whole point, and it is paired: an index with no
    /// names would pass "no body is present" trivially, so the names are
    /// asserted first and the body's own words are looked for second.
    #[tokio::test]
    async fn a_boot_names_the_skills_and_carries_none_of_their_bodies() {
        let booted = json_of(
            &handler()
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    resume: None,
                    skill: None,
                    section: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here answers"),
        );

        let listed = booted["skills"]
            .as_array()
            .unwrap_or_else(|| panic!("the boot must name the skills that exist: {booted}"));
        assert_eq!(
            listed.len(),
            skills::SKILLS.len(),
            "every shipped skill is listed: {booted}"
        );
        assert_eq!(listed[0]["name"], "recommend", "{booted}");
        assert!(
            listed[0]["when_to_use"]
                .as_str()
                .is_some_and(|w| w.contains("recommendation")),
            "the index says what the skill is FOR — it is what a session chooses on: {booted}"
        );

        // …and not one procedure travelled with it. Asserted against the
        // shipped bodies themselves rather than against words quoted out of
        // them, so rewriting a procedure cannot quietly disarm this.
        //
        // **Against the ESCAPED form**, because the haystack is serialized
        // JSON: a body's newlines are `\n` there, so searching for its raw
        // bytes finds nothing whether or not it was shipped.
        let whole = booted.to_string();
        for skill in skills::SKILLS {
            assert!(
                !skill.body.is_empty(),
                "an empty body would make the check below vacuous: {}",
                skill.name
            );
            let as_json = serde_json::Value::String(skill.body.to_string()).to_string();
            assert!(
                !whole.contains(as_json.trim_matches('"')),
                "the {} body rode along in the boot payload: {whole}",
                skill.name
            );
        }
    }

    /// …and the body comes back when it is asked for by name.
    #[tokio::test]
    async fn a_skill_body_is_fetched_by_name() {
        let body = json_of(
            &handler()
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    resume: None,
                    skill: Some("recommend".into()),
                    section: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here answers"),
        );
        assert_eq!(body["skill"]["name"], "recommend", "{body}");
        // **Compared against the shipped constant, not against phrasing.** An
        // earlier version pinned two sentences out of the body and went red the
        // day the text was rewritten with nothing wrong — the same defect the
        // boot story carried. What this verb owes is that the wire hands over
        // what the binary holds, whole and unaltered, and that is checkable
        // without knowing a word of it.
        assert_eq!(
            body["skill"]["body"].as_str(),
            Some(skills::named("recommend").expect("it ships").body),
            "the fetched skill must be the shipped body, byte for byte: {body}"
        );
    }

    /// **A fetch that also names a bot is two asks, and is refused.** The
    /// fetch returns before the boot and starts no session, so honouring the
    /// `bot` would have handed back a body, no `sid`, and no word about the
    /// argument that was dropped — a caller could not tell it had not booted.
    #[tokio::test]
    async fn a_skill_fetch_that_also_names_a_bot_is_blocked() {
        let jojobot = handler();
        let body = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: Some("otto".into()),
                    brief: None,
                    skill: Some("recommend".into()),
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here answers"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        let how = body["how_to_proceed"].as_str().unwrap_or_default();
        assert!(
            how.contains("skill") && how.contains("bot"),
            "the refusal must name both ways forward: {body}"
        );
        // Paired: nothing was served. A refusal that also handed over the body
        // would be the silent drop wearing a status field.
        assert!(
            body["skill"].is_null(),
            "a refused fetch must not also serve the procedure: {body}"
        );
        // **The text reads as a sentence, not as source.** A multi-line Rust
        // literal without a trailing continuation bakes the next line's
        // indentation into the string, and this is agent-facing prose: the
        // caller reads it to find out what to do next.
        assert!(
            !how.contains("   "),
            "the refusal carries source indentation, so it reads as broken text: {how:?}"
        );
    }

    /// A name that is no skill is blocked with the ones that are — the same
    /// shape every other miss on this surface wears, so a caller branches on
    /// `status` here exactly as everywhere else.
    #[tokio::test]
    async fn an_unknown_skill_is_blocked_and_names_the_ones_that_exist() {
        let body = json_of(
            &handler()
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    resume: None,
                    skill: Some("recomend".into()),
                    section: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here answers"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["attempted"], "recomend", "{body}");
        assert!(
            body.to_string().contains("recommend"),
            "the refusal names the skills that do exist: {body}"
        );
    }
}

/// **`essay_for_boot`'s cut behaviour, budget-precise.** Exercised directly
/// against the real essay's remainder, at budgets derived from that text's
/// own measured unit lengths (`essay::remainder_units`) rather than
/// invented — a boundary here is a boundary the real essay actually has,
/// not a guess at one. `budget` is the remainder's own allowance: the
/// caller (`orient`) always prepends the core separately, having already
/// paid for it in the floor.
#[cfg(test)]
mod essay_for_boot_budget {
    use crate::orientation::essay;
    use crate::orientation::orient::essay_for_boot;

    /// **What a piece of the essay costs in the answer a caller receives** —
    /// serialized, where a newline or a quote is two characters. The budget
    /// bounds that answer, so every boundary here is measured the same way.
    fn cost(text: &str) -> usize {
        serde_json::to_string(text)
            .expect("text serializes")
            .chars()
            .count()
            - 2
    }

    fn remainder_shape() -> (&'static str, Vec<&'static str>, Vec<usize>) {
        let (preamble, sections) = essay::remainder_units(essay::ORIENTATION_REMAINDER);
        let headings: Vec<&'static str> = sections.iter().map(|s| s.heading).collect();
        let lens: Vec<usize> = sections.iter().map(|s| cost(s.body)).collect();
        (preamble, headings, lens)
    }

    /// A budget covering the whole remainder ships it whole, byte for byte
    /// — the case this whole mechanism must never regress.
    #[test]
    fn a_budget_covering_the_whole_remainder_ships_it_whole() {
        let whole_remainder_len = cost(essay::ORIENTATION_REMAINDER);
        let served = essay_for_boot(false, &serde_json::Value::Null, whole_remainder_len);
        assert_eq!(
            served.text.as_deref(),
            Some(essay::orientation()).as_deref()
        );
        assert!(!served.elided, "{:?}", served.note);
        assert!(served.note.is_none(), "{:?}", served.note);
    }

    /// A budget covering the preamble and the first two `##` sections, and
    /// no more, ships exactly those and names exactly the other two as
    /// left out — never a `###` subheading on its own.
    #[test]
    fn a_partial_budget_ships_the_leading_sections_and_names_the_rest() {
        let (preamble, headings, lens) = remainder_shape();
        assert!(headings.len() >= 3, "{headings:?}");
        let budget = cost(preamble) + lens[0] + lens[1];
        let served = essay_for_boot(false, &serde_json::Value::Null, budget);
        let text = served.text.expect("some text always ships");
        assert!(text.starts_with(essay::ORIENTATION_CORE), "{text:?}");
        assert!(text.contains(preamble), "{text:?}");
        let (_, sections) = essay::remainder_units(essay::ORIENTATION_REMAINDER);
        for heading in &headings[..2] {
            let body = sections
                .iter()
                .find(|s| &s.heading == heading)
                .unwrap()
                .body;
            assert!(text.contains(body), "shipped section missing: {heading}");
        }
        for heading in &headings[2..] {
            assert!(
                !text.contains(heading),
                "left-out section shipped: {heading}\n{text}"
            );
        }
        assert!(served.elided);
        let note = served.note.expect("a cut answer names what it left out");
        for heading in &headings[2..] {
            assert!(note.contains(heading), "{note}");
        }
        for heading in &headings[..2] {
            assert!(
                !note.contains(heading),
                "a shipped heading is named as left out: {note}"
            );
        }
        assert!(
            !note.contains("###"),
            "a subheading is never named on its own in the note: {note}"
        );
        assert!(
            note.contains("start_here") && note.contains("section"),
            "the note names the call that reaches a left-out section: {note}"
        );
    }

    /// A budget covering the preamble exactly, and nothing past it, ships
    /// the preamble alone and names every `##` heading as left out.
    #[test]
    fn a_budget_covering_only_the_preamble_ships_it_alone_and_names_every_heading() {
        let (preamble, headings, _) = remainder_shape();
        let budget = cost(preamble);
        let served = essay_for_boot(false, &serde_json::Value::Null, budget);
        let text = served.text.expect("some text always ships");
        assert!(text.starts_with(essay::ORIENTATION_CORE), "{text:?}");
        assert!(text.contains(preamble), "{text:?}");
        for heading in &headings {
            assert!(!text.contains(heading), "{text}");
        }
        assert!(served.elided);
        let note = served.note.expect("a cut answer names what it left out");
        for heading in &headings {
            assert!(note.contains(heading), "{note}");
        }
        assert!(
            note.contains("start_here") && note.contains("section"),
            "the note names the call that reaches a left-out section: {note}"
        );
    }

    /// A budget too tight even for the preamble ships the core alone — the
    /// note says the whole remainder was left out and names its opening
    /// paragraphs in plain words, since they have no heading to be named by.
    #[test]
    fn a_budget_too_tight_for_the_preamble_ships_core_only_and_names_the_opening_paragraphs() {
        let (preamble, headings, _) = remainder_shape();
        let budget = cost(preamble).saturating_sub(1);
        let served = essay_for_boot(false, &serde_json::Value::Null, budget);
        let text = served.text.expect("the core always ships");
        assert_eq!(text, essay::ORIENTATION_CORE, "{text:?}");
        assert!(served.elided);
        let note = served.note.expect("a cut answer names what it left out");
        assert!(
            note.contains("opening paragraph"),
            "the un-headed preamble is named in plain words: {note}"
        );
        for heading in &headings {
            assert!(note.contains(heading), "{note}");
        }
        assert!(
            note.contains("start_here") && note.contains("\"opening\""),
            "the note names the call and the literal word that reaches the opening paragraphs: \
             {note}"
        );
    }
}

//! **The one orientation, anonymous or identified.**
//!
//! Naming a bot adds the identity half to an answer that is otherwise the same
//! text and the same snapshot; it does not open a second way in. The one call
//! site is the point, and `there_is_exactly_one_orientation_verb` counts it.

use super::*;

/// **Null out one rule's `details`, marked** — the same shape whichever
/// caller reaches for it: a ranked cut, or the floor measurement below that
/// has to know the answer's size with every rule's reasoning already gone.
fn elide_rule_details(rule: &mut serde_json::Value) {
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
                rule_slots.push((i, details.chars().count()));
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
/// Returns the text to ship (`None` when `brief` already dropped it, or an
/// anonymous boot's whole essay did not fit) and whether an anonymous boot's
/// omission was the ceiling's doing rather than `brief`'s.
fn essay_for_boot(
    brief: bool,
    identity: &serde_json::Value,
    budget: usize,
) -> (Option<String>, bool) {
    if brief {
        return (None, false);
    }
    if !identity.is_null() {
        return (Some(essay::ORIENTATION_CORE.to_string()), false);
    }
    let whole = format!(
        "{}{}",
        essay::ORIENTATION_CORE,
        essay::ORIENTATION_REMAINDER
    );
    let candidates = [whole.chars().count()];
    let kept = (text::Capped { budget }).head_or_none(&candidates, |len| *len);
    match kept.kept().is_empty() {
        false => (Some(whole), false),
        true => (None, true),
    }
}

impl Jojobot {
    /// The one orientation, anonymous or identified — **the one call site is
    /// the point.** Naming a bot adds the identity half to an answer that is
    /// otherwise the same text and the same snapshot; it does not open a second
    /// way in.
    pub(crate) async fn orient(
        &self,
        bot: Option<&EntityId>,
        brief: bool,
        resume: Option<&str>,
        // The IANA zone this run resolves days in, validated at the door.
        timezone: Option<&str>,
        today: Option<jiff::civil::Date>,
        // What the handle this caller arrived with is worth — from
        // [`Jojobot::standing`], and `Null` when they arrived with none.
        carried: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        // The entity index is read ONCE for the whole answer. Three parts of
        // a boot need it — the counts by kind, which boxes the caller drains,
        // and the identity itself — and reading it three times would mean
        // three remote round trips per boot, and three reads that can
        // disagree with one another inside a single payload.
        //
        // Best-effort per world: orientation must land even when one world is
        // down — a fresh agent on a half-configured server still gets the map.
        let index = self.memory.list_entities(None).await;
        let entities = match &index {
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
        };
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
                        None => self.clock().today_in(
                            &timezone
                                .and_then(|name| jiff::tz::TimeZone::get(name).ok())
                                .unwrap_or(jiff::tz::TimeZone::UTC),
                        ),
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
                             rather than beside the bots",
                    "by_owner": boxes
                        .iter()
                        .map(|b| serde_json::json!({
                            "owner": b.owner.as_str(),
                            "yours": mine.drains(b.name.as_str()),
                            "mail": match mine.drains(b.name.as_str()) {
                                true => mailbox_json(b),
                                false => serde_json::json!({
                                    "counts": serde_json::Value::Null,
                                    "counts_elided": true,
                                    "quarantined": quarantined_json(b),
                                }),
                            },
                        }))
                        .collect::<Vec<_>>(),
                })
            }
            Ok(_) => serde_json::json!({ "available": true }),
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
                        true => mailbox_json(b),
                        // Somebody else's queue is not yours to weigh. Their
                        // quarantine still rides out: it is the only place an
                        // unreadable card shows, and the caller who most needs
                        // it is a SENDER, who would otherwise conclude their
                        // message was never sent.
                        false => serde_json::json!({
                            "counts": serde_json::Value::Null,
                            "counts_elided": true,
                            "quarantined": quarantined_json(b),
                        }),
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
        // **Names and origin, never bodies.** What a kind means and which keys
        // it asks for is bigger than the list and is a deliberate second read.
        // The origin rides along because it is what a caller can ACT on: a
        // shipped name is closed to redeclaration and a declared one is theirs
        // to reshape, and no other field says which.
        //
        // **Read from the store rather than from the list this build ships**,
        // for the same reason the boot's other reads are: a kind a caller
        // declared is part of the vocabulary, and a shipped kind the store
        // somehow lost is not.
        let vocabulary = match self.memory.declared_kinds().await {
            Ok(kinds) => {
                let mut named: Vec<serde_json::Value> = kinds
                    .iter()
                    .map(|(token, origin)| {
                        serde_json::json!({ "kind": token, "origin": origin.as_token() })
                    })
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
                    .map(|declared| {
                        serde_json::json!({
                            "type": declared.name,
                            "origin": declared.origin.as_token(),
                        })
                    })
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
        let snapshot =
            serde_json::json!({ "entities": entities, "mail": mail, "vocabulary": vocabulary });
        // **Only after the identity resolved.** A name that is no bot boots
        // nothing, so it starts no session and sweeps nothing either — binding
        // a connection to an identity jojobot just refused would be a session
        // belonging to nobody.
        let session = match bot {
            None => serde_json::Value::Null,
            Some(bot) => match self.attach(bot, resume, timezone, today).await {
                Ok(session) => session,
                // A handle that addresses nothing stops the whole answer.
                // Handing back orientation around it would bury the one thing
                // the caller has to act on.
                Err(refused) => return Ok(refused),
            },
        };
        // **ONE declared ceiling for the WHOLE answer** — [`text::BOOT_ANSWER`]
        // — not for its prose alone (rule 138's own bar: a payload the client
        // cannot read, not a field inside it). Measure the FLOOR first:
        // everything that ships whatever the ranking below decides — bot
        // metadata, charter whole (it is never cut, see `rank_rule_details`),
        // the essay's own core for a named boot (also never cut, see
        // `essay_for_boot`), every rule's own structural fields (address,
        // dates, provenance, standing, status, fields, refs), session,
        // snapshot, skills — with every rule's `details` already gone. What
        // is LEFT of the ceiling after that floor is what the rules'
        // `details` compete for, and — for an anonymous boot only — what the
        // whole essay is ranked against; see `essay_for_boot`.
        let mut floor_identity = identity.clone();
        if let Some(rules) = floor_identity
            .get_mut("rules")
            .and_then(|r| r.as_array_mut())
        {
            for rule in rules.iter_mut() {
                elide_rule_details(rule);
            }
        }
        // **The essay's core rides in the floor for a named boot, exactly
        // like the charter — an anonymous boot pays nothing here, because it
        // either gets the whole essay or none of it, ranked below.** It is
        // short by design and never ranked for a named boot, so its true
        // cost is counted here, once, rather than competing with the rules'
        // `details`.
        let core = (!brief && !identity.is_null()).then_some(essay::ORIENTATION_CORE);
        let floor_len = serde_json::json!({
            "orientation": core,
            "orientation_elided": true,
            "skills": skills::index(),
            "snapshot": snapshot.clone(),
            "identity": floor_identity,
            "session": session.clone(),
            "carried_session": carried.clone(),
            "clock": self.stated_clock(),
        })
        .to_string()
        .chars()
        .count();
        let remaining_for_prose = text::BOOT_ANSWER.budget.saturating_sub(floor_len);

        let mut identity = identity;
        rank_rule_details(&mut identity, remaining_for_prose);
        let (orientation, essay_elided_by_ceiling) =
            essay_for_boot(brief, &identity, remaining_for_prose);
        let mut answer = serde_json::json!({
            "orientation": orientation,
            // **The elision is marked, and that is all it is.** The essay used
            // to arrive stamped with a version so a returning session could ask
            // whether the copy it held was current; the stamp is gone, and no
            // staleness check replaces it. What is left is the marker every
            // elision on this surface owes — less came back, and the caller is
            // told so rather than left to infer withheld from empty.
            //
            // **True unless an anonymous boot got the whole essay** — the
            // only shape that is ever NOT missing something: a named boot's
            // core is elided by design, and `None` (brief, or an anonymous
            // boot the ceiling declined) is elided by construction.
            "orientation_elided": !(identity.is_null() && orientation.is_some()),
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
        });
        // **Three reasons an answer can carry less than the whole essay, and
        // `brief`'s is the only one that needs no note** — the caller set
        // that flag and already knows why. The other two are `orientation`
        // itself distinguishing them: a named boot's core, present, means
        // the remainder was never offered — a designed omission, not a
        // ceiling one. `None` on an anonymous boot means the ceiling
        // declined the whole essay outright.
        if !brief && let Some(obj) = answer.as_object_mut() {
            let note = match (identity.is_null(), essay_elided_by_ceiling) {
                (false, _) => Some(
                    "a named boot's orientation is the essay's core; call start_here again \
                     naming no bot to read the whole essay"
                        .to_string(),
                ),
                (true, true) => Some(
                    "the essay did not fit this boot's declared prose ceiling — call start_here \
                     again with nothing else competing for it to read it whole"
                        .to_string(),
                ),
                (true, false) => None,
            };
            if let Some(note) = note {
                obj.insert("orientation_note".into(), note.into());
            }
        }
        json_result(&answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;

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
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
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
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
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
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
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
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
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
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "x".repeat(20_000),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");

        let result = jojobot
            .start_here(Parameters(OrientArgs {
                timezone: None,
                bot: Some("gamma".into()),
                brief: Some(false),
                skill: None,
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
    /// A named boot can never get the whole essay — the remainder is not a
    /// ranking candidate for one, it is a rule (`essay_for_boot`'s own doc).
    /// That was this case's original shape, resting on whatever margin a
    /// light identity happened to leave; it broke the day the margin ran out
    /// (206 characters short, on the lightest real identity measured), and
    /// fixing the margin would only have moved the cliff. **An anonymous
    /// boot is where "whole" is the actual, load-bearing behaviour** — no
    /// identity to pay for, so it is what a caller who asked to be taught
    /// the surface actually gets, and it is worth proving the essay's own
    /// path through the cap still ships it whole rather than assuming an
    /// anonymous boot never has to ask.
    #[tokio::test]
    async fn an_anonymous_boot_ships_the_essay_whole() {
        let jojobot = handler();
        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    timezone: None,
                    bot: None,
                    brief: Some(false),
                    skill: None,
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
            "an anonymous boot must ship the essay whole, core and remainder joined",
        );
        assert_eq!(booted["orientation_elided"], false, "{booted}");
        assert!(
            booted["orientation_note"].is_null(),
            "nothing was cut, so there is nothing to explain: {booted}"
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
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
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

    /// **A fault on the board is not somebody's queue, and it is not scoped
    /// away with one.** What jojobot cannot read as a message is counted
    /// nowhere and delivered nowhere, so the only caller who can act on knowing
    /// it exists is often a SENDER — somebody who by definition does not drain
    /// that box, and who would otherwise read the silence as "my message never
    /// arrived". So the counts are withheld from a box that is not yours and
    /// the unreadable report is not.
    ///
    /// This is the only answer that renders a box the caller does not drain:
    /// `read_mailbox`'s counting mode is about your own box by construction,
    /// and `list_sent` only reaches boxes you have posted into.
    #[tokio::test]
    async fn a_boot_shows_what_cannot_be_read_even_on_a_box_it_will_not_count() {
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(boxes.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        boxes.quarantine(
            &MailboxName("delta".into()),
            &MessageId("4212".into()),
            "its row cannot be read — a state or a sender has been edited past parsing",
        );

        let booted = boot(&jojobot, "gamma").await;
        let theirs = bot_entry(&booted, "delta");
        assert_eq!(theirs["yours"], false);
        assert!(
            theirs["mail"]["counts"].is_null(),
            "somebody else's queue stays theirs: {theirs}"
        );
        assert_eq!(
            theirs["mail"]["quarantined"]["count"], 1,
            "…and the fault on it does not: {booted}"
        );
        assert_eq!(theirs["mail"]["quarantined"]["ids"][0], "4212");
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
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
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
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
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
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(false),
                    skill: None,
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
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
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
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(
            counts_for(&anonymous)["mail"]["counts"].is_null(),
            "{anonymous}"
        );
        assert_eq!(counts_for(&anonymous)["yours"], false);
        // Elided, never silently — the same rule the whole surface keeps: a
        // reader must not have to infer withheld from empty.
        assert_eq!(
            counts_for(&anonymous)["mail"]["counts_elided"],
            true,
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
                    timezone: None,
                    bot: None,
                    brief: None,
                    resume: None,
                    skill: None,
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
                    timezone: None,
                    bot: None,
                    brief: None,
                    resume: None,
                    skill: Some("recommend".into()),
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
                    timezone: None,
                    bot: Some("otto".into()),
                    brief: None,
                    skill: Some("recommend".into()),
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
                    timezone: None,
                    bot: None,
                    brief: None,
                    resume: None,
                    skill: Some("recomend".into()),
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

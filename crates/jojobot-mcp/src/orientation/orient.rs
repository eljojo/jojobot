//! **The one orientation, anonymous or identified.**
//!
//! Naming a bot adds the identity half to an answer that is otherwise the same
//! text and the same snapshot; it does not open a second way in. The one call
//! site is the point, and `there_is_exactly_one_orientation_verb` counts it.

use super::*;

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
                let mut by_kind = std::collections::BTreeMap::<&str, usize>::new();
                for e in entities {
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
                let mut bots: Vec<&str> = entities
                    .iter()
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
        json_result(&serde_json::json!({
            "orientation": if brief { serde_json::Value::Null } else { essay::ORIENTATION.into() },
            // **The elision is marked, and that is all it is.** The essay used
            // to arrive stamped with a version so a returning session could ask
            // whether the copy it held was current; the stamp is gone, and no
            // staleness check replaces it. What is left is the marker every
            // elision on this surface owes — less came back, and the caller is
            // told so rather than left to infer withheld from empty.
            "orientation_elided": brief,
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
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;

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

    #[tokio::test]
    async fn an_unreadable_entity_index_no_longer_hides_who_drains_what() {
        let memory = Arc::new(InMemoryMemory::booted());
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let seeded = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            Arc::new(InMemorySessions::new()),
            crate::harness::seeded_registry(),
        );
        make_box(&seeded, "dev").await;
        send(&seeded, "dev", "delta", "your hand-off").await;

        let blind = Jojobot::new(
            Arc::new(DownMemory(Down::EntityIndex, memory)),
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
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

    /// **One orientation, one door.** Naming a bot is `start_here` plus an
    /// identity — not a second world-model to drift out of step with the first.
    #[tokio::test]
    async fn a_named_boot_and_an_anonymous_one_hand_over_the_same_world() {
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
        assert_eq!(
            anonymous["orientation"], identified["orientation"],
            "the world-model is one text, or the two doors teach different jojobots"
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

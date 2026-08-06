//! **Starting or resuming this bot's session** — the half of a boot that reads
//! the board and decides what to hand back.
//!
//! Booting an identity IS starting its session; there is no separate verb. The
//! sweep itself is domain policy and lives in `jojobot_domain::session`. What
//! is here is the decision on top of it: a handle at once when nothing
//! survives, or the choice and no handle when something does.

use super::*;

impl Jojobot {
    /// Start or resume this bot's session, and hand back the handle for it.
    ///
    /// **Booting an identity IS starting its session** — there is no separate
    /// verb, because there is no moment between "I am gamma" and "gamma is
    /// working" for one to sit in. The sweep runs first either way: any `active`
    /// session of THIS bot whose last beat is older than [`ABANDONED_AFTER`] is
    /// closed as `abandoned` — lazily, at boot, because there is no background
    /// runtime until M8 and a session left open forever would make "resume where
    /// you left off" resume something from last month.
    ///
    /// **Then the boot has two branches, and which one you get is not a
    /// preference.**
    ///
    /// * **Nothing survives the sweep** → the handle comes back immediately.
    ///   There is nothing to decide, and making the caller ask twice for an
    ///   address would invent the moment this verb exists to deny.
    /// * **Something does** → the CHOICE comes back and no handle: every
    ///   resumable run, each named by what it was working on, because that is
    ///   the only thing that tells two runs of one identity apart. The handle
    ///   arrives when the caller answers — `resume` with one of them, or `new`.
    ///   Never attach silently: that decides for the caller, and worse, for a
    ///   caller who deliberately left the run open for somebody else.
    ///
    /// Either way the **card stays lazy**: no card until the first write, so a
    /// boot that never works leaves no trace — which is what keeps "creation is
    /// an intentional act" true for a verb whose whole job is to start
    /// something. And **nothing here auto-wraps**: choosing `new` beside a
    /// running session leaves that session running.
    ///
    /// `Err` is a handle that addresses nothing — see [`handle_declined`].
    /// A session store that is down degrades exactly as the mailbox world does:
    /// the boot still lands, and the block says jojobot does not know rather
    /// than guessing.
    pub(crate) async fn attach(
        &self,
        bot: &EntityId,
        resume: Option<&str>,
    ) -> Result<serde_json::Value, CallToolResult> {
        // **A boot is a read-the-board → decide → write-the-registry span like
        // the write verbs, so it takes the same gate, on the same key.** Its
        // board read is full of awaits — sweeping a stale card is one — and a
        // first write running inside them commits a card the boot then sees
        // with no handle against it yet, which is a second handle minted for a
        // run that already has one. See [`Jojobot::gate_key`] for why the key
        // is the identity: it is the only name a boot and a write share.
        //
        // Taken here rather than inside `sweep_and_find`, which runs under it —
        // the mutex is not reentrant.
        let gate = self.registry.gate(bot.as_str());
        let _serialized = gate.lock().await;
        // The clock is read HERE and handed down: the sweep is domain policy
        // and the domain is clock-free, so the instant it decides against is
        // stamped at the edge exactly as a capture's date is.
        let swept_at = jiff::Timestamp::now();
        let Board {
            live,
            offerable,
            swept,
            unswept,
        } = match sweep_and_find(self.sessions.as_ref(), bot, swept_at).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(error = %e, bot = %bot, "the session world is not reachable");
                // **No handle is minted.** One handed out here would address
                // either a fresh session or one already running, and jojobot
                // cannot say which — so it hands out none and says so.
                return Ok(serde_json::json!({
                    "available": false,
                    "note": "the session world is not reachable right now, so jojobot cannot say \
                             whether you have a session in flight, and has not started one — a \
                             fresh session here could fork one that is already running. It will \
                             try again on your first write. Everything else here is unaffected; \
                             the session verbs will say why.",
                }));
            }
        };
        // **The domain named them; the log is this layer's.** A stale session
        // the store refused to close is left active for the next boot, and the
        // boot itself carries on — but it must not go quiet, because nothing
        // else on the surface will ever mention it.
        for (session, e) in &unswept {
            tracing::warn!(
                error = %e, %session,
                "a stale session could not be swept — left active for the next boot"
            );
        }

        let mut block = match resume {
            // ── the caller answered the offer ───────────────────────────────
            Some(answer) if answer.eq_ignore_ascii_case(sid::NEW) => {
                let handle = self.mint_or_say_why(bot, None)?;
                // Bound to the identity with no session: the first write is
                // what begins the card, exactly as a first boot's is. **The run
                // that was offered is left running** — a new session never
                // closes an old one.
                self.fresh_block(handle)
            }
            Some(answer) => {
                let (handle, session) = self.resumable(bot, answer, &live).await?;
                match session {
                    Some(session) => {
                        let block = serde_json::json!({
                            "available": true,
                            "sid": handle.as_str(),
                            "resumed": true,
                            "session": session_json(&session),
                            "note": "you are resuming a session already in flight — its \
                                     chronology is above. Read it before you start: somebody \
                                     (you, before a disconnect) was part way through something.",
                        });
                        block
                    }
                    // A handle whose session was never written: theirs, still
                    // good, and still nothing behind it.
                    None => self.fresh_block(handle),
                }
            }
            // ── a first boot: the two branches ──────────────────────────────
            None if live.is_empty() && offerable.is_none() => {
                let handle = self.mint_or_say_why(bot, None)?;
                self.fresh_block(handle)
            }
            None => {
                // Every live run, then the one stop worth bringing up. The
                // abandoned one comes last because it is the weaker claim on
                // the caller's attention, not because it is worse.
                let offered: Vec<&Session> = live.iter().chain(offerable.iter()).collect();
                let mut choices = Vec::with_capacity(offered.len());
                for session in offered {
                    let handle = self.handle_for(bot, &session.id)?;
                    let mut choice = serde_json::json!({
                        "sid": handle.as_str(),
                        // **What it was working on is the whole point of the
                        // offer.** A bot may have several runs at once, and a
                        // list of opaque handles is not a choice anybody can
                        // make.
                        "working_on": session.focus,
                        // **Marked, never silently mixed in.** Not because a
                        // stop is worse — it is not a failure — but because
                        // "this one was never wrapped up" is what tells the
                        // caller which of these is still warm.
                        "state": session.state.as_token(),
                        "started_at": session.started_at.to_string(),
                        "last_beat": session.last_beat().to_string(),
                        "entry_count": session.entries.len(),
                    });
                    if session.state == SessionState::Abandoned
                        && let Some(obj) = choice.as_object_mut()
                    {
                        obj.insert(
                            "note".into(),
                            "this run stopped without being wrapped up — a disconnect, a closed \
                         laptop, an agent that moved on. Resuming it is ordinary: it reopens \
                         where it left off and its chronology continues."
                                .into(),
                        );
                    }
                    choices.push(choice);
                }
                // **Bound as it has always been, to the newest run.** This
                // round moves what the DOOR hands back; the write path still
                // resolves an unaddressed write to the live session, and
                // binding to nothing here would make the next bare write fork a
                // second card beside the one being offered.
                serde_json::json!({
                    "available": true,
                    // **No handle until the choice is answered.** Its absence
                    // is the question: there is more than one thing this boot
                    // could mean, and jojobot is not picking.
                    "sid": serde_json::Value::Null,
                    "resumed": false,
                    "session": serde_json::Value::Null,
                    "choices": choices,
                    "how_to_proceed": "This identity has work already in flight. Call start_here \
                                       again with resume: the sid of the run you are picking up — \
                                       read what it was working on above — or resume: \"new\" for \
                                       a fresh session. Nothing was closed and nothing was \
                                       written; choosing new leaves the runs above running.",
                })
            }
        };
        if let Some(obj) = block.as_object_mut() {
            obj.insert("swept".into(), swept.into());
        }
        Ok(block)
    }

    /// The block for a session with no card behind it yet — a first boot, or
    /// `new`, or a handle nothing has been written under.
    pub(crate) fn fresh_block(&self, handle: sid::Sid) -> serde_json::Value {
        serde_json::json!({
            "available": true,
            "sid": handle.as_str(),
            "resumed": false,
            "session": serde_json::Value::Null,
            "note": "a fresh session, and this is its sid. Nothing is written yet — the record \
                     begins on your first journal entry or the first write you make, so a boot \
                     that does nothing leaves nothing behind.",
        })
    }

    /// Read an answer to the offer: the handle it names, and the live session it
    /// addresses if it addresses one.
    ///
    /// **Four refusals, and none of them is a correction.** A handle jojobot
    /// could not have minted, one it is not holding, one that belongs to another
    /// identity, and one whose session is closed or gone from the board. Each is
    /// blocked in its own words, because a caller's next move differs in every
    /// case — and none is repaired into a nearby handle, which would be jojobot
    /// guessing which session somebody meant.
    pub(crate) async fn resumable(
        &self,
        bot: &EntityId,
        answer: &str,
        live: &[Session],
    ) -> Result<(sid::Sid, Option<Session>), CallToolResult> {
        if !sid::is_readable(answer) {
            return Err(handle_declined(
                answer,
                format!(
                    "No session was started. '{answer}' is not a handle jojobot mints — those are \
                 {} characters of 0-9 and a-z, with i, l, o and u left out because they read \
                 as one another. jojobot will not correct one, because correcting it means \
                 guessing which session you meant. Call start_here with your bot name and no \
                 resume to see what there is.",
                    jojobot_domain::session::SID_LEN,
                ),
            ));
        }
        let Some(held) = self.registry.lookup(answer) else {
            return Err(handle_declined(
                answer,
                format!(
                    "No session was started. That session is gone: '{answer}' is not a handle \
                 this jojobot is holding — a handle whose run never wrote a card has nothing \
                 to be recovered from. The work itself is untouched and still \
                 readable. Call start_here with your bot name again and take the offer it \
                 makes."
                ),
            ));
        };
        if held.bot != *bot {
            // Refused without naming whose it is, and without offering a way
            // to switch: disclosing the other identity would prime
            // identity-switching as something an agent might do. A handle
            // that is not yours is simply not yours; the way forward is your
            // own session.
            return Err(handle_declined(
                answer,
                format!(
                    "No session was started. The handle '{answer}' is not yours — a session is \
                     bound to its identity at boot and never switches. Call start_here as \
                     '{bot}' with no resume to see what is."
                ),
            ));
        }
        let handle = sid::Sid(answer.to_string());
        let Some(card) = held.card else {
            // Minted, never written under. Still theirs, still empty.
            return Ok((handle, None));
        };
        if let Some(session) = live.iter().find(|s| s.id == card) {
            return Ok((handle, Some(session.clone())));
        }
        // **Not among the live runs, so it stopped — and stopping is not the
        // end.** Reopening is what makes "resume last session" always work, and
        // it is bounded by nothing but the state the run reached: the offer's
        // age window governs what jojobot VOLUNTEERS, never what a handle
        // someone kept can still reach.
        match self.sessions.reopen(&card).await {
            Ok(session) => Ok((handle, Some(session))),
            // The one end that is the last word: a run which told its story
            // is over, and its chronology stands as the record of what
            // happened.
            Err(SessionError::Closed { state, .. }) => Err(handle_declined(
                answer,
                format!(
                    "No session was started. '{answer}' addresses a session that is {state} — its \
                 story has been told, so this end is the last word. Its chronology stands as \
                 the record of what happened. Call start_here with your bot name and no \
                 resume to begin the next run."
                ),
            )),
            Err(SessionError::UnknownSession { .. }) => Err(handle_declined(
                answer,
                format!(
                    "No session was started. '{answer}' is a handle jojobot is holding, but the \
                 session it addresses is not on the board any more. Nothing was changed. Call \
                 start_here with your bot name and no resume to see what is there."
                ),
            )),
            // **Degrades the way the rest of a boot degrades**: nothing was
            // changed, the caller is told plainly, and the underlying fault
            // goes to the log where an operator reads it — rather than a 500
            // that says nothing about what happened to the session.
            Err(e) => {
                tracing::warn!(error = %e, session = %card, "a session could not be reopened");
                Err(handle_declined(
                    answer,
                    format!(
                        "No session was started, and nothing was changed. '{answer}' addresses a \
                     session that stopped, and jojobot could not reopen it: the session store \
                     refused. This is not something your call can fix by being different — \
                     try again, and if it persists a person has to look at the store."
                    ),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;
    use jojobot_domain::session::Sid;

    /// **An anonymous boot is an orientation preview: nothing usable behind
    /// it.** The world and the snapshot, no identity, and above all no handle —
    /// a caller who was handed one would reasonably believe it addressed
    /// something, and there is nothing for it to address.
    #[tokio::test]
    async fn an_anonymous_boot_hands_back_no_handle_at_all() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        make_bot(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    skill: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(
            body["identity"].is_null(),
            "no identity was claimed: {body}"
        );
        assert!(
            body["session"].is_null(),
            "and no session was begun: {body}"
        );
        // Asserted over the whole payload, not over the one key it would sit
        // on: a handle smuggled anywhere in this answer is a handle a caller
        // will try to use.
        assert!(
            !body.to_string().contains("\"sid\""),
            "an anonymous boot carries no handle anywhere: {body}"
        );
    }

    /// **Nothing to resume, so the handle comes back immediately.** There is no
    /// moment between "I am gamma" and "gamma is working", and a boot that made
    /// the caller ask a second time for the address would invent one.
    #[tokio::test]
    async fn a_boot_with_nothing_to_resume_hands_back_a_handle_at_once() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let body = boot(&jojobot, "gamma").await;
        let handle = sid_of(&body).unwrap_or_else(|| panic!("a handle comes back: {body}"));
        assert!(
            sid::is_readable(&handle),
            "…and it is a readable one: {handle}"
        );
        assert_eq!(body["session"]["resumed"], false);
        assert!(
            body["session"]["choices"].is_null(),
            "there was nothing to choose: {body}"
        );

        // **The card stays lazy.** A boot that does nothing leaves nothing
        // behind, handle or no handle.
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "the handle is minted here; the card waits for the first write"
        );
    }

    /// Something to resume, so the choice comes first and the handle waits.
    /// Never attach silently — that decides for the caller. Each option is
    /// named by what it was working on, because that is the only thing that
    /// tells two runs of one identity apart.
    #[tokio::test]
    async fn a_resumable_session_comes_back_as_a_choice_and_no_handle() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        // A distinct handle per run — see the note in the two-handles test: one
        // `line!()` inside a loop is one handle, and two runs cannot share one.
        for (nth, focus) in ["reading the hand-off", "chasing the flaky test"]
            .into_iter()
            .enumerate()
        {
            store
                .begin(NewSession {
                    bot: EntityId("bot:gamma".into()),
                    sid: fixture_sid(line!() + nth as u32),
                    focus: focus.into(),
                    started_at: jiff::Timestamp::now(),
                })
                .await
                .expect("begin ok");
        }

        let body = boot(&jojobot, "gamma").await;
        assert!(
            sid_of(&body).is_none(),
            "the handle arrives with the answer, not before it: {body}"
        );

        let choices = body["session"]["choices"]
            .as_array()
            .expect("the offer is a list");
        assert_eq!(
            choices.len(),
            2,
            "a bot may have several runs at once: {body}"
        );
        let mut working_on: Vec<&str> = choices
            .iter()
            .map(|c| c["working_on"].as_str().expect("what it was working on"))
            .collect();
        working_on.sort_unstable();
        assert_eq!(
            working_on,
            ["chasing the flaky test", "reading the hand-off"]
        );
        for choice in choices {
            let handle = choice["sid"].as_str().expect("each option is addressable");
            assert!(
                sid::is_readable(handle),
                "{handle} is not a readable handle"
            );
        }
        assert!(
            body["session"]["how_to_proceed"]
                .as_str()
                .is_some_and(|h| h.contains("resume") && h.contains("new")),
            "…and the way to answer is stated: {body}"
        );
    }

    /// Answering it: resume returns that session's handle and its chronology;
    /// choosing new returns a different handle and leaves the old run alone.
    #[tokio::test]
    async fn resuming_returns_that_session_s_handle_and_new_returns_another() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let begun = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "reading the hand-off".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");

        let offered = boot(&jojobot, "gamma").await;
        let offer = offered["session"]["choices"][0]["sid"]
            .as_str()
            .expect("one option")
            .to_string();

        let resumed = boot_answering(&jojobot, "gamma", &offer).await;
        assert_eq!(
            sid_of(&resumed).as_deref(),
            Some(offer.as_str()),
            "{resumed}"
        );
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(
            resumed["session"]["session"]["focus"], "reading the hand-off",
            "resuming hands back the run itself, chronology and all: {resumed}"
        );

        // **The offer is stable**: the same card keeps the handle it was first
        // given, so a caller who boots twice before answering is not looking at
        // two addresses for one run.
        assert_eq!(
            boot(&jojobot, "gamma").await["session"]["choices"][0]["sid"],
            offer.as_str()
        );

        let fresh = boot_answering(&jojobot, "gamma", sid::NEW).await;
        let minted = sid_of(&fresh).unwrap_or_else(|| panic!("new mints one: {fresh}"));
        assert_ne!(
            minted, offer,
            "choosing new is a different session: {fresh}"
        );
        assert_eq!(fresh["session"]["resumed"], false);

        // **Nothing auto-wrapped.** A new session never closes an old one.
        let all = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(all.len(), 1, "and new stays lazy until it writes: {all:?}");
        assert_eq!(all[0].id, begun.id);
        assert_eq!(
            all[0].state,
            SessionState::Active,
            "the old run is untouched"
        );
    }

    /// **A handle survives the process that minted it, because the card holds
    /// it.** This is the restart cliff closed: the registry is rebuilt from the
    /// board before anything is served, so the handle a caller wrote down
    /// yesterday still addresses its run today.
    ///
    /// It matters beyond convenience. The sid is the address every later verb
    /// carries, so a handle that died with the process meant a deploy silently
    /// re-pointed every agent at nothing.
    #[tokio::test]
    async fn a_handle_written_on_the_card_survives_a_restart() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection_sharing(
            memory.clone(),
            store.clone(),
            Arc::new(sid::SessionRegistry::new()),
        );
        seed_bot(&memory, "gamma").await;

        let handle = sid_of(&boot(&jojobot, "gamma").await).expect("a handle");
        journal_entry(&jojobot, &handle, "read the hand-off").await;
        let card = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok")
            .into_iter()
            .next()
            .expect("a card");
        assert_eq!(
            card.sid.as_ref().map(|s| s.as_str()),
            Some(handle.as_str()),
            "the card carries the handle the door handed out: {card:?}"
        );

        // A restart: same board, an empty registry, filled from the board before
        // the first request — exactly what the composition root does.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        let board = store.all_sessions().await.expect("board read ok");
        assert_eq!(rebuilt.rebuild_from(&board), 1, "one handle recovered");
        let restarted = connection_sharing(memory, store.clone(), rebuilt);

        let resumed = boot_answering(&restarted, "gamma", &handle).await;
        assert_eq!(
            sid_of(&resumed).as_deref(),
            Some(handle.as_str()),
            "the same handle, still addressing the same run: {resumed}"
        );
        assert_eq!(resumed["session"]["session"]["id"], card.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["chronology"][0]["text"],
            "read the hand-off"
        );
    }

    /// **A card is born with the handle its caller is holding**, on a client
    /// with no session affinity — which is every real client.
    ///
    /// One round ago this was the gap: the write arrived carrying a bot name and
    /// nothing else, so jojobot minted the card a handle of its own rather than
    /// guessing which of possibly several booted agents was writing, and the
    /// caller's own handle stayed card-less. The sid rides the write now, so
    /// there is nothing to guess and the two are the same handle.
    #[tokio::test]
    async fn a_card_is_born_with_the_handle_its_caller_is_holding() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma").await;
        let door_gave = sid_of(&boot(&client.call(), "gamma").await).expect("a handle");

        json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off".into(),
                    focus: None,
                    sid: door_gave.clone(),
                }))
                .await
                .expect("journal ok"),
        );

        let card = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok")
            .into_iter()
            .next()
            .expect("a card");
        let stored = card.sid.as_ref().expect("a card is never born handle-less");
        assert!(sid::is_readable(stored.as_str()));
        assert_eq!(
            stored.as_str(),
            door_gave.as_str(),
            "…and it is the caller's OWN handle, because the sid rides the write: jojobot no \
             longer has to guess which of several booted agents is writing"
        );

        // The card's own handle is what survives a restart and addresses the run.
        //
        // **Counted over THIS bot's cards, not the whole board.** Standing the
        // fixture bot up is itself an attributed write now, so it materializes
        // a card of its own — a raw board count would be measuring the fixture
        // as much as the subject.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        let board = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("read ok");
        assert_eq!(rebuilt.rebuild_from(&board), 1);
        assert_eq!(
            rebuilt.lookup(stored.as_str()).expect("held").card,
            Some(card.id.clone())
        );
    }

    /// **A card written before handles were persisted carries none**, and that
    /// is not a broken card: the boot that offers it mints one on the spot. The
    /// migration is a no-op *only because* minting-on-offer already exists —
    /// stated here so nobody later "simplifies" the offer into requiring a
    /// stored handle.
    #[tokio::test]
    async fn a_card_with_no_stored_handle_is_offered_one_on_the_spot() {
        let store = Arc::new(InMemorySessions::new());
        let registry = Arc::new(sid::SessionRegistry::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection_sharing(memory.clone(), store.clone(), registry.clone());
        seed_bot(&memory, "gamma").await;

        let legacy = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t900".into()),
                focus: "from before handles were stored".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        // Strip the handle, which is what an older jojobot's card looks like.
        store.forget_sid(&legacy.id);
        let board = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("read ok");
        assert_eq!(
            registry.rebuild_from(&board),
            0,
            "a card with no handle contributes none"
        );

        let offered = boot(&jojobot, "gamma").await;
        let choice = &offered["session"]["choices"][0];
        let minted = choice["sid"]
            .as_str()
            .expect("a handle, minted on the spot");
        assert!(sid::is_readable(minted));
        assert_eq!(choice["working_on"], "from before handles were stored");

        let resumed = boot_answering(&jojobot, "gamma", minted).await;
        assert_eq!(resumed["session"]["session"]["id"], legacy.id.as_str());
    }

    /// **A handle that never reached a card does not survive a restart, and
    /// says so** — even though the restart rebuilt everything it could.
    ///
    /// A card is written lazily, so a boot that did no work leaves the handle
    /// with nothing behind it, and nothing behind it is nothing to rebuild
    /// FROM: `rebuild_from` reads handles off the cards on the board, and this
    /// handle is on no card. It comes back blocked, which is not a 404 from the
    /// store — the store was never asked — and above all not a silent new
    /// session, which would leave a caller writing into a run they did not mean
    /// under an id they think they know.
    ///
    /// **Age is not what blocks it.** The old name here said "a handle from
    /// before a restart", which is the opposite of the spec: a pre-restart
    /// handle whose card exists RESOLVES, and its sibling
    /// `a_handle_written_on_the_card_survives_a_restart` is what pins that. The
    /// rebuild is run here rather than skipped so the two cases are told apart
    /// by the thing that actually decides them.
    #[tokio::test]
    async fn a_handle_that_never_reached_a_card_is_blocked_after_a_rebuild() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma").await;
        let handle = sid_of(&boot(&client.call(), "gamma").await).expect("a handle");

        // Same stores, new process: the registry is what a restart empties, and
        // filling it back from the board is what a restart then does.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        // Scoped to this bot, for the same reason as above: the fixture
        // bot's own card would otherwise be counted.
        let board = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("read ok");
        assert_eq!(
            rebuilt.rebuild_from(&board),
            0,
            "the boot wrote no card, so the rebuild has nothing to recover: {board:?}"
        );
        let restarted = Jojobot::new(
            client.memory.clone(),
            Arc::new(SpySearch::default()),
            client.mailboxes.clone(),
            client.sessions.clone(),
            rebuilt,
        );
        let body = blocked(
            &restarted
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    resume: Some(handle.clone()),
                }))
                .await
                .expect("a dead handle is an answer, not a protocol failure"),
        );
        assert_eq!(body["attempted"], handle);
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("gone") && how.contains("start_here"),
            "that session is gone; boot again: {how}"
        );

        // An unreadable handle is refused too — never repaired into a near one.
        let mistyped = blocked(
            &restarted
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    resume: Some("k3fo".into()),
                }))
                .await
                .expect("an unreadable handle is an answer too"),
        );
        assert_eq!(mistyped["attempted"], "k3fo");
    }

    /// **A handle is bound to its identity at boot and never switches.** Naming
    /// somebody else's session is refused rather than quietly honoured — the
    /// whole bug class deleted instead of guarded against downstream.
    #[tokio::test]
    async fn a_handle_belonging_to_another_identity_is_refused() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store);
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

        let gammas = sid_of(&boot(&jojobot, "gamma").await).expect("a handle");
        let body = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("delta".into()),
                    brief: None,
                    skill: None,
                    resume: Some(gammas.clone()),
                }))
                .await
                .expect("somebody else's handle is an answer, not a protocol failure"),
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        // It refuses without naming whose it is, and without offering a way
        // to switch: disclosing the other identity would prime
        // identity-switching, which rule 22 forbids. A session is bound to
        // its identity at boot and never switches; the way forward is the
        // caller's own session, not somebody else's.
        assert!(
            !advice.contains("gamma"),
            "the refusal must not disclose the other identity: {advice}"
        );
        assert!(
            !advice.to_lowercase().contains("boot as"),
            "…nor offer booting as it: {advice}"
        );
        assert!(
            advice.contains("delta"),
            "…and it still points the caller at what IS theirs: {advice}"
        );
    }

    /// **The handle says nothing about the work.** Two runs of one identity on
    /// the same focus get different handles, and no handle carries anything
    /// derived from what its session is doing.
    #[tokio::test]
    async fn two_sessions_on_one_focus_get_different_and_opaque_handles() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let focus = "chasing the flaky test";
        // **A distinct handle per run, and the `nth` is why.** `fixture_sid`
        // was called with `line!()` inside this loop, so both iterations asked
        // for the SAME handle — and the store let a second run be created under
        // it, which is the fork batch 2 removed. The test passed for the wrong
        // reason: it was asserting two handles differ while setting up one.
        for nth in 0..2 {
            store
                .begin(NewSession {
                    bot: EntityId("bot:gamma".into()),
                    sid: fixture_sid(line!() + nth),
                    focus: focus.into(),
                    started_at: jiff::Timestamp::now(),
                })
                .await
                .expect("begin ok");
        }

        let offered = boot(&jojobot, "gamma").await;
        let handles: Vec<&str> = offered["session"]["choices"]
            .as_array()
            .expect("the offer")
            .iter()
            .map(|c| c["sid"].as_str().expect("a handle"))
            .collect();
        assert_eq!(handles.len(), 2);
        assert_ne!(handles[0], handles[1], "identical work, different handles");

        for handle in &handles {
            assert!(sid::is_readable(handle));
            // Nothing of the focus survives into the handle: not a slug, not a
            // word, not even a run of three of its characters.
            let slug = focus.to_lowercase();
            for window in slug.as_bytes().windows(3) {
                let fragment = String::from_utf8(window.to_vec()).expect("ascii");
                assert!(
                    !handle.contains(&fragment),
                    "{handle} carries {fragment:?} out of the focus it is for"
                );
            }
        }
    }

    /// **An abandoned run is picked up, not recovered from.** It stopped without
    /// telling its story — a disconnect, a closed laptop — so the boot offers it
    /// back, resuming REOPENS it, and the record continues where it stopped
    /// instead of starting again beside it.
    ///
    /// Without this, an interrupted run could never be wrapped at all: the verb
    /// that tells the story refuses a closed session, so the story was lost by
    /// construction.
    #[tokio::test]
    async fn resuming_an_abandoned_run_reopens_it_and_continues_the_record() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let stopped = abandoned_run(&store, "gamma", "reading the hand-off", 30).await;

        let offered = boot(&jojobot, "gamma").await;
        assert!(
            sid_of(&offered).is_none(),
            "there is something to choose: {offered}"
        );
        let choice = &offered["session"]["choices"][0];
        assert_eq!(choice["working_on"], "reading the hand-off");
        assert_eq!(
            choice["state"], "abandoned",
            "**marked, never silently mixed in with the live runs**: {offered}"
        );

        let resumed = boot_answering(
            &jojobot,
            "gamma",
            choice["sid"].as_str().expect("an addressable option"),
        )
        .await;
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(resumed["session"]["session"]["id"], stopped.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["state"], "active",
            "resuming reopens it — it is running again: {resumed}"
        );
        let sid = sid_of(&resumed).expect("the resumed handle");

        // The proof it meant something: the write that would have been refused
        // a moment ago lands, on the same record.
        journal_entry(&jojobot, &sid, "picked it back up").await;
        let read = store.read_session(&stopped.id).await.expect("read ok");
        assert_eq!(read.state, SessionState::Active);
        assert_eq!(
            read.entries.last().expect("an entry").text,
            "picked it back up"
        );
        let all = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(all.len(), 1, "continued, not forked beside: {all:?}");
    }

    /// **Bounded attention, unbounded reachability.** A run nobody has touched
    /// in months is not something to bring up — but a handle its caller still
    /// holds still addresses it, and resuming it still works.
    #[tokio::test]
    async fn an_old_abandoned_run_is_not_offered_and_is_still_resumable() {
        let store = Arc::new(InMemorySessions::new());
        let registry = crate::harness::seeded_registry();
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection_sharing(memory.clone(), store.clone(), registry.clone());
        seed_bot(&memory, "gamma").await;
        let ancient = abandoned_run(&store, "gamma", "something from last winter", 24 * 240).await;

        let booted = boot(&jojobot, "gamma").await;
        assert!(
            booted["session"]["choices"].is_null(),
            "nothing recent enough to offer, so the sid comes back at once: {booted}"
        );
        assert!(sid_of(&booted).is_some());

        // The caller kept the handle from when this process issued it.
        let held = registry
            .for_card(&EntityId("bot:gamma".into()), &ancient.id)
            .expect("a handle");
        let resumed = boot_answering(&jojobot, "gamma", held.as_str()).await;
        assert_eq!(
            resumed["session"]["resumed"], true,
            "age bounds what is volunteered, never what a handle reaches: {resumed}"
        );
        assert_eq!(resumed["session"]["session"]["id"], ancient.id.as_str());
        assert_eq!(resumed["session"]["session"]["state"], "active");
    }

    /// The offer reaches **at most one** abandoned run — the most recent — while
    /// every live run is offered. One is a memory jog; a list of them is a
    /// history nobody asked for.
    #[tokio::test]
    async fn the_offer_carries_every_live_run_and_only_the_newest_abandoned_one() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        abandoned_run(&store, "gamma", "the oldest stop", 100).await;
        abandoned_run(&store, "gamma", "the middle stop", 70).await;
        abandoned_run(&store, "gamma", "the newest stop", 40).await;
        store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "still going".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");

        let offered = boot(&jojobot, "gamma").await;
        let choices = offered["session"]["choices"].as_array().expect("the offer");
        let shown: Vec<(&str, &str)> = choices
            .iter()
            .map(|c| {
                (
                    c["working_on"].as_str().expect("a focus"),
                    c["state"].as_str().expect("a state"),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [("still going", "active"), ("the newest stop", "abandoned")],
            "every live run, and only the most recent stop: {offered}"
        );
    }

    /// **A wrapped run is over, both in the offer and by handle.** It told its
    /// story and ended; reopening it would reopen something that said it was
    /// finished.
    #[tokio::test]
    async fn a_wrapped_run_is_never_offered_and_never_reopens() {
        let store = Arc::new(InMemorySessions::new());
        let registry = crate::harness::seeded_registry();
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection_sharing(memory.clone(), store.clone(), registry.clone());
        seed_bot(&memory, "gamma").await;

        let told = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(2),
            })
            .await
            .expect("begin ok");
        store
            .close(&told.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let booted = boot(&jojobot, "gamma").await;
        assert!(
            booted["session"]["choices"].is_null(),
            "a told story is not on offer: {booted}"
        );

        let held = registry
            .for_card(&EntityId("bot:gamma".into()), &told.id)
            .expect("a handle");
        let refused = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    resume: Some(held.as_str().into()),
                }))
                .await
                .expect("a wrapped run is an answer, not a protocol failure"),
        );
        let how = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("wrapped") && how.contains("story"),
            "the refusal says why this end is the last word: {how}"
        );
        assert_eq!(
            store.read_session(&told.id).await.expect("read ok").state,
            SessionState::Wrapped,
            "and nothing moved"
        );
    }

    /// **A reconnect is OFFERED the work in flight.** A session is the unit of
    /// work, not of connection, so a second boot of the same identity finds the
    /// live run and hands back its chronology rather than forking a new one —
    /// which is the whole reason a device hop is survivable.
    ///
    /// It is offered rather than attached: the run comes back as a choice named
    /// by what it was working on, and resuming it is the caller's answer. The
    /// difference matters most for the case the offer exists for — a run left
    /// open on purpose, for somebody who has not arrived yet.
    #[tokio::test]
    async fn booting_again_is_offered_the_session_in_flight() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let registry = crate::harness::seeded_registry();
        let first = connection_sharing(memory.clone(), store.clone(), registry.clone());
        make_bot(&first, "gamma").await;
        let sid = booted(&first, "gamma").await;
        let started = journal_entry(&first, &sid, "read the hand-off").await;

        // A different connection over the same worlds, exactly as a reconnect
        // builds one — a fresh binding, so anything it knows it read.
        let second = connection_sharing(memory, store.clone(), registry);
        let offered = boot(&second, "gamma").await;
        assert!(
            sid_of(&offered).is_none(),
            "the choice comes first: {offered}"
        );
        let choice = &offered["session"]["choices"][0];
        assert_eq!(choice["working_on"], "read the hand-off");

        let resumed = boot_answering(
            &second,
            "gamma",
            choice["sid"].as_str().expect("an addressable option"),
        )
        .await;
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(resumed["session"]["session"]["id"], started["session"]);
        assert_eq!(
            resumed["session"]["session"]["chronology"][0]["text"], "read the hand-off",
            "the work in flight comes back with it: {resumed}"
        );

        // …and writing on the new connection continues the same session.
        let again = sid_of(&resumed).expect("the resumed handle");
        journal_entry(&second, &again, "picked it back up").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "one session, not two: {live:?}");
        assert_eq!(live[0].entries.len(), 2);
    }

    /// **The sweep, and what it is measured from.** A session that has gone a
    /// day without a beat is closed as `abandoned` at the next boot of its bot —
    /// never deleted, never wrapped, because its story was never told.
    ///
    /// **And the same boot offers it straight back**, which is not a
    /// contradiction: sweeping records that the run stopped, and the offer is
    /// how "resume last session" reaches it. A run that stopped yesterday is the
    /// archetypal thing a returning agent means, so closing it and then hiding
    /// it would make the sweep a way of losing work rather than of marking it.
    #[tokio::test]
    async fn a_stale_session_is_swept_to_abandoned_at_the_next_boot() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        // Begun two days ago and never touched since.
        let stale = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "something from the day before yesterday".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
            })
            .await
            .expect("begin ok");

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(
            booted["session"]["swept"],
            serde_json::json!([stale.id.as_str()]),
            "the boot says what it closed: {booted}"
        );
        assert_eq!(
            booted["session"]["resumed"], false,
            "sweeping resumes nothing by itself — the caller still chooses"
        );

        let read = store.read_session(&stale.id).await.expect("read ok");
        assert_eq!(read.state, mailbox_state_abandoned(), "closed, not deleted");
        assert_eq!(
            read.focus, "something from the day before yesterday",
            "…and its record is untouched"
        );

        // **The run this very boot swept is the one it offers back.** It
        // stopped the day before yesterday, which is exactly the run a
        // returning agent means by "resume last session".
        let choice = &booted["session"]["choices"][0];
        assert_eq!(
            choice["state"], "abandoned",
            "offered, and marked: {booted}"
        );
        assert_eq!(
            choice["working_on"],
            "something from the day before yesterday"
        );

        let resumed = boot_answering(
            &jojobot,
            "gamma",
            choice["sid"].as_str().expect("an addressable option"),
        )
        .await;
        assert_eq!(resumed["session"]["session"]["id"], stale.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["state"], "active",
            "…and taking the offer reopens it: {resumed}"
        );
    }

    /// A session that is merely quiet — an hour, not a day — is still yours, and
    /// being offered it back is the point.
    #[tokio::test]
    async fn a_recent_session_is_offered_back_rather_than_swept() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let recent = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "still going".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(1),
            })
            .await
            .expect("begin ok");

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(booted["session"]["choices"][0]["working_on"], "still going");
        assert_eq!(booted["session"]["swept"], serde_json::json!([]));

        let resumed = boot_answering(
            &jojobot,
            "gamma",
            booted["session"]["choices"][0]["sid"]
                .as_str()
                .expect("an option"),
        )
        .await;
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(resumed["session"]["session"]["id"], recent.id.as_str());
    }

    /// **The acceptance case: "start jojobot as the PM" and the session knows
    /// who it is.** One call answers all of it — the world (the same orientation
    /// an anonymous session gets), what exists, and *which identity this is*:
    /// the charter, the rules with their provenance showing, and the state of
    /// the box whose mail is this bot's.
    #[tokio::test]
    async fn booting_lands_a_session_knowing_which_identity_it_is() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        send(&jojobot, "otto", "epsilon", "the shipment landed").await;

        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "otto".into(),
                prose: "Keeps the schedule.\n\nHard line: never writes to the ledger.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");
        jojobot
            .capture(Parameters(CaptureArgs {
                provenance: Some("testimony".into()),
                ..capture_args("bot:otto", "answers before noon")
            }))
            .await
            .expect("capture ok");

        let body = boot(&jojobot, "otto").await;
        assert_ne!(body["status"], "blocked", "a bot that exists boots: {body}");

        // The world, and what is in it — everything start_here hands over.
        assert!(
            body["orientation"]
                .as_str()
                .is_some_and(|o| o.contains("provenance"))
        );
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["bot"], 1);

        let me = &body["identity"];
        assert_eq!(me["bot"]["id"], "bot:otto");
        assert_eq!(me["bot"]["type"], "SoftwareApplication");
        assert!(
            me["charter"]
                .as_str()
                .is_some_and(|c| c.contains("never writes to the ledger")),
            "the charter is the orienting text, and it arrives: {me}"
        );

        let rules = me["rules"].as_array().expect("rules are a list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["content"], "answers before noon");
        assert_eq!(
            rules[0]["provenance"], "testimony",
            "a rule arrives with its provenance showing, or it reads as settled when it is a guess"
        );
        assert!(
            rules[0]["address"].as_str().is_some(),
            "and with the address that edits it"
        );

        let owned = &me["owned_mailbox"];
        assert_eq!(owned["name"], "otto", "a bot's box is named for it");
        assert_eq!(
            owned["counts"]["new"], 1,
            "the state of its own box: {owned}"
        );
        assert_eq!(owned["available"], true, "the box is there and says so");
    }

    /// **A boot writes nothing a concurrent first write can lose.** A boot reads
    /// the board, sweeps what is stale and answers; a write on a handle already
    /// held reads that handle's card and begins one if there is none. The two
    /// overlap: sweeping a stale card is an await sitting inside the boot's
    /// board read, and that is exactly when the racing write gets to run.
    ///
    /// The boot writes no identity anywhere a write reads from, so there is
    /// nothing for it to clobber. What is pinned here: whatever the
    /// interleaving between a boot and a racing first write, the handle keeps
    /// addressing one card and the next write keeps accruing to it. The
    /// remaining overlap between the two — a boot reading the board inside
    /// the gap a first write leaves — is a different defect with its own
    /// test below.
    ///
    /// **Both orders, because only one of them forked.** `tokio::join!` rotates
    /// which future it polls first, so a single ordering proves whichever
    /// interleaving it happened to produce; the invariant is that neither
    /// produces two cards.
    #[tokio::test]
    async fn a_racing_boot_writes_nothing_the_first_write_can_lose() {
        for boot_first in [true, false] {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = racing(store.clone());
            make_bot(&jojobot, "gamma").await;
            let sid = booted(&jojobot, "gamma").await;

            // Something for the racing boot to sweep. Closing it is an await
            // inside the boot's board read — the gap the racing write slips
            // through.
            store
                .begin(NewSession {
                    bot: EntityId("bot:gamma".into()),
                    sid: fixture_sid(line!()),
                    focus: "from the day before yesterday".into(),
                    started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                })
                .await
                .expect("begin ok");

            let booting = jojobot.start_here(Parameters(OrientArgs {
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                resume: None,
            }));
            let writing = jojobot.journal(Parameters(JournalArgs {
                entry: "the first beat".into(),
                focus: None,
                sid: sid.clone(),
            }));
            if boot_first {
                let (b, w) = tokio::join!(booting, writing);
                b.expect("boot ok");
                w.expect("journal ok");
            } else {
                let (w, b) = tokio::join!(writing, booting);
                b.expect("boot ok");
                w.expect("journal ok");
            }

            // The next write must continue that session rather than mint a second.
            journal_entry(&jojobot, &sid, "the second beat").await;

            let live: Vec<Session> = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .into_iter()
                .filter(|s| !s.state.is_terminal())
                .collect();
            assert_eq!(
                live.len(),
                1,
                "boot_first={boot_first}: one card, not one per racing boot: {live:?}"
            );
            assert_eq!(
                live[0].entries.len(),
                2,
                "boot_first={boot_first}: …and it kept accruing: {:?}",
                live[0].entries
            );
        }
    }

    /// **One run answers to one handle, even when a boot reads the board in the
    /// middle of the write that creates it.**
    ///
    /// A first write begins the card and then tells the registry which handle it
    /// landed on, and those two are not one step: the card is on the board the
    /// moment the store commits it, and the registry learns of it only when the
    /// write's own future is polled again. A boot reading the board inside that
    /// gap finds a live run no handle addresses and mints a second one for it —
    /// so the offer names an address the run's own writer has never heard of,
    /// and one session answers to two names. That is the fork the per-run gate
    /// exists to prevent, one layer up.
    ///
    /// **The gate has to be keyed on the identity rather than the handle**,
    /// because that is the only key the two callers share: the boot knows the
    /// bot, the write knows its sid, and they are talking about the same run.
    /// Keying the boot on the bot and the write on its handle put them in
    /// different queues, which is a lock that excludes the pair it was for.
    ///
    /// **Both orders, and only one of them forks.** Polled boot-first, the board
    /// read lands before the card exists and the boot legitimately hands back a
    /// fresh handle with nothing behind it; polled write-first, the boot reads
    /// inside the gap. `tokio::join!` rotates which future it polls first, so a
    /// single ordering proves only whichever it happened to produce.
    #[tokio::test]
    async fn a_boot_reading_the_board_mid_write_offers_the_handle_the_run_has() {
        for boot_first in [true, false] {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = racing(store.clone());
            make_bot(&jojobot, "gamma").await;
            let sid = booted(&jojobot, "gamma").await;

            let booting = jojobot.start_here(Parameters(OrientArgs {
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                resume: None,
            }));
            let writing = jojobot.journal(Parameters(JournalArgs {
                entry: "the first beat, which is what mints the card".into(),
                focus: None,
                sid: sid.clone(),
            }));
            let booted_answer = if boot_first {
                let (b, w) = tokio::join!(booting, writing);
                w.expect("journal ok");
                json_of(&b.expect("boot ok"))
            } else {
                let (w, b) = tokio::join!(writing, booting);
                w.expect("journal ok");
                json_of(&b.expect("boot ok"))
            };

            // A boot that saw the card offers it back. Whether it saw one is the
            // interleaving's business; what it may never do is offer it under a
            // handle minted beside the one its writer is already using.
            if let Some(choices) = booted_answer["session"]["choices"].as_array() {
                for choice in choices {
                    assert_eq!(
                        choice["sid"].as_str(),
                        Some(sid.as_str()),
                        "boot_first={boot_first}: the offer minted a second handle for a run that \
                         already has one: {choice}"
                    );
                }
            }
        }
    }

    /// **A resume comes back readable.** A chronology grows with every beat and
    /// nothing bounded it, so the longer a run was worth resuming the more
    /// certainly its own boot was a payload the caller could not read — and a
    /// response the caller cannot read is a failed response.
    ///
    /// The tail is what is kept: a resuming run reads the newest beats first,
    /// and the oldest are the ones it is least likely to need.
    ///
    /// **The positive comes first and the negative depends on it**: the newest
    /// beat is present and the kept entries are the run of beats that ends at
    /// it, in order. "Fewer than all" asserted alone would pass on an empty
    /// chronology.
    #[tokio::test]
    async fn a_resumed_boot_carries_the_newest_beats_and_says_what_it_left_out() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        // Dense beats, the shape a real chronology has: a resume note runs to
        // thousands of characters, which is why a cap counted in entries would
        // bound nothing.
        for nth in 0..20 {
            journal_entry(
                &jojobot,
                &sid,
                &format!("beat {nth:02} {}", "w".repeat(1500)),
            )
            .await;
        }

        let resumed = boot_answering(&jojobot, "gamma", &sid).await;
        let session = &resumed["session"]["session"];
        let kept: Vec<&str> = session["chronology"]
            .as_array()
            .expect("a chronology")
            .iter()
            .map(|e| e["text"].as_str().expect("an entry's text"))
            .collect();

        assert!(
            kept.last()
                .expect("the newest beat is what a resume is for")
                .starts_with("beat 19"),
            "the newest beat is the one that must survive: {kept:?}"
        );
        let oldest_kept = 20 - kept.len();
        assert!(
            kept[0].starts_with(&format!("beat {oldest_kept:02}")),
            "the kept beats are the tail, in order: {kept:?}"
        );
        assert!(
            kept.len() < 20,
            "…and it is a tail rather than the whole record: {} entries kept",
            kept.len()
        );

        // The elision is stated, and the record's own size is not restated as
        // the number served: a reader has to be able to tell how much it is
        // not looking at.
        assert_eq!(session["chronology_elided"], true, "{session}");
        assert_eq!(
            session["entry_count"], 20,
            "entry_count is the whole record: {session}"
        );
        assert_eq!(
            session["entries_omitted"],
            (20 - kept.len()) as u64,
            "what was left out is counted: {session}"
        );
        // The note is about THIS elision, not a fixed sentence: it names the
        // number that was dropped. Its wording is not pinned — that would break
        // the day somebody improves it and would prove nothing about behaviour.
        let note = session["chronology_note"]
            .as_str()
            .expect("an elision says what it did");
        assert!(
            note.contains(&(20 - kept.len()).to_string()),
            "the note names how much is missing: {note}"
        );

        // **And the payload is one a client can read**, which is the whole
        // reason for the cap.
        assert!(
            resumed.to_string().len() < 40_000,
            "a resumed boot is {} characters",
            resumed.to_string().len()
        );
    }

    /// …and a chronology that fits comes back whole, with the marker saying so.
    /// Without this, the test above passes on a build that serves one entry and
    /// calls the rest elided.
    #[tokio::test]
    async fn a_short_chronology_comes_back_whole() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        for nth in 0..3 {
            journal_entry(&jojobot, &sid, &format!("beat {nth}")).await;
        }

        let session = boot_answering(&jojobot, "gamma", &sid).await["session"]["session"].clone();
        let kept: Vec<&str> = session["chronology"]
            .as_array()
            .expect("a chronology")
            .iter()
            .map(|e| e["text"].as_str().expect("an entry's text"))
            .collect();
        assert_eq!(kept, ["beat 0", "beat 1", "beat 2"], "{session}");
        assert_eq!(session["chronology_elided"], false, "{session}");
        assert!(
            session["entries_omitted"].is_null(),
            "nothing was left out, so there is no count to report: {session}"
        );
        assert!(session["chronology_note"].is_null(), "{session}");
    }
}

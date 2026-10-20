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
        // **What this boot is an answer to.** Only the answer to a live offer
        // leaves out the handover story the offering call shipped.
        answering: Answering,
        // **The zone this run resolves days in**, already validated by the
        // door: an unresolvable name never reaches here. `None` is a boot that
        // supplied none, and on a RESUME that means keep what the run carries
        // rather than reframing it — a caller that said nothing has not asked
        // for the fallback, and silently moving a running session's frame is
        // the one thing this must not do.
        timezone: Option<&str>,
        today: Option<jiff::civil::Date>,
        // **The moment an offer made by this call is cut off at**, taken by the
        // door before the rules were read.
        offered_at: jiff::Timestamp,
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
        // **A wrap code is its own route and runs BEFORE the sweep.** It
        // reopens one wrapped run for one last change, so it abandons no other
        // run of the bot and mints no handle: the sweep is for a boot that
        // starts or resumes a live run.
        if let Some(answer) = resume
            && jojobot_domain::session::is_wrap_code(answer)
        {
            return self.resume_by_wrap_code(bot, answer).await;
        }
        // The clock is read HERE and handed down: the sweep is domain policy
        // and the domain is clock-free, so the instant it decides against is
        // stamped at the edge exactly as a capture's date is.
        // **The SERVER's now, which is not always the real one.** A server
        // acting out a day decides against that day, so this comparison and
        // the moments the runs it is judging were stamped with are on one
        // clock. The day itself arrives from the door.
        let swept_at = self.clock().now();
        let Board {
            live,
            offerable,
            handover,
            swept,
            unswept,
            windows_open,
        } = match sweep_and_find(self.sessions.as_ref(), bot, swept_at, today).await {
            Ok(found) => found,
            // **No handle is minted.** One handed out here would address
            // either a fresh session or one already running, and jojobot
            // cannot say which — so it hands out none and says so. A run the
            // store cannot read is told apart: it is a person's to repair.
            Err(e) => {
                return crate::session::window::board_unread(
                    &e,
                    bot,
                    "the session world is not reachable right now, so jojobot cannot say \
                     whether you have a session in flight, and has not started one — a \
                     fresh session here could fork one that is already running. It will \
                     try again on your first write. Everything else here is unaffected; \
                     the session verbs will say why.",
                );
            }
        };
        // **An abandoned run holds no lease**, so each run the sweep closed
        // gives up the roles it held.
        for closed in &swept {
            let run = match self.sessions.read_session(&SessionId(closed.clone())).await {
                Ok(run) => run,
                Err(e) => {
                    tracing::warn!(error = %e, session = %closed, "a swept run could not be read to release its roles");
                    continue;
                }
            };
            if let Some(sid) = &run.sid {
                self.release_role_claims(bot, sid.as_str()).await;
            }
        }
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
                // The charter this run holds is the one the offering call shipped,
                // when the answer carries that offer's code.
                let told = match answering {
                    Answering::Offer(offered_at) => Some(offered_at),
                    _ => None,
                };
                let run = self.mint_or_say_why(bot, told)?;
                self.end_windows(&windows_open).await;
                self.registry
                    .set_zone(&run.name, timezone.map(str::to_string));
                self.registry.set_day(&run.name, today);
                self.note_minted(&run.name).await;
                // Bound to the identity with no session: the first write is
                // what begins the card, exactly as a first boot's is. **The run
                // that was offered is left running**: a new session closes
                // another run only by claiming the role that run holds.
                self.fresh_block(run)
            }
            Some(answer) => {
                let (handle, session, stopped) = self
                    .resumable(bot, answer, &live, offerable.as_ref())
                    .await?;
                // **The resume hands the run a new key**, written to its card
                // before the call returns: the sitting that held the old one is
                // refused from here on, and nobody else's key moves.
                // **Every voucher the run holds is void first**, and a failure to
                // void any of them refuses the resume: the old grants must not
                // outlive the sitting that held them.
                // **When the run last wrote, said to the one who took it over**,
                // read before anything below touches the card.
                let last_wrote = session.as_ref().map(|session| session.last_beat());
                if let Err(failure) = self.void_vouchers_held_by(handle.as_str(), bot).await {
                    tracing::warn!(
                        step = failure.step, voided = failure.voided, run = %handle,
                        "a resume could not void the run's vouchers"
                    );
                    return Err(handle_declined(
                        "",
                        format!(
                            "No session was started, and run {handle} was not taken over: the \
                             vouchers it holds could not all be voided, so none was issued and \
                             the run is as it was. Try again, and if it persists a person has \
                             to look at the store."
                        ),
                    ));
                }
                // **A pick-up by name is a different agent holding an old run**,
                // so what jojobot taught the run and showed the run was true of
                // the agent that is gone. The teaching is forgotten before the
                // key moves, and a failure to forget it refuses the pick-up for
                // the same reason a voucher that cannot be voided does: the old
                // sitting's state must not outlive it. The shown-ledger lives in
                // this process and cannot fail, so it is cleared once the key is
                // new. A wrap code and a call with the live key take another
                // route and forget nothing.
                if let Err(e) = self.teachings.forget(&handle).await {
                    tracing::warn!(error = %e, run = %handle, "a resume could not forget what the run was taught");
                    return Err(handle_declined(
                        "",
                        format!(
                            "No session was started, and run {handle} was not taken over: what \
                             it was taught could not be forgotten, so the run is as it was. Try \
                             again, and if it persists a person has to look at the store."
                        ),
                    ));
                }
                let key = self.rekey(&handle, sid::RetiredBy::Resumed).await?;
                self.registry.forget_shown(handle.as_str());
                // **The reopen is the last step that can refuse**, so every
                // refusal before it leaves a stopped run stopped.
                let session = match session {
                    Some(stopped_run) if stopped => {
                        Some(self.reopen_for_resume(&handle, &stopped_run.id).await?)
                    }
                    other => other,
                };
                // **A resume that names a zone moves the run into it**, card
                // included, because a run outlives a device hop and the zone it
                // began in is not always the zone it is being worked in. A
                // resume that names none keeps what the run had: see `attach`.
                //
                // **Independent of the day, on purpose.** These used to be one
                // `if let Some(zone) = timezone` block, which meant a resume
                // naming `today` alone never reached `set_day` at all, and one
                // naming `timezone` alone called `set_day(&handle, None)` —
                // clearing a day this call never asked to touch. Two callers
                // asking two different questions must not answer each other's.
                if let Some(zone) = timezone {
                    self.registry.set_zone(&handle, Some(zone.to_string()));
                    if let Some(session) = &session
                        && let Err(e) = self.sessions.set_timezone(&session.id, Some(zone)).await
                    {
                        tracing::warn!(
                            error = %e, session = %session.id,
                            "the resumed session's timezone could not be written — this run \
                             answers in the zone it was given, and a restart would read the old \
                             one back"
                        );
                    }
                }
                if let Some(day) = today {
                    self.registry.set_day(&handle, Some(day));
                    if let Some(session) = &session
                        && let Err(e) = self.sessions.set_stated_day(&session.id, Some(day)).await
                    {
                        tracing::warn!(
                            error = %e, session = %session.id,
                            "the resumed session's stated day could not be written — this run \
                             answers in the day it was given, and a restart would read the \
                             original one back"
                        );
                    }
                }
                let mut block = match session {
                    Some(session) => {
                        let mut block = serde_json::json!({
                            "available": true,
                            "sid": key,
                            "run": handle.as_str(),
                            "resumed": true,
                            "last_wrote": last_wrote.map(|at| at.to_string()),
                            "session": session_json(&session),
                            "note": "you are resuming a session already in flight — its \
                                     chronology is above. Read it before you start: somebody \
                                     (you, before a disconnect) was part way through something. \
                                     `sid` is the run's new key; the one it had is closed.",
                        });
                        // **Only when THIS call named no day of its own.** A
                        // caller stating its own day is answered in that
                        // frame already and has nothing to be told.
                        if today.is_none() {
                            // **Read from the registry, the same place a write
                            // reads `caller.day` from — never the session
                            // row's own `started_on`,** which is set once at
                            // creation and never moved by a resume that
                            // changes the day. Reading the row would let this
                            // note name a day no write will actually use.
                            let live_day = self
                                .registry
                                .lookup_name(handle.as_str())
                                .and_then(|h| h.day);
                            crate::answer::note_resumed_day(&mut block, live_day);
                        }
                        block
                    }
                    // A run that never wrote: theirs, still good, and still
                    // nothing behind it.
                    None => self.fresh_block(sid::Minted { name: handle, key }),
                };
                if let Some(obj) = block.as_object_mut() {
                    obj.insert(
                        "shown_and_taught".into(),
                        "This run was taken over by its name, so what it was shown and taught \
                         before is forgotten: your answers from here teach and show as to a \
                         fresh session."
                            .into(),
                    );
                }
                block
            }
            // ── a first boot: the two branches ──────────────────────────────
            None if live.is_empty() && offerable.is_none() => {
                // This boot ships the charter, so the run holds it from now.
                let run = self.mint_or_say_why(bot, Some(self.clock().now()))?;
                self.end_windows(&windows_open).await;
                self.registry
                    .set_zone(&run.name, timezone.map(str::to_string));
                self.registry.set_day(&run.name, today);
                self.note_minted(&run.name).await;
                self.fresh_block(run)
            }
            None => {
                // Every live run, then the one stop worth bringing up. The
                // abandoned one comes last because it is the weaker claim on
                // the caller's attention, not because it is worse.
                let choices: Vec<serde_json::Value> = live
                    .iter()
                    .chain(offerable.iter())
                    .map(super::in_flight::choice_json)
                    .collect();
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
                    // **The proof that the caller holds this answer.** Sent
                    // back beside `resume`, once, it is what lets the answer
                    // leave out what this boot shipped.
                    "offer_code": self.registry.mint_offer(bot, offered_at),
                    "how_to_proceed": format!(
                        "This identity has work already in flight. Call start_here \
                         again with resume: the name of the run you are picking up, \
                         or resume: \"new\" for a fresh session, and send \
                         offer_code beside it to get an answer that carries only \
                         what is new. Without the code the answer is the full boot. \
                         Only a run that has been quiet for {} \
                         can be picked up by its name; a run still writing \
                         is continued with its own key. Nothing was closed but the runs under \
                         `swept`; choosing new leaves the runs above \
                         running.",
                        crate::windows::quiet(self.quiet_after),
                    ),
                })
            }
        };
        // **The frame is said only to a run that may be in another one.** A
        // boot on the real clock that sent its own zone and swept nothing has
        // judged every run in the frame it is itself in, so there is nothing to
        // tell it. A stated day, a swept run, a server acting out a day and a
        // zone the caller did not send (the instance's decided the day) each
        // keep the answer.
        let frame_is_news = today.is_some()
            || self.clock().stated().is_some()
            || !swept.is_empty()
            || timezone.is_none();
        if let Some(obj) = block.as_object_mut() {
            obj.insert("swept".into(), swept.into());
            // 🚨 **A sweep that closed nothing is not a sweep that could not
            // run.** Both leave `swept` empty, and only one of them means a
            // stale run is still standing. Said only in the log, the surface
            // reads the same for both and the caller meets that run again on
            // every boot with nothing saying why.
            obj.insert(
                "unswept".into(),
                unswept
                    .iter()
                    .map(|(session, _)| session.to_string())
                    .collect::<Vec<_>>()
                    .into(),
            );
            if frame_is_news {
                obj.insert("swept_in".into(), Self::frame(today, self.clock().stated()));
            }
            // **On every branch but the answer to the offer**, because the
            // sitting that most needs a handover is the one with nothing to
            // resume. The answer to the offer is the other one: the offering
            // call shipped the story, and `left_out` names it.
            if !answering.is_offer() {
                obj.insert("handover".into(), Self::handover(handover.as_ref()));
            }
        }
        Ok(block)
    }

    /// **What the last sitting that closed properly left for this one.**
    ///
    /// 🚨 **Wrapping used to be how a run disappeared.** Only live and
    /// abandoned runs are offered, so the one sitting that told its story
    /// vanished from the next boot while every sitting that merely stopped
    /// stayed on the list. **Closing cleanly was how you became invisible**,
    /// and the measured behaviour followed it.
    ///
    /// ⛔️ **This is not a resume candidate and must never become one.**
    /// `wrapped` is terminal: nothing appends to it. It is here to be READ.
    /// Offering it as something to pick up would break the never-auto-wrap
    /// rule and the resume rule at once.
    ///
    /// **Present with nothing in it rather than absent.** Most sittings never
    /// wrap, so "nobody left you anything" is the ordinary answer — and a
    /// missing key would make that read the same as a build that cannot say.
    pub(crate) fn handover(wrapped: Option<&Session>) -> serde_json::Value {
        let Some(run) = wrapped else {
            return serde_json::json!({
                "story": serde_json::Value::Null,
                "note": "no run of this identity has been wrapped up, so nobody has left you a \
                         closing story. That is an empty board rather than a story jojobot could \
                         not read.",
            });
        };
        serde_json::json!({
            // **The entry the wrap closed the run with**, because wrapping folds
            // the still-open focus into the chronology as one final beat and that
            // entry IS the story. A run reopened for one last change takes entries
            // after it, so the newest entry is not the story. A run from before the
            // closing mark has no marked entry, but a wrap of it recorded the run's
            // focus on its closing entry, and that entry is its story; with no focus
            // to record, nothing marks it and its last entry is the story.
            "story": run
                .closing_entry()
                .or_else(|| run.entries.last())
                .map(|e| e.text.clone()),
            "working_on": run.focus,
            "session": run.id.to_string(),
            "wrapped_at": run.wrapped_at().to_string(),
            "note": "the last run of this identity that was wrapped up. It is here to READ: a \
                     wrapped run is read-only unless you hold its wrap_code, and it cannot be \
                     resumed as a running session. Read it, then start your own.",
        })
    }

    /// **The frame this boot decided staleness in**, said where a caller meets
    /// the decision.
    ///
    /// 🚨 **A caller that does not know the frame is its to state never will.**
    /// It is handed a list of its own runs, cannot tell which are over, and has
    /// nothing saying the list was decided on a clock — so a run acting out
    /// months in minutes reads every sitting it ever had as still working. The
    /// one place it can learn otherwise is the answer that is wrong, which is
    /// this one.
    ///
    /// A caller that stated a day is told which day and nothing more: it does
    /// not need to be sold an argument it already sent.
    ///
    /// ⛔️ **A server acting out a day gets a third answer, and the reason is
    /// the advice.** Every word of the note below stays true on such a server —
    /// no day was stated and the server's clock decided it — but the way out it
    /// names does not apply, because the clock the sweep used is already the
    /// day the caller is in. **Advice naming a way through that does not apply
    /// is the shape a reader treats as a fault**, and this is the sentence a
    /// run acting out a year meets sixteen times.
    fn frame(
        today: Option<jiff::civil::Date>,
        stated: Option<jiff::civil::Date>,
    ) -> serde_json::Value {
        match (today, stated) {
            (Some(day), _) => serde_json::json!({ "day": day.to_string() }),
            (None, Some(day)) => serde_json::json!({
                "day": day.to_string(),
                "note": format!(
                    "this server is acting out {day}, so that is the day your runs were judged \
                     quiet against. Your run stated no day of its own and does not need to: \
                     send `today` only if the day YOU are in differs from the server's."
                ),
            }),
            (None, None) => serde_json::json!({
                "day": serde_json::Value::Null,
                "note": "no day was stated, so runs were judged quiet on this server's clock. \
                         If your run is not happening now — catching up on an earlier day, or \
                         acting out a stretch of time — send the day you are in as `today` and \
                         the decision is made in that frame instead.",
            }),
        }
    }

    /// **End every wrap window this bot has open, because a newer run of it is
    /// starting.** A window is for one last change to the run that wrapped, and
    /// a run that starts after it is the work that came next. Best-effort: a
    /// store that cannot clear one leaves it for the use of its code, which
    /// checks for a newer run itself.
    ///
    /// **This one reads the bot's runs.** A caller that already holds them, as
    /// a boot does after its sweep, ends the windows through
    /// [`end_windows`](Self::end_windows) instead and reads nothing.
    pub(crate) async fn end_wrap_windows(&self, bot: &EntityId) {
        let runs = match self.sessions.sessions_of(bot).await {
            Ok(runs) => runs,
            Err(e) => {
                tracing::warn!(error = %e, %bot, "could not read the runs to end their windows");
                return;
            }
        };
        self.end_windows(&jojobot_domain::session::windows_open(&runs))
            .await;
    }

    /// Clear the wrap window of each run named, with no read of the store.
    pub(crate) async fn end_windows(&self, open: &[SessionId]) {
        for run in open {
            if let Err(e) = self.sessions.set_wrap_window(run, None).await {
                tracing::warn!(error = %e, %run, "a wrap window could not be ended");
            }
        }
    }

    /// **Answer a `resume` that is a wrap code.**
    ///
    /// The code names one wrapped run of this bot whose window is still there.
    /// Using it opens the window and hands back that run's own sid, with the
    /// run still `wrapped`. It starts no run, abandons no run, takes no role
    /// and renews no lease. A code with no window behind it, or one whose run
    /// a newer run has since followed, is refused with the way to a fresh run.
    async fn resume_by_wrap_code(
        &self,
        bot: &EntityId,
        code: &str,
    ) -> Result<serde_json::Value, CallToolResult> {
        let spent = |why: &str| {
            handle_declined(
                code,
                format!(
                    "{why} Nothing was reopened and nothing was written. To start a fresh run, \
                     call start_here with bot set to {} and no resume.",
                    bot.as_str()
                ),
            )
        };
        let runs = match self.sessions.sessions_of(bot).await {
            Ok(runs) => runs,
            Err(e) => {
                let note = "the session world is not reachable right now, so jojobot cannot \
                            say whether this code still reopens a run. Nothing was written.";
                return crate::session::window::board_unread(&e, bot, note);
            }
        };
        // **A code is spent on its first use.** Only a window that is still
        // OFFERED answers to one: using the code opens the window, and a code
        // whose window is open is the spent one.
        let Some(wrapped) = runs.iter().find(|run| {
            run.state == SessionState::Wrapped
                && run
                    .wrap_window
                    .as_ref()
                    .is_some_and(|window| !window.is_open() && window.answers_to(code))
        }) else {
            return Err(spent(
                "That code is spent or unknown: its run was wrapped a second time, or a newer \
                 run of this bot started, or it was never handed out.",
            ));
        };
        // **A newer run ends the window even when nothing cleared it.** The
        // clear is best-effort, so the use of the code decides for itself.
        if runs
            .iter()
            .any(|run| run.id != wrapped.id && run.started_at > wrapped.started_at)
        {
            self.end_windows(&jojobot_domain::session::windows_open(&runs))
                .await;
            return Err(spent(
                "A newer run of this bot has started, which ends the window for the run that \
                 wrapped.",
            ));
        }
        // **The clock ends the window too.** A code used after the window closed
        // is refused in words that say when it closed.
        if let Some(ends) = wrapped
            .window_ends_at()
            .filter(|_| wrapped.window_lapsed_at(self.clock().now()))
        {
            let closed = self
                .window_end_in_words(wrapped.timezone.as_deref(), ends)
                .await;
            return Err(spent(&format!(
                "The window for that run closed at {closed}: a wrap code reopens a run for {} \
                 hours after the wrap.",
                jojobot_domain::session::WRAP_WINDOW_HOURS
            )));
        }
        // **The key is issued before the code is spent.** The run is taken into
        // the registry first, because a run it does not hold has no entry to
        // set a key on, and a key that opens nothing is worse than a refusal.
        // The key is written to the card before the call returns, so a store
        // that refuses it leaves the window offered and the code still good.
        self.registry.adopt(wrapped);
        let name = wrapped
            .sid
            .clone()
            .unwrap_or_else(|| sid::Sid(wrapped.id.as_str().to_string()));
        let key = self.rekey(&name, sid::RetiredBy::Wrapped).await?;
        let opened = match self
            .sessions
            .set_wrap_window(
                &wrapped.id,
                Some(jojobot_domain::session::WrapWindow::Open(
                    jojobot_domain::session::hash_wrap_code(code),
                )),
            )
            .await
        {
            Ok(opened) => opened,
            Err(e) => {
                tracing::warn!(error = %e, run = %wrapped.id, "a wrap window could not be opened");
                // The window did not open, so the key just issued is taken away
                // again: the run is as the refusal says it is.
                self.registry.set_key(&name, sid::KeyState::Void, None);
                let trail = self
                    .registry
                    .lookup_name(name.as_str())
                    .map(|held| held.trail())
                    .unwrap_or_default();
                if let Err(e) = self
                    .sessions
                    .set_key_hash(&wrapped.id, jojobot_domain::session::KEY_VOID, &trail)
                    .await
                {
                    tracing::warn!(error = %e, run = %wrapped.id, "a key issued for a window that did not open could not be taken away");
                }
                return Err(spent("The window could not be opened just now."));
            }
        };
        let window = match opened.window_ends_at() {
            Some(ends) => self.window_sentence(&opened, ends).await,
            None => String::new(),
        };
        Ok(serde_json::json!({
            "available": true,
            "sid": key,
            "run": name.as_str(),
            "resumed": true,
            "session": session_json(&opened),
            "note": format!(
                "This run was wrapped and is reopened. {window} A second wrap adds a closing \
                 entry beside the first unless its story is identical to a closing story already told. No \
                 role was taken and no lease was renewed."
            ),
        }))
    }

    /// **Keep the key this boot handed out**, for a run that has no card until
    /// its first write. Without a note a restart between the boot and the first
    /// write leaves the caller holding a key that opens nothing. A store that
    /// refuses the note is logged and the boot goes on: the run works until the
    /// next restart, as every run did before the note existed.
    async fn note_minted(&self, name: &sid::Sid) {
        let Some(held) = self.registry.lookup_name(name.as_str()) else {
            return;
        };
        let sid::KeyState::Held(key_hash) = held.key else {
            return;
        };
        let run = jojobot_domain::session::MintedRun {
            name: held.name,
            key_hash,
            bot: held.bot,
            zone: held.zone,
            day: held.day,
            minted_at: self.clock().now(),
        };
        if let Err(e) = self.sessions.note_minted(&run).await {
            tracing::warn!(error = %e, run = %name, "a minted run's key could not be kept");
        }
    }

    /// The block for a session with no card behind it yet — a first boot, or
    /// `new`, or a handle nothing has been written under.
    pub(crate) fn fresh_block(&self, run: sid::Minted) -> serde_json::Value {
        serde_json::json!({
            "available": true,
            "sid": run.key,
            "run": run.name.as_str(),
            "resumed": false,
            "session": serde_json::Value::Null,
            "note": "a fresh session. `sid` is its key: send it on every call and tell it to \
                     nobody. `run` is its name, which records may carry and which opens nothing. \
                     Only a note of the key is kept until the run writes — the record begins on \
                     your first journal entry or the first write you make.",
        })
    }

    /// Read an answer to the offer: the run it names, and the live session it
    /// addresses if it addresses one.
    ///
    /// **A name is not a way in; it is a way to ask.** The answer succeeds only
    /// for a run of this identity that has been quiet for a full lease by the
    /// server's clock, because a run still writing belongs to whoever holds its
    /// key. Every other case is refused in its own words, and none is repaired
    /// into a nearby name, which would be jojobot guessing which session
    /// somebody meant. **Nothing the caller sent is repeated in a refusal**:
    /// the run is named by what the registry holds.
    pub(crate) async fn resumable(
        &self,
        bot: &EntityId,
        answer: &str,
        live: &[Session],
        offerable: Option<&Session>,
    ) -> Result<(sid::Sid, Option<Session>, bool), CallToolResult> {
        // **A card on this bot's own board is a run the registry may not have
        // met**: it was begun by another writer, or since the last rebuild. Its
        // name is public, so it is taken in by the name it goes by.
        for session in live.iter().chain(offerable) {
            if session
                .sid
                .as_ref()
                .map_or(session.id.as_str(), |n| n.as_str())
                == answer
                && session.bot == *bot
            {
                self.registry.adopt(session);
            }
        }
        let again = "Call start_here with your bot name and no resume to see what there is.";
        if !sid::is_readable(answer) {
            return Err(handle_declined(
                "",
                format!(
                    "No session was started. That is not a run name jojobot mints, and a name \
                 is not a key either. jojobot will not correct one, because correcting it \
                 means guessing which session you meant. {again}"
                ),
            ));
        }
        let Some(held) = self.registry.lookup_name(answer) else {
            return Err(handle_declined(
                "",
                format!(
                    "No session was started. No run goes by that name: this jojobot is not \
                 holding one, and a run that never wrote has nothing to be recovered from. \
                 The work itself is untouched and still readable. {again}"
                ),
            ));
        };
        if held.bot != *bot {
            // Refused without naming whose it is, and without offering a way
            // to switch: disclosing the other identity would prime
            // identity-switching as something an agent might do. A run that is
            // not yours is simply not yours; the way forward is your own
            // session.
            return Err(handle_declined(
                "",
                format!(
                    "No session was started. That run is not yours — a session is bound to \
                     its identity at boot and never switches. Call start_here as '{bot}' with \
                     no resume to see what is."
                ),
            ));
        }
        let name = held.name.clone();
        let Some(card) = held.card else {
            // Minted, never written under: nothing records it and nothing says
            // it went quiet, so a name cannot take it.
            return Err(handle_declined(
                "",
                format!(
                    "No session was started. Run {name} has not written anything, so there is \
                     nothing to pick up and nothing says it went quiet. It is continued only \
                     with its own key. {again}"
                ),
            ));
        };
        let existing = match live.iter().find(|s| s.id == card) {
            Some(session) => session.clone(),
            None => match self.sessions.read_session(&card).await {
                Ok(session) => session,
                Err(SessionError::UnknownSession { .. }) => {
                    return Err(handle_declined(
                        "",
                        format!(
                            "No session was started. Run {name} is held by this jojobot, but \
                             the session it names is not on the board any more. Nothing was \
                             changed. {again}"
                        ),
                    ));
                }
                Err(e) => {
                    tracing::warn!(error = %e, session = %card, "a session could not be read");
                    return Err(handle_declined(
                        "",
                        "No session was started, and nothing was changed: the session store \
                         refused. This is not something your call can fix by being different — \
                         try again, and if it persists a person has to look at the store."
                            .to_string(),
                    ));
                }
            },
        };
        // **Quiet is the server's own reading.** The caller's `today` plays no
        // part: a run acting out another day would otherwise age a live run into
        // being takeable.
        if let super::in_flight::Pickup::From { wrote, free_at } = super::in_flight::pickup_of(
            &existing,
            held.taken_at,
            self.clock().now(),
            self.quiet_after,
        ) {
            let way = "only its key continues it";
            return Err(handle_declined(
                "",
                format!(
                    "No session was started. Run {name} wrote at {wrote}, so it is still \
                     its own: {way}. A run quiet for {} can be picked up by its \
                     name, which for this one is from {free_at}.",
                    crate::windows::quiet(self.quiet_after),
                ),
            ));
        }
        if let Some(session) = live.iter().find(|s| s.id == card) {
            return Ok((name, Some(session.clone()), false));
        }
        // **Not among the live runs, so it stopped — and stopping is not the
        // end.** Reopening is what makes "resume last session" always work, and
        // it is bounded by nothing but the state the run reached: the offer's
        // age window governs what jojobot VOLUNTEERS, never what a name
        // someone kept can still reach.
        //
        // **The one end that is the last word is refused here, before anything
        // is changed**: a run which told its story is over, and its chronology
        // stands as the record of what happened. The reopen itself comes last,
        // after the vouchers are void and the key is new, so a refusal before it
        // leaves the run as it was.
        if existing.state == SessionState::Wrapped {
            return Err(handle_declined(
                "",
                format!(
                    "No session was started. Run {name} is wrapped — its story has been told, \
                     so this end is the last word, except through the wrap_code its wrap handed \
                     back: pass that code as resume within {hours} hours of the wrap, and the run is a normal session again. Its chronology is the account of \
                     what happened. {again}",
                    hours = jojobot_domain::session::WRAP_WINDOW_HOURS
                ),
            ));
        }
        Ok((name, Some(existing), true))
    }

    /// **Reopen the stopped run a resume is taking back**, as the last step of
    /// the resume. By now the run's vouchers are void and its key is new, so a
    /// refusal here says what landed: the run is still stopped, and the same
    /// resume goes through once the store answers.
    async fn reopen_for_resume(
        &self,
        name: &sid::Sid,
        card: &SessionId,
    ) -> Result<Session, CallToolResult> {
        let again = "Call start_here with your bot name and no resume to see what there is.";
        match self.sessions.reopen(card).await {
            Ok(session) => Ok(session),
            // The run was read as stopped a moment ago; a state that closed in
            // between is the last word.
            Err(SessionError::Closed { state, .. }) => Err(handle_declined(
                "",
                format!(
                    "No session was started. Run {name} is {state} — its story has been told, \
                     so this end is the last word, except through the wrap_code its wrap handed \
                     back: pass that code as resume within {hours} hours of the wrap, and the run is a normal session again. {again}",
                    hours = jojobot_domain::session::WRAP_WINDOW_HOURS
                ),
            )),
            Err(SessionError::UnknownSession { .. }) => Err(handle_declined(
                "",
                format!(
                    "No session was started. Run {name} is held by this jojobot, but the \
                     session it names is not on the board any more. Its vouchers were voided \
                     and its key was replaced. {again}"
                ),
            )),
            // **Degrades the way the rest of a boot degrades**: the caller is
            // told plainly what landed, and the underlying fault goes to the
            // log where an operator reads it.
            Err(e) => {
                tracing::warn!(error = %e, session = %card, "a session could not be reopened");
                Err(handle_declined(
                    "",
                    format!(
                        "No session was started. Run {name} could not be reopened: the session \
                         store refused. Its vouchers were voided and its key was replaced, and \
                         the run is still stopped. Send the same call again, and if it persists \
                         a person has to look at the store."
                    ),
                ))
            }
        }
    }

    /// **Give a run a new key**, writing it to the card before it is returned.
    ///
    /// The old key stops opening the run at once and is remembered as retired
    /// with `by`, so the sitting that holds it is told why. If the card cannot
    /// be written nothing changes and the call is refused: a key the store does
    /// not hold would be brought back by the next restart.
    pub(crate) async fn rekey(
        &self,
        name: &sid::Sid,
        by: sid::RetiredBy,
    ) -> Result<String, CallToolResult> {
        // **Only a run the registry holds is given a key.** The registry is
        // where a key is looked up, so a key for a name it does not hold would
        // open nothing, and the caller would carry it as if it did.
        let Some(held) = self.registry.lookup_name(name.as_str()) else {
            return Err(handle_declined(
                "",
                "No key was handed out: no run goes by that name in this jojobot, so a key for \
                 it would open nothing. Nothing was changed. Call start_here with your bot name \
                 and no resume to see what there is."
                    .to_string(),
            ));
        };
        let key = jojobot_domain::session::mint_key();
        let hash = jojobot_domain::session::hash_key(&key);
        let at = self.clock().now();
        // **The card gets the key and what it replaces in one write**: the
        // retired key and the pick-up mark are what a restart cannot rebuild.
        let trail = held.trail_after(Some((at, by)));
        if let Some(card) = &held.card
            && let Err(e) = self.sessions.set_key_hash(card, &hash, &trail).await
        {
            tracing::warn!(error = %e, session = %card, "a run's new key could not be written");
            return Err(handle_declined(
                "",
                "No session was started, and no key was handed out: the session store \
                 refused the new key. Nothing was changed. Try again, and if it persists a \
                 person has to look at the store."
                    .to_string(),
            ));
        }
        self.registry
            .set_key(name, sid::KeyState::Held(hash), Some((at, by)));
        Ok(key)
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

    /// **A resume that names a zone moves the run into it; one that names none
    /// leaves it where it was.**
    ///
    /// A run outlives a device hop, so the zone it began in is not always the
    /// zone it is being worked in — and a caller that said nothing has not
    /// asked for the fallback. Silently reframing a running session is the one
    /// thing this must not do, because nothing about it would be visible to the
    /// caller until a date came out wrong.
    ///
    /// **Both halves in one case**, because a build that always overwrote and a
    /// build that never did are each told apart by only one of them. The card
    /// is read back rather than the answer to the write: a zone the door
    /// changed in the process and not on the record is one a restart loses.
    #[tokio::test]
    async fn a_resume_moves_the_zone_only_when_it_names_one() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let sid = booted_in(&jojobot, "gamma", "Europe/Madrid", None).await;
        let name = run_name(&jojobot, &sid);
        // The record is lazy, so a write is what puts the run on it.
        journal_entry(&jojobot, &sid, "started").await;

        let zone_on_record = async || {
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .into_iter()
                .find(|s| s.sid.as_ref().map(|h| h.as_str()) == Some(name.as_str()))
                .expect("the run is on the record")
                .timezone
        };
        assert_eq!(zone_on_record().await.as_deref(), Some("Europe/Madrid"));

        // Picked up somewhere else, naming the new zone.
        let sid = booted_in(&jojobot, "gamma", "America/New_York", Some(&sid)).await;
        assert_eq!(
            zone_on_record().await.as_deref(),
            Some("America/New_York"),
            "a resume that names a zone moves the run into it",
        );

        // Picked up again, naming none: the run keeps what it had.
        boot_answering(&jojobot, "gamma", &sid).await;
        assert_eq!(
            zone_on_record().await.as_deref(),
            Some("America/New_York"),
            "…and a resume that names none leaves it where it was",
        );
    }

    /// **A resume names `today` and `timezone` independently — neither
    /// clears the other, and naming neither keeps both.** These used to
    /// share one `if let Some(zone) = timezone` guard, so a resume naming
    /// `today` alone never reached `set_day` at all, and one naming
    /// `timezone` alone cleared the day by calling `set_day(&handle, None)`.
    #[tokio::test]
    async fn a_resume_moves_the_day_independently_of_the_zone() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        ensure(&jojobot, "person:milhouse").await;

        let booted = boot_on(&jojobot, "gamma", "2026-03-15").await;
        let sid = sid_of(&booted).expect("a handle");
        journal_entry(&jojobot, &sid, "started").await;

        let day_of = async |sid: &str| {
            let mut args = capture_args("milhouse", "dated by the run");
            args.sid = Some(sid.to_string());
            args.recorded_at = None;
            capture_ok(&jojobot, args).await["recorded_at"]
                .as_str()
                .expect("a capture is stamped with a day")
                .to_string()
        };
        assert_eq!(day_of(&sid).await, "2026-03-15");

        // `today` alone moves the day.
        let sid =
            sid_of(&boot_answering_dated(&jojobot, "gamma", &sid, Some("2026-04-01"), None).await)
                .expect("a resume hands back the run's new key");
        assert_eq!(
            day_of(&sid).await,
            "2026-04-01",
            "a resume naming today alone must move the day"
        );

        // `timezone` alone must not clear a day this call never named.
        let sid = sid_of(
            &boot_answering_dated(&jojobot, "gamma", &sid, None, Some("Europe/Madrid")).await,
        )
        .expect("a resume hands back the run's new key");
        assert_eq!(
            day_of(&sid).await,
            "2026-04-01",
            "a resume naming a zone alone must leave the day exactly where it was"
        );

        // Naming neither keeps both.
        let sid = sid_of(&boot_answering_dated(&jojobot, "gamma", &sid, None, None).await)
            .expect("a resume hands back the run's new key");
        assert_eq!(
            day_of(&sid).await,
            "2026-04-01",
            "a resume naming neither must leave the day exactly where it was"
        );
    }

    /// **`day_note` names the day a write will actually use, never the
    /// session row's own stale `started_on`.** After a resume moves the day,
    /// a further resume naming no day of its own must report — and a
    /// following capture must record under — the SAME day, read from the
    /// receipt rather than recomputed here.
    #[tokio::test]
    async fn a_resumed_days_note_names_the_day_a_capture_actually_records_under() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        ensure(&jojobot, "person:milhouse").await;

        let booted = boot_on(&jojobot, "gamma", "2026-03-15").await;
        let sid = sid_of(&booted).expect("a handle");
        journal_entry(&jojobot, &sid, "started").await;

        // Move the day.
        let sid =
            sid_of(&boot_answering_dated(&jojobot, "gamma", &sid, Some("2026-04-01"), None).await)
                .expect("a resume hands back the run's new key");

        // Resume again, naming no day of its own: the note must read the
        // moved day.
        let resumed = boot_answering(&jojobot, "gamma", &sid).await;
        let noted = resumed["session"]["day_note"]
            .as_str()
            .unwrap_or_else(|| panic!("a day note: {resumed}"));
        assert!(noted.contains("2026-04-01"), "{noted}");

        // And a capture on this same run, naming no day, must land on that
        // same day. The resume gave the run a new key; the old one is closed.
        let sid = sid_of(&resumed).expect("a resume hands back the run's new key");
        let mut args = capture_args("milhouse", "after the moved day");
        args.sid = Some(sid.clone());
        args.recorded_at = None;
        let captured = capture_ok(&jojobot, args).await;
        assert_eq!(
            captured["recorded_at"], "2026-04-01",
            "the note and the write must agree: {captured}"
        );
    }

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
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
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

    /// **A wrapped run's story is what the door hands the next sitting — and
    /// it is never a session to resume.**
    ///
    /// 🚨 **Wrapping a run correctly used to make it disappear.** Only live and
    /// abandoned runs are offered, so the one sitting that closed properly —
    /// a real closing story, told for somebody with none of its context —
    /// vanished from the next boot, while every sitting that just stopped was
    /// still on the list. **Closing cleanly was how you became invisible**, and
    /// the measured behaviour followed: nobody wrapped a session again.
    ///
    /// ⚠️ **This is a design change and not a missed case.** `wrapped` stays
    /// terminal: nothing appends to it and it is NOT offered as something to
    /// resume. It is offered as a handover to READ. Conflating the two would
    /// break the never-auto-wrap rule and the resume rule at once.
    ///
    /// ⭐ **The pairing is the whole case**: the story reaches the door, and
    /// the run it came from is absent from `choices`. A build that simply added
    /// wrapped runs to the offer would pass the first half and break both
    /// rules.
    #[tokio::test]
    async fn a_wrapped_runs_story_reaches_the_door_and_is_not_offered_to_resume() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let sid = booted(&jojobot, "gamma").await;
        jojobot
            .journal(Parameters(crate::session::JournalArgs {
                request_id: None,
                working_on: None,
                sid: sid.clone(),
                entry: "found the boot spends nine seconds rebuilding the index".into(),
                focus: Some("chasing the slow boot".into()),
            }))
            .await
            .expect("journal ok");
        let story = "the index rebuild is the whole nine seconds. Next run should cache it \
                     rather than reading the store twice.";
        jojobot
            .wrap_session(Parameters(crate::session::WrapSessionArgs {
                sid: sid.clone(),
                story: story.into(),
            }))
            .await
            .expect("wrap ok");

        let body = boot(&jojobot, "gamma").await;
        let handover = &body["session"]["handover"];
        assert!(
            handover["story"]
                .as_str()
                .is_some_and(|told| told.contains("cache it")),
            "the closing story never reaches the next sitting: {body}",
        );

        // ⛔️ **Terminal stays terminal.** The run is readable, not resumable.
        let offered: Vec<&str> = body["session"]["choices"]
            .as_array()
            .map(|cs| {
                cs.iter()
                    .filter_map(|c| c["run"].as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(
            !offered.contains(&sid.as_str()),
            "a wrapped run is being offered as something to resume: {body}",
        );
    }

    /// **The handover is the run that WRAPPED last, not the one that STARTED
    /// last.**
    ///
    /// `sessions_of` sorts newest-start-first, and the handover used to take
    /// the first `Wrapped` run off that list — the newest START among the
    /// wrapped runs, not the newest CLOSE. A run started Monday, worked all
    /// week and wrapped Friday lost to a same-evening run started Wednesday,
    /// because Wednesday sorts ahead of Monday — so the next boot read the
    /// older story and never saw the newer one.
    ///
    /// **Both halves asserted**, because a build that always returned the
    /// week-long run's story would pass the first half by accident.
    #[tokio::test]
    async fn the_handover_is_the_run_that_wrapped_most_recently_not_the_one_that_started_most_recently()
     {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let now = jiff::Timestamp::now();

        // Started Monday, worked all week, and is the run that wrapped LAST.
        let long_running = store
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "the week-long piece of work".into(),
                started_at: now - jiff::SignedDuration::from_hours(96),
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        store
            .append(
                &long_running.id,
                NewEntry::manual(
                    "wrapped Friday evening — the story that must win",
                    now - jiff::SignedDuration::from_hours(2),
                    None,
                ),
            )
            .await
            .expect("append ok");
        store
            .close(&long_running.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        // Started Wednesday — AFTER the run above — and wrapped that same
        // evening, well BEFORE it: the newer start, the older close.
        let short_run = store
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t002".into()),
                focus: "a same-day errand".into(),
                started_at: now - jiff::SignedDuration::from_hours(48),
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        store
            .append(
                &short_run.id,
                NewEntry::manual(
                    "wrapped Wednesday evening — the older close",
                    now - jiff::SignedDuration::from_hours(46),
                    None,
                ),
            )
            .await
            .expect("append ok");
        store
            .close(&short_run.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let body = boot(&jojobot, "gamma").await;
        let story = body["session"]["handover"]["story"]
            .as_str()
            .expect("a story");
        assert!(
            story.contains("must win"),
            "the run that wrapped LAST must be the handover: {body}"
        );
        assert!(
            !story.contains("older close"),
            "…not the run that merely started later: {body}"
        );
        // A wrapped run is read-only unless its wrap_code is held, and the note
        // on every handover has to say so rather than call the run closed for good.
        let note = body["session"]["handover"]["note"]
            .as_str()
            .expect("a handover carries its note");
        assert!(
            note.contains("wrap_code"),
            "the handover note names the way a wrapped run reopens: {note}"
        );
    }

    /// **The handover is the run that was WRAPPED last, by the moment of its
    /// closing entry, and not the run that was WRITTEN to last.** The older run
    /// is wrapped first, then the newer one, then the older one is reopened for
    /// one last change: its last write is now the newest in the board, and its
    /// wrap is still the older. The handover is the newer run's story, at the
    /// newer run's wrap. Without the closing mark both halves would name the
    /// reopened run and the afterthought.
    #[tokio::test]
    async fn the_handover_follows_the_wrap_and_not_the_last_write_of_a_reopened_run() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let now = jiff::Timestamp::now();
        let hours = |h: i64| now - jiff::SignedDuration::from_hours(h);

        let wrapped = |sid: &str, started: i64, story: &str, wrapped_at: i64| {
            let store = store.clone();
            let (sid, story) = (sid.to_string(), story.to_string());
            async move {
                let run = store
                    .begin(NewSession {
                        timezone: None,
                        started_on: None,
                        bot: EntityId("bot:gamma".into()),
                        sid: Sid(sid),
                        focus: "a piece of work".into(),
                        started_at: hours(started),
                        id: None,
                        key_hash: None,
                    })
                    .await
                    .expect("begin ok");
                store
                    .append(
                        &run.id,
                        NewEntry::manual(story, hours(wrapped_at), None).closing(None),
                    )
                    .await
                    .expect("append ok");
                store
                    .close(&run.id, SessionState::Wrapped)
                    .await
                    .expect("close ok");
                run
            }
        };
        let older = wrapped("t001", 30, "older story: wrapped first", 20).await;
        let newer = wrapped("t002", 10, "newer story: wrapped second", 5).await;

        // Both are reopened and take one more entry each. The older run's is the
        // newest in the board; the newer run's comes after its own wrap, so the
        // handover's moment has to be the wrap and not that entry.
        for (run, entry, at) in [
            (&older, "an afterthought on the older run", 1),
            (&newer, "an afterthought on the newer run", 2),
        ] {
            store
                .set_wrap_window(
                    &run.id,
                    Some(jojobot_domain::session::WrapWindow::Open(
                        "wc-testtesttest".into(),
                    )),
                )
                .await
                .expect("window opens");
            store
                .append(&run.id, NewEntry::manual(entry, hours(at), None))
                .await
                .expect("append ok");
        }

        let body = boot(&jojobot, "gamma").await;
        let handover = &body["session"]["handover"];
        let story = handover["story"].as_str().expect("a story");
        assert!(
            story.contains("newer story"),
            "the handover is the run wrapped last, not the one written to last: {handover}"
        );
        assert_eq!(
            handover["wrapped_at"].as_str(),
            Some(hours(5).to_string().as_str()),
            "and it was wrapped at its own closing entry's moment: {handover}"
        );
    }

    /// **A run reopened in the shape an older build stored it hands over its
    /// closing entry, not its afterthought.** Before the closing mark existed a
    /// wrap carried the run's focus on its closing entry and marked nothing, and a
    /// run could already be reopened by its wrap code. Such a run holds entries
    /// after its closing one, none of them marked. Its story is the newest entry
    /// that carries the focus the wrap recorded, at that entry's moment; the pair
    /// beside it is a run with no such entry, whose last entry is still its story.
    #[tokio::test]
    async fn the_handover_of_a_run_reopened_in_the_old_shape_is_its_closing_entry() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let now = jiff::Timestamp::now();
        let hours = |h: i64| now - jiff::SignedDuration::from_hours(h);

        let begin = |sid: &str, started: i64| {
            let store = store.clone();
            let sid = sid.to_string();
            async move {
                store
                    .begin(NewSession {
                        timezone: None,
                        started_on: None,
                        bot: EntityId("bot:gamma".into()),
                        sid: Sid(sid),
                        focus: "a piece of work".into(),
                        started_at: hours(started),
                        id: None,
                        key_hash: None,
                    })
                    .await
                    .expect("begin ok")
            }
        };
        // The older run: wrapped in the old shape (a focus, no mark), then
        // reopened and given an afterthought. Its story is the closing entry.
        let reopened = begin("t001", 30).await;
        store
            .append(
                &reopened.id,
                NewEntry {
                    closing_focus: Some("the piece of work, at the close".into()),
                    ..NewEntry::manual("reopened story: the wrap", hours(20), None)
                },
            )
            .await
            .expect("append ok");
        store
            .close(&reopened.id, SessionState::Wrapped)
            .await
            .expect("close ok");
        store
            .set_wrap_window(
                &reopened.id,
                Some(jojobot_domain::session::WrapWindow::Open(
                    "wc-testtesttest".into(),
                )),
            )
            .await
            .expect("window opens");
        store
            .append(
                &reopened.id,
                NewEntry::manual("an afterthought on the reopened run", hours(1), None),
            )
            .await
            .expect("append ok");

        let body = boot(&jojobot, "gamma").await;
        let handover = &body["session"]["handover"];
        assert!(
            handover["story"]
                .as_str()
                .is_some_and(|story| story.contains("reopened story")),
            "the handover is the closing entry and not the afterthought: {handover}"
        );
        assert_eq!(
            handover["wrapped_at"].as_str(),
            Some(hours(20).to_string().as_str()),
            "and it was wrapped at that entry's moment, not at the last beat: {handover}"
        );

        // The pair: a newer run wrapped in the old shape with no focus to carry
        // and never reopened. Nothing marks its closing entry, so its last entry
        // is its story, as it always was.
        let plain = begin("t002", 10).await;
        store
            .append(
                &plain.id,
                NewEntry::manual("plain story: the last entry", hours(5), None),
            )
            .await
            .expect("append ok");
        store
            .close(&plain.id, SessionState::Wrapped)
            .await
            .expect("close ok");
        let body = boot(&jojobot, "gamma").await;
        let handover = &body["session"]["handover"];
        assert!(
            handover["story"]
                .as_str()
                .is_some_and(|story| story.contains("plain story")),
            "a run that was never reopened hands over its last entry: {handover}"
        );
    }

    /// **A sitting that left nothing says so, and does not read as silence.**
    ///
    /// ⭐ **On the measured evidence this is the COMMON path, not an edge**:
    /// most sittings never wrapped and never journalled, so the door's usual
    /// answer is that there is no handover. **A missing key would make "nobody
    /// left you anything" and "this build cannot tell you" render identically**
    /// — and a reader who has to infer which will eventually infer wrong.
    #[tokio::test]
    async fn a_door_with_no_handover_says_so_rather_than_omitting_it() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        make_bot(&jojobot, "gamma").await;

        let body = boot(&jojobot, "gamma").await;
        let handover = &body["session"]["handover"];
        assert!(
            !handover.is_null(),
            "nothing to hand over and nothing said about it: {body}",
        );
        assert_eq!(
            handover["story"],
            serde_json::Value::Null,
            "a story appeared where no run has been wrapped: {body}",
        );
        assert!(
            handover["note"].is_string(),
            "…and no words saying which kind of nothing this is: {body}",
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
            jojobot_domain::session::is_key(&handle),
            "…and it is a key: {handle}"
        );
        assert!(
            sid::is_readable(&run_of(&body).expect("the run is named")),
            "…beside a readable name: {body}"
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
                    timezone: None,
                    bot: EntityId("bot:gamma".into()),
                    sid: fixture_sid(line!() + nth as u32),
                    focus: focus.into(),
                    started_at: jiff::Timestamp::now(),
                    started_on: None,
                    id: None,
                    key_hash: None,
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
            let handle = choice["run"].as_str().expect("each option is addressable");
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
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "reading the hand-off".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");

        let offered = boot(&jojobot, "gamma").await;
        let offer = offered["session"]["choices"][0]["run"]
            .as_str()
            .expect("one option")
            .to_string();

        let resumed = boot_answering(&jojobot, "gamma", &offer).await;
        assert_eq!(
            run_of(&resumed).as_deref(),
            Some(offer.as_str()),
            "the run keeps its name: {resumed}"
        );
        assert!(
            sid_of(&resumed).is_some_and(|key| jojobot_domain::session::is_key(&key)),
            "…and is handed a key: {resumed}"
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
            boot(&jojobot, "gamma").await["session"]["choices"][0]["run"],
            offer.as_str()
        );

        let fresh = boot_answering(&jojobot, "gamma", sid::NEW).await;
        let minted = run_of(&fresh).unwrap_or_else(|| panic!("new mints one: {fresh}"));
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

    /// **A run survives the process that minted it, because the card holds its
    /// name and the hash of its key.** This is the restart cliff closed: the
    /// registry is rebuilt from the board before anything is served, so the key
    /// a caller wrote down yesterday still opens its run today.
    ///
    /// It matters beyond convenience. The key is the credential every later
    /// verb carries, so a key that died with the process meant a deploy
    /// silently re-pointed every agent at nothing.
    #[tokio::test]
    async fn a_handle_written_on_the_card_survives_a_restart() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::booted());
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
            Some(run_name(&jojobot, &handle).as_str()),
            "the card carries the name the door handed out: {card:?}"
        );
        assert_eq!(
            card.key_hash.as_deref(),
            Some(jojobot_domain::session::hash_key(&handle).as_str()),
            "…and the hash of the key, never the key: {card:?}"
        );

        // A restart: same board, an empty registry, filled from the board before
        // the first request — exactly what the composition root does.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        let board = store.all_sessions().await.expect("board read ok");
        assert_eq!(
            rebuilt.rebuild_from(&board).recovered,
            1,
            "one handle recovered"
        );
        let restarted = connection_sharing(memory, store.clone(), rebuilt);

        // The key still opens the run it opened before the restart.
        assert_eq!(
            run_name(&restarted, &handle),
            run_name(&jojobot, &handle),
            "the same key, still opening the same run"
        );
        let resumed = boot_answering(&restarted, "gamma", &handle).await;
        assert_eq!(
            run_of(&resumed),
            Some(run_name(&jojobot, &handle)),
            "the same run, still addressed by the same name: {resumed}"
        );
        assert_eq!(resumed["session"]["session"]["id"], card.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["chronology"][0]["text"],
            "read the hand-off"
        );
    }

    /// **A moved day survives a restart, exactly as a moved zone already
    /// does.** Before this, a resume's `today` only ever reached the
    /// in-process registry — `rebuild_from` reads `started_on`, the creation
    /// day, off the board, so a restart silently reverted any day a resume
    /// had moved. Mirrors
    /// [`a_handle_written_on_the_card_survives_a_restart`]'s own shape.
    #[tokio::test]
    async fn a_moved_day_survives_a_restart() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = connection_sharing(
            memory.clone(),
            store.clone(),
            Arc::new(sid::SessionRegistry::new()),
        );
        seed_bot(&memory, "gamma").await;

        let handle = sid_of(&boot_on(&jojobot, "gamma", "2026-03-15").await).expect("a handle");
        journal_entry(&jojobot, &handle, "started").await;

        // Picked up again, stating a different day — the device hop a
        // resume's `today` argument is for.
        let handle = sid_of(
            &boot_answering_dated(&jojobot, "gamma", &handle, Some("2026-04-01"), None).await,
        )
        .expect("a resume hands back the run's new key");

        // A restart: same board, an empty registry, filled from the board
        // before the first request.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        let board = store.all_sessions().await.expect("board read ok");
        rebuilt.rebuild_from(&board);
        let restarted = connection_sharing(memory, store, rebuilt);

        let resumed = boot_answering(&restarted, "gamma", &handle).await;
        let note = resumed["session"]["day_note"]
            .as_str()
            .unwrap_or_else(|| panic!("a day note: {resumed}"));
        assert!(
            note.contains("2026-04-01"),
            "the moved day did not survive the restart: {note}"
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
                    request_id: None,
                    working_on: None,
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
            run_name(&client.call(), &door_gave),
            "…and it is the caller's OWN run, because the key rides the write: jojobot no \
             longer has to guess which of several booted agents is writing"
        );
        assert_eq!(
            card.key_hash.as_deref(),
            Some(jojobot_domain::session::hash_key(&door_gave).as_str()),
            "…and the card holds the hash of the key the door handed out, never the key"
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
        assert_eq!(rebuilt.rebuild_from(&board).recovered, 1);
        assert_eq!(
            rebuilt.lookup_name(stored.as_str()).expect("held").card,
            Some(card.id.clone())
        );
    }

    /// **A card written before names were persisted carries none**, and that is
    /// not a broken card: it goes by its own id, holds no key, and is offered
    /// by that name. The migration is a no-op *only because* the id already is
    /// a public name — stated here so nobody later "simplifies" the offer into
    /// requiring a stored name.
    #[tokio::test]
    async fn a_card_with_no_stored_handle_is_offered_one_on_the_spot() {
        let store = Arc::new(InMemorySessions::new());
        let registry = Arc::new(sid::SessionRegistry::new());
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = connection_sharing(memory.clone(), store.clone(), registry.clone());
        seed_bot(&memory, "gamma").await;

        let legacy = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t900".into()),
                focus: "from before handles were stored".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
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
            registry.rebuild_from(&board).recovered,
            1,
            "a card with no stored name is taken in under its own id"
        );
        assert_eq!(
            registry
                .lookup_name(legacy.id.as_str())
                .expect("held under its id")
                .key,
            sid::KeyState::Void,
            "…holding no key"
        );

        let offered = boot(&jojobot, "gamma").await;
        let choice = &offered["session"]["choices"][0];
        let minted = choice["run"]
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
        let booted_run = boot(&client.call(), "gamma").await;
        let handle = run_of(&booted_run).expect("a run name");
        let key = sid_of(&booted_run).expect("a key");

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
            rebuilt.rebuild_from(&board).recovered,
            0,
            "the boot wrote no card, so the rebuild has nothing to recover: {board:?}"
        );
        let restarted = Jojobot::new(
            client.memory.clone(),
            Arc::new(SpySearch::default()),
            client.mailboxes.clone(),
            client.sessions.clone(),
            client.teachings.clone(),
            rebuilt,
        );
        let body = blocked(
            &restarted
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some(handle.clone()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("a dead handle is an answer, not a protocol failure"),
        );
        assert!(
            !body.to_string().contains(&key),
            "the refusal repeats nothing it was sent: {body}"
        );
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("No run goes by that name") && how.contains("start_here"),
            "that run is gone; boot again: {how}"
        );

        // The key of that run opens nothing either.
        let by_key = restarted
            .caller(Some(&key))
            .expect_err("the key opens nothing");
        assert!(
            !format!("{by_key:?}").contains(&key),
            "the refusal repeats nothing it was sent"
        );

        // An unreadable name is refused too — never repaired into a near one.
        let mistyped = blocked(
            &restarted
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("k3fo".into()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("an unreadable handle is an answer too"),
        );
        assert!(
            !mistyped.to_string().contains("k3fo"),
            "the refusal repeats nothing it was sent: {mistyped}"
        );
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

        let gammas = run_of(&boot(&jojobot, "gamma").await).expect("a run name");
        let body = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("delta".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some(gammas.clone()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
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
                    timezone: None,
                    bot: EntityId("bot:gamma".into()),
                    sid: fixture_sid(line!() + nth),
                    focus: focus.into(),
                    started_at: jiff::Timestamp::now(),
                    started_on: None,
                    id: None,
                    key_hash: None,
                })
                .await
                .expect("begin ok");
        }

        let offered = boot(&jojobot, "gamma").await;
        let handles: Vec<&str> = offered["session"]["choices"]
            .as_array()
            .expect("the offer")
            .iter()
            .map(|c| c["run"].as_str().expect("a handle"))
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
            choice["run"].as_str().expect("an addressable option"),
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
        let memory = Arc::new(InMemoryMemory::booted());
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
        let held = registry.mint_on_card(&EntityId("bot:gamma".into()), ancient.id.clone());
        let resumed = boot_answering(&jojobot, "gamma", held.name.as_str()).await;
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
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "still going".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
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
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = connection_sharing(memory.clone(), store.clone(), registry.clone());
        seed_bot(&memory, "gamma").await;

        let told = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(2),
                started_on: None,
                id: None,
                key_hash: None,
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

        let held = registry.mint_on_card(&EntityId("bot:gamma".into()), told.id.clone());
        let refused = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some(held.name.as_str().into()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("a wrapped run is an answer, not a protocol failure"),
        );
        let how = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("wrapped") && how.contains("story"),
            "the refusal says why this end is the last word: {how}"
        );
        assert!(
            how.contains("wrap_code"),
            "…and names the one way a wrapped run takes another change: {how}"
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
        let memory = Arc::new(InMemoryMemory::booted());
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
            choice["run"].as_str().expect("an addressable option"),
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

    /// **Two sequential boots of one identity, with nothing written between
    /// them, do not see each other.** The second gets a fresh handle, not a
    /// choice. Paired with
    /// [`a_granted_claim_alone_leaves_the_session_live_for_the_next_boot`]:
    /// naming a claim is what tells the two apart, because a GRANTED claim
    /// is itself a write.
    #[tokio::test]
    async fn two_sequential_boots_with_nothing_written_between_them_do_not_see_each_other() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::booted());
        let registry = crate::harness::seeded_registry();

        // A name other than "otto" — the harness's own fixture-writer
        // identity, whose boot inside `make_bot` would otherwise land a
        // session card on the very bot this case is trying to leave
        // untouched.
        let first = connection_sharing(memory.clone(), store.clone(), registry.clone());
        make_bot(&first, "milhouse").await;
        boot(&first, "milhouse").await;

        let second = connection_sharing(memory, store, registry);
        let after = boot(&second, "milhouse").await;
        assert!(
            sid_of(&after).is_some(),
            "two sequential boots with nothing written between them do not see each other: \
             {after}"
        );
        assert!(after["session"]["choices"].is_null(), "{after}");
    }

    /// **A GRANTED claim is itself a write, so it materializes a session
    /// record — the same lazy path any other first write uses.** A second
    /// boot of the same identity, with nothing else written, now finds that
    /// run live and is offered it, exactly as
    /// [`booting_again_is_offered_the_session_in_flight`] shows for a
    /// journal beat.
    #[tokio::test]
    async fn a_granted_claim_alone_leaves_the_session_live_for_the_next_boot() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::booted());
        let registry = crate::harness::seeded_registry();

        let first = connection_sharing(memory.clone(), store.clone(), registry.clone());
        make_bot(&first, "gamma").await;
        let claimed = json_of(
            &first
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("the boot call is ok"),
        );
        assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
        let holder_run = run_of(&claimed).expect("a run name");

        let second = connection_sharing(memory, store, registry);
        let offered = boot(&second, "gamma").await;
        assert!(
            sid_of(&offered).is_none(),
            "the choice comes first: {offered}"
        );
        let choice = &offered["session"]["choices"][0];
        assert_eq!(choice["run"], holder_run, "{offered}");
        assert_eq!(choice["working_on"], "claimed the dev-dispatch role");
    }

    /// **A REFUSED claim writes nothing new — the caller that lost the race
    /// gets no card for a run that never did anything.** Paired with the
    /// positive: the holder's own claim, granted moments earlier, does leave
    /// one.
    #[tokio::test]
    async fn a_refused_claim_leaves_no_session_record_beside_the_granted_one() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

        let first = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(first["session"]["claim"]["status"], "taken", "{first}");
        let holder_sid = sid_of(&first).expect("a handle");

        // Another bot naming the same role while the first lease is still
        // fresh: refused, because the role is gamma's.
        let second = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("delta".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(second["session"]["claim"]["status"], "refused", "{second}");
        assert_ne!(
            sid_of(&second).as_deref(),
            Some(holder_sid.as_str()),
            "a distinct session: {second}"
        );

        let rows = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(rows.len(), 1, "the granted claim left one row: {rows:?}");
        let refused_rows = store
            .sessions_of(&EntityId("bot:delta".into()))
            .await
            .expect("list ok");
        assert!(
            refused_rows.is_empty(),
            "the refused claim added none: {refused_rows:?}"
        );
    }

    /// **A resumed run's own day, told to a caller that named none of its
    /// own on THIS call.** Two shapes: the run already has one, set by an
    /// earlier call — and the run has none, so the server's clock answers.
    #[tokio::test]
    async fn a_resumed_run_names_its_own_day_when_the_caller_states_none() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "milhouse").await;

        // A run that stated its own day, then was resumed without one.
        let dated_boot = boot_on(&jojobot, "gamma", "2026-03-15").await;
        let dated_sid = sid_of(&dated_boot).expect("a handle");
        journal_entry(&jojobot, &dated_sid, "started").await;

        let resumed = boot_answering(&jojobot, "gamma", &dated_sid).await;
        let note = resumed["session"]["day_note"]
            .as_str()
            .unwrap_or_else(|| panic!("a day note: {resumed}"));
        assert!(
            note.contains("2026-03-15") && note.to_lowercase().contains("earlier call"),
            "{note}"
        );

        // A run with no stated day, resumed the same way.
        let undated_boot = boot(&jojobot, "milhouse").await;
        let undated_sid = sid_of(&undated_boot).expect("a handle");
        journal_entry(&jojobot, &undated_sid, "started").await;

        let resumed_undated = boot_answering(&jojobot, "milhouse", &undated_sid).await;
        let note = resumed_undated["session"]["day_note"]
            .as_str()
            .unwrap_or_else(|| panic!("a day note: {resumed_undated}"));
        assert!(
            note.to_lowercase().contains("no stated day") && note.to_lowercase().contains("clock"),
            "{note}"
        );
    }

    /// **A wrapped run for the cases below, with its window offered**, built
    /// below the surface: the run, its code, and the sid the registry addresses
    /// it by.
    async fn wrapped_with_a_code(
        store: &InMemorySessions,
        tens_of_minutes_ago: i64,
        sid: &str,
    ) -> (jojobot_domain::session::Session, String) {
        let run = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid(sid.into()),
                focus: "the run that wrapped".into(),
                // Ten-minute units, so the run was wrapped inside its window.
                started_at: jiff::Timestamp::now()
                    - jiff::SignedDuration::from_mins(tens_of_minutes_ago * 10),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        store
            .close(&run.id, jojobot_domain::session::SessionState::Wrapped)
            .await
            .expect("wrap ok");
        let code = jojobot_domain::session::mint_wrap_code();
        let offered = store
            .set_wrap_window(
                &run.id,
                Some(jojobot_domain::session::WrapWindow::Offered(code.clone())),
            )
            .await
            .expect("offered");
        (offered, code)
    }

    /// **Using a wrap code abandons no other run, takes no role and sweeps
    /// nothing.** A live run of the bot that is stale enough for the sweep is
    /// still active afterwards, which it would not be if the code went through
    /// the ordinary boot. A claim sent beside the code is not decided: the
    /// answer carries none, and no holder is written. Paired with the same
    /// boot WITHOUT the code, which sweeps the stale run: the sweep is real, so
    /// its absence above is the code's doing.
    #[tokio::test]
    async fn a_wrap_code_sweeps_nothing_and_takes_no_role() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let stale = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "left open two days ago".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        let (wrapped, code) = wrapped_with_a_code(&store, 1, "t002").await;

        let opened = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    today: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some(code.clone()),
                    sid: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(opened["session"]["resumed"], true, "{opened}");
        assert_eq!(opened["session"]["session"]["state"], "wrapped", "{opened}");
        assert!(
            opened["session"].get("claim").is_none(),
            "a claim sent beside a wrap code is not decided: {opened}"
        );
        assert!(
            opened["session"].get("swept").is_none(),
            "a wrap code sweeps nothing: {opened}"
        );
        let fields = jojobot
            .memory
            .fields(&EntityId("bot:gamma".into()))
            .await
            .expect("fields ok");
        assert!(
            !fields.contains_key("role/dev-dispatch/holder"),
            "no role was taken: {fields:?}"
        );
        assert_eq!(
            store.read_session(&stale.id).await.expect("read ok").state,
            jojobot_domain::session::SessionState::Active,
            "the stale live run was left alone"
        );
        let after = store.read_session(&wrapped.id).await.expect("read ok");
        assert_eq!(after.state, jojobot_domain::session::SessionState::Wrapped);
        assert!(
            after.wrap_window.as_ref().is_some_and(|w| w.is_open()),
            "the window is open: {after:?}"
        );

        // The paired boot, with no code: the sweep takes the stale run.
        let ordinary = boot(&jojobot, "gamma").await;
        assert_eq!(
            ordinary["session"]["swept"],
            serde_json::json!([stale.id.as_str()]),
            "without the code the same boot sweeps the stale run: {ordinary}"
        );
    }

    /// **A newer run ends the window even when the store could not clear it.**
    /// The clear is best-effort, so using the code checks for a newer run
    /// itself. The store here refuses to clear a window; the newer run exists;
    /// the code is refused anyway, with the route to a fresh run.
    #[tokio::test]
    async fn a_code_is_refused_when_a_newer_run_exists_even_if_its_window_could_not_be_cleared() {
        let store = Arc::new(RefusingWindowClear(InMemorySessions::new()));
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        make_bot(&jojobot, "gamma").await;
        let (_, code) = wrapped_with_a_code(&store.0, 5, "t003").await;
        store
            .0
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t004".into()),
                focus: "the newer run".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");

        let refused = boot_answering(&jojobot, "gamma", &code).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .unwrap_or_default()
                .contains("start_here"),
            "the refusal names the way to a fresh run: {refused}"
        );
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
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "something from the day before yesterday".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                started_on: None,
                id: None,
                key_hash: None,
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
            choice["run"].as_str().expect("an addressable option"),
        )
        .await;
        assert_eq!(resumed["session"]["session"]["id"], stale.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["state"], "active",
            "…and taking the offer reopens it: {resumed}"
        );
    }

    /// 🚨 **The boot says which frame it swept in, and names the way to change
    /// it.**
    ///
    /// The frame belongs to the caller, and a caller that does not know it may
    /// state one never will: it meets a list of its own runs, cannot tell which
    /// are over, and has nothing to tell it the list was decided on a clock. A
    /// run acting out months in minutes reads every sitting it ever had as
    /// still working, and the only place it could learn otherwise is here —
    /// **at the moment it is looking at the answer that is wrong.**
    ///
    /// ⛔️ **The essay is not where this lands.** A session that never reads it
    /// closely still meets this, because it is in the answer it already asked
    /// for.
    ///
    /// **Both halves.** A caller that stated nothing is told what was used and
    /// what to send; a caller that stated a day is told which day, and is not
    /// handed advice about an argument it already used.
    #[tokio::test]
    async fn a_boot_says_which_frame_it_swept_in_and_how_to_state_another() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let quiet = boot(&jojobot, "gamma").await;
        let said = quiet["session"]["swept_in"].to_string();
        // **The argument name is read off the served schema**, so a rename
        // reaches this case and a name no verb publishes fails it.
        let published = crate::arguments::published_argument_names();
        assert!(
            published.contains("today"),
            "no verb publishes `today`, so this case is pinning an argument that does not \
             exist: {published:?}",
        );
        assert!(
            said.contains("today"),
            "a boot that swept on the clock does not name the argument that would have stated \
             a frame, so a run acting out months has no way to learn the list is wrong: {said}",
        );

        let stated = boot_on(&jojobot, "gamma", "2026-09-13").await;
        let told = stated["session"]["swept_in"].to_string();
        assert!(
            told.contains("2026-09-13"),
            "a boot that stated a day is not told which day it swept in: {told}",
        );
        assert!(
            !told.contains("today"),
            "a caller that already sent the argument is being told to send it: {told}",
        );
    }

    /// **A plain boot carries no `swept_in`; every boot that needs the frame
    /// still does.** The note exists for a run that may be in a different frame
    /// than the one the sweep judged by. A boot on the real clock that sent its
    /// own zone and swept nothing has no such run, so it is not told. A boot that
    /// stated a day, one that swept a run, one on a server acting out a day and
    /// one that sent no zone of its own (the day then came from the instance's
    /// zone, not from the caller) each keep it.
    #[tokio::test]
    async fn a_plain_boot_carries_no_swept_in_and_each_boot_that_needs_it_does() {
        let zone = "America/New_York";
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let plain = boot_in_zone(&jojobot, "gamma", zone, None).await;
        assert!(
            plain["session"]["sid"].is_string(),
            "the plain boot began a run, so the absence below is about the note: {plain}"
        );
        assert!(plain["session"].get("swept_in").is_none(), "{plain}");

        // A day was stated: the answer says which day the sweep used.
        let stated = boot_in_zone(&jojobot, "gamma", zone, Some("2026-09-13")).await;
        assert_eq!(
            stated["session"]["swept_in"]["day"], "2026-09-13",
            "{stated}"
        );

        // The caller sent no zone: the instance's own decided the day.
        let no_zone = boot(&jojobot, "gamma").await;
        assert!(
            no_zone["session"]["swept_in"]["note"].is_string(),
            "{no_zone}"
        );

        // A run was swept.
        make_bot(&jojobot, "delta").await;
        let stale = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:delta".into()),
                sid: Sid("t002".into()),
                focus: "something from the day before yesterday".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        let swept = boot_in_zone(&jojobot, "delta", zone, None).await;
        assert_eq!(
            swept["session"]["swept"],
            serde_json::json!([stale.id.as_str()]),
            "{swept}"
        );
        assert!(swept["session"]["swept_in"]["note"].is_string(), "{swept}");

        // A server acting out a day.
        let day: jiff::civil::Date = "2026-06-01".parse().expect("a day");
        let acting = with_sessions(Arc::new(InMemorySessions::new()))
            .on_clock(jojobot_domain::clock::Clock::stating(day));
        make_bot(&acting, "gamma").await;
        let booted = boot_in_zone(&acting, "gamma", zone, None).await;
        assert_eq!(
            booted["session"]["swept_in"]["day"], "2026-06-01",
            "{booted}"
        );
    }

    /// ⛔️ **A server acting out a day does not advise sending the day it is
    /// already in.**
    ///
    /// Every word of the clock note is TRUE on such a server — the run stated
    /// nothing and the server's clock decided the sweep — and it is still
    /// wrong to serve, because the way out it names is a thing the caller does
    /// not need to do. **Advice pointing at a step that does not apply reads as
    /// a fault**, and a run acting out a year meets this sentence at every
    /// sitting.
    ///
    /// **Three halves, because the third is what makes it a change rather than
    /// a rewording**: the server's day is named, the argument is not pressed on
    /// a caller who does not need it, and the ordinary server still gets the
    /// advice it always did.
    #[tokio::test]
    async fn a_boot_on_a_server_acting_out_a_day_names_that_day_rather_than_asking_for_one() {
        let day: jiff::civil::Date = "2026-06-01".parse().expect("a day");
        let store = Arc::new(InMemorySessions::new());
        let jojobot =
            with_sessions(store.clone()).on_clock(jojobot_domain::clock::Clock::stating(day));
        make_bot(&jojobot, "gamma").await;

        let acting = boot(&jojobot, "gamma").await;
        let said = acting["session"]["swept_in"].to_string();
        assert!(
            said.contains(&day.to_string()),
            "a boot on a server acting out a day does not say which day it swept in: {said}",
        );
        assert!(
            !said.contains("send the day you are in as `today` and"),
            "a caller on a server already in its day is being told to send that day: {said}",
        );

        // The ordinary server is unchanged, which is what says the arm above is
        // an addition rather than the advice being dropped for everybody.
        let ordinary = with_sessions(Arc::new(InMemorySessions::new()));
        make_bot(&ordinary, "gamma").await;
        let plain = boot(&ordinary, "gamma").await["session"]["swept_in"].to_string();
        assert!(
            plain.contains("send the day you are in as `today` and"),
            "a boot on the real clock stopped naming the way to state a frame: {plain}",
        );
    }

    /// 🚨 **A sweep that closed nothing does not read like a sweep that could
    /// not run.**
    ///
    /// Both leave `swept` empty. One means every run is still working; the
    /// other means a stale run is still standing because the store refused to
    /// retire it — and the second is the one a reader has to act on. Told apart
    /// only by a log line, the surface says the same thing for both, and the
    /// caller is looking at a run it will be offered again on every boot with
    /// nothing saying why.
    ///
    /// **Both halves, because either alone passes on a build that reports
    /// neither.**
    #[tokio::test]
    async fn a_sweep_that_closed_nothing_is_not_a_sweep_that_could_not_run() {
        let (jojobot, store, _memory, _sid) = crate::session::testing::refusing_close().await;
        // This double refuses until it is told not to, so the half that must
        // find nothing to sweep is taken with it agreeing.
        store
            .refuse
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // Nothing stale: one run, an hour old, which is quiet rather than over.
        let quiet = boot(&jojobot, "gamma").await;
        assert_eq!(quiet["session"]["swept"], serde_json::json!([]));
        assert_eq!(
            quiet["session"]["unswept"],
            serde_json::json!([]),
            "a boot that found nothing stale reported something it could not sweep",
        );

        // Now a run that is plainly over, and a store that will not retire it.
        store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "long over".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        store
            .refuse
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let refused = boot(&jojobot, "gamma").await;
        assert_eq!(
            refused["session"]["swept"],
            serde_json::json!([]),
            "the store refused, so nothing was closed",
        );
        assert_ne!(
            refused["session"]["unswept"],
            serde_json::json!([]),
            "a stale run the store refused to retire is reported nowhere a caller reads, so it \
             is offered again on every boot with nothing saying why",
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
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "still going".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(1),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(booted["session"]["choices"][0]["working_on"], "still going");
        assert_eq!(booted["session"]["swept"], serde_json::json!([]));

        let resumed = boot_answering(
            &jojobot,
            "gamma",
            booted["session"]["choices"][0]["run"]
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
                request_id: None,
                bot: "otto".into(),
                prose: "Keeps the schedule.\n\nHard line: never writes to the ledger.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");
        jojobot
            .capture(Parameters(CaptureArgs {
                provenance: Some("testimony".into()),
                // A boot only carries a marked rule (rule 306) — this test
                // is about the rule arriving at all, not about the cap.
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
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
                    timezone: None,
                    bot: EntityId("bot:gamma".into()),
                    sid: fixture_sid(line!()),
                    focus: "from the day before yesterday".into(),
                    started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                    started_on: None,
                    id: None,
                    key_hash: None,
                })
                .await
                .expect("begin ok");

            let booting = jojobot.start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }));
            let writing = jojobot.journal(Parameters(JournalArgs {
                request_id: None,
                working_on: None,
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
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }));
            let writing = jojobot.journal(Parameters(JournalArgs {
                request_id: None,
                working_on: None,
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
                        choice["run"].as_str(),
                        Some(run_name(&jojobot, &sid).as_str()),
                        "boot_first={boot_first}: the offer minted a second handle for a run that \
                         already has one: {choice}"
                    );
                }
            }
        }
    }

    /// **The bound is on the payload, so the payload is what is measured.**
    ///
    /// An entry does not travel as its text. It travels as an object with an
    /// id, a timestamp and a beat around it, and that envelope is most of a
    /// short entry. Sized by the text alone, a chronology of short beats served
    /// several times the budget while every other check on it passed — the cap
    /// was there, the number it held was not the number that ships.
    #[tokio::test]
    async fn short_beats_are_bounded_by_what_they_render_as() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        // Short enough that the text alone fits the budget several times over:
        // whatever is over it comes from the envelope.
        for nth in 0..200 {
            journal_entry(&jojobot, &sid, &format!("beat {nth:03}")).await;
        }

        let resumed = boot_answering(&jojobot, "gamma", &sid).await;
        let chronology = resumed["session"]["session"]["chronology"]
            .as_array()
            .expect("a chronology")
            .clone();

        let text: usize = chronology
            .iter()
            .map(|e| e["text"].as_str().expect("an entry's text").chars().count())
            .sum();
        let served: usize = chronology
            .iter()
            .map(|e| e.to_string().chars().count())
            .sum();
        assert!(
            served <= jojobot_domain::text::SESSION_CHRONOLOGY.budget,
            "the served entries are {served} characters against a budget of {}, and their text \
             is only {text} of it",
            jojobot_domain::text::SESSION_CHRONOLOGY.budget
        );
        // The positive the bound depends on: a cap that served nothing, or that
        // dropped the newest beat, would satisfy the assertion above.
        assert!(
            chronology.last().expect("a resume carries beats")["text"]
                .as_str()
                .expect("an entry's text")
                .starts_with("beat 199"),
            "the newest beat survives the cap: {chronology:?}"
        );
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
        // reason for the cap. This bot carries no rules and, being named,
        // never gets the essay's remainder at all — only the core, riding in
        // the floor — so this is [`text::BOOT_ANSWER`]'s own declared
        // ceiling, the real architectural bound, not a looser one: a named
        // boot's answer was never going to approach it once the remainder
        // stopped being a candidate for one.
        let whole = resumed.to_string().chars().count();
        assert!(
            whole <= jojobot_domain::text::BOOT_ANSWER.budget,
            "a resumed boot is {whole} characters"
        );
    }

    /// 🚨 **The strict-cap bar: a rules-free identity's boot fits the ONE
    /// declared ceiling, and still carries the essay's core.**
    ///
    /// A rules-free named bot with a long chronology is the exact shape that
    /// used to defeat the ceiling: the essay's remainder was the one
    /// candidate `Capped::head` ranked here, and `head` always takes its
    /// first candidate whatever it costs — so the remainder shipped whole
    /// however far past the budget it went. **A named boot no longer ranks
    /// the remainder at all** — see `essay_for_boot` — so this case is now
    /// unconditional rather than a close call: the core rides in the floor
    /// regardless of chronology weight, and the chronology's own cap
    /// (`SESSION_CHRONOLOGY`) is what keeps the rest of the answer inside
    /// the ceiling.
    ///
    /// **The positive is not optional**: a build that served no orientation
    /// at all for every rules-free bot would pass the ceiling assertion
    /// alone. The core is what a session needs to do anything, and a named
    /// boot always gets it.
    #[tokio::test]
    async fn a_rules_free_bot_with_a_long_chronology_fits_the_ceiling_and_still_gets_its_core() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        for nth in 0..20 {
            journal_entry(
                &jojobot,
                &sid,
                &format!("beat {nth:02} {}", "w".repeat(1500)),
            )
            .await;
        }

        let resumed = boot_answering(&jojobot, "gamma", &sid).await;
        let whole = resumed.to_string().chars().count();
        assert!(
            whole <= jojobot_domain::text::BOOT_ANSWER.budget,
            "the whole answer must fit the one declared ceiling: {whole} chars"
        );

        let orientation = resumed["orientation"]
            .as_str()
            .expect("a rules-free bot still gets the essay's core: {resumed}");
        assert_eq!(
            orientation,
            crate::orientation::essay::ORIENTATION_CORE.as_str(),
            "the core must ride whole even when the remainder is declined entirely",
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

    /// **A boot reads the runs of its bot once, whichever way it goes.** The
    /// sweep already holds every run, so ending the wrap windows of an older
    /// run has to use that read rather than ask the store for the same list
    /// again. Three routes end windows — a first boot, the answer `new` to an
    /// offer, and a wrap code refused because a newer run exists — and each is
    /// counted and paired with the windows actually having ended, so a boot
    /// that read once and ended nothing does not pass.
    #[tokio::test]
    async fn a_boot_reads_the_runs_of_its_bot_once_and_still_ends_their_windows() {
        async fn window_of(
            store: &InMemorySessions,
            run: &jojobot_domain::session::Session,
        ) -> Option<jojobot_domain::session::WrapWindow> {
            store
                .read_session(&run.id)
                .await
                .expect("read ok")
                .wrap_window
        }

        // A first boot: nothing live, two wrapped runs holding windows.
        let counting = Arc::new(CountingSessions::new(InMemorySessions::new()));
        let (jojobot, _memory) = with_sessions_port_and_memory(counting.clone());
        make_bot(&jojobot, "gamma").await;
        let (first, _) = wrapped_with_a_code(counting.store(), 5, "t010").await;
        let (second, _) = wrapped_with_a_code(counting.store(), 4, "t011").await;
        let before = counting.reads();
        let booted = boot(&jojobot, "gamma").await;
        assert!(
            sid_of(&booted).is_some(),
            "a first boot hands back a sid: {booted}"
        );
        assert_eq!(
            counting.reads() - before,
            1,
            "a first boot reads the runs once"
        );
        for run in [&first, &second] {
            assert!(
                window_of(counting.store(), run).await.is_none(),
                "…and the older run's window ended: {run:?}"
            );
        }

        // The answer `new` to an offer.
        let counting = Arc::new(CountingSessions::new(InMemorySessions::new()));
        let (jojobot, _memory) = with_sessions_port_and_memory(counting.clone());
        make_bot(&jojobot, "gamma").await;
        let (wrapped, _) = wrapped_with_a_code(counting.store(), 5, "t012").await;
        counting
            .store()
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t013".into()),
                focus: "a run still going".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        let offer = boot(&jojobot, "gamma").await;
        assert!(
            sid_of(&offer).is_none(),
            "a live run makes the boot an offer: {offer}"
        );
        let before = counting.reads();
        let answered = boot_answering(&jojobot, "gamma", "new").await;
        assert!(
            sid_of(&answered).is_some(),
            "the answer hands back a sid: {answered}"
        );
        assert_eq!(
            counting.reads() - before,
            1,
            "answering `new` reads the runs once"
        );
        assert!(
            window_of(counting.store(), &wrapped).await.is_none(),
            "…and the older run's window ended"
        );

        // A wrap code refused because a newer run exists.
        let counting = Arc::new(CountingSessions::new(InMemorySessions::new()));
        let (jojobot, _memory) = with_sessions_port_and_memory(counting.clone());
        make_bot(&jojobot, "gamma").await;
        let (wrapped, code) = wrapped_with_a_code(counting.store(), 5, "t014").await;
        counting
            .store()
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t015".into()),
                focus: "the newer run".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        let before = counting.reads();
        let refused = boot_answering(&jojobot, "gamma", &code).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(
            counting.reads() - before,
            1,
            "a refused code reads the runs once"
        );
        assert!(
            window_of(counting.store(), &wrapped).await.is_none(),
            "…and the older run's window ended"
        );
    }

    /// **A resume whose revocation fails is refused, issues no key, and leaves
    /// the run exactly as it was.** The listing, a read and a write of the
    /// run's vouchers each fail in turn. The sitting that holds the old key
    /// goes on working with it, the voucher it holds is still held, and once the
    /// store answers again the same resume goes through and the old key stops
    /// opening the run.
    #[tokio::test]
    async fn a_resume_whose_vouchers_cannot_all_be_voided_issues_no_key() {
        use crate::session::scripted::Scripted;
        use jojobot_domain::voucher::{Voucher, VoucherState};
        for step in ["list", "read", "write"] {
            let scripted = Arc::new(Scripted::over(Arc::new(InMemoryMemory::seeded())));
            let jojobot = Jojobot::new(
                scripted.clone(),
                Arc::new(crate::memory::testing::SpySearch::default()),
                Arc::new(InMemoryMailboxes::knowing_any_owner()),
                Arc::new(InMemorySessions::new()),
                Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
                seeded_registry(),
            );
            name_the_operator(&jojobot, "milhouse").await;
            make_bot(&jojobot, "gamma").await;
            let key = booted(&jojobot, "gamma").await;
            journal_entry(&jojobot, &key, "started").await;
            let name = run_name(&jojobot, &key);
            let held = Voucher {
                right: "boot-as".into(),
                target: "bot:gamma".into(),
                holder: name.clone(),
                issuer: "person:milhouse".into(),
                parent: None,
                state: VoucherState::Held,
            };
            let day = jiff::civil::date(2026, 10, 9);
            let grant = jojobot.issue_voucher(&held, day).await.expect("issued");
            scripted.script(|s| match step {
                "list" => s.listing_fails_for = Some(EntityKind::VOUCHER),
                "read" => s.recall_fails_for = Some(grant.clone()),
                _ => s.update_fails = true,
            });

            let refused = boot_answering(&jojobot, "gamma", &name).await;
            assert_eq!(refused["status"], "blocked", "{step}: {refused}");
            assert!(
                refused["session"]["sid"].is_null() && refused.get("sid").is_none(),
                "{step}: a refused resume handed out a key: {refused}"
            );
            assert!(
                !refused.to_string().contains(&key),
                "{step}: the refusal repeats the key"
            );
            // The sitting that holds the key goes on, and the voucher is as it was.
            journal_entry(&jojobot, &key, "still here").await;
            let state = jojobot.memory.fields(&grant).await.expect("fields")["state"].clone();
            assert_eq!(state, "held", "{step}");

            // The positive: the same resume, once the store answers.
            scripted.script(|s| *s = Default::default());
            let taken = boot_answering(&jojobot, "gamma", &name).await;
            let new_key = taken["session"]["sid"]
                .as_str()
                .unwrap_or_else(|| panic!("{step}: the resume goes through: {taken}"))
                .to_string();
            assert_ne!(new_key, key, "{step}");
            let state = jojobot.memory.fields(&grant).await.expect("fields")["state"].clone();
            assert_eq!(state, "void", "{step}");
            assert!(
                jojobot.caller(Some(&key)).is_err(),
                "{step}: the old key still opens the run"
            );
            assert_eq!(run_name(&jojobot, &new_key), name, "{step}");
        }
    }

    /// **A wrap code for a run the registry has not met hands back a key that
    /// opens it.** The wrapped run is on the board and the registry never
    /// learned it, as when another writer began it after the last rebuild. The
    /// code takes the run in before it issues the key, so the key it hands back
    /// is held by the registry and opens the run.
    #[tokio::test]
    async fn a_wrap_code_for_a_run_the_registry_never_met_hands_back_a_key_that_opens_it() {
        let store = Arc::new(InMemorySessions::new());
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        make_bot(&jojobot, "gamma").await;
        let (_wrapped, code) = wrapped_with_a_code(&store, 5, "bcd345").await;

        let answered = boot_answering(&jojobot, "gamma", &code).await;
        assert_eq!(answered["session"]["resumed"], true, "{answered}");
        let key = answered["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("the code hands back a key: {answered}"))
            .to_string();
        assert_eq!(
            run_name(&jojobot, &key),
            "bcd345",
            "the key opens the run it was issued for"
        );
    }

    /// **A wrap code is good for two hours after the wrap and no longer.** The
    /// same window, offered by the same call, differs only in how long ago the run
    /// was wrapped: fifty minutes opens the run, five hours is refused, and the
    /// refusal says when the window closed and leaves the window as it was. A
    /// window an older build left standing is judged the same way, because nothing
    /// is stored of it beyond the wrap's own moment.
    #[tokio::test]
    async fn a_wrap_code_is_good_for_two_hours_after_the_wrap_and_not_after() {
        for (tens_of_minutes, opens) in [(5, true), (30, false)] {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = with_sessions(store.clone());
            make_bot(&jojobot, "gamma").await;
            let (wrapped, code) = wrapped_with_a_code(&store, tens_of_minutes, "bcd345").await;

            let answer = boot_answering(&jojobot, "gamma", &code).await;
            if opens {
                assert!(
                    answer["session"]["sid"].is_string(),
                    "a wrap inside the window answers to its code: {answer}"
                );
                continue;
            }
            assert_eq!(answer["status"], "blocked", "{answer}");
            let closed = wrapped
                .window_ends_at()
                .expect("a run with a window has an end")
                .to_zoned(jiff::tz::TimeZone::UTC)
                .strftime("%Y-%m-%d %H:%M")
                .to_string();
            assert!(
                answer["how_to_proceed"]
                    .as_str()
                    .is_some_and(|how| how.contains(&closed)),
                "the refusal says when the window closed ({closed}): {answer}"
            );
            assert_eq!(
                store
                    .read_session(&wrapped.id)
                    .await
                    .expect("read ok")
                    .wrap_window,
                wrapped.wrap_window,
                "the refused code left the window as it was"
            );
        }
    }

    /// **A key that cannot be issued leaves the code unspent.** The store
    /// refuses the new key, so the code is refused with its window as it was and
    /// no key handed out, and the holder of the code can try again once the
    /// store answers.
    #[tokio::test]
    async fn a_wrap_code_whose_key_cannot_be_issued_is_not_spent() {
        let store = Arc::new(crate::session::testing::Refusing::refusing_keys());
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        make_bot(&jojobot, "gamma").await;
        let (wrapped, code) = wrapped_with_a_code(&store.inner, 5, "bcd345").await;

        let refused = boot_answering(&jojobot, "gamma", &code).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused["session"]["sid"].is_null() && refused.get("sid").is_none(),
            "a refused code handed out a key: {refused}"
        );
        let window = store
            .inner
            .read_session(&wrapped.id)
            .await
            .expect("read ok")
            .wrap_window;
        assert_eq!(
            window,
            Some(jojobot_domain::session::WrapWindow::Offered(code)),
            "the code was spent although no key was issued"
        );
    }

    /// **A window that cannot be opened takes the key it just issued away.** The
    /// key is written before the window opens, so a store that then refuses the
    /// window must not leave a live key on a run whose code was refused: the
    /// refusal says nothing was reopened. The code is still good, and the same
    /// call goes through once the store answers.
    #[tokio::test]
    async fn a_wrap_code_whose_window_cannot_be_opened_leaves_no_live_key_and_goes_through_on_retry()
     {
        let store = Arc::new(crate::session::testing::Refusing::refusing_nothing());
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        make_bot(&jojobot, "gamma").await;
        let (wrapped, code) = wrapped_with_a_code(&store.inner, 5, "bcd345").await;
        store
            .open_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let refused = boot_answering(&jojobot, "gamma", &code).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused["session"]["sid"].is_null() && refused.get("sid").is_none(),
            "a refused code handed out a key: {refused}"
        );
        assert_eq!(
            jojobot
                .registry
                .lookup_name("bcd345")
                .expect("the registry holds the run")
                .key,
            sid::KeyState::Void,
            "the key issued for the window that did not open is still live"
        );
        let stored = store
            .inner
            .read_session(&wrapped.id)
            .await
            .expect("read ok");
        assert_eq!(
            stored.key_hash.as_deref(),
            Some(jojobot_domain::session::KEY_VOID)
        );
        assert!(
            !stored.wrap_window.as_ref().is_some_and(|w| w.is_open()),
            "the window opened although the store refused"
        );

        store
            .open_fails
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let taken = boot_answering(&jojobot, "gamma", &code).await;
        let key = taken["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("the code goes through on retry: {taken}"))
            .to_string();
        assert_eq!(run_name(&jojobot, &key), "bcd345");
    }

    /// **A key is issued only for a run the registry holds.** A name nothing
    /// holds has no entry to set a key on and no card to write it to, so a key
    /// for it would open nothing.
    #[tokio::test]
    async fn rekey_refuses_a_name_the_registry_does_not_hold() {
        let jojobot = Jojobot::new(
            Arc::new(InMemoryMemory::seeded()),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        let refused = jojobot
            .rekey(&Sid("zzzzzz".into()), sid::RetiredBy::Wrapped)
            .await;
        assert!(
            refused.is_err(),
            "a key was issued for a name nothing holds"
        );
    }

    /// **A resume refused before the key moves leaves a stopped run stopped.**
    /// The run was abandoned; what it was taught cannot be forgotten, so the
    /// resume is refused and says the run is as it was. The run must be as it
    /// was: still abandoned, and still opened by its old key.
    #[tokio::test]
    async fn a_resume_refused_before_the_key_moves_leaves_a_stopped_run_stopped() {
        struct CannotForget(jojobot_domain::teaching::testing::InMemoryTeachings);
        #[jojobot_domain::forward(jojobot_domain::forward_teachings, via = 0)]
        impl Teachings for CannotForget {
            async fn forget(&self, _sid: &Sid) -> Result<(), TeachingError> {
                Err(TeachingError::Store("the store is down".into()))
            }
        }

        let store = Arc::new(InMemorySessions::new());
        let jojobot = Jojobot::new(
            Arc::new(InMemoryMemory::seeded()),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            store.clone(),
            Arc::new(CannotForget(
                jojobot_domain::teaching::testing::InMemoryTeachings::new(),
            )),
            seeded_registry(),
        );
        name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        let key = booted(&jojobot, "gamma").await;
        journal_entry(&jojobot, &key, "started").await;
        let name = run_name(&jojobot, &key);
        let card = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("the runs")[0]
            .id
            .clone();
        store
            .close(&card, SessionState::Abandoned)
            .await
            .expect("the sweep ended the run");

        let refused = boot_answering(&jojobot, "gamma", &name).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused["session"]["sid"].is_null() && refused.get("sid").is_none(),
            "a refused resume handed out a key: {refused}"
        );
        assert_eq!(
            store.read_session(&card).await.expect("read ok").state,
            SessionState::Abandoned,
            "a refusal that says the run is as it was reopened it"
        );
        assert!(
            jojobot.caller(Some(&key)).is_ok(),
            "the old key stopped opening a run the refusal left alone"
        );
    }

    /// **A reopen that the store refuses comes after the key moved, and the
    /// same resume goes through once the store answers.** The refusal hands out
    /// no key and leaves the run stopped; the next resume reopens it.
    #[tokio::test]
    async fn a_resume_whose_reopen_is_refused_leaves_the_run_stopped_and_goes_through_on_retry() {
        let store = Arc::new(crate::session::testing::Refusing::refusing_reopens());
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        let key = booted(&jojobot, "gamma").await;
        journal_entry(&jojobot, &key, "started").await;
        let name = run_name(&jojobot, &key);
        let card = store
            .inner
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("the runs")[0]
            .id
            .clone();
        store
            .inner
            .close(&card, SessionState::Abandoned)
            .await
            .expect("the sweep ended the run");

        let refused = boot_answering(&jojobot, "gamma", &name).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused["session"]["sid"].is_null() && refused.get("sid").is_none(),
            "a refused resume handed out a key: {refused}"
        );
        assert_eq!(
            store
                .inner
                .read_session(&card)
                .await
                .expect("read ok")
                .state,
            SessionState::Abandoned,
            "the run was reopened although the store refused"
        );

        store
            .reopen_fails
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let taken = boot_answering(&jojobot, "gamma", &name).await;
        let new_key = taken["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("the resume goes through: {taken}"))
            .to_string();
        assert_eq!(run_name(&jojobot, &new_key), name);
        assert_eq!(
            store
                .inner
                .read_session(&card)
                .await
                .expect("read ok")
                .state,
            SessionState::Active
        );
    }

    /// **A wrapped run is refused before anything about it changes.** Its story
    /// is told and the end is the last word, so the resume says so up front: the
    /// vouchers it holds, what it was taught and its key state are as they were.
    #[tokio::test]
    async fn a_wrapped_run_resumed_by_its_name_is_refused_with_its_key_state_as_it_was() {
        let store = Arc::new(InMemorySessions::new());
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        make_bot(&jojobot, "gamma").await;
        let (wrapped, _code) = wrapped_with_a_code(&store, 5, "bcd345").await;
        store
            .set_key_hash(
                &wrapped.id,
                jojobot_domain::session::KEY_VOID,
                &Default::default(),
            )
            .await
            .expect("the wrap took the key away");
        let board = store.all_sessions().await.expect("the board");
        jojobot.registry.rebuild_from(&board);

        let refused = boot_answering(&jojobot, "gamma", "bcd345").await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused["session"]["sid"].is_null() && refused.get("sid").is_none(),
            "a refused resume handed out a key: {refused}"
        );
        assert_eq!(
            jojobot
                .registry
                .lookup_name("bcd345")
                .expect("the registry holds the run")
                .key,
            sid::KeyState::Void,
            "a refused resume replaced the key of a wrapped run"
        );
    }

    /// **A pick-up whose teaching cannot be forgotten is refused and changes
    /// nothing.** The old key still opens the run, so the sitting that holds it
    /// is not cut off by a store that was down.
    #[tokio::test]
    async fn a_pick_up_that_cannot_forget_what_the_run_was_taught_is_refused() {
        struct CannotForget(jojobot_domain::teaching::testing::InMemoryTeachings);
        #[jojobot_domain::forward(jojobot_domain::forward_teachings, via = 0)]
        impl Teachings for CannotForget {
            async fn forget(&self, _sid: &Sid) -> Result<(), TeachingError> {
                Err(TeachingError::Store("the store is down".into()))
            }
        }

        let jojobot = Jojobot::new(
            Arc::new(InMemoryMemory::seeded()),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(CannotForget(
                jojobot_domain::teaching::testing::InMemoryTeachings::new(),
            )),
            seeded_registry(),
        );
        name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        let key = booted(&jojobot, "gamma").await;
        journal_entry(&jojobot, &key, "started").await;
        let name = run_name(&jojobot, &key);

        let refused = boot_answering(&jojobot, "gamma", &name).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(refused.to_string().contains(&name), "{refused}");
        assert!(
            refused["session"]["sid"].is_null(),
            "no key was handed out: {refused}"
        );
        assert!(
            jojobot.caller(Some(&key)).is_ok(),
            "the old key still opens the run"
        );
    }

    /// **Two pick-ups of one quiet run, started together, hand a key to one.**
    /// The first is parked in the gap between the quiet check and the new key —
    /// the voucher listing that comes between them waits on a hold — and the
    /// second is started while it is there. Without the bot's lock the second
    /// meets a run that still reads as quiet and is handed a key too; with it
    /// the second waits, and when the first is let go it meets a run that was
    /// just taken and is refused.
    ///
    /// **Nothing here waits on a clock or a scheduler.** The gap is held open
    /// by the case, and the second pick-up either finishes while the first is
    /// parked or is stuck behind the lock; the case lets the first go after the
    /// second has had every turn it can use.
    #[tokio::test]
    async fn two_pick_ups_of_one_quiet_run_are_not_both_handed_a_key() {
        use crate::session::scripted::{Hold, Scripted};
        let scripted = Arc::new(Scripted::over(Arc::new(InMemoryMemory::seeded())));
        let jojobot = Jojobot::new(
            scripted.clone(),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        )
        .quiet_after(jiff::SignedDuration::from_millis(200));
        name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        let key = booted(&jojobot, "gamma").await;
        journal_entry(&jojobot, &key, "started").await;
        let name = run_name(&jojobot, &key);
        // The run goes quiet by the clock the quiet rule reads.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let hold = Hold::new();
        scripted.script(|s| s.hold_listing_of = Some((EntityKind::VOUCHER, hold.clone())));
        let second_is_over = std::sync::atomic::AtomicBool::new(false);
        let first = boot_answering(&jojobot, "gamma", &name);
        let second = async {
            let answer = boot_answering(&jojobot, "gamma", &name).await;
            second_is_over.store(true, std::sync::atomic::Ordering::SeqCst);
            answer
        };
        let letting_go = async {
            hold.parked().await;
            // The first is in the gap. The second has every turn there is to
            // run to its end; when nothing lets it, it is waiting on the lock.
            for _ in 0..1000 {
                if second_is_over.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                tokio::task::yield_now().await;
            }
            hold.release();
        };
        let (first, second, ()) = tokio::join!(first, second, letting_go);

        let keys: Vec<String> = [&first, &second]
            .into_iter()
            .filter_map(|answer| answer["session"]["sid"].as_str().map(str::to_string))
            .collect();
        assert_eq!(
            keys.len(),
            1,
            "exactly one pick-up is handed a key: {first} / {second}"
        );
        // The refusal of the other says whose run it is, and hands out nothing.
        let refused = if first["session"]["sid"].is_null() {
            &first
        } else {
            &second
        };
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(refused.to_string().contains(&name), "{refused}");
        // The positive the count depends on: the key handed out opens the run,
        // and the one it replaced does not.
        assert_eq!(run_name(&jojobot, &keys[0]), name);
        assert!(jojobot.caller(Some(&key)).is_err());
    }
    /// **A second pick-up of a run that began before keys existed is refused
    /// inside the lease.** The first pick-up replaces nothing, because the run
    /// held no key, and it was the replaced key that marked the run as taken.
    /// The mark is made whatever the run held, so the second asker meets a run
    /// that was just taken and is handed no key.
    #[tokio::test]
    async fn a_second_pick_up_of_a_keyless_run_inside_the_lease_is_handed_no_key() {
        let store = Arc::new(InMemorySessions::new());
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        let jojobot = jojobot.quiet_after(jiff::SignedDuration::from_mins(45));
        name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("bcd345".into()),
                focus: "a run the older build began".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(5),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("the older build's run is on the board");
        let board = store.all_sessions().await.expect("the board");
        jojobot.registry.rebuild_from(&board);

        let first = boot_answering(&jojobot, "gamma", "bcd345").await;
        let key = first["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("the first pick-up is handed a key: {first}"))
            .to_string();
        let second = boot_answering(&jojobot, "gamma", "bcd345").await;
        assert_eq!(second["status"], "blocked", "{second}");
        assert!(
            second["session"]["sid"].is_null() && second.get("sid").is_none(),
            "a second pick-up inside the lease was handed a key: {second}"
        );
        assert!(
            second.to_string().contains("bcd345"),
            "the refusal names the run: {second}"
        );
        // The positive the refusal rests on: the first key still opens the run.
        assert_eq!(run_name(&jojobot, &key), "bcd345");
    }
    /// **A run that began before keys existed is refused a resume inside its lease
    /// as a keyed run is.** It holds no key, and the refusal points neither it nor
    /// a keyed run at a swap of its name for a key: there is none.
    #[tokio::test]
    async fn a_resume_refused_for_a_keyless_run_is_refused_as_a_keyed_run_is() {
        let store = Arc::new(InMemorySessions::new());
        let (jojobot, _memory) = with_sessions_port_and_memory(store.clone());
        let jojobot = jojobot.quiet_after(jiff::SignedDuration::from_mins(45));
        name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("bcd345".into()),
                focus: "a run the older build began".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("the older build's run is on the board");
        let board = store.all_sessions().await.expect("the board");
        jojobot.registry.rebuild_from(&board);

        let keyless = boot_answering(&jojobot, "gamma", "bcd345").await;
        assert_eq!(keyless["status"], "blocked", "{keyless}");
        let how = keyless["how_to_proceed"].as_str().unwrap_or_default();
        assert!(!how.contains("exchange"), "{keyless}");

        make_bot(&jojobot, "delta").await;
        let key = booted(&jojobot, "delta").await;
        journal_entry(&jojobot, &key, "started").await;
        let name = run_name(&jojobot, &key);
        let keyed = boot_answering(&jojobot, "delta", &name).await;
        assert_eq!(keyed["status"], "blocked", "{keyed}");
        let how = keyed["how_to_proceed"].as_str().unwrap_or_default();
        assert!(!how.contains("exchange"), "{keyed}");
    }

    /// **The quiet period the offer and the refusal state is the one the decision
    /// uses.** A handler whose period is set back to seven minutes says seven in
    /// the offer of a stopped run and in the refusal of a name picked up too soon.
    #[tokio::test]
    async fn the_offer_and_the_refusal_state_the_handlers_quiet_period() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone()).quiet_after(jiff::SignedDuration::from_mins(7));
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;

        let offering = boot(&jojobot, "gamma").await;
        let how = offering["session"]["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("the offer says how to proceed: {offering}"));
        assert!(how.contains("quiet for 7 minutes"), "{how}");
        assert!(!how.contains("45"), "{how}");

        make_bot(&jojobot, "delta").await;
        store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:delta".into()),
                sid: Sid("cde456".into()),
                focus: "a run that just wrote".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("the run is on the board");
        let board = store.all_sessions().await.expect("the board");
        jojobot.registry.rebuild_from(&board);
        let refused = boot_answering(&jojobot, "delta", "cde456").await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert!(
            refused.to_string().contains("quiet for 7 minutes"),
            "{refused}"
        );
    }

    /// The parts of a boot an answer to the offer leaves out, by the name the
    /// answer's `left_out` gives each.
    const OFFER_ELIDES: [&str; 5] = [
        "orientation",
        "snapshot",
        "rules",
        "unseated_rules",
        "handover",
    ];

    /// **A bot with a wrapped run behind it, a run to pick up, one carried rule
    /// and one rule at home**: the shape whose offering boot ships every part an
    /// answer to the offer then leaves out.
    async fn a_bot_whose_boot_offers_a_run(jojobot: &Jojobot, store: &InMemorySessions) {
        make_bot(jojobot, "gamma").await;
        capture_ok(
            jojobot,
            CaptureArgs {
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("bot:gamma", "checks the board before starting")
            },
        )
        .await;
        capture_ok(
            jojobot,
            capture_args("bot:gamma", "reports at the end of each piece of work"),
        )
        .await;
        let wrapped = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: fixture_sid(line!()),
                focus: "the run that wrapped".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(3),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        store
            .append(
                &wrapped.id,
                NewEntry::manual(
                    "wrapped with the plan half done",
                    jiff::Timestamp::now() - jiff::SignedDuration::from_hours(2),
                    None,
                ),
            )
            .await
            .expect("append ok");
        store
            .close(&wrapped.id, SessionState::Wrapped)
            .await
            .expect("close ok");
        store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: fixture_sid(line!()),
                focus: "reading the hand-off".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
    }

    /// An answer to the offer, naming a role and a zone on the same call, and
    /// sending the code the offer carried when it is given one.
    async fn answer_the_offer(
        jojobot: &Jojobot,
        answer: &str,
        brief: Option<bool>,
        offer_code: Option<&str>,
    ) -> serde_json::Value {
        answer_the_offer_of(jojobot, "gamma", answer, brief, offer_code).await
    }

    /// The same, as the named bot.
    async fn answer_the_offer_of(
        jojobot: &Jojobot,
        bot: &str,
        answer: &str,
        brief: Option<bool>,
        offer_code: Option<&str>,
    ) -> serde_json::Value {
        json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: Some("America/New_York".into()),
                    today: None,
                    bot: Some(bot.into()),
                    brief,
                    skill: None,
                    section: None,
                    resume: Some(answer.into()),
                    sid: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: offer_code.map(str::to_string),
                }))
                .await
                .expect("start_here ok"),
        )
    }

    /// The code an offering boot carries, beside its choices.
    fn the_offer_code(offering: &serde_json::Value) -> String {
        offering["session"]["offer_code"]
            .as_str()
            .unwrap_or_else(|| panic!("the offer carries a code to send back: {offering}"))
            .to_string()
    }

    /// Everything an answer to the offer leaves out is present in the boot that
    /// made the offer, and absent from the answer.
    fn assert_every_part_shipped(boot: &serde_json::Value) {
        assert!(boot["orientation"].is_string(), "{boot}");
        assert!(boot["snapshot"]["entities"].is_object(), "{boot}");
        assert!(
            boot["identity"]["rules"]
                .as_array()
                .is_some_and(|rules| !rules.is_empty()),
            "{boot}"
        );
        assert!(boot["identity"]["unseated_rules"].is_object(), "{boot}");
        assert!(boot["session"]["handover"]["story"].is_string(), "{boot}");
        assert!(boot.get("left_out").is_none(), "{boot}");
    }

    /// **The answer to the resume-or-new offer carries what is new and leaves
    /// out what the offering call just shipped.** The essay, the snapshot, the
    /// carried rules, the map of the rules at home and the handover story are
    /// each absent, and one `left_out` block names each of them and the call
    /// that returns them. The run, its key, the role claim and the zone are
    /// still in it.
    ///
    /// **Paired**: the offering boot ships every one of those parts (the
    /// positive the absence depends on), and the call `left_out` names, run
    /// as named, ships them again.
    #[tokio::test]
    async fn the_answer_to_the_offer_leaves_out_what_the_offer_shipped() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;

        let offering = boot(&jojobot, "gamma").await;
        assert_every_part_shipped(&offering);
        let run = offering["session"]["choices"][0]["run"]
            .as_str()
            .expect("one option")
            .to_string();

        let code = the_offer_code(&offering);
        let answer = answer_the_offer(&jojobot, &run, None, Some(&code)).await;
        // What the answer exists to deliver.
        assert_eq!(answer["session"]["resumed"], true, "{answer}");
        assert!(answer["session"]["sid"].is_string(), "{answer}");
        assert_eq!(answer["session"]["claim"]["status"], "taken", "{answer}");
        assert_eq!(answer["timezone"]["zone"], "America/New_York", "{answer}");

        // What it leaves out, each part by the name `left_out` gives it.
        assert!(answer["orientation"].is_null(), "{answer}");
        assert_eq!(answer["orientation_elided"], true, "{answer}");
        assert!(answer["snapshot"].is_null(), "{answer}");
        assert!(answer["identity"].get("rules").is_none(), "{answer}");
        assert!(
            answer["identity"].get("unseated_rules").is_none(),
            "{answer}"
        );
        assert!(answer["session"].get("handover").is_none(), "{answer}");
        let mut parts: Vec<&str> = answer["left_out"]["parts"]
            .as_array()
            .unwrap_or_else(|| panic!("an elision names its parts: {answer}"))
            .iter()
            .filter_map(|part| part.as_str())
            .collect();
        parts.sort_unstable();
        let mut expected = OFFER_ELIDES.to_vec();
        expected.sort_unstable();
        assert_eq!(parts, expected, "{answer}");
        // The rules at home still say they exist: the count is what a reader
        // needs to know there is more to read.
        assert_eq!(answer["identity"]["rules_elided"], true, "{answer}");

        // The call it names is a boot of this bot with no `resume`, and that
        // boot ships every part again.
        let call = &answer["left_out"]["returned_by"];
        assert_eq!(call["tool"], "start_here", "{answer}");
        assert_eq!(
            call["arguments"],
            serde_json::json!({"bot": "bot:gamma"}),
            "{answer}"
        );
        let again = boot(&jojobot, "gamma").await;
        assert_every_part_shipped(&again);

        // The point of it: the answer is smaller by at least the essay.
        let (offered, answered) = (offering.to_string().len(), answer.to_string().len());
        println!("offering boot {offered} bytes, answer to the offer {answered} bytes");
        assert!(
            answered + crate::orientation::essay::ORIENTATION_CORE.len() <= offered,
            "the answer ({answered}) is not smaller than the offer ({offered}) by the essay"
        );
    }

    /// **`new` answers the same offer, so it leaves out the same parts**, and
    /// `brief` changes nothing about what is named: a caller that sent it is
    /// still told where the essay went.
    #[tokio::test]
    async fn answering_the_offer_with_new_leaves_out_the_same_parts() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        let offering = boot(&jojobot, "gamma").await;
        assert_every_part_shipped(&offering);
        let code = the_offer_code(&offering);

        let answer = answer_the_offer(&jojobot, "new", Some(true), Some(&code)).await;
        assert!(answer["session"]["sid"].is_string(), "{answer}");
        assert!(answer["snapshot"].is_null(), "{answer}");
        assert!(answer["identity"].get("rules").is_none(), "{answer}");
        assert!(answer["session"].get("handover").is_none(), "{answer}");
        assert_eq!(
            answer["left_out"]["parts"].as_array().map(Vec::len),
            Some(OFFER_ELIDES.len()),
            "{answer}"
        );
    }

    /// **A wrap code is not an answer to the offer.** It reopens one wrapped
    /// run and has no offer behind it, so its answer ships the parts the offer
    /// answer leaves out. Paired with the offer answer above, which leaves them
    /// out on the same bot.
    #[tokio::test]
    async fn a_wrap_code_answer_still_ships_every_part() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("bot:gamma", "checks the board before starting")
            },
        )
        .await;
        let (_wrapped, code) = wrapped_with_a_code(&store, 1, "t002").await;

        let opened = answer_the_offer(&jojobot, &code, None, None).await;
        assert_eq!(opened["session"]["resumed"], true, "{opened}");
        assert!(opened["orientation"].is_string(), "{opened}");
        assert!(opened["snapshot"]["entities"].is_object(), "{opened}");
        assert!(
            opened["identity"]["rules"]
                .as_array()
                .is_some_and(|rules| !rules.is_empty()),
            "{opened}"
        );
        assert!(opened.get("left_out").is_none(), "{opened}");
    }

    /// **An answer that sends no code is a full boot.** `resume` is an ordinary
    /// argument and a client that kept no offer can send it on its first call,
    /// so the answer cannot claim that an offering call shipped anything. Every
    /// part ships, `left_out` is absent, and the charter keeps its own note.
    ///
    /// **Paired** with the answer that sends the code, which leaves the same
    /// parts out on the same bot.
    #[tokio::test]
    async fn an_answer_with_no_code_is_the_full_boot() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        let offering = boot(&jojobot, "gamma").await;
        let run = offering["session"]["choices"][0]["run"]
            .as_str()
            .expect("one option")
            .to_string();

        for answer in [run.as_str(), "new"] {
            let whole = answer_the_offer(&jojobot, answer, None, None).await;
            assert!(whole["session"]["sid"].is_string(), "{answer}: {whole}");
            assert_every_part_shipped(&whole);
            assert_eq!(whole["identity"]["charter_elided"], true, "{whole}");
        }

        // The code was never presented, so it is still good for its owner.
        let code = the_offer_code(&offering);
        let short = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        assert!(short["snapshot"].is_null(), "{short}");
        assert!(short.get("left_out").is_some(), "{short}");
    }

    /// **A code binds one bot and one answer.** A code another bot's offer gave
    /// is not this bot's, a code already spent is gone, and each answer is the
    /// full boot. **Paired**: the owner's code, untouched by the other bot's
    /// try, still gives the short answer, and a second use of it does not.
    #[tokio::test]
    async fn a_code_of_another_bot_or_one_already_used_gives_the_full_boot() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        make_bot(&jojobot, "delta").await;
        let code = the_offer_code(&boot(&jojobot, "gamma").await);

        let other = answer_the_offer_of(&jojobot, "delta", "new", None, Some(&code)).await;
        assert!(other["session"]["sid"].is_string(), "{other}");
        assert!(other["orientation"].is_string(), "{other}");
        assert!(other["snapshot"]["entities"].is_object(), "{other}");
        assert!(other.get("left_out").is_none(), "{other}");

        let owner = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        assert!(owner["snapshot"].is_null(), "{owner}");
        assert!(owner.get("left_out").is_some(), "{owner}");

        let reused = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        assert!(reused["orientation"].is_string(), "{reused}");
        assert!(reused["snapshot"]["entities"].is_object(), "{reused}");
        assert!(reused.get("left_out").is_none(), "{reused}");
    }

    /// **A run that reports a compaction is owed the whole orientation, whatever
    /// else the call sends**, a `resume` and a live code included. **Paired**:
    /// the same code on a call that reports no compaction gives the short
    /// answer, so the whole boot comes from the report and not from the code.
    #[tokio::test]
    async fn a_compaction_report_gets_the_full_boot_even_with_resume_and_a_live_code() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        let code = the_offer_code(&boot(&jojobot, "gamma").await);
        let live = answer_the_offer(&jojobot, "new", None, None).await;
        let sid = live["session"]["sid"].as_str().expect("a key").to_string();

        let compacted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: Some(true),
                    claim: None,
                    timezone: None,
                    today: None,
                    bot: Some("gamma".into()),
                    brief: Some(true),
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: Some(sid),
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: Some(code.clone()),
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(compacted["orientation"].is_string(), "{compacted}");
        assert!(compacted["snapshot"]["entities"].is_object(), "{compacted}");
        assert!(compacted.get("left_out").is_none(), "{compacted}");

        let short = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        assert!(short["snapshot"].is_null(), "{short}");
    }

    /// **A compaction report keeps the run it reports**, a `resume` beside it
    /// included: the same run, the same key, and no new run started. **Paired**:
    /// the same `resume: new` with no report starts another run.
    #[tokio::test]
    async fn a_compaction_report_keeps_its_run_even_with_resume_new() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        let live = answer_the_offer(&jojobot, "new", None, None).await;
        let sid = live["session"]["sid"].as_str().expect("a key").to_string();
        let run = live["session"]["run"].as_str().expect("a run").to_string();

        let compacted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: Some(true),
                    claim: None,
                    timezone: None,
                    today: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: Some(sid.clone()),
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(compacted["session"]["sid"], sid, "{compacted}");
        assert_eq!(compacted["session"]["run"], run, "{compacted}");

        let another = answer_the_offer(&jojobot, "new", None, None).await;
        assert_ne!(another["session"]["run"], run, "{another}");
    }

    /// **A run that reports a compaction always gets its charter back**, a
    /// `resume` beside the report included: the charter is the first thing a run
    /// that lost its context needs. **Paired**: the same `resume` with no report
    /// leaves the charter out and says so.
    #[tokio::test]
    async fn a_compaction_report_carries_the_charter_even_with_resume() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                request_id: None,
                bot: "gamma".into(),
                prose: "Holds the plan.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");
        let bare = answer_the_offer(&jojobot, "new", None, None).await;
        assert!(bare["identity"]["charter"].is_null(), "{bare}");
        assert_eq!(bare["identity"]["charter_elided"], true, "{bare}");
        let sid = bare["session"]["sid"].as_str().expect("a key").to_string();

        let compacted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: Some(true),
                    claim: None,
                    timezone: None,
                    today: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: Some(sid),
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            compacted["identity"]["charter"], "Holds the plan.",
            "{compacted}"
        );
        assert_eq!(
            compacted["identity"]["charter_elided"], false,
            "{compacted}"
        );
        assert!(compacted["identity"].get("note").is_none(), "{compacted}");
    }

    /// Rename a bot. A near-miss against a name another bot still wears is
    /// blocked with a token, and the rename is sent again with it.
    async fn rename_bot(jojobot: &Jojobot, from: &str, to: &str) {
        let mut override_token = None;
        for _ in 0..2 {
            let answer = json_of(
                &jojobot
                    .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
                        request_id: None,
                        handle: from.into(),
                        to: to.into(),
                        parent: None,
                        recorded_at: None,
                        override_token: override_token.take(),
                        override_reason: None,
                        sid: Some(crate::harness::TEST_SID.into()),
                    }))
                    .await
                    .expect("rename ok"),
            );
            // The token rides the refusal's own instruction, quoted.
            let token = answer["how_to_proceed"]
                .as_str()
                .and_then(|way| way.split("override_token: \"").nth(1))
                .and_then(|rest| rest.split('"').next());
            match token {
                Some(token) if answer["status"] == "blocked" => {
                    override_token = Some(token.to_string())
                }
                _ => {
                    assert_eq!(answer["id"], to, "{answer}");
                    return;
                }
            }
        }
        panic!("the rename of {from} to {to} was blocked twice");
    }

    /// **An offer belongs to the bot it was made for, not to the handle that bot
    /// wore.** The code follows its bot through a rename, so the bot that now
    /// wears the old handle cannot spend it. **Paired**: the bot that was
    /// renamed spends the same code under its new handle and gets the short
    /// answer.
    #[tokio::test]
    async fn an_offer_follows_its_bot_through_a_rename_and_not_the_handle() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        make_bot(&jojobot, "delta").await;
        let code = the_offer_code(&boot(&jojobot, "gamma").await);

        rename_bot(&jojobot, "bot:gamma", "bot:sigma").await;
        rename_bot(&jojobot, "bot:delta", "bot:gamma").await;

        let former_delta = answer_the_offer_of(&jojobot, "gamma", "new", None, Some(&code)).await;
        assert!(former_delta["orientation"].is_string(), "{former_delta}");
        assert!(
            former_delta["snapshot"]["entities"].is_object(),
            "{former_delta}"
        );
        assert!(former_delta.get("left_out").is_none(), "{former_delta}");

        let renamed = answer_the_offer_of(&jojobot, "sigma", "new", None, Some(&code)).await;
        assert!(renamed["snapshot"].is_null(), "{renamed}");
        assert!(renamed.get("left_out").is_some(), "{renamed}");
    }

    /// **A rule written after the offer rides the short answer**, because the
    /// offering call could not have shipped it. **Paired**: a rule written before
    /// the offer does not ride, and the answer says that rules were left out.
    #[tokio::test]
    async fn a_rule_written_after_the_offer_rides_the_answer() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        let code = the_offer_code(&boot(&jojobot, "gamma").await);
        capture_ok(
            &jojobot,
            capture_args("bot:gamma", "asks the operator before it deletes a branch"),
        )
        .await;

        let answer = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        let rules: Vec<&str> = answer["identity"]["rules"]
            .as_array()
            .unwrap_or_else(|| panic!("the newer rule rides: {answer}"))
            .iter()
            .filter_map(|rule| rule["content"].as_str())
            .collect();
        assert_eq!(
            rules,
            ["asks the operator before it deletes a branch"],
            "{answer}"
        );
        assert_eq!(answer["identity"]["rules_elided"], true, "{answer}");
        assert!(answer["snapshot"].is_null(), "{answer}");
    }

    /// **A rule written while the offering boot is still running rides the
    /// short answer**, because neither answer shipped it. The boot reads its
    /// rules, then waits at the bot's gate; a rule lands there, and the gate
    /// opens. **Paired**: a rule the offering boot did read does not ride, so the
    /// cutoff is not moved past everything.
    #[tokio::test]
    async fn a_rule_written_while_the_offering_boot_waits_rides_the_answer() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;

        let gate = jojobot.registry.gate("bot:gamma");
        let held = gate.lock().await;
        let offering = tokio::spawn({
            let jojobot = jojobot.clone();
            async move { boot(&jojobot, "gamma").await }
        });
        for _ in 0..200 {
            tokio::task::yield_now().await;
        }
        assert!(
            !offering.is_finished(),
            "the boot has read its rules and waits at the gate"
        );
        capture_ok(
            &jojobot,
            capture_args("bot:gamma", "writes the handover before the standup"),
        )
        .await;
        drop(held);
        let offer = offering.await.expect("the boot ran");
        let shipped: Vec<&str> = offer["identity"]["rules"]
            .as_array()
            .unwrap_or_else(|| panic!("the offer carries rules: {offer}"))
            .iter()
            .filter_map(|rule| rule["content"].as_str())
            .collect();
        assert!(
            !shipped.iter().any(|rule| rule.contains("standup")),
            "the offer read its rules before this one was written: {offer}"
        );

        let code = the_offer_code(&offer);
        let answer = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        let rules: Vec<&str> = answer["identity"]["rules"]
            .as_array()
            .unwrap_or_else(|| panic!("the newer rule rides: {answer}"))
            .iter()
            .filter_map(|rule| rule["content"].as_str())
            .collect();
        assert_eq!(
            rules,
            ["writes the handover before the standup"],
            "{answer}"
        );
    }

    /// **A rule rewritten after the offer rides the short answer as well**: the
    /// offering call shipped its earlier words.
    #[tokio::test]
    async fn a_rule_rewritten_after_the_offer_rides_the_answer() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        let offering = boot(&jojobot, "gamma").await;
        let address = offering["identity"]["rules"][0]["address"]
            .as_str()
            .expect("a carried rule")
            .to_string();
        let code = the_offer_code(&offering);
        let rewritten = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                content: Some("checks the board twice before starting".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update_fact ok");
        assert_ne!(json_of(&rewritten)["status"], "blocked");

        let answer = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        let rules: Vec<&str> = answer["identity"]["rules"]
            .as_array()
            .unwrap_or_else(|| panic!("the rewritten rule rides: {answer}"))
            .iter()
            .filter_map(|rule| rule["content"].as_str())
            .collect();
        assert_eq!(
            rules,
            ["checks the board twice before starting"],
            "{answer}"
        );
    }

    /// **The block naming what the answer left out is counted in the ceiling.**
    /// It ships whatever the ranking decides, so it belongs to what cannot be
    /// cut. A newer rule is padded until the answer, less the notice about the
    /// ceiling, is over the ceiling by less than that block: the floor without
    /// the block fits and the floor with it does not. The answer must then say
    /// it is over.
    ///
    /// **Paired**: the same bot's answer with a short rule fits, says nothing
    /// about the ceiling, and sizes the padding.
    #[tokio::test]
    async fn the_left_out_block_of_the_answer_to_the_offer_counts_toward_the_ceiling() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        a_bot_whose_boot_offers_a_run(&jojobot, &store).await;
        let budget = jojobot_domain::text::BOOT_ANSWER.budget;

        let code = the_offer_code(&boot(&jojobot, "gamma").await);
        for _ in 0..2 {
            capture_ok(&jojobot, capture_args("bot:gamma", "w")).await;
        }
        let fitting = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
        let base = fitting.to_string().chars().count();
        assert!(base < budget, "{base}");
        assert!(fitting.get("over_the_ceiling").is_none(), "{fitting}");

        // Over the ceiling by half the block, so the block alone decides it. A
        // long rule serializes with more than its words, so the padding is
        // corrected by what the last round overshot until it lands in the
        // window.
        let block = crate::answer::under_key("left_out", &fitting["left_out"]);
        let target = budget + block / 2;
        let mut padding = target - base;
        let mut landed = None;
        for _ in 0..4 {
            let code = the_offer_code(&boot(&jojobot, "gamma").await);
            // Two rules, since one record can carry only so much.
            for share in [padding / 2, padding - padding / 2] {
                capture_ok(
                    &jojobot,
                    capture_args("bot:gamma", &format!("w{}", "w".repeat(share))),
                )
                .await;
            }
            let answer = answer_the_offer(&jojobot, "new", None, Some(&code)).await;
            let mut without_notice = answer.clone();
            without_notice
                .as_object_mut()
                .expect("an object")
                .remove("over_the_ceiling");
            let shipped = without_notice.to_string().chars().count();
            if shipped > budget && shipped - block <= budget {
                landed = Some((answer, shipped));
                break;
            }
            padding = (padding + target).saturating_sub(shipped);
        }
        let (answer, shipped) = landed.expect("the padding reached the window");
        assert!(
            answer.get("over_the_ceiling").is_some(),
            "an answer over the ceiling says so: {shipped} characters"
        );
    }
}

//! `journal` — Record one beat in this session's chronology, and move its focus.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// **What a takeover found and did to the run holding the role.**
pub(crate) struct HolderTakenOver {
    /// The state the run was in when it was found.
    pub(crate) was: SessionState,
    /// Set when the run lost a role and could not be closed as displaced, so it is
    /// still active.
    pub(crate) left_open: bool,
}

/// Arguments to `journal`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct JournalArgs {
    /// One high-level beat: what you did, what you found, what went
    /// wrong. Prose — paragraphs are fine.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as
    /// the handle that thing wears today, even after a rename.
    /// Text inside backticks or a fenced block is literal: a handle there is
    /// kept as written and never becomes a link.
    pub(crate) entry: String,
    /// Optional: omit it to keep the session's current focus. What you are
    /// working on NOW, in one line (200 characters at most, no backtick). When
    /// sent it **replaces** the current focus, and a blank one is refused. Carries
    /// `@kind:slug` mentions as `entry` does.
    #[serde(default)]
    pub(crate) focus: Option<String>,
    /// **The thing your bot is working on, as its handle** (`work:sigma`), shown
    /// to any reader of the bot while this session is live. An empty string
    /// takes it off. Optional; moving work your bot owns to `now` with `capture`
    /// or `update_fact` points it for you.
    #[serde(default)]
    pub(crate) working_on: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. A session is
    /// bound to the bot that booted it; there is no way to write into another
    /// one.
    pub(crate) sid: String,
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of the beat**, because a call sent without
    /// one cannot be repeated safely. A beat is made of parts, the entry, the
    /// focus it moves and the thing you say you are working on: if an answer is
    /// lost or a part fails, send the same call again with the same
    /// `request_id` and the parts that landed are not written again and the rest
    /// are, so the entry lands once.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
}

/// Record one beat in this session's chronology, and optionally move what
/// it says it is working on.
#[tool_router(router = journal_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Record ONE beat in your session's chronology — a literal journal, not a \
                       log. High-level: what you did, what you found, what went wrong — except a resume note, left in case this run goes quiet without wrapping, which is dense and specific. Work that must move to another agent now is a wrap, not a note. Not every tool call, not every file: a reader \
                       months from now wants the story, and a firehose buries it. `focus` \
                       rewrites what your session says it is working on RIGHT NOW, in place — \
                       the chronology is history, the focus is the present, and they answer \
                       different questions. The first journal entry (or the first write of any \
                       kind) is what brings your session's record into being; a boot alone keeps only a \
                       note of its key. PASS `sid` — the session id the boot door \
                       gave you — ON EVERY CALL; it is the only address, and it is what tells \
                       jojobot which bot is writing. A `sid` whose session is closed comes back \
                       status: blocked: a closed session takes no more entries, whichever end it \
                       reached — except a wrapped run whose wrap window is open, which still takes them. The two ends part company on what comes NEXT — a run that stopped \
                       without being wrapped up is offered back at your next boot for {{offer}}, and resuming \
                       it continues this same record, while a wrapped one is the last word — its \
                       story is told and nothing appends to it except through the wrap_code its \
                       wrap handed back, so carrying on means a fresh session. IT ANSWERS \
                       WITH A RECEIPT, NOT YOUR BEAT: the id the entry was \
                       given, when it was stamped, the run it landed in, the byte count of what \
                       was stored and a digest of about 120 characters. You wrote the entry; a resume through start_here \
                       returns the newest entries and says how many older ones it left out. IF YOUR SESSION HOLDS A CLAIMED \
                       ROLE, this beat renews it, under the same rule as any other write — the lease is \
                       not journal's alone to keep fresh. The receipt's `changed` lists changes, or is `nothing` or `not_read`."
    )]
    pub(crate) async fn journal(
        &self,
        Parameters(args): Parameters<JournalArgs>,
    ) -> Result<CallToolResult, McpError> {
        let focus = args.focus.as_deref();
        let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
        let _serialized = gate.lock().await;
        // Resolved inside the gate: a racing write may have materialized this
        // session's card since, and beginning a second one is the fork the lock
        // exists to prevent.
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        if let Some(card) = caller.card.as_ref()
            && let Err(refused) = self.end_a_lapsed_window(card).await
        {
            return Ok(refused);
        }
        // **Screened here so the refusal is an ANSWER, and before anything is
        // written.** A focus the record cannot carry is a caller mistake, and
        // it reaches the store by two paths: the call that OPENS a session, and
        // the one that moves the focus of a session already open. The first
        // came back as a protocol error, which is a failure where the caller
        // needs a next move (rule 68); the second came back as a partial answer
        // saying jojobot's storage failed, which is not true of a bad argument
        // (rule 130). One screen closes both, and it runs before the entry is
        // appended, so nothing is written and the whole call can simply be sent
        // again.
        // **A blank focus is screened too**: the store refuses it after the entry
        // is appended, and that was told as a fault of its own.
        // **A blank one says what to do instead**: a client that fills every
        // optional string with "" is told to leave the argument out.
        if focus.map(str::trim).is_some_and(str::is_empty) {
            return Ok(crate::caller::handle_declined(
                caller.sid.as_str(),
                "Nothing was written: a focus is empty. Omit `focus` to keep the current one, \
                 or send one line saying what you are working on now.",
            ));
        }
        if let Some(theirs) = focus.map(str::trim)
            && let Err(e) = jojobot_domain::session::validate_focus(theirs)
        {
            return session_declined(e, caller.sid.as_str());
        }
        // **Screened here for the reason the focus is**: a handle that names
        // nothing is refused before the entry is appended, so the whole call can
        // simply be sent again.
        let declared = match args.working_on.as_deref().map(str::trim) {
            None => None,
            Some("") => Some(crate::memory::working_on::Declared::Nothing),
            Some(handle) => {
                let thing = jojobot_domain::memory::EntityId(handle.to_string());
                if !handle.contains(':') {
                    return memory_declined(
                        "journal",
                        MemoryError::InvalidSubject(format!(
                            "'{handle}' is no handle: `working_on` takes kind:slug, for example \
                             work:sigma"
                        )),
                    );
                }
                if let Err(e) = self.memory.recall(&thing).await {
                    return memory_declined("journal", e);
                }
                Some(crate::memory::working_on::Declared::At(thing))
            }
        };
        // **What the receipt's `changed` needs from before the write.** Whether
        // this beat brings the record into being, and the focus it holds now: a
        // focus is read only when the call sends one, because only then can it
        // move.
        let record_is_new = caller.card.is_none();
        let focus_was: Result<Option<String>, ()> = match (&caller.card, focus) {
            (Some(card), Some(_)) => self
                .sessions
                .read_session(card)
                .await
                .map(|read| Some(read.focus))
                .map_err(|_| ()),
            _ => Ok(None),
        };
        let session = self
            .session_for(&_serialized, &caller, focus, Some(&args.entry))
            .await?;
        // **The beat is made of up to three parts under a `request_id`**: the
        // entry, the focus it moves, and the thing the bot says it is working on.
        // A resend finds the parts that landed and writes the rest.
        use jojobot_domain::request::{Part, part};
        let appended = part(
            "entry",
            None,
            self.sessions.append(
                &session,
                NewEntry::manual(args.entry, self.clock().now(), caller.day),
            ),
        )
        .await;
        let entry = match appended {
            Part::Ran(Ok(entry)) => entry,
            // **An earlier call wrote the entry**; it is read back by the id it
            // left in `landed`.
            Part::Recorded(recorded) => {
                let wanted = serde_json::from_str::<serde_json::Value>(&recorded.landed)
                    .ok()
                    .and_then(|landed| landed.get("entry")?.as_str().map(str::to_string));
                let found = match (wanted, self.sessions.read_session(&session).await) {
                    (Some(wanted), Ok(read)) => {
                        read.entries.into_iter().find(|e| e.id.as_str() == wanted)
                    }
                    _ => None,
                };
                match found {
                    Some(entry) => entry,
                    None => {
                        return Ok(crate::caller::handle_declined(
                            "journal",
                            crate::answer::WayForward::person(
                                "The entry was written under this request_id, but it cannot be \
                                 read back, so the rest of the beat cannot be finished.",
                            ),
                        ));
                    }
                }
            }
            // **An append can fail with its write already on the page** — a
            // reread that failed after the entry landed, or a rollback that
            // failed too. A flat error reads as "nothing was written", and the
            // natural next move is the retry that appends a second entry
            // beside the first. That is the same hazard the focus case below
            // fixes, one call earlier.
            //
            // Uncertain rather than partial: unlike the focus, nothing here
            // knows whether the entry landed. So the answer says that plainly
            // and sends the caller to look, which is the conservative reading
            // and the only honest one.
            // **Only a STORE failure can have written before it failed.**
            // Every other variant is a clean refusal decided before anything
            // was touched — a closed run, an unknown session, a malformed
            // entry — and dressing those as uncertain would send a caller to
            // go and look for a write that provably never happened.
            Part::Ran(Err(e)) if !matches!(e, SessionError::Store(_)) => {
                return session_declined(e, caller.sid.as_str());
            }
            Part::Ran(Err(e)) => {
                tracing::error!(error = %e, "store failure left an append uncertain");
                // **With an id the question has an answer**: the entry was claimed in
                // its own transaction, so the same call sent again writes it only if
                // it is not there.
                if jojobot_domain::request::carries_an_id() {
                    return Ok(crate::caller::handle_declined(
                        "journal",
                        crate::answer::WayForward::retry(
                            "jojobot's own storage failed, and the entry may or may not have \
                             been recorded. Send the same call again with the same request_id: \
                             the entry lands once either way.",
                        ),
                    ));
                }
                return json_result(&serde_json::json!({
                    "status": "uncertain",
                    "wrote": "unknown",
                    "session": session.as_str(),
                    "why": "jojobot's own storage failed",
                    "how_to_proceed": "The entry may or may not have been recorded — this \
                                       failure cannot tell you which. Do NOT send it again \
                                       blind: read the session back first, and re-send only if \
                                       your entry is not the newest one in its chronology.",
                }));
            }
        };
        // **The claim renews on the beat that was just written — no second
        // mechanism.** Best-effort: a renewal failure is logged and never
        // turns a landed beat into a failed call.
        self.renew_role_claims(&caller.bot, caller.sid.as_str(), entry.at)
            .await;
        // The focus moves only once the beat is recorded: a session whose focus
        // says it is doing something its chronology never mentions is a record
        // that disagrees with itself.
        let moved = match focus {
            None => None,
            Some(focus) => {
                match part("focus", None, self.sessions.set_focus(&session, focus)).await {
                    Part::Ran(Ok(session)) => Some(session),
                    // An earlier call moved the focus: the session reads as it stands.
                    Part::Recorded(_) => self.sessions.read_session(&session).await.ok(),
                    // **The entry is already recorded, so this is not a failed
                    // call — it is a call that half succeeded.**
                    //
                    // A flat error here reads as "nothing was written", which is
                    // the safe assumption for every other failure on this surface
                    // and the dangerous one here: the caller's natural next move is
                    // to repeat the whole call, and repeating it appends the entry
                    // a second time beside the one already recorded. That is not
                    // hypothetical — it is what a session did after exactly this
                    // failure, and it only avoided the duplicate because the error
                    // happened to carry enough of the record to see the entry in.
                    //
                    // So the answer says what LANDED first and what did not, and
                    // the way forward names the smaller call rather than the one
                    // just made.
                    // **The entry is already recorded, so this is not a failed
                    // call — it is a call that half succeeded.**
                    //
                    // A flat error here reads as "nothing was written", which is
                    // the safe assumption for every other failure on this surface
                    // and the dangerous one here: the caller's natural next move is
                    // to repeat the whole call, and repeating it appends the entry
                    // a second time beside the one already recorded. That is not
                    // hypothetical — it is what a session met after exactly this
                    // failure, and the duplicate was avoided only because the error
                    // happened to carry enough of the record to see the entry in.
                    //
                    // So the answer says what LANDED first, and the way forward
                    // names the smaller call rather than the one just made.
                    Part::Ran(Err(e)) => {
                        tracing::error!(
                            error = %e,
                            "store failure left the focus unmoved after the entry landed"
                        );
                        if jojobot_domain::request::carries_an_id() {
                            return Ok(crate::caller::handle_declined(
                                "journal",
                                crate::stopped::Stopped::of_session(&e).unfinished(
                                    "The entry is recorded, and the focus did not move.",
                                    "Send the same call again with the same request_id: the \
                                     entry is not written twice and the focus moves.",
                                ),
                            ));
                        }
                        return json_result(&serde_json::json!({
                            "status": "partial",
                            "wrote": true,
                            "recorded": "entry",
                            "not_recorded": "focus",
                            "session": session.as_str(),
                            "entry": entry_receipt_json(&entry),
                            "why": "jojobot's own storage failed",
                            "how_to_proceed": "The entry IS recorded — do not send this call again, \
                                               or the entry lands twice. Only the focus did not move. \
                                               Set it with the `focus` of your next journal beat, or \
                                               leave it: the chronology is what outlives the \
                                               session, and the focus is overwritten by the next \
                                               beat anyway.",
                        }));
                    }
                }
            }
        };
        // **Written once the beat is recorded**, as the focus is, and answered
        // the same way when only this part fails.
        if let Some(what) = declared {
            let today = self.dated(None, Some(&args.sid)).await?;
            let marked = part(
                "working_on",
                None,
                self.set_working_on(&caller, what, today),
            )
            .await;
            if let Part::Ran(Err(e)) = marked {
                tracing::error!(error = %e, "the entry landed and working_on did not");
                if jojobot_domain::request::carries_an_id() {
                    return Ok(crate::caller::handle_declined(
                        "journal",
                        crate::stopped::Stopped::of_memory(&e).unfinished(
                            "The entry is recorded, and the thing you are working on was not \
                             set.",
                            "Send the same call again with the same request_id: the entry is \
                             not written twice and the work is set.",
                        ),
                    ));
                }
                return json_result(&serde_json::json!({
                    "status": "partial",
                    "wrote": true,
                    "recorded": "entry",
                    "not_recorded": "working_on",
                    "session": session.as_str(),
                    "entry": entry_receipt_json(&entry),
                    "why": e.to_string(),
                    "how_to_proceed": "The entry IS recorded — do not send this call again, or \
                                       the entry lands twice. Send working_on again on your next \
                                       journal beat.",
                }));
            }
        }
        jojobot_domain::request::finished();
        let focus_now = moved.map(|s| s.focus);
        let mut body = serde_json::json!({
            "session": session.as_str(),
            "entry": entry_receipt_json(&entry),
            "focus": focus_now,
        });
        let mut changed = if record_is_new {
            crate::changed::Changed::created(session.as_str())
        } else {
            crate::changed::Changed::default()
        }
        .and(crate::changed::Changed::value(
            session.as_str(),
            crate::changed::ENTRY,
            None,
            Some(entry.id.as_str()),
        ));
        // **A first beat that sends no focus still gives the record one**,
        // derived from the entry, so the focus moved from nothing to that line.
        let derived_focus = match (focus_now.as_deref(), record_is_new) {
            (None, true) => Some(
                self.sessions
                    .read_session(&session)
                    .await
                    .map(|read| read.focus),
            ),
            _ => None,
        };
        if let Some(derived) = derived_focus {
            changed = changed.and(match derived {
                Ok(now) => crate::changed::Changed::value(
                    session.as_str(),
                    crate::changed::FOCUS,
                    None,
                    Some(now.as_str()),
                ),
                Err(_) => crate::changed::Changed::not_read(),
            });
        }
        if let Some(now) = focus_now.as_deref() {
            changed = changed.and(match &focus_was {
                Ok(was) => crate::changed::Changed::value(
                    session.as_str(),
                    crate::changed::FOCUS,
                    was.as_deref(),
                    Some(now),
                ),
                Err(()) => crate::changed::Changed::not_read(),
            });
        }
        // **A beat finished by a resend cannot say what it changed**: it did not
        // read the record before the parts an earlier call wrote.
        let changed = match crate::request::note_resumed(&mut body) {
            true => crate::changed::Changed::not_read(),
            false => changed,
        };
        crate::changed::note_changed(&mut body, changed);
        crate::answer::note_postcondition(
            &mut body,
            self.what_a_beat_left_standing(&session).await,
        );
        if self
            .first_contact(crate::teaching::SESSION_ID_DOMAIN, Some(&caller))
            .await
        {
            crate::answer::note_teaching(
                &mut body,
                &crate::teaching::session_id_teaching(caller.sid.as_str(), session.as_str()),
            );
        }
        json_result(&body)
    }
}

impl Jojobot {
    /// **Renew every role this session's own claimant holds, on the write it
    /// just made — no second mechanism, no cheap-renew path.**
    ///
    /// Called from here AND from [`crate::beat`], which is what makes this
    /// "every write carrying the holder's sid", not only a journal beat: a
    /// journal entry is not one of jojobot's own automatic beats, so it
    /// renews here directly, while every write class `beat` covers (capture,
    /// add_entity, post_message, and the rest) renews there instead — one
    /// mechanism, called from the two places a write actually lands.
    ///
    /// A role claim lives on the role's own object, a child of the bot (see
    /// the boot door's `decide_role_claim`). This asks the store which roles
    /// that bot holds rather than tracking a claimed role on the session
    /// record: a second place to remember what is claimed is a second place
    /// for the two to disagree. The write goes through `write_role_keys`.
    ///
    /// Best-effort: a read or write failure here is logged and never turns a
    /// write that landed into a failed call — renewing a lease is not what a
    /// caller who made an ordinary write asked for.
    pub(crate) async fn renew_role_claims(
        &self,
        bot: &EntityId,
        claimant: &str,
        now: jiff::Timestamp,
    ) {
        self.write_role_claims(bot, claimant, now, "renew").await;
    }

    /// **The renewal a write makes, for the run behind `sid`**: every write
    /// renews, and every read of the caller's own box does, so each verb that is
    /// one of those and has no beat calls this. One place for the lookup
    /// (rule 498). A call that carries no handle, or one that opens no run,
    /// renews nothing. Best-effort, as [`Jojobot::renew_role_claims`] is.
    pub(crate) async fn renew_role_claims_of(&self, sid: Option<&str>) {
        if let Ok(Some(caller)) = self.caller(sid) {
            self.renew_role_claims(&caller.bot, caller.sid.as_str(), self.clock().now())
                .await;
        }
    }

    /// **The renewal a read makes, which the read does not wait for.**
    ///
    /// A due renewal is a memory write, and a write waits for the gate every
    /// write waits for. A poll that waited for it sat behind a merge for as long
    /// as the merge took. This starts the renewal on its own task, with the
    /// moment of the call, and returns at once; the task is not cut when the
    /// answer goes out. The renewal is best-effort already, and its failures are
    /// logged by [`Jojobot::renew_role_claims`] and never reach the answer. A
    /// claimant with a renewal running starts no second one, so polls that arrive
    /// while the gate is busy do not queue renewals behind it.
    pub(crate) fn renew_role_claims_after_answer(&self, sid: Option<&str>) {
        let Ok(Some(caller)) = self.caller(sid) else {
            return;
        };
        let claimant = caller.sid.as_str().to_string();
        let Some(running) = self.registry.role_renewals().begin(&claimant) else {
            tracing::debug!(bot = %caller.bot, role_renewal = "skipped", "a role renewal is already running; this poll starts none");
            return;
        };
        tracing::debug!(bot = %caller.bot, role_renewal = "after_the_answer", "the role renewal runs after the answer");
        let server = self.clone();
        let now = self.clock().now();
        tokio::spawn(async move {
            if tokio::time::timeout(
                super::renewals::RENEWAL_TIMEOUT,
                server.renew_role_claims(&caller.bot, &claimant, now),
            )
            .await
            .is_err()
            {
                tracing::warn!(bot = %caller.bot, "a role renewal ran past its timeout and was cut off");
            }
            drop(running);
        });
    }

    /// **Release every role this session's own claimant holds, on wrap.**
    ///
    /// A role's holder is a session id, not a bot handle, and a session
    /// belongs to one bot for its whole life — the same bot `decide_role_claim`
    /// records the claim on — so this reads that bot's own fields, exactly as
    /// [`Jojobot::renew_role_claims`] does, rather than a graph-wide search
    /// for the sid.
    ///
    /// **The holder is cleared** on the role's object, through
    /// `write_role_keys`, and the moment is written as the epoch. A write of this session's that was in flight when the wrap
    /// landed then finds no holder to renew: the store applies a renewal only
    /// while its own sid still holds the role.
    ///
    /// A read or write failure here never turns a wrap that landed into a
    /// failed call. It is logged and returned, one outcome per role, so the
    /// wrap's answer says which roles are still held (rule 345).
    pub(crate) async fn release_role_claims(
        &self,
        bot: &EntityId,
        claimant: &str,
    ) -> Vec<RoleRelease> {
        match self
            .write_role_claims(bot, claimant, jiff::Timestamp::UNIX_EPOCH, "release")
            .await
        {
            None => vec![RoleRelease::NotRead],
            Some(written) => written
                .into_iter()
                .map(|(held, released)| {
                    if released {
                        RoleRelease::Released { role: held.role }
                    } else {
                        RoleRelease::NotReleased {
                            until: held
                                .claimed_at
                                .map(|at| at + jojobot_domain::session::LEASE_FRESHNESS),
                            role: held.role,
                        }
                    }
                })
                .collect(),
        }
    }

    /// **Abandon the run of `bot` named `run`, because the run `displaced_by`
    /// claimed its role, and release every role it held.** The close records
    /// the displacing run, so a later refusal of the displaced run can say it.
    /// An abandoned run holds no lease: the release is part of the abandon, so
    /// a role never waits out the lease of a run that is over. Best-effort in
    /// the release, as a wrap's is; the close is what the caller is told about.
    ///
    /// **A wrapped run is not closed again, and its roles are released all the
    /// same**: a wrap whose release failed leaves the role held for a run whose
    /// key is void, and a claim by the next run of the bot must not wait out
    /// that lease (rules 277, 345). A release that has nothing to release is no
    /// harm.
    ///
    /// `role` is the role the claim is for. Returns the state the run was in when
    /// it was found and whether it was left open, or `None` when the bot has no such run.
    pub(crate) async fn abandon_run_and_release(
        &self,
        bot: &EntityId,
        run: &str,
        displaced_by: &str,
        role: &str,
    ) -> Result<Option<HolderTakenOver>, SessionError> {
        let runs = self.sessions.sessions_of(bot).await?;
        let Some(found) = runs
            .iter()
            .find(|r| r.sid.as_ref().is_some_and(|sid| sid.as_str() == run))
        else {
            return Ok(None);
        };
        // **The holder is closed as displaced once it has lost the CLAIMED role.**
        // A holder whose claimed role stayed held keeps it, the claimant is
        // refused, and the holder is not told another run took it, though a
        // release of another role of its own may have landed. A holder that lost
        // the claimed role is told at once, and a role it still holds beside it is
        // released at its lease.
        let released = self.release_role_claims(bot, run).await;
        let lost_a_role = released.iter().any(
            |outcome| matches!(outcome, RoleRelease::Released { role: freed } if freed == role),
        );
        let mut left_open = false;
        if found.state == SessionState::Active && lost_a_role {
            // **A close that fails after the release does not undo it**: the
            // role is free and stays free. The failure is returned, so the claim
            // goes on and tells the claimant which holder is still active.
            if let Err(e) = self
                .sessions
                .close_displaced(
                    &found.id,
                    &jojobot_domain::session::Sid(displaced_by.into()),
                )
                .await
            {
                tracing::warn!(error = %e, %bot, run, "the run that lost its role could not be closed");
                left_open = true;
            }
        }
        Ok(Some(HolderTakenOver {
            was: found.state,
            left_open,
        }))
    }

    /// **The one mechanism behind renew and release: a move the store
    /// applies only while this sid still holds the role.** A renewal writes
    /// the holder and `now`; a release clears the holder and writes the UNIX
    /// epoch. Both carry a [`RoleMove`], which is what tells the store they
    /// are not claims: a claim may take a role nobody holds, so a renewal
    /// decided as a claim would lease the role again to a session whose
    /// release landed an instant earlier.
    ///
    /// **Both are compare-and-write, not read-then-write.** The store checks
    /// the holder in the same act as the write, so a rival that took over, or
    /// a release that landed, between this function's own pre-read and its
    /// write is protected by that check, not by the pre-read, which exists
    /// only to find which roles are worth attempting.
    ///
    /// **A `RoleTaken` or `RoleNotHeld` refusal here is success, not
    /// failure**: it is exactly "apply only while the holder is still this
    /// sid; otherwise change nothing", so it is not logged as an error. A
    /// `Conflict` is
    /// retried once — the store's own optimistic concurrency caught a
    /// genuine simultaneous write, and retrying the same call is the
    /// documented, correct response to it, not a guess.
    ///
    /// Best-effort throughout: a read or write failure here is logged and
    /// never turns the write this was called from into a failed call. It
    /// answers each role it wrote and whether the write held, or `None` when
    /// the roles could not be read.
    async fn write_role_claims(
        &self,
        bot: &EntityId,
        claimant: &str,
        at: jiff::Timestamp,
        verb: &'static str,
    ) -> Option<Vec<(crate::session::role_write::HeldRole, bool)>> {
        let held_roles = match self.roles_held_by(bot, claimant).await {
            Ok(roles) => roles,
            Err(e) => {
                tracing::warn!(error = %e, %bot, verb, "could not read the bot's roles");
                return None;
            }
        };
        let mut written = Vec::new();
        let today = at.to_zoned(jiff::tz::TimeZone::UTC).date();
        for held in held_roles {
            // **A renewal that would barely move the expiry writes nothing.**
            // The moment read with the held roles decides it, so a write
            // inside the renewal age costs no store write. A release is never
            // skipped, and a moment that does not parse is due.
            if verb == "renew" && !jojobot_domain::session::renewal_is_due(held.claimed_at, at) {
                continue;
            }
            let role = held.role.as_str();
            let write = || {
                if verb == "release" {
                    crate::session::role_write::RoleWrite::Release { claimant }
                } else {
                    crate::session::role_write::RoleWrite::Renew { claimant, at }
                }
            };
            let mut outcome = self.write_role_keys(bot, role, write(), today).await;
            if matches!(outcome, Err(MemoryError::Conflict)) {
                outcome = self.write_role_keys(bot, role, write(), today).await;
            }
            let held_up = match outcome {
                Ok(()) | Err(MemoryError::RoleTaken { .. } | MemoryError::RoleNotHeld { .. }) => {
                    true
                }
                Err(e) => {
                    tracing::warn!(error = %e, %bot, role, verb, "a role claim write failed");
                    false
                }
            };
            written.push((held, held_up));
        }
        Some(written)
    }
}

/// **What a wrap's release did to one role its run held.**
pub(crate) enum RoleRelease {
    /// The run no longer holds it, so a fresh session may claim it now.
    Released { role: String },
    /// The release was not written, so the run still holds it until its lease
    /// runs out at `until`; `None` when the claim moment does not read.
    NotReleased {
        role: String,
        until: Option<jiff::Timestamp>,
    },
    /// The roles could not be read, so which of them the run still holds is not
    /// known.
    NotRead,
}

/// **The one place a release's outcome becomes an answer**: one line per role
/// the run held, or nothing when it held none.
pub(crate) fn roles_json(released: &[RoleRelease]) -> Option<serde_json::Value> {
    if released.is_empty() {
        return None;
    }
    let lines = released
        .iter()
        .map(|outcome| match outcome {
            RoleRelease::Released { role } => {
                serde_json::json!({ "role": role, "released": true })
            }
            RoleRelease::NotReleased { role, until } => {
                let mut line = serde_json::json!({
                    "role": role,
                    "released": false,
                    "reason": "the store did not take the release, so this run still holds \
                               the role and a new claim on it is refused until its lease \
                               runs out",
                });
                if let Some(until) = until {
                    line["held_until"] = until.to_string().into();
                }
                line
            }
            RoleRelease::NotRead => serde_json::json!({
                "released": false,
                "reason": "the store could not say which roles this run held, so any it \
                           held stays held until its lease runs out",
            }),
        })
        .collect();
    Some(serde_json::Value::Array(lines))
}

impl Jojobot {
    /// **What a caller's own beat did to its chronology.**
    ///
    /// A chronology is append-only and only its newest entry can be amended,
    /// so a beat adds and never thins out. Saying so is the same work the
    /// memory writes do: a session that suspects a write replaces what it
    /// already recorded writes less than it knows, and a chronology is the one
    /// record whose whole worth is that nothing was left out of it.
    ///
    /// The length is read for this line and left out when the store cannot
    /// answer, since a number nothing backs is worse than the sentence alone.
    async fn what_a_beat_left_standing(&self, session: &SessionId) -> String {
        let length =
            self.sessions
                .read_session(session)
                .await
                .ok()
                .map_or_else(String::new, |run| {
                    format!(
                        " Its chronology is now {} long.",
                        crate::answer::counted(run.entries.len(), "entry", "entries")
                    )
                });
        format!(
            "Recorded as a new entry at the end of this session's chronology.{length} No earlier \
             entry was rewritten; only the newest one can be amended."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::session::Sid;

    /// **A beat names the entry it added, the focus it moved and the record it
    /// brought into being** (card 1333), all on the run's public name.
    ///
    /// The first beat of a run creates its record; a later one does not. A beat
    /// that sends the focus the run already holds moves nothing, so it names no
    /// focus; one that sends a new focus names what it was and what it is.
    #[tokio::test]
    async fn a_beat_names_the_entry_the_focus_and_the_record_it_made() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let beat = |entry: &str, focus: Option<&str>| {
            let (jojobot, sid) = (&jojobot, sid.clone());
            let (entry, focus) = (entry.to_string(), focus.map(str::to_string));
            async move {
                json_of(
                    &jojobot
                        .journal(Parameters(JournalArgs {
                            request_id: None,
                            entry,
                            focus,
                            working_on: None,
                            sid,
                        }))
                        .await
                        .expect("journal call ok"),
                )
            }
        };
        let named = |body: &serde_json::Value, what: &str| -> Option<serde_json::Value> {
            body["changed"]
                .as_array()
                .unwrap_or_else(|| panic!("a list of changes: {body}"))
                .iter()
                .find(|c| c["what"] == what)
                .cloned()
        };

        let first = beat("set out to read the plan", Some("reading the plan")).await;
        let run = first["session"].clone();
        assert_eq!(
            first["changed"].as_array().map(Vec::len),
            Some(3),
            "{first}"
        );
        let created = named(&first, "created").unwrap_or_else(|| panic!("created: {first}"));
        assert_eq!(created["on"], run, "{first}");
        let entry = named(&first, "entry").unwrap_or_else(|| panic!("entry: {first}"));
        assert_eq!(entry["on"], run, "{first}");
        assert_eq!(entry["to"], first["entry"]["id"], "{first}");
        assert!(entry.get("from").is_none(), "{first}");
        let focus = named(&first, "focus").unwrap_or_else(|| panic!("focus: {first}"));
        assert_eq!(focus["on"], run, "{first}");
        assert_eq!(focus["to"], "reading the plan", "{first}");
        assert!(focus.get("from").is_none(), "{first}");

        // The same focus again: the entry is new, the record and the focus are not.
        let second = beat("read the first page", Some("reading the plan")).await;
        assert_eq!(
            second["changed"].as_array().map(Vec::len),
            Some(1),
            "{second}"
        );
        let entry = named(&second, "entry").unwrap_or_else(|| panic!("entry: {second}"));
        assert_eq!(entry["to"], second["entry"]["id"], "{second}");
        assert_ne!(entry["to"], first["entry"]["id"], "{second}");

        let third = beat("moved on to the budget", Some("reading the budget")).await;
        assert_eq!(
            third["changed"].as_array().map(Vec::len),
            Some(2),
            "{third}"
        );
        let focus = named(&third, "focus").unwrap_or_else(|| panic!("focus: {third}"));
        assert_eq!(focus["from"], "reading the plan", "{third}");
        assert_eq!(focus["to"], "reading the budget", "{third}");
        assert!(named(&third, "created").is_none(), "{third}");
    }

    /// **A first beat that sends no focus still names the focus the record
    /// took** (card 2347). The record is made with a focus derived from the
    /// entry, so the focus moved from nothing to that line. A later beat with no
    /// focus moves nothing, which is the control: it names the entry alone.
    #[tokio::test]
    async fn a_first_beat_with_no_focus_names_the_focus_it_derived() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = journal_entry(&jojobot, &sid, "set out to read the plan").await;
        let changed = first["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {first}"));
        let focus = changed
            .iter()
            .find(|c| c["what"] == "focus")
            .unwrap_or_else(|| panic!("the derived focus is named: {first}"));
        assert_eq!(focus["on"], first["session"], "{first}");
        assert_eq!(focus["to"], "set out to read the plan", "{first}");
        assert!(focus.get("from").is_none(), "nothing was held: {first}");

        let second = journal_entry(&jojobot, &sid, "read the first page").await;
        assert!(
            second["changed"]
                .as_array()
                .unwrap_or_else(|| panic!("a list of changes: {second}"))
                .iter()
                .all(|c| c["what"] != "focus"),
            "a later beat with no focus moves none: {second}"
        );
    }

    /// **A beat says what now stands and what it left alone.**
    ///
    /// A chronology is append-only and only its newest entry can be amended,
    /// so a beat changes nothing that came before it. That is the same fear
    /// the memory writes answer — an agent that suspects a write overwrites
    /// what it already sent writes less than it knows — and a session's
    /// chronology is the one record whose whole value is that nothing thins it
    /// out.
    ///
    /// **The count is what makes the line worth reading**, so it is what this
    /// pins: a constant sentence cannot say the chronology got longer.
    #[tokio::test]
    async fn a_beat_says_the_chronology_only_grew() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);

        let first = journal_entry(&jojobot, &sid, "set out to find why the boot is slow").await;
        let opening = first["postcondition"]
            .as_str()
            .unwrap_or_else(|| panic!("a write states what now stands: {first}"))
            .to_string();
        assert!(
            opening.contains('1'),
            "the line has to say how long the chronology now is: {first}",
        );

        let second =
            journal_entry(&jojobot, &sid, "found it: the index rebuilds on every read").await;
        assert!(
            second["postcondition"]
                .as_str()
                .expect("a write states what now stands")
                .contains('2'),
            "a second beat and the line still says one, so it is not counting: {second}",
        );
    }

    /// **The count reads as English at one and at none.**
    ///
    /// A real model read *"1 entries long"* off this line in a paid run. The
    /// line exists to be read by something that reasons about what it says, so
    /// prose that announces itself as generated spends the trust the line was
    /// added to build.
    #[tokio::test]
    async fn the_chronology_count_reads_as_english_at_one() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let first = journal_entry(&jojobot, &sid, "set out to find why the boot is slow").await;
        let line = first["postcondition"].as_str().expect("a line").to_string();
        assert!(
            line.contains("1 entry") && !line.contains("1 entries"),
            "the singular case reads as generated text: {line}",
        );

        let second = journal_entry(&jojobot, &sid, "found it").await;
        let plural = second["postcondition"].as_str().expect("a line");
        assert!(
            plural.contains("2 entries"),
            "…and the plural still has to be plural: {plural}",
        );
    }

    /// **A failure that cannot say whether the write landed says THAT.**
    ///
    /// An append can fail with its entry already on the page — a reread that
    /// failed after it landed, or a rollback that failed too. A flat error
    /// reads as nothing-happened, and the retry it invites appends a second
    /// entry. Nothing here knows which state it is in, so the answer says so
    /// and sends the caller to look rather than guessing for them.
    #[tokio::test]
    async fn a_journal_whose_entry_fails_says_it_cannot_tell() {
        let store = Arc::new(RefusingAppend(InMemorySessions::new()));
        let jojobot = Jojobot::new(
            Arc::new(crate::memory::testing::InMemoryMemory::booted()),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            store.clone(),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: "set out to read the box".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("an uncertain outcome is an answer, not a protocol failure"),
        );

        assert_eq!(body["status"], "uncertain", "{body}");
        assert_eq!(
            body["wrote"], "unknown",
            "neither true nor false: nothing here knows: {body}"
        );
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("Do NOT send it again blind"),
            "the blind retry is the danger: {how}"
        );
        assert!(
            how.contains("read the session back"),
            "…and the way out is to look, not to guess: {how}"
        );

        assert!(
            !body.to_string().contains("the entry row on the page"),
            "the adapter's own words crossed: {body}"
        );
    }

    /// **A call that half succeeded says so, and says which half.**
    ///
    /// One journal call carries an entry and a focus. In production the entry
    /// committed, the focus rolled back, and the answer was a flat error —
    /// indistinguishable from nothing-happened, which is the safe reading
    /// everywhere else on this surface and the dangerous one here. The natural
    /// retry appends the entry a second time beside the one already recorded.
    ///
    /// Both halves are asserted, because the obvious one passes on its own:
    /// the caller learns the entry landed AND is told not to repeat the call.
    /// **A beat is answered with a receipt, not with the beat.**
    ///
    /// The caller wrote the entry in the call it is reading the answer to, so
    /// the text is the one thing in that answer it already holds — and a
    /// journal entry is prose at the length prose reaches, which makes this the
    /// most expensive echo a session pays for, once per beat, all run long.
    ///
    /// **Both halves.** What must go is the text; what must stay is everything
    /// the caller could not know — the id the entry was given, the moment
    /// jojobot stamped it, the session it landed in — plus the byte count and
    /// the opening, which is how a caller tells two beats apart without the
    /// bodies. A test for the absence alone passes on an empty answer.
    #[tokio::test]
    async fn a_beat_is_receipted_without_shipping_the_entry_back() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let entry = "read the hand-off, scoped the slice, and found the guard already covered it";

        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: entry.into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );

        assert_eq!(
            body["entry"]["text"],
            serde_json::Value::Null,
            "the author of the entry is the one reader it teaches nothing: {body}"
        );
        assert_eq!(body["entry"]["text_elided"], true, "{body}");
        assert_eq!(
            body["entry"]["text_bytes"],
            entry.len(),
            "the count is of what was stored, so a caller learns a trim happened: {body}"
        );
        assert!(
            body["entry"]["text_head"]
                .as_str()
                .is_some_and(|head| entry.starts_with(&head[..20])),
            "…and enough of the opening to tell two beats apart: {body}"
        );
        assert!(
            body["entry"]["id"].is_string(),
            "the id the entry was given is what a caller cannot know: {body}"
        );
        assert!(
            body["entry"]["at"].is_string(),
            "…and when it landed: {body}"
        );
        assert!(
            body["session"].is_string(),
            "the run the beat landed in is named: {body}"
        );

        // The whole thing is still reachable, and the answer says by which call.
        let how = body["entry"]["how_to_read"].as_str().expect("a way to it");
        assert!(
            how.contains("start_here"),
            "eliding is never silent — it names the call that returns it: {how}"
        );
    }

    #[tokio::test]
    async fn a_journal_whose_focus_fails_says_the_entry_landed() {
        let store = Arc::new(RefusingFocus(InMemorySessions::new()));
        let jojobot = Jojobot::new(
            Arc::new(crate::memory::testing::InMemoryMemory::booted()),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            store.clone(),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            crate::harness::seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: "set out to read the box".into(),
                    focus: Some("reading the box".into()),
                    sid: sid.clone(),
                }))
                .await
                .expect("a half-success is an answer, not a protocol failure"),
        );

        assert_eq!(body["status"], "partial", "{body}");
        assert_eq!(
            body["wrote"], true,
            "the default reading must be that something LANDED: {body}"
        );
        assert_eq!(body["recorded"], "entry");
        assert_eq!(body["not_recorded"], "focus");
        assert_eq!(
            body["entry"]["text"],
            serde_json::Value::Null,
            "the entry that landed is receipted rather than read back: {body}"
        );
        assert!(
            body["entry"]["id"].is_string() && body["entry"]["text_elided"] == true,
            "…and the receipt is what says it landed, which is the whole point of this \
             half-success: {body}"
        );

        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("do not send this call again"),
            "the retry is the danger, so the advice has to forbid it: {how}"
        );

        assert!(
            how.contains("`focus`") && !how.contains("no entry"),
            "the advice must name a call that exists, and `entry` is required: {how}"
        );

        assert!(
            !body.to_string().contains("the focus cell on the page"),
            "the adapter's own words crossed: {body}"
        );

        // The entry really is on the record, which is what makes a repeat a
        // duplicate rather than a retry.
        let session = store
            .0
            .read_session(&SessionId(
                body["session"]
                    .as_str()
                    .expect("the answer names it")
                    .into(),
            ))
            .await
            .expect("the session reads");
        assert_eq!(session.entries.len(), 1, "{session:?}");
    }

    /// Writing to a closed run must say something different depending on
    /// which end it reached, because the way forward is different: an
    /// abandoned run reopens, and telling its owner to start a new one
    /// instead sends them to fork the work they were trying to continue.
    #[tokio::test]
    async fn writing_to_a_closed_run_says_which_end_it_reached() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let stopped = abandoned_run(&store, "gamma", "reading the hand-off", 30).await;
        let told = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now(),
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

        let advice = |session: &SessionId| {
            let jojobot = &jojobot;
            let sid = as_run(jojobot, "gamma", session);
            async move {
                let body = blocked(
                    &jojobot
                        .journal(Parameters(JournalArgs {
                            request_id: None,
                            working_on: None,
                            entry: "one more thing".into(),
                            focus: None,
                            sid,
                        }))
                        .await
                        .expect("a closed session is an answer, not a protocol failure"),
                );
                body["how_to_proceed"].as_str().expect("advice").to_string()
            }
        };

        let on_stopped = advice(&stopped.id).await;
        assert!(
            on_stopped.contains("resume") && on_stopped.contains("start_here"),
            "a run that stopped is picked back up, not replaced: {on_stopped}"
        );
        assert!(
            on_stopped.contains("no bot"),
            "…and a caller that lost its bot name is told which call lists the bots: {on_stopped}"
        );
        assert!(
            !on_stopped.contains("belongs to a new session"),
            "…and it must not send the caller off to fork the work: {on_stopped}"
        );

        // The description that calls a wrapped run the last word carries the
        // wrap_code in the same sentence: a wrap hands one back.
        let tools = crate::Jojobot::tool_router().list_all();
        let journal = tools
            .iter()
            .find(|t| t.name == "journal")
            .expect("journal is served");
        let description = journal.description.as_deref().unwrap_or_default();
        let at = description
            .find("is the last word")
            .expect("the description says a wrapped run is the last word");
        let sentence = description[at..].split(". ").next().expect("a sentence");
        assert!(
            sentence.contains("wrap_code"),
            "the exception rides in the sentence that states the rule: {sentence}"
        );

        let on_told = advice(&told.id).await;
        assert!(
            on_told.contains("story has been told"),
            "a told story names the reason this end is the last word: {on_told}"
        );
        assert!(
            on_told.contains("wrap_code") && on_told.contains("start_here"),
            "…and names the wrap_code route to one last change: {on_told}"
        );
        assert!(
            !on_told.contains("Journal"),
            "…and never a shared Journal, which is not a thing here: {on_told}"
        );
        assert!(
            on_told.contains("new run") && on_told.contains("no resume"),
            "…and there the next run really is the way forward: {on_told}"
        );
    }

    /// **The refusal must name the identifier the caller actually carries.**
    ///
    /// The handle a caller passes is their `sid` — the only public name they
    /// hold (rule 205) — never the store's own internal session id. A refusal
    /// that named the internal id instead would leave a caller unable to tell
    /// that the run `ping` just called `wrapped` is the same one this refusal
    /// is about, because the two answers would be talking in two different
    /// vocabularies about the same run.
    #[tokio::test]
    async fn the_closed_refusal_names_the_sid_the_caller_carries_not_the_internal_id() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let told = store
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t002".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now(),
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
        let sid = as_run(&jojobot, "gamma", &told.id);

        let body = blocked(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: "one more thing".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("a closed session is an answer, not a protocol failure"),
        );

        assert_eq!(
            body["attempted"],
            run_name(&jojobot, &sid),
            "the refusal must name the caller's own run: {body}"
        );
        assert!(
            !body.to_string().contains(&sid),
            "…and never repeat the key it was sent: {body}"
        );
        assert_ne!(
            body["attempted"],
            told.id.as_str(),
            "…and not the store's internal session id, which the caller never carries: {body}"
        );
    }

    /// **A boot that does nothing leaves nothing behind.** The card materializes
    /// on the first write and never before, which is what keeps "creation is an
    /// intentional act" true for the one verb whose job is to start something.
    #[tokio::test]
    async fn booting_writes_no_session_card_until_the_first_write() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(booted["session"]["available"], true);
        assert_eq!(booted["session"]["resumed"], false, "nothing was in flight");
        assert!(
            booted["session"]["session"].is_null(),
            "…and no card was written"
        );
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "a boot that never works must leave no card at all"
        );

        // The first beat is what brings it into being.
        let sid = sid_of(&booted).expect("a handle");
        let journalled = journal_entry(&jojobot, &sid, "read the hand-off").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "the first entry materializes the card");
        assert_eq!(journalled["session"], live[0].id.as_str());
        assert_eq!(live[0].entries.len(), 1);
        assert_eq!(live[0].entries[0].text, "read the hand-off");
        assert_eq!(
            live[0].focus, "read the hand-off",
            "with nothing else to go on, what it first recorded is what it is doing"
        );
    }

    /// **THE BLOCKER: a first write is prose, and prose is not a focus.** The
    /// card materializes with a focus derived from the entry, so the focus's
    /// rules — one line, 200 characters, no backtick — were being applied to
    /// text nobody offered as a focus. A multi-line entry, a long story, or a
    /// one-liner naming code in backticks failed with `invalid entry` naming a
    /// `focus` parameter the caller never passed; the entry was dropped and no
    /// card appeared at all.
    ///
    /// The entry reaches the chronology **whole**. The focus is a glance, so it
    /// is derived: flattened, cut, and stripped of what a one-line display field
    /// cannot carry.
    #[tokio::test]
    async fn a_first_entry_is_prose_and_still_lands_whole() {
        let backticked = "started on `working_session`, which was the wrong shape";
        let long = "x".repeat(400);
        let cut = format!("{}…", "x".repeat(199));
        // The derived focus in full, not just its shape — a flatten that joined
        // with nothing would glue the words either side of a paragraph break
        // into one, and every rule-shaped assertion (no newline, no backtick,
        // within the cap) still holds of the glued line.
        let cases: [(&str, &str, &str); 3] = [
            (
                "multi-line",
                "read the hand-off\n\nthen scoped the slice",
                "read the hand-off then scoped the slice",
            ),
            (
                "backticked",
                backticked,
                "started on working_session, which was the wrong shape",
            ),
            ("over-long", &long, &cut),
        ];
        for (shape, entry, focus) in cases {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = with_sessions(store.clone());
            make_bot(&jojobot, "gamma").await;
            let sid = booted(&jojobot, "gamma").await;

            let body = json_of(
                &jojobot
                    .journal(Parameters(JournalArgs {
                        request_id: None,
                        working_on: None,
                        entry: entry.into(),
                        focus: None,
                        sid,
                    }))
                    .await
                    .unwrap_or_else(|e| panic!("a {shape} first entry must not error: {e:?}")),
            );
            assert_ne!(body["status"], "blocked", "{shape}: {body}");

            let live = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok");
            assert_eq!(live.len(), 1, "{shape}: the card must materialize");
            assert_eq!(
                live[0].entries[0].text,
                jojobot_domain::session::normalize_entry(entry),
                "{shape}: the entry reaches the chronology whole"
            );
            assert_eq!(
                live[0].focus, focus,
                "{shape}: the derived focus is display text, word for word"
            );
            assert!(
                live[0].focus.chars().count() <= 200,
                "{shape}: …and it is cut to fit: {:?}",
                live[0].focus
            );
        }
    }

    /// A focus the caller passed IS validated as a focus — the rules were never
    /// wrong, only misapplied. Its refusal names the parameter they actually
    /// sent, and it is a blocked ANSWER rather than a protocol error: a caller
    /// mistake does not leave through the error channel (rule 68).
    ///
    /// **The whole call is refused, so nothing is written.** The screen runs
    /// before the entry is appended, which is what lets the caller fix the
    /// argument and send the same call again — the alternative left the entry
    /// recorded and the focus unmoved, and reported that as jojobot's own
    /// storage failing.
    #[tokio::test]
    async fn an_explicit_focus_is_still_held_to_the_focus_rules() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        // **On a session that is already open**, which is the case that bites:
        // the entry appends before the focus moves, so a screen placed after
        // the append refuses the call with the entry already on the record.
        jojobot
            .journal(Parameters(JournalArgs {
                request_id: None,
                working_on: None,
                entry: "read the hand-off".into(),
                focus: None,
                sid: sid.clone(),
            }))
            .await
            .expect("the first beat lands");

        let body = blocked(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: "scoped the slice".into(),
                    focus: Some("two\nlines".into()),
                    sid,
                }))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false, "{body}");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        let said = jojobot_domain::session::validate_focus("two\nlines")
            .expect_err("a focus over two lines is refused")
            .to_string();
        assert!(
            advice.contains(&said),
            "the refusal names the fault.\n  wanted: {said}\n  got: {advice}"
        );

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let texts: Vec<&str> = live[0].entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            ["read the hand-off"],
            "a refused call writes nothing at all — not even the entry it carried"
        );
    }

    /// **A blank focus is refused before anything is written, and the refusal
    /// names the focus.** The screen used to skip a blank one, so the entry was
    /// appended and the focus then refused by the store, and the caller was told
    /// that jojobot's own storage had failed: a caller's mistake dressed as a
    /// fault (rule 130). Both shapes of a blank, on a run that is not open yet
    /// and on one that is. **The positive is in the same case**: a focus with
    /// words is taken, so the screen did not start refusing everything.
    #[tokio::test]
    async fn a_blank_focus_is_refused_before_the_entry_is_appended() {
        for blank in ["", "   "] {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = with_sessions(store.clone());
            make_bot(&jojobot, "gamma").await;
            let sid = booted(&jojobot, "gamma").await;
            let journal = |entry: &str, focus: Option<&str>| {
                let jojobot = &jojobot;
                let sid = sid.clone();
                let args = JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: entry.into(),
                    focus: focus.map(str::to_string),
                    sid,
                };
                async move { json_of(&jojobot.journal(Parameters(args)).await.expect("an answer")) }
            };

            // A run that is not open yet: the blank focus is refused, and no
            // record comes into being.
            let first = journal("scoped the slice", Some(blank)).await;
            assert_eq!(first["status"], "blocked", "{blank:?}: {first}");
            assert_eq!(first["wrote"], false, "{blank:?}: {first}");
            assert_eq!(
                first["fix_by"], "change",
                "{blank:?}: the call itself must change: {first}"
            );
            assert!(
                store
                    .sessions_of(&EntityId("bot:gamma".into()))
                    .await
                    .expect("list ok")
                    .is_empty(),
                "{blank:?}: a refused call brought no record into being"
            );

            // A run that is open: the entry is not appended either.
            let opened = journal("read the hand-off", Some("reading the hand-off")).await;
            assert_ne!(
                opened["status"], "blocked",
                "{blank:?}: a focus with words is taken: {opened}"
            );
            let second = journal("moved on", Some(blank)).await;
            assert_eq!(second["status"], "blocked", "{blank:?}: {second}");
            assert!(second.get("recorded").is_none(), "{blank:?}: {second}");
            let live = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok");
            let texts: Vec<&str> = live[0].entries.iter().map(|e| e.text.as_str()).collect();
            assert_eq!(
                texts,
                ["read the hand-off"],
                "{blank:?}: nothing was appended"
            );
            assert_eq!(
                live[0].focus, "reading the hand-off",
                "{blank:?}: the focus stands"
            );
        }
    }

    /// **The whole arc through the surface:** boot, journal with a focus, amend
    /// the beat, wrap. The focus is current truth and the chronology is history,
    /// and the wrap writes the story to both the session and the Journal.
    #[tokio::test]
    async fn the_session_arc_through_the_handler() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: "read the hand-off and scoped the slice".into(),
                    focus: Some("building the session context".into()),
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        assert_eq!(first["focus"], "building the session context");
        assert!(
            first["entry"]["beat"].is_null(),
            "a session's own entry is not a beat"
        );

        let amended = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "read the hand-off and scoped the slice properly".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("amend ok"),
        );
        assert_eq!(amended["entry"]["id"], first["entry"]["id"], "in place");

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "built the session context; the sweep is lazy until M8".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("wrap ok"),
        );
        assert_eq!(wrapped["session"]["state"], "wrapped");
        // **Wrapping publishes NOWHERE.** It told the story into a shared
        // Journal document, and the operator's ruling deletes that: the journal
        // goes dark until events land, and a wrap is the session's own record
        // closing.
        assert!(
            wrapped.get("journal").is_none(),
            "a wrap publishes nowhere, so it reports no publication: {wrapped}"
        );
        assert!(
            !jojobot
                .memory
                .scan()
                .await
                .expect("scan ok")
                .iter()
                .any(|doc| doc.title.trim() == "Journal"),
            "…and no shared Journal document was brought into being"
        );

        let read = store
            .read_session(&SessionId(
                first["session"].as_str().expect("a session id").to_string(),
            ))
            .await
            .expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        // The story is stored exactly as written; the closing entry's own
        // field carries the unpublished focus beside it, never folded in.
        assert_eq!(
            texts,
            vec![
                "read the hand-off and scoped the slice properly",
                "built the session context; the sweep is lazy until M8",
            ],
            "two entries: the amended one, and the story, unchanged"
        );
        assert_eq!(
            read.entries[1].closing_focus.as_deref(),
            Some("building the session context"),
            "…with the unpublished focus on the closing entry's own field"
        );
    }

    /// **The role a case holds, claimed through the door, and the handler's
    /// memory beside it** so the case can age the lease by writing its moment
    /// the way time would have left it.
    async fn holding_dev_dispatch() -> (Jojobot, Arc<InMemoryMemory>, String) {
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = handler_over(memory.clone());
        make_bot(&jojobot, "gamma").await;
        let booted = json_of(
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
        assert_eq!(booted["session"]["claim"]["status"], "taken", "{booted}");
        let sid = sid_of(&booted).expect("a handle");
        (jojobot, memory, sid)
    }

    /// **A beat renews the session's own role claim — no second mechanism.**
    /// The claim's own `claimed_at` field must actually move, not merely
    /// survive: a claim that renewed only in name would still read `stale`
    /// once its original threshold passed. The lease is aged past the renewal
    /// age first, because a write inside that age renews nothing.
    #[tokio::test]
    async fn a_beat_renews_the_session_s_own_role_claim() {
        let (jojobot, memory, sid) = holding_dev_dispatch().await;
        age_role_lease(&memory, "dev-dispatch", 10);

        let before = jojobot
            .memory
            .fields(&EntityId("role:dev-dispatch".into()))
            .await
            .expect("fields ok");
        let claimed_at_before = before
            .get("claimed_at")
            .cloned()
            .expect("the claim's own field is present");
        let holder_before = before
            .get("holder")
            .cloned()
            .expect("the claim's own holder is present");

        journal_entry(&jojobot, &sid, "did some of the work").await;

        let after = jojobot
            .memory
            .fields(&EntityId("role:dev-dispatch".into()))
            .await
            .expect("fields ok");
        assert_ne!(
            after.get("claimed_at"),
            Some(&claimed_at_before),
            "the beat must move the claim's own timestamp, not merely leave it standing: {after:?}"
        );
        assert_eq!(
            after.get("holder"),
            Some(&holder_before),
            "…and the holder is unchanged by a renewal: {after:?}"
        );
    }

    /// **A holder's many writes leave the lease record bounded.** Every write
    /// by the holder used to rewrite the lease moment, so the record gained a
    /// write per write for as long as the role was held. Twenty beats and
    /// twenty captures inside the renewal age write the moment no more than
    /// the claim itself did, the holder still holds, and the lease is still
    /// fresh. Paired with the aged case above, where the same beat DOES write.
    #[tokio::test]
    async fn a_holders_many_writes_leave_the_lease_record_bounded() {
        let (jojobot, _memory, sid) = holding_dev_dispatch().await;
        let role = EntityId("role:dev-dispatch".into());
        let key = "claimed_at";
        let written_by_the_claim = jojobot
            .memory
            .history(&role, key)
            .await
            .expect("history ok");
        assert_eq!(
            written_by_the_claim.len(),
            1,
            "the case rests on the claim writing the moment once: {written_by_the_claim:?}"
        );

        for n in 0..20 {
            journal_entry(&jojobot, &sid, &format!("beat number {n}")).await;
            capture_ok(
                &jojobot,
                CaptureArgs {
                    sid: Some(sid.clone()),
                    ..capture_args("person:alpha", &format!("a claim number {n}"))
                },
            )
            .await;
        }

        let writes = jojobot
            .memory
            .history(&role, key)
            .await
            .expect("history ok");
        assert_eq!(
            writes.len(),
            1,
            "forty writes inside the renewal age rewrote the lease moment: {writes:?}"
        );
        let fields = jojobot.memory.fields(&role).await.expect("fields ok");
        assert_eq!(
            fields.get("holder"),
            Some(&run_name(&jojobot, &sid)),
            "the holder still holds: {fields:?}"
        );
    }

    /// **Renewals past the renewal age overwrite the lease moment.** The
    /// bounded case above stays inside the renewal age, where nothing is
    /// written. Here the lease is aged before each beat, so every beat renews:
    /// the moment moves each time, and the holder's history stays at the one
    /// write the claim made, because a renewal appends nothing.
    #[tokio::test]
    async fn renewals_past_the_renewal_age_add_no_write_to_the_claim() {
        let (jojobot, memory, sid) = holding_dev_dispatch().await;
        let role = EntityId("role:dev-dispatch".into());
        let mut seen = Vec::new();
        for n in 0..5 {
            age_role_lease(&memory, "dev-dispatch", 10);
            journal_entry(&jojobot, &sid, &format!("aged beat number {n}")).await;
            let fields = jojobot.memory.fields(&role).await.expect("fields ok");
            seen.push(fields.get("claimed_at").cloned().expect("a moment"));
        }
        let moved: std::collections::BTreeSet<_> = seen.iter().collect();
        assert_eq!(
            moved.len(),
            5,
            "every aged beat renewed, so every moment is new: {seen:?}"
        );
        let holder = jojobot
            .memory
            .history(&role, "holder")
            .await
            .expect("history ok");
        assert_eq!(
            holder.len(),
            1,
            "five renewals rewrote no holder: {holder:?}"
        );
    }

    /// **A lease still lapses once it is old enough, and a write inside the
    /// renewal age does not stretch it.** Forty-six minutes after the last
    /// renewal a rival takes the role; forty-four minutes after, it is
    /// refused. The write the holder made at the start did not move the moment.
    #[tokio::test]
    async fn a_lease_still_lapses_after_the_lease_window_without_a_renewal() {
        let claim_as_rival = |jojobot: &Jojobot| {
            let jojobot = jojobot.clone();
            async move {
                json_of(
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
                )
            }
        };

        let (jojobot, memory, sid) = holding_dev_dispatch().await;
        journal_entry(&jojobot, &sid, "a write inside the renewal age").await;
        age_role_lease(&memory, "dev-dispatch", 44);
        let inside = claim_as_rival(&jojobot).await;
        // A lease 44 minutes old is still held.
        assert_eq!(inside["session"]["claim"]["status"], "refused", "{inside}");

        age_role_lease(&memory, "dev-dispatch", 46);
        let past = claim_as_rival(&jojobot).await;
        // A lease 46 minutes old with no renewal has lapsed.
        crate::harness::assert_lease_had_lapsed(&past);
    }

    /// A beat on a session holding no role claim does nothing extra and
    /// still lands — the renewal path is silent when there is nothing of
    /// this session's own to renew.
    #[tokio::test]
    async fn a_beat_with_no_role_claim_still_lands() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: "no role was ever claimed here".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        assert!(body["entry"]["id"].is_string(), "{body}");
    }

    /// A session verb on a connection that never booted is blocked with the way
    /// forward — jojobot will not guess which identity made the call.
    #[tokio::test]
    async fn a_session_verb_without_a_boot_is_blocked_with_the_way_forward() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    request_id: None,
                    working_on: None,
                    entry: "who am i".into(),
                    focus: None,
                    sid: String::new(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        let how = body["how_to_proceed"].as_str().expect("advice");
        // The remedy must be one that works on the caller's next call: it
        // must name `bot`, the address that survives a fresh connection, or
        // the very next call lands back here.
        assert!(
            how.contains("`sid`"),
            "the way out names the address: {how}"
        );
    }
}

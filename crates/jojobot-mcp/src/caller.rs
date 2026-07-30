//! **Who is calling**, resolved from the handle they carry — and the session
//! their write lands in.
//!
//! No client holds one MCP connection across a conversation, so an identity
//! written on the connection would be gone before the next request arrives.
//! **The handle is the only address**: it rides every verb, and jojobot looks
//! the caller up rather than remembering them.
//!
//! `sid` mints handles and holds them; this reads them, refuses the ones that
//! address nothing, and takes the gate a write runs under. It is not any one
//! context's: a memory write, a mailbox post and a session journal all pass
//! through here.

use super::*;

/// **Who is calling**, resolved from the handle they carry.
///
/// No client holds one MCP connection across a conversation — claude.ai and
/// ChatGPT both open a fresh, unbound connection per tool call — so an
/// identity written on the connection would be gone before the next request
/// arrives. **The handle is the only address**: it rides every verb, and
/// jojobot looks the caller up rather than remembering them.
#[derive(Debug, Clone)]
pub(crate) struct Caller {
    /// The handle itself, exactly as the caller passed it.
    pub(crate) sid: sid::Sid,
    /// The identity it belongs to. **Bound at boot and never switched**, which
    /// is what makes naming somebody else's session a refusal rather than a
    /// thing jojobot quietly honours.
    pub(crate) bot: EntityId,
    /// The card this run landed on, once one exists. `None` until the first
    /// real write materializes it.
    pub(crate) card: Option<SessionId>,
}

/// A session verb reached on a connection that never booted. Not an error: the
/// caller did nothing malformed, they just have no identity yet.
pub(crate) fn session_unbound() -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        "how_to_proceed": "Nothing was written. This call carried no `sid`, and jojobot will not \
                           guess which session is writing. Call start_here with your bot name to \
                           get one, then pass it on every call — reads included. It is the only \
                           address, and it is what tells jojobot which bot is asking: most \
                           clients open a fresh connection per tool call, so nothing about who \
                           you are survives from your last one.",
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **A session handle that addresses nothing.** The guards' own shape, so a
/// caller branches on `status` here exactly as everywhere else — and `wrote:
/// false` says the thing that matters most: a boot jojobot refused started no
/// session, so nothing on the board moved.
pub(crate) fn handle_declined(attempted: &str, how_to_proceed: String) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

impl Jojobot {
    /// **What this call serializes on: the IDENTITY, not the handle.**
    ///
    /// The handle looks like the right key — it names exactly the run this call
    /// will write to, and two writes on one handle are the pair the gate was
    /// first built for. It is too narrow by one caller. A boot resolves the
    /// whole bot's board, so it can only key on the bot; a write knows its
    /// handle and nothing else; and the two are about the same run whenever
    /// that run's card does not exist yet. Keyed separately they queue apart,
    /// and the boot reads the board inside the gap between the write committing
    /// its card and the registry being told which handle it landed on — finding
    /// a live run no handle addresses, and minting a second one for it.
    ///
    /// The identity is the only name both callers hold, so it is the key. It
    /// serializes two writes on one handle exactly as before (same bot, same
    /// key) and two runs of one bot besides, which is a cost bounded by how many
    /// identities this operator has.
    ///
    /// A handle this process is not holding keys on itself, and a call carrying
    /// none keys on the empty string rather than skipping the lock, so there is
    /// exactly one code path; both are refused downstream anyway.
    pub(crate) fn gate_key(&self, sid: Option<&str>) -> String {
        let Some(raw) = sid.map(str::trim).filter(|s| !s.is_empty()) else {
            return String::new();
        };
        match self.registry.lookup(raw) {
            Some(held) => held.bot.as_str().to_string(),
            None => raw.to_string(),
        }
    }

    /// **Who is calling.** `None` is an anonymous caller, which is a legitimate
    /// thing to be: a reader, or a poster who has not booted.
    pub(crate) fn caller(&self, sid: Option<&str>) -> Result<Option<Caller>, CallToolResult> {
        let Some(raw) = sid.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        if !sid::is_readable(raw) {
            return Err(handle_declined(
                raw,
                format!(
                    "Nothing was written. '{raw}' is not a handle jojobot mints — those are {} \
                 characters of 0-9 and a-z, with i, l, o and u left out because they read as \
                 one another. jojobot will not correct one, because correcting it means \
                 guessing whose session you meant.",
                    jojobot_domain::session::SID_LEN,
                ),
            ));
        }
        let Some(held) = self.registry.lookup(raw) else {
            return Err(handle_declined(
                raw,
                format!(
                    "Nothing was written. That session is gone: '{raw}' is not a handle jojobot \
                 is holding. Call start_here with your bot name to boot again — the work on \
                 the board is untouched, and it will be offered back by what it was working \
                 on."
                ),
            ));
        };
        Ok(Some(Caller {
            sid: sid::Sid(raw.to_string()),
            bot: held.bot,
            card: held.card,
        }))
    }

    /// **A handle that is present must be good, even where carrying one is
    /// optional.**
    ///
    /// The write verbs outside the session surface take an optional `sid`:
    /// carrying none is legitimate — a reader, a poster that never booted —
    /// and costs only the automatic beat. Carrying a DEAD one must be
    /// refused up front: leaving it to [`Jojobot::beat`] (silent by design)
    /// would mean the write lands, the caller's chronology stops, and they
    /// find out only at wrap, if ever.
    ///
    /// Called BEFORE the write, never after. `beat` runs once the store has
    /// already answered, and `blocked` means `wrote: false` everywhere on this
    /// surface — one handed back over a write that landed would be a worse lie
    /// than the silence it replaced.
    pub(crate) fn attributable(&self, sid: Option<&str>) -> Result<(), CallToolResult> {
        self.caller(sid).map(|_| ())
    }

    /// The caller, required — for the verbs that write to a session.
    pub(crate) fn identified(&self, sid: Option<&str>) -> Result<Caller, CallToolResult> {
        match self.caller(sid)? {
            Some(caller) => Ok(caller),
            None => Err(session_unbound()),
        }
    }

    /// Mint a handle, or turn the one failure into an answer rather than a 500.
    pub(crate) fn mint_or_say_why(
        &self,
        bot: &EntityId,
        card: Option<SessionId>,
    ) -> Result<sid::Sid, CallToolResult> {
        self.registry.mint(bot, card).map_err(|_| {
            handle_declined(
                "",
                "No session was started. jojobot could not mint a free session handle, which \
             means this process is holding a great many of them. Nothing is wrong with your \
             call and nothing on the board was touched — a restart clears the handles it is \
             holding."
                    .to_string(),
            )
        })
    }

    /// The handle for a card that exists — the one it already has, or a new one.
    pub(crate) fn handle_for(
        &self,
        bot: &EntityId,
        card: &SessionId,
    ) -> Result<sid::Sid, CallToolResult> {
        match self.registry.addressing(card) {
            Some(handle) => Ok(handle),
            None => self.mint_or_say_why(bot, Some(card.clone())),
        }
    }

    /// **The session this call writes to, resolved from the handle it carries.**
    ///
    /// One address, and no ladder. The old resolver had three — an explicit
    /// session id, a bot name resolved against the board, and the connection's
    /// binding — because none of them worked everywhere: the binding died with
    /// the connection, and a session id could not be used before the first write
    /// had minted one. The handle has neither problem. It exists from the moment
    /// the door hands it over, it rides every call, and it names exactly one
    /// run.
    ///
    /// **The card is still lazy.** A handle with no card behind it gets one
    /// here, on the first real write and never before, so a boot that does
    /// nothing still leaves nothing behind.
    pub(crate) async fn session_for(
        &self,
        // **Proof the gate is held.** This reads the registry, awaits a store
        // call and writes the registry back; two calls inside that span would
        // both find no card and both begin one. Taking the guard by reference
        // makes the requirement impossible to forget rather than a comment
        // somebody has to read.
        _serialized: &tokio::sync::MutexGuard<'_, ()>,
        caller: &Caller,
        explicit_focus: Option<&str>,
        derive_from: Option<&str>,
    ) -> Result<SessionId, McpError> {
        if let Some(card) = &caller.card {
            return Ok(card.clone());
        }
        // **The focus is DERIVED, and the entry is not touched.** A first write
        // is prose — a multi-line entry, a story, a line naming code in
        // backticks — and a focus is one line of display text. Feeding the one
        // to the other applied the focus's rules to text nobody offered as a
        // focus: the write failed with `invalid entry`, naming a parameter the
        // caller never passed, and the entry it was carrying was dropped.
        let focus = match explicit_focus.map(str::trim).filter(|f| !f.is_empty()) {
            Some(theirs) => theirs.to_string(),
            None => display_line(derive_from.unwrap_or(FRESH_FOCUS)),
        };
        let begun = self
            .sessions
            .begin(NewSession {
                bot: caller.bot.clone(),
                sid: caller.sid.clone(),
                focus,
                started_at: jiff::Timestamp::now(),
            })
            .await
            .map_err(session_error)?;
        // The registry learns the card here — this is the moment one exists.
        self.registry.attach_card(&caller.sid, begun.id.clone());
        Ok(begun.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;
    use rmcp::handler::server::wrapper::Parameters;

    /// **THE PRODUCTION SHAPE: identity does not survive to the next call.**
    /// Every session verb was addressed by a connection binding, and no real
    /// client holds a connection — so the boot bound an identity to something
    /// that evaporated before the next request arrived, and every write after it
    /// came back "not running as any identity".
    ///
    /// The chicken-and-egg made addressing by `session` no help either: a
    /// session materializes lazily on the first write, and the first write could
    /// never land, so no id was ever minted to name. **The `sid` has neither
    /// problem** — the door mints it before any card exists and hands it back,
    /// so a caller that keeps nothing but that string writes to the same run
    /// across as many connections as its client opens.
    #[tokio::test]
    async fn a_stateless_client_can_journal_by_carrying_its_sid() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma").await;

        // Call 1: boot. Succeeds, as it did in production.
        let opened = boot(&client.call(), "gamma").await;
        assert_eq!(opened["session"]["available"], true);
        let sid = sid_of(&opened).expect("a handle");

        // Call 2: a different connection, as every real client presents.
        let body = json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal call ok"),
        );
        assert_ne!(
            body["status"], "blocked",
            "the sid is enough, on a connection that remembers nothing: {body}"
        );

        let live = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session, minted by the first write: {live:?}"
        );
        assert_eq!(live[0].entries[0].text, "read the hand-off");

        // Call 3: another fresh connection ATTACHES to that session rather than
        // forking a second one — the whole point of resolving from the board.
        json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "picked it back up".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal call ok"),
        );
        let live = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "still one session, not one per call: {live:?}"
        );
        assert_eq!(
            live[0].entries.len(),
            2,
            "…and it accrued: {:?}",
            live[0].entries
        );
    }

    /// Writing with another identity's `sid` must never move mine: a `sid`
    /// addresses one run only and says nothing about the caller's other
    /// handles.
    ///
    /// This is the stateful-transport shape — stdio, where connections really
    /// do persist — so it holds one handler across calls on purpose: the shape
    /// where a leftover binding would still have somewhere to live.
    #[tokio::test]
    async fn writing_with_another_identitys_sid_leaves_mine_where_it_was() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection(memory.clone(), store.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_session = mine["session"].as_str().expect("a session").to_string();

        // A deliberate write into the other identity's session.
        let other = booted(&jojobot, "delta").await;
        let theirs = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "a note for delta".into(),
                    focus: None,
                    sid: other,
                }))
                .await
                .expect("journal ok"),
        );
        assert_ne!(
            theirs["session"],
            my_session.as_str(),
            "it landed in delta's session"
        );

        // …and I am still gamma.
        let after = journal_entry(&jojobot, &sid, "my second beat").await;
        assert_eq!(
            after["session"],
            my_session.as_str(),
            "the connection is still gamma's"
        );

        let gamma = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(gamma.len(), 1, "one session for gamma: {gamma:?}");
        assert_eq!(gamma[0].entries.len(), 2, "…carrying both of my beats");
        let delta = store
            .sessions_of(&EntityId("bot:delta".into()))
            .await
            .expect("list ok");
        assert_eq!(delta.len(), 1, "and one for delta");
        assert_eq!(delta[0].entries.len(), 1);
    }

    /// Two identities alive on ONE connection must each keep their own
    /// session: nothing may be remembered between calls — the answer must
    /// always come from the `sid`, never from whichever identity spoke last
    /// on this connection.
    #[tokio::test]
    async fn two_identities_on_one_connection_each_keep_their_own_session() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection(memory.clone(), store.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_session = mine["session"].as_str().expect("a session").to_string();

        // My own handle must land in MY session.
        let named = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "named myself".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        assert_eq!(
            named["session"],
            my_session.as_str(),
            "my own handle lands in my own session, not another: {named}"
        );

        // And a DIFFERENT identity's handle must not be served from mine.
        let theirs = booted(&jojobot, "delta").await;
        let other = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "named somebody else".into(),
                    focus: None,
                    sid: theirs,
                }))
                .await
                .expect("journal ok"),
        );
        assert_ne!(
            other["session"],
            my_session.as_str(),
            "gamma's session must not answer for delta's handle: {other}"
        );
    }

    /// **BLOCKER: a write must not mint a session for a bot that does not
    /// exist.** The door refuses an unknown name with the roster, and its own
    /// comment says why — a session bound to an identity jojobot just refused
    /// belongs to nobody. Making the bot NAME the address opened a second door
    /// into `begin` with no such screen; making the HANDLE the address closes it
    /// for good, because a handle is not a thing a caller can compose. jojobot
    /// either issued it or it did not.
    ///
    /// What a typo costs if this ever regresses: one permanent card (there is no
    /// delete verb; the sweep only marks it `abandoned` a day later), a beat
    /// misattributed away from the caller's real session, and through
    /// `wrap_session` a dated story written into the operator's Journal under a
    /// run nobody started.
    #[tokio::test]
    async fn a_session_verb_carrying_an_unheld_handle_blocks_and_writes_nothing() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma").await;
        // A well-formed handle jojobot never minted — the nearest thing left to
        // the typo this spec was about.
        let typo = "gamm";

        for (verb, body) in [
            (
                "journal",
                json_of(
                    &client
                        .call()
                        .journal(Parameters(JournalArgs {
                            entry: "read the hand-off".into(),
                            focus: None,
                            sid: typo.into(),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "wrap_session",
                json_of(
                    &client
                        .call()
                        .wrap_session(Parameters(WrapSessionArgs {
                            story: "a story for nobody".into(),
                            sid: typo.into(),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "amend_journal",
                json_of(
                    &client
                        .call()
                        .amend_journal(Parameters(AmendJournalArgs {
                            entry: "actually".into(),
                            sid: typo.into(),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
        ] {
            assert_eq!(
                body["status"], "blocked",
                "{verb} minted a session for a handle nobody was given: {body}"
            );
            assert_eq!(body["wrote"], false);
            assert_eq!(
                body["attempted"], typo,
                "{verb}: the refusal quotes it back: {body}"
            );
            // **No candidates, and that is the difference from a name.** A bot
            // name is a thing jojobot can suggest neighbours for; a handle is
            // four characters of entropy, and the nearest one is somebody
            // else's session. Guessing here would hand a caller a run that is
            // not theirs, so the way out is to boot rather than to pick.
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("start_here"),
                "{verb}: the way out is the door, not a neighbour: {how}"
            );
        }

        assert!(
            client
                .sessions
                .sessions_of(&EntityId("bot:gamm".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "no card was written for an identity nobody created"
        );
        // …and the refused wrap told its story nowhere.
        //
        // It reads every session on the store rather than this bot's, because
        // "this bot has no sessions" is already asserted above; what is left to
        // rule out is the story landing in somebody ELSE's chronology, which is
        // the failure a handle four characters from a live one would produce.
        let told: Vec<String> = client
            .sessions
            .all_sessions()
            .await
            .expect("all_sessions ok")
            .into_iter()
            .flat_map(|s| s.entries.into_iter().map(|e| e.text))
            .collect();
        assert!(
            !told
                .iter()
                .any(|entry| entry.contains("a story for nobody")),
            "a refused wrap wrote its story into a chronology: {told:?}"
        );
    }

    /// The other two session verbs take the same one address — a stateless
    /// client has to be able to amend and to wrap, not only to journal.
    #[tokio::test]
    async fn a_stateless_client_can_amend_and_wrap_by_carrying_its_sid() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma").await;
        let sid = booted(&client.call(), "gamma").await;

        // Amending before anything exists is still refused, not a begin.
        let nothing = json_of(
            &client
                .call()
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "actually".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(
            nothing["status"], "blocked",
            "nothing to amend, and nothing begun: {nothing}"
        );
        assert!(
            client
                .sessions
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "an amend never mints a card"
        );

        client
            .call()
            .journal(Parameters(JournalArgs {
                entry: "read the hand-off".into(),
                focus: None,
                sid: sid.clone(),
            }))
            .await
            .expect("journal ok");

        let amended = json_of(
            &client
                .call()
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "read the hand-off, and scoped it".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("call ok"),
        );
        assert_ne!(amended["status"], "blocked", "{amended}");

        let wrapped = json_of(
            &client
                .call()
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "built the thing and told the story".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("wrap ok"),
        );
        assert_eq!(wrapped["session"]["state"], "wrapped", "{wrapped}");

        let live = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session across five connections: {live:?}"
        );
        assert_eq!(
            live[0].entries[0].text, "read the hand-off, and scoped it",
            "the amend landed in place: {:?}",
            live[0].entries
        );
    }

    /// **A boot that fails leaves a session already in flight alone.** A typo in
    /// a bot name must not disturb the handle its caller is already writing
    /// under — that would turn one mistyped call into lost work on the next
    /// write, and a boot has no business reaching a run it did not name.
    #[tokio::test]
    async fn a_failed_boot_leaves_a_live_sid_writing_where_it_was() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_id = mine["session"].as_str().expect("a session id").to_string();

        // A name that is no bot.
        let missed = boot(&jojobot, "nobody-by-that-name").await;
        assert_eq!(missed["status"], "blocked", "the boot missed: {missed}");

        // …and the next write is still mine.
        let after = journal_entry(&jojobot, &sid, "my second beat").await;
        assert_eq!(
            after["session"],
            my_id.as_str(),
            "the handle still addresses the same run after the miss"
        );
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "and no second card was minted: {live:?}");
        assert_eq!(live[0].entries.len(), 2);
    }

    /// **A handle jojobot is not holding is refused by the write verbs, not
    /// quietly ignored.**
    ///
    /// These seven verbs take an optional `sid`, and `beat` was the only place
    /// any of them looked at it. `beat` is silent by design — three cases where
    /// there is nobody to record for — and it swallowed the refusal along with
    /// them, because it read `caller()` as "some caller or none" when that
    /// method distinguishes THREE answers: nobody (fine), a handle that is not
    /// a handle, and a handle whose session is gone.
    ///
    /// So a caller holding a dead sid wrote successfully, its chronology
    /// silently stopped, and it found out at wrap or never — which is the
    /// failure mode a handle exists to prevent, arriving in the one shape
    /// nothing reports.
    ///
    /// **Refused BEFORE the write, not propagated out of `beat`**, which runs
    /// after it: `blocked` means `wrote: false` everywhere on this surface, and
    /// one handed back over a write that already landed would be a worse lie
    /// than the silence.
    #[tokio::test]
    async fn a_dead_sid_is_refused_by_the_write_verbs_rather_than_swallowed() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        ensure(&jojobot, "alpha").await;
        make_bot(&jojobot, "gamma").await;
        make_box(&jojobot, "somewhere").await;
        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "somewhere".into(),
                    body: "something to retire later".into(),
                    sid: booted(&jojobot, "gamma").await,
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        let message = posted["id"].as_str().expect("an id").to_string();

        // Well-formed and never minted: the shape a handle takes after a
        // restart, or after the run it named was swept.
        let dead = "2gf7".to_string();
        assert!(sid::is_readable(&dead));

        let refusals: Vec<(&str, serde_json::Value)> = vec![
            (
                "capture",
                blocked(
                    &jojobot
                        .capture(Parameters(CaptureArgs {
                            sid: Some(dead.clone()),
                            ..capture_args("alpha", "plays go")
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "add_entity",
                blocked(
                    &jojobot
                        // **A bot, not a person**, so the box assertion below
                        // has something to bite on: `add_entity` is the verb
                        // that opens boxes now, and a person would leave the
                        // mail world untouched however broken the refusal was.
                        .add_entity(Parameters(AddEntityArgs {
                            kind: "bot".into(),
                            handle: "delta".into(),
                            name: "Delta".into(),
                            aliases: None,
                            source: "test-fixture".into(),
                            crm: None,
                            boot: None,
                            create_new: None,
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "update_entity",
                blocked(
                    &jojobot
                        .update_entity(Parameters(UpdateEntityArgs {
                            handle: "person:alpha".into(),
                            name: Some("Alpha".into()),
                            aliases: None,
                            source: None,
                            crm: None,
                            create_new: None,
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "update_fact",
                blocked(
                    &jojobot
                        .update_fact(Parameters(UpdateFactArgs {
                            sid: Some(dead.clone()),
                            ..update_args("person:alpha#1")
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "set_charter",
                blocked(
                    &jojobot
                        .set_charter(Parameters(SetCharterArgs {
                            bot: "gamma".into(),
                            prose: "a charter nobody asked for".into(),
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "mark_processed",
                blocked(
                    &jojobot
                        .mark_processed(Parameters(MarkProcessedArgs {
                            message_id: message.clone(),
                            notes: None,
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
        ];
        for (verb, body) in &refusals {
            assert_eq!(
                body["attempted"], dead,
                "{verb} must name the handle it refused: {body}"
            );
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("gone") && how.contains("start_here"),
                "{verb} must say the session is gone and where to get another: {how}"
            );
        }

        // …and every one of them wrote nothing, which is what `wrote: false`
        // above is claiming.
        assert!(
            !jojobot
                .memory
                .list_entities(None)
                .await
                .expect("list ok")
                .iter()
                .any(|e| e.id.as_str() == "bot:delta"),
            "add_entity wrote an entity behind a refusal"
        );
        assert!(
            jojobot
                .memory
                .recall(&EntityId("person:alpha".into()))
                .await
                .expect("recall ok")
                .is_empty(),
            "capture wrote a fact behind a refusal"
        );
        // **`add_entity` is the box-opening verb now**, so its refusal is the one
        // that must leave the mail world untouched: a bot refused for a dead
        // handle must not leave a box behind either.
        assert!(
            !jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .iter()
                .any(|b| b.name.as_str() == "delta"),
            "add_entity opened a box behind a refusal"
        );
    }

    /// **Two tool calls in flight on one handle must not fork the session.**
    /// rmcp runs one task per request, and the card behind a handle is read,
    /// awaited across, and written back — so without a gate both calls see "no
    /// card yet" and both materialize one, and two same-class verbs both append
    /// a beat.
    #[tokio::test]
    async fn concurrent_first_writes_materialize_exactly_one_card() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = racing(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let one = jojobot.journal(Parameters(JournalArgs {
            entry: "first".into(),
            focus: None,
            sid: sid.clone(),
        }));
        let two = jojobot.journal(Parameters(JournalArgs {
            entry: "second".into(),
            focus: None,
            sid: sid.clone(),
        }));
        let (a, b) = tokio::join!(one, two);
        a.expect("journal ok");
        b.expect("journal ok");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session, not one per racing call: {live:?}"
        );
        assert_eq!(live[0].entries.len(), 2, "…carrying both entries");
    }

    /// **Two writers on one identity, on two connections, must not fork it.**
    ///
    /// The gate that stops this was a mutex on the HANDLER, and the transport
    /// builds one handler per connection — so it excluded nothing between calls,
    /// which on a client with no session affinity means it excluded nothing at
    /// all. Both callers read a card that did not exist, both began one, and the
    /// loser's chronology was orphaned on a card nothing would ever address
    /// again: a session whose story is never told, by construction.
    ///
    /// It was masked while addressing was by bot name and the board was
    /// re-resolved every call. Nothing masks it now that the `sid` names one
    /// specific session, so the lock lives on the one structure that is already
    /// process-wide and already keyed by the thing being serialized — the
    /// registry, which two connections of one process share and a handler does
    /// not.
    ///
    /// **Both orders, because only one of them forks.** `tokio::join!` rotates
    /// which future it polls first.
    #[tokio::test]
    async fn two_connections_writing_as_one_bot_do_not_fork_the_card() {
        for first_wins in [true, false] {
            let client = NoAffinity::new();
            make_bot(&client.call(), "gamma").await;
            // The handle outlives the connection that was handed it — that is
            // the whole point of it — so one boot serves both writers below.
            let sid = booted(&client.call(), "gamma").await;

            // **A store that yields between its steps.** The in-memory fake
            // never suspends inside the read-then-begin span, so two futures on
            // one runtime finish it one after the other and the race cannot
            // happen — a green test proving nothing. This is the same double the
            // single-connection race test uses.
            let racing_ports = |ports: &NoAffinity| {
                Jojobot::new(
                    ports.memory.clone(),
                    Arc::new(SpySearch::default()),
                    ports.mailboxes.clone(),
                    Arc::new(Yielding(ports.sessions.clone())),
                    ports.registry.clone(),
                )
            };
            let write = |entry: &'static str| {
                let jojobot = racing_ports(&client);
                let sid = sid.clone();
                async move {
                    jojobot
                        .journal(Parameters(JournalArgs {
                            entry: entry.into(),
                            focus: None,
                            sid,
                        }))
                        .await
                        .expect("journal ok")
                }
            };

            // Two connections, as two agents booted as one identity present —
            // or as one assistant turn issuing parallel tool calls.
            let (a, b) = (write("the first beat"), write("the second beat"));
            if first_wins {
                let (x, y) = tokio::join!(a, b);
                json_of(&x);
                json_of(&y);
            } else {
                let (y, x) = tokio::join!(b, a);
                json_of(&x);
                json_of(&y);
            }

            let live: Vec<Session> = client
                .sessions
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .into_iter()
                .filter(|s| !s.state.is_terminal())
                .collect();
            assert_eq!(
                live.len(),
                1,
                "first_wins={first_wins}: one card, not one per connection: {live:?}"
            );
            assert_eq!(
                live[0].entries.len(),
                2,
                "first_wins={first_wins}: …and neither beat was orphaned: {:?}",
                live[0].entries
            );
        }
    }
}

#[cfg(test)]
mod begin_retry {
    //! **A `begin` that commits and then fails must not fork the run.**

    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;
    use rmcp::handler::server::wrapper::Parameters;

    /// Driven through the real `journal` verb, not through the port: the
    /// failure the caller actually meets is a write it made, not a store call
    /// it chose.
    ///
    /// A store's `begin` is a write followed by a read-back. When the write
    /// lands and the read-back does not, `begin` returns `Err` with the row
    /// committed — and the registry, which learns the card only on success,
    /// still holds `card: None`. The agent still has its `sid`, so it journals
    /// again. Nothing on the append path asked whether a run already carried
    /// that handle, so a second row appeared under it: one sid, two active
    /// runs, and the id the whole trace hangs from naming two things.
    #[tokio::test]
    async fn a_begin_that_commits_and_then_fails_leaves_one_run_under_one_sid() {
        let store = Arc::new(CommitsThenFails::new());
        let jojobot = with_sessions_port(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let entry = |text: &str| JournalArgs {
            entry: text.into(),
            focus: None,
            sid: sid.clone(),
        };

        // The first write commits its row and reports failure.
        let failed = jojobot.journal(Parameters(entry("the first beat"))).await;
        assert!(
            failed.is_err(),
            "the double reports the read-back failure: {failed:?}"
        );

        // The agent still holds its sid, so it writes again — the retry.
        jojobot
            .journal(Parameters(entry("the same run, carrying on")))
            .await
            .expect("the retry lands");

        let live: Vec<_> = store
            .inner
            .all_sessions()
            .await
            .expect("read ok")
            .into_iter()
            .filter(|s| s.sid.as_ref().is_some_and(|h| h.as_str() == sid))
            .collect();
        assert_eq!(
            live.len(),
            1,
            "one sid must address one run — found {}: {live:?}",
            live.len()
        );
    }
}

//! `read_message` — Take delivery of one message by id, leaving the rest of its box alone.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `read_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadMessageArgs {
    /// The message's id, exactly as a search hit, a delivery or `post_message`
    /// returned it.
    pub(crate) message_id: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **Read the message's history of state changes instead of taking it.**
    /// With `true` the call takes NO delivery and moves nothing: the message is
    /// left in the state it is in, and the answer is its changes in the order
    /// they happened, each naming the run, when, the state it left, and the
    /// notes or reason it was given. Allowed exactly where reading the message
    /// is. The author of a message in a box above its own reads it without the
    /// recipient's notes and reasons, and `notes_withheld` says so. Without it a
    /// read of your own box takes delivery.
    #[serde(default)]
    pub(crate) history: Option<bool>,
}

/// Take delivery of one message by id, leaving the rest of its box alone.
#[tool_router(router = read_message_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Take delivery of ONE message by id — the selective half of \
                       read_mailbox, for when you want a single message (the one a search hit \
                       named) and have no business owning the rest of the box. That one moves \
                       `new` to `read` in YOUR OWN BOX; nothing else in the box is touched. \
                       DOWN THE CHART, you may also read any message in every bot's box below \
                       you on the reports_to chain, or a message you sent above or beside you. \
                       That read shows its current state, new \
                       included, with taken: false: it moves nothing and obliges nobody. \
                       The message remains its owner's to handle. Other mail above or beside you \
                       is refused in every state, processed included; post_message can ask \
                       its owner for what you need. THE OPERATOR'S BOX IS NOT A BOT'S: no bot opens a \
                       message in it, in any state, whoever wrote it. Same envelope a \
                       delivery hands over, seen_before and all: true means somebody had \
                       already taken this message, so it is a leftover rather than fresh \
                       mail. Taking delivery is NOT handling: call mark_processed once you \
                       have acted on your own mail, and only then. Reading your own mail renews \
                       your role claim. WITH `history: true` THIS VERB TAKES NO \
                       DELIVERY: it answers the message's changes of state in order, with the \
                       run that made each, and leaves the message where it is. A refusal is an answer with status: blocked \
                       and nothing taken, in three kinds: an id that names nothing at all; an \
                       id naming an item jojobot cannot read (which comes back saying why, \
                       and needs a person rather than a retry); and a message that is not \
                       yours to take. A message a DIFFERENT run posted than the one reading \
                       it carries `written_by_other_run`, naming that run and whether it has \
                       since ended."
    )]
    pub(crate) async fn read_message(
        &self,
        Parameters(args): Parameters<ReadMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = MessageId(args.message_id.trim().to_string());
        // **Whose message this is has to be settled BEFORE the delivery**, not
        // after, because the delivery is the thing being guarded: `read_message`
        // moves `new` to `read`, and a refusal handed back over a message it had
        // already moved would be the breach plus a lie about it.
        //
        // Located through `message_by_id`, which is a read that moves nothing
        // and asks the store for this one id. An id it cannot place falls
        // through untouched: a message nobody can find is not deliverable
        // either, and the verb below owns those two refusals (an id that names
        // nothing, and one naming something unreadable) with words this has no
        // business duplicating.
        let located = self
            .mailboxes
            .message_by_id(&id)
            .await
            .map_err(mailbox_error)?;
        // **A person's box is read by no bot, in any state.** This privacy
        // refusal must precede the chart refusal so it names no private box.
        // A board that cannot say refuses too, and so does a
        // box it does not list. A card jojobot cannot read answers no lookup by
        // id, and the refusal that follows would describe what is wrong with
        // it.
        if let Some(refused) = self
            .privacy_refusal(&id, located.as_ref())
            .await
            .map_err(mailbox_error)?
        {
            return Ok(refused);
        }
        let readable = located;
        let mut peek = false;
        // **An author reading its own mail in a box above it** is told nothing the
        // recipient wrote on it (rule 139: never up).
        let mut upward = false;
        if let Some(message) = &readable {
            // The same three refusals a read of your own box gets — no handle,
            // a world that cannot say, a bot whose box is missing — because it
            // is the same question, asked by the same caller, about the same
            // thing.
            let mine = match self.my_box(args.sid.as_deref()).await {
                Ok(mine) => mine,
                Err(refused) => return Ok(refused),
            };
            let visible = self.mail_visibility(Some(&mine.owner)).await?;
            if visible.message(&message.mailbox, Some(&message.sender))
                != mailbox::visibility::MessageVisibility::Readable
            {
                return Ok(not_yours(
                    &id,
                    &message.mailbox,
                    visible.above.contains(&message.mailbox),
                ));
            }
            peek = mine.name != message.mailbox;
            upward = !visible.reads_inside(&message.mailbox);
            // **A read of one's own box renews the claim**, as `read_mailbox`
            // does: a session that only reads its mail writes nothing.
            if !peek {
                self.renew_role_claims_after_answer(args.sid.as_deref());
            }
        }
        // **The caller's key is checked before either path**, so a key that
        // opens nothing is refused the history of an archived message exactly as
        // it is refused a plain read of it.
        let run = match self.acting_run(args.sid.as_deref()) {
            Ok(run) => run,
            Err(refused) => return Ok(refused),
        };
        // **The history is read where the message is readable, and nowhere
        // else.** Everything above, privacy and ownership included, has already
        // been asked; a message nobody can read falls through to the delivery
        // verb, which owns the refusals for one that is unknown or set aside and
        // moves nothing when it gives them.
        if args.history.unwrap_or(false)
            && let Some(message) = &readable
        {
            return match self.mailboxes.message_history(&id).await {
                Ok(events) => json_result(&history_json(message, &events, upward)),
                Err(e) => mailbox_declined(e),
            };
        }
        // A manager reads the located bytes, never the delivering store verb:
        // even a new message stays new, with no delivery event or obligation.
        if peek && let Some(message) = &readable {
            let mut rendered = message_json(message);
            if upward {
                withhold_notes(&mut rendered);
            }
            rendered["seen_before"] = (message.state != mailbox::MessageState::New).into();
            rendered["taken"] = false.into();
            let viewer = self
                .caller(args.sid.as_deref())
                .ok()
                .flatten()
                .and_then(|caller| caller.card);
            self.mark_other_run(&mut rendered, message, viewer.as_ref())
                .await;
            return json_result(&rendered);
        }
        let acting = jojobot_domain::mailbox::Acting {
            session: run.as_deref(),
            at: self.clock().now(),
        };
        match self.mailboxes.read_message(&id, acting).await {
            Ok(delivered) => {
                let mut rendered = delivered_json(&delivered);
                let viewer = self
                    .caller(args.sid.as_deref())
                    .ok()
                    .flatten()
                    .and_then(|caller| caller.card);
                self.mark_other_run(&mut rendered, &delivered.message, viewer.as_ref())
                    .await;
                json_result(&rendered)
            }
            Err(e) => mailbox_declined(e),
        }
    }
}

/// **A message's changes of state on the wire**, oldest first.
///
/// The envelope is the message's identity and the state it holds now, never its
/// body: a history is a question about what happened to it. **An empty history
/// says what that means**, because a message changed before jojobot kept events
/// reads the same as one nobody has touched, and an empty list would pass for
/// *nothing happened*.
fn history_json(
    message: &mailbox::Message,
    events: &[mailbox::MessageEvent],
    upward: bool,
) -> serde_json::Value {
    let mut withheld = false;
    let rendered: Vec<serde_json::Value> = events
        .iter()
        .map(|event| {
            let mut one = serde_json::json!({
                "nth": event.ordinal,
                "kind": event.kind.as_token(),
                "at": event.at.to_string(),
                "prior_state": event.prior_state.as_token(),
            });
            if let Some(fields) = one.as_object_mut() {
                if let Some(run) = &event.by_session {
                    fields.insert("run".into(), run.as_str().into());
                }
                if let Some(by_box) = &event.by_box {
                    fields.insert("box".into(), by_box.as_str().into());
                }
                if upward {
                    withheld |= event.notes.is_some() || event.reason.is_some();
                } else {
                    if let Some(notes) = &event.notes {
                        fields.insert("notes".into(), notes.as_str().into());
                    }
                    if let Some(reason) = &event.reason {
                        fields.insert("reason".into(), reason.as_str().into());
                    }
                }
            }
            one
        })
        .collect();
    let mut history = serde_json::json!({
        "count": events.len(),
        "events": rendered,
    });
    if withheld && let Some(fields) = history.as_object_mut() {
        fields.insert("notes_withheld".into(), true.into());
    }
    if let Some(fields) = history.as_object_mut()
        && events.is_empty()
    {
        fields.insert(
            "note".into(),
            "no change of state is recorded for this message. It may have changed before \
             jojobot kept them, so this does not say nothing happened to it"
                .into(),
        );
    }
    serde_json::json!({
        "id": message.id.as_str(),
        "mailbox": message.mailbox.as_str(),
        "state": message.state.as_token(),
        "taken": false,
        "history": history,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// **A subject travels the whole surface.** It goes in on the post and comes
    /// back on the post, the delivery and the archive — a title only the poster
    /// ever sees is not a title.
    #[tokio::test]
    async fn a_subject_is_carried_by_every_verb_that_renders_a_message() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "inbox").await;
        let posted = send_titled(
            &jojobot,
            "inbox",
            "alpha",
            Some("the shipment"),
            "it landed at dawn; the crates are by the north door",
        )
        .await;
        assert_eq!(posted["subject"], "the shipment");
        assert_eq!(
            posted["body_head"], "it landed at dawn; the crates are by the north door",
            "the subject sits beside the body, never carved out of it"
        );
        let id = posted["id"].as_str().expect("an id").to_string();

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["messages"][0]["subject"], "the shipment");

        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    request_id: None,
                    message_id: id,
                    notes: None,
                    sid: Some(reader.clone()),
                    quarantine: None,
                    message_ids: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        // The receipt does not restate the title the caller just read; the
        // archive still holds it, and any read of the message returns it.
        assert!(processed.get("subject").is_none(), "{processed}");
        let archived = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: processed["id"].as_str().expect("an id").to_string(),
                    sid: Some(reader),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(
            archived["subject"], "the shipment",
            "the archive keeps the title"
        );
    }

    /// **One message, taken by id.** The named message is delivered and the rest
    /// of the box is left where it was — the point of the verb: a session that
    /// wants one filed finding must not have to own everything beside it.
    #[tokio::test]
    async fn read_message_delivers_one_and_leaves_the_box_alone() {
        let jojobot = mailbox_handler();
        // The box's own drainer, because taking delivery is something only it
        // can do — see `a_message_in_somebody_elses_box_is_not_delivered_by_id`.
        let reader = owning(&jojobot, "inbox").await;
        let wanted = send(&jojobot, "inbox", "epsilon", "the one worth reading").await;
        send(&jojobot, "inbox", "sigma", "the rest of the box").await;
        let id = wanted["id"].as_str().expect("an id").to_string();

        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: id.clone(),
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(delivered["id"], id.as_str());
        assert_eq!(delivered["body"], "the one worth reading");
        assert_eq!(
            delivered["state"], "read",
            "taking one message moves its column"
        );
        assert_eq!(delivered["seen_before"], false);

        let counted = counts(&jojobot, "inbox").await;
        assert_eq!(counted["counts"]["read"], 1);
        assert_eq!(
            counted["counts"]["new"], 1,
            "the rest of the box was not delivered with it"
        );

        // Taken again: a leftover, not a second delivery.
        let again = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: id,
                    sid: Some(reader),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(again["seen_before"], true);
    }

    /// **The door was locked and the window was open.**
    ///
    /// `read_mailbox` takes no box name precisely so a caller cannot open
    /// somebody else's — reading IS delivery, and a message taken by the wrong
    /// bot is one its real consumer never sees as fresh again. `read_message`
    /// reached exactly the same mail through a bare id, from anyone holding it,
    /// and ids are a decimal counter: the one beside yours is somebody else's.
    /// The norm the read side made structural, this verb left as advice.
    #[tokio::test]
    async fn a_message_in_somebody_elses_box_is_not_delivered_by_id() {
        let jojobot = mailbox_handler();
        let gamma = owning(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        let theirs = send(&jojobot, "delta", "epsilon", "for delta's eyes").await;
        let id = theirs["id"].as_str().expect("an id").to_string();

        let refused = blocked(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: id.clone(),
                    sid: Some(gamma),
                }))
                .await
                .expect("a refusal is a successful call"),
        );
        assert_eq!(refused["attempted"], id.as_str());
        // **A way forward, not a wall.** There IS a legitimate move on somebody
        // else's box and the refusal has to name it, or a session that needs
        // something from that box invents a worse way to get it.
        let how = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("post_message"),
            "the sanctioned way to reach another box is to write into it: {how}"
        );

        // **And the message is untouched**, which is the whole point: it is
        // still fresh mail for the bot it was sent to.
        let counted = counts(&jojobot, "delta").await;
        assert_eq!(
            counted["counts"]["new"], 1,
            "delta's mail is still waiting for delta: {counted}"
        );
        assert_eq!(counted["counts"]["read"], 0, "{counted}");
    }

    /// **An anonymous caller takes delivery of nothing**, for the same reason
    /// `read_mailbox` refuses one: delivery moves somebody's mail, and jojobot
    /// will not move it on behalf of nobody. The advice is the door's, because
    /// what this caller lacks is an identity rather than permission.
    #[tokio::test]
    async fn an_anonymous_caller_takes_delivery_of_nothing_by_id() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        let theirs = send(&jojobot, "gamma", "epsilon", "for gamma").await;
        let id = theirs["id"].as_str().expect("an id").to_string();

        let refused = blocked(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: id,
                    sid: None,
                }))
                .await
                .expect("a refusal is a successful call"),
        );
        let how = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("start_here"),
            "an anonymous caller is sent to the door that gives it an identity: {how}"
        );
        let counted = counts(&jojobot, "gamma").await;
        assert_eq!(counted["counts"]["new"], 1, "nothing moved: {counted}");
    }

    /// An owner still reads its processed archive without moving it.
    #[tokio::test]
    async fn a_processed_message_is_history_and_reading_one_takes_nothing() {
        let jojobot = mailbox_handler();
        let delta = owning(&jojobot, "delta").await;
        let theirs = send(&jojobot, "delta", "epsilon", "a finding worth filing").await;
        let id = theirs["id"].as_str().expect("an id").to_string();
        jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                request_id: None,
                message_id: id.clone(),
                notes: Some("acted on".into()),
                sid: Some(delta.clone()),
                quarantine: None,
                message_ids: None,
            }))
            .await
            .expect("mark_processed ok");

        let read = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: id.clone(),
                    sid: Some(delta),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_ne!(read["status"], "blocked", "the archive is readable: {read}");
        assert_eq!(read["body"], "a finding worth filing");
        assert_eq!(
            read["state"], "processed",
            "…and reading it did not move it: {read}"
        );
    }

    /// An invalid session key cannot read an owner's archived history.
    /// The owner's valid key reads the same history.
    #[tokio::test]
    async fn the_history_of_an_archived_message_is_refused_to_a_key_that_opens_nothing() {
        let jojobot = mailbox_handler();
        let delta = owning(&jojobot, "delta").await;
        let theirs = send(&jojobot, "delta", "epsilon", "a finding worth filing").await;
        let id = theirs["id"].as_str().expect("an id").to_string();
        jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                request_id: None,
                message_id: id.clone(),
                notes: Some("acted on".into()),
                sid: Some(delta.clone()),
                quarantine: None,
                message_ids: None,
            }))
            .await
            .expect("mark_processed ok");
        let ask = |sid: &str, history: bool| {
            let jojobot = &jojobot;
            let id = id.clone();
            let sid = sid.to_string();
            async move {
                json_of(
                    &jojobot
                        .read_message(Parameters(ReadMessageArgs {
                            history: Some(history),
                            message_id: id,
                            sid: Some(sid),
                        }))
                        .await
                        .expect("read_message answers"),
                )
            }
        };

        let opened = ask(&delta, true).await;
        assert_eq!(
            opened["history"]["count"], 1,
            "a key that opens a session reads the history: {opened}"
        );
        let plain = ask("not-a-key-at-all", false).await;
        assert_eq!(
            plain["status"], "blocked",
            "the plain read refuses a key that opens nothing: {plain}"
        );
        let refused = ask("not-a-key-at-all", true).await;
        assert_eq!(
            refused["status"], "blocked",
            "the history of the same message is refused to the same key: {refused}"
        );
        assert!(refused.get("history").is_none(), "{refused}");
    }

    /// **An id that names nothing is blocked, not an error** — the same answer
    /// `mark_processed` gives, so one client branch handles both.
    #[tokio::test]
    async fn reading_an_unknown_message_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;

        let result = jojobot
            .read_message(Parameters(ReadMessageArgs {
                history: None,
                message_id: "999999".into(),
                sid: None,
            }))
            .await
            .expect("a blocked read is a successful call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "999999");
        assert!(
            body["candidates"]
                .as_array()
                .expect("candidates key")
                .is_empty(),
            "nothing resembles a message id: {body}"
        );
    }

    /// A quarantined card addressed by `read_message` gets the quarantine's own
    /// words, not "no such message" — the distinction `mark_processed` draws,
    /// drawn by every verb that addresses a card by id.
    #[tokio::test]
    async fn reading_a_quarantined_card_is_blocked_without_the_stores_words() {
        let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(store.clone());
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();
        store.quarantine_by_damage(
            &MailboxName("inbox".into()),
            &MessageId(id.clone()),
            "its row on the page cannot be read — a state or a sender has been edited past parsing",
        );

        let result = jojobot
            .read_message(Parameters(ReadMessageArgs {
                history: None,
                message_id: id.clone(),
                sid: None,
            }))
            .await
            .expect("a quarantined card is a successful, refusing call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], id.as_str());
        let reason = body["reason"]
            .as_str()
            .expect("a quarantined card says why");
        // The store's own account does NOT come through: it names which
        // field of which row failed to parse, the detail an operator
        // repairing it needs and an agent must never be handed. It is
        // logged instead.
        assert!(
            !reason.contains("edited past parsing"),
            "the adapter's own words must not reach a caller: {reason}"
        );
        assert!(
            reason.contains("only a person can put it back"),
            "…and what the caller gets instead has to be an answer: {reason}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("retrying will not help") && advice.contains("operator"),
            "retrying does not help and a person must repair it: {advice}"
        );
        // The advice must never teach the store's own shape or give repair
        // steps in its vocabulary — not an agent's business, and not
        // accurate either.
        for retired in ["card", "board", "column", "label"] {
            assert!(
                !advice.to_lowercase().contains(retired),
                "the advice teaches the retired store ({retired:?}): {advice}"
            );
        }
    }

    /// **A message in a person's box is read by no bot, in any state.** The
    /// bot that wrote it, a different bot and a caller with no handle all meet
    /// the same refusal, whether the message is waiting or already processed:
    /// `processed` follows the chart rule too, and this box is not another
    /// box. The refusal carries none of the message, moves nothing, and is
    /// paired with the post that landed, counted from the store.
    #[tokio::test]
    async fn a_message_in_a_persons_box_is_read_by_no_bot_in_any_state() {
        let jojobot = mailbox_handler();
        let held = a_persons_box(&jojobot, "milhouse").await;
        let writer = owning(&jojobot, "epsilon").await;
        let other = owning(&jojobot, "sigma").await;
        let posted = send_titled(
            &jojobot,
            "person:milhouse",
            "epsilon",
            Some("the quarterly figure"),
            "the secret figure is upsilon",
        )
        .await;
        let id = posted["id"].as_str().expect("an id").to_string();
        assert_eq!(
            store_counts(&jojobot, &held).await,
            (1, 0, 0),
            "the post landed, counted from the store"
        );

        let readers = vec![Some(writer), Some(other), None];
        for state in ["waiting", "processed"] {
            if state == "processed" {
                jojobot
                    .mailboxes
                    .mark_processed(
                        &mailbox::MessageId(id.clone()),
                        &held,
                        None,
                        jojobot_domain::mailbox::Acting::anyone_anytime(),
                    )
                    .await
                    .expect("the store retires it");
            }
            for reader in &readers {
                let refused = json_of(
                    &jojobot
                        .read_message(Parameters(ReadMessageArgs {
                            history: None,
                            message_id: id.clone(),
                            sid: reader.clone(),
                        }))
                        .await
                        .expect("a refusal is an answer"),
                );
                assert_eq!(refused["status"], "blocked", "{state}: {refused}");
                for leaked in ["upsilon", "quarterly", "epsilon"] {
                    assert!(
                        !refused.to_string().contains(leaked),
                        "{state}: the refusal carries {leaked}: {refused}"
                    );
                }
            }
        }
        assert_eq!(
            store_counts(&jojobot, &held).await,
            (0, 0, 1),
            "no refusal moved anything: the message is where the retirement left it"
        );
    }

    /// **A message in a person's box that jojobot cannot read is still the
    /// person's.** A damaged or set-aside card answers no `message_by_id`, so the
    /// guards that locate the message by it never see which box it sits in, and
    /// the refusals that follow describe the damage: a set-aside message even
    /// names who set it aside and why. Neither says anything of a person's box.
    /// The unreadable report in `list_sent` names every box that holds one, so
    /// the person's box, and its ids, are left out of it. A damaged card in a
    /// bot's box beside it is still refused as damage and still reported, so the
    /// silence is the person's box's and not a blanket one.
    #[tokio::test]
    async fn an_unreadable_message_in_a_persons_box_is_the_persons_and_not_reported() {
        use crate::mailboxes::testing::*;
        let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(store.clone());
        let held = a_persons_box(&jojobot, "milhouse").await;
        let writer = owning(&jojobot, "epsilon").await;
        let other = owning(&jojobot, "sigma").await;
        let private = send(
            &jojobot,
            "person:milhouse",
            "epsilon",
            "the secret figure is upsilon",
        )
        .await;
        let ordinary = send(&jojobot, "sigma", "epsilon", "an ordinary note").await;
        let private_id = private["id"].as_str().expect("an id").to_string();
        let ordinary_id = ordinary["id"].as_str().expect("an id").to_string();
        store.quarantine_by_damage(
            &held,
            &mailbox::MessageId(private_id.clone()),
            "torn by hand",
        );
        store.quarantine_by_damage(
            &mailbox::MailboxName("sigma".into()),
            &mailbox::MessageId(ordinary_id.clone()),
            "torn by hand",
        );

        // ── the person's: the same refusal as any message in that box ───────
        for reader in [&writer, &other] {
            let read = json_of(
                &jojobot
                    .read_message(Parameters(ReadMessageArgs {
                        history: None,
                        message_id: private_id.clone(),
                        sid: Some(reader.clone()),
                    }))
                    .await
                    .expect("a refusal is an answer"),
            );
            let retire = json_of(
                &jojobot
                    .mark_processed(Parameters(MarkProcessedArgs {
                        request_id: None,
                        message_id: private_id.clone(),
                        notes: Some("handled".into()),
                        sid: Some(reader.clone()),
                        quarantine: None,
                        message_ids: None,
                    }))
                    .await
                    .expect("a refusal is an answer"),
            );
            for refused in [&read, &retire] {
                assert_eq!(refused["status"], "blocked", "{refused}");
                assert!(
                    refused.get("reason").is_none(),
                    "no word of what is wrong with a card in a person's box: {refused}"
                );
            }
        }
        // ── the listing does not name the person's box or its ids ───────────
        let listed = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    offset: None,
                    sender: None,
                    to: None,
                    include_bodies: None,
                    sid: Some(other),
                }))
                .await
                .expect("list_sent ok"),
        );
        assert!(
            !listed.to_string().contains("person-milhouse"),
            "the person's box is not named: {listed}"
        );
        // ── the positive: the bot's own damaged card is still reported ──────
        let unreadable = listed["unreadable"].as_array().expect("a report");
        assert_eq!(unreadable.len(), 1, "{listed}");
        assert_eq!(unreadable[0]["mailbox"], "sigma", "{listed}");
        assert_eq!(unreadable[0]["ids"][0], ordinary_id.as_str(), "{listed}");
    }

    /// **A person renamed while a message is read does not open their box.**
    /// The verb locates the message, then reads the board to learn whose box it
    /// sits in. A rename of the owner between the two leaves the lookup holding
    /// the old name of the box and the board only the new one, so a guard that
    /// asks "is that name a person's" gets no for being unmatched. A box the
    /// board does not list is judged by nothing, the call is refused with the
    /// word that says to send it again, and nothing moves. Sent again, the board
    /// lists the box under its new name and refuses it as a person's.
    #[tokio::test]
    async fn a_person_renamed_while_a_message_is_read_does_not_open_their_box() {
        let (jojobot, store) = renaming_handler();
        let held = a_persons_box(&jojobot, "milhouse").await;
        owning(&jojobot, "epsilon").await;
        let reader = owning(&jojobot, "sigma").await;
        let posted = send_titled(
            &jojobot,
            "person:milhouse",
            "epsilon",
            Some("the quarterly figure"),
            "the secret figure is upsilon",
        )
        .await;
        let id = posted["id"].as_str().expect("an id").to_string();
        // Privacy is checked before the chart, including in the archive.
        jojobot
            .mailboxes
            .mark_processed(
                &mailbox::MessageId(id.clone()),
                &held,
                None,
                jojobot_domain::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("the store retires it");
        let read = |sid: String| {
            let jojobot = &jojobot;
            let id = id.clone();
            async move {
                json_of(
                    &jojobot
                        .read_message(Parameters(ReadMessageArgs {
                            history: None,
                            message_id: id,
                            sid: Some(sid),
                        }))
                        .await
                        .expect("a refusal is an answer"),
                )
            }
        };

        store.arm("person:milhouse", "person:lisa");
        let midway = read(reader.clone()).await;
        assert_eq!(midway["status"], "blocked", "{midway}");
        assert_eq!(midway["fix_by"], "retry", "{midway}");
        assert!(!midway.to_string().contains("upsilon"), "{midway}");
        assert_eq!(
            store_counts(&jojobot, &mailbox::MailboxName("person-lisa".into())).await,
            (0, 0, 1),
            "the box now wears its new name and its retired message is where it was"
        );

        let again = read(reader).await;
        assert_eq!(again["status"], "blocked", "{again}");
        assert_eq!(again["fix_by"], "change", "{again}");
        assert!(!again.to_string().contains("upsilon"), "{again}");
    }
}

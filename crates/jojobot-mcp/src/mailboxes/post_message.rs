//! `post_message` — Leave a message in a box — the one verb that reaches another.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `post_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PostMessageArgs {
    /// **The bot to write to** — a colleague, not a container: a bare name
    /// like `gamma`, or its full handle. You address the identity and jojobot
    /// finds its mail; a bot has exactly one box, and what that box is called
    /// is not something you should have to know.
    ///
    /// **It must already exist** — a name no bot answers to comes back with
    /// candidates and nothing is written.
    pub(crate) to: String,
    /// The message itself. Prose: paragraphs are fine.
    pub(crate) body: String,
    /// **Your session id.** Required here, because it is what jojobot records
    /// as the sender: a message from nobody is a message nobody can reply to,
    /// and identity that is merely declared is identity that can be wrong.
    pub(crate) sid: String,
    /// What this message is about, in one line — a title, not a summary.
    /// Optional, and worth giving: it is what a reader sees in a listing and on
    /// a search hit before they open anything. Do NOT also repeat it as the
    /// body's first line.
    ///
    /// **Validated, not styled: one plain line of unformatted text.** It is
    /// shown as a title rather than rendered, so a line break, a backtick or
    /// any other control character is refused and nothing is written — name a
    /// tool or a field in plain words, even though every other prose surface
    /// here takes markdown. A title over 120 characters is refused rather than
    /// cut, because shortening your own title is yours to do.
    #[serde(default)]
    pub(crate) subject: Option<String>,

    /// The id of the message this one answers, when it answers one. Optional.
    /// It must name a message that exists — a miss comes back blocked and
    /// nothing is written — and it links the two without saying anything about
    /// either: it does not deliver, handle, or oblige.
    #[serde(default)]
    pub(crate) in_reply_to: Option<String>,
}

/// **A bare name is a bot.** `gamma` and `bot:gamma` address the same colleague, and
/// a caller writing to one should not have to spell the kind — this surface has
/// exactly one kind of correspondent.
pub(crate) fn bot_handle(named: &str) -> EntityId {
    let named = named.trim();
    match named.contains(':') {
        true => EntityId(named.to_string()),
        false => EntityId(format!("bot:{named}")),
    }
}

/// Leave a message in a box.
impl Jojobot {
    /// **Nothing was written: that addressee has nowhere to receive.**
    ///
    /// Three different repairs wearing one refusal, and the answer says which:
    /// a name no bot answers to (the ordinary caller mistake, answered with the
    /// bots that do exist), a bot whose box is missing, and a bot holding more
    /// than one — the last two being damage rather than anything a caller did.
    pub(crate) async fn no_such_addressee(
        &self,
        addressee: &EntityId,
        found: OwnBox,
    ) -> CallToolResult {
        let roster = self.bot_roster().await;
        // **The near-miss screen, over the bot directory.** A typo must not
        // send a report somewhere nobody reads, and the candidates are names
        // the caller already knows: the same screen a creation is held to,
        // asked the other way round.
        let nearby = match self.memory.list_entities(Some(EntityKind::BOT)).await {
            Ok(bots) => guard::screen(addressee, &[addressee.slug()], &bots),
            Err(_) => Vec::new(),
        };
        let how_to_proceed = match found {
            OwnBox::Several(boxes) => format!(
                "Nothing was written. '{addressee}' owns more than one mailbox ({}), and one bot \
                 has exactly one. This is damage rather than anything you did: jojobot will not \
                 guess which half of somebody's mail to deliver. Report it — it needs a person.",
                boxes
                    .iter()
                    .map(|b| b.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            OwnBox::None => format!(
                "Nothing was written. '{addressee}' has no mailbox. A box opens with the bot that \
                 owns it, so this is a creation that did not finish rather than a step somebody \
                 skipped — booting that bot with start_here opens it. If you meant somebody \
                 else: {roster}.",
            ),
            OwnBox::Unreadable => "Nothing was written. jojobot could not read the mail board, so \
                 it cannot say where this belongs. Nothing is wrong with your call — try it again."
                .to_string(),
            OwnBox::The(_) => unreachable!("a resolved box is not a refusal"),
        };
        // The blocked shape every other refusal on this surface wears, with
        // the addressee as what was attempted: a caller branches on `status`,
        // never on which gate fired.
        blocked_body(addressee, &nearby, how_to_proceed)
    }

    /// Every bot on the board, for a refusal that offers somewhere to go.
    /// Names only: a caller that named the wrong one is choosing, not weighing.
    async fn bot_roster(&self) -> String {
        let Ok(boxes) = self.mailboxes.list_mailboxes().await else {
            return "jojobot cannot read the board to say who is there".to_string();
        };
        let mut names: Vec<String> = boxes
            .iter()
            .map(|held| held.owner.to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        names.sort();
        match names.is_empty() {
            true => "no bot has a mailbox yet".to_string(),
            false => names.join(", "),
        }
    }
    /// **Your own waiting mail, taken as part of posting.**
    ///
    /// A real delivery: everything unprocessed moves out of `new` and becomes
    /// yours to finish, exactly as opening the box would. It is recorded as
    /// taken by POSTING rather than by reading, so the sender's own view stays
    /// honest — `new` is the only pickup signal a sender has, and this is the
    /// one way mail leaves it without anybody looking.
    ///
    /// `None` when there is nothing to hand over or nowhere to read from. A
    /// post that could not also deliver is still a post that succeeded: the
    /// message was filed, which is what the caller asked for.
    async fn delivered_with_the_post(
        &self,
        bot: &EntityId,
        posted_into: &mailbox::MailboxName,
    ) -> Option<serde_json::Value> {
        let OwnBox::The(own) = self.own_box(bot).await else {
            return None;
        };
        // **A note you left yourself is not mail you have received.** Posting
        // into your own box and taking delivery in the same call would hand
        // you back the message you had just written, marked as delivered — the
        // call undoing itself, and the one message on the board a caller
        // demonstrably already has.
        if &own == posted_into {
            return None;
        }
        let taken = self
            .mailboxes
            .read_mailbox(&own, mailbox::TakenBy::Posting)
            .await
            .ok()?;
        let mailbox::Guarded::Written(delivery) = taken else {
            return None;
        };
        if delivery.messages.is_empty() {
            return None;
        }
        // The same rendering `read_mailbox` uses, so mail taken this way reads
        // identically to mail somebody went and got. `new_only` is the same
        // default too: a leftover is still owed, and its body was shipped once.
        Some(delivery_json(&delivery, true))
    }
}

#[tool_router(router = post_message_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Leave a message for someone who is not in this conversation. THE GATE IS \
                       THE ADDRESSEE'S MAILBOX, not the name alone: `to` must come down to exactly \
                       one box jojobot can read, so a name no colleague answers to and a colleague \
                       whose mail is missing or doubled both come back status: blocked with \
                       nothing written. The answer says which — the first is a name to fix, \
                       offered the bots that do exist; the second is damage on a bot that really \
                       is there, and it says what repairs it. There is no verb that opens a box: a \
                       box is some bot's own and arrives with it, so a name nobody answers to is a \
                       name nobody drains. Returns the stored message, including the id that \
                       read_message and mark_processed later target. Give it a `subject`: one line \
                       saying what the message is about, which is what a reader sees on the \
                       listing and on a search hit before opening anything — put it there rather \
                       than on the body's first line. The `state` you get back is the state as it \
                       stands — it can already say `read` if a person picked the message up in \
                       between, and that is success, not a problem: the message exists and someone \
                       has it. POSTING ALSO DELIVERS: anything waiting in YOUR OWN box rides back \
                       with this answer under your_mail — out of `new` and yours to finish, \
                       exactly as read_mailbox would have handed it over — so read it rather than \
                       treating this as a write. Posting into your own box delivers nothing, and a \
                       post that had nothing to hand over still succeeded. The sender is not yours \
                       to declare: jojobot records the bot behind the `sid` you pass, so a reply \
                       can always find you and nothing can be posted under somebody else's name. A \
                       `sid` jojobot is not holding comes back status: blocked and nothing is \
                       written. YOUR BODY IS NOT ECHOED BACK — you wrote it, so the answer carries \
                       the id, the state and body_bytes with body_elided: true rather than the \
                       text. `list_sent` with include_bodies returns it and takes no delivery. \
                       `in_reply_to` links this message to the one it answers: optional, it must \
                       name a message that exists (a miss comes back blocked, nothing written), \
                       and it says only that the two are one exchange — it does not deliver the \
                       original, handle it, or oblige anybody."
    )]
    pub(crate) async fn post_message(
        &self,
        Parameters(args): Parameters<PostMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        // **The sender is derived, never declared.** A free-text field recorded
        // exactly as claimed makes every "who left this?" answer only as good
        // as the caller's honesty and their memory of what they called
        // themselves last time. The handle says who is asking, so the handle
        // says who sent it.
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **Screened here so the refusal is an ANSWER.** A subject the record
        // cannot carry is a caller mistake, and every other caller mistake on
        // this surface comes back blocked with a way forward — this one reached
        // the store's validator and came back as a protocol error, which is a
        // failure rather than a next move (rule 68). The domain still refuses
        // it; what this decides is the shape the caller sees.
        if let Err(e) = mailbox::validate_subject(args.subject.as_deref()) {
            return Ok(subject_declined(
                args.subject.as_deref().unwrap_or_default(),
                &e,
            ));
        }
        // **The addressee is a bot, and its box is found by owner.** A box is
        // named for its handle by convention and nothing enforces that, so
        // reading ownership is what makes the address a fact rather than a
        // guess about what somebody called their box.
        let addressee = bot_handle(&args.to);
        // **A name that is no handle is refused as one**, before anything looks
        // for a box. Without this, `In Box!` reads as a colleague nobody has
        // heard of — which sends the caller looking for a missing bot when what
        // is wrong is the name they typed.
        if let Err(e) = jojobot_domain::memory::validate_subject(&addressee) {
            return Ok(misused(format!(
                "Nothing was written: {e}. Write to a bot by name — a bare name like 'gamma', or \
                 its full handle."
            )));
        }
        let destination = match self.own_box(&addressee).await {
            OwnBox::The(name) => name,
            elsewhere => return Ok(self.no_such_addressee(&addressee, elsewhere).await),
        };
        let new = NewMessage {
            mailbox: destination,
            body: args.body,
            subject: args.subject,
            sender: caller.bot.as_str().to_string(),
            // Stamped here, at the edge, for the same reason `capture` stamps a
            // date here: the domain stays clock-free, and a caller does not get
            // to backdate a message it is posting now.
            sent_at: self.clock().now(),
            in_reply_to: args
                .in_reply_to
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(|id| MessageId(id.to_string())),
        };
        // Declined rather than errored: a reply naming a message jojobot does
        // not hold is a bad reference, and every other bad reference on this
        // surface comes back as the blocked shape.
        let posted = match self.mailboxes.post_message(new).await {
            Ok(posted) => posted,
            Err(e) => return mailbox_declined(e),
        };
        match posted {
            mailbox::Guarded::Written(message) => {
                self.beat("post_message", message.mailbox.as_str(), Some(&args.sid))
                    .await;
                let mut body = message_receipt_json(
                    &message,
                    Some(
                        "you wrote this body, so it is not shipped back to you. list_sent with \
                         include_bodies: true returns it, and takes no delivery",
                    ),
                );
                // **Posting takes delivery of your own box, in the same call.**
                // The moment an agent posts is the moment a reply is most
                // likely waiting, because posting is what it does at the end of
                // a piece of work — and two agents each holding an unread reply
                // are two agents talking past each other, with nothing blocked
                // and nothing to notice.
                let mut collected = 0;
                if let Some(delivered) = self
                    .delivered_with_the_post(&caller.bot, &message.mailbox)
                    .await
                {
                    collected = delivered["count"].as_u64().unwrap_or_default();
                    if let Some(object) = body.as_object_mut() {
                        object.insert("your_mail".into(), delivered);
                    }
                }
                crate::answer::note_postcondition(
                    &mut body,
                    what_a_post_left_standing(&message, collected),
                );
                json_result(&body)
            }
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(mailbox_blocked(
                &attempted,
                &candidates,
                BlockedBox::MustExist("post_message"),
            )),
            mailbox::Guarded::UnknownOwner {
                attempted,
                candidates,
            } => Ok(unknown_owner(&attempted, &candidates)),
        }
    }
}

/// **What a caller's own post did, on both boxes it touched.**
///
/// The message is in the addressee's box and nothing else in that box moved.
///
/// ⚠️ **The second half is the one nothing else says.** Posting takes delivery
/// of the sender's own box in the same call, so a caller that wrote one message
/// can leave holding several it now owes work on. That obligation was arriving
/// with the mail and being announced by nothing.
///
/// **Named only when something came back**, so a post that collected nothing
/// does not claim work that does not exist.
fn what_a_post_left_standing(message: &Message, collected: u64) -> String {
    let took = match collected {
        0 => String::new(),
        n => format!(
            " This call also took delivery of {n} of your own: they are out of new and yours to \
             finish with mark_processed."
        ),
    };
    format!(
        "The message is in {}'s box, waiting. Nothing else in that box moved, and nobody is \
         obliged to read it before they next open the box.{took}",
        message.mailbox.as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// **Posting says where the message got to, and what the same call did to
    /// the caller's own box.**
    ///
    /// ⚠️ **Posting is not a pure write.** It takes delivery of whatever was
    /// waiting for the sender, in the same call — out of `new` and theirs to
    /// finish. That is a real obligation acquired as a side effect of writing,
    /// and the answer carried the mail without saying it had become owed work.
    ///
    /// **The count is what makes the line worth computing.** A post that
    /// collected nothing must not say it collected something, and a constant
    /// sentence says the same thing to both callers — which on the empty one is
    /// a claim about work that does not exist.
    #[tokio::test]
    async fn posting_says_where_it_landed_and_what_it_collected() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;

        // Nothing is waiting for the sender, so the line must not say anything
        // came back.
        let quiet = send(&jojobot, "otto", "epsilon", "the urn is descaled").await;
        let empty_handed = quiet["postcondition"]
            .as_str()
            .unwrap_or_else(|| panic!("a write states what now stands: {quiet}"))
            .to_string();
        assert!(
            empty_handed.contains("otto"),
            "the line has to say which box it landed in: {quiet}",
        );

        // Now there IS something waiting, and the same call takes it.
        send(
            &jojobot,
            "epsilon",
            "otto",
            "before you do, check the filter",
        )
        .await;
        let collected = send(&jojobot, "otto", "epsilon", "and the filter is clear").await;
        assert!(
            collected["your_mail"]["count"].as_u64() == Some(1),
            "the call took delivery, which is the thing the line is about: {collected}",
        );
        let line = collected["postcondition"]
            .as_str()
            .expect("a write states what now stands")
            .to_string();
        assert_ne!(
            line, empty_handed,
            "one sentence for both, so the empty post claims work that does not exist: \
             {collected}",
        );
        assert!(
            line.contains('1'),
            "the line has to say how much became the caller's to finish: {collected}",
        );
    }

    /// **The surface tells a caller what it has, never how jojobot satisfied
    /// itself.** How the server checks its own write is jojobot's business, and
    /// a caller told about the guard reasons about the guard. The word also
    /// carried two claims under one meaning: the bytes reached storage, which
    /// is what it checked, and the message is true, which it never checked.
    ///
    /// The useful half stands on its own — the body is not echoed back, and
    /// `list_sent` is the verb that returns it — so both halves of the pair
    /// below read the same answer: an answer carrying nothing at all would
    /// satisfy the negative by itself.
    #[tokio::test]
    async fn the_post_receipt_does_not_explain_how_the_server_checks_its_own_write() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;

        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "the kiln reached temperature".into(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        let how_to_read = posted["how_to_read"].as_str().expect("a pointer");
        assert!(
            posted["id"].as_str().is_some(),
            "the receipt still identifies the message: {posted}"
        );
        assert_eq!(posted["state"], "new");
        assert_eq!(posted["body_elided"], true);
        assert!(
            how_to_read.contains("list_sent"),
            "the caller still gets the verb that hands the body over: {how_to_read}"
        );
        assert!(
            !how_to_read.to_lowercase().contains("verif"),
            "the receipt describes the server's own check: {how_to_read}"
        );
    }

    /// The same line, on the description a client reads before it calls
    /// anything. It is the served text rather than the source literal, so a
    /// description that never reaches the tool list cannot satisfy it.
    #[test]
    fn the_post_description_does_not_explain_how_the_server_checks_its_own_write() {
        let tools = Jojobot::tool_router().list_all();
        let served = tools
            .iter()
            .find(|tool| tool.name == "post_message")
            .expect("post_message is served");
        let description = served.description.as_deref().expect("a description");
        assert!(
            description.contains("list_sent"),
            "the caller is still told where the body is: {description}"
        );
        assert!(
            !description.to_lowercase().contains("verif"),
            "the description explains the server's own check: {description}"
        );
    }

    /// **Blocked is a result, not a protocol error** — the same shape the Memory
    /// verbs use, so one client-side branch handles both contexts.
    ///
    /// And what is screened is the ADDRESSEE, because that is what a caller now
    /// names. A typo in a colleague's name must not send a report somewhere
    /// nobody reads, and the candidates come from the bot directory the caller
    /// already knows.
    #[tokio::test]
    async fn writing_to_a_bot_that_is_not_there_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "epsilon").await;

        let result = jojobot
            .post_message(Parameters(PostMessageArgs {
                to: "epsilo".into(),
                sid: as_bot(&jojobot, "epsilon"),
                body: "the shipment landed".into(),
                subject: None,
                in_reply_to: None,
            }))
            .await
            .expect("a blocked post is a successful call");
        let body = blocked(&result);
        assert_eq!(
            body["attempted"], "bot:epsilo",
            "a bare name is a bot, and the refusal says which handle it tried: {body}"
        );
        assert_eq!(
            body["candidates"][0]["handle"], "bot:epsilon",
            "the colleague they meant is offered: {body}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            !advice.contains("create_mailbox"),
            "never a verb that does not exist: {advice}"
        );
    }

    /// Malformed input is a client error that says what the grammar is, rather
    /// than a store failure or a silently-normalized name.
    #[tokio::test]
    async fn malformed_mailbox_input_is_a_client_error() {
        let jojobot = mailbox_handler();
        // **A post with no handle is a blocked ANSWER, not a malformed call.**
        // The caller's grammar is fine; what is missing is who they are, and
        // absence on this surface is always an answer with a way forward.
        make_box(&jojobot, "inbox").await;
        let body = blocked(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "inbox".into(),
                    sid: "  ".into(),
                    body: "the shipment landed".into(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("a message with no sender is an answer, not a protocol failure"),
        );
        assert_eq!(
            body["wrote"], false,
            "nothing is recorded from nobody: {body}"
        );
    }

    /// **The two verbs that echo a body back echo it to the one caller who
    /// already has it.** `post_message` returned the whole stored body to its
    /// author; `mark_processed` returned the entire original message to the
    /// consumer who had just read it. On 4–8 KB reports that doubled the cost
    /// of the behaviour the crash contract asks for, which is a price that
    /// scales with thoroughness — the wrong thing to charge for.
    ///
    /// What the full echo proved is preserved: the store's read-back invariant
    /// means a body that did not survive storage is an ERROR, not a success
    /// with mangled bytes, so fidelity is proven server-side. The receipt keeps
    /// what a caller cannot derive — the id, the state, the notes, the exact
    /// stored size — and says plainly that the body was left out.
    #[tokio::test]
    async fn a_post_is_receipted_without_shipping_the_body_back() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        let long = "counted the crates and reconciled them against the manifest. ".repeat(60);

        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: long.clone(),
                    subject: Some("the crate count".into()),
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        // Everything a caller cannot derive is still here.
        assert!(posted["id"].as_str().is_some());
        assert_eq!(posted["mailbox"], "pm");
        assert_eq!(posted["state"], "new");
        assert_eq!(posted["subject"], "the crate count");
        assert!(posted["sent_at"].is_string());
        // …and the body is not, loudly.
        assert!(posted["body"].is_null());
        assert_eq!(posted["body_elided"], true);
        assert_eq!(posted["body_bytes"], long.trim().len());
        assert!(
            posted["body_head"]
                .as_str()
                .expect("a head")
                .starts_with("counted the crates")
        );
        assert!(
            posted["body_head"]
                .as_str()
                .expect("a head")
                .chars()
                .count()
                < long.chars().count() / 4,
            "the head is a head, not the body under another key"
        );
        assert!(
            posted["how_to_read"]
                .as_str()
                .expect("a pointer")
                .contains("list_sent")
        );
    }

    /// **A reply names what it answers, and a dangling link is blocked.** The
    /// hand-off ↔ report chain was correlated by prose convention alone, which
    /// is manual archaeology the moment there is any volume. The link is
    /// optional, carries no semantics beyond itself, and — like every other
    /// reference on this surface — must name something that exists.
    #[tokio::test]
    async fn a_reply_carries_the_message_it_answers_and_a_dangling_link_is_blocked() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        let original = send(&jojobot, "pm", "delta", "build the kiln slice").await;
        let original_id = original["id"].as_str().expect("an id").to_string();
        assert!(
            original["in_reply_to"].is_null(),
            "a message answering nothing says so"
        );

        let reply = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "the kiln slice is done".into(),
                    subject: None,
                    in_reply_to: Some(original_id.clone()),
                }))
                .await
                .expect("post ok"),
        );
        assert_eq!(reply["in_reply_to"], original_id.as_str());

        // …and it rides on every verb that renders a message.
        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: reply["id"].as_str().expect("an id").to_string(),
                    // Read by the box's own drainer: taking delivery is the
                    // owner's move now, and this reply landed in pm's box.
                    sid: Some(as_bot(&jojobot, "pm")),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(delivered["in_reply_to"], original_id.as_str());

        // A link to nothing is the blocked shape, never a protocol error and
        // never a stored message.
        let dangling = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "answering something nobody said".into(),
                    subject: None,
                    in_reply_to: Some("9999".into()),
                }))
                .await
                .expect("a bad reference is an answer, not an error"),
        );
        assert_eq!(dangling["status"], "blocked", "{dangling}");
        assert_eq!(dangling["wrote"], false);

        // **A blank link is no link.** A client that sends `in_reply_to: ""`
        // meant to send nothing; refusing the whole post over an empty string
        // would be the second-worst way to answer, and the message reads back
        // as answering nothing — which is what it says.
        let unlinked = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "answering nothing in particular".into(),
                    subject: None,
                    in_reply_to: Some("   ".into()),
                }))
                .await
                .expect("a blank link is not a malformed call"),
        );
        assert_ne!(unlinked["status"], "blocked", "{unlinked}");
        assert!(
            unlinked["in_reply_to"].is_null(),
            "blank is absent, not empty: {unlinked}"
        );
    }
}

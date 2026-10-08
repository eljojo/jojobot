//! `post_message` — Leave a message in a box — the one verb that reaches another.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `post_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PostMessageArgs {
    /// **The bot to write to, or the operator** — a colleague, not a container:
    /// a bare name like `gamma`, or its full handle. You address the identity and jojobot
    /// finds its mail; a bot has exactly one box, and what that box is called
    /// is not something you should have to know.
    ///
    /// **It must already exist** — a name no bot answers to comes back with
    /// candidates and nothing is written.
    ///
    /// **The operator is the one person you may write to**, by their person
    /// handle (`person:` and the slug): the boot names it under `operator`.
    /// Nobody else has a box, and no bot reads the operator's box back.
    pub(crate) to: String,
    /// The message itself. Prose: paragraphs are fine.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as
    /// the handle that thing wears today, even after a rename.
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
    /// cut, because shortening your own title is yours to do. Carries
    /// `@kind:slug` mentions exactly as `body` does.
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
/// a caller writing to one should not have to spell the kind — a bot is the
/// ordinary correspondent. The one person who has a box, the operator, is
/// addressed by a full handle and never by a bare name.
pub(crate) fn bot_handle(named: &str) -> EntityId {
    let named = named.trim();
    match named.contains(':') {
        true => EntityId(named.to_string()),
        false => EntityId(format!("bot:{named}")),
    }
}

/// **If `addressee`'s own slug is how some bot is known** — its display name
/// or one of its aliases — that bot's handle, and whether the match came
/// through an alias specifically rather than the name itself.
///
/// The same comparison [`guard::screen`]'s own `SameName` channel makes
/// (fold, then slugify, each label), asked here to tell an alias apart from
/// a coincidental name: `screen` reports both the same way, because both are
/// equally a reason to hold a WRITE for confirmation, but only this
/// distinction tells a caller what to send to instead of what they typed.
fn named_by(addressee: &EntityId, bots: &[Entity]) -> Option<(EntityId, bool)> {
    let slug = addressee.slug();
    let matches = |label: &str| guard::slugify(&guard::normalize_name(label)) == slug;
    bots.iter().find_map(|bot| {
        if matches(&bot.name) {
            return Some((bot.id.clone(), false));
        }
        bot.aliases
            .iter()
            .any(|alias| matches(alias))
            .then(|| (bot.id.clone(), true))
    })
}

/// Leave a message in a box.
impl Jojobot {
    /// **Nothing was written: that addressee has nowhere to receive.**
    ///
    /// Three different repairs wearing one refusal, and the answer says which:
    /// a name no bot answers to (the ordinary caller mistake, answered with the
    /// bots that do exist), a bot whose box is missing, and a bot holding more
    /// than one — the last two being damage rather than anything a caller did.
    ///
    /// **`OwnBox::None` is itself two conditions, told apart here.** A name
    /// nobody answers to at all is the "creation that did not finish" case
    /// below. A name that IS answered to — as another bot's own name or one
    /// of its aliases — is not damage and start_here cannot repair it: the
    /// box already exists, under the handle that name belongs to. Aliasing a
    /// bot does not make the alias a second address; only the handle is one.
    pub(crate) async fn no_such_addressee(
        &self,
        addressee: &EntityId,
        found: OwnBox,
    ) -> CallToolResult {
        let roster = self.bot_roster().await;
        // **The near-miss screen, over the bot directory.** A typo must not
        // send a report somewhere nobody reads, and the candidates are names
        // the caller already knows: the same screen a creation is held to,
        // asked the other way round. Fetched once and reused below, so the
        // "is this actually somebody's name or alias" question asks the same
        // directory the screen just read rather than a second lookup.
        let bots = self
            .memory
            .list_entities(Some(EntityKind::BOT))
            .await
            .unwrap_or_default();
        let nearby = guard::screen(addressee, &[addressee.slug()], &bots);
        // **Which word the refusal wears is decided by what was found**: damage
        // is a person's, and a board that could not be read may read on a later try.
        let is_damage = matches!(found, OwnBox::Several(_));
        let may_read_later = matches!(found, OwnBox::Unreadable);
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
            // A person is not booted and has no creation to finish: the
            // operator's box opens at the first post to them.
            OwnBox::None if addressee.kind() == Some(EntityKind::PERSON) => format!(
                "Nothing was found. Only the operator has a mailbox among people, and \
                 '{addressee}' has none, so nothing has been sent. To reach the operator, write \
                 to the handle the boot names under operator: their box opens at the first \
                 post_message to it."
            ),
            OwnBox::None => match named_by(addressee, &bots) {
                Some((owner, via_alias)) => format!(
                    "Nothing was written. '{addressee}' is not a bot's own handle — it is {} of \
                     {owner}. An alias is a second name, never a second address: send to \
                     {owner} instead.",
                    if via_alias { "an alias" } else { "the name" },
                ),
                None => format!(
                    "Nothing was written. '{addressee}' has no mailbox. A box opens with the \
                     bot that owns it, so this is a creation that did not finish rather than a \
                     step somebody skipped — booting that bot with start_here opens it. If you \
                     meant somebody else: {roster}.",
                ),
            },
            OwnBox::Unreadable => "Nothing was written. jojobot could not read the mail board, so \
                 it cannot say where this belongs. Nothing is wrong with your call — try it again."
                .to_string(),
            OwnBox::The(_) => unreachable!("a resolved box is not a refusal"),
        };
        // The blocked shape every other refusal on this surface wears, with
        // the addressee as what was attempted: a caller branches on `status`,
        // never on which gate fired.
        let how_to_proceed = if is_damage {
            WayForward::person(how_to_proceed)
        } else if may_read_later {
            WayForward::mailbox_store_failure(how_to_proceed)
        } else {
            WayForward::from(how_to_proceed)
        };
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
        viewer: Option<&SessionId>,
    ) -> Option<(serde_json::Value, Delivery)> {
        let OwnBox::The(own) = self.own_box(bot).await else {
            return None;
        };
        // **A person's box is handed to nobody by a post.** A handle bound to a
        // person is not a bot's, and the box it owns is read by that person
        // alone. A board that cannot say whose the box is hands nothing over.
        if self.box_is_private(&own).await.unwrap_or(true) {
            return None;
        }
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
        // **Only what nobody has taken is listed.** A bot still acting on its
        // mail posts again and again, and a leftover is mail it was handed by
        // an earlier read: an envelope for each, on every post, is the same
        // thing shipped over and over. They stay owed, and `read_mailbox`
        // still returns them flagged — that is crash recovery. Here they are
        // named, and the answer names the call that returns them.
        let (delivery, leftovers) = split_leftovers(delivery, true);
        // The same rendering `read_mailbox` uses, so mail taken this way reads
        // identically to mail somebody went and got.
        let mut rendered = delivery_json(&delivery, true);
        self.mark_other_runs(&mut rendered, &delivery, viewer).await;
        note_leftovers(&mut rendered, &leftovers);
        Some((rendered, delivery))
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
                       is there, and it says what repairs it. No verb opens a bot's \
                       box: it arrives with the bot, so a name nobody answers to is a name nobody \
                       drains. THE ONE PERSON YOU MAY ADDRESS IS THE OPERATOR, by their person \
                       handle (`person:` and the slug), which the boot names under `operator`: your first post to them \
                       opens their box, nobody else has one, and no bot reads it back, you \
                       included. Returns the stored message, including the id that \
                       read_message and mark_processed later target. Give it a `subject`: one line \
                       saying what the message is about, which is what a reader sees on the \
                       listing and on a search hit before opening anything — put it there rather \
                       than on the body's first line. The `state` you get back is the state as it \
                       stands — it can already say `read` if a person picked the message up in \
                       between, and that is success, not a problem: the message exists and someone \
                       has it. POSTING ALSO DELIVERS: anything in YOUR OWN box that no read has \
                       handed you yet rides back with this answer under your_mail — out of `new` \
                       and yours to finish, exactly as read_mailbox would have handed it over — \
                       so read it rather than treating this as a write. Mail an earlier read \
                       already handed you is not listed again: it is counted under \
                       your_mail.leftovers, still owed, and read_mailbox returns it. Posting into your own box delivers nothing, and a \
                       post that had nothing to hand over still succeeded. THE MESSAGE ALSO CARRIES \
                       `sender_mail_waiting_at_send`: what was genuinely waiting in YOUR OWN box at \
                       the moment you sent it, the same count your_mail's delivery would have shown, \
                       read before this post could touch it. `null` means jojobot could not tell \
                       (no session, no box, an unreadable board) — NEVER read it as \"zero waiting\"; \
                       `0` is a real answer and a different one. It is stamped once, here, and \
                       carried unchanged on this same message wherever `read_mailbox` or `list_sent` \
                       returns it later, so a reader can check a claim about your own mail against \
                       what was actually true when you hit send. The sender is not yours \
                       to declare: jojobot records the bot behind the `sid` you pass, so a reply \
                       can always find you and nothing can be posted under somebody else's name. A \
                       `sid` jojobot is not holding comes back status: blocked and nothing is \
                       written. YOUR BODY IS NOT ECHOED BACK — you wrote it, so the answer carries \
                       the id, the state and body_bytes with body_elided: true rather than the \
                       text. `list_sent` with include_bodies returns it and takes no delivery. \
                       `in_reply_to` links this message to the one it answers: optional, it must \
                       name a message that exists (a miss comes back blocked, nothing written), \
                       and it says only that the two are one exchange — it does not deliver the \
                       original, handle it, or oblige anybody. Messages riding back under \
                       your_mail carry `written_by_other_run` when a different run posted them, \
                       exactly as read_mailbox's own description says."
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
        let caller = match self.identified_for_write(Some(&args.sid)).await {
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
        if let Some(refused) = self.refuses_a_non_person_operator(&addressee).await {
            return Ok(refused);
        }
        // **A person is addressed by their handle, and only the operator has a
        // box.** Posting to the operator is what opens it the first time.
        let destination = match addressee.kind() {
            Some(EntityKind::PERSON) => match self.operators_box(&addressee).await {
                Ok(name) => name,
                Err(refused) => return Ok(refused),
            },
            _ => match self.own_box(&addressee).await {
                OwnBox::The(name) => name,
                elsewhere => return Ok(self.no_such_addressee(&addressee, elsewhere).await),
            },
        };
        // **Read before the write, and before delivered_with_the_post drains
        // it.** This is the true count for the sender right now — the same
        // number the status bar would show them — captured so it can travel
        // with the message to a reader, rather than living only in the
        // sender's own answer where it never reaches anybody who did not send
        // this.
        let sender_mail_waiting_at_send = self.own_new_count(&caller.bot).await;
        // **Stamped here, alongside `sender_mail_waiting_at_send`.** A run's
        // record is made lazily by its first write, and a post can BE that
        // first write: stamping before the record existed left the message
        // with no run, so `written_by_other_run` stayed silent for exactly the
        // message a fresh run opened with. The record is made here, under the
        // same gate a beat takes, when the caller has none yet.
        //
        // A post the store then refuses (a reply naming nothing) has still
        // opened the run's record. The record carries no claim about the post:
        // it is the run, which exists because the caller is writing.
        let posted_by_session = match caller.card.as_ref() {
            Some(card) => Some(card.as_str().to_string()),
            None => {
                let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
                let serialized = gate.lock().await;
                // Re-read inside the gate: a racing write may have made the
                // record since, and a second one is the fork the gate prevents.
                match self.caller(Some(&args.sid)) {
                    Ok(Some(inside)) => self
                        .session_for(&serialized, &inside, None, None)
                        .await
                        .ok()
                        .map(|card| card.as_str().to_string()),
                    _ => None,
                }
            }
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
            sender_mail_waiting_at_send,
            posted_by_session,
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
                let delivered = self
                    .delivered_with_the_post(&caller.bot, &message.mailbox, caller.card.as_ref())
                    .await;
                // **The count of messages TAKEN, read before the delivery is
                // fitted**: fitting changes how many are shown, and the line
                // says how many became the caller's to finish.
                let collected = delivered.as_ref().map_or(0, |(rendered, _)| {
                    rendered["count"].as_u64().unwrap_or_default()
                });
                crate::answer::note_postcondition(
                    &mut body,
                    what_a_post_left_standing(&message, collected),
                );
                if let Some((mut delivered, delivery)) = delivered {
                    // **The delivery shares the ceiling with the receipt.**
                    // Fitted last, against everything else the answer holds,
                    // and a key and its comma for the delivery to ride under.
                    let beside = body.to_string().chars().count() + ",\"your_mail\":".len();
                    fit_delivery(&mut delivered, &delivery, beside);
                    if let Some(object) = body.as_object_mut() {
                        object.insert("your_mail".into(), delivered);
                    }
                }
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

    /// **The description names the key the leftovers are counted under**, so
    /// a caller that meets it has read where it comes from before it does.
    #[test]
    fn the_description_names_where_the_leftovers_are_counted() {
        let tools = Jojobot::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == "post_message")
            .expect("post_message is a tool");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("leftovers"), "{description}");
    }

    /// **A post hands back mail nobody has taken, and only counts the rest.**
    ///
    /// A bot still acting on its mail posts again and again, and every post
    /// used to re-ship an envelope for each message it had already read and
    /// not yet finished. The delivery that rides on a post lists only what no
    /// read has handed over; the leftovers are counted under `leftovers`, with
    /// the call that returns them.
    ///
    /// **Both halves in one case.** The two read messages are absent from the
    /// second post and a third, new one rides back whole, so a build that
    /// listed nothing at all fails the second half and a build that listed
    /// everything fails the first. `leftovers` is a key nothing outside this
    /// process declares, so the literal is pinned here.
    #[tokio::test]
    async fn a_post_lists_only_mail_nobody_has_taken_and_counts_the_leftovers() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;

        let first = send(&jojobot, "otto", "epsilon", "first thing waiting").await;
        let second = send(&jojobot, "otto", "epsilon", "second thing waiting").await;
        let (first, second) = (
            first["id"].as_str().expect("an id").to_string(),
            second["id"].as_str().expect("an id").to_string(),
        );
        // otto reads both and has not finished either.
        let read = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(as_bot(&jojobot, "otto")),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(read["count"], 2, "the two were delivered by a real read");

        let once = send(&jojobot, "epsilon", "otto", "first post").await;
        let again = send(&jojobot, "epsilon", "otto", "second post").await;
        for (which, post) in [("first", &once), ("second", &again)] {
            let mail = &post["your_mail"];
            assert_eq!(
                mail["leftovers"]["count"], 2,
                "the {which} post counts what it did not list: {post}"
            );
            assert_eq!(
                mail["count"], 0,
                "the {which} post listed nothing nobody had taken: {post}"
            );
            assert_eq!(
                mail["messages"].as_array().map(Vec::len),
                Some(0),
                "the {which} post listed an envelope: {post}"
            );
            // Ids are plain counters, so the needle is the id as an envelope
            // spells it and not the digit, which `count` would also carry.
            let shipped = mail.to_string();
            for id in [&first, &second] {
                assert!(
                    !shipped.contains(&format!("\"id\":\"{id}\"")),
                    "the {which} post shipped an envelope for {id}: {shipped}"
                );
            }
            assert!(
                mail["leftovers"]["how_to_read"]
                    .as_str()
                    .is_some_and(|how| how.contains("read_mailbox")),
                "the count names the call that returns them: {post}"
            );
        }

        // A third message arrives, and rides back in full.
        let third = send(&jojobot, "otto", "epsilon", "third thing waiting").await;
        let third = third["id"].as_str().expect("an id").to_string();
        let last = send(&jojobot, "epsilon", "otto", "third post").await;
        let mail = &last["your_mail"];
        assert_eq!(mail["count"], 1, "{last}");
        assert_eq!(mail["messages"][0]["id"], third.as_str(), "{last}");
        assert_eq!(
            mail["messages"][0]["body"], "third thing waiting",
            "a message nobody had taken comes back whole: {last}"
        );
        assert_eq!(mail["messages"][0]["seen_before"], false, "{last}");
        assert_eq!(
            mail["leftovers"]["count"], 2,
            "the two already read are still counted, not listed: {last}"
        );

        // The crash contract is untouched: the leftovers are still owed and the
        // recovery read still returns them, flagged.
        let drained = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(false),
                    sid: Some(as_bot(&jojobot, "otto")),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(drained["count"], 3, "{drained}");
        assert_eq!(drained["messages"][0]["seen_before"], true, "{drained}");
    }

    /// Twenty-five messages waiting for `otto`, each `body_len` characters, and
    /// the answer to a post that collects them.
    async fn a_post_collecting_a_box(body_len: usize) -> (Jojobot, serde_json::Value) {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;
        let body = "x".repeat(body_len);
        for _ in 0..25 {
            send(&jojobot, "otto", "epsilon", &body).await;
        }
        let posted = send(&jojobot, "epsilon", "otto", "reporting in").await;
        (jojobot, posted)
    }

    /// Every message in the delivery that rode on a post, carried or named, and
    /// how many were named.
    fn accounted_for(posted: &serde_json::Value) -> (Vec<String>, usize) {
        let mail = &posted["your_mail"];
        let named: Vec<String> = mail["not_shown"]["ids"]
            .as_array()
            .map(|ids| {
                ids.iter()
                    .map(|id| id.as_str().expect("an id").to_string())
                    .collect()
            })
            .unwrap_or_default();
        let mut all: Vec<String> = mail["messages"]
            .as_array()
            .expect("a list of messages")
            .iter()
            .map(|m| m["id"].as_str().expect("an id").to_string())
            .chain(named.iter().cloned())
            .collect();
        all.sort();
        (all, named.len())
    }

    /// **A post that collects a box past the ceiling stays under it and takes
    /// every message.** The delivery that rides on a post is fitted in the room
    /// the post's own receipt leaves, with the three tiers a read uses: whole,
    /// flagged with the body left out, or named by id. Every message is carried
    /// or named exactly once, all of them are taken, the line that says how
    /// many the post took counts the taken and not the shown, and each body that
    /// was left out is whole when asked for by id.
    #[tokio::test]
    async fn a_post_that_collects_a_hostile_box_fits_the_ceiling_and_takes_every_message() {
        let (jojobot, posted) = a_post_collecting_a_box(1_500).await;

        let size = posted.to_string().chars().count();
        assert!(
            size + crate::answer::STATUS_BAR_ROOM <= jojobot_domain::text::ANSWER_CEILING,
            "the post's whole answer is {size} characters"
        );
        let (accounted, named) = accounted_for(&posted);
        assert_eq!(accounted.len(), 25, "carried or named, once each: {posted}");
        let mut unique = accounted.clone();
        unique.dedup();
        assert_eq!(unique.len(), 25, "none twice: {posted}");
        let mail = &posted["your_mail"];
        let flagged: Vec<&str> = mail["messages"]
            .as_array()
            .expect("a list of messages")
            .iter()
            .filter(|m| m["body_elided"] == true)
            .filter_map(|m| m["id"].as_str())
            .collect();
        assert!(
            !flagged.is_empty() || named > 0,
            "twenty-five messages of this size cannot all come whole: {posted}"
        );
        assert_eq!(
            mail["not_shown"]["count"].as_u64().unwrap_or(0) as usize,
            named,
            "{posted}"
        );
        assert!(
            posted["postcondition"]
                .as_str()
                .is_some_and(|line| line.contains("25")),
            "the line counts the messages taken, not the ones shown: {posted}"
        );

        let counted = counts(&jojobot, "otto").await;
        assert_eq!(counted["counts"]["read"], 25, "all taken: {counted}");
        assert_eq!(counted["counts"]["new"], 0, "{counted}");

        let left_out: Vec<String> = flagged
            .iter()
            .map(|id| id.to_string())
            .chain(
                mail["not_shown"]["ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| id.as_str().map(str::to_string)),
            )
            .collect();
        for id in left_out {
            let read = json_of(
                &jojobot
                    .read_message(Parameters(ReadMessageArgs {
                        message_id: id.clone(),
                        sid: Some(as_bot(&jojobot, "otto")),
                    }))
                    .await
                    .expect("read_message ok"),
            );
            assert_eq!(
                read["body"].as_str().map(str::len),
                Some(1_500),
                "{id} is whole by id: {read}"
            );
        }
    }

    /// **The receipt is counted against the ceiling at every size of box.** A
    /// fill that spends the whole ceiling on the delivery passes it by the
    /// receipt and the postcondition, but only when the bodies happen to add up
    /// inside that window. Sweeping the size of the body in steps finer than the
    /// window is what lands in it, whatever the receipt weighs.
    #[tokio::test]
    async fn a_post_counts_its_own_receipt_against_the_ceiling_at_every_size_of_box() {
        for body_len in (800..=1_300).step_by(20) {
            let (jojobot, posted) = a_post_collecting_a_box(body_len).await;
            let size = posted.to_string().chars().count();
            assert!(
                size + crate::answer::STATUS_BAR_ROOM <= jojobot_domain::text::ANSWER_CEILING,
                "bodies of {body_len}: the post's whole answer is {size} characters"
            );
            let (accounted, _) = accounted_for(&posted);
            assert_eq!(accounted.len(), 25, "bodies of {body_len}: {posted}");
            let counted = counts(&jojobot, "otto").await;
            assert_eq!(counted["counts"]["new"], 0, "bodies of {body_len}");
        }
    }

    /// **A box that fits comes back whole and unmarked.** The control for the
    /// two cases above: a fill that flagged or named everything would pass
    /// them, and fails here.
    #[tokio::test]
    async fn a_post_that_collects_a_small_box_carries_every_message_whole() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;
        for n in 0..3 {
            send(&jojobot, "otto", "epsilon", &format!("small message {n}")).await;
        }
        let posted = send(&jojobot, "epsilon", "otto", "reporting in").await;
        let mail = &posted["your_mail"];
        assert_eq!(mail["count"], 3, "{posted}");
        assert!(mail.get("not_shown").is_none(), "{posted}");
        assert!(mail.get("how_to_read").is_none(), "{posted}");
        let shown = mail["messages"].as_array().expect("a list of messages");
        assert!(
            shown
                .iter()
                .all(|m| m["body"].is_string() && m.get("body_elided").is_none()),
            "every body comes whole: {posted}"
        );
    }

    /// **What was genuinely waiting in the sender's own box at send time
    /// travels to the READER, not only to the sender's own receipt.**
    ///
    /// The number has to be a real count the code computed, not a stub: two
    /// specific messages land in otto's box before otto ever posts anywhere,
    /// so `sender_mail_waiting_at_send` has to read `2` — a build that always
    /// writes `None`, or that always writes `Some(0)`, fails this the same way
    /// a build that genuinely counts does not.
    #[tokio::test]
    async fn the_senders_own_waiting_count_travels_to_the_reader() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;

        // Two things land for otto before otto posts anywhere else.
        send(&jojobot, "otto", "epsilon", "first thing waiting").await;
        send(&jojobot, "otto", "epsilon", "second thing waiting").await;

        let posted = send(&jojobot, "epsilon", "otto", "reporting in").await;
        assert_eq!(
            posted["sender_mail_waiting_at_send"], 2,
            "the sender's own receipt already carries the true count: {posted}"
        );

        // The point of the slice: a reader who never sent anything sees the
        // same number, on the very read that hands them the message.
        let delivered = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(as_bot(&jojobot, "epsilon")),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            delivered["messages"][0]["sender_mail_waiting_at_send"], 2,
            "the reader sees the sender's true count, not just the sender: {delivered}"
        );
    }

    /// **A sender who genuinely had nothing waiting reads back as zero, never
    /// as the same absence an old message wears.** The two must not collapse:
    /// "nothing was waiting" and "this message predates the field" are
    /// different claims, and a reader who cannot tell them apart cannot use
    /// either one.
    #[tokio::test]
    async fn a_sender_with_nothing_waiting_reads_back_as_zero_not_absent() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;

        let posted = send(&jojobot, "epsilon", "otto", "nothing was waiting for me").await;
        assert_eq!(
            posted["sender_mail_waiting_at_send"], 0,
            "zero waiting is a real, computed answer: {posted}"
        );
        assert!(
            !posted["sender_mail_waiting_at_send"].is_null(),
            "zero must not render as the absence an old message wears: {posted}"
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

    /// **A name that is a bot's alias is refused honestly** — named as an
    /// alias, naming the bot it belongs to, and never advised to boot a bot
    /// that cannot exist: the box is already open, under a different handle.
    #[tokio::test]
    async fn posting_to_a_bots_alias_names_the_bot_and_the_address_that_works() {
        let jojobot = mailbox_handler();
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                aliases: Some(vec!["Dev Two".into()]),
                ..crate::memory::testing::add_args("bot", "gamma", "gamma")
            }))
            .await
            .expect("add ok");

        let refused = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "dev-two".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "reporting in".into(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("a bad address is an answer, not an error"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["wrote"], false);
        let advice = refused["how_to_proceed"]
            .as_str()
            .expect("advice is a string");
        assert!(
            advice.contains("alias") && advice.contains("bot:gamma"),
            "the refusal has to say this is an alias and name the bot it belongs to: {advice}",
        );
        assert!(
            !advice.contains("start_here"),
            "start_here cannot open a box for a name that was never a bot — the box it means \
             already exists, under a different handle: {advice}",
        );
    }

    /// **The genuinely-missing case reads exactly as it did before.** A name
    /// nothing answers to — no bot, no alias — still gets the repair that
    /// actually fits it.
    #[tokio::test]
    async fn posting_to_a_name_nothing_answers_to_still_gets_the_old_advice() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;

        let refused = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "nobody-here".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "reporting in".into(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("a bad address is an answer, not an error"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        let advice = refused["how_to_proceed"]
            .as_str()
            .expect("advice is a string");
        assert!(
            advice.contains("start_here"),
            "a name nothing answers to still points at the repair that fits it: {advice}",
        );
    }

    /// **The first post to the operator's handle opens their box, and the post
    /// lands in it.** The box is named for the person, owned by them, and the
    /// only one they have: a second post goes to the same box. Counted from the
    /// store, not from the verb.
    #[tokio::test]
    async fn the_first_post_to_the_operator_opens_their_one_box_and_lands() {
        let jojobot = mailbox_handler();
        owning(&jojobot, "epsilon").await;
        name_the_operator(&jojobot, "milhouse").await;
        assert!(
            boxes_owned_by(&jojobot, "person:milhouse").await.is_empty(),
            "naming the operator opens nothing: a post is what opens the box"
        );

        let first = send(&jojobot, "person:milhouse", "epsilon", "the first note").await;
        assert_eq!(first["mailbox"], "person-milhouse", "{first}");
        let opened = boxes_owned_by(&jojobot, "person:milhouse").await;
        assert_eq!(opened.len(), 1, "one box: {opened:?}");
        assert_eq!(store_counts(&jojobot, &opened[0]).await, (1, 0, 0));

        send(&jojobot, "person:milhouse", "epsilon", "the second note").await;
        assert_eq!(
            boxes_owned_by(&jojobot, "person:milhouse").await,
            opened,
            "a second post opens no second box"
        );
        assert_eq!(store_counts(&jojobot, &opened[0]).await, (2, 0, 0));
    }

    /// **Only the operator has a box.** A post to any other person is blocked,
    /// says so without naming anybody else, and opens nothing; the same call to
    /// the operator lands, so the refusal is about the addressee.
    #[tokio::test]
    async fn a_post_to_a_person_who_is_not_the_operator_is_blocked_and_opens_nothing() {
        let jojobot = mailbox_handler();
        owning(&jojobot, "epsilon").await;
        name_the_operator(&jojobot, "milhouse").await;
        crate::memory::testing::ensure(&jojobot, "person:ned-flanders").await;

        let refused = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "person:ned-flanders".into(),
                    sid: as_bot(&jojobot, "epsilon"),
                    subject: None,
                    body: "for somebody else".into(),
                    in_reply_to: None,
                }))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            !refused.to_string().contains("milhouse"),
            "the refusal names nobody else: {refused}"
        );
        assert!(
            boxes_owned_by(&jojobot, "person:ned-flanders")
                .await
                .is_empty(),
            "nothing was opened for them"
        );

        send(&jojobot, "person:milhouse", "epsilon", "for the operator").await;
        assert_eq!(boxes_owned_by(&jojobot, "person:milhouse").await.len(), 1);
    }

    /// **With no operator named, a post to a person is blocked with the way to
    /// name one**, and opens nothing. The same call lands once the operator is
    /// named, so the refusal is about the missing name.
    #[tokio::test]
    async fn a_post_to_a_person_while_nobody_is_the_operator_says_how_to_name_one() {
        let jojobot = mailbox_handler();
        owning(&jojobot, "epsilon").await;
        crate::memory::testing::ensure(&jojobot, "person:milhouse").await;

        let refused = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "person:milhouse".into(),
                    sid: as_bot(&jojobot, "epsilon"),
                    subject: None,
                    body: "for the operator".into(),
                    in_reply_to: None,
                }))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        let way = refused["how_to_proceed"].as_str().expect("a way forward");
        for identifier in ["add_entity", "topic:instance", "operator"] {
            assert!(way.contains(identifier), "{identifier} is named: {way}");
        }
        assert!(boxes_owned_by(&jojobot, "person:milhouse").await.is_empty());

        name_the_operator(&jojobot, "milhouse").await;
        send(&jojobot, "person:milhouse", "epsilon", "for the operator").await;
        assert_eq!(boxes_owned_by(&jojobot, "person:milhouse").await.len(), 1);
    }

    /// **Re-pointing the operator leaves the old box where it is.** The first
    /// person's box stays, private, with its mail; the new operator gets their
    /// own box at the first post to them. Nothing is deleted or re-owned.
    #[tokio::test]
    async fn naming_a_new_operator_leaves_the_old_operators_box_and_opens_a_new_one() {
        let jojobot = mailbox_handler();
        owning(&jojobot, "epsilon").await;
        name_the_operator(&jojobot, "milhouse").await;
        send(&jojobot, "person:milhouse", "epsilon", "for the first").await;
        let old = boxes_owned_by(&jojobot, "person:milhouse").await;

        // **A different operator is named by the bot that heads the chart**, so
        // epsilon is given a report and nothing above it.
        let epsilon = as_bot(&jojobot, "epsilon");
        crate::memory::testing::ensure(&jojobot, "bot:omega").await;
        crate::memory::testing::capture_as(
            &jojobot,
            &epsilon,
            CaptureArgs {
                fields: Some([("reports_to".to_string(), "bot:epsilon".to_string())].into()),
                ..crate::memory::testing::capture_args("bot:omega", "omega reports to epsilon")
            },
        )
        .await;
        name_the_operator_as(&jojobot, &epsilon, "ned-flanders").await;
        send(&jojobot, "person:ned-flanders", "epsilon", "for the second").await;

        assert_eq!(boxes_owned_by(&jojobot, "person:milhouse").await, old);
        assert_eq!(store_counts(&jojobot, &old[0]).await, (1, 0, 0));
        let new = boxes_owned_by(&jojobot, "person:ned-flanders").await;
        assert_eq!(new.len(), 1);
        assert_ne!(new, old);
        // The old operator is no longer addressable: only the one named has a box.
        let refused = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "person:milhouse".into(),
                    sid: as_bot(&jojobot, "epsilon"),
                    subject: None,
                    body: "to the old operator".into(),
                    in_reply_to: None,
                }))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
    }

    /// **A box that resembles the operator's name does not stop it opening.**
    /// A bot's box `person-lisa-box` contains `person-lisa`, which the creation
    /// screen flags as a resemblance. The operator's box name is derived and its
    /// owner differs, so the box opens, and the other box is untouched.
    #[tokio::test]
    async fn a_box_that_resembles_the_operators_does_not_stop_it_opening() {
        let jojobot = mailbox_handler();
        owning(&jojobot, "epsilon").await;
        // Written straight to the store: the memory screen would refuse a bot
        // handle this close to the person's, and the box screen is what is
        // being exercised.
        a_second_box(&jojobot, "omega", "person-lisa-box").await;
        name_the_operator(&jojobot, "lisa").await;

        let landed = send(&jojobot, "person:lisa", "epsilon", "for the operator").await;
        assert_eq!(landed["mailbox"], "person-lisa", "{landed}");
        assert_eq!(
            boxes_owned_by(&jojobot, "person:lisa").await.len(),
            1,
            "the person has one box"
        );
        let other = boxes_owned_by(&jojobot, "bot:omega").await;
        assert_eq!(other.len(), 1, "the other box is still its owner's");
        assert_eq!(other[0].as_str(), "person-lisa-box");
        assert_eq!(store_counts(&jojobot, &other[0]).await, (0, 0, 0));
    }
    /// 🚨 **A post to a handle the instance names as operator is refused when
    /// that handle is not a person.** The operator is a person; a record that
    /// names a bot as operator names nobody, and a post to that bot would land
    /// in an ordinary bot's box that anyone booting the bot can read. The
    /// refusal is the no-operator sentence and nothing is written. The same bot
    /// not named as operator still receives its mail, so the refusal is about
    /// the operator record and not about bots.
    #[tokio::test]
    async fn a_post_to_an_operator_handle_that_is_not_a_person_is_refused() {
        let jojobot = mailbox_handler();
        let sender = owning(&jojobot, "epsilon").await;
        owning(&jojobot, "gamma").await;
        let post = || {
            jojobot.post_message(Parameters(PostMessageArgs {
                to: "bot:gamma".into(),
                body: "for the operator".into(),
                in_reply_to: None,
                subject: None,
                sid: sender.clone(),
            }))
        };

        // The positive first: bot:gamma is an ordinary addressee and it lands.
        let landed = json_of(&post().await.expect("post ok"));
        assert_ne!(landed["status"], "blocked", "{landed}");

        // The instance's record now names that bot as operator.
        crate::memory::testing::capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some([("operator".to_string(), "bot:gamma".to_string())].into()),
                provenance: Some("testimony".into()),
                ..crate::memory::testing::capture_args("topic:instance", "who the operator is")
            },
        )
        .await;
        let refused = json_of(&post().await.expect("a refusal is an answer"));
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
        let how = refused["how_to_proceed"].as_str().expect("a way forward");
        for identifier in ["bot:gamma", "topic:instance", "operator"] {
            assert!(how.contains(identifier), "names {identifier}: {how}");
        }
        assert!(
            !how.contains("no entity yet"),
            "the record names an entity that exists: {how}"
        );
        assert_eq!(
            counts(&jojobot, "gamma").await["counts"]["total"],
            1,
            "only the first post landed"
        );
    }
    async fn post_to(jojobot: &Jojobot, to: &str, sid: &str) -> Result<CallToolResult, McpError> {
        jojobot
            .post_message(Parameters(PostMessageArgs {
                to: to.into(),
                body: "hello".into(),
                in_reply_to: None,
                subject: None,
                sid: sid.into(),
            }))
            .await
    }

    /// 🚨 **The refusals for an addressee wear the word their repair needs.** A
    /// name nobody answers to is a call that has to change; a bot holding two
    /// boxes is damage no caller can repair, a person's. A board nobody can read
    /// is a storage failure and wears that word.
    #[tokio::test]
    async fn the_refusals_for_an_addressee_wear_their_words() {
        let jojobot = mailbox_handler();
        let sender = owning(&jojobot, "epsilon").await;
        owning(&jojobot, "gamma").await;

        let unknown = post_to(&jojobot, "nobody-answers-to-this", &sender)
            .await
            .expect("a refusal is an answer");
        assert_fix_by("unknown addressee", &unknown, "change");

        // The positive: a post to a bot with exactly one box lands.
        let landed = json_of(
            &post_to(&jojobot, "bot:gamma", &sender)
                .await
                .expect("post ok"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
        assert!(landed.get("fix_by").is_none(), "{landed}");

        a_second_box(&jojobot, "gamma", "sigma").await;
        let damaged = post_to(&jojobot, "bot:gamma", &sender)
            .await
            .expect("a refusal is an answer");
        assert_fix_by("a bot holding two boxes", &damaged, "person");

        let down = handler_with_mailboxes_down(std::sync::Arc::new(
            jojobot_domain::memory::testing::InMemoryMemory::booted(),
        ));
        let down_sender = as_bot(&down, "epsilon");
        let unreadable = blocked(
            &post_to(&down, "bot:gamma", &down_sender)
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(
            unreadable["fix_by"].as_str(),
            other_store_failure_word().map(FixBy::as_token),
            "{unreadable}"
        );
    }
    /// **A post to a person, while the record names a bot as operator, says
    /// what the record holds.** The person is not the operator, but "this person
    /// is not the operator, write to the handle the boot names" would send the
    /// caller to a handle that is no person. The refusal names the held handle
    /// and why it does not count. Paired with the same post once the record
    /// names that person, which lands.
    #[tokio::test]
    async fn a_post_to_a_person_while_the_record_names_a_bot_says_what_the_record_holds() {
        let jojobot = mailbox_handler();
        let sender = owning(&jojobot, "epsilon").await;
        owning(&jojobot, "gamma").await;
        crate::memory::testing::ensure(&jojobot, "person:milhouse").await;
        let names = |handle: &str| CaptureArgs {
            fields: Some([("operator".to_string(), handle.to_string())].into()),
            provenance: Some("testimony".into()),
            ..crate::memory::testing::capture_args("topic:instance", "who the operator is")
        };

        crate::memory::testing::capture_ok(&jojobot, names("bot:gamma")).await;
        let refused = blocked(
            &post_to(&jojobot, "person:milhouse", &sender)
                .await
                .expect("a refusal is an answer"),
        );
        let how = refused["how_to_proceed"].as_str().expect("a way forward");
        assert!(
            how.contains("bot:gamma"),
            "names what the record holds: {how}"
        );
        assert!(
            !how.contains("no entity yet"),
            "the record names an entity that exists: {how}"
        );

        // **Changing the operator once one is named is the head of the chart's**,
        // so the sender is given a report and nothing above it.
        let head = as_bot(&jojobot, "epsilon");
        crate::memory::testing::ensure(&jojobot, "bot:omega").await;
        crate::memory::testing::capture_as(
            &jojobot,
            &head,
            CaptureArgs {
                fields: Some([("reports_to".to_string(), "bot:epsilon".to_string())].into()),
                ..crate::memory::testing::capture_args("bot:omega", "omega reports to epsilon")
            },
        )
        .await;
        crate::memory::testing::capture_as(&jojobot, &head, names("person:milhouse")).await;
        let landed = json_of(
            &post_to(&jojobot, "person:milhouse", &sender)
                .await
                .expect("post ok"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
    }
}

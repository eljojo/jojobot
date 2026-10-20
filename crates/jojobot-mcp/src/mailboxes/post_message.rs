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
    ///
    /// A post into your own box delivers nothing, and a post that had nothing to
    /// hand over still succeeded.
    pub(crate) to: String,
    /// The message itself. Prose: paragraphs are fine.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as
    /// the handle that thing wears today, even after a rename.
    /// Text inside backticks or a fenced block is literal: a handle there is
    /// kept as written and never becomes a link.
    ///
    /// To quote the operator, write the address of the claim that holds their
    /// words, `@kind:slug#fN`: jojobot writes those words in beside it, and
    /// refuses a claim they did not say themselves.
    pub(crate) body: String,
    /// **Your session id.** Required here, because it is what jojobot records
    /// as the sender: a message from nobody is a message nobody can reply to,
    /// and identity that is merely declared is identity that can be wrong.
    ///
    /// The message carries `sender_mail_waiting_at_send`: what was genuinely
    /// waiting in YOUR OWN box at the moment you sent it, the same count
    /// your_mail's delivery would have shown, read before this post could touch it.
    /// `null` means jojobot could not tell (no session, no box, an unreadable
    /// board) and is NEVER zero; `0` is a real and different answer. It is stamped
    /// once, here, and carried unchanged wherever read_mailbox or list_sent returns
    /// the message later, so a reader can check a claim about your own mail against
    /// what was true when you hit send.
    pub(crate) sid: String,
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of the write**, because a call sent without
    /// one cannot be repeated safely. If an answer is lost, send the
    /// same call again with the same `request_id`: if
    /// the first call wrote, you get its receipt back marked `repeat` and nothing
    /// is written a second time, so the message is posted once. It needs a `sid`, and a call that
    /// wrote nothing spends no id.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
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
    ///
    /// It says only that the two are one exchange: it does not deliver the
    /// original, handle it, or oblige anybody. Messages riding back under your_mail
    /// carry `written_by_other_run` when a different run posted them, exactly as
    /// read_mailbox's `new_only` argument says.
    #[serde(default)]
    pub(crate) in_reply_to: Option<String>,

    /// **The id of a message you sent that this one replaces.** Optional. Use it
    /// to correct or withdraw: the reader is handed this message, and the one
    /// it replaces is not handed over as fresh mail when nobody has read it.
    /// It must be a message YOU sent into the SAME box as this one, and one no
    /// other message has replaced; each refusal names its reason and writes
    /// nothing. If the reader already has the original, this message arrives
    /// saying it replaces one they have seen. The original is not deleted or
    /// edited: read_message returns it with `superseded_by`, and list_sent
    /// shows you the same. On a post to the operator, the page they read marks
    /// the original as replaced and names this message, and no push goes out for
    /// the original.
    #[serde(default)]
    pub(crate) supersedes: Option<String>,
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
    /// A real delivery: everything unprocessed, except mail its sender replaced
    /// before anybody read it, moves out of `new` and becomes
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
        run: &str,
    ) -> Collected {
        let OwnBox::The(own) = self.own_box(bot).await else {
            return Collected::default();
        };
        // **A person's box is handed to nobody by a post.** A handle bound to a
        // person is not a bot's, and the box it owns is read by that person
        // alone. A board that cannot say whose the box is hands nothing over.
        if self.box_is_private(&own).await.unwrap_or(true) {
            return Collected::default();
        }
        // **A note you left yourself is not mail you have received.** Posting
        // into your own box and taking delivery in the same call would hand
        // you back the message you had just written, marked as delivered — the
        // call undoing itself, and the one message on the board a caller
        // demonstrably already has.
        if &own == posted_into {
            return Collected::default();
        }
        // **The delivery is a part of its own, written once under a
        // `request_id`.** An earlier call that took the mail left the ids it took,
        // so a resend hands over the same mail rather than the leftovers of it.
        use jojobot_domain::request::{Part, part};
        let taken = match part(
            "delivery",
            None,
            self.mailboxes.read_mailbox(
                &own,
                mailbox::TakenBy::Posting,
                jojobot_domain::mailbox::Acting::by(run, self.clock().now()),
            ),
        )
        .await
        {
            Part::Ran(Ok(mailbox::Guarded::Written(delivery))) => (delivery, false),
            Part::Ran(Ok(_)) => return Collected::default(),
            Part::Ran(Err(_)) => {
                return Collected {
                    failed: true,
                    ..Collected::default()
                };
            }
            Part::Recorded(recorded) => {
                let ids: Vec<MessageId> =
                    serde_json::from_str::<serde_json::Value>(&recorded.landed)
                        .ok()
                        .and_then(|landed| {
                            landed.get("ids")?.as_array().map(|ids| {
                                ids.iter()
                                    .filter_map(|id| {
                                        id.as_str().map(|id| MessageId(id.to_string()))
                                    })
                                    .collect()
                            })
                        })
                        .unwrap_or_default();
                let Ok(mut messages) = self.mailboxes.messages_by_id(&ids).await else {
                    return Collected {
                        failed: true,
                        ..Collected::default()
                    };
                };
                messages.sort_by_key(|message| message.sent_at);
                (
                    Delivery {
                        mailbox: own.clone(),
                        messages: messages
                            .into_iter()
                            .map(|message| Delivered {
                                message,
                                seen_before: false,
                            })
                            .collect(),
                        superseded: Vec::new(),
                    },
                    true,
                )
            }
        };
        let (delivery, resumed) = taken;
        if delivery.messages.is_empty() {
            return Collected::default();
        }
        // **Only what nobody has taken is listed.** A bot still acting on its
        // mail posts again and again, and a leftover is mail it was handed by
        // an earlier read: an envelope for each, on every post, is the same
        // thing shipped over and over. They stay owed, and `read_mailbox`
        // still returns them flagged — that is crash recovery. Here they are
        // named, and the answer names the call that returns them.
        let (delivery, leftovers) = match resumed {
            true => (delivery, Vec::new()),
            false => split_leftovers(delivery, true),
        };
        // The same rendering `read_mailbox` uses, so mail taken this way reads
        // identically to mail somebody went and got.
        let mut rendered = delivery_json(&delivery, true);
        self.mark_other_runs(&mut rendered, &delivery, viewer).await;
        self.mark_replaces_seen(&mut rendered, &delivery).await;
        // **Counts, not lists.** The ids of the leftovers and the withheld
        // messages are the same on every post; `read_mailbox` lists them.
        note_unshipped_mail_as_counts(&mut rendered, &leftovers);
        if delivery.messages.is_empty() {
            drop_the_envelope_of_an_empty_delivery(&mut rendered);
        }
        Collected {
            delivery: Some((rendered, delivery)),
            failed: false,
        }
    }
}

/// **What a post's collection of the sender's own mail left**: the mail, if
/// there was any, and whether the collection failed.
#[derive(Default)]
struct Collected {
    delivery: Option<(serde_json::Value, Delivery)>,
    failed: bool,
}

#[tool_router(router = post_message_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Leave a message for someone who is not in this conversation. \
                       `to` must come down to exactly one box jojobot can read: a bot \
                       by name or handle, or the operator by their person handle \
                       (`person:` and the slug; the boot names it under `operator`; \
                       your first post to them opens their box, nobody else has one, \
                       and no bot reads it back, you included). A name no colleague \
                       answers to, or a colleague whose mail is missing or doubled, \
                       comes back status: blocked with nothing written, and the \
                       answer says which. Give it a `subject`: one line on what it is \
                       about, which a reader sees on a listing before opening \
                       anything. A body too long to be handed to its reader whole is \
                       refused with nothing written: split it, or put the text on a \
                       document (put_document) and send its address. To correct or \
                       withdraw a message you sent, post again to the same box with \
                       `supersedes` set to its id: the reader is handed the new one, \
                       and the old one, if unread, is not handed over as fresh mail. \
                       IT ANSWERS WITH A RECEIPT, NOT THE MESSAGE: the id that \
                       read_message and mark_processed later target, the state as it \
                       stands (`read` already, if a person picked it up, is success), \
                       body_bytes and body_elided: true. Your body is not echoed \
                       back; list_sent with include_bodies returns it, taking no \
                       delivery. POSTING ALSO DELIVERS: mail in YOUR OWN box that no \
                       read has handed you yet rides back under your_mail, out of \
                       `new` and yours to finish. Mail an earlier read already handed \
                       you is not listed again: it is counted under \
                       your_mail.leftovers and still owed, and mail replaced before \
                       you read it is counted under your_mail.superseded; \
                       read_mailbox lists both, and read_message returns one by id. \
                       The sender is the bot behind the `sid` you pass, never one you \
                       declare, and a sid jojobot is not holding comes back status: \
                       blocked with nothing written. The receipt's `changed` lists \
                       what moved. Each argument's description holds the rest. A \
                       `request_id` makes a resend safe: a repeat returns the first \
                       receipt and writes nothing."
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
        let to_the_operator = addressee.kind() == Some(EntityKind::PERSON);
        // **A call that finds its message already posted under this id is
        // finishing the write.** What the post is checked against (a reply that
        // names a message, a message it replaces) was true before the post and
        // is not asked again; the receipt is built from the message as posted.
        if let Some(recorded) = jojobot_domain::request::skip("post") {
            let posted = serde_json::from_str::<serde_json::Value>(&recorded.landed)
                .ok()
                .and_then(|landed| {
                    landed
                        .get("id")?
                        .as_str()
                        .map(|id| MessageId(id.to_string()))
                });
            let found = match posted {
                Some(id) => self.mailboxes.message_by_id(&id).await.ok().flatten(),
                None => None,
            };
            return match found {
                Some(message) => {
                    self.posted(&args.sid, &caller, to_the_operator, message)
                        .await
                }
                None => Ok(crate::caller::handle_declined(
                    "post_message",
                    crate::answer::WayForward::person(
                        "The message was posted under this request_id, but it cannot be read \
                         back, so the rest cannot be finished.",
                    ),
                )),
            };
        }
        let supersedes = args
            .supersedes
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| MessageId(id.to_string()));
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
        // **A post to the operator quotes them by address, and the address is
        // checked.** Expanded here, before the body is measured and before it is
        // written, so what is stored and what is measured against the ceiling are
        // the same words.
        let body = if to_the_operator {
            match self.quotes_expanded(&args.body).await {
                Ok(expanded) => expanded,
                Err(e) => return mailbox_declined(e),
            }
        } else {
            args.body
        };
        let new = NewMessage {
            mailbox: destination,
            body,
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
            supersedes,
            sender_mail_waiting_at_send,
            posted_by_session,
        };
        // **A message nobody could be handed whole is not posted.** The reader
        // of a box is handed the messages in it under the answer ceiling, and
        // one whose body cannot fit even alone would come back with its body
        // left out. Refused here, before anything is written, with the ways
        // to send it.
        if let Some(room) = body_room_beyond(&new) {
            return mailbox_declined(MailboxError::InvalidMessage(format!(
                "this body is {} characters and the largest a reader can be handed whole is \
                 about {room}. Split it into several messages, or put the long text on a document \
                 with put_document and send its address",
                new.body.chars().count()
            )));
        }
        // Declined rather than errored: a reply naming a message jojobot does
        // not hold is a bad reference, and every other bad reference on this
        // surface comes back as the blocked shape.
        let posted = match jojobot_domain::request::armed_part(
            "post",
            None,
            self.mailboxes.post_message(new),
        )
        .await
        {
            Ok(posted) => posted,
            Err(e) => return mailbox_declined(e),
        };
        match posted {
            mailbox::Guarded::Written(message) => {
                self.posted(&args.sid, &caller, to_the_operator, message)
                    .await
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

impl Jojobot {
    /// **The answer to a post, and the second part of it**: the receipt of the
    /// message and the sender's own mail, taken in the same call. A call that
    /// finds the message already posted under its id comes here too, with the
    /// message as the earlier call posted it.
    async fn posted(
        &self,
        sid: &str,
        caller: &crate::caller::Caller,
        to_the_operator: bool,
        message: Message,
    ) -> Result<CallToolResult, McpError> {
        self.beat("post_message", message.mailbox.as_str(), Some(sid))
            .await;
        let mut body = message_receipt_json(
            &message,
            Some(if to_the_operator {
                "you wrote this body, so it is not shipped back to you. It is in a person's \
                 box, so no bot reads it back, its writer included"
            } else {
                "you wrote this body, so it is not shipped back to you. list_sent with \
                 include_bodies: true returns it, and takes no delivery"
            }),
        );
        // **A replacement changes two messages**: the one it posts and
        // the one it replaces, which its reader is no longer handed.
        let mut changed = crate::changed::Changed::created(message.id.as_str());
        if let Some(replaced) = &message.supersedes {
            changed = changed.and(crate::changed::Changed::value(
                replaced.as_str(),
                crate::changed::SUPERSEDED_BY,
                None,
                Some(message.id.as_str()),
            ));
        }
        // **Posting takes delivery of your own box, in the same call.**
        // The moment an agent posts is the moment a reply is most
        // likely waiting, because posting is what it does at the end of
        // a piece of work — and two agents each holding an unread reply
        // are two agents talking past each other, with nothing blocked
        // and nothing to notice.
        let collected_mail = self
            .delivered_with_the_post(
                &caller.bot,
                &message.mailbox,
                caller.card.as_ref(),
                caller.sid.as_str(),
            )
            .await;
        // **With an id the call is not finished until the mail is taken**: the
        // same call sent again hands over the same mail and posts nothing twice.
        if collected_mail.failed && jojobot_domain::request::carries_an_id() {
            return Ok(crate::caller::handle_declined(
                "post_message",
                crate::answer::WayForward::retry(
                    "The message was posted, and your own mail was not collected. Send the same \
                     call again with the same request_id and it hands you the mail and posts \
                     nothing twice.",
                ),
            ));
        }
        jojobot_domain::request::finished();
        let delivered = collected_mail.delivery;
        let changed = match crate::request::note_resumed(&mut body) {
            true => changed.and(crate::changed::Changed::not_read()),
            false => changed,
        };
        crate::changed::note_changed(&mut body, changed);
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
        if to_the_operator
            && self
                .first_contact(crate::teaching::OPERATOR_QUOTE_DOMAIN, Some(caller))
                .await
        {
            crate::answer::note_teaching(&mut body, crate::teaching::OPERATOR_QUOTE_TEACHING);
        }
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
/// **How many leftover ids a read may name beside the message and still hand it
/// over whole.**
const LEFTOVER_IDS_ROOM: usize = 25;

/// **A moment written with every fractional digit a timestamp can carry.** A
/// timestamp prints without its trailing zeros, so the same message prints a
/// few characters longer or shorter from one moment to the next. The probe in
/// [`body_room_beyond`] wears this one, so the answer depends on the lengths of
/// the message's fields and on nothing that changes between two calls.
const WIDEST_TIMESTAMP: &str = "2026-10-08T15:30:12.123456789Z";

/// **`Some(room)` when this message could not be handed to a reader whole**,
/// with `room` the longest body that could be.
///
/// The question is asked of the same fill a read of the box uses
/// ([`fit_delivery`]), on a probe of the message as it would be delivered, so
/// the refusal and the delivery cannot come to disagree. The probe wears the
/// widest id a message may have and the widest timestamp, which keeps the answer
/// on the safe side: a body this lets through is delivered whole whatever id the
/// store draws and whenever it is sent.
///
/// **What the guarantee covers**: a body this lets through is delivered whole
/// while the addressee's box holds at most [`LEFTOVER_IDS_ROOM`] leftovers, a
/// read naming each of them beside the message. Past that, and when other fresh
/// mail arrives in the same read, the body can still be left out of the answer;
/// `read_message` returns it whole by id. A read names every leftover and there
/// is no bound on how many a box holds, so no probe can promise more.
fn body_room_beyond(new: &NewMessage) -> Option<usize> {
    let whole = |body: String| -> bool {
        let message = Message {
            id: MessageId("x".repeat(64)),
            mailbox: new.mailbox.clone(),
            body,
            subject: new.subject.clone(),
            sender: new.sender.clone(),
            sent_at: WIDEST_TIMESTAMP.parse().expect("a fixed moment"),
            state: mailbox::MessageState::Read,
            notes: None,
            in_reply_to: new.in_reply_to.clone(),
            taken_by: Some(mailbox::TakenBy::Reading),
            sender_mail_waiting_at_send: new.sender_mail_waiting_at_send,
            posted_by_session: new.posted_by_session.clone(),
            supersedes: new.supersedes.clone(),
            superseded_by: None,
        };
        // The leftovers a read names beside the message, at the widest id each.
        let leftovers: Vec<Delivered> = (0..LEFTOVER_IDS_ROOM)
            .map(|_| Delivered {
                message: Message {
                    body: String::new(),
                    ..message.clone()
                },
                seen_before: true,
            })
            .collect();
        let delivery = Delivery {
            mailbox: new.mailbox.clone(),
            messages: vec![Delivered {
                message,
                seen_before: false,
            }],
            superseded: Vec::new(),
        };
        let mut rendered = delivery_json(&delivery, true);
        // The notice a replacement of a message already seen carries, at the
        // widest id, so the room found here holds when it applies.
        if new.supersedes.is_some() {
            note_replaces_one_already_seen(
                &mut rendered["messages"][0],
                &MessageId("x".repeat(64)),
                mailbox::MessageState::Read,
            );
        }
        if let Some(run) = new.posted_by_session.as_deref() {
            note_written_by_other_run(&mut rendered["messages"][0], run, Some(false));
        }
        note_leftovers(&mut rendered, &leftovers);
        fit_delivery(&mut rendered, &delivery, 0);
        rendered.get("not_shown").is_none() && rendered["messages"][0].get("body_elided").is_none()
    };
    if whole(new.body.clone()) {
        return None;
    }
    // The room is found by the same question asked of shorter bodies: the
    // longest one that still comes through whole. A body of nothing is the
    // floor, and a message that cannot be handed whole empty has no room.
    let (mut fits, mut over) = (0usize, new.body.chars().count());
    while over - fits > 1 {
        let mid = (fits + over) / 2;
        if whole("x".repeat(mid)) {
            fits = mid;
        } else {
            over = mid;
        }
    }
    Some(fits)
}

fn what_a_post_left_standing(message: &Message, collected: u64) -> String {
    let took = match collected {
        0 => String::new(),
        n => format!(
            " This call also took delivery of {n} of your own: they are out of new and yours to \
             finish with mark_processed."
        ),
    };
    let replaces = match &message.supersedes {
        Some(replaced) => format!(
            " It replaces {replaced}: if that one is still unread, its reader is handed this \
             one instead and is told that {replaced} was replaced."
        ),
        None => String::new(),
    };
    format!(
        "The message is in {}'s box, waiting. Nothing else in that box moved, and nobody is \
         obliged to read it before they next open the box.{replaces}{took}",
        message.mailbox.as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// **A post names the message it made, by its id and nothing of its words**
    /// (card 1333). Two posts name two ids, so the entry is the message and not
    /// a constant.
    #[tokio::test]
    async fn a_post_names_the_message_it_created() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;

        let first = send(&jojobot, "otto", "epsilon", "the urn is descaled").await;
        let second = send(&jojobot, "otto", "epsilon", "the urn is empty").await;
        for posted in [&first, &second] {
            let changed = posted["changed"]
                .as_array()
                .unwrap_or_else(|| panic!("a list of changes: {posted}"));
            assert_eq!(changed.len(), 1, "{posted}");
            assert_eq!(changed[0]["on"], posted["id"], "{posted}");
            assert_eq!(changed[0]["what"], "created", "{posted}");
            assert!(
                changed[0].get("to").is_none(),
                "no words come back: {posted}"
            );
        }
        assert_ne!(first["changed"][0]["on"], second["changed"][0]["on"]);
    }

    /// **A post that replaces a message names the replaced one too** (card
    /// 2347). The old message changes beside the new one: it is withheld from
    /// its reader and reads as replaced by the new id. `superseded_by` is a key
    /// the wire serves and nothing outside this process declares, so the literal
    /// is pinned here.
    ///
    /// The control is the same verb with no `supersedes`: one entry, the new
    /// message, so the case does not hold on a build that always names a second
    /// message.
    #[tokio::test]
    async fn a_post_that_replaces_names_the_replaced_message() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;

        let old = send(&jojobot, "otto", "epsilon", "the urn is descaled").await;
        let old_id = old["id"].as_str().expect("an id").to_string();
        let plain = send(&jojobot, "otto", "epsilon", "the urn is empty").await;
        assert_eq!(
            plain["changed"].as_array().map(Vec::len),
            Some(1),
            "a post that replaces nothing names only its message: {plain}"
        );

        let replaced = replacing(
            &jojobot,
            "otto",
            "epsilon",
            "the urn is descaled and full",
            &old_id,
        )
        .await;
        assert_ne!(replaced["status"], "blocked", "{replaced}");
        let changed = replaced["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {replaced}"));
        assert_eq!(changed.len(), 2, "{replaced}");
        assert_eq!(changed[0]["on"], replaced["id"], "{replaced}");
        assert_eq!(changed[0]["what"], "created", "{replaced}");
        assert_eq!(changed[1]["on"], old_id, "{replaced}");
        assert_eq!(changed[1]["what"], "superseded_by", "{replaced}");
        assert_eq!(changed[1]["to"], replaced["id"], "{replaced}");
        assert!(
            changed[1].get("from").is_none(),
            "nothing replaced it before: {replaced}"
        );
    }

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
            assert!(
                mail.get("messages").is_none(),
                "the {which} post carried an envelope for mail nobody had taken: {post}"
            );
            assert!(
                mail["leftovers"].get("ids").is_none(),
                "the {which} post listed the leftovers it counted: {post}"
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

    /// **A box's leftovers and its superseded mail ride a post as counts, never as
    /// lists, however many posts are made.** `otto` holds one message read and not
    /// finished, and one that an earlier message replaced before anybody read it. Twenty posts in a row each say how many there are and which call
    /// lists them, and none carries an id, an envelope or the list of superseded
    /// mail. **The positive is in the same case**: the counts are there, and the
    /// read still lists both, so what a post stopped repeating was not lost.
    /// `leftovers`, `superseded` and `how_to_read` are keys nothing outside this
    /// process declares, so the literals are pinned here.
    #[tokio::test]
    async fn twenty_posts_carry_the_counts_of_leftover_and_superseded_mail_and_no_list() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;
        let first = send(&jojobot, "otto", "epsilon", "first thing waiting").await;
        let first = first["id"].as_str().expect("an id").to_string();
        let second = replacing(&jojobot, "otto", "epsilon", "second thing waiting", &first).await;
        let second = second["id"].as_str().expect("an id").to_string();
        // otto reads: the replacement is handed over and stays owed, the replaced
        // message is withheld and stays in `new`.
        let read = reading(&jojobot, "otto").await;
        assert_eq!(ids_of(&read), vec![second.clone()], "{read}");
        assert_eq!(
            read["superseded"]["messages"][0]["id"],
            first.as_str(),
            "{read}"
        );

        for post in 1..=20 {
            let receipt = send(&jojobot, "epsilon", "otto", &format!("post {post}")).await;
            let mail = &receipt["your_mail"];
            assert_eq!(mail["leftovers"]["count"], 1, "post {post}: {receipt}");
            assert_eq!(mail["superseded"]["count"], 1, "post {post}: {receipt}");
            for held in ["leftovers", "superseded"] {
                assert!(
                    mail[held]["how_to_read"]
                        .as_str()
                        .is_some_and(|how| how.contains("read_mailbox")),
                    "post {post}: the {held} count names the call that lists them: {receipt}"
                );
            }
            assert!(
                mail["leftovers"].get("ids").is_none(),
                "post {post}: {receipt}"
            );
            assert!(
                mail["superseded"].get("messages").is_none(),
                "post {post}: {receipt}"
            );
            for envelope in ["messages", "mailbox", "new_only"] {
                assert!(
                    mail.get(envelope).is_none(),
                    "post {post}: no fresh mail, so no {envelope}: {receipt}"
                );
            }
            let shipped = mail.to_string();
            for id in [&first, &second] {
                assert!(
                    !shipped.contains(&format!("\"{id}\"")),
                    "post {post} shipped the id {id}: {shipped}"
                );
            }
        }

        // What a post stopped repeating is still there to ask for.
        let again = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(false),
                    sid: Some(as_bot(&jojobot, "otto")),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(ids_of(&again), vec![second.clone()], "{again}");
        assert_eq!(
            again["superseded"]["messages"][0]["id"],
            first.as_str(),
            "{again}"
        );
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

        let size = crate::answer::served_len(&posted);
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
                        history: None,
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
            let size = crate::answer::served_len(&posted);
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

    /// **The room does not move with the moment the message is sent.** A
    /// timestamp prints without its trailing zeros, so two moments a second
    /// apart can print nine characters apart. If the check read the real one, a
    /// body at the limit would land on one call and be refused on the next.
    #[test]
    fn the_room_for_a_body_does_not_depend_on_how_the_moment_prints() {
        let message = |sent_at: &str| NewMessage {
            mailbox: mailbox::MailboxName("otto".into()),
            body: "x".repeat(40_000),
            subject: None,
            sender: "bot:epsilon".into(),
            sent_at: sent_at.parse().expect("a moment"),
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        };
        let short = message("2026-10-08T15:30:12Z");
        let long = message("2026-10-08T15:30:12.123456789Z");
        assert_ne!(
            short.sent_at.to_string().len(),
            long.sent_at.to_string().len(),
            "the two moments must print at different widths for this to prove anything"
        );
        assert!(body_room_beyond(&short).is_some(), "40,000 does not fit");
        assert_eq!(body_room_beyond(&short), body_room_beyond(&long));
    }

    /// A post of `body_len` characters into `otto`'s box from `epsilon`, on a
    /// fresh handler, read back as the answer the verb served.
    async fn posting_a_body(body_len: usize) -> (Jojobot, serde_json::Value) {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;
        let result = jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "otto".into(),
                sid: as_bot(&jojobot, "epsilon"),
                subject: None,
                body: "x".repeat(body_len),
                in_reply_to: None,
                supersedes: None,
            }))
            .await
            .expect("post_message call ok");
        (jojobot, json_of(&result))
    }

    /// The first body length a post refuses, found by bisection between a length
    /// that lands and one that does not.
    async fn first_refused_body() -> usize {
        let (mut fits, mut refused) = (100usize, 60_000usize);
        let (_, top) = posting_a_body(refused).await;
        assert_eq!(
            top["status"], "blocked",
            "a body of {refused} characters was not refused: {top}"
        );
        while refused - fits > 1 {
            let mid = (fits + refused) / 2;
            if posting_a_body(mid).await.1["status"] == "blocked" {
                refused = mid;
            } else {
                fits = mid;
            }
        }
        refused
    }

    /// **A message too big to be delivered whole is refused, with nothing
    /// written, and the answer names the way to send it.** A body that is just
    /// under the limit lands and is handed to its reader whole; one character
    /// more is refused. Nobody could receive the second whole, and a message a
    /// reader cannot receive whole is not delivered at all in any sense that
    /// matters to the sender.
    #[tokio::test]
    async fn a_body_too_big_to_be_delivered_whole_is_refused_and_one_under_it_is_delivered_whole() {
        let refused_at = first_refused_body().await;
        assert!(
            (20_000..28_000).contains(&refused_at),
            "the limit is the ceiling less the envelope, not a number of its own: {refused_at}"
        );

        let (jojobot, under) = posting_a_body(refused_at - 1).await;
        assert_ne!(under["status"], "blocked", "{under}");
        let reader = as_bot(&jojobot, "otto");
        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader),
                }))
                .await
                .expect("read_mailbox ok"),
        );
        let message = &delivery["messages"][0];
        assert_eq!(
            message["body"].as_str().map(str::len),
            Some(refused_at - 1),
            "the largest body that lands is handed over whole: {delivery}"
        );
        assert!(message.get("body_elided").is_none(), "{delivery}");
        assert!(delivery.get("not_shown").is_none(), "{delivery}");

        let (jojobot, over) = posting_a_body(refused_at).await;
        assert_eq!(over["status"], "blocked", "{over}");
        assert_eq!(over["wrote"], false, "{over}");
        assert!(
            over.to_string().to_lowercase().contains("split"),
            "the refusal names the way to send it: {over}"
        );
        let counted = counts(&jojobot, "otto").await;
        assert_eq!(
            counted["counts"]["total"], 0,
            "nothing was written: {counted}"
        );
    }

    /// **A box that already holds leftovers is handed the largest body whole.**
    /// A read names every leftover by id beside the new message, and that block
    /// shares the answer ceiling with it. The check once measured the message
    /// alone, so in a box with leftovers the largest body it let through arrived
    /// with its body left out.
    #[tokio::test]
    async fn the_largest_body_is_delivered_whole_to_a_box_that_holds_leftovers() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "otto").await;
        make_box(&jojobot, "epsilon").await;
        for n in 0..LEFTOVER_IDS_ROOM {
            send(&jojobot, "otto", "epsilon", &format!("earlier {n}")).await;
        }
        let reader = as_bot(&jojobot, "otto");
        let read = |sid: String| {
            let jojobot = &jojobot;
            async move {
                json_of(
                    &jojobot
                        .read_mailbox(Parameters(ReadMailboxArgs {
                            counts_only: None,
                            new_only: None,
                            sid: Some(sid),
                        }))
                        .await
                        .expect("read_mailbox ok"),
                )
            }
        };
        // The first read hands the earlier mail over; it is owed from then on.
        read(reader.clone()).await;

        // The largest body is the one the refusal names, asked in the state the
        // post will be made in: a sender that has posted before is not the
        // sender of a first post, and the two differ in what the message carries.
        let over = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    request_id: None,
                    to: "otto".into(),
                    sid: as_bot(&jojobot, "epsilon"),
                    subject: None,
                    body: "x".repeat(40_000),
                    in_reply_to: None,
                    supersedes: None,
                }))
                .await
                .expect("post_message call ok"),
        );
        let said = over["how_to_proceed"].as_str().expect("a way forward");
        let room: usize = said
            .split("about ")
            .nth(1)
            .and_then(|rest| {
                rest.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .unwrap_or_else(|| panic!("the refusal names its room: {over}"));
        send(&jojobot, "otto", "epsilon", &"x".repeat(room)).await;
        let delivery = read(reader).await;
        assert_eq!(
            delivery["leftovers"]["count"], LEFTOVER_IDS_ROOM,
            "the box holds the leftovers this case is about: {delivery}"
        );
        let message = &delivery["messages"][0];
        assert_eq!(
            message["body"].as_str().map(str::len),
            Some(room),
            "the largest body that lands is handed over whole: {delivery}"
        );
        assert!(message.get("body_elided").is_none(), "{delivery}");
        assert!(delivery.get("not_shown").is_none(), "{delivery}");
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
                    request_id: None,
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "the kiln reached temperature".into(),
                    subject: None,
                    in_reply_to: None,
                    supersedes: None,
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
                request_id: None,
                to: "epsilo".into(),
                sid: as_bot(&jojobot, "epsilon"),
                body: "the shipment landed".into(),
                subject: None,
                in_reply_to: None,
                supersedes: None,
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
                    request_id: None,
                    to: "inbox".into(),
                    sid: "  ".into(),
                    body: "the shipment landed".into(),
                    subject: None,
                    in_reply_to: None,
                    supersedes: None,
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
                    request_id: None,
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: long.clone(),
                    subject: Some("the crate count".into()),
                    in_reply_to: None,
                    supersedes: None,
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
                    request_id: None,
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "the kiln slice is done".into(),
                    subject: None,
                    in_reply_to: Some(original_id.clone()),
                    supersedes: None,
                }))
                .await
                .expect("post ok"),
        );
        assert_eq!(reply["in_reply_to"], original_id.as_str());

        // …and it rides on every verb that renders a message.
        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
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
                    request_id: None,
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "answering something nobody said".into(),
                    subject: None,
                    in_reply_to: Some("9999".into()),
                    supersedes: None,
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
                    request_id: None,
                    to: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "answering nothing in particular".into(),
                    subject: None,
                    in_reply_to: Some("   ".into()),
                    supersedes: None,
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
                    request_id: None,
                    to: "dev-two".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "reporting in".into(),
                    subject: None,
                    in_reply_to: None,
                    supersedes: None,
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
                    request_id: None,
                    to: "nobody-here".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "reporting in".into(),
                    subject: None,
                    in_reply_to: None,
                    supersedes: None,
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
        // list_sent returns no body for mail to a person, so the receipt must not
        // point at it; the receipt for mail to a bot does.
        assert!(
            !first["how_to_read"]
                .as_str()
                .expect("a pointer")
                .contains("include_bodies"),
            "{first}"
        );
        make_box(&jojobot, "pm").await;
        let to_a_bot = send(&jojobot, "pm", "epsilon", "an ordinary report").await;
        assert!(
            to_a_bot["how_to_read"]
                .as_str()
                .expect("a pointer")
                .contains("include_bodies"),
            "{to_a_bot}"
        );
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
                    request_id: None,
                    to: "person:ned-flanders".into(),
                    sid: as_bot(&jojobot, "epsilon"),
                    subject: None,
                    body: "for somebody else".into(),
                    in_reply_to: None,
                    supersedes: None,
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
                    request_id: None,
                    to: "person:milhouse".into(),
                    sid: as_bot(&jojobot, "epsilon"),
                    subject: None,
                    body: "for the operator".into(),
                    in_reply_to: None,
                    supersedes: None,
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
                sets: Some([("reports_to".to_string(), "bot:epsilon".to_string())].into()),
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
                    request_id: None,
                    to: "person:milhouse".into(),
                    sid: as_bot(&jojobot, "epsilon"),
                    subject: None,
                    body: "to the old operator".into(),
                    in_reply_to: None,
                    supersedes: None,
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
                request_id: None,
                to: "bot:gamma".into(),
                body: "for the operator".into(),
                in_reply_to: None,
                subject: None,
                sid: sender.clone(),
                supersedes: None,
            }))
        };

        // The positive first: bot:gamma is an ordinary addressee and it lands.
        let landed = json_of(&post().await.expect("post ok"));
        assert_ne!(landed["status"], "blocked", "{landed}");

        // The instance's record now names that bot as operator.
        crate::memory::testing::capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("operator".to_string(), "bot:gamma".to_string())].into()),
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
                request_id: None,
                to: to.into(),
                body: "hello".into(),
                in_reply_to: None,
                subject: None,
                sid: sid.into(),
                supersedes: None,
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
            sets: Some([("operator".to_string(), handle.to_string())].into()),
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
                sets: Some([("reports_to".to_string(), "bot:epsilon".to_string())].into()),
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

    /// Post `body` to `to` as `from`, replacing message `target`, and return the
    /// answer as it came: a refusal is an answer the cases below read.
    async fn replacing(
        jojobot: &Jojobot,
        to: &str,
        from: &str,
        body: &str,
        target: &str,
    ) -> serde_json::Value {
        json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    request_id: None,
                    to: to.into(),
                    sid: as_bot(jojobot, from),
                    subject: None,
                    body: body.into(),
                    in_reply_to: None,
                    supersedes: Some(target.into()),
                }))
                .await
                .expect("post_message call ok"),
        )
    }

    /// `bot` opens its box.
    async fn reading(jojobot: &Jojobot, bot: &str) -> serde_json::Value {
        json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(as_bot(jojobot, bot)),
                }))
                .await
                .expect("read ok"),
        )
    }

    /// The ids of the messages a delivery carries, in order.
    fn ids_of(delivery: &serde_json::Value) -> Vec<String> {
        delivery["messages"]
            .as_array()
            .expect("a list of messages")
            .iter()
            .map(|m| m["id"].as_str().expect("an id").to_string())
            .collect()
    }

    /// **A replaced message nobody read is not delivered, and the delivery names
    /// it apart.** gamma sends A, then B replacing A, to delta. delta's read
    /// carries B, lists A as replaced by B, and the counts account for both.
    ///
    /// The control is a pair without `supersedes`: both arrive and nothing is
    /// listed apart, so the case does not hold on a build that withholds every
    /// second message. `superseded` and `superseded_by` are keys nothing outside
    /// this process declares, so the literals are pinned here.
    #[tokio::test]
    async fn a_replaced_message_nobody_read_is_listed_apart_and_the_counts_account_for_it() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "gamma").await;
        make_box(&jojobot, "delta").await;
        let a = send(&jojobot, "delta", "gamma", "the deploy is friday").await;
        let a = a["id"].as_str().expect("an id").to_string();
        let b = replacing(&jojobot, "delta", "gamma", "the deploy is monday", &a).await;
        let b_id = b["id"].as_str().expect("an id").to_string();
        assert_eq!(
            b["supersedes"],
            a.as_str(),
            "the receipt of the replacement names what it replaces: {b}"
        );
        assert!(
            b["postcondition"]
                .as_str()
                .is_some_and(|line| line.contains(a.as_str())),
            "the line that says what now stands names what was replaced: {b}"
        );

        let polled = counts(&jojobot, "delta").await;
        assert_eq!(
            polled["counts"]["new"], 1,
            "one is waiting, as a delivery would hand over: {polled}"
        );
        assert_eq!(polled["counts"]["total"], 2, "{polled}");
        assert_eq!(
            polled["counts"]["superseded"], 1,
            "the other was replaced and is counted apart: {polled}"
        );

        let delivery = reading(&jojobot, "delta").await;
        assert_eq!(ids_of(&delivery), vec![b_id.clone()], "{delivery}");
        assert_eq!(delivery["count"], 1, "{delivery}");
        assert_eq!(delivery["superseded"]["count"], 1, "{delivery}");
        assert_eq!(
            delivery["superseded"]["messages"][0]["id"],
            a.as_str(),
            "{delivery}"
        );
        assert_eq!(
            delivery["superseded"]["messages"][0]["superseded_by"],
            b_id.as_str(),
            "{delivery}"
        );
        assert!(
            delivery["superseded"]["how_to_read"]
                .as_str()
                .is_some_and(|how| how.contains("read_message")),
            "the listing names the call that returns it: {delivery}"
        );
        assert!(
            delivery["messages"][0].get("already_seen").is_none(),
            "nothing was read before, so there is no notice: {delivery}"
        );

        let after = counts(&jojobot, "delta").await;
        assert_eq!(
            after["counts"]["new"], 0,
            "nothing is waiting once the replacement is delivered: {after}"
        );
        assert_eq!(after["counts"]["read"], 1, "{after}");
        assert_eq!(after["counts"]["superseded"], 1, "{after}");

        // The withheld message is readable on request and says what replaced it.
        let asked = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: a.clone(),
                    sid: Some(as_bot(&jojobot, "delta")),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(asked["body"], "the deploy is friday", "{asked}");
        assert_eq!(asked["superseded_by"], b_id.as_str(), "{asked}");

        // The control.
        make_box(&jojobot, "epsilon").await;
        send(&jojobot, "epsilon", "gamma", "first, no link").await;
        send(&jojobot, "epsilon", "gamma", "second, no link").await;
        let plain = reading(&jojobot, "epsilon").await;
        assert_eq!(plain["count"], 2, "without supersedes both arrive: {plain}");
        assert!(plain.get("superseded").is_none(), "{plain}");
        let polled = counts(&jojobot, "epsilon").await;
        assert!(polled["counts"].get("superseded").is_none(), "{polled}");
    }

    /// **A replacement of a message the reader already has says so before its
    /// body.** delta reads A; gamma then sends B replacing it. delta's next read
    /// carries B with `already_seen` naming A, ahead of `body` in the serialized
    /// answer, and lists nothing apart.
    #[tokio::test]
    async fn a_replacement_of_a_message_already_read_says_so_before_its_body() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "gamma").await;
        make_box(&jojobot, "delta").await;
        let a = send(&jojobot, "delta", "gamma", "the deploy is friday").await;
        let a = a["id"].as_str().expect("an id").to_string();
        reading(&jojobot, "delta").await;
        let b = replacing(&jojobot, "delta", "gamma", "the deploy is monday", &a).await;
        let b_id = b["id"].as_str().expect("an id").to_string();

        let delivery = reading(&jojobot, "delta").await;
        assert_eq!(ids_of(&delivery), vec![b_id], "{delivery}");
        let notice = &delivery["messages"][0]["already_seen"];
        assert_eq!(notice["id"], a.as_str(), "{delivery}");
        assert_eq!(notice["state"], "read", "{delivery}");
        assert!(delivery.get("superseded").is_none(), "{delivery}");
        assert_eq!(
            delivery["leftovers"]["ids"][0],
            a.as_str(),
            "the original is still owed: {delivery}"
        );
        let text = delivery.to_string();
        let notice_at = text.find("\"already_seen\"").expect("the notice is served");
        let body_at = text.find("\"body\"").expect("the body is served");
        assert!(
            notice_at < body_at,
            "the notice comes before the body: {text}"
        );
    }

    /// **The delivery that rides on a post carries the same two answers.** A
    /// reader who posts is handed the replacement and told how many messages were
    /// withheld and which call lists them; the list itself is the read's.
    #[tokio::test]
    async fn a_post_delivers_a_replacement_and_lists_the_replaced_original_apart() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "gamma").await;
        make_box(&jojobot, "delta").await;
        let a = send(&jojobot, "delta", "gamma", "the deploy is friday").await;
        let a = a["id"].as_str().expect("an id").to_string();
        let b = replacing(&jojobot, "delta", "gamma", "the deploy is monday", &a).await;
        let b_id = b["id"].as_str().expect("an id").to_string();

        let posted = send(&jojobot, "gamma", "delta", "noted").await;
        let mail = &posted["your_mail"];
        assert_eq!(mail["messages"][0]["id"], b_id.as_str(), "{posted}");
        assert_eq!(mail["count"], 1, "{posted}");
        assert_eq!(mail["superseded"]["count"], 1, "{posted}");
        assert!(
            mail["superseded"].get("messages").is_none(),
            "a post counts the withheld mail and does not list it: {posted}"
        );
        let read = reading(&jojobot, "delta").await;
        assert_eq!(
            read["superseded"]["messages"][0]["id"],
            a.as_str(),
            "the read lists what the post counted: {read}"
        );
    }

    /// **Each refusal names its reason, writes nothing, and the allowed call
    /// beside them lands.** Another sender's message, another box, an id that
    /// names nothing, another box through the operator's, and a message already
    /// replaced.
    #[tokio::test]
    async fn a_replacement_is_refused_with_its_reason_and_writes_nothing() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "gamma").await;
        make_box(&jojobot, "delta").await;
        make_box(&jojobot, "epsilon").await;
        let a = send(&jojobot, "delta", "gamma", "the deploy is friday").await;
        let a = a["id"].as_str().expect("an id").to_string();
        let board = |jojobot: &Jojobot| {
            let jojobot = jojobot.clone();
            async move {
                let mut all = Vec::new();
                for name in ["delta", "epsilon"] {
                    all.push(store_counts(&jojobot, &MailboxName(name.into())).await);
                }
                all
            }
        };
        let before = board(&jojobot).await;

        // Another sender's message.
        let theirs = replacing(&jojobot, "delta", "epsilon", "no", &a).await;
        assert_eq!(theirs["status"], "blocked", "{theirs}");
        assert_eq!(theirs["wrote"], false, "{theirs}");
        assert_eq!(theirs["attempted"], a.as_str(), "{theirs}");
        assert_eq!(theirs["fix_by"], "change", "{theirs}");
        assert!(
            theirs["how_to_proceed"]
                .as_str()
                .is_some_and(|how| how.contains("in_reply_to")),
            "it names the way forward: {theirs}"
        );

        // Another box.
        let elsewhere = replacing(&jojobot, "epsilon", "gamma", "no", &a).await;
        assert_eq!(elsewhere["status"], "blocked", "{elsewhere}");
        let said = elsewhere["how_to_proceed"].as_str().expect("a way forward");
        assert!(
            said.contains("delta") && said.contains("epsilon"),
            "it names both boxes: {said}"
        );

        // An id that names nothing.
        let nothing = replacing(&jojobot, "delta", "gamma", "no", "zzzzzz").await;
        assert_eq!(nothing["status"], "blocked", "{nothing}");
        assert_eq!(nothing["attempted"], "zzzzzz", "{nothing}");

        // The operator's box takes a replacement like any box, so a message from
        // another box is refused there for the same reason.
        name_the_operator(&jojobot, "milhouse").await;
        let operator = replacing(&jojobot, "person:milhouse", "gamma", "no", &a).await;
        assert_eq!(operator["status"], "blocked", "{operator}");
        assert!(
            operator["how_to_proceed"]
                .as_str()
                .is_some_and(|how| how.contains("delta")),
            "{operator}"
        );

        assert_eq!(before, board(&jojobot).await, "no refusal wrote anything");

        // The positive the refusals depend on, then the second replacement.
        let b = replacing(&jojobot, "delta", "gamma", "the deploy is monday", &a).await;
        assert_ne!(b["status"], "blocked", "the allowed call lands: {b}");
        let b_id = b["id"].as_str().expect("an id").to_string();
        let again = replacing(&jojobot, "delta", "gamma", "the deploy is tuesday", &a).await;
        assert_eq!(again["status"], "blocked", "{again}");
        assert!(
            again["how_to_proceed"]
                .as_str()
                .is_some_and(|how| how.contains(&b_id)),
            "it names the newest to replace instead: {again}"
        );
    }

    /// **The sender sees what replaced a message in `list_sent`.**
    #[tokio::test]
    async fn the_senders_own_view_shows_what_replaced_a_message() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "gamma").await;
        make_box(&jojobot, "delta").await;
        let a = send(&jojobot, "delta", "gamma", "the deploy is friday").await;
        let a = a["id"].as_str().expect("an id").to_string();
        let b = replacing(&jojobot, "delta", "gamma", "the deploy is monday", &a).await;
        let b_id = b["id"].as_str().expect("an id").to_string();

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    offset: None,
                    sender: None,
                    to: None,
                    include_bodies: None,
                    sid: Some(as_bot(&jojobot, "gamma")),
                }))
                .await
                .expect("list_sent ok"),
        );
        let listed = sent["messages"].as_array().expect("a list");
        let of = |id: &str| {
            listed
                .iter()
                .find(|m| m["id"] == id)
                .expect("listed")
                .clone()
        };
        assert_eq!(of(&a)["superseded_by"], b_id.as_str(), "{sent}");
        assert_eq!(of(&b_id)["supersedes"], a.as_str(), "{sent}");
        assert!(of(&b_id).get("superseded_by").is_none(), "{sent}");
    }

    /// **`retract` given a message id refuses and names `supersedes`.** A sent
    /// message is not taken back; the message stays exactly as it was. An id
    /// that names no message keeps the refusal it always had.
    #[tokio::test]
    async fn retracting_a_message_id_names_supersedes_as_the_way_to_withdraw_it() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "gamma").await;
        make_box(&jojobot, "delta").await;
        let a = send(&jojobot, "delta", "gamma", "the deploy is friday").await;
        let a = a["id"].as_str().expect("an id").to_string();

        let refused = json_of(
            &jojobot
                .retract(Parameters(crate::memory::retract::RetractArgs {
                    request_id: None,
                    address: a.clone(),
                    reason: None,
                    recorded_at: None,
                    sid: Some(as_bot(&jojobot, "gamma")),
                }))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
        let said = refused["how_to_proceed"].as_str().expect("a way forward");
        assert!(
            said.contains("post_message") && said.contains("supersedes"),
            "it names the verb and the argument: {said}"
        );
        let polled = counts(&jojobot, "delta").await;
        assert_eq!(
            polled["counts"]["new"], 1,
            "the message is untouched: {polled}"
        );

        let nonsense = jojobot
            .retract(Parameters(crate::memory::retract::RetractArgs {
                request_id: None,
                address: "zzzzzz".into(),
                reason: None,
                recorded_at: None,
                sid: Some(as_bot(&jojobot, "gamma")),
            }))
            .await;
        assert!(
            nonsense.is_err(),
            "a string that is neither an address nor a message keeps its old refusal"
        );
    }

    /// **The sentence that teaches `supersedes` is in the description a sender
    /// reads before calling.** The word is an identifier, so it is pinned here.
    #[test]
    fn the_description_teaches_supersedes() {
        let tools = Jojobot::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == "post_message")
            .expect("post_message is a tool");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("supersedes"), "{description}");
    }
}

//! `read_mailbox` — Take delivery of everything unprocessed in the caller's own box.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `read_mailbox`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadMailboxArgs {
    /// **Count what is waiting and take delivery of none of it.** Off by
    /// default: this verb delivers, and a caller that reaches for a delivery
    /// verb wants the delivery.
    ///
    /// Pass `true` to poll. You get the per-state counts of your own box and
    /// its unreadable report, nothing moves out of `new`, and nothing becomes
    /// yours to finish — so a poll that finds an empty box costs nothing and
    /// owes nothing. `new_only` has nothing to say here: no bodies are shipped
    /// either way.
    #[serde(default)]
    pub(crate) counts_only: Option<bool>,
    /// Ship bodies only for messages nobody has taken yet — **the default**.
    /// Leftovers, the ones flagged `seen_before`, still come back, still
    /// counted, still owed; only their bodies are left out, and each says so.
    ///
    /// Pass `false` to get those bodies back — the read a consumer makes when
    /// it is recovering from a crash and no longer holds what it was given.
    #[serde(default)]
    pub(crate) new_only: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Which mailbox gate stopped a write — because the way out of each is
/// different, and one copy-pasted paragraph fits neither.
/// Why a read has no ONE box to open. Each state is a different next move —
/// one generic miss would be advice that fits none of them.
enum NoBox {
    /// No handle, so no identity, so no box.
    Anonymous,
    /// A world that is down. jojobot does not know, which is not the same as
    /// "you own none" and must never be rendered as it.
    Unknowable,
    /// A bot with no box. **Not a state a bot reaches by any route** — a box
    /// opens with its bot — so this is a broken identity rather than an
    /// incomplete one, and it takes a person rather than a verb.
    Broken,
    /// A bot with more than one box. One box per bot is settled, so this is
    /// damage too — and the opposite repair from [`NoBox::Broken`]: no boot
    /// heals it, because jojobot cannot know which of them is the real one.
    Several(Vec<mailbox::MailboxName>),
    /// **The box this handle owns is a person's.** A bot reads the box it
    /// owns, and a person's box is read by nobody who speaks through this
    /// surface, so a handle bound to a person has no box here. Nothing on the
    /// surface binds one, and the refusal holds if something does.
    Private,
}

/// The refusal a read gets when there is no box behind its handle.
fn no_box_for(attempted: &str, why: NoBox) -> CallToolResult {
    // **Which word the refusal wears is decided by why there is no box.**
    // Damage is a person's. A world that cannot be reached has no word yet: it
    // reads like a transient failure and may be damage, and which it is has
    // not been ruled on.
    let is_damage = matches!(why, NoBox::Several(_));
    let not_ruled_on = matches!(why, NoBox::Unknowable);
    let how_to_proceed = match why {
        NoBox::Anonymous => {
            format!(
                "Nothing was delivered. This call carried no `sid`, and a read opens the box of \
                 whoever is asking — so jojobot has nobody to open one for. {ROUTE_TO_A_SID} \
                 Then pass the handle on every call. To leave mail in somebody else's box you \
                 do not need one of your own: post_message leaves mail in a box without reading \
                 that box."
            )
        }
        NoBox::Unknowable => {
            "Nothing was delivered, and nothing is wrong with your call. Which box you drain is \
             stated on the box itself, and the mailbox world is not reachable right now — so \
             jojobot cannot say whose box this is rather than saying you have none. Try again; \
             if it persists a person has to look."
                .to_string()
        }
        NoBox::Broken => format!(
            "Nothing was delivered, and nothing was created. '{attempted}' is a bot with no \
             mailbox, and that should not be possible: a box opens with the bot that owns it, so \
             an identity without one was interrupted mid-creation or predates the rule. \
             BOOT AGAIN through start_here and jojobot will open it — the repair needs no verb \
             of yours and no person, because the owner is known and the name is its handle. Tell \
             the operator afterwards: mail sent to you before the repair was refused as an \
             unknown box and was never stored. Posting into other boxes still works and needs \
             none of this: post_message leaves mail in a box without reading that box."
        ),
        NoBox::Private => format!(
            "Nothing was delivered, and nothing moved. '{attempted}' owns a person's mailbox, and \
             no bot reads one: a bot reads the box it owns, and this handle is not a bot's. \
             Mail written to a person is read by that person alone, on a surface that is not \
             this one."
        ),
        NoBox::Several(boxes) => format!(
            "Nothing was delivered, and nothing moved out of new. '{attempted}' owns more than \
             one mailbox ({}), and one bot has exactly one. This is damage rather than anything \
             you did: draining one of them would hand you half your mail and report it as all of \
             it, so jojobot will not choose. Report it — it needs a person, and no boot repairs \
             it, because which box is the real one is not something jojobot can work out. Posting \
             into other boxes still works and needs none of this: post_message leaves mail in a \
             box without reading that box.",
            boxes
                .iter()
                .map(|b| b.as_str().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ),
    };
    let how_to_proceed = if is_damage {
        WayForward::person(how_to_proceed)
    } else if not_ruled_on {
        WayForward::mailbox_store_failure(how_to_proceed)
    } else {
        WayForward::from(how_to_proceed)
    };
    let mut body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
    });
    how_to_proceed.write_into(&mut body);
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

impl Jojobot {
    /// The boxes a caller drains — the ones whose state is theirs to see.
    ///
    /// **Whose box a read opens — resolved from the handle, never named.**
    ///
    /// Reading IS delivery: a name in the caller's hand is a way to move
    /// somebody else's mail out of `new` and make it theirs-no-longer. The
    /// own-box norm was stated in the essay in the strongest words available
    /// and was still only advice for as long as the parameter sat there. The
    /// `sid` already says whose box it is, so the parameter is gone and the
    /// norm is structural.
    ///
    /// **Posting keeps its name, deliberately.** `post_message` reaches
    /// somebody else's box and leaves mail there without reading it, which is
    /// exactly the shape of a request — and is the way forward this refusal
    /// points at. The only box it takes delivery of is the sender's own. The
    /// asymmetry is the design.
    ///
    /// Four ways to have no one box, and they are not one answer: the caller
    /// has no identity, jojobot cannot read who owns what, the bot has no box
    /// at all, or it holds more than one. A bot can never claim a box nobody
    /// has opened, so that is not a fifth case.
    ///
    /// **More than one is damage, and it refuses rather than choosing.** One
    /// box per bot is settled everywhere else on this surface; taking the
    /// first match here would drain one box, leave the other's mail waiting,
    /// and report the delivery as whole.
    ///
    /// The whole record, not just its name. Counting needs the box's
    /// per-state counts and its unreadable report, and both are already in
    /// the listing this read walks — fetching the name here and the counts
    /// again a moment later would be two reads of one world that can
    /// disagree with each other.
    pub(crate) async fn my_box(&self, sid: Option<&str>) -> Result<Mailbox, CallToolResult> {
        let caller = match self.caller(sid) {
            Ok(Some(caller)) => caller,
            Ok(None) => return Err(no_box_for("", NoBox::Anonymous)),
            Err(refused) => return Err(refused),
        };
        // One read, of one world: which box is mine is a lookup by owner
        // over the boxes themselves, not a claim read off this bot's entity
        // record — this path needs only Mailboxes, not Memory.
        //
        // **A world that is down is not an answer of "no".** An outage means
        // jojobot cannot say whose box this is; rendering that as "you have
        // none" would send a caller off to repair a box it already has.
        let boxes = match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => boxes,
            Err(_) => return Err(no_box_for(caller.bot.as_str(), NoBox::Unknowable)),
        };
        // The count is taken over the list already in hand: which of the four
        // answers this is depends on how many boxes name this owner, and
        // asking the board a second time would be two reads that can disagree.
        let mut owned: Vec<Mailbox> = boxes
            .into_iter()
            .filter(|b| b.owner == caller.bot)
            .collect();
        match owned.len() {
            // **A person's box is never a caller's own.** The rule that a bot
            // reads the box it owns assumes a bot owns it.
            1 if owned[0].is_private() => Err(no_box_for(caller.bot.as_str(), NoBox::Private)),
            1 => Ok(owned.remove(0)),
            0 => Err(no_box_for(caller.bot.as_str(), NoBox::Broken)),
            _ => Err(no_box_for(
                caller.bot.as_str(),
                NoBox::Several(owned.into_iter().map(|b| b.name).collect()),
            )),
        }
    }
}

/// Take delivery of everything unprocessed in the caller's own box.
/// Take delivery of everything unprocessed in a box.
#[tool_router(router = read_mailbox_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Take delivery of everything unprocessed in YOUR OWN mailbox, oldest \
                       first, moving each message from `new` to `read`. There is no peek: \
                       reading IS taking delivery. WHICH BOX IS NOT AN ARGUMENT — the `sid` you \
                       pass says which bot is asking, and a bot reads the box it owns, full \
                       stop. Reading somebody else's would move their mail out of `new` and \
                       make it no longer waiting for them; to reach another box, post_message \
                       writes into it without reading it, which is the shape of a request. No \
                       box to open comes back status: blocked, saying which kind of nothing it \
                       found — no sid, no claim, or a claim nobody has opened — and delivers \
                       nothing. Messages a previous read already handed over are leftovers from \
                       an interrupted earlier read, not fresh mail: they are named under \
                       `leftovers` (a count and their ids) and not shipped again. A message somebody else finished while this delivery was \
                       in flight is left out, so a delivery can be smaller than counts you saw a \
                       moment ago. Act on what you receive, then call \
                       mark_processed for each. Draining a whole box makes every message in it \
                       yours to finish — use read_message when you want only one. ONLY CHECKING \
                       WHETHER ANYTHING IS WAITING? Call this with counts_only: true — you get \
                       your box's per-state counts and its unreadable report, NOTHING moves out \
                       of new and nothing becomes yours to finish, so a poll that finds an empty \
                       box costs nothing and owes nothing. BY DEFAULT you get the messages nobody has taken yet, \
                       whole: leftovers stay counted and owed, and `leftovers` names them by id \
                       (read_message returns one) — because you were handed them once already. \
                       Pass new_only: false to get every message back whole, flagged seen_before, \
                       which is the read for a consumer recovering from a crash that no longer \
                       holds what it was given. Either \
                       way it changes what is SHIPPED, never what is owed. AN ANSWER STOPS AT \
                       THE ANSWER CEILING AND TAKES EVERY MESSAGE ANYWAY: the oldest bodies come \
                       whole, a body that does not fit is left out and flagged body_elided with \
                       body_bytes and body_head, and a message that does not fit even so is named \
                       by id under not_shown; read_message returns any of them whole. Each message may also \
                       carry `sender_mail_waiting_at_send`, stamped once when it was posted — see \
                       `post_message`'s own description for what it means; `null` there is unknown, \
                       never zero. A message that a DIFFERENT run posted than the one reading it \
                       carries `written_by_other_run`, naming that run and whether it has since \
                       ended (`null` when that could not be read); a message your own run posted \
                       does not carry the key. A POLL ALSO KEEPS YOUR ROLE: reading your own box \
                       with your own sid renews the role claim your session holds, the same as a \
                       write does, so a session that only polls does not go stale. AN EMPTY DELIVERY NAMES WHAT IT \
                       LOOKED THROUGH: it carries `searched`, one line naming the box, what was \
                       left out and the call that reads it."
    )]
    pub(crate) async fn read_mailbox(
        &self,
        Parameters(args): Parameters<ReadMailboxArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mine = match self.my_box(args.sid.as_deref()).await {
            Ok(mine) => mine,
            Err(refused) => return Ok(refused),
        };
        // **A read of one's OWN box, made with one's own sid, renews the
        // caller's role claims** — the same renewal a write makes. A session
        // that only polls writes nothing, so its healthy claim would age out.
        // It runs for the count and the delivery alike and before either, and
        // no other read renews. The store applies it only while this sid still
        // holds the role, so a wrapped session's poll leases nothing.
        if let Ok(Some(caller)) = self.caller(args.sid.as_deref()) {
            self.renew_role_claims(&caller.bot, caller.sid.as_str(), self.clock().now())
                .await;
        }
        // **Counting returns before the delivery path is entered at all.** Not
        // "deliver, then render less" — that would move every message out of
        // `new` and hand the caller work it only wanted to weigh, which is
        // worse than having no way to poll at all. The counts are read off the
        // listing `my_box` already walked; nothing else is called.
        if args.counts_only.unwrap_or(false) {
            return json_result(&counted_json(&mine));
        }
        let name = mine.name;
        let archived = mine.counts.processed;
        let unreadable = mine.quarantined.len();
        // **The safe branch is the default.** The cheap, common read is a poll
        // for news; re-shipping a body its reader already has is the expensive
        // case, and a caller that follows defaults rather than prose must land
        // on the conservative one. Nothing goes silent either way — a leftover
        // is still delivered, counted, flagged and owed.
        let new_only = args.new_only.unwrap_or(true);
        match self
            .mailboxes
            // Somebody opened their box: they were looking.
            .read_mailbox(&name, mailbox::TakenBy::Reading)
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(delivery) => {
                let (delivery, leftovers) = split_leftovers(delivery, new_only);
                let mut rendered = delivery_json(&delivery, new_only);
                note_leftovers(&mut rendered, &leftovers);
                // **An empty delivery names what it looked through.** Nothing
                // waiting and nothing there are the same empty list without it.
                // Added only to a delivery that came back empty.
                if delivery.messages.is_empty() && leftovers.is_empty() {
                    let mut left_out = vec![format!("processed mail ({archived})")];
                    if unreadable > 0 {
                        left_out.push(format!("{unreadable} unreadable cards"));
                    }
                    rendered["searched"] = crate::answer::population_line(
                        &format!("the unprocessed mail in box {}", name.as_str()),
                        &left_out,
                        "read_message takes one message by id, processed ones included; \
                         search with include_mail finds it by its words",
                    )
                    .into();
                }
                let viewer = self
                    .caller(args.sid.as_deref())
                    .ok()
                    .flatten()
                    .and_then(|caller| caller.card);
                self.mark_other_runs(&mut rendered, &delivery, viewer.as_ref())
                    .await;
                // Last, so the sizes it fits against include everything the
                // answer carries.
                fit_delivery(&mut rendered, &delivery, 0);
                json_result(&rendered)
            }
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(mailbox_blocked(
                &attempted,
                &candidates,
                BlockedBox::MustExist("read_mailbox"),
            )),
            mailbox::Guarded::UnknownOwner {
                attempted,
                candidates,
            } => Ok(unknown_owner(&attempted, &candidates)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// A poll costs nothing and owes nothing, and that is the whole point. If
    /// counting moved a message out of `new`, the caller would owe work it
    /// only wanted to weigh — worse than not being able to poll at all. So
    /// this is asserted from the other side: not "the answer has no bodies in
    /// it", but "the box afterwards is untouched, and the real read that
    /// follows still finds fresh mail".
    #[tokio::test]
    async fn counting_takes_no_delivery_and_leaves_nothing_owed() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "dev").await;
        send(&jojobot, "dev", "epsilon", "the shipment landed").await;

        let counted = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: Some(true),
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("counting ok"),
        );
        assert_eq!(counted["mailbox"], "dev");
        assert_eq!(counted["counts"]["new"], 1, "it counted: {counted}");
        assert_eq!(counted["counts"]["read"], 0);
        assert_eq!(counted["counts"]["total"], 1);
        assert_eq!(
            counted["delivered"], false,
            "…and says it delivered nothing, rather than leaving a reader to \
             infer it from an absent list: {counted}"
        );
        assert!(
            counted["messages"].is_null(),
            "a count is not a delivery with the bodies taken out: {counted}"
        );

        // **The proof is on the other side of the call.** A real read now has
        // to find the message still fresh — not a leftover somebody already
        // took delivery of and owes.
        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["count"], 1);
        assert_eq!(
            delivery["messages"][0]["seen_before"], false,
            "counting must not have taken delivery: {delivery}"
        );
        assert_eq!(delivery["messages"][0]["body"], "the shipment landed");
    }

    /// **The other job that had nowhere else to go.** What jojobot cannot read
    /// as a message is counted nowhere, delivered nowhere and processable
    /// nowhere, so the verb that reported it was the only place its existence
    /// showed for the bot that drains the box. Retiring that verb without this
    /// would have made a fault on somebody's own box silently invisible.
    #[tokio::test]
    async fn counting_reports_what_jojobot_cannot_read_on_your_own_box() {
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(boxes.clone());
        let reader = owning(&jojobot, "dev").await;
        send(&jojobot, "dev", "epsilon", "the shipment landed").await;
        boxes.quarantine_by_damage(
            &MailboxName("dev".into()),
            &MessageId("4212".into()),
            "its row cannot be read — a state or a sender has been edited past parsing",
        );

        let counted = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: Some(true),
                    new_only: None,
                    sid: Some(reader),
                }))
                .await
                .expect("counting ok"),
        );
        assert_eq!(counted["quarantined"]["count"], 1, "got {counted}");
        assert_eq!(counted["quarantined"]["ids"][0], "4212");
        assert_eq!(
            counted["counts"]["total"], 1,
            "what cannot be read is not a message and is never counted as one: {counted}"
        );
    }

    /// **Counting is a read, and a read still has to know whose box.** The
    /// refusals are `my_box`'s, unchanged — the point here is that the counting
    /// branch goes through the same gate rather than around it, since a branch
    /// that resolved the box its own way is how the two answers drift apart.
    #[tokio::test]
    async fn counting_with_no_box_to_open_is_refused_exactly_as_a_read_is() {
        let jojobot = mailbox_handler();
        let counted = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: Some(true),
                    new_only: None,
                    sid: None,
                }))
                .await
                .expect("an answer, not a protocol failure"),
        );
        let how = counted["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("start_here"),
            "an anonymous caller is sent to the door that gives it an identity: {how}"
        );
    }

    /// The whole arc through the MCP surface: make a box, leave a message, see
    /// it as new, take delivery, mark it handled.
    #[tokio::test]
    async fn the_mailbox_arc_through_the_handler() {
        let jojobot = mailbox_handler();
        // The box's owner IS its reader: a box belongs to the bot it is named
        // for, so there is no third party to hand the draining to.
        let reader = as_bot(&jojobot, "inbox");
        let created = make_box(&jojobot, "inbox").await;
        assert_eq!(created["name"], "inbox");
        assert_eq!(created["counts"]["new"], 0);

        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        assert_eq!(posted["mailbox"], "inbox");
        assert_eq!(posted["sender"], "bot:epsilon");
        assert_eq!(posted["state"], "new");
        // The author's own body is not shipped back to them — see
        // `a_post_is_receipted_without_shipping_the_body_back`.
        assert!(posted["body"].is_null());
        assert_eq!(posted["body_bytes"], "the shipment landed".len());
        assert!(
            posted["sent_at"].is_string(),
            "a message says when it was sent"
        );
        let id = posted["id"]
            .as_str()
            .expect("a message carries its id")
            .to_string();

        let counted = counts(&jojobot, "inbox").await;
        assert_eq!(counted["mailbox"], "inbox");
        assert_eq!(counted["counts"]["new"], 1);

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
        assert_eq!(delivery["mailbox"], "inbox");
        assert_eq!(delivery["count"], 1);
        assert_eq!(delivery["messages"][0]["id"], id);
        assert_eq!(
            delivery["messages"][0]["state"], "read",
            "delivery moves the column"
        );
        assert_eq!(
            delivery["messages"][0]["seen_before"], false,
            "a first delivery is nobody's leftover"
        );

        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: id.clone(),
                    notes: Some("filed under shipments".into()),
                    sid: None,
                    quarantine: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(processed["state"], "processed");
        assert_eq!(processed["notes"], "filed under shipments");
        assert!(
            processed["subject"].is_null(),
            "a message posted without a subject has none, on every verb that renders it"
        );

        let after = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            after["count"], 0,
            "a processed message is never delivered again"
        );
    }

    /// **A crashed consumer's leftovers are visible as such.** A second read
    /// hands the same message back flagged, rather than as fresh mail.
    #[tokio::test]
    async fn a_redelivered_message_says_it_was_seen_before() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                counts_only: None,
                new_only: None,
                sid: Some(reader.clone()),
            }))
            .await
            .expect("read ok");

        let again = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(again["count"], 0, "nothing is fresh: {again}");
        assert_eq!(again["leftovers"]["count"], 1, "…but it is named: {again}");
        assert!(
            again.get("searched").is_none(),
            "a read with mail owed has not looked through nothing: {again}"
        );

        // The recovery read hands it back whole and flagged.
        let recovery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(false),
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(recovery["count"], 1);
        assert_eq!(recovery["messages"][0]["seen_before"], true);
    }

    /// **The box is not an argument on the read side: the `sid` says whose it
    /// is.** Reading IS delivery, so a name in the caller's hand is a way to
    /// take somebody else's mail out of `new` and make it theirs-no-longer. The
    /// own-box norm was written in the essay in the strongest words available
    /// and was still only advice, because the parameter was right there. It is
    /// structural now.
    #[tokio::test]
    async fn a_read_opens_the_callers_own_box_and_needs_no_name() {
        let jojobot = mailbox_handler();
        let sid = owning(&jojobot, "gamma").await;
        let theirs_sid = owning(&jojobot, "delta").await;
        send(&jojobot, "gamma", "delta", "for gamma").await;
        send(&jojobot, "delta", "sigma", "not for gamma").await;

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(sid),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["mailbox"], "gamma");
        assert_eq!(delivery["count"], 1);
        assert_eq!(delivery["messages"][0]["body"], "for gamma");

        // …and the other box was not touched, which is the whole point: a
        // delivery it never took is still waiting in `new` for its own drainer.
        // Counted by ITS OWN drainer, which is the only caller that can see
        // those counts at all — and the right one to ask, since the question is
        // whether delta's mail is still waiting for delta.
        let theirs = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: Some(true),
                    new_only: None,
                    sid: Some(theirs_sid),
                }))
                .await
                .expect("counting ok"),
        );
        assert_eq!(theirs["mailbox"], "delta");
        assert_eq!(
            theirs["counts"]["new"], 1,
            "gamma's read must not have taken delivery of delta's mail: {theirs}"
        );
    }

    /// **Two ways to have no box, two different next moves.** Folding them into
    /// one miss would be advice that fits neither: a caller with no identity has
    /// to boot, and a bot with no box is BROKEN and needs a person.
    ///
    /// A claim nobody has opened is unreachable: a box opens with its bot, so
    /// a claim cannot outlive the thing it claims. The remaining broken case
    /// survives only as damage, so its advice names the operator, not a verb.
    #[tokio::test]
    async fn a_read_with_no_box_to_open_says_which_kind_of_nothing_it_found() {
        let jojobot = mailbox_handler();

        // 1. No handle at all.
        let anonymous = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: None,
                }))
                .await
                .expect("an answer, not a protocol failure"),
        );
        let how = anonymous["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("start_here"),
            "an anonymous caller is sent to the door that gives it an identity: {how}"
        );

        // 2. A bot with no box. **Written straight to the store**, because the
        //    surface cannot produce one: `add_entity` opens the box with the
        //    bot. This is the shape of damage — an interrupted creation, or a
        //    record predating the rule — and a read of it must still answer.
        jojobot
            .memory
            .add_entity(NewEntity {
                id: EntityId("bot:gamma".into()),
                name: "gamma".into(),
                aliases: Vec::new(),
                source: "user-named".into(),
                crm: None,
                parent: None,
                boot: Default::default(),
                override_token: None,
            })
            .await
            .expect("the store writes it");
        let broken = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(as_bot(&jojobot, "gamma")),
                }))
                .await
                .expect("an answer"),
        );
        let how = broken["how_to_proceed"].as_str().expect("advice");
        // **The way out is the door that heals, not a person.** This advice
        // named the operator until they ruled that jojobot repairs this itself;
        // sending a session to a human for something the next boot fixes is a
        // way forward that costs more than the problem.
        assert!(
            how.contains("start_here"),
            "a bot with no box is damage the boot door repairs: {how}"
        );
        assert!(
            !how.contains("create_mailbox"),
            "…and never a verb that does not exist: {how}"
        );
        assert!(
            jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .is_empty(),
            "and it stayed a report: nothing was minted"
        );
    }

    /// **A bot holding two boxes is damage, and a read says so rather than
    /// picking one.** One box per bot is settled, so draining the first match
    /// would hand over half somebody's mail and report it as all of it — the
    /// caller could not tell a whole delivery from a partial one, and the
    /// second box's mail would sit there with nothing said about it.
    #[tokio::test]
    async fn a_read_by_a_bot_holding_two_boxes_refuses_rather_than_draining_one() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "gamma").await;
        // Mail first: posting to a bot that already holds two boxes is itself
        // refused, so the fixture's own mail has to land while it holds one.
        send(&jojobot, "gamma", "delta", "the shipment landed").await;
        a_second_box(&jojobot, "gamma", "sigma").await;

        let refused = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("an answer, not a protocol failure"),
        );
        assert_eq!(
            refused["fix_by"], "person",
            "damage no verb of the caller's repairs is a person's: {refused}"
        );
        let how = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("gamma") && how.contains("sigma"),
            "the refusal names both boxes, or nobody can go and look: {how}"
        );
        assert!(
            how.contains("person"),
            "…and says it takes a person, because no verb of the caller's repairs it: {how}"
        );
        assert!(
            refused["messages"].is_null(),
            "a refusal is not a delivery with a status on it: {refused}"
        );

        // **The positive the refusal rests on.** There is mail in one of those
        // boxes, still in `new` — so this refused a delivery it could have
        // made, rather than reporting an empty board as damage.
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
            "and the message is still waiting, untaken: {held:?}"
        );
    }

    /// **The safe branch is the DEFAULT, not the documented preference.** A
    /// caller that passes nothing gets the cheap, common read — news whole,
    /// leftovers named but not re-shipped — and pays for the expensive one only
    /// by asking. Prose recommending the cheap option does not help a client
    /// that follows defaults, which is most of them.
    ///
    /// **What makes that safe is that nothing goes silent**, so it is pinned
    /// here rather than left to the description: under the default, a leftover
    /// is still delivered, still counted, still flagged `seen_before`, and
    /// still owed. Only its body is withheld, and it says so.
    #[tokio::test]
    async fn a_read_that_asks_for_nothing_still_hands_over_every_leftover() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "dev").await;
        let held_body = "a long hand-off that stays open until the round closes. ".repeat(40);
        let held = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "dev".into(),
                    sid: as_bot(&jojobot, "delta"),
                    body: held_body.clone(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        let held_id = held["id"].as_str().expect("an id").to_string();

        // Delivered once and deliberately not processed.
        json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        send(&jojobot, "dev", "delta", "and here is the next batch").await;

        // The plain read — no argument, no opinion.
        let plain = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(plain["new_only"], true, "the safe branch is the default");
        assert_eq!(plain["count"], 1, "only the news is shipped: {plain}");
        assert_eq!(
            plain["leftovers"]["count"], 1,
            "the leftover is still counted: {plain}"
        );
        assert_eq!(
            plain["leftovers"]["ids"][0],
            held_id.as_str(),
            "…and named, so it is still owed and still findable: {plain}"
        );
        assert!(
            !plain.to_string().contains("a long hand-off"),
            "…and none of it is shipped again: {plain}"
        );

        let fresh = plain["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] != held_id.as_str())
            .expect("the fresh message");
        assert_eq!(
            fresh["body"], "and here is the next batch",
            "news is what a plain read is for, so news arrives whole: {fresh}"
        );

        // And the expensive read is still there for the caller who asks.
        let whole = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(false),
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        let recovered = whole["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == held_id.as_str())
            .expect("still there");
        assert_eq!(
            recovered["body"],
            held_body.trim(),
            "new_only: false is how a crashed consumer gets the body back: {recovered}"
        );
    }

    /// `new_only` changes what is SHIPPED, never what is owed: the leftover is
    /// still in the delivery, still counted, still flagged, still to be marked
    /// processed. Only its body is left out, and it says so.
    ///
    /// What holds the invariant here is the `.find(...).expect(...)` below, not
    /// the count: `count` is `delivery.messages.len()`, so an implementation
    /// that dropped leftovers from the RENDERED list alone would still report
    /// two. The lookup is what fails.
    #[tokio::test]
    async fn new_only_elides_a_leftover_s_body_and_never_its_existence() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "dev").await;
        let held_body = "a long hand-off that stays open until the round closes. ".repeat(40);
        let held = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "dev".into(),
                    sid: as_bot(&jojobot, "delta"),
                    body: held_body.clone(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        let held_id = held["id"].as_str().expect("an id").to_string();

        // Take delivery once, and deliberately do NOT process it.
        let first = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            first["messages"][0]["body"],
            held_body.trim(),
            "the first read is whole"
        );

        // Fresh mail arrives, and the poll asks for news only.
        send(&jojobot, "dev", "delta", "and here is the next batch").await;
        let poll = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(true),
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(poll["count"], 1, "only the news is shipped: {poll}");
        assert_eq!(poll["new_only"], true);

        assert_eq!(
            poll["leftovers"]["count"], 1,
            "the leftover is STILL counted: {poll}"
        );
        assert_eq!(
            poll["leftovers"]["ids"][0],
            held_id.as_str(),
            "…and named, because what is owed is never silent: {poll}"
        );
        assert!(
            !poll.to_string().contains("a long hand-off"),
            "…and none of its text is shipped again: {poll}"
        );

        let fresh = poll["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] != held_id.as_str())
            .expect("the fresh message");
        assert_eq!(
            fresh["body"], "and here is the next batch",
            "news is the point of the poll, so news arrives whole: {fresh}"
        );

        // And it is still owed: processing it is still the caller's job.
        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: held_id,
                    notes: None,
                    sid: None,
                    quarantine: None,
                }))
                .await
                .expect("mark ok"),
        );
        assert_eq!(processed["state"], "processed");
    }

    /// **The delivery verbs still ship bodies.** The elision is for the caller
    /// who wrote or already read the text; a consumer taking delivery is being
    /// handed something they have never seen, and that is the whole verb.
    #[tokio::test]
    async fn taking_delivery_still_hands_over_the_whole_body() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed at dawn").await;

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
        assert_eq!(
            delivery["messages"][0]["body"],
            "the shipment landed at dawn"
        );
        assert!(
            delivery["messages"][0]["body_elided"].is_null(),
            "nothing was withheld"
        );
    }

    /// **A delivered message names the run that posted it, when that run is
    /// not the one reading now — and says whether that run has since
    /// ended.**
    ///
    /// `dev` posts a note into its own box under one run, then a FRESH run
    /// of `dev` — a different session of the same identity, a new sid with
    /// no card of its own yet — reads the box. The note has to say which
    /// run wrote it, and the value on the wire round-trips: it is read
    /// straight off the stored message, never reconstructed from what the
    /// test happens to know about the posting run.
    ///
    /// **Paired with a read inside the SAME run**, which must carry no such
    /// key at all — not `false`, absent — because a build that always
    /// marked would pass the positive half identically to a correct one.
    #[tokio::test]
    async fn a_message_names_the_run_that_posted_it_when_it_differs_from_the_readers_own() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "dev").await;

        // Run A: dev's first write (a journal entry) materializes its card,
        // so the post right after it carries a real stamp rather than
        // "unknown".
        let run_a = as_bot(&jojobot, "dev");
        jojobot
            .journal(Parameters(JournalArgs {
                entry: "starting work".into(),
                focus: None,
                sid: run_a.clone(),
            }))
            .await
            .expect("journal ok");
        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "dev".into(),
                    sid: run_a.clone(),
                    subject: None,
                    body: "leaving myself a note".into(),
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        assert!(
            posted.get("written_by_other_run").is_none(),
            "posting is not a delivery to anybody: {posted}"
        );
        let id = posted["id"].as_str().expect("an id").to_string();

        // Read inside run A: the same run posted and delivered it.
        let same_run = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(run_a.clone()),
                }))
                .await
                .expect("read ok"),
        );
        let mine = same_run["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == id)
            .expect("the note is there");
        assert!(
            mine.get("written_by_other_run").is_none(),
            "a run reading what it just posted must carry no mark at all: {mine}"
        );

        // A fresh run of the SAME identity — a different session, no card
        // of its own yet.
        let run_b = as_bot(&jojobot, "dev");
        // The message was handed to run A already, so a default read names it and
        // does not ship it; the recovery read ships it, with the run marked.
        let cross_run = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(false),
                    sid: Some(run_b.clone()),
                }))
                .await
                .expect("read ok"),
        );
        let theirs = cross_run["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == id)
            .expect("the note is there");
        let marked = theirs
            .get("written_by_other_run")
            .unwrap_or_else(|| panic!("a different run reading it must be told: {theirs}"));
        assert_eq!(
            marked["ended"], false,
            "run A is still active, never wrapped: {marked}"
        );

        // The positive half: the run named on the wire is exactly the run
        // the store holds against the message.
        let stored = jojobot
            .mailboxes
            .scan_messages()
            .await
            .expect("scan ok")
            .into_iter()
            .find(|m| m.id.as_str() == id)
            .expect("the message is there");
        assert_eq!(
            marked["run"],
            stored
                .posted_by_session
                .expect("this message was stamped with a run"),
            "the stamped run read back equals the run that posted it: {marked}"
        );

        // And the mark survives a second read of the same run — a
        // leftover, not a first delivery.
        let again = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(false),
                    sid: Some(run_b),
                }))
                .await
                .expect("read ok"),
        );
        let still = again["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == id)
            .expect("the note is there");
        assert!(
            still.get("written_by_other_run").is_some(),
            "the mark survives a re-read: {still}"
        );
    }

    /// **Upgrade case: a message with no stamp at all reads as unknown,
    /// never as the reader's own run.** A row written before this column
    /// existed — or one the edge could not resolve — carries
    /// `posted_by_session: None`, and a build that read absence as "mine"
    /// would tell a fresh run it wrote something it never saw.
    #[tokio::test]
    async fn an_unstamped_message_reads_as_unknown_never_as_the_readers_own_run() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "dev").await;
        // Posted straight through the store, bypassing the edge that
        // stamps a run — the shape a row written before this column
        // existed left behind.
        jojobot
            .mailboxes
            .post_message(jojobot_domain::mailbox::NewMessage {
                mailbox: jojobot_domain::mailbox::MailboxName("dev".into()),
                body: "a message with no run recorded".into(),
                subject: None,
                sender: "epsilon".into(),
                sent_at: jiff::Timestamp::now(),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
            })
            .await
            .expect("post ok");

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(reader),
                }))
                .await
                .expect("read ok"),
        );
        let message = &delivery["messages"][0];
        assert!(
            message.get("written_by_other_run").is_none(),
            "a message with no stamp must carry no mark at all — never present-and-false, and \
             never claiming it is the reader's own: {message}"
        );
    }

    /// **No bot's read of its own box ever reaches a person's.** The bot that
    /// wrote to a person and a different bot each read, and count, their own
    /// box: they get their own mail, none of the person's, and the person's
    /// message is left exactly where it was. The positive is each bot's own mail
    /// arriving, so a read that returned nothing at all would not pass; the
    /// person's box is counted from the store, not from the verb.
    #[tokio::test]
    async fn a_bots_read_of_its_own_box_never_reaches_a_persons() {
        let jojobot = mailbox_handler();
        let held = a_persons_box(&jojobot, "milhouse").await;
        let writer = owning(&jojobot, "epsilon").await;
        let other = owning(&jojobot, "sigma").await;
        send(
            &jojobot,
            "person:milhouse",
            "epsilon",
            "the secret figure is 4242",
        )
        .await;
        // A third bot writes both notes, so neither reader's own post takes
        // delivery of the mail the read below is meant to find.
        owning(&jojobot, "delta").await;
        send(&jojobot, "epsilon", "delta", "a note for epsilon").await;
        send(&jojobot, "sigma", "delta", "a note for sigma").await;
        assert_eq!(
            store_counts(&jojobot, &held).await,
            (1, 0, 0),
            "the post landed, counted from the store"
        );

        for (sid, own_note) in [(writer, "note for epsilon"), (other, "note for sigma")] {
            let counted = json_of(
                &jojobot
                    .read_mailbox(Parameters(ReadMailboxArgs {
                        counts_only: Some(true),
                        new_only: None,
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("counting ok"),
            );
            assert_eq!(counted["counts"]["new"], 1, "{counted}");
            let delivered = json_of(
                &jojobot
                    .read_mailbox(Parameters(ReadMailboxArgs {
                        counts_only: None,
                        new_only: None,
                        sid: Some(sid),
                    }))
                    .await
                    .expect("read ok"),
            );
            assert_eq!(delivered["count"], 1, "{delivered}");
            assert!(delivered.to_string().contains(own_note), "{delivered}");
            assert!(!delivered.to_string().contains("4242"), "{delivered}");
        }
        assert_eq!(
            store_counts(&jojobot, &held).await,
            (1, 0, 0),
            "neither read moved the person's message"
        );
    }

    /// **A handle bound to a person never opens a person's box, by a read or by
    /// a post.** No verb produces such a handle once a retype is refused, and a
    /// guard that only holds while nothing else is wrong is not a guard: a
    /// handle minted for the person who owns a private box meets a refusal when
    /// it reads, the post it makes hands it none of that box's mail. (The status
    /// bar rides the served router and not these direct calls, so its own case
    /// is beside it, in the status bar's module.) The message stays where it was, counted
    /// from the store.
    #[tokio::test]
    async fn a_handle_bound_to_a_person_never_opens_a_persons_box() {
        let jojobot = mailbox_handler();
        let held = a_persons_box(&jojobot, "milhouse").await;
        owning(&jojobot, "epsilon").await;
        owning(&jojobot, "sigma").await;
        send(
            &jojobot,
            "person:milhouse",
            "epsilon",
            "the secret figure is 4242",
        )
        .await;
        assert_eq!(store_counts(&jojobot, &held).await, (1, 0, 0));
        // Written straight to the registry: nothing on the surface binds a
        // handle to a person, and the guard has to hold if something does.
        let person = jojobot
            .registry
            .mint(&EntityId("person:milhouse".into()), None)
            .expect("a free handle")
            .as_str()
            .to_string();

        // ── a read, by delivery and by count ────────────────────────────────
        for counts_only in [None, Some(true)] {
            let refused = json_of(
                &jojobot
                    .read_mailbox(Parameters(ReadMailboxArgs {
                        counts_only,
                        new_only: None,
                        sid: Some(person.clone()),
                    }))
                    .await
                    .expect("a refusal is an answer"),
            );
            assert_eq!(refused["status"], "blocked", "{refused}");
            assert!(!refused.to_string().contains("4242"), "{refused}");
        }

        // ── a post that would hand over the sender's own box ────────────────
        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    to: "sigma".into(),
                    sid: person,
                    subject: None,
                    body: "from the person's handle".into(),
                    in_reply_to: None,
                }))
                .await
                .expect("an answer"),
        );
        assert!(!posted.to_string().contains("4242"), "{posted}");
        assert!(
            posted.get("your_mail").is_none(),
            "a person's box is handed to nobody by a post: {posted}"
        );
        assert_eq!(
            store_counts(&jojobot, &held).await,
            (1, 0, 0),
            "neither the read nor the post moved the message"
        );
    }
    /// 🚨 **An empty delivery names what it looked through.** "Nothing waiting"
    /// and "nothing here at all" are the same empty list without it. The line
    /// names the box, says the processed archive was left out, and names the
    /// call that reads it. Added only to a delivery that came back empty.
    #[tokio::test]
    async fn an_empty_delivery_names_the_box_and_the_archive_it_left_out() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "dev").await;
        let sent = send(&jojobot, "dev", "epsilon", "the shipment landed").await;
        jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: sent["id"].as_str().expect("a message id").to_string(),
                notes: None,
                sid: None,
                quarantine: None,
            }))
            .await
            .expect("mark_processed ok");
        let deliver = || ReadMailboxArgs {
            counts_only: None,
            new_only: None,
            sid: Some(reader.clone()),
        };

        let empty = json_of(
            &jojobot
                .read_mailbox(Parameters(deliver()))
                .await
                .expect("read ok"),
        );
        assert_eq!(empty["count"], 0, "{empty}");
        let line = empty["searched"]
            .as_str()
            .expect("an empty delivery names its population");
        assert!(!line.contains('\n'), "one line: {line}");
        assert!(line.contains("dev"), "names the box: {line}");
        assert!(
            line.contains("processed"),
            "names the archive it left out: {line}"
        );
        assert!(
            line.contains("read_message"),
            "names the call that reads it: {line}"
        );

        // The positive: fresh mail is delivered and the answer carries no line.
        send(&jojobot, "dev", "epsilon", "a second shipment").await;
        let full = json_of(
            &jojobot
                .read_mailbox(Parameters(deliver()))
                .await
                .expect("read ok"),
        );
        assert_eq!(full["count"], 1, "{full}");
        assert!(
            full["searched"].is_null(),
            "a delivery needs no line: {full}"
        );
    }
    async fn ask_box(jojobot: &Jojobot, sid: Option<String>) -> Result<CallToolResult, McpError> {
        jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                counts_only: None,
                new_only: None,
                sid,
            }))
            .await
    }

    /// 🚨 **The refusals a read gets when no box stands behind its handle each
    /// wear the word their repair needs.** No `sid` and a bot that has no box
    /// are both a call that has to change (boot, and the boot opens the box). A
    /// board nobody can read is a storage failure, which wears the word that
    /// failure wears (`retry` while the store-failure switch is on).
    #[tokio::test]
    async fn the_refusals_for_no_box_wear_their_words() {
        let jojobot = mailbox_handler();
        // No handle at all: the call has to change.
        let anonymous = ask_box(&jojobot, None).await.expect("an answer");
        assert_fix_by("anonymous read", &anonymous, "change");
        // A bot with no box behind its handle: boot, and the boot opens it.
        let boxless = ask_box(&jojobot, Some(as_bot(&jojobot, "boxless")))
            .await
            .expect("an answer");
        assert_fix_by("a bot with no box", &boxless, "change");

        // The positive: a bot with a box reads, and gets no refusal word.
        let reader = owning(&jojobot, "gamma").await;
        let read = json_of(&ask_box(&jojobot, Some(reader)).await.expect("read ok"));
        assert!(read.get("fix_by").is_none(), "{read}");

        // A board that cannot be read is a storage failure: a retry.
        let down = handler_with_mailboxes_down(std::sync::Arc::new(
            jojobot_domain::memory::testing::InMemoryMemory::booted(),
        ));
        let unreadable = blocked(
            &ask_box(&down, Some(as_bot(&down, "gamma")))
                .await
                .expect("an answer"),
        );
        assert_eq!(
            unreadable["fix_by"].as_str(),
            other_store_failure_word().map(FixBy::as_token),
            "{unreadable}"
        );
        assert!(unreadable["how_to_proceed"].is_string(), "{unreadable}");
    }
    /// 🚨 **A default read names what it already handed over, and does not hand
    /// it over again.** Mail read but not finished stays owed, and every poll
    /// used to ship an envelope for each such message again. Now the delivery
    /// carries the fresh mail and a `leftovers` block: how many, which ids, and
    /// the call that returns them. The read a consumer makes to recover after a
    /// crash, `new_only: false`, still returns every message whole and flagged.
    #[tokio::test]
    async fn a_default_read_names_leftovers_by_id_and_ships_only_the_fresh_envelope() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "inbox").await;
        let first = send(&jojobot, "inbox", "epsilon", "the first shipment").await;
        let second = send(&jojobot, "inbox", "epsilon", "the second shipment").await;
        let read = |new_only: Option<bool>| {
            let reader = reader.clone();
            let jojobot = &jojobot;
            async move {
                json_of(
                    &jojobot
                        .read_mailbox(Parameters(ReadMailboxArgs {
                            counts_only: None,
                            new_only,
                            sid: Some(reader),
                        }))
                        .await
                        .expect("read ok"),
                )
            }
        };
        let taken = read(None).await;
        assert_eq!(taken["count"], 2, "both were fresh the first time: {taken}");
        assert!(
            taken.get("leftovers").is_none(),
            "nothing was left over yet: {taken}"
        );

        let third = send(&jojobot, "inbox", "epsilon", "the third shipment").await;
        let again = read(None).await;
        assert_eq!(
            again["count"], 1,
            "only the fresh message is shipped: {again}"
        );
        assert_eq!(again["messages"][0]["id"], third["id"], "{again}");
        assert_eq!(again["leftovers"]["count"], 2, "{again}");
        let ids: Vec<&str> = again["leftovers"]["ids"]
            .as_array()
            .expect("the leftovers are named by id")
            .iter()
            .filter_map(|id| id.as_str())
            .collect();
        for owed in [&first, &second] {
            assert!(
                ids.contains(&owed["id"].as_str().expect("an id")),
                "{owed} is named: {again}"
            );
        }
        assert!(
            !again.to_string().contains("the first shipment"),
            "no envelope or opening of a leftover is shipped again: {again}"
        );

        // The recovery read still returns every message whole and flagged.
        let recovery = read(Some(false)).await;
        assert_eq!(recovery["count"], 3, "{recovery}");
        assert!(recovery.get("leftovers").is_none(), "{recovery}");
        assert!(
            recovery.to_string().contains("the first shipment"),
            "new_only false hands the bodies back: {recovery}"
        );
    }

    /// **A delivery counts every key it adds when it cuts, at every size of
    /// box.** Cutting adds `not_shown` and `how_to_read` to the answer after
    /// the messages are chosen. A fill that leaves room for the first and not
    /// the second passes the ceiling by the second, but only when the bodies add
    /// up inside that window, so the size of the body is swept in steps finer
    /// than the window.
    #[tokio::test]
    async fn a_delivery_counts_the_keys_it_adds_when_it_cuts_at_every_size_of_box() {
        for body_len in (800..=1_300).step_by(20) {
            let jojobot = mailbox_handler();
            let sid = owning(&jojobot, "dev").await;
            for _ in 0..25 {
                send(&jojobot, "dev", "epsilon", &"x".repeat(body_len)).await;
            }
            let delivery = json_of(
                &jojobot
                    .read_mailbox(Parameters(ReadMailboxArgs {
                        counts_only: None,
                        new_only: None,
                        sid: Some(sid),
                    }))
                    .await
                    .expect("read_mailbox ok"),
            );
            let size = delivery.to_string().chars().count();
            assert!(
                size + crate::answer::STATUS_BAR_ROOM <= jojobot_domain::text::ANSWER_CEILING,
                "bodies of {body_len}: the delivery is {size} characters"
            );
            let carried = delivery["messages"].as_array().expect("messages").len();
            let named = delivery["not_shown"]["count"].as_u64().unwrap_or(0) as usize;
            assert_eq!(carried + named, 25, "bodies of {body_len}: {delivery}");
        }
    }

    /// **A delivery counts its own envelope against the ceiling.** The messages
    /// are sized so that twenty whole bodies come to just under the ceiling and
    /// nineteen leave room for everything else the answer carries; a delivery
    /// that spent the whole ceiling on messages would pass it by the envelope.
    /// Every message is still taken, and the ones left out are named.
    #[tokio::test]
    async fn a_delivery_counts_its_own_envelope_against_the_ceiling() {
        let read = |jojobot: &Jojobot, sid: &str| {
            let sid = sid.to_string();
            let jojobot = jojobot.clone();
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
        let probe = mailbox_handler();
        let sid = owning(&probe, "dev").await;
        send(&probe, "dev", "epsilon", &"x".repeat(500)).await;
        let delivery = read(&probe, &sid).await;
        let probed = delivery["messages"][0].to_string().chars().count() + 1;

        let one = 1_376usize;
        let body = "x".repeat(500 + one - probed);
        let jojobot = mailbox_handler();
        let sid = owning(&jojobot, "dev").await;
        for _ in 0..25 {
            send(&jojobot, "dev", "epsilon", &body).await;
        }
        let delivery = read(&jojobot, &sid).await;
        let size = delivery.to_string().chars().count();
        // The status bar joins the answer after the verb has returned, in the room
        // the verb left for it.
        assert!(
            size + crate::answer::STATUS_BAR_ROOM <= jojobot_domain::text::ANSWER_CEILING,
            "the delivery is {size} characters"
        );
        let carried = delivery["messages"].as_array().expect("messages").len();
        let named = delivery["not_shown"]["count"].as_u64().unwrap_or(0) as usize;
        assert_eq!(carried + named, 25, "every message is carried or named");
        let counts = counts(&jojobot, "dev").await;
        assert_eq!(counts["counts"]["read"], 25, "all taken: {counts}");
        assert_eq!(counts["counts"]["new"], 0, "{counts}");
    }
}

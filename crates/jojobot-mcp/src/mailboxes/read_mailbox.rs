//! `read_mailbox` — Take delivery of everything unprocessed in the caller's own box.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `read_mailbox`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadMailboxArgs {
    /// Ship bodies only for messages nobody has taken yet — **the default**.
    /// Leftovers, the ones flagged `seen_before`, still come back, still
    /// counted, still owed; only their bodies are left out, and each says so.
    ///
    /// Pass `false` to get those bodies back — the read a consumer makes when
    /// it is recovering from a crash and no longer holds what it was given.
    #[serde(default)]
    pub new_only: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Which mailbox gate stopped a write — because the way out of each is
/// different, and one copy-pasted paragraph fits neither.
/// Why a read had no box to open. Three states, three different next moves —
/// one generic miss would be advice that fits none of them.
enum NoBox {
    /// No handle, so no identity, so no box.
    Anonymous,
    /// A world that is down. jojobot does not know, which is not the same as
    /// "you own none" and must never be rendered as it.
    Unknowable,
    /// A bot with no box. **Not a normal state any more** — a box opens with
    /// its bot — so this is a broken identity rather than an incomplete one,
    /// and it takes a person rather than a verb.
    Broken,
}

/// The refusal a read gets when there is no box behind its handle.
fn no_box_for(attempted: &str, why: NoBox) -> CallToolResult {
    let how_to_proceed = match why {
        NoBox::Anonymous => {
            "Nothing was delivered. This call carried no `sid`, and a read opens the box of \
             whoever is asking — so jojobot has nobody to open one for. Call start_here with \
             your bot name to get a handle, then pass it on every call. To leave mail in \
             somebody else's box you do not need one of your own: post_message writes without \
             reading."
                .to_string()
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
             none of this: post_message writes without reading."
        ),
    };
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "how_to_proceed": how_to_proceed,
    });
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
    /// somebody else's box and writes without reading, which is exactly the
    /// shape of a request — and is the way forward this refusal points at. The
    /// asymmetry is the design.
    ///
    /// Three ways to have no box, and they are not one answer: the caller has
    /// no identity, jojobot cannot read who owns what, or the bot has no box at
    /// all. Each needs a different next move — and the last one used to be two,
    /// because a bot could claim a box nobody had opened. It cannot now.
    pub(crate) async fn my_box(&self, sid: Option<&str>) -> Result<MailboxName, CallToolResult> {
        let caller = match self.caller(sid) {
            Ok(Some(caller)) => caller,
            Ok(None) => return Err(no_box_for("", NoBox::Anonymous)),
            Err(refused) => return Err(refused),
        };
        // **One read, of one world.** Which box is mine is a lookup by owner
        // over the boxes themselves — it used to be a claim read off this bot's
        // entity record, so this path needed Memory up as well as Mailboxes and
        // had a "claimed but never opened" answer to give. Owning a box and it
        // existing are the same fact now.
        //
        // **A world that is down is not an answer of "no".** An outage means
        // jojobot cannot say whose box this is; rendering that as "you have
        // none" would send a caller off to repair a box it already has.
        let boxes = match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => boxes,
            Err(_) => return Err(no_box_for(caller.bot.as_str(), NoBox::Unknowable)),
        };
        boxes
            .into_iter()
            .find(|b| b.owner == caller.bot)
            .map(|b| b.name)
            .ok_or_else(|| no_box_for(caller.bot.as_str(), NoBox::Broken))
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
                       nothing. Messages a previous read already handed over come back too, \
                       flagged seen_before: true — leftovers from an interrupted earlier read, \
                       not fresh mail. A message somebody else finished while this delivery was \
                       in flight is left out, so a delivery can be smaller than counts you saw a \
                       moment ago. Act on what you receive, then call \
                       mark_processed for each. Draining a whole box makes every message in it \
                       yours to finish — use read_message when you want only one. ONLY CHECKING \
                       WHETHER ANYTHING IS WAITING? Use list_mailboxes — it reads counts without \
                       taking delivery, so a poll that finds an empty box costs nothing and owes \
                       nothing. BY DEFAULT you get bodies for the messages nobody has taken yet: \
                       leftovers still come back, still counted, still flagged and still owed, \
                       but with their bodies left out (body_elided: true, plus body_bytes and the \
                       opening line) — because you were handed those bodies once already. Pass \
                       new_only: false to get them back, which is the read for a consumer \
                       recovering from a crash that no longer holds what it was given. Either \
                       way it changes what is SHIPPED, never what is owed."
    )]
    pub(crate) async fn read_mailbox(
        &self,
        Parameters(args): Parameters<ReadMailboxArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = match self.my_box(args.sid.as_deref()).await {
            Ok(name) => name,
            Err(refused) => return Ok(refused),
        };
        // **The safe branch is the default.** The cheap, common read is a poll
        // for news; re-shipping a body its reader already has is the expensive
        // case, and a caller that follows defaults rather than prose must land
        // on the conservative one. Nothing goes silent either way — a leftover
        // is still delivered, counted, flagged and owed.
        let new_only = args.new_only.unwrap_or(true);
        match self
            .mailboxes
            .read_mailbox(&name)
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(delivery) => json_result(&delivery_json(&delivery, new_only)),
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

        let listed = drains(&jojobot, "inbox").await;
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["mailboxes"][0]["name"], "inbox");
        assert_eq!(listed["mailboxes"][0]["counts"]["new"], 1);

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
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
                new_only: None,
                sid: Some(reader.clone()),
            }))
            .await
            .expect("read ok");

        let again = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(again["count"], 1);
        assert_eq!(again["messages"][0]["seen_before"], true);
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
        let theirs = json_of(
            &jojobot
                .list_mailboxes(Parameters(ListMailboxesArgs {
                    sid: Some(theirs_sid),
                }))
                .await
                .expect("list ok"),
        );
        let mine = theirs["mailboxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .find(|b| b["name"] == "delta")
            .expect("delta's box");
        assert_eq!(
            mine["counts"]["new"], 1,
            "gamma's read must not have taken delivery of delta's mail: {mine}"
        );
    }

    /// **Two ways to have no box, two different next moves.** Folding them into
    /// one miss would be advice that fits neither: a caller with no identity has
    /// to boot, and a bot with no box is BROKEN and needs a person.
    ///
    /// **There used to be three.** The third was "a claim nobody has opened" —
    /// a bot naming a box that did not exist — and it is unreachable now: a box
    /// opens with its bot, so a claim cannot outlive the thing it claims. The
    /// second case survives only as damage, which is why its advice names the
    /// operator rather than a verb; there is no verb.
    #[tokio::test]
    async fn a_read_with_no_box_to_open_says_which_kind_of_nothing_it_found() {
        let jojobot = mailbox_handler();

        // 1. No handle at all.
        let anonymous = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
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
                id: EntityId::new(EntityKind::Bot, "gamma"),
                name: "gamma".into(),
                aliases: Vec::new(),
                source: "user-named".into(),
                crm: None,
                parent: None,
                boot: Default::default(),
                create_new: false,
            })
            .await
            .expect("the store writes it");
        let broken = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(as_bot(&jojobot, "gamma")),
                }))
                .await
                .expect("an answer"),
        );
        let how = broken["how_to_proceed"].as_str().expect("advice");
        // **The way out is the door that heals, not a person.** This advice
        // named the operator until he ruled that jojobot repairs this itself;
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
                    mailbox: "dev".into(),
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
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(plain["new_only"], true, "the safe branch is the default");
        assert_eq!(
            plain["count"], 2,
            "the leftover is still delivered: {plain}"
        );

        let leftover = plain["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == held_id.as_str())
            .expect("a default read still hands the leftover over");
        assert_eq!(leftover["seen_before"], true, "…still owed: {leftover}");
        assert_eq!(leftover["body_elided"], true, "…and says what it withheld");
        assert_eq!(leftover["body_bytes"], held_body.trim().len());
        assert!(leftover["body"].is_null());

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
                    mailbox: "dev".into(),
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
                    new_only: Some(true),
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            poll["count"], 2,
            "the leftover is STILL in the delivery: {poll}"
        );
        assert_eq!(poll["new_only"], true);

        let leftover = poll["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == held_id.as_str())
            .expect("the held message is still handed over");
        assert_eq!(
            leftover["seen_before"], true,
            "…still flagged as owed: {leftover}"
        );
        assert!(
            leftover["body"].is_null(),
            "…and its body is what was dropped"
        );
        assert_eq!(leftover["body_elided"], true);
        assert_eq!(leftover["body_bytes"], held_body.trim().len());

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
}

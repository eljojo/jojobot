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
    pub message_id: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Take delivery of one message by id, leaving the rest of its box alone.
#[tool_router(router = read_message_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Take delivery of ONE message by id — the selective half of read_mailbox, \
                       for when you want a single message (the one a search hit named) and have \
                       no business owning the rest of the box. That one moves `new` to `read`; \
                       nothing else in the box is touched. IT MUST BE IN YOUR OWN BOX: reading is \
                       taking delivery, so opening somebody else's message would move their mail \
                       out of `new` and it would never look fresh to them again — the same reason \
                       read_mailbox takes no box name. Ids are a plain counter, so the one beside \
                       yours is somebody else's; that comes back status: blocked and moves \
                       nothing. To reach another box, post_message writes into it without reading \
                       it, which is the shape of a request. THE ARCHIVE IS THE EXCEPTION, and it \
                       is not a loophole: a `processed` message comes back unchanged and flagged, \
                       from any box, because processed is terminal — reading one moves nothing and \
                       takes nothing on, and it is what a search hit over old mail points at. \
                       Same envelope a delivery hands over, seen_before and all: true means \
                       somebody had already taken this message, so it is a leftover rather than \
                       fresh mail. Taking delivery is NOT handling: call mark_processed once you \
                       have acted, and only then. Three refusals wear the status: blocked shape — \
                       an id that names nothing at all, an id naming an item jojobot cannot read \
                       (which comes with a `reason` and needs a person, not a retry), and a \
                       message that is not yours to take."
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
        // Located through `scan_messages`, which is the read that moves nothing
        // — the same one `list_sent` is built on. An id it cannot place falls
        // through untouched: a message nobody can find is not deliverable
        // either, and the verb below owns those two refusals (an id that names
        // nothing, and one naming something unreadable) with words this has no
        // business duplicating.
        let located = self
            .mailboxes
            .scan_messages()
            .await
            .map_err(mailbox_error)?
            .into_iter()
            .find(|m| m.id == id);
        if let Some(message) = located
            // **The guard is on the STATE CHANGE, not on the bytes.** `processed`
            // is a terminal archive: reading one moves nothing and takes nothing
            // on, so there is no delivery to guard — and `search` returns mail
            // from every box in that state by design, each hit carrying the id
            // this verb takes. Guarding the archive would hand a session a hit
            // it is structurally forbidden to open.
            .filter(|m| !matches!(m.state, mailbox::MessageState::Processed))
        {
            // The same three refusals a read of your own box gets — no handle,
            // a world that cannot say, a bot whose box is missing — because it
            // is the same question, asked by the same caller, about the same
            // thing.
            let mine = match self.my_box(args.sid.as_deref()).await {
                Ok(mine) => mine,
                Err(refused) => return Ok(refused),
            };
            if mine.name != message.mailbox {
                return Ok(not_yours(&id, &message.mailbox));
            }
        }
        match self.mailboxes.read_message(&id).await {
            Ok(delivered) => json_result(&delivered_json(&delivered)),
            Err(e) => mailbox_declined(e),
        }
    }
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
                    message_id: id,
                    notes: None,
                    sid: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(
            processed["subject"], "the shipment",
            "the archive keeps the title"
        );
    }

    /// **One message, taken by id.** The named message is delivered and the rest
    /// of the box is left where it was — the point of the verb: a session that
    /// wants one filed finding must not have to own everything beside it.
    #[tokio::test]
    async fn read_message_delivers_one_and_leaves_the_box_alone() {
        let jojobot = mailbox_handler();
        // The box's own drainer, because taking delivery is now something only
        // it can do — see `a_message_in_somebody_elses_box_is_not_delivered_by_id`.
        let reader = owning(&jojobot, "inbox").await;
        let wanted = send(&jojobot, "inbox", "epsilon", "the one worth reading").await;
        send(&jojobot, "inbox", "sigma", "the rest of the box").await;
        let id = wanted["id"].as_str().expect("an id").to_string();

        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
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

    /// **A processed message is history, and history is readable by anybody.**
    ///
    /// The scoping call this batch left open, and it goes on the STATE CHANGE
    /// rather than on the bytes. What the owner check exists to prevent is one
    /// bot taking delivery of another's mail — moving it out of `new` so its
    /// real consumer never sees it as fresh. `processed` is terminal: reading
    /// one moves nothing, takes nothing on, and owes nobody anything, so there
    /// is no delivery to guard.
    ///
    /// **And guarding it would break the front door.** `search` returns mail
    /// from every box in every state, processed included, by design and by
    /// default — the reader who needs a filed finding is a later session that
    /// does not know it is there. Every one of those hits carries the id
    /// `read_message` takes. An owner check over the archive would hand a
    /// session a hit it is structurally forbidden to open, which is a worse
    /// answer than either extreme.
    #[tokio::test]
    async fn a_processed_message_is_history_and_reading_one_takes_nothing() {
        let jojobot = mailbox_handler();
        let gamma = owning(&jojobot, "gamma").await;
        let delta = owning(&jojobot, "delta").await;
        let theirs = send(&jojobot, "delta", "epsilon", "a finding worth filing").await;
        let id = theirs["id"].as_str().expect("an id").to_string();
        jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: id.clone(),
                notes: Some("acted on".into()),
                sid: Some(delta),
            }))
            .await
            .expect("mark_processed ok");

        let read = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: id.clone(),
                    sid: Some(gamma),
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

    /// **An id that names nothing is blocked, not an error** — the same answer
    /// `mark_processed` gives, so one client branch handles both.
    #[tokio::test]
    async fn reading_an_unknown_message_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;

        let result = jojobot
            .read_message(Parameters(ReadMessageArgs {
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
    async fn reading_a_quarantined_card_is_blocked_with_its_own_words() {
        let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(store.clone());
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId(id.clone()),
            "its row on the page cannot be read — a state or a sender has been edited past parsing",
        );

        let result = jojobot
            .read_message(Parameters(ReadMessageArgs {
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
        assert!(
            reason.contains("edited past parsing"),
            "the store's own account of the fault comes through: {reason}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("retrying will not help") && advice.contains("operator"),
            "retrying does not help and a person must repair it: {advice}"
        );
        // **And it does not teach the store it no longer uses.** This advice
        // used to hand over the anatomy of a kanban card and tell an agent
        // which column to put it back in — a store's shape an agent must never
        // learn, and repair steps for a system that no longer holds the
        // message. Asserting the absence is the half that keeps it gone.
        for retired in ["card", "board", "column", "label"] {
            assert!(
                !advice.to_lowercase().contains(retired),
                "the advice teaches the retired store ({retired:?}): {advice}"
            );
        }
    }
}

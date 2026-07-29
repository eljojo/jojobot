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
                       nothing else in the box is touched. Same envelope a delivery hands over, \
                       seen_before and all: true means somebody had already taken this message, \
                       so it is a leftover rather than fresh mail. A `processed` message comes \
                       back unchanged and flagged — processed is a terminal archive, and reading \
                       one is reading history, not taking it on. Taking delivery is NOT handling: \
                       call mark_processed once you have acted, and only then. Two refusals wear \
                       the status: blocked shape — an id that names nothing at all, and an id \
                       naming an item jojobot cannot read, which comes with a `reason` and needs \
                       a person, not a retry."
    )]
    pub(crate) async fn read_message(
        &self,
        Parameters(args): Parameters<ReadMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = MessageId(args.message_id.trim().to_string());
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
        make_box(&jojobot, "inbox").await;
        let wanted = send(&jojobot, "inbox", "epsilon", "the one worth reading").await;
        send(&jojobot, "inbox", "sigma", "the rest of the box").await;
        let id = wanted["id"].as_str().expect("an id").to_string();

        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: id.clone(),
                    sid: None,
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
                    sid: None,
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(again["seen_before"], true);
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

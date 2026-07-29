//! `mark_processed` — Retire a message once it has actually been acted on.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `mark_processed`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MarkProcessedArgs {
    /// The message's id, exactly as `read_mailbox` returned it.
    pub message_id: String,
    /// What happened — including a failure. Optional, one plain line.
    #[serde(default)]
    pub notes: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Retire a message once it has actually been acted on.
#[tool_router(router = mark_processed_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Retire a message once it has been handled — terminal, an archive, never \
                       a deletion — optionally recording the outcome in `notes`. \
                       THE CRASH CONTRACT: call this ONLY AFTER you have acted on the message. \
                       Mark first and then fail, and the message is gone from every future \
                       delivery with nobody the wiser; act first and crash before marking, and \
                       the next read_mailbox hands it back as a leftover — recoverable. A \
                       FAILURE IS DATA, NOT A STATE: record it in notes (and reply with a new \
                       message if someone needs to know) — there is no failed status, because a \
                       message whose handling failed has still been handled. When a message asks \
                       nothing of you — its whole content is known to you once you have read it \
                       — READING IT IS THE ACTING, so process it with a note and move on; the \
                       order matters for work you still owe, not for work that was never owed. \
                       Write the outcome you actually have: a note \
                       longer than the record holds is CUT to fit and says so (a trailing ellipsis, \
                       and notes_truncated: true), never refused — the verb that retires a \
                       message will not fail over the length of its own record. The answer \
                       confirms the move — state, notes, id — WITHOUT echoing the message's body \
                       back at you, since the read that handed it over already gave you that; it \
                       carries body_bytes and body_elided: true instead, and read_message returns \
                       the text unchanged for a processed message. A message can be \
                       processed straight from `new`, no delivery first. Two refusals wear the \
                       same status: blocked shape and mean different things: an id that names \
                       nothing at all (use one read_mailbox or post_message handed you), and an \
                       id naming an item jojobot cannot read, which comes back saying why — \
                       retrying that one will not help, a person has to repair it, and until \
                       then treat whatever it carried as unhandled and say so."
    )]
    pub(crate) async fn mark_processed(
        &self,
        Parameters(args): Parameters<MarkProcessedArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let id = MessageId(args.message_id.trim().to_string());
        // What the caller asked to record, blank-is-absent.
        let asked = args
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        match self
            .mailboxes
            .mark_processed(&id, args.notes.as_deref())
            .await
        {
            Ok(processed) => {
                self.beat("mark_processed", processed.id.as_str(), args.sid.as_deref())
                    .await;
                let mut body = message_receipt_json(
                    &processed,
                    "you had this body from the read that handed it to you. read_message returns \
                     it, and a processed message comes back unchanged — processed is terminal",
                );
                if let Some(obj) = body.as_object_mut() {
                    // **Always present, never inferred from the ellipsis.** The
                    // record can legitimately end in one, and a reader that has
                    // to guess whether a store cut its text is a reader that
                    // will eventually guess wrong.
                    //
                    // **Only a record this call OFFERED can have been cut.**
                    // Both stores carry a pre-existing note forward when the
                    // caller supplies none, and nothing gates re-processing, so
                    // comparing unconditionally made a second call report a cut
                    // of a record it never sent — the same wrong inference,
                    // pointing the other way.
                    obj.insert(
                        "notes_truncated".into(),
                        asked
                            .is_some_and(|asked| processed.notes.as_deref() != Some(asked))
                            .into(),
                    );
                }
                json_result(&body)
            }
            // Both misses here are answers, not failures: an id that names
            // nothing, and an id naming a card jojobot cannot read. They stay
            // different answers — one is repairable by a better id, the other
            // only by a person on the board — in the guards' one shape.
            Err(e) => mailbox_declined(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// The same, for the terminal verb — whose caller got the body from the
    /// read that handed it to them.
    #[tokio::test]
    async fn processing_receipts_without_shipping_the_body_back() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed at dawn").await;

        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: posted["id"].as_str().expect("an id").to_string(),
                    notes: Some("filed under shipments".into()),
                    sid: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(
            body["state"], "processed",
            "the proof that matters: it moved"
        );
        assert_eq!(
            body["notes"], "filed under shipments",
            "…and what was recorded"
        );
        assert!(body["body"].is_null());
        assert_eq!(body["body_elided"], true);
        assert_eq!(body["body_bytes"], "the shipment landed at dawn".len());
        assert!(
            body["how_to_read"]
                .as_str()
                .expect("a pointer")
                .contains("read_message")
        );
    }

    /// **A long outcome record is cut, and the caller is told it was cut.** The
    /// crash contract asks for an account of what happened; refusing the whole
    /// call over its length left the message unprocessed and cost exactly the
    /// record the cap was policing — which is what it did to a caller in
    /// production. Cutting silently would be the other half of the same
    /// mistake: notes that stop mid-sentence read as a consumer who trailed
    /// off, not a store that ran out of room.
    #[tokio::test]
    async fn a_long_outcome_record_is_cut_and_says_so_rather_than_failing() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();

        let long = "counted the crates and reconciled them against the manifest ".repeat(200);
        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: id.clone(),
                    notes: Some(long.clone()),
                    sid: None,
                }))
                .await
                .expect("a long note must not fail the terminal verb"),
        );
        assert_eq!(
            body["state"], "processed",
            "the message WAS handled: {body}"
        );
        assert_eq!(
            body["notes_truncated"], true,
            "…and the cut is said out loud: {body}"
        );
        let kept = body["notes"].as_str().expect("the outcome is recorded");
        assert!(
            kept.ends_with('…'),
            "the record itself says it was cut: {kept:?}"
        );
        assert!(kept.chars().count() < long.chars().count());
    }

    /// **A caller who recorded nothing was cut off from nothing.** The flag
    /// compared the stored notes against what this call asked to store, on the
    /// premise that the store applies the same rule — but both stores carry a
    /// PRE-EXISTING note forward when the caller supplies none, and
    /// `mark_processed` has no state gate, so re-processing is reachable. The
    /// second call then saw notes it had not sent and reported a cut nobody
    /// made: the same wrong inference the flag exists to prevent, pointing the
    /// other way.
    #[tokio::test]
    async fn processing_again_without_notes_reports_no_cut() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();

        let processed = |notes: Option<String>| {
            let id = id.clone();
            async {
                json_of(
                    &jojobot
                        .mark_processed(Parameters(MarkProcessedArgs {
                            message_id: id,
                            notes,
                            sid: None,
                        }))
                        .await
                        .expect("mark_processed ok"),
                )
            }
        };

        let first = processed(Some("filed under shipments".into())).await;
        assert_eq!(first["notes_truncated"], false);

        // Again, recording nothing. The store keeps the earlier note.
        let again = processed(None).await;
        assert_eq!(
            again["notes"], "filed under shipments",
            "the record stands: {again}"
        );
        assert_eq!(
            again["notes_truncated"], false,
            "no record was offered, so none was cut: {again}"
        );
    }

    /// A record that fits is stored whole and reports no cut — the flag is
    /// always present, so a reader never branches on whether it is there.
    #[tokio::test]
    async fn an_outcome_record_that_fits_reports_no_cut() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: posted["id"].as_str().expect("an id").to_string(),
                    notes: Some("filed under shipments".into()),
                    sid: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(body["notes"], "filed under shipments");
        assert_eq!(body["notes_truncated"], false, "{body}");
    }

    /// **An id that names nothing is an answer, not a failure** — and no longer
    /// a protocol error either: naming something that does not exist is the
    /// same kind of answer whichever gate catches it, so it wears one shape.
    #[tokio::test]
    async fn processing_an_unknown_message_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        let result = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "999999".into(),
                notes: None,
                sid: None,
            }))
            .await
            .expect("an id that names nothing is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "999999");
        assert!(
            body["candidates"].as_array().is_some_and(|c| c.is_empty()),
            "nothing resembles a message id: {body}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("read_mailbox"),
            "the way out is a delivery that hands back real ids: {advice}"
        );
    }

    /// **`mark_processed` on a quarantined id says so.** Answering "no message
    /// with that id" — for an id `list_mailboxes` published one call ago — is a
    /// false statement about jojobot's own output, and it sends the caller
    /// hunting for a lost message instead of at the card sitting on the board.
    /// The answer takes the blocked shape the guards use, so one client-side
    /// branch handles every "declined, here is what to do" in this context.
    #[tokio::test]
    async fn processing_a_quarantined_card_is_blocked_without_the_stores_words() {
        let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(store.clone());
        make_box(&jojobot, "inbox").await;
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId("4212".into()),
            "its row on the page cannot be read — a state or a sender has been edited past parsing",
        );

        let result = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "4212".into(),
                notes: Some("filed".into()),
                sid: None,
            }))
            .await
            .expect("a quarantined card is a structured answer, not a protocol error");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "4212");
        assert_eq!(body["wrote"], false);
        let reason = body["reason"].as_str().expect("a reason");
        // **The store's own account does NOT come through**, which is what
        // this used to assert. It names which field of which row failed to
        // parse — what an operator repairing it needs, and what an agent must
        // never be handed. It is logged instead.
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
            advice.contains("4212")
                && advice.contains("retrying will not help")
                && advice.contains("operator"),
            "…and that the way out is a person, not a retry: {advice}"
        );
        // The same absence `read_message` pins: no store anatomy, no repair
        // steps for a system that no longer holds the message.
        for retired in ["card", "board", "column", "label"] {
            assert!(
                !advice.to_lowercase().contains(retired),
                "the advice teaches the retired store ({retired:?}): {advice}"
            );
        }

        // Both wear the blocked shape now — but they are still different
        // answers, and the difference is the one that matters: a quarantined
        // card is a real card no retry can reach, while an unknown id names
        // nothing at all.
        let unknown = blocked(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: "999999".into(),
                    notes: None,
                    sid: None,
                }))
                .await
                .expect("an id nothing answers to is still an answer"),
        );
        assert!(
            unknown["reason"].is_null(),
            "there is no card to explain — that field belongs to the quarantine answer: {unknown}"
        );
        assert!(
            !unknown["how_to_proceed"]
                .as_str()
                .expect("advice")
                .contains("PERSON"),
            "and its way out is not a human on the board: {unknown}"
        );
    }
}

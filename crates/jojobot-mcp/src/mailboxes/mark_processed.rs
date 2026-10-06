//! `mark_processed` — Retire a message once it has actually been acted on.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `mark_processed`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MarkProcessedArgs {
    /// The message's id, exactly as `read_mailbox` returned it.
    pub(crate) message_id: String,
    /// What happened — including a failure. Optional, one plain line.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as
    /// the handle that thing wears today, even after a rename.
    #[serde(default)]
    pub(crate) notes: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **Quarantine this message instead of retiring it — a decision, not
    /// damage.** Give the reason here and the message becomes unreadable from
    /// every future delivery, exactly like storage damage, except the refusal
    /// names who decided and why instead of asking for repair. Only your own
    /// mail: naming a message sitting in another box is refused, and nothing
    /// is written. Mutually exclusive with `notes` — a quarantine records no
    /// processing outcome, because none happened.
    #[serde(default)]
    pub(crate) quarantine: Option<String>,
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
                       and a delta naming what was stored), never refused — the verb that retires a \
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
                       then treat whatever it carried as unhandled and say so. TO QUARANTINE A \
                       MESSAGE INSTEAD OF RETIRING IT — a decision, not damage — pass `quarantine` \
                       with your reason and leave `notes` unset; the message becomes unreadable \
                       from every future delivery, and its refusal names you and your reason \
                       rather than asking for repair. Only your own mail: naming a message sitting \
                       in another box is refused, and nothing is written. There is no verb that \
                       lifts a quarantine."
    )]
    pub(crate) async fn mark_processed(
        &self,
        Parameters(args): Parameters<MarkProcessedArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable_for_write(args.sid.as_deref()).await {
            return Ok(refused);
        }
        let id = MessageId(args.message_id.trim().to_string());
        // **A blank reason is refused, never read as absent.** `notes` may be
        // blank-is-absent because leaving it off retires the message anyway.
        // A quarantine is a decision that needs its reason: reading a blank one
        // as "no quarantine" retired a message nobody had handled, with
        // whatever `notes` rode along.
        let quarantine_reason = args.quarantine.as_deref().map(str::trim);
        if quarantine_reason.is_some_and(str::is_empty) {
            return Ok(misused(
                "Nothing was written: `quarantine` was sent blank, and a quarantine records a \
                 decision, so it needs the reason. Send the reason in `quarantine`, or leave \
                 `quarantine` off to retire the message with `notes` instead.",
            ));
        }
        if let Some(reason) = quarantine_reason {
            // **The two are mutually exclusive, and the pair is refused rather
            // than one half dropped.** A quarantine records no processing
            // outcome, so `notes` beside it has nowhere to go; taking the
            // quarantine and losing the text without a word would cost the
            // caller what it wrote and teach it nothing.
            if args
                .notes
                .as_deref()
                .map(str::trim)
                .is_some_and(|notes| !notes.is_empty())
            {
                return Ok(misused(
                    "Nothing was written: `quarantine` and `notes` were both sent, and a \
                     quarantine records no outcome, so there is nowhere to keep the notes. Send \
                     `quarantine` alone to set the message aside, or `notes` alone to retire it \
                     with the outcome.",
                ));
            }
            let mine = match self.my_box(args.sid.as_deref()).await {
                Ok(mine) => mine,
                Err(refused) => return Ok(refused),
            };
            return match self
                .mailboxes
                .quarantine(&id, &mine.name, reason, self.clock().now())
                .await
            {
                Ok(quarantined) => {
                    self.beat("quarantine", quarantined.id.as_str(), args.sid.as_deref())
                        .await;
                    json_result(&quarantine_receipt_json(&quarantined))
                }
                Err(e) => mailbox_declined(e),
            };
        }
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
                    Some(
                        "you had this body from the read that handed it to you. read_message \
                         returns it, and a processed message comes back unchanged — processed \
                         is terminal",
                    ),
                );
                // **The cut is a substitution, announced the way every
                // other one is.** It had a flag of its own, so a caller who
                // had learnt to read `delta` for what jojobot changed had
                // to learn a second field for this one verb.
                //
                // **Only a record this call OFFERED can have been cut.**
                // Both stores carry a pre-existing note forward when the
                // caller supplies none, and nothing gates re-processing, so
                // comparing unconditionally made a second call report a cut
                // of a record it never sent.
                crate::answer::note_delta(
                    &mut body,
                    crate::answer::Difference::between(
                        "notes",
                        asked,
                        processed.notes.as_deref().unwrap_or(""),
                    )
                    .into_iter()
                    .collect(),
                );
                crate::answer::note_postcondition(
                    &mut body,
                    what_a_retirement_left_standing(&processed),
                );
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

/// **What a caller's own retirement did to the message.**
///
/// ⚠️ **`processed` is terminal and it is an archive.** The message is not
/// deleted — this rail has no verb that deletes anything — and it stays
/// readable from any box, because reading history moves nothing. A caller that
/// believes a retirement throws work away handles the next one differently.
///
/// The outcome record is named only when this call carries one, so the line
/// does not claim an account that was never written.
/// The receipt for a deliberate quarantine — never `message_receipt_json`,
/// because the row it describes is now unreadable: there is no body to elide,
/// only the decision and who made it.
fn quarantine_receipt_json(quarantined: &mailbox::Quarantined) -> serde_json::Value {
    serde_json::json!({
        "id": quarantined.id.as_str(),
        "mailbox": quarantined.mailbox.as_str(),
        "state": mailbox::QUARANTINE_STATE_TOKEN,
        "quarantined_by": quarantined.by,
        "quarantine_reason": quarantined.reason,
        "quarantined_at": quarantined.at.to_string(),
    })
}

fn what_a_retirement_left_standing(processed: &Message) -> String {
    let recorded = match processed.notes.as_deref() {
        Some(_) => " Your outcome record is stored on it.",
        None => "",
    };
    format!(
        "{} is processed, which is the last state on this rail. It is archived and not deleted: \
         it stays readable, and a read of it moves nothing.{recorded}",
        processed.id.as_str(),
    )
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
                    quarantine: None,
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

    /// **Retiring a message says it is archived and still there.**
    ///
    /// ⚠️ **`processed` is terminal and it is an ARCHIVE, not a deletion.**
    /// There is no delete verb on this rail at all — a promise the surface
    /// makes and that nothing in the answer to a retirement repeated. A caller
    /// who believes it has thrown something away handles the next one
    /// differently, and the message is exactly as readable afterwards as it
    /// was before.
    ///
    /// **The outcome record is what makes the line worth computing**: a
    /// retirement that recorded one says so and one that recorded none does
    /// not claim to. A constant cannot tell the two apart.
    ///
    /// **The promise is checked against the rail in the same case.** A line
    /// saying the message is still readable is worth nothing unless it is, and
    /// a case pinning only the wording would keep passing on the day it
    /// stopped being true.
    #[tokio::test]
    async fn retiring_a_message_says_it_is_archived_and_not_gone() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed at dawn").await;
        let id = posted["id"].as_str().expect("an id").to_string();

        let retired = json_of(
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
        let line = retired["postcondition"]
            .as_str()
            .unwrap_or_else(|| panic!("a write states what now stands: {retired}"))
            .to_string();
        assert!(
            line.contains(&id),
            "the line has to name the message it retired: {retired}",
        );
        assert!(
            line.contains("notes") || line.contains("outcome") || line.contains("recorded"),
            "this retirement recorded an outcome and the line does not mention one: {retired}",
        );

        // The pairing: nothing was sent, so the line does not say an outcome
        // was recorded. A constant sentence says it either way.
        let second = send(&jojobot, "inbox", "epsilon", "and the second crate too").await;
        let bare = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: second["id"].as_str().expect("an id").to_string(),
                    notes: None,
                    sid: None,
                    quarantine: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_ne!(
            bare["postcondition"], retired["postcondition"],
            "one sentence for both, so it promises on the bare call what only the other did: \
             {bare}",
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
                    quarantine: None,
                }))
                .await
                .expect("a long note must not fail the terminal verb"),
        );
        assert_eq!(
            body["state"], "processed",
            "the message WAS handled: {body}"
        );
        // **The cut is said out loud, the way every other substitution is.**
        assert_eq!(body["delta"][0]["field"], "notes", "{body}");
        assert_eq!(
            body["delta"][0]["stored"], body["notes"],
            "the delta names what the store kept: {body}"
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
    /// made: the same wrong inference this line exists to prevent, pointing the
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
                            quarantine: None,
                        }))
                        .await
                        .expect("mark_processed ok"),
                )
            }
        };

        let first = processed(Some("filed under shipments".into())).await;
        assert!(first.get("delta").is_none(), "{first}");

        // Again, recording nothing. The store keeps the earlier note.
        let again = processed(None).await;
        assert_eq!(
            again["notes"], "filed under shipments",
            "the record stands: {again}"
        );
        assert!(
            again.get("delta").is_none(),
            "no record was offered, so none was cut: {again}"
        );
    }

    /// **A record that fits is stored whole and carries no delta at all.**
    ///
    /// The paired half of the cut above, and the one that keeps the line
    /// meaning something: a receipt that named a difference on every call is
    /// one a reader learns to skip, and it would be gone from view on the call
    /// that needed it.
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
                    quarantine: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(body["notes"], "filed under shipments");
        assert!(body.get("delta").is_none(), "{body}");
    }

    /// An id that names nothing is an answer, not a protocol error: naming
    /// something that does not exist is the same kind of answer whichever
    /// gate catches it, so it wears one shape.
    #[tokio::test]
    async fn processing_an_unknown_message_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        let result = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "999999".into(),
                notes: None,
                sid: None,
                quarantine: None,
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
        store.quarantine_by_damage(
            &MailboxName("inbox".into()),
            &MessageId("4212".into()),
            "its row on the page cannot be read — a state or a sender has been edited past parsing",
        );

        let result = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "4212".into(),
                notes: Some("filed".into()),
                sid: None,
                quarantine: None,
            }))
            .await
            .expect("a quarantined card is a structured answer, not a protocol error");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "4212");
        assert_eq!(body["wrote"], false);
        let reason = body["reason"].as_str().expect("a reason");
        // The store's own account does NOT come through: it names which
        // field of which row failed to parse, which an operator repairing it
        // needs and an agent must never be handed. It is logged instead.
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
        // The advice must never describe repair in the store's own
        // vocabulary — not an agent's business, and not accurate either.
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
                    quarantine: None,
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

    /// **A deliberate quarantine names who decided and why, and stops
    /// serving the message everywhere else — but only that one.** The
    /// readable neighbour is the proof the reach is scoped to the id named,
    /// not to the whole box.
    #[tokio::test]
    async fn quarantining_a_message_names_who_and_why_and_stops_serving_it() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let target = send(
            &jojobot,
            "inbox",
            "epsilon",
            "the shipment is short a crate",
        )
        .await;
        let neighbor = send(
            &jojobot,
            "inbox",
            "epsilon",
            "the second crate arrived whole",
        )
        .await;
        let sid = as_bot(&jojobot, "inbox");

        let receipt = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: target["id"].as_str().expect("an id").to_string(),
                    notes: None,
                    sid: Some(sid.clone()),
                    quarantine: Some("the count on this one cannot be trusted".into()),
                }))
                .await
                .expect("quarantining is an answer, not a protocol failure"),
        );
        assert_eq!(receipt["state"], "quarantined");
        assert_eq!(receipt["quarantined_by"], "inbox");
        assert_eq!(
            receipt["quarantine_reason"],
            "the count on this one cannot be trusted"
        );
        assert!(
            receipt["quarantined_at"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "when it happened has to be on the receipt: {receipt}"
        );

        // read_message on the quarantined id is blocked, and the refusal
        // names the decision — never the damage wording that a parse
        // failure gets.
        let read = blocked(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: target["id"].as_str().expect("an id").to_string(),
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("a quarantined id is a structured answer"),
        );
        assert_eq!(read["quarantined_by"], "inbox");
        assert_eq!(
            read["quarantine_reason"],
            "the count on this one cannot be trusted"
        );
        let advice = read["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("decision") && !advice.contains("operator"),
            "a decision is not damage, and it needs no person to repair it: {advice}"
        );

        // read_mailbox omits the quarantined message and still delivers the
        // one beside it, unaffected.
        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: Some(false),
                    sid: Some(sid),
                }))
                .await
                .expect("read_mailbox ok"),
        );
        let ids: Vec<&str> = delivery["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|m| m["id"].as_str().expect("an id"))
            .collect();
        assert!(
            !ids.contains(&target["id"].as_str().expect("an id")),
            "the quarantined message must not be delivered: {delivery}"
        );
        assert!(
            ids.contains(&neighbor["id"].as_str().expect("an id")),
            "…and the readable one beside it still must be: {delivery}"
        );
    }

    /// **A bot may quarantine only its own mail.** Naming a message sitting
    /// in somebody else's box is refused, and the message stays exactly as
    /// readable as it was.
    #[tokio::test]
    async fn a_bot_cannot_quarantine_another_boxs_message() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        make_box(&jojobot, "dev").await;
        let theirs = send(&jojobot, "dev", "epsilon", "dev's own shipment").await;
        let mine = as_bot(&jojobot, "inbox");

        let refused = blocked(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: theirs["id"].as_str().expect("an id").to_string(),
                    notes: None,
                    sid: Some(mine),
                    quarantine: Some("not mine to decide".into()),
                }))
                .await
                .expect("reaching another box's mail is an answer, not a protocol failure"),
        );
        assert_eq!(refused["mailbox"], "dev");
        let advice = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("post_message"),
            "the way to reach another box is the one verb that does: {advice}"
        );

        // Nothing was written: dev can still read its own message.
        let dev_sid = as_bot(&jojobot, "dev");
        let read = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: theirs["id"].as_str().expect("an id").to_string(),
                    sid: Some(dev_sid),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(read["body"], "dev's own shipment");
    }
}

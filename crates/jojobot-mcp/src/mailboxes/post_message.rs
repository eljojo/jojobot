//! `post_message` — Leave a message in a box — the one verb that reaches another.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `post_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PostMessageArgs {
    /// The box to leave it in. **It must already exist** — an unknown name comes
    /// back with candidates and nothing is written.
    pub mailbox: String,
    /// The message itself. Prose: paragraphs are fine.
    pub body: String,
    /// **Your session id.** Required here, because it is what jojobot records
    /// as the sender: a message from nobody is a message nobody can reply to,
    /// and identity that is merely declared is identity that can be wrong.
    pub sid: String,
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
    pub subject: Option<String>,

    /// The id of the message this one answers, when it answers one. Optional.
    /// It must name a message that exists — a miss comes back blocked and
    /// nothing is written — and it links the two without saying anything about
    /// either: it does not deliver, handle, or oblige.
    #[serde(default)]
    pub in_reply_to: Option<String>,
}

/// Leave a message in a box.
#[tool_router(router = post_message_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Leave a message for someone who is not in this conversation. The box \
                       must ALREADY EXIST — an unknown name comes back status: blocked with \
                       candidates and nothing is written. There is no verb that opens a box: a \
                       box is some bot's own and arrives with it, so a name nobody answers to is \
                       a name nobody drains. Returns the stored message, including the id that \
                       read_message and mark_processed later target. Give it a `subject`: one \
                       line saying what the message is about, which is what a reader sees on the \
                       listing and on a search hit before opening anything — put it there rather \
                       than on the body's first line. The `state` you get back is the state as \
                       it stands — it can already say `read` if a person picked the message up \
                       in between, and that is success, not a problem: the message exists and \
                       someone has it. The sender is not yours to declare: jojobot records the \
                       bot behind the `sid` you pass, so a reply can always find you and nothing \
                       can be posted under somebody else's name. A `sid` jojobot is not holding \
                       comes back status: blocked and nothing is written. YOUR BODY IS NOT \
                       ECHOED BACK — you wrote it, and \
                       jojobot verified it by reading the stored record back, so the answer carries \
                       the id, the state and body_bytes with body_elided: true rather than the \
                       text. `list_sent` with include_bodies returns it and takes no delivery. \
                       `in_reply_to` links this message to the one it \
                       answers: optional, it must name a message that exists (a miss comes back \
                       blocked, nothing written), and it says only that the two are one exchange \
                       — it does not deliver the original, handle it, or oblige anybody."
    )]
    pub(crate) async fn post_message(
        &self,
        Parameters(args): Parameters<PostMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        // **The sender is derived, never declared.** It was a free-text field
        // recorded exactly as claimed, which made every "who left this?" answer
        // only as good as the caller's honesty and their memory of what they
        // called themselves last time. The handle says who is asking, so the
        // handle says who sent it.
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
        let new = NewMessage {
            mailbox: MailboxName(args.mailbox.trim().to_string()),
            body: args.body,
            subject: args.subject,
            sender: caller.bot.as_str().to_string(),
            // Stamped here, at the edge, for the same reason `capture` stamps a
            // date here: the domain stays clock-free, and a caller does not get
            // to backdate a message it is posting now.
            sent_at: jiff::Timestamp::now(),
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
                json_result(&message_receipt_json(
                    &message,
                    "you wrote this body; jojobot verified it by reading the stored record back. \
                     list_sent with include_bodies: true returns it, and takes no delivery",
                ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// **Blocked is a result, not a protocol error** — the same shape the Memory
    /// verbs use, so one client-side branch handles both contexts.
    #[tokio::test]
    async fn posting_into_an_unknown_box_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;

        let result = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "inbx".into(),
                sid: as_bot(&jojobot, "epsilon"),
                body: "the shipment landed".into(),
                subject: None,
                in_reply_to: None,
            }))
            .await
            .expect("a blocked post is a successful call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "inbx");
        assert_eq!(body["candidates"][0]["name"], "inbox");
        assert_eq!(body["candidates"][0]["reason"], "near");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        // The way out is a candidate, because there is no verb: nothing opens
        // a box on its own, so advice naming one would point at nothing.
        assert!(
            advice.contains("bot"),
            "…and it says why: a box is some bot's own: {advice}"
        );
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
                    mailbox: "inbox".into(),
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
                    mailbox: "pm".into(),
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
                    mailbox: "pm".into(),
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
                    mailbox: "pm".into(),
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
                    mailbox: "pm".into(),
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

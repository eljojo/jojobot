//! `list_sent` — What a sender has sent, and where it got to — without touching any of it.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// How many messages come back when the caller does not say — the same twenty
/// `search` answers with, and for the same reason: an answer nobody sized is an
/// answer that grows until it is unreadable.
const DEFAULT_LIMIT: usize = 20;

/// Arguments to `list_sent`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListSentArgs {
    /// Whose outgoing mail to show, matched **exactly** against the sender
    /// recorded on each message. Omit it for your own — your `sid` says who
    /// that is, and your own mail is what this verb is for.
    #[serde(default)]
    pub(crate) sender: Option<String>,
    /// **Only what you sent to this bot** — a bare name like `gamma`, or its
    /// full handle. Omit for everyone you have written to. You addressed a
    /// colleague, so this is how you ask after them.
    #[serde(default)]
    pub(crate) to: Option<String>,
    /// How many messages to return, newest first. Defaults to twenty.
    ///
    /// **No pagination and no cursor**, as `search` has none: a second page is
    /// a narrower question — one box, or one sender. What is left out is
    /// counted and said, never silently dropped.
    #[serde(default)]
    pub(crate) limit: Option<u32>,
    /// Ship the bodies back too. Off by default: you wrote them, so the useful
    /// answer is where they got to, not what they say.
    #[serde(default)]
    pub(crate) include_bodies: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// What a sender has sent, and where it got to — without touching any of it.
#[tool_router(router = list_sent_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "See the mail YOU have sent and where it got to — read-only, and it moves \
                       NOTHING: no state changes, nobody's delivery is taken, and the messages \
                       stay exactly as owed as they were. It answers whether something you sent \
                       arrived and whether anyone has read it — questions every other verb could \
                       only answer by taking delivery of the box you posted into. A `to` that \
                       names no colleague comes back status: blocked with candidates, never an \
                       empty list, because an empty list would read as 'it never arrived'. Messages \
                       jojobot cannot read are reported separately under \
                       `unreadable`: it cannot tell who sent them, so one of yours could be \
                       there. Newest first, each with its \
                       state (`new` = nobody has picked it up · `read` = delivered, not yet \
                       finished with · `processed` = acted on) plus notes when the consumer \
                       recorded an outcome. Bodies are left out unless you ask for them — you \
                       wrote them — so each carries body_bytes and the opening line instead, and \
                       says body_elided: true rather than leaving you to guess, with the one \
                       instruction for getting them said once beside the list rather than on \
                       every message. Twenty newest by default: raise `limit` for more, and \
                       whatever a cut leaves out is counted under not_shown rather than \
                       silently dropped. OMIT `sender` for \
                       your own mail — your `sid` already says who that is. Pass one to ask after \
                       somebody else's outgoing mail: it is matched exactly against the bot \
                       handle recorded on each message (`bot:gamma`), which is allowed, because \
                       where a message got to is not private to its sender."
    )]
    pub(crate) async fn list_sent(
        &self,
        Parameters(args): Parameters<ListSentArgs>,
    ) -> Result<CallToolResult, McpError> {
        // **Your own mail by default.** The sender is derived from the handle
        // now, so the caller does not have to remember what they called
        // themselves — and asking after somebody else's is still allowed,
        // because where a message got to is not private to its writer.
        let caller = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let declared = args
            .sender
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let own = caller.as_ref().map(|c| c.bot.as_str().to_string());
        let Some(sender) = declared.map(str::to_string).or(own) else {
            return Ok(session_unbound());
        };
        let sender = sender.as_str();
        // **Named as a bot, resolved to their box.** You wrote to a colleague,
        // so you ask after a colleague — what their box is called is not
        // something a sender should have to know to check on their own mail.
        //
        // This is the whole existence screen: a box that came back from a read
        // of who owns what is a box that is on the board, so nothing below has
        // a name left to check.
        let addressed_to = args.to.as_deref().map(str::trim).filter(|m| !m.is_empty());
        let mut only: Option<String> = None;
        if let Some(named) = addressed_to {
            let addressee = crate::mailboxes::post_message::bot_handle(named);
            match self.own_box(&addressee).await {
                OwnBox::The(name) => only = Some(name.as_str().to_string()),
                // **The same refusal posting gives, and for the same reason.**
                // This verb answers "did my report land", so a typo answered
                // with a confident zero says "no, it did not" — and the sender
                // writes it again, leaving duplicate mail behind the original.
                elsewhere => return Ok(self.no_such_addressee(&addressee, elsewhere).await),
            }
        }
        let only = only.as_deref();
        let bodies = args.include_bodies.unwrap_or(false);

        // Built on the scan, which is the one read that moves nothing: it is
        // how the search projection is rebuilt, and its "nothing moves" is
        // pinned by the shared contract on every tier.
        // **The tie breaks on the scan's own order, reversed.** The board read
        // hands messages back oldest first, with the store's ordinal breaking a
        // tie inside one instant, so its position IS the total order every
        // other tier reads — and newest-first here is that order backwards.
        //
        // Never on the id: an id is drawn, so it sorts as nothing at all, and a
        // tie-break that read one would be a caller-facing order resting on a
        // shape the store is free to change.
        let mut sent: Vec<(usize, Message)> = self
            .mailboxes
            .scan_messages()
            .await
            .map_err(mailbox_error)?
            .into_iter()
            .enumerate()
            .filter(|(_, m)| m.sender.trim() == sender)
            .filter(|(_, m)| only.is_none_or(|name| m.mailbox.as_str() == name))
            .collect();
        sent.sort_by(|(a_at, a), (b_at, b)| b.sent_at.cmp(&a.sent_at).then_with(|| b_at.cmp(a_at)));
        let mut sent: Vec<Message> = sent.into_iter().map(|(_, m)| m).collect();

        // **Something jojobot cannot read is not a message that was never
        // sent.** The scan leaves quarantined items out — it cannot parse them,
        // so it has nothing to return — and this verb answers "did my report
        // land". Staying silent about them means the honest answer ("something
        // here is broken") arrives as a confident "no". Their senders are
        // unreadable too, so they cannot be filtered to this caller; the count
        // is reported per box and the ids are named — `ids`, in the same
        // spelling `quarantined_json` uses, and for the same reason.
        let unreadable: Vec<serde_json::Value> = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(mailbox_error)?
            .iter()
            .filter(|b| only.is_none_or(|name| b.name.as_str() == name))
            .filter(|b| !b.quarantined.is_empty())
            .map(|b| {
                serde_json::json!({
                    "mailbox": b.name.as_str(),
                    "ids": b.quarantined.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect();

        // **The cut is the last thing that happens**, after the ordering, so
        // what comes back is the newest rather than whatever the store handed
        // over first.
        let held = sent.len();
        let limit = args.limit.map_or(DEFAULT_LIMIT, |l| l as usize);
        sent.truncate(limit);

        json_result(&serde_json::json!({
            "sender": sender,
            "mailbox": only,
            "count": sent.len(),
            "sent_total": held,
            // **Eliding is never silent.** The count above is what came back;
            // this says how many there are and what to do about it, and it is
            // absent when nothing was cut rather than saying "0 left out".
            "not_shown": (held > sent.len()).then(|| serde_json::json!({
                "count": held - sent.len(),
                "how_to_proceed": "these are the newest; raise limit, or narrow to one mailbox",
            })),
            // **Said once, beside the list.** It is the same sentence for every
            // message, so carrying it on each one is most of the answer and
            // teaches a reader nothing after the first.
            "how_to_read": (!bodies).then_some(
                "call list_sent again with include_bodies: true — these are your own messages, \
                 so reading them takes no delivery from anybody",
            ),
            "unreadable": unreadable,
            "unreadable_note": "Messages jojobot cannot read are not in the list above — \
                                it cannot tell who sent them. If one of yours is missing, it may \
                                be here, and a person has to repair it before any verb can act on it.",
            "messages": sent
                .iter()
                .map(|m| if bodies {
                    message_json(m)
                } else {
                    message_receipt_json(m, None)
                })
                .collect::<Vec<_>>(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// **A sender can see where their own mail got to, and seeing moves
    /// nothing.** Twice a session wanted to confirm a report had been *read*
    /// rather than merely delivered, and could not: the only verbs that show a
    /// message's state take delivery, and taking delivery of somebody else's box
    /// makes their mail yours to finish. So the question went unanswered because
    /// asking it cost more than the answer was worth.
    #[tokio::test]
    async fn a_sender_sees_where_their_mail_got_to_without_moving_any_of_it() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "pm", "otto", "the kiln slice is done").await;
        send(&jojobot, "inbox", "otto", "a note for somebody else").await;
        let theirs = send(&jojobot, "pm", "delta", "not yours to see").await;

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    sender: Some("bot:otto".into()),
                    to: None,
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(sent["count"], 2, "only what this sender sent: {sent}");
        let bodies: Vec<&str> = sent["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|m| m["sender"].as_str().expect("a sender"))
            .collect();
        assert_eq!(bodies, vec!["bot:otto", "bot:otto"]);

        // The body is elided, and says so rather than leaving a reader to guess.
        let first = &sent["messages"][0];
        assert!(first["body"].is_null(), "{first}");
        assert_eq!(first["body_elided"], true);
        assert!(first["body_bytes"].as_u64().expect("a size") > 0);
        assert!(
            first["body_head"]
                .as_str()
                .expect("a head")
                .contains("note for somebody else")
        );
        // **The pointer is on the answer, once.** It is the same sentence for
        // every message, so it sits beside the list rather than on each item —
        // and a reader still learns how to get a body it was not shipped.
        assert!(
            first.get("how_to_read").is_none(),
            "the per-message copy is gone: {first}"
        );
        assert!(
            sent["how_to_read"]
                .as_str()
                .expect("a pointer beside the list")
                .contains("include_bodies")
        );

        // **Nothing moved — read from the STORE, not from the verb.** Asserting
        // `state == "new"` on `list_sent`'s own response lets the verb grade
        // itself: its body is built from a snapshot taken before it returns, so
        // a version that took delivery afterwards would still report `new`. The
        // counts come from the other side of the store.
        let pm = counts(&jojobot, "pm").await;
        assert_eq!(
            pm["counts"]["read"], 0,
            "looking at your own outbox is not a delivery: {pm}"
        );
        assert_eq!(
            pm["counts"]["new"], 2,
            "…and everything is still waiting: {pm}"
        );
        assert!(
            !json_of(
                &jojobot
                    .read_message(Parameters(ReadMessageArgs {
                        message_id: theirs["id"].as_str().expect("an id").to_string(),
                        // Asked of the box's OWN drainer, which is the only
                        // caller that can take delivery — and the only one whose
                        // `seen_before` answers the question being put here.
                        sid: Some(as_bot(&jojobot, "pm"))
                    }))
                    .await
                    .expect("read ok")
            )["seen_before"]
                .as_bool()
                .expect("a flag"),
            "somebody else's message was never taken"
        );
    }

    /// **A mistyped colleague is a near miss, not an empty outbox.** This
    /// verb's whole job is answering "did my report land", so answering
    /// `count: 0` for a typo says "no, it did not" — and the sender writes it
    /// again, leaving duplicate mail with the original still unprocessed.
    #[tokio::test]
    async fn a_mistyped_addressee_is_blocked_with_candidates_rather_than_answering_empty() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "epsilon").await;
        send(&jojobot, "epsilon", "otto", "the kiln slice is done").await;

        let body = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    sender: Some("bot:otto".into()),
                    to: Some("epsilo".into()),
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("a near miss is an answer, not an error"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_ne!(body["count"], 0, "…and never a confident zero: {body}");
        let handles: Vec<&str> = body["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .map(|c| c["handle"].as_str().expect("a handle"))
            .collect();
        assert!(
            handles.contains(&"bot:epsilon"),
            "the colleague they meant is named: {body}"
        );
    }

    /// **Narrowing to a colleague costs ONE read of the board.** The name is
    /// turned into a box by asking who owns what, and a box that came back from
    /// that read exists by construction — so any further check that it exists
    /// fetches the whole table to answer a question already answered.
    ///
    /// Measured as the difference against the same call without `to`, so it
    /// states what narrowing costs rather than counting whatever this verb
    /// happens to do for its own reasons. Paired with both answers, because a
    /// call that returned nothing would read the board cheaply too.
    #[tokio::test]
    async fn narrowing_to_a_colleague_costs_one_read_of_the_board() {
        let (jojobot, board) = counting_handler();
        make_box(&jojobot, "epsilon").await;
        send(&jojobot, "epsilon", "otto", "the kiln slice is done").await;
        let asking = |to: Option<String>| ListSentArgs {
            limit: None,
            sender: Some("bot:otto".into()),
            to,
            include_bodies: None,
            sid: None,
        };

        let before = board.listings();
        let everywhere = json_of(
            &jojobot
                .list_sent(Parameters(asking(None)))
                .await
                .expect("list_sent ok"),
        );
        let unscoped = board.listings() - before;

        let before = board.listings();
        let narrowed = json_of(
            &jojobot
                .list_sent(Parameters(asking(Some("epsilon".into()))))
                .await
                .expect("list_sent ok"),
        );
        let scoped = board.listings() - before;

        assert_eq!(everywhere["count"], 1, "the message is there: {everywhere}");
        assert_eq!(
            narrowed["count"], 1,
            "…and narrowing to its box still finds it: {narrowed}"
        );
        assert_eq!(narrowed["mailbox"], "epsilon", "{narrowed}");
        assert_eq!(
            scoped - unscoped,
            1,
            "naming a colleague costs the one read that resolves them"
        );
    }

    /// **Something jojobot cannot read is not a message that was never sent.**
    /// The scan cannot parse a quarantined item, so it leaves it out — and this
    /// verb would then answer "no, your report never landed" while the report
    /// sits right there, unreadable.
    #[tokio::test]
    async fn list_sent_surfaces_what_it_cannot_read_rather_than_answering_no() {
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(boxes.clone());
        make_box(&jojobot, "pm").await;
        boxes.quarantine(
            &MailboxName("pm".into()),
            &MessageId("4212".into()),
            "its row on the page cannot be read — a state or a sender has been edited past parsing",
        );

        let body = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    sender: Some("dev (implementer)".into()),
                    to: None,
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(body["count"], 0, "nothing readable is theirs");
        assert_eq!(
            body["unreadable"][0]["mailbox"], "pm",
            "…but what cannot be read is not silence: {body}"
        );
        assert_eq!(body["unreadable"][0]["ids"][0], "4212");
        assert!(
            body["unreadable_note"]
                .as_str()
                .is_some_and(|n| n.contains("repair")),
            "…and it says what fixes it: {body}"
        );
    }

    /// Ids are minted as decimal counters, so ordering them as text puts `9`
    /// after `10`. Both other sort sites in this subsystem compare them as
    /// numbers on purpose; this one did not.
    #[tokio::test]
    async fn list_sent_breaks_a_tie_on_the_id_as_a_number() {
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(boxes.clone());
        make_box(&jojobot, "pm").await;
        // **Seeded through the store, with ONE instant across all ten.** The
        // handler stamps `now()` per call, so posting through it never produces
        // the tie this sorts on and the tie-break would go unexercised.
        let at = jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        for n in 1..=10 {
            boxes
                .post_message(NewMessage {
                    mailbox: MailboxName("pm".into()),
                    body: format!("report {n}"),
                    subject: None,
                    sender: "dev (implementer)".into(),
                    sent_at: at,
                    in_reply_to: None,
                })
                .await
                .expect("post ok");
        }

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    sender: Some("dev (implementer)".into()),
                    to: None,
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        let first = sent["messages"][0]["id"].as_str().expect("an id");
        assert_eq!(first, "10", "the newest is id 10, not id 9: {sent}");
    }

    /// Asking for the bodies gets them — the elision is a default, not a rule.
    /// **A limit cuts the list and says what it cut.**
    ///
    /// One box's worth of sent mail was 144 messages and about 117,000
    /// characters with the bodies already left out, which is an answer nobody
    /// can read and every caller pays for. So the list is sized, newest first.
    ///
    /// **What is left out is counted rather than dropped.** A short list with
    /// no marker reads as "that is all there is", which is the one wrong thing
    /// a truncation can say — and the marker is absent when nothing was cut,
    /// rather than present saying zero.
    #[tokio::test]
    async fn a_limit_cuts_the_list_and_names_what_it_left_out() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        for n in 0..4 {
            send(&jojobot, "pm", "otto", &format!("report {n}")).await;
        }

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: Some(2),
                    sender: None,
                    to: None,
                    include_bodies: None,
                    sid: Some(as_bot(&jojobot, "otto")),
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(sent["count"], 2, "the caller got what it asked for: {sent}");
        assert_eq!(sent["sent_total"], 4, "…out of what there is: {sent}");
        assert_eq!(sent["not_shown"]["count"], 2);
        assert!(
            sent["not_shown"]["how_to_proceed"]
                .as_str()
                .expect("a way on")
                .contains("limit"),
            "a cut list says how to see the rest: {sent}"
        );

        // **The newest, not the first two the store handed over.** A cut that
        // kept the oldest would satisfy every count above.
        let kept: Vec<&str> = sent["messages"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|m| m["body_head"].as_str().expect("a head"))
            .collect();
        assert!(
            kept.iter().any(|h| h.contains("report 3"))
                && kept.iter().any(|h| h.contains("report 2")),
            "the newest two came back: {kept:?}"
        );

        // **And a list that fits carries no marker at all**, rather than one
        // saying nothing was left out.
        let whole = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    sender: None,
                    to: None,
                    include_bodies: None,
                    sid: Some(as_bot(&jojobot, "otto")),
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(whole["count"], 4);
        assert!(
            whole["not_shown"].is_null(),
            "nothing was cut, so nothing says it was: {whole}"
        );
    }

    #[tokio::test]
    async fn a_sender_can_ask_for_the_bodies_of_their_own_mail() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        send(&jojobot, "pm", "otto", "the kiln slice is done").await;

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    sender: Some("bot:otto".into()),
                    to: None,
                    include_bodies: Some(true),
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(sent["messages"][0]["body"], "the kiln slice is done");
        assert!(
            sent["messages"][0]["body_elided"].is_null(),
            "nothing was elided to announce"
        );
    }
}

//! `list_mailboxes` — Every box, and what is waiting in the ones you drain.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `list_mailboxes`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListMailboxesArgs {
    /// Your session id. The boxes your bot owns come back with their counts;
    /// every other box comes back as a name only. Omit it and you own nothing,
    /// which is right for a caller that only posts.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Which boxes a caller drains, and **whether jojobot could tell**.
///
/// The two are separate answers on purpose. "You drain none of these" and
/// "jojobot cannot read the store that says which you drain" produce the same
/// listing and mean opposite things, and a caller acts on both.
/// **Ownership is never unknown here, and that is why there is no flag.** This
/// used to carry a `known: bool`, because ownership was a read of Memory and
/// the mailbox listing could arrive with nobody able to say whose was whose.
/// The two are one read now: whoever renders a listing has already answered the
/// ownership question, so the flag could only ever say `true` where it appeared
/// — it was rendered inside the `Ok` arm of the very read whose `Err` arm set
/// it false.
pub(crate) struct Ownership {
    /// The boxes this caller drains. Empty when they drain none.
    mine: Vec<String>,
}

impl Ownership {
    pub(crate) fn known(mine: Vec<String>) -> Self {
        Ownership { mine }
    }

    /// Whether this caller drains this box — and so whether its counts are
    /// theirs to see. **One question, not two.** It used to be two: a box
    /// nobody drained had no queue to shield, so its counts went to everybody.
    /// A box has an owner by construction now, so that second case cannot
    /// arise.
    pub(crate) fn drains(&self, name: &str) -> bool {
        self.mine.iter().any(|m| m == name)
    }

    /// Which of the boxes actually on the board this answer counted.
    pub(crate) fn shown_for(&self, boxes: &[Mailbox]) -> Vec<String> {
        boxes
            .iter()
            .map(|b| b.name.as_str())
            .filter(|name| self.drains(name))
            .map(str::to_string)
            .collect()
    }

    /// The clause that says what this listing's counts mean, including when it
    /// cannot say.
    pub(crate) fn note(&self) -> &'static str {
        "Counts are shown for the boxes you drain. A box somebody else works is listed by \
         name only — it exists and you can post into it; what is waiting in it belongs to \
         whoever works it."
    }
}

impl Jojobot {
    /// Which boxes this caller drains, **off a listing already in hand**.
    ///
    /// **Ownership is a read of the boxes themselves, never an ACL and no
    /// longer a read of Memory**: a box states its one owner, so the answer is
    /// in the same listing being rendered. It used to be a `mailbox:` claim on
    /// the bot's own entity record, which is why this once asked the entity
    /// index — and why there was a separate `boxes_drained_by` that went and
    /// fetched a listing of its own. Two reads of one world could disagree,
    /// and the disagreement rendered as "jojobot cannot tell who drains what"
    /// beside a listing that plainly said. One read, one answer.
    ///
    /// A caller that names no bot drains nothing — the right answer for a pure
    /// sender, and for an anonymous `start_here`.
    pub(crate) fn ownership_of(
        &self,
        boxes: &[mailbox::Mailbox],
        named: Option<&EntityId>,
    ) -> Ownership {
        // **Whoever the caller says they are, and nobody by default.** There is
        // no connection to fall back to any more: a caller with no handle owns
        // nothing, which is exactly right for one that only posts.
        let bot = named.cloned();
        // **Every box has an owner now**, so the old "a box nobody owns has no
        // queue to protect" case cannot arise: there is no unclaimed box to
        // leave visible to everybody. The scoping is therefore simply what it
        // always meant — a caller sees the counts of the boxes it drains, and
        // every other box by name alone.
        let Some(bot) = bot else {
            return Ownership::known(Vec::new());
        };
        Ownership::known(
            boxes
                .iter()
                .filter(|b| b.owner == bot)
                .map(|b| b.name.to_string())
                .collect(),
        )
    }
}

/// Every mailbox, with what is new, seen, and handled in each.
#[tool_router(router = list_mailboxes_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Every mailbox and what is waiting in it: new (left, never delivered) · \
                       read (delivered, nobody has finished with it) · processed (acted on — \
                       terminal, an archive; nothing is ever deleted). Each box also reports \
                       any items that could NOT be read as messages: they are counted nowhere, \
                       delivered nowhere, and cannot be processed, so this is the only place \
                       their existence shows — their ids are listed, and repairing one takes a \
                       person. If a message somebody expected is missing, look here before \
                       concluding it was never sent, and say what you find. COUNTS ARE FOR YOUR \
                       OWN BOXES: the `sid` you pass says which bot is asking, and the boxes that \
                       bot owns come back with their per-state counts; every other box comes back \
                       as a NAME ONLY, marked `yours: false`. You can still see that a box \
                       EXISTS — which is what you need to post into it — but not what is waiting \
                       in somebody else's, because that is their queue to work and not yours to \
                       weigh. Call without a `sid` and you own nothing — exactly right for a \
                       caller that only posts."
    )]
    pub(crate) async fn list_mailboxes(
        &self,
        Parameters(args): Parameters<ListMailboxesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let named = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller.map(|c| c.bot),
            Err(refused) => return Ok(refused),
        };
        let boxes = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(mailbox_error)?;
        let mine = self.ownership_of(&boxes, named.as_ref());
        let body = serde_json::json!({
            "count": boxes.len(),
            "counts_shown_for": mine.shown_for(&boxes),
            "note": mine.note(),
            "mailboxes": boxes
                .iter()
                .map(|b| {
                    if mine.drains(b.name.as_str()) {
                        let mut body = mailbox_json(b);
                        if let Some(obj) = body.as_object_mut() {
                            obj.insert("yours".into(), true.into());
                        }
                        body
                    } else {
                        // **Existence, not state.** The name is what a writer
                        // needs; the counts are what posed "is that one mine?"
                        // Quarantine stays: it is a fault on the board rather
                        // than somebody's queue, and this listing is the only
                        // place it shows.
                        serde_json::json!({
                            "name": b.name.as_str(),
                            "yours": false,
                            "counts": serde_json::Value::Null,
                            "counts_elided": true,
                            "quarantined": quarantined_json(b),
                        })
                    }
                })
                .collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// **Existence is public; what is waiting in somebody's queue is not.**
    /// Live report: sender bots posting into boxes they do not drain kept
    /// narrating "there is an unread message there that is not mine to pick up"
    /// — correct restraint, and attention spent on a question that should never
    /// have been posed. The affordance posed it: every box's per-state counts
    /// were shown to everybody, and the own-box norm then had to suppress in
    /// prose what the payload kept suggesting.
    ///
    /// Names stay visible, because a writer needs them — `post_message` must
    /// name an existing box, and a near-miss comes back with candidates.
    #[tokio::test]
    async fn counts_are_shown_for_the_boxes_you_drain_and_names_for_the_rest() {
        let jojobot = mailbox_handler();
        // A box IS its bot: `gamma` the bot drains `gamma` the box, so the two
        // names in this test are two identities rather than a bot and a label.
        make_bot(&jojobot, "gamma").await;
        // **A second bot that drains the other box** — without one, "your boxes"
        // and "every box" are the same set and the scoping proves nothing.
        make_bot(&jojobot, "delta").await;
        send(&jojobot, "gamma", "delta", "your hand-off").await;
        send(&jojobot, "delta", "sigma", "not your business").await;

        let listed = drains(&jojobot, "gamma").await;
        assert_eq!(listed["count"], 2, "every box is still LISTED: {listed}");
        // **No `ownership_known` flag.** It could only ever say `true` here: it
        // was rendered inside the `Ok` arm of the very read whose `Err` arm was
        // the only thing that set it false. A field that cannot vary is a
        // question a reader branches on and learns nothing from.
        assert!(
            listed.get("ownership_known").is_none(),
            "a flag that cannot be false is not an answer: {listed}"
        );
        assert_eq!(
            listed["counts_shown_for"],
            serde_json::json!(["gamma"]),
            "…and the answer says whose counts these are: {listed}"
        );

        let by_name = |name: &str| -> serde_json::Value {
            listed["mailboxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .find(|b| b["name"] == name)
                .expect("the box")
                .clone()
        };

        let mine = by_name("gamma");
        assert_eq!(mine["yours"], true);
        assert_eq!(mine["counts"]["new"], 1, "my own queue, in full: {mine}");

        let theirs = by_name("delta");
        assert_eq!(
            theirs["name"], "delta",
            "it EXISTS — post_message needs the name"
        );
        assert_eq!(theirs["yours"], false);
        assert!(
            theirs["counts"].is_null(),
            "…and its queue is not mine to weigh: {theirs}"
        );
        assert_eq!(theirs["counts_elided"], true, "elided, never silently");
    }

    /// **A quarantined card is visible on the wire, and it is not a count of
    /// zero.** A card jojobot cannot read is invisible to every other verb —
    /// not counted, not delivered, not processable — so this field is the only
    /// place a caller learns it exists at all. Rendering it wrong reads as an
    /// empty, healthy box.
    #[tokio::test]
    async fn a_quarantined_card_is_rendered_with_its_count_and_its_ids() {
        let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(store.clone());
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId("4212".into()),
            "its row on the page cannot be read — a state or a sender has been edited past parsing",
        );

        let listed = drains(&jojobot, "inbox").await;
        let inbox = &listed["mailboxes"][0];
        assert_eq!(inbox["quarantined"]["count"], 1, "got {listed}");
        assert_eq!(inbox["quarantined"]["card_ids"][0], "4212");
        assert_eq!(
            inbox["counts"]["total"], 1,
            "a quarantined card is not a message and is never counted as one: {listed}"
        );
    }

    /// The other half, and the one the scoping exists for: a box somebody else
    /// drains comes back to an anonymous caller as a name only.
    #[tokio::test]
    async fn an_anonymous_caller_sees_no_counts_for_a_box_somebody_drains() {
        let jojobot = handler();
        make_box(&jojobot, "dev").await;
        send(&jojobot, "dev", "delta", "your hand-off").await;

        let listed = json_of(
            &jojobot
                .list_mailboxes(Parameters(ListMailboxesArgs { sid: None }))
                .await
                .expect("list ok"),
        );
        let dev = listed["mailboxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .find(|b| b["name"] == "dev")
            .expect("the box");
        assert_eq!(dev["yours"], false);
        assert!(dev["counts"].is_null(), "somebody drains this one: {dev}");
        assert_eq!(dev["counts_elided"], true, "elided, never silently");
        assert_eq!(
            listed["counts_shown_for"],
            serde_json::json!([]),
            "…and the answer names what it counted: {listed}"
        );
    }
}

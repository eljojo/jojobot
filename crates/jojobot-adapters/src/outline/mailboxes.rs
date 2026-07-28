//! The Mailboxes adapter — one page per box, a message a row on it.
//!
//! The sessions precedent, applied to mail: truth in a table that is rewritten
//! in place, immutable content in fenced blocks that are appended. The encoding
//! and the live-API findings behind it are in
//! [`mailbox_codec`](super::mailbox_codec).
//!
//! # Where a box's page sits, and what says which box it is
//!
//! **The `name:` line, and only that.** The port is name-keyed from end to end
//! and nothing in it — not `Mailboxes`, not `NewMessage`, not `Mailbox` — names
//! an owner, because ownership is a claim on the owner's own record and this
//! context is deliberately ignorant of it.
//!
//! The page is nonetheless filed **under whichever entity claims the box**, when
//! one does, so a bot's mail sits under the bot in a wiki a human reads. That is
//! placement, not permission: nothing here refuses a caller, checks a claim, or
//! behaves differently for an unclaimed box. It is the tree slice's rule again —
//! the line is the truth, the position is navigability.
//!
//! # The write lock is Memory's lock
//!
//! Same reason as sessions: three stores now write different documents in one
//! collection, so a mutex each would exclude nobody.

use std::sync::Arc;

use async_trait::async_trait;

use jojobot_domain::mailbox::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, MessageState, NewMessage, StateCounts, guard, message_title, normalize_body,
    normalize_notes, normalize_subject, validate_body, validate_mailbox_name, validate_message_id,
    validate_notes, validate_sender, validate_subject,
};
use jojobot_domain::memory::MemoryError;

use super::api::{DocRec, OutlineApi};
use super::mailbox_codec::{
    Row, message, next_message_id, parse_bodies, parse_name, parse_rows, render_body, seeded_page,
    with_rows_replaced,
};
use super::{Workspace, parse_entity};

/// The real Mailboxes adapter, over Outline.
pub struct OutlineMailboxes {
    ws: Arc<Workspace>,
}

impl OutlineMailboxes {
    pub(super) fn new(ws: Arc<Workspace>) -> Self {
        Self { ws }
    }

    fn api(&self) -> &dyn OutlineApi {
        self.ws.api()
    }

    async fn collection(&self) -> Result<String, MailboxError> {
        self.ws.resolve_collection().await.map_err(store)
    }

    /// Every mailbox page in the collection, with the box each holds.
    async fn pages(&self, collection_id: &str) -> Result<Vec<(MailboxName, DocRec)>, MailboxError> {
        let mut found: Vec<(MailboxName, DocRec)> = self
            .ws
            .all_docs(collection_id)
            .await
            .map_err(store)?
            .into_iter()
            .filter_map(|d| parse_name(&d.text).map(|n| (n, d)))
            .collect();
        // Oldest wins where a double-create left two pages for one box, so a
        // box's mail never forks across them.
        found.sort_by(|a, b| {
            a.1.created_at
                .cmp(&b.1.created_at)
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        found.dedup_by(|a, b| a.0 == b.0);
        Ok(found)
    }

    async fn page_of(
        &self,
        collection_id: &str,
        name: &MailboxName,
    ) -> Result<Option<DocRec>, MailboxError> {
        Ok(self
            .pages(collection_id)
            .await?
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d))
    }

    /// The names of every box that exists — what the guard screens against.
    async fn names(&self, collection_id: &str) -> Result<Vec<MailboxName>, MailboxError> {
        Ok(self
            .pages(collection_id)
            .await?
            .into_iter()
            .map(|(n, _)| n)
            .collect())
    }

    /// Every message on one page, assembled from its rows and bodies.
    ///
    /// **A row whose body is missing still reads**, with an empty body: the row
    /// is the message's existence and the body is its content, and losing the
    /// second is not grounds for pretending the first never happened.
    fn assemble(name: &MailboxName, doc: &DocRec) -> Vec<Message> {
        let bodies = parse_bodies(&doc.text);
        let (rows, _) = parse_rows(&doc.text);
        rows.iter()
            .map(|row| {
                let body = bodies
                    .iter()
                    .find(|(id, _)| id == &row.id)
                    .map(|(_, b)| b.clone())
                    .unwrap_or_default();
                message(name, row, body)
            })
            .collect()
    }

    /// Find the page holding a message, and the message itself.
    async fn locate(
        &self,
        collection_id: &str,
        id: &MessageId,
    ) -> Result<(MailboxName, DocRec, Message), MailboxError> {
        validate_message_id(id)?;
        for (name, doc) in self.pages(collection_id).await? {
            if let Some(found) = Self::assemble(&name, &doc)
                .into_iter()
                .find(|m| &m.id == id)
            {
                return Ok((name, doc, found));
            }
            // Looked at, and unreadable — a different answer from "no such
            // message", and the one that sends a person to the right page.
            if parse_rows(&doc.text).1.contains(id) {
                return Err(MailboxError::Quarantined {
                    attempted: id.to_string(),
                    reason: format!(
                        "its row on the page for {name} cannot be read — a state, a sender or a                          timestamp has been edited past parsing. A person has to repair the row"
                    ),
                });
            }
        }
        Err(MailboxError::UnknownMessage {
            attempted: id.to_string(),
        })
    }

    async fn reread(&self, doc: &DocRec, verb: &str) -> Result<DocRec, MailboxError> {
        let collection_id = self.collection().await?;
        self.ws
            .all_docs(&collection_id)
            .await
            .map_err(store)?
            .into_iter()
            .find(|d| d.id == doc.id)
            .ok_or_else(|| store_msg(format!("the mailbox page vanished mid-{verb}")))
    }

    async fn put(&self, doc: &DocRec, text: &str, verb: &str) -> Result<DocRec, MailboxError> {
        self.api()
            .update_document(&doc.id, text)
            .await
            .map_err(store)?;
        self.reread(doc, verb).await
    }

    async fn restore(&self, doc: &DocRec, verb: &str) -> String {
        match self.api().update_document(&doc.id, &doc.text).await {
            Ok(()) => format!("the page was restored to its state before this {verb}"),
            Err(e) => format!("AND restoring the page failed ({e}) — the page is mid-{verb}"),
        }
    }

    /// Rewrite the table on one page, then read the touched messages back.
    /// Shared by every verb that moves a state, so none can drift on what
    /// moving one means.
    async fn rewrite_rows(
        &self,
        name: &MailboxName,
        doc: &DocRec,
        verb: &str,
        edit: impl FnOnce(&mut Vec<Row>),
    ) -> Result<Vec<Message>, MailboxError> {
        let (mut rows, _) = parse_rows(&doc.text);
        edit(&mut rows);
        let updated = with_rows_replaced(&doc.text, &rows)
            .ok_or_else(|| store_msg(format!("the page for {name} has no table")))?;
        let seen = self.put(doc, &updated, verb).await?;

        let back = Self::assemble(name, &seen);
        for wanted in &rows {
            let Some(got) = back.iter().find(|m| m.id == wanted.id) else {
                let restored = self.restore(doc, verb).await;
                return Err(store_msg(format!(
                    "message {} did not read back after {verb}; {restored}",
                    wanted.id
                )));
            };
            if got.state != wanted.state || got.notes != normalize_notes(wanted.notes.as_deref()) {
                let restored = self.restore(doc, verb).await;
                return Err(store_msg(format!(
                    "message {} read back changed after {verb}: wrote {wanted:?}, read \
                     {got:?}; {restored}",
                    wanted.id
                )));
            }
        }
        Ok(back)
    }
}

fn store(e: MemoryError) -> MailboxError {
    match e {
        MemoryError::NotConfigured(m) => MailboxError::NotConfigured(m),
        other => MailboxError::Store(other.to_string()),
    }
}

fn store_msg(message: String) -> MailboxError {
    MailboxError::Store(message)
}

#[async_trait]
impl Mailboxes for OutlineMailboxes {
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        create_new: bool,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        validate_mailbox_name(name)?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;

        let existing = self.names(&collection_id).await?;
        if let guard::Decision::Block(candidates) =
            guard::decide_create(name, &existing, create_new)
        {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }

        // **Filed under whoever claims it, when anybody does.** Placement only:
        // an unclaimed box is created at the top of the collection and works
        // exactly the same. Nothing here reads a claim to decide whether a
        // caller may do something — only where the page goes.
        let under = self
            .ws
            .all_docs(&collection_id)
            .await
            .map_err(store)?
            .into_iter()
            .find(|d| {
                parse_entity(&d.text)
                    .and_then(|e| e.mailbox)
                    .is_some_and(|m| m == name.as_str())
            })
            .map(|d| d.id);

        self.api()
            .create_document(
                &collection_id,
                name.as_str(),
                &seeded_page(name),
                under.as_deref(),
            )
            .await
            .map_err(store)?;

        self.page_of(&collection_id, name)
            .await?
            .ok_or_else(|| store_msg(format!("the page for {name} vanished after create")))?;
        Ok(Guarded::Written(Mailbox {
            name: name.clone(),
            counts: StateCounts::default(),
            quarantined: Vec::new(),
        }))
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        let collection_id = self.collection().await?;
        Ok(self
            .pages(&collection_id)
            .await?
            .into_iter()
            .map(|(name, doc)| {
                let (rows, quarantined) = parse_rows(&doc.text);
                let mut counts = StateCounts::default();
                for row in &rows {
                    counts.add(row.state);
                }
                Mailbox {
                    name,
                    counts,
                    quarantined,
                }
            })
            .collect())
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        let collection_id = self.collection().await?;
        Ok(self
            .pages(&collection_id)
            .await?
            .iter()
            .flat_map(|(name, doc)| Self::assemble(name, doc))
            .collect())
    }

    async fn post_message(&self, new: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        validate_mailbox_name(&new.mailbox)?;
        validate_body(&new.body)?;
        validate_sender(&new.sender)?;
        validate_subject(new.subject.as_deref())?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;

        let existing = self.names(&collection_id).await?;
        if let guard::Decision::Block(candidates) = guard::decide_existing(&new.mailbox, &existing)
        {
            return Ok(Guarded::Blocked {
                attempted: new.mailbox,
                candidates,
            });
        }
        // A reply names a message, so that message must exist — the same rule
        // every other reference here follows.
        if let Some(answering) = &new.in_reply_to {
            self.locate(&collection_id, answering).await?;
        }

        let doc = self
            .page_of(&collection_id, &new.mailbox)
            .await?
            .ok_or_else(|| store_msg(format!("the page for {} vanished", new.mailbox)))?;

        let id = next_message_id(&doc.text, &new.mailbox);
        let body = normalize_body(&new.body);

        // **The body first, then the row.** Two writes, and if only one lands
        // the survivor should be the harmless one: a body with no row is an
        // orphan block nothing reads, where a row with no body is a message
        // that exists and says nothing.
        self.api()
            .append_document(&doc.id, &render_body(&id, &body))
            .await
            .map_err(store)?;
        let doc = self.reread(&doc, "post_message").await?;

        let row = Row {
            id: id.clone(),
            state: MessageState::New,
            sender: new.sender.trim().to_string(),
            sent_at: new.sent_at,
            subject: normalize_subject(new.subject.as_deref()),
            in_reply_to: new.in_reply_to,
            notes: None,
        };
        let (mut rows, _) = parse_rows(&doc.text);
        rows.push(row.clone());
        let updated = with_rows_replaced(&doc.text, &rows)
            .ok_or_else(|| store_msg(format!("the page for {} has no table", new.mailbox)))?;
        let seen = self.put(&doc, &updated, "post_message").await?;

        let back = Self::assemble(&new.mailbox, &seen)
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| store_msg(format!("message {id} did not read back")))?;
        if back.body != body || back.sender != row.sender || back.subject != row.subject {
            let restored = self.restore(&doc, "post_message").await;
            return Err(store_msg(format!(
                "message {id} read back changed: wrote {row:?} / {body:?}, read {back:?}; \
                 {restored}"
            )));
        }
        // The title is cosmetic and best-effort, exactly as an entity doc's is.
        let _ = message_title(&back.sender, back.subject.as_deref(), &back.body);
        Ok(Guarded::Written(back))
    }

    async fn read_mailbox(&self, name: &MailboxName) -> Result<Guarded<Delivery>, MailboxError> {
        validate_mailbox_name(name)?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;

        let existing = self.names(&collection_id).await?;
        if let guard::Decision::Block(candidates) = guard::decide_existing(name, &existing) {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }
        let doc = self
            .page_of(&collection_id, name)
            .await?
            .ok_or_else(|| store_msg(format!("the page for {name} vanished")))?;

        // Everything unprocessed — including what a previous read already
        // handed over, which is the leftover a crashed consumer left behind.
        //
        // **Oldest by `sent_at`, not by where the row sits.** The two disagree
        // whenever mail is posted out of order, and the instant is what
        // "oldest" means; row order is only where the table happened to grow.
        // The id breaks a tie so the order is total and two reads agree.
        let mut unprocessed: Vec<Message> = Self::assemble(name, &doc)
            .into_iter()
            .filter(|m| m.state != MessageState::Processed)
            .collect();
        unprocessed.sort_by(|a, b| a.sent_at.cmp(&b.sent_at).then_with(|| a.id.cmp(&b.id)));
        let owed: Vec<(MessageId, bool)> = unprocessed
            .into_iter()
            .map(|m| (m.id, m.state == MessageState::Read))
            .collect();

        let taking: Vec<MessageId> = owed.iter().map(|(id, _)| id.clone()).collect();
        let back = self
            .rewrite_rows(name, &doc, "read_mailbox", |rows| {
                for row in rows.iter_mut() {
                    if taking.contains(&row.id) {
                        row.state = MessageState::Read;
                    }
                }
            })
            .await?;

        let messages = owed
            .into_iter()
            .filter_map(|(id, seen_before)| {
                back.iter().find(|m| m.id == id).map(|m| Delivered {
                    message: m.clone(),
                    seen_before,
                })
            })
            .collect();
        Ok(Guarded::Written(Delivery {
            mailbox: name.clone(),
            messages,
        }))
    }

    async fn read_message(&self, id: &MessageId) -> Result<Delivered, MailboxError> {
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;
        let (name, doc, found) = self.locate(&collection_id, id).await?;

        // **Processed is terminal, so reading one is reading an archive.** It
        // comes back as it is, flagged, and nothing moves.
        if found.state == MessageState::Processed {
            return Ok(Delivered {
                message: found,
                seen_before: true,
            });
        }
        let seen_before = found.state == MessageState::Read;
        let target = id.clone();
        let back = self
            .rewrite_rows(&name, &doc, "read_message", |rows| {
                if let Some(row) = rows.iter_mut().find(|r| r.id == target) {
                    row.state = MessageState::Read;
                }
            })
            .await?;
        let message = back
            .into_iter()
            .find(|m| &m.id == id)
            .ok_or_else(|| store_msg(format!("message {id} did not read back")))?;
        Ok(Delivered {
            message,
            seen_before,
        })
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        validate_notes(notes)?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;
        let (name, doc, _) = self.locate(&collection_id, id).await?;

        let target = id.clone();
        let recorded = normalize_notes(notes);
        let back = self
            .rewrite_rows(&name, &doc, "mark_processed", |rows| {
                if let Some(row) = rows.iter_mut().find(|r| r.id == target) {
                    row.state = MessageState::Processed;
                    if recorded.is_some() {
                        row.notes = recorded.clone();
                    }
                }
            })
            .await?;
        back.into_iter()
            .find(|m| &m.id == id)
            .ok_or_else(|| store_msg(format!("message {id} did not read back")))
    }
}

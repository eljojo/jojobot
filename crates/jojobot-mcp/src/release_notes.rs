//! **jojobot tells each bot what a deploy changed.** A change bots can use ships
//! with a note, one file in `release-notes/`, written for the roles that run
//! here. At start, jojobot leaves ONE message in each bot's box listing the notes
//! that box has not been sent, from the system sender, so nobody relays them by
//! hand.
//!
//! **The mailbox is the ledger.** A message lists the ids of the notes it carries
//! on its last line, `Notes: a, b`, and a note is sent to a box when no message
//! from the system sender in that box lists it. Nothing is added to the store and
//! nothing is migrated, and a restart sends nothing twice.
//!
//! The pass never stops a boot: a read or a post that fails warns, and the next
//! start sends what is still unsent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jojobot_domain::clock::Clock;
use jojobot_domain::mailbox::{Guarded, Mailboxes, NewMessage};
use jojobot_domain::memory::{EntityKind, Memory, REPORTS_TO};

/// **The sender of a message jojobot itself leaves.** Reserved by being no
/// handle: a session's messages are sent under its bot, and a bot's handle has a
/// kind in front of it. The edge derives a session's sender and never takes this
/// from a caller.
pub const SYSTEM_SENDER: &str = "jojobot";

/// The prefix of the last line of a message, which names the notes it carries.
const NOTES_LINE: &str = "Notes: ";

/// One note, read from its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Note {
    /// The file's name without its extension. A note is sent by this.
    pub(crate) id: String,
    /// One line saying what the note is about.
    pub(crate) title: String,
    /// Who the note is for, by role or by kind of work.
    pub(crate) audience: String,
    /// What a bot can now do, an example, and what to do.
    pub(crate) body: String,
}

/// **The notes this build ships**, by id and text, in the order they are sent.
/// A file in `release-notes/` that is not listed here is never sent, and a test
/// fails for it.
pub(crate) const SHIPPED: &[(&str, &str)] = &[(
    "release-notes",
    include_str!("../release-notes/release-notes.md"),
)];

/// The three parts every note holds, in the order it holds them.
const SECTIONS: [&str; 3] = ["What you can now do", "Example", "What to do"];

/// **A note read from its text**, or the reason it is not one: two headers,
/// `title:` and `for:`, a blank line, then the three sections. A note that breaks
/// the shape is refused whole, because a message built from half of one would be
/// sent and never corrected.
pub(crate) fn parse(id: &str, text: &str) -> Result<Note, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!("the id '{id}' is not a lowercase slug"));
    }
    let (head, body) = text
        .split_once("\n\n")
        .ok_or_else(|| format!("note '{id}' has no blank line after its headers"))?;
    let mut title = None;
    let mut audience = None;
    for line in head.lines() {
        match line.split_once(':') {
            Some(("title", value)) => title = Some(value.trim().to_string()),
            Some(("for", value)) => audience = Some(value.trim().to_string()),
            _ => {
                return Err(format!(
                    "note '{id}' has a header that is not title or for: {line}"
                ));
            }
        }
    }
    let title = title
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("note '{id}' has no title"))?;
    let audience = audience
        .filter(|a| !a.is_empty())
        .ok_or_else(|| format!("note '{id}' says nobody it is for"))?;
    // The three headings, each a whole line, in order.
    let mut headings = body.lines().filter(|line| line.starts_with("## "));
    for section in SECTIONS {
        if headings.next() != Some(&format!("## {section}")) {
            return Err(format!("note '{id}' has no '{section}' section, in order"));
        }
    }
    if headings.next().is_some() {
        return Err(format!("note '{id}' has a section after the three"));
    }
    if body.lines().any(|line| line.starts_with(NOTES_LINE)) {
        return Err(format!(
            "note '{id}' has a line that starts like the line that names the notes"
        ));
    }
    Ok(Note {
        id: id.to_string(),
        title,
        audience,
        body: body.trim().to_string(),
    })
}

/// **Every shipped note, read.** A note that does not read is warned about and
/// left out, so one bad file cannot stop the rest from being sent.
fn read_all(source: &[(&str, &str)]) -> Vec<Note> {
    let mut notes: Vec<Note> = Vec::new();
    for (id, text) in source {
        match parse(id, text) {
            Ok(note) if notes.iter().any(|seen| seen.id == note.id) => {
                tracing::warn!(note = %note.id, "RELEASE NOTE LISTED TWICE — the second is not sent");
            }
            Ok(note) => notes.push(note),
            Err(why) => tracing::warn!(note = %id, %why, "RELEASE NOTE UNREADABLE — not sent"),
        }
    }
    notes
}

/// **The message for these notes** to one bot: what it is and who it is from, each
/// note, the line naming the bot's manager when it has one, and the line that
/// names the notes, which is what a later start reads.
fn message_body(notes: &[&Note], manager: Option<&str>) -> String {
    let mut body = String::from(
        "This message comes from jojobot itself, not from a bot. It says what changed in the \
         software you run on. A reply to it goes nowhere.",
    );
    if let Some(manager) = manager {
        body.push_str(&format!(
            " Take questions about what a note means for your work to your manager, @{manager}."
        ));
    }
    for note in notes {
        body.push_str(&format!(
            "\n\n# {}\nFor: {}\n\n{}",
            note.title, note.audience, note.body
        ));
    }
    let ids: Vec<&str> = notes.iter().map(|note| note.id.as_str()).collect();
    body.push_str(&format!("\n\n{NOTES_LINE}{}", ids.join(", ")));
    body
}

/// The subject of a message that carries these notes.
fn subject(notes: &[&Note]) -> String {
    match notes {
        [only] => format!("What changed: {}", only.title),
        _ => format!("What changed: {} notes", notes.len()),
    }
}

/// **The ids of the notes a message lists**, read off its last line.
fn listed(body: &str) -> BTreeSet<String> {
    body.lines()
        .last()
        .and_then(|line| line.strip_prefix(NOTES_LINE))
        .map(|ids| ids.split(',').map(|id| id.trim().to_string()).collect())
        .unwrap_or_default()
}

/// **Send each bot the notes this build ships and its box has not been sent.**
/// Called at start by the one function the binary and the story harness both run.
pub async fn send_unsent_notes(
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
    clock: &Clock,
) {
    send_from(SHIPPED, memory, mailboxes, clock).await;
}

/// [`send_unsent_notes`] over the notes it is given, so a case can ship others.
pub(crate) async fn send_from(
    source: &[(&str, &str)],
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
    clock: &Clock,
) {
    let notes = read_all(source);
    if notes.is_empty() {
        return;
    }
    // Every read is made once, and a failed one ends the pass: sending on a ledger
    // that could not be read would send twice.
    let bots = match memory.list_entities(Some(EntityKind::BOT)).await {
        Ok(bots) => bots,
        Err(e) => {
            tracing::warn!(error = %e, "RELEASE NOTES NOT SENT — the bots could not be listed");
            return;
        }
    };
    let boxes = match mailboxes.list_mailboxes().await {
        Ok(boxes) => boxes,
        Err(e) => {
            tracing::warn!(error = %e, "RELEASE NOTES NOT SENT — the boxes could not be listed");
            return;
        }
    };
    let sent = match mailboxes.sent_by(&[SYSTEM_SENDER]).await {
        Ok(sent) => sent,
        Err(e) => {
            tracing::warn!(error = %e, "RELEASE NOTES NOT SENT — what was sent could not be read");
            return;
        }
    };
    let mut told: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for message in &sent {
        told.entry(message.mailbox.as_str().to_string())
            .or_default()
            .extend(listed(&message.body));
    }
    for bot in bots.iter().filter(|bot| bot.browsable()) {
        // A bot's own box, found by its owner and never made here: a bot without
        // one is a boot that has not healed it, and the next start sends.
        let Some(mailbox) = boxes.iter().find(|held| held.owner == bot.id) else {
            tracing::warn!(bot = %bot.id, "no release notes sent — the bot has no box yet");
            continue;
        };
        let already = told.get(mailbox.name.as_str());
        let unsent: Vec<&Note> = notes
            .iter()
            .filter(|note| !already.is_some_and(|ids| ids.contains(&note.id)))
            .collect();
        if unsent.is_empty() {
            continue;
        }
        let manager = match memory.fields(&bot.id).await {
            Ok(fields) => fields.get(REPORTS_TO).cloned(),
            Err(e) => {
                tracing::warn!(bot = %bot.id, error = %e, "no release notes sent — the bot could not be read");
                continue;
            }
        };
        let compose = |count: usize| NewMessage {
            mailbox: mailbox.name.clone(),
            body: message_body(&unsent[..count], manager.as_deref()),
            subject: Some(subject(&unsent[..count])),
            sender: SYSTEM_SENDER.to_string(),
            sent_at: clock.now(),
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        };
        // As many whole notes as one message can be handed whole; the rest wait
        // for the next start.
        let mut count = unsent.len();
        while count > 0
            && crate::mailboxes::post_message::body_room_beyond(&compose(count)).is_some()
        {
            count -= 1;
        }
        if count == 0 {
            tracing::warn!(bot = %bot.id, note = %unsent[0].id, "no release notes sent — the first note is longer than a message can be");
            continue;
        }
        match mailboxes.post_message(compose(count)).await {
            Ok(Guarded::Written(_)) => {
                tracing::info!(bot = %bot.id, notes = count, "release notes sent");
            }
            Ok(_) => tracing::warn!(bot = %bot.id, "release notes not sent — the post was blocked"),
            Err(e) => {
                tracing::warn!(bot = %bot.id, error = %e, "release notes not sent — the post failed")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{MailboxError, MailboxName, Message};
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{EntityId, NewEntity, NewFact};

    const ONE: &str = "title: Homer's list can be ticked\nfor: bots that keep lists\n\n## What you can now do\n\nTick an entry.\n\n## Example\n\nHomer ticks the tent.\n\n## What to do\n\nUpdate your process.";
    const TWO: &str = "title: Marge reads her own history\nfor: bots that keep records\n\n## What you can now do\n\nRead a run.\n\n## Example\n\nMarge reads run one.\n\n## What to do\n\nAsk your manager.";
    const THREE: &str = "title: Bart sends a note\nfor: every bot\n\n## What you can now do\n\nSend it.\n\n## Example\n\nBart sends it.\n\n## What to do\n\nRead it.";

    fn ports() -> (Arc<dyn Memory>, Arc<InMemoryMailboxes>) {
        (
            Arc::new(InMemoryMemory::booted()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
        )
    }

    /// A bot with its box, the two made together as the software makes them.
    async fn a_bot(
        memory: &Arc<dyn Memory>,
        mailboxes: &Arc<dyn Mailboxes>,
        slug: &str,
    ) -> EntityId {
        let id = EntityId(format!("bot:{slug}"));
        memory
            .add_entity(NewEntity::new(id.clone(), slug, "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("an empty board blocks nothing");
        mailboxes
            .create_mailbox(&MailboxName(slug.to_string()), &id, None)
            .await
            .expect("the box opens");
        id
    }

    async fn reports_to(memory: &Arc<dyn Memory>, bot: &EntityId, manager: &EntityId) {
        let mut fact = NewFact::about(
            bot.clone(),
            format!("{bot} reports to {manager}"),
            jiff::civil::date(2026, 10, 1),
        );
        fact.fields.insert(REPORTS_TO.into(), manager.to_string());
        fact.sets.insert(REPORTS_TO.into());
        memory.capture(fact).await.expect("capture ok");
    }

    async fn pass(
        notes: &[(&str, &str)],
        memory: &Arc<dyn Memory>,
        mailboxes: &Arc<dyn Mailboxes>,
    ) {
        send_from(notes, memory, mailboxes, &Clock::default()).await;
    }

    /// The messages the system sender left in one box, oldest first.
    async fn from_the_system(mailboxes: &Arc<dyn Mailboxes>, box_name: &str) -> Vec<Message> {
        mailboxes
            .sent_by(&[SYSTEM_SENDER])
            .await
            .expect("the sent mail reads")
            .into_iter()
            .filter(|message| message.mailbox.as_str() == box_name)
            .collect()
    }

    /// **The ruler, below the surface.** Two bots, two notes, the pass runs: each
    /// bot holds exactly one message from the system sender that lists both notes,
    /// Homer's names his manager and Marge's, who has none, names no one. A second
    /// pass sends nothing, a third note sends a message with only the third, and
    /// an archived bot is told nothing.
    #[tokio::test]
    async fn each_bot_is_sent_the_notes_it_has_not_been_sent_once() {
        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        let homer = a_bot(&memory, &mailboxes, "homer").await;
        let marge = a_bot(&memory, &mailboxes, "marge").await;
        let bart = a_bot(&memory, &mailboxes, "bart").await;
        reports_to(&memory, &homer, &marge).await;
        memory
            .archive_entity(&bart, "retired", None)
            .await
            .expect("archive ok");

        pass(&[("tick", ONE), ("history", TWO)], &memory, &mailboxes).await;
        let homers = from_the_system(&mailboxes, "homer").await;
        assert_eq!(homers.len(), 1, "one message, not one for each note");
        let body = &homers[0].body;
        assert!(
            body.contains("Homer's list can be ticked")
                && body.contains("Marge reads her own history")
        );
        assert_eq!(
            listed(body),
            ["history", "tick"].into_iter().map(String::from).collect()
        );
        assert!(
            body.contains("@bot:marge"),
            "Homer is told who his manager is: {body}"
        );
        let marges = from_the_system(&mailboxes, "marge").await;
        assert_eq!(marges.len(), 1, "Marge is sent her own");
        assert!(
            !marges[0].body.contains("manager,"),
            "Marge has no manager and so no line naming one: {}",
            marges[0].body
        );
        assert!(
            from_the_system(&mailboxes, "bart").await.is_empty(),
            "an archived bot is told nothing"
        );

        // ── a restart sends nothing, and a third note sends only the third ────
        pass(&[("tick", ONE), ("history", TWO)], &memory, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "homer").await.len(), 1);
        pass(
            &[("tick", ONE), ("history", TWO), ("send", THREE)],
            &memory,
            &mailboxes,
        )
        .await;
        let homers = from_the_system(&mailboxes, "homer").await;
        assert_eq!(homers.len(), 2);
        assert_eq!(
            listed(&homers[1].body),
            ["send".to_string()].into_iter().collect(),
            "only the note that was not sent"
        );
        assert!(!homers[1].body.contains("Homer's list can be ticked"));
    }

    /// **A bot far behind is sent as many whole notes as one message can be handed
    /// whole**, and the rest at the next start.
    #[tokio::test]
    async fn notes_that_do_not_fit_one_message_wait_for_the_next_start() {
        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        a_bot(&memory, &mailboxes, "homer").await;
        let long = |title: &str| {
            format!(
                "title: {title}\nfor: every bot\n\n## What you can now do\n\n{}\n\n## Example\n\nA line.\n\n## What to do\n\nRead it.",
                "word ".repeat(2_000)
            )
        };
        let (a, b, c) = (long("First"), long("Second"), long("Third"));
        let all: Vec<(&str, &str)> = vec![("a", a.as_str()), ("b", b.as_str()), ("c", c.as_str())];

        pass(&all, &memory, &mailboxes).await;
        let first = from_the_system(&mailboxes, "homer").await;
        assert_eq!(first.len(), 1);
        let sent = listed(&first[0].body);
        assert!(
            !sent.is_empty() && sent.len() < 3,
            "whole notes that fit, not all: {sent:?}"
        );

        pass(&all, &memory, &mailboxes).await;
        pass(&all, &memory, &mailboxes).await;
        let every: BTreeSet<String> = from_the_system(&mailboxes, "homer")
            .await
            .iter()
            .flat_map(|message| listed(&message.body))
            .collect();
        assert_eq!(every.len(), 3, "each note arrives, once: {every:?}");
        let messages = from_the_system(&mailboxes, "homer").await;
        let total: usize = messages
            .iter()
            .map(|message| listed(&message.body).len())
            .sum();
        assert_eq!(total, 3, "and none twice");
    }

    /// What a store that went away cannot read.
    #[derive(Clone, Copy, PartialEq)]
    enum Unread {
        Nothing,
        WhatWasSent,
    }

    /// A mailbox store that refuses the first post to one box, as a store that
    /// went away between two bots would, and that can refuse to read what was sent.
    struct FailsOnce {
        inner: Arc<InMemoryMailboxes>,
        box_name: &'static str,
        failed: std::sync::atomic::AtomicBool,
        unread: Unread,
    }

    #[async_trait::async_trait]
    impl Mailboxes for FailsOnce {
        async fn create_mailbox(
            &self,
            name: &MailboxName,
            owner: &EntityId,
            note: Option<&str>,
        ) -> Result<Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError> {
            self.inner.create_mailbox(name, owner, note).await
        }
        async fn repoint_owner(
            &self,
            from: &EntityId,
            to: &EntityId,
        ) -> Result<Option<jojobot_domain::mailbox::Mailbox>, MailboxError> {
            self.inner.repoint_owner(from, to).await
        }
        async fn list_mailboxes(
            &self,
        ) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
            self.inner.list_mailboxes().await
        }
        async fn post_message(&self, new: NewMessage) -> Result<Guarded<Message>, MailboxError> {
            if new.mailbox.as_str() == self.box_name
                && !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(MailboxError::InvalidMessage("the store went away".into()));
            }
            self.inner.post_message(new).await
        }
        async fn read_mailbox(
            &self,
            name: &MailboxName,
            taken_by: jojobot_domain::mailbox::TakenBy,
            acting: jojobot_domain::mailbox::Acting<'_>,
        ) -> Result<Guarded<jojobot_domain::mailbox::Delivery>, MailboxError> {
            self.inner.read_mailbox(name, taken_by, acting).await
        }
        async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
            self.inner.scan_messages().await
        }
        async fn message_by_id(
            &self,
            id: &jojobot_domain::mailbox::MessageId,
        ) -> Result<Option<Message>, MailboxError> {
            self.inner.message_by_id(id).await
        }
        async fn messages_by_id(
            &self,
            ids: &[jojobot_domain::mailbox::MessageId],
        ) -> Result<Vec<Message>, MailboxError> {
            self.inner.messages_by_id(ids).await
        }
        async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
            if self.unread == Unread::WhatWasSent {
                return Err(MailboxError::InvalidMessage("the store went away".into()));
            }
            self.inner.sent_by(senders).await
        }
        async fn read_message(
            &self,
            id: &jojobot_domain::mailbox::MessageId,
            acting: jojobot_domain::mailbox::Acting<'_>,
        ) -> Result<jojobot_domain::mailbox::Delivered, MailboxError> {
            self.inner.read_message(id, acting).await
        }
        async fn mark_processed(
            &self,
            id: &jojobot_domain::mailbox::MessageId,
            by: &MailboxName,
            notes: Option<&str>,
            acting: jojobot_domain::mailbox::Acting<'_>,
        ) -> Result<Message, MailboxError> {
            self.inner.mark_processed(id, by, notes, acting).await
        }
        async fn quarantine(
            &self,
            id: &jojobot_domain::mailbox::MessageId,
            by: &MailboxName,
            reason: &str,
            acting: jojobot_domain::mailbox::Acting<'_>,
        ) -> Result<jojobot_domain::mailbox::Quarantined, MailboxError> {
            self.inner.quarantine(id, by, reason, acting).await
        }
        async fn message_history(
            &self,
            id: &jojobot_domain::mailbox::MessageId,
        ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, MailboxError> {
            self.inner.message_history(id).await
        }
        async fn scan_message_events(
            &self,
        ) -> Result<
            Vec<(
                jojobot_domain::mailbox::MessageId,
                Vec<jojobot_domain::mailbox::MessageEvent>,
            )>,
            MailboxError,
        > {
            self.inner.scan_message_events().await
        }
    }

    /// **A post that fails between two bots stops nothing**: the pass goes on to
    /// the next bot, and a rerun sends only to the one that missed.
    #[tokio::test]
    async fn a_post_that_fails_between_two_bots_is_made_up_by_the_next_start() {
        let (memory, concrete) = ports();
        let unfailing: Arc<dyn Mailboxes> = concrete.clone();
        a_bot(&memory, &unfailing, "homer").await;
        a_bot(&memory, &unfailing, "marge").await;
        let mailboxes: Arc<dyn Mailboxes> = Arc::new(FailsOnce {
            inner: concrete,
            box_name: "homer",
            failed: std::sync::atomic::AtomicBool::new(false),
            unread: Unread::Nothing,
        });

        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert!(
            from_the_system(&mailboxes, "homer").await.is_empty(),
            "the failed post left nothing"
        );
        assert_eq!(
            from_the_system(&mailboxes, "marge").await.len(),
            1,
            "the bot after it was still sent"
        );

        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "homer").await.len(), 1);
        assert_eq!(
            from_the_system(&mailboxes, "marge").await.len(),
            1,
            "and the one that was sent is not sent again"
        );
    }

    /// **A bot with no box is skipped, and the bots after it are still sent.** A box
    /// is never made here: the next start sends once the boot has healed it.
    #[tokio::test]
    async fn a_bot_with_no_box_is_skipped_and_the_rest_are_sent() {
        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        memory
            .add_entity(NewEntity::new(
                EntityId("bot:moe".into()),
                "moe",
                "user-named",
            ))
            .await
            .expect("add ok")
            .written()
            .expect("an empty board blocks nothing");
        a_bot(&memory, &mailboxes, "marge").await;

        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(
            from_the_system(&mailboxes, "marge").await.len(),
            1,
            "the bot after the one with no box was still sent"
        );
        let boxes = mailboxes.list_mailboxes().await.expect("the boxes list");
        assert!(
            boxes.iter().all(|held| held.name.as_str() != "moe"),
            "no box was made for him"
        );
    }

    /// **A note longer than one message can be is sent to no one**, and the pass
    /// does not stop: a fitting note is sent at the next start.
    #[tokio::test]
    async fn a_note_longer_than_a_message_is_sent_to_no_one() {
        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        a_bot(&memory, &mailboxes, "homer").await;
        a_bot(&memory, &mailboxes, "marge").await;
        let huge = format!(
            "title: Huge\nfor: every bot\n\n## What you can now do\n\n{}\n\n## Example\n\nA line.\n\n## What to do\n\nRead it.",
            "word ".repeat(20_000)
        );

        pass(&[("huge", huge.as_str())], &memory, &mailboxes).await;
        for bot in ["homer", "marge"] {
            assert!(
                from_the_system(&mailboxes, bot).await.is_empty(),
                "{bot} was sent nothing"
            );
        }
        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "homer").await.len(), 1);
    }

    /// **A pass that cannot read what was sent sends nothing**, because sending on a
    /// ledger it could not read would send twice. The next start sends.
    #[tokio::test]
    async fn a_pass_that_cannot_read_what_was_sent_sends_nothing() {
        let (memory, concrete) = ports();
        let unfailing: Arc<dyn Mailboxes> = concrete.clone();
        a_bot(&memory, &unfailing, "homer").await;
        let failing: Arc<dyn Mailboxes> = Arc::new(FailsOnce {
            inner: concrete,
            box_name: "nobody",
            failed: std::sync::atomic::AtomicBool::new(false),
            unread: Unread::WhatWasSent,
        });

        pass(&[("tick", ONE)], &memory, &failing).await;
        assert!(
            from_the_system(&unfailing, "homer").await.is_empty(),
            "nothing was sent on a ledger that could not be read"
        );
        pass(&[("tick", ONE)], &memory, &unfailing).await;
        assert_eq!(from_the_system(&unfailing, "homer").await.len(), 1);
    }

    /// **Every file in the directory is shipped, and every shipped note reads.** A
    /// note a dev adds and forgets to list would be written and never sent.
    #[test]
    fn every_note_in_the_directory_is_shipped_and_reads() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("release-notes");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .expect("the notes directory reads")
            .filter_map(|entry| {
                let path = entry.expect("an entry").path();
                (path.extension()? == "md").then(|| path.file_stem()?.to_str().map(String::from))?
            })
            .collect();
        files.sort();
        let mut shipped: Vec<String> = SHIPPED.iter().map(|(id, _)| id.to_string()).collect();
        shipped.sort();
        assert_eq!(files, shipped, "the directory and the shipped list agree");
        for (id, text) in SHIPPED {
            parse(id, text).unwrap_or_else(|why| panic!("{why}"));
            let on_disk =
                std::fs::read_to_string(dir.join(format!("{id}.md"))).expect("the file reads");
            assert_eq!(&on_disk, text, "the shipped text is the file's: {id}");
        }
    }

    /// **A note that breaks the shape is refused whole.** The shipped note is the
    /// positive each of these departs from.
    #[test]
    fn a_note_that_breaks_the_shape_is_refused() {
        assert!(parse("tick", ONE).is_ok(), "the positive these depart from");
        assert!(parse("Tick", ONE).is_err(), "an id is a lowercase slug");
        assert!(
            parse("tick", &ONE.replace("title:", "name:")).is_err(),
            "a header that is not title or for"
        );
        assert!(
            parse(
                "tick",
                &ONE.replace("title: Homer's list can be ticked\n", "")
            )
            .is_err(),
            "no title"
        );
        assert!(
            parse("tick", &ONE.replace("for: bots that keep lists\n", "")).is_err(),
            "nobody it is for"
        );
        assert!(
            parse("tick", &ONE.replace("## Example", "## Examples")).is_err(),
            "a missing section"
        );
        assert!(
            parse("tick", &format!("{ONE}\n\nNotes: tick")).is_err(),
            "a line that looks like the ledger line"
        );
    }
}

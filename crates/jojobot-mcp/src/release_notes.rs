//! **jojobot tells each bot what a deploy changed.** A change bots can use ships
//! with a note, one file in the `release-notes/deploy-N/` folder of the deploy that ships it, written for the roles that run
//! here. At start, jojobot leaves ONE message in each bot's box listing the notes
//! that box has not been sent, from the system sender, so nobody relays them by
//! hand.
//!
//! **The ledger is on the bot.** Which notes a bot has been told is a fact about
//! the bot, kept by jojobot under [`RELEASE_NOTES_TOLD`] on one record of its own,
//! so a quarantine or a box repair cannot undo it (rule 493). A message still lists
//! the ids of the notes it carries on its last line, `Notes: a, b`: a bot that
//! holds no ledger yet, on a store written before it existed, is seeded once from
//! the messages that list them. A bot made by `add_entity` is told every note its
//! build ships and sent none, because a note is about a change the bot did not
//! live through. A restart sends nothing twice and writes nothing.
//!
//! The pass never stops a boot: a read or a post that fails warns, and the next
//! start sends what is still unsent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jojobot_domain::clock::Clock;
use jojobot_domain::mailbox::{Guarded, Mailboxes, NewMessage};
use jojobot_domain::memory::capacity::{ENGINE_RECORD, RELEASE_NOTES_RECORD};
use jojobot_domain::memory::review;
use jojobot_domain::memory::{
    EntityId, EntityKind, FactPatch, FactStatus, Memory, MemoryError, NewFact, RELEASE_NOTES_TOLD,
    REPORTS_TO,
};

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
/// A file in a `release-notes/deploy-N/` folder that is not listed here is never sent, and a test
/// fails for it.
pub(crate) const SHIPPED: &[(&str, &str)] = &[
    (
        "list-read-pages-and-counts-ticks",
        include_str!("../release-notes/deploy-9/list-read-pages-and-counts-ticks.md"),
    ),
    (
        "many-entries-or-ticks-in-one-call",
        include_str!("../release-notes/deploy-9/many-entries-or-ticks-in-one-call.md"),
    ),
    (
        "rhythm-on-the-calendar",
        include_str!("../release-notes/deploy-9/rhythm-on-the-calendar.md"),
    ),
    (
        "trip-is-a-kind",
        include_str!("../release-notes/deploy-9/trip-is-a-kind.md"),
    ),
    (
        "defrag-ships-a-charter",
        include_str!("../release-notes/deploy-9/defrag-ships-a-charter.md"),
    ),
    (
        "release-notes",
        include_str!("../release-notes/deploy-9/release-notes.md"),
    ),
];

/// The three parts every note holds, in the order it holds them.
const SECTIONS: [&str; 3] = ["What you can now do", "Example", "What to do"];

/// **A note read from its text**, or the reason it is not one: three headers,
/// `title:`, `for:` and `miss:` (what a bot that never read the note would do
/// worse, in one line), a blank line, then the three sections. A note that breaks
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
    let mut miss = None;
    for line in head.lines() {
        match line.split_once(':') {
            Some(("title", value)) => title = Some(value.trim().to_string()),
            Some(("for", value)) => audience = Some(value.trim().to_string()),
            Some(("miss", value)) => miss = Some(value.trim().to_string()),
            _ => {
                return Err(format!(
                    "note '{id}' has a header that is not title, for or miss: {line}"
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
    // **A note says what a bot that never read it would miss**: a change no bot
    // would notice is not worth a message (rule 603).
    miss.filter(|m| !m.is_empty()).ok_or_else(|| {
        format!("note '{id}' says nothing about what a bot that never read it would miss")
    })?;
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
    let note = Note {
        id: id.to_string(),
        title,
        audience,
        body: body.trim().to_string(),
    };
    // **A title its own mail would refuse is refused here**, where the case over
    // the shipped notes reads it: posted, it would fail at every start and no bot
    // would ever be told.
    jojobot_domain::mailbox::validate_subject(Some(&subject(&[&note])))
        .map_err(|why| format!("note '{id}' has a title its own mail refuses: {why}"))?;
    Ok(note)
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
         software you run on. jojobot has no box, so a post addressed to `jojobot` is refused \
         and writes nothing.",
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
    body.trim_end()
        .lines()
        .last()
        .and_then(|line| line.strip_prefix(NOTES_LINE))
        .map(|ids| ids.split(',').map(|id| id.trim().to_string()).collect())
        .unwrap_or_default()
}

/// **The ids of every note this build ships that reads.**
pub(crate) fn shipped_ids() -> BTreeSet<String> {
    read_all(SHIPPED).into_iter().map(|note| note.id).collect()
}

/// **The notes a bot's ledger holds**, or `None` for a bot that holds none yet.
fn ledger_of(fields: &BTreeMap<String, String>) -> Option<BTreeSet<String>> {
    fields.get(RELEASE_NOTES_TOLD).map(|ids| {
        ids.split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect()
    })
}

/// **What a bot has been told, and so what its box is owed**: the notes its ledger
/// holds and the notes the system's messages in its box list. A note in the box is
/// told whether or not the write that records it landed, so a ledger write that
/// failed is recovered from the box at the next start (rule 498: the one place
/// this is decided).
fn told(ledger: Option<&BTreeSet<String>>, mailed: Option<&BTreeSet<String>>) -> BTreeSet<String> {
    ledger
        .into_iter()
        .chain(mailed)
        .flatten()
        .cloned()
        .collect()
}

/// **Write a bot's ledger** onto the one record jojobot keeps for it, rewritten in
/// place when it exists. The record takes no slot and is not one of the bot's
/// rules (see [`RELEASE_NOTES_RECORD`]). A ledger that already says this writes
/// nothing.
pub(crate) async fn record_told(
    memory: &Arc<dyn Memory>,
    bot: &EntityId,
    told: &BTreeSet<String>,
    day: jiff::civil::Date,
) -> Result<(), MemoryError> {
    let value = ledger_text(told);
    let standing = memory.recall(bot).await?.into_iter().find(|fact| {
        fact.status == FactStatus::Active && review::is_release_notes_record(&fact.fields)
    });
    match standing {
        Some(record) if record.fields.get(RELEASE_NOTES_TOLD) == Some(&value) => Ok(()),
        Some(record) => memory
            .update_fact(
                &record.address(),
                FactPatch {
                    fields: [(RELEASE_NOTES_TOLD.to_string(), value)].into(),
                    sets: [RELEASE_NOTES_TOLD.to_string()].into(),
                    ..Default::default()
                },
                bot,
            )
            .await
            .map(|_| ()),
        None => memory
            .capture(ledger_record(bot, told, day))
            .await
            .map(|_| ()),
    }
}

/// **The ledger as a value**: the ids, comma-joined, as the key holds them.
pub(crate) fn ledger_text(told: &BTreeSet<String>) -> String {
    told.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// **The record that carries a bot's ledger**, labelled as jojobot's own so it is
/// no rule of the bot. A bot is made with it as its first claim when the call
/// sets nothing (see `add_entity`).
pub(crate) fn ledger_record(
    bot: &EntityId,
    told: &BTreeSet<String>,
    day: jiff::civil::Date,
) -> NewFact {
    NewFact {
        fields: [
            (ENGINE_RECORD.to_string(), RELEASE_NOTES_RECORD.to_string()),
            (RELEASE_NOTES_TOLD.to_string(), ledger_text(told)),
        ]
        .into(),
        sets: [RELEASE_NOTES_TOLD.to_string()].into(),
        ..NewFact::about(
            bot.clone(),
            "jojobot keeps here the release notes this bot has been told.".to_string(),
            day,
        )
    }
}

/// **Send each bot the notes this build ships and its box has not been sent.**
/// Called at start by the one function the binary and the story harness both run.
pub async fn send_unsent_notes(
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
    clock: &Clock,
) {
    send_from(SHIPPED, memory, mailboxes, clock).await;
    // **The pass ends with a line of its own**, so a caller that reads the log can
    // tell the notes are in the boxes and the ledgers are written.
    tracing::info!("release notes pass finished");
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
    let mut mailed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for message in &sent {
        mailed
            .entry(message.mailbox.as_str().to_string())
            .or_default()
            .extend(listed(&message.body));
    }
    let day = clock.today_in(&jiff::tz::TimeZone::UTC);
    for bot in bots.iter().filter(|bot| bot.browsable()) {
        // A bot's own box, found by its owner and never made here: a bot without
        // one is a boot that has not healed it, and the next start sends.
        let Some(mailbox) = boxes.iter().find(|held| held.owner == bot.id) else {
            tracing::warn!(bot = %bot.id, "no release notes sent — the bot has no box yet");
            continue;
        };
        let fields = match memory.fields(&bot.id).await {
            Ok(fields) => fields,
            Err(e) => {
                tracing::warn!(bot = %bot.id, error = %e, "no release notes sent — the bot could not be read");
                continue;
            }
        };
        let ledger = ledger_of(&fields);
        let mut told = told(ledger.as_ref(), mailed.get(mailbox.name.as_str()));
        let unsent: Vec<&Note> = notes
            .iter()
            .filter(|note| !told.contains(&note.id))
            .collect();
        if unsent.is_empty() {
            keep_ledger(memory, &bot.id, ledger.as_ref(), &told, day).await;
            continue;
        }
        let manager = fields.get(REPORTS_TO).cloned();
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
        // **A post that does not land names the notes it carried**, so the line
        // says which note to fix as well as why.
        let carried = unsent[..count]
            .iter()
            .map(|note| note.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        match mailboxes.post_message(compose(count)).await {
            Ok(Guarded::Written(_)) => {
                tracing::info!(bot = %bot.id, notes = count, "release notes sent");
                told.extend(unsent[..count].iter().map(|note| note.id.clone()));
            }
            Ok(_) => {
                tracing::warn!(bot = %bot.id, notes = %carried, "release notes not sent — the post was blocked")
            }
            Err(e) => {
                tracing::warn!(bot = %bot.id, notes = %carried, error = %e, "release notes not sent — the post failed")
            }
        }
        keep_ledger(memory, &bot.id, ledger.as_ref(), &told, day).await;
    }
}

/// **Write the ledger when it is not what the bot holds.** A write that fails
/// warns: the notes it would record are in the box, and the next start seeds
/// from them again.
async fn keep_ledger(
    memory: &Arc<dyn Memory>,
    bot: &EntityId,
    held: Option<&BTreeSet<String>>,
    told: &BTreeSet<String>,
    day: jiff::civil::Date,
) {
    // A bot told nothing yet holds no ledger rather than an empty one.
    if held == Some(told) || (held.is_none() && told.is_empty()) {
        return;
    }
    if let Err(e) = record_told(memory, bot, told, day).await {
        tracing::warn!(bot = %bot, error = %e, "the release notes a bot was told were not recorded");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{MailboxError, MailboxName, Message};
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{EntityId, NewEntity, NewFact};

    const ONE: &str = "title: Homer's list can be ticked\nfor: bots that keep lists\nmiss: Ticks stay unmarked.\n\n## What you can now do\n\nTick an entry.\n\n## Example\n\nHomer ticks the tent.\n\n## What to do\n\nUpdate your process.";
    const TWO: &str = "title: Marge reads her own history\nfor: bots that keep records\nmiss: History goes unread.\n\n## What you can now do\n\nRead a run.\n\n## Example\n\nMarge reads run one.\n\n## What to do\n\nAsk your manager.";
    const THREE: &str = "title: Bart sends a note\nfor: every bot\nmiss: Notes go unsent.\n\n## What you can now do\n\nSend it.\n\n## Example\n\nBart sends it.\n\n## What to do\n\nRead it.";

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

    /// **A bot told some notes is sent only the ones it was not told**: a ledger
    /// written for one note, then a pass over that note and a second, sends one
    /// message carrying the second alone and records both.
    #[tokio::test]
    async fn a_bot_told_a_note_is_sent_only_the_notes_shipped_after_it() {
        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        let homer = a_bot(&memory, &mailboxes, "homer").await;
        let told: BTreeSet<String> = ["tick".to_string()].into();
        record_told(&memory, &homer, &told, jiff::civil::date(2026, 10, 1))
            .await
            .expect("the ledger is written");

        pass(&[("tick", ONE), ("history", TWO)], &memory, &mailboxes).await;
        let homers = from_the_system(&mailboxes, "homer").await;
        assert_eq!(homers.len(), 1, "one message: {homers:?}");
        assert_eq!(
            listed(&homers[0].body),
            ["history".to_string()].into(),
            "only the note shipped after the bot was told"
        );
        let fields = memory.fields(&homer).await.expect("the bot reads");
        assert_eq!(
            ledger_of(&fields),
            Some(["history".to_string(), "tick".to_string()].into())
        );
    }

    /// **A note already in the box is told, whatever the ledger says.** The bot
    /// holds a ledger of one note, a pass sends it a second while the ledger
    /// write fails, and the passes after it send nothing: with the store healthy
    /// or with the write still failing. The ledger is then written from the box.
    #[tokio::test]
    async fn a_note_sent_while_the_ledger_write_failed_is_not_sent_again() {
        use crate::memory::testing::{Down, DownMemory};
        let store = Arc::new(InMemoryMemory::booted());
        let memory: Arc<dyn Memory> = store.clone();
        let mailboxes: Arc<dyn Mailboxes> = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let homer = a_bot(&memory, &mailboxes, "homer").await;
        let told: BTreeSet<String> = ["tick".to_string()].into();
        record_told(&memory, &homer, &told, jiff::civil::date(2026, 10, 1))
            .await
            .expect("the ledger is written");
        let failing: Arc<dyn Memory> = Arc::new(DownMemory(Down::Writes, store));

        pass(&[("tick", ONE), ("history", TWO)], &failing, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "homer").await.len(), 1);
        let held = memory.fields(&homer).await.expect("the bot reads");
        assert_eq!(
            ledger_of(&held),
            Some(told.clone()),
            "the setup: the write failed and the ledger is as it was"
        );

        // The write is still failing: the note is not sent a second time.
        pass(&[("tick", ONE), ("history", TWO)], &failing, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "homer").await.len(), 1);
        // The store answers again: nothing is sent, and the ledger catches up.
        pass(&[("tick", ONE), ("history", TWO)], &memory, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "homer").await.len(), 1);
        let held = memory.fields(&homer).await.expect("the bot reads");
        assert_eq!(
            ledger_of(&held),
            Some(["history".to_string(), "tick".to_string()].into())
        );
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
            !marges[0].body.contains("@bot:") && !listed(&marges[0].body).is_empty(),
            "Marge has no manager and so no mention of one, and the note is there: {}",
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
                "title: {title}\nfor: every bot\nmiss: Nothing.\n\n## What you can now do\n\n{}\n\n## Example\n\nA line.\n\n## What to do\n\nRead it.",
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

    /// **What `future` logs**, as the formatter writes it. A case is a
    /// current-thread runtime, so a subscriber set for this thread sees all of it.
    async fn logged_during(future: impl std::future::Future<Output = ()>) -> String {
        #[derive(Clone, Default)]
        struct Lines(Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for Lines {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0
                    .lock()
                    .expect("the log lock")
                    .extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let lines = Lines::default();
        let writer = lines.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        {
            let _for_this_thread = tracing::subscriber::set_default(subscriber);
            future.await;
        }
        let bytes = lines.0.lock().expect("the log lock").clone();
        String::from_utf8(bytes).expect("the log is text")
    }

    /// What a mailbox store that went away cannot do.
    #[derive(Clone, Copy, PartialEq)]
    enum Fault {
        Nothing,
        /// The sent mail cannot be read.
        WhatWasSent,
        /// The boxes cannot be listed.
        TheBoxes,
        /// Every post comes back blocked, as the guard answers a name it does not know.
        PostsBlocked,
    }

    /// A mailbox store that refuses the first post to one box, as a store that
    /// went away between two bots would, and that can refuse to read what was sent.
    struct FailsOnce {
        inner: Arc<InMemoryMailboxes>,
        box_name: &'static str,
        failed: std::sync::atomic::AtomicBool,
        fault: Fault,
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
            if self.fault == Fault::TheBoxes {
                return Err(MailboxError::InvalidMessage("the store went away".into()));
            }
            self.inner.list_mailboxes().await
        }
        async fn post_message(&self, new: NewMessage) -> Result<Guarded<Message>, MailboxError> {
            if self.fault == Fault::PostsBlocked {
                return Ok(Guarded::Blocked {
                    attempted: new.mailbox,
                    candidates: Vec::new(),
                });
            }
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
            if self.fault == Fault::WhatWasSent {
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
            fault: Fault::Nothing,
        });

        let logged = logged_during(pass(&[("tick", ONE)], &memory, &mailboxes)).await;
        assert!(
            from_the_system(&mailboxes, "homer").await.is_empty(),
            "the failed post left nothing"
        );
        let failed: Vec<&str> = logged
            .lines()
            .filter(|line| line.contains("post failed"))
            .collect();
        assert_eq!(failed.len(), 1, "{logged}");
        assert!(
            failed[0].contains("notes=tick"),
            "the line names the note: {logged}"
        );
        assert!(
            failed[0].contains("the store went away"),
            "the line says why: {logged}"
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
            "title: Huge\nfor: every bot\nmiss: Nothing.\n\n## What you can now do\n\n{}\n\n## Example\n\nA line.\n\n## What to do\n\nRead it.",
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
            fault: Fault::WhatWasSent,
        });

        pass(&[("tick", ONE)], &memory, &failing).await;
        assert!(
            from_the_system(&unfailing, "homer").await.is_empty(),
            "nothing was sent on a ledger that could not be read"
        );
        pass(&[("tick", ONE)], &memory, &unfailing).await;
        assert_eq!(from_the_system(&unfailing, "homer").await.len(), 1);
    }

    /// A pass over these two stores, with the first one failing in the way named.
    async fn a_down_memory(
        down: crate::memory::testing::Down,
    ) -> (Arc<InMemoryMemory>, Arc<dyn Memory>) {
        let inner = Arc::new(InMemoryMemory::booted());
        let memory: Arc<dyn Memory> =
            Arc::new(crate::memory::testing::DownMemory(down, inner.clone()));
        (inner, memory)
    }

    /// **The line that names the notes survives what a store does to the end of a
    /// body.** A body that gains a newline, or a blank line, still lists the same
    /// notes, so a restart over it sends nothing again.
    #[tokio::test]
    async fn the_ledger_line_survives_a_trailing_newline() {
        let sent = listed("Text.\n\nNotes: tick, history");
        assert_eq!(sent.len(), 2, "the positive these depart from: {sent:?}");
        for tail in ["\n", "\n\n", " \n", "\r\n", "\n \n"] {
            assert_eq!(
                listed(&format!("Text.\n\nNotes: tick, history{tail}")),
                sent,
                "a body ending {tail:?} lists the same notes"
            );
        }

        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        a_bot(&memory, &mailboxes, "homer").await;
        mailboxes
            .post_message(NewMessage {
                mailbox: MailboxName("homer".into()),
                body: "Text.\n\nNotes: tick\n\n".into(),
                subject: None,
                sender: SYSTEM_SENDER.to_string(),
                sent_at: Clock::default().now(),
                in_reply_to: None,
                supersedes: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
            })
            .await
            .expect("the post lands");
        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(
            from_the_system(&mailboxes, "homer").await.len(),
            1,
            "the note the stored message lists is not sent again"
        );
    }

    /// **A pass that cannot list the bots sends nothing**, and the next start
    /// sends to every bot.
    #[tokio::test]
    async fn a_pass_that_cannot_list_the_bots_sends_nothing() {
        let (inner, down) = a_down_memory(crate::memory::testing::Down::ListingKind("bot")).await;
        let memory: Arc<dyn Memory> = inner;
        let mailboxes: Arc<dyn Mailboxes> = Arc::new(InMemoryMailboxes::knowing_any_owner());
        a_bot(&memory, &mailboxes, "homer").await;

        pass(&[("tick", ONE)], &down, &mailboxes).await;
        assert!(
            from_the_system(&mailboxes, "homer").await.is_empty(),
            "nothing was sent on a bot list that could not be read"
        );
        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "homer").await.len(), 1);
    }

    /// **A pass that cannot list the boxes sends nothing**, and the next start
    /// sends.
    #[tokio::test]
    async fn a_pass_that_cannot_list_the_boxes_sends_nothing() {
        let (memory, concrete) = ports();
        let unfailing: Arc<dyn Mailboxes> = concrete.clone();
        a_bot(&memory, &unfailing, "homer").await;
        let failing: Arc<dyn Mailboxes> = Arc::new(FailsOnce {
            inner: concrete,
            box_name: "nobody",
            failed: std::sync::atomic::AtomicBool::new(false),
            fault: Fault::TheBoxes,
        });

        pass(&[("tick", ONE)], &memory, &failing).await;
        assert!(
            from_the_system(&unfailing, "homer").await.is_empty(),
            "nothing was sent to boxes that could not be listed"
        );
        pass(&[("tick", ONE)], &memory, &unfailing).await;
        assert_eq!(from_the_system(&unfailing, "homer").await.len(), 1);
    }

    /// **A bot whose fields cannot be read is skipped, and the bots after it are
    /// still sent.** The next start sends to the one that was skipped, and sends
    /// again to no one else.
    #[tokio::test]
    async fn a_bot_whose_fields_cannot_be_read_is_skipped_and_the_rest_are_sent() {
        let (inner, down) =
            a_down_memory(crate::memory::testing::Down::FieldsOf("bot:gamma")).await;
        let memory: Arc<dyn Memory> = inner;
        let mailboxes: Arc<dyn Mailboxes> = Arc::new(InMemoryMailboxes::knowing_any_owner());
        a_bot(&memory, &mailboxes, "gamma").await;
        a_bot(&memory, &mailboxes, "marge").await;

        pass(&[("tick", ONE)], &down, &mailboxes).await;
        assert!(
            from_the_system(&mailboxes, "gamma").await.is_empty(),
            "the bot that could not be read was sent nothing"
        );
        assert_eq!(
            from_the_system(&mailboxes, "marge").await.len(),
            1,
            "the bot after it was still sent"
        );

        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "gamma").await.len(), 1);
        assert_eq!(
            from_the_system(&mailboxes, "marge").await.len(),
            1,
            "and the one that was sent is not sent again"
        );
    }

    /// **A post the guard blocks sends nothing and stops nothing**, and the next
    /// start sends.
    #[tokio::test]
    async fn a_blocked_post_sends_nothing_and_the_next_start_sends() {
        let (memory, concrete) = ports();
        let unfailing: Arc<dyn Mailboxes> = concrete.clone();
        a_bot(&memory, &unfailing, "homer").await;
        a_bot(&memory, &unfailing, "marge").await;
        let blocking: Arc<dyn Mailboxes> = Arc::new(FailsOnce {
            inner: concrete,
            box_name: "nobody",
            failed: std::sync::atomic::AtomicBool::new(false),
            fault: Fault::PostsBlocked,
        });

        pass(&[("tick", ONE)], &memory, &blocking).await;
        for bot in ["homer", "marge"] {
            assert!(
                from_the_system(&unfailing, bot).await.is_empty(),
                "{bot} was sent nothing by a post that was blocked"
            );
        }
        pass(&[("tick", ONE)], &memory, &unfailing).await;
        for bot in ["homer", "marge"] {
            assert_eq!(from_the_system(&unfailing, bot).await.len(), 1, "{bot}");
        }
    }

    /// **One note that does not read is left out, and the rest are sent.** A file
    /// listed twice is sent once.
    #[tokio::test]
    async fn one_bad_note_is_dropped_and_the_rest_are_sent() {
        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        a_bot(&memory, &mailboxes, "homer").await;

        pass(
            &[
                (
                    "broken",
                    "title: No sections\nfor: every bot\nmiss: Nothing.\n\nJust a line.",
                ),
                ("tick", ONE),
                ("tick", TWO),
                ("history", TWO),
            ],
            &memory,
            &mailboxes,
        )
        .await;
        let homers = from_the_system(&mailboxes, "homer").await;
        assert_eq!(homers.len(), 1, "one message carries the notes that read");
        assert_eq!(
            listed(&homers[0].body),
            ["history", "tick"].into_iter().map(String::from).collect(),
            "the broken note and the second 'tick' are not sent"
        );
        assert_eq!(
            homers[0]
                .body
                .lines()
                .filter(|line| line.starts_with("# "))
                .count(),
            2,
            "each sent note is in the message once: {}",
            homers[0].body
        );
    }

    /// **A bot healed of a missing box is sent its notes at the next start**, and
    /// only once.
    #[tokio::test]
    async fn a_bot_healed_of_a_missing_box_is_sent_its_notes() {
        let (memory, concrete) = ports();
        let mailboxes: Arc<dyn Mailboxes> = concrete;
        let moe = EntityId("bot:moe".into());
        memory
            .add_entity(NewEntity::new(moe.clone(), "moe", "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("an empty board blocks nothing");

        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert!(from_the_system(&mailboxes, "moe").await.is_empty());

        mailboxes
            .create_mailbox(&MailboxName("moe".into()), &moe, None)
            .await
            .expect("the box is healed");
        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(from_the_system(&mailboxes, "moe").await.len(), 1);
        pass(&[("tick", ONE)], &memory, &mailboxes).await;
        assert_eq!(
            from_the_system(&mailboxes, "moe").await.len(),
            1,
            "and not sent again"
        );
    }

    /// **The notes directory holds only deploy folders**, one for each deploy
    /// that ships notes, named `deploy-N`. A note at the top level, or in a
    /// folder with another name, would be filed by nothing, so the case names the
    /// file and the order cannot drift.
    #[test]
    fn the_notes_directory_holds_only_deploy_folders() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("release-notes");
        let strays: Vec<String> = std::fs::read_dir(&dir)
            .expect("the notes directory reads")
            .map(|entry| entry.expect("an entry"))
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                let a_deploy = name
                    .strip_prefix("deploy-")
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
                !(entry.path().is_dir() && a_deploy)
            })
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            strays.is_empty(),
            "a note outside a deploy-N folder: {strays:?}"
        );
    }

    /// **Every file in a deploy folder is shipped, and every shipped note reads.** A
    /// note a dev adds and forgets to list would be written and never sent.
    #[test]
    fn every_note_in_the_directory_is_shipped_and_reads() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("release-notes");
        let mut files: Vec<(String, String)> = Vec::new();
        for folder in std::fs::read_dir(&dir).expect("the notes directory reads") {
            let folder = folder.expect("an entry").path();
            if !folder.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&folder).expect("a deploy folder reads") {
                let path = entry.expect("an entry").path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    let id = path.file_stem().and_then(|s| s.to_str()).expect("a name");
                    let text = std::fs::read_to_string(&path).expect("a note reads");
                    files.push((id.to_string(), text));
                }
            }
        }
        files.sort();
        let mut shipped: Vec<(String, String)> = SHIPPED
            .iter()
            .map(|(id, text)| (id.to_string(), text.to_string()))
            .collect();
        shipped.sort();
        assert_eq!(
            files, shipped,
            "the deploy folders and the shipped list agree, note for note"
        );
        for (id, text) in SHIPPED {
            parse(id, text).unwrap_or_else(|why| panic!("{why}"));
        }
    }

    /// **Every shipped note is sent under a subject the mail takes**, alone and
    /// with all the others: a subject the mail refuses would fail the post at
    /// every start, and no bot would ever be told.
    #[test]
    fn every_shipped_note_is_sent_under_a_subject_the_mail_takes() {
        let notes: Vec<Note> = SHIPPED
            .iter()
            .map(|(id, text)| parse(id, text).unwrap_or_else(|why| panic!("{why}")))
            .collect();
        for note in &notes {
            let alone = subject(&[note]);
            jojobot_domain::mailbox::validate_subject(Some(&alone))
                .unwrap_or_else(|why| panic!("note '{}' under {alone:?}: {why}", note.id));
        }
        let all: Vec<&Note> = notes.iter().collect();
        let together = subject(&all);
        jojobot_domain::mailbox::validate_subject(Some(&together))
            .unwrap_or_else(|why| panic!("the notes together under {together:?}: {why}"));
    }

    /// **A title its own mail would refuse is refused where the note is read**, so
    /// it fails the case over the shipped notes rather than the post at every start.
    #[test]
    fn a_note_whose_title_its_own_mail_refuses_is_refused() {
        let titled = |title: &str| ONE.replace("Homer's list can be ticked", title);
        assert!(
            parse("tick", &titled("Homer ticks the `tent`")).is_err(),
            "a backtick"
        );
        // The most a title can hold: what a subject may hold, less what the
        // subject adds around the title.
        let positive = parse("tick", ONE).expect("the positive");
        let around = subject(&[&positive]).chars().count() - positive.title.chars().count();
        let room = 120 - around;
        assert!(
            parse("tick", &titled(&"d".repeat(room))).is_ok(),
            "a title that just fits"
        );
        assert!(
            parse("tick", &titled(&"d".repeat(room + 1))).is_err(),
            "one character over"
        );
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
        let refused = parse("tick", &ONE.replace("miss: Ticks stay unmarked.\n", ""))
            .expect_err("a note that says nothing about what a bot misses is refused");
        assert!(
            refused.contains("'tick'"),
            "the refusal names the note: {refused}"
        );
        assert!(
            parse("tick", &format!("{ONE}\n\nNotes: tick")).is_err(),
            "a line that looks like the ledger line"
        );
    }
}

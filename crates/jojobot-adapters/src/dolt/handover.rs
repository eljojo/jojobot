//! **The one-time handover** — mailboxes and sessions move out of the document
//! store and into this one.
//!
//! Not a sync, not a reconciliation loop, and not something that runs quietly
//! on every boot doing nothing. It is a deliberate operation that carries the
//! records across once. **It refuses rather than doubling**: a target that
//! already holds any of these records is left exactly as it is, loudly, because
//! a migration that silently doubles a mailbox is worse than one that will not
//! start.
//!
//! **The source is read and never written.** Nothing here deletes, edits or
//! marks anything on the old store. What happens to it afterwards is a separate
//! decision that belongs to a person.
//!
//! # Verification is a comparison, not a claim
//!
//! Counting what was written proves that writes happened. It does not prove
//! that what landed is what was there. So every carried record is **read back
//! through the target's own read path** — the same code a caller would use, not
//! the rows this module just wrote — and compared field by field. A record that
//! does not match is [`HandoverError::Mismatch`] and the handover fails; it is
//! never a warning beside a success.
//!
//! **A message's state must survive.** `new` must not arrive as `read`, and
//! `processed` must arrive as `processed` with its notes intact. That is why
//! this writes rows rather than calling `post_message`: the verb files
//! everything as `new` by design, which is right for a message somebody is
//! sending and wrong for one that already has a history.
//!
//! # What cannot be carried is reported, never dropped
//!
//! The old store holds cards jojobot cannot read. It cannot write them here
//! either — it does not know what they say. They are counted and named in the
//! report as **not carried**, which is the honest answer: silence would let a
//! reader conclude the board was empty of them.

use jojobot_domain::mailbox::{MailboxError, Mailboxes, Message, MessageId};
use jojobot_domain::session::{Session, SessionError, Sessions};
use sqlx::{MySql, MySqlPool, Transaction};

/// Why the handover did not complete.
#[derive(Debug, thiserror::Error)]
pub enum HandoverError {
    /// The target already holds records of this kind. **Nothing was written.**
    #[error(
        "the target already holds {held} {what}, so this would double them — \
         nothing was written, and a populated target has to be cleared by a person"
    )]
    Populated {
        /// Which kind of record was already there.
        what: &'static str,
        /// How many.
        held: usize,
    },
    /// The old store could not be read. Nothing was written.
    #[error("the records could not be read from the old store: {0}")]
    Source(String),
    /// The new store refused a write.
    #[error("the new store refused the handover: {0}")]
    Target(String),
    /// **A record did not read back as what it was.** The handover failed; the
    /// target is left holding whatever landed, and a person has to look.
    #[error("{what} '{which}' did not read back as it was written: {field} differs")]
    Mismatch {
        /// Which kind of record.
        what: &'static str,
        /// Which one, by id.
        which: String,
        /// The first field that differs, named so a reader is not left
        /// diffing two records by eye.
        field: &'static str,
    },
}

/// What the handover did, in numbers a reader can check rather than a claim
/// they have to take.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// Boxes read, written and verified.
    pub boxes: Carried,
    /// Messages read, written and verified.
    pub messages: Carried,
    /// Sessions read, written and verified.
    pub sessions: Carried,
    /// Chronology entries read, written and verified.
    pub entries: Carried,
    /// **Cards the old store holds and jojobot cannot read**, so they were not
    /// carried. Named rather than counted alone: a reader who has to repair one
    /// needs to know which.
    pub not_carried: Vec<MessageId>,
}

/// One kind of record, at each of the three stages that matter.
///
/// Three numbers rather than one, because they answer different questions and
/// the interesting failures are where they disagree: read but not written is a
/// refused write, written but not verified is a record that did not survive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Carried {
    /// How many the old store had.
    pub read: usize,
    /// How many rows landed.
    pub written: usize,
    /// How many read back identical through the new store's own read path.
    pub verified: usize,
}

impl Carried {
    /// Whether every record that was read landed and read back as itself.
    pub fn whole(&self) -> bool {
        self.read == self.written && self.written == self.verified
    }
}

impl Report {
    /// Whether every kind came through whole. **Not the same as "it ran"**: a
    /// handover that read nothing is whole and carried nothing.
    pub fn whole(&self) -> bool {
        self.boxes.whole() && self.messages.whole() && self.sessions.whole() && self.entries.whole()
    }
}

fn source_mail(e: MailboxError) -> HandoverError {
    HandoverError::Source(e.to_string())
}

fn source_session(e: SessionError) -> HandoverError {
    HandoverError::Source(e.to_string())
}

fn target(e: sqlx::Error) -> HandoverError {
    tracing::error!(error = %e, "the handover's target refused a write");
    HandoverError::Target("the store refused the records".into())
}

/// Refuse if the target already holds anything of this kind.
async fn must_be_empty(
    tx: &mut Transaction<'_, MySql>,
    table: &str,
    what: &'static str,
) -> Result<(), HandoverError> {
    // The table name is this module's own literal, never a caller's — there is
    // no value here to bind.
    let held: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM `{table}`"))
        .fetch_one(&mut **tx)
        .await
        .map_err(target)?;
    if held > 0 {
        return Err(HandoverError::Populated {
            what,
            held: held as usize,
        });
    }
    Ok(())
}

/// **Carry the mailboxes and sessions across, then prove it.**
///
/// The source ports are read-only here by use rather than by type: nothing in
/// this function calls a verb that writes.
/// **The target is named twice on purpose**: `pool` is where the rows are
/// written, and `to_mail`/`to_sessions` are how they are read back. They are the
/// same store, and the read side is the PORT rather than the concrete adapter
/// because the verification's whole claim is that a caller reading normally
/// gets what was sent.
pub async fn run(
    from_mail: &dyn Mailboxes,
    from_sessions: &dyn Sessions,
    to_mail: &dyn Mailboxes,
    to_sessions: &dyn Sessions,
    pool: &MySqlPool,
) -> Result<Report, HandoverError> {
    // **Read everything first, and refuse before writing anything.** A handover
    // that discovered a populated target halfway would leave the target holding
    // a mixture nobody can reason about.
    let boxes = from_mail.list_mailboxes().await.map_err(source_mail)?;
    let messages = from_mail.scan_messages().await.map_err(source_mail)?;
    let sessions = from_sessions.all_sessions().await.map_err(source_session)?;

    let mut tx = pool.begin().await.map_err(target)?;
    must_be_empty(&mut tx, "mailbox", "mailboxes").await?;
    must_be_empty(&mut tx, "message", "messages").await?;
    must_be_empty(&mut tx, "session", "sessions").await?;
    must_be_empty(&mut tx, "journal_entry", "chronology entries").await?;

    let mut report = Report {
        not_carried: boxes.iter().flat_map(|b| b.quarantined.clone()).collect(),
        ..Report::default()
    };
    report.boxes.read = boxes.len();
    report.messages.read = messages.len();
    report.sessions.read = sessions.len();
    report.entries.read = sessions.iter().map(|s| s.entries.len()).sum();

    for mailbox in &boxes {
        sqlx::query("INSERT INTO mailbox (name, owner) VALUES (?, ?)")
            .bind(mailbox.name.as_str())
            .bind(mailbox.owner.as_str())
            .execute(&mut *tx)
            .await
            .map_err(target)?;
        report.boxes.written += 1;
    }

    // Delivery order is the order the old store reports, which is the order a
    // reader of the old board saw. Carrying the position rather than recomputing
    // it is what keeps two messages sent in the same second in the order they
    // were already in.
    for (position, message) in messages.iter().enumerate() {
        sqlx::query(
            "INSERT INTO message
               (id, mailbox, ordinal, body, subject, sender, sent_at, state, notes, in_reply_to)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(message.id.as_str())
        .bind(message.mailbox.as_str())
        .bind(position as i64 + 1)
        .bind(&message.body)
        .bind(message.subject.as_deref())
        .bind(&message.sender)
        .bind(message.sent_at.to_string())
        .bind(message.state.as_token())
        .bind(message.notes.as_deref())
        .bind(message.in_reply_to.as_ref().map(MessageId::as_str))
        .execute(&mut *tx)
        .await
        .map_err(target)?;
        report.messages.written += 1;
    }

    for session in &sessions {
        sqlx::query(
            "INSERT INTO session (id, sid, bot, focus, started_at, state) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(session.id.as_str())
        .bind(session.sid.as_ref().map(|s| s.as_str()))
        .bind(session.bot.as_str())
        .bind(&session.focus)
        .bind(session.started_at.to_string())
        .bind(session.state.as_token())
        .execute(&mut *tx)
        .await
        .map_err(target)?;
        report.sessions.written += 1;

        for (ordinal, entry) in session.entries.iter().enumerate() {
            sqlx::query(
                "INSERT INTO journal_entry (session, id, ordinal, at, text, touched, beat)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(session.id.as_str())
            .bind(entry.id.as_str())
            .bind(ordinal as i64 + 1)
            .bind(entry.at.to_string())
            .bind(&entry.text)
            .bind(entry.touched.map(|t| t.to_string()))
            .bind(entry.beat.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(target)?;
            report.entries.written += 1;
        }
    }

    // **The counters have to clear what was carried.** Ids come across as they
    // are, so a counter still at zero would mint an id a carried record already
    // wears and the next write would collide on a record nobody could see.
    advance(
        &mut tx,
        "message",
        highest(messages.iter().map(|m| m.id.as_str())),
    )
    .await?;
    advance(
        &mut tx,
        "session",
        highest(sessions.iter().map(|s| s.id.as_str())),
    )
    .await?;
    advance(
        &mut tx,
        "entry",
        highest(
            sessions
                .iter()
                .flat_map(|s| s.entries.iter().map(|e| e.id.as_str())),
        ),
    )
    .await?;

    tx.commit().await.map_err(target)?;

    // **Read back through the new store's own read path**, not the rows just
    // written. A comparison against this module's own memory of what it sent
    // would agree with itself whatever the store did with it.
    verify(
        &mut report,
        &boxes,
        &messages,
        &sessions,
        to_mail,
        to_sessions,
    )
    .await?;
    Ok(report)
}

/// The largest numeric id among those carried, or zero.
///
/// Ids are a counter rendered decimal. One that is not a number belongs to no
/// counter this store mints from, so it cannot collide and does not raise it.
fn highest<'a>(ids: impl Iterator<Item = &'a str>) -> i64 {
    ids.filter_map(|id| id.parse::<i64>().ok())
        .max()
        .unwrap_or(0)
}

/// Raise a counter so it will never mint an id a carried record already wears.
async fn advance(
    tx: &mut Transaction<'_, MySql>,
    kind: &str,
    highest: i64,
) -> Result<(), HandoverError> {
    if highest <= 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO minted (kind, counter) VALUES (?, ?)
         ON DUPLICATE KEY UPDATE counter = GREATEST(counter, VALUES(counter))",
    )
    .bind(kind)
    .bind(highest)
    .execute(&mut **tx)
    .await
    .map_err(target)?;
    Ok(())
}

/// Compare every carried record against what the new store hands back.
async fn verify(
    report: &mut Report,
    boxes: &[jojobot_domain::mailbox::Mailbox],
    messages: &[Message],
    sessions: &[Session],
    to_mail: &dyn Mailboxes,
    to_sessions: &dyn Sessions,
) -> Result<(), HandoverError> {
    let landed_boxes = to_mail.list_mailboxes().await.map_err(source_mail)?;
    for was in boxes {
        let Some(now) = landed_boxes.iter().find(|b| b.name == was.name) else {
            return Err(HandoverError::Mismatch {
                what: "mailbox",
                which: was.name.to_string(),
                field: "the box itself is not there",
            });
        };
        if now.owner != was.owner {
            return Err(HandoverError::Mismatch {
                what: "mailbox",
                which: was.name.to_string(),
                field: "owner",
            });
        }
        if now.counts != was.counts {
            return Err(HandoverError::Mismatch {
                what: "mailbox",
                which: was.name.to_string(),
                field: "counts",
            });
        }
        report.boxes.verified += 1;
    }

    let landed = to_mail.scan_messages().await.map_err(source_mail)?;
    for was in messages {
        let Some(now) = landed.iter().find(|m| m.id == was.id) else {
            return Err(HandoverError::Mismatch {
                what: "message",
                which: was.id.to_string(),
                field: "the message itself is not there",
            });
        };
        // Field by field, and named individually: "they differ" sends a reader
        // diffing two records by eye, which is how a state change gets missed.
        let field = if now.mailbox != was.mailbox {
            Some("mailbox")
        } else if now.body != was.body {
            Some("body")
        } else if now.subject != was.subject {
            Some("subject")
        } else if now.sender != was.sender {
            Some("sender")
        } else if now.sent_at != was.sent_at {
            Some("sent_at")
        } else if now.state != was.state {
            Some("state")
        } else if now.notes != was.notes {
            Some("notes")
        } else if now.in_reply_to != was.in_reply_to {
            Some("in_reply_to")
        } else {
            None
        };
        if let Some(field) = field {
            return Err(HandoverError::Mismatch {
                what: "message",
                which: was.id.to_string(),
                field,
            });
        }
        report.messages.verified += 1;
    }

    for was in sessions {
        let now = to_sessions
            .read_session(&was.id)
            .await
            .map_err(|_| HandoverError::Mismatch {
                what: "session",
                which: was.id.to_string(),
                field: "the session itself is not there",
            })?;
        let field = if now.sid != was.sid {
            Some("sid")
        } else if now.bot != was.bot {
            Some("bot")
        } else if now.focus != was.focus {
            Some("focus")
        } else if now.started_at != was.started_at {
            Some("started_at")
        } else if now.state != was.state {
            Some("state")
        } else if now.entries.len() != was.entries.len() {
            Some("the number of chronology entries")
        } else {
            None
        };
        if let Some(field) = field {
            return Err(HandoverError::Mismatch {
                what: "session",
                which: was.id.to_string(),
                field,
            });
        }
        report.sessions.verified += 1;

        // The chronology is compared **in order**, because the order is the
        // record: two entries that both landed but swapped places is a
        // chronology that no longer says what happened first.
        for (position, (was_entry, now_entry)) in
            was.entries.iter().zip(now.entries.iter()).enumerate()
        {
            let field = if now_entry.id != was_entry.id {
                Some("the entry at this position is a different entry")
            } else if now_entry.at != was_entry.at {
                Some("at")
            } else if now_entry.text != was_entry.text {
                Some("text")
            } else if now_entry.touched != was_entry.touched {
                Some("touched")
            } else if now_entry.beat != was_entry.beat {
                Some("beat")
            } else {
                None
            };
            if let Some(field) = field {
                return Err(HandoverError::Mismatch {
                    what: "chronology entry",
                    which: format!("{} #{}", was.id, position + 1),
                    field,
                });
            }
            report.entries.verified += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::mailboxes::DoltMailboxes;
    use crate::dolt::sessions::DoltSessions;
    use crate::dolt::tests::{Scratch, free_port};
    use crate::dolt::{Dolt, migrate};
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{Guarded, MailboxName, NewMessage, StateCounts};
    use jojobot_domain::memory::EntityId;
    use jojobot_domain::session::testing::InMemorySessions;
    use jojobot_domain::session::{NewEntry, NewSession, Sid};
    use std::sync::Arc;

    /// Every well-formed owner resolves. Ownership has its own cases; a
    /// handover that refused an owner would be answering a question nobody
    /// asked here.
    struct AnyOwner;

    #[async_trait::async_trait]
    impl jojobot_domain::mailbox::OwnerIndex for AnyOwner {
        async fn look_up(
            &self,
            _: &EntityId,
        ) -> Result<jojobot_domain::mailbox::OwnerLookup, MailboxError> {
            Ok(jojobot_domain::mailbox::OwnerLookup::Known)
        }
    }

    fn at(offset: i64) -> jiff::Timestamp {
        jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant")
            + jiff::SignedDuration::from_secs(offset)
    }

    /// The old board, with a message in **each** state and a session with a
    /// chronology on it.
    ///
    /// Every state, deliberately: a handover that filed everything as `new`
    /// would satisfy a fixture that only ever posted, and losing which messages
    /// are already handled is the defect that costs a reader most.
    async fn old_board() -> (InMemoryMailboxes, InMemorySessions) {
        let mail = InMemoryMailboxes::knowing_any_owner();
        let owner = EntityId("bot:gamma".into());
        for name in ["gamma", "delta"] {
            mail.create_mailbox(&MailboxName(name.into()), &owner, None)
                .await
                .expect("create ok")
                .written()
                .expect("not blocked");
        }
        let post = async |body: &str, offset: i64| {
            mail.post_message(NewMessage {
                mailbox: MailboxName("gamma".into()),
                body: body.to_string(),
                subject: Some("a subject that must survive".into()),
                sender: "gamma".into(),
                sent_at: at(offset),
                in_reply_to: None,
            })
            .await
            .expect("post ok")
            .written()
            .expect("not blocked")
        };
        let untouched = post("nobody has taken this one \u{1F5FF} \u{1D11E} \u{0301}e", 0).await;
        let taken = post("somebody took this one", 1).await;
        let handled = post("somebody finished this one", 2).await;
        mail.read_message(&taken.id).await.expect("read ok");
        mail.mark_processed(&handled.id, Some("the outcome, recorded"))
            .await
            .expect("processed ok");
        // A card the old store cannot read. It cannot be carried, and it must
        // not be silently absent from the report either.
        mail.quarantine(
            &MailboxName("gamma".into()),
            &jojobot_domain::mailbox::MessageId("hand-edited".into()),
            "a person edited it past parsing",
        );
        let _ = untouched;

        let sessions = InMemorySessions::new();
        let run = sessions
            .begin(NewSession {
                bot: owner.clone(),
                sid: Sid("abcd".into()),
                focus: "carrying the board across".into(),
                started_at: at(0),
            })
            .await
            .expect("begin ok");
        sessions
            .append(&run.id, NewEntry::manual("what I set out to do", at(1)))
            .await
            .expect("append ok");
        sessions
            .append(&run.id, NewEntry::manual("what I found", at(2)))
            .await
            .expect("append ok");
        (mail, sessions)
    }

    /// A live target, and the process holding it up.
    async fn new_store(what: &str) -> (Dolt, DoltMailboxes, DoltSessions) {
        let scratch = Scratch::new(what);
        let path = scratch.0.clone();
        // Leaked deliberately: dropping it removes the data under a running
        // server. The caller stops the process and the temp dir goes with it.
        std::mem::forget(scratch);
        let store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        let mail = DoltMailboxes::open(store.pool().clone(), Arc::new(AnyOwner));
        let sessions = DoltSessions::open(store.pool().clone());
        (store, mail, sessions)
    }

    /// **The whole handover: every record across, every state intact, and the
    /// numbers to check it by.**
    ///
    /// The verification is a comparison the module makes against the target's
    /// own read path, and this case asserts the comparison ran — `verified`
    /// equal to `read` — rather than trusting that it did.
    #[tokio::test]
    async fn every_record_crosses_and_reads_back_as_itself() {
        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("handover").await;

        let report = run(&old_mail, &old_sessions, &mail, &sessions, store.pool())
            .await
            .expect("the handover completes");

        assert!(report.whole(), "every kind came through whole: {report:?}");
        assert_eq!(report.boxes.read, 2);
        assert_eq!(report.messages.read, 3);
        assert_eq!(report.sessions.read, 1);
        assert_eq!(report.entries.read, 2);
        assert_eq!(
            report.messages.verified, 3,
            "the comparison ran on every message, not on none: {report:?}"
        );

        // **The states survived**, which is the half a count cannot show. A
        // handover that filed everything as `new` satisfies every number above.
        let landed = mail.list_mailboxes().await.expect("list ok");
        let gamma = landed
            .iter()
            .find(|b| b.name.as_str() == "gamma")
            .expect("the box came across");
        assert_eq!(
            gamma.counts,
            StateCounts {
                new: 1,
                read: 1,
                processed: 1
            },
            "one message in each state, exactly as the old board had them"
        );
        let processed = mail
            .scan_messages()
            .await
            .expect("scan ok")
            .into_iter()
            .find(|m| m.state == jojobot_domain::mailbox::MessageState::Processed)
            .expect("the handled message came across handled");
        assert_eq!(
            processed.notes.as_deref(),
            Some("the outcome, recorded"),
            "its notes came with it — a processed message without them is a record of nothing"
        );
        assert_eq!(
            processed.subject.as_deref(),
            Some("a subject that must survive")
        );

        // **What could not be carried is named, not dropped.**
        assert_eq!(
            report
                .not_carried
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec!["hand-edited"],
            "the unreadable card is reported as not carried: {report:?}"
        );

        // **The counters cleared what was carried — ALL THREE of them.**
        // Ids come across as they are, so a counter left where it was mints an
        // id a carried record already wears and the first write after the
        // cutover collides on a record nobody can see.
        //
        // Each counter is exercised by writing the thing it mints for. Proving
        // one of the three proves nothing about the other two: they are three
        // separate rows named by three separate strings, and a misspelling in
        // any of them is silent until the first write lands on top of a carried
        // record.
        let posted = mail
            .post_message(NewMessage {
                mailbox: MailboxName("gamma".into()),
                body: "the first message written after the move".into(),
                subject: None,
                sender: "gamma".into(),
                sent_at: at(9),
                in_reply_to: None,
            })
            .await
            .expect("the store takes a new message after the handover")
            .written()
            .expect("not blocked");
        assert!(
            mail.scan_messages()
                .await
                .expect("scan ok")
                .iter()
                .filter(|m| m.id == posted.id)
                .count()
                == 1,
            "the new message got an id nothing else wears"
        );

        // The session counter, and the entry counter under it.
        let fresh = sessions
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("efgh".into()),
                focus: "the first run after the move".into(),
                started_at: at(9),
            })
            .await
            .expect("the store takes a new session after the handover");
        let carried_ids: Vec<String> = sessions
            .all_sessions()
            .await
            .expect("list ok")
            .iter()
            .map(|s| s.id.to_string())
            .collect();
        assert_eq!(
            carried_ids
                .iter()
                .filter(|id| *id == &fresh.id.to_string())
                .count(),
            1,
            "the new session got an id nothing else wears: {carried_ids:?}"
        );

        let appended = sessions
            .append(
                &fresh.id,
                NewEntry::manual("the first beat after the move", at(10)),
            )
            .await
            .expect("the store takes a new entry after the handover");
        let carried = sessions.all_sessions().await.expect("list ok");
        // **Beyond every carried id, not merely different from them.** An
        // absence of collision can be luck — two id shapes that happen not to
        // overlap — and luck is not what the counter is for.
        //
        // Only ids that are numbers count here, because only those come from a
        // counter this store mints from. A source whose entry ids wear a prefix
        // contributes none, the counter is left at zero, and nothing can
        // collide because this store never mints that shape. That is why
        // `highest` ignores them rather than trying to read a number out.
        let numeric = |id: &str| id.parse::<i64>().ok();
        let carried_entries: Vec<i64> = carried
            .iter()
            .flat_map(|s| s.entries.iter())
            .filter(|e| e.id != appended.id)
            .filter_map(|e| numeric(e.id.as_str()))
            .collect();
        let minted = numeric(appended.id.as_str()).expect("this store mints numeric entry ids");
        assert!(
            carried_entries.iter().all(|carried| *carried < minted),
            "the new entry's id is beyond every carried one: {minted} against {carried_entries:?}"
        );

        // The chronology came across in order, read through the new store.
        let carried = sessions
            .all_sessions()
            .await
            .expect("list ok")
            .pop()
            .expect("the session came across");
        assert_eq!(carried.focus, "carrying the board across");
        assert_eq!(
            carried
                .entries
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            vec!["what I set out to do", "what I found"],
            "oldest first, as it was"
        );

        store.stop().await;
    }

    /// **A second run refuses rather than doubling**, and the refusal names
    /// what is already there.
    ///
    /// This runs on every start once it ships, so "it would double a mailbox"
    /// is not a hypothetical — it is what happens on the second boot.
    #[tokio::test]
    async fn a_second_handover_refuses_and_writes_nothing() {
        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("handover-twice").await;

        run(&old_mail, &old_sessions, &mail, &sessions, store.pool())
            .await
            .expect("the first handover completes");
        let after_first = mail.scan_messages().await.expect("scan ok").len();

        let again = run(&old_mail, &old_sessions, &mail, &sessions, store.pool()).await;
        assert!(
            matches!(again, Err(HandoverError::Populated { .. })),
            "a populated target refuses: {again:?}"
        );
        assert_eq!(
            mail.scan_messages().await.expect("scan ok").len(),
            after_first,
            "and the refusal wrote nothing — the board is what the first run left"
        );

        store.stop().await;
    }

    /// **Each table is guarded on its own.**
    ///
    /// The whole-board case populates all four, so any one check can be deleted
    /// and the other three still refuse — every guard invisible behind its
    /// neighbours. This puts exactly one row on the target at a time, so each
    /// check is the only thing that can produce the refusal.
    ///
    /// It is not a hypothetical split. A running jojobot writes a mailbox when
    /// a bot is created and a session on that session's first write, without
    /// necessarily writing a message or a beat — so a target holding sessions
    /// and nothing else is an ordinary state, and only the session check sees
    /// it.
    #[tokio::test]
    async fn every_table_refuses_the_handover_on_its_own() {
        let (old_mail, old_sessions) = old_board().await;
        let scratch = Scratch::new("handover-guards");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");

        // One row, in one table, per case — and the row is the least a real
        // occupant could leave behind.
        for (n, (row, expected)) in [
            (
                "INSERT INTO mailbox (name, owner) VALUES ('squatter', 'bot:gamma')",
                "mailboxes",
            ),
            (
                "INSERT INTO message (id, mailbox, ordinal, body, subject, sender, sent_at, state,                  notes, in_reply_to) VALUES ('sq', 'squatter', 1, 'b', NULL, 's',                  '2026-01-01T00:00:00Z', 'new', NULL, NULL)",
                "messages",
            ),
            (
                "INSERT INTO session (id, sid, bot, focus, started_at, state) VALUES ('sq', NULL,                  'bot:gamma', 'squatting', '2026-01-01T00:00:00Z', 'active')",
                "sessions",
            ),
            (
                "INSERT INTO journal_entry (session, id, ordinal, at, text, touched, beat) VALUES                  ('sq', 'e1', 1, '2026-01-01T00:00:00Z', 'a beat', NULL, NULL)",
                "chronology entries",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let pool = store
                .database(&format!("guard{n}"))
                .await
                .expect("a database of this case's own");
            migrate::run(&pool).await.expect("the schema");
            sqlx::query(row)
                .execute(&pool)
                .await
                .expect("the occupant lands");

            let mail = DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner));
            let sessions = DoltSessions::open(pool.clone());
            let outcome = run(&old_mail, &old_sessions, &mail, &sessions, &pool).await;

            let Err(HandoverError::Populated { what, held }) = outcome else {
                panic!("a target already holding {expected} must refuse: {outcome:?}");
            };
            assert_eq!(
                what, expected,
                "the refusal names the kind it found, so a person knows what to clear"
            );
            assert_eq!(held, 1);

            // …and it refused before writing: the occupant is still alone.
            let boxes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mailbox")
                .fetch_one(&pool)
                .await
                .expect("count ok");
            assert!(
                boxes <= 1,
                "a refused handover writes nothing, so no board came across"
            );
        }

        store.stop().await;
    }

    /// **Every field the verification compares is proven by a target that
    /// changes exactly that field.**
    ///
    /// The verification reads through the port, so a target whose read path
    /// disagrees with what was written is the condition it exists for — and the
    /// only way to produce one deliberately.
    ///
    /// **One case per field, because one case proves one comparison.** A single
    /// changed body leaves the other seven clauses unreached: drop any of them
    /// and the handover reports success over a record that did not survive.
    /// `state` is the one that matters most — the whole reason this writes rows
    /// instead of calling the posting verb is that a state must not move, and a
    /// missing comparison there would report a clean migration over mail that
    /// had silently gone back to unread.
    #[tokio::test]
    async fn each_field_the_verification_compares_is_proven_on_its_own() {
        /// The real store, with one message rewritten on the way out — the
        /// shape of a store that accepted a write and kept something else.
        struct Mangling(DoltMailboxes, fn(&mut Message));

        #[async_trait::async_trait]
        impl Mailboxes for Mangling {
            async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
                let mut messages = self.0.scan_messages().await?;
                if let Some(first) = messages.first_mut() {
                    (self.1)(first);
                }
                Ok(messages)
            }
            async fn create_mailbox(
                &self,
                name: &MailboxName,
                owner: &EntityId,
                token: Option<&str>,
            ) -> Result<Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError> {
                self.0.create_mailbox(name, owner, token).await
            }
            async fn list_mailboxes(
                &self,
            ) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
                self.0.list_mailboxes().await
            }
            async fn post_message(
                &self,
                message: NewMessage,
            ) -> Result<Guarded<Message>, MailboxError> {
                self.0.post_message(message).await
            }
            async fn read_mailbox(
                &self,
                name: &MailboxName,
            ) -> Result<Guarded<jojobot_domain::mailbox::Delivery>, MailboxError> {
                self.0.read_mailbox(name).await
            }
            async fn read_message(
                &self,
                id: &jojobot_domain::mailbox::MessageId,
            ) -> Result<jojobot_domain::mailbox::Delivered, MailboxError> {
                self.0.read_message(id).await
            }
            async fn mark_processed(
                &self,
                id: &jojobot_domain::mailbox::MessageId,
                notes: Option<&str>,
            ) -> Result<Message, MailboxError> {
                self.0.mark_processed(id, notes).await
            }
        }

        /// One field's mutation, and the clause it must make fire.
        type Case = (&'static str, fn(&mut Message));

        // Each mutation changes exactly one field to a value the source cannot
        // have had, so the clause named beside it is the only one that can fire.
        let cases: [Case; 8] = [
            ("mailbox", |m| m.mailbox = MailboxName("delta".into())),
            ("body", |m| m.body.push_str(" and something nobody wrote")),
            ("subject", |m| {
                m.subject = Some("a title nobody gave it".into())
            }),
            ("sender", |m| m.sender = "somebody-else".into()),
            ("sent_at", |m| {
                m.sent_at += jiff::SignedDuration::from_secs(1)
            }),
            ("state", |m| {
                m.state = jojobot_domain::mailbox::MessageState::Processed
            }),
            ("notes", |m| {
                m.notes = Some("an outcome nobody recorded".into())
            }),
            ("in_reply_to", |m| {
                m.in_reply_to = Some(jojobot_domain::mailbox::MessageId("1".into()))
            }),
        ];

        for (expected, mangle) in cases {
            let (old_mail, old_sessions) = old_board().await;
            let (mut store, mail, sessions) = new_store(&format!("mismatch-{expected}")).await;

            let outcome = run(
                &old_mail,
                &old_sessions,
                &Mangling(mail, mangle),
                &sessions,
                store.pool(),
            )
            .await;

            let Err(HandoverError::Mismatch { what, field, .. }) = outcome else {
                panic!("a changed {expected} must fail the handover: {outcome:?}");
            };
            assert_eq!(what, "message");
            assert_eq!(
                field, expected,
                "and it names the field that moved, so nobody has to diff two records by eye"
            );

            store.stop().await;
        }
    }
}

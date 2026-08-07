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
//! # Done-ness is a record, not an inference
//!
//! "The target has rows in it" cannot tell a completed run from a half-verified
//! one, and once this store is the one being served from, that gap is a way to
//! lose data rather than a nicety. Wiping the data directory is the obvious
//! repair; the old store still holds the pre-migration snapshot; an empty target
//! then looks exactly like a fresh install, and the next start carries the OLD
//! records back over everything written since.
//!
//! So the handover writes down what it did. Two states, because verification is
//! post-commit: `written` goes in with the carried rows, `verified` is set once
//! the read-back passed. [`carry_over`] is the verb a start calls, and **only a
//! `verified` record lets it say the store may be served from.**
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
    /// **The record says the rows were committed and the read-back never
    /// finished.** The target holds a board nobody checked, so it must not be
    /// served from until a person has looked at it.
    #[error(
        "the handover's record says '{state}', not 'verified' — the rows were committed and the \
         read-back never completed, so the store holds a board nobody checked"
    )]
    Halfway {
        /// The token the record wears, quoted rather than interpreted: a state
        /// this build does not know is exactly the thing a person must see.
        state: String,
    },
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

/// **What the record is a record of.** One handover, named — so the row says
/// which body of records it speaks for rather than being a bare flag.
const CARRIED: &str = "mail-and-sessions";

/// The rows are committed and the read-back has not passed yet.
const WRITTEN: &str = "written";

/// The read-back passed. **This and only this means the store may be served
/// from**, and everything that is not exactly this token refuses.
const VERIFIED: &str = "verified";

fn source_mail(e: MailboxError) -> HandoverError {
    HandoverError::Source(e.to_string())
}

fn source_session(e: SessionError) -> HandoverError {
    HandoverError::Source(e.to_string())
}

/// A read-back the target refused.
///
/// **Post-commit, always**: the verification runs after the rows are in, and
/// this is the target's own read path. So a refusal here says the target holds
/// records nobody could check — never that the old store was unreadable, which
/// would tell a caller nothing was written and it is safe to retry. The
/// underlying cause is logged because the variant carries only names.
fn unread_back(what: &'static str) -> impl FnOnce(MailboxError) -> HandoverError {
    move |e| {
        tracing::error!(error = %e, what, "the handover could not read the carried records back");
        HandoverError::Mismatch {
            what,
            which: "all of them".into(),
            field: "the read-back itself",
        }
    }
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

    // **The record goes in with the rows it is about.** Same transaction, so
    // there is no state where the records are committed and nothing says a
    // handover happened — which is the state a later boot cannot tell from
    // somebody else's data, and refuses.
    sqlx::query("INSERT INTO handover (what, state) VALUES (?, ?)")
        .bind(CARRIED)
        .bind(WRITTEN)
        .execute(&mut *tx)
        .await
        .map_err(target)?;

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

    // Only now. The record was `written` from the commit until this line, which
    // is exactly the window in which the target holds a board nobody checked.
    promote(pool).await?;
    Ok(report)
}

/// Say the read-back passed.
///
/// Its own commit, and it cannot be otherwise: the verification reads through
/// the target's own read path, which is a caller of the pool rather than of the
/// handover's transaction, so it can only run once that transaction is gone.
///
/// **A store that refuses this update wedges the boot, on purpose.** The
/// handover really did complete, and the record still says `written`, so this
/// start and every later one refuse until a person looks — which is the rule
/// applied to itself rather than an oversight: a run that could not say it
/// verified has not said it.
///
/// **UNPROVEN IN THE CODE: nothing makes this failure alone go red.** No test
/// produces a store that takes the carried rows, answers the whole read-back
/// and then refuses one `UPDATE`. The state it leaves behind — a `written`
/// record over committed rows — is covered, by the case that reaches it through
/// a failed read-back instead.
async fn promote(pool: &MySqlPool) -> Result<(), HandoverError> {
    sqlx::query("UPDATE handover SET state = ? WHERE what = ?")
        .bind(VERIFIED)
        .bind(CARRIED)
        .execute(pool)
        .await
        .map_err(target)?;
    Ok(())
}

/// What the record says, or nothing at all if there is no record.
///
/// The token is returned as it is read, never parsed into a state this build
/// knows: a token this build does not recognise must reach a person intact, and
/// mapping it onto a known state on the way is how it would stop doing that.
async fn recorded(pool: &MySqlPool) -> Result<Option<String>, HandoverError> {
    sqlx::query_scalar::<_, String>("SELECT state FROM handover WHERE what = ?")
        .bind(CARRIED)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "the store would not say whether the handover has run");
            HandoverError::Target("the store would not say whether the handover has run".into())
        })
}

/// What a boot found when it asked whether the records still need carrying.
#[derive(Debug)]
pub enum Carryover {
    /// This boot carried them, and the read-back passed.
    Carried(Report),
    /// A previous boot carried them and its read-back passed.
    AlreadyCarried,
    /// **The store must not be served from.**
    Refused(HandoverError),
}

/// **The store may be served from only when the record says verified.
/// Everything else refuses.**
///
/// That is the whole rule, and it is one line of code rather than a list of
/// cases on purpose: the accepting arm names the one token it accepts, and
/// everything that is not that token — a state this build has never heard of, a
/// failure `run` grew a variant for after this was written — lands on the
/// refusing side without anybody remembering to put it there.
///
/// What the arms mean:
///
/// - **verified** — a previous boot did this and its read-back passed. The
///   source is not touched. `run` reads the entire old board before it looks at
///   whether the target is populated, so a steady-state boot that reached it
///   would pay a full remote scan to learn it has nothing to do.
/// - **no record, and `run` succeeds** — this boot carried them.
/// - **written** — the rows are committed and the read-back never finished. The
///   target holds a board nobody checked, and no count of what is in it can
///   tell that from a completed run. A person has to look.
/// - **no record and rows already there** — `run`'s own refusal. Something else
///   wrote them, and adopting them is the guess this build refuses to make.
///
/// **There is no arm for a report that came back partial**, because `run`
/// cannot produce one: every counter is either incremented once per record read
/// or the function has already returned `Err` — the write loops bail on a
/// refused insert, and the read-back loops bail on the first record that
/// differs, with the chronology's length compared before its entries are
/// walked. So `Ok` implies `report.whole()`. If a later change makes a partial
/// report reachable, this is where the policy for it has to be decided rather
/// than inferred.
pub async fn carry_over(
    from_mail: &dyn Mailboxes,
    from_sessions: &dyn Sessions,
    to_mail: &dyn Mailboxes,
    to_sessions: &dyn Sessions,
    pool: &MySqlPool,
) -> Carryover {
    match recorded(pool).await {
        Err(unreadable) => Carryover::Refused(unreadable),
        Ok(Some(state)) if state == VERIFIED => Carryover::AlreadyCarried,
        Ok(Some(state)) => Carryover::Refused(HandoverError::Halfway { state }),
        Ok(None) => match run(from_mail, from_sessions, to_mail, to_sessions, pool).await {
            Ok(report) => Carryover::Carried(report),
            Err(refused) => Carryover::Refused(refused),
        },
    }
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
    let landed_boxes = to_mail
        .list_mailboxes()
        .await
        .map_err(unread_back("mailbox"))?;
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

    let landed = to_mail
        .scan_messages()
        .await
        .map_err(unread_back("message"))?;
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
    use jojobot_domain::session::{JournalEntry, NewEntry, NewSession, SessionId, Sid};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    /// The old board, wrapped so a test can say whether it was READ at all.
    ///
    /// The steady state's whole claim is that a boot with a verified record
    /// never touches the old store. `AlreadyCarried` on its own is compatible
    /// with a run that scanned the entire old board first and threw the answer
    /// away — a full remote scan, every boot, to learn there is nothing to do.
    /// So the reads are counted rather than inferred from the outcome.
    struct WatchedMail {
        board: InMemoryMailboxes,
        reads: AtomicUsize,
        refuses: bool,
    }

    impl WatchedMail {
        fn watching(board: InMemoryMailboxes) -> Self {
            WatchedMail {
                board,
                reads: AtomicUsize::new(0),
                refuses: false,
            }
        }
        /// The same double, unreadable — an old store that will not answer.
        fn refusing(board: InMemoryMailboxes) -> Self {
            WatchedMail {
                refuses: true,
                ..WatchedMail::watching(board)
            }
        }
        fn reads(&self) -> usize {
            self.reads.load(Ordering::Relaxed)
        }
        fn forget(&self) {
            self.reads.store(0, Ordering::Relaxed);
        }
        /// Count the read, then answer it — or refuse.
        fn asked(&self) -> Result<(), MailboxError> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            if self.refuses {
                return Err(MailboxError::Store("the old store would not answer".into()));
            }
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl Mailboxes for WatchedMail {
        async fn list_mailboxes(
            &self,
        ) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
            self.asked()?;
            self.board.list_mailboxes().await
        }
        async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
            self.asked()?;
            self.board.scan_messages().await
        }
        async fn create_mailbox(
            &self,
            name: &MailboxName,
            owner: &EntityId,
            token: Option<&str>,
        ) -> Result<Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError> {
            self.board.create_mailbox(name, owner, token).await
        }
        async fn post_message(
            &self,
            message: NewMessage,
        ) -> Result<Guarded<Message>, MailboxError> {
            self.board.post_message(message).await
        }
        async fn read_mailbox(
            &self,
            name: &MailboxName,
        ) -> Result<Guarded<jojobot_domain::mailbox::Delivery>, MailboxError> {
            self.board.read_mailbox(name).await
        }
        async fn read_message(
            &self,
            id: &jojobot_domain::mailbox::MessageId,
        ) -> Result<jojobot_domain::mailbox::Delivered, MailboxError> {
            self.board.read_message(id).await
        }
        async fn mark_processed(
            &self,
            id: &jojobot_domain::mailbox::MessageId,
            notes: Option<&str>,
        ) -> Result<Message, MailboxError> {
            self.board.mark_processed(id, notes).await
        }
    }

    /// The session half of the same watch. Counted separately because the two
    /// sources are two remote boards, and a skip that touched only one of them
    /// is still a skip that touched a source.
    struct WatchedSessions(InMemorySessions, AtomicUsize);

    impl WatchedSessions {
        fn watching(runs: InMemorySessions) -> Self {
            WatchedSessions(runs, AtomicUsize::new(0))
        }
        fn reads(&self) -> usize {
            self.1.load(Ordering::Relaxed)
        }
        fn forget(&self) {
            self.1.store(0, Ordering::Relaxed);
        }
    }

    #[async_trait::async_trait]
    impl Sessions for WatchedSessions {
        async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.all_sessions().await
        }
        async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.0.read_session(id).await
        }
        async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
            self.0.sessions_of(bot).await
        }
        async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
            self.0.begin(new).await
        }
        async fn append(
            &self,
            id: &SessionId,
            entry: NewEntry,
        ) -> Result<JournalEntry, SessionError> {
            self.0.append(id, entry).await
        }
        async fn amend_last(
            &self,
            id: &SessionId,
            text: &str,
        ) -> Result<JournalEntry, SessionError> {
            self.0.amend_last(id, text).await
        }
        async fn amend_beat(
            &self,
            id: &SessionId,
            entry: &jojobot_domain::session::EntryId,
            text: &str,
            at: jiff::Timestamp,
        ) -> Result<JournalEntry, SessionError> {
            self.0.amend_beat(id, entry, text, at).await
        }
        async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
            self.0.set_focus(id, focus).await
        }
        async fn close(
            &self,
            id: &SessionId,
            to: jojobot_domain::session::SessionState,
        ) -> Result<Session, SessionError> {
            self.0.close(id, to).await
        }
        async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.0.reopen(id).await
        }
    }

    /// What the record says, read the way an operator would rather than through
    /// this module's own helper — a verify that shares the reader it is checking
    /// is not a verify.
    async fn recorded_state(pool: &MySqlPool) -> Option<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM handover WHERE what = 'mail-and-sessions'",
        )
        .fetch_optional(pool)
        .await
        .expect("the record is readable")
    }

    /// The old board, with a message in **each** state and a session with a
    /// chronology on it.
    ///
    /// Every state, deliberately: a handover that filed everything as `new`
    /// would satisfy a fixture that only ever posted, and losing which messages
    /// are already handled is the defect that costs a reader most.
    /// The real store, with one message rewritten on the way out — the shape of
    /// a store that accepted a write and kept something else.
    ///
    /// Module-scoped because two questions need it: which comparison fires, and
    /// what a handover whose read-back failed LEAVES BEHIND.
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

    /// **Every field the SESSION verification compares is proven by a target
    /// that changes exactly that field.**
    ///
    /// The message half's case one tier over. A session's five fields and an
    /// entry's four are compared, and nothing produced a mismatch on any of
    /// them, so every clause could be dropped and the handover would report
    /// success over a chronology that did not survive.
    ///
    /// The order of the chronology is compared too, and it is the one that
    /// cannot be checked field by field: two entries that both landed but
    /// swapped places is a record that no longer says what happened first.
    #[tokio::test]
    async fn each_session_field_the_verification_compares_is_proven_on_its_own() {
        /// The real store, with one session rewritten on the way out.
        struct Warping(DoltSessions, fn(&mut Session));

        #[async_trait::async_trait]
        impl Sessions for Warping {
            async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
                let mut session = self.0.read_session(id).await?;
                (self.1)(&mut session);
                Ok(session)
            }
            async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
                self.0.sessions_of(bot).await
            }
            async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
                self.0.all_sessions().await
            }
            async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
                self.0.begin(new).await
            }
            async fn append(
                &self,
                id: &SessionId,
                entry: NewEntry,
            ) -> Result<JournalEntry, SessionError> {
                self.0.append(id, entry).await
            }
            async fn amend_last(
                &self,
                id: &SessionId,
                text: &str,
            ) -> Result<JournalEntry, SessionError> {
                self.0.amend_last(id, text).await
            }
            async fn amend_beat(
                &self,
                id: &SessionId,
                entry: &jojobot_domain::session::EntryId,
                text: &str,
                at: jiff::Timestamp,
            ) -> Result<JournalEntry, SessionError> {
                self.0.amend_beat(id, entry, text, at).await
            }
            async fn set_focus(
                &self,
                id: &SessionId,
                focus: &str,
            ) -> Result<Session, SessionError> {
                self.0.set_focus(id, focus).await
            }
            async fn close(
                &self,
                id: &SessionId,
                to: jojobot_domain::session::SessionState,
            ) -> Result<Session, SessionError> {
                self.0.close(id, to).await
            }
            async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
                self.0.reopen(id).await
            }
        }

        /// One field's mutation, and the clause it must make fire.
        type Case = (&'static str, fn(&mut Session));

        // Each mutation changes exactly one field to a value the source cannot
        // have had, so the clause named beside it is the only one that can fire.
        let cases: [Case; 10] = [
            ("sid", |s| s.sid = Some(Sid("zzzz".into()))),
            ("bot", |s| s.bot = EntityId("bot:delta".into())),
            ("focus", |s| s.focus = "something it never worked on".into()),
            ("started_at", |s| {
                s.started_at += jiff::SignedDuration::from_secs(1)
            }),
            ("state", |s| {
                s.state = jojobot_domain::session::SessionState::Wrapped
            }),
            ("the number of chronology entries", |s| {
                s.entries.pop();
            }),
            // The entry clauses. The chronology is compared in order, so the
            // first entry is where a difference has to land for the position to
            // be the one reported.
            ("at", |s| {
                s.entries[0].at += jiff::SignedDuration::from_secs(1)
            }),
            ("text", |s| s.entries[0].text = "a beat nobody wrote".into()),
            ("touched", |s| s.entries[0].touched = Some(s.entries[0].at)),
            ("beat", |s| {
                s.entries[0].beat = Some("a class nobody recorded".into())
            }),
        ];

        for (expected, warp) in cases {
            let (old_mail, old_sessions) = old_board().await;
            let (mut store, mail, sessions) =
                new_store(&format!("session-mismatch-{}", expected.replace(' ', "-"))).await;

            let outcome = run(
                &old_mail,
                &old_sessions,
                &mail,
                &Warping(sessions, warp),
                store.pool(),
            )
            .await;

            let Err(HandoverError::Mismatch { field, .. }) = outcome else {
                panic!("a changed {expected} must fail the handover: {outcome:?}");
            };
            assert_eq!(
                field, expected,
                "and it names the field that moved, so nobody has to diff two records by eye"
            );

            store.stop().await;
        }
    }

    /// **A chronology that landed in the wrong order fails the handover.**
    ///
    /// The one difference no field comparison can catch: every entry is
    /// present and every field of each is intact, and the record still no
    /// longer says what happened first.
    #[tokio::test]
    async fn a_chronology_that_came_back_reordered_fails_the_handover() {
        struct Reversing(DoltSessions);

        #[async_trait::async_trait]
        impl Sessions for Reversing {
            async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
                let mut session = self.0.read_session(id).await?;
                session.entries.reverse();
                Ok(session)
            }
            async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
                self.0.sessions_of(bot).await
            }
            async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
                self.0.all_sessions().await
            }
            async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
                self.0.begin(new).await
            }
            async fn append(
                &self,
                id: &SessionId,
                entry: NewEntry,
            ) -> Result<JournalEntry, SessionError> {
                self.0.append(id, entry).await
            }
            async fn amend_last(
                &self,
                id: &SessionId,
                text: &str,
            ) -> Result<JournalEntry, SessionError> {
                self.0.amend_last(id, text).await
            }
            async fn amend_beat(
                &self,
                id: &SessionId,
                entry: &jojobot_domain::session::EntryId,
                text: &str,
                at: jiff::Timestamp,
            ) -> Result<JournalEntry, SessionError> {
                self.0.amend_beat(id, entry, text, at).await
            }
            async fn set_focus(
                &self,
                id: &SessionId,
                focus: &str,
            ) -> Result<Session, SessionError> {
                self.0.set_focus(id, focus).await
            }
            async fn close(
                &self,
                id: &SessionId,
                to: jojobot_domain::session::SessionState,
            ) -> Result<Session, SessionError> {
                self.0.close(id, to).await
            }
            async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
                self.0.reopen(id).await
            }
        }

        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("session-reordered").await;

        let outcome = run(
            &old_mail,
            &old_sessions,
            &mail,
            &Reversing(sessions),
            store.pool(),
        )
        .await;

        let Err(HandoverError::Mismatch { what, field, .. }) = outcome else {
            panic!("a reordered chronology must fail the handover: {outcome:?}");
        };
        assert_eq!(what, "chronology entry");
        assert_eq!(field, "the entry at this position is a different entry");

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

    /// **A read-back that fails is a failure of the TARGET, and says so.**
    ///
    /// The verification runs after the commit, so by the time these reads
    /// happen the rows are already in the new store. Reporting a refusal here
    /// as if the old store were unreadable tells a caller the opposite of
    /// what is true — nothing was written, retry — while the target sits
    /// holding a board nobody verified.
    #[tokio::test]
    async fn a_read_back_the_target_refuses_is_a_mismatch_not_a_source_failure() {
        /// The real target, with one of its two read-back paths refusing.
        ///
        /// `run` reaches the target through this port only in the
        /// verification, so a refusal here can only be post-commit.
        struct Unreadable(DoltMailboxes, Option<Read>);

        #[derive(Clone, Copy, PartialEq)]
        enum Read {
            Boxes,
            Messages,
        }

        impl Unreadable {
            fn refuses(&self, read: Read) -> Result<(), MailboxError> {
                if self.1 == Some(read) {
                    return Err(MailboxError::Store("the store would not answer".into()));
                }
                Ok(())
            }
        }

        #[async_trait::async_trait]
        impl Mailboxes for Unreadable {
            async fn list_mailboxes(
                &self,
            ) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
                self.refuses(Read::Boxes)?;
                self.0.list_mailboxes().await
            }
            async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
                self.refuses(Read::Messages)?;
                self.0.scan_messages().await
            }
            async fn create_mailbox(
                &self,
                name: &MailboxName,
                owner: &EntityId,
                token: Option<&str>,
            ) -> Result<Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError> {
                self.0.create_mailbox(name, owner, token).await
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

        // One case per read-back, because each is its own mapping: fix the
        // boxes and the messages still lie.
        for (refusing, expected, what) in [
            (Read::Boxes, "the mailboxes", "mailbox"),
            (Read::Messages, "the messages", "message"),
        ] {
            let (old_mail, old_sessions) = old_board().await;
            let (mut store, mail, sessions) =
                new_store(&format!("read-back-{}", expected.replace(' ', "-"))).await;

            let outcome = run(
                &old_mail,
                &old_sessions,
                &Unreadable(mail, Some(refusing)),
                &sessions,
                store.pool(),
            )
            .await;

            let Err(HandoverError::Mismatch { what: kind, .. }) = outcome else {
                panic!(
                    "a target that will not hand {expected} back is a mismatch, \
                     not a source failure: {outcome:?}"
                );
            };
            assert_eq!(
                kind, what,
                "and it names the kind that could not be read back"
            );

            store.stop().await;
        }

        // **The positive the two negatives rest on.** The same decorator,
        // refusing nothing, carries the board and verifies it — so the cases
        // above failed because the read was refused, not because this double
        // breaks the handover or hands back an empty board either way.
        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("read-back-intact").await;
        let report = run(
            &old_mail,
            &old_sessions,
            &Unreadable(mail, None),
            &sessions,
            store.pool(),
        )
        .await
        .expect("a target that answers both reads completes the handover");
        assert!(report.whole(), "every kind came through whole: {report:?}");
        assert_eq!(report.boxes.verified, 2);
        assert_eq!(report.messages.verified, 3);

        store.stop().await;
    }

    /// **A completed handover leaves the record saying `verified`, and only a
    /// completed one does.**
    ///
    /// Both halves in one case, because either alone is satisfied by a build
    /// that gets the record wrong. A run that never wrote the record at all
    /// passes any assertion about the failing path; a run that wrote `verified`
    /// up front passes any assertion about the succeeding one.
    ///
    /// The failing path is produced the way it happens: verification is
    /// post-commit, so a target whose read-back disagrees leaves the rows in
    /// and the record un-promoted. That is the same state a death between the
    /// commit and the promotion leaves, and it is the state the next boot has
    /// to be able to tell from a completed run.
    #[tokio::test]
    async fn the_record_says_verified_only_after_the_read_back_passed() {
        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("record-verified").await;

        assert_eq!(
            recorded_state(store.pool()).await,
            None,
            "a store nothing has been carried into holds no record"
        );

        run(&old_mail, &old_sessions, &mail, &sessions, store.pool())
            .await
            .expect("the handover completes");
        assert_eq!(
            recorded_state(store.pool()).await.as_deref(),
            Some("verified"),
            "the read-back passed, so the record is promoted — this and only this means the \
             store may be served from"
        );

        store.stop().await;

        // The other half, on a store of its own: the read-back fails, so the
        // record stops at `written`.
        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("record-written").await;
        let outcome = run(
            &old_mail,
            &old_sessions,
            &Mangling(mail, |m| m.body.push_str(" and something nobody wrote")),
            &sessions,
            store.pool(),
        )
        .await;
        assert!(
            matches!(outcome, Err(HandoverError::Mismatch { .. })),
            "the read-back must fail for this half to mean anything: {outcome:?}"
        );
        assert_eq!(
            recorded_state(store.pool()).await.as_deref(),
            Some("written"),
            "the rows are committed and the read-back did not pass, so the record says so"
        );

        store.stop().await;
    }

    /// **A verified record answers the boot without touching the old store.**
    ///
    /// The steady state, and the reason the record is consulted FIRST. `run`
    /// reads the whole old board before it looks at whether the target is
    /// populated, so a boot that reached `run` to learn it had nothing to do
    /// would pay a full remote scan every time — and would still be paying it
    /// on a build where the outcome happens to come back right.
    ///
    /// So the claim asserted is that the source was NOT READ, positively, from
    /// a source that counts. Its positive twin is in the same case: the first
    /// call, through the same doubles, carries the board and the counters move
    /// — otherwise the zeroes below would pass on a double nobody ever calls.
    #[tokio::test]
    async fn a_verified_record_skips_the_handover_without_reading_the_source() {
        let (old_mail, old_sessions) = old_board().await;
        let source_mail = WatchedMail::watching(old_mail);
        let source_sessions = WatchedSessions::watching(old_sessions);
        let (mut store, mail, sessions) = new_store("carry-over-steady").await;

        let first = carry_over(
            &source_mail,
            &source_sessions,
            &mail,
            &sessions,
            store.pool(),
        )
        .await;
        let Carryover::Carried(report) = first else {
            panic!("the first boot carries the board: {first:?}");
        };
        assert!(report.whole(), "every kind came through whole: {report:?}");
        assert!(
            source_mail.reads() > 0 && source_sessions.reads() > 0,
            "the carrying boot DID read both sources — {} mail reads, {} session reads",
            source_mail.reads(),
            source_sessions.reads()
        );

        source_mail.forget();
        source_sessions.forget();

        let again = carry_over(
            &source_mail,
            &source_sessions,
            &mail,
            &sessions,
            store.pool(),
        )
        .await;
        assert!(
            matches!(again, Carryover::AlreadyCarried),
            "a verified record means a previous boot already did this: {again:?}"
        );
        assert_eq!(
            (source_mail.reads(), source_sessions.reads()),
            (0, 0),
            "and it was answered from the record — the old store was not touched at all"
        );

        store.stop().await;
    }

    /// **A record that says `written` and never reached `verified` refuses.**
    ///
    /// The state that makes the record worth having. The rows are committed and
    /// nobody checked them, which is indistinguishable from a completed run by
    /// any count of what the target holds — so a build that decides done-ness
    /// from "the target has rows in it" serves an unverified board and says
    /// nothing.
    ///
    /// It refuses with the state NAMED, because the answer here is that a
    /// person has to look, and "the handover refused" without the state sends
    /// them reading the code to find out which refusal they got.
    #[tokio::test]
    async fn a_record_that_never_verified_refuses_and_names_that_state() {
        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("carry-over-halfway").await;

        // The halfway state, made the way a real one is made: the commit lands
        // and the post-commit read-back does not pass.
        let interrupted = run(
            &old_mail,
            &old_sessions,
            &Mangling(mail, |m| m.body.push_str(" and something nobody wrote")),
            &sessions,
            store.pool(),
        )
        .await;
        assert!(
            matches!(interrupted, Err(HandoverError::Mismatch { .. })),
            "the read-back must fail to leave the state this case is about: {interrupted:?}"
        );
        assert_eq!(
            recorded_state(store.pool()).await.as_deref(),
            Some("written"),
            "the precondition: rows in, record un-promoted"
        );

        // The next boot, with a target that reads back perfectly well. Nothing
        // about the DATA is wrong now — only the record says the read-back
        // never finished, and that alone must stop it.
        let healthy = DoltMailboxes::open(store.pool().clone(), Arc::new(AnyOwner));
        let outcome = carry_over(&old_mail, &old_sessions, &healthy, &sessions, store.pool()).await;
        let Carryover::Refused(HandoverError::Halfway { state }) = &outcome else {
            panic!("a record that never verified must refuse as such: {outcome:?}");
        };
        assert_eq!(
            state, "written",
            "and it names the state the record wears, so a person knows what they are looking at"
        );

        store.stop().await;
    }

    /// **A record that cannot be read refuses, without touching the source.**
    ///
    /// Not hypothetical: a data directory restored from before this migration
    /// has every other table and no record table, which is exactly the shape
    /// this produces. "The record does not say verified" covers a record that
    /// cannot be consulted at all, and the refusal has to come from the record
    /// step rather than from whatever `run` happens to trip over later.
    ///
    /// Which is why the assertion is that the OLD STORE WAS NOT READ. A build
    /// that ignored the unreadable record and ran anyway also ends in a refusal
    /// — the carrying transaction fails on the missing table — so the outcome
    /// alone cannot tell the two apart. The counter can.
    #[tokio::test]
    async fn a_record_that_cannot_be_read_refuses_before_the_source_is_touched() {
        let (old_mail, old_sessions) = old_board().await;
        let source = WatchedMail::watching(old_mail);
        let (mut store, mail, sessions) = new_store("carry-over-no-record").await;

        sqlx::raw_sql("DROP TABLE handover")
            .execute(store.pool())
            .await
            .expect("the record table goes");

        let outcome = carry_over(&source, &old_sessions, &mail, &sessions, store.pool()).await;
        assert!(
            matches!(outcome, Carryover::Refused(_)),
            "a record this store will not answer for is a refusal: {outcome:?}"
        );
        assert_eq!(
            source.reads(),
            0,
            "and it refused at the record, before reading a thing from the old store"
        );

        store.stop().await;
    }

    /// **A target somebody else wrote to is refused, never adopted.**
    ///
    /// No record and rows already there is the one state that must not be
    /// guessed at: it is either a store this handover has no business writing
    /// to, or a repair that went half-way. Adopting it — treating the rows as
    /// though this handover had put them there — is the guess-instead-of-refuse
    /// trap, and it ends with the old snapshot carried over live data.
    #[tokio::test]
    async fn a_target_with_rows_and_no_record_refuses() {
        let (old_mail, old_sessions) = old_board().await;
        let (mut store, mail, sessions) = new_store("carry-over-squatter").await;
        sqlx::query("INSERT INTO mailbox (name, owner) VALUES ('squatter', 'bot:gamma')")
            .execute(store.pool())
            .await
            .expect("the occupant lands");

        let outcome = carry_over(&old_mail, &old_sessions, &mail, &sessions, store.pool()).await;
        assert!(
            matches!(outcome, Carryover::Refused(HandoverError::Populated { .. })),
            "rows nobody recorded are refused, not adopted: {outcome:?}"
        );

        // And it left both sides alone: no board came across, and nothing
        // wrote a record claiming it had.
        let boxes = mail.list_mailboxes().await.expect("list ok");
        assert_eq!(
            boxes.iter().map(|b| b.name.to_string()).collect::<Vec<_>>(),
            vec!["squatter".to_string()],
            "the occupant is still alone — the old board did not land on top of it"
        );
        assert_eq!(
            recorded_state(store.pool()).await,
            None,
            "and no record was minted for a handover that did not happen"
        );

        store.stop().await;
    }

    /// **Anything the handover refuses, the boot refuses.**
    ///
    /// Written as one arm rather than one per error, and this is what makes
    /// that structural instead of aspirational: an unreadable SOURCE is a
    /// different failure from a populated target, reaches this from a different
    /// place, and lands on the refusing side without a line of its own. A
    /// variant added later does the same.
    #[tokio::test]
    async fn a_handover_that_fails_refuses_the_boot() {
        let (old_mail, old_sessions) = old_board().await;
        let source = WatchedMail::refusing(old_mail);
        let (mut store, mail, sessions) = new_store("carry-over-unreadable").await;

        let outcome = carry_over(&source, &old_sessions, &mail, &sessions, store.pool()).await;
        assert!(
            matches!(outcome, Carryover::Refused(HandoverError::Source(_))),
            "an old store that will not answer refuses the boot: {outcome:?}"
        );

        // Nothing was carried and nothing was recorded — a refusal that left a
        // record behind would wedge every later boot.
        assert!(
            mail.list_mailboxes().await.expect("list ok").is_empty(),
            "no board came across"
        );
        assert_eq!(recorded_state(store.pool()).await, None);

        store.stop().await;
    }
}

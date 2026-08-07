//! **Mailboxes, as rows.**
//!
//! A box is one row and the messages in it are rows under it. The state machine
//! that was a column on a board is a column in a table, and the rules that held
//! it — `new → read → processed`, `processed` terminal, no deletion — are the
//! same rules; only where they are written down has moved.
//!
//! **What this adapter does NOT carry, and the absence is the point.** No
//! read-back guard, no golden fixture, no linearization lock, no escaping. Each
//! existed because a document editor rewrites prose that passes through it; a
//! SQL store hands back the bytes it was given, and a transaction either commits
//! or does not.
//!
//! **Quarantine survives the move, and it has to.** A row whose state token is
//! no state, or whose stamp is no instant, is a record jojobot cannot read. It
//! is not counted, not delivered, not scanned and not processable, and the box
//! it wears reports it as unreadable — because "N unreadable" is the difference
//! between a record nobody can act on and one nobody can see. That is why the
//! `state` column holds a token rather than an enum the store would enforce: a
//! store that cannot HOLD the condition cannot report it.
//!
//! **The owner question crosses a context boundary, through one narrow port.** A
//! box is created for somebody, and entities do not live here. The mail store
//! asks [`OwnerIndex`] whether a handle resolves and what is near it, which is
//! exactly what the refusal carries — never a fact, a kind or an edge.

use std::sync::Arc;

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::mailbox::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, MessageState, NewMessage, OwnerIndex, OwnerLookup, StateCounts, guard,
    normalize_body, normalize_notes, normalize_subject, validate_body, validate_mailbox_name,
    validate_message_id, validate_notes, validate_sender, validate_subject,
};
use jojobot_domain::memory::EntityId;
use sqlx::{MySql, MySqlPool, Row, Transaction};

/// Mailboxes kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets nobody
/// set.
#[derive(Clone)]
pub struct DoltMailboxes {
    pool: MySqlPool,
    owners: Arc<dyn OwnerIndex>,
}

impl DoltMailboxes {
    /// Open the store over an existing pool, with the index that answers
    /// whether an owner exists.
    ///
    /// **The schema is not this adapter's to create.** It arrives through the
    /// migrations the server applies on start — see [`crate::dolt::migrate`].
    pub fn open(pool: MySqlPool, owners: Arc<dyn OwnerIndex>) -> Self {
        DoltMailboxes { pool, owners }
    }

    /// Every box name, for the guards that screen against them.
    async fn names(&self) -> Result<Vec<MailboxName>, MailboxError> {
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM mailbox ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(store)?;
        Ok(names.into_iter().map(MailboxName).collect())
    }

    /// Every card on the board, readable and not, in delivery order.
    ///
    /// **One reader for every verb**, so what counts as unreadable cannot come
    /// to mean two different things in two places — the bug that would let a
    /// card be counted here and refused there.
    async fn cards(&self, tx: &mut Transaction<'_, MySql>) -> Result<Vec<Card>, MailboxError> {
        let rows = sqlx::query(
            "SELECT id, mailbox, ordinal, body, subject, sender, sent_at, state, notes, in_reply_to
             FROM message",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut cards: Vec<Card> = rows.iter().map(card_from).collect::<Result<_, _>>()?;
        // **Oldest by the instant the sender declared**, with the store's own
        // ordinal breaking a tie — the same total order every other tier reads.
        // Sorted here rather than in SQL because the stamp is text: a column
        // sort would be lexicographic, which is only chronological by accident.
        cards.sort_by_key(Card::order);
        Ok(cards)
    }
}

/// One row, read as far as it can be read.
#[derive(Debug, Clone)]
enum Card {
    /// A message jojobot can act on.
    Readable(Box<Message>, i64),
    /// A row jojobot cannot read as a message. It keeps its id and its box, so
    /// the box can say it is there; nothing else about it is trusted.
    Unreadable {
        id: MessageId,
        mailbox: MailboxName,
        ordinal: i64,
        reason: String,
    },
}

impl Card {
    fn id(&self) -> &MessageId {
        match self {
            Card::Readable(m, _) => &m.id,
            Card::Unreadable { id, .. } => id,
        }
    }

    fn mailbox(&self) -> &MailboxName {
        match self {
            Card::Readable(m, _) => &m.mailbox,
            Card::Unreadable { mailbox, .. } => mailbox,
        }
    }

    /// The total order a delivery reads in.
    fn order(&self) -> (Option<Timestamp>, i64) {
        match self {
            Card::Readable(m, ordinal) => (Some(m.sent_at), *ordinal),
            Card::Unreadable { ordinal, .. } => (None, *ordinal),
        }
    }

    fn readable(&self) -> Option<&Message> {
        match self {
            Card::Readable(m, _) => Some(m),
            Card::Unreadable { .. } => None,
        }
    }
}

/// A store failure, in the domain's own words. **The server's account never
/// crosses** — no SQL, no table names, no product (rule 53); it goes to the log
/// where an operator debugging a real failure wants it.
fn store(e: sqlx::Error) -> MailboxError {
    tracing::error!(error = %e, "the mailbox store failed");
    MailboxError::Store("the mailbox store could not be reached".into())
}

/// One row, as the domain's record — or as a card jojobot cannot read.
///
/// **An unreadable row is not an error.** The board carries what it carries;
/// refusing to answer at all because one row is malformed would take every
/// other message down with it, which is the opposite of what quarantine is for.
fn card_from(row: &sqlx::mysql::MySqlRow) -> Result<Card, MailboxError> {
    let id = MessageId(row.try_get::<String, _>("id").map_err(store)?);
    let mailbox = MailboxName(row.try_get::<String, _>("mailbox").map_err(store)?);
    let ordinal: i64 = row.try_get("ordinal").map_err(store)?;
    let unreadable = |reason: &str| Card::Unreadable {
        id: id.clone(),
        mailbox: mailbox.clone(),
        ordinal,
        reason: reason.to_string(),
    };

    let token: String = row.try_get("state").map_err(store)?;
    let Some(state) = MessageState::from_token(&token) else {
        return Ok(unreadable("it sits in no state jojobot recognizes"));
    };
    let stamp: String = row.try_get("sent_at").map_err(store)?;
    let Ok(sent_at) = stamp.parse::<Timestamp>() else {
        return Ok(unreadable("its send time cannot be read as a time"));
    };

    Ok(Card::Readable(
        Box::new(Message {
            id,
            mailbox,
            body: row.try_get::<String, _>("body").map_err(store)?,
            subject: row.try_get::<Option<String>, _>("subject").map_err(store)?,
            sender: row.try_get::<String, _>("sender").map_err(store)?,
            sent_at,
            state,
            notes: row.try_get::<Option<String>, _>("notes").map_err(store)?,
            in_reply_to: row
                .try_get::<Option<String>, _>("in_reply_to")
                .map_err(store)?
                .map(MessageId),
        }),
        ordinal,
    ))
}

/// The refusal every verb that addresses a card by id owes: a quarantined id is
/// on the board and cannot be read, which is a different answer from "no such
/// message" and has to stay one.
fn refuse(card: &Card) -> Result<&Message, MailboxError> {
    match card {
        Card::Readable(m, _) => Ok(m),
        Card::Unreadable { id, reason, .. } => Err(MailboxError::Quarantined {
            attempted: id.to_string(),
            reason: reason.clone(),
        }),
    }
}

/// The next id, minted inside the caller's transaction so two writers cannot
/// take the same one.
async fn mint(tx: &mut Transaction<'_, MySql>) -> Result<String, MailboxError> {
    // **`counter`, not `next`.** This store's parser treats `next` as a
    // reserved word and refuses the statement.
    sqlx::query(
        "INSERT INTO minted (kind, counter) VALUES ('message', 1)
         ON DUPLICATE KEY UPDATE counter = counter + 1",
    )
    .execute(&mut **tx)
    .await
    .map_err(store)?;
    let counter: i64 = sqlx::query_scalar("SELECT counter FROM minted WHERE kind = 'message'")
        .fetch_one(&mut **tx)
        .await
        .map_err(store)?;
    Ok(counter.to_string())
}

#[async_trait]
impl Mailboxes for DoltMailboxes {
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &EntityId,
        override_token: Option<&str>,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        validate_mailbox_name(name)?;
        jojobot_domain::memory::validate_subject(owner)
            .map_err(|e| MailboxError::InvalidName(e.to_string()))?;

        // **The owner is screened first.** "There is no such owner" is the more
        // fundamental mistake, and hearing it first matters: near-miss advice
        // about a box name is advice about a box the caller may have no
        // business creating at all.
        if let OwnerLookup::Unknown(candidates) = self.owners.look_up(owner).await? {
            return Ok(Guarded::UnknownOwner {
                attempted: owner.clone(),
                candidates,
            });
        }

        let existing = self.names().await?;
        if let guard::Decision::Block(candidates) =
            guard::decide_create_for(name, Some(owner.slug()), &existing, override_token)
        {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }

        sqlx::query("INSERT INTO mailbox (name, owner) VALUES (?, ?)")
            .bind(name.as_str())
            .bind(owner.as_str())
            .execute(&self.pool)
            .await
            .map_err(store)?;
        Ok(Guarded::Written(Mailbox {
            name: name.clone(),
            owner: owner.clone(),
            counts: StateCounts::default(),
            quarantined: Vec::new(),
        }))
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let rows = sqlx::query("SELECT name, owner FROM mailbox ORDER BY name")
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?;
        let cards = self.cards(&mut tx).await?;
        tx.commit().await.map_err(store)?;

        Ok(rows
            .iter()
            .map(|row| {
                let name = MailboxName(row.get::<String, _>("name"));
                let mut counts = StateCounts::default();
                for message in cards
                    .iter()
                    .filter(|c| c.mailbox() == &name)
                    .filter_map(Card::readable)
                {
                    counts.add(message.state);
                }
                Mailbox {
                    quarantined: cards
                        .iter()
                        .filter(|c| c.mailbox() == &name && c.readable().is_none())
                        .map(|c| c.id().clone())
                        .collect(),
                    owner: EntityId(row.get::<String, _>("owner")),
                    name,
                    counts,
                }
            })
            .collect())
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        validate_mailbox_name(&message.mailbox)?;
        validate_sender(&message.sender)?;
        validate_body(&message.body)?;
        validate_subject(message.subject.as_deref())?;

        let names = self.names().await?;
        if let guard::Decision::Block(candidates) = guard::decide_existing(&message.mailbox, &names)
        {
            return Ok(Guarded::Blocked {
                attempted: message.mailbox,
                candidates,
            });
        }

        let mut tx = self.pool.begin().await.map_err(store)?;

        // Everything a write names must already exist — a reply link included.
        // A quarantined card counts as there: the row exists, and refusing the
        // link because jojobot cannot parse the message would be a different
        // claim from the one the caller is making.
        if let Some(answered) = &message.in_reply_to {
            validate_message_id(answered)?;
            let known: Option<String> = sqlx::query_scalar("SELECT id FROM message WHERE id = ?")
                .bind(answered.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(store)?;
            if known.is_none() {
                return Err(MailboxError::UnknownMessage {
                    attempted: answered.to_string(),
                });
            }
        }

        let id = MessageId(mint(&mut tx).await?);
        let ordinal: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(ordinal), 0) + 1 FROM message")
            .fetch_one(&mut *tx)
            .await
            .map_err(store)?;
        let stored = Message {
            id,
            mailbox: message.mailbox,
            body: normalize_body(&message.body),
            subject: normalize_subject(message.subject.as_deref()),
            sender: message.sender.trim().to_string(),
            sent_at: message.sent_at,
            state: MessageState::New,
            notes: None,
            in_reply_to: message.in_reply_to,
        };
        sqlx::query(
            "INSERT INTO message
               (id, mailbox, ordinal, body, subject, sender, sent_at, state, notes, in_reply_to)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
        )
        .bind(stored.id.as_str())
        .bind(stored.mailbox.as_str())
        .bind(ordinal)
        .bind(&stored.body)
        .bind(stored.subject.as_deref())
        .bind(&stored.sender)
        .bind(stored.sent_at.to_string())
        .bind(stored.state.as_token())
        .bind(stored.in_reply_to.as_ref().map(MessageId::as_str))
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(stored))
    }

    async fn read_mailbox(&self, name: &MailboxName) -> Result<Guarded<Delivery>, MailboxError> {
        validate_mailbox_name(name)?;
        let names = self.names().await?;
        if let guard::Decision::Block(candidates) = guard::decide_existing(name, &names) {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }

        let mut tx = self.pool.begin().await.map_err(store)?;
        let cards = self.cards(&mut tx).await?;
        let mut delivered = Vec::new();
        for message in cards
            .iter()
            .filter(|c| c.mailbox() == name)
            .filter_map(Card::readable)
            .filter(|m| m.state.is_unprocessed())
        {
            let seen_before = message.state == MessageState::Read;
            if !seen_before {
                sqlx::query("UPDATE message SET state = ? WHERE id = ?")
                    .bind(MessageState::Read.as_token())
                    .bind(message.id.as_str())
                    .execute(&mut *tx)
                    .await
                    .map_err(store)?;
            }
            delivered.push(Delivered {
                message: Message {
                    state: MessageState::Read,
                    ..message.clone()
                },
                seen_before,
            });
        }
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(Delivery {
            mailbox: name.clone(),
            messages: delivered,
        }))
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let cards = self.cards(&mut tx).await?;
        tx.commit().await.map_err(store)?;
        Ok(cards.iter().filter_map(Card::readable).cloned().collect())
    }

    async fn read_message(&self, id: &MessageId) -> Result<Delivered, MailboxError> {
        validate_message_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let cards = self.cards(&mut tx).await?;
        let card =
            cards
                .iter()
                .find(|c| c.id() == id)
                .ok_or_else(|| MailboxError::UnknownMessage {
                    attempted: id.to_string(),
                })?;
        let message = refuse(card)?;

        // Anything but `new` has been handed over or handled already — the one
        // state this verb advances is the one nobody has taken.
        let seen_before = message.state != MessageState::New;
        if !seen_before {
            sqlx::query("UPDATE message SET state = ? WHERE id = ?")
                .bind(MessageState::Read.as_token())
                .bind(id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(store)?;
        }
        let state = if seen_before {
            message.state
        } else {
            MessageState::Read
        };
        let message = Message {
            state,
            ..message.clone()
        };
        tx.commit().await.map_err(store)?;
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
        validate_message_id(id)?;
        validate_notes(notes)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let cards = self.cards(&mut tx).await?;
        let card =
            cards
                .iter()
                .find(|c| c.id() == id)
                .ok_or_else(|| MailboxError::UnknownMessage {
                    attempted: id.to_string(),
                })?;
        let message = refuse(card)?;

        let notes = normalize_notes(notes).or_else(|| message.notes.clone());
        sqlx::query("UPDATE message SET state = ?, notes = ? WHERE id = ?")
            .bind(MessageState::Processed.as_token())
            .bind(notes.as_deref())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        let message = Message {
            state: MessageState::Processed,
            notes,
            ..message.clone()
        };
        tx.commit().await.map_err(store)?;
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::{Scratch, free_port};
    use crate::dolt::{Dolt, migrate};
    use jojobot_domain::mailbox::NewMessage;

    /// Every well-formed owner resolves. The owner question has its own case in
    /// the shared contract; these are about quarantine, and a store that
    /// refused an owner here would be answering a question nobody asked.
    struct AnyOwner;

    #[async_trait]
    impl OwnerIndex for AnyOwner {
        async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
            Ok(OwnerLookup::Known)
        }
    }

    /// A live store with one box and one ordinary message in it, plus the
    /// process holding it up.
    async fn board(what: &str) -> (Dolt, DoltMailboxes, MessageId) {
        let scratch = Scratch::new(what);
        // The directory outlives this call only because the server holds it
        // open; the process is stopped by the caller and the path goes with the
        // temp dir. Leaked deliberately rather than dropped here, which would
        // remove the data under a running server.
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        let mail = DoltMailboxes::open(store.pool().clone(), Arc::new(AnyOwner));

        mail.create_mailbox(
            &MailboxName("inbox".into()),
            &EntityId("bot:gamma".into()),
            None,
        )
        .await
        .expect("create ok")
        .written()
        .expect("not blocked");
        let readable = mail
            .post_message(NewMessage {
                mailbox: MailboxName("inbox".into()),
                body: "the readable one".into(),
                subject: None,
                sender: "gamma".into(),
                sent_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                in_reply_to: None,
            })
            .await
            .expect("post ok")
            .written()
            .expect("not blocked")
            .id;
        (store, mail, readable)
    }

    /// Put a row on the board that jojobot cannot read as a message, the way a
    /// hand edit or a record from a schema nobody remembers would.
    async fn unreadable(store: &Dolt, id: &str, state: &str, sent_at: &str) {
        sqlx::query(
            "INSERT INTO message
               (id, mailbox, ordinal, body, subject, sender, sent_at, state, notes, in_reply_to)
             VALUES (?, 'inbox', 99, 'whatever this was', NULL, 'gamma', ?, ?, NULL, NULL)",
        )
        .bind(id)
        .bind(sent_at)
        .bind(state)
        .execute(store.pool())
        .await
        .expect("the board takes the row");
    }

    /// **An owner index that cannot be reached is a failure, never "no such
    /// owner".**
    ///
    /// The two are different claims and only one of them is about the caller's
    /// roster. Rendering unreachable as absent refuses a creation that should
    /// have succeeded and tells the caller something false about their own
    /// entities — and it does it with an empty candidate list, which reads as
    /// "nothing even resembles this" when in fact nothing was looked at.
    ///
    /// The port's own contract says so and nothing held it: the condition
    /// cannot arise through any verb, so no test that goes through the door
    /// could produce it.
    #[tokio::test]
    async fn an_owner_index_that_cannot_be_reached_is_a_failure_not_an_absence() {
        /// An index that is down, which is the one answer a real one can give
        /// that says nothing about who exists.
        struct Down;

        #[async_trait]
        impl OwnerIndex for Down {
            async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
                Err(MailboxError::Store("the entity world is down".into()))
            }
        }

        let scratch = Scratch::new("owner-index-down");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");

        let down = DoltMailboxes::open(store.pool().clone(), Arc::new(Down));
        let outcome = down
            .create_mailbox(
                &MailboxName("inbox".into()),
                &EntityId("bot:gamma".into()),
                None,
            )
            .await;
        assert!(
            matches!(outcome, Err(MailboxError::Store(_))),
            "an index that is down is a failure, not a verdict about the owner: {outcome:?}"
        );
        assert!(
            down.list_mailboxes().await.expect("list ok").is_empty(),
            "and nothing was written"
        );

        // **The positive it rests on.** The same call, the same owner, against
        // an index that answers — otherwise this passes on a store that
        // refuses every creation for any reason at all.
        let up = DoltMailboxes::open(store.pool().clone(), Arc::new(AnyOwner));
        let opened = up
            .create_mailbox(
                &MailboxName("inbox".into()),
                &EntityId("bot:gamma".into()),
                None,
            )
            .await
            .expect("create ok")
            .written()
            .expect("a reachable index opens the box");
        assert_eq!(opened.name.as_str(), "inbox");

        store.stop().await;
    }

    /// **A card jojobot cannot read is invisible to every verb that acts, and
    /// visible on the box that holds it.**
    ///
    /// Both halves in one case, because they are one rule: the reason it is
    /// safe for the acting verbs to skip it is that the box says it is there.
    /// A store that merely dropped it would report a board with nothing wrong.
    #[tokio::test]
    async fn an_unreadable_card_is_skipped_by_every_verb_and_named_by_its_box() {
        let (mut store, mail, readable) = board("quarantine").await;
        // Two ways a row stops being readable: a state that is no state, and a
        // stamp that is no time. Both, so neither channel can rot unnoticed.
        unreadable(&store, "q-state", "in-flight", "2026-01-01T00:00:01Z").await;
        unreadable(&store, "q-stamp", "new", "the day before yesterday").await;

        // The box counts what it can read and NAMES what it cannot.
        let listed = mail.list_mailboxes().await.expect("list ok");
        let inbox = listed.first().expect("the box is on the board");
        assert_eq!(
            inbox.counts.total(),
            1,
            "only the readable message is counted: {inbox:?}"
        );
        let mut named: Vec<&str> = inbox.quarantined.iter().map(MessageId::as_str).collect();
        named.sort();
        assert_eq!(
            named,
            vec!["q-stamp", "q-state"],
            "both unreadable cards are named, or nobody can tell they exist"
        );

        // A scan is what search is built from: an unreadable card is nothing
        // jojobot can index, so it is not there.
        let scanned = mail.scan_messages().await.expect("scan ok");
        assert_eq!(
            scanned.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            vec![readable.clone()],
            "a scan carries what can be read and nothing else"
        );

        // A delivery hands over what a consumer can act on. Handing over a card
        // nobody can parse would make it owed work that cannot be done.
        let jojobot_domain::mailbox::Guarded::Written(delivery) = mail
            .read_mailbox(&MailboxName("inbox".into()))
            .await
            .expect("read ok")
        else {
            panic!("the box exists, so the read is not blocked");
        };
        assert_eq!(
            delivery
                .messages
                .iter()
                .map(|d| d.message.id.clone())
                .collect::<Vec<_>>(),
            vec![readable],
            "the delivery carries the readable message alone"
        );

        // Addressed by id, each verb says WHICH refusal this is. "No such
        // message" would be a lie — the row is right there — and it would send
        // a caller re-posting instead of asking for a repair.
        for id in ["q-state", "q-stamp"] {
            let id = MessageId(id.to_string());
            assert!(
                matches!(
                    mail.read_message(&id).await,
                    Err(MailboxError::Quarantined { .. })
                ),
                "reading {id} must say it cannot be read, not that it is missing"
            );
            assert!(
                matches!(
                    mail.mark_processed(&id, Some("tried")).await,
                    Err(MailboxError::Quarantined { .. })
                ),
                "processing {id} must say it cannot be read, not that it is missing"
            );
        }

        // The positive every refusal above rests on: an id that names nothing
        // is a MISS, not a quarantine. Without this the assertions pass on a
        // store that answers Quarantined for every id it is handed.
        assert!(
            matches!(
                mail.read_message(&MessageId("nobody".into())).await,
                Err(MailboxError::UnknownMessage { .. })
            ),
            "an id that names nothing is a miss"
        );

        store.stop().await;
    }
}

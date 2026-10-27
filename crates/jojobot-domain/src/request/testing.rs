//! An in-memory [`Requests`], standing for the one table a real store keeps.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{PartRow, Recorded, Request, RequestError, RequestId, Requests, Reservation, Stored};
use crate::memory::EntityId;

/// A fake ledger, keyed by the bot's handle. A real store keys on the bot's
/// permanent identity, and the real-store suite holds the rename case; a fake
/// has no second name for a bot to be renamed to.
#[derive(Debug, Default)]
pub struct InMemoryRequests {
    rows: Mutex<HashMap<(String, String), Stored>>,
    /// The parts of writes that commit in several, in the order written.
    parts: Mutex<Vec<(String, String, PartRow)>>,
    /// Parts whose next commit fails, as a store that dies between two parts.
    refused: Mutex<std::collections::HashSet<String>>,
    /// Parts whose every commit is refused by a rule, with the words of the
    /// refusal: a store that answers and says no, as against one that is down.
    refusing: Mutex<HashMap<String, String>>,
    /// The words of the refusal the last failed commit met, for the store that
    /// asked to turn the failure into its own error.
    last_refusal: Mutex<Option<String>>,
}

impl InMemoryRequests {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// **What a store's commit does with the claim it took**: spend the id with
    /// what the write returned, or find it already spent, which fails the commit
    /// as the real store's duplicate key does and hands the claim back. Returns
    /// whether this write spent it.
    ///
    /// A fake store calls this under its own lock, in the same step as the write,
    /// so the check and the write cannot be separated.
    pub fn spend(&self, reservation: Reservation, landed: impl Into<String>) -> bool {
        let request = reservation.request().clone();
        let key = (request.bot.0.clone(), request.id.as_str().to_string());
        if let Some(part) = reservation.part().map(str::to_string) {
            let landed = landed.into();
            if self
                .refused
                .lock()
                .expect("the ledger is poisoned")
                .remove(&part)
            {
                reservation.failed();
                return false;
            }
            if let Some(words) = self
                .refusing
                .lock()
                .expect("the ledger is poisoned")
                .get(&part)
                .cloned()
            {
                *self.last_refusal.lock().expect("the ledger is poisoned") = Some(words);
                reservation.failed();
                return false;
            }
            let mut parts = self.parts.lock().expect("the ledger is poisoned");
            if parts
                .iter()
                .any(|(bot, id, row)| *bot == key.0 && *id == key.1 && row.part == part)
            {
                drop(parts);
                reservation.failed();
                return false;
            }
            parts.push((
                key.0,
                key.1,
                PartRow {
                    part,
                    verb: request.verb,
                    digest: request.digest,
                    recorded: Recorded {
                        landed: landed.clone(),
                        note: reservation.note().map(str::to_string),
                    },
                },
            ));
            drop(parts);
            reservation.spent_with(landed);
            return true;
        }
        let mut rows = self.rows.lock().expect("the ledger is poisoned");
        if rows.contains_key(&key) {
            drop(rows);
            reservation.failed();
            return false;
        }
        rows.insert(
            key,
            Stored {
                verb: request.verb,
                digest: request.digest,
                landed: landed.into(),
                receipt: None,
            },
        );
        drop(rows);
        reservation.spent();
        true
    }

    /// **The next commit of the part called `part` fails once and stores nothing**,
    /// as a store that stops between two parts of one write. The commit before it
    /// stands, and the same call sent again finds the failed part open.
    pub fn fail_the_next_commit_of(&self, part: &str) {
        self.refused
            .lock()
            .expect("the ledger is poisoned")
            .insert(part.to_string());
    }

    /// **Every commit of the part called `part` is refused by a rule**, with
    /// `words`, and stores nothing. The same call sent again meets the refusal
    /// again, as it would a constraint the call itself breaks.
    pub fn refuse_every_commit_of(&self, part: &str, words: &str) {
        self.refusing
            .lock()
            .expect("the ledger is poisoned")
            .insert(part.to_string(), words.to_string());
    }

    /// **The words of the refusal the last failed commit met**, taken: a store
    /// whose `spend` came back false asks, and answers its own refusal when there
    /// are words, and its conflict when there are none.
    pub fn take_refusal(&self) -> Option<String> {
        self.last_refusal
            .lock()
            .expect("the ledger is poisoned")
            .take()
    }

    /// How many ids the ledger holds.
    pub fn len(&self) -> usize {
        self.rows.lock().expect("the ledger is poisoned").len()
    }

    /// How many parts of writes that commit in several the ledger holds.
    pub fn parts_held(&self) -> usize {
        self.parts.lock().expect("the ledger is poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait::async_trait]
impl Requests for InMemoryRequests {
    async fn parts(&self, bot: &EntityId, id: &RequestId) -> Result<Vec<PartRow>, RequestError> {
        Ok(self
            .parts
            .lock()
            .expect("the ledger is poisoned")
            .iter()
            .filter(|(held_bot, held_id, _)| *held_bot == bot.0 && held_id == id.as_str())
            .map(|(_, _, row)| row.clone())
            .collect())
    }

    async fn finish(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        let names = self
            .parts(&request.bot, &request.id)
            .await?
            .iter()
            .map(|row| format!("{:?}", row.part))
            .collect::<Vec<_>>()
            .join(",");
        let landed = format!("{{\"parts\":[{names}]}}");
        self.rows
            .lock()
            .expect("the ledger is poisoned")
            .entry((request.bot.0.clone(), request.id.as_str().to_string()))
            .or_insert_with(|| Stored {
                verb: request.verb.clone(),
                digest: request.digest.clone(),
                landed,
                receipt: None,
            })
            .receipt = Some(receipt.to_string());
        Ok(())
    }

    async fn lookup(&self, bot: &EntityId, id: &RequestId) -> Result<Option<Stored>, RequestError> {
        Ok(self
            .rows
            .lock()
            .expect("the ledger is poisoned")
            .get(&(bot.0.clone(), id.as_str().to_string()))
            .cloned())
    }

    async fn fill(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        if let Some(row) = self
            .rows
            .lock()
            .expect("the ledger is poisoned")
            .get_mut(&(request.bot.0.clone(), request.id.as_str().to_string()))
        {
            row.receipt = Some(receipt.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{Scope, armed, armed_part, digest_of, take, within};

    fn request(bot: &str, id: &str) -> Request {
        Request {
            bot: EntityId(bot.into()),
            id: RequestId::parse(id).expect("a good id"),
            verb: "capture".into(),
            digest: digest_of("{}"),
        }
    }

    /// One call: open a scope, let a commit take the claim and spend it.
    async fn write(ledger: &InMemoryRequests, request: Request) -> bool {
        let scope = Scope::new(request);
        within(
            Arc::clone(&scope),
            armed(async {
                ledger.spend(
                    take().expect("the claim"),
                    "{\"address\":\"person:alpha#f1\"}",
                )
            }),
        )
        .await
    }

    #[tokio::test]
    async fn an_id_is_spent_once_and_the_second_spender_is_told() {
        let ledger = InMemoryRequests::new();
        let me = request("bot:milhouse", "r1");
        // The positive the negative rests on: the first call spends it.
        assert!(write(&ledger, me.clone()).await);
        assert_eq!(ledger.len(), 1);
        assert!(!write(&ledger, me.clone()).await);
        assert_eq!(ledger.len(), 1, "the lost spender stored nothing");
    }

    #[tokio::test]
    async fn the_same_id_from_another_bot_is_another_row() {
        let ledger = InMemoryRequests::new();
        assert!(write(&ledger, request("bot:milhouse", "r1")).await);
        assert!(write(&ledger, request("bot:nelson", "r1")).await);
        assert_eq!(ledger.len(), 2);
    }

    #[tokio::test]
    async fn a_lookup_finds_what_was_spent_and_a_fill_adds_the_receipt() {
        let ledger = InMemoryRequests::new();
        let me = request("bot:milhouse", "r1");
        assert!(
            ledger.lookup(&me.bot, &me.id).await.unwrap().is_none(),
            "nothing is spent yet"
        );
        write(&ledger, me.clone()).await;
        let held = ledger
            .lookup(&me.bot, &me.id)
            .await
            .unwrap()
            .expect("a row");
        assert_eq!(held.verb, "capture");
        assert_eq!(held.landed, "{\"address\":\"person:alpha#f1\"}");
        assert_eq!(held.receipt, None, "the receipt comes after the commit");
        ledger.fill(&me, "{\"ok\":true}").await.unwrap();
        let filled = ledger
            .lookup(&me.bot, &me.id)
            .await
            .unwrap()
            .expect("a row");
        assert_eq!(filled.receipt.as_deref(), Some("{\"ok\":true}"));
    }

    /// One call writing one named part, the way a verb that commits in several
    /// writes it.
    async fn write_part(ledger: &InMemoryRequests, request: Request, name: &str) -> bool {
        let scope = Scope::new(request);
        within(
            Arc::clone(&scope),
            armed_part(name, Some(format!("plan of {name}")), async {
                ledger.spend(
                    take().expect("the claim"),
                    format!("{{\"part\":\"{name}\"}}"),
                )
            }),
        )
        .await
    }

    #[tokio::test]
    async fn a_part_is_written_once_and_the_ledger_hands_the_parts_back_in_order() {
        let ledger = InMemoryRequests::new();
        let me = request("bot:milhouse", "r1");
        assert!(write_part(&ledger, me.clone(), "one").await);
        assert!(write_part(&ledger, me.clone(), "two").await);
        assert!(
            !write_part(&ledger, me.clone(), "one").await,
            "a part another call wrote stores nothing"
        );
        let held = ledger.parts(&me.bot, &me.id).await.unwrap();
        assert_eq!(
            held.iter().map(|row| row.part.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(held[0].recorded.landed, "{\"part\":\"one\"}");
        assert_eq!(held[0].recorded.note.as_deref(), Some("plan of one"));
        assert_eq!(held[0].digest, me.digest);
        // The parts of another id, and of another bot, are not these.
        assert!(
            ledger
                .parts(&me.bot, &RequestId::parse("r2").unwrap())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            ledger
                .parts(&EntityId("bot:nelson".into()), &me.id)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            ledger.lookup(&me.bot, &me.id).await.unwrap().is_none(),
            "parts spend nothing of the whole write until it is finished"
        );
    }

    #[tokio::test]
    async fn finishing_spends_the_id_with_the_receipt_and_a_repeat_finds_it() {
        let ledger = InMemoryRequests::new();
        let me = request("bot:milhouse", "r1");
        write_part(&ledger, me.clone(), "one").await;
        ledger.finish(&me, "{\"ok\":true}").await.unwrap();
        let held = ledger
            .lookup(&me.bot, &me.id)
            .await
            .unwrap()
            .expect("spent");
        assert_eq!(held.receipt.as_deref(), Some("{\"ok\":true}"));
        assert_eq!(held.verb, "capture");
        assert!(held.landed.contains("one"), "{}", held.landed);
        // Finishing twice keeps one row and the newest receipt.
        ledger.finish(&me, "{\"ok\":2}").await.unwrap();
        assert_eq!(ledger.len(), 1);
    }
}

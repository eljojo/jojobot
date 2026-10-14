//! An in-memory [`Requests`], standing for the one table a real store keeps.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{Request, RequestError, RequestId, Requests, Reservation, Stored};
use crate::memory::EntityId;

/// A fake ledger, keyed by the bot's handle. A real store keys on the bot's
/// permanent identity, and the real-store suite holds the rename case; a fake
/// has no second name for a bot to be renamed to.
#[derive(Debug, Default)]
pub struct InMemoryRequests {
    rows: Mutex<HashMap<(String, String), Stored>>,
}

impl InMemoryRequests {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// **What a store's commit does with the claim it took**: spend the id with
    /// what the write returned, or find it already spent. Returns whether this
    /// write spent it. The scope behind the claim is told which.
    ///
    /// A fake store calls this under its own lock, in the same step as the write,
    /// so the check and the write cannot be separated.
    pub fn spend(&self, reservation: Reservation, landed: impl Into<String>) -> bool {
        let request = reservation.request().clone();
        let key = (request.bot.0.clone(), request.id.as_str().to_string());
        let mut rows = self.rows.lock().expect("the ledger is poisoned");
        if rows.contains_key(&key) {
            drop(rows);
            reservation.lost();
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

    /// How many ids the ledger holds.
    pub fn len(&self) -> usize {
        self.rows.lock().expect("the ledger is poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait::async_trait]
impl Requests for InMemoryRequests {
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
    use crate::request::{Scope, armed, digest_of, take, within};

    fn request(bot: &str, id: &str) -> Request {
        Request {
            bot: EntityId(bot.into()),
            id: RequestId::parse(id).expect("a good id"),
            verb: "capture".into(),
            digest: digest_of("{}"),
        }
    }

    /// One call: open a scope, let a commit take the claim and spend it.
    async fn write(ledger: &InMemoryRequests, request: Request) -> (bool, bool) {
        let scope = Scope::new(request);
        let spent = within(
            Arc::clone(&scope),
            armed(async {
                ledger.spend(
                    take().expect("the claim"),
                    "{\"address\":\"person:alpha#f1\"}",
                )
            }),
        )
        .await;
        (spent, scope.was_lost())
    }

    #[tokio::test]
    async fn an_id_is_spent_once_and_the_second_spender_is_told() {
        let ledger = InMemoryRequests::new();
        let me = request("bot:milhouse", "r1");
        // The positive the negative rests on: the first call spends it.
        assert_eq!(write(&ledger, me.clone()).await, (true, false));
        assert_eq!(ledger.len(), 1);
        assert_eq!(write(&ledger, me.clone()).await, (false, true));
        assert_eq!(ledger.len(), 1, "the lost spender stored nothing");
    }

    #[tokio::test]
    async fn the_same_id_from_another_bot_is_another_row() {
        let ledger = InMemoryRequests::new();
        assert!(write(&ledger, request("bot:milhouse", "r1")).await.0);
        assert!(write(&ledger, request("bot:nelson", "r1")).await.0);
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
}

//! An in-memory [`Teachings`], and the contract every backend must satisfy.

use std::collections::HashSet;
use std::sync::Mutex;

use jiff::Timestamp;

use super::{TeachingError, Teachings};
use crate::session::Sid;

/// A fake store, backed by a set rather than a table. Existence in the set is
/// exactly what a row's existence is in the real store — there is no second
/// mechanism here to keep honest.
#[derive(Debug, Default)]
pub struct InMemoryTeachings {
    contacted: Mutex<HashSet<(String, String)>>,
}

impl InMemoryTeachings {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Teachings for InMemoryTeachings {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        _at: Timestamp,
    ) -> Result<bool, TeachingError> {
        let mut contacted = self.contacted.lock().expect("the ledger is poisoned");
        Ok(contacted.insert((sid.as_str().to_string(), domain.to_string())))
    }
}

/// The contract — run against the fake here and against the real store in
/// `jojobot-adapters`, so the two answer alike.
pub mod contract {
    use super::*;

    fn sid(s: &str) -> Sid {
        Sid(s.to_string())
    }

    /// A fixed instant, so the contract never reads a clock.
    fn at() -> Timestamp {
        Timestamp::from_second(1_780_000_000).expect("a valid fixed instant")
    }

    /// The first call for a `(sid, domain)` pair is first contact.
    async fn first_touch_is_first_contact(store: &dyn Teachings) {
        assert!(
            store
                .first_contact(&sid("kwyj"), "claims", at())
                .await
                .expect("the store answers"),
            "nothing has touched this pair yet"
        );
    }

    /// The same session touching the same domain again is not first contact.
    async fn a_second_touch_is_not_first_contact(store: &dyn Teachings) {
        let who = sid("boba");
        assert!(
            store
                .first_contact(&who, "claims", at())
                .await
                .expect("the store answers"),
            "the first call records it"
        );
        assert!(
            !store
                .first_contact(&who, "claims", at())
                .await
                .expect("the store answers"),
            "the second call finds the row the first one left"
        );
    }

    /// A second domain does not inherit the first's contact — the mechanism
    /// carries no notion of which domains exist, only which pairs have been
    /// recorded.
    async fn a_second_domain_is_untouched_by_the_first(store: &dyn Teachings) {
        let who = sid("flnj");
        store
            .first_contact(&who, "claims", at())
            .await
            .expect("the store answers");
        assert!(
            store
                .first_contact(&who, "mailbox", at())
                .await
                .expect("the store answers"),
            "a different domain, same session, is still a first contact"
        );
    }

    /// A second session is untouched by the first's contact — the row is
    /// keyed on both, not on the domain alone.
    async fn a_second_session_is_untouched_by_the_first(store: &dyn Teachings) {
        store
            .first_contact(&sid("gzod"), "claims", at())
            .await
            .expect("the store answers");
        assert!(
            store
                .first_contact(&sid("vhrm"), "claims", at())
                .await
                .expect("the store answers"),
            "a different session, same domain, is still a first contact"
        );
    }

    /// Run every case, each against a store of its own.
    pub async fn run_all<S, F, Fut>(fresh: F)
    where
        S: Teachings,
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = S>,
    {
        first_touch_is_first_contact(&fresh().await).await;
        a_second_touch_is_not_first_contact(&fresh().await).await;
        a_second_domain_is_untouched_by_the_first(&fresh().await).await;
        a_second_session_is_untouched_by_the_first(&fresh().await).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_fake_satisfies_the_contract() {
        contract::run_all(|| async { InMemoryTeachings::new() }).await;
    }
}

//! [`FoldedTeachings`] — the whole teaching ledger, held in RAM above the store
//! (decision log 292).
//!
//! jojobot is one server instance and the only writer, so it holds the ledger
//! in memory. [`FoldedTeachings`] wraps a [`Teachings`] and, once
//! [`rebuild`](FoldedTeachings::rebuild) has run, answers "has this session been
//! told this" from the held set. A first contact is a write and goes to the
//! store; a contact the set already holds changes nothing, so it never does.
//!
//! Before the first `rebuild`, and when the store cannot list its ledger, no set
//! is held and every call goes to the store underneath: slower and still correct.
//!
//! ⚠️ **This is only correct with exactly one writer in the process**, for the
//! reason [`super::Folded`] gives.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::RwLock;

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::{TeachingError, Teachings};

/// A [`Teachings`] wrapped in a RAM copy of the ledger.
pub struct FoldedTeachings {
    inner: Arc<dyn Teachings>,
    /// Every `(sid, domain)` pair the store holds; `None` sends every call to
    /// the store.
    taught: RwLock<Option<BTreeSet<(Sid, String)>>>,
    /// **Held across a write and the set's following it.**
    writing: tokio::sync::Mutex<()>,
}

impl FoldedTeachings {
    /// Wrap a store with **no set held**. Every call answers correctly before a
    /// rebuild: it goes to the store.
    pub fn new(inner: Arc<dyn Teachings>) -> Self {
        FoldedTeachings {
            inner,
            taught: RwLock::new(None),
            writing: tokio::sync::Mutex::new(()),
        }
    }

    /// Fill the set from a full read of the store — the boot path. Returns how
    /// many pairs were held.
    pub async fn rebuild(&self) -> Result<usize, TeachingError> {
        let _gate = self.writing.lock().await;
        let all = self.inner.taught().await?;
        let held = all.as_ref().map_or(0, Vec::len);
        *self.taught.write().expect("fold lock") = all.map(|all| all.into_iter().collect());
        Ok(held)
    }
}

#[async_trait]
impl Teachings for FoldedTeachings {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        self.inner.first_contact(sid, domain, at).await
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        let _gate = self.writing.lock().await;
        self.inner.forget(sid).await?;
        if let Some(taught) = self.taught.write().expect("fold lock").as_mut() {
            taught.retain(|(held, _)| held != sid);
        }
        Ok(())
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        self.inner.taught().await
    }
}

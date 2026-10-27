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
//! A `rebuild` that failed is tried again by the next write.
//!
//! ⚠️ **This is only correct with exactly one writer in the process**, for the
//! reason [`super::Folded`] gives.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::{TeachingError, Teachings};

use crate::writer::{Writer, owned};

type Ledger = BTreeSet<(Sid, String)>;

/// A [`Teachings`] wrapped in a RAM copy of the ledger.
#[derive(Clone)]
pub struct FoldedTeachings {
    writer: Writer,
    inner: Arc<dyn Teachings>,
    /// Every `(sid, domain)` pair the store holds; `None` sends every call to
    /// the store.
    taught: Arc<RwLock<Option<Ledger>>>,
    /// Set by the first `rebuild`, whether or not its read succeeded. A read that
    /// failed leaves no set, and this is what tells the next write to read the
    /// ledger again.
    wanted: Arc<AtomicBool>,
}

impl FoldedTeachings {
    /// Wrap a store with **no set held**. Every call answers correctly before a
    /// rebuild: it goes to the store.
    pub fn new(inner: Arc<dyn Teachings>) -> Self {
        FoldedTeachings {
            writer: Writer::default(),
            inner,
            taught: Arc::new(RwLock::new(None)),
            wanted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Share the serving process's writer with the other rails.
    pub fn with_writer(mut self, writer: Writer) -> Self {
        self.writer = writer;
        self
    }

    /// Wait for publication after a fixture cancels the caller's wait.
    #[cfg(any(test, feature = "testing"))]
    pub async fn wait_for_writes(&self) {
        self.writer.drain().await;
    }

    /// Fill the set from a full read of the store — the boot path. Returns how
    /// many pairs were held.
    pub async fn rebuild(&self) -> Result<usize, TeachingError> {
        let this = self.clone();
        self.writer
            .run(async move {
                if this.writer.rail_uncertain("teachings") {
                    return Err(TeachingError::Store(
                        "the rail is uncertain; rebuild requires its fence".into(),
                    ));
                }
                this.wanted.store(true, Ordering::SeqCst);
                this.fill().await
            })
            .await
    }

    /// The read behind [`rebuild`](Self::rebuild), already in the writer task.
    /// A read that fails leaves no set rather than a stale one.
    async fn fill(&self) -> Result<usize, TeachingError> {
        match crate::writer::sql::reading(self.inner.taught()).await {
            Ok(all) => {
                let held = all.as_ref().map_or(0, Vec::len);
                *self.taught.write().expect("fold lock") = all.map(|all| all.into_iter().collect());
                Ok(held)
            }
            Err(e) => {
                if !self.writer.rail_uncertain("teachings") {
                    *self.taught.write().expect("fold lock") = None;
                }
                Err(e)
            }
        }
    }

    /// Keep the last published image until an uncertain write's rail has been
    /// fenced and reloaded through its boot loader.
    async fn scoped<T>(
        &self,
        write: impl std::future::Future<Output = Result<T, TeachingError>>,
    ) -> Result<T, TeachingError> {
        crate::writer::recovery::guarded(&self.writer, "teachings", write, self.fill()).await
    }

    /// Bring the set up to date with a write that landed. With no set held, a
    /// ledger that was asked for is read again, so the set carries the write.
    async fn follow(&self, verb: &str, apply: impl FnOnce(&mut BTreeSet<(Sid, String)>)) {
        {
            let mut slot = self.taught.write().expect("fold lock");
            if let Some(taught) = slot.as_mut() {
                apply(taught);
                return;
            }
        }
        if !self.wanted.load(Ordering::SeqCst) {
            return;
        }
        if let Err(error) = self.fill().await {
            tracing::warn!(
                verb,
                %error,
                "the in-memory teaching ledger could not be filled again, so calls go to the \
                 store until the next write fills it"
            );
        }
    }
}

impl FoldedTeachings {
    /// Whether the ledger holds this pair; `None` when no ledger is held.
    fn holds(&self, pair: &(Sid, String)) -> Option<bool> {
        self.taught
            .read()
            .expect("fold lock")
            .as_ref()
            .map(|taught| taught.contains(pair))
    }
    fn readable(&self) -> Result<(), TeachingError> {
        if self.writer.rail_uncertain("teachings")
            && self.taught.read().expect("fold lock").is_none()
        {
            return Err(TeachingError::Store(
                "the uncertain rail has no published image".into(),
            ));
        }
        Ok(())
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
        if self.writer.rail_uncertain("teachings") {
            return Err(TeachingError::Store(
                "the teaching rail is uncertain; this contact was not written".into(),
            ));
        }
        let pair = (sid.clone(), domain.to_string());
        // A pair the ledger holds changes nothing, so it never reaches the store.
        if self.holds(&pair) == Some(true) {
            return Ok(false);
        }
        owned!(
            self,
            this,
            [sid = sid.clone(), domain = domain.to_string()],
            this.first_contact(&sid, &domain, at).await
        );
        self.scoped(async {
            if self.holds(&pair) == Some(true) {
                return Ok(false);
            }
            let first = self.inner.first_contact(sid, domain, at).await?;
            self.follow("first_contact", |taught| {
                taught.insert(pair);
            })
            .await;
            Ok(first)
        })
        .await
    }

    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        self.readable()?;
        // Answered from the held set when there is one; a set that is not held
        // sends the ask to the store, which is slower and still correct.
        match self.holds(&(sid.clone(), domain.to_string())) {
            Some(held) => Ok(held),
            None => self.inner.has_contact(sid, domain).await,
        }
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        owned!(self, this, [sid = sid.clone()], this.forget(&sid).await);
        self.scoped(async {
            self.inner.forget(sid).await?;
            self.follow("forget", |taught| taught.retain(|(held, _)| held != sid))
                .await;
            Ok(())
        })
        .await
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        self.readable()?;
        if let Some(taught) = self.taught.read().expect("fold lock").as_ref() {
            return Ok(Some(taught.iter().cloned().collect()));
        }
        self.inner.taught().await
    }
}

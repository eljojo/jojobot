//! **A store that misbehaves on cue**, for the cases that need a read to lag, a
//! listing to fail or a landed write to be reported behind.
//!
//! Everything is the in-memory store's own behaviour except the calls a
//! [`Script`] names. A case sets the script, runs the verb, and reads what the
//! verb did. Test only.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jiff::civil::Date;
use jojobot_domain::memory::search::Behind;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, EntityKind, Landed, MemoryError};

/// **A call that waits until a case lets it go**, so a case decides where two
/// callers meet instead of hoping a scheduler puts them there.
///
/// The first caller to arrive parks until [`Hold::release`]; every later caller
/// goes straight through. A case waits on [`Hold::parked`] to know the first one
/// is in the gap, starts the second, and releases the first when the second has
/// either got past or is stuck behind a lock.
pub(crate) struct Hold {
    taken: std::sync::atomic::AtomicBool,
    arrived: tokio::sync::Semaphore,
    freed: tokio::sync::Semaphore,
}

impl Hold {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Hold {
            taken: std::sync::atomic::AtomicBool::new(false),
            arrived: tokio::sync::Semaphore::new(0),
            freed: tokio::sync::Semaphore::new(0),
        })
    }

    /// Returns once the first caller is parked.
    pub(crate) async fn parked(&self) {
        self.arrived
            .acquire()
            .await
            .expect("the hold is never closed")
            .forget();
    }

    /// Lets the parked caller go.
    pub(crate) fn release(&self) {
        self.freed.add_permits(1);
    }

    async fn pass(&self) {
        if !self.taken.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.arrived.add_permits(1);
            self.freed
                .acquire()
                .await
                .expect("the hold is never closed")
                .forget();
        }
    }
}

/// What goes wrong, and when. Every switch is off by default.
#[derive(Default)]
pub(crate) struct Script {
    /// After an `update_fact` lands, `fields` keeps answering with what the thing
    /// held before it: the fold has not caught up.
    pub(crate) fold_lags_after_update: bool,
    /// An `update_fact` that lands is reported as `FoldBehind`.
    pub(crate) update_reports_fold_behind: bool,
    /// An `update_fact` fails and writes nothing.
    pub(crate) update_fails: bool,
    /// An `update_fact` on a claim of this thing fails and writes nothing; every
    /// other update lands.
    pub(crate) update_fails_for: Option<EntityId>,
    /// `list_entities` of this kind fails.
    pub(crate) listing_fails_for: Option<EntityKind>,
    /// The `scan_entity` call with this number, counting from 1 once it is set,
    /// fails, and no other does. Every write lands: a case picks the read
    /// before a write, behind a `changed` block, and the read after it works.
    pub(crate) scan_entity_fails_on: Option<usize>,
    /// `recall` of this entity fails.
    pub(crate) recall_fails_for: Option<EntityId>,
    /// An `add_entity_with_first_claim` that lands is reported as `FoldBehind`.
    pub(crate) creation_reports_fold_behind: bool,
    /// The first `list_entities` of this kind parks until the hold is released.
    pub(crate) hold_listing_of: Option<(EntityKind, Arc<Hold>)>,
}

pub(crate) struct Scripted {
    pub(crate) inner: Arc<InMemoryMemory>,
    pub(crate) script: Mutex<Script>,
    /// What each thing held before the last update that landed, served in place
    /// of the fold while [`Script::fold_lags_after_update`] is set.
    lagging: Mutex<BTreeMap<EntityId, BTreeMap<String, String>>>,
}

impl Scripted {
    pub(crate) fn over(inner: Arc<InMemoryMemory>) -> Self {
        Scripted {
            inner,
            script: Mutex::new(Script::default()),
            lagging: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn script(&self, set: impl FnOnce(&mut Script)) {
        set(&mut self.script.lock().expect("the script is not poisoned"));
    }

    fn behind(landed: Landed) -> MemoryError {
        MemoryError::FoldBehind {
            landed,
            behind: Behind::Stale,
            source: Box::new(MemoryError::Store("the fold's re-read failed".into())),
        }
    }
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for Scripted {
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        let landed = self.inner.add_entity_with_first_claim(new, first).await?;
        let behind = self.script.lock().unwrap().creation_reports_fold_behind;
        match landed {
            Guarded::Written(created) if behind => {
                Err(Self::behind(Landed::Creation(Box::new(created))))
            }
            other => Ok(other),
        }
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        if kind.is_some() && self.script.lock().unwrap().listing_fails_for == kind {
            return Err(MemoryError::Store("the listing cannot be read".into()));
        }
        // Taken out of the script before the wait, so a parked caller does not
        // hold the script against the case that wants to release it.
        let hold = self
            .script
            .lock()
            .unwrap()
            .hold_listing_of
            .as_ref()
            .filter(|(held, _)| Some(*held) == kind)
            .map(|(_, hold)| hold.clone());
        if let Some(hold) = hold {
            hold.pass().await;
        }
        self.inner.list_entities(kind).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        if self.script.lock().unwrap().recall_fails_for.as_ref() == Some(subject) {
            return Err(MemoryError::Store("the record cannot be read".into()));
        }
        self.inner.recall(subject).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let (lags, behind, fails) = {
            let script = self.script.lock().unwrap();
            (
                script.fold_lags_after_update,
                script.update_reports_fold_behind,
                script.update_fails || script.update_fails_for.as_ref() == Some(&address.home),
            )
        };
        if fails {
            return Err(MemoryError::Store("the write cannot be made".into()));
        }
        let before = if lags {
            self.inner.fields(&address.home).await.ok()
        } else {
            None
        };
        let written = self.inner.update_fact(address, patch, caller).await?;
        if let (Some(before), Guarded::Written(_)) = (before, &written) {
            self.lagging
                .lock()
                .unwrap()
                .insert(address.home.clone(), before);
        }
        match written {
            Guarded::Written(fact) if behind => Err(Self::behind(Landed::Fact(Box::new(fact)))),
            other => Ok(other),
        }
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        if let Some(stale) = self.lagging.lock().unwrap().get(entity) {
            return Ok(stale.clone());
        }
        self.inner.fields(entity).await
    }
    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Option<jojobot_domain::memory::search::DocScan>, MemoryError> {
        let failing = {
            let mut script = self.script.lock().unwrap();
            let due = script.scan_entity_fails_on == Some(1);
            script.scan_entity_fails_on = script
                .scan_entity_fails_on
                .and_then(|n| n.checked_sub(1))
                .filter(|n| *n > 0);
            due
        };
        if failing {
            return Err(MemoryError::Store("the thing cannot be scanned".into()));
        }
        self.inner.scan_entity(entity).await
    }
}

/// **A run store that cannot be read**, with every write delegated. It is the
/// state a boot meets when the session world is down: `sessions_of` fails, and
/// the boot says no handle was started.
pub(crate) struct UnreachableRuns(pub(crate) jojobot_domain::session::testing::InMemorySessions);

#[async_trait]
impl jojobot_domain::session::Sessions for UnreachableRuns {
    async fn sessions_of(
        &self,
        _: &EntityId,
    ) -> Result<Vec<jojobot_domain::session::Session>, jojobot_domain::session::SessionError> {
        Err(jojobot_domain::session::SessionError::Store(
            "the runs cannot be read".into(),
        ))
    }
    async fn all_sessions(
        &self,
    ) -> Result<Vec<jojobot_domain::session::Session>, jojobot_domain::session::SessionError> {
        self.0.all_sessions().await
    }
    async fn read_session(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.read_session(id).await
    }
    async fn begin(
        &self,
        new: jojobot_domain::session::NewSession,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.begin(new).await
    }
    async fn set_key_hash(
        &self,
        id: &jojobot_domain::session::SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.0.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.0.prune_minted(before).await
    }

    async fn append(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: jojobot_domain::session::NewEntry,
    ) -> Result<jojobot_domain::session::JournalEntry, jojobot_domain::session::SessionError> {
        self.0.append(id, entry).await
    }
    async fn amend_last(
        &self,
        id: &jojobot_domain::session::SessionId,
        text: &str,
    ) -> Result<jojobot_domain::session::JournalEntry, jojobot_domain::session::SessionError> {
        self.0.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
        text: &str,
        touched: jiff::Timestamp,
    ) -> Result<jojobot_domain::session::JournalEntry, jojobot_domain::session::SessionError> {
        self.0.amend_beat(id, entry, text, touched).await
    }
    async fn set_focus(
        &self,
        id: &jojobot_domain::session::SessionId,
        focus: &str,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &jojobot_domain::session::SessionId,
        zone: Option<&str>,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.set_timezone(id, zone).await
    }
    async fn set_stated_day(
        &self,
        id: &jojobot_domain::session::SessionId,
        day: Option<Date>,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.set_stated_day(id, day).await
    }
    async fn set_wrap_window(
        &self,
        id: &jojobot_domain::session::SessionId,
        window: Option<jojobot_domain::session::WrapWindow>,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.set_wrap_window(id, window).await
    }
    async fn close(
        &self,
        id: &jojobot_domain::session::SessionId,
        to: jojobot_domain::session::SessionState,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.close(id, to).await
    }
    async fn close_displaced(
        &self,
        id: &jojobot_domain::session::SessionId,
        by: &jojobot_domain::session::Sid,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.close_displaced(id, by).await
    }
    async fn add_served(
        &self,
        id: &jojobot_domain::session::SessionId,
        chars: u64,
        day: jiff::civil::Date,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.add_served(id, chars, day).await
    }
    async fn reset_served(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.reset_served(id).await
    }
    async fn reopen(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        self.0.reopen(id).await
    }
    async fn set_charter_told(
        &self,
        id: &jojobot_domain::session::SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.0.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.0.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.0.journal_history(id, entry).await
    }
    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: jojobot_domain::session::CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.note_charter_change(bot, change, by_session).await
    }
    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<jojobot_domain::session::CharterChange>, jojobot_domain::session::SessionError>
    {
        self.0.charter_change_of(bot).await
    }
}

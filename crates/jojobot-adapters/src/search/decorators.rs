use super::*;

use crate::writer::owned;

type CachedMemoryScan = (WriteSummary, Vec<DocScan>);

/// The memory half of the search projection.
///
/// It reads the changes the writer publishes after the State applies a write,
/// re-reads each document the change names, and re-indexes it. That is what makes
/// read-back cover search too: writing a fact that search can't find is the same
/// class of failure as writing one `recall` can't return.
#[derive(Clone)]
pub struct IndexedMemory {
    writer: crate::writer::Writer,
    inner: Arc<dyn Memory>,
    index: Arc<FullTextIndex>,
    /// **What the last real read saw, and the signal it saw it under.**
    /// `None` until the first real read lands. Compared against a fresh
    /// [`Memory::write_summary`] on every call so an unchanged store can be
    /// answered from here instead of paying for [`Memory::scan`] again.
    last_scan: Arc<RwLock<Option<CachedMemoryScan>>>,
    /// The fold a fixture put in front of its store.
    #[cfg(any(test, feature = "testing"))]
    state: Option<Arc<crate::fold::Folded>>,
}

/// **A document the per-write path just re-read**: the entity it is, and the
/// scan the store answered with, or `None` when the document is gone.
type Reread = (EntityId, Option<DocScan>);

impl IndexedMemory {
    /// An index over a store, for a fixture: the store gets a fold in front of it
    /// that shares this index's writer, so a write through [`memory`] publishes
    /// the change the index reads. The **index is empty**: it is filled by
    /// [`rebuild`](IndexedMemory::rebuild) — separately, so a store that isn't
    /// reachable yet can't stop the server from booting.
    ///
    /// [`memory`]: IndexedMemory::memory
    #[cfg(any(test, feature = "testing"))]
    pub fn new(inner: Arc<dyn Memory>) -> Result<Self, MemoryError> {
        let writer = crate::writer::Writer::default();
        let state = Arc::new(crate::fold::Folded::new(inner).with_writer(writer.clone()));
        let mut indexed = Self::built(state.clone(), writer)?;
        indexed.state = Some(state);
        Ok(indexed)
    }

    /// The fold [`new`](IndexedMemory::new) put in front of the store: a concrete
    /// memory a contract can run against.
    #[cfg(any(test, feature = "testing"))]
    pub fn state(&self) -> Arc<crate::fold::Folded> {
        self.state
            .clone()
            .expect("this index was built over a fold")
    }

    /// An index that reads `inner`, the memory every caller is served by, and
    /// subscribes to the process writer's publication. The writer is shared with
    /// the store and the fold, which publish into it.
    pub fn with_writer(
        inner: Arc<dyn Memory>,
        writer: crate::writer::Writer,
    ) -> Result<Self, MemoryError> {
        Self::built(inner, writer)
    }

    fn built(inner: Arc<dyn Memory>, writer: crate::writer::Writer) -> Result<Self, MemoryError> {
        let indexed = IndexedMemory {
            index: Arc::new(FullTextIndex::with_owner(writer.clone())?),
            writer: writer.clone(),
            inner,
            last_scan: Arc::new(RwLock::new(None)),
            #[cfg(any(test, feature = "testing"))]
            state: None,
        };
        writer.subscribe(Arc::new(indexed.clone()));
        Ok(indexed)
    }

    /// The memory port the index reads documents from, which is the one every
    /// caller is served through.
    pub fn memory(&self) -> Arc<dyn Memory> {
        self.inner.clone()
    }

    /// The owner shared by the store, fold, and search publication.
    pub fn writer(&self) -> crate::writer::Writer {
        self.writer.clone()
    }

    /// Rebuild the whole index from a full re-scan of the store — the boot path.
    /// Returns how many documents were indexed.
    ///
    /// **The boot path, and only it, reports what the scan saw.** The
    /// consistency report names rows a hand edit left pointing at nothing; it is
    /// a diagnostic about the corpus, so it is worth one reading of the corpus
    /// and not one per answer. Every read re-scans now, and repeating the report
    /// on each would bury the reading that is worth having.
    pub async fn rebuild(&self) -> Result<usize, MemoryError> {
        self.rebuild_scanned().await.map(|scan| scan.len())
    }

    /// [`rebuild`](IndexedMemory::rebuild), handing back the scan it indexed, so
    /// the boot can ask more of the corpus it already holds in memory without
    /// reading the store again.
    pub async fn rebuild_scanned(&self) -> Result<Vec<DocScan>, MemoryError> {
        let scan = self.rescan().await?;
        let known = search::known_entities(&scan);
        for doc in &scan {
            report_consistency(doc, &known);
        }
        Ok(scan)
    }

    /// Replace the projection from a full scan of the store, and hand back what
    /// the scan saw. The one refresh — the boot path and the read path run the
    /// same one, so there is no second way for the index to be filled that could
    /// drift from this.
    ///
    /// **Skips the read entirely when nothing has changed.** [`Memory::write_summary`]
    /// is a cheap signal a store may offer; when it matches what the last real
    /// scan was taken under, no refresh is on record as failed (a stuck
    /// failure must still get a real attempt to clear it — see
    /// [`FullTextIndex::memory_refresh_pending`]) and no reload is owed (the
    /// reader is shared by every half, and a reload another half left owed is
    /// made up by the scan's ingest, see [`FullTextIndex::owes_a_reload`]), the cached scan from that
    /// read is the answer, unread again. A store with no signal (`None`) is
    /// scanned every time, exactly as before this existed.
    pub(crate) async fn rescan(&self) -> Result<Vec<DocScan>, MemoryError> {
        match self.catch_up().await? {
            Some(scan) => Ok(scan),
            None => Ok(self
                .last_scan
                .read()
                .expect("scan cache poisoned")
                .as_ref()
                .map(|(_, scan)| scan.clone())
                .unwrap_or_default()),
        }
    }

    /// Bring the index up to the store, and hand back the scan only when one
    /// was taken. `None` says the cached scan is still the answer, so a caller
    /// that only needs the index current — every `search` — copies nothing.
    async fn catch_up(&self) -> Result<Option<Vec<DocScan>>, MemoryError> {
        let summary = self.inner.write_summary().await?;
        if !self.index.memory_refresh_pending()
            && !self.index.owes_a_reload()
            && let Some(summary) = &summary
        {
            let cached = self.last_scan.read().expect("scan cache poisoned");
            if let Some((last_summary, _)) = cached.as_ref()
                && last_summary == summary
            {
                return Ok(None);
            }
        }

        // **Before the read, never after.** What this scan is entitled to clear
        // is what was already marked when it looked; a write that fails its
        // re-read while the scan is in flight is not covered by it.
        let began = self.index.reading_begins();
        let scan = self.inner.scan().await?;
        let mut history = CorpusHistoryTerms::new();
        for doc in &scan {
            if let Some(entity) = &doc.entity {
                history.insert(entity.id.clone(), self.history_terms(&entity.id).await?);
            }
        }
        self.index.ingest_all(&scan, began, &history)?;
        if let Some(summary) = summary {
            *self.last_scan.write().expect("scan cache poisoned") = Some((summary, scan.clone()));
        }
        Ok(Some(scan))
    }

    /// The index, for handing to whatever serves the `search` verb.
    pub fn index(&self) -> Arc<FullTextIndex> {
        self.index.clone()
    }

    /// **One entity's facts, each mapped to the terms of its earlier
    /// wordings** — empty for a fact nobody has ever corrected.
    ///
    /// One batched call against the write substrate rather than one per fact:
    /// [`Memory::claim_histories`] is the door this reaches through, and the
    /// reindex path pays one query per entity, not one per claim.
    async fn history_terms(&self, entity: &EntityId) -> Result<FactHistoryTerms, MemoryError> {
        Ok(self
            .inner
            .claim_histories(entity)
            .await?
            .into_iter()
            .filter_map(|(id, chain)| {
                let earlier = chain.len().checked_sub(1)?;
                if earlier == 0 {
                    return None;
                }
                Some((
                    id,
                    chain[..earlier]
                        .iter()
                        .map(|write| write.content.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                ))
            })
            .collect())
    }

    /// Re-index one entity's doc by **re-reading it from the store**. A doc that
    /// has vanished is dropped from the index rather than left as a ghost — by
    /// the id its postings were stored under, which is the store's, not the
    /// handle (see [`FullTextIndex::forget`]).
    pub(crate) async fn reindex(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        match self.inner.scan_entity(entity).await? {
            Some(scan) => {
                let history = self.history_terms(entity).await?;
                self.index.ingest_doc(&scan, &history)?;
                report_consistency(&scan, &self.index.known_entities());
                Ok(Some(scan))
            }
            None => self.index.forget(entity).map(|()| None),
        }
    }

    /// Refresh the projection for a document a **committed** write just changed.
    ///
    /// **A failure here is not the write failing.** The store took the write and
    /// read it back; what could not be done is re-reading the page to refresh a
    /// projection of it. Failing the verb for that tells the caller nothing was
    /// written and to try again, and a caller who obeys records the same thing
    /// twice.
    ///
    /// So the write is reported as what it is, and the index records that it is
    /// holding an older version of this document than the store — which is what
    /// makes `search` say so on every answer afterwards. Swallowing the failure
    /// without that mark would trade a wrong answer to one caller for a wrong
    /// answer to every later one.
    async fn refresh(&self, entity: &EntityId) -> Option<Reread> {
        match self.reindex(entity).await {
            Ok(scan) => {
                self.index.current(entity);
                Some((entity.clone(), scan))
            }
            Err(e) => {
                self.index.behind(entity);
                tracing::warn!(
                    %entity,
                    error = %e,
                    "SEARCH INDEX BEHIND — this document was written and the index could not \
                     re-read it, so `search` serves the version before that write and reports \
                     itself partial until a write to this document succeeds or the server \
                     restarts. The write itself landed; the memory verbs are unaffected."
                );
                None
            }
        }
    }
}

/// A fixture drives writes through the port the binary serves, so the verbs
/// read as they did when the index wrapped the store.
#[cfg(any(test, feature = "testing"))]
impl std::ops::Deref for IndexedMemory {
    type Target = dyn Memory;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl IndexedMemory {
    /// **Record that the index has caught up with a write**, so the next read
    /// does not scan the store for it.
    ///
    /// The State counted `first..=last` as its changes for this write, and the
    /// scan the index holds was taken under the count just before them. With any
    /// other state, or a document the index could not re-read, this records
    /// nothing and the next read scans. Otherwise the cached scan takes the re-read
    /// documents and the State's count now.
    async fn caught_up(
        &self,
        before: impl Fn(&WriteSummary) -> bool,
        after: impl Fn(&WriteSummary) -> bool,
        reread: Vec<Option<Reread>>,
    ) {
        let Some(reread) = reread.into_iter().collect::<Option<Vec<Reread>>>() else {
            return;
        };
        if self.index.memory_refresh_pending() {
            return;
        }
        let Ok(Some(now)) = self.inner.write_summary().await else {
            return;
        };
        if !after(&now) {
            return;
        }
        let mut cached = self.last_scan.write().expect("scan cache poisoned");
        let Some((summary, scan)) = cached.as_mut() else {
            return;
        };
        if !before(summary) {
            return;
        }
        for (entity, doc) in reread {
            let at = scan
                .iter()
                .position(|d| d.entity.as_ref().is_some_and(|e| e.id == entity));
            match (at, doc) {
                (Some(i), Some(doc)) => scan[i] = doc,
                (Some(i), None) => {
                    scan.remove(i);
                }
                (None, Some(doc)) => scan.push(doc),
                (None, None) => {}
            }
        }
        *summary = now;
    }
}

/// **The memory half of the index reads what the memory State changed.** For each
/// thing a change names, the index re-reads its document through the memory
/// every caller is served by, so search holds what a reader sees. A failure to
/// re-read is not the write failing: the index marks itself behind that
/// document, which makes `search` say so on every answer afterwards.
#[async_trait]
impl crate::writer::publish::Subscriber for IndexedMemory {
    async fn changed(&self, changes: &[crate::writer::publish::Change]) {
        use crate::writer::publish::Change;
        let mut reread = Vec::new();
        let (mut first, mut last, mut counted, mut uncounted) = (None, 0, 0, false);
        let mut began = None;
        for change in changes {
            if let Change::Began(summary) = change {
                began.get_or_insert(summary.clone());
            }
            let Change::Things {
                touched,
                names_changed,
                generation,
            } = change
            else {
                continue;
            };
            // A run's owner, and a mention in a message, read as the handle a
            // thing wears now.
            if *names_changed {
                self.index.names_changed();
            }
            for entity in touched {
                reread.push(self.refresh(entity).await);
            }
            match generation {
                Some(generation) => {
                    first.get_or_insert(*generation);
                    last = *generation;
                    counted += 1;
                }
                None => uncounted = true,
            }
        }
        if let (Some(first), false) = (first, uncounted)
            && last + 1 - first == counted
        {
            self.caught_up(
                |cached| cached.entities.0 + 1 == first as i64,
                |now| now.entities.0 == last as i64,
                reread,
            )
            .await;
        } else if first.is_none()
            && !reread.is_empty()
            && let Some(began) = began
        {
            self.caught_up(|cached| *cached == began, |_| true, reread)
                .await;
        }
    }
}

/// One half of the corpus behind `search`, able to bring itself level with the
/// store it mirrors.
///
/// **This is what makes one mechanism serve two stores.** Memory and mail are
/// two halves of one index fed by two stores, and the promise the [`Search`]
/// port makes — an answer backed by a scan taken for it — is the same promise
/// on both. A half implements it for its own store; nothing above has to know
/// which stores exist, so a third one later is a third implementor rather than
/// an edit here.
#[async_trait]
pub trait Refresh: Send + Sync {
    /// Take a scan and bring this half of the index to what it says.
    ///
    /// **Infallible on purpose.** A refresh that cannot reach its store does
    /// not fail the search: it records that this half is behind, and the last
    /// scan that succeeded goes on answering. Degrading is the promise; the
    /// coverage is where it is admitted.
    async fn refresh(&self);
}

#[async_trait]
impl Refresh for IndexedMemory {
    async fn refresh(&self) {
        if self.catch_up().await.is_err() {
            self.index.refresh_failed();
        }
    }
}

/// A [`Mailboxes`] with the search projection behind it — the mail half's
/// [`IndexedMemory`], and it exists for the same reason.
///
/// It wraps no verb. It is a subscriber of the writer: after each write, the
/// mail State publishes the messages the write wrote or changed, and the index
/// reads them again. Posting makes a message findable on the next call rather
/// than after a restart, and a delivery or a retirement keeps the `state` on a
/// search hit honest. A hit that says `new` for a message somebody drained an
/// hour ago is worse than no hit, because a reader acts on it.
///
/// **The limit of that, stated rather than left to be discovered.** This is a
/// boot-loaded projection updated from what the State publishes, so a message
/// that changes any other way keeps its indexed state until a write publishes it
/// again: a person moving a card on the board by hand is a second writer, which
/// the design does not have. The state on a hit is what jojobot last saw, which
/// is why a hit carries the id: `read_message` reads the State.
#[derive(Clone)]
pub struct IndexedMailboxes {
    writer: crate::writer::Writer,
    inner: Arc<dyn Mailboxes>,
    index: Arc<FullTextIndex>,
    /// **The port's change count when the index last held the whole board**,
    /// or `NOT_SEEN`. A refresh that finds the same count and no failure on
    /// record has nothing to read: every change to mail goes through the port.
    /// [`IndexedMailboxes::publish`] is the only writer of it.
    board_seen_at: Arc<std::sync::atomic::AtomicU64>,
    /// **The naming epoch at that read.** A mention in a message is served as the
    /// handle a thing wears now, so a rename changes the text the index should
    /// hold without a single write to mail.
    board_seen_epoch: Arc<std::sync::atomic::AtomicU64>,
}

/// The value of [`IndexedMailboxes::board_seen_at`] before a board read landed.
const NOT_SEEN: u64 = u64::MAX;

impl IndexedMailboxes {
    /// Wrap a store, writing into the index the Memory half already uses. **One
    /// index, not two** — that is what makes one ranked list possible at all.
    ///
    /// **A published change holds the index's mail order from its first re-read
    /// to the count it publishes; a board read holds it from the count it
    /// captures to its publication.** So no index write is older than a count
    /// published after it, and no board copy is older than a change that a later
    /// count covers. The order is the index's, so two decorators over one index
    /// share it. A search that finds the count current takes no part in it.
    pub fn new(inner: Arc<dyn Mailboxes>, index: Arc<FullTextIndex>) -> Self {
        IndexedMailboxes {
            writer: index.owner(),
            inner,
            index,
            board_seen_at: Arc::new(std::sync::atomic::AtomicU64::new(NOT_SEEN)),
            board_seen_epoch: Arc::new(std::sync::atomic::AtomicU64::new(NOT_SEEN)),
        }
    }

    /// Load the mail half from a full board read — the boot path. Returns how
    /// many messages were indexed.
    pub async fn rebuild(&self) -> Result<usize, MailboxError> {
        owned!(self, this, [], this.rebuild_owned().await);
        self.rebuild_owned().await
    }

    async fn rebuild_owned(&self) -> Result<usize, MailboxError> {
        // Before the read, never after — the point has to predate the board
        // read it will be handed back with.
        let began = self.index.reading_begins();
        let changes = self.inner.changes();
        let epoch = self.index.names_epoch();
        let messages = self.indexable_board().await?;
        self.index
            .ingest_mail_changes(&messages, began)
            .map_err(indexing)?;
        self.publish(changes, Some(epoch));
        Ok(messages.len())
    }

    /// **The board as the index may hold it: the messages in a box the board
    /// still lists and no person owns.** That mail is written to a person and
    /// read by nobody who speaks through search, so its text never enters the
    /// index. The exclusion is made here, where the index is written, rather than
    /// on the hits a query returns, so there is no filter to forget and no
    /// posting to leak.
    ///
    /// **A box the listing does not know leaves its messages out.** The listing
    /// is read after the scan, and a rename can land between the two: the scan
    /// holds a message under the old name of a box and the listing only the new
    /// one. Judging by "not in the set of private names" would call that message
    /// public for being unmatched. Judging by "in the set of public names" calls
    /// it nothing yet, and the next refresh reads both under one name.
    ///
    /// A board whose boxes cannot be read is an error: the messages would have
    /// no owners to be judged by.
    async fn indexable_board(&self) -> Result<Vec<Message>, MailboxError> {
        let messages = self.inner.scan_messages().await?;
        let public = self.public_boxes().await?;
        Ok(messages
            .into_iter()
            .filter(|message| public.contains(&message.mailbox))
            .collect())
    }

    /// The names of the boxes the board lists that no person owns.
    async fn public_boxes(
        &self,
    ) -> Result<std::collections::BTreeSet<jojobot_domain::mailbox::MailboxName>, MailboxError>
    {
        Ok(self
            .inner
            .list_mailboxes()
            .await?
            .into_iter()
            .filter(|held| !held.is_private())
            .map(|held| held.name)
            .collect())
    }

    /// Index the messages a verb just wrote or changed, each when its box is on
    /// the board and no person owns it. **A board that does not list the box
    /// leaves the message out**: the next refresh reads the whole board and
    /// indexes it if it belongs there, while a message wrongly indexed could
    /// not be taken back.
    ///
    /// **The boxes are listed once for the whole batch**, never once per
    /// message: the listing is a tally of every box, so a delivery of many
    /// messages would otherwise cost the boxes times the messages.
    ///
    /// **A board that cannot be listed leaves the messages out and says so.**
    /// The index cannot tell whose box each one is, so it holds an older board
    /// than the store does; the mail half is marked behind and the reason is
    /// logged, the same way a board read that fails in a refresh is. The verb
    /// itself succeeded and is not failed for it.
    async fn reindex(&self, messages: &[&Message]) -> Result<(), MailboxError> {
        if messages.is_empty() {
            return Ok(());
        }
        let public = match self.public_boxes().await {
            Ok(public) => public,
            Err(e) => {
                self.index.mail_refresh_failed();
                tracing::warn!(
                    error = %e,
                    mailbox = %messages[0].mailbox,
                    "SEARCH INDEX BEHIND — a message was written and the boxes could not be \
                     listed, so the index could not tell whether its box is a person's and \
                     left it out. Mail search reports itself stale until a board read lands."
                );
                return Ok(());
            }
        };
        for message in messages
            .iter()
            .filter(|message| public.contains(&message.mailbox))
        {
            self.index.ingest_message(message).map_err(indexing)?;
        }
        Ok(())
    }

    /// Index the messages a write just changed, read from the State, and move
    /// the count to the State's when this write is the only change since the
    /// index held the whole board.
    async fn absorb(&self, ids: &[jojobot_domain::mailbox::MessageId], position: u64) {
        let indexed = match self.inner.messages_by_id(ids).await {
            Ok(messages) => self.reindex(&messages.iter().collect::<Vec<_>>()).await,
            Err(e) => Err(e),
        };
        self.settle(position.checked_sub(1), indexed);
    }

    /// **The one place the count of the board the index holds is published.**
    /// The refresh, the rebuild and the re-read of a published change
    /// all end here, and each holds the index's mail order when it does. `epoch` is
    /// given by the readers of the whole board, which are the only ones that
    /// know the names they read under.
    fn publish(&self, count: Option<u64>, epoch: Option<u64>) {
        if let Some(epoch) = epoch {
            self.board_seen_epoch
                .store(epoch, std::sync::atomic::Ordering::SeqCst);
        }
        self.board_seen_at.store(
            count.unwrap_or(NOT_SEEN),
            std::sync::atomic::Ordering::SeqCst,
        );
    }

    /// **Whether the index holds the board as of the port's count now**, with no
    /// failure on record and under the names it was read in.
    fn board_held(&self) -> bool {
        let Some(changes) = self.inner.changes() else {
            return false;
        };
        !self.index.mail_refresh_pending()
            && self.board_seen_at.load(std::sync::atomic::Ordering::SeqCst) == changes
            && self
                .board_seen_epoch
                .load(std::sync::atomic::Ordering::SeqCst)
                == self.index.names_epoch()
    }

    /// **End the re-read of a published change.** Called with
    /// the index's mail order held. The count moves to the State's only when the index
    /// held the whole board before the write and the write is the only change the
    /// State has counted since (`before` was read under the same hold). Any other
    /// state leaves the count where it is, and the next refresh reads the board.
    ///
    /// **A failure of the index is not the write failing.** The State took the
    /// change; what could not be done is putting it in a projection. A subscriber
    /// that failed its caller would tell it nothing was written, and a caller who
    /// obeys posts the message twice or never marks a delivery it already holds.
    /// So the write stands, and the index is marked behind, which makes mail search say so on every answer until a board
    /// read lands. The count stays where it is.
    fn settle(&self, before: Option<u64>, indexed: Result<(), MailboxError>) {
        if let Err(e) = indexed {
            self.index.mail_refresh_failed();
            tracing::warn!(
                error = %e,
                "SEARCH INDEX BEHIND — a mail verb's change landed and the index could not \
                 take it, so mail search serves the board before that change and reports \
                 itself stale until a board read lands. The verb itself landed; the mailbox \
                 verbs are unaffected."
            );
            return;
        }
        if let (Some(before), Some(now)) = (before, self.inner.changes())
            && now == before.wrapping_add(1)
            && self.board_seen_at.load(std::sync::atomic::Ordering::SeqCst) == before
        {
            self.publish(Some(now), None);
        }
    }
}

/// **The session half of the index, over the sessions store.**
///
/// It decorates nothing: sessions are written through their own port and this
/// only reads them, so there is no verb to wrap. What it owns is the same job
/// the other two halves own — filling its share of the index at boot, and
/// refreshing it before an answer, from its own store.
/// The signal [`jojobot_domain::session::Sessions::write_summary`] answers,
/// paired with the read it was taken beside.
///
/// **And the names epoch it was taken under**, because a rename changes what the
/// runs read as without moving the signal.
type CachedSessionScan = (
    (i64, Option<jiff::Timestamp>),
    u64,
    Vec<jojobot_domain::session::Session>,
);

pub struct IndexedSessions {
    inner: Arc<dyn jojobot_domain::session::Sessions>,
    index: Arc<FullTextIndex>,
    /// **What the last real read saw, and the signal it saw it under.**
    /// `None` until the first real read lands. Compared against a fresh
    /// [`jojobot_domain::session::Sessions::write_summary`] on every call so
    /// an unchanged store can be answered from here instead of paying for
    /// [`jojobot_domain::session::Sessions::all_sessions`] again — the same
    /// mechanism [`IndexedMemory::rescan`] uses, on this port's own signal.
    last_scan: RwLock<Option<CachedSessionScan>>,
}

impl IndexedSessions {
    pub fn new(
        inner: Arc<dyn jojobot_domain::session::Sessions>,
        index: Arc<FullTextIndex>,
    ) -> Self {
        IndexedSessions {
            inner,
            index,
            last_scan: RwLock::new(None),
        }
    }

    /// Load every bot's runs — the boot path. Returns how many were indexed.
    ///
    /// **Every bot's, not the asker's.** Who may read a run is decided when a
    /// query asks; an index built per caller would let the first bot to search
    /// decide what the second one could find.
    pub async fn rebuild(&self) -> Result<usize, jojobot_domain::session::SessionError> {
        Ok(self.sync().await?.len())
    }

    /// Replace the projection from a full read of the sessions store, and
    /// hand back what it saw — skipping the read entirely when nothing has
    /// changed. The one refresh: the boot path and the read path run the
    /// same one, exactly as the memory half's `rescan` does.
    async fn sync(
        &self,
    ) -> Result<Vec<jojobot_domain::session::Session>, jojobot_domain::session::SessionError> {
        let summary = self.inner.write_summary().await?;
        // Taken before the read, so a rename that lands while the read is in
        // flight leaves the next refresh to read again rather than being
        // absorbed into this one.
        let epoch = self.index.names_epoch();
        if let Some(summary) = &summary {
            let cached = self.last_scan.read().expect("session scan cache poisoned");
            if let Some((last_summary, last_epoch, sessions)) = cached.as_ref()
                && last_summary == summary
                && *last_epoch == epoch
            {
                return Ok(sessions.clone());
            }
        }

        // Taken before the read that can fail, so a failure that lands while
        // it is in flight is not cleared by this reading's own success — see
        // [`FullTextIndex::ingest_sessions_changes`].
        let began = self.index.reading_begins();
        let sessions = self.inner.all_sessions().await?;
        self.index
            .ingest_sessions_changes(&sessions, began)
            .map_err(|e| jojobot_domain::session::SessionError::Store(e.to_string()))?;
        if let Some(summary) = summary {
            *self.last_scan.write().expect("session scan cache poisoned") =
                Some((summary, epoch, sessions.clone()));
        }
        Ok(sessions)
    }
}

/// TEMPORARY(9.3.8) the session refresh. A read-path catch-up, not a write reaction: search as a subscriber and a query replaces it in 9.3.8.
#[async_trait]
impl Refresh for IndexedSessions {
    async fn refresh(&self) {
        // A read that cannot reach the store leaves the last good one
        // standing, and now says so: `session_coverage` is what lets an
        // answer tell that from having searched and found nothing.
        if self.sync().await.is_err() {
            self.index.session_refresh_failed();
        }
    }
}

/// TEMPORARY(9.3.8) the mail refresh. A read-path catch-up, not a write reaction: search as a subscriber and a query replaces it in 9.3.8.
#[async_trait]
impl Refresh for IndexedMailboxes {
    /// **`scan_messages`, never a delivering read.** Reading IS delivery on the
    /// verbs that deliver, so a refresh built on one would drain a box as a side
    /// effect of answering a question. This board read takes nothing and moves
    /// nothing.
    ///
    /// **Both halves of a refresh can leave the index behind, so both mark it.**
    /// The store read is the obvious one. The index write is the other: the
    /// board arrived and the index does not end up holding it, which is the same
    /// state by a different route, and reporting `Loaded` over it is the shape
    /// rule 130 forbids. The memory half gets this free — `rescan` chains the
    /// store read and the index write through one `?`.
    ///
    /// **This branch is now proven.** The window is an index write that fails
    /// after a board read that landed — `payload_json` still cannot fail over
    /// a `Message`, and tantivy's own internals still cannot be made to fail
    /// from outside, but `FullTextIndex` carries a narrow, always-inert fault
    /// injector on its own write path (`write_message`, and
    /// `ingest_mail_changes`'s own `commit` and `reload`), reached only from
    /// `#[cfg(any(test, feature = "testing"))]` code. It sits outside
    /// `search()` entirely — that method touches `index` and `reader`, never
    /// the fault or the writer — so the branch every search goes through
    /// gained nothing. `a_commit_failure_and_a_reload_failure_each_surface_as_an_error`
    /// proves this branch: a commit or a reload failure after a landed board
    /// read reaches `mail_refresh_failed`, the same way a failed board read
    /// itself always could. **What it does NOT reach: a real disk.** The
    /// index is still `Index::create_in_ram`, in every build, so this proves
    /// the branch reachable and the failure surfaced — never a real I/O
    /// error, which does not exist here to inject.
    ///
    /// **The second consequence of that same failure is fixed, not merely
    /// addressed.** `ingest_mail_changes` stages every `delete_term` before it
    /// writes any message; on a write failing partway, the writer is rolled
    /// back to its last commit before the error is returned, so nothing
    /// staged by the failed call — not its deletes, not documents an earlier
    /// iteration of its own loop already added — survives to be picked up by
    /// the next commit from anywhere else.
    /// `a_failed_write_does_not_leave_its_staged_delete_for_a_later_commit`
    /// proves it: watched red without the rollback (a message neither call
    /// touched vanished from search after an unrelated commit), green with
    /// it.
    async fn refresh(&self) {
        if self.board_held() {
            return;
        }
        // **Held from the count to the publication.** A verb that finished while
        // this waited has published, so the check is made again.
        // No case pins this lock: removing it leaves every case green, because the
        // publication checks in `settle` already refuse the interleavings a case can
        // pause. It closes the window no case can pause in: a board ingest that lands
        // between a verb's last index write and its publication.
        owned!(self, this, [], this.refresh().await);
        if self.board_held() {
            return;
        }
        // **Read before the board, never after**: a write that lands while the
        // board is read leaves the next refresh to read again.
        let changes = self.inner.changes();
        let epoch = self.index.names_epoch();
        let began = self.index.reading_begins();
        match self.indexable_board().await {
            Ok(messages) => {
                if self.index.ingest_mail_changes(&messages, began).is_err() {
                    self.index.mail_refresh_failed();
                } else {
                    self.publish(changes, Some(epoch));
                }
            }
            Err(_) => self.index.mail_refresh_failed(),
        }
    }
}

/// An index failure, in the mailbox context's vocabulary. The seam between the
/// two contexts is exactly here and nowhere else.
fn indexing(e: MemoryError) -> MailboxError {
    MailboxError::Store(format!("search index: {e}"))
}

/// **The mail half of the index reads what the mail State changed.** The writer
/// delivers, after a write, the messages it wrote or changed and the State's new
/// count. The index re-reads those messages through the same render a reader
/// gets, so search holds what a reader sees.
///
/// A write that changes no message (a box created or repointed) carries only the
/// count: the index may hold other messages once a box changes owner, so the
/// next refresh reads the board.
#[async_trait]
impl crate::writer::publish::Subscriber for IndexedMailboxes {
    async fn changed(&self, changes: &[crate::writer::publish::Change]) {
        use crate::writer::publish::Change;
        let mut touched = Vec::new();
        for change in changes {
            match change {
                Change::Messages(ids) => touched.extend(ids.iter().cloned()),
                Change::MailPosition(position) => {
                    let ids = std::mem::take(&mut touched);
                    if !ids.is_empty() {
                        self.absorb(&ids, *position).await;
                    }
                }
                Change::Posted(_) | Change::Began(_) | Change::Things { .. } => {}
            }
        }
    }
}

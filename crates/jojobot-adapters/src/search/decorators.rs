use super::*;

/// A [`Memory`] with a live search projection behind it.
///
/// Every verb delegates to the store, and every **successful** write re-scans the
/// document it touched and re-indexes it. That is what makes read-back cover
/// search too: writing a fact that search can't find is the same class of failure
/// as writing one `recall` can't return.
pub struct IndexedMemory {
    inner: Arc<dyn Memory>,
    index: Arc<FullTextIndex>,
    /// **What the last real read saw, and the signal it saw it under.**
    /// `None` until the first real read lands. Compared against a fresh
    /// [`Memory::write_summary`] on every call so an unchanged store can be
    /// answered from here instead of paying for [`Memory::scan`] again.
    last_scan: RwLock<Option<(WriteSummary, Vec<DocScan>)>>,
}

impl IndexedMemory {
    /// Wrap a store with an **empty** index. The index is filled by
    /// [`rebuild`](IndexedMemory::rebuild) — separately, so a store that isn't
    /// reachable yet can't stop the server from booting.
    pub fn new(inner: Arc<dyn Memory>) -> Result<Self, MemoryError> {
        Ok(IndexedMemory {
            inner,
            index: Arc::new(FullTextIndex::open()?),
            last_scan: RwLock::new(None),
        })
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
        let scan = self.rescan().await?;
        let known = search::known_entities(&scan);
        for doc in &scan {
            report_consistency(doc, &known);
        }
        Ok(scan.len())
    }

    /// Replace the projection from a full scan of the store, and hand back what
    /// the scan saw. The one refresh — the boot path and the read path run the
    /// same one, so there is no second way for the index to be filled that could
    /// drift from this.
    ///
    /// **Skips the read entirely when nothing has changed.** [`Memory::write_summary`]
    /// is a cheap signal a store may offer; when it matches what the last real
    /// scan was taken under, and no refresh is on record as failed (a stuck
    /// failure must still get a real attempt to clear it — see
    /// [`FullTextIndex::memory_refresh_pending`]), the cached scan from that
    /// read is the answer, unread again. A store with no signal (`None`) is
    /// scanned every time, exactly as before this existed.
    pub(crate) async fn rescan(&self) -> Result<Vec<DocScan>, MemoryError> {
        let summary = self.inner.write_summary().await?;
        if !self.index.memory_refresh_pending()
            && let Some(summary) = &summary
        {
            let cached = self.last_scan.read().expect("scan cache poisoned");
            if let Some((last_summary, scan)) = cached.as_ref()
                && last_summary == summary
            {
                return Ok(scan.clone());
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
        Ok(scan)
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
    pub(crate) async fn reindex(&self, entity: &EntityId) -> Result<(), MemoryError> {
        match self.inner.scan_entity(entity).await? {
            Some(scan) => {
                let history = self.history_terms(entity).await?;
                self.index.ingest_doc(&scan, &history)?;
                report_consistency(&scan, &self.index.known_entities());
                Ok(())
            }
            None => self.index.forget(entity),
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
    async fn refresh(&self, entity: &EntityId) {
        match self.reindex(entity).await {
            Ok(()) => self.index.current(entity),
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
            }
        }
    }
}

#[async_trait]
impl Memory for IndexedMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        let written = self.inner.add_entity(new).await?;
        if let Guarded::Written(entity) = &written {
            self.refresh(&entity.id).await;
        }
        Ok(written)
    }

    /// **The thing and its first claim enter the index together**, as an entity
    /// and a capture each would. Forwarded and not left to the port's default,
    /// which refuses.
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        let written = self.inner.add_entity_with_first_claim(new, first).await?;
        if let Guarded::Written((entity, _)) = &written {
            self.refresh(&entity.id).await;
        }
        Ok(written)
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
    }

    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.inner.former_handles().await
    }

    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        let written = self.inner.update_entity(handle, patch).await?;
        if let Guarded::Written(entity) = &written {
            self.refresh(&entity.id).await;
        }
        Ok(written)
    }

    /// **Reindexed under its new handle**, the same "reindex the doc the
    /// store just wrote" step every other write here takes. The doc itself
    /// is found by badge underneath, so this is what makes the new spelling
    /// findable rather than what makes the record exist.
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        override_token: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        let written = self
            .inner
            .rename_entity(from, to, parent, date, override_token)
            .await?;
        if let Guarded::Written(entity) = &written {
            self.refresh(&entity.id).await;
            // A run's owner reads as the handle the entity wears now.
            self.index.names_changed();
        }
        Ok(written)
    }

    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        let entity = self.inner.archive_entity(id, reason).await?;
        self.refresh(&entity.id).await;
        Ok(entity)
    }

    async fn restore_entity(&self, id: &EntityId) -> Result<(Entity, Archived), MemoryError> {
        let restored = self.inner.restore_entity(id).await?;
        self.refresh(&restored.0.id).await;
        Ok(restored)
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let written = self.inner.capture(fact).await?;
        if let Guarded::Written(fact) = &written {
            self.refresh(&fact.home).await;
        }
        Ok(written)
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }

    /// **Straight through, and it does not touch the index.** The writes behind
    /// a key are the store's own record; the index projects current truth and
    /// holds nothing a history is read from.
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }

    /// **Straight through.** A claim's writes are the store's record and the
    /// index holds no version of them.
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }

    /// **Straight through, and it does not touch the index**, for the reason
    /// [`claim_history`](Self::claim_history) does not. A decorator that
    /// leaves this to the trait default serves the default in production.
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        self.inner.claim_histories(entity).await
    }

    /// **Straight through, and it does not touch the index.** What a thing
    /// holds is decided by the order its keys were written, which is the
    /// store's record and not something a projection could re-derive.
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.inner.fields(entity).await
    }

    /// **Straight through, and it does not touch the index.** Which claim
    /// backs each folded value is read off the writes, which the store keeps
    /// and the projection does not.
    ///
    /// **A decorator that leaves this to the trait default serves the default
    /// in production**, whatever the store underneath it implements — so the
    /// three reads below forward for the same reason `fields` does.
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, jojobot_domain::memory::FieldBacking>, MemoryError> {
        self.inner.backing(entity).await
    }

    /// **Straight through, and it does not touch the index.** Lineage is a
    /// pointer between two records, which the store selects on.
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.inner.built_on(source).await
    }

    /// **Straight through, and it does not touch the index.** Which records
    /// name a handle in a field is a read of the store's own rows.
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.referring_to(target).await
    }

    /// **Straight through, and it does not touch the index.** What this build
    /// ships as a default is known to `Provisioned` alone; the default body
    /// here would say no write ever echoes one.
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &BTreeMap<String, String>,
    ) -> Vec<String> {
        self.inner.echoed_defaults(entity, fields).await
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let written = self.inner.update_fact(address, patch, caller).await?;
        if let Guarded::Written(fact) = &written {
            self.refresh(&fact.home).await;
        }
        Ok(written)
    }

    /// One reindex, not two: a retraction writes both rows into the same
    /// document, so re-reading that document once is what makes the marked row
    /// and the account of it visible together. Re-reading twice would only
    /// re-read the same page.
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
    ) -> Result<Retraction, MemoryError> {
        let taken_back = self.inner.retract(address, reason, date, caller).await?;
        self.refresh(&taken_back.retracted.home).await;
        Ok(taken_back)
    }

    /// **Both sides are reindexed, and the folded one matters most.** Its rows
    /// moved to the survivor, so an index still holding them would go on
    /// answering with a handle that is no longer a thing — the split this verb
    /// exists to close, reopened in the half a reader actually searches.
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
    ) -> Result<Merge, MemoryError> {
        let done = self
            .inner
            .merge(folded, survivor, reason, date, caller)
            .await?;
        // The folded bot's runs read as the survivor's now.
        self.index.names_changed();
        self.refresh(folded).await;
        // ⚠️ **This line is not what makes the survivor findable by the names it
        // just took.** Removing it leaves the name-carry contract green: the merge
        // already appends an entity write for the survivor, and the index
        // re-reads a doc whose write it has not seen. It stays as the explicit
        // statement that both sides are re-read, and nothing should lean on it
        // as the thing that carries the result.
        self.refresh(survivor).await;
        Ok(done)
    }

    /// Prose is indexed material, so a charter written here is findable on the
    /// next call — the same "reindex the doc the store just wrote" step every
    /// other write takes, and for the same reason: without it, the one part of
    /// a bot that is pure prose would be the one part search could not see.
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        let stored = self.inner.set_prose(entity, prose).await?;
        self.refresh(entity).await;
        Ok(stored)
    }

    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.inner.scan().await
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.inner.scan_entity(entity).await
    }

    async fn composed_prose(&self, entity: &EntityId, own: &str) -> Result<String, MemoryError> {
        self.inner.composed_prose(entity, own).await
    }

    // **No document is re-read on either of these** — a declaration is not
    // a document: it says which keys a writer should fill and is carried by
    // no entity, so there is no ONE doc for `refresh` to re-scan.
    //
    // **The scan CACHE is still dropped.** Every doc's fields are folded
    // against the CURRENT declarations at scan time (`folded_fields`), so a
    // declaration that changes how a key folds changes what every affected
    // doc's fields answer — not one doc, and not zero. `write_summary`
    // never counts a declaration write, so the ordinary skip-if-unchanged
    // check in `rescan` cannot see this on its own; dropping the cache
    // forces the NEXT read to take a real scan rather than serve one taken
    // under the old declarations.
    async fn declare_type(&self, declared: DeclaredType) -> Result<DeclaredType, MemoryError> {
        let written = self.inner.declare_type(declared).await?;
        *self.last_scan.write().expect("scan cache poisoned") = None;
        Ok(written)
    }

    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }

    async fn displaced_type(&self, name: &str) -> Result<Option<Displaced>, MemoryError> {
        self.inner.displaced_type(name).await
    }

    /// The same reason [`declare_type`](Self::declare_type) drops the
    /// cache: a kind's keys fold exactly like a declared type's.
    async fn declare_kind(
        &self,
        token: &str,
        origin: jojobot_domain::memory::types::Origin,
        fields: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        self.inner.declare_kind(token, origin, fields).await?;
        *self.last_scan.write().expect("scan cache poisoned") = None;
        Ok(())
    }

    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }

    /// **Reclaiming changes folding too**: a key that summed while the kind
    /// held it falls back to newest-write-wins once reclaimed, for every doc
    /// answering to it.
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await?;
        *self.last_scan.write().expect("scan cache poisoned") = None;
        Ok(())
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
        if self.rescan().await.is_err() {
            self.index.refresh_failed();
        }
    }
}

/// A [`Mailboxes`] with the search projection behind it — the mail half's
/// [`IndexedMemory`], and it exists for the same reason.
///
/// Every verb delegates to the store, and every one that **changes** a message
/// re-indexes it: posting makes it findable on the next call rather than after a
/// restart, and a delivery or a retirement keeps the `state` on a search hit
/// honest. A hit that says `new` for a message somebody drained an hour ago is
/// worse than no hit, because a reader acts on it.
///
/// **The limit of that, stated rather than left to be discovered.** This is a
/// boot-loaded projection updated by the verbs that pass through it, so a
/// message that changes any other way keeps its indexed state until some verb
/// touches it again: a person moving a card on the board by hand, and — inside
/// jojobot — a message a delivery deliberately excluded, which by definition is
/// one somebody else moved under it. Neither is a live view, and neither can be
/// made one without polling the board. The state on a hit is what jojobot last
/// saw, which is why a hit carries the id: `read_message` reads the store.
pub struct IndexedMailboxes {
    inner: Arc<dyn Mailboxes>,
    index: Arc<FullTextIndex>,
}

impl IndexedMailboxes {
    /// Wrap a store, writing into the index the Memory half already uses. **One
    /// index, not two** — that is what makes one ranked list possible at all.
    pub fn new(inner: Arc<dyn Mailboxes>, index: Arc<FullTextIndex>) -> Self {
        IndexedMailboxes { inner, index }
    }

    /// Load the mail half from a full board read — the boot path. Returns how
    /// many messages were indexed.
    pub async fn rebuild(&self) -> Result<usize, MailboxError> {
        // Before the read, never after — the point has to predate the board
        // read it will be handed back with.
        let began = self.index.reading_begins();
        let messages = self.indexable_board().await?;
        self.index
            .ingest_mail_changes(&messages, began)
            .map_err(indexing)?;
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
        let began = self.index.reading_begins();
        match self.indexable_board().await {
            Ok(messages) => {
                if self.index.ingest_mail_changes(&messages, began).is_err() {
                    self.index.mail_refresh_failed();
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

#[async_trait]
impl Mailboxes for IndexedMailboxes {
    async fn create_mailbox(
        &self,
        name: &jojobot_domain::mailbox::MailboxName,
        owner: &jojobot_domain::memory::EntityId,
        override_token: Option<&str>,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError>
    {
        // A box holds no text of its own — nothing to index until a message
        // lands in it.
        self.inner.create_mailbox(name, owner, override_token).await
    }

    async fn repoint_owner(
        &self,
        from: &jojobot_domain::memory::EntityId,
        to: &jojobot_domain::memory::EntityId,
    ) -> Result<Option<jojobot_domain::mailbox::Mailbox>, MailboxError> {
        // Same reason as create_mailbox: a box holds no text of its own, so
        // there is nothing here to reindex.
        self.inner.repoint_owner(from, to).await
    }

    async fn list_mailboxes(&self) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
        self.inner.list_mailboxes().await
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.inner.scan_messages().await
    }

    async fn message_by_id(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Option<Message>, MailboxError> {
        self.inner.message_by_id(id).await
    }

    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        self.inner.sent_by(senders).await
    }

    async fn post_message(
        &self,
        message: jojobot_domain::mailbox::NewMessage,
    ) -> Result<jojobot_domain::mailbox::Guarded<Message>, MailboxError> {
        let written = self.inner.post_message(message).await?;
        if let jojobot_domain::mailbox::Guarded::Written(message) = &written {
            self.reindex(&[message]).await?;
        }
        Ok(written)
    }

    async fn read_mailbox(
        &self,
        name: &jojobot_domain::mailbox::MailboxName,
        taken_by: jojobot_domain::mailbox::TakenBy,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Delivery>, MailboxError>
    {
        let delivered = self.inner.read_mailbox(name, taken_by).await?;
        if let jojobot_domain::mailbox::Guarded::Written(delivery) = &delivered {
            let delivered: Vec<&Message> = delivery.messages.iter().map(|d| &d.message).collect();
            self.reindex(&delivered).await?;
        }
        Ok(delivered)
    }

    async fn read_message(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<jojobot_domain::mailbox::Delivered, MailboxError> {
        let delivered = self.inner.read_message(id).await?;
        self.reindex(&[&delivered.message]).await?;
        Ok(delivered)
    }

    async fn mark_processed(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        let processed = self.inner.mark_processed(id, notes).await?;
        self.reindex(&[&processed]).await?;
        Ok(processed)
    }

    /// **No incremental reindex — there is no `Message` left to feed one.**
    /// A quarantined row is unreadable by design, the same way a damaged
    /// one already is, and a damaged row is not incrementally deindexed
    /// either: it stops answering on the next full refresh, which already
    /// reads the board through this same port and already excludes
    /// anything unreadable.
    async fn quarantine(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
        by: &jojobot_domain::mailbox::MailboxName,
        reason: &str,
        at: jiff::Timestamp,
    ) -> Result<jojobot_domain::mailbox::Quarantined, MailboxError> {
        self.inner.quarantine(id, by, reason, at).await
    }
}

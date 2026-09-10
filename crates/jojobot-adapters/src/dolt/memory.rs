//! **Memory, as rows.**
//!
//! An entity is a row, a fact is a row under it, and the two tables that hang
//! off a fact carry its fields and its references. The rules are the
//! domain's and have not moved: what changed is that they are held in columns a
//! query can reach rather than in a page a person could edit.
//!
//! **The model is ported, not redesigned.** Every column here exists because
//! the domain has a field for it. Where the domain holds one optional edge,
//! this holds one; where it holds an open bag nothing interprets, this holds it
//! open. A schema that decided something the model leaves undecided would be a
//! change to the model wearing a storage decision's clothes.
//!
//! **A fact names ONE entity.** Whose claim it is and what it is about are the
//! same thing (rule 147); they only ever came apart in a store where a row's
//! page could say something the row's own cell did not, and two columns here
//! would carry that disagreement forward as though it were a concept. The
//! record's `home` and `subject` both read from that column, so nothing above
//! this adapter has to know the difference has gone.
//!
//! **A standing nobody declared is NULL** rather than the value it implies —
//! the difference between "somebody said settled" and "nobody said" is one the
//! reader has to be able to make.
//!
//! **What this adapter does NOT carry** is the same list the mail rail's
//! header names: no read-back guard, no escaping, no linearization lock. A
//! transaction either commits or does not, and the store hands back the bytes
//! it was given.

use async_trait::async_trait;
use jiff::civil::Date;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::{
    ClaimWrite, Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress,
    FactId, FactPatch, FactStatus, FieldWrite, FormerHandle, Guarded, KeyWrite, Memory,
    MemoryError, Merge, NewEntity, NewFact, Provenance, Retraction, Standing, apply_entity_patch,
    apply_fact_patch, folded_fields, guard, guard_fit,
    kinds::{self, NotAKind},
    merge_account, normalize_content, normalize_details, normalize_prose, referenced_by,
    retraction_of, screen_entity_patch, search, standing_of, stood_after, stood_after_capture,
    types::{DeclaredType, Field, Fold, Origin, ValueType, guard_replacement, validate_type},
    validate_content, validate_details, validate_edge, validate_entity, validate_fields,
    validate_prose, validate_provenance_source, validate_subject, validate_write_subject,
    writes_of,
};
use sqlx::{MySql, MySqlPool, Row, Transaction};

use super::ids::{self, Draw};

/// Memory kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets nobody
/// set.
#[derive(Clone)]
pub struct DoltMemory {
    pool: MySqlPool,
    /// Where a badge comes from. **A value rather than a call**, so the
    /// collision path can be watched through the verb that mints: entropy will
    /// not produce a collision on demand.
    draw: Draw,
    /// **What the build supplies over this store**, for the one question the
    /// existence guard asks: does this handle name something that exists?
    ///
    /// Nothing here is written and nothing is listed to a caller. It is read
    /// where a guard would otherwise see only the stored rows and refuse a
    /// claim pointing at a record every read answers for.
    supplied: Provisions,
    /// **The clock the store stamps with.** A value rather than a call, for the
    /// same reason the draw is one: an instance acting out a day has to stamp
    /// *when jojobot took this in* with that day, and a store reaching for the
    /// wall clock could never be told about it.
    clock: Clock,
}

impl DoltMemory {
    /// Open the store over an existing pool.
    ///
    /// **The schema is not this adapter's to create.** It arrives through the
    /// migrations the server applies on start — see [`crate::dolt::migrate`].
    /// **Opening a store is not booting one.** It loads no kinds: the set a
    /// process parses against is written and read by the boot's seed, and a
    /// load here would make every caller that happens to open a memory store
    /// look seeded while a caller that opens only the mail rail does not.
    /// That is a difference nobody can see from a handle's refusal, which is
    /// exactly the failure the two refusals exist to name.
    pub fn open(pool: MySqlPool) -> Self {
        DoltMemory {
            pool,
            draw: ids::drawing(),
            supplied: Provisions::default(),
            clock: Clock::default(),
        }
    }

    /// **The store, told what the build supplies over it.** Only the existence
    /// guard reads it: nothing is stored, nothing is listed, and a read still
    /// resolves supplied records in the layer above.
    ///
    /// A guard that consults what the store holds has to see what the build
    /// supplies as well, or the two halves disagree about what exists — a read
    /// answers for a record and a write pointing at it is refused as naming
    /// nothing.
    pub fn knowing(mut self, supplied: Provisions) -> Self {
        self.supplied = supplied;
        self
    }

    /// **The store, told which clock it stamps with.** The real one unless an
    /// operator stated a day for the whole run.
    #[must_use]
    pub fn on_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// The same store over a supplied draw, **so the collision path can be
    /// watched through the verb that mints**.
    pub fn open_drawing(pool: MySqlPool, draw: Draw) -> Self {
        DoltMemory {
            pool,
            draw,
            supplied: Provisions::default(),
            clock: Clock::default(),
        }
    }

    /// **Give a badge to every row written before the column existed.**
    ///
    /// A row gains one when it is next rewritten, which reaches the rows
    /// something touches and no others. **A store where nothing is edited would
    /// keep unbadged rows for ever**, so this reaches the rest — at startup,
    /// after the migrations, where the schema is already known to be current.
    ///
    /// **Not a migration**, because a migration is SQL and the badge comes from
    /// the store's own draw: probed inside a transaction, retried on a
    /// collision, the same shape every badge has. A SQL backfill would be a
    /// second generator, and rows of two different shapes is a cost that never
    /// expires.
    ///
    /// **One transaction per row rather than one for all of them.** The draw
    /// probes what is already committed, so a single transaction would be
    /// probing against rows it had not written yet — and a fill that fails
    /// half way has still filled half, which is progress rather than damage.
    ///
    /// Returns how many it gave out. **Idempotent: a second run fills none.**
    pub async fn badge_the_unbadged(&self) -> Result<usize, MemoryError> {
        let waiting: Vec<String> =
            sqlx::query_scalar("SELECT id FROM entity WHERE badge IS NULL ORDER BY id")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        let mut given = 0;
        for handle in waiting {
            let mut tx = self.pool.begin().await.map_err(store)?;
            let badge = mint_badge(&mut tx, &self.draw).await?;
            sqlx::query("UPDATE entity SET badge = ? WHERE id = ? AND badge IS NULL")
                .bind(&badge)
                .bind(&handle)
                .execute(&mut *tx)
                .await
                .map_err(store)?;
            tx.commit().await.map_err(store)?;
            given += 1;
        }
        Ok(given)
    }

    /// **Rekey a row written before entities carried a badge**, onto the
    /// badge the row's own entity wears now.
    ///
    /// A migration cannot do this: it runs before [`Self::badge_the_unbadged`]
    /// does, over a database where the badge column may exist but no row's
    /// badge is drawn yet — so a migrated backfill would rewrite nothing, the
    /// ledger would record it as applied, and it would never run again. This
    /// sits beside the badge fill instead, at startup, immediately after it —
    /// the one place both preconditions (the column, and the values in it)
    /// are actually met.
    ///
    /// **The completion gate is a live question, not a memory of having
    /// run.** There is no ledger row for this: a second call re-asks, of
    /// every badged entity, whether any of its rows still hold its handle,
    /// and only touches the ones that answer yes. A boot that half-finished
    /// and a boot that never started reach the same place, because both are
    /// just "some entities still answer yes" to the same question.
    ///
    /// **Every badge-keyed column, and no other.** `fact.entity`,
    /// `fact.derived_from`, `field_write.entity`, `fact_write.entity`,
    /// `fact_write.derived_from` and `fact_event_metadata`/`fact_event_ref`'s
    /// `fact_home` are what [`Self::merge`] already treats as holding a
    /// storage key rather than a plain handle — the same list, because a fold
    /// and a backfill are rekeying the same columns for different reasons.
    /// `entity_alias.entity` joins the list here: it is an alias row's own
    /// foreign key back to the entity it belongs to, the same shape
    /// `fact.entity` was, so a rename that left it on the handle would sever
    /// a thing from its own nicknames — and from every search hit that came
    /// through one — the moment the row moved.
    /// `fact_event_ref.entity` and `entity.parent`, the columns that name a
    /// DIFFERENT entity rather than the row's own, are untouched — they
    /// resolve on the way out instead, in the mention layer, not badge-keyed
    /// at all.
    ///
    /// Returns how many entities had rows rekeyed. **Idempotent**: a second
    /// run touches none.
    pub async fn backfill_handle_keyed_rows(&self) -> Result<usize, MemoryError> {
        let badged: Vec<(String, String)> =
            sqlx::query_as("SELECT id, badge FROM entity WHERE badge IS NOT NULL")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        let mut rekeyed = 0;
        for (handle, badge) in badged {
            if !Self::still_handle_keyed(&self.pool, &handle, &badge).await? {
                continue;
            }
            let mut tx = self.pool.begin().await.map_err(store)?;
            for statement in [
                "UPDATE fact SET entity = ? WHERE entity = ?",
                "UPDATE fact SET derived_from = ? WHERE derived_from = ?",
                "UPDATE field_write SET entity = ? WHERE entity = ?",
                "UPDATE fact_write SET entity = ? WHERE entity = ?",
                "UPDATE fact_write SET derived_from = ? WHERE derived_from = ?",
                "UPDATE fact_event_metadata SET fact_home = ? WHERE fact_home = ?",
                "UPDATE fact_event_ref SET fact_home = ? WHERE fact_home = ?",
                "UPDATE entity_alias SET entity = ? WHERE entity = ?",
            ] {
                sqlx::query(statement)
                    .bind(&badge)
                    .bind(&handle)
                    .execute(&mut *tx)
                    .await
                    .map_err(store)?;
            }
            tx.commit().await.map_err(store)?;
            rekeyed += 1;
        }
        Ok(rekeyed)
    }

    /// **The completion gate `backfill_handle_keyed_rows` asks**, over one
    /// entity: does any badge-keyed column still hold this handle rather than
    /// the badge it now wears. A handle that already equals its own badge
    /// (never true in practice, but checked rather than assumed) needs
    /// nothing either.
    async fn still_handle_keyed(
        pool: &MySqlPool,
        handle: &str,
        badge: &str,
    ) -> Result<bool, MemoryError> {
        if handle == badge {
            return Ok(false);
        }
        for (table, column) in [
            ("fact", "entity"),
            ("fact", "derived_from"),
            ("field_write", "entity"),
            ("fact_write", "entity"),
            ("fact_write", "derived_from"),
            ("fact_event_metadata", "fact_home"),
            ("fact_event_ref", "fact_home"),
            ("entity_alias", "entity"),
        ] {
            let hit: Option<i64> =
                sqlx::query_scalar(&format!("SELECT 1 FROM {table} WHERE {column} = ? LIMIT 1"))
                    .bind(handle)
                    .fetch_optional(pool)
                    .await
                    .map_err(store)?;
            if hit.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Every entity, whole — what the write guard screens against.
    ///
    /// **The whole roster, because the guard's answer is a function of all of
    /// it**: what is near a handle cannot be decided from a subset, and a
    /// screen over half the index is a screen that reports a free name as free
    /// when it is not.
    /// **What EXISTS, as a guard has to see it**: the rows the store holds,
    /// plus what the build supplies over it.
    ///
    /// A read resolves a supplied record in the layer above, and a guard that
    /// consulted only the rows refused a claim pointing at one — the two halves
    /// disagreeing about what exists, with the guard as the half that fails
    /// silently. **A stored row wins**, so nothing the operator wrote is
    /// shadowed by what the build ships.
    ///
    /// ⭐ **The READS ask it too, not only the guards.** A claim may be written
    /// on a supplied record, because the write path's gate already reads this
    /// set — so a read gated on the rows alone answered as if that claim did
    /// not exist, over a store holding its rows.
    async fn known(&self, tx: &mut Transaction<'_, MySql>) -> Result<Vec<Entity>, MemoryError> {
        let rows = Self::index(tx).await?;
        Ok(self.extend_with_supplied(rows))
    }

    /// Every rename event, inside the transaction a caller is already in.
    async fn former_handles_in(
        tx: &mut Transaction<'_, MySql>,
    ) -> Result<Vec<FormerHandle>, MemoryError> {
        let rows = sqlx::query(
            "SELECT former_handle, badge, changed_at FROM entity_former_handle ORDER BY former_handle",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let former = EntityId(row.try_get::<String, _>("former_handle").map_err(store)?);
            let badge = row.try_get::<String, _>("badge").map_err(store)?;
            let changed_at: Date = row
                .try_get::<String, _>("changed_at")
                .map_err(store)?
                .parse()
                .map_err(|_| {
                    unreadable("a former handle's changed-on day cannot be read as a date")
                })?;
            out.push(FormerHandle {
                former,
                badge,
                changed_at,
            });
        }
        Ok(out)
    }

    /// **What a handle is stored as, and what it is called today, together**
    /// — the pair every write and every miss-report needs. Mirrors the fake's
    /// `resolve`, over a real store's own rows and its own rename history.
    async fn resolve(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &EntityId,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        let known = self.known(tx).await?;
        let former = Self::former_handles_in(tx).await?;
        Ok(
            jojobot_domain::memory::resolve_handle(id, &known, &former).map(|entity| {
                let key = match &entity.badge {
                    Some(badge) => EntityId(badge.clone()),
                    None => entity.id.clone(),
                };
                (key, entity.id.clone())
            }),
        )
    }

    /// **The other direction**: a stored key read back as whatever handle it
    /// answers to today, or kept as written when it wears nobody's badge.
    async fn current_handle(
        &self,
        tx: &mut Transaction<'_, MySql>,
        stored: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        let known = self.known(tx).await?;
        Ok(
            match jojobot_domain::memory::entity_wearing(stored.as_str(), &known) {
                Some(entity) => entity.id.clone(),
                None => stored.clone(),
            },
        )
    }

    /// **Rows plus what the build supplies, over rows already read.** Split out
    /// of [`Self::known`] so a caller that needs the rows on their own — to
    /// find the one row a write targets, never a supplied record that is not
    /// one — can still build the wider set to screen against, without a second
    /// query for the same rows.
    fn extend_with_supplied(&self, mut rows: Vec<Entity>) -> Vec<Entity> {
        for (entity, _) in self.supplied.records() {
            if !rows.iter().any(|held| held.id == entity.id) {
                rows.push(entity.clone());
            }
        }
        rows
    }

    async fn index(tx: &mut Transaction<'_, MySql>) -> Result<Vec<Entity>, MemoryError> {
        let rows = sqlx::query(
            "SELECT id, kind, name, source, crm, parent, boot, merged_into, badge
             FROM entity ORDER BY id",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let aliases =
            sqlx::query("SELECT entity, alias FROM entity_alias ORDER BY entity, ordinal")
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?;
        let mut entities = Vec::with_capacity(rows.len());
        for row in &rows {
            let id = EntityId(row.try_get::<String, _>("id").map_err(store)?);
            // **Joined on the badge, falling back to the handle for a row this
            // build has not badged yet** — the same tolerant join every other
            // badge-keyed lookup here makes, so an entity still waiting on
            // `badge_the_unbadged` reads its aliases exactly as it always
            // did, and one already badged reads them by the key its rows
            // actually carry.
            let key = row
                .try_get::<Option<String>, _>("badge")
                .map_err(store)?
                .unwrap_or_else(|| id.0.clone());
            let mine = aliases
                .iter()
                .filter(|a| a.get::<String, _>("entity") == key)
                .map(|a| a.get::<String, _>("alias"))
                .collect();
            entities.push(entity_from(row, mine)?);
        }
        // **A parent pointer is resolved on the way out, like an edge or a
        // ref — never badge-keyed.** It names a DIFFERENT row, not this one's
        // own key, and nothing here queries by it: `children` reads every
        // entity and filters in memory, so a rename leaves nothing for a
        // stored badge to protect. Resolved against a snapshot taken before
        // the loop mutates, since `resolve_handle` needs the whole set to
        // search and a row cannot lend itself out while it is being written.
        let former = Self::former_handles_in(tx).await?;
        let snapshot = entities.clone();
        for entity in &mut entities {
            if let Some(parent) = &entity.parent
                && let Some(resolved) =
                    jojobot_domain::memory::resolve_handle(parent, &snapshot, &former)
            {
                entity.parent = Some(resolved.id.clone());
            }
        }
        Ok(entities)
    }

    /// Every fact naming this entity — which is the whole of what `recall`
    /// answers for, and the whole of what a scan of it holds.
    ///
    /// **One question, where the document store asked two.** There a row was
    /// reachable through the page it sat on AND through the subject cell it
    /// carried, because those could disagree; here they are one column, so
    /// "filed here" and "about this" are the same query rather than two that
    /// have to be kept in step.
    async fn facts_of(
        &self,
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
    ) -> Result<Vec<Fact>, MemoryError> {
        self.facts_projected(tx, entity).await
    }

    /// **The same claims, projected from the substrate rather than read off
    /// the row** — the newest write of each claim on this thing.
    ///
    /// ⭐ **This is what a claim read answers from.** The claim's own row is
    /// still written and still current, but nothing reads it: what a caller
    /// gets is the newest write, which is the same value by construction and
    /// is the one the rulings say a fact IS.
    ///
    /// **The newest write IS the claim.** There is no fold to do beyond that: a
    /// key's writes are combined because a thing is described a piece at a
    /// time, and a claim is not — each write says the whole of what the claim
    /// is, so the last one said is what it says.
    ///
    /// The write table names its key `fact_id`, so it is aliased to what
    /// [`Self::assemble`] reads. **One row shape, one assembler**, rather than
    /// a second one that could drift from it.
    /// **`entity` must already be a storage key** — a badge, or an unrenamed
    /// handle stored as itself — never a raw handle a caller sent. A caller
    /// resolves it once, at the top of its own write or read, exactly as
    /// every other lookup here does.
    async fn facts_projected(
        &self,
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
    ) -> Result<Vec<Fact>, MemoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {FACT_WRITE_COLUMNS} FROM fact_write w
             JOIN (SELECT entity, fact_id, MAX(ordinal) AS newest FROM fact_write
                   WHERE entity = ? GROUP BY entity, fact_id) n
               ON n.entity = w.entity AND n.fact_id = w.fact_id AND n.newest = w.ordinal
             ORDER BY w.fact_id"
        ))
        .bind(entity.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        self.assemble(tx, &rows).await
    }

    /// One addressed claim, projected from the substrate. **`address.home`
    /// must already be a storage key**, for the same reason `facts_projected`
    /// needs one.
    async fn fact_projected(
        &self,
        tx: &mut Transaction<'_, MySql>,
        address: &FactAddress,
    ) -> Result<Option<Fact>, MemoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {FACT_WRITE_COLUMNS} FROM fact_write w
             WHERE w.entity = ? AND w.fact_id = ?
               AND w.ordinal = (SELECT MAX(ordinal) FROM fact_write
                                WHERE entity = w.entity AND fact_id = w.fact_id)"
        ))
        .bind(address.home.as_str())
        .bind(address.local.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        Ok(self.assemble(tx, &rows).await?.pop())
    }

    /// One addressed fact, or nothing. **`address.home` must already be a
    /// storage key.**
    async fn read_fact(
        &self,
        tx: &mut Transaction<'_, MySql>,
        address: &FactAddress,
    ) -> Result<Option<Fact>, MemoryError> {
        self.fact_projected(tx, address).await
    }

    /// Rows into facts, each with its fields and references read back beside
    /// it.
    ///
    /// **They are read per fact rather than joined**, because a fact carrying
    /// no field must come back with an empty bag rather than with a row of
    /// NULLs a join invents for it.
    async fn assemble(
        &self,
        tx: &mut Transaction<'_, MySql>,
        rows: &[sqlx::mysql::MySqlRow],
    ) -> Result<Vec<Fact>, MemoryError> {
        let mut facts = Vec::with_capacity(rows.len());
        for row in rows {
            let entity = EntityId(row.try_get::<String, _>("entity").map_err(store)?);
            let id = FactId(row.try_get::<String, _>("id").map_err(store)?);
            let fields = Self::fields_of(tx, &entity, &id).await?;
            let refs = sqlx::query(
                "SELECT entity FROM fact_event_ref
                 WHERE fact_home = ? AND fact_id = ? ORDER BY ordinal",
            )
            .bind(entity.as_str())
            .bind(id.as_str())
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?
            .iter()
            .map(|r| EntityId(r.get::<String, _>("entity")))
            .collect();
            // **Served under the handle this storage key answers to today**
            // — `entity` here is the badge (or an unrenamed handle stored as
            // itself), never what a reader is shown.
            let handle = self.current_handle(tx, &entity).await?;
            let raw = fact_from(row, entity, id, fields, refs)?;
            let mut served = raw;
            served.home = handle.clone();
            served.subject = handle;
            if let Some(source) = &served.derived_from {
                let resolved = self.current_handle(tx, &source.home).await?;
                served.derived_from = Some(FactAddress::new(resolved, source.local.clone()));
            }
            facts.push(served);
        }
        Ok(facts)
    }

    /// **A record's fields, projected from the writes it made.**
    ///
    /// One value per key: the newest write of that key inside this record. A
    /// key whose newest write here took it off is not on the record — the write
    /// stays where it is, and the key stops being current.
    async fn fields_of(
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
        fact: &FactId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let rows = sqlx::query(
            "SELECT `key`, value FROM field_write
             WHERE entity = ? AND fact_id = ? ORDER BY `key`, ordinal",
        )
        .bind(entity.as_str())
        .bind(fact.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut fields = std::collections::BTreeMap::new();
        for row in &rows {
            let key: String = row.try_get("key").map_err(store)?;
            match row.try_get::<Option<String>, _>("value").map_err(store)? {
                Some(value) => fields.insert(key, value),
                None => fields.remove(&key),
            };
        }
        Ok(fields)
    }

    /// **Every write on this thing, each with the standing of the record that
    /// carried it.**
    ///
    /// The substrate under [`Self::held_by`], read whole because that is what a
    /// write about to change a record's standing has to be weighed against. The
    /// status is joined in rather than assumed: a write inside a record
    /// somebody took back still happened and no longer counts.
    async fn writes_on(
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
    ) -> Result<Vec<KeyWrite>, MemoryError> {
        let rows = sqlx::query(
            "SELECT w.`key`, w.ordinal, w.value, w.fact_id, f.status, f.provenance, f.standing,
                     f.details
             FROM field_write w
             JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id
             WHERE w.entity = ?",
        )
        .bind(entity.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut writes = Vec::with_capacity(rows.len());
        for row in &rows {
            writes.push(KeyWrite {
                key: row.try_get("key").map_err(store)?,
                ordinal: row.try_get::<i64, _>("ordinal").map_err(store)? as u64,
                value: row.try_get("value").map_err(store)?,
                fact: FactId(row.try_get::<String, _>("fact_id").map_err(store)?),
                // Read as tolerantly as a record's own status is read, and for
                // the same reason: a token this build does not know must not
                // make a thing unreadable.
                status: FactStatus::from_token(&row.try_get::<String, _>("status").map_err(store)?),
                // **Who backs the claim this write came from**, off the row the
                // status already comes from: a folded value that dropped this
                // handed back the user's own word and an assistant's guess in
                // the same shape.
                provenance: Provenance::from_token(
                    &row.try_get::<String, _>("provenance").map_err(store)?,
                ),
                standing: Standing::parse(
                    row.try_get::<Option<String>, _>("standing")
                        .map_err(store)?
                        .as_deref()
                        .unwrap_or(""),
                    Provenance::from_token(&row.try_get::<String, _>("provenance").map_err(store)?),
                ),
                // **The note off the same row the standing comes from.** A
                // folded value that dropped it hands back an estimate and the
                // sentence saying it is an estimate stays on a record nothing
                // points at.
                note: row.try_get::<Option<String>, _>("details").map_err(store)?,
            });
        }
        Ok(writes)
    }

    /// **A thing's fields, projected from every write on it.**
    ///
    /// The other read of this table: [`Self::fields_of`] asks what one record
    /// says, this asks what the thing holds — one value per key, the newest
    /// write of that key on this thing, wherever it landed. Which of them wins
    /// is decided by [`folded_fields`], the one fold both stores run.
    async fn held_by(
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let writes = Self::writes_on(tx, entity).await?;
        // Read inside the same transaction the writes are, because a key's fold
        // is part of what its writes come to and a roster read beside the write
        // is one that may already have moved.
        let declared = Self::types_in(tx).await?;
        Ok(folded_fields(&writes, &declared))
    }

    /// **Append what a write said about a record's keys**, each row taking the
    /// next ordinal for its own (thing, key).
    ///
    /// The ordinal is counted inside the transaction that writes it, so two
    /// writes of one key cannot come to share a place in its history. A value
    /// of `None` is a clear, and it is stored rather than acted on: nothing is
    /// removed from this table.
    async fn append_writes(
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
        fact: &FactId,
        wrote: Vec<(String, Option<String>)>,
    ) -> Result<(), MemoryError> {
        for (key, value) in wrote {
            let highest: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(ordinal) FROM field_write WHERE entity = ? AND `key` = ?",
            )
            .bind(entity.as_str())
            .bind(&key)
            .fetch_one(&mut **tx)
            .await
            .map_err(store)?;
            sqlx::query(
                "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(entity.as_str())
            .bind(&key)
            .bind(highest.unwrap_or(0) + 1)
            .bind(value.as_deref())
            .bind(fact.as_str())
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        }
        Ok(())
    }

    /// Write one whole fact — the row and the references under it — replacing
    /// whatever was there under the same address.
    ///
    /// **The fields are not written here.** They are writes, and a write is
    /// appended by the verb that made it: this rewrites the claim, which is
    /// what edit-in-place means at the surface, and the substrate under the
    /// keys is untouched by it.
    ///
    /// **The `event_kind` column is not written.** It held the free-text label
    /// that said what class a record was, and there are no classes: a record is
    /// its fields. The column stays in the table because dropping it is a
    /// migration and this writes NULL into it either way.
    ///
    /// **One writer for every verb that produces a fact**, so a capture, an
    /// edit and a retraction cannot come to write a record three different
    /// ways.
    async fn write_fact(
        tx: &mut Transaction<'_, MySql>,
        fact: &Fact,
        clock: &Clock,
    ) -> Result<(), MemoryError> {
        sqlx::query(
            "REPLACE INTO fact (entity, id, content, details, provenance, standing, status,
                                recorded_at, happened_at, edge_shape, edge_object, derived_from,
                                derived_from_id, inserted_at, stale_after)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(fact.home.as_str())
        .bind(fact.id.as_str())
        .bind(&fact.content)
        .bind(fact.details.as_deref())
        .bind(fact.provenance.as_token())
        .bind(fact.standing.as_token())
        .bind(fact.status.as_token())
        .bind(fact.recorded_at.to_string())
        .bind(fact.happened_at.map(|d| d.to_string()))
        .bind(fact.edge.as_ref().map(|e| e.shape.as_token()))
        .bind(fact.edge.as_ref().map(|e| e.object.as_str()))
        .bind(fact.derived_from.as_ref().map(|d| d.home.as_str()))
        .bind(fact.derived_from.as_ref().map(|d| d.local.as_str()))
        // **Carried, never re-stamped.** One writer serves the capture, the
        // edit and the retraction, and only the first of those is the moment
        // this store took the record in: an edit that stamped again would say
        // jojobot learned the claim when somebody corrected its wording.
        .bind(fact.inserted_at.map(|at| at.to_string()))
        .bind(fact.stale_after.map(|day| day.to_string()))
        .execute(&mut **tx)
        .await
        .map_err(store)?;

        sqlx::query("DELETE FROM fact_event_ref WHERE fact_home = ? AND fact_id = ?")
            .bind(fact.home.as_str())
            .bind(fact.id.as_str())
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        for (ordinal, object) in fact.refs.iter().enumerate() {
            sqlx::query(
                "INSERT INTO fact_event_ref (fact_home, fact_id, ordinal, entity)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(fact.home.as_str())
            .bind(fact.id.as_str())
            .bind(ordinal as i64 + 1)
            .bind(object.as_str())
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        }
        Self::append_fact_write(tx, fact, clock).await?;
        Ok(())
    }

    /// **Keep this write of the claim, beside the row it just rewrote.**
    ///
    /// The row above is the claim as it now stands; this is the claim as it now
    /// stands KEPT, in the order its writes happened. A correction overwrote the
    /// row and left nothing behind, so a session could not tell a claim nobody
    /// ever made from one somebody made and corrected.
    ///
    /// **The whole claim, not its content and edge.** The fold that projects a
    /// thing's fields keeps only writes whose record is ACTIVE, so a status
    /// sitting in a column above a versioned content would be a filter reading
    /// the value it is meant to be deciding. Everything is versioned or nothing
    /// is (rule 201).
    ///
    /// **The write is stamped with the moment IT happened**, beside the claim's
    /// own first-recorded moment which it carries unchanged. A claim taken in
    /// last April and corrected in September has one of the first and two of
    /// the second, and a row copying the claim's would report both corrections
    /// at one instant.
    async fn append_fact_write(
        tx: &mut Transaction<'_, MySql>,
        fact: &Fact,
        clock: &Clock,
    ) -> Result<(), MemoryError> {
        let highest: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(ordinal) FROM fact_write WHERE entity = ? AND fact_id = ?",
        )
        .bind(fact.home.as_str())
        .bind(fact.id.as_str())
        .fetch_one(&mut **tx)
        .await
        .map_err(store)?;
        sqlx::query(
            "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                     standing, status, recorded_at, happened_at, edge_shape,
                                     edge_object,
                                     derived_from, derived_from_id, inserted_at, stale_after,
                                     written_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(fact.home.as_str())
        .bind(fact.id.as_str())
        .bind(highest.unwrap_or(0) + 1)
        .bind(&fact.content)
        .bind(fact.details.as_deref())
        .bind(fact.provenance.as_token())
        .bind(fact.standing.as_token())
        .bind(fact.status.as_token())
        .bind(fact.recorded_at.to_string())
        .bind(fact.happened_at.map(|d| d.to_string()))
        .bind(fact.edge.as_ref().map(|e| e.shape.as_token()))
        .bind(fact.edge.as_ref().map(|e| e.object.as_str()))
        .bind(fact.derived_from.as_ref().map(|d| d.home.as_str()))
        .bind(fact.derived_from.as_ref().map(|d| d.local.as_str()))
        .bind(fact.inserted_at.map(|t| t.to_string()))
        .bind(fact.stale_after.map(|d| d.to_string()))
        // **Stamped here and nowhere else.** The claim's own moment is carried
        // above, never re-stamped; this is the moment this write happened —
        // on the server's own clock, which is not the wall clock on an
        // instance acting out a day.
        .bind(clock.now().to_string())
        .execute(&mut **tx)
        .await
        .map_err(store)?;
        Ok(())
    }

    /// The next local id on this page: `f` and the highest number already
    /// there, plus one.
    ///
    /// **Counted rather than drawn, and that is the model being ported.** The
    /// shared contract pins the first claim on an entity at `f1` and calls that
    /// address the thing a link carries, so a drawn id here would not be this
    /// store answering the specification differently — it would be this store
    /// failing it. Every other id jojobot mints is drawn; this one is what the
    /// model says it is until the model says otherwise.
    ///
    /// **Free within its home**, which is the whole of this table's key: an
    /// address is the pair, so two pages may hold the same local id without
    /// either being reachable through the other.
    async fn mint(tx: &mut Transaction<'_, MySql>, home: &EntityId) -> Result<FactId, MemoryError> {
        let taken: Vec<String> = sqlx::query_scalar("SELECT id FROM fact WHERE entity = ?")
            .bind(home.as_str())
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
        let highest = taken
            .iter()
            .filter_map(|id| id.strip_prefix('f')?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        Ok(FactId(format!("f{}", highest + 1)))
    }

    /// The declared types, read inside the transaction that is about to write.
    ///
    /// **Inside it rather than beside it**, because a guard that read the
    /// declarations outside the write is a guard deciding against a roster that
    /// may already have moved.
    async fn types_in(tx: &mut Transaction<'_, MySql>) -> Result<Vec<DeclaredType>, MemoryError> {
        let rows = sqlx::query(
            "SELECT type_name, key_name, holds, folds, origin, required, one_of FROM type_field
             ORDER BY type_name, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        Ok(gather_types(&rows))
    }

    /// **The keys a KIND names, read as the kind's own.**
    ///
    /// The two halves share one table and are told apart by an owner column.
    /// [`Self::types_in`] reads both, which is right for the fold — how a key
    /// folds is declared by whoever declared it — and wrong for the fit guard,
    /// which selects a declaration whose NAME matches the thing's kind. Handed
    /// the mixed list, that guard read a caller's type named `place` as the
    /// kind `place`'s schema and gated every write to every place with it.
    ///
    /// **So the kind's keys arrive as the kind's**, rather than being found by
    /// name in a list holding both halves.
    async fn kind_keys_in(
        tx: &mut Transaction<'_, MySql>,
        kind: &str,
    ) -> Result<Vec<DeclaredType>, MemoryError> {
        let rows = sqlx::query(
            "SELECT type_name, key_name, holds, folds, origin, required, one_of FROM type_field
             WHERE type_name = ? AND owner = ? ORDER BY ordinal",
        )
        .bind(kind)
        .bind(KEYS_OF_A_KIND)
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        Ok(gather_types(&rows))
    }

    /// The addresses a page already holds, which is what a fact miss carries so
    /// a caller can see what it might have meant.
    async fn addresses_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
        home: &EntityId,
    ) -> Result<Vec<String>, MemoryError> {
        Ok(self
            .facts_of(tx, home)
            .await?
            .iter()
            .map(|f| f.address().to_string())
            .collect())
    }
}

/// **The writes a new record makes: every key it carries.** A capture and a
/// retraction both land a record whose keys have never been written before, so
/// each one is the first write of its key on that thing.
fn written_keys(fact: &Fact) -> Vec<(String, Option<String>)> {
    fact.fields
        .iter()
        .map(|(key, value)| (key.clone(), Some(value.clone())))
        .collect()
}

/// The columns a fact reads back from, in one place so every read takes the
/// same ones.
const FACT_COLUMNS: &str = "entity, id, content, details, provenance, standing, status, \
                            recorded_at, happened_at, edge_shape, edge_object, derived_from, derived_from_id, \
                            inserted_at, stale_after";

/// The same columns off the write table, with its key aliased to what
/// [`DoltMemory::assemble`] reads. **The alias is the whole difference**: a
/// second assembler would be a second place for the row shape to drift.
const FACT_WRITE_COLUMNS: &str = "w.entity, w.fact_id AS id, w.content, w.details, w.provenance, \
                                  w.standing, w.status, w.recorded_at, w.happened_at, \
                                  w.edge_shape, \
                                  w.edge_object, w.derived_from, w.derived_from_id, \
                                  w.inserted_at, w.stale_after";

/// A store failure, in the domain's own words. **The server's account never
/// crosses** — no SQL, no table names, no product (rule 53); it goes to the log
/// where an operator debugging a real failure wants it.
fn store(e: sqlx::Error) -> MemoryError {
    tracing::error!(error = %e, "the memory store failed");
    MemoryError::Store("the memory store could not be reached".into())
}

/// A row jojobot cannot read as the record it must be. **Its own failure rather
/// than a guess**: a kind read as some default files an entity under a handle
/// nobody wrote.
fn unreadable(what: &str) -> MemoryError {
    MemoryError::Store(format!("a stored record could not be read: {what}"))
}

/// Where a stored type came from.
///
/// **A token nothing can read is treated as shipped**, which is the safe
/// branch here (rule 62). The two mistakes are not equal: reading a shipped
/// type as a caller's lets the next declaration overwrite it and nobody sees
/// it happen, while reading a caller's type as shipped refuses one write and
/// tells the caller exactly what to do instead. This is the one place `holds`
/// takes the other branch, and for the same reason — there, the permissive
/// answer is the one that loses nothing.
fn read_origin(token: &str) -> Origin {
    Origin::of_token(token).unwrap_or(Origin::Shipped)
}

fn entity_from(row: &sqlx::mysql::MySqlRow, aliases: Vec<String>) -> Result<Entity, MemoryError> {
    let id = EntityId(row.try_get::<String, _>("id").map_err(store)?);
    // **The two ways a stored handle fails to name a kind are not one failure**
    // (rule 68). A process that loaded no set cannot read the most ordinary
    // handle in the store, and reporting that as a damaged record sends a
    // reader after damage that is not there — and names a repair only a person
    // can perform, while the repair is a boot. So the never-loaded answer is
    // the same refusal the write half of this rail gives, in the same words.
    let kind = kinds::resolve(id.kind_token()).map_err(|why| match why {
        NotAKind::SetNeverLoaded => MemoryError::KindsNeverLoaded {
            attempted: Some(id.to_string()),
        },
        // A row whose kind nobody declares really is a record this process
        // cannot read, and it stays that. The sentence names no kinds: the set
        // is data (rule 213).
        NotAKind::NotDeclared { .. } => unreadable("its handle names no kind"),
    })?;
    Ok(Entity {
        kind,
        id,
        name: row.try_get::<String, _>("name").map_err(store)?,
        aliases,
        source: row.try_get::<String, _>("source").map_err(store)?,
        crm: row.try_get::<Option<String>, _>("crm").map_err(store)?,
        parent: row
            .try_get::<Option<String>, _>("parent")
            .map_err(store)?
            .map(EntityId),
        boot: jojobot_domain::memory::Boot::from_token(
            &row.try_get::<String, _>("boot").map_err(store)?,
        ),
        merged_into: row
            .try_get::<Option<String>, _>("merged_into")
            .map_err(store)?
            .map(EntityId),
        // **Read, never written from here.** A badge is minted by
        // `write_entity` and carried across every rewrite; this is the read
        // that lets the layer above resolve a mention both ways.
        badge: row.try_get::<Option<String>, _>("badge").map_err(store)?,
    })
}

fn fact_from(
    row: &sqlx::mysql::MySqlRow,
    entity: EntityId,
    id: FactId,
    fields: std::collections::BTreeMap<String, String>,
    refs: Vec<EntityId>,
) -> Result<Fact, MemoryError> {
    let provenance =
        Provenance::from_token(&row.try_get::<String, _>("provenance").map_err(store)?);
    // **NULL is "nobody declared one", and the reader derives it.** Storing the
    // derived value would make a standing somebody asserted and one nobody did
    // indistinguishable on the way back out.
    let standing = match row
        .try_get::<Option<String>, _>("standing")
        .map_err(store)?
    {
        Some(token) => Standing::parse(&token, provenance),
        None => Standing::parse("", provenance),
    };
    // **Read as tolerantly as the domain reads it**, retired spellings and all:
    // the status is a token, `from_token` is total, and a store that refused a
    // token this build does not know would refuse a row somebody wrote.
    let status = FactStatus::from_token(&row.try_get::<String, _>("status").map_err(store)?);
    let recorded_at: Date = row
        .try_get::<String, _>("recorded_at")
        .map_err(store)?
        .parse()
        .map_err(|_| unreadable("its recorded-on day cannot be read as a date"))?;
    let edge = match (
        row.try_get::<Option<String>, _>("edge_shape")
            .map_err(store)?,
        row.try_get::<Option<String>, _>("edge_object")
            .map_err(store)?,
    ) {
        (Some(shape), Some(object)) => EdgeShape::from_token(&shape).map(|shape| Edge {
            shape,
            object: EntityId(object),
        }),
        _ => None,
    };
    let derived_from = match (
        row.try_get::<Option<String>, _>("derived_from")
            .map_err(store)?,
        row.try_get::<Option<String>, _>("derived_from_id")
            .map_err(store)?,
    ) {
        (Some(home), Some(local)) => Some(FactAddress {
            home: EntityId(home),
            local: FactId(local),
        }),
        _ => None,
    };
    Ok(Fact {
        id,
        // **One column, read into both fields.** The record above this adapter
        // still has a home and a subject; here they are the same value, so
        // nothing that reads a `Fact` has to know the difference has gone.
        home: entity.clone(),
        subject: entity,
        content: row.try_get::<String, _>("content").map_err(store)?,
        details: row.try_get::<Option<String>, _>("details").map_err(store)?,
        provenance,
        standing,
        status,
        recorded_at,
        // **A day nothing can read is a day nobody set.** The claim then says
        // nothing about when the thing happened, which is the honest reading
        // and the one this column exists to make possible.
        happened_at: row
            .try_get::<Option<String>, _>("happened_at")
            .map_err(store)?
            .and_then(|day| day.parse().ok()),
        edge,
        fields,
        refs,
        derived_from,
        // **NULL is a row written before the store recorded this**, and it
        // reads back as nothing rather than as a guess.
        inserted_at: row
            .try_get::<Option<String>, _>("inserted_at")
            .map_err(store)?
            .and_then(|stamp| stamp.parse().ok()),
        // A day nothing can read is a day nobody set: the claim is ordinary
        // rather than stale, which is the safe branch for a value that only
        // ever makes a reader distrust something.
        stale_after: row
            .try_get::<Option<String>, _>("stale_after")
            .map_err(store)?
            .and_then(|day| day.parse().ok()),
    })
}

#[async_trait]
impl Memory for DoltMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        validate_entity(
            &new.id,
            &new.name,
            &new.aliases,
            &new.source,
            new.crm.as_deref(),
            new.parent.as_ref(),
        )?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = self.known(&mut tx).await?;
        if let guard::Decision::Block(candidates) = guard::decide(
            &new.id,
            &new.labels(),
            &index,
            new.override_token.as_deref(),
        ) {
            return Ok(Guarded::Blocked {
                attempted: new.id,
                candidates,
            });
        }
        let entity = Entity {
            kind: new.id.kind().expect("a validated id has a kind"),
            id: new.id,
            name: new.name.trim().to_string(),
            aliases: new.aliases.iter().map(|a| a.trim().to_string()).collect(),
            source: new.source.trim().to_string(),
            crm: new.crm.map(|c| c.trim().to_string()),
            parent: new.parent,
            boot: new.boot,
            merged_into: None,
            badge: None,
        };
        // The entity this one sits under must already exist, and must not be
        // this one. Screened after the record is assembled because a
        // self-parenting block reports the write itself.
        if let Some(parent) = &entity.parent
            && let guard::Decision::Block(candidates) =
                guard::decide_parent(&entity, parent, &index)
        {
            return Ok(Guarded::Blocked {
                attempted: parent.clone(),
                candidates,
            });
        }
        let badge = write_entity(&mut tx, &self.draw, &entity).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(Entity {
            badge: Some(badge),
            ..entity
        }))
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let all = Self::index(&mut tx).await?;
        tx.commit().await.map_err(store)?;
        Ok(all
            .into_iter()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .collect())
    }

    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let out = Self::former_handles_in(&mut tx).await?;
        tx.commit().await.map_err(store)?;
        Ok(out)
    }

    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        validate_write_subject(handle)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **A row, never a supplied record.** `update_entity` mutates a stored
        // row — there is no row to mutate for a record the build supplies, and
        // finding one here would let an edit turn a shipped record into a
        // stored one wearing its handle, which is exactly what a caller must
        // not be able to do (rule 234's own exception).
        let rows = Self::index(&mut tx).await?;
        let Some(mut entity) = rows.iter().find(|e| &e.id == handle).cloned() else {
            return Err(MemoryError::UnknownEntity {
                attempted: handle.to_string(),
                nearest: guard::screen(handle, &[], &rows),
            });
        };
        // **The screen alone reads the wider set** — rows plus what the build
        // supplies — so a rename that collides with a supplied record is
        // caught exactly as one against a stored record is.
        let known = self.extend_with_supplied(rows);
        // Changing what an entity is CALLED is an entity-touching write, so it
        // faces the same gate — display name and aliases alike.
        if let guard::Decision::Block(candidates) = screen_entity_patch(&entity, &patch, &known) {
            return Ok(Guarded::Blocked {
                attempted: handle.clone(),
                candidates,
            });
        }
        apply_entity_patch(&mut entity, &patch)?;
        // **The badge rides on the record this edit was read from**, and
        // `write_entity` carries the row's own across — so the answer already
        // says what the row says and nothing has to put it back.
        write_entity(&mut tx, &self.draw, &entity).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(entity))
    }

    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        override_token: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        validate_write_subject(from)?;
        if from == to {
            return Err(MemoryError::NothingToRename {
                attempted: from.to_string(),
            });
        }
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **A row, never a supplied record** — the same exception
        // `update_entity` reads off: a rename mutates a stored row, and a
        // build-shipped record has none to mutate.
        let rows = Self::index(&mut tx).await?;
        let known = self.extend_with_supplied(rows.clone());
        let Some(entity) = rows.iter().find(|e| &e.id == from).cloned() else {
            // **A supplied record is real and still not a row** — checked
            // before `resolve_handle`, whose own direct-match branch reads
            // the wider `known` set and would otherwise report this exact
            // case as a rename that moved the handle to itself.
            if self.supplied.record_for(from).is_some() {
                return Err(MemoryError::SuppliedHandle {
                    attempted: from.to_string(),
                });
            }
            // **A stale-but-renamed FROM still resolves**, through the wider
            // set, so the caller is told where the thing went rather than
            // met with a miss indistinguishable from a handle that never
            // existed.
            let former = Self::former_handles_in(&mut tx).await?;
            if let Some(moved) = jojobot_domain::memory::resolve_handle(from, &known, &former) {
                return Err(MemoryError::HandleMoved {
                    attempted: from.to_string(),
                    now: moved.id.to_string(),
                });
            }
            return Err(MemoryError::UnknownEntity {
                attempted: from.to_string(),
                nearest: guard::screen(from, &[], &known),
            });
        };
        // A forwarding row is not a thing to rename, for the same reason it
        // is not a side to fold or a survivor to fold into.
        if let Some(into) = &entity.merged_into {
            return Err(MemoryError::AlreadyMerged {
                attempted: from.to_string(),
                into: into.to_string(),
            });
        }
        let effective_parent = parent.clone().or_else(|| entity.parent.clone());
        validate_entity(
            to,
            &entity.name,
            &entity.aliases,
            &entity.source,
            entity.crm.as_deref(),
            effective_parent.as_ref(),
        )?;
        let renamed = Entity {
            id: to.clone(),
            kind: to.kind().expect("validated above"),
            parent: effective_parent,
            ..entity.clone()
        };
        // Screened like a creation, with the thing's own row excluded — or
        // it would collide with its own former name the moment the new one
        // is close to it.
        let others: Vec<Entity> = known.into_iter().filter(|e| &e.id != from).collect();
        if let guard::Decision::Block(candidates) =
            guard::decide(to, &renamed.labels(), &others, override_token)
        {
            return Ok(Guarded::Blocked {
                attempted: to.clone(),
                candidates,
            });
        }
        if let Some(new_parent) = &parent
            && let guard::Decision::Block(candidates) =
                guard::decide_parent(&renamed, new_parent, &others)
        {
            return Ok(Guarded::Blocked {
                attempted: new_parent.clone(),
                candidates,
            });
        }

        // **A straight primary-key rewrite, never delete-then-insert.**
        // Every other column — the badge above all — rides along untouched:
        // nothing here asks "what was this row's badge" the way
        // `write_entity`'s REPLACE would if handed a row that already
        // carries the new id, which is exactly the shape that would mint a
        // fresh one and sever it from everything the old one wore.
        sqlx::query("UPDATE entity SET id = ?, kind = ?, parent = ? WHERE id = ?")
            .bind(to.as_str())
            .bind(to.kind_token())
            .bind(renamed.parent.as_ref().map(EntityId::as_str))
            .bind(from.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        // Every entity naming the OLD handle as its parent follows to the
        // new one — the same statement shape `merge` already uses on this
        // column.
        sqlx::query("UPDATE entity SET parent = ? WHERE parent = ?")
            .bind(to.as_str())
            .bind(from.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        sqlx::query(
            "INSERT INTO entity_former_handle (former_handle, badge, changed_at) VALUES \
             (?, ?, ?)",
        )
        .bind(from.as_str())
        .bind(
            entity
                .badge
                .as_deref()
                .expect("a written row wears a badge"),
        )
        .bind(date.to_string())
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(renamed))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        validate_write_subject(&fact.subject)?;
        validate_content(&fact.content)?;
        validate_details(fact.details.as_deref())?;
        if let Some(edge) = &fact.edge {
            validate_edge(edge)?;
        }
        validate_fields(&fact.fields)?;
        validate_provenance_source(fact.provenance, &fact.fields)?;
        let standing = standing_of(&fact);

        let mut tx = self.pool.begin().await.map_err(store)?;
        // **What EXISTS**, which is the rows plus what the build supplies —
        // the same set the creation screen reads (rule 234).
        let index = self.known(&mut tx).await?;
        // **A stale-but-renamed subject exists too.** The direct check is
        // what the guard already asks; a miss on it is checked again through
        // the thing's own rename history before it is called unknown. Kept,
        // rather than re-resolved, for its storage key below.
        let subject_resolved = self.resolve(&mut tx, &fact.subject).await?;
        if subject_resolved.is_none()
            && let guard::Decision::Block(candidates) =
                guard::decide_existing(&fact.subject, &index)
        {
            return Ok(Guarded::Blocked {
                attempted: fact.subject,
                candidates,
            });
        }
        if let Some(edge) = &fact.edge
            && let guard::Decision::Block(candidates) = guard::decide_existing(&edge.object, &index)
        {
            return Ok(Guarded::Blocked {
                attempted: edge.object.clone(),
                candidates,
            });
        }
        for object in &fact.refs {
            validate_write_subject(object)?;
            if let guard::Decision::Block(candidates) = guard::decide_existing(object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object.clone(),
                    candidates,
                });
            }
        }
        // **A reference key names an entity, so it faces the same rule the
        // edge's object faces.** A walkable link into a node nobody recorded is
        // the hole rule 3 exists to close, and arriving through a key rather
        // than through an edge does not make it a different hole.
        for object in referenced_by(&fact.fields, &Self::types_in(&mut tx).await?) {
            if let guard::Decision::Block(candidates) = guard::decide_existing(&object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object,
                    candidates,
                });
            }
        }
        // A claim this one is derived from is named, so it must already exist —
        // an unknown home is an entity miss and a home holding no such row is a
        // fact miss, which are the two shapes this rail already has.
        let derived_from = if let Some(source) = &fact.derived_from {
            let Some((source_key, _)) = self.resolve(&mut tx, &source.home).await? else {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            };
            let resolved_source = FactAddress::new(source_key, source.local.clone());
            match self.read_fact(&mut tx, &resolved_source).await? {
                None => {
                    return Err(MemoryError::UnknownFact {
                        attempted: source.to_string(),
                        nearest: self.addresses_in(&mut tx, &resolved_source.home).await?,
                    });
                }
                // A withdrawn claim is still there and is no longer evidence.
                Some(held) if held.status == FactStatus::Retracted => {
                    return Err(MemoryError::SourceRetracted {
                        attempted: source.to_string(),
                    });
                }
                Some(_) => {}
            }
            Some(resolved_source)
        } else {
            None
        };

        // **What the subject is stored as** — the badge it wears, or its own
        // handle when it wears none. Already resolved above, direct or
        // through its rename history.
        let (home, subject_handle) = subject_resolved.expect("checked to exist just above");
        let id = Self::mint(&mut tx, &home).await?;
        let stored = Fact {
            id,
            home: home.clone(),
            // One column, stored into both fields — read back into both the
            // same way on every fact this store serves.
            subject: home,
            content: normalize_content(&fact.content),
            details: normalize_details(fact.details.as_deref()),
            provenance: fact.provenance,
            standing,
            status: fact.status,
            recorded_at: fact.recorded_at,
            happened_at: fact.happened_at,
            edge: fact.edge,
            fields: fact.fields,
            refs: fact.refs,
            derived_from,
            // **The store stamps it, so nothing above can.** The moment a
            // record is taken in is this one, and a caller that could name it
            // could claim jojobot knew something before it did.
            inserted_at: Some(self.clock.now()),
            stale_after: fact.stale_after,
        };
        // **A new record's keys land on the thing too**, so the same guard the
        // edit path runs applies here: a write may not drop a thing below a
        // type it already fits, by taking a key away or by putting a value in
        // one that the key does not hold. One function, called from both verbs
        // in both stores.
        let held = Self::writes_on(&mut tx, &stored.home).await?;
        let declared = Self::types_in(&mut tx).await?;
        // **The fold reads both halves and the guard reads one.** How a key
        // folds is declared by whoever declared it; what governs a thing is
        // its own kind, and nothing else. **Off the subject's own kind, never
        // off `stored.home`** — that is a badge now, and a badge carries no
        // kind token to parse.
        let subject_kind = index
            .iter()
            .find(|e| e.id == subject_handle)
            .expect("resolved above")
            .kind;
        let governs = Self::kind_keys_in(&mut tx, subject_kind.as_token()).await?;
        guard_fit(
            subject_kind.as_token(),
            &folded_fields(&held, &declared),
            &stood_after_capture(&held, &stored, &declared),
            &governs,
        )?;
        Self::write_fact(&mut tx, &stored, &self.clock).await?;
        // Every key this record carries is a write of its own, appended to the
        // history of that key on this thing.
        Self::append_writes(&mut tx, &stored.home, &stored.id, written_keys(&stored)).await?;
        // **Served under the handle, stored under the key** — resolved
        // before the commit closes the transaction this needs to do it in.
        let served_derived_from = match &stored.derived_from {
            Some(source) => Some(FactAddress::new(
                self.current_handle(&mut tx, &source.home).await?,
                source.local.clone(),
            )),
            None => None,
        };
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(Fact {
            home: subject_handle.clone(),
            subject: subject_handle,
            derived_from: served_derived_from,
            ..stored
        }))
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = self.known(&mut tx).await?;
        // An unknown entity is a miss with its near candidates — never an
        // empty page. Empty-but-real and nonexistent are different answers.
        // A stale-but-renamed handle is not this miss: it resolves through
        // its own history to the one storage key its claims were ever filed
        // under, so this is one lookup rather than a walk of every handle it
        // has worn.
        let Some((key, _)) = self.resolve(&mut tx, subject).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: subject.to_string(),
                nearest: guard::screen(subject, &[], &index),
            });
        };
        let facts = self.facts_of(&mut tx, &key).await?;
        tx.commit().await.map_err(store)?;
        Ok(facts)
    }

    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = self.known(&mut tx).await?;
        // An unknown entity is a miss with its near candidates, exactly as a
        // recall of one is: a thing nobody has written a key on and a handle
        // nobody created are different answers with different repairs. A
        // stale-but-renamed handle resolves through its own history, same as
        // every other lookup here.
        let Some((key, _)) = self.resolve(&mut tx, entity).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        let held = Self::held_by(&mut tx, &key).await?;
        tx.commit().await.map_err(store)?;
        Ok(held)
    }

    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = self.known(&mut tx).await?;
        // A miss on the HANDLE is an entity miss, exactly as every other
        // addressed read answers one. A stale-but-renamed handle resolves
        // through its own history, same as every other lookup here.
        let Some((key, _)) = self.resolve(&mut tx, &address.home).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        // **The whole chain, through the same assembler every other claim read
        // uses.** One row shape and one reader: a second would be a second
        // place for the columns to drift.
        let rows = sqlx::query(&format!(
            "SELECT w.ordinal, w.written_at, {FACT_WRITE_COLUMNS} FROM fact_write w
             WHERE w.entity = ? AND w.fact_id = ? ORDER BY w.ordinal"
        ))
        .bind(key.as_str())
        .bind(address.local.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        if rows.is_empty() {
            // **No writes is no record.** A claim that stands has at least one
            // write, and a read of one answers from the substrate — so a claim
            // with nothing behind it is a miss here for the same reason it is a
            // miss everywhere else, rather than an empty chain that would read
            // as a record saying nothing.
            let nearest = self.addresses_in(&mut tx, &key).await?;
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest,
            });
        }

        let mut history = Vec::with_capacity(rows.len());
        for row in &rows {
            let ordinal: i64 = row.try_get("ordinal").map_err(store)?;
            // **A row an older build appended carries none**, and it reads as
            // absent rather than being filled from the claim.
            let written_at = row
                .try_get::<Option<String>, _>("written_at")
                .map_err(store)?
                .and_then(|at| at.parse::<jiff::Timestamp>().ok());
            // **Fields and references are left empty rather than read.** They
            // are versioned by their own substrate and by nothing here, so
            // reading today's keys onto a write from a year ago would report
            // them as what that write said. `ClaimWrite` carries neither.
            let mut fact = fact_from(
                row,
                key.clone(),
                address.local.clone(),
                Default::default(),
                Vec::new(),
            )?;
            // **A lineage pointer in the chain is a `FactAddress` like any
            // other** — stored under its source's key, served under the
            // handle that key answers to today.
            if let Some(source) = &fact.derived_from {
                let resolved = self.current_handle(&mut tx, &source.home).await?;
                fact.derived_from = Some(FactAddress::new(resolved, source.local.clone()));
            }
            history.push(ClaimWrite::of(&fact, ordinal.max(0) as usize, written_at));
        }
        tx.commit().await.map_err(store)?;
        Ok(history)
    }

    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = self.known(&mut tx).await?;
        let Some((key, _)) = self.resolve(&mut tx, entity).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        // **One query for the whole entity**, the batched sibling of
        // claim_history's per-record read: every write of every claim this
        // entity holds, ordered so each claim's own writes stay oldest-first
        // once grouped by id below.
        let rows = sqlx::query(&format!(
            "SELECT w.ordinal, w.written_at, {FACT_WRITE_COLUMNS} FROM fact_write w
             WHERE w.entity = ? ORDER BY w.fact_id, w.ordinal"
        ))
        .bind(key.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;

        let mut histories: std::collections::HashMap<FactId, Vec<ClaimWrite>> =
            std::collections::HashMap::new();
        for row in &rows {
            let id = FactId(row.try_get::<String, _>("id").map_err(store)?);
            let ordinal: i64 = row.try_get("ordinal").map_err(store)?;
            let written_at = row
                .try_get::<Option<String>, _>("written_at")
                .map_err(store)?
                .and_then(|at| at.parse::<jiff::Timestamp>().ok());
            let mut fact = fact_from(row, key.clone(), id.clone(), Default::default(), Vec::new())?;
            if let Some(source) = &fact.derived_from {
                let resolved = self.current_handle(&mut tx, &source.home).await?;
                fact.derived_from = Some(FactAddress::new(resolved, source.local.clone()));
            }
            histories.entry(id).or_default().push(ClaimWrite::of(
                &fact,
                ordinal.max(0) as usize,
                written_at,
            ));
        }
        tx.commit().await.map_err(store)?;
        Ok(histories)
    }

    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = self.known(&mut tx).await?;
        // An unknown entity is a miss with its near candidates, exactly as a
        // recall of one is: a key nobody wrote and a handle nobody created are
        // different answers with different repairs. A stale-but-renamed
        // handle resolves through its own history, same as every other
        // lookup here.
        let Some((storage_key, _)) = self.resolve(&mut tx, entity).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        let rows = sqlx::query(
            "SELECT value, fact_id FROM field_write
             WHERE entity = ? AND `key` = ? ORDER BY ordinal",
        )
        .bind(storage_key.as_str())
        .bind(key)
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        // The record a write arrived in says when it happened and what became
        // of it, so the claims are read alongside.
        let facts = self.facts_of(&mut tx, &storage_key).await?;
        tx.commit().await.map_err(store)?;

        let mut history = Vec::with_capacity(rows.len());
        for row in &rows {
            let carried = FactId(row.try_get::<String, _>("fact_id").map_err(store)?);
            let Some(fact) = facts.iter().find(|f| f.id == carried) else {
                continue;
            };
            history.push(FieldWrite {
                value: row.try_get::<Option<String>, _>("value").map_err(store)?,
                fact: fact.address(),
                recorded_at: fact.recorded_at,
                status: fact.status,
                provenance: fact.provenance,
                standing: fact.standing,
                note: fact.details.clone(),
            });
        }
        Ok(history)
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **What EXISTS**, for the same reason `capture` reads it: an edge may
        // point at a record the build supplies.
        let index = self.known(&mut tx).await?;
        // An edge's object names an entity, so an edit that attaches one is an
        // entity-touching write and faces the guard — screened before anything
        // is rewritten.
        if let Some(edge) = &patch.edge {
            validate_edge(edge)?;
            if let guard::Decision::Block(candidates) = guard::decide_existing(&edge.object, &index)
            {
                return Ok(Guarded::Blocked {
                    attempted: edge.object.clone(),
                    candidates,
                });
            }
        }
        // **The same rule on the edit path**, over the keys this patch sets: a
        // reference names an entity, and nothing a write names is created as a
        // side effect of being named.
        for object in referenced_by(&patch.fields, &Self::types_in(&mut tx).await?) {
            if let guard::Decision::Block(candidates) = guard::decide_existing(&object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object,
                    candidates,
                });
            }
        }
        // A miss on the HANDLE is an entity miss, with the near candidates that
        // explain it — not a fact miss trailing an empty address list. A
        // stale-but-renamed handle resolves through its own history, same as
        // every other lookup here.
        let Some((key, handle)) = self.resolve(&mut tx, &address.home).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let resolved_address = FactAddress::new(key.clone(), address.local.clone());
        let Some(mut fact) = self.read_fact(&mut tx, &resolved_address).await? else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: self.addresses_in(&mut tx, &key).await?,
            });
        };
        // **`read_fact` serves the handle form, for the readers it usually
        // answers.** This one writes the row back, so it is lowered to the
        // storage key it was read under before anything touches it — writing
        // the handle would file the edit under a different primary key and
        // strand it there, unread by anything keyed on the badge.
        fact.home = key.clone();
        fact.subject = key.clone();
        if let Some(source) = &fact.derived_from
            && let Some((source_key, _)) = self.resolve(&mut tx, &source.home).await?
        {
            fact.derived_from = Some(FactAddress::new(source_key, source.local.clone()));
        }
        // A retracted row is out of reach of an ordinary edit — one-way is
        // enforced here rather than intended elsewhere.
        if fact.status == FactStatus::Retracted {
            return Err(MemoryError::NotRetractable {
                attempted: address.to_string(),
                why: "it is retracted, and a retracted record is not editable — retraction is \
                      one-way. Capture what is so now as a new record"
                    .to_string(),
            });
        }
        // What the ADDRESSED RECORD carries right now — read off before the
        // patch rewrites it, because that is what decides which of the patch's
        // clears is a write and which names a key this record never had.
        let carried = fact.fields.clone();
        // **A source named by an EDIT faces the rule a source named at capture
        // faces.** Lineage is learned late, so it is set here too — and a
        // pointer at a claim nobody wrote would be a link that reads as
        // evidence and leads nowhere.
        if let Some(source) = &patch.derived_from {
            let index = Self::index(&mut tx).await?;
            let Some((source_key, _)) = self.resolve(&mut tx, &source.home).await? else {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            };
            let resolved_source = FactAddress::new(source_key, source.local.clone());
            match self.read_fact(&mut tx, &resolved_source).await? {
                None => {
                    return Err(MemoryError::UnknownFact {
                        attempted: source.to_string(),
                        nearest: self.addresses_in(&mut tx, &source.home).await?,
                    });
                }
                Some(held) if held.status == FactStatus::Retracted => {
                    return Err(MemoryError::SourceRetracted {
                        attempted: source.to_string(),
                    });
                }
                Some(_) => {}
            }
            // **Stored under its source's storage key, exactly as `home`
            // is.** `apply_fact_patch` only carries the patch's address
            // through as given; this corrects it to what the fake and the
            // real store both key by.
            apply_fact_patch(&mut fact, &patch)?;
            fact.derived_from = Some(resolved_source);
        } else {
            apply_fact_patch(&mut fact, &patch)?;
        }
        // **The thing's fields as they will stand, against the thing's fields
        // as they stand now.** A write may not drop a thing below a type it
        // already fits; a thing that fits nothing has nothing to protect, so
        // its records stay repairable. One function, called from both stores.
        let held = Self::writes_on(&mut tx, &fact.home).await?;
        let declared = Self::types_in(&mut tx).await?;
        // **Off the entity's own kind, never off `fact.home`** — that is a
        // badge now, and a badge carries no kind token to parse.
        let kind = index
            .iter()
            .find(|e| e.id == handle)
            .expect("resolved above")
            .kind;
        let governs = Self::kind_keys_in(&mut tx, kind.as_token()).await?;
        guard_fit(
            kind.as_token(),
            &folded_fields(&held, &declared),
            &stood_after(&held, &fact, &patch, &carried, &declared),
            &governs,
        )?;
        Self::write_fact(&mut tx, &fact, &self.clock).await?;
        // **The edit appends.** The record reads back changed — that is the
        // surface — and the value it replaced stays where it was written.
        Self::append_writes(&mut tx, &fact.home, &fact.id, writes_of(&patch, &carried)).await?;
        // Read back from the substrate rather than from what the patch
        // believed, so the answer is the projection a later read will give.
        let fields = Self::fields_of(&mut tx, &fact.home, &fact.id).await?;
        // **Served under the handle, stored under the key.**
        let served_derived_from = match &fact.derived_from {
            Some(source) => Some(FactAddress::new(
                self.current_handle(&mut tx, &source.home).await?,
                source.local.clone(),
            )),
            None => None,
        };
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(Fact {
            fields,
            home: handle.clone(),
            subject: handle,
            derived_from: served_derived_from,
            ..fact
        }))
    }

    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Merge, MemoryError> {
        if folded == survivor {
            return Err(MemoryError::NothingToMerge {
                attempted: folded.to_string(),
            });
        }
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        // **Everything is decided before anything moves.** A fold rewrites rows
        // across every table that holds a handle, so a refusal discovered
        // half-way through is the one outcome this must not produce.
        for side in [folded, survivor] {
            let Some(held) = index.iter().find(|e| &e.id == side) else {
                return Err(MemoryError::UnknownEntity {
                    attempted: side.to_string(),
                    nearest: guard::screen(side, &[], &index),
                });
            };
            if let Some(into) = &held.merged_into {
                return Err(MemoryError::AlreadyMerged {
                    attempted: side.to_string(),
                    into: into.to_string(),
                });
            }
        }
        let account = merge_account(folded, survivor, reason, date)?;
        let standing = standing_of(&account);

        // **Both sides resolved to their storage keys once, up front.** Every
        // statement below that touches `entity` or `fact_home` moves the KEY,
        // never the raw handle — a claim's home is the badge now, not the
        // handle, and updating rows by the handle would silently move
        // nothing.
        let (folded_key, _) = self.resolve(&mut tx, folded).await?.expect("checked above");
        let (survivor_key, survivor_handle) = self
            .resolve(&mut tx, survivor)
            .await?
            .expect("checked above");

        // **Every column that holds this key, in one transaction.** A fold
        // that moved the claims and not the writes under them would leave a
        // thing whose fields disagree with its records.
        // 🚨 **A row is renumbered as it moves, never bulk-updated.** A fact id
        // is local to the doc that holds it, so both sides own an `f1`: one
        // `UPDATE ... SET entity = ?` collides on the primary key the moment
        // the two have the same number of rows. **The in-memory double cannot
        // see this** — its rows are a `Vec` with no key — so the real store is
        // what says the addresses have to be reassigned one at a time.
        //
        // ⚠️ **Moving a row therefore CHANGES ITS ADDRESS**, and everything
        // pointing at that address moves with it in the same transaction.
        let moving: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM fact WHERE entity = ? ORDER BY CAST(SUBSTRING(id, 2) AS UNSIGNED)",
        )
        .bind(folded_key.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let rehomed = moving.len();
        for was in moving {
            let now = Self::mint(&mut tx, &survivor_key).await?;
            for statement in [
                "UPDATE fact SET entity = ?, id = ? WHERE entity = ? AND id = ?",
                "UPDATE fact SET derived_from = ?, derived_from_id = ? \
                 WHERE derived_from = ? AND derived_from_id = ?",
                "UPDATE field_write SET entity = ?, fact_id = ? WHERE entity = ? AND fact_id = ?",
                // **The claim's own writes move with the claim**, for the
                // reason its field writes do: a fold that moved the row and
                // not the substrate under it would leave a claim the
                // projection cannot find, which is the claim gone.
                "UPDATE fact_write SET entity = ?, fact_id = ? WHERE entity = ? AND fact_id = ?",
                // 🚨 **And the lineage on the write table, not only on the
                // claim's own row.** A claim reads back from its newest write,
                // so the row above is the copy nobody serves: fixing the
                // pointer there and not here leaves every reader following an
                // address the fold has just emptied.
                "UPDATE fact_write SET derived_from = ?, derived_from_id = ? \
                 WHERE derived_from = ? AND derived_from_id = ?",
                "UPDATE fact_event_metadata SET fact_home = ?, fact_id = ? \
                 WHERE fact_home = ? AND fact_id = ?",
                "UPDATE fact_event_ref SET fact_home = ?, fact_id = ? \
                 WHERE fact_home = ? AND fact_id = ?",
            ] {
                sqlx::query(statement)
                    .bind(survivor_key.as_str())
                    .bind(now.as_str())
                    .bind(folded_key.as_str())
                    .bind(&was)
                    .execute(&mut *tx)
                    .await
                    .map_err(store)?;
            }
        }
        // What names the handle rather than a row inside it moves wholesale.
        // An edge's object and a ref's entity are stored as the plain handle
        // merge was given, never a badge (step 3 resolves an edge on the way
        // OUT; nothing resolves one in) — so these compare against the raw
        // handles, exactly as they always have.
        for statement in [
            "UPDATE fact SET edge_object = ? WHERE edge_object = ?",
            "UPDATE fact_write SET edge_object = ? WHERE edge_object = ?",
            "UPDATE fact_event_ref SET entity = ? WHERE entity = ?",
            "UPDATE entity SET parent = ? WHERE parent = ?",
        ] {
            sqlx::query(statement)
                .bind(survivor.as_str())
                .bind(folded.as_str())
                .execute(&mut *tx)
                .await
                .map_err(store)?;
        }

        let record = Fact {
            id: Self::mint(&mut tx, &survivor_key).await?,
            home: survivor_key.clone(),
            subject: survivor_key.clone(),
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            recorded_at: account.recorded_at,
            happened_at: account.happened_at,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            inserted_at: Some(self.clock.now()),
            stale_after: None,
        };
        Self::write_fact(&mut tx, &record, &self.clock).await?;
        Self::append_writes(&mut tx, &record.home, &record.id, written_keys(&record)).await?;

        // **The folded row stays and starts forwarding.** Written last, so a
        // failure anywhere above rolls back a row that still says it is a thing
        // rather than one pointing at a fold that did not happen.
        sqlx::query("UPDATE entity SET merged_into = ? WHERE id = ?")
            .bind(survivor.as_str())
            .bind(folded.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;

        // **Read back rather than reconstructed.** The fold may have moved the
        // survivor's own row — a folded parent is re-pointed above — so the
        // answer is what the store now holds and not what this call assembled.
        let survived = Self::index(&mut tx)
            .await?
            .into_iter()
            .find(|e| &e.id == survivor)
            .ok_or_else(|| MemoryError::UnknownEntity {
                attempted: survivor.to_string(),
                nearest: Vec::new(),
            })?;
        // **Served under the handle, stored under the key.**
        let served_record = Fact {
            home: survivor_handle.clone(),
            subject: survivor_handle,
            ..record
        };
        tx.commit().await.map_err(store)?;
        Ok(Merge {
            survivor: survived,
            folded: folded.clone(),
            record: served_record,
            rehomed,
        })
    }

    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        let Some((key, handle)) = self.resolve(&mut tx, &address.home).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let resolved_address = FactAddress::new(key.clone(), address.local.clone());
        // Everything is decided before anything moves, so a refusal leaves the
        // row exactly as it was.
        let Some(mut target) = self.read_fact(&mut tx, &resolved_address).await? else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: self.addresses_in(&mut tx, &key).await?,
            });
        };
        // **Lowered back to the storage key it was read under** — this row is
        // about to be written again, under the address it actually lives at.
        target.home = key.clone();
        target.subject = key.clone();
        if let Some(source) = &target.derived_from
            && let Some((source_key, _)) = self.resolve(&mut tx, &source.home).await?
        {
            target.derived_from = Some(FactAddress::new(source_key, source.local.clone()));
        }
        // **`retraction_of` addresses the claim it takes back in a field
        // value, as free text** — served under the handle, exactly as any
        // other address a reader is given, never under the storage key.
        let served_target = Fact {
            home: handle.clone(),
            subject: handle.clone(),
            ..target.clone()
        };
        let account = retraction_of(&served_target, reason, date)?;
        let standing = standing_of(&account);
        let record = Fact {
            id: Self::mint(&mut tx, &key).await?,
            home: key.clone(),
            subject: key,
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            recorded_at: account.recorded_at,
            happened_at: account.happened_at,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            // A retraction is a record in its own right, taken in now.
            inserted_at: Some(self.clock.now()),
            stale_after: None,
        };
        let retracted = Fact {
            status: FactStatus::Retracted,
            ..target
        };
        Self::write_fact(&mut tx, &retracted, &self.clock).await?;
        Self::write_fact(&mut tx, &record, &self.clock).await?;
        // The account is a record like any other, and the key naming what it
        // takes back is a write of its own. The record being taken back writes
        // no key: what changed there is its status.
        Self::append_writes(&mut tx, &record.home, &record.id, written_keys(&record)).await?;
        // **Served under the handle, stored under the key.**
        let served_retracted = Fact {
            home: handle.clone(),
            subject: handle.clone(),
            ..retracted
        };
        let served_record = Fact {
            home: handle.clone(),
            subject: handle,
            ..record
        };
        tx.commit().await.map_err(store)?;
        Ok(Retraction {
            retracted: served_retracted,
            record: served_record,
        })
    }

    /// **One read of the writes, folded by the domain's own rule.** The default
    /// asks for each key's history in turn; this store keeps every write on a
    /// thing in one table and reads them together, then hands them to the same
    /// function that decides what the thing holds — so the value and its
    /// backing cannot disagree about which write won.
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, jojobot_domain::memory::FieldBacking>, MemoryError>
    {
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **The same gate every read beside it keeps** (rule 234): the rows
        // plus what the build supplies. The port derives this read from
        // `fields` and `history`, and both of those miss a handle nobody has,
        // so a store that overrides it owes that answer too. An empty map says
        // nobody has written a key here, which is a different answer from
        // there is no such thing and has a different repair.
        let index = self.known(&mut tx).await?;
        let Some((key, _)) = self.resolve(&mut tx, entity).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        let writes = Self::writes_on(&mut tx, &key).await?;
        let declared = Self::types_in(&mut tx).await?;
        tx.commit().await.map_err(store)?;
        Ok(jojobot_domain::memory::folded_backing(&writes, &declared))
    }

    /// **Selected on the pointer, because the store has an index for it.** The
    /// default reads every entity's records to answer this; the pair of columns
    /// the pointer lives in carries an index, so the claims standing on one
    /// claim are a query.
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        validate_subject(&source.home)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **The column holds the badge, and a caller's address names a
        // handle.** Resolved here for the same reason every other addressed
        // lookup resolves one first — a stale-but-renamed handle still finds
        // what was built on it.
        let key = match self.resolve(&mut tx, &source.home).await? {
            Some((key, _)) => key,
            None => source.home.clone(),
        };
        let rows = sqlx::query(&format!(
            "SELECT {FACT_COLUMNS} FROM fact WHERE derived_from = ? AND derived_from_id = ? \
             ORDER BY entity, id"
        ))
        .bind(key.as_str())
        .bind(source.local.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        // **Assembled by the one reader every other read here uses**, so a
        // claim reached through its lineage is the same record a recall gives.
        let standing_on = self.assemble(&mut tx, &rows).await?;
        tx.commit().await.map_err(store)?;
        Ok(standing_on)
    }

    /// **Targeted, because the default reads every entity.** A field write
    /// keeps its value in a row of its own, so the holders are one query away:
    /// the entities with a write whose value is this handle. Only those are
    /// read back, rather than the whole index.
    ///
    /// The rows say which entities to read and nothing more. Which of their
    /// records still carry the handle is decided from the records themselves,
    /// so a key that was written and later overwritten does not come back as a
    /// pointer that is no longer there.
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        validate_subject(target)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let rows = sqlx::query("SELECT DISTINCT entity FROM field_write WHERE value = ?")
            .bind(target.as_str())
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?;
        let mut pointing = Vec::new();
        for row in rows {
            let holder = EntityId(row.try_get::<String, _>("entity").map_err(store)?);
            for fact in self.facts_of(&mut tx, &holder).await? {
                if fact
                    .fields
                    .values()
                    .any(|value| value.trim() == target.as_str())
                {
                    pointing.push(fact);
                }
            }
        }
        tx.commit().await.map_err(store)?;
        Ok(pointing)
    }

    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        validate_write_subject(entity)?;
        validate_prose(prose)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        // Never creates: a handle that names nothing is a miss with its near
        // candidates, exactly as it is for every other verb here.
        if !index.iter().any(|e| &e.id == entity) {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        }
        let stored = normalize_prose(prose);
        sqlx::query("UPDATE entity SET prose = ? WHERE id = ?")
            .bind(&stored)
            .bind(entity.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        tx.commit().await.map_err(store)?;
        Ok(stored)
    }

    /// **An entity is its own document here**, so its handle is the honest
    /// answer to "which document is this": there is no page to open and no
    /// second identifier to invent.
    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let entities = Self::index(&mut tx).await?;
        let mut scanned = Vec::with_capacity(entities.len());
        for entity in entities {
            let (prose, badge): (String, Option<String>) =
                sqlx::query_as("SELECT prose, badge FROM entity WHERE id = ?")
                    .bind(entity.id.as_str())
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(store)?;
            // **The document is identified by the badge and not by the
            // handle.** The index evicts a document by this id, so while it was
            // the handle the two were one thing and a rename would have had to
            // move the postings with it.
            //
            // **No fallback to the handle for a row that has none.** A fallback
            // that works is how a half-filled column survives: nothing breaks
            // visibly, so nothing forces the fill. The fill runs at every boot
            // before the index is built, so a row without a badge means that
            // fill did not happen — which the boot has already said out loud,
            // and this says it again rather than papering over it.
            let Some(badge) = badge else {
                return Err(MemoryError::Store(format!(
                    "{} carries no badge, so its document has no id — the startup fill did not \
                     reach it",
                    entity.id
                )));
            };
            // **The storage key this entity's own rows are filed under** —
            // its badge, read just above. `facts_of` and `held_by` both read
            // by this key; the facts they return already carry the handle,
            // resolved on the way out exactly as every other read is.
            let key = EntityId(badge.clone());
            scanned.push(search::DocScan {
                doc_id: badge,
                title: entity.name.clone(),
                prose,
                facts: self.facts_of(&mut tx, &key).await?,
                // What the thing IS travels with the doc: the records beside it
                // cannot be folded back into it.
                fields: Self::held_by(&mut tx, &key).await?,
                entity: Some(entity),
                owner: None,
            });
        }
        tx.commit().await.map_err(store)?;
        Ok(scanned)
    }

    /// **A type is the set of rows sharing its name**, so declaring one is
    /// deleting those rows and writing the new set. One transaction, because a
    /// type that was half replaced would describe a record nobody declared.
    ///
    /// The origin of what is already there is read inside that transaction and
    /// decides whether the replacement may happen at all: a caller cannot write
    /// over a type the software ships.
    async fn declare_type(&self, declared: DeclaredType) -> Result<DeclaredType, MemoryError> {
        validate_type(&declared)?;
        let declared = declared.normalized();
        let mut tx = self.pool.begin().await.map_err(store)?;
        let held: Option<String> =
            sqlx::query_scalar("SELECT origin FROM type_field WHERE type_name = ? LIMIT 1")
                .bind(&declared.name)
                .fetch_optional(&mut *tx)
                .await
                .map_err(store)?;
        guard_replacement(&declared, held.as_deref().map(read_origin))?;
        owned_by_the_other_half(&mut tx, &declared.name, KEYS_OF_A_KIND).await?;
        sqlx::query("DELETE FROM type_field WHERE type_name = ? AND owner = ?")
            .bind(&declared.name)
            .bind(KEYS_OF_A_TYPE)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        sqlx::query("DELETE FROM type_field WHERE type_name = ? AND owner = ?")
            .bind(&declared.name)
            .bind(KEYS_OF_A_TYPE)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        for (ordinal, field) in declared.fields.iter().enumerate() {
            sqlx::query(
                "INSERT INTO type_field (type_name, key_name, ordinal, holds, folds, origin, owner, required, one_of)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&declared.name)
            .bind(&field.key)
            .bind(ordinal as i64 + 1)
            .bind(field.holds_token())
            .bind(field.folds.as_token())
            .bind(declared.origin.as_token())
            .bind(KEYS_OF_A_TYPE)
            .bind(field.required)
            .bind(field.one_of_cell())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        }
        tx.commit().await.map_err(store)?;
        Ok(declared)
    }

    /// The rows, gathered back into the types they are.
    ///
    /// **A row whose `holds` names no value type reads as text** rather than
    /// dropping the key. Text holds anything, so the key still describes what a
    /// writer should fill and still matches a record — where dropping it would
    /// quietly shrink a type and report the key as one no record carries.
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        let rows = sqlx::query(
            "SELECT type_name, key_name, holds, folds, origin, required, one_of FROM type_field
             ORDER BY type_name, ordinal",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        Ok(gather_types(&rows))
    }

    async fn declare_kind(
        &self,
        token: &str,
        origin: Origin,
        fields: Vec<Field>,
    ) -> Result<(), MemoryError> {
        // The keys are screened exactly as a schema's are — one declaration
        // shape, one validator, so a kind cannot name a key twice where a type
        // could not.
        let declared = DeclaredType {
            name: token.to_string(),
            fields,
            origin,
        };
        if !declared.fields.is_empty() {
            validate_type(&declared)?;
        }
        let declared = declared.normalized();

        // **One transaction.** A kind whose row landed and whose keys did not
        // would be a kind describing something nobody declared.
        let mut tx = self.pool.begin().await.map_err(store)?;

        // **The shipped ten are closed to a caller**, read from the row rather
        // than from a list here: what makes a kind the software's is the origin
        // it was written with, so there is nothing to keep in step with code.
        let held: Option<String> = sqlx::query_scalar("SELECT origin FROM kind WHERE token = ?")
            .bind(token)
            .fetch_optional(&mut *tx)
            .await
            .map_err(store)?;
        jojobot_domain::memory::types::guard_kind_replacement(
            token,
            origin,
            held.as_deref().and_then(Origin::of_token),
        )?;
        sqlx::query("REPLACE INTO kind (token, origin) VALUES (?, ?)")
            .bind(token)
            .bind(origin.as_token())
            .execute(&mut *tx)
            .await
            .map_err(store)?;

        // **Naming no keys is not taking every key away.** The seed declares
        // every shipped kind on every boot and names none, so a delete here
        // would take away whatever that name held at each restart.
        if !declared.fields.is_empty() {
            owned_by_the_other_half(&mut tx, token, KEYS_OF_A_TYPE).await?;
            sqlx::query("DELETE FROM type_field WHERE type_name = ? AND owner = ?")
                .bind(token)
                .bind(KEYS_OF_A_KIND)
                .execute(&mut *tx)
                .await
                .map_err(store)?;
            for (ordinal, field) in declared.fields.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO type_field (type_name, key_name, ordinal, holds, folds, origin, owner, required, one_of)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(token)
                .bind(&field.key)
                .bind(ordinal as i64 + 1)
                .bind(field.holds_token())
                .bind(field.folds.as_token())
                .bind(origin.as_token())
                .bind(KEYS_OF_A_KIND)
                .bind(field.required)
                .bind(field.one_of_cell())
                .execute(&mut *tx)
                .await
                .map_err(store)?;
            }
        }
        tx.commit().await.map_err(store)?;
        Ok(())
    }

    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        // **One transaction**, for the reason declaring one is: a kind whose
        // row went and whose keys stayed would leave key rows describing a
        // kind nothing can be.
        let mut tx = self.pool.begin().await.map_err(store)?;

        // **The origin decides, and it is read off the row.** A token naming a
        // kind the operator declared reaches the same statements and matches
        // nothing, so what a caller wrote cannot be taken by this path however
        // its name got here.
        let removed = sqlx::query("DELETE FROM kind WHERE token = ? AND origin = ?")
            .bind(token)
            .bind(Origin::Shipped.as_token())
            .execute(&mut *tx)
            .await
            .map_err(store)?
            .rows_affected();
        if removed > 0 {
            sqlx::query("DELETE FROM type_field WHERE type_name = ? AND owner = ?")
                .bind(token)
                .bind(KEYS_OF_A_KIND)
                .execute(&mut *tx)
                .await
                .map_err(store)?;
        }
        tx.commit().await.map_err(store)?;
        Ok(())
    }

    async fn declared_kinds(&self) -> Result<Vec<(String, Origin)>, MemoryError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT token, origin FROM kind ORDER BY token")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        Ok(rows
            .into_iter()
            // A row whose origin names nothing this build knows reads as a
            // caller's, which is the weaker claim: it says the software does
            // not vouch for the kind rather than that it does.
            .map(|(token, origin)| (token, Origin::of_token(&origin).unwrap_or(Origin::Declared)))
            .collect())
    }
}

/// **Which half of the sentence a key row belongs to.** A kind is a schema
/// that is also identity, so both keep their keys here — and the name alone
/// cannot say which one wrote a row.
const KEYS_OF_A_TYPE: &str = "type";
const KEYS_OF_A_KIND: &str = "kind";

/// **Refuse to write over the other half's keys.**
///
/// Sharing the table is the model working: a kind's keys ARE keys. Taking the
/// other side's rows is not, and the name is the whole key, so without this
/// each side silently replaced whatever the other had put there.
async fn owned_by_the_other_half(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    name: &str,
    theirs: &str,
) -> Result<(), MemoryError> {
    let held: Option<String> = sqlx::query_scalar(
        "SELECT owner FROM type_field WHERE type_name = ? AND owner = ? LIMIT 1",
    )
    .bind(name)
    .bind(theirs)
    .fetch_optional(&mut **tx)
    .await
    .map_err(store)?;
    if held.is_some() {
        let holder = if theirs == KEYS_OF_A_KIND {
            "a kind"
        } else {
            "a declared type"
        };
        return Err(MemoryError::InvalidEntity(format!(
            "'{name}' already names {holder}, and its keys are not this declaration's to replace"
        )));
    }
    Ok(())
}

/// Rows into the types they are — **one reader**, so the roster a write is
/// screened against and the roster a caller lists cannot come to be assembled
/// two different ways.
///
/// **A row whose `holds` names no value type reads as text** rather than
/// dropping the key. Text holds anything, so the key still describes what a
/// writer should fill and still matches a record — where dropping it would
/// quietly shrink a type and report the key as one no record carries.
///
/// **A row whose `folds` names no fold reads as newest-wins**, which is the
/// unconfigured behaviour and the one every key had before there was a choice.
/// A token this build does not know must not turn a key into a counter.
fn gather_types(rows: &[sqlx::mysql::MySqlRow]) -> Vec<DeclaredType> {
    let mut types: Vec<DeclaredType> = Vec::new();
    for row in rows {
        let name: String = row.get("type_name");
        let key: String = row.get("key_name");
        let field = Field {
            folds: Fold::of_token(&row.get::<String, _>("folds")).unwrap_or_default(),
            required: row.get::<bool, _>("required"),
            one_of: Field::one_of_from_cell(row.get::<Option<String>, _>("one_of").as_deref()),
            ..Field::of_token(&key, &row.get::<String, _>("holds"))
                .unwrap_or_else(|| Field::new(&key, ValueType::Text))
        };
        match types.last_mut() {
            Some(last) if last.name == name => last.fields.push(field),
            _ => types.push(DeclaredType {
                origin: read_origin(&row.get::<String, _>("origin")),
                ..DeclaredType::new(&name, vec![field])
            }),
        }
    }
    types
}

/// Write one whole entity — the row and the aliases under it — replacing
/// whatever was there. One writer for the creation and the edit alike.
/// **A badge no entity wears**, drawn inside the caller's transaction — so the
/// answer to "is it free" covers the row this call is about to write.
async fn mint_badge(tx: &mut Transaction<'_, MySql>, draw: &Draw) -> Result<String, MemoryError> {
    ids::draw_free(tx, draw, "SELECT 1 FROM entity WHERE badge = ?", None)
        .await
        .map_err(store)?
        .ok_or_else(|| MemoryError::Store("no free entity badge could be drawn".into()))
}

/// **Hands back the badge the row wears afterwards**, so the caller's copy of
/// the entity says what the row says. A receipt carrying no badge while the row
/// wears one is the two halves disagreeing about the same record.
async fn write_entity(
    tx: &mut Transaction<'_, MySql>,
    draw: &Draw,
    entity: &Entity,
) -> Result<String, MemoryError> {
    // **The prose is carried across rather than blanked.** A rewrite of an
    // entity's metadata is not a rewrite of what somebody wrote on its page,
    // and `REPLACE` deletes the row before inserting the new one.
    let prose: Option<String> = sqlx::query_scalar("SELECT prose FROM entity WHERE id = ?")
        .bind(entity.id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
    // **The badge is carried across for the same reason and it matters more.**
    // Prose that came back blank would be visible to whoever wrote it; a badge
    // that came back different is a row that quietly stopped being the thing
    // anything else was pointing at. **Every rewrite of an entity goes through
    // here**, so carrying it once covers both of them.
    //
    // A row that has none is given one now: drawn, probed inside this
    // transaction, never accepted from a caller.
    let held: Option<Option<String>> = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(entity.id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
    let badge = match held.flatten() {
        Some(already) => already,
        None => mint_badge(tx, draw).await?,
    };
    sqlx::query(
        "REPLACE INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entity.id.as_str())
    .bind(entity.kind.as_token())
    .bind(&entity.name)
    .bind(&entity.source)
    .bind(entity.crm.as_deref())
    .bind(entity.parent.as_ref().map(EntityId::as_str))
    .bind(entity.boot.as_token())
    .bind(prose.unwrap_or_default())
    .bind(&badge)
    .bind(entity.merged_into.as_ref().map(EntityId::as_str))
    .execute(&mut **tx)
    .await
    .map_err(store)?;
    // **Keyed on the badge, not the handle.** An alias row is the same shape
    // `fact.entity` was before `ccc926f`: its own foreign key back to the
    // thing it belongs to, so a rename that left it on the handle would sever
    // it the moment the row it names moves — silently, since nothing else
    // here would refuse the write.
    sqlx::query("DELETE FROM entity_alias WHERE entity = ?")
        .bind(&badge)
        .execute(&mut **tx)
        .await
        .map_err(store)?;
    for (ordinal, alias) in entity.aliases.iter().enumerate() {
        sqlx::query("INSERT INTO entity_alias (entity, ordinal, alias) VALUES (?, ?, ?)")
            .bind(&badge)
            .bind(ordinal as i64 + 1)
            .bind(alias)
            .execute(&mut **tx)
            .await
            .map_err(store)?;
    }
    Ok(badge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::{Scratch, free_port};
    use crate::dolt::{Dolt, migrate};
    use jiff::civil::date;
    use jojobot_domain::memory::{FactPatch, NewEntity, NewFact};

    /// 🚨 **The substrate and the claim's own row answer the same thing**, on a
    /// claim written once and on a claim corrected twice.
    ///
    /// ⛔️ **This is what makes the switch that follows a no-op rather than a
    /// leap.** The projection lands before anything reads it precisely so the
    /// two can be held against each other first: a store where they disagree is
    /// one where moving the reads changes answers, and nobody would know which
    /// answers.
    ///
    /// **Both shapes, because they fail differently.** A claim with one write
    /// is the case the backfill produces and the commonest thing in the store;
    /// a claim with three is the case the projection exists for, and a
    /// projection that took the OLDEST write would pass the first and fail the
    /// second.
    #[tokio::test]
    async fn the_projection_and_the_row_agree() {
        let scratch = Scratch::new("projection");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("projection")
            .await
            .expect("a database of its own");
        migrate::run(&pool).await.expect("the schema");
        let memory = DoltMemory::open(pool.clone());
        // The kinds, or no handle parses and every write is refused.
        jojobot_domain::memory::kinds::seed(&memory)
            .await
            .expect("the kinds are seeded");

        let subject = EntityId::person("person:projected");
        memory
            .add_entity(NewEntity::new(
                subject.clone(),
                "Projected",
                "contract-fixture",
            ))
            .await
            .expect("add_entity ok")
            .written()
            .expect("the guard waves it through");
        let once = memory
            .capture(NewFact::about(
                subject.clone(),
                "written once and left alone",
                date(2026, 8, 10),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through");
        let corrected = memory
            .capture(NewFact::about(
                subject.clone(),
                "first thing said",
                date(2026, 8, 10),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through");
        for said in ["second thing said", "third thing said"] {
            memory
                .update_fact(
                    &corrected.address(),
                    FactPatch {
                        content: Some(said.into()),
                        ..Default::default()
                    },
                )
                .await
                .expect("update_fact ok")
                .written()
                .expect("the guard waves it through");
        }

        let mut tx = pool.begin().await.expect("a transaction");
        // **Both these low-level readers want the storage key**, not the
        // handle a caller sees — resolved here for the same reason every
        // other lookup at this level resolves one first.
        let (key, _) = memory
            .resolve(&mut tx, &subject)
            .await
            .expect("resolve ok")
            .expect("the subject exists");
        let off_the_row = memory.facts_of(&mut tx, &key).await.expect("the rows read");
        let projected = memory
            .facts_projected(&mut tx, &key)
            .await
            .expect("the substrate projects");
        assert_eq!(
            projected, off_the_row,
            "the substrate and the row disagree, so moving the reads would change answers",
        );

        let one = memory
            .fact_projected(&mut tx, &FactAddress::new(key.clone(), once.id.clone()))
            .await
            .expect("the substrate projects one");
        assert_eq!(
            one.as_ref().map(|f| f.content.as_str()),
            Some("written once and left alone"),
            "a claim with one write did not project as itself",
        );
        let many = memory
            .fact_projected(
                &mut tx,
                &FactAddress::new(key.clone(), corrected.id.clone()),
            )
            .await
            .expect("the substrate projects one");
        assert_eq!(
            many.as_ref().map(|f| f.content.as_str()),
            Some("third thing said"),
            "the projection took a write that is not the newest",
        );
        tx.commit().await.expect("the read commits");

        store.stop().await;
    }
}

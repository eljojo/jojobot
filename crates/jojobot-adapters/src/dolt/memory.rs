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
    Archived, ClaimWrite, Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact,
    FactAddress, FactId, FactPatch, FactStatus, FieldWrite, FormerHandle, Guarded, KeyWrite,
    Memory, MemoryError, Merge, NewEntity, NewFact, Provenance, Retraction, Standing, WriteSummary,
    apply_entity_patch, apply_fact_patch, folded_fields, guard, guard_fit_in,
    kinds::{self, NotAKind},
    merge_account, normalize_content, normalize_details, normalize_prose, referenced_by,
    retraction_of, screen_entity_patch, search, standing_of, stood_after, stood_after_capture,
    types::{
        DeclaredType, Displaced, Field, Fold, Origin, ValueType, guard_replacement, validate_type,
    },
    validate_content, validate_edge, validate_entity, validate_field, validate_fields,
    validate_happened_span, validate_prose, validate_provenance_source, validate_subject,
    validate_write_subject, writes_of,
};
use sqlx::{MySql, MySqlPool, Row, Transaction};

use super::ids::{self, Draw};

/// **What the field-link migration did, and what it left alone.**
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FieldLinkReport {
    /// Writes whose value was plain handle text and is now the permanent id.
    pub lowered: usize,
    /// Link rows the pass added.
    pub linked: usize,
    /// Link rows the pass removed because no write holds them.
    pub unlinked: usize,
    /// Values that stay plain text because the handle they hold does not name
    /// one thing: one line each, naming the thing, the key and the value.
    pub left_as_text: Vec<String>,
}

/// Memory kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets nobody
/// set.
#[derive(Clone)]
pub struct DoltMemory {
    pool: MySqlPool,
    /// **How many times [`index`](Self::index) has built the full listing** —
    /// the one raw query every path to it, [`known`](Self::known) included,
    /// runs through. Test-only instrumentation for the one property nothing
    /// else here can observe: whether a read paid for every entity in the
    /// store or only the one (or few) it actually needed. Shared across a
    /// clone, exactly as the pool it counts calls against is.
    index_listings: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// **How many times one of this store's own overrides of a defaulted
    /// read has run** — `backing`, `built_on`, `referring_to` and
    /// `claim_histories`. Test-only instrumentation for the one property a
    /// decorator stack hides: whether a call that entered at the top reached
    /// the store's targeted query or was answered, slowly, by the port's
    /// default body in a layer above it. Shared across a clone, like the
    /// listing count.
    targeted_reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
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
            index_listings: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            targeted_reads: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            draw: ids::drawing(),
            supplied: Provisions::default(),
            clock: Clock::default(),
        }
    }

    /// **How many times the full listing has been built, this instance's
    /// whole life.** Test-only: a caller measures a delta across the
    /// operation it is checking, since a store already used for setup has
    /// paid for listings this count includes.
    #[cfg(any(test, feature = "testing"))]
    pub fn index_listings(&self) -> usize {
        self.index_listings
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// **How many times this store's own `backing`, `built_on`,
    /// `referring_to` or `claim_histories` has run.** Test-only: a caller
    /// measures a delta across one call made through the layers above.
    #[cfg(any(test, feature = "testing"))]
    pub fn targeted_reads(&self) -> usize {
        self.targeted_reads
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn counted_targeted_read(&self) {
        self.targeted_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
            index_listings: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            targeted_reads: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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
            append_entity_write(&mut tx, &EntityId(handle.clone()), &self.clock).await?;
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
    ///
    /// **`fact_stands_for` is not in this list.** It is a new table, born
    /// badge-keyed from its first row: the write path resolves a mark's
    /// addresses to badges before anything lands, exactly as `fact.entity`
    /// and `fact.derived_from` now do, so there are no legacy handle-keyed
    /// rows under it for a backfill to find. This list is for what predates
    /// a column's own badge-awareness, never for what was born with it.
    /// `fact_event_ref.entity`, `entity.parent` and `edge_object` are
    /// badge-keyed too (rule 268), but not by this call: they name a
    /// DIFFERENT entity rather than the row's own, and [`Self::
    /// resolve_stale_pointer_columns`] is the migration that rekeys those,
    /// separately, because an unresolvable value there is an error rather
    /// than a row this backfill can quietly skip.
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

    /// **Rekey `fact.edge_object` (and its `fact_write` mirror),
    /// `fact_event_ref.entity` and `entity.parent` onto the badge each row's
    /// handle answers to now** (rule 268). A row written before this build
    /// stored the badge at write time still holds whatever handle it was
    /// given; this reaches those, once, at startup, beside the badge fill
    /// and the handle-keyed rekey above — for the same reason neither of
    /// those is a migration: it reads the badge and the rename history the
    /// fill and the rekey just made current.
    ///
    /// **Unlike the two migrations beside it, an unresolvable row is not a
    /// warning.** The operator's ruling is that a pointer at nothing should
    /// never have been writable, and a migration that quietly drops or
    /// quietly keeps one is the same silent damage that ruling exists to
    /// end. So every row this CAN resolve — through the entity's current
    /// handle or any handle it has ever worn — is rewritten, and if any
    /// stored value resolves through none of those, nothing for THAT value
    /// is written and the whole call comes back an error naming the count
    /// and the rows. What this call already resolved for other values stays
    /// resolved: a restart does not undo progress, it repeats the same scan
    /// and finds less to do.
    ///
    /// Returns how many distinct stored values it rewrote.
    pub async fn resolve_stale_pointer_columns(&self) -> Result<usize, MemoryError> {
        let known: Vec<(String, String)> =
            sqlx::query_as("SELECT id, badge FROM entity WHERE badge IS NOT NULL")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        let badges: std::collections::HashSet<&str> =
            known.iter().map(|(_, badge)| badge.as_str()).collect();
        let mut resolve: std::collections::HashMap<&str, &str> = known
            .iter()
            .map(|(handle, badge)| (handle.as_str(), badge.as_str()))
            .collect();
        // **Newest event first**, the same order [`Self::former_handles_in`]
        // reads in — a former handle may carry more than one event, and
        // `or_insert` below keeps only the first one it sees for a given
        // handle.
        let former: Vec<(String, String)> = sqlx::query_as(
            "SELECT former_handle, badge FROM entity_former_handle \
             ORDER BY former_handle, ordinal DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        for (handle, badge) in &former {
            resolve.entry(handle.as_str()).or_insert(badge.as_str());
        }

        let mut rewritten = 0;
        let mut unresolved: Vec<String> = Vec::new();
        for (table, column) in [
            ("fact", "edge_object"),
            ("fact_write", "edge_object"),
            ("fact_event_ref", "entity"),
            ("entity", "parent"),
        ] {
            let stored: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT DISTINCT {column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .fetch_all(&self.pool)
            .await
            .map_err(store)?;
            for value in stored {
                if badges.contains(value.as_str()) {
                    continue;
                }
                match resolve.get(value.as_str()) {
                    Some(badge) => {
                        sqlx::query(&format!(
                            "UPDATE {table} SET {column} = ? WHERE {column} = ?"
                        ))
                        .bind(*badge)
                        .bind(&value)
                        .execute(&self.pool)
                        .await
                        .map_err(store)?;
                        rewritten += 1;
                    }
                    None => {
                        let count: i64 = sqlx::query_scalar(&format!(
                            "SELECT COUNT(*) FROM {table} WHERE {column} = ?"
                        ))
                        .bind(&value)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(store)?;
                        unresolved.push(format!(
                            "{table}.{column} = '{value}' ({count} row{})",
                            if count == 1 { "" } else { "s" }
                        ));
                    }
                }
            }
        }

        if unresolved.is_empty() {
            Ok(rewritten)
        } else {
            Err(MemoryError::Store(format!(
                "{} stored pointer{} cannot be resolved to anything this store has ever named, \
                 through a current handle or a former one — a person has to repair these before \
                 the migration can finish: {}",
                unresolved.len(),
                if unresolved.len() == 1 { "" } else { "s" },
                unresolved.join("; "),
            )))
        }
    }

    /// **Lower the plain handle text left under undeclared keys, and bring the
    /// link table into agreement with the field writes — in one pass.**
    ///
    /// A value stored as plain handle text before ids were kept names whatever
    /// answers to that handle now unless a rename has moved it, so it cannot be
    /// told from a value a caller meant for somebody else. This reads every
    /// handle in such a value through the entity list, the rename history and
    /// the records the build supplies. **A handle that names exactly one thing is
    /// stored as that thing's id behind the mark. A handle that names none, or
    /// more than one, leaves the whole value as written** and is listed in the
    /// report, one line each: the pass does not guess. A key some type declares a
    /// reference whose value is already an id holds no handle, so it is left as it
    /// is; one still holding handle text is lowered here as any other key is.
    ///
    /// **Then the link table is made to equal what the writes imply**: a row the
    /// writes imply and the table lacks is added, and a row the table holds that
    /// no write implies is removed. The table is derived, so this is also the
    /// repair for one that was lost or filled by a build that did not keep it.
    ///
    /// **Idempotent.** A second run lowers nothing and changes no row; the values
    /// left as text are listed again, because they are still there.
    pub async fn migrate_field_links(&self) -> Result<FieldLinkReport, MemoryError> {
        use std::collections::{BTreeMap, BTreeSet};

        let mut report = FieldLinkReport::default();
        let mut tx = self.pool.begin().await.map_err(store)?;
        let declared = Self::types_in(&mut tx).await?;

        let entities: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT id, badge FROM entity")
                .fetch_all(&mut *tx)
                .await
                .map_err(store)?;
        let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut handle_of: BTreeMap<String, String> = BTreeMap::new();
        for (handle, badge) in &entities {
            if let Some(badge) = badge.as_deref().filter(|badge| !badge.is_empty()) {
                names
                    .entry(handle.clone())
                    .or_default()
                    .insert(badge.to_string());
                handle_of.insert(badge.to_string(), handle.clone());
            }
        }
        let former: Vec<(String, String)> =
            sqlx::query_as("SELECT former_handle, badge FROM entity_former_handle")
                .fetch_all(&mut *tx)
                .await
                .map_err(store)?;
        for (handle, badge) in former {
            names.entry(handle).or_default().insert(badge);
        }
        // A record the build supplies wears no badge, and a write stores its
        // handle as its id.
        for (entity, _) in self.supplied.records() {
            let id = entity
                .badge
                .clone()
                .filter(|badge| !badge.is_empty())
                .unwrap_or_else(|| entity.id.to_string());
            names.entry(entity.id.to_string()).or_default().insert(id);
        }

        let mut writes: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT entity, `key`, ordinal, value FROM field_write WHERE value IS NOT NULL
             ORDER BY entity, `key`, ordinal",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;

        for (entity, key, ordinal, value) in &mut writes {
            let handles = jojobot_domain::memory::handles_in_value(value);
            if handles.is_empty() {
                continue;
            }
            let mut ids = Vec::with_capacity(handles.len());
            let mut why = None;
            for handle in &handles {
                match names.get(handle.as_str()).map(|named| named.len()) {
                    Some(1) => {
                        let named = &names[handle.as_str()];
                        ids.extend(named.iter().cloned());
                    }
                    Some(_) => why = Some("the handle has named more than one thing"),
                    None => why = Some("the handle names nothing this store has held"),
                }
            }
            if let Some(why) = why {
                let holder = handle_of.get(entity.as_str()).unwrap_or(entity);
                report
                    .left_as_text
                    .push(format!("{holder} key '{key}' value '{value}': {why}"));
                continue;
            }
            let lowered = ids
                .iter()
                .map(|id| jojobot_domain::memory::marked_stored_handle(id))
                .collect::<Vec<_>>()
                .join(", ");
            sqlx::query(
                "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ? AND ordinal = ?",
            )
            .bind(&lowered)
            .bind(&*entity)
            .bind(&*key)
            .bind(*ordinal)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
            *value = lowered;
            report.lowered += 1;
        }

        let mut expected: BTreeSet<(String, String, i64, String)> = BTreeSet::new();
        for (entity, key, ordinal, value) in &writes {
            for target in jojobot_domain::memory::field_link_targets(key, value, &declared) {
                expected.insert((entity.clone(), key.clone(), *ordinal, target));
            }
        }
        let held: BTreeSet<(String, String, i64, String)> =
            sqlx::query_as::<_, (String, String, i64, String)>(
                "SELECT entity, `key`, ordinal, target FROM field_link",
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?
            .into_iter()
            .collect();
        for (entity, key, ordinal, target) in held.difference(&expected) {
            sqlx::query(
                "DELETE FROM field_link
                 WHERE entity = ? AND `key` = ? AND ordinal = ? AND target = ?",
            )
            .bind(entity)
            .bind(key)
            .bind(ordinal)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
            report.unlinked += 1;
        }
        for (entity, key, ordinal, target) in expected.difference(&held) {
            sqlx::query(
                "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, ?, ?, ?)",
            )
            .bind(entity)
            .bind(key)
            .bind(ordinal)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
            report.linked += 1;
        }
        tx.commit().await.map_err(store)?;
        Ok(report)
    }

    /// **Rewrite every reference-typed field value stored as plain handle
    /// text onto the permanent ids it names**, one write at a time.
    ///
    /// [`Self::compose_reference_fields`] cannot be this backstop: composing
    /// only ever resolves a BADGE, and a row from before this mechanism
    /// existed still holds a handle — `current_handle` finds no entity
    /// wearing one as a badge, so leaves it exactly as written until this
    /// runs. Not a migration proper, for the same reason
    /// [`Self::resolve_stale_pointer_columns`] is not one: it needs the
    /// badged entity list and the declared reference keys, which exist only
    /// once the store has already run its own boot steps.
    ///
    /// **Idempotent**: an item already lowered names no `kind:slug` handle
    /// this store has ever worn, so a second run finds nothing left to
    /// rewrite. An item that resolves through neither a current handle nor
    /// a former one stops the migration and is named, exactly as
    /// [`Self::resolve_stale_pointer_columns`] answers the same shape of
    /// failure — a person has to repair it rather than have it guessed at.
    ///
    /// Returns how many writes it rewrote.
    pub async fn migrate_reference_fields(&self) -> Result<usize, MemoryError> {
        let known: Vec<(String, String)> =
            sqlx::query_as("SELECT id, badge FROM entity WHERE badge IS NOT NULL")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        let mut resolve: std::collections::HashMap<String, String> = known.into_iter().collect();
        let former: Vec<(String, String)> = sqlx::query_as(
            "SELECT former_handle, badge FROM entity_former_handle \
             ORDER BY former_handle, ordinal DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        for (handle, badge) in former {
            resolve.entry(handle).or_insert(badge);
        }

        let type_rows = sqlx::query(
            "SELECT type_name, key_name, holds, folds, origin, required, one_of FROM type_field
             ORDER BY type_name, ordinal",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let declared = gather_types(&type_rows)?;
        let reference_keys: std::collections::HashSet<&str> = declared
            .iter()
            .flat_map(|d| &d.fields)
            .filter(|f| f.holds == jojobot_domain::memory::types::ValueType::Reference)
            .map(|f| f.key.as_str())
            .collect();
        if reference_keys.is_empty() {
            return Ok(0);
        }

        let rows: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT entity, `key`, ordinal, value FROM field_write WHERE value IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;

        let mut rewritten = 0;
        let mut unresolved: Vec<String> = Vec::new();
        for (entity, key, ordinal, value) in rows {
            if !reference_keys.contains(key.as_str()) {
                continue;
            }
            let field = declared
                .iter()
                .filter_map(|d| d.field(&key))
                .find(|f| f.holds == jojobot_domain::memory::types::ValueType::Reference)
                .expect("key was just found among reference_keys");
            let mut changed = false;
            let mut ok = true;
            let lowered: Vec<String> = field
                .items(&value)
                .into_iter()
                .map(|item| {
                    let item = item.trim();
                    match resolve.get(item) {
                        Some(badge) => {
                            changed = true;
                            badge.clone()
                        }
                        None if jojobot_domain::memory::EntityId(item.to_string())
                            .kind()
                            .is_none() =>
                        {
                            // Not handle-shaped at all — already a permanent
                            // id, or never a reference in the first place.
                            // Left exactly as written.
                            item.to_string()
                        }
                        None => {
                            ok = false;
                            unresolved.push(format!(
                                "field_write.value = '{item}' (key '{key}' on {entity})"
                            ));
                            item.to_string()
                        }
                    }
                })
                .collect();
            if !ok || !changed {
                continue;
            }
            sqlx::query(
                "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ? AND ordinal = ?",
            )
            .bind(lowered.join(", "))
            .bind(&entity)
            .bind(&key)
            .bind(ordinal)
            .execute(&self.pool)
            .await
            .map_err(store)?;
            rewritten += 1;
        }

        if unresolved.is_empty() {
            Ok(rewritten)
        } else {
            Err(MemoryError::Store(format!(
                "{} stored reference-field value{} cannot be resolved to anything this store has \
                 ever named, through a current handle or a former one — a person has to repair \
                 these before the migration can finish: {}",
                unresolved.len(),
                if unresolved.len() == 1 { "" } else { "s" },
                unresolved.join("; "),
            )))
        }
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
        let rows = self.index(tx).await?;
        Ok(self.extend_with_supplied(rows))
    }

    /// **[`guard::decide_existing`], without the listing a hit does not need.**
    /// A handle that answers directly, stored or supplied, is one targeted row
    /// and proceeds. Only a handle that answered to nothing builds
    /// [`known`](Self::known), because the candidates it is blocked with come
    /// from the whole roster.
    async fn decide_existing_targeted(
        &self,
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
    ) -> Result<guard::Decision, MemoryError> {
        if self.resolve_by_id(tx, handle).await?.is_some() {
            return Ok(guard::Decision::Proceed);
        }
        let index = self.known(tx).await?;
        Ok(guard::decide_existing(handle, &index))
    }

    /// Every rename event, inside the transaction a caller is already in.
    ///
    /// **Newest event first, within a former handle.** A former handle may
    /// carry more than one event — reused after a rename vacated it, or
    /// renamed away and back — and [`jojobot_domain::memory::resolve_handle`]
    /// takes the first match in this list. Ordering by `ordinal DESC` here is
    /// what makes that the newest one, the same newest-write-wins rule every
    /// other repeated write in this store follows.
    async fn former_handles_in(
        tx: &mut Transaction<'_, MySql>,
    ) -> Result<Vec<FormerHandle>, MemoryError> {
        let rows = sqlx::query(
            "SELECT former_handle, badge, changed_at FROM entity_former_handle \
             ORDER BY former_handle, ordinal DESC",
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
    ///
    /// **Builds no full listing.** Both tiers below are one targeted row
    /// each; only a caller who gets `None` back needs [`known`](Self::known)
    /// at all, to say what a name that answered to nothing resembles.
    async fn resolve(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &EntityId,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        if let Some(found) = self.resolve_by_id(tx, id).await? {
            return Ok(Some(found));
        }
        let former = Self::former_handles_in(tx).await?;
        let Some(badge) = former
            .iter()
            .find(|f| &f.former == id)
            .map(|f| f.badge.clone())
        else {
            return Ok(None);
        };
        self.resolve_by_badge(tx, &badge).await
    }

    /// **Tier one of [`resolve`](Self::resolve): an id that already answers
    /// directly, stored or supplied.** One row by primary key — mirrors
    /// [`jojobot_domain::memory::resolve_handle`]'s own first tier
    /// (`known.iter().find(|e| &e.id == id)`), expressed as SQL instead of an
    /// in-memory scan, so the common case never has to build the list that
    /// scan runs over.
    async fn resolve_by_id(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &EntityId,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        let row = sqlx::query("SELECT badge FROM entity WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?;
        if let Some(row) = row {
            let badge = row.try_get::<Option<String>, _>("badge").map_err(store)?;
            let key = badge.map(EntityId).unwrap_or_else(|| id.clone());
            return Ok(Some((key, id.clone())));
        }
        if let Some((entity, _)) = self.supplied.record_for(id) {
            let key = entity
                .badge
                .clone()
                .map(EntityId)
                .unwrap_or_else(|| entity.id.clone());
            return Ok(Some((key, entity.id.clone())));
        }
        Ok(None)
    }

    /// **Tier two of [`resolve`](Self::resolve): a former handle, resolved to
    /// whoever wears its badge today.** One row by badge — the same query
    /// [`current_handle`](Self::current_handle) already runs for the same
    /// reason, and the same second tier `resolve_handle` walks in memory
    /// (`known.iter().find(|e| e.badge == Some(badge))`).
    async fn resolve_by_badge(
        &self,
        tx: &mut Transaction<'_, MySql>,
        badge: &str,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT id FROM entity WHERE badge = ? ORDER BY id LIMIT 1")
                .bind(badge)
                .fetch_optional(&mut **tx)
                .await
                .map_err(store)?;
        if let Some((handle,)) = row {
            return Ok(Some((EntityId(badge.to_string()), EntityId(handle))));
        }
        for (entity, _) in self.supplied.records() {
            if entity.badge.as_deref() == Some(badge) {
                return Ok(Some((EntityId(badge.to_string()), entity.id.clone())));
            }
        }
        Ok(None)
    }

    /// **The other direction**: a stored key read back as whatever handle it
    /// answers to today, or kept as written when it wears nobody's badge.
    ///
    /// **One row, not the whole table.** [`entity_wearing`](jojobot_domain::memory::entity_wearing)
    /// finds the first entity in a list whose badge matches; the list this
    /// used to search was [`known`](Self::known) — every row in the store,
    /// resolved fresh on every single call. This is the same lookup a rename
    /// makes findable, `WHERE badge = ?`, checked against what the build
    /// supplies first because [`known`](Self::known) does too: a stored row
    /// wins a collision, but nothing stored collides with what nobody wrote.
    async fn current_handle(
        &self,
        tx: &mut Transaction<'_, MySql>,
        stored: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        let wearing: Option<String> =
            sqlx::query_scalar("SELECT id FROM entity WHERE badge = ? ORDER BY id LIMIT 1")
                .bind(stored.as_str())
                .fetch_optional(&mut **tx)
                .await
                .map_err(store)?;
        if let Some(id) = wearing {
            return Ok(EntityId(id));
        }
        for (entity, _) in self.supplied.records() {
            if entity.badge.as_deref() == Some(stored.as_str()) {
                return Ok(entity.id.clone());
            }
        }
        Ok(stored.clone())
    }

    /// **Every pointer-bearing field on a fact, lowered from served (handle)
    /// form to stored (badge) form, in one pass, unconditionally.**
    ///
    /// `read_fact` serves a record under today's handles — `home`,
    /// `subject`, `derived_from`, `edge`, `refs` and `stands_for` all come
    /// back resolved. A verb about to write that record back has to reverse
    /// every one of them before it does, or a field the write did not
    /// happen to touch is stored as the handle it was served under —
    /// correct until the pointed-to thing is renamed, and silently wrong
    /// after. `home` and `subject` are lowered by the caller, from the
    /// storage key it already resolved; this lowers the rest, the same way,
    /// from whatever `apply_fact_patch` left in place.
    ///
    /// Called unconditionally, right before the row is written, from both
    /// `update_fact` and `retract` (rule 268) — never behind a check on
    /// which field a patch touched, because that check is the defect.
    async fn lower_pointers(
        &self,
        tx: &mut Transaction<'_, MySql>,
        fact: &mut Fact,
        key: &EntityId,
    ) -> Result<(), MemoryError> {
        fact.home = key.clone();
        fact.subject = key.clone();
        if let Some(source) = &fact.derived_from {
            let resolved = self
                .resolve(tx, &source.home)
                .await?
                .map(|(key, _)| key)
                .unwrap_or_else(|| source.home.clone());
            fact.derived_from = Some(FactAddress::new(resolved, source.local.clone()));
        }
        if let Some(edge) = &fact.edge {
            let resolved = self
                .resolve(tx, &edge.object)
                .await?
                .map(|(key, _)| key)
                .unwrap_or_else(|| edge.object.clone());
            fact.edge = Some(Edge {
                shape: edge.shape,
                object: resolved,
            });
        }
        let mut lowered_refs = Vec::with_capacity(fact.refs.len());
        for object in &fact.refs {
            lowered_refs.push(
                self.resolve(tx, object)
                    .await?
                    .map(|(key, _)| key)
                    .unwrap_or_else(|| object.clone()),
            );
        }
        fact.refs = lowered_refs;
        let mut lowered_stands_for = Vec::with_capacity(fact.stands_for.len());
        for named in &fact.stands_for {
            let resolved = self
                .resolve(tx, &named.home)
                .await?
                .map(|(key, _)| key)
                .unwrap_or_else(|| named.home.clone());
            lowered_stands_for.push(FactAddress::new(resolved, named.local.clone()));
        }
        fact.stands_for = lowered_stands_for;
        Ok(())
    }

    /// **Every reference-typed field value, lowered from whatever handle a
    /// caller wrote to the permanent id the store keeps.** Each item was
    /// already checked to exist by the caller of this function, so
    /// `resolve` cannot miss on one; the fallback only covers a value that
    /// never looked like a handle in the first place.
    async fn lower_reference_fields(
        &self,
        tx: &mut Transaction<'_, MySql>,
        fields: &mut std::collections::BTreeMap<String, String>,
    ) -> Result<(), MemoryError> {
        let declared = Self::types_in(tx).await?;
        for (key, items) in jojobot_domain::memory::reference_field_values(fields, &declared) {
            let mut lowered = Vec::with_capacity(items.len());
            for item in items {
                let id = EntityId(item);
                let resolved = self
                    .resolve(tx, &id)
                    .await?
                    .map(|(key, _)| key)
                    .unwrap_or(id);
                lowered.push(resolved.to_string());
            }
            fields.insert(key, lowered.join(", "));
        }
        // **A handle under a key nobody declared is lowered too**, behind the
        // mark that tells a stored id from a word. Each item was already
        // checked to exist by the caller of this function.
        for (key, items) in jojobot_domain::memory::handle_field_values(fields, &declared) {
            let mut lowered = Vec::with_capacity(items.len());
            for item in items {
                let id = EntityId(item);
                let stored = self.resolve(tx, &id).await?.map_or(id, |(key, _)| key);
                lowered.push(jojobot_domain::memory::marked_stored_handle(
                    stored.as_str(),
                ));
            }
            fields.insert(key, lowered.join(", "));
        }
        Ok(())
    }

    /// **The other direction**: every reference-typed field value, composed
    /// from the permanent id it is stored as to the handle it answers to
    /// today — the mirror of [`Self::lower_reference_fields`], run once,
    /// right before a fields map reaches whoever asked for it.
    async fn compose_reference_fields(
        &self,
        tx: &mut Transaction<'_, MySql>,
        fields: &mut std::collections::BTreeMap<String, String>,
    ) -> Result<(), MemoryError> {
        let declared = Self::types_in(tx).await?;
        let referenced = jojobot_domain::memory::reference_field_values(fields, &declared);
        for (key, items) in &referenced {
            let mut composed = Vec::with_capacity(items.len());
            for item in items {
                // A key declared a reference AFTER a handle was stored under it
                // as an undeclared one holds that value behind the mark.
                let stored = jojobot_domain::memory::stored_handle_id(item).unwrap_or(item);
                composed.push(
                    self.current_handle(tx, &EntityId(stored.to_string()))
                        .await?
                        .to_string(),
                );
            }
            fields.insert(key.clone(), composed.join(", "));
        }
        // **Every other value, served as the handle it answers to now.** A value
        // stored behind the mark is the thing's id. One stored as plain handle
        // text, from before ids were kept, is resolved through the thing's
        // rename history, so it keeps pointing at the thing it named and is
        // lowered the next time the key is written.
        for (key, value) in fields.clone() {
            if referenced.iter().any(|(held, _)| held == &key) {
                continue;
            }
            let items: Vec<&str> = value.split(',').collect();
            if items
                .iter()
                .all(|item| jojobot_domain::memory::stored_handle_id(item).is_some())
            {
                let mut composed = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(id) = jojobot_domain::memory::stored_handle_id(item) {
                        composed.push(
                            self.current_handle(tx, &EntityId(id.to_string()))
                                .await?
                                .to_string(),
                        );
                    }
                }
                fields.insert(key, composed.join(", "));
            } else {
                let handles = jojobot_domain::memory::handles_in_value(&value);
                if handles.is_empty() {
                    continue;
                }
                let mut composed = Vec::with_capacity(handles.len());
                for id in handles {
                    composed.push(match self.resolve(tx, &id).await? {
                        Some((_, current)) => current.to_string(),
                        None => id.to_string(),
                    });
                }
                fields.insert(key, composed.join(", "));
            }
        }
        Ok(())
    }

    /// **The writes [`Self::append_writes`] is about to persist, with every
    /// reference-typed value lowered to the permanent id it names** — the
    /// same treatment [`Self::lower_reference_fields`] gives a whole fields
    /// map, applied to the delta a patch produces instead of the thing's
    /// whole state. A clear carries no value to lower.
    async fn lower_writes(
        &self,
        tx: &mut Transaction<'_, MySql>,
        writes: Vec<(String, Option<String>)>,
    ) -> Result<Vec<(String, Option<String>)>, MemoryError> {
        let mut out = Vec::with_capacity(writes.len());
        for (key, value) in writes {
            match value {
                None => out.push((key, None)),
                Some(value) => {
                    let mut one = std::collections::BTreeMap::new();
                    one.insert(key.clone(), value);
                    self.lower_reference_fields(tx, &mut one).await?;
                    out.push((key.clone(), one.remove(&key)));
                }
            }
        }
        Ok(out)
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

    async fn index(&self, tx: &mut Transaction<'_, MySql>) -> Result<Vec<Entity>, MemoryError> {
        self.index_listings
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let rows = sqlx::query(
            "SELECT id, kind, name, source, crm, parent, boot, merged_into, badge,
                    archived_reason, archived_at
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
        // **A parent pointer is badge-keyed exactly as `home` is** (rule
        // 268), resolved here on the way out. It names a DIFFERENT row, not
        // this one's own key, and nothing here queries by it: `children`
        // reads every entity and filters in memory. Resolved against a
        // snapshot taken before the loop mutates, since a row cannot lend
        // itself out while it is being written.
        let snapshot = entities.clone();
        for entity in &mut entities {
            if let Some(parent) = &entity.parent {
                entity.parent = Some(
                    jojobot_domain::memory::entity_wearing(parent.as_str(), &snapshot)
                        .map(|found| found.id.clone())
                        .unwrap_or_else(|| parent.clone()),
                );
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
        // **The newest write is asked for by ordering, never by a correlated
        // `MAX`.** The correlated form is evaluated once per write the claim
        // has, so its cost grows with the square of that count: a claim
        // rewritten on every write by a role holder renewing its lease took
        // 2.5 s at 2,000 writes where this takes 3 ms.
        let rows = sqlx::query(&format!(
            "SELECT {FACT_WRITE_COLUMNS} FROM fact_write w
             WHERE w.entity = ? AND w.fact_id = ?
             ORDER BY w.ordinal DESC LIMIT 1"
        ))
        .bind(address.home.as_str())
        .bind(address.local.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        Ok(self.assemble(tx, &rows).await?.pop())
    }

    /// **A creation, in a transaction its caller owns.** The caller commits it only
    /// when this answers `Written`. [`add_entity`](Memory::add_entity) is that, and so
    /// is the creation that writes its own first claim before it commits.
    async fn create_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
        new: NewEntity,
    ) -> Result<Guarded<Entity>, MemoryError> {
        let index = self.known(tx).await?;
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
            archived: None,
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
        // **Stored as the badge the parent wears, never the handle it was
        // named with** (rule 268). The check above already found it, so
        // `resolve` cannot miss here. Kept apart from `entity`, which still
        // carries the handle the caller sent and is served back exactly as
        // written.
        let stored = if let Some(parent) = &entity.parent {
            let stored_parent = self
                .resolve(tx, parent)
                .await?
                .map(|(key, _)| key)
                .unwrap_or_else(|| parent.clone());
            Entity {
                parent: Some(stored_parent),
                ..entity.clone()
            }
        } else {
            entity.clone()
        };
        let badge = write_entity(tx, &self.draw, &stored, &self.clock).await?;
        Ok(Guarded::Written(Entity {
            badge: Some(badge),
            ..entity
        }))
    }

    /// **A capture, in a transaction its caller owns.** The caller begins the
    /// transaction and commits it only when this answers `Written`, so a refusal
    /// or a block leaves nothing behind. [`capture`](Memory::capture) is that, and so
    /// is the creation that writes its own first claim.
    async fn capture_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
        fact: NewFact,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let standing = standing_of(&fact);

        // **What EXISTS**, which is the rows plus what the build supplies —
        // the same set the creation screen reads (rule 234).
        let index = self.known(tx).await?;
        // **A stale-but-renamed subject exists too.** The direct check is
        // what the guard already asks; a miss on it is checked again through
        // the thing's own rename history before it is called unknown. Kept,
        // rather than re-resolved, for its storage key below.
        let subject_resolved = self.resolve(tx, &fact.subject).await?;
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
        for object in referenced_by(&fact.fields, &Self::types_in(tx).await?) {
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
            let Some((source_key, _)) = self.resolve(tx, &source.home).await? else {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            };
            let resolved_source = FactAddress::new(source_key, source.local.clone());
            // **Archive is a visibility switch, not a validity gate** — a
            // source that is archived may still be cited. The claim must
            // exist; whether it still stands is the reader's judgment, and
            // the MCP layer names an archived source in the write's own
            // receipt so the judgment has something to work from.
            if self.read_fact(tx, &resolved_source).await?.is_none() {
                return Err(MemoryError::UnknownFact {
                    attempted: source.to_string(),
                    nearest: self.addresses_in(tx, &resolved_source.home).await?,
                });
            }
            Some(resolved_source)
        } else {
            None
        };

        // **What the subject is stored as** — the badge it wears, or its own
        // handle when it wears none. Already resolved above, direct or
        // through its rename history.
        let (home, subject_handle) = subject_resolved.expect("checked to exist just above");
        // **An edge's object and a ref are stored as the badge they wear,
        // never the handle they were named with** (rule 268). The existence
        // check above already found each one in `index`, so `resolve` cannot
        // miss here. Storing the badge is what makes a later handle
        // collision harmless: there is nothing left in the row for a
        // newcomer to inherit.
        let edge = match &fact.edge {
            Some(edge) => Some(Edge {
                shape: edge.shape,
                object: self
                    .resolve(tx, &edge.object)
                    .await?
                    .map(|(key, _)| key)
                    .unwrap_or_else(|| edge.object.clone()),
            }),
            None => None,
        };
        let mut refs = Vec::with_capacity(fact.refs.len());
        for object in &fact.refs {
            refs.push(
                self.resolve(tx, object)
                    .await?
                    .map(|(key, _)| key)
                    .unwrap_or_else(|| object.clone()),
            );
        }
        // **Off the subject's own kind, never off `home`** — that is a badge
        // now, and a badge carries no kind token to parse. Computed once,
        // ahead of the cap check that needs it and the fit guard further
        // down that already did.
        let subject_kind = index
            .iter()
            .find(|e| e.id == subject_handle)
            .expect("resolved above")
            .kind;
        // **A role's own claim is decided atomically with the write it
        // gates, against the state this same transaction reads**, in both
        // shapes. Only a write whose subject is a role object reads anything:
        // every other write costs nothing here.
        if let Some((role, state)) = self.role_state_of(tx, &subject_handle, &home).await?
            && let Some(refused) =
                jojobot_domain::memory::refuses_role_write(&role, None, &fact.fields, &state)
        {
            return Err(refused);
        }
        // **A ceiling's cardinality is enforced here, atomically with the
        // write it gates** — never as a separate call, because a drop with
        // nothing yet written in its place is a state the room must never
        // reach. **Structural, never a kind question**: a room's member is
        // an ordinary claim on the bound thing's own handle drawing a
        // `connection` edge, and capacity is an ordinary field on that
        // thing, folded like any other. A thing carrying none is uncapped —
        // this reaches every kind identically, the same rule the interface
        // above it already runs on.
        if edge
            .as_ref()
            .is_some_and(|e| e.shape == EdgeShape::Connection)
        {
            let held = Self::held_by(tx, &home).await?;
            // **The body cap, checked before the room has anything to say.**
            // A thought over its container's cap is refused whether or not
            // the room has space — see `refuses_thought_over_cap`.
            let cap = held
                .get(jojobot_domain::memory::THOUGHT_BODY_CAP)
                .and_then(|v| v.trim().parse::<usize>().ok());
            let capacity = held
                .get(jojobot_domain::memory::THOUGHT_CAPACITY)
                .and_then(|v| v.trim().parse::<usize>().ok());
            if let Some(err) = jojobot_domain::memory::refuses_thought_over_cap(
                &subject_handle,
                &jojobot_domain::memory::normalize_content(&fact.content),
                cap,
                capacity,
            ) {
                return Err(err);
            }
            if let Some(capacity) = capacity {
                let existing = self.facts_of(tx, &home).await?;
                let nominal_room = jojobot_domain::memory::thought_room(&existing);
                // **Ageing's own cost is paid only when the room might
                // actually be full** — a bot nowhere near capacity never
                // pays for a touch-moment lookup on every write.
                if nominal_room.len() >= capacity {
                    let ids: Vec<FactId> = nominal_room.iter().map(|f| f.id.clone()).collect();
                    let touched = Self::touched_moments(tx, &home, &ids).await?;
                    let split = jojobot_domain::memory::split_by_age(
                        nominal_room,
                        &touched,
                        fact.aged_before,
                    );
                    if split.live.len() >= capacity {
                        let aged_out = split.aged_out.len();
                        let refuse = |room: Vec<Fact>| MemoryError::RoomFull {
                            // **The handle the caller reached this thing
                            // by, never `home`** — `home` is the badge every
                            // entity wears from creation, and a refusal
                            // naming it hands back a token the caller
                            // cannot address anything by.
                            subject: subject_handle.to_string(),
                            live: room.len(),
                            capacity,
                            room,
                            aged_out,
                        };
                        let room = split.live;
                        match (&fact.drop, &fact.drop_because) {
                            (Some(victim), Some(reason)) => {
                                // **The victim must resolve to a live thought IN
                                // THIS ROOM** — not merely to a fact that exists,
                                // and not to one aged out of the count. Anything
                                // else is answered exactly as an empty drop is:
                                // refused, with the room shown, because a caller
                                // that named the wrong thing still needs to see
                                // what it could have named.
                                let in_room = self
                                    .resolve(tx, &victim.home)
                                    .await?
                                    .filter(|(key, _)| *key == home)
                                    .and_then(|_| {
                                        room.iter().find(|f| f.id == victim.local).cloned()
                                    });
                                let Some(mut victim_fact) = in_room else {
                                    return Err(refuse(room));
                                };
                                // **A thought that carries a key only its own
                                // relation may change is not droppable.** A
                                // capture carries no caller to ask who may change
                                // that key, and archiving the thought takes the
                                // key off the fold. Retracting it asks.
                                if !jojobot_domain::memory::guarded_keys_in(&victim_fact.fields)
                                    .is_empty()
                                {
                                    return Err(refuse(room));
                                }
                                // **`facts_of` serves under the current handle,
                                // never the storage key** (see `assemble`'s own
                                // comment) — exactly right for a reader, and
                                // exactly wrong for a row about to be written
                                // again. `lower_pointers` is the one seam
                                // `update_fact` and `retract` both already call
                                // for this: every pointer-bearing field at
                                // once — `.home`, `.subject`, `.edge`,
                                // `.derived_from`, `.refs` — never two of them
                                // by hand while the rest stay served.
                                self.lower_pointers(tx, &mut victim_fact, &home).await?;
                                // **Archived, through the one writer every
                                // archive goes through** — `write_fact` is what
                                // appends the claim's own write history; a
                                // status flip that skipped it would read back
                                // changed with nothing behind it saying when.
                                victim_fact.status = FactStatus::Archived;
                                victim_fact.details = Some(reason.clone());
                                Self::write_fact(tx, &victim_fact, &self.clock, None).await?;
                            }
                            // **The emergency reserve, spent — never twice
                            // in a row.** Only honoured at the exact
                            // threshold: a room already OVER capacity means
                            // a borrow already landed and was not repaid,
                            // and the ceiling's answer to that is steering
                            // back, not borrowing again.
                            _ if fact.borrow && room.len() == capacity => {}
                            _ => return Err(refuse(room)),
                        }
                    }
                }
            }
        }
        let id = Self::mint(tx, &home).await?;
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
            happened_through: fact.happened_through,
            edge,
            fields: fact.fields,
            refs,
            derived_from,
            // A capture never carries a mark — see [`NewFact`]; the mark is
            // an edit's to make, once the record it names already exists.
            stands_for: Vec::new(),
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
        let held = Self::writes_on(tx, &stored.home).await?;
        let declared = Self::types_in(tx).await?;
        // **The fold reads both halves and the guard reads one.** How a key
        // folds is declared by whoever declared it; what governs a thing is
        // its own kind, and nothing else — `subject_kind`, computed once
        // above, ahead of the cap check that also needs it.
        let governs = Self::kind_keys_in(tx, subject_kind.as_token()).await?;
        let columns = self.project_columns_of(tx, &index, &subject_handle).await?;
        guard_fit_in(
            subject_kind.as_token(),
            &folded_fields(&held, &declared),
            &stood_after_capture(&held, &stored, &declared),
            &governs,
            columns.as_deref(),
        )?;
        Self::write_fact(tx, &stored, &self.clock, fact.session.as_deref()).await?;
        // Every key this record carries is a write of its own, appended to the
        // history of that key on this thing. **A reference-typed value is
        // lowered to the permanent id it names first** (rule 268), guarded
        // above against the handle-form value the caller actually sent —
        // the guard has already run, so what lands here is free to be the
        // storage shape rather than the served one.
        let lowered_writes = self.lower_writes(tx, written_keys(&stored)).await?;
        Self::append_writes(tx, &stored.home, &stored.id, lowered_writes, &self.clock).await?;
        // **Served under the handle, stored under the key** — resolved
        // before the commit closes the transaction this needs to do it in.
        let served_derived_from = match &stored.derived_from {
            Some(source) => Some(FactAddress::new(
                self.current_handle(tx, &source.home).await?,
                source.local.clone(),
            )),
            None => None,
        };
        let served_edge = match &stored.edge {
            Some(edge) => Some(Edge {
                shape: edge.shape,
                object: self.current_handle(tx, &edge.object).await?,
            }),
            None => None,
        };
        let mut served_refs = Vec::with_capacity(stored.refs.len());
        for object in &stored.refs {
            served_refs.push(self.current_handle(tx, object).await?);
        }
        let mut served_fields = stored.fields.clone();
        self.compose_reference_fields(tx, &mut served_fields)
            .await?;
        Ok(Guarded::Written(Fact {
            home: subject_handle.clone(),
            subject: subject_handle,
            derived_from: served_derived_from,
            edge: served_edge,
            refs: served_refs,
            fields: served_fields,
            ..stored
        }))
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
            let stands_for = sqlx::query(
                "SELECT source_home, source_id FROM fact_stands_for
                 WHERE fact_home = ? AND fact_id = ? ORDER BY ordinal",
            )
            .bind(entity.as_str())
            .bind(id.as_str())
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?
            .iter()
            .map(|r| {
                FactAddress::new(
                    EntityId(r.get::<String, _>("source_home")),
                    FactId(r.get::<String, _>("source_id")),
                )
            })
            .collect();
            // **Served under the handle this storage key answers to today**
            // — `entity` here is the badge (or an unrenamed handle stored as
            // itself), never what a reader is shown.
            let handle = self.current_handle(tx, &entity).await?;
            let raw = fact_from(row, entity, id, fields, refs, stands_for)?;
            let mut served = raw;
            served.home = handle.clone();
            served.subject = handle;
            if let Some(source) = &served.derived_from {
                let resolved = self.current_handle(tx, &source.home).await?;
                served.derived_from = Some(FactAddress::new(resolved, source.local.clone()));
            }
            // **An edge and a ref are badge-keyed exactly as `home` is**
            // (rule 268), resolved here the same way, once, rather than a
            // reader chasing a handle's rename history on every use.
            if let Some(edge) = &served.edge {
                let resolved = self.current_handle(tx, &edge.object).await?;
                served.edge = Some(Edge {
                    shape: edge.shape,
                    object: resolved,
                });
            }
            let mut resolved_refs = Vec::with_capacity(served.refs.len());
            for object in &served.refs {
                resolved_refs.push(self.current_handle(tx, object).await?);
            }
            served.refs = resolved_refs;
            // **A mark's addresses are badge-keyed exactly as `derived_from`
            // is**, resolved here the same way.
            let mut resolved_stands_for = Vec::with_capacity(served.stands_for.len());
            for named in &served.stands_for {
                resolved_stands_for.push(FactAddress::new(
                    self.current_handle(tx, &named.home).await?,
                    named.local.clone(),
                ));
            }
            served.stands_for = resolved_stands_for;
            // **A reference-typed field value is a pointer like any
            // other** (rule 268), composed here too, the same way.
            self.compose_reference_fields(tx, &mut served.fields)
                .await?;
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

    /// **A fold with its `reports_to` served as the handle it answers to
    /// today**, by the one domain step every store's chart question goes
    /// through. The lookup is this store's own, read inside the write's
    /// transaction; the step is
    /// [`jojobot_domain::memory::with_manager_served`].
    async fn with_manager_rendered(
        &self,
        tx: &mut Transaction<'_, MySql>,
        fold: &std::collections::BTreeMap<String, String>,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let looked_up = match jojobot_domain::memory::stored_manager(fold) {
            Some(stored) => {
                // A manager folded into another is the one it became: the folded
                // row stays, forwarding, with its claims moved to the survivor.
                let mut at = self.current_handle(tx, &stored).await?;
                for _ in 0..jojobot_domain::memory::MAX_CHAIN {
                    let into: Option<String> =
                        sqlx::query_scalar("SELECT merged_into FROM entity WHERE id = ?")
                            .bind(at.as_str())
                            .fetch_optional(&mut **tx)
                            .await
                            .map_err(store)?
                            .flatten();
                    match into {
                        Some(next) if next != at.as_str() => at = EntityId(next),
                        _ => break,
                    }
                }
                Some(at)
            }
            None => None,
        };
        Ok(jojobot_domain::memory::with_manager_served(fold, |_| {
            looked_up.expect("a manager was stored, so it was looked up")
        }))
    }

    /// **The chart around a write, read inside the write's own transaction**:
    /// the bots above `subject`, the manager the write names and the bots above
    /// that one. Read in the transaction so no chart change can land between the
    /// question and the write it gates.
    async fn lineage_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
        subject: &EntityId,
        named: Option<EntityId>,
    ) -> Result<jojobot_domain::memory::Lineage, MemoryError> {
        let above = self.chain_above_in(tx, subject).await?;
        let above_named = match &named {
            Some(manager) => self.chain_above_in(tx, manager).await?,
            None => Vec::new(),
        };
        // Asked only by the write that names a manager for a thing with none.
        let has_reports = if above.is_empty() && named.is_some() {
            self.has_reports_in(tx, subject).await?
        } else {
            false
        };
        Ok(jojobot_domain::memory::Lineage {
            above,
            named,
            above_named,
            has_reports,
        })
    }

    /// **Whether any other thing reports to `subject`**, read inside the write's
    /// own transaction. The candidates are the things that ever wrote a manager;
    /// each is then folded and served as handles, as the chain is, so a manager
    /// that was taken back or moved does not count.
    async fn has_reports_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
        subject: &EntityId,
    ) -> Result<bool, MemoryError> {
        let Some((subject_key, subject_handle)) = self.resolve(tx, subject).await? else {
            return Ok(false);
        };
        let candidates: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT entity FROM field_write WHERE `key` = ? AND value IS NOT NULL",
        )
        .bind(jojobot_domain::memory::REPORTS_TO)
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        for candidate in candidates {
            let candidate = EntityId(candidate);
            if candidate == subject_key {
                continue;
            }
            let held = Self::held_by(tx, &candidate).await?;
            let served = self.with_manager_rendered(tx, &held).await?;
            if jojobot_domain::memory::reports_to(&served, &subject_handle) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The bots above `start` on its `reports_to` chain, nearest first.
    async fn chain_above_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
        start: &EntityId,
    ) -> Result<Vec<EntityId>, MemoryError> {
        let mut walk = jojobot_domain::memory::ChainWalk::from(start);
        while let Some(at) = walk.at().cloned() {
            let held = match self.resolve(tx, &at).await? {
                Some((key, _)) => Self::held_by(tx, &key).await?,
                None => Default::default(),
            };
            let held = self.with_manager_rendered(tx, &held).await?;
            walk.step(
                held.get(jojobot_domain::memory::REPORTS_TO)
                    .map(String::as_str),
            );
        }
        Ok(walk.above())
    }

    /// **A role object's lease state, read in both shapes inside the write's own
    /// transaction**, or `None` when the subject is no role. The role's own
    /// folded fields answer first; the holding bot's old `role/<role>/...` keys
    /// answer until the role carries a moment. The parent is read straight off
    /// the entity row, which keeps the badge it wears, so a write to anything
    /// else pays nothing here.
    async fn role_state_of(
        &self,
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
        key: &EntityId,
    ) -> Result<Option<(String, jojobot_domain::session::RoleState)>, MemoryError> {
        let Some(role) = jojobot_domain::memory::role_named_by(handle) else {
            return Ok(None);
        };
        let child = Self::held_by(tx, key).await?;
        let parent: Option<String> = sqlx::query_scalar("SELECT parent FROM entity WHERE id = ?")
            .bind(handle.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?
            .flatten();
        let bot = match parent {
            Some(parent) => Self::held_by(tx, &EntityId(parent)).await?,
            None => Default::default(),
        };
        let state = jojobot_domain::session::role_state(&role, Some(&child), &bot);
        Ok(Some((role, state)))
    }

    /// **The one place a row of `field_write` is changed after it was made, and
    /// the only key it may change.** A role's lease moment is machine state: the
    /// holder is alive, and what changes is when. Appending a write per renewal
    /// grew the claim for as long as the role was held, and nothing reads those
    /// writes back. So a renewal replaces the value of the newest `claimed_at`
    /// write on the role and adds nothing. A claim and a release still append, and
    /// so does every other key, which is what the contract's second role case
    /// holds. The row it changes is stamped with the store's clock, as an appended
    /// write is.
    ///
    /// `false` when the role has no live moment to overwrite (a role claimed in
    /// the old shape, whose first renewal on the object is an append): the caller
    /// then writes as it always did.
    async fn overwrite_lease_moment(
        tx: &mut Transaction<'_, MySql>,
        role: &EntityId,
        moment: &str,
        clock: &Clock,
    ) -> Result<bool, MemoryError> {
        let newest: Option<(i64, Option<String>)> = sqlx::query_as(
            "SELECT ordinal, value FROM field_write
             WHERE entity = ? AND `key` = ? ORDER BY ordinal DESC LIMIT 1",
        )
        .bind(role.as_str())
        .bind(jojobot_domain::session::ROLE_CLAIMED_AT)
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
        let Some((ordinal, Some(_))) = newest else {
            return Ok(false);
        };
        // **The row is stamped as the write it now is.** Every write of a key
        // carries the moment the store wrote it, and a merge places two writes
        // of one key by that stamp; a row that took the new value and kept the
        // old stamp would read as the older write it replaced.
        sqlx::query(
            "UPDATE field_write SET value = ?, written_at = ?
             WHERE entity = ? AND `key` = ? AND ordinal = ?",
        )
        .bind(moment)
        .bind(clock.now().to_string())
        .bind(role.as_str())
        .bind(jojobot_domain::session::ROLE_CLAIMED_AT)
        .bind(ordinal)
        .execute(&mut **tx)
        .await
        .map_err(store)?;
        Ok(true)
    }

    /// **The columns of the project a work item or project is filed under**, read inside
    /// the write's own transaction off the project's folded fields. `None` is a
    /// thing that is not work, one under no project, or one whose project lists
    /// none — each is held to the shipped five. `index` serves handles, so the
    /// parent is found by the handle the entity wears today.
    async fn project_columns_of(
        &self,
        tx: &mut Transaction<'_, MySql>,
        index: &[Entity],
        thing: &EntityId,
    ) -> Result<Option<Vec<String>>, MemoryError> {
        let Some(entity) = index.iter().find(|e| &e.id == thing) else {
            return Ok(None);
        };
        if !kinds::holds_columns(entity.kind.as_token()) {
            return Ok(None);
        }
        let Some(parent) = &entity.parent else {
            return Ok(None);
        };
        let is_project = index
            .iter()
            .any(|e| &e.id == parent && e.kind == EntityKind::PROJECT);
        if !is_project {
            return Ok(None);
        }
        let Some((key, _)) = self.resolve(tx, parent).await? else {
            return Ok(None);
        };
        Ok(jojobot_domain::memory::kinds::columns_of(
            &Self::held_by(tx, &key).await?,
        ))
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
        clock: &Clock,
    ) -> Result<(), MemoryError> {
        // **The link rows ride the write that made them**, in its transaction, so
        // a write and the question of who points at its targets never disagree.
        let declared = match wrote.iter().any(|(_, value)| value.is_some()) {
            true => Self::types_in(tx).await?,
            false => Vec::new(),
        };
        for (key, value) in wrote {
            let highest: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(ordinal) FROM field_write WHERE entity = ? AND `key` = ?",
            )
            .bind(entity.as_str())
            .bind(&key)
            .fetch_one(&mut **tx)
            .await
            .map_err(store)?;
            let ordinal = highest.unwrap_or(0) + 1;
            sqlx::query(
                "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(entity.as_str())
            .bind(&key)
            .bind(ordinal)
            .bind(value.as_deref())
            .bind(fact.as_str())
            .bind(clock.now().to_string())
            .execute(&mut **tx)
            .await
            .map_err(store)?;
            if let Some(value) = &value {
                for target in jojobot_domain::memory::field_link_targets(&key, value, &declared) {
                    sqlx::query(
                        "INSERT INTO field_link (entity, `key`, ordinal, target)
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(entity.as_str())
                    .bind(&key)
                    .bind(ordinal)
                    .bind(&target)
                    .execute(&mut **tx)
                    .await
                    .map_err(store)?;
                }
            }
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
        session: Option<&str>,
    ) -> Result<(), MemoryError> {
        sqlx::query(
            "REPLACE INTO fact (entity, id, content, details, provenance, standing, status,
                                recorded_at, happened_at, happened_through, edge_shape,
                                edge_object, derived_from, derived_from_id, inserted_at,
                                stale_after)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(fact.happened_through.map(|d| d.to_string()))
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

        // **Replaced whole, exactly as the row above and the refs above it
        // are.** Current state only — see the migration's own doc — so a
        // write that carries no mark leaves this fact standing for nothing,
        // the same way it leaves a capture or a retraction with none.
        sqlx::query("DELETE FROM fact_stands_for WHERE fact_home = ? AND fact_id = ?")
            .bind(fact.home.as_str())
            .bind(fact.id.as_str())
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        for (ordinal, named) in fact.stands_for.iter().enumerate() {
            sqlx::query(
                "INSERT INTO fact_stands_for (fact_home, fact_id, ordinal, source_home, source_id)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(fact.home.as_str())
            .bind(fact.id.as_str())
            .bind(ordinal as i64 + 1)
            .bind(named.home.as_str())
            .bind(named.local.as_str())
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        }
        Self::append_fact_write(tx, fact, clock, session).await?;
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
        session: Option<&str>,
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
                                     standing, status, recorded_at, happened_at,
                                     happened_through, edge_shape,
                                     edge_object,
                                     derived_from, derived_from_id, inserted_at, stale_after,
                                     written_at, session)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(fact.happened_through.map(|d| d.to_string()))
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
        .bind(session)
        .execute(&mut **tx)
        .await
        .map_err(store)?;
        Ok(())
    }

    /// **Each named claim's own last write moment, inside the ongoing
    /// transaction.**
    ///
    /// A minimal cousin of [`Self::claim_history`]: that reads the whole
    /// chain through the shared assembler, for a caller showing one claim's
    /// past. This reads one column, for as many claims as the cap check is
    /// weighing, and never opens a transaction of its own — the room's own
    /// count and each thought's own touch must agree on the instant they
    /// were read as of, and a second transaction cannot promise that.
    /// A claim with no write behind it, or one written before this column
    /// existed, is simply absent from the answer.
    async fn touched_moments(
        tx: &mut Transaction<'_, MySql>,
        home: &EntityId,
        ids: &[FactId],
    ) -> Result<std::collections::HashMap<FactId, jiff::Timestamp>, MemoryError> {
        let mut touched = std::collections::HashMap::with_capacity(ids.len());
        if ids.is_empty() {
            return Ok(touched);
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        // **The newest write per claim comes from a grouped join, the shape
        // `facts_projected` uses, and not from a correlated `MAX`**, which is
        // evaluated once per write a claim has and so grows with the square of
        // that count.
        let sql = format!(
            "SELECT w.fact_id, w.written_at FROM fact_write w
             JOIN (SELECT entity, fact_id, MAX(ordinal) AS newest FROM fact_write
                   WHERE entity = ? AND fact_id IN ({placeholders})
                   GROUP BY entity, fact_id) n
               ON n.entity = w.entity AND n.fact_id = w.fact_id AND n.newest = w.ordinal"
        );
        let mut query = sqlx::query(&sql).bind(home.as_str());
        for id in ids {
            query = query.bind(id.as_str());
        }
        let rows = query.fetch_all(&mut **tx).await.map_err(store)?;
        for row in rows {
            let fact_id: String = row.try_get("fact_id").map_err(store)?;
            let written_at = row
                .try_get::<Option<String>, _>("written_at")
                .map_err(store)?
                .and_then(|at| at.parse::<jiff::Timestamp>().ok());
            if let Some(at) = written_at {
                touched.insert(FactId(fact_id), at);
            }
        }
        Ok(touched)
    }

    /// **Move the duplicate's field writes onto the survivor, in the order they
    /// were made.**
    ///
    /// The fold of a thing's fields takes the newest write of each key by
    /// ordinal, so a merge that put the duplicate's writes after the survivor's
    /// would make the duplicate's value win whatever its age. Each key's two
    /// histories are interleaved by the moment the store stamped each write
    /// ([`interleave`]) and both sides are renumbered from one as one mapping,
    /// across `field_write` and `field_link`, which is keyed by the write's
    /// address. A claim's new id comes from `renamed`.
    ///
    /// **The move goes through a parked range.** The primary key is (thing, key,
    /// ordinal), so renumbering in place would collide half way. Every row is
    /// first set to its final ordinal plus [`PARKED`], a range nothing holds,
    /// then the whole range is brought down in one statement.
    async fn interleave_writes(
        tx: &mut Transaction<'_, MySql>,
        folded: &str,
        survivor: &str,
        renamed: &std::collections::HashMap<String, String>,
    ) -> Result<(), MemoryError> {
        let mut by_key: std::collections::BTreeMap<String, (Vec<StampedWrite>, Vec<StampedWrite>)> =
            std::collections::BTreeMap::new();
        for write in Self::stamped_writes(tx, folded).await? {
            by_key.entry(write.key.clone()).or_default().0.push(write);
        }
        for write in Self::stamped_writes(tx, survivor).await? {
            by_key.entry(write.key.clone()).or_default().1.push(write);
        }
        for (key, (theirs, ours)) in by_key {
            // A key only the survivor holds has nothing to interleave.
            if theirs.is_empty() {
                continue;
            }
            for (place, (side, write)) in interleave(&theirs, &ours).into_iter().enumerate() {
                let (from, fact_id) = match side {
                    Side::Folded => (
                        folded,
                        renamed.get(&write.fact_id).unwrap_or(&write.fact_id),
                    ),
                    Side::Survivor => (survivor, &write.fact_id),
                };
                let parked = place as i64 + 1 + PARKED;
                sqlx::query(
                    "UPDATE field_write SET entity = ?, fact_id = ?, ordinal = ? \
                     WHERE entity = ? AND `key` = ? AND ordinal = ?",
                )
                .bind(survivor)
                .bind(fact_id)
                .bind(parked)
                .bind(from)
                .bind(&key)
                .bind(write.ordinal)
                .execute(&mut **tx)
                .await
                .map_err(store)?;
                sqlx::query(
                    "UPDATE field_link SET entity = ?, ordinal = ? \
                     WHERE entity = ? AND `key` = ? AND ordinal = ?",
                )
                .bind(survivor)
                .bind(parked)
                .bind(from)
                .bind(&key)
                .bind(write.ordinal)
                .execute(&mut **tx)
                .await
                .map_err(store)?;
            }
        }
        for table in ["field_write", "field_link"] {
            sqlx::query(&format!(
                "UPDATE {table} SET ordinal = ordinal - ? WHERE entity = ? AND ordinal >= ?"
            ))
            .bind(PARKED)
            .bind(survivor)
            .bind(PARKED)
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        }
        Ok(())
    }

    /// Every field write on one thing with the moment the store stamped it,
    /// each key's writes in ordinal order. **A stamp that does not parse reads as
    /// no stamp**, the same as a write that was never stamped.
    async fn stamped_writes(
        tx: &mut Transaction<'_, MySql>,
        entity: &str,
    ) -> Result<Vec<StampedWrite>, MemoryError> {
        let rows = sqlx::query(
            "SELECT `key`, ordinal, fact_id, written_at FROM field_write \
             WHERE entity = ? ORDER BY `key`, ordinal",
        )
        .bind(entity)
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut writes = Vec::with_capacity(rows.len());
        for row in &rows {
            let key: String = row.try_get("key").map_err(store)?;
            let stamp: Option<String> = row.try_get("written_at").map_err(store)?;
            let written_at = stamp.and_then(|text| match text.parse::<jiff::Timestamp>() {
                Ok(at) => Some(at),
                Err(e) => {
                    tracing::warn!(error = %e, entity, key, "a write's stamp does not parse; it reads as unstamped");
                    None
                }
            });
            writes.push(StampedWrite {
                ordinal: row.try_get("ordinal").map_err(store)?,
                fact_id: row.try_get("fact_id").map_err(store)?,
                key,
                written_at,
            });
        }
        Ok(writes)
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
        gather_types(&rows)
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
        gather_types(&rows)
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
                            recorded_at, happened_at, happened_through, edge_shape, edge_object, \
                            derived_from, derived_from_id, \
                            inserted_at, stale_after";

/// The same columns off the write table, with its key aliased to what
/// [`DoltMemory::assemble`] reads. **The alias is the whole difference**: a
/// second assembler would be a second place for the row shape to drift.
const FACT_WRITE_COLUMNS: &str = "w.entity, w.fact_id AS id, w.content, w.details, w.provenance, \
                                  w.standing, w.status, w.recorded_at, w.happened_at, \
                                  w.happened_through, \
                                  w.edge_shape, \
                                  w.edge_object, w.derived_from, w.derived_from_id, \
                                  w.inserted_at, w.stale_after";

/// The first ordinal of the range a merge parks field writes in while it
/// renumbers them: far above any count of writes of one key.
const PARKED: i64 = 1 << 40;

/// One field write as a merge orders it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StampedWrite {
    key: String,
    ordinal: i64,
    fact_id: String,
    /// When the store stamped the write. `None` on a write an older build
    /// appended, which has no moment of its own.
    written_at: Option<jiff::Timestamp>,
}

/// Which thing a write came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Folded,
    Survivor,
}

/// **Two histories of one key, interleaved oldest first.**
///
/// Each side is already in the order it was written, and that order is kept:
/// the two are merged, not sorted, so a clock that stepped back inside one
/// thing's history cannot reorder it. A write with no stamp is older than every
/// stamped one. **On a tie the survivor's write comes after the duplicate's,
/// two unstamped writes included**, so the survivor wins: it is the thing kept.
fn interleave<'a>(
    folded: &'a [StampedWrite],
    survivor: &'a [StampedWrite],
) -> Vec<(Side, &'a StampedWrite)> {
    let mut merged = Vec::with_capacity(folded.len() + survivor.len());
    let (mut theirs, mut ours) = (folded.iter().peekable(), survivor.iter().peekable());
    loop {
        match (theirs.peek(), ours.peek()) {
            (Some(their), Some(our)) => {
                if their.written_at <= our.written_at {
                    merged.push((Side::Folded, theirs.next().expect("peeked")));
                } else {
                    merged.push((Side::Survivor, ours.next().expect("peeked")));
                }
            }
            (Some(_), None) => merged.push((Side::Folded, theirs.next().expect("peeked"))),
            (None, Some(_)) => merged.push((Side::Survivor, ours.next().expect("peeked"))),
            (None, None) => return merged,
        }
    }
}

/// A store failure, in the domain's own words. **The server's account never
/// crosses** — no SQL, no table names, no product (rule 53); it goes to the log
/// where an operator debugging a real failure wants it.
///
/// **A conflict is told apart from every other failure here, at the one seam
/// every write already passes through.** SQLSTATE `40001` is Dolt's own
/// optimistic concurrency catching two writes that landed on the same
/// instant — the store answered correctly and promptly, so folding it into
/// the same bucket as a store that could not be reached at all would tell a
/// caller to escalate the commonest, most self-healing case exactly as it
/// would a genuine outage.
///
/// **A write the store refused on a rule it enforces is told apart too.** A
/// duplicate key, a reference to nothing, a required value left out or a failed
/// check is the store answering promptly and correctly, so the words for an
/// outage would send a caller to retry a write that can never land. The answer
/// carries the kind of rule and nothing of the server's own account.
fn store(e: sqlx::Error) -> MemoryError {
    if e.as_database_error().and_then(|db| db.code()).as_deref() == Some("40001") {
        tracing::warn!(error = %e, "a write conflicted with another landing the same instant");
        return MemoryError::Conflict;
    }
    if let Some(rule) = super::refused_rule(&e) {
        tracing::error!(error = %e, rule, "the memory store refused a write on a rule it enforces");
        return MemoryError::Refused(rule.to_string());
    }
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

/// The kind a stored handle names, read off the handle itself.
///
/// **The two ways a stored handle fails to name a kind are not one failure**
/// (rule 68). A process that loaded no set cannot read the most ordinary
/// handle in the store, and reporting that as a damaged record sends a
/// reader after damage that is not there — and names a repair only a person
/// can perform, while the repair is a boot. So the never-loaded answer is
/// the same refusal the write half of this rail gives, in the same words.
fn kind_of_handle(id: &EntityId) -> Result<EntityKind, MemoryError> {
    kinds::resolve(id.kind_token()).map_err(|why| match why {
        NotAKind::SetNeverLoaded => MemoryError::KindsNeverLoaded {
            attempted: Some(id.to_string()),
        },
        // A row whose kind nobody declares really is a record this process
        // cannot read, and it stays that. The sentence names no kinds: the set
        // is data (rule 213).
        NotAKind::NotDeclared { .. } => unreadable("its handle names no kind"),
    })
}

fn entity_from(row: &sqlx::mysql::MySqlRow, aliases: Vec<String>) -> Result<Entity, MemoryError> {
    let id = EntityId(row.try_get::<String, _>("id").map_err(store)?);
    let kind = kind_of_handle(&id)?;
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
        archived: {
            let reason = row
                .try_get::<Option<String>, _>("archived_reason")
                .map_err(store)?;
            let at = row
                .try_get::<Option<String>, _>("archived_at")
                .map_err(store)?
                .and_then(|stamp| stamp.parse().ok());
            // **Both halves or neither.** A row carrying one without the
            // other is damage no caller's write can produce — `write_entity`
            // always sets them together — so it reads as not archived rather
            // than inventing the missing half.
            reason.zip(at).map(|(reason, at)| Archived { reason, at })
        },
    })
}

fn fact_from(
    row: &sqlx::mysql::MySqlRow,
    entity: EntityId,
    id: FactId,
    fields: std::collections::BTreeMap<String, String>,
    refs: Vec<EntityId>,
    stands_for: Vec<FactAddress>,
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
        happened_through: row
            .try_get::<Option<String>, _>("happened_through")
            .map_err(store)?
            .and_then(|day| day.parse().ok()),
        edge,
        fields,
        refs,
        derived_from,
        stands_for,
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

/// **The checks a new claim faces before any store is asked.** Shared by the
/// capture and by the creation that writes its own first claim.
fn validate_new_fact(fact: &NewFact) -> Result<(), MemoryError> {
    validate_write_subject(&fact.subject)?;
    validate_content(&fact.content)?;
    if let Some(edge) = &fact.edge {
        validate_edge(edge)?;
    }
    validate_happened_span(fact.happened_at, fact.happened_through)?;
    validate_fields(&fact.fields)?;
    validate_provenance_source(fact.provenance, &fact.fields)?;
    Ok(())
}

/// **The checks a new entity faces before any store is asked.**
fn validate_new_entity(new: &NewEntity) -> Result<(), MemoryError> {
    validate_entity(
        &new.id,
        &new.name,
        &new.aliases,
        &new.source,
        new.crm.as_deref(),
        new.parent.as_ref(),
    )
}

#[async_trait]
impl Memory for DoltMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        validate_new_entity(&new)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let created = self.create_in(&mut tx, new).await?;
        if matches!(created, Guarded::Written(_)) {
            tx.commit().await.map_err(store)?;
        }
        Ok(created)
    }

    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        if first.subject != new.id {
            return Err(MemoryError::InvalidFact(format!(
                "the first claim is about {}, and the thing being made is {}",
                first.subject, new.id
            )));
        }
        validate_new_entity(&new)?;
        validate_new_fact(&first)?;
        // **One transaction for both writes.** A refusal or a block of the claim
        // returns before the commit, so the entity is never stored without it.
        let mut tx = self.pool.begin().await.map_err(store)?;
        let entity = match self.create_in(&mut tx, new).await? {
            Guarded::Written(entity) => entity,
            Guarded::Blocked {
                attempted,
                candidates,
            } => {
                return Ok(Guarded::Blocked {
                    attempted,
                    candidates,
                });
            }
        };
        match self.capture_in(&mut tx, first).await? {
            Guarded::Written(fact) => {
                tx.commit().await.map_err(store)?;
                Ok(Guarded::Written((entity, fact)))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(Guarded::Blocked {
                attempted,
                candidates,
            }),
        }
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let all = self.index(&mut tx).await?;
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
        let rows = self.index(&mut tx).await?;
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
        //
        // **`entity.parent` came off `index`, which serves the handle**
        // (rule 268). `EntityPatch` carries no field that could have changed
        // it, so it is resolved back to the badge here before the row is
        // written — or every metadata edit would quietly turn a badge-keyed
        // parent back into a handle.
        let stored = if let Some(parent) = &entity.parent {
            let stored_parent = self
                .resolve(&mut tx, parent)
                .await?
                .map(|(key, _)| key)
                .unwrap_or_else(|| parent.clone());
            Entity {
                parent: Some(stored_parent),
                ..entity.clone()
            }
        } else {
            entity.clone()
        };
        write_entity(&mut tx, &self.draw, &stored, &self.clock).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(entity))
    }

    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        validate_write_subject(id)?;
        validate_field("reason", reason)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **A row, never a supplied record** — the same exception
        // `update_entity` reads off: this mutates a stored row, and a
        // build-shipped record has none to mutate.
        let rows = self.index(&mut tx).await?;
        let Some(mut entity) = rows.iter().find(|e| &e.id == id).cloned() else {
            if self.supplied.record_for(id).is_some() {
                return Err(MemoryError::SuppliedHandle {
                    attempted: id.to_string(),
                });
            }
            return Err(MemoryError::UnknownEntity {
                attempted: id.to_string(),
                nearest: guard::screen(id, &[], &rows),
            });
        };
        // A forwarding row is not a thing to archive, for the same reason it
        // is not a side to fold or a survivor to fold into.
        if let Some(into) = &entity.merged_into {
            return Err(MemoryError::AlreadyMerged {
                attempted: id.to_string(),
                into: into.to_string(),
            });
        }
        if entity.archived.is_some() {
            return Err(MemoryError::AlreadyArchived {
                attempted: id.to_string(),
            });
        }
        entity.archived = Some(jojobot_domain::memory::Archived {
            reason: reason.trim().to_string(),
            at: self.clock.now(),
        });
        // **The badge rides on the record this edit was read from**, exactly
        // as `update_entity` carries it — no name changed, so there is
        // nothing for the guard to screen.
        let stored = if let Some(parent) = &entity.parent {
            let stored_parent = self
                .resolve(&mut tx, parent)
                .await?
                .map(|(key, _)| key)
                .unwrap_or_else(|| parent.clone());
            Entity {
                parent: Some(stored_parent),
                ..entity.clone()
            }
        } else {
            entity.clone()
        };
        write_entity(&mut tx, &self.draw, &stored, &self.clock).await?;
        tx.commit().await.map_err(store)?;
        Ok(entity)
    }

    async fn restore_entity(&self, id: &EntityId) -> Result<(Entity, Archived), MemoryError> {
        validate_write_subject(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **A row, never a supplied record**, for the reason archiving reads
        // the same way: a build-shipped record has no row to change.
        let rows = self.index(&mut tx).await?;
        let Some(mut entity) = rows.iter().find(|e| &e.id == id).cloned() else {
            if self.supplied.record_for(id).is_some() {
                return Err(MemoryError::SuppliedHandle {
                    attempted: id.to_string(),
                });
            }
            return Err(MemoryError::UnknownEntity {
                attempted: id.to_string(),
                nearest: guard::screen(id, &[], &rows),
            });
        };
        if let Some(into) = &entity.merged_into {
            return Err(MemoryError::AlreadyMerged {
                attempted: id.to_string(),
                into: into.to_string(),
            });
        }
        let Some(was) = entity.archived.take() else {
            return Err(MemoryError::NotArchived {
                attempted: id.to_string(),
            });
        };
        // The parent rides as the key the row stores, exactly as archiving
        // writes it back.
        let stored = if let Some(parent) = &entity.parent {
            let stored_parent = self
                .resolve(&mut tx, parent)
                .await?
                .map(|(key, _)| key)
                .unwrap_or_else(|| parent.clone());
            Entity {
                parent: Some(stored_parent),
                ..entity.clone()
            }
        } else {
            entity.clone()
        };
        write_entity(&mut tx, &self.draw, &stored, &self.clock).await?;
        tx.commit().await.map_err(store)?;
        Ok((entity, was))
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
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **A row, never a supplied record** — the same exception
        // `update_entity` reads off: a rename mutates a stored row, and a
        // build-shipped record has none to mutate.
        let rows = self.index(&mut tx).await?;
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
        // **A same handle only reparents when the parent actually
        // differs** — compared as storage keys, the same rule the fake's
        // copy of this guard carries, so a former spelling of the parent it
        // already has is recognised as no change too. An unresolvable new
        // parent is never "unchanged": that is the near-miss guard's own
        // question, asked below with candidates rather than with this
        // refusal.
        if from == to {
            let target_key = match &parent {
                Some(new_parent) => self.resolve(&mut tx, new_parent).await?.map(|(key, _)| key),
                None => None,
            };
            let current_key = match &entity.parent {
                Some(current) => self.resolve(&mut tx, current).await?.map(|(key, _)| key),
                None => None,
            };
            let unchanged = match &parent {
                None => true,
                Some(_) => target_key.is_some() && target_key == current_key,
            };
            if unchanged {
                return Err(MemoryError::NothingToRename {
                    attempted: from.to_string(),
                });
            }
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

        // **A row without a badge cannot leave a forwarding row behind, and
        // that is a readable error rather than a panic** — the same
        // condition, and the same sentence, [`Self::scan`] and
        // [`Self::scan_entity`] already answer: a row predating the badge
        // column, or a fill that has not reached it yet. Checked before
        // anything moves, so a doomed rename never half-executes.
        let Some(badge) = entity.badge.clone() else {
            return Err(MemoryError::Store(format!(
                "{} carries no badge, so its document has no id — the startup fill did not \
                 reach it",
                entity.id
            )));
        };
        // **What is actually written is the badge** (rule 268): unchanged
        // when this rename named no new parent (`effective_parent` came off
        // `entity.parent`, already served through it once, so it is
        // resolved back), or resolved fresh when it did — the guard just
        // above already found it, so `resolve` cannot miss here.
        let stored_parent = match &renamed.parent {
            Some(parent) => Some(
                self.resolve(&mut tx, parent)
                    .await?
                    .map(|(key, _)| key)
                    .unwrap_or_else(|| parent.clone()),
            ),
            None => None,
        };
        // **A straight primary-key rewrite, never delete-then-insert.**
        // Every other column — the badge above all — rides along untouched:
        // nothing here asks "what was this row's badge" the way
        // `write_entity`'s REPLACE would if handed a row that already
        // carries the new id, which is exactly the shape that would mint a
        // fresh one and sever it from everything the old one wore.
        sqlx::query("UPDATE entity SET id = ?, kind = ?, parent = ? WHERE id = ?")
            .bind(to.as_str())
            .bind(to.kind_token())
            .bind(stored_parent.as_ref().map(EntityId::as_str))
            .bind(from.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        // **Nothing else needs to move.** A parent pointer is a badge now;
        // the badge this row wears never changes across a rename, so a
        // child naming it as `parent` needs no sweep — the former,
        // handle-rewriting version of this statement touched every other
        // entity's `parent` column for exactly the reason this no longer
        // does.
        //
        // **This former handle may already carry an event** — nothing
        // reserves a handle a rename vacates, so it may have been claimed by
        // something else and renamed away in turn, and a rename may return
        // a handle to one it already left — so the ordinal is this former
        // handle's own next one, the same [`Self::append_writes`] pattern,
        // never an assumption that this is its first.
        //
        // **A same-handle reparent is not a former handle of itself** — the
        // handle never moved, so there is nothing here for a later resolve
        // to walk back through.
        if from != to {
            let highest: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(ordinal) FROM entity_former_handle WHERE former_handle = ?",
            )
            .bind(from.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(store)?;
            sqlx::query(
                "INSERT INTO entity_former_handle (former_handle, badge, changed_at, ordinal) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(from.as_str())
            .bind(&badge)
            .bind(date.to_string())
            .bind(highest.unwrap_or(0) + 1)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        }
        append_entity_write(&mut tx, to, &self.clock).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(renamed))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        validate_new_fact(&fact)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let written = self.capture_in(&mut tx, fact).await?;
        if matches!(written, Guarded::Written(_)) {
            tx.commit().await.map_err(store)?;
        }
        Ok(written)
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        // An unknown entity is a miss with its near candidates — never an
        // empty page. Empty-but-real and nonexistent are different answers.
        // A stale-but-renamed handle is not this miss: it resolves through
        // its own history to the one storage key its claims were ever filed
        // under, so this is one lookup rather than a walk of every handle it
        // has worn.
        //
        // **The full listing is built only here, on the miss** — see
        // `fields`'s own comment for why.
        let Some((key, _)) = self.resolve(&mut tx, subject).await? else {
            let index = self.known(&mut tx).await?;
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
        // An unknown entity is a miss with its near candidates, exactly as a
        // recall of one is: a thing nobody has written a key on and a handle
        // nobody created are different answers with different repairs. A
        // stale-but-renamed handle resolves through its own history, same as
        // every other lookup here.
        //
        // **The full listing is built only here, on the miss.** `resolve`
        // above answers a hit from one targeted row; a caller here pays for
        // the whole store only when it has to say what a name that answered
        // to nothing resembles.
        let Some((key, _)) = self.resolve(&mut tx, entity).await? else {
            let index = self.known(&mut tx).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        let mut held = Self::held_by(&mut tx, &key).await?;
        self.compose_reference_fields(&mut tx, &mut held).await?;
        tx.commit().await.map_err(store)?;
        Ok(held)
    }

    /// **The same read as [`fields`](Self::fields), plus a count from the
    /// ledger nothing here ever removes a row from.**
    ///
    /// `fact_write` keeps every write of every claim, forever — a
    /// correction is a new row beside the one it corrects. The one exception is
    /// a role's lease moment, which a renewal overwrites and signals through
    /// `entity_write` instead ([`Self::overwrite_lease_moment`]), so the count
    /// is the claim writes plus the signal rows filed under the entity's
    /// storage key. It only grows, and it grows by at least one for every write that could have
    /// changed what [`fields`](Self::fields) answers. Read in the SAME
    /// transaction as the fields it counts for, so the two halves of the answer
    /// describe the same instant rather than two reads a write could land
    /// between.
    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(std::collections::BTreeMap<String, String>, u64), MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let Some((key, _)) = self.resolve(&mut tx, entity).await? else {
            let index = self.known(&mut tx).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        let mut held = Self::held_by(&mut tx, &key).await?;
        self.compose_reference_fields(&mut tx, &mut held).await?;
        let written: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact_write WHERE entity = ?")
            .bind(key.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(store)?;
        // **A renewal of a lease adds no claim write**, only a signal row filed
        // under the entity's storage key (see [`Self::overwrite_lease_moment`]),
        // so the claim writes alone would leave the version where it was while the
        // moment moved. Rows only ever join that table, so the sum only grows.
        let signalled: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entity_write WHERE entity = ?")
                .bind(key.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(store)?;
        tx.commit().await.map_err(store)?;
        Ok((held, (written + signalled) as u64))
    }

    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        // A miss on the HANDLE is an entity miss, exactly as every other
        // addressed read answers one. A stale-but-renamed handle resolves
        // through its own history, same as every other lookup here.
        //
        // **The full listing is built only on a miss** — two miss branches
        // below can reach one, each building its own: a handle that resolves
        // to nothing, or a resolved handle with no writes under this fact id.
        let Some((key, handle)) = self.resolve(&mut tx, &address.home).await? else {
            let index = self.known(&mut tx).await?;
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
            let index = self.known(&mut tx).await?;
            if let Some(resolved) = index.iter().find(|e| e.id == handle)
                && let Some(err) = jojobot_domain::memory::already_merged(&address.home, resolved)
            {
                return Err(err);
            }
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
                Vec::new(),
            )?;
            // **A lineage pointer in the chain is a `FactAddress` like any
            // other** — stored under its source's key, served under the
            // handle that key answers to today.
            if let Some(source) = &fact.derived_from {
                let resolved = self.current_handle(&mut tx, &source.home).await?;
                fact.derived_from = Some(FactAddress::new(resolved, source.local.clone()));
            }
            if let Some(edge) = &fact.edge {
                let resolved = self.current_handle(&mut tx, &edge.object).await?;
                fact.edge = Some(Edge {
                    shape: edge.shape,
                    object: resolved,
                });
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
        self.counted_targeted_read();
        let mut tx = self.pool.begin().await.map_err(store)?;
        // The full listing is built only on the miss — see `fields`'s own
        // comment for why.
        let Some((key, _)) = self.resolve(&mut tx, entity).await? else {
            let index = self.known(&mut tx).await?;
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
            let mut fact = fact_from(
                row,
                key.clone(),
                id.clone(),
                Default::default(),
                Vec::new(),
                Vec::new(),
            )?;
            if let Some(source) = &fact.derived_from {
                let resolved = self.current_handle(&mut tx, &source.home).await?;
                fact.derived_from = Some(FactAddress::new(resolved, source.local.clone()));
            }
            if let Some(edge) = &fact.edge {
                let resolved = self.current_handle(&mut tx, &edge.object).await?;
                fact.edge = Some(Edge {
                    shape: edge.shape,
                    object: resolved,
                });
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
        // An unknown entity is a miss with its near candidates, exactly as a
        // recall of one is: a key nobody wrote and a handle nobody created are
        // different answers with different repairs. A stale-but-renamed
        // handle resolves through its own history, same as every other
        // lookup here.
        //
        // The full listing is built only here, on the miss.
        let Some((storage_key, _)) = self.resolve(&mut tx, entity).await? else {
            let index = self.known(&mut tx).await?;
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
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        // An edge's object names an entity, so an edit that attaches one is an
        // entity-touching write and faces the guard — screened before anything
        // is rewritten. **What EXISTS includes what the build supplies**, for
        // the same reason `capture` reads it: an edge may point at a record the
        // build supplies. The check is one targeted row; the full listing is
        // built only for a name that answered to nothing.
        if let Some(edge) = &patch.edge {
            validate_edge(edge)?;
            if let guard::Decision::Block(candidates) =
                self.decide_existing_targeted(&mut tx, &edge.object).await?
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
            if let guard::Decision::Block(candidates) =
                self.decide_existing_targeted(&mut tx, &object).await?
            {
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
            let index = self.known(&mut tx).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let resolved_address = FactAddress::new(key.clone(), address.local.clone());
        let Some(mut fact) = self.read_fact(&mut tx, &resolved_address).await? else {
            let index = self.known(&mut tx).await?;
            if let Some(resolved) = index.iter().find(|e| e.id == handle)
                && let Some(err) = jojobot_domain::memory::already_merged(&address.home, resolved)
            {
                return Err(err);
            }
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: self.addresses_in(&mut tx, &key).await?,
            });
        };
        // **`read_fact` serves the handle form, for the readers it usually
        // answers.** This one writes the row back, so `home` and `subject`
        // are lowered to the storage key it was read under before anything
        // touches it — writing the handle would file the edit under a
        // different primary key and strand it there, unread by anything
        // keyed on the badge. The other pointer fields are lowered once,
        // unconditionally, in [`Self::lower_pointers`], right before the
        // row is written — not here, because the patch below may still
        // rewrite any of them.
        fact.home = key.clone();
        fact.subject = key.clone();
        // An archived row is out of reach of an ordinary edit — one-way is
        // enforced here rather than intended elsewhere.
        if fact.status == FactStatus::Archived {
            return Err(MemoryError::NotRetractable {
                attempted: address.to_string(),
                why: "it is archived, and an archived record is not editable — archiving is \
                      one-way. Capture what is so now as a new record"
                    .to_string(),
            });
        }
        // **A role's own claim, renewed or re-claimed by patch, is decided
        // atomically with the write it gates** — the same check `capture`
        // runs, against the same state under the same transaction, so a
        // renewal and a fresh claim are one mechanism rather than two.
        let role_state = self.role_state_of(&mut tx, &handle, &key).await?;
        if let Some((role, state)) = &role_state
            && let Some(refused) = jojobot_domain::memory::refuses_role_write(
                role,
                patch.role_move.as_ref(),
                &patch.fields,
                state,
            )
        {
            return Err(refused);
        }
        // **A renewal overwrites the lease moment and appends nothing.** See
        // [`Self::overwrite_lease_moment`] for the one exception this is. A claim
        // that also carries an edge, a source, a ref or a mark is served the
        // ordinary way, below, so this answers only for the plain claim a role
        // is made of.
        if role_state.is_some()
            && patch
                .role_move
                .as_ref()
                .is_some_and(|m| m.kind == jojobot_domain::session::RoleMoveKind::Renew)
            && fact.edge.is_none()
            && fact.derived_from.is_none()
            && fact.refs.is_empty()
            && fact.stands_for.is_empty()
            && let Some(moment) = patch.fields.get(jojobot_domain::session::ROLE_CLAIMED_AT)
            && Self::overwrite_lease_moment(&mut tx, &key, moment, &self.clock).await?
        {
            // The signal `write_summary` reads, so the search index sees the new
            // moment as a change although no claim write was added.
            append_entity_write(&mut tx, &key, &self.clock).await?;
            tx.commit().await.map_err(store)?;
            let mut fields = fact.fields.clone();
            fields.insert(
                jojobot_domain::session::ROLE_CLAIMED_AT.to_string(),
                moment.clone(),
            );
            return Ok(Guarded::Written(Fact {
                fields,
                home: handle.clone(),
                subject: handle,
                ..fact
            }));
        }
        // What the ADDRESSED RECORD carries right now — read off before the
        // patch rewrites it, because that is what decides which of the patch's
        // clears is a write and which names a key this record never had.
        let carried = fact.fields.clone();
        // **Whether this record was already a thought, read before the patch
        // rewrites it.** The archived check just above guarantees `status` is
        // `Active` here, so a connection edge is the whole of the question.
        // This is what tells an edit that merely touches an EXISTING thought
        // apart from one that MAKES a claim into a thought — the cap and the
        // room only have anything to say about the second.
        let was_thought = fact
            .edge
            .as_ref()
            .is_some_and(|e| e.shape == EdgeShape::Connection);
        // **Every claim a mark names faces the existence rule a source
        // does** — a mark is a set of citations, and a citation to nothing
        // is exactly the failure `derived_from` is screened against below,
        // just plural.
        if let Some(stands_for) = &patch.stands_for {
            for named in stands_for {
                let Some((named_key, _)) = self.resolve(&mut tx, &named.home).await? else {
                    // **The listing is built for a name that answered to
                    // nothing**: the candidates come from the whole roster.
                    let index = self.index(&mut tx).await?;
                    return Err(MemoryError::UnknownEntity {
                        attempted: named.home.to_string(),
                        nearest: guard::screen(&named.home, &[], &index),
                    });
                };
                let resolved_named = FactAddress::new(named_key.clone(), named.local.clone());
                let Some(named_fact) = self.read_fact(&mut tx, &resolved_named).await? else {
                    return Err(MemoryError::UnknownFact {
                        attempted: named.to_string(),
                        nearest: self.addresses_in(&mut tx, &named_key).await?,
                    });
                };
                // **Self-reference is checked on the same storage key the
                // record being edited already resolved to** — never on the
                // name the patch sent, which is not yet in that key space
                // and would let a self-reference through unnoticed whenever
                // a badge is in play.
                if named_key == key && named.local == address.local {
                    return Err(MemoryError::InvalidFact(format!(
                        "a record cannot be marked as standing for itself: {named} is its own \
                         address"
                    )));
                }
                // **One layer, so the pile is always one step away.** A
                // record already marked as standing for others is itself a
                // shape, and a shape cannot be folded into another mark: that
                // would let a walk that unfolded a shape find a second shape
                // underneath, and a read one layer deep would leave the
                // pile's bottom permanently out of reach.
                if !named_fact.stands_for.is_empty() {
                    return Err(MemoryError::InvalidFact(format!(
                        "{named} already stands for its own sources, so it cannot be folded into \
                         another mark: a shape may only name sources that are not themselves \
                         shapes"
                    )));
                }
            }
        }
        // **A source named by an EDIT faces the rule a source named at capture
        // faces.** Lineage is learned late, so it is checked here too — and a
        // pointer at a claim nobody wrote would be a link that reads as
        // evidence and leads nowhere. The resolved form is not kept here —
        // [`Self::lower_pointers`], below, lowers whatever `apply_fact_patch`
        // leaves in `fact.derived_from`, touched by this patch or not.
        if let Some(source) = &patch.derived_from {
            let Some((source_key, _)) = self.resolve(&mut tx, &source.home).await? else {
                // **The listing is built for a name that answered to nothing**:
                // the candidates come from the whole roster.
                let index = self.index(&mut tx).await?;
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            };
            let resolved_source = FactAddress::new(source_key, source.local.clone());
            // **Archive is a visibility switch, not a validity gate** — a
            // source that is archived may still be cited; see the same note
            // in `capture`.
            if self.read_fact(&mut tx, &resolved_source).await?.is_none() {
                return Err(MemoryError::UnknownFact {
                    attempted: source.to_string(),
                    // **The resolved key, not the raw handle** — `addresses_in`
                    // reads by storage key, exactly as `capture`'s own check
                    // does; passing the handle here always came back empty.
                    nearest: self.addresses_in(&mut tx, &resolved_source.home).await?,
                });
            }
        }
        // **Whose words these are, asked of the claim's FIRST write.** A later
        // write by another session does not make it the owner.
        let first_session: Option<String> = sqlx::query_scalar(
            "SELECT session FROM fact_write WHERE entity = ? AND fact_id = ?
             ORDER BY ordinal ASC LIMIT 1",
        )
        .bind(key.as_str())
        .bind(address.local.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(store)?
        .flatten();
        let patch = jojobot_domain::memory::settle_rewrite(
            address,
            &fact,
            patch,
            first_session.as_deref(),
        )?;
        apply_fact_patch(&mut fact, &patch)?;
        // **Every pointer-bearing field, lowered in one pass, unconditionally**
        // (rule 268) — `apply_fact_patch` carries whatever the patch named, or
        // whatever `read_fact` served, and both are handle form. Not three
        // more conditionals on which field the patch happened to touch: that
        // is the defect, not a workaround for it.
        self.lower_pointers(&mut tx, &mut fact, &key).await?;
        // **The thing's fields as they will stand, against the thing's fields
        // as they stand now.** A write may not drop a thing below a type it
        // already fits; a thing that fits nothing has nothing to protect, so
        // its records stay repairable. One function, called from both stores.
        let held = Self::writes_on(&mut tx, &fact.home).await?;
        let declared = Self::types_in(&mut tx).await?;
        // **Off the entity's own kind, never off `fact.home`** — that is a
        // badge now, and a badge carries no kind token to parse.
        let kind = kind_of_handle(&handle)?;
        let governs = Self::kind_keys_in(&mut tx, kind.as_token()).await?;
        let before_fold = folded_fields(&held, &declared);
        let after_fold = stood_after(&held, &fact, &patch, &carried, &declared);
        let columns = if kinds::holds_columns(kind.as_token()) {
            let index = self.index(&mut tx).await?;
            self.project_columns_of(&mut tx, &index, &handle).await?
        } else {
            None
        };
        // **The type check reads the folds as handles, as a caller does.** A
        // reference key holds the permanent id of what it names, and an edit that
        // takes a claim back leaves the fold on an older claim's stored id, which
        // is no handle and breaks the key's type. Served first, the check asks
        // whether the value is an entity of the kind the key names.
        let mut served_before = before_fold.clone();
        self.compose_reference_fields(&mut tx, &mut served_before)
            .await?;
        let mut served_after = after_fold.clone();
        self.compose_reference_fields(&mut tx, &mut served_after)
            .await?;
        guard_fit_in(
            kind.as_token(),
            &served_before,
            &served_after,
            &governs,
            columns.as_deref(),
        )?;
        // **The ceiling and the room, both on the state this edit leaves
        // behind, atomically with the write that would leave it.** See
        // `refuses_unlicensed_change` and `refuses_room_overflow` in the
        // domain crate for why the fold rather than the raw patch, and why
        // no ageing here.
        let chart_before = self.with_manager_rendered(&mut tx, &before_fold).await?;
        let chart_after = self.with_manager_rendered(&mut tx, &after_fold).await?;
        let lineage =
            match jojobot_domain::memory::chart_wanted_by_change(&chart_before, &chart_after) {
                Some(named) => Some(self.lineage_in(&mut tx, &handle, named).await?),
                None => None,
            };
        if let Some(err) = jojobot_domain::memory::refuses_unlicensed_change(
            &handle,
            caller,
            &chart_before,
            &chart_after,
            lineage.as_ref(),
        ) {
            return Err(err);
        }
        let becomes_thought = fact.status == FactStatus::Active
            && fact
                .edge
                .as_ref()
                .is_some_and(|e| e.shape == EdgeShape::Connection);
        // **The cap runs on a content change, or on newly becoming a
        // thought — never on an edit that leaves an existing thought's
        // content exactly as it was.** An edit seeded over the cap, or over
        // capacity, before either check existed must still take an edit
        // that does not touch the content the cap protects or the room
        // membership the capacity protects.
        let capacity = after_fold
            .get(jojobot_domain::memory::THOUGHT_CAPACITY)
            .and_then(|v| v.trim().parse::<usize>().ok());
        if becomes_thought && (patch.content.is_some() || !was_thought) {
            // **The body cap, checked before the room has anything to say.**
            // A thought over its container's cap is refused whether or not
            // the room has space — see `refuses_thought_over_cap`.
            let cap = after_fold
                .get(jojobot_domain::memory::THOUGHT_BODY_CAP)
                .and_then(|v| v.trim().parse::<usize>().ok());
            if let Some(err) = jojobot_domain::memory::refuses_thought_over_cap(
                &handle,
                &fact.content,
                cap,
                capacity,
            ) {
                return Err(err);
            }
        }
        // **The room only grows when a record newly joins it.** An edit to
        // a thought already in the room does not grow it, whatever else the
        // edit changes — a room already over capacity (a borrow, or seeded
        // that way) is an allowed state an unrelated edit must not re-judge.
        if becomes_thought && !was_thought {
            // **The storage key, not the display handle** — `facts_of` reads
            // by the badge a row is filed under (see its own doc comment),
            // exactly as `writes_on` above is called with `fact.home` rather
            // than `handle`.
            let others: Vec<Fact> = self
                .facts_of(&mut tx, &key)
                .await?
                .into_iter()
                .filter(|f| f.id != fact.id)
                .collect();
            let mut room = jojobot_domain::memory::thought_room(&others);
            let mut aged_out = 0;
            // **Ageing's cost is paid only when the room might be full**,
            // exactly as `capture` pays it, and the cutoff is the caller's.
            if capacity.is_some_and(|capacity| room.len() >= capacity) {
                let ids: Vec<FactId> = room.iter().map(|f| f.id.clone()).collect();
                let touched = Self::touched_moments(&mut tx, &key, &ids).await?;
                let split = jojobot_domain::memory::split_by_age(room, &touched, patch.aged_before);
                aged_out = split.aged_out.len();
                room = split.live;
            }
            if let Some(err) =
                jojobot_domain::memory::refuses_room_overflow(&handle, room, capacity, aged_out)
            {
                return Err(err);
            }
        }
        Self::write_fact(&mut tx, &fact, &self.clock, patch.session.as_deref()).await?;
        // **The edit appends.** The record reads back changed — that is the
        // surface — and the value it replaced stays where it was written.
        // **A reference-typed value is lowered to the permanent id it
        // names first** (rule 268) — the guard just above already checked
        // the handle-form value the patch sent, so what lands here is free
        // to be the storage shape.
        let lowered_writes = self
            .lower_writes(&mut tx, writes_of(&patch, &carried))
            .await?;
        Self::append_writes(&mut tx, &fact.home, &fact.id, lowered_writes, &self.clock).await?;
        // Read back from the substrate rather than from what the patch
        // believed, so the answer is the projection a later read will give.
        let mut fields = Self::fields_of(&mut tx, &fact.home, &fact.id).await?;
        self.compose_reference_fields(&mut tx, &mut fields).await?;
        // **Served under the handle, stored under the key.**
        let served_derived_from = match &fact.derived_from {
            Some(source) => Some(FactAddress::new(
                self.current_handle(&mut tx, &source.home).await?,
                source.local.clone(),
            )),
            None => None,
        };
        let served_edge = match &fact.edge {
            Some(edge) => Some(Edge {
                shape: edge.shape,
                object: self.current_handle(&mut tx, &edge.object).await?,
            }),
            None => None,
        };
        let mut served_stands_for = Vec::with_capacity(fact.stands_for.len());
        for named in &fact.stands_for {
            served_stands_for.push(FactAddress::new(
                self.current_handle(&mut tx, &named.home).await?,
                named.local.clone(),
            ));
        }
        // **Refs are stored as permanent ids, exactly as the edge is**, so
        // they are served the same way: answering with the stored form hands
        // the caller opaque ids for the handles it wrote.
        let mut served_refs = Vec::with_capacity(fact.refs.len());
        for object in &fact.refs {
            served_refs.push(self.current_handle(&mut tx, object).await?);
        }
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(Fact {
            fields,
            home: handle.clone(),
            subject: handle,
            derived_from: served_derived_from,
            edge: served_edge,
            refs: served_refs,
            stands_for: served_stands_for,
            ..fact
        }))
    }

    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
    ) -> Result<Merge, MemoryError> {
        // **Wrong by kind alone, before either side's existence is even
        // asked.** A fold naming a session steps past the same three things
        // any other memory write onto its handle would.
        validate_write_subject(folded)?;
        validate_write_subject(survivor)?;
        if folded == survivor {
            return Err(MemoryError::NothingToMerge {
                attempted: folded.to_string(),
            });
        }
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **Rows only, deliberately** — a fold mutates stored rows, and a
        // build-supplied record has none to mutate. The same exception
        // `rename_entity`'s own existence check reads off.
        let index = self.index(&mut tx).await?;
        // **Everything is decided before anything moves.** A fold rewrites rows
        // across every table that holds a handle, so a refusal discovered
        // half-way through is the one outcome this must not produce.
        for side in [folded, survivor] {
            let Some(held) = index.iter().find(|e| &e.id == side) else {
                // **A supplied record is real and still not a row** — reported
                // apart from an ordinary miss (rule 234): the handle reads,
                // lists and searches as existing, so `UnknownEntity` here
                // would tell the caller in the next breath that it does not.
                if self.supplied.record_for(side).is_some() {
                    return Err(MemoryError::SuppliedHandle {
                        attempted: side.to_string(),
                    });
                }
                let known = self.extend_with_supplied(index.clone());
                return Err(MemoryError::UnknownEntity {
                    attempted: side.to_string(),
                    nearest: guard::screen(side, &[], &known),
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
        let (folded_key, folded_handle) =
            self.resolve(&mut tx, folded).await?.expect("checked above");
        let (survivor_key, survivor_handle) = self
            .resolve(&mut tx, survivor)
            .await?
            .expect("checked above");

        // **A merge into the caller's own bot cannot carry a ceiling onto
        // it**, decided over what the duplicate holds, read inside this
        // transaction so no write can land between the question and the move.
        let carried = Self::held_by(&mut tx, &folded_key).await?;
        let carried = self.with_manager_rendered(&mut tx, &carried).await?;
        let lineage = match jojobot_domain::memory::needs_the_chart(&carried) {
            Some(named) => Some(self.lineage_in(&mut tx, &survivor_handle, named).await?),
            None => None,
        };
        if let Some(err) = jojobot_domain::memory::refuses_merge_carrying(
            caller,
            &survivor_handle,
            &folded_handle,
            &carried,
            lineage.as_ref(),
        ) {
            return Err(err);
        }
        // **And not into a bot that sits below it**, which no key it carries
        // shows: the loop is made by the forwarding alone. The chain is read in
        // this transaction, so no chart change can land between the question and
        // the move.
        let above_survivor = self.chain_above_in(&mut tx, &survivor_handle).await?;
        if let Some(err) = jojobot_domain::memory::refuses_merge_below_the_duplicate(
            &survivor_handle,
            &folded_handle,
            &above_survivor,
        ) {
            return Err(err);
        }
        // **The duplicate's thoughts become the survivor's, so they meet the
        // survivor's room and body cap before anything moves** — the same
        // question `capture` asks of a thought, asked of every one that
        // arrives. Read inside this transaction, so no write can land between
        // the question and the move.
        let incoming = self.facts_of(&mut tx, &folded_key).await?;
        if !jojobot_domain::memory::thought_room(&incoming).is_empty() {
            let held = Self::held_by(&mut tx, &survivor_key).await?;
            let room =
                jojobot_domain::memory::thought_room(&self.facts_of(&mut tx, &survivor_key).await?);
            if let Some(err) = jojobot_domain::memory::refuses_merge_into_room(
                survivor,
                &incoming,
                room,
                held.get(jojobot_domain::memory::THOUGHT_CAPACITY)
                    .and_then(|v| v.trim().parse::<usize>().ok()),
                held.get(jojobot_domain::memory::THOUGHT_BODY_CAP)
                    .and_then(|v| v.trim().parse::<usize>().ok()),
            ) {
                return Err(err);
            }
        }

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
        // **A field write is not moved claim by claim.** Its key is (thing, key,
        // ordinal), so a write cannot land on the survivor until its place in the
        // key's history is settled, and that place depends on every write of the
        // key on both sides. The claims move here, and the id each one is given on
        // the survivor is kept so the writes can take it when they move after.
        let moving: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM fact WHERE entity = ? ORDER BY CAST(SUBSTRING(id, 2) AS UNSIGNED)",
        )
        .bind(folded_key.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let rehomed = moving.len();
        let mut renamed: std::collections::HashMap<String, String> =
            std::collections::HashMap::with_capacity(moving.len());
        for was in moving {
            let now = Self::mint(&mut tx, &survivor_key).await?;
            renamed.insert(was.clone(), now.as_str().to_string());
            for statement in [
                "UPDATE fact SET entity = ?, id = ? WHERE entity = ? AND id = ?",
                "UPDATE fact SET derived_from = ?, derived_from_id = ? \
                 WHERE derived_from = ? AND derived_from_id = ?",
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
                // **A mark moves with the row it marks, and a mark that
                // NAMED the moved row follows it too** — the same two
                // directions `derived_from` gets above, just on the mark's
                // own table.
                "UPDATE fact_stands_for SET fact_home = ?, fact_id = ? \
                 WHERE fact_home = ? AND fact_id = ?",
                "UPDATE fact_stands_for SET source_home = ?, source_id = ? \
                 WHERE source_home = ? AND source_id = ?",
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
        // **The writes move last, once every claim has its new id**, interleaved
        // with the survivor's by the moment each was made.
        Self::interleave_writes(
            &mut tx,
            folded_key.as_str(),
            survivor_key.as_str(),
            &renamed,
        )
        .await?;

        // **An edge's object and a ref's entity are stored as the badge the
        // folded side wears** (rule 268), so these compare against that
        // badge alone and rewrite to the survivor's — never a handle, and
        // never a former handle, because nothing but the entity's own
        // current badge was ever written into either column.
        for statement in [
            "UPDATE fact SET edge_object = ? WHERE edge_object = ?",
            "UPDATE fact_write SET edge_object = ? WHERE edge_object = ?",
            "UPDATE fact_event_ref SET entity = ? WHERE entity = ?",
        ] {
            sqlx::query(statement)
                .bind(survivor_key.as_str())
                .bind(folded_key.as_str())
                .execute(&mut *tx)
                .await
                .map_err(store)?;
        }

        // **`entity.parent` names a different row and is stored as the
        // badge it wears** (rule 268), so this compares against the folded
        // side's own badge alone and rewrites to the survivor's — the same
        // shape the edge and ref sweeps above use, and for the same reason:
        // nothing but the entity's own current badge was ever written into
        // this column.
        sqlx::query("UPDATE entity SET parent = ? WHERE parent = ?")
            .bind(survivor_key.as_str())
            .bind(folded_key.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;

        // **The duplicate's names go to the survivor**: every alias it had and
        // the name it was displayed under, so a name it was known by now finds
        // the survivor. The survivor keeps its own name and aliases, and its own
        // page; the duplicate's page stays on the row that forwards. A name the
        // survivor already answers to is not added twice.
        let folded_row = index
            .iter()
            .find(|e| &e.id == folded)
            .expect("checked present above");
        let survivor_row = index
            .iter()
            .find(|e| &e.id == survivor)
            .expect("checked present above");
        let carried = jojobot_domain::memory::names_to_carry(
            &survivor_row.name,
            &survivor_row.aliases,
            &folded_row.name,
            &folded_row.aliases,
        );
        if !carried.is_empty() {
            let mut ordinal: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(ordinal), 0) FROM entity_alias WHERE entity = ?",
            )
            .bind(survivor_key.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(store)?;
            for alias in &carried {
                ordinal += 1;
                sqlx::query("INSERT INTO entity_alias (entity, ordinal, alias) VALUES (?, ?, ?)")
                    .bind(survivor_key.as_str())
                    .bind(ordinal)
                    .bind(alias)
                    .execute(&mut *tx)
                    .await
                    .map_err(store)?;
            }
            sqlx::query("DELETE FROM entity_alias WHERE entity = ?")
                .bind(folded_key.as_str())
                .execute(&mut *tx)
                .await
                .map_err(store)?;
            append_entity_write(&mut tx, survivor, &self.clock).await?;
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
            happened_through: account.happened_through,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            stands_for: Vec::new(),
            inserted_at: Some(self.clock.now()),
            stale_after: None,
        };
        Self::write_fact(&mut tx, &record, &self.clock, None).await?;
        Self::append_writes(
            &mut tx,
            &record.home,
            &record.id,
            written_keys(&record),
            &self.clock,
        )
        .await?;

        // **The folded row stays and starts forwarding.** Written last, so a
        // failure anywhere above rolls back a row that still says it is a thing
        // rather than one pointing at a fold that did not happen.
        sqlx::query("UPDATE entity SET merged_into = ? WHERE id = ?")
            .bind(survivor.as_str())
            .bind(folded.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        // **One row, not one per entity the sweep above touched.** The log
        // exists to answer "has anything changed", never "which entity" —
        // see the migration's own doc — so a single write here is enough to
        // make a merge visible to it, whatever the sweep's fan-out was.
        append_entity_write(&mut tx, folded, &self.clock).await?;

        // **Read back rather than reconstructed.** The fold may have moved the
        // survivor's own row — a folded parent is re-pointed above — so the
        // answer is what the store now holds and not what this call assembled.
        let survived = self
            .index(&mut tx)
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
        caller: &EntityId,
    ) -> Result<Retraction, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **The full listing is built only on a miss** — two miss branches
        // below can reach one, each building its own: a handle that resolves
        // to nothing, or a resolved handle with no fact at this address.
        let Some((key, handle)) = self.resolve(&mut tx, &address.home).await? else {
            let index = self.index(&mut tx).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let resolved_address = FactAddress::new(key.clone(), address.local.clone());
        // Everything is decided before anything moves, so a refusal leaves the
        // row exactly as it was.
        let Some(mut target) = self.read_fact(&mut tx, &resolved_address).await? else {
            let index = self.index(&mut tx).await?;
            if let Some(resolved) = index.iter().find(|e| e.id == handle)
                && let Some(err) = jojobot_domain::memory::already_merged(&address.home, resolved)
            {
                return Err(err);
            }
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: self.addresses_in(&mut tx, &key).await?,
            });
        };
        // **Lowered back to the storage form it was read under, every
        // pointer-bearing field at once** (rule 268) — this row is about to
        // be written again, twice: once retracted, once as the account
        // built from it. Neither write-back may leave a served handle
        // behind in a column that answers to the badge.
        self.lower_pointers(&mut tx, &mut target, &key).await?;
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
            subject: key.clone(),
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            recorded_at: account.recorded_at,
            happened_at: account.happened_at,
            happened_through: account.happened_through,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            stands_for: Vec::new(),
            // A retraction is a record in its own right, taken in now.
            inserted_at: Some(self.clock.now()),
            stale_after: None,
        };
        let retracted = Fact {
            status: FactStatus::Archived,
            ..target
        };
        // **The ceiling, on the state this retraction leaves behind.** A
        // retraction carries no `FactPatch` of its own, so `stood_after`
        // takes an empty one — the only thing it needs from the patch is
        // the record's own new status, which `retracted` already carries.
        let held = Self::writes_on(&mut tx, &key).await?;
        let declared = Self::types_in(&mut tx).await?;
        let before_fold = folded_fields(&held, &declared);
        let after_fold = stood_after(
            &held,
            &retracted,
            &FactPatch::default(),
            &Default::default(),
            &declared,
        );
        let chart_before = self.with_manager_rendered(&mut tx, &before_fold).await?;
        let chart_after = self.with_manager_rendered(&mut tx, &after_fold).await?;
        let lineage =
            match jojobot_domain::memory::chart_wanted_by_change(&chart_before, &chart_after) {
                Some(named) => Some(self.lineage_in(&mut tx, &handle, named).await?),
                None => None,
            };
        if let Some(err) = jojobot_domain::memory::refuses_unlicensed_change(
            &handle,
            caller,
            &chart_before,
            &chart_after,
            lineage.as_ref(),
        ) {
            return Err(err);
        }
        Self::write_fact(&mut tx, &retracted, &self.clock, None).await?;
        Self::write_fact(&mut tx, &record, &self.clock, None).await?;
        // The account is a record like any other, and the key naming what it
        // takes back is a write of its own. The record being taken back writes
        // no key: what changed there is its status.
        Self::append_writes(
            &mut tx,
            &record.home,
            &record.id,
            written_keys(&record),
            &self.clock,
        )
        .await?;
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
        self.counted_targeted_read();
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **The same gate every read beside it keeps** (rule 234): the rows
        // plus what the build supplies. The port derives this read from
        // `fields` and `history`, and both of those miss a handle nobody has,
        // so a store that overrides it owes that answer too. An empty map says
        // nobody has written a key here, which is a different answer from
        // there is no such thing and has a different repair.
        //
        // The full listing is built only on the miss.
        let Some((key, _)) = self.resolve(&mut tx, entity).await? else {
            let index = self.known(&mut tx).await?;
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
        self.counted_targeted_read();
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

    /// **An index seek, because the link table holds who points at what.** Each
    /// field write leaves a row per thing its value names, so the holders are the
    /// entities with a row whose target is this thing's id. Only those are read
    /// back, rather than the whole index, and no value is scanned.
    ///
    /// The rows say which entities to read and nothing more. Which of their
    /// records still carry the handle is decided from the records themselves,
    /// so a key that was written and later overwritten does not come back as a
    /// pointer that is no longer there.
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        validate_subject(target)?;
        self.counted_targeted_read();
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **The rows hold the permanent id**, and a caller's handle may be a
        // former one, so it is resolved first.
        let key = match self.resolve(&mut tx, target).await? {
            Some((key, _)) => key,
            None => target.clone(),
        };
        let declared = Self::types_in(&mut tx).await?;
        let rows = sqlx::query("SELECT DISTINCT entity FROM field_link WHERE target = ?")
            .bind(key.as_str())
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?;
        let mut pointing = Vec::new();
        for row in rows {
            let holder = EntityId(row.try_get::<String, _>("entity").map_err(store)?);
            for fact in self.facts_of(&mut tx, &holder).await? {
                if jojobot_domain::memory::fields_name_target(&fact.fields, &declared, target) {
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
        let index = self.index(&mut tx).await?;
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
        append_entity_write(&mut tx, entity, &self.clock).await?;
        tx.commit().await.map_err(store)?;
        Ok(stored)
    }

    /// **An entity is its own document here**, so its handle is the honest
    /// answer to "which document is this": there is no page to open and no
    /// second identifier to invent.
    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let entities = self.index(&mut tx).await?;
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
            let mut fields = Self::held_by(&mut tx, &key).await?;
            self.compose_reference_fields(&mut tx, &mut fields).await?;
            scanned.push(search::DocScan {
                doc_id: badge,
                title: entity.name.clone(),
                prose,
                facts: self.facts_of(&mut tx, &key).await?,
                // What the thing IS travels with the doc: the records beside it
                // cannot be folded back into it.
                fields,
                entity: Some(entity),
                owner: None,
            });
        }
        tx.commit().await.map_err(store)?;
        Ok(scanned)
    }

    /// **Two aggregates, not a read of either table's rows — plus two
    /// hashes, which are not either.** [`Self::scan`] pays for every entity
    /// and fact body it returns; the aggregates pay for neither —
    /// `entity_write` and `fact_write` are written on every mutation to
    /// each (see their own migrations' docs), so their count and newest
    /// moment answer "has anything changed through this application"
    /// without touching a body.
    ///
    /// **The hashes answer the question the aggregates cannot: has anything
    /// changed at all.** `DOLT_HASHOF_TABLE` is the table's own
    /// content-addressed root — maintained on every write to the table
    /// itself, application or not, and read as a lookup of an already-
    /// current value rather than recomputed by scanning. It is what makes a
    /// row removed or edited directly against `entity` or `fact` — rule 60's
    /// own supported path for a record to leave — visible to a signal built
    /// from an audit log that write never touches.
    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let (entity_count, entity_latest): (i64, Option<String>) =
            sqlx::query_as("SELECT COUNT(*), MAX(written_at) FROM entity_write")
                .fetch_one(&mut *tx)
                .await
                .map_err(store)?;
        let (fact_count, fact_latest): (i64, Option<String>) =
            sqlx::query_as("SELECT COUNT(*), MAX(written_at) FROM fact_write")
                .fetch_one(&mut *tx)
                .await
                .map_err(store)?;
        let entity_hash: String = sqlx::query_scalar("SELECT DOLT_HASHOF_TABLE('entity')")
            .fetch_one(&mut *tx)
            .await
            .map_err(store)?;
        // `fact_write` rather than `fact`: `facts_projected`'s own doc says
        // a claim read answers from the newest write, never from the
        // claim's own row — so `fact_write` is the table a direct delete
        // has to be noticed against, and hashing `fact` would watch a table
        // nothing reads.
        let fact_hash: String = sqlx::query_scalar("SELECT DOLT_HASHOF_TABLE('fact_write')")
            .fetch_one(&mut *tx)
            .await
            .map_err(store)?;
        tx.commit().await.map_err(store)?;
        Ok(Some(WriteSummary {
            entities: (
                entity_count,
                entity_latest.and_then(|at| at.parse::<jiff::Timestamp>().ok()),
            ),
            facts: (
                fact_count,
                fact_latest.and_then(|at| at.parse::<jiff::Timestamp>().ok()),
            ),
            entity_hash: Some(entity_hash),
            fact_hash: Some(fact_hash),
        }))
    }

    /// **One entity's document, by id — never the whole table.**
    ///
    /// [`scan`](Self::scan) is defaulted here on top of by [`Memory::scan_entity`]'s
    /// own default, which reads every document in the store and keeps one.
    /// That is right for `scan`'s own job — a boot has nothing to look up by —
    /// and wrong for a write's reindex, which already knows the one document it
    /// wants: it paid a full-store read on every single mutating call, and the
    /// cost grew with the store because the store is what a write's reindex
    /// walked to find nothing new about anyone else.
    ///
    /// **Every field is built the same way [`scan`](Self::scan) builds it**,
    /// scoped to one row instead of all of them: the same badge-or-id alias
    /// key, the same single-column parent lookup [`index`](Self::index) does
    /// with [`jojobot_domain::memory::entity_wearing`] against a full snapshot
    /// — a single `WHERE badge = ?` finds the same row without reading the
    /// rest — and the same `facts_of`/`held_by` this document's badge already
    /// keys into for [`scan`](Self::scan).
    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<search::DocScan>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let row = sqlx::query(
            "SELECT id, kind, name, source, crm, parent, boot, merged_into, badge, prose,
                    archived_reason, archived_at
             FROM entity WHERE id = ?",
        )
        .bind(entity.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(store)?;
        let Some(row) = row else {
            tx.commit().await.map_err(store)?;
            return Ok(None);
        };
        let prose: String = row.try_get("prose").map_err(store)?;
        let key_for_aliases = row
            .try_get::<Option<String>, _>("badge")
            .map_err(store)?
            .unwrap_or_else(|| entity.0.clone());
        let aliases: Vec<String> =
            sqlx::query_scalar("SELECT alias FROM entity_alias WHERE entity = ? ORDER BY ordinal")
                .bind(&key_for_aliases)
                .fetch_all(&mut *tx)
                .await
                .map_err(store)?;
        let mut resolved = entity_from(&row, aliases)?;
        if let Some(parent) = &resolved.parent {
            let wearing: Option<String> =
                sqlx::query_scalar("SELECT id FROM entity WHERE badge = ? ORDER BY id LIMIT 1")
                    .bind(parent.as_str())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(store)?;
            if let Some(current) = wearing {
                resolved.parent = Some(EntityId(current));
            }
        }
        let Some(badge) = resolved.badge.clone() else {
            return Err(MemoryError::Store(format!(
                "{} carries no badge, so its document has no id — the startup fill did not \
                 reach it",
                resolved.id
            )));
        };
        let key = EntityId(badge.clone());
        let mut fields = Self::held_by(&mut tx, &key).await?;
        self.compose_reference_fields(&mut tx, &mut fields).await?;
        let doc = search::DocScan {
            doc_id: badge,
            title: resolved.name.clone(),
            prose,
            facts: self.facts_of(&mut tx, &key).await?,
            fields,
            entity: Some(resolved),
            owner: None,
        };
        tx.commit().await.map_err(store)?;
        Ok(Some(doc))
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
        let held_origin = held.as_deref().map(read_origin);
        if let Err(MemoryError::ShippedType { name, .. }) =
            guard_replacement(&declared, held_origin)
        {
            let displaced = fetch_displaced(&mut tx, &name).await?;
            return Err(MemoryError::ShippedType { name, displaced });
        }
        owned_by_the_other_half(&mut tx, &declared.name, KEYS_OF_A_KIND).await?;
        // **A shipped write landing on a caller's own declaration is not a
        // refusal** — the guard above only stops the other direction. This
        // is the one place that remembers what it took, read here before the
        // delete below removes it. Reached at most once per name: once this
        // write lands, the name is shipped, and a caller's write over it is
        // refused before it ever reaches here again.
        if declared.origin == Origin::Shipped && held_origin == Some(Origin::Declared) {
            displace(&mut tx, &declared.name, &self.clock).await?;
        }
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
        gather_types(&rows)
    }

    async fn displaced_type(&self, name: &str) -> Result<Option<Displaced>, MemoryError> {
        let rows = sqlx::query(
            "SELECT key_name, holds, folds, required, one_of, replaced_on FROM displaced_type_field \
             WHERE type_name = ? ORDER BY ordinal",
        )
        .bind(name)
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        displaced_from_rows(name, &rows)
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
            // **A shipped kind landing on a caller's own type of that name is
            // not a refusal**, for the reason a shipped type landing on a
            // caller's is not: a build that adds a kind has to boot on a store
            // where somebody already declared a type under the name, because
            // the seed runs on every boot and a refusal here leaves the
            // process with no kinds loaded. The caller's keys are remembered
            // beside the shipped ones' own displacement record, and then
            // replaced.
            if origin == Origin::Shipped {
                let held_type: Option<String> = sqlx::query_scalar(
                    "SELECT origin FROM type_field WHERE type_name = ? AND owner = ? LIMIT 1",
                )
                .bind(token)
                .bind(KEYS_OF_A_TYPE)
                .fetch_optional(&mut *tx)
                .await
                .map_err(store)?;
                if held_type.as_deref().map(read_origin) == Some(Origin::Declared) {
                    displace(&mut tx, token, &self.clock).await?;
                    sqlx::query("DELETE FROM type_field WHERE type_name = ? AND owner = ?")
                        .bind(token)
                        .bind(KEYS_OF_A_TYPE)
                        .execute(&mut *tx)
                        .await
                        .map_err(store)?;
                }
            }
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

/// **Whether `holds` names a reference to a kind this process cannot tell
/// from a typo, because it has loaded no kinds at all.**
///
/// Distinct from `holds` simply being malformed (an unknown value type, a
/// kind suffix on a type that takes none): those stay [`gather_types`]'s
/// existing fallback, unrelated to this rule and not what this asks about.
/// Only a reference whose kind fails with
/// [`NotAKind::SetNeverLoaded`](kinds::NotAKind::SetNeverLoaded) is this.
fn unresolvable_only_because_unloaded(token: &str) -> bool {
    let held = token.trim().strip_prefix("list:").unwrap_or(token.trim());
    match held.split_once(':') {
        Some((holds, kind)) if holds.trim() == ValueType::Reference.as_token() => {
            matches!(kinds::resolve(kind.trim()), Err(NotAKind::SetNeverLoaded))
        }
        _ => false,
    }
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
///
/// **Refuses rather than retyping a reference this process cannot resolve
/// because it has loaded no kinds.** The fallback above is for a token that
/// is genuinely malformed; an unloaded set is a different failure; retyping
/// it to text would silently hand back a type's schema as something other
/// than what was declared, with nothing saying so.
fn gather_types(rows: &[sqlx::mysql::MySqlRow]) -> Result<Vec<DeclaredType>, MemoryError> {
    let mut types: Vec<DeclaredType> = Vec::new();
    for row in rows {
        let name: String = row.get("type_name");
        let key: String = row.get("key_name");
        let holds_token: String = row.get("holds");
        let held = Field::of_token(&key, &holds_token);
        if held.is_none() && unresolvable_only_because_unloaded(&holds_token) {
            return Err(MemoryError::KindsNeverLoaded { attempted: None });
        }
        let field = Field {
            folds: Fold::of_token(&row.get::<String, _>("folds")).unwrap_or_default(),
            required: row.get::<bool, _>("required"),
            one_of: Field::one_of_from_cell(row.get::<Option<String>, _>("one_of").as_deref()),
            ..held.unwrap_or_else(|| Field::new(&key, ValueType::Text))
        };
        match types.last_mut() {
            Some(last) if last.name == name => last.fields.push(field),
            _ => types.push(DeclaredType {
                origin: read_origin(&row.get::<String, _>("origin")),
                ..DeclaredType::new(&name, vec![field])
            }),
        }
    }
    Ok(types)
}

/// **What a caller's own declaration held under `name`, read here before a
/// shipped write's own DELETE removes it — the one place that remembers.**
///
/// A no-op when `name` names nothing yet (a shipped type declared for the
/// first time) or already names a shipped row (a normal reboot, landing on a
/// previous build's own declaration): both read as `held_origin !=
/// Some(Declared)` at the call site, which is the whole of the guard.
async fn displace(
    tx: &mut Transaction<'_, MySql>,
    name: &str,
    clock: &Clock,
) -> Result<(), MemoryError> {
    let prior_rows = sqlx::query(
        "SELECT key_name, holds, folds, required, one_of FROM type_field \
         WHERE type_name = ? ORDER BY ordinal",
    )
    .bind(name)
    .fetch_all(&mut **tx)
    .await
    .map_err(store)?;
    let today = clock.today_in(&jiff::tz::TimeZone::UTC);
    // **Every cell is carried as stored, never parsed.** Displacing runs
    // inside the seed, while the kind set is still empty, and a parse of a
    // `reference:<kind>` cell needs that set. The displaced rows are read back
    // by `displaced_from_rows`, after the set is loaded.
    for (ordinal, row) in prior_rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO displaced_type_field \
             (type_name, key_name, ordinal, holds, folds, required, one_of, replaced_on) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(row.get::<String, _>("key_name"))
        .bind(ordinal as i64 + 1)
        .bind(row.get::<String, _>("holds"))
        .bind(row.get::<String, _>("folds"))
        .bind(row.get::<bool, _>("required"))
        .bind(row.get::<Option<String>, _>("one_of"))
        .bind(today.to_string())
        .execute(&mut **tx)
        .await
        .map_err(store)?;
    }
    Ok(())
}

/// **A caller's own declaration, read back beside the refusal a
/// redeclaration of a shipped name meets.** `None` when nothing was ever
/// displaced under `name`.
async fn fetch_displaced(
    tx: &mut Transaction<'_, MySql>,
    name: &str,
) -> Result<Option<Displaced>, MemoryError> {
    let rows = sqlx::query(
        "SELECT key_name, holds, folds, required, one_of, replaced_on FROM displaced_type_field \
         WHERE type_name = ? ORDER BY ordinal",
    )
    .bind(name)
    .fetch_all(&mut **tx)
    .await
    .map_err(store)?;
    displaced_from_rows(name, &rows)
}

/// Rows into the [`Displaced`] they are — **one reader**, shared by the read
/// taken inside a `declare_type` transaction and the standalone read a
/// caller's own query makes, so the two cannot come to parse a row two
/// different ways.
fn displaced_from_rows(
    name: &str,
    rows: &[sqlx::mysql::MySqlRow],
) -> Result<Option<Displaced>, MemoryError> {
    if rows.is_empty() {
        return Ok(None);
    }
    let mut fields = Vec::with_capacity(rows.len());
    let mut replaced_on: Option<Date> = None;
    for row in rows {
        let key: String = row.get("key_name");
        let holds_token: String = row.get("holds");
        let held = Field::of_token(&key, &holds_token);
        if held.is_none() && unresolvable_only_because_unloaded(&holds_token) {
            return Err(MemoryError::KindsNeverLoaded { attempted: None });
        }
        fields.push(Field {
            folds: Fold::of_token(&row.get::<String, _>("folds")).unwrap_or_default(),
            required: row.get::<bool, _>("required"),
            one_of: Field::one_of_from_cell(row.get::<Option<String>, _>("one_of").as_deref()),
            ..held.unwrap_or_else(|| Field::new(&key, ValueType::Text))
        });
        if replaced_on.is_none() {
            replaced_on = Some(
                row.try_get::<String, _>("replaced_on")
                    .map_err(store)?
                    .parse::<Date>()
                    .map_err(|_| unreadable("its replaced-on day cannot be read as a date"))?,
            );
        }
    }
    Ok(Some(Displaced {
        name: name.to_string(),
        fields,
        replaced_on: replaced_on.expect("at least one row was read, so a date was too"),
    }))
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
    clock: &Clock,
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
        "REPLACE INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, \
         merged_into, archived_reason, archived_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(entity.archived.as_ref().map(|a| a.reason.as_str()))
    .bind(entity.archived.as_ref().map(|a| a.at.to_string()))
    .execute(&mut **tx)
    .await
    .map_err(store)?;
    append_entity_write(tx, &entity.id, clock).await?;
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

/// **Keep that this entity was written, and when** — the cheap signal
/// [`DoltMemory::write_summary`] reads, mirroring [`DoltMemory::append_fact_write`]'s
/// role for facts. Never the row's content: nothing today asks what an
/// entity used to say, only whether it has changed since a caller last
/// looked, so a full copy here would be a second history nothing reads.
async fn append_entity_write(
    tx: &mut Transaction<'_, MySql>,
    entity: &EntityId,
    clock: &Clock,
) -> Result<(), MemoryError> {
    let highest: Option<i64> =
        sqlx::query_scalar("SELECT MAX(ordinal) FROM entity_write WHERE entity = ?")
            .bind(entity.as_str())
            .fetch_one(&mut **tx)
            .await
            .map_err(store)?;
    sqlx::query("INSERT INTO entity_write (entity, ordinal, written_at) VALUES (?, ?, ?)")
        .bind(entity.as_str())
        .bind(highest.unwrap_or(0) + 1)
        .bind(clock.now().to_string())
        .execute(&mut **tx)
        .await
        .map_err(store)?;
    Ok(())
}

#[cfg(test)]
mod tests;

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
use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FactStatus, FieldWrite, Guarded, KeyWrite, Memory, MemoryError, NewEntity, NewFact,
    Provenance, Retraction, Standing, apply_entity_patch, apply_fact_patch, folded_fields, guard,
    guard_fit,
    kinds::{self, NotAKind},
    normalize_content, normalize_details, normalize_prose, referenced_by, retraction_of,
    screen_entity_patch, search, standing_of, stood_after, stood_after_capture,
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
        }
    }

    /// The same store over a supplied draw, **so the collision path can be
    /// watched through the verb that mints**.
    pub fn open_drawing(pool: MySqlPool, draw: Draw) -> Self {
        DoltMemory { pool, draw }
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

    /// Every entity, whole — what the write guard screens against.
    ///
    /// **The whole roster, because the guard's answer is a function of all of
    /// it**: what is near a handle cannot be decided from a subset, and a
    /// screen over half the index is a screen that reports a free name as free
    /// when it is not.
    async fn index(tx: &mut Transaction<'_, MySql>) -> Result<Vec<Entity>, MemoryError> {
        let rows =
            sqlx::query("SELECT id, kind, name, source, crm, parent, boot FROM entity ORDER BY id")
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
            let mine = aliases
                .iter()
                .filter(|a| a.get::<String, _>("entity") == id.0)
                .map(|a| a.get::<String, _>("alias"))
                .collect();
            entities.push(entity_from(row, mine)?);
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
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
    ) -> Result<Vec<Fact>, MemoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {FACT_COLUMNS} FROM fact WHERE entity = ? ORDER BY id"
        ))
        .bind(entity.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        Self::assemble(tx, &rows).await
    }

    /// One addressed fact, or nothing.
    async fn read_fact(
        tx: &mut Transaction<'_, MySql>,
        address: &FactAddress,
    ) -> Result<Option<Fact>, MemoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {FACT_COLUMNS} FROM fact WHERE entity = ? AND id = ?"
        ))
        .bind(address.home.as_str())
        .bind(address.local.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        Ok(Self::assemble(tx, &rows).await?.pop())
    }

    /// Rows into facts, each with its fields and references read back beside
    /// it.
    ///
    /// **They are read per fact rather than joined**, because a fact carrying
    /// no field must come back with an empty bag rather than with a row of
    /// NULLs a join invents for it.
    async fn assemble(
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
            facts.push(fact_from(row, entity, id, fields, refs)?);
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
            "SELECT w.`key`, w.ordinal, w.value, w.fact_id, f.status, f.provenance, f.standing
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
    async fn write_fact(tx: &mut Transaction<'_, MySql>, fact: &Fact) -> Result<(), MemoryError> {
        sqlx::query(
            "REPLACE INTO fact (entity, id, content, details, provenance, standing, status,
                                date, edge_shape, edge_object, derived_from, derived_from_id,
                                inserted_at, stale_after)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(fact.home.as_str())
        .bind(fact.id.as_str())
        .bind(&fact.content)
        .bind(fact.details.as_deref())
        .bind(fact.provenance.as_token())
        .bind(fact.standing.as_token())
        .bind(fact.status.as_token())
        .bind(fact.date.to_string())
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
        tx: &mut Transaction<'_, MySql>,
        home: &EntityId,
    ) -> Result<Vec<String>, MemoryError> {
        Ok(Self::facts_of(tx, home)
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
const FACT_COLUMNS: &str = "entity, id, content, details, provenance, standing, status, date, \
                            edge_shape, edge_object, derived_from, derived_from_id, inserted_at, \
                            stale_after";

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
    let date: Date = row
        .try_get::<String, _>("date")
        .map_err(store)?
        .parse()
        .map_err(|_| unreadable("its date cannot be read as a date"))?;
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
        date,
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
        let index = Self::index(&mut tx).await?;
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
        write_entity(&mut tx, &self.draw, &entity).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(entity))
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

    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        validate_write_subject(handle)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        let Some(mut entity) = index.iter().find(|e| &e.id == handle).cloned() else {
            return Err(MemoryError::UnknownEntity {
                attempted: handle.to_string(),
                nearest: guard::screen(handle, &[], &index),
            });
        };
        // Changing what an entity is CALLED is an entity-touching write, so it
        // faces the same gate — display name and aliases alike.
        if let guard::Decision::Block(candidates) = screen_entity_patch(&entity, &patch, &index) {
            return Ok(Guarded::Blocked {
                attempted: handle.clone(),
                candidates,
            });
        }
        apply_entity_patch(&mut entity, &patch)?;
        write_entity(&mut tx, &self.draw, &entity).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(entity))
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
        let index = Self::index(&mut tx).await?;
        // Every entity this write names must already exist — the subject first,
        // then the edge's object, then anything the record points at. Nothing
        // here provisions.
        if let guard::Decision::Block(candidates) = guard::decide_existing(&fact.subject, &index) {
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
        if let Some(source) = &fact.derived_from {
            if !index.iter().any(|e| e.id == source.home) {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            }
            match Self::read_fact(&mut tx, source).await? {
                None => {
                    return Err(MemoryError::UnknownFact {
                        attempted: source.to_string(),
                        nearest: Self::addresses_in(&mut tx, &source.home).await?,
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
        }

        let home = fact.subject.clone();
        let id = Self::mint(&mut tx, &home).await?;
        let stored = Fact {
            id,
            home,
            subject: fact.subject,
            content: normalize_content(&fact.content),
            details: normalize_details(fact.details.as_deref()),
            provenance: fact.provenance,
            standing,
            status: fact.status,
            date: fact.date,
            edge: fact.edge,
            fields: fact.fields,
            refs: fact.refs,
            derived_from: fact.derived_from,
            // **The store stamps it, so nothing above can.** The moment a
            // record is taken in is this one, and a caller that could name it
            // could claim jojobot knew something before it did.
            inserted_at: Some(jiff::Timestamp::now()),
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
        // folds is declared by whoever declared it; what governs a thing is its
        // own kind, and nothing else.
        let governs = Self::kind_keys_in(&mut tx, stored.home.kind_token()).await?;
        guard_fit(
            stored.home.kind_token(),
            &folded_fields(&held, &declared),
            &stood_after_capture(&held, &stored, &declared),
            &governs,
        )?;
        Self::write_fact(&mut tx, &stored).await?;
        // Every key this record carries is a write of its own, appended to the
        // history of that key on this thing.
        Self::append_writes(&mut tx, &stored.home, &stored.id, written_keys(&stored)).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(stored))
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        // An unknown entity is a miss with its near candidates — never an empty
        // page. Empty-but-real and nonexistent are different answers.
        if !index.iter().any(|e| &e.id == subject) {
            return Err(MemoryError::UnknownEntity {
                attempted: subject.to_string(),
                nearest: guard::screen(subject, &[], &index),
            });
        }
        let facts = Self::facts_of(&mut tx, subject).await?;
        tx.commit().await.map_err(store)?;
        Ok(facts)
    }

    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        // An unknown entity is a miss with its near candidates, exactly as a
        // recall of one is: a thing nobody has written a key on and a handle
        // nobody created are different answers with different repairs.
        if !index.iter().any(|e| &e.id == entity) {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        }
        let held = Self::held_by(&mut tx, entity).await?;
        tx.commit().await.map_err(store)?;
        Ok(held)
    }

    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        // An unknown entity is a miss with its near candidates, exactly as a
        // recall of one is: a key nobody wrote and a handle nobody created are
        // different answers with different repairs.
        if !index.iter().any(|e| &e.id == entity) {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        }
        let rows = sqlx::query(
            "SELECT value, fact_id FROM field_write
             WHERE entity = ? AND `key` = ? ORDER BY ordinal",
        )
        .bind(entity.as_str())
        .bind(key)
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        // The record a write arrived in says when it happened and what became
        // of it, so the claims are read alongside.
        let facts = Self::facts_of(&mut tx, entity).await?;
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
                date: fact.date,
                status: fact.status,
                provenance: fact.provenance,
                standing: fact.standing,
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
        let index = Self::index(&mut tx).await?;
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
        // explain it — not a fact miss trailing an empty address list.
        if !index.iter().any(|e| e.id == address.home) {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        }
        let Some(mut fact) = Self::read_fact(&mut tx, address).await? else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: Self::addresses_in(&mut tx, &address.home).await?,
            });
        };
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
            if !index.iter().any(|e| e.id == source.home) {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            }
            match Self::read_fact(&mut tx, source).await? {
                None => {
                    return Err(MemoryError::UnknownFact {
                        attempted: source.to_string(),
                        nearest: Self::addresses_in(&mut tx, &source.home).await?,
                    });
                }
                Some(held) if held.status == FactStatus::Retracted => {
                    return Err(MemoryError::SourceRetracted {
                        attempted: source.to_string(),
                    });
                }
                Some(_) => {}
            }
        }
        apply_fact_patch(&mut fact, &patch)?;
        // **The thing's fields as they will stand, against the thing's fields
        // as they stand now.** A write may not drop a thing below a type it
        // already fits; a thing that fits nothing has nothing to protect, so
        // its records stay repairable. One function, called from both stores.
        let held = Self::writes_on(&mut tx, &fact.home).await?;
        let declared = Self::types_in(&mut tx).await?;
        let governs = Self::kind_keys_in(&mut tx, fact.home.kind_token()).await?;
        guard_fit(
            fact.home.kind_token(),
            &folded_fields(&held, &declared),
            &stood_after(&held, &fact, &patch, &carried, &declared),
            &governs,
        )?;
        Self::write_fact(&mut tx, &fact).await?;
        // **The edit appends.** The record reads back changed — that is the
        // surface — and the value it replaced stays where it was written.
        Self::append_writes(&mut tx, &fact.home, &fact.id, writes_of(&patch, &carried)).await?;
        // Read back from the substrate rather than from what the patch
        // believed, so the answer is the projection a later read will give.
        let fields = Self::fields_of(&mut tx, &fact.home, &fact.id).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(Fact { fields, ..fact }))
    }

    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let index = Self::index(&mut tx).await?;
        if !index.iter().any(|e| e.id == address.home) {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        }
        // Everything is decided before anything moves, so a refusal leaves the
        // row exactly as it was.
        let Some(target) = Self::read_fact(&mut tx, address).await? else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: Self::addresses_in(&mut tx, &address.home).await?,
            });
        };
        let account = retraction_of(&target, reason, date)?;
        let standing = standing_of(&account);
        let home = target.home.clone();
        let record = Fact {
            id: Self::mint(&mut tx, &home).await?,
            home,
            subject: account.subject,
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            date: account.date,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            // A retraction is a record in its own right, taken in now.
            inserted_at: Some(jiff::Timestamp::now()),
            stale_after: None,
        };
        let retracted = Fact {
            status: FactStatus::Retracted,
            ..target
        };
        Self::write_fact(&mut tx, &retracted).await?;
        Self::write_fact(&mut tx, &record).await?;
        // The account is a record like any other, and the key naming what it
        // takes back is a write of its own. The record being taken back writes
        // no key: what changed there is its status.
        Self::append_writes(&mut tx, &record.home, &record.id, written_keys(&record)).await?;
        tx.commit().await.map_err(store)?;
        Ok(Retraction { retracted, record })
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
        let writes = Self::writes_on(&mut tx, entity).await?;
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
        let rows = sqlx::query(&format!(
            "SELECT {FACT_COLUMNS} FROM fact WHERE derived_from = ? AND derived_from_id = ? \
             ORDER BY entity, id"
        ))
        .bind(source.home.as_str())
        .bind(source.local.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        // **Assembled by the one reader every other read here uses**, so a
        // claim reached through its lineage is the same record a recall gives.
        let standing_on = Self::assemble(&mut tx, &rows).await?;
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
            for fact in Self::facts_of(&mut tx, &holder).await? {
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
            scanned.push(search::DocScan {
                doc_id: badge,
                title: entity.name.clone(),
                prose,
                facts: Self::facts_of(&mut tx, &entity.id).await?,
                // What the thing IS travels with the doc: the records beside it
                // cannot be folded back into it.
                fields: Self::held_by(&mut tx, &entity.id).await?,
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

async fn write_entity(
    tx: &mut Transaction<'_, MySql>,
    draw: &Draw,
    entity: &Entity,
) -> Result<(), MemoryError> {
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
        "REPLACE INTO entity (id, kind, name, source, crm, parent, boot, prose, badge)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(&mut **tx)
    .await
    .map_err(store)?;
    sqlx::query("DELETE FROM entity_alias WHERE entity = ?")
        .bind(entity.id.as_str())
        .execute(&mut **tx)
        .await
        .map_err(store)?;
    for (ordinal, alias) in entity.aliases.iter().enumerate() {
        sqlx::query("INSERT INTO entity_alias (entity, ordinal, alias) VALUES (?, ?, ?)")
            .bind(entity.id.as_str())
            .bind(ordinal as i64 + 1)
            .bind(alias)
            .execute(&mut **tx)
            .await
            .map_err(store)?;
    }
    Ok(())
}

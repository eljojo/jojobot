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
    guard_fit, normalize_content, normalize_details, normalize_prose, retraction_of,
    screen_entity_patch, search, standing_of, stood_after,
    types::{DeclaredType, Field, Origin, ValueType, guard_replacement, validate_type},
    validate_content, validate_details, validate_edge, validate_entity, validate_fields,
    validate_prose, validate_subject, writes_of,
};
use sqlx::{MySql, MySqlPool, Row, Transaction};

/// Memory kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets nobody
/// set.
#[derive(Clone)]
pub struct DoltMemory {
    pool: MySqlPool,
}

impl DoltMemory {
    /// Open the store over an existing pool.
    ///
    /// **The schema is not this adapter's to create.** It arrives through the
    /// migrations the server applies on start — see [`crate::dolt::migrate`].
    pub fn open(pool: MySqlPool) -> Self {
        DoltMemory { pool }
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
            "SELECT w.`key`, w.ordinal, w.value, w.fact_id, f.status FROM field_write w
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
        Ok(folded_fields(&Self::writes_on(tx, entity).await?))
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
                                date, edge_shape, edge_object, derived_from, derived_from_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
            "SELECT type_name, key_name, holds, origin FROM type_field
             ORDER BY type_name, ordinal",
        )
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
                            edge_shape, edge_object, derived_from, derived_from_id";

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
    let kind = id
        .kind()
        .ok_or_else(|| unreadable("its handle names no kind"))?;
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
        write_entity(&mut tx, &entity).await?;
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
        validate_subject(handle)?;
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
        write_entity(&mut tx, &entity).await?;
        tx.commit().await.map_err(store)?;
        Ok(Guarded::Written(entity))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        validate_subject(&fact.subject)?;
        validate_content(&fact.content)?;
        validate_details(fact.details.as_deref())?;
        if let Some(edge) = &fact.edge {
            validate_edge(edge)?;
        }
        validate_fields(&fact.fields)?;
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
            validate_subject(object)?;
            if let guard::Decision::Block(candidates) = guard::decide_existing(object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object.clone(),
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
            if Self::read_fact(&mut tx, source).await?.is_none() {
                return Err(MemoryError::UnknownFact {
                    attempted: source.to_string(),
                    nearest: Self::addresses_in(&mut tx, &source.home).await?,
                });
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
        };
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
        apply_fact_patch(&mut fact, &patch)?;
        // **The thing's fields as they will stand, against the thing's fields
        // as they stand now.** A write may not drop a thing below a type it
        // already fits; a thing that fits nothing has nothing to protect, so
        // its records stay repairable. One function, called from both stores.
        let held = Self::writes_on(&mut tx, &fact.home).await?;
        guard_fit(
            &folded_fields(&held),
            &stood_after(&held, &fact, &patch, &carried),
            &Self::types_in(&mut tx).await?,
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

    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        validate_subject(entity)?;
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
            let prose: String = sqlx::query_scalar("SELECT prose FROM entity WHERE id = ?")
                .bind(entity.id.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(store)?;
            scanned.push(search::DocScan {
                doc_id: entity.id.to_string(),
                title: entity.name.clone(),
                prose,
                facts: Self::facts_of(&mut tx, &entity.id).await?,
                // What the thing IS travels with the doc: the records beside it
                // cannot be folded back into it.
                fields: Self::held_by(&mut tx, &entity.id).await?,
                entity: Some(entity),
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
        sqlx::query("DELETE FROM type_field WHERE type_name = ?")
            .bind(&declared.name)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        for (ordinal, field) in declared.fields.iter().enumerate() {
            sqlx::query(
                "INSERT INTO type_field (type_name, key_name, ordinal, holds, origin)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&declared.name)
            .bind(&field.key)
            .bind(ordinal as i64 + 1)
            .bind(field.holds.as_token())
            .bind(declared.origin.as_token())
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
            "SELECT type_name, key_name, holds, origin FROM type_field
             ORDER BY type_name, ordinal",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        Ok(gather_types(&rows))
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
fn gather_types(rows: &[sqlx::mysql::MySqlRow]) -> Vec<DeclaredType> {
    let mut types: Vec<DeclaredType> = Vec::new();
    for row in rows {
        let name: String = row.get("type_name");
        let field = Field::new(
            &row.get::<String, _>("key_name"),
            ValueType::of_token(&row.get::<String, _>("holds")).unwrap_or(ValueType::Text),
        );
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
async fn write_entity(tx: &mut Transaction<'_, MySql>, entity: &Entity) -> Result<(), MemoryError> {
    // **The prose is carried across rather than blanked.** A rewrite of an
    // entity's metadata is not a rewrite of what somebody wrote on its page,
    // and `REPLACE` deletes the row before inserting the new one.
    let prose: Option<String> = sqlx::query_scalar("SELECT prose FROM entity WHERE id = ?")
        .bind(entity.id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
    sqlx::query(
        "REPLACE INTO entity (id, kind, name, source, crm, parent, boot, prose)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entity.id.as_str())
    .bind(entity.kind.as_token())
    .bind(&entity.name)
    .bind(&entity.source)
    .bind(entity.crm.as_deref())
    .bind(entity.parent.as_ref().map(EntityId::as_str))
    .bind(entity.boot.as_token())
    .bind(prose.unwrap_or_default())
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

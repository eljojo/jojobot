//! **A handle whose writes outlived its record gets a row.**
//!
//! A record the build supplies has no row, so a claim or a field the operator
//! wrote on it is filed under the handle's own text. While the build supplies
//! the handle, the supplied record answers for it. When a later build stops
//! supplying it, or renames it, nothing answers: the existence gate reads rows
//! plus the supplied set, so the handle names nothing, and what the operator
//! wrote stays in the store where no read reaches it (rule 508).
//!
//! This gives such a handle a row, once, at startup, so it reads back as any
//! stored thing does. The kind comes from the handle, because the handle is
//! `kind:slug` and nothing else about the record survives: the build's display
//! name was never stored. The name is the slug as written. The row wears no
//! parent, no aliases and no prose, for the same reason. The operator renames it
//! like any other thing.
//!
//! **Only a handle a write was filed under, and nothing else.** A handle the
//! build still supplies keeps its writes under its text. A key with no colon is
//! no handle, and a badge never has one: a key like that is a badge whose row is
//! gone, which is other damage. A handle a thing wore before a rename, and a key
//! whose kind this build does not declare, are not an orphan to adopt either.

use std::collections::BTreeSet;

use jojobot_domain::memory::{Boot, Entity, EntityId, MemoryError};

use super::{DoltMemory, store, write_entity};

/// What the pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AdoptionReport {
    /// The handles that were given a row.
    pub adopted: Vec<EntityId>,
    /// The keys it found and did not adopt, because their kind is not one this
    /// build declares. Nothing was written for them.
    pub left: Vec<String>,
}

impl DoltMemory {
    /// **Record the handles this build supplies as records, and the handles the
    /// build lists as supplied by an earlier build**, in `supplied_handle`. Every
    /// boot calls it before the adoption below, and a row is never removed: a
    /// handle a later build stops supplying has to stay listed, because that is
    /// what makes its writes adoptable.
    pub async fn record_supplied_handles(&self, retired: &[EntityId]) -> Result<(), MemoryError> {
        let supplied = self
            .supplied
            .records()
            .map(|(entity, _)| entity.id.as_str());
        let listed = retired.iter().map(EntityId::as_str);
        for handle in supplied.chain(listed) {
            sqlx::query("INSERT IGNORE INTO supplied_handle (handle) VALUES (?)")
                .bind(handle)
                .execute(&self.pool)
                .await
                .map_err(store)?;
        }
        Ok(())
    }

    /// **Give a row to every handle that has writes filed under its text, no row,
    /// and no record the build supplies.** Idempotent: a second call finds none.
    ///
    /// One transaction per handle, for the reason the badge fill gives: the badge
    /// draw probes what is committed, and a failure half way has still made
    /// progress.
    pub async fn adopt_handles_without_a_row(&self) -> Result<AdoptionReport, MemoryError> {
        let keys: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT entity FROM (
                 SELECT entity FROM fact UNION SELECT entity FROM field_write
             ) AS written
             WHERE entity LIKE '%:%'
               AND entity NOT IN (SELECT id FROM entity)
               AND entity NOT IN (SELECT former_handle FROM entity_former_handle)
               AND entity IN (SELECT handle FROM supplied_handle)
             ORDER BY entity",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let mut report = AdoptionReport::default();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for key in keys {
            if !seen.insert(key.clone()) {
                continue;
            }
            let id = EntityId(key.clone());
            if self.supplied.record_for(&id).is_some() {
                continue;
            }
            let Some(kind) = id.kind().filter(|_| !id.slug().is_empty()) else {
                report.left.push(key);
                continue;
            };
            let entity = Entity {
                folded_names: Vec::new(),
                id: id.clone(),
                kind,
                name: id.slug().to_string(),
                aliases: Vec::new(),
                source: "jojobot".to_string(),
                crm: None,
                parent: None,
                boot: Boot::default(),
                merged_into: None,
                badge: None,
                archived: None,
            };
            let mut tx = self.pool.begin().await.map_err(store)?;
            let badge = write_entity(&mut tx, &self.draw, &entity, &self.clock, None).await?;
            Self::rekey_adopted(&mut tx, id.as_str(), &badge).await?;
            tx.commit().await.map_err(store)?;
            report.adopted.push(id);
        }
        Ok(report)
    }
}

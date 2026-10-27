use super::*;

impl DoltMemory {
    pub(super) async fn scan_docs(&self) -> Result<Vec<search::DocScan>, MemoryError> {
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
            let mut fields = self.held_by(&mut tx, &key).await?;
            let mut asked = assembly::Asked::default();
            let field_links = self
                .marked_links_asked(&mut tx, &mut asked, &fields)
                .await?;
            self.compose_reference_fields_asked(&mut tx, &mut asked, &mut fields)
                .await?;
            scanned.push(search::DocScan {
                doc_id: badge,
                title: entity.name.clone(),
                prose,
                facts: self.facts_of(&mut tx, &key).await?,
                // What the thing IS travels with the doc: the records beside it
                // cannot be folded back into it.
                fields,
                field_links,
                entity: Some(entity),
                owner: None,
                unmapped: self.unmapped_under(&mut tx, &key).await?,
            });
        }
        super::commit_native(tx).await.map_err(store)?;
        Ok(scanned)
    }

    pub(super) async fn scan_one_doc(
        &self,
        entity: &EntityId,
    ) -> Result<Option<search::DocScan>, MemoryError> {
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
            super::commit_native(tx).await.map_err(store)?;
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
            let parent = self.forward_key(&mut tx, parent).await?;
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
        // **A thing a merge folded into another, or into which another was
        // folded, is named as the whole index names it**: the survivor answers
        // to the folded things' names, set apart from its own aliases.
        if let Some(badge) = &resolved.badge
            && (self
                .is_folded_key(&mut tx, &EntityId(badge.clone()))
                .await?
                || !Self::merges_into(&mut tx, &EntityId(badge.clone()))
                    .await?
                    .is_empty())
            && let Some(named) = self
                .index(&mut tx)
                .await?
                .into_iter()
                .find(|known| known.id == resolved.id)
        {
            resolved.aliases = named.aliases;
            resolved.folded_names = named.folded_names;
        }
        let Some(badge) = resolved.badge.clone() else {
            return Err(MemoryError::Store(format!(
                "{} carries no badge, so its document has no id — the startup fill did not \
                 reach it",
                resolved.id
            )));
        };
        let key = EntityId(badge.clone());
        let mut fields = self.held_by(&mut tx, &key).await?;
        let mut asked = assembly::Asked::default();
        let field_links = self
            .marked_links_asked(&mut tx, &mut asked, &fields)
            .await?;
        self.compose_reference_fields_asked(&mut tx, &mut asked, &mut fields)
            .await?;
        let doc = search::DocScan {
            doc_id: badge,
            title: resolved.name.clone(),
            prose,
            facts: self.facts_of(&mut tx, &key).await?,
            fields,
            field_links,
            entity: Some(resolved),
            owner: None,
            unmapped: self.unmapped_under(&mut tx, &key).await?,
        };
        super::commit_native(tx).await.map_err(store)?;
        Ok(Some(doc))
    }
}

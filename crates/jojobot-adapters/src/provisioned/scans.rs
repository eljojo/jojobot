use super::*;

impl<M: Memory + Send + Sync> Provisioned<M> {
    /// A record the build supplies, as the document a reader would have got had
    /// the store held it. **Its facts are the claims the store holds about
    /// it**: the build ships no claim, but a caller may write one on a supplied
    /// record, and the store keeps it under the handle with no entity row to
    /// make a document of its own. Without them the index never holds the
    /// claim's words, and a search for them finds nothing.
    ///
    /// **Its fields are what a read of the record folds to**: the fields the
    /// build ships, under whatever keys the store holds for the handle. The
    /// index is built from this document, so a key written onto a supplied
    /// record has to be in it, or the key finds the stored records that carry it
    /// and never this one.
    pub(super) async fn supplied_scan(
        &self,
        entity: &EntityId,
    ) -> Result<Option<search::DocScan>, MemoryError> {
        let Some((shipped, _)) = self.provisions.record_for(entity) else {
            return Ok(None);
        };
        let held = self.with_overlay(shipped).await?;
        Ok(Some(search::DocScan {
            doc_id: held.id.to_string(),
            title: held.name.clone(),
            prose: self
                .provisions
                .prose_for(entity)
                .unwrap_or_default()
                .to_string(),
            entity: Some(held),
            facts: self.inner.recall(entity).await?,
            fields: self.fields(entity).await?,
            field_links: Default::default(),
            owner: None,
            unmapped: Vec::new(),
        }))
    }
}

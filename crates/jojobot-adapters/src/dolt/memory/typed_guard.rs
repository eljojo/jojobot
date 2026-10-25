//! Kind checks use the resolved member while literal constraints keep the input.

use super::*;
use std::collections::BTreeMap;

impl DoltMemory {
    /// **[`guard::decide_existing`], without the listing a hit does not need.**
    /// A handle that answers directly, stored or supplied, is one targeted row
    /// and proceeds. Only a handle that answered to nothing builds
    /// [`known`](Self::known), because the candidates it is blocked with come
    /// from the whole roster.
    pub(super) async fn decide_existing_targeted(
        &self,
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
    ) -> Result<guard::Decision, MemoryError> {
        // **The store's whole resolution, not the id alone**: a name that answers
        // directly, through a rename, or as the member a number names exists.
        let resolved = self.resolve(tx, handle).await?;
        if resolved.is_some() {
            return Ok(guard::decide_named(
                handle,
                resolved.as_ref().map(|(_, found)| found),
                &[],
            ));
        }
        let index = self.known(tx).await?;
        Ok(guard::decide_existing(handle, &index))
    }

    pub(super) async fn decide_edge(
        &self,
        tx: &mut Transaction<'_, MySql>,
        edge: &Edge,
        index: &[Entity],
    ) -> Result<guard::Decision, MemoryError> {
        let resolved = self.resolve(tx, &edge.object).await?;
        if let Some((_, handle)) = &resolved {
            jojobot_domain::memory::validate_resolved_edge(edge, handle)?;
        }
        Ok(guard::decide_named(
            &edge.object,
            resolved.as_ref().map(|(_, h)| h),
            index,
        ))
    }

    pub(super) async fn decide_edge_targeted(
        &self,
        tx: &mut Transaction<'_, MySql>,
        edge: &Edge,
    ) -> Result<guard::Decision, MemoryError> {
        if let Some((_, handle)) = self.resolve(tx, &edge.object).await? {
            jojobot_domain::memory::validate_resolved_edge(edge, &handle)?;
        }
        self.decide_existing_targeted(tx, &edge.object).await
    }

    pub(super) async fn guard_fit_resolved(
        &self,
        tx: &mut Transaction<'_, MySql>,
        kind: &str,
        before: &BTreeMap<String, String>,
        after: &BTreeMap<String, String>,
        declared: &[DeclaredType],
    ) -> Result<(), MemoryError> {
        let mut resolved = BTreeMap::new();
        for (_, items) in jojobot_domain::memory::reference_field_values(after, declared) {
            for item in items {
                let id = EntityId(item);
                if jojobot_domain::memory::numbered::numbered_handle(&id).is_some()
                    && let Some((_, handle)) = self.resolve(tx, &id).await?
                {
                    resolved.insert(id, handle);
                }
            }
        }
        jojobot_domain::memory::guard_fit_resolved(kind, before, after, declared, &resolved)
    }
}

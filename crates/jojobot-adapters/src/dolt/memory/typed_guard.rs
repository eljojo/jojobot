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

    /// **The open entries of the live lists hung on a thing**, read inside the
    /// write's own transaction. `thing` is a storage key. A list folded into
    /// another or archived is no longer part of the thing.
    async fn open_entries_hung_on(
        &self,
        tx: &mut Transaction<'_, MySql>,
        thing: &EntityId,
    ) -> Result<Vec<String>, MemoryError> {
        let bare = Self::folds_without_an_event(tx).await?;
        let mut hung = Vec::new();
        for child in self.children_served_under(tx, thing, &bare).await? {
            // **The claims are keyed by the badge, and an entry is a claim on a
            // thing of kind list**, which only the handle says: so the list is
            // read by its key and its claims are served under its handle.
            let key: Option<String> = sqlx::query_scalar(
                "SELECT COALESCE(badge, id) FROM entity
                 WHERE id = ? AND kind = 'list' AND archived_reason IS NULL",
            )
            .bind(child.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?;
            let Some(key) = key.map(EntityId) else {
                continue;
            };
            let fields = self.held_by(tx, &key).await?;
            let facts: Vec<Fact> = self
                .facts_of(tx, &key)
                .await?
                .into_iter()
                .map(|fact| Fact {
                    home: child.clone(),
                    subject: child.clone(),
                    ..fact
                })
                .collect();
            hung.push((fields, facts));
        }
        let lists: Vec<jojobot_domain::memory::done_gate::HungList<'_>> = hung
            .iter()
            .map(|(fields, facts)| jojobot_domain::memory::done_gate::HungList { fields, facts })
            .collect();
        Ok(jojobot_domain::memory::done_gate::open_entries_of(&lists))
    }

    pub(super) async fn guard_fit_resolved(
        &self,
        tx: &mut Transaction<'_, MySql>,
        thing: &EntityId,
        kind: &str,
        before: &BTreeMap<String, String>,
        after: &BTreeMap<String, String>,
        declared: &[DeclaredType],
    ) -> Result<(), MemoryError> {
        let open = if jojobot_domain::memory::done_gate::moves_to_done(kind, before, after) {
            self.open_entries_hung_on(tx, thing).await?
        } else {
            Vec::new()
        };
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
        jojobot_domain::memory::guard_fit_resolved(kind, before, after, declared, &resolved, &open)
    }
}

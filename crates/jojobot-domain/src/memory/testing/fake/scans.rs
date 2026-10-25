use super::*;

impl InMemoryMemory {
    /// The rows this store holds — used where a supplied record has no place
    /// (`list_entities`, and as the base [`InMemoryMemory::known`] extends).
    ///
    /// **A parent pointer is badge-keyed exactly as `home` is** (rule 268),
    /// resolved here on the way out. It names a DIFFERENT row, not this
    /// one's own key, and nothing here queries by it — `children` reads
    /// every entity and filters in memory — so nothing but this resolution
    /// stands between the stored badge and a reader who wants the handle.
    /// The documents of `rows`, each built the way a scan builds it: its
    /// frontmatter, its prose, the facts homed in it and what the thing holds.
    pub(super) fn docs_of(&self, rows: Vec<Entity>) -> Vec<search::DocScan> {
        let facts = self.facts_view();
        let prose = self.prose.lock().expect("fake mutex poisoned").clone();
        let badges = self.badges.lock().expect("fake mutex poisoned").clone();
        let resolution = self.resolution();
        rows.into_iter()
            .map(|entity| {
                // **The storage key this entity's own rows are filed under**
                // — its badge, or its own handle when it wears none. `home`
                // and `writes_on` both read the key; `facts` served back
                // carry the handle, exactly as any other read does.
                let key = match &entity.badge {
                    Some(badge) => EntityId(badge.clone()),
                    None => entity.id.clone(),
                };
                // The scan carries what the thing IS, because the records
                // it also carries cannot be folded back into it.
                let mut fields = crate::memory::folded_fields(
                    &self.writes_on(&key, &facts),
                    &resolution.declared,
                );
                let field_links = self.marked_links_in(&resolution, &fields);
                self.compose_reference_fields_in(&resolution, &mut fields);
                search::DocScan {
                    doc_id: badges
                        .get(&entity.id)
                        .cloned()
                        .unwrap_or_else(|| entity.id.to_string()),
                    title: entity.name.clone(),
                    prose: prose.get(&entity.id).cloned().unwrap_or_default(),
                    facts: facts
                        .iter()
                        .filter(|f| f.home == key && !self.is_folded_key(&key))
                        .map(|f| self.served_in(&resolution, self.projected(f), &entity.id))
                        .collect(),
                    fields,
                    field_links,
                    entity: Some(entity),
                    // A stored row is the whole instance's, exactly as it was.
                    owner: None,
                    unmapped: self.unmapped_among(&key, &facts),
                }
            })
            .collect()
    }
}

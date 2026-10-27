use super::*;

impl Ctx<'_> {
    /// The entities a declared relation reaches from `id`, in the direction
    /// asked for.
    ///
    /// **Out** reads this object's own records: the key holds a handle, and
    /// that handle is where the link goes. **In** reads everybody else's: every
    /// record pointing that key at this object, and the link goes to whoever
    /// each record is about.
    ///
    /// **The inbound walk is scoped by the KEY and by nothing else.** It reaches
    /// every record pointing here through that key, whatever else the record is
    /// — which is what "everything pointing at me through `owner`" means, and is
    /// not the same question as "my pets".
    pub(super) fn along_relation(
        &self,
        id: &EntityId,
        from: &[&Fact],
        name: &str,
        direction: Direction,
    ) -> Vec<(Via, EntityId)> {
        // ⚠️ **UNREACHABLE, AND DELIBERATELY KEPT.** No test reaches this
        // branch, and the comment exists so the next reader knows the coverage
        // is absent rather than assuming it: `check_declared` refuses a
        // relation no declaration backs before any walk starts, so a name that
        // gets this far has already resolved once.
        //
        // It stays because the refusal and the lookup are in different
        // functions, and an empty walk is the right answer if they ever
        // disagree. Nothing downstream depends on it.
        let Some(field) = relation_field(name, self.declarations) else {
            return Vec::new();
        };
        let key = field.key.as_str();
        let link = || Link::Relation(key.to_string());
        // **Each item, the way the declaration says the cell is written.** An
        // ordinary key carries one handle and a list carries several; asking
        // the field is what tells the two apart, and it is the same accessor
        // the write guard screens each handle through (rule 217).
        //
        // **A key a standing record SETS is read from what its thing holds
        // now**, once per thing, the value the ready view reads too. Read per
        // record, an older record that still carries a list a later write
        // shortened would draw the removed item as a live edge. A claim's own
        // field folds onto nothing, and an archived record's value no longer
        // stands, so those two are still read record by record, the archived
        // one marked.
        let folded = |f: &Fact| f.status != FactStatus::Archived && f.reaching.contains(key);
        let held_now = |thing: &EntityId| -> Vec<EntityId> {
            self.field_links
                .get(thing)
                .and_then(|links| links.get(key))
                .cloned()
                .unwrap_or_default()
        };
        let own = |fact: &Fact| -> Vec<EntityId> {
            fact.field_links.get(key).cloned().unwrap_or_default()
        };
        let via = |retracted: bool| Via {
            link: link(),
            direction,
            retracted,
        };
        match direction {
            Direction::Out => {
                let mut reached: Vec<(Via, EntityId)> = Vec::new();
                let mut things_read: Vec<&EntityId> = Vec::new();
                for f in from {
                    if !f.fields.contains_key(key) {
                        continue;
                    }
                    if folded(f) {
                        if !things_read.contains(&&f.subject) {
                            things_read.push(&f.subject);
                            reached.extend(
                                held_now(&f.subject)
                                    .into_iter()
                                    .map(|handle| (via(false), handle)),
                            );
                        }
                        continue;
                    }
                    let retracted = f.status == FactStatus::Archived;
                    reached.extend(own(f).into_iter().map(|handle| (via(retracted), handle)));
                }
                reached
            }
            Direction::In => {
                let mut reached: Vec<(Via, EntityId)> = Vec::new();
                let mut things_read: Vec<&EntityId> = Vec::new();
                for f in &self.all {
                    if !f.fields.contains_key(key) {
                        continue;
                    }
                    if folded(f) {
                        if !things_read.contains(&&f.subject) {
                            things_read.push(&f.subject);
                            if held_now(&f.subject).contains(id) {
                                reached.push((via(false), f.subject.clone()));
                            }
                        }
                        continue;
                    }
                    if own(f).contains(id) {
                        reached.push((via(f.status == FactStatus::Archived), f.subject.clone()));
                    }
                }
                reached
            }
        }
    }
}

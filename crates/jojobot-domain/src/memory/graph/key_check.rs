//! **The keys of an expression, judged against the whole selection.**
//!
//! A call that hands in candidates reads only those things in full. The key
//! check in [`resolve`](super::resolve) judges the keys the scan holds, so over a
//! partial scan it would call a key held by none when a thing the scan did not
//! read holds it. This check is made once, before the walk reads anything, over
//! the things the call could select.

use super::*;

/// **Refuse a key that no thing of the whole selection holds**, the refusal
/// [`resolve`](super::resolve) makes over a full scan, and refuse nothing else.
///
/// The index answers first: a key some selected thing holds is proven by one
/// lookup. Only a key that lookup does not prove is read out, over the selection
/// (and over the world when a walk keeps by a key), so a misspelt key costs the
/// read that names the keys held and a held key costs none.
pub(super) async fn check_whole_selection<M>(
    store: &M,
    entities: &[Entity],
    sessions: &[DocScan],
    declarations: &[types::DeclaredType],
    query: &GraphQuery,
) -> Result<(), MemoryError>
where
    M: super::super::Memory + ?Sized,
{
    let select = &query.select;
    let declared = |key: &str| declarations.iter().any(|d| d.field(key).is_some());
    let in_selection = |entity: &Entity| {
        select.kind.is_none_or(|kind| entity.kind == kind)
            && select
                .parent
                .as_ref()
                .is_none_or(|parent| entity.parent.as_ref() == Some(parent))
    };
    let selected: HashSet<&EntityId> = entities
        .iter()
        .filter(|entity| in_selection(entity))
        .map(|entity| &entity.id)
        .collect();

    // The keys asked of the selection: its terms (a subject names one thing, and
    // its terms are not judged) and its order, then the keys a walk keeps by.
    let mut of_selection: Vec<(&str, Scope)> = Vec::new();
    if select.subject.is_none() {
        for field in select.fields.iter().filter(|f| f.tree.is_some()) {
            if let Some(tree) = field.tree.as_ref() {
                of_selection.extend(tree.keys().into_iter().map(|key| (key, field.scope)));
            }
        }
        if let Some(order) = select.order.as_ref() {
            of_selection.push((order.key.trim(), Scope::Thing));
        }
    }
    let kept_by: Vec<&str> = query
        .follow
        .iter()
        .flat_map(|follow| follow.keeping.iter())
        .filter_map(|field| field.tree.as_ref())
        .flat_map(Filter::keys)
        .collect();

    let walks_by_keys = !kept_by.is_empty();
    let mut proven_selection = true;
    for (key, scope) in of_selection {
        if declared(key) {
            continue;
        }
        let held = scope == Scope::Thing
            && store
                .keys_holding(key, &Test::Exists, select.kind)
                .await?
                .iter()
                .any(|holder| selected.contains(holder));
        proven_selection &= held;
    }
    let mut proven_world = true;
    for key in kept_by {
        if declared(key) {
            continue;
        }
        let held = !store
            .keys_holding(key, &Test::Exists, None)
            .await?
            .is_empty();
        proven_world &= held;
    }
    if proven_selection && proven_world {
        return Ok(());
    }

    // **Read out what the lookup did not prove**, and let the one check say what
    // it says over a full scan. Its two halves read the selection and the world,
    // so each is read whole or the other half would call a held key unheld.
    let mut scanned: Vec<DocScan> = Vec::with_capacity(entities.len());
    for entity in entities {
        if entity.kind.is_projected() {
            scanned.extend(
                sessions
                    .iter()
                    .find(|doc| doc.entity.as_ref().map(|e| &e.id) == Some(&entity.id))
                    .cloned(),
            );
        } else if selected.contains(&entity.id) || walks_by_keys {
            scanned.push(read_doc(store, entity, false, &query.masked).await?);
        } else {
            scanned.push(DocScan {
                doc_id: entity.id.to_string(),
                title: entity.name.clone(),
                prose: String::new(),
                facts: Vec::new(),
                fields: BTreeMap::new(),
                entity: Some(entity.clone()),
                owner: None,
                unmapped: Vec::new(),
            });
        }
    }
    Ctx::of(&scanned, declarations).check_expression_keys(query)
}

impl Ctx<'_> {
    /// **A term of an expression on a key nothing holds is refused**, naming the
    /// keys that are held. An empty answer to it would read as *there is none*,
    /// and the likelier truth is a misspelt key.
    ///
    /// The population is the things the call could select (its kind, and a
    /// parent when it names one), asked in the scope the expression is asked in:
    /// the folded fields of a thing, or the fields of a record. **A key some type
    /// declares is a real question** even while nothing holds it. A key filter
    /// that is not an expression is not asked: it has always answered empty.
    pub(super) fn check_expression_keys(&self, query: &GraphQuery) -> Result<(), MemoryError> {
        self.check_walk_keys(query)?;
        let select = &query.select;
        if select.subject.is_some() {
            return Ok(());
        }
        let population: Vec<&Entity> = self
            .entities
            .values()
            .copied()
            .filter(|e| self.matches_kind_and_parent(e, select))
            .collect();
        // **A selection that holds nothing answers with nothing.** With no thing
        // to read, no key can be misspelt against it: a project with no work has
        // an empty board, and neither a term nor the order is refused for it.
        // This one condition serves the term check below and `check_order_key`.
        if population.is_empty() {
            return Ok(());
        }
        for field in select.fields.iter().filter(|f| f.tree.is_some()) {
            let tree = field.as_filter();
            let held_on_things: BTreeSet<&str> = population
                .iter()
                .filter_map(|e| self.held(&e.id))
                .flat_map(|fields| fields.keys().map(String::as_str))
                .collect();
            let held_on_records: BTreeSet<&str> = population
                .iter()
                .filter_map(|e| self.facts.get(&e.id))
                .flatten()
                .flat_map(|fact| fact.fields.keys().map(String::as_str))
                .collect();
            let (held_keys, other_keys, other_scope) = match field.scope {
                Scope::Thing => (&held_on_things, &held_on_records, Scope::Record),
                Scope::Record => (&held_on_records, &held_on_things, Scope::Thing),
            };
            for key in tree.keys() {
                let declared = self.declarations.iter().any(|d| d.field(key).is_some());
                if declared || held_keys.contains(key) {
                    continue;
                }
                // **A key held at the other scope is a scope mistake, not a
                // spelling mistake**, and the refusal names the scope that asks
                // it. A key some scope holds is never told to be respelt.
                if other_keys.contains(key) {
                    let advice = match other_scope {
                        Scope::Record => {
                            "the records of these things hold it, so ask the \
                            question of the records with scope: \"record\", or put the key on \
                            the thing with sets"
                        }
                        Scope::Thing => {
                            "the things hold it, so ask the question of what each \
                            thing holds, which is the default scope: leave scope: \"record\" off"
                        }
                    };
                    return Err(MemoryError::InvalidQuery(format!(
                        "no {} among the {} this call selected holds a key named '{key}' in \
                         the scope the question is asked in, but {advice}",
                        match field.scope {
                            Scope::Thing => "thing",
                            Scope::Record => "record",
                        },
                        population.len(),
                    )));
                }
                let in_use = if held_keys.is_empty() {
                    "none of the things this call selected holds any key".to_string()
                } else {
                    format!(
                        "the keys held among them are: {}",
                        held_keys.iter().copied().collect::<Vec<_>>().join(", ")
                    )
                };
                return Err(MemoryError::InvalidQuery(format!(
                    "no thing among the {} this call selected holds a key named '{key}', and no \
                     type declares one, so asking about it could only answer empty. Check the \
                     spelling: {in_use}",
                    population.len(),
                )));
            }
        }
        self.check_order_key(query, &population)
    }

    /// **A walk's `keeping` expression is checked for its keys as a selection's
    /// is.** What a walk reaches is not known before it runs, so the population is
    /// every thing the world holds: a key held by none of them, and declared by no
    /// type, could only answer empty. Asked of the folded fields, which is what a
    /// walk's expression is asked of.
    fn check_walk_keys(&self, query: &GraphQuery) -> Result<(), MemoryError> {
        let Some(follow) = query.follow.as_ref() else {
            return Ok(());
        };
        let held_keys: BTreeSet<&str> = self
            .entities
            .keys()
            .filter_map(|id| self.held(id))
            .flat_map(|fields| fields.keys().map(String::as_str))
            .collect();
        for field in follow.keeping.iter().filter(|f| f.tree.is_some()) {
            for key in field.as_filter().keys() {
                if held_keys.contains(key)
                    || self.declarations.iter().any(|d| d.field(key).is_some())
                {
                    continue;
                }
                return Err(MemoryError::InvalidQuery(format!(
                    "no thing this walk could reach holds a key named '{key}', and no type \
                     declares one, so keeping by it could only drop everything. Check the \
                     spelling: the keys held among them are: {}",
                    held_keys.iter().copied().collect::<Vec<_>>().join(", ")
                )));
            }
        }
        Ok(())
    }

    /// **An order on a key nothing holds is refused**, as a term on it is: every
    /// thing would tie and come back in handle order, which reads as an order.
    /// The population and the declarations are the ones a term is judged by, and
    /// the keys are the folded fields of the things, since an ordering reads what
    /// a thing holds now.
    fn check_order_key(
        &self,
        query: &GraphQuery,
        population: &[&Entity],
    ) -> Result<(), MemoryError> {
        let Some(order) = query.select.order.as_ref() else {
            return Ok(());
        };
        let key = order.key.trim();
        let held_keys: BTreeSet<&str> = population
            .iter()
            .filter_map(|e| self.held(&e.id))
            .flat_map(|fields| fields.keys().map(String::as_str))
            .collect();
        let declared = self.declarations.iter().any(|d| d.field(key).is_some());
        if declared || held_keys.contains(key) {
            return Ok(());
        }
        let in_use = if held_keys.is_empty() {
            "none of the things this call selected holds any key".to_string()
        } else {
            format!(
                "the keys held among them are: {}",
                held_keys.iter().copied().collect::<Vec<_>>().join(", ")
            )
        };
        Err(MemoryError::InvalidQuery(format!(
            "no thing among the {} this call selected holds a key named '{key}', and no type \
             declares one, so ordering by it would leave them in handle order. Check the \
             spelling: {in_use}",
            population.len(),
        )))
    }
}

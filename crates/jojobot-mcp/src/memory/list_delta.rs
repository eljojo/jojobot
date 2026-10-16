//! **Add or remove one item of a list key, against what the thing holds now.**
//!
//! A key declared a list holds its items as one value, so changing one item by
//! sending the whole list is how a careless edit drops another. `capture` and
//! `update_fact` take `sets_add` and `sets_remove` beside `sets`; this is the one
//! function that turns them into the whole value `sets` would have carried.
//!
//! **Resolved at the verb, under the thing's own lock.** The caller holds
//! [`Jojobot::hold_placing`] on the thing from before this reads its fold to the
//! write that stores the answer, so two callers changing the same list read one
//! after the other and neither loses the other's item. What is stored is an
//! ordinary whole value: the reference checks, readiness, history and a retry by
//! request id see nothing different.

use super::*;

use std::collections::BTreeMap;

impl Jojobot {
    /// **Whether this call changes a list by item**, and so must hold the thing's
    /// lock before it resolves anything.
    pub(crate) fn changes_a_list_by_item(
        add: &Option<BTreeMap<String, String>>,
        remove: &Option<BTreeMap<String, String>>,
    ) -> bool {
        add.as_ref().is_some_and(|keys| !keys.is_empty())
            || remove.as_ref().is_some_and(|keys| !keys.is_empty())
    }

    /// **Put the whole value of every key a delta names into `sets`.** The thing's
    /// current value is read from its fold, so a key another record wrote is
    /// changed as it stands and not as this record last saw it.
    ///
    /// Refused, with nothing written: a key declared no list, a key named in
    /// `sets` as well (two answers), and a thing that does not exist.
    pub(crate) async fn resolve_list_deltas(
        &self,
        thing: &EntityId,
        add: Option<BTreeMap<String, String>>,
        remove: Option<BTreeMap<String, String>>,
        sets: &mut BTreeMap<String, String>,
    ) -> Result<(), MemoryError> {
        let (add, remove) = (add.unwrap_or_default(), remove.unwrap_or_default());
        let keys: std::collections::BTreeSet<&String> = add.keys().chain(remove.keys()).collect();
        if keys.is_empty() {
            return Ok(());
        }
        let declared = self.memory.declared_types().await?;
        let kind = thing.kind().map(|kind| kind.as_token());
        for key in &keys {
            if sets.contains_key(*key) {
                return Err(MemoryError::InvalidFact(format!(
                    "'{key}' is named in `sets` and in `sets_add` or `sets_remove`. They are \
                     two answers for one key: send the whole value under `sets`, or the items \
                     to add and take off, not both"
                )));
            }
            if !jojobot_domain::memory::filter::declares_a_list(&declared, kind, key) {
                return Err(MemoryError::InvalidFact(format!(
                    "'{key}' is not declared a list on {thing}, so `sets_add` and \
                     `sets_remove` do not take it. Send its whole value under `sets`, or \
                     declare the key as a list (declare_type, list:text or list:reference) \
                     and send the items again"
                )));
            }
        }
        let held = self.memory.fields(thing).await?;
        let (known, former) = (
            self.memory.list_entities(None).await?,
            self.memory.former_handles().await?,
        );
        for key in keys {
            let items = |from: &BTreeMap<String, String>| -> Vec<String> {
                from.get(key)
                    .map(|value| {
                        jojobot_domain::memory::types::list_items(value)
                            .into_iter()
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let (adding, mut removing) = (items(&add), items(&remove));
            // **An item to take off names the thing it names now.** The list
            // serves each reference under the handle its thing wears today, and a
            // caller may still hold the one it wore. An item that names nothing
            // stays as sent, which is how a reference to a thing that is gone
            // comes off.
            if jojobot_domain::memory::filter::declares_a_reference_list(&declared, kind, key) {
                for item in removing.iter_mut() {
                    if let Some(now) = jojobot_domain::memory::resolve_handle(
                        &EntityId(item.trim().to_string()),
                        &known,
                        &former,
                    ) {
                        *item = now.id.as_str().to_string();
                    }
                }
            }
            let whole = jojobot_domain::memory::types::apply_list_delta(
                held.get(key).map(String::as_str),
                &adding.iter().map(String::as_str).collect::<Vec<_>>(),
                &removing.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            sets.insert(key.clone(), whole);
        }
        Ok(())
    }
}

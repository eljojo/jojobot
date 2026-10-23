//! **The roster a text-taking write resolves against.**

use super::{Attempt, handle_at, named};
use crate::memory::{Entity, EntityId};

/// **The roster, with each numbered address the texts write answering to the
/// member that holds the number.** The member is entered under the address, as
/// the entity it is, so the one resolver that turns a handle into a badge links
/// it with no second path (rule 498). A collection that numbers also enters
/// under [`numbering_marker`], which is what lets a text read `/N` after it as an
/// address: a number nobody holds then names nothing and the write is refused as
/// any handle naming nothing is. After a collection that numbers nothing, `/N` is
/// words and the mention ends at the slash.
///
/// **This is the one function that builds the roster a text-taking write
/// resolves against**, for every store and every caller: the claim, the message,
/// the run's focus and beats, a document's sections. A write that resolves or
/// screens text over a plain list of entities finds no member for a numbered
/// address, so none builds its own.
pub async fn roster_for(
    memory: &dyn crate::memory::Memory,
    texts: &[&str],
) -> Result<Vec<Entity>, crate::memory::MemoryError> {
    let mut known = render_roster(memory).await?;
    let addresses = numbered_addresses(texts);
    if addresses.is_empty() {
        return Ok(known);
    }
    let former = memory.former_handles().await?;
    for (collection, number) in addresses {
        let address = EntityId(format!("{collection}/{number}"));
        let Some(resolved) = crate::memory::resolve_handle(&collection, &known, &former) else {
            continue;
        };
        let resolved_entity = resolved.clone();
        let resolved = resolved.id.clone();
        let held = memory.fields(&resolved).await?;
        let Some(key) = held
            .get(crate::memory::numbered::NUMBERS_BY)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        // The collection numbers, so `/N` after it is a member address (held or
        // not) and never words.
        if !known.iter().any(|e| e.id == numbering_marker(&collection)) {
            known.push(Entity {
                id: numbering_marker(&collection),
                badge: None,
                ..resolved_entity
            });
        }
        let mut holder: Option<Entity> = None;
        for member in crate::memory::numbered::descendants(&resolved, &known) {
            let value = memory.fields(&member.id).await?;
            if value
                .get(&key)
                .and_then(|v| crate::memory::numbered::whole_number(v))
                == Some(number)
            {
                holder = Some(member.clone());
                break;
            }
        }
        if let Some(member) = holder {
            known.push(Entity {
                id: address,
                ..member
            });
        }
    }
    Ok(known)
}

/// The id a collection that numbers its members enters the roster under, beside
/// its members. It holds no badge, so it links nothing.
fn numbering_marker(collection: &EntityId) -> EntityId {
    EntityId(format!("{collection}/#"))
}

/// **The roster a read renders over, and the one every write starts from**: what
/// exists, and each collection that numbers entered under its marker. A text a
/// write left as typed (a number nobody held) renders as unresolved only when the
/// roster says its collection numbers, so a reader has to know that too.
pub async fn render_roster(
    memory: &dyn crate::memory::Memory,
) -> Result<Vec<Entity>, crate::memory::MemoryError> {
    let mut known = memory.list_entities(None).await?;
    let numbering = memory
        .keys_holding(
            crate::memory::numbered::NUMBERS_BY,
            &crate::memory::filter::Test::NotEquals(String::new()),
            None,
        )
        .await?;
    for collection in numbering {
        if let Some(entity) = known.iter().find(|e| e.id == collection).cloned() {
            known.push(Entity {
                id: numbering_marker(&collection),
                badge: None,
                ..entity
            });
        }
    }
    Ok(known)
}

/// Whether `/N` after `collection` is a member address where `holds` answers for
/// the roster: it holds the member, or says the collection numbers.
fn numbers(address: &EntityId, collection: &EntityId, holds: &dyn Fn(&EntityId) -> bool) -> bool {
    holds(address) || holds(&numbering_marker(collection))
}

/// [`handle_at`], reading `/N` as a member address only where `holds` says the
/// collection numbers. A page holds no roster and asks with `|_| true`: a whole
/// number is then an address it links to the collection, and digits that are no
/// number still end the mention at the slash. Anywhere else the mention ends at the slash and `/N`
/// stays words, as before numbered addresses existed.
pub(super) fn mention_among(text: &str, at: usize, holds: &dyn Fn(&EntityId) -> bool) -> Attempt {
    match handle_at(text, at) {
        Attempt::Mention(handle, end) => {
            // A slash and digits no whole number (a leading zero, a number too
            // long to count) is no address at all: the mention ends at the slash.
            let (collection, address) = match crate::memory::numbered::numbered_handle(&handle) {
                Some((collection, _)) => {
                    let address = numbers(&handle, &collection, holds);
                    (collection, address)
                }
                None => match handle.as_str().rsplit_once('/') {
                    Some((collection, _)) => (EntityId(collection.to_string()), false),
                    None => return Attempt::Mention(handle, end),
                },
            };
            match address {
                true => Attempt::Mention(handle, end),
                false => {
                    let end = end - (handle.as_str().len() - collection.as_str().len());
                    Attempt::Mention(collection, end)
                }
            }
        }
        other => other,
    }
}

/// [`mention_among`] over a roster.
pub(super) fn mention_at(text: &str, at: usize, known: &[Entity]) -> Attempt {
    mention_among(text, at, &|id| known.iter().any(|e| e.id == *id))
}

/// **The entities a claim's served words name**, for a walk over the graph. The
/// graph holds no numbered address and no marker, so `/N` after a handle is words
/// there and the mention is the thing.
pub fn served_names(content: &str, details: Option<&str>) -> Vec<EntityId> {
    super::named_words(content, details, &|_| false)
}

/// **The near-miss candidates of a handle that names nothing**, from real things
/// only: a numbering marker is no thing and is never offered.
pub(super) fn screened(
    handle: &EntityId,
    known: &[Entity],
) -> Vec<crate::memory::guard::EntityMatch> {
    let real: Vec<Entity> = known
        .iter()
        .filter(|e| !e.id.as_str().ends_with("/#"))
        .cloned()
        .collect();
    crate::memory::guard::screen(handle, &[], &real)
}

/// **The numbered addresses these texts write**, as the collection and the
/// number, each once. Text inside code is not read.
pub(super) fn numbered_addresses(texts: &[&str]) -> Vec<(EntityId, u64)> {
    let mut found: Vec<(EntityId, u64)> = Vec::new();
    for text in texts {
        for handle in named(text) {
            if let Some(address) = crate::memory::numbered::numbered_handle(&handle)
                && !found.contains(&address)
            {
                found.push(address);
            }
        }
    }
    found
}

/// A reason a write carries (`drop_because`), resolved as its content is.
pub(super) fn resolved_reason(
    reason: Option<&str>,
    known: &[Entity],
) -> Result<Option<String>, crate::memory::MemoryError> {
    reason
        .map(|reason| super::resolved(reason, known))
        .transpose()
        .map_err(super::unloaded)
}

impl super::Mentioning {
    /// **Render the reason an archived thing was archived for**, which is free
    /// text a caller wrote and so carries mentions like any other. Reads the
    /// roster only when a reason holds the stored mark.
    pub(super) async fn render_reasons(
        &self,
        entities: &mut [Entity],
    ) -> Result<(), crate::memory::MemoryError> {
        let marked = |e: &Entity| {
            e.archived
                .as_ref()
                .is_some_and(|a| a.reason.contains(super::MARK))
        };
        if entities.iter().any(marked) {
            let known = self.known().await?;
            for archived in entities.iter_mut().filter_map(|e| e.archived.as_mut()) {
                archived.reason = super::rendered(&archived.reason, &known);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Boot;

    fn thing(handle: &str, badge: Option<&str>) -> Entity {
        Entity {
            folded_names: Vec::new(),
            id: EntityId(handle.into()),
            kind: EntityId(handle.into()).kind().expect("a test handle"),
            name: handle.into(),
            aliases: Vec::new(),
            source: "the roster".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
            merged_into: None,
            badge: badge.map(str::to_string),
            archived: None,
        }
    }

    /// **A number nobody holds renders as unresolved only where the roster says
    /// its collection numbers**, and the same words after a collection the roster
    /// does not say numbers read as the collection with `/N` as words. Both sides,
    /// so a read over the plain list cannot pass for the one that knows.
    #[test]
    fn an_unheld_number_renders_unresolved_only_after_a_collection_that_numbers() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let collection = thing("topic:contract-ordinances-story", Some("k7h2mn"));
        let plain = vec![collection.clone()];
        let mut numbering = plain.clone();
        numbering.push(Entity {
            id: numbering_marker(&collection.id),
            badge: None,
            ..collection
        });
        let typed = "see @topic:contract-ordinances-story/999 now";
        assert_eq!(
            crate::memory::mention::rendered(typed, &numbering),
            "see @topic:contract-ordinances-story/999 (no such handle) now"
        );
        assert_eq!(crate::memory::mention::rendered(typed, &plain), typed);
        // Old stored prose, a badge followed by words that read as a number, serves
        // the same over either roster and is followed to the collection.
        let stored = "see @#k7h2mn/2 now";
        for known in [&plain, &numbering] {
            assert_eq!(
                crate::memory::mention::rendered(stored, known),
                "see @topic:contract-ordinances-story/2 now"
            );
        }
    }
}

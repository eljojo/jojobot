//! **The roster a text-taking write resolves against.**

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
    let mut known = memory.list_entities(None).await?;
    let addresses = super::numbered_addresses(texts);
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

/// Whether `/N` after `collection` is a member address in this roster: the roster
/// holds the member, or says the collection numbers.
pub(super) fn numbers(address: &EntityId, collection: &EntityId, known: &[Entity]) -> bool {
    let marker = numbering_marker(collection);
    known.iter().any(|e| e.id == *address || e.id == marker)
}

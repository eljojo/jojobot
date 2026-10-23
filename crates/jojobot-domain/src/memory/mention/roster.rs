//! **The roster a text-taking write resolves against.**

use crate::memory::{Entity, EntityId};

/// **The roster, with each numbered address the texts write answering to the
/// member that holds the number.** The member is entered under the address, as
/// the entity it is, so the one resolver that turns a handle into a badge links
/// it with no second path (rule 498). A number nobody holds, and a collection
/// that numbers nothing, add nothing: the address then names nothing and the
/// write is refused as any handle naming nothing is.
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
        let resolved = resolved.id.clone();
        let held = memory.fields(&resolved).await?;
        let Some(key) = held
            .get(crate::memory::numbered::NUMBERS_BY)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
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

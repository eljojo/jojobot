//! **The roster a text-taking write resolves against.**

use super::{Attempt, handle_at, named};
use crate::memory::{Entity, EntityId};

#[cfg(test)]
mod address_tests;

/// **The numbered segments after a slug, and where they end.** A numbered
/// member's address ends in a slash and its number (`recall` takes
/// `<collection>/<N>` as a subject), and a member that numbers its own members
/// makes a chain, `<collection>/<N>/<M>`. The slug ends at the first slash, so
/// without this the collection was linked and `/N` stayed behind as words: a link
/// to the wrong thing that reads as the right one.
///
/// **A number is read only when its digits end the segment**: letters or a hyphen
/// after them make `/2abc` the words that follow the collection, never member 2.
/// The chain stops at the first segment that is no number, and what it read
/// before stays, so a whole chain is one address and is never cut at its first
/// number.
pub(super) fn number_chain(text: &str, after: usize) -> (String, usize) {
    let bytes = text.as_bytes();
    let (mut numbers, mut end) = (String::new(), after);
    while bytes.get(end) == Some(&b'/') {
        let digits = super::run_of(text, end + 1, |b| b.is_ascii_digit());
        let stop = end + 1 + digits.len();
        if digits.is_empty() || bytes.get(stop).is_some_and(|b| super::is_slug_byte(*b)) {
            break;
        }
        numbers.push_str(&text[end..stop]);
        end = stop;
    }
    (numbers, end)
}

/// **The member addresses a saved expression names as values**, each written as
/// a mention, for [`roster_for`] to enter in the roster: the one function that
/// builds it reads `@` mentions, and a `where` writes its handles bare. An
/// expression that does not read names none; the parser says why where it is
/// parsed.
pub fn expression_member_mentions(text: &str) -> Vec<String> {
    let Ok(values) = crate::memory::filter::expression::saved_value_words(text) else {
        return Vec::new();
    };
    values
        .iter()
        .map(|value| value.text.trim())
        .filter(|word| {
            crate::memory::numbered::numbered_handle(&EntityId(word.to_string())).is_some()
        })
        .map(|word| format!("@{word}"))
        .collect()
}

impl super::Mentioning {
    /// **[`bind_expression_fields`](super::bind_expression_fields) over a roster
    /// that holds the member addresses the expressions name**, so a saved `where`
    /// binds a member by its number as it binds a thing by its handle.
    pub(super) async fn bind_where(
        &self,
        fields: &mut std::collections::BTreeMap<String, String>,
        known: &[Entity],
        owner: &EntityId,
    ) -> Result<(), crate::memory::MemoryError> {
        let declared = self.declared().await?;
        let mentions: Vec<String> = fields
            .iter()
            .filter(|(key, _)| super::holds_an_expression(key, &declared))
            .flat_map(|(_, value)| expression_member_mentions(value))
            .collect();
        let mut roster = known.to_vec();
        if !mentions.is_empty() {
            let texts: Vec<&str> = mentions.iter().map(String::as_str).collect();
            roster.extend(roster_for(&*self.beneath, &texts).await?);
        }
        super::bind_expression_fields(fields, &declared, &roster, owner)
    }
}

/// **A saved expression with each member's address in a value written as the
/// handle of the member**, for a reader that compares a value with what a field
/// serves, which is the handle. An address the roster does not hold is left as
/// written, and compares with nothing.
pub fn members_named(text: &str, known: &[Entity]) -> String {
    let Ok(values) = crate::memory::filter::expression::saved_value_words(text) else {
        return text.to_string();
    };
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut cut) = (String::new(), 0);
    for value in values {
        let address = EntityId(value.text.trim().to_string());
        let member = known
            .iter()
            .find(|e| e.id == address)
            .and_then(|held| held.badge.as_deref())
            .and_then(|badge| {
                known
                    .iter()
                    .find(|e| !is_synthetic(&e.id) && e.badge.as_deref() == Some(badge))
            });
        let Some(member) = member else { continue };
        out.extend(&chars[cut..value.start]);
        out.push_str(member.id.as_str());
        cut = value.end;
    }
    out.extend(&chars[cut..]);
    out
}

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
        let mut held_numbers = Vec::new();
        // **Only real things hold a number.** A marker or an address entered earlier
        // is no thing the store knows, so it is never read for a number.
        for member in crate::memory::numbered::descendants(&resolved, &known)
            .into_iter()
            .filter(|member| !is_synthetic(&member.id))
        {
            let value = memory.fields(&member.id).await?;
            if let Some(held_number) = value
                .get(&key)
                .and_then(|v| crate::memory::numbered::whole_number(v))
            {
                held_numbers.push(Entity {
                    id: EntityId(format!("{collection}/{held_number}")),
                    ..member.clone()
                });
                if held_number == number {
                    holder = Some(member.clone());
                    break;
                }
            }
        }
        if let Some(member) = holder {
            if !known.iter().any(|held| held.id == address) {
                known.push(Entity {
                    id: address,
                    ..member
                });
            }
        } else {
            // A miss must offer the addresses that can be sent back, not names
            // resembling the collection. These aliases still carry the member's badge.
            for member in held_numbers {
                if !known.iter().any(|held| held.id == member.id) {
                    known.push(member);
                }
            }
        }
    }
    Ok(known)
}

/// Whether an id is one this module entered in the roster beside the real
/// things: a collection's numbering marker or a member's numbered address.
fn is_synthetic(id: &EntityId) -> bool {
    id.as_str().ends_with("/#") || crate::memory::numbered::numbered_handle(id).is_some()
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

/// **The near-miss candidates of a handle that names nothing.** A missing
/// numbered member offers held member addresses; a marker is never offered.
pub(super) fn screened(
    handle: &EntityId,
    known: &[Entity],
) -> Vec<crate::memory::guard::EntityMatch> {
    if let Some((collection, _)) = crate::memory::numbered::numbered_handle(handle)
        && known.iter().any(|e| e.id == numbering_marker(&collection))
    {
        return known
            .iter()
            .filter(|e| {
                crate::memory::numbered::numbered_handle(&e.id)
                    .is_some_and(|(parent, _)| parent == collection)
            })
            .map(|e| crate::memory::guard::EntityMatch {
                handle: e.id.clone(),
                kind: e.kind,
                name: e.name.clone(),
                source: e.source.clone(),
                reason: crate::memory::guard::MatchReason::HeldNumber,
                told_apart_by: None,
            })
            .collect();
    }
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

    async fn nested_numbering() -> crate::memory::testing::InMemoryMemory {
        use crate::memory::{Memory, NewEntity, NewFact};
        let memory = crate::memory::testing::InMemoryMemory::booted();
        let put = |handle: &str, name: &str, parent: Option<&str>, keys: &[(&str, &str)]| {
            let id = EntityId(handle.into());
            let new = NewEntity {
                parent: parent.map(|p| EntityId(p.into())),
                ..NewEntity::new(id.clone(), name, "contract-fixture")
            };
            let fact = NewFact {
                sets: keys.iter().map(|(key, _)| key.to_string()).collect(),
                fields: keys
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                ..NewFact::about(id, "sets its numbering", jiff::civil::date(2026, 10, 23))
            };
            (new, fact, keys.is_empty())
        };
        for (new, fact, bare) in [
            put(
                "topic:contract-ordinances-story",
                "Town ordinances",
                None,
                &[("numbers_by", "number")],
            ),
            put(
                "topic:contract-ordinances-guard",
                "Guard ordinances",
                Some("topic:contract-ordinances-story"),
                &[("numbers_by", "subnumber")],
            ),
            put(
                "work:zeta",
                "Zeta",
                Some("topic:contract-ordinances-guard"),
                &[("number", "2"), ("subnumber", "1")],
            ),
        ] {
            memory
                .add_entity(new)
                .await
                .expect("the fixture is added")
                .written()
                .expect("nothing collides with it");
            if !bare {
                memory
                    .capture(fact)
                    .await
                    .expect("the fixture is written")
                    .written()
                    .expect("nothing collides with it");
            }
        }
        memory
    }

    /// **A numbering marker is no member, so it never stands between a number and
    /// the thing that holds it.** The town's collection numbers, and a guard
    /// collection under it numbers on its own: the guard's marker is a child of
    /// the town's collection in the roster, and sorts before a `work` member below
    /// it. The member is found, and a number nobody holds leaves the roster
    /// without an address entry instead of failing on the marker.
    #[tokio::test]
    async fn a_numbering_marker_is_not_read_for_a_number() {
        let memory = nested_numbering().await;
        let found = roster_for(&memory, &["see @topic:contract-ordinances-story/2"])
            .await
            .expect("the number is found below the guard collection");
        let held = found
            .iter()
            .find(|e| e.id.as_str() == "topic:contract-ordinances-story/2")
            .expect("the address names a member");
        assert_eq!(held.name, "Zeta");
        let missing = roster_for(&memory, &["see @topic:contract-ordinances-story/9"])
            .await
            .expect("a number nobody holds is a miss and not a failure");
        assert!(
            missing
                .iter()
                .all(|e| e.id.as_str() != "topic:contract-ordinances-story/9"),
            "no member holds 9"
        );
    }
}

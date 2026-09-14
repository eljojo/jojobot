use super::support::{add, capture, edit};
use super::*;

// --- retrieval: the search verb ------------------------------------------
//
// These run against a store that also carries the search projection. They
// are scoped to handles this suite owns, so they hold against a shared,
// pre-populated collection as much as against an empty fake.

/// Search the store, expecting the query to be well-formed.
async fn found<S: Search>(search: &S, query: SearchQuery) -> Vec<Hit> {
    search
        .search(&query)
        .await
        .unwrap_or_else(|e| panic!("search should succeed: {e}"))
}

/// The facts in a result list, by address.
fn fact_hits(hits: &[Hit]) -> Vec<&Fact> {
    hits.iter()
        .filter_map(|h| match h {
            Hit::Fact { fact, .. } => Some(&**fact),
            _ => None,
        })
        .collect()
}

/// **Read-back extends to the index.** A fact captured a moment ago is
/// findable by the next search call, with no restart — otherwise "captured"
/// means "written somewhere the assistant can't look".
pub async fn search_finds_a_fact_captured_moments_ago<M: Memory, S: Search>(store: &M, search: &S) {
    let subject = EntityId::person("person:contract-searchable");
    let captured = capture(
        store,
        NewFact::about(
            subject.clone(),
            "keeps a zamboni in the garage",
            date(2026, 7, 1),
        ),
    )
    .await;

    let hits = found(search, SearchQuery::text("zamboni")).await;
    let addresses: Vec<String> = fact_hits(&hits)
        .iter()
        .map(|f| f.address().to_string())
        .collect();
    assert!(
        addresses.contains(&captured.address().to_string()),
        "the fact just captured must be findable without a restart: {hits:?}"
    );
}

/// **A caller's search finds an existing thing by a word it actually
/// carries — a stored thing's own captured name, or a supplied thing's own
/// shipped one** (rule 234): the index is built from the same substrate
/// every other read is, so a record the build supplies and never a caller
/// wrote is exactly as findable as one that was — a search that only ever
/// found rows would make a shipped view unreachable by the one verb whose
/// whole job is finding things by what they say.
async fn existing_thing_is_found_by_its_own_word<S: Search>(
    search: &S,
    word: &str,
    existing: &EntityId,
) {
    let hits = found(search, SearchQuery::text(word)).await;
    assert!(
        hits.iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if &entity.id == existing)),
        "the existing thing must be found by its own word {word:?}: {hits:?}",
    );
}

/// 🚨 **A word an existing thing carries finds it — a stored thing's own
/// captured name, or a supplied thing's own shipped one, asked in one
/// call** (rule 234).
///
/// **Both searches are arguments, not a choice of two functions** — see
/// [`super::add_entity_guards_hold_for_stored_and_supplied`], the same
/// shape for the same reason: the supplied half cannot be silently
/// dropped from a suite that calls this one. No store for the supplied
/// half: nothing is written, because there is nothing to write — the
/// fixture wiring the supplied record is the caller's, so the word
/// searched for on that side must match what that fixture names it.
pub async fn a_things_own_word_is_found_stored_and_supplied<M: Memory, SM: Search, SS: Search>(
    stored: &M,
    stored_search: &SM,
    supplied_search: &SS,
) {
    let id = EntityId("thing:contract-searchable-stored".into());
    add(
        stored,
        NewEntity::new(id.clone(), "Zambonium Register", "user-named"),
    )
    .await;
    existing_thing_is_found_by_its_own_word(stored_search, "zambonium", &id).await;
    existing_thing_is_found_by_its_own_word(
        supplied_search,
        "shipped",
        &EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into()),
    )
    .await;
}

/// Every fact hit carries the **whole row** — its address and its provenance
/// included. The address is what an edit needs; the provenance is what keeps a
/// guess from being read as something the user said.
/// **A record found through the index carries both clocks and the day it
/// stays good.**
///
/// The index keeps a record as JSON and builds the hit back from it, so it
/// is a second storage path with a second chance to drop a value — and a
/// read-back inside either store cannot see it, because both halves of that
/// comparison come from the same construction.
///
/// **The negative is paired here on purpose.** A record that made no
/// promise about how long it stays good comes back with none, so this
/// cannot pass on a build that fills the field in on the way through.
pub async fn a_hit_carries_the_clocks_the_store_kept<M: Memory, S: Search>(store: &M, search: &S) {
    let subject = EntityId::person("person:contract-nelson");
    let watched = capture(
        store,
        NewFact {
            stale_after: Some(date(2026, 11, 30)),
            ..NewFact::about(subject.clone(), "rides a penny farthing", date(2026, 7, 1))
        },
    )
    .await;
    capture(
        store,
        NewFact::about(subject.clone(), "owns a penny whistle", date(2026, 7, 1)),
    )
    .await;

    let hits = found(search, SearchQuery::text("penny")).await;
    let facts = fact_hits(&hits);
    let hit = |needle: &str| {
        (*facts
            .iter()
            .find(|f| f.content.contains(needle))
            .unwrap_or_else(|| panic!("the record saying {needle} must come back: {hits:?}")))
        .clone()
    };

    let carried = hit("farthing");
    assert_eq!(
        carried.stale_after,
        Some(date(2026, 11, 30)),
        "the day this reading stays good did not survive the index",
    );
    assert_eq!(
        carried.inserted_at, watched.inserted_at,
        "the moment the store took the record in did not survive the index",
    );
    assert!(
        carried.inserted_at.is_some(),
        "the store kept no stamp at all, so the check above compares two absences",
    );

    let ordinary = hit("whistle");
    assert_eq!(
        ordinary.stale_after, None,
        "a record that made no promise came back carrying one",
    );
}

pub async fn search_fact_hits_carry_an_address_and_provenance<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let subject = EntityId::person("person:contract-searchable");
    capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            details: Some("said so twice".into()),
            ..NewFact::about(subject.clone(), "cycles to the velodrome", date(2026, 7, 1))
        },
    )
    .await;

    let hits = found(search, SearchQuery::text("velodrome")).await;
    let facts = fact_hits(&hits);
    let found_fact = facts
        .iter()
        .find(|f| f.subject == subject)
        .unwrap_or_else(|| panic!("the captured fact must come back: {hits:?}"));
    assert_eq!(found_fact.provenance, Provenance::Testimony);
    assert_eq!(found_fact.details.as_deref(), Some("said so twice"));
    assert_eq!(
        found_fact.address().home,
        subject,
        "the address names its home doc"
    );
    assert_eq!(found_fact.address().local, found_fact.id);
}

/// 🚨 **A term that lived only in a superseded wording is unfindable by
/// default, and findable when a caller opts in — never the other way
/// round.**
///
/// A claim corrected from "borrowed the drill from milhouse on tuesday"
/// to "returned the drill to milhouse" loses `tuesday` from the current
/// wording. The edit was correct; the word is still real history. This is
/// the exact gap `include_history` exists to close.
///
/// **Both halves in one read.** Without the flag the term is genuinely
/// unfindable — not a coverage gap, not a relaxed-match miss, the corpus
/// itself. With it, the SAME fact — same address, same current content —
/// comes back, never a second thing standing in its own right.
pub async fn a_term_from_a_superseded_wording_is_found_only_when_asked_for<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let subject = EntityId::person("person:contract-superseded-wording");
    let captured = capture(
        store,
        NewFact::about(
            subject.clone(),
            "borrowed the drill from milhouse on tuesday",
            date(2026, 7, 1),
        ),
    )
    .await;
    edit(
        store,
        &captured.address(),
        FactPatch {
            content: Some("returned the drill to milhouse".into()),
            ..Default::default()
        },
    )
    .await;

    let ordinary = found(search, SearchQuery::text("tuesday")).await;
    assert!(
        fact_hits(&ordinary).is_empty(),
        "by default, a word only the superseded wording carried is unfindable: {ordinary:?}"
    );

    let widened = found(
        search,
        SearchQuery {
            include_history: true,
            ..SearchQuery::text("tuesday")
        },
    )
    .await;
    let facts = fact_hits(&widened);
    let hit = facts
        .iter()
        .find(|f| f.address() == captured.address())
        .unwrap_or_else(|| {
            panic!("the claim is found once its earlier wording is searched: {widened:?}")
        });
    assert_eq!(
        hit.content, "returned the drill to milhouse",
        "the hit is the CURRENT record, never the superseded wording standing in for it: \
         {widened:?}"
    );

    // The negative that gives the positive meaning: a word only ever in
    // the CURRENT wording is found either way, so include_history is a
    // widening and not a second, different query.
    let current_word = found(
        search,
        SearchQuery {
            include_history: true,
            ..SearchQuery::text("returned")
        },
    )
    .await;
    assert!(
        fact_hits(&current_word)
            .iter()
            .any(|f| f.address() == captured.address()),
        "a word in the current wording is still found with the flag on: {current_word:?}"
    );
}

/// A superseded fact is **out of a default search** — a claim the store has
/// already moved past coming back as current truth is worse than no memory
/// at all — and `status: superseded` is how it is reached deliberately, so
/// nothing is destroyed, only demoted.
///
/// This is the default-exclusion contract.
/// **A retracted record is out of a default search, and reachable when
/// asked for by name.** The same rule superseded lives under, for a
/// different reason: superseded says a later claim replaced this one,
/// retracted says it should not have been recorded — and neither is
/// something a reader should be handed as current truth.
///
/// The reachable half matters more here than it does for superseded.
/// Nothing is deleted, so the record has to stay findable by somebody who
/// goes looking; a mark that hid a record from every possible read would
/// be a delete with extra steps.
pub async fn search_excludes_a_retracted_record_by_default<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let subject = EntityId::person("person:contract-retraction-hit");
    let live = capture(
        store,
        NewFact::about(subject.clone(), "the quartet rehearsed", date(2026, 7, 1)),
    )
    .await;
    let taken_back = capture(
        store,
        NewFact::about(
            subject.clone(),
            "the quartet rehearsed twice",
            date(2026, 7, 2),
        ),
    )
    .await;
    store
        .retract(
            &taken_back.address(),
            Some("it never happened"),
            date(2026, 7, 3),
        )
        .await
        .expect("retracting a record should succeed");

    let addresses = |hits: &[Hit]| -> Vec<String> {
        fact_hits(hits)
            .iter()
            .map(|f| f.address().to_string())
            .collect()
    };

    let default = found(search, SearchQuery::text("rehearsed")).await;
    let seen = addresses(&default);
    // Both halves, because "not in the results" on its own passes just as
    // well when the query matched nothing at all.
    assert!(
        seen.contains(&live.address().to_string()),
        "the record that still stands must be found: {default:?}"
    );
    assert!(
        !seen.contains(&taken_back.address().to_string()),
        "a retracted record must not come back as current: {default:?}"
    );

    let asked = found(
        search,
        SearchQuery {
            status: Some(FactStatus::Archived),
            ..SearchQuery::text("rehearsed")
        },
    )
    .await;
    assert!(
        addresses(&asked).contains(&taken_back.address().to_string()),
        "nothing was deleted, so asking for it by name finds it: {asked:?}"
    );
}

pub async fn search_excludes_superseded_by_default_and_lists_it_on_request<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let subject = EntityId::person("person:contract-search-superseded");
    let live = capture(
        store,
        NewFact::about(subject.clone(), "plays the theremin", date(2026, 7, 1)),
    )
    .await;
    let retired = capture(
        store,
        NewFact::about(
            subject.clone(),
            "plays the theremin on Tuesdays",
            date(2026, 7, 2),
        ),
    )
    .await;
    edit(
        store,
        &retired.address(),
        FactPatch {
            status: Some(FactStatus::Archived),
            ..Default::default()
        },
    )
    .await;

    let default = found(search, SearchQuery::text("theremin")).await;
    let addresses: Vec<String> = fact_hits(&default)
        .iter()
        .map(|f| f.address().to_string())
        .collect();
    assert!(
        addresses.contains(&live.address().to_string()),
        "the active fact must be found: {default:?}"
    );
    assert!(
        !addresses.contains(&retired.address().to_string()),
        "a superseded fact must not come back as current truth: {default:?}"
    );

    let asked = found(
        search,
        SearchQuery {
            status: Some(FactStatus::Archived),
            ..SearchQuery::text("theremin")
        },
    )
    .await;
    let asked_addresses: Vec<String> = fact_hits(&asked)
        .iter()
        .map(|f| f.address().to_string())
        .collect();
    assert!(
        asked_addresses.contains(&retired.address().to_string()),
        "asking for it by name is how a superseded fact is reached: {asked:?}"
    );
    assert!(
        !asked_addresses.contains(&live.address().to_string()),
        "…and that list holds only the superseded ones: {asked:?}"
    );
}

/// **Ask-across, the capability this milestone exists for:** one call answers
/// "which people are in X". The filter walks the typed edges, so a fact that
/// merely *mentions* X in its text is not an answer — that difference is the
/// whole reason edges are written at capture instead of inferred later.
pub async fn search_answers_ask_across_by_kind_and_edge<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let far = EntityId("place:contract-faraway".into());
    let here = capture_at(store, "contract-away-one", &far, date(2026, 7, 1)).await;
    let there = capture_at(store, "contract-away-two", &far, date(2026, 7, 2)).await;

    // A fact that talks about the place but draws no edge to it.
    let talker = EntityId::person("person:contract-away-talker");
    capture(
        store,
        NewFact::about(
            talker.clone(),
            "keeps talking about contract-faraway",
            date(2026, 7, 3),
        ),
    )
    .await;
    // …and a place that is edged there but is not a person.
    let project = EntityId("project:contract-away-project".into());
    capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Location, far.clone())),
            ..NewFact::about(
                project.clone(),
                "runs out of contract-faraway",
                date(2026, 7, 4),
            )
        },
    )
    .await;

    let hits = found(
        search,
        SearchQuery {
            kind: Some(EntityKind::PERSON),
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::Location),
                object: far.clone(),
            }),
            ..Default::default()
        },
    )
    .await;
    let mut subjects: Vec<String> = fact_hits(&hits)
        .iter()
        .map(|f| f.subject.to_string())
        .collect();
    subjects.sort();
    subjects.dedup();
    assert_eq!(
        subjects,
        vec![here.subject.to_string(), there.subject.to_string()],
        "exactly the people edged there — not the one who merely mentions it, \
         not the project that is: {hits:?}"
    );
}

/// An edge filter with **no shape** answers "what's connected to X" — every
/// edge pointing at it, whatever its shape.
pub async fn search_by_edge_object_alone_finds_any_shape<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let fest = EntityId("event:contract-connected-fest".into());
    let attendee = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Attendance, fest.clone())),
            ..NewFact::about(
                EntityId::person("person:contract-conn-one"),
                "went both nights",
                date(2026, 7, 1),
            )
        },
    )
    .await;
    let about = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::About, fest.clone())),
            ..NewFact::about(
                EntityId("work:contract-conn-mix".into()),
                "recorded live that weekend",
                date(2026, 7, 2),
            )
        },
    )
    .await;

    // A fact drawing the same shape at a DIFFERENT event. Without it this
    // spec is a containment assertion, and an `edge` filter that matched
    // everything would satisfy it — "connected to X" has to mean X.
    let elsewhere = capture(
        store,
        NewFact {
            edge: Some(Edge::new(
                EdgeShape::Attendance,
                EntityId("event:contract-connected-other".into()),
            )),
            ..NewFact::about(
                EntityId::person("person:contract-conn-two"),
                "went to the other one",
                date(2026, 7, 3),
            )
        },
    )
    .await;

    let hits = found(
        search,
        SearchQuery {
            edge: Some(EdgeFilter {
                shape: None,
                object: fest,
            }),
            ..Default::default()
        },
    )
    .await;
    let addresses: Vec<String> = fact_hits(&hits)
        .iter()
        .map(|f| f.address().to_string())
        .collect();
    for expected in [attendee.address(), about.address()] {
        assert!(
            addresses.contains(&expected.to_string()),
            "every shape pointing at it must come back, got {addresses:?}"
        );
    }
    assert!(
        !addresses.contains(&elsewhere.address().to_string()),
        "…and only the ones pointing at it: {addresses:?}"
    );
}

/// A query that names an entity outright puts **that entity first** — decided
/// by the write guard's own matcher, so search and the guard can never
/// disagree about what counts as the same thing.
pub async fn search_pins_a_named_entity_first<M: Memory, S: Search>(store: &M, search: &S) {
    let handle = EntityId("org:contract-pinnable-guild".into());
    add(
        store,
        NewEntity::new(handle.clone(), "Pinnable Guild", "user-named"),
    )
    .await;
    // Facts that also match the query text, so the pin has something to beat.
    capture(
        store,
        NewFact::about(
            handle.clone(),
            "meets at the contract-pinnable-guild hall",
            date(2026, 7, 1),
        ),
    )
    .await;

    let hits = found(search, SearchQuery::text(handle.as_str())).await;
    assert!(
        matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == handle),
        "an exact handle query must return that entity first: {hits:?}"
    );
}

/// **No bare hits.** A fact hit names the entity it is about and the entity
/// whose page it sits on — handle, kind AND display name — so a reader knows
/// what came back without spending a call per handle to find out.
///
/// The name is the part that cannot be derived: a handle carries its kind in
/// its grammar, but `person:contract-orient` says nothing about who that is.
pub async fn search_fact_hits_name_their_subject_and_home<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let subject = EntityId::person("person:contract-orienteer");
    add(
        store,
        NewEntity {
            aliases: vec!["Contract Compass".into()],
            ..NewEntity::new(subject.clone(), "Orienteering Otto", "user-named")
        },
    )
    .await;
    capture(
        store,
        NewFact::about(subject.clone(), "reads a map for fun", date(2026, 7, 1)),
    )
    .await;

    let hits = found(search, SearchQuery::text("map for fun")).await;
    let (fact_subject, fact_home) = hits
        .iter()
        .find_map(|h| match h {
            Hit::Fact {
                fact,
                subject: s,
                home,
                ..
            } if fact.subject == subject => Some((s.clone(), home.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the captured fact must come back: {hits:?}"));

    assert_eq!(fact_subject.id, subject);
    assert_eq!(fact_subject.kind, Some(EntityKind::PERSON));
    assert_eq!(
        fact_subject.name.as_deref(),
        Some("Orienteering Otto"),
        "a hit that names only the handle is the bare hit this exists to kill"
    );
    // Every name it answers to, not only the preferred one: a search on the
    // nickname otherwise returns a row labelled with a name the asker did
    // not use and has no way to connect to the one they did.
    assert_eq!(
        fact_subject.aliases,
        vec!["Contract Compass".to_string()],
        "the nickname rides along with the hit that names them"
    );
    // A single-subject capture homes the row on its own subject, so the two
    // agree here. What matters is that home is *resolved*, not that it
    // differs — a reader has to be able to tell when it does.
    assert_eq!(fact_home.id, subject);
    assert_eq!(fact_home.name.as_deref(), Some("Orienteering Otto"));
    assert_eq!(fact_home.aliases, vec!["Contract Compass".to_string()]);
}

/// An entity hit arrives with **where it sits in the graph** — the edges its
/// facts draw. Asking about someone and getting back only their name is the
/// same bare answer as a fact with no subject: the surroundings are the part
/// that makes the next question askable.
pub async fn search_entity_hits_carry_their_edges<M: Memory, S: Search>(store: &M, search: &S) {
    let handle = EntityId("org:contract-orient-guild".into());
    let hall = EntityId("place:contract-orient-hall".into());
    add(
        store,
        NewEntity::new(handle.clone(), "Orienting Guild", "user-named"),
    )
    .await;
    capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Location, hall.clone())),
            ..NewFact::about(
                handle.clone(),
                "meets on the first Sunday",
                date(2026, 7, 1),
            )
        },
    )
    .await;

    let hits = found(search, SearchQuery::text(handle.as_str())).await;
    let edges = hits
        .iter()
        .find_map(|h| match h {
            Hit::Entity { entity, edges, .. } if entity.id == handle => Some(edges.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the entity must come back: {hits:?}"));

    assert!(
        edges.contains(&Edge::new(EdgeShape::Location, hall)),
        "the entity's own edges ride along with it: {edges:?}"
    );
}

/// Capture a fact placing `who` at `place`, and return it.
async fn capture_at<M: Memory>(store: &M, who: &str, place: &EntityId, on: Date) -> Fact {
    capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Location, place.clone())),
            ..NewFact::about(EntityId::person(who), "spending the season there", on)
        },
    )
    .await
}

/// **Search finds a record by the keys it carries, and the record never
/// said which type it was.**
///
/// This is the card's load-bearing claim, exercised through the verb a
/// caller actually uses rather than through the matcher. The records here
/// name no type at all, so nothing on a record admits it to the answer:
/// only its keys do. A build that matched on a name the record carried
/// would pass every case in the domain and fail this one.
///
/// Both matches come back in the one answer — complete and partial — and
/// the partial names what it lacks. Complete-versus-partial is reported,
/// never filtered.
pub async fn search_finds_things_that_answer_a_type_structurally<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let declared = store
        .declare_type(DeclaredType::new(
            "contract-crate",
            vec![
                Field::required("weight", ValueType::Number),
                Field::required("arrives", ValueType::Date),
            ],
        ))
        .await
        .expect("declaring should succeed");

    // **Carries both keys across TWO records, and neither answers alone.**
    // This is the whole point of the unit: what a thing is gets written
    // down a piece at a time, and a store full of half-descriptions is what
    // a real one looks like.
    let whole = EntityId::person("person:contract-crate-whole");
    for (key, value, said) in [
        ("weight", "12", "somebody weighed it"),
        ("arrives", "2026-08-10", "and somebody else was told when"),
    ] {
        capture(
            store,
            NewFact {
                fields: [(key.to_string(), value.to_string())].into_iter().collect(),
                ..NewFact::about(whole.clone(), said, date(2026, 8, 1))
            },
        )
        .await;
    }
    // Carries one of them, and nothing else ever says the rest.
    let partial = EntityId::person("person:contract-crate-partial");
    capture(
        store,
        NewFact {
            fields: [("weight".to_string(), "3".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                partial.clone(),
                "a lighter one, and nobody wrote down when it lands",
                date(2026, 8, 2),
            )
        },
    )
    .await;
    // Carries neither, and is a thing all the same.
    let unrelated = EntityId::person("person:contract-crate-unrelated");
    capture(
        store,
        NewFact {
            fields: [("mood".to_string(), "curious".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                unrelated.clone(),
                "nothing to do with crates",
                date(2026, 8, 3),
            )
        },
    )
    .await;

    let hits = found(
        search,
        SearchQuery {
            answers_type: Some(declared),
            limit: 50,
            ..Default::default()
        },
    )
    .await;
    // **The answer is about THINGS**, so it is the entity hits that carry
    // the match. A fact hit could not: the question was never asked of one
    // row.
    let answered: Vec<(&EntityId, &crate::memory::types::Match)> = hits
        .iter()
        .filter_map(|h| match h {
            Hit::Entity {
                entity, answers, ..
            } => Some((&entity.id, answers.as_deref()?)),
            _ => None,
        })
        .collect();

    let (_, whole_match) = answered
        .iter()
        .find(|(id, _)| **id == whole)
        .unwrap_or_else(|| panic!("a thing carrying every key must come back: {answered:?}"));
    assert!(
        whole_match.complete(),
        "two records between them hold every key, and the thing says so \
         rather than the caller counting: {whole_match:?}",
    );

    let (_, partial_match) = answered
        .iter()
        .find(|(id, _)| **id == partial)
        .unwrap_or_else(|| panic!("a partial match is returned, not filtered out: {answered:?}"));
    assert!(!partial_match.complete(), "{partial_match:?}");
    assert_eq!(
        partial_match.lacking,
        vec!["arrives"],
        "and it names what it lacks, by name: {partial_match:?}",
    );

    // **The negative, and it rests on the two positives above.** A thing
    // sharing no key with the type is not a weak match, it is not a match:
    // without this, "it matched" says nothing, because everything would.
    assert!(
        !answered.iter().any(|(id, _)| **id == unrelated),
        "a thing carrying none of the keys is not in the answer: {answered:?}",
    );
}

/// **The strict question keeps only what fits; the tolerant one still
/// reports the gaps.**
///
/// Which question a reader is asking is the reader's choice, and `recall`
/// is what lets them make it. Both halves in one case: strict alone passes
/// on a build that returns nothing, and tolerant alone passes on a build
/// that never asked the strict question.
///
/// **The tolerant one is what a caller naming neither gets.** A thing
/// arriving with its gaps named can neither hide nor overclaim; a thing
/// missing from an answer looks exactly like a thing that is not there.
pub async fn search_keeps_only_what_fits_when_the_caller_asks<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    // **Declared the way a caller declares one.** A key is optional unless
    // somebody says otherwise, and the surface tells callers to leave it
    // that way — so a type whose every key is required is a shape almost no
    // caller produces, and a strict question proved only against that shape
    // is proved against nothing anybody asks.
    let declared = store
        .declare_type(DeclaredType::new(
            "contract-pallet",
            vec![
                Field::new("stacked", ValueType::Number),
                Field::new("shipped_on", ValueType::Date),
            ],
        ))
        .await
        .expect("declaring should succeed");

    let whole = EntityId("thing:contract-pallet-whole".into());
    for (key, value, said) in [
        ("stacked", "12", "somebody counted the boxes"),
        (
            "shipped_on",
            "2026-08-10",
            "and somebody else stamped the day",
        ),
    ] {
        capture(
            store,
            NewFact {
                fields: [(key.to_string(), value.to_string())].into_iter().collect(),
                ..NewFact::about(whole.clone(), said, date(2026, 8, 1))
            },
        )
        .await;
    }
    let partial = EntityId("thing:contract-pallet-half".into());
    capture(
        store,
        NewFact {
            fields: [("stacked".to_string(), "3".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                partial.clone(),
                "a shorter stack, and no day",
                date(2026, 8, 2),
            )
        },
    )
    .await;

    let strict = found(
        search,
        SearchQuery {
            fits_type: Some(declared.clone()),
            limit: 50,
            ..Default::default()
        },
    )
    .await;
    let kept: Vec<&EntityId> = strict
        .iter()
        .filter_map(|h| match h {
            Hit::Entity { entity, .. } => Some(&entity.id),
            _ => None,
        })
        .collect();
    assert!(
        kept.contains(&&whole),
        "a thing holding every key is what the strict question is for: {kept:?}"
    );
    assert!(
        !kept.contains(&&partial),
        "…and a thing with a gap is not in that answer: {kept:?}"
    );

    // The other question, over the same two things: both come back, and
    // the incomplete one says what it lacks rather than disappearing.
    let tolerant = found(
        search,
        SearchQuery {
            answers_type: Some(declared),
            limit: 50,
            ..Default::default()
        },
    )
    .await;
    let answered: Vec<(&EntityId, &crate::memory::types::Match)> = tolerant
        .iter()
        .filter_map(|h| match h {
            Hit::Entity {
                entity, answers, ..
            } => Some((&entity.id, answers.as_deref()?)),
            _ => None,
        })
        .collect();
    let (_, gapped) = answered
        .iter()
        .find(|(id, _)| **id == partial)
        .unwrap_or_else(|| panic!("the tolerant question keeps the gapped thing: {answered:?}"));
    assert_eq!(
        gapped.lacking,
        vec!["shipped_on"],
        "…and names the gap: {gapped:?}"
    );
    assert!(
        answered.iter().any(|(id, _)| **id == whole),
        "…without losing the whole one: {answered:?}"
    );
}

/// **A type query says which keys are wrong, and returns the thing
/// anyway.**
///
/// The typed path is not a gate at read time either. A value that does not
/// hold what the type declared comes back flagged, with what was declared
/// and what is actually there, on a thing that is still found.
pub async fn a_type_query_flags_a_bad_value_and_returns_the_record<M: Memory, S: Search>(
    store: &M,
    search: &S,
) {
    let declared = store
        .declare_type(DeclaredType::new(
            "contract-pallet",
            vec![Field::required("arrives", ValueType::Date)],
        ))
        .await
        .expect("declaring should succeed");

    let messy = EntityId::person("person:contract-pallet-messy");
    capture(
        store,
        NewFact {
            fields: [("arrives".to_string(), "next tuesday".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                messy.clone(),
                "somebody wrote the date in words",
                date(2026, 8, 4),
            )
        },
    )
    .await;

    let hits = found(
        search,
        SearchQuery {
            answers_type: Some(declared),
            limit: 50,
            ..Default::default()
        },
    )
    .await;
    let flagged = hits
        .iter()
        .find_map(|h| match h {
            Hit::Entity {
                entity, answers, ..
            } if entity.id == messy => answers.as_deref(),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the thing is found, not dropped: {hits:?}"));

    // The tolerant question keeps it and says what is wrong with it, which
    // is the point of this case: no key is absent…
    assert!(flagged.lacking.is_empty(), "{flagged:?}");
    // …and it still does not fit, because holding a key badly is not
    // holding it.
    assert!(
        !flagged.complete(),
        "a date slot holding a phrase leaves the type unanswered: {flagged:?}",
    );
    assert_eq!(flagged.mistyped.len(), 1, "{flagged:?}");
    assert_eq!(flagged.mistyped[0].key, "arrives");
    assert_eq!(flagged.mistyped[0].declared, ValueType::Date);
    assert_eq!(
        flagged.mistyped[0].value, "next tuesday",
        "the reader sees the mistake rather than being told one happened",
    );
}

/// Run the whole contract, **including retrieval**, against a store that
/// carries the search projection. The search half can't live in `run_all`:
/// the bare Memory port has no read side for it.
pub async fn run_all_searchable<M: Memory, S: Search>(store: &M, search: &S) {
    run_all(store).await;

    search_finds_a_fact_captured_moments_ago(store, search).await;
    search_fact_hits_carry_an_address_and_provenance(store, search).await;
    a_term_from_a_superseded_wording_is_found_only_when_asked_for(store, search).await;
    a_hit_carries_the_clocks_the_store_kept(store, search).await;
    search_excludes_superseded_by_default_and_lists_it_on_request(store, search).await;
    search_excludes_a_retracted_record_by_default(store, search).await;
    search_answers_ask_across_by_kind_and_edge(store, search).await;
    search_by_edge_object_alone_finds_any_shape(store, search).await;
    search_pins_a_named_entity_first(store, search).await;
    search_fact_hits_name_their_subject_and_home(store, search).await;
    search_entity_hits_carry_their_edges(store, search).await;

    search_finds_things_that_answer_a_type_structurally(store, search).await;
    search_keeps_only_what_fits_when_the_caller_asks(store, search).await;
    a_type_query_flags_a_bad_value_and_returns_the_record(store, search).await;
}

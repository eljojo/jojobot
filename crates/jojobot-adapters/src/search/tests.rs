use jiff::civil::date;
use jojobot_domain::mailbox::testing::{InMemoryMailboxes, contract as mail_contract};
use jojobot_domain::mailbox::{MailboxName, Message, MessageId, MessageState};
use jojobot_domain::memory::search::{
    DEFAULT_LIMIT, EdgeFilter, EntityRef, RankClock, SourceStanding,
};
use jojobot_domain::memory::testing::{InMemoryMemory, contract};
use jojobot_domain::memory::{
    Archived, Boot, Edge, EdgeShape, FactStatus, KeyWrite, NewEntity, NewFact, Provenance,
    Standing, folded_fields, validate_subject,
};

use super::*;

/// The shared contract's search case (rule 234), asked of a stored row
/// and a record the build supplies, in one call. A supplied record is
/// never captured, so nothing writes it into the index incrementally
/// the way the stored half relies on — `rebuild` is what a boot does,
/// and it is the only path that reaches a record the build supplies
/// and a caller never touches.
#[tokio::test]
async fn a_things_own_word_is_found_stored_and_supplied() {
    let stored =
        Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens"));

    let id = EntityId(contract::SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    let supplied = jojobot_domain::memory::owned::Provisions::new(vec![
        jojobot_domain::memory::owned::Provision::record(
            Entity {
                id: id.clone(),
                kind: EntityKind::VIEW,
                name: "Contract Shipped View".into(),
                aliases: Vec::new(),
                source: "jojobot".into(),
                crm: None,
                parent: None,
                boot: Default::default(),
                merged_into: None,
                badge: None,
                archived: None,
            },
            BTreeMap::new(),
        ),
    ]);
    let inner = Arc::new(crate::provisioned::Provisioned::new(
        InMemoryMemory::booted().knowing(supplied.clone()),
        supplied,
    ));
    let supplied_store = Arc::new(IndexedMemory::new(inner).expect("index opens"));
    supplied_store.rebuild().await.expect("the boot scan reads");

    contract::a_things_own_word_is_found_stored_and_supplied(
        stored.as_ref(),
        &Retrieval::new(stored.index(), vec![stored.clone()]),
        &Retrieval::new(supplied_store.index(), vec![supplied_store.clone()]),
    )
    .await;
}

/// The whole contract — Memory *and* retrieval — against the in-memory fake
/// behind the index. The fast loop.
#[tokio::test]
async fn the_contract_holds_over_the_fake() {
    let store =
        Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens"));
    contract::run_all_searchable(
        store.as_ref(),
        &Retrieval::new(store.index(), vec![store.clone()]),
    )
    .await;
}

/// **The same, dealt into parts: every case, retrieval included, runs once
/// across the parts, and each part holds on an index of its own.**
#[tokio::test]
async fn the_parts_of_the_searchable_contract_run_every_case_once_each_on_its_own_index() {
    let mut ran = Vec::new();
    for part in 0..contract::PARTS {
        let store =
            Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens"));
        ran.push(
            contract::run_part_searchable(
                store.as_ref(),
                &Retrieval::new(store.index(), vec![store.clone()]),
                part,
                contract::PARTS,
            )
            .await,
        );
    }
    assert!(ran.iter().all(|n| *n > 0), "a part ran nothing: {ran:?}");
    assert_eq!(
        ran.iter().sum::<usize>(),
        contract::searchable_case_count(),
        "parts: {ran:?}"
    );
}

// --- the index as a projection --------------------------------------------

/// A doc built by hand, so the index's behaviour can be examined without a
/// store underneath it.
fn scan(doc_id: &str, entity: Option<Entity>, prose: &str, facts: Vec<Fact>) -> DocScan {
    // **The store is what folds a thing's fields, so this stands in for
    // it — by calling the one fold, never by restating what it does.**
    // Which writes count and which keys never fold are the fold's to
    // decide; a fixture with its own copy of those rules drifts from the
    // store silently and teaches the index a system nobody runs.
    //
    // The writes are recovered from the records because a doc built here
    // has no substrate under it: every doc writes each of its keys once, in
    // the order its records are listed, which is the case where the records
    // and the writes agree about what the thing holds. A fixture that needs
    // them to disagree says so itself.
    let mut ordinals: BTreeMap<String, u64> = BTreeMap::new();
    let mut writes: Vec<KeyWrite> = Vec::new();
    for fact in facts
        .iter()
        .filter(|f| entity.as_ref().is_some_and(|e| f.home == e.id))
    {
        for (key, value) in &fact.fields {
            let ordinal = ordinals.entry(key.clone()).or_default();
            *ordinal += 1;
            writes.push(KeyWrite {
                key: key.clone(),
                ordinal: *ordinal,
                value: Some(value.clone()),
                fact: fact.id.clone(),
                status: fact.status,
                note: fact.details.clone(),
                provenance: fact.provenance,
                standing: fact.standing,
                written_at: None,
            });
        }
    }
    // Nothing is declared over a doc built by hand, so every key here folds
    // the unconfigured way. A fixture that needs a counter declares one.
    let fields = folded_fields(&writes, &[]);
    DocScan {
        doc_id: doc_id.into(),
        title: entity.as_ref().map(|e| e.name.clone()).unwrap_or_default(),
        prose: prose.into(),
        entity,
        facts,
        fields,
        owner: None,
    }
}

fn entity(id: &str, name: &str) -> Entity {
    // **The fixture stands a store up, because the set is setup here.**
    // Reading a handle asks the kinds this process loaded, and no case
    // behind this fixture asserts anything about the set — so the set
    // arrives the way a boot delivers it, from what a store holds.
    let _booted = jojobot_domain::memory::testing::InMemoryMemory::booted();
    let id = EntityId(id.into());
    assert!(validate_subject(&id).is_ok(), "test ids are well-formed");
    Entity {
        kind: id.kind().expect("a well-formed id has a kind"),
        id,
        name: name.into(),
        aliases: Vec::new(),
        source: "user-named".into(),
        crm: None,
        parent: None,
        boot: Boot::OnDemand,
        merged_into: None,
        badge: None,
        archived: None,
    }
}

fn fact(home: &str, id: &str, content: &str, on: jiff::civil::Date) -> Fact {
    Fact {
        id: jojobot_domain::memory::FactId(id.into()),
        home: EntityId(home.into()),
        subject: EntityId(home.into()),
        content: content.into(),
        details: None,
        provenance: Provenance::Inference,
        standing: Standing::Open,
        status: FactStatus::Active,
        recorded_at: on,
        happened_at: None,
        happened_through: None,
        edge: None,
        fields: Default::default(),
        refs: Vec::new(),
        derived_from: None,
        stands_for: Vec::new(),
        inserted_at: None,
        stale_after: None,
    }
}

/// A free-text query that asks for mail as well. **Mail is opt-in** — the
/// bare query leaves it out — and the tests below are about what the mail
/// half of the index holds, so they ask for it rather than relying on a
/// default that would hide the flag from every one of them.
fn asking_for_mail(text: &str) -> SearchQuery {
    SearchQuery {
        include_mail: true,
        ..SearchQuery::text(text)
    }
}

fn index_of(scans: Vec<DocScan>) -> FullTextIndex {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_all(&scans, index.reading_begins(), &Default::default())
        .expect("ingest");
    index
}

/// **The hand-built doc holds what the store would hold.**
///
/// [`scan`] stands in for a store, so the day it stops folding the way the
/// store folds it is a fake telling the tests a story about a system that
/// does not exist. The records here are the awkward pair: one taken back,
/// and the account that took it back — which carries the marker key
/// jojobot writes itself and no thing holds.
#[tokio::test]
async fn a_hand_built_doc_holds_what_the_store_would_hold() {
    let store = InMemoryMemory::booted();
    let who = EntityId::person("person:milhouse");
    store
        .add_entity(NewEntity::new(who.clone(), "Milhouse", "user-named"))
        .await
        .expect("the entity lands")
        .written()
        .expect("nothing to disambiguate");
    let claim = store
        .capture(NewFact {
            fields: [("crates".to_string(), "4".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(who.clone(), "four crates arrived", date(2026, 4, 18))
        })
        .await
        .expect("the claim lands")
        .written()
        .expect("nothing to disambiguate");
    store
        .retract(
            &claim.address(),
            Some("it was three"),
            date(2026, 4, 19),
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("the retraction lands");

    let held = store.scan().await.expect("scan ok");
    let stored = held
        .iter()
        .find(|d| d.entity.as_ref().is_some_and(|e| e.id == who))
        .expect("the thing has a doc");
    let built = scan(
        &stored.doc_id,
        stored.entity.clone(),
        &stored.prose,
        stored.facts.clone(),
    );
    assert_eq!(
        built.fields, stored.fields,
        "a doc built here holds what the store holds, or the fixture is teaching \
             the index a rule the store does not have: {:?}",
        stored.facts
    );
}

/// **The read-side leak, closed.** A detail that lives only in a doc's prose —
/// nobody filed it as a fact — comes back in the same ranked list as the fact
/// and entity hits. Without this, "it's in the doc" means "it is gone".
#[tokio::test]
async fn a_match_only_in_prose_comes_back_beside_the_other_hits() {
    let alpha = entity("person:alpha", "Alpha");
    let index = index_of(vec![scan(
        "doc-1",
        Some(alpha.clone()),
        "Alpha is allergic to penicillin, which came up once and never got filed.",
        vec![fact(
            "person:alpha",
            "f1",
            "plays go on Tuesdays",
            date(2026, 7, 1),
        )],
    )]);

    let hits = index
        .search(&SearchQuery::text("penicillin"))
        .expect("search ok");
    let prose: Vec<&Hit> = hits
        .iter()
        .filter(|h| matches!(h, Hit::Prose { .. }))
        .collect();
    assert_eq!(prose.len(), 1, "the prose match must be a hit: {hits:?}");
    let Some(Hit::Prose {
        doc_id,
        entity: owner,
        snippet,
        ..
    }) = prose.first().copied()
    else {
        unreachable!("filtered to prose");
    };
    assert_eq!(doc_id, "doc-1", "a prose hit says which doc to open");
    assert_eq!(
        owner.as_ref().map(|e| &e.id),
        Some(&alpha.id),
        "…and whose entity doc it is"
    );
    assert!(
        snippet.to_lowercase().contains("penicillin"),
        "the snippet must carry the match: {snippet:?}"
    );

    // …and the same query, in one list, still reaches the fact and the entity.
    let mixed = index
        .search(&SearchQuery::text("alpha"))
        .expect("search ok");
    assert!(
        mixed.iter().any(|h| matches!(h, Hit::Entity { .. })),
        "{mixed:?}"
    );
    assert!(
        mixed.iter().any(|h| matches!(h, Hit::Prose { .. })),
        "{mixed:?}"
    );
}

/// Prose in a doc that is nobody's entity is still searchable — a page the
/// user wrote by hand is exactly the page worth finding.
#[tokio::test]
async fn prose_in_a_doc_that_is_no_entity_is_still_found() {
    let index = index_of(vec![scan(
        "doc-loose",
        None,
        "Notes from the trip: the pass was closed on Tuesday.",
        Vec::new(),
    )]);
    let hits = index
        .search(&SearchQuery::text("pass closed"))
        .expect("search ok");
    assert!(
        matches!(hits.first(), Some(Hit::Prose { entity: None, doc_id, .. }) if doc_id == "doc-loose"),
        "{hits:?}"
    );
}

/// Recency breaks a tie with teeth: two facts of equal text relevance come
/// back newest first.
#[tokio::test]
async fn equal_relevance_ranks_the_newer_fact_first() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![
            fact("person:alpha", "f1", "winter kayak trip", date(2024, 1, 1)),
            fact("person:alpha", "f2", "kayak winter trip", date(2026, 1, 1)),
        ],
    )]);
    let hits = index
        .search(&SearchQuery::text("kayak trip"))
        .expect("search ok");
    let ids: Vec<String> = hits
        .iter()
        .filter_map(|h| match h {
            Hit::Fact { fact, .. } => Some(fact.id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ids,
        vec!["f2", "f1"],
        "same words, same length — the newer one leads"
    );
}

/// **Matching every term outranks matching some of them.**
///
/// Every term used to be required, and a caller who half-remembered two
/// words got nothing at all. ⛔️ **Requiring all of them is not what kept a
/// precise question precise — the RANKING is.** So a partial match comes
/// back, below the document that matched everything, and the precise
/// question still gets the precise answer first.
///
/// ⚠️ **The order is the assertion.** A case that only checked both were
/// present would pass against a build that ranked them either way round.
#[tokio::test]
async fn matching_every_term_outranks_matching_some_of_them() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![
            fact(
                "person:alpha",
                "f1",
                "bakes sourdough bread",
                date(2026, 1, 1),
            ),
            fact("person:alpha", "f2", "bakes almond cake", date(2026, 1, 2)),
        ],
    )]);
    let hits = index
        .search(&SearchQuery::text("bakes sourdough"))
        .expect("search ok");
    let contents: Vec<String> = hits
        .iter()
        .filter_map(|h| match h {
            Hit::Fact { fact, .. } => Some(fact.content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        contents,
        vec!["bakes sourdough bread", "bakes almond cake"],
        "the document matching every term did not lead: {hits:?}",
    );
}

/// **Naming an entity outranks matching its text.** A fact that repeats the
/// query's words scores higher on relevance alone; asking for `org:guild` is
/// asking about the guild, so the guild leads. The pin is decided by the write
/// guard's matcher, so "close enough to be the same thing" has one definition
/// in this system rather than two.
#[tokio::test]
async fn naming_an_entity_pins_it_above_a_more_relevant_fact() {
    // A long name spreads the entity's relevance thin; a terse fact about it
    // concentrates the same words — so BM25 alone ranks the fact first.
    let guild = entity(
        "org:guild",
        "Guild of the Northern Riverside Makers and Menders",
    );
    let index = index_of(vec![scan(
        "doc-1",
        Some(guild.clone()),
        "",
        vec![
            fact("org:guild", "f1", "guild", date(2026, 1, 1)),
            fact("org:guild", "f2", "guild night", date(2026, 1, 2)),
        ],
    )]);

    let hits = index
        .search(&SearchQuery::text("org:guild"))
        .expect("search ok");
    assert!(
        matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == guild.id),
        "the named entity must lead, whatever the facts score: {hits:?}"
    );
    assert!(
        hits.iter().any(|h| matches!(h, Hit::Fact { .. })),
        "…and the facts are still in the same list: {hits:?}"
    );
}

/// **A name the user actually says finds the thing.** An entity known as
/// Homer Simpson and called Max Power has to answer to "Max Power" — the entity itself, and
/// the facts on its page, which is what the question was really about.
///
/// A fact carries the labels of the entity **whose page it sits on**. That
/// is what the doc knows at ingest; a fact about X written on Y's page keeps
/// X's handle and not X's nickname, because resolving that would mean a
/// global pass on every write.
#[tokio::test]
async fn a_query_on_an_alias_finds_the_entity_and_the_facts_on_its_page() {
    let homer = Entity {
        aliases: vec!["Max Power".into()],
        ..entity("person:homer", "Homer Simpson")
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(homer.clone()),
        "",
        vec![fact(
            "person:homer",
            "f1",
            "plays the bass",
            date(2026, 1, 1),
        )],
    )]);

    let hits = index
        .search(&SearchQuery::text("Max Power"))
        .expect("search ok");
    assert!(
        matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == homer.id),
        "the entity that wears the nickname leads: {hits:?}"
    );
    assert!(
        hits.iter()
            .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays the bass")),
        "…and the facts on its page come with it: {hits:?}"
    );

    // The display name still works, and the two are not different questions.
    // The handle deliberately does NOT spell the display name out: one that
    // did would put every token of the name into the fact's text through the
    // subject alone, and this assertion would then hold with the labels
    // stripped out entirely — passing while proving nothing.
    assert!(
        index
            .search(&SearchQuery::text("Homer Simpson"))
            .expect("search ok")
            .iter()
            .any(|h| matches!(h, Hit::Fact { .. })),
        "a label is a label, preferred or not"
    );

    // The alias has to be in the POSTINGS, not only in the pin. Pinning
    // fires on a query that names the entity outright ("Max Power"); a query
    // that merely contains the nickname among other words can only be
    // answered by the index, and that is the common case.
    assert!(
        index
            .search(&SearchQuery::text("Max Power person"))
            .expect("search ok")
            .iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id == homer.id)),
        "the entity record itself is indexed under every name it answers to"
    );
}

/// **Out of ordinary search, mirroring `list_entities`'s own default read
/// (rule 60): a browse never surfaces an archived entity.** An entity's
/// facts are a different question — archiving says the SUBJECT is out of
/// scope, never that anything said about it was wrong — so this leaves
/// them untouched and asks only about the entity document itself.
#[tokio::test]
async fn an_archived_entity_is_out_of_ordinary_search() {
    let bart = Entity {
        archived: Some(Archived {
            reason: "a mistaken write".into(),
            at: jiff::Timestamp::constant(1_780_000_000, 0),
        }),
        ..entity("person:bart", "Bart Simpson")
    };
    let milhouse = entity("person:milhouse", "Milhouse Van Houten");
    let index = index_of(vec![
        scan("doc-1", Some(bart.clone()), "", vec![]),
        scan("doc-2", Some(milhouse.clone()), "", vec![]),
    ]);

    let hits = index
        .search(&SearchQuery::text("Bart Simpson"))
        .expect("search ok");
    assert!(
        !hits
            .iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id == bart.id)),
        "an archived entity must not surface as an ordinary search hit: {hits:?}"
    );

    // The pairing: a live entity in the same index still surfaces.
    let hits = index
        .search(&SearchQuery::text("Milhouse Van Houten"))
        .expect("search ok");
    assert!(
        hits.iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id == milhouse.id)),
        "a live entity in the same index still surfaces: {hits:?}"
    );
}

/// **The two halves a fact hit has to keep apart.** A row about Beta written
/// on Alpha's page names both, resolved — that difference is precisely what a
/// reader has to be able to see, and it is invisible if either side comes
/// back as a bare handle.
///
/// And a subject naming nothing comes back with **no name rather than an
/// invented one**. Filling it with the handle would make the orphan look
/// exactly like a resolved hit, which is how the split brain stayed
/// undetected for a milestone.
#[tokio::test]
async fn a_fact_hit_resolves_a_home_and_a_subject_that_differ() {
    let alpha = entity("person:alpha", "Alpha");
    let beta = entity("person:beta", "Beta");
    let guest = Fact {
        subject: beta.id.clone(),
        ..fact(
            "person:alpha",
            "f1",
            "brought the sourdough",
            date(2026, 1, 1),
        )
    };
    let orphan = Fact {
        subject: EntityId("person:ghost".into()),
        ..fact(
            "person:alpha",
            "f2",
            "brought the sourdough too",
            date(2026, 1, 2),
        )
    };
    let index = index_of(vec![
        scan("doc-1", Some(alpha.clone()), "", vec![guest, orphan]),
        scan("doc-2", Some(beta.clone()), "", vec![]),
    ]);

    let hits = index
        .search(&SearchQuery::text("sourdough"))
        .expect("search ok");
    let refs: Vec<(&EntityRef, &EntityRef)> = hits
        .iter()
        .filter_map(|h| match h {
            Hit::Fact { subject, home, .. } => Some((subject, home)),
            _ => None,
        })
        .collect();

    let resolved = refs
        .iter()
        .find(|(s, _)| s.id == beta.id)
        .expect("the row about beta must come back");
    assert_eq!(resolved.0.name.as_deref(), Some("Beta"), "who it is about");
    assert_eq!(resolved.1.id, alpha.id, "…and whose page it sits on");
    assert_eq!(resolved.1.name.as_deref(), Some("Alpha"));

    let ghost = refs
        .iter()
        .find(|(s, _)| s.id.as_str() == "person:ghost")
        .expect("the orphaned row is indexed, not dropped");
    assert_eq!(
        ghost.0.kind,
        Some(EntityKind::PERSON),
        "the handle still declares a kind"
    );
    assert_eq!(
        ghost.0.name, None,
        "a subject that names nothing must read as unresolved, not as itself"
    );
    assert!(
        ghost.0.aliases.is_empty(),
        "…and it answers to nothing either: an unresolvable handle reports no \
             names rather than inventing one from its own slug"
    );
    assert_eq!(
        ghost.1.name.as_deref(),
        Some("Alpha"),
        "its home still resolves"
    );
}

/// An entity's edges are the ones its **facts** draw, wherever those rows are
/// homed — and only its own. A row about someone else, sitting on this page,
/// belongs to that someone else's neighborhood.
#[tokio::test]
async fn entity_hits_carry_the_edges_of_their_own_facts_only() {
    let alpha = entity("person:alpha", "Alpha");
    let beta = entity("person:beta", "Beta");
    let shelbyville = Edge::new(EdgeShape::Location, EntityId("place:shelbyville".into()));
    let guild = Edge::new(EdgeShape::Membership, EntityId("org:guild".into()));
    let index = index_of(vec![
        scan(
            "doc-1",
            Some(alpha.clone()),
            "",
            vec![
                Fact {
                    edge: Some(shelbyville.clone()),
                    ..fact("person:alpha", "f1", "wintering", date(2026, 1, 1))
                },
                // Beta's row, homed on Alpha's page: Beta's edge, not Alpha's.
                Fact {
                    subject: beta.id.clone(),
                    edge: Some(guild.clone()),
                    ..fact("person:alpha", "f2", "joined up", date(2026, 1, 2))
                },
            ],
        ),
        scan("doc-2", Some(beta.clone()), "", vec![]),
    ]);

    let edges_for = |handle: &EntityId| -> Vec<Edge> {
        index
            .search(&SearchQuery::text(handle.as_str()))
            .expect("search ok")
            .iter()
            .find_map(|h| match h {
                Hit::Entity { entity, edges, .. } if &entity.id == handle => Some(edges.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{handle} must come back as an entity hit"))
    };

    assert_eq!(
        edges_for(&alpha.id),
        vec![shelbyville],
        "its own claim's edge"
    );
    assert_eq!(
        edges_for(&beta.id),
        vec![guild],
        "an edge follows the claim's SUBJECT, not the page the row happens to sit on"
    );
}

/// A prose hit carries its doc's entity's neighborhood too. Prose is where
/// this is easiest to lose: the stored payload holds a bare handle, so the
/// entity and its edges are assembled on the way out or not at all.
#[tokio::test]
async fn prose_hits_carry_the_edges_of_their_docs_entity() {
    let neighbor = entity("person:ned-flanders", "Ned Flanders");
    let shop = Edge::new(EdgeShape::Location, EntityId("place:leftorium".into()));
    let index = index_of(vec![scan(
        "doc-prose-edge",
        Some(neighbor.clone()),
        "Keeps a spare key under the third flowerpot; it came up once and never got filed.",
        vec![Fact {
            edge: Some(shop.clone()),
            ..fact(
                "person:ned-flanders",
                "f1",
                "opens on the first Sunday",
                date(2026, 1, 1),
            )
        }],
    )]);

    let hits = index
        .search(&SearchQuery::text("flowerpot"))
        .expect("search ok");
    let Some(Hit::Prose {
        entity: owner,
        edges,
        ..
    }) = hits.iter().find(|h| matches!(h, Hit::Prose { .. }))
    else {
        panic!("the prose match must come back: {hits:?}")
    };
    assert_eq!(owner.as_ref().map(|e| &e.id), Some(&neighbor.id));
    assert_eq!(
        edges,
        &vec![shop],
        "a prose hit sits in the graph too: {edges:?}"
    );
}

/// **A rename changes every hit that names the entity** — not only the hits
/// whose own doc was re-indexed since.
///
/// This is the claim that justifies resolving on the way OUT rather than
/// freezing a name into the stored payload, and it is only testable where a
/// row lives somewhere other than its subject's page: renaming an entity
/// re-indexes that entity's doc alone, so a fact homed elsewhere is never
/// re-read. If the name were stored at ingest, that hit would go on
/// answering with the old one until something unrelated touched its page.
#[tokio::test]
async fn a_rename_reaches_a_hit_whose_own_doc_was_never_reindexed() {
    let renamed = entity("person:milhouse", "Milhouse Van Houten");
    let guest = Fact {
        subject: renamed.id.clone(),
        ..fact(
            "person:alpha",
            "f1",
            "brought the sourdough",
            date(2026, 1, 1),
        )
    };
    let store = Arc::new(
        IndexedMemory::new(Scanned::new(vec![
            scan(
                "doc-alpha",
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![guest],
            ),
            scan("doc-milhouse", Some(renamed.clone()), "", Vec::new()),
        ]))
        .expect("index opens"),
    );
    store.rebuild().await.expect("rebuild");

    // Asked for by the fact's own CONTENT: a row about someone else, sitting
    // on this page, is deliberately not indexed under their labels, so
    // querying the name would fail for an unrelated reason.
    async fn named(store: &Arc<IndexedMemory>, who: &EntityId) -> Option<String> {
        store
            .search_via_port(&SearchQuery::text("sourdough"))
            .await
            .expect("search ok")
            .iter()
            .find_map(|h| match h {
                Hit::Fact { subject, .. } if &subject.id == who => subject.name.clone(),
                _ => None,
            })
    }
    assert_eq!(
        named(&store, &renamed.id).await.as_deref(),
        Some("Milhouse Van Houten")
    );

    store
        .update_entity(
            &renamed.id,
            EntityPatch {
                name: Some("Thrillhouse".into()),
                ..Default::default()
            },
        )
        .await
        .expect("rename ok")
        .written()
        .expect("this double does not guard");

    assert_eq!(
        named(&store, &renamed.id).await.as_deref(),
        Some("Thrillhouse"),
        "the row on the OTHER doc still names them, and must name them correctly"
    );
}

/// A `kind:slug` handle is an ordinary query, not query syntax. tantivy's own
/// parser reads `person:` as a field name and errors — which would make the
/// most natural query in this system a hard failure.
#[tokio::test]
async fn a_handle_shaped_query_is_not_query_syntax() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact("person:alpha", "f1", "plays go", date(2026, 1, 1))],
    )]);
    for query in ["person:alpha", "AND", "a(b", "\"unclosed", "-alpha"] {
        assert!(
            index.search(&SearchQuery::text(query)).is_ok(),
            "{query:?} must be treated as text, not syntax"
        );
    }
}

/// A fact-only filter narrows to facts. Asking for "archived" and getting an
/// entity back — entities have no lifecycle — would be noise dressed as a hit.
#[tokio::test]
async fn a_fact_only_filter_returns_facts_alone() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "Alpha writes about alpha things.",
        vec![fact("person:alpha", "f1", "alpha claim", date(2026, 1, 1))],
    )]);
    let hits = index
        .search(&SearchQuery {
            provenance: Some(Provenance::Inference),
            ..SearchQuery::text("alpha")
        })
        .expect("search ok");
    assert!(!hits.is_empty());
    assert!(
        hits.iter().all(|h| matches!(h, Hit::Fact { .. })),
        "a fact-only filter must not surface entities or prose: {hits:?}"
    );
}

/// 🚨 **A claim about something that happened years ago does not rank as
/// stale.**
///
/// Ranking is a recency boost, and recency read the claim's one date field
/// — which held the day the thing happened whenever a writer put it there.
/// **So a claim recorded this morning about a purchase in 2024 scored as
/// two years old** and lost to anything with a newer-looking day, on a
/// corpus where it was the freshest thing in the store.
///
/// **Ranking reads `recorded_at` and nothing else.** The event day has its
/// own field and no ranking reads it.
///
/// **Both claims are recorded on one day and differ only in what they say
/// happened**, so nothing but the bug could separate them.
#[tokio::test]
async fn a_claim_about_something_long_ago_ranks_by_when_it_was_recorded() {
    let old_event = Fact {
        happened_at: Some(date(2020, 1, 1)),
        ..fact(
            "person:alpha",
            "f1",
            "alpha bought the bike",
            date(2026, 8, 25),
        )
    };
    let recent_event = Fact {
        happened_at: Some(date(2026, 8, 24)),
        ..fact(
            "person:alpha",
            "f2",
            "alpha bought the pump",
            date(2026, 8, 25),
        )
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![old_event, recent_event],
    )]);

    let hits = index
        .search(&SearchQuery::text("alpha bought"))
        .expect("search ok");
    let found = format!("{hits:?}");
    assert!(
        found.contains("bought the bike"),
        "the claim about the older event fell out of the answer: {found}",
    );
    assert!(
        found.contains("bought the pump"),
        "the control claim is missing, so this case measures nothing: {found}",
    );
}

/// 🚨 **Asked for by name, ranking reads `happened_at` instead — and
/// only when asked.**
///
/// The mirror of the case above: two claims recorded on the same day,
/// so `recorded_at` alone cannot order them, and `happened_at` values
/// that disagree about which is newer. `RankClock::HappenedAt` must put
/// the one that happened recently ahead of the one from years ago;
/// `RankClock::RecordedAt` (the case above, unchanged) must not.
#[tokio::test]
async fn asked_for_by_name_ranking_reads_happened_at_instead() {
    let old_event = Fact {
        happened_at: Some(date(2020, 1, 1)),
        ..fact(
            "person:alpha",
            "f1",
            "alpha bought the bike",
            date(2026, 8, 25),
        )
    };
    let recent_event = Fact {
        happened_at: Some(date(2026, 8, 24)),
        ..fact(
            "person:alpha",
            "f2",
            "alpha bought the pump",
            date(2026, 8, 25),
        )
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![old_event, recent_event],
    )]);

    let hits = index
        .search(&SearchQuery {
            rank_clock: RankClock::HappenedAt,
            ..SearchQuery::text("alpha bought")
        })
        .expect("search ok");
    let first = format!("{:?}", hits.first().expect("a hit"));
    assert!(
        first.contains("bought the pump"),
        "asked to rank by happened_at, the claim that happened recently did not come \
             first: {hits:?}",
    );
}

/// 🚨 **The hedged ones can be asked for, and the settled ones stay out.**
///
/// `standing` was stored, served and documented as the axis answering *how
/// sure is anybody*, and nothing could query it: a store full of claims
/// somebody hedged could be read one claim at a time and never gathered.
/// **It is the question an agent asks about its own work** — which of the
/// things I wrote down am I not sure about.
///
/// **Paired, and the pair is the whole case.** Asking for `open` returns
/// the open one AND leaves the settled one out. ⛔️ **The positive alone
/// passes against a filter that ignores its argument**, which is exactly what
/// a field that is indexed and never clauses does.
#[tokio::test]
async fn asking_for_the_open_claims_returns_them_and_leaves_the_settled_ones_out() {
    let hedged = Fact {
        standing: Standing::Open,
        ..fact(
            "person:alpha",
            "f1",
            "alpha may be moving",
            date(2026, 1, 1),
        )
    };
    let settled = Fact {
        standing: Standing::Settled,
        ..fact("person:alpha", "f2", "alpha moved", date(2026, 1, 2))
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![hedged, settled],
    )]);

    let open = index
        .search(&SearchQuery {
            standing: Some(Standing::Open),
            ..SearchQuery::text("alpha")
        })
        .expect("search ok");
    let said = format!("{open:?}");
    assert!(
        said.contains("may be moving"),
        "the hedged claim cannot be asked for: {said}",
    );
    assert!(
        !said.contains("alpha moved"),
        "a settled claim came back to a caller asking what is still in doubt: {said}",
    );
}

/// **A type query is answered from the whole corpus, not from a page of it.**
///
/// The corpus here is deliberately far bigger than the candidate depth the
/// limit buys, and the two things that answer are ordinary in every other
/// way — nothing about them ranks. A type filter that ran over an already
/// truncated page would answer `0` here and say nothing about it, which is
/// byte-identical to "nothing in memory answers this type".
///
/// The small limits are paired with a limit past the whole corpus, in the
/// same read: without the positive, a zero could just as well mean the
/// things were never indexed.
#[tokio::test]
async fn a_type_query_reaches_past_the_candidate_depth() {
    use jojobot_domain::memory::types::{Field as Key, ValueType};
    let declared = DeclaredType::new(
        "delivery",
        vec![
            Key::required("arrives", ValueType::Date),
            Key::required("crates", ValueType::Number),
        ],
    );

    // A crowd of documents carrying no keys, competing for the same page
    // the answering things have to reach, so the two that answer are buried
    // rather than merely present. Prose rather than more things, because
    // prose is collected by the same half of the query and needs no handle
    // of its own.
    let mut scans: Vec<DocScan> = (1..=200)
        .map(|n| {
            scan(
                &format!("doc-plain-{n}"),
                None,
                "an ordinary page with nothing typed on it",
                Vec::new(),
            )
        })
        .collect();
    for (handle, keys) in [
        (
            "person:milhouse",
            vec![("arrives", "2026-08-10"), ("crates", "4")],
        ),
        ("person:beta", vec![("arrives", "2026-08-11")]),
    ] {
        scans.push(scan(
            &format!("doc-{handle}"),
            Some(entity(handle, "Somebody Else")),
            "",
            vec![Fact {
                fields: keys
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                ..fact(
                    handle,
                    "f1",
                    "an ordinary claim, with keys on it",
                    date(2026, 1, 1),
                )
            }],
        ));
    }
    let index = index_of(scans);

    let answering = |limit: usize| -> Vec<String> {
        index
            .search(&SearchQuery {
                answers_type: Some(declared.clone()),
                limit,
                ..Default::default()
            })
            .expect("search ok")
            .iter()
            .filter_map(|h| match h {
                Hit::Entity {
                    entity,
                    answers: Some(_),
                    ..
                } => Some(entity.id.to_string()),
                _ => None,
            })
            .collect()
    };

    let mut past_the_corpus = answering(500);
    past_the_corpus.sort();
    assert_eq!(
        past_the_corpus,
        vec!["person:beta", "person:milhouse"],
        "the corpus holds exactly two things that answer",
    );
    for limit in [20, 50] {
        let mut found = answering(limit);
        found.sort();
        assert_eq!(
            found,
            vec!["person:beta", "person:milhouse"],
            "both things answer at limit {limit}, however deep in the corpus they sit",
        );
    }
}

/// 🚨 **The strict question selects before the depth cut, exactly as the
/// tolerant one does.** A thing answers a type when it carries any of its
/// keys, so the selecting clause is a disjunction, and the page cut after
/// ranking keeps the best few of everything that carries ONE key. A strict
/// question run over that page answers with whatever whole things happened to
/// rank into it — none, when many half-finished things outrank the finished
/// one — and an empty answer reads as "nothing fits this type".
///
/// The crowd here is wider than the depth a limit of one buys. Each member
/// carries two of the type's three keys, so no key is rare; the finished thing
/// carries all three among a great many others, which makes its key field long
/// and ranks it below every one of them. The same read asks the tolerant
/// question and the strict one at the same limit, and the positive the strict
/// one rests on is the finished thing coming back.
#[tokio::test]
async fn a_strict_type_query_reaches_a_whole_thing_ranked_below_the_partial_ones() {
    use jojobot_domain::memory::types::{Field as Key, ValueType};
    let declared = DeclaredType::new(
        "delivery",
        vec![
            Key::required("arrives", ValueType::Date),
            Key::required("crates", ValueType::Number),
            Key::required("driver", ValueType::Text),
        ],
    );
    let carrying = |handle: &str, keys: Vec<(String, String)>| {
        scan(
            &format!("doc-{handle}"),
            Some(entity(handle, "Somebody Else")),
            "",
            vec![Fact {
                fields: keys.into_iter().collect(),
                ..fact(
                    handle,
                    "f1",
                    "an ordinary claim, with keys on it",
                    date(2026, 1, 1),
                )
            }],
        )
    };
    let key = |k: &str, v: &str| (k.to_string(), v.to_string());

    // More half-finished things than a limit of one has depth for.
    let crowd = candidate_depth(1) * 3;
    let pairs = [
        ("arrives", "2026-08-11", "crates", "4"),
        ("crates", "4", "driver", "beta"),
        ("arrives", "2026-08-11", "driver", "beta"),
    ];
    let mut scans: Vec<DocScan> = (0..crowd)
        .map(|n| {
            let (a, av, b, bv) = pairs[n % pairs.len()];
            carrying(
                &format!("person:{}", "p".repeat(n + 1)),
                vec![key(a, av), key(b, bv)],
            )
        })
        .collect();
    // The finished one: every key, and so many others beside them that its
    // key field is long and each match counts for less.
    let mut whole = vec![
        key("arrives", "2026-08-10"),
        key("crates", "4"),
        key("driver", "beta"),
    ];
    whole.extend((0..300).map(|n| key(&format!("other-{n}"), "x")));
    scans.push(carrying("person:milhouse", whole));
    let index = index_of(scans);

    let ids = |query: SearchQuery| -> Vec<String> {
        index
            .search(&query)
            .expect("search ok")
            .iter()
            .filter_map(|h| match h {
                Hit::Entity { entity, .. } => Some(entity.id.to_string()),
                _ => None,
            })
            .collect()
    };

    // The positive the rest rests on: the tolerant question's whole answer
    // holds every thing, so the crowd and the finished thing are all indexed.
    let everything = ids(SearchQuery {
        answers_type: Some(declared.clone()),
        limit: 500,
        ..Default::default()
    });
    assert_eq!(
        everything.len(),
        crowd + 1,
        "every thing carrying a key of the type is in the corpus"
    );

    // The tolerant question at limit one answers with one of the crowd,
    // which is the ranking this case relies on: the finished thing is not
    // what the depth cut keeps first.
    let tolerant = ids(SearchQuery {
        answers_type: Some(declared.clone()),
        limit: 1,
        ..Default::default()
    });
    assert_eq!(tolerant.len(), 1);
    assert_ne!(
        tolerant,
        vec!["person:milhouse".to_string()],
        "the finished thing must rank below the crowd for this case to measure anything"
    );

    for limit in [1, 5] {
        assert_eq!(
            ids(SearchQuery {
                fits_type: Some(declared.clone()),
                limit,
                ..Default::default()
            }),
            vec!["person:milhouse".to_string()],
            "the one thing carrying every key fits at limit {limit}, however far down it ranks"
        );
    }
}

/// 🚨 **A crowd of things that hold every key with a value of the wrong kind
/// cannot hide a thing that holds them all well.** The strict question's
/// clause selects on the keys a thing carries and cannot see what a key
/// holds, so a mistyped thing passes it, takes a place in the depth cut, and
/// is dropped afterwards by the whole-match filter. A crowd wider than the cut
/// then leaves an empty answer while a well-typed thing exists.
///
/// The crowd here carries all three keys with `crates` holding words, and is
/// wider than the depth a limit of one buys. The finished thing carries them
/// all well, beside a great many others, which ranks it below every one of
/// them. The positive the strict question rests on is that thing coming back;
/// the paired negative is that none of the crowd does.
#[tokio::test]
async fn a_strict_type_query_reaches_a_well_typed_thing_behind_a_crowd_of_mistyped_ones() {
    use jojobot_domain::memory::types::{Field as Key, ValueType};
    let declared = DeclaredType::new(
        "delivery",
        vec![
            Key::required("arrives", ValueType::Date),
            Key::required("crates", ValueType::Number),
            Key::required("driver", ValueType::Text),
        ],
    );
    let carrying = |handle: &str, keys: Vec<(String, String)>| {
        scan(
            &format!("doc-{handle}"),
            Some(entity(handle, "Somebody Else")),
            "",
            vec![Fact {
                fields: keys.into_iter().collect(),
                ..fact(
                    handle,
                    "f1",
                    "an ordinary claim, with keys on it",
                    date(2026, 1, 1),
                )
            }],
        )
    };
    let key = |k: &str, v: &str| (k.to_string(), v.to_string());

    let crowd = candidate_depth(1) * 3;
    let mut scans: Vec<DocScan> = (0..crowd)
        .map(|n| {
            carrying(
                &format!("person:{}", "p".repeat(n + 1)),
                vec![
                    key("arrives", "2026-08-11"),
                    key("crates", "a great many"),
                    key("driver", "beta"),
                ],
            )
        })
        .collect();
    let mut whole = vec![
        key("arrives", "2026-08-10"),
        key("crates", "4"),
        key("driver", "beta"),
    ];
    whole.extend((0..300).map(|n| key(&format!("other-{n}"), "x")));
    scans.push(carrying("person:milhouse", whole));
    let index = index_of(scans);

    let ids = |query: SearchQuery| -> Vec<String> {
        index
            .search(&query)
            .expect("search ok")
            .iter()
            .filter_map(|h| match h {
                Hit::Entity { entity, .. } => Some(entity.id.to_string()),
                _ => None,
            })
            .collect()
    };

    // The tolerant question keeps the whole crowd, so the crowd really is in
    // the corpus and really is wider than the cut.
    assert_eq!(
        ids(SearchQuery {
            answers_type: Some(declared.clone()),
            limit: 500,
            ..Default::default()
        })
        .len(),
        crowd + 1,
    );
    for limit in [1, 5] {
        assert_eq!(
            ids(SearchQuery {
                fits_type: Some(declared.clone()),
                limit,
                ..Default::default()
            }),
            vec!["person:milhouse".to_string()],
            "the one thing that fits is found at limit {limit}, and none of the mistyped crowd is"
        );
    }
}

/// **A pinned hit is a hit, so the type filter governs it too.**
///
/// An exact naming of an entity is prepended to the answer, and that path
/// consults the text alone. Left ungoverned it defeats the whole strict
/// question: a caller asking *which of these ARE deliveries* gets back a
/// thing that is not one, and gets it with no answer attached, so the gap
/// is not even reported.
///
/// Three things all answer to the same name here and only one fits, so
/// every read below asserts both halves at once — the one that fits is
/// there, the ones that do not are gone. A bare absence would pass over an
/// empty answer.
#[tokio::test]
async fn a_type_filter_governs_the_pinned_hits_too() {
    use jojobot_domain::memory::types::{Field as Key, ValueType};
    let delivery = DeclaredType::new(
        "delivery",
        vec![
            Key::required("arrives", ValueType::Date),
            Key::required("crates", ValueType::Number),
        ],
    );

    let carrying = |home: &str, keys: &[(&str, &str)]| Fact {
        fields: keys
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..fact(
            home,
            "f1",
            "an ordinary claim, with keys on it",
            date(2026, 1, 1),
        )
    };
    // Every one of them answers to "Beta", so every one of them pins.
    let answers_to_beta = |id: &str, name: &str| Entity {
        aliases: vec!["Beta".into()],
        ..entity(id, name)
    };
    let index = index_of(vec![
        scan(
            "doc-milhouse",
            Some(answers_to_beta("person:milhouse", "Milhouse")),
            "",
            vec![carrying(
                "person:milhouse",
                &[("arrives", "2026-08-10"), ("crates", "4")],
            )],
        ),
        scan(
            "doc-beta",
            Some(entity("person:beta", "Beta")),
            "",
            vec![carrying("person:beta", &[("arrives", "2026-08-11")])],
        ),
        scan(
            "doc-maude",
            Some(answers_to_beta("person:maude", "Maude")),
            "",
            vec![fact(
                "person:maude",
                "f1",
                "an ordinary claim, with no keys on it",
                date(2026, 1, 1),
            )],
        ),
    ]);

    let things = |query: SearchQuery| -> Vec<(String, Option<Vec<String>>)> {
        let mut found: Vec<(String, Option<Vec<String>>)> = index
            .search(&query)
            .expect("search ok")
            .iter()
            .filter_map(|hit| match hit {
                Hit::Entity {
                    entity, answers, ..
                } => Some((
                    entity.id.to_string(),
                    answers.as_deref().map(|m| m.lacking.clone()),
                )),
                _ => None,
            })
            .collect();
        found.sort();
        found
    };

    assert_eq!(
        things(SearchQuery {
            fits_type: Some(delivery.clone()),
            ..SearchQuery::text("Beta")
        }),
        vec![("person:milhouse".to_string(), Some(Vec::new()))],
        "the strict question keeps only the thing carrying every key, \
             however it reached the answer",
    );

    assert_eq!(
        things(SearchQuery {
            answers_type: Some(delivery.clone()),
            ..SearchQuery::text("Beta")
        }),
        vec![
            ("person:beta".to_string(), Some(vec!["crates".to_string()])),
            ("person:milhouse".to_string(), Some(Vec::new())),
        ],
        "the tolerant question keeps the partial thing ONCE, saying what it \
             lacks, drops the thing carrying no key at all, and keeps the whole one",
    );
}

/// **Deduped by identity, not by the whole hit.** The same claim can be
/// sitting in two docs at once — a correction has landed and one doc's
/// re-index has not caught up with it yet — and both answer the same
/// query until it does. `identity` keys a fact on its address, never its
/// content, so two copies of one claim differing only in what they say
/// are one fact and must be one hit. A dedupe comparing whole hits would
/// see two different bodies and keep both — the stale one included.
#[tokio::test]
async fn one_fact_scanned_into_two_docs_answers_once() {
    let stale = fact(
        "person:milhouse",
        "f1",
        "the couch needs a leg fixed, quimby-forty",
        date(2026, 1, 1),
    );
    let fresh = fact(
        "person:milhouse",
        "f1",
        "the couch is fixed now, quimby-forty",
        date(2026, 1, 2),
    );
    let index = index_of(vec![
        scan("doc-a-stale", None, "", vec![stale]),
        scan("doc-b-fresh", None, "", vec![fresh]),
    ]);

    let found: Vec<String> = index
        .search(&SearchQuery::text("quimby-forty"))
        .expect("search ok")
        .iter()
        .filter_map(|hit| match hit {
            Hit::Fact { fact, .. } => Some(fact.address().to_string()),
            _ => None,
        })
        .collect();

    assert_eq!(
        found.len(),
        1,
        "one claim sitting in two docs must answer once, not once per doc: {found:?}",
    );
}

/// **The same shape, for an entity.** `identity` keys an entity on its
/// id, never its name or fields, so two copies of one entity's own doc
/// disagreeing about what it is called are one entity and must be one
/// hit — the same catching-up reindex the fact case covers, one layer up.
#[tokio::test]
async fn one_entity_scanned_into_two_docs_answers_once() {
    let index = index_of(vec![
        scan(
            "doc-a-stale",
            Some(entity("person:milhouse", "Milhouse, quimby-forty")),
            "",
            vec![],
        ),
        scan(
            "doc-b-fresh",
            Some(entity(
                "person:milhouse",
                "Milhouse Van Houten, quimby-forty",
            )),
            "",
            vec![],
        ),
    ]);

    let found: Vec<String> = index
        .search(&SearchQuery::text("quimby-forty"))
        .expect("search ok")
        .iter()
        .filter_map(|hit| match hit {
            Hit::Entity { entity, .. } => Some(entity.id.to_string()),
            _ => None,
        })
        .collect();

    assert_eq!(
        found.len(),
        1,
        "one entity sitting in two docs must answer once, not once per doc: {found:?}",
    );
}

/// **The same shape, for prose.** `identity` keys a prose hit on its
/// `doc_id` alone — the one thing that has to name the SAME page for two
/// writes to collide at all, unlike a fact or an entity, which carry
/// their own address independent of which doc they arrived through. Two
/// writes under one doc id, with different bodies, are one page's prose
/// and must be one hit.
#[tokio::test]
async fn one_pages_prose_scanned_into_two_docs_answers_once() {
    let index = index_of(vec![
        scan(
            "doc-milhouse",
            None,
            "the couch needs a leg fixed, quimby-forty",
            vec![],
        ),
        scan(
            "doc-milhouse",
            None,
            "the couch is fixed now, quimby-forty",
            vec![],
        ),
    ]);

    let found: Vec<String> = index
        .search(&SearchQuery::text("quimby-forty"))
        .expect("search ok")
        .iter()
        .filter_map(|hit| match hit {
            Hit::Prose { doc_id, .. } => Some(doc_id.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(
        found.len(),
        1,
        "one page's prose written twice must answer once, not once per write: {found:?}",
    );
}

/// **The same shape, for mail.** `identity` keys a message hit on its
/// mailbox and id, never its body — a message a board read handed over
/// twice, once before and once after an edit, is one message and must be
/// one hit.
#[tokio::test]
async fn one_message_scanned_into_two_writes_answers_once() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_mail(&[
            message(
                "42",
                "inbox",
                "milhouse",
                None,
                "the shipment is short a crate, quimby-forty",
                MessageState::New,
            ),
            message(
                "42",
                "inbox",
                "milhouse",
                None,
                "the shipment arrived complete after all, quimby-forty",
                MessageState::Read,
            ),
        ])
        .expect("ingest mail");

    let found: Vec<String> = index
        .search(&asking_for_mail("quimby-forty"))
        .expect("search ok")
        .iter()
        .filter_map(|hit| match hit {
            Hit::Message { message, .. } => Some(format!("{}/{}", message.mailbox, message.id)),
            _ => None,
        })
        .collect();

    assert_eq!(
        found.len(),
        1,
        "one message written twice must answer once, not once per write: {found:?}",
    );
}

/// **The same shape, for a session's own beat.** A run's entries are
/// diffed against the mirror per beat id, one run at a time — so two
/// overlapping reads of ONE run in a single ingest each diff against the
/// same empty mirror and each re-add every beat they hold, including one
/// the other write already added. This is the real shape an append-only
/// chronology takes: the run gains a beat, and a catching-up reindex
/// writes the new beat while the old one's own text is untouched, so its
/// snippet is identical in both writes — `identity` keys a session hit on
/// (bot, run, snippet), and the older beat is one hit, however many
/// writes it survived unchanged through.
#[tokio::test]
async fn one_beat_scanned_into_two_writes_of_a_run_answers_once() {
    let stale = run(
        "run-1",
        "bot:milhouse",
        "chasing a bug",
        "the couch needs a leg fixed, quimby-forty",
    );
    let mut fresh = stale.clone();
    // **`focus` moves on; the old beat's own text does not.** Giving the
    // two writes a different focus is what makes the identity arm worth
    // sabotaging: swap it for `working_on` (which carries `focus`) and
    // the two writes of the SAME beat stop matching, because that field
    // is the one thing that genuinely differs between them.
    fresh.focus = "found it, wrapping up".into();
    fresh.entries.push(jojobot_domain::session::JournalEntry {
        id: jojobot_domain::session::EntryId("e2".into()),
        at: jiff::Timestamp::from_second(1_780_000_100).expect("a fixed instant"),
        text: "found it".into(),
        touched: None,
        beat: None,
        on: None,
        closing_focus: None,
        closing: false,
    });

    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[stale, fresh])
        .expect("sessions ingested");

    let found: Vec<String> = index
        .search(&SearchQuery {
            text: Some("quimby-forty".into()),
            asked_by: Some(EntityId("bot:milhouse".into())),
            ..SearchQuery::default()
        })
        .expect("search ok")
        .iter()
        .filter_map(|hit| match hit {
            Hit::Session { session, .. } => Some(session.0.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(
        found.len(),
        1,
        "one beat written twice by two overlapping reads of one run must answer once: \
             {found:?}",
    );
}

/// The limit is honoured, and defaults to twenty.
#[tokio::test]
async fn the_limit_caps_the_list_and_defaults_to_twenty() {
    let facts: Vec<Fact> = (1..=30)
        .map(|n| {
            fact(
                "person:alpha",
                &format!("f{n}"),
                "repeated claim",
                date(2026, 1, 1),
            )
        })
        .collect();
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        facts,
    )]);
    assert_eq!(
        index
            .search(&SearchQuery::text("repeated"))
            .expect("search ok")
            .len(),
        DEFAULT_LIMIT
    );
    assert_eq!(
        index
            .search(&SearchQuery {
                limit: 3,
                ..SearchQuery::text("repeated")
            })
            .expect("search ok")
            .len(),
        3
    );
}

/// A rebuild replaces the projection wholesale: what the store no longer says
/// is no longer findable. A projection that accumulates is a second, wrong
/// source of truth.
#[tokio::test]
async fn a_rebuild_drops_what_the_store_no_longer_says() {
    let index = FullTextIndex::open().expect("index opens");
    let before = scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact(
            "person:alpha",
            "f1",
            "keeps a ferret",
            date(2026, 1, 1),
        )],
    );
    index
        .ingest_all(&[before], index.reading_begins(), &Default::default())
        .expect("ingest");
    assert_eq!(
        index
            .search(&SearchQuery::text("ferret"))
            .expect("search ok")
            .len(),
        1
    );

    let after = scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact(
            "person:alpha",
            "f1",
            "keeps a tortoise",
            date(2026, 1, 1),
        )],
    );
    index
        .ingest_all(&[after], index.reading_begins(), &Default::default())
        .expect("re-ingest");
    assert!(
        index
            .search(&SearchQuery::text("ferret"))
            .expect("search ok")
            .is_empty(),
        "the old row must be gone from the index, not left beside the new one"
    );
    assert_eq!(
        index
            .search(&SearchQuery::text("tortoise"))
            .expect("search ok")
            .len(),
        1
    );
}

/// An edited fact is re-indexed in place: one hit, saying the new thing. The
/// incremental path re-reads the doc, so a stale copy can't survive an edit.
#[tokio::test]
async fn an_edit_leaves_one_indexed_copy_saying_the_new_thing() {
    let store =
        Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens"));
    store
        .add_entity(NewEntity::new(
            EntityId::person("person:alpha"),
            "Alpha",
            "user-named",
        ))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    let captured = store
        .capture(NewFact::about(
            EntityId::person("person:alpha"),
            "works at the old place",
            date(2026, 7, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    store
        .update_fact(
            &captured.address(),
            FactPatch {
                content: Some("works at the new place".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("update ok")
        .written()
        .expect("not blocked");

    assert!(
        store
            // **One distinctive word, not a phrase.** Matching is
            // approximate now, so "old place" would match the NEW text on
            // `place` alone and this would stop measuring a stale copy.
            .search_via_port(&SearchQuery::text("old"))
            .await
            .expect("search ok")
            .is_empty(),
        "the superseded text must be gone from the index"
    );
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("new place"))
            .await
            .expect("search ok")
            .len(),
        1
    );
}

/// A blocked write indexes nothing — the guard said nothing was written, and
/// the projection has to agree.
#[tokio::test]
async fn a_blocked_write_indexes_nothing() {
    let store =
        Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens"));
    store
        .add_entity(NewEntity::new(
            EntityId::person("person:zenith"),
            "Zenith",
            "user-named",
        ))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");

    let blocked = store
        .capture(NewFact::about(
            EntityId::person("person:zenit"),
            "should not be indexed",
            date(2026, 7, 1),
        ))
        .await
        .expect("call ok");
    assert!(matches!(blocked, Guarded::Blocked { .. }));
    assert!(
        store
            .search_via_port(&SearchQuery::text("should not be indexed"))
            .await
            .expect("search ok")
            .is_empty(),
        "a blocked capture must leave nothing in the index either"
    );
}

/// The boot path: a store already holding docs is indexed by one full re-scan.
#[tokio::test]
async fn rebuild_indexes_a_store_that_was_already_full() {
    let inner = Arc::new(InMemoryMemory::booted());
    inner
        .add_entity(NewEntity::new(
            EntityId::person("person:alpha"),
            "Alpha",
            "user-named",
        ))
        .await
        .expect("add ok");
    inner
        .capture(NewFact::about(
            EntityId::person("person:alpha"),
            "was here before the server started",
            date(2026, 7, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let store = Arc::new(IndexedMemory::new(inner).expect("index opens"));
    // An index nobody has filled yet still answers from the store, because
    // the read takes its own scan. "Nothing is searchable until the boot
    // scan runs" is a statement about the projection rather than about what
    // the store holds.
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("before"))
            .await
            .expect("search ok")
            .len(),
        1,
        "the store holds it, so a read finds it with no boot scan"
    );
    assert_eq!(
        store.rebuild().await.expect("rebuild"),
        1,
        "one doc scanned"
    );
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("before"))
            .await
            .expect("search ok")
            .len(),
        1
    );
}

/// **A claim taken back before this server started is not served by it.**
///
/// The shared contract drives both states against a live index, so what is
/// left to ask is the path it does not travel: a record marked in the store
/// and read back by the BOOT SCAN, rather than by the refresh that follows a
/// write. Taking something back is the one move whose whole purpose is that
/// it stops being served, so the state has to survive the journey out to the
/// store and back rather than only the write that set it.
///
/// Both roads to archived are driven here, because they are set by
/// different verbs: `retract` marks an event, and an ordinary edit is
/// what moves a fact there directly.
#[tokio::test]
async fn a_claim_taken_back_before_the_scan_is_not_served_after_it() {
    let inner = Arc::new(InMemoryMemory::booted());
    inner
        .add_entity(NewEntity::new(
            EntityId::person("person:alpha"),
            "Alpha",
            "user-named",
        ))
        .await
        .expect("add ok");
    let stands = inner
        .capture(NewFact::about(
            EntityId::person("person:alpha"),
            "the quartet rehearsed",
            date(2026, 7, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    let taken_back = inner
        .capture(NewFact::about(
            EntityId::person("person:alpha"),
            "the quartet rehearsed twice",
            date(2026, 7, 2),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    inner
        .retract(
            &taken_back.address(),
            Some("it never happened"),
            date(2026, 7, 3),
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("retract ok");
    // The card's claim names both states, so both are driven: they leave a
    // row standing for different reasons and are marked by different verbs.
    let moved_past = inner
        .capture(NewFact::about(
            EntityId::person("person:alpha"),
            "the quartet rehearsed on Tuesdays",
            date(2026, 7, 4),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    inner
        .update_fact(
            &moved_past.address(),
            FactPatch {
                status: Some(FactStatus::Archived),
                ..Default::default()
            },
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("edit ok");

    let store = Arc::new(IndexedMemory::new(inner).expect("index opens"));
    store.rebuild().await.expect("rebuild");

    let seen: Vec<String> = store
        .search_via_port(&SearchQuery::text("rehearsed"))
        .await
        .expect("search ok")
        .iter()
        .filter_map(|h| match h {
            Hit::Fact { fact, .. } => Some(fact.address().to_string()),
            _ => None,
        })
        .collect();
    // The positive first: "the retracted one is absent" passes identically
    // on a scan that indexed nothing at all.
    assert!(
        seen.contains(&stands.address().to_string()),
        "the claim that still stands is served: {seen:?}"
    );
    assert!(
        !seen.contains(&taken_back.address().to_string()),
        "the claim that was taken back is not: {seen:?}"
    );
    assert!(
        !seen.contains(&moved_past.address().to_string()),
        "and neither is the claim the record has moved past: {seen:?}"
    );
}

/// Two documents, one of which is about to be removed from the store
/// behind jojobot's back. Two rather than one so that every assertion
/// below has a survivor to pair with.
fn a_store_of_two() -> Arc<Scanned> {
    Scanned::new(vec![
        DocScan {
            doc_id: Scanned::DOC_ID.into(),
            title: "Alpha".into(),
            prose: "Alpha is allergic to penicillin.".into(),
            entity: Some(entity("person:alpha", "Alpha")),
            facts: vec![fact(
                "person:alpha",
                "f1",
                "keeps a ferret",
                date(2026, 1, 1),
            )],
            fields: Default::default(),
            owner: None,
        },
        DocScan {
            doc_id: "outline-uuid-9b2c".into(),
            title: "Beta".into(),
            prose: "Beta plays the sousaphone.".into(),
            entity: Some(entity("person:beta", "Beta")),
            facts: vec![fact("person:beta", "f1", "keeps a gecko", date(2026, 1, 1))],
            fields: Default::default(),
            owner: None,
        },
    ])
}

/// **A record the store lost stops being served, with no write to prompt
/// it.** The removal happens outside jojobot — rule 60 makes true deletion
/// a human act — so nothing calls a refresh, and the read path is the only
/// place left that can notice.
///
/// Every assertion is a pair: the survivor is asserted present in the same
/// read that asserts the removed one absent, because "the stale record is
/// gone" passes identically against a search wired to nothing.
#[tokio::test]
async fn a_record_removed_from_the_store_stops_being_served() {
    let inner = a_store_of_two();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    for present in ["ferret", "penicillin", "gecko", "sousaphone"] {
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text(present))
                .await
                .unwrap()
                .len(),
            1,
            "{present:?} is served while both pages exist"
        );
    }

    inner.lose(Scanned::DOC_ID);

    for gone in ["ferret", "penicillin"] {
        assert!(
            store
                .search_via_port(&SearchQuery::text(gone))
                .await
                .unwrap()
                .is_empty(),
            "{gone:?} left the store, so search must stop serving it"
        );
    }
    for kept in ["gecko", "sousaphone"] {
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text(kept))
                .await
                .unwrap()
                .len(),
            1,
            "{kept:?} is still in the store and must still be served"
        );
    }
    assert!(
        store
            .search_via_port(&SearchQuery::text("person:alpha"))
            .await
            .unwrap()
            .is_empty(),
        "the entity goes with its page"
    );
    assert!(
        !store
            .search_via_port(&SearchQuery::text("person:beta"))
            .await
            .unwrap()
            .is_empty(),
        "…and the entity whose page remains does not"
    );
}

/// **The answer never claims to be complete while it is coming from a scan
/// that could not be taken.** Rule 130: a wrong answer is a defect, and a
/// wrong answer that vouches for itself is worse, because the caller has
/// nothing to act on.
///
/// The pair here is hits-and-coverage in one read. An implementation that
/// answers with nothing and reports itself partial satisfies the coverage
/// assertion alone, and it is a worse store than one that serves what it
/// can reach.
#[tokio::test]
async fn coverage_stops_claiming_loaded_when_the_read_cannot_reach_the_store() {
    let inner = a_store_of_two();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "a scan that just succeeded is complete coverage"
    );

    inner.blinded();

    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("ferret"))
            .await
            .unwrap()
            .len(),
        1,
        "degrade, don't error: the last good scan still answers"
    );
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "…and the answer says it is holding an older version than the store"
    );

    inner.sighted();

    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("ferret"))
            .await
            .unwrap()
            .len(),
        1,
        "the record is still there and still served"
    );
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "a scan that reaches the store again clears the mark"
    );
}

/// **A board that hands back the messages it was given, and can stop
/// answering** — [`Scanned`]'s opposite number for the mail half, hostile
/// about the one thing this half's coverage turns on.
///
/// It only scans. Every other verb on the port is unimplemented, because
/// what is under test here is what the refresh does with a board read, and
/// a double that could do more would invite a test that proves less.
struct Board {
    messages: RwLock<Vec<Message>>,
    /// The board stops answering a read, so the refresh behind an answer
    /// fails and the mail half is left a version behind.
    blind: std::sync::atomic::AtomicBool,
    /// **A board read that has taken its snapshot and not answered yet** —
    /// the window a real read has and this double otherwise does not, since
    /// it answers without ever suspending. See [`Scanned::park`].
    park: std::sync::atomic::AtomicBool,
    parked: std::sync::atomic::AtomicBool,
    /// **The boxes the board lists, when they are not the ones its messages
    /// sit in** — a rename that landed between the scan and the listing.
    listed: RwLock<Option<Vec<jojobot_domain::mailbox::Mailbox>>>,
}

impl Board {
    fn new(messages: Vec<Message>) -> Arc<Self> {
        Arc::new(Board {
            messages: RwLock::new(messages),
            blind: std::sync::atomic::AtomicBool::new(false),
            park: std::sync::atomic::AtomicBool::new(false),
            parked: std::sync::atomic::AtomicBool::new(false),
            listed: RwLock::new(None),
        })
    }

    /// From here the board lists exactly these boxes.
    fn lists_only(&self, boxes: Vec<jojobot_domain::mailbox::Mailbox>) {
        *self.listed.write().expect("listing poisoned") = Some(boxes);
    }

    fn blinded(&self) {
        self.blind.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn sighted(&self) {
        self.blind.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn hold_reads(&self) {
        self.park.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn read_is_holding(&self) -> bool {
        self.parked.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn release_reads(&self) {
        self.park.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl jojobot_domain::mailbox::Mailboxes for Board {
    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        if self.blind.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(MailboxError::Store("the board cannot be read".into()));
        }
        let snapshot = self.messages.read().expect("messages poisoned").clone();
        if self.park.load(std::sync::atomic::Ordering::SeqCst) {
            self.parked.store(true, std::sync::atomic::Ordering::SeqCst);
            while self.park.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            self.parked
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(snapshot)
    }

    async fn message_by_id(
        &self,
        _: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Option<Message>, MailboxError> {
        unimplemented!("this double only scans messages")
    }

    async fn sent_by(&self, _: &[&str]) -> Result<Vec<Message>, MailboxError> {
        unimplemented!("this double only scans messages")
    }

    async fn create_mailbox(
        &self,
        _: &MailboxName,
        _: &jojobot_domain::memory::EntityId,
        _: Option<&str>,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError>
    {
        unimplemented!("this double only scans messages")
    }

    async fn repoint_owner(
        &self,
        _: &jojobot_domain::memory::EntityId,
        _: &jojobot_domain::memory::EntityId,
    ) -> Result<Option<jojobot_domain::mailbox::Mailbox>, MailboxError> {
        unimplemented!("this double only scans messages")
    }

    /// **One box per name the messages sit in, each owned by the bot of that
    /// name**, and unreadable exactly when the scan is: the index asks which
    /// boxes are a person's before it indexes a board, so a double that could be
    /// scanned and not listed would answer where the store refuses.
    async fn list_mailboxes(&self) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
        if self.blind.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(MailboxError::Store("the board cannot be read".into()));
        }
        if let Some(listed) = self.listed.read().expect("listing poisoned").clone() {
            return Ok(listed);
        }
        let names: std::collections::BTreeSet<MailboxName> = self
            .messages
            .read()
            .expect("messages poisoned")
            .iter()
            .map(|message| message.mailbox.clone())
            .collect();
        Ok(names
            .into_iter()
            .map(|name| jojobot_domain::mailbox::Mailbox {
                owner: jojobot_domain::memory::EntityId(format!("bot:{}", name.as_str())),
                name,
                counts: Default::default(),
                quarantined: Vec::new(),
            })
            .collect())
    }

    async fn post_message(
        &self,
        _: jojobot_domain::mailbox::NewMessage,
    ) -> Result<jojobot_domain::mailbox::Guarded<Message>, MailboxError> {
        unimplemented!("this double only scans messages")
    }

    async fn read_mailbox(
        &self,
        _: &MailboxName,
        _: jojobot_domain::mailbox::TakenBy,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Delivery>, MailboxError>
    {
        unimplemented!("this double only scans messages")
    }

    async fn read_message(
        &self,
        _: &MessageId,
    ) -> Result<jojobot_domain::mailbox::Delivered, MailboxError> {
        unimplemented!("this double only scans messages")
    }

    async fn mark_processed(
        &self,
        _: &MessageId,
        _: Option<&str>,
    ) -> Result<Message, MailboxError> {
        unimplemented!("this double only scans messages")
    }

    async fn quarantine(
        &self,
        _: &MessageId,
        _: &MailboxName,
        _: &str,
        _: jiff::Timestamp,
    ) -> Result<jojobot_domain::mailbox::Quarantined, MailboxError> {
        unimplemented!("this double only scans messages")
    }
}

/// The mail half's twin of the corpus-failure race above. A board read
/// already in flight when another one fails must not clear that failure:
/// its own snapshot was taken before the board stopped answering.
#[tokio::test]
async fn a_board_read_in_flight_does_not_clear_a_failure_it_predates() {
    let board = Board::new(vec![message(
        "42",
        "pm",
        "dev",
        Some("the kiln slice"),
        "the damper is still hand-cut",
        MessageState::New,
    )]);
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let mail = Arc::new(IndexedMailboxes::new(board.clone(), index.clone()));
    mail.rebuild().await.expect("rebuild");
    assert_eq!(
        index.mail_coverage(),
        Coverage::Loaded,
        "a board read that just succeeded is complete coverage"
    );

    board.hold_reads();
    let refreshing = tokio::spawn({
        let mail = mail.clone();
        async move { Refresh::refresh(mail.as_ref()).await }
    });
    while !board.read_is_holding() {
        tokio::task::yield_now().await;
    }

    // Underneath it: another read cannot reach the board at all. It fails
    // on the blind check before it can park.
    board.blinded();
    Refresh::refresh(mail.as_ref()).await;
    assert_eq!(
        index.mail_coverage(),
        Coverage::Partial(Behind::Stale),
        "the index says the mail half is behind, which is what must survive"
    );

    board.sighted();
    board.release_reads();
    refreshing.await.expect("the refresh task");

    assert_eq!(
        index.mail_coverage(),
        Coverage::Partial(Behind::Stale),
        "a board read taken before the failure cannot vouch for the board after it"
    );
}

/// The other direction, and the positive both rest on: a read that began
/// after the failure clears it, and a run with nothing failed reports
/// complete. Without these, never clearing satisfies every negative.
#[tokio::test]
async fn a_board_read_that_began_after_a_failure_clears_it() {
    let board = Board::new(vec![message(
        "42",
        "pm",
        "dev",
        None,
        "the damper is still hand-cut",
        MessageState::New,
    )]);
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let mail = Arc::new(IndexedMailboxes::new(board.clone(), index.clone()));
    mail.rebuild().await.expect("rebuild");
    assert_eq!(
        index.mail_coverage(),
        Coverage::Loaded,
        "nothing has failed yet, so nothing is behind"
    );

    board.blinded();
    Refresh::refresh(mail.as_ref()).await;
    assert_eq!(
        index.mail_coverage(),
        Coverage::Partial(Behind::Stale),
        "a read that could not reach the board leaves the index behind"
    );

    board.sighted();
    Refresh::refresh(mail.as_ref()).await;
    assert_eq!(
        index.mail_coverage(),
        Coverage::Loaded,
        "a read taken after the failure speaks for the board as it stands"
    );
}

/// One run with one beat, owned by `bot` — the fixture the owner-scoping
/// cases are built from.
fn run(id: &str, bot: &str, focus: &str, beat: &str) -> jojobot_domain::session::Session {
    use jojobot_domain::session::{EntryId, JournalEntry, Session, SessionId, SessionState};
    Session {
        started_on: None,
        timezone: None,
        served_chars: 0,
        stated_day: None,
        wrap_window: None,
        id: SessionId(id.into()),
        sid: None,
        bot: EntityId(bot.into()),
        focus: focus.into(),
        started_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
        state: SessionState::Active,
        entries: vec![JournalEntry {
            id: EntryId("e1".into()),
            at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
            text: beat.into(),
            touched: None,
            beat: None,
            on: None,
            closing_focus: None,
            closing: false,
        }],
    }
}

/// A [`jojobot_domain::session::Sessions`] that answers a settable
/// `write_summary` and counts how many times the real `all_sessions` ran
/// — the session half's version of [`SummarizedScan`].
struct SummarizedSessions {
    sessions: RwLock<Vec<jojobot_domain::session::Session>>,
    summary: RwLock<Option<(i64, Option<jiff::Timestamp>)>>,
    reads: std::sync::atomic::AtomicUsize,
    /// The store stops answering `all_sessions`, so a refresh behind it
    /// fails and the sessions half is left serving what it last read —
    /// [`Board::blind`]'s own mechanism, for the third store.
    blind: std::sync::atomic::AtomicBool,
}

impl SummarizedSessions {
    fn new(sessions: Vec<jojobot_domain::session::Session>) -> Arc<Self> {
        Arc::new(SummarizedSessions {
            sessions: RwLock::new(sessions),
            summary: RwLock::new(None),
            reads: std::sync::atomic::AtomicUsize::new(0),
            blind: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// What `all_sessions` answers from here on — the runs the store would
    /// render after something outside the sessions store changed what they
    /// read as, such as their owner's rename.
    fn set_sessions(&self, sessions: Vec<jojobot_domain::session::Session>) {
        *self.sessions.write().expect("sessions poisoned") = sessions;
    }

    /// What `write_summary` answers from here on.
    fn set_summary(&self, summary: Option<(i64, Option<jiff::Timestamp>)>) {
        *self.summary.write().expect("summary poisoned") = summary;
    }

    /// How many times the real `all_sessions` ran.
    fn reads_ran(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn blinded(&self) {
        self.blind.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn sighted(&self) {
        self.blind.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl jojobot_domain::session::Sessions for SummarizedSessions {
    async fn sessions_of(
        &self,
        _: &EntityId,
    ) -> Result<Vec<jojobot_domain::session::Session>, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn all_sessions(
        &self,
    ) -> Result<Vec<jojobot_domain::session::Session>, jojobot_domain::session::SessionError> {
        if self.blind.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(jojobot_domain::session::SessionError::Store(
                "the sessions store cannot be read".into(),
            ));
        }
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.sessions.read().expect("sessions poisoned").clone())
    }
    async fn write_summary(
        &self,
    ) -> Result<Option<(i64, Option<jiff::Timestamp>)>, jojobot_domain::session::SessionError> {
        Ok(*self.summary.read().expect("summary poisoned"))
    }
    async fn read_session(
        &self,
        _: &jojobot_domain::session::SessionId,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn begin(
        &self,
        _: jojobot_domain::session::NewSession,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn append(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: jojobot_domain::session::NewEntry,
    ) -> Result<jojobot_domain::session::JournalEntry, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn amend_last(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: &str,
    ) -> Result<jojobot_domain::session::JournalEntry, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn amend_beat(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: &jojobot_domain::session::EntryId,
        _: &str,
        _: jiff::Timestamp,
    ) -> Result<jojobot_domain::session::JournalEntry, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn set_focus(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: &str,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn set_timezone(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: Option<&str>,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn set_stated_day(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: Option<jiff::civil::Date>,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn set_wrap_window(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: Option<jojobot_domain::session::WrapWindow>,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn close(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: jojobot_domain::session::SessionState,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn add_served(
        &self,
        _: &jojobot_domain::session::SessionId,
        _: u64,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
    async fn reopen(
        &self,
        _: &jojobot_domain::session::SessionId,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        unimplemented!("this double only answers all_sessions and write_summary")
    }
}

/// **The sessions half reports its own coverage** — [`Board`]'s own test
/// on `mail_coverage`, asked of the third store. A fresh index has read
/// nothing; a read that cannot reach the store leaves the last good one
/// standing and says so; a read taken after recovery clears it.
#[tokio::test]
async fn a_session_read_that_cannot_reach_the_store_leaves_the_index_behind() {
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    assert_eq!(
        index.session_coverage(),
        Coverage::Unread,
        "nothing has read the sessions store yet"
    );

    let spy = SummarizedSessions::new(vec![run(
        "s-gamma",
        "bot:gamma",
        "the kiln slice",
        "the damper is hand-cut",
    )]);
    let sessions = Arc::new(IndexedSessions::new(spy.clone(), index.clone()));
    sessions.rebuild().await.expect("rebuild");
    assert_eq!(
        index.session_coverage(),
        Coverage::Loaded,
        "nothing has failed yet, so nothing is behind"
    );

    spy.blinded();
    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(
        index.session_coverage(),
        Coverage::Partial(Behind::Stale),
        "a read that could not reach the store leaves the index behind"
    );

    spy.sighted();
    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(
        index.session_coverage(),
        Coverage::Loaded,
        "a read taken after the failure speaks for the store as it stands"
    );
}

/// **A reading already in flight does not clear a failure it predates** —
/// [`a_board_read_in_flight_does_not_clear_a_failure_it_predates`]'s own
/// case, for the third store. A reading's own snapshot only speaks for the
/// store as it stood when the reading began; a failure recorded after that
/// point happened to a store the reading never saw, so landing late must
/// not vouch for it.
#[tokio::test]
async fn a_session_read_in_flight_does_not_clear_a_failure_it_predates() {
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let sessions = vec![run(
        "s-gamma",
        "bot:gamma",
        "the kiln slice",
        "the damper is hand-cut",
    )];
    // Loaded once, so the failure below reaches `Stale` rather than
    // `Unread` — the same reason the coverage test above rebuilds first.
    index
        .ingest_sessions_changes(&[], index.reading_begins())
        .expect("the empty read still counts as having reached the store");

    // The reading begins — its point is taken here, before the failure.
    let began = index.reading_begins();
    // The failure lands while that reading is still in flight.
    index.session_refresh_failed();
    assert_eq!(
        index.session_coverage(),
        Coverage::Partial(Behind::Stale),
        "the failure is on record"
    );
    // The earlier reading lands now, carrying the point it took before
    // the failure — it was in flight when the store went down and never
    // saw that.
    index
        .ingest_sessions_changes(&sessions, began)
        .expect("the read that began earlier still writes what it saw");
    assert_eq!(
        index.session_coverage(),
        Coverage::Partial(Behind::Stale),
        "a reading that began before the failure must not clear it on landing late"
    );
}

/// 🚨 **A rename reaches the session index without a session write.** A run's
/// owner is rendered from the entity tree at read, so renaming the bot changes
/// what the runs say without changing the sessions store's own signal. A
/// refresh that trusted that signal alone answered from what it last read, and
/// the renamed bot found none of its runs until its next session write.
///
/// Both halves in one read: the bot finds its run under the handle it wears now
/// and no longer under the one it wore, and a store that did not change — no
/// rename in between — still costs no re-read.
#[tokio::test]
async fn a_rename_reaches_the_session_index_without_a_session_write() {
    use jojobot_domain::memory::{Memory, NewEntity};
    let memory = Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index"));
    let gamma = EntityId("bot:gamma".into());
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("the bot is added");
    let spy = SummarizedSessions::new(vec![run(
        "s-gamma",
        "bot:gamma",
        "the kiln slice",
        "the damper is hand-cut",
    )]);
    spy.set_summary(Some((1, None)));
    let sessions = Arc::new(IndexedSessions::new(spy.clone(), memory.index()));
    sessions.rebuild().await.expect("rebuild");

    let owners_for = |bot: &str| -> Vec<String> {
        memory
            .index()
            .search(&SearchQuery {
                text: Some("damper".into()),
                asked_by: Some(EntityId(bot.into())),
                ..SearchQuery::default()
            })
            .expect("search ok")
            .iter()
            .filter_map(|h| match h {
                Hit::Session { bot, .. } => Some(bot.to_string()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(owners_for("bot:gamma"), vec!["bot:gamma".to_string()]);

    // No rename yet: an unchanged store is still not read again.
    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(spy.reads_ran(), 1, "nothing changed, so nothing is re-read");

    // The rename, through the memory half. The sessions store's own signal does
    // not move, and what its runs read as does.
    memory
        .rename_entity(
            &gamma,
            &EntityId("bot:delta".into()),
            None,
            date(2026, 8, 1),
            None,
        )
        .await
        .expect("the rename lands");
    spy.set_sessions(vec![run(
        "s-gamma",
        "bot:delta",
        "the kiln slice",
        "the damper is hand-cut",
    )]);
    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(
        owners_for("bot:delta"),
        vec!["bot:delta".to_string()],
        "the renamed bot finds its run under the handle it wears now"
    );
    assert!(
        owners_for("bot:gamma").is_empty(),
        "and not under the one it wore"
    );

    // A merge changes what a folded bot's runs read as the same way.
    let otto = EntityId("bot:otto".into());
    memory
        .add_entity(NewEntity::new(otto.clone(), "Otto", "user-named"))
        .await
        .expect("the second bot is added");
    memory
        .merge(
            &EntityId("bot:delta".into()),
            &otto,
            Some("one bot, filed twice"),
            date(2026, 8, 2),
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("the merge lands");
    spy.set_sessions(vec![run(
        "s-gamma",
        "bot:otto",
        "the kiln slice",
        "the damper is hand-cut",
    )]);
    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(
        owners_for("bot:otto"),
        vec!["bot:otto".to_string()],
        "the survivor finds the run the folded bot wrote"
    );
}

/// **An unchanged session store costs no re-read.** The session half's
/// pair to [`an_unchanged_store_costs_no_re_read`] — the same mechanism,
/// this port's own signal.
#[tokio::test]
async fn an_unchanged_session_store_costs_no_re_read() {
    let spy = SummarizedSessions::new(vec![run(
        "s-gamma",
        "bot:gamma",
        "the kiln slice",
        "the damper is hand-cut",
    )]);
    spy.set_summary(Some((1, None)));
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let sessions = Arc::new(IndexedSessions::new(spy.clone(), index));
    sessions.rebuild().await.expect("rebuild");
    assert_eq!(spy.reads_ran(), 1, "the boot rebuild always reads once");

    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(
        spy.reads_ran(),
        1,
        "the signal did not move, so the refresh paid for no re-read"
    );
}

/// **The positive control.** The same shape, except the signal DOES move
/// between the two refreshes.
#[tokio::test]
async fn a_changed_session_store_is_still_seen() {
    let spy = SummarizedSessions::new(vec![run(
        "s-gamma",
        "bot:gamma",
        "the kiln slice",
        "the damper is hand-cut",
    )]);
    spy.set_summary(Some((1, None)));
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let sessions = Arc::new(IndexedSessions::new(spy.clone(), index));
    sessions.rebuild().await.expect("rebuild");
    assert_eq!(spy.reads_ran(), 1, "the boot rebuild always reads once");

    spy.set_summary(Some((2, None)));
    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(
        spy.reads_ran(),
        2,
        "the signal moved, so the refresh paid for a real re-read"
    );
}

/// **A session store with no signal is scanned every time** — the
/// fallback that keeps every store this slice does not touch exactly as
/// it was.
#[tokio::test]
async fn a_session_store_with_no_signal_is_scanned_every_refresh() {
    let spy = SummarizedSessions::new(vec![run(
        "s-gamma",
        "bot:gamma",
        "the kiln slice",
        "the damper is hand-cut",
    )]);
    // `set_summary` never called: the double's own default is `None`,
    // exactly the trait's default for a store that has not opted in.
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let sessions = Arc::new(IndexedSessions::new(spy.clone(), index));
    sessions.rebuild().await.expect("rebuild");
    assert_eq!(spy.reads_ran(), 1);

    Refresh::refresh(sessions.as_ref()).await;
    assert_eq!(
        spy.reads_ran(),
        2,
        "no signal means no cache to trust, so every refresh still reads"
    );
}

/// **A bot finds its own run, does not find another bot's, and the second
/// is distinguishable from an index that holds nothing.**
///
/// All three in one read, because an owner filter and an empty index look
/// identical to a caller: a bot searching its own history and getting
/// nothing reads it as "there is no such thing" rather than "that is not
/// yours". A case that only asserts the absence passes on a build that
/// indexes no session at all, which is the failure this one is written
/// against.
#[test]
fn a_bot_finds_its_own_run_and_not_another_bots() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[
            run(
                "s-gamma",
                "bot:gamma",
                "the kiln slice",
                "the damper is hand-cut",
            ),
            run(
                "s-otto",
                "bot:otto",
                "the kiln slice",
                "the damper is hand-cut",
            ),
        ])
        .expect("sessions ingested");

    let asking = |bot: &str| SearchQuery {
        text: Some("damper".into()),
        asked_by: Some(EntityId(bot.into())),
        ..SearchQuery::default()
    };

    let mine = index.search(&asking("bot:gamma")).expect("search ok");
    let owners: Vec<String> = mine
        .iter()
        .filter_map(|h| match h {
            Hit::Session { bot, .. } => Some(bot.to_string()),
            _ => None,
        })
        .collect();

    // The positive, first and in the same read: gamma's own beat comes back.
    assert!(
        owners.contains(&"bot:gamma".to_string()),
        "a bot must find its own run: {mine:?}"
    );
    // And the negative it makes meaningful: otto's does not, in the same
    // answer that just proved the index is not empty.
    assert!(
        !owners.contains(&"bot:otto".to_string()),
        "another bot's run is not this bot's to read: {mine:?}"
    );

    // …and the mirror image, so the filter is scoping rather than hiding
    // one fixture: otto asking finds otto's and not gamma's.
    let theirs = index.search(&asking("bot:otto")).expect("search ok");
    let theirs: Vec<String> = theirs
        .iter()
        .filter_map(|h| match h {
            Hit::Session { bot, .. } => Some(bot.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        theirs,
        vec!["bot:otto".to_string()],
        "the same query asked by the other bot returns that bot's run and only it"
    );
}

/// **A refresh that finds the runs unchanged rewrites nothing.**
///
/// The session half is refreshed before every answer, so an ingest that
/// rewrote the whole half regardless would take the writer lock, commit the
/// shared index and reload the reader once per search — work that grows
/// with every run every bot has ever had rather than with what changed.
/// It is the rule the memory half states forty lines away and applies.
///
/// Paired with the positives it rests on: the first ingest writes, a
/// changed run writes again, and a run the store no longer holds is
/// evicted. Without those, a build that ingested nothing at all would
/// satisfy the zero.
#[test]
fn a_session_ingest_that_changes_nothing_writes_nothing() {
    let index = FullTextIndex::open().expect("index opens");
    let corpus = |beat: &str| {
        vec![
            run("s-gamma", "bot:gamma", "the kiln slice", beat),
            run(
                "s-otto",
                "bot:otto",
                "the kiln slice",
                "the damper is hand-cut",
            ),
        ]
    };

    assert_eq!(
        index
            .ingest_sessions(&corpus("the damper is hand-cut"))
            .expect("sessions ingested"),
        2,
        "the first reading has both runs to write",
    );
    assert_eq!(
        index
            .ingest_sessions(&corpus("the damper is hand-cut"))
            .expect("sessions ingested"),
        0,
        "the same runs again are already held, so nothing is rewritten",
    );
    assert_eq!(
        index
            .ingest_sessions(&corpus("the damper is cut by hand"))
            .expect("sessions ingested"),
        1,
        "the run whose beat moved is rewritten, and only it",
    );

    // …and the run the store no longer holds goes with it, or a session
    // deleted anywhere would be served from this index for ever.
    let remaining = vec![run(
        "s-otto",
        "bot:otto",
        "the kiln slice",
        "the damper is hand-cut",
    )];
    assert_eq!(
        index
            .ingest_sessions(&remaining)
            .expect("sessions ingested"),
        1,
        "the run that left the store is evicted",
    );
    let gone = index
        .search(&SearchQuery {
            text: Some("damper".into()),
            asked_by: Some(EntityId("bot:gamma".into())),
            ..SearchQuery::default()
        })
        .expect("search ok");
    assert!(
        !gone.iter().any(|h| matches!(h, Hit::Session { .. })),
        "an evicted run is no longer served: {gone:?}",
    );
}

/// A run with three named beats, for the per-entry eviction cases below.
fn run_of(id: &str, bot: &str, beats: &[(&str, &str)]) -> jojobot_domain::session::Session {
    use jojobot_domain::session::{EntryId, JournalEntry, Session, SessionId, SessionState};
    Session {
        started_on: None,
        timezone: None,
        served_chars: 0,
        stated_day: None,
        wrap_window: None,
        id: SessionId(id.into()),
        sid: None,
        bot: EntityId(bot.into()),
        focus: "the kiln slice".into(),
        started_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
        state: SessionState::Active,
        entries: beats
            .iter()
            .map(|(id, text)| JournalEntry {
                id: EntryId((*id).into()),
                at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                text: (*text).into(),
                touched: None,
                beat: None,
                on: None,
                closing_focus: None,
                closing: false,
            })
            .collect(),
    }
}

fn finds(index: &FullTextIndex, bot: &str, needle: &str) -> bool {
    index
        .search(&SearchQuery {
            text: Some(needle.into()),
            asked_by: Some(EntityId(bot.into())),
            ..SearchQuery::default()
        })
        .expect("search ok")
        .iter()
        .any(|h| matches!(h, Hit::Session { .. }))
}

/// **A new beat writes one document and touches none of its siblings.**
///
/// Paired with a positive control: the same instrument, run over a run
/// evicted outright, moves by that run's whole entry count rather than by
/// one — proving the counter distinguishes the two rather than reading the
/// same number regardless of what happened.
#[test]
fn appending_one_beat_writes_only_that_beat_and_leaves_its_siblings_alone() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[run_of(
            "s-gamma",
            "bot:gamma",
            &[
                ("e1", "the damper is hand-cut"),
                ("e2", "the flue draws clean"),
            ],
        )])
        .expect("sessions ingested");
    let written_before = index.session_entries_written();
    let deleted_before = index.session_entries_deleted();

    assert_eq!(
        index
            .ingest_sessions(&[run_of(
                "s-gamma",
                "bot:gamma",
                &[
                    ("e1", "the damper is hand-cut"),
                    ("e2", "the flue draws clean"),
                    ("e3", "the glaze is mixed"),
                ],
            )])
            .expect("sessions ingested"),
        1,
        "one run changed, so one run is reported rewritten",
    );

    assert_eq!(
        index.session_entries_written() - written_before,
        1,
        "only the new beat's own document should be written"
    );
    assert_eq!(
        index.session_entries_deleted() - deleted_before,
        0,
        "nothing existing changed or left, so nothing should be deleted"
    );
    assert!(
        finds(&index, "bot:gamma", "damper") && finds(&index, "bot:gamma", "flue"),
        "the untouched siblings must still be findable"
    );
    assert!(
        finds(&index, "bot:gamma", "glaze"),
        "the new beat must be findable"
    );
}

/// **Amending the newest beat replaces only its own document.**
///
/// The deleted count is the assertion that matters: two, not one, would
/// mean a sibling was rewritten it did not have to be; three would mean
/// the old whole-run wipe survived under a different name.
#[test]
fn amending_a_beat_replaces_only_its_own_document_and_leaves_its_siblings_alone() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[run_of(
            "s-gamma",
            "bot:gamma",
            &[
                ("e1", "the damper is hand-cut"),
                ("e2", "the flue draws clean"),
                ("e3", "the glaze is mixed"),
            ],
        )])
        .expect("sessions ingested");
    let written_before = index.session_entries_written();
    let deleted_before = index.session_entries_deleted();

    assert_eq!(
        index
            .ingest_sessions(&[run_of(
                "s-gamma",
                "bot:gamma",
                &[
                    ("e1", "the damper is hand-cut"),
                    ("e2", "the flue draws hot"),
                    ("e3", "the glaze is mixed"),
                ],
            )])
            .expect("sessions ingested"),
        1,
        "one run changed, so one run is reported rewritten",
    );

    assert_eq!(
        index.session_entries_written() - written_before,
        1,
        "only the amended beat's replacement document should be written"
    );
    assert_eq!(
        index.session_entries_deleted() - deleted_before,
        1,
        "only the amended beat's stale document should be deleted"
    );
    assert!(
        !finds(&index, "bot:gamma", "clean"),
        "the beat's superseded wording must not still be found"
    );
    assert!(
        finds(&index, "bot:gamma", "hot"),
        "the beat's new wording must be found"
    );
    assert!(
        finds(&index, "bot:gamma", "damper") && finds(&index, "bot:gamma", "glaze"),
        "the untouched siblings must still be findable"
    );
}

/// **The positive control: a run evicted outright deletes every entry it
/// had**, moving the same counter the two cases above move by exactly one.
/// A counter that never moved would have passed both cases above for the
/// wrong reason.
#[test]
fn evicting_a_run_deletes_every_entry_it_held() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[run_of(
            "s-gamma",
            "bot:gamma",
            &[
                ("e1", "the damper is hand-cut"),
                ("e2", "the flue draws clean"),
                ("e3", "the glaze is mixed"),
            ],
        )])
        .expect("sessions ingested");
    let deleted_before = index.session_entries_deleted();

    assert_eq!(
        index.ingest_sessions(&[]).expect("sessions ingested"),
        1,
        "the one run the store no longer holds is evicted",
    );
    assert_eq!(
        index.session_entries_deleted() - deleted_before,
        3,
        "an evicted run must account for every entry it held, not just one"
    );
}

/// **A session is reachable, and it loses to anything else that matched.**
///
/// Both halves, because each alone is satisfiable by the wrong build. A
/// rank so low the hit never surfaces satisfies "lower priority" and
/// defeats the ruling; a session that outranks a fact buries what a search
/// is usually for. So: a query only the session matches returns it, and a
/// query both match puts the other one first.
#[test]
fn a_session_is_reachable_and_ranks_below_what_it_shares_an_answer_with() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_all(
            &[DocScan {
                doc_id: "outline-uuid-1".into(),
                title: "Alpha".into(),
                prose: "the kiln was relined last spring".into(),
                entity: Some(entity("person:alpha", "Alpha")),
                facts: Vec::new(),
                fields: Default::default(),
                owner: None,
            }],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("memory ingested");
    index
        .ingest_sessions(&[run(
            "s-gamma",
            "bot:gamma",
            "the kiln slice",
            // Deliberately a far stronger match on the shared term than the
            // entity is: without the demotion this session outranks it, so
            // the ordering below is the demotion's doing and not the
            // scorer's.
            // Deliberately a far stronger match on the shared term than the
            // prose is, and on a term that pins nothing: without the
            // demotion this session outranks it, so the ordering below is
            // the demotion's doing rather than the scorer's or the pin's.
            "kiln kiln kiln kiln the damper is hand-cut",
        )])
        .expect("sessions ingested");

    let asking = |text: &str| SearchQuery {
        text: Some(text.into()),
        asked_by: Some(EntityId("bot:gamma".into())),
        ..SearchQuery::default()
    };

    // Reachable: nothing else in the corpus says "damper".
    let alone = index.search(&asking("damper")).expect("search ok");
    assert!(
        alone.iter().any(|h| matches!(h, Hit::Session { .. })),
        "a session must be findable when it is what you are looking for: {alone:?}"
    );

    // …and demoted: both match "Alpha", and the entity comes first.
    let shared = index.search(&asking("kiln")).expect("search ok");
    let first_session = shared
        .iter()
        .position(|h| matches!(h, Hit::Session { .. }))
        .expect("the session matched this query too, or the ordering below proves nothing");
    let first_other = shared
        .iter()
        .position(|h| !matches!(h, Hit::Session { .. }))
        .expect("something other than a session matched, or there is nothing to rank against");
    assert!(
        first_other < first_session,
        "a session is context, not an answer, and ranks below what it shares a list with: \
             {shared:?}"
    );
}

/// 🚨 **A singular question finds a plural claim — and a handle still
/// resolves exactly.**
///
/// Measured against a real index: `Tuesdays` returned one hit and `Tuesday`
/// returned none. **One letter, not a synonym and not a paraphrase.** ⛔️
/// **And the consequence is the fabrication class through a door nobody
/// watched:** an empty answer is indistinguishable from never having been
/// told, so an assistant says *I have nothing on that* — honest-sounding,
/// and wrong over a plural.
///
/// ⚠️ **BOTH HALVES IN ONE CASE, deliberately.** Handles are `kind:slug`
/// and everything about matching touches them too. **A case proving only
/// the new recall passes against a build whose handles are broken**, which
/// is a far worse regression than the miss it fixes.
#[test]
fn a_singular_query_finds_a_stored_plural_and_a_handle_still_resolves() {
    let index = FullTextIndex::open().expect("index opens");
    let day = "2026-08-10".parse().expect("a civil date");
    index
        .ingest_all(
            &[DocScan {
                doc_id: "outline-uuid-1".into(),
                title: "Milhouse".into(),
                prose: String::new(),
                entity: Some(entity("person:milhouse", "Milhouse")),
                facts: vec![fact(
                    "person:milhouse",
                    "f1",
                    "the committee meets on Tuesdays",
                    day,
                )],
                fields: Default::default(),
                owner: None,
            }],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("memory ingested");

    let hits = |text: &str| {
        index
            .search(&SearchQuery {
                text: Some(text.into()),
                ..SearchQuery::default()
            })
            .expect("search ok")
    };

    // **The plural, asked in the singular.**
    assert!(
        hits("Tuesday")
            .iter()
            .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.id.0 == "f1")),
        "a singular question missed a plural claim: {:?}",
        hits("Tuesday"),
    );
    // **And the exact form still works** — otherwise this passes against a
    // build that matches everything to everything.
    assert!(
        hits("Tuesdays")
            .iter()
            .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.id.0 == "f1")),
        "the stored wording stopped finding itself",
    );

    // ⛔️ **The handle, exactly.** It must still name its entity.
    assert!(
        hits("person:milhouse")
            .iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id.0 == "person:milhouse")),
        "an exact handle stopped resolving: {:?}",
        hits("person:milhouse"),
    );
    // And a handle that names nothing still finds nothing under it — a
    // relaxed matcher that answers every handle with some entity is worse
    // than one that answers none.
    assert!(
        !hits("person:ralph")
            .iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id.0 == "person:milhouse")),
        "a handle nobody holds resolved to somebody else's entity",
    );
}

/// 🚨 **A question whose words live in two different records finds both.**
///
/// This is the shape a real sitting takes: somebody asks about an earlier
/// claim in words that claim did not use, and the terms end up spread
/// across records rather than gathered in one. ⛔️ **Every term being
/// required meant the answer was empty**, and an empty answer reads as
/// *nobody ever said this*.
///
/// ⚠️ **Measured rather than inferred.** The majority rule SHOULD make this
/// work, and *should* is what an expensive run is for finding out is wrong.
#[test]
fn a_question_spread_across_two_records_finds_them_both() {
    let index = FullTextIndex::open().expect("index opens");
    let day = "2026-08-10".parse().expect("a civil date");
    index
        .ingest_all(
            &[DocScan {
                doc_id: "outline-uuid-1".into(),
                title: "Milhouse".into(),
                prose: String::new(),
                entity: Some(entity("person:milhouse", "Milhouse")),
                facts: vec![
                    fact("person:milhouse", "f1", "the club meets upstairs", day),
                    fact("person:milhouse", "f2", "rehearsals are on Tuesdays", day),
                ],
                fields: Default::default(),
                owner: None,
            }],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("memory ingested");

    // Neither record holds both words, and the singular is in neither.
    let found = index
        .search(&SearchQuery {
            text: Some("club Tuesday".into()),
            ..SearchQuery::default()
        })
        .expect("search ok");
    let ids: Vec<&str> = found
        .iter()
        .filter_map(|h| match h {
            Hit::Fact { fact, .. } => Some(fact.id.0.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        ids.contains(&"f1") && ids.contains(&"f2"),
        "a question spread across two records found {ids:?} instead of both",
    );
}

/// 🚨 **A summary never outranks the words it summarises.**
///
/// Measured in a paid run, not argued: an assistant filed the operator's
/// own message and wrote a gloss of it. **The gloss came back complete and
/// the words themselves came back truncated**, and on a later query the
/// gloss ranked first — five months after it had stopped being true.
///
/// ⛔️ **Not deletion and not hiding.** A derivation is a legitimate answer
/// and stays reachable; it just does not get to speak over its own source.
///
/// **The derivation is deliberately the STRONGER match here.** Without the
/// demotion it wins on relevance alone, so the ordering below is the
/// demotion's doing rather than the scorer's — the same trap the session
/// case had to design around.
#[test]
fn a_derivation_ranks_below_the_testimony_about_the_same_subject() {
    let index = FullTextIndex::open().expect("index opens");
    let day = "2026-08-10".parse().expect("a civil date");
    let spoken = Fact {
        provenance: Provenance::Testimony,
        standing: Standing::Settled,
        ..fact(
            "person:milhouse",
            "f1",
            "the committee meets on Tuesdays",
            day,
        )
    };
    let gloss = Fact {
        derived_from: Some(jojobot_domain::memory::FactAddress::new(
            EntityId("person:milhouse".into()),
            jojobot_domain::memory::FactId("f1".into()),
        )),
        ..fact(
            "person:milhouse",
            "f2",
            "committee committee committee — meets weekly, Tuesdays, per the note",
            day,
        )
    };
    index
        .ingest_all(
            &[DocScan {
                doc_id: "outline-uuid-1".into(),
                title: "Milhouse".into(),
                prose: String::new(),
                entity: Some(entity("person:milhouse", "Milhouse")),
                facts: vec![spoken, gloss],
                fields: Default::default(),
                owner: None,
            }],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("memory ingested");

    let found = index
        .search(&SearchQuery {
            text: Some("committee".into()),
            ..SearchQuery::default()
        })
        .expect("search ok");

    let at = |id: &str| {
        found.iter().position(|h| match h {
            Hit::Fact { fact, .. } => fact.id.0 == id,
            _ => false,
        })
    };
    let words = at("f1").expect("the testimony matched, or there is nothing to rank against");
    let derived = at("f2").expect(
        "the derivation must stay reachable — this is a demotion, \
                                       never a filter",
    );
    assert!(
        words < derived,
        "a derivation outranked the testimony it was worked out from: {found:?}",
    );

    // **And it is a demotion, not a burial.** A derivation is still about
    // the world, where a run is about the work, so it keeps its place above
    // one. Without this, pushing the constant up until a gloss sits below
    // everything would pass the assertion above and quietly turn a ranking
    // rule into a filter.
    index
        .ingest_sessions(&[run(
            "s-gamma",
            "bot:gamma",
            "the committee slice",
            "committee committee committee committee",
        )])
        .expect("sessions ingested");
    let with_a_run = index
        .search(&SearchQuery {
            text: Some("committee".into()),
            asked_by: Some(EntityId("bot:gamma".into())),
            ..SearchQuery::default()
        })
        .expect("search ok");
    let derived_now = with_a_run
        .iter()
        .position(|h| matches!(h, Hit::Fact { fact, .. } if fact.derived_from.is_some()))
        .expect("the derivation is still reachable");
    let session_now = with_a_run
        .iter()
        .position(|h| matches!(h, Hit::Session { .. }))
        .expect("the session matched too, or there is nothing to rank against");
    assert!(
        derived_now < session_now,
        "a derivation was pushed below a session, so the demotion has become a burial: \
             {with_a_run:?}",
    );
}

/// 🚨 **A derivation says when the claim under it was archived.**
///
/// The ranking demotion is not enough on its own: a gloss that still ranks
/// is still served, and a reader has no way to tell one whose source
/// stands from one whose source was archived. **The store holds the
/// answer and no hit carried it.**
///
/// ⭐ **The signal is the source's own status.** One value now covers a
/// source taken back with no replacement and a source a later claim
/// replaced — two fixtures below reach `Archived` by different roads
/// (one directly, one through a claim that then names it as
/// [`Fact::derived_from`]) and both report the same standing, which is
/// the point: nothing downstream needs to tell them apart.
///
/// ⚠️ **A stale one and a fresh one in the SAME read**, because a marker
/// that always fires is noise and one that never fires is not measuring.
/// The plain claim is the third: not derived from anything is an absence,
/// never a verdict.
#[test]
fn a_derivation_says_whether_the_claim_under_it_still_stands() {
    let index = FullTextIndex::open().expect("index opens");
    let day = "2026-08-10".parse().expect("a civil date");
    let at = |home: &str, id: &str| {
        jojobot_domain::memory::FactAddress::new(
            EntityId(home.into()),
            jojobot_domain::memory::FactId(id.into()),
        )
    };
    let stands = fact("person:milhouse", "f1", "the committee meets", day);
    let withdrawn = Fact {
        status: FactStatus::Archived,
        ..fact("person:milhouse", "f2", "the committee is disbanded", day)
    };
    let on_solid_ground = Fact {
        derived_from: Some(at("person:milhouse", "f1")),
        ..fact("person:milhouse", "f3", "the committee is weekly", day)
    };
    let left_hanging = Fact {
        derived_from: Some(at("person:milhouse", "f2")),
        ..fact("person:milhouse", "f4", "the committee is over", day)
    };
    // **A source the index does not hold.** Not the same answer as a source
    // that stands: nobody read it, so nobody can vouch for it.
    let pointing_nowhere = Fact {
        derived_from: Some(at("person:ralph", "f9")),
        ..fact("person:milhouse", "f5", "the committee moved rooms", day)
    };
    // **Archived by replacement, not by being taken back.** A later
    // claim named this one as its source, so the source did not stand —
    // reaching `Archived` the other road from `withdrawn` above.
    let replaced = Fact {
        status: FactStatus::Archived,
        ..fact("person:milhouse", "f7", "the committee met monthly", day)
    };
    let read_off_it = Fact {
        derived_from: Some(at("person:milhouse", "f7")),
        ..fact("person:milhouse", "f8", "the committee is infrequent", day)
    };
    let plain = fact("person:milhouse", "f6", "the committee has a chair", day);
    index
        .ingest_all(
            &[DocScan {
                doc_id: "outline-uuid-1".into(),
                title: "Milhouse".into(),
                prose: String::new(),
                entity: Some(entity("person:milhouse", "Milhouse")),
                facts: vec![
                    stands,
                    withdrawn,
                    on_solid_ground,
                    left_hanging,
                    pointing_nowhere,
                    replaced,
                    read_off_it,
                    plain,
                ],
                fields: Default::default(),
                owner: None,
            }],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("memory ingested");

    let found = index
        .search(&SearchQuery {
            text: Some("committee".into()),
            ..SearchQuery::default()
        })
        .expect("search ok");
    let standing_of = |id: &str| {
        found
            .iter()
            .find_map(|h| match h {
                Hit::Fact { fact, source, .. } if fact.id.0 == id => Some(*source),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{id} is not in the answer: {found:?}"))
    };

    assert_eq!(
        standing_of("f3"),
        Some(SourceStanding::Stands),
        "a derivation whose source stands was not said to stand",
    );
    assert_eq!(
        standing_of("f4"),
        Some(SourceStanding::Archived),
        "a derivation outlived the claim it was worked out from and said nothing",
    );
    assert_eq!(
        standing_of("f5"),
        Some(SourceStanding::Unreadable),
        "a source nobody has read was vouched for by silence",
    );
    assert_eq!(
        standing_of("f8"),
        Some(SourceStanding::Archived),
        "a derivation whose source was replaced was reported as resting on one that stands",
    );
    assert_eq!(
        standing_of("f6"),
        None,
        "a claim derived from nothing was given a verdict about a source it has not got",
    );
}

/// **A caller with no identity gets no session hit at all** — and the same
/// index answers one that does, so this is the scoping and not an empty
/// index.
#[test]
fn a_caller_with_no_identity_gets_no_session_hit() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[run(
            "s-gamma",
            "bot:gamma",
            "the kiln slice",
            "the damper is hand-cut",
        )])
        .expect("sessions ingested");

    let anonymous = index
        .search(&SearchQuery {
            text: Some("damper".into()),
            ..SearchQuery::default()
        })
        .expect("search ok");
    assert!(
        !anonymous.iter().any(|h| matches!(h, Hit::Session { .. })),
        "a caller with no identity is not everybody: {anonymous:?}"
    );

    let identified = index
        .search(&SearchQuery {
            text: Some("damper".into()),
            asked_by: Some(EntityId("bot:gamma".into())),
            ..SearchQuery::default()
        })
        .expect("search ok");
    assert!(
        identified.iter().any(|h| matches!(h, Hit::Session { .. })),
        "…and the same index does answer a caller that says who it is: {identified:?}"
    );
}

/// A store that just hands back the docs it was given, and can drop them.
///
/// Its doc ids are deliberately **not** entity handles — the real store's
/// shape, where the doc id is the entity's own badge. The fake keys facts
/// by handle, so anything that turns on the gap between the two ids is
/// invisible to it and shows up only here.
struct Scanned {
    docs: RwLock<Vec<DocScan>>,
    /// The store takes writes and cannot be read back — a transient fault on
    /// the re-read the decorator makes after a write has already committed.
    blind: std::sync::atomic::AtomicBool,
    /// **A scan that has read the store and has not answered yet.**
    ///
    /// A real scan is I/O: it takes its snapshot, suspends, and returns some
    /// time later, and the store can move in between. This double answers
    /// without ever suspending, so two futures on one runtime run
    /// start-to-finish in turn and never interleave — and a race test
    /// written against it passes on code that races. Holding a scan open is
    /// what makes that window exist here.
    park: std::sync::atomic::AtomicBool,
    /// Set once a parked scan has taken its snapshot, so a test waits for
    /// the window to be open rather than guessing at it.
    parked: std::sync::atomic::AtomicBool,
}

impl Scanned {
    /// Nothing like a handle: the ghost bug was deleting by one id having
    /// stored under the other.
    const DOC_ID: &'static str = "outline-uuid-7f3a";

    fn new(docs: Vec<DocScan>) -> Arc<Self> {
        Arc::new(Scanned {
            docs: RwLock::new(docs),
            blind: std::sync::atomic::AtomicBool::new(false),
            park: std::sync::atomic::AtomicBool::new(false),
            parked: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// The document is gone from the store.
    fn vanish(&self) {
        self.docs.write().expect("docs poisoned").clear();
    }

    /// One page is deleted and the rest of the store is untouched — a hand
    /// deletion, as opposed to a store that went away.
    fn lose(&self, doc_id: &str) {
        self.docs
            .write()
            .expect("docs poisoned")
            .retain(|d| d.doc_id != doc_id);
    }

    /// Writes still land; reads fail from here until [`sighted`](Self::sighted).
    fn blinded(&self) {
        self.blind.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn sighted(&self) {
        self.blind.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether reads are failing from here right now.
    fn is_blind(&self) -> bool {
        self.blind.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The next scan takes its snapshot and then waits to be let go.
    fn hold_scans(&self) {
        self.park.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether a scan is holding its snapshot right now — the window is open.
    fn scan_is_holding(&self) -> bool {
        self.parked.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Let a held scan answer with the snapshot it took.
    fn release_scans(&self) {
        self.park.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl Memory for Scanned {
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        unimplemented!("this double only scans")
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        if self.blind.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(MemoryError::Store("the store cannot be read".into()));
        }
        // **The snapshot is taken here, before the wait.** That is the whole
        // of what a held scan models: an answer describes the store as it
        // was when the read happened, never as it is when the answer lands.
        let snapshot = self.docs.read().expect("docs poisoned").clone();
        if self.park.load(std::sync::atomic::Ordering::SeqCst) {
            self.parked.store(true, std::sync::atomic::Ordering::SeqCst);
            while self.park.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            self.parked
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(snapshot)
    }

    /// Append a row to the subject's page and hand it back. **No guard** —
    /// the gates have their own specs, and what this double exists to
    /// exercise is what the decorator does *after* a write lands.
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let mut docs = self.docs.write().expect("docs poisoned");
        let doc = docs
            .iter_mut()
            .find(|d| d.entity.as_ref().is_some_and(|e| e.id == fact.subject))
            .ok_or_else(|| MemoryError::Store("this double writes to pages it holds".into()))?;
        let stored = Fact {
            id: jojobot_domain::memory::FactId(format!("f{}", doc.facts.len() + 1)),
            home: fact.subject.clone(),
            subject: fact.subject,
            content: fact.content,
            details: fact.details,
            provenance: fact.provenance,
            standing: Standing::Open,
            status: fact.status,
            recorded_at: fact.recorded_at,
            happened_at: None,
            happened_through: None,
            edge: fact.edge,
            fields: fact.fields,
            refs: fact.refs,
            derived_from: fact.derived_from,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
        };
        // A store that took a write says what the thing holds afterwards.
        doc.fields.extend(
            stored
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        doc.facts.push(stored.clone());
        Ok(Guarded::Written(stored))
    }

    async fn add_entity(&self, _: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        unimplemented!("this double only scans")
    }
    async fn list_entities(&self, _: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        unimplemented!("this double only scans")
    }
    async fn declare_type(&self, _: DeclaredType) -> Result<DeclaredType, MemoryError> {
        unimplemented!("this double only scans")
    }
    async fn declare_kind(
        &self,
        _: &str,
        _: jojobot_domain::memory::types::Origin,
        _: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        unimplemented!("this double only scans")
    }

    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        unimplemented!("this double only scans")
    }

    async fn reclaim_kind(&self, _: &str) -> Result<(), MemoryError> {
        unimplemented!("this double only scans")
    }

    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        unimplemented!("this double only scans")
    }
    /// Rename the entity on the page that declares it. **No guard**, for the
    /// same reason `capture` has none: what is under test is what the
    /// decorator does after a write lands, not whether the write was allowed.
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        let mut docs = self.docs.write().expect("docs poisoned");
        let doc = docs
            .iter_mut()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == handle))
            .ok_or_else(|| MemoryError::Store("this double edits pages it holds".into()))?;
        let entity = doc.entity.as_mut().expect("found by its entity");
        jojobot_domain::memory::apply_entity_patch(entity, &patch)?;
        doc.title = entity.name.clone();
        Ok(Guarded::Written(entity.clone()))
    }
    /// Same shape as `update_entity` above: no guard, because archiving
    /// carries none either.
    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        let mut docs = self.docs.write().expect("docs poisoned");
        let doc = docs
            .iter_mut()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == id))
            .ok_or_else(|| MemoryError::Store("this double edits pages it holds".into()))?;
        let entity = doc.entity.as_mut().expect("found by its entity");
        entity.archived = Some(jojobot_domain::memory::Archived {
            reason: reason.to_string(),
            at: jiff::Timestamp::now(),
        });
        Ok(entity.clone())
    }
    /// The mirror of the archive above, under the same terms.
    async fn restore_entity(&self, id: &EntityId) -> Result<(Entity, Archived), MemoryError> {
        let mut docs = self.docs.write().expect("docs poisoned");
        let doc = docs
            .iter_mut()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == id))
            .ok_or_else(|| MemoryError::Store("this double edits pages it holds".into()))?;
        let entity = doc.entity.as_mut().expect("found by its entity");
        let was = entity
            .archived
            .take()
            .ok_or_else(|| MemoryError::NotArchived {
                attempted: id.to_string(),
            })?;
        Ok((entity.clone(), was))
    }
    /// Same shape as `update_entity` above: no guard, because what is
    /// under test is what the decorator does after the write, not
    /// whether it was allowed.
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        _: Date,
        _: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        let mut docs = self.docs.write().expect("docs poisoned");
        let doc = docs
            .iter_mut()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == from))
            .ok_or_else(|| MemoryError::Store("this double edits pages it holds".into()))?;
        let entity = doc.entity.as_mut().expect("found by its entity");
        entity.id = to.clone();
        entity.kind = to.kind().expect("a staged handle names a kind");
        if let Some(new_parent) = parent {
            entity.parent = Some(new_parent);
        }
        Ok(Guarded::Written(entity.clone()))
    }
    async fn recall(&self, _: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        unimplemented!("this double only scans")
    }
    async fn history(&self, _: &EntityId, _: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        unimplemented!("this double only scans")
    }
    async fn claim_history(&self, _: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        unimplemented!("this double only scans")
    }
    /// **This double holds no write substrate, only a flat snapshot.** The
    /// default impl would reach `recall`/`claim_history`, neither of which
    /// this double has — so this is truthful rather than a workaround: a
    /// scan-only double genuinely has no history to report.
    async fn claim_histories(
        &self,
        _: &EntityId,
    ) -> Result<std::collections::HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        Ok(std::collections::HashMap::new())
    }
    async fn fields(&self, _: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        unimplemented!("this double only scans")
    }
    /// Replace the prose on the page that declares this entity. **No
    /// guard**, for the reason `capture` has none.
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        let mut docs = self.docs.write().expect("docs poisoned");
        let doc = docs
            .iter_mut()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == entity))
            .ok_or_else(|| MemoryError::Store("this double edits pages it holds".into()))?;
        doc.prose = prose.trim().to_string();
        Ok(doc.prose.clone())
    }
    async fn update_fact(
        &self,
        _: &FactAddress,
        _: FactPatch,
        _: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        unimplemented!("this double only scans")
    }

    async fn retract(
        &self,
        _: &FactAddress,
        _: Option<&str>,
        _: Date,
        _: &EntityId,
    ) -> Result<Retraction, MemoryError> {
        unimplemented!("this double only scans")
    }
    async fn merge(
        &self,
        _: &EntityId,
        _: &EntityId,
        _: Option<&str>,
        _: Date,
        _: &EntityId,
    ) -> Result<Merge, MemoryError> {
        unimplemented!("this double only scans")
    }
}

/// A [`Scanned`] wrapped with a settable [`Memory::write_summary`] and a
/// count of how many times [`Memory::scan`] actually ran.
///
/// **The positive control the sabotage bar asks for.** A counter that
/// never moves proves the same thing a broken one does, so a test built
/// on this drives one case where the count stands still (nothing
/// changed) and one where it moves (something did) — the pair, not
/// either alone.
struct SummarizedScan {
    inner: Arc<Scanned>,
    summary: RwLock<Option<WriteSummary>>,
    scans: std::sync::atomic::AtomicUsize,
}

impl SummarizedScan {
    fn new(inner: Arc<Scanned>) -> Arc<Self> {
        Arc::new(SummarizedScan {
            inner,
            summary: RwLock::new(None),
            scans: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// What [`Memory::write_summary`] answers from here on.
    fn set_summary(&self, summary: Option<WriteSummary>) {
        *self.summary.write().expect("summary poisoned") = summary;
    }

    /// How many times the real [`Memory::scan`] ran.
    fn scans_ran(&self) -> usize {
        self.scans.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl Memory for SummarizedScan {
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.inner.former_handles().await
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.scan().await
    }
    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        // **The same connection a real store's `scan` reads through.** A
        // store that cannot be read cannot answer this cheaply either —
        // both queries reach the same down connection — so this fails
        // exactly when `scan` would.
        if self.inner.is_blind() {
            return Err(MemoryError::Store("the store cannot be read".into()));
        }
        Ok(self.summary.read().expect("summary poisoned").clone())
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.capture(fact).await
    }
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.add_entity(new).await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
    }
    async fn declare_type(&self, declared: DeclaredType) -> Result<DeclaredType, MemoryError> {
        self.inner.declare_type(declared).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: jojobot_domain::memory::types::Origin,
        fields: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }
    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        self.inner.archive_entity(id, reason).await
    }
    async fn restore_entity(&self, id: &EntityId) -> Result<(Entity, Archived), MemoryError> {
        self.inner.restore_entity(id).await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.update_entity(handle, patch).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        override_token: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner
            .rename_entity(from, to, parent, date, override_token)
            .await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }
    /// **Overridden rather than left to the default.** [`Memory::claim_histories`]'s
    /// default asks [`Memory::recall`], which [`Scanned`] leaves
    /// `unimplemented!` on purpose — and `rescan`'s own read of this, for
    /// every entity a scan finds, is exactly what this double exists to
    /// let run without exercising history terms at all.
    async fn claim_histories(
        &self,
        _entity: &EntityId,
    ) -> Result<
        std::collections::HashMap<jojobot_domain::memory::FactId, Vec<ClaimWrite>>,
        MemoryError,
    > {
        Ok(std::collections::HashMap::new())
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.inner.fields(entity).await
    }
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        self.inner.set_prose(entity, prose).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.update_fact(address, patch, caller).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
    ) -> Result<Retraction, MemoryError> {
        self.inner.retract(address, reason, date, caller).await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
    ) -> Result<Merge, MemoryError> {
        self.inner
            .merge(folded, survivor, reason, date, caller)
            .await
    }
}

/// **A store that answers the three reads itself, and nothing else.**
///
/// Every method the trait can default is left `unimplemented!` on purpose:
/// a decorator that does not forward falls through to a default, and each
/// default reads `fields`, `history`, `list_entities` or `recall` to build
/// its answer. So a fall-through here panics naming the method it reached
/// for, rather than quietly returning the empty answer an unwritten store
/// would give.
struct Delegated;

impl Delegated {
    /// A word no default can produce: the defaults read rows, and this
    /// store has none.
    const MARKER: &'static str = "forwarded";

    fn marked() -> Fact {
        Fact {
            id: jojobot_domain::memory::FactId("f1".into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId::person("person:alpha"),
            content: Self::MARKER.into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: jojobot_domain::memory::Standing::Settled,
            status: FactStatus::Active,
            recorded_at: date(2026, 7, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
        }
    }
}

#[async_trait]
impl Memory for Delegated {
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn backing(
        &self,
        _: &EntityId,
    ) -> Result<BTreeMap<String, jojobot_domain::memory::FieldBacking>, MemoryError> {
        Ok([(
            Delegated::MARKER.to_string(),
            jojobot_domain::memory::FieldBacking {
                fact: jojobot_domain::memory::FactId("f1".into()),
                provenance: Provenance::Testimony,
                standing: jojobot_domain::memory::Standing::Settled,
                note: None,
            },
        )]
        .into_iter()
        .collect())
    }
    async fn built_on(&self, _: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        Ok(vec![Delegated::marked()])
    }
    async fn referring_to(&self, _: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        Ok(vec![Delegated::marked()])
    }

    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        Ok(Vec::new())
    }
    async fn scan_entity(&self, _: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        Ok(None)
    }
    async fn add_entity(&self, _: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn list_entities(&self, _: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        unimplemented!("a default walked every entity, so this read was not forwarded")
    }
    async fn update_entity(
        &self,
        _: &EntityId,
        _: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn rename_entity(
        &self,
        _: &EntityId,
        _: &EntityId,
        _: Option<EntityId>,
        _: Date,
        _: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn archive_entity(&self, _: &EntityId, _: &str) -> Result<Entity, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn restore_entity(&self, _: &EntityId) -> Result<(Entity, Archived), MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn capture(&self, _: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn recall(&self, _: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        unimplemented!("a default read a page, so this read was not forwarded")
    }
    async fn history(&self, _: &EntityId, _: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        unimplemented!("a default read a key's writes, so this read was not forwarded")
    }
    async fn claim_history(&self, _: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        unimplemented!("a default read a claim's writes, so this read was not forwarded")
    }
    async fn fields(&self, _: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        unimplemented!("a default read the folded fields, so this read was not forwarded")
    }
    async fn update_fact(
        &self,
        _: &FactAddress,
        _: FactPatch,
        _: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn retract(
        &self,
        _: &FactAddress,
        _: Option<&str>,
        _: Date,
        _: &EntityId,
    ) -> Result<Retraction, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn merge(
        &self,
        _: &EntityId,
        _: &EntityId,
        _: Option<&str>,
        _: Date,
        _: &EntityId,
    ) -> Result<Merge, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn set_prose(&self, _: &EntityId, _: &str) -> Result<String, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn declare_type(&self, _: DeclaredType) -> Result<DeclaredType, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        unimplemented!("a default read the declarations, so this read was not forwarded")
    }
    async fn declare_kind(
        &self,
        _: &str,
        _: jojobot_domain::memory::types::Origin,
        _: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
    async fn reclaim_kind(&self, _: &str) -> Result<(), MemoryError> {
        unimplemented!("this double answers the three reads a store owns")
    }
}

/// **The decorator forwards the reads a store answers for itself.**
///
/// The server wraps the store in this decorator and serves the wrapper, so
/// a method the wrapper does not implement is a method production never
/// runs — whatever the store underneath it does. That is invisible from
/// every other case, because a default and an override that agree on their
/// answer are indistinguishable by their answer.
///
/// So the store here answers with a word no default can produce, and every
/// method a default would read from panics naming itself.
#[tokio::test]
async fn the_decorator_forwards_the_reads_the_store_answers_for_itself() {
    let indexed = IndexedMemory::new(Arc::new(Delegated)).expect("index opens");
    let alpha = EntityId::person("person:alpha");

    let backed = Memory::backing(&indexed, &alpha)
        .await
        .expect("the store answers");
    assert!(
        backed.contains_key(Delegated::MARKER),
        "the decorator answered for itself instead of asking the store: {backed:?}",
    );

    let standing_on = Memory::built_on(
        &indexed,
        &FactAddress {
            home: alpha.clone(),
            local: jojobot_domain::memory::FactId("f1".into()),
        },
    )
    .await
    .expect("the store answers");
    assert_eq!(
        standing_on.first().map(|f| f.content.as_str()),
        Some(Delegated::MARKER),
        "lineage came from somewhere other than the store: {standing_on:?}",
    );

    let pointing = Memory::referring_to(&indexed, &alpha)
        .await
        .expect("the store answers");
    assert_eq!(
        pointing.first().map(|f| f.content.as_str()),
        Some(Delegated::MARKER),
        "the records pointing here came from somewhere other than the store: {pointing:?}",
    );
}

/// One page with one entity on it, for the tests that turn on what the
/// decorator does after a write lands.
fn one_page() -> Arc<Scanned> {
    Scanned::new(vec![DocScan {
        doc_id: Scanned::DOC_ID.into(),
        title: "Alpha".into(),
        prose: String::new(),
        entity: Some(entity("person:alpha", "Alpha")),
        facts: Vec::new(),
        fields: Default::default(),
        owner: None,
    }])
}

/// One row to write onto that page.
fn ferret() -> NewFact {
    NewFact {
        subject: EntityId::person("person:alpha"),
        content: "keeps a ferret".into(),
        details: None,
        provenance: Provenance::Testimony,
        standing: None,
        status: FactStatus::Active,
        recorded_at: date(2026, 1, 1),
        happened_at: None,
        happened_through: None,
        edge: None,
        fields: Default::default(),
        refs: Vec::new(),
        derived_from: None,
        stale_after: None,
        drop: None,
        drop_because: None,
        borrow: false,
        aged_before: None,
        session: None,
        sets: Default::default(),
    }
}

/// **An unchanged store costs no re-read.** [`Memory::write_summary`]
/// reported the same signal before and after a search that found nothing
/// to do, so the second search's refresh must not have paid for
/// [`Memory::scan`] again.
#[tokio::test]
async fn an_unchanged_store_costs_no_re_read() {
    let spy = SummarizedScan::new(one_page());
    spy.set_summary(Some(WriteSummary {
        entities: (1, Some(jiff::Timestamp::now())),
        facts: (0, None),
        entity_hash: None,
        fact_hash: None,
    }));
    let store = Arc::new(IndexedMemory::new(spy.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(spy.scans_ran(), 1, "the boot rebuild always reads once");

    store
        .search_via_port(&SearchQuery::text("Alpha"))
        .await
        .expect("search answers");
    assert_eq!(
        spy.scans_ran(),
        1,
        "the signal did not move, so the second search paid for no re-read"
    );
}

/// **The positive control for the test above.** The same shape, except
/// the signal DOES move between the two searches — proving the count is
/// a real instrument and not one that would read `1` regardless of what
/// the store did.
#[tokio::test]
async fn a_changed_store_is_still_seen() {
    let spy = SummarizedScan::new(one_page());
    spy.set_summary(Some(WriteSummary {
        entities: (1, Some(jiff::Timestamp::now())),
        facts: (0, None),
        entity_hash: None,
        fact_hash: None,
    }));
    let store = Arc::new(IndexedMemory::new(spy.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(spy.scans_ran(), 1, "the boot rebuild always reads once");

    spy.set_summary(Some(WriteSummary {
        entities: (1, Some(jiff::Timestamp::now())),
        facts: (1, Some(jiff::Timestamp::now())),
        entity_hash: None,
        fact_hash: None,
    }));
    store
        .search_via_port(&SearchQuery::text("Alpha"))
        .await
        .expect("search answers");
    assert_eq!(
        spy.scans_ran(),
        2,
        "the signal moved, so the refresh paid for a real re-read"
    );
}

/// **A refresh already on record as failed still gets a real attempt**,
/// even when the signal reports exactly what it reported before the
/// store went down.
///
/// That is the ordinary shape of a real outage: nothing wrote while the
/// store was unreachable, so the signal on the way back up equals the
/// signal on the way down. A skip keyed on the signal ALONE would read
/// that as "unchanged" forever and never attempt the read that clears
/// the failure — this is what [`FullTextIndex::memory_refresh_pending`]
/// exists to stop.
#[tokio::test]
async fn a_refresh_marked_failed_still_gets_a_real_read_on_recovery() {
    let spy = SummarizedScan::new(one_page());
    let summary = WriteSummary {
        entities: (1, Some(jiff::Timestamp::now())),
        facts: (0, None),
        entity_hash: None,
        fact_hash: None,
    };
    spy.set_summary(Some(summary.clone()));
    let store = Arc::new(IndexedMemory::new(spy.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(spy.scans_ran(), 1, "the boot rebuild always reads once");

    spy.inner.blinded();
    Refresh::refresh(store.as_ref()).await;
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "the store went down mid-refresh, so the index is behind"
    );
    assert_eq!(
        spy.scans_ran(),
        1,
        "the failed attempt never reached scan — it failed on the signal itself"
    );

    // The store recovers. Nothing wrote while it was down, so the signal
    // is exactly what it was before — the case a signal-only skip cannot
    // tell from "still fine".
    spy.inner.sighted();
    store
        .search_via_port(&SearchQuery::text("Alpha"))
        .await
        .expect("search answers");
    assert_eq!(
        spy.scans_ran(),
        2,
        "a refresh on record as failed must get a real attempt, signal or no signal"
    );
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "the real attempt succeeded, so the failure is cleared"
    );
}

/// **A store with no signal is scanned every time** — the fallback that
/// keeps every store this slice does not touch exactly as it was.
#[tokio::test]
async fn a_store_with_no_signal_is_scanned_every_search() {
    let spy = SummarizedScan::new(one_page());
    // `set_summary` never called: the double's own default is `None`,
    // exactly the trait's default for a store that has not opted in.
    let store = Arc::new(IndexedMemory::new(spy.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(spy.scans_ran(), 1);

    store
        .search_via_port(&SearchQuery::text("Alpha"))
        .await
        .expect("search answers");
    assert_eq!(
        spy.scans_ran(),
        2,
        "no signal means no cache to trust, so every search still reads"
    );
}

/// **A refresh clears only what its own reading covered.**
///
/// A whole-corpus scan says "nothing is behind" on the strength of having
/// read everything. That is a claim about the store **as of the moment it
/// read**, and a scan is I/O: a write can commit, fail its re-read and mark
/// the index behind while the scan is still in flight. Clearing the whole
/// mark set on arrival then vouches for a write the scan never saw, and
/// `search` reports complete coverage over a document the index does not
/// hold — rule 130's shape, in the channel built to report exactly this.
///
/// **The double holds its scan open** so the window exists at all: it
/// answers instantly otherwise, and two futures on one runtime would run in
/// turn and never overlap.
#[tokio::test]
async fn a_scan_in_flight_does_not_clear_a_mark_its_snapshot_predates() {
    let inner = one_page();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "a scan that just succeeded is complete coverage"
    );

    // A search's whole-corpus refresh reads the store and holds its answer.
    inner.hold_scans();
    let searching = tokio::spawn({
        let store = store.clone();
        async move { store.search_via_port(&SearchQuery::text("Alpha")).await }
    });
    while !inner.scan_is_holding() {
        tokio::task::yield_now().await;
    }

    // Underneath it: a write commits and its re-read cannot run.
    inner.blinded();
    let written = store.capture(ferret()).await.expect("the store took it");
    assert_eq!(
        written
            .written()
            .expect("this double does not guard")
            .content,
        "keeps a ferret",
        "the write really landed, so there is something to be behind on"
    );
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "the index says it is behind, which is the mark the scan must not clear"
    );

    // The held scan now answers with the snapshot it took before that write.
    inner.sighted();
    inner.release_scans();
    searching
        .await
        .expect("the search task")
        .expect("the search answers");

    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "a reading taken before the write cannot vouch for the write"
    );
}

/// **A reading in flight does not clear a corpus failure its snapshot
/// predates** — the sibling of the mark race above, on the flag rather than
/// on the set.
///
/// One reading of the whole corpus says "the index holds the store". A
/// refresh that could not reach the store at all says the opposite, about
/// everything at once. If the first was already in flight when the second
/// failed, it lands afterwards and clears a failure it never saw — and
/// coverage comes back complete on the strength of a reading taken before
/// the store stopped answering.
///
/// The claim here is weaker than the mark race's and still wrong in the
/// same direction: the index does hold a full reading, just an older one
/// than the caller is being promised.
#[tokio::test]
async fn a_reading_in_flight_does_not_clear_a_corpus_failure_it_predates() {
    let inner = one_page();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "a scan that just succeeded is complete coverage"
    );

    // A search's whole-corpus refresh reads the store and holds its answer.
    inner.hold_scans();
    let searching = tokio::spawn({
        let store = store.clone();
        async move { store.search_via_port(&SearchQuery::text("Alpha")).await }
    });
    while !inner.scan_is_holding() {
        tokio::task::yield_now().await;
    }

    // Underneath it: a refresh of the whole corpus cannot reach the store.
    // It fails on the blind check before it can park, so this one does not
    // queue up behind the reading being held.
    inner.blinded();
    Refresh::refresh(store.as_ref()).await;
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "the index says it is behind the store, which is what must survive"
    );

    // The held reading now answers with the snapshot it took beforehand.
    inner.sighted();
    inner.release_scans();
    searching
        .await
        .expect("the search task")
        .expect("the search answers");

    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "a reading taken before the failure cannot vouch for the store after it"
    );
}

/// **The other direction, and the positive both directions rest on.**
///
/// A failure that a reading BEGAN AFTER is exactly the one that reading is
/// entitled to clear: it read the store later than the failure, so it
/// speaks for the store as it stands. Without this the test above is
/// satisfied by a flag that is never cleared at all, which reports every
/// answer as behind forever.
///
/// And a run where nothing failed reports complete, or "behind" would be
/// the only state this index can produce.
#[tokio::test]
async fn a_reading_that_began_after_a_failure_clears_it() {
    let inner = one_page();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "nothing has failed yet, so nothing is behind"
    );

    inner.blinded();
    Refresh::refresh(store.as_ref()).await;
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "a refresh that could not reach the store leaves the index behind"
    );

    // A reading that starts now sees the store after the failure, so its
    // word is good.
    inner.sighted();
    Refresh::refresh(store.as_ref()).await;
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "a reading taken after the failure speaks for the store as it stands"
    );
}

/// **A write the store took is not failed by the projection behind it.**
///
/// The re-index re-reads the doc AFTER the store has committed, so a
/// transient fault on that read must not fail the whole verb. A caller told
/// nothing was written tries again, and the fact is recorded twice. The
/// store is the truth; an index that could not refresh is a stale
/// projection, not a failed write.
#[tokio::test]
async fn a_write_the_store_took_is_not_failed_by_a_refresh_that_could_not_run() {
    let inner = one_page();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");

    inner.blinded();
    let written = store
        .capture(ferret())
        .await
        .expect("the store took the write, so the verb reports it");

    // The positive the whole case rests on: the write is reported, and it is
    // the write that was made. Asserting only that no error came back would
    // pass on a verb that returned an empty answer.
    let fact = written.written().expect("this double does not guard");
    assert_eq!(fact.content, "keeps a ferret");
    assert_eq!(
        inner
            .docs
            .read()
            .expect("docs poisoned")
            .iter()
            .flat_map(|d| d.facts.iter())
            .map(|f| f.content.as_str())
            .collect::<Vec<_>>(),
        vec!["keeps a ferret"],
        "the row is on the page: the write committed and stayed committed"
    );
}

/// **An index that could not refresh says so, on every answer.**
///
/// Keeping the write and saying nothing would move the wrong answer from the
/// caller who wrote to every session that reads afterwards — and those have
/// no way to know. `search` is the only verb served from the index, so this
/// is where it has to be said.
#[tokio::test]
async fn a_doc_the_index_could_not_re_read_makes_the_memory_half_partial() {
    let inner = one_page();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "a scan that read the store holds all of it"
    );

    inner.blinded();
    store
        .capture(ferret())
        .await
        .expect("the store took the write");

    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale),
        "the index holds the version before that write and has to say so"
    );
    assert_eq!(
        store.index().behind_now(),
        vec![EntityId::person("person:alpha")],
        "and it knows which document it is behind on"
    );
    // What `Partial` claims, and the half a bare "not Loaded" assertion would
    // miss: the hits that ARE there are real, and the new row is not among
    // them.
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("person:alpha"))
            .await
            .expect("search ok")
            .len(),
        1,
        "the entity is still findable — partial is not empty"
    );
    assert!(
        store
            .search_via_port(&SearchQuery::text("ferret"))
            .await
            .expect("search ok")
            .is_empty(),
        "and the row the index could not read is the part that is missing"
    );
}

/// **The other way into `Partial`, and it takes away far more.**
///
/// A boot scan that never ran leaves the index holding nothing except what
/// this process has written since. A stale doc leaves it holding everything
/// except one write. Those are opposite claims about an empty result, and a
/// caller decides whether to believe one on exactly that difference — so
/// `Partial` has to say which state it is reporting.
#[tokio::test]
async fn a_boot_scan_that_never_ran_is_partial_for_the_other_reason() {
    let inner = Scanned::new(vec![
        DocScan {
            doc_id: Scanned::DOC_ID.into(),
            title: "Alpha".into(),
            prose: String::new(),
            entity: Some(entity("person:alpha", "Alpha")),
            facts: Vec::new(),
            fields: Default::default(),
            owner: None,
        },
        DocScan {
            doc_id: "outline-uuid-b2c9".into(),
            title: "Beta".into(),
            prose: String::new(),
            entity: Some(entity("person:beta", "Beta")),
            facts: vec![fact(
                "person:beta",
                "f1",
                "restrings the harp",
                date(2026, 1, 1),
            )],
            fields: Default::default(),
            owner: None,
        },
    ]);
    inner.blinded();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store
        .rebuild()
        .await
        .expect_err("the scan cannot read the store");
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Unread,
        "nothing was read and nothing has been written"
    );

    inner.sighted();
    store
        .capture(ferret())
        .await
        .expect("the store took the write");

    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Unscanned),
        "the scan never ran, so the index holds only the page this write touched"
    );
    assert!(
        store.index().behind_now().is_empty(),
        "and no document is behind: this write was re-read, which is the other state"
    );
    // Blind again to read the state itself. A read takes its own scan, so
    // "the scan never ran" is only observable while it still cannot run —
    // and that is the state this test is about.
    inner.blinded();
    // The positive the negative below depends on. Without it, "beta is
    // missing" passes just as well on an index that holds nothing at all,
    // which is the state this one is being told apart from.
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("ferret"))
            .await
            .expect("search ok")
            .len(),
        1,
        "what this process wrote is indexed and findable"
    );
    assert!(
        store
            .search_via_port(&SearchQuery::text("harp"))
            .await
            .expect("search ok")
            .is_empty(),
        "and the page the scan never read is the part that is missing"
    );
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Unscanned),
        "the wider claim still wins: almost nothing is searchable"
    );

    // And it is not a state that lasts until a restart. The first read that
    // reaches the store fills the half that was never scanned.
    inner.sighted();
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("harp"))
            .await
            .expect("search ok")
            .len(),
        1,
        "the page arrives on the first read that can reach the store"
    );
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "…and the answer stops hedging, because the scan behind it ran"
    );
}

/// **A refresh that runs takes the mark off.** The state is "the index is
/// behind on this document", not "something went wrong once" — so the write
/// that catches it up clears it, and the answers stop hedging.
#[tokio::test]
async fn a_refresh_that_runs_clears_the_mark_and_the_hedge() {
    let inner = one_page();
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");

    inner.blinded();
    store.capture(ferret()).await.expect("the store took it");
    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Partial(Behind::Stale)
    );

    inner.sighted();
    store
        .capture(NewFact {
            content: "plays the sousaphone".into(),
            ..ferret()
        })
        .await
        .expect("the store took it");

    assert_eq!(
        store.memory_coverage_via_port(),
        Coverage::Loaded,
        "the doc was re-read whole, so nothing on it is behind any more"
    );
    assert!(store.index().behind_now().is_empty());
    // The re-read is of the whole document, so the row written while the
    // store was unreadable comes back with it.
    for missed in ["ferret", "sousaphone"] {
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text(missed))
                .await
                .expect("search ok")
                .len(),
            1,
            "{missed} is findable once the refresh runs"
        );
    }
}

/// **A vanished doc leaves no ghost.** Eviction has to key on the id the
/// postings were written under — the store's doc id — not the entity handle.
/// It keyed on the handle, so in the real store (where a doc id is the
/// entity's own badge, not its handle) the delete matched nothing: the
/// entity was gone and every one of its hits was still being served,
/// forever, from the last scan.
#[tokio::test]
async fn reindexing_a_vanished_doc_evicts_every_hit_it_had() {
    let inner = Scanned::new(vec![DocScan {
        doc_id: Scanned::DOC_ID.into(),
        title: "Alpha".into(),
        prose: "Alpha is allergic to penicillin.".into(),
        entity: Some(entity("person:alpha", "Alpha")),
        facts: vec![fact(
            "person:alpha",
            "f1",
            "keeps a ferret",
            date(2026, 1, 1),
        )],
        fields: Default::default(),
        owner: None,
    }]);
    let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
    store.rebuild().await.expect("rebuild");

    let alpha = EntityId::person("person:alpha");
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("ferret"))
            .await
            .expect("search ok")
            .len(),
        1
    );
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("penicillin"))
            .await
            .expect("search ok")
            .len(),
        1
    );
    assert!(
        store
            .search_via_port(&SearchQuery::text("person:alpha"))
            .await
            .expect("search ok")
            .iter()
            .any(|h| matches!(h, Hit::Entity { .. }))
    );

    inner.vanish();
    store.reindex(&alpha).await.expect("reindex ok");

    for gone in ["ferret", "penicillin"] {
        assert!(
            store
                .search_via_port(&SearchQuery::text(gone))
                .await
                .expect("search ok")
                .is_empty(),
            "{gone:?} must be gone from the index with its doc"
        );
    }
    assert!(
        store
            .search_via_port(&SearchQuery::text("person:alpha"))
            .await
            .expect("search ok")
            .is_empty(),
        "…and so must the entity, pin and all"
    );
}

/// The crate's one log sink. **Shared, not local**: other stores also
/// reports things whose only surface is a log line, and a process gets
/// exactly one global subscriber — so the sink lives beside both of them.
use crate::log_capture::log_sink;

/// **A row whose subject names nothing is counted and said out loud.** This
/// is the split-brain tell a hand edit leaves: the row stays reachable
/// through its home doc, so nothing breaks and nothing looks wrong — which
/// is exactly the problem. A scan that silently normalizes a corruption is
/// how the corruption becomes permanent.
///
/// Never a failure, never a drop: the fact is still indexed and still found.
#[tokio::test]
async fn a_scan_counts_and_logs_a_subject_that_names_no_entity() {
    let logged = log_sink();

    let orphan = Fact {
        subject: EntityId::person("person:alphaa"),
        ..fact("person:alpha", "f1", "plays chess", date(2026, 1, 1))
    };
    // Its own doc id, unique in this binary. The sink is the binary's — one
    // subscriber shared by every test in it — so anything counted over its
    // text has to be counted by something no other test can log.
    const REPORTED_DOC: &str = "outline-uuid-orphan-report";
    let store = Arc::new(
        IndexedMemory::new(Scanned::new(vec![scan(
            REPORTED_DOC,
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![
                orphan,
                fact("person:alpha", "f2", "plays go", date(2026, 1, 2)),
            ],
        )]))
        .expect("index opens"),
    );
    store.rebuild().await.expect("rebuild");

    let text = logged.text();
    assert!(
        text.contains(REPORTED_DOC),
        "the log must say which doc: {text}"
    );
    assert!(text.contains("person:alphaa"), "…and which subject: {text}");
    assert!(text.contains("count=1"), "…and how many: {text}");
    assert!(
        !text.contains("person:alpha\""),
        "the doc's own well-formed subject is not an orphan: {text}"
    );

    // …and the row is still there. Reporting it is not quarantining it.
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("chess"))
            .await
            .expect("search ok")
            .len(),
        1,
        "a counted row is still indexed and still findable"
    );
    // That read re-scanned the store, and re-scanning is not re-reporting.
    // The report is a reading of the corpus; one per answer would bury it.
    assert_eq!(
        logged.text().matches(REPORTED_DOC).count(),
        1,
        "a read refreshes the index without repeating the report"
    );
}

/// **The count survives the incremental path too.** A boot scan is not the
/// only way a doc gets read: every write re-reads the page it touched, and
/// that is the read most likely to be looking at a page a human just edited
/// by hand. A counter wired only into `rebuild` would go quiet for as long as
/// the server stayed up — exactly the window in which the damage is done.
///
/// Nothing here calls `rebuild`, so the only thing that can have written to
/// the log is the write.
#[tokio::test]
async fn a_write_re_reads_its_doc_and_counts_the_orphans_it_finds() {
    let logged = log_sink();
    const DOC: &str = "outline-uuid-re1nd3x";

    let hand_edited = Fact {
        subject: EntityId::person("person:ghostly"),
        ..fact(
            "person:alpha",
            "f1",
            "subject cell retyped by hand",
            date(2026, 1, 1),
        )
    };
    let store = Arc::new(
        IndexedMemory::new(Scanned::new(vec![scan(
            DOC,
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![hand_edited],
        )]))
        .expect("index opens"),
    );

    store
        .capture(NewFact::about(
            EntityId::person("person:alpha"),
            "an ordinary fact, written now",
            date(2026, 1, 2),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("this double does not guard");

    let text = logged.text();
    assert!(text.contains(DOC), "the reindex must say which doc: {text}");
    assert!(
        text.contains("person:ghostly"),
        "…and which subject: {text}"
    );

    // …and the write is findable, which is the reindex having run at all.
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("ordinary"))
            .await
            .expect("search ok")
            .len(),
        1
    );
}

/// The **other** counter, on the same path. A subject retyped onto a handle
/// that EXISTS orphans nothing, so the orphan reporter has nothing to say
/// about it — which is the disguise a split brain wears. Both counters have
/// to survive a write rather than only a boot scan: pinned on the boot scan
/// alone, dropping the foreign one from the write path leaves the whole
/// suite green.
#[tokio::test]
async fn a_write_re_reads_its_doc_and_counts_the_foreign_subjects_it_finds() {
    let logged = log_sink();
    const DOC: &str = "outline-uuid-f0r31gn";

    let elsewhere = Fact {
        subject: EntityId::person("person:kappa"),
        ..fact(
            "person:alpha",
            "f1",
            "subject cell retyped onto a live handle",
            date(2026, 1, 1),
        )
    };
    let store = Arc::new(
        IndexedMemory::new(Scanned::new(vec![
            scan(
                DOC,
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![elsewhere],
            ),
            scan(
                "outline-uuid-kappa",
                Some(entity("person:kappa", "Kappa")),
                "",
                Vec::new(),
            ),
        ]))
        .expect("index opens"),
    );

    // Make kappa known the way a running server does — one doc at a time.
    // Never a rebuild: a rebuild reports this doc itself, and the assertion
    // would stop being about the write path.
    store
        .reindex(&EntityId::person("person:kappa"))
        .await
        .expect("reindex ok");

    store
        .capture(NewFact::about(
            EntityId::person("person:alpha"),
            "another ordinary fact, written now",
            date(2026, 1, 2),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("this double does not guard");

    // Per LINE, not per buffer: the sink is process-wide and append-only, so
    // a substring match would happily find another test's event.
    let text = logged.text();
    assert!(
        text.lines().any(|l| l.contains(DOC)
            && l.contains("person:kappa")
            && l.contains("about a different entity that exists")),
        "the write path must count the foreign subject, not only the orphans: {text}"
    );
}

/// **A row about another live entity is counted too** — the consistency check
/// the orphan counter cannot make. A hand edit that retypes a subject cell
/// into a handle that *exists* orphans nothing and leaves every read
/// working, while the entity is quietly readable under one id and writable
/// under another.
///
/// Legitimate as often as not — a fact about one entity is frequently
/// written on another's page — so this is a signal, never a fault: counted,
/// logged with its doc, and the row left exactly where it is.
#[tokio::test]
async fn a_scan_counts_and_logs_a_row_about_another_entity() {
    let logged = log_sink();
    const DOC: &str = "outline-uuid-c05m3";

    let elsewhere = Fact {
        subject: EntityId::person("person:beta"),
        ..fact("person:alpha", "f1", "took the ferry", date(2026, 1, 1))
    };
    let store = Arc::new(
        IndexedMemory::new(Scanned::new(vec![
            scan(
                DOC,
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![
                    elsewhere,
                    fact("person:alpha", "f2", "took the bus", date(2026, 1, 2)),
                ],
            ),
            scan(
                "outline-uuid-beta",
                Some(entity("person:beta", "Beta")),
                "",
                Vec::new(),
            ),
        ]))
        .expect("index opens"),
    );
    store.rebuild().await.expect("rebuild");

    let text = logged.text();
    assert!(text.contains(DOC), "the log must say which doc: {text}");
    assert!(text.contains("person:beta"), "…and which subject: {text}");
    assert!(
        text.contains("count=1"),
        "…and how many — the doc's own subject is not one of them: {text}"
    );

    // Counted is not quarantined: the row is still indexed and still found.
    assert_eq!(
        store
            .search_via_port(&SearchQuery::text("ferry"))
            .await
            .expect("search ok")
            .len(),
        1
    );
}

// --- mail in the one list -------------------------------------------------

fn message(
    id: &str,
    mailbox: &str,
    sender: &str,
    subject: Option<&str>,
    body: &str,
    state: MessageState,
) -> Message {
    // **The fixture stands a store up, because the set is setup here.** A
    // message carries its sender's handle, and reading one asks the kinds
    // this process loaded — so the set arrives the way a boot delivers it,
    // from what a store holds.
    let _booted = jojobot_domain::memory::testing::InMemoryMemory::booted();
    Message {
        id: MessageId(id.into()),
        mailbox: MailboxName(mailbox.into()),
        body: body.into(),
        subject: subject.map(str::to_string),
        sender: sender.into(),
        sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
        state,
        notes: None,
        in_reply_to: None,
        taken_by: None,
        sender_mail_waiting_at_send: None,
        posted_by_session: None,
    }
}

/// **The write-only rail, opened.** A finding filed in a message comes back
/// in the same ranked list as the fact, entity and prose hits — which is the
/// whole slice: a later session finds context it did not know to look for.
/// And it arrives unmistakably as mail: its box, its state, its sender, and
/// the id `read_message` takes.
#[tokio::test]
async fn a_message_comes_back_in_the_same_ranked_list() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact(
            "person:alpha",
            "f1",
            "runs the kiln on Tuesdays",
            date(2026, 1, 1),
        )],
    )]);
    index
        .ingest_mail(&[message(
            "42",
            "pm",
            "dev (implementer)",
            Some("the kiln slice"),
            "The kiln rebuild landed; the damper is still hand-cut.",
            MessageState::Read,
        )])
        .expect("ingest mail");

    let hits = index.search(&asking_for_mail("damper")).expect("search ok");
    let Some(Hit::Message { message, snippet }) =
        hits.iter().find(|h| matches!(h, Hit::Message { .. }))
    else {
        panic!("the message must be a hit: {hits:?}")
    };
    assert_eq!(
        message.id.as_str(),
        "42",
        "…carrying the id read_message takes"
    );
    assert_eq!(message.mailbox.as_str(), "pm", "…and which box it is in");
    assert_eq!(
        message.state,
        MessageState::Read,
        "…and what state it is in"
    );
    assert_eq!(message.sender, "dev (implementer)");
    assert_eq!(message.subject.as_deref(), Some("the kiln slice"));
    assert!(snippet.to_lowercase().contains("damper"), "got {snippet:?}");

    // One list: the same query reaches mail and memory together.
    let mixed = index.search(&asking_for_mail("kiln")).expect("search ok");
    assert!(
        mixed.iter().any(|h| matches!(h, Hit::Fact { .. })),
        "{mixed:?}"
    );
    assert!(
        mixed.iter().any(|h| matches!(h, Hit::Message { .. })),
        "{mixed:?}"
    );
}

/// **A failed write mid-ingest must not leave its own staged delete for a
/// LATER, unrelated commit to apply.** A rewrite stages the old posting's
/// delete before writing the new one; if the write fails, that delete is
/// still sitting in the shared writer, uncommitted — and without a
/// rollback, the next successful ingest of something else entirely
/// commits it too, dropping a message neither call meant to touch out of
/// search.
#[tokio::test]
async fn a_failed_write_does_not_leave_its_staged_delete_for_a_later_commit() {
    let index = index_of(vec![]);
    index
        .ingest_mail(&[message(
            "1",
            "pm",
            "dev",
            None,
            "original body unique-needle-survivor",
            MessageState::New,
        )])
        .expect("the first ingest lands");
    assert!(
        index
            .search(&asking_for_mail("unique-needle-survivor"))
            .expect("search ok")
            .iter()
            .any(|h| matches!(h, Hit::Message { message, .. } if message.id.as_str() == "1")),
        "findable before anything goes wrong",
    );

    // Rewriting it stages the old posting's delete, then the write that
    // was meant to replace it is made to fail.
    index.fail_add_document_at(1);
    let failed = index.ingest_mail(&[message(
        "1",
        "pm",
        "dev",
        None,
        "rewritten body unique-needle-rewrite",
        MessageState::New,
    )]);
    assert!(
        failed.is_err(),
        "the injected fault must surface: {failed:?}"
    );

    // An unrelated, entirely successful ingest — of a different message,
    // alongside the board's own unchanged report of message 1's ORIGINAL
    // content. `ingest_mail_changes` reads a full board state each call,
    // so leaving message 1 out here would evict it legitimately, which
    // is a different question from the one this case asks: whether the
    // FAILED call's own staged delete survives a commit that never
    // touched message 1 at all.
    index
        .ingest_mail(&[
            message(
                "1",
                "pm",
                "dev",
                None,
                "original body unique-needle-survivor",
                MessageState::New,
            ),
            message(
                "2",
                "pm",
                "dev",
                None,
                "an unrelated message unique-needle-other",
                MessageState::New,
            ),
        ])
        .expect("the unrelated ingest lands");

    // The original message must still be findable: the failed call's own
    // staged delete must not have been left for this unrelated commit to
    // apply.
    let hits = index
        .search(&asking_for_mail("unique-needle-survivor"))
        .expect("search ok");
    assert!(
        hits.iter()
            .any(|h| matches!(h, Hit::Message { message, .. } if message.id.as_str() == "1")),
        "the failed call's own staged delete must not survive it: {hits:?}",
    );
}

/// **The paired proof that commit and reload failures surface, rather
/// than being reported as a success.** Each is a stated failure the
/// caller can see — `ingest_mail_changes` returns `Err`, never `Ok` with
/// silently mangled state.
#[tokio::test]
async fn a_commit_failure_and_a_reload_failure_each_surface_as_an_error() {
    let index = index_of(vec![]);
    index.fail_next_commit();
    let commit_failed =
        index.ingest_mail(&[message("1", "pm", "dev", None, "body", MessageState::New)]);
    assert!(
        commit_failed.is_err(),
        "a commit failure must not read as success: {commit_failed:?}"
    );

    // The negative it rests on: with no fault armed, the same call lands.
    index
        .ingest_mail(&[message("1", "pm", "dev", None, "body", MessageState::New)])
        .expect("an ordinary ingest still lands");

    index.fail_next_reload();
    let reload_failed = index.ingest_mail(&[message(
        "2",
        "pm",
        "dev",
        None,
        "other body",
        MessageState::New,
    )]);
    assert!(
        reload_failed.is_err(),
        "a reload failure must not read as success: {reload_failed:?}"
    );
}

/// **The same proof, on `ingest_message`'s single-item path.** No board
/// diff to get wrong here — the fault sits in the same shared writer, so
/// the failed call's staged delete is exactly as reachable by a later,
/// unrelated commit.
#[tokio::test]
async fn a_failed_single_message_write_does_not_leave_its_staged_delete_for_a_later_commit() {
    let index = index_of(vec![]);
    index
        .ingest_message(&message(
            "1",
            "pm",
            "dev",
            None,
            "original body unique-needle-survivor",
            MessageState::New,
        ))
        .expect("the first write lands");
    assert!(
        index
            .search(&asking_for_mail("unique-needle-survivor"))
            .expect("search ok")
            .iter()
            .any(|h| matches!(h, Hit::Message { message, .. } if message.id.as_str() == "1")),
        "findable before anything goes wrong",
    );

    index.fail_add_document_at(1);
    let failed = index.ingest_message(&message(
        "1",
        "pm",
        "dev",
        None,
        "rewritten body unique-needle-rewrite",
        MessageState::New,
    ));
    assert!(
        failed.is_err(),
        "the injected fault must surface: {failed:?}"
    );

    index
        .ingest_message(&message(
            "2",
            "pm",
            "dev",
            None,
            "an unrelated message unique-needle-other",
            MessageState::New,
        ))
        .expect("the unrelated write lands");

    let hits = index
        .search(&asking_for_mail("unique-needle-survivor"))
        .expect("search ok");
    assert!(
        hits.iter()
            .any(|h| matches!(h, Hit::Message { message, .. } if message.id.as_str() == "1")),
        "the failed call's own staged delete must not survive it: {hits:?}",
    );
}

/// **The same proof, on the sessions half.** Amending a beat stages the
/// old posting's delete before writing the new one, exactly as a mail
/// rewrite does; a failed amend must not leave that delete for a later,
/// unrelated commit to apply.
#[tokio::test]
async fn a_failed_session_entry_write_does_not_leave_its_staged_delete_for_a_later_commit() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[run_of(
            "s-gamma",
            "bot:gamma",
            &[
                ("e1", "the damper is hand-cut"),
                ("e2", "original beat unique-needle-survivor"),
                ("e3", "the glaze is mixed"),
            ],
        )])
        .expect("the first ingest lands");
    assert!(
        finds(&index, "bot:gamma", "unique-needle-survivor"),
        "findable before anything goes wrong",
    );

    // Amending e2 stages its old posting's delete, then the write meant
    // to replace it is made to fail.
    index.fail_add_document_at(3);
    let failed = index.ingest_sessions(&[run_of(
        "s-gamma",
        "bot:gamma",
        &[
            ("e1", "the damper is hand-cut"),
            ("e2", "rewritten beat unique-needle-rewrite"),
            ("e3", "the glaze is mixed"),
        ],
    )]);
    assert!(
        failed.is_err(),
        "the injected fault must surface: {failed:?}"
    );

    // An unrelated, entirely successful ingest — a different run
    // entirely, alongside this run's own unchanged report of its
    // ORIGINAL entries. `ingest_sessions_changes` reads a full board
    // state each call, so leaving s-gamma out here would evict it
    // legitimately, a different question from the one this case asks.
    index
        .ingest_sessions(&[
            run_of(
                "s-gamma",
                "bot:gamma",
                &[
                    ("e1", "the damper is hand-cut"),
                    ("e2", "original beat unique-needle-survivor"),
                    ("e3", "the glaze is mixed"),
                ],
            ),
            run_of(
                "s-other",
                "bot:otto",
                &[("f1", "an unrelated beat unique-needle-other")],
            ),
        ])
        .expect("the unrelated ingest lands");

    assert!(
        finds(&index, "bot:gamma", "unique-needle-survivor"),
        "the failed call's own staged delete must not survive it",
    );
}

/// **The same proof, on the memory half's batch path.** `ingest_changes`
/// stages a document's delete before rewriting it; a failed rewrite must
/// not leave that delete for a later, unrelated commit to apply.
#[tokio::test]
async fn a_failed_doc_write_does_not_leave_its_staged_delete_for_a_later_commit() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_all(
            &[scan(
                "doc-1",
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![fact(
                    "person:alpha",
                    "f1",
                    "keeps a ferret unique-needle-survivor",
                    date(2026, 1, 1),
                )],
            )],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("the first ingest lands");
    assert_eq!(
        index
            .search(&SearchQuery::text("unique-needle-survivor"))
            .expect("search ok")
            .len(),
        1,
        "findable before anything goes wrong",
    );

    // Rewriting it stages the old posting's delete, then the write that
    // was meant to replace it is made to fail.
    index.fail_add_document_at(2);
    let failed = index.ingest_all(
        &[scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "keeps a tortoise unique-needle-rewrite",
                date(2026, 1, 1),
            )],
        )],
        index.reading_begins(),
        &Default::default(),
    );
    assert!(
        failed.is_err(),
        "the injected fault must surface: {failed:?}"
    );

    // An unrelated, entirely successful ingest — a different document
    // entirely, alongside doc-1's own unchanged, pre-failure state.
    index
        .ingest_all(
            &[
                scan(
                    "doc-1",
                    Some(entity("person:alpha", "Alpha")),
                    "",
                    vec![fact(
                        "person:alpha",
                        "f1",
                        "keeps a ferret unique-needle-survivor",
                        date(2026, 1, 1),
                    )],
                ),
                scan(
                    "doc-2",
                    Some(entity("person:beta", "Beta")),
                    "",
                    vec![fact(
                        "person:beta",
                        "f2",
                        "an unrelated fact unique-needle-other",
                        date(2026, 1, 1),
                    )],
                ),
            ],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("the unrelated ingest lands");

    let hits = index
        .search(&SearchQuery::text("unique-needle-survivor"))
        .expect("search ok");
    assert!(
        hits.iter()
            .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.id.as_str() == "f1")),
        "the failed call's own staged delete must not survive it: {hits:?}",
    );
}

/// **The same proof, on `ingest_doc`'s single-item path.** No board diff
/// to get wrong here — the fault sits in the same shared writer, so the
/// failed call's staged delete is exactly as reachable by a later,
/// unrelated commit.
#[tokio::test]
async fn a_failed_single_doc_write_does_not_leave_its_staged_delete_for_a_later_commit() {
    let index = FullTextIndex::open().expect("index opens");
    let none = FactHistoryTerms::new();
    index
        .ingest_doc(
            &scan(
                "doc-1",
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![fact(
                    "person:alpha",
                    "f1",
                    "keeps a ferret unique-needle-survivor",
                    date(2026, 1, 1),
                )],
            ),
            &none,
        )
        .expect("the first write lands");
    assert_eq!(
        index
            .search(&SearchQuery::text("unique-needle-survivor"))
            .expect("search ok")
            .len(),
        1,
        "findable before anything goes wrong",
    );

    index.fail_add_document_at(2);
    let failed = index.ingest_doc(
        &scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "keeps a tortoise unique-needle-rewrite",
                date(2026, 1, 1),
            )],
        ),
        &none,
    );
    assert!(
        failed.is_err(),
        "the injected fault must surface: {failed:?}"
    );

    index
        .ingest_doc(
            &scan(
                "doc-2",
                Some(entity("person:beta", "Beta")),
                "",
                vec![fact(
                    "person:beta",
                    "f2",
                    "an unrelated fact unique-needle-other",
                    date(2026, 1, 1),
                )],
            ),
            &none,
        )
        .expect("the unrelated write lands");

    let hits = index
        .search(&SearchQuery::text("unique-needle-survivor"))
        .expect("search ok");
    assert!(
        hits.iter()
            .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.id.as_str() == "f1")),
        "the failed call's own staged delete must not survive it: {hits:?}",
    );
}

/// The ids of the messages a mail search returns for one distinctive word.
fn mail_ids_for(index: &FullTextIndex, needle: &str) -> Vec<String> {
    index
        .search(&asking_for_mail(needle))
        .expect("search ok")
        .iter()
        .filter_map(|h| match h {
            Hit::Message { message, .. } => Some(message.id.as_str().to_string()),
            _ => None,
        })
        .collect()
}

/// The ids of the facts a memory search returns for one distinctive word.
fn fact_ids_for(index: &FullTextIndex, needle: &str) -> Vec<String> {
    index
        .search(&SearchQuery::text(needle))
        .expect("search ok")
        .iter()
        .filter_map(|h| match h {
            Hit::Fact { fact, .. } => Some(fact.id.as_str().to_string()),
            _ => None,
        })
        .collect()
}

/// **A commit that fails must not leave its staged operations for the next
/// commit.** The fault is injected on `commit` itself, after staging has
/// succeeded: the rewrite's delete and its new document are both in the
/// shared writer, uncommitted. Every case below follows the failed call with
/// an unrelated commit and asserts three things: the original document
/// survives, the failed call's rewrite is absent, and the unrelated call's
/// own document is there — the last so the other two cannot pass over an
/// empty index.
#[tokio::test]
async fn a_failed_mail_batch_commit_leaves_nothing_for_a_later_commit() {
    let index = index_of(vec![]);
    let original = message("1", "pm", "dev", None, "original quokka", MessageState::New);
    index
        .ingest_mail(std::slice::from_ref(&original))
        .expect("the first ingest lands");
    assert_eq!(mail_ids_for(&index, "quokka"), ["1"]);

    index.fail_next_commit();
    let failed = index.ingest_mail(&[message(
        "1",
        "pm",
        "dev",
        None,
        "rewritten pangolin",
        MessageState::New,
    )]);
    assert!(failed.is_err(), "the commit fault must surface: {failed:?}");

    index
        .ingest_mail(&[
            original,
            message(
                "2",
                "pm",
                "dev",
                None,
                "unrelated axolotl",
                MessageState::New,
            ),
        ])
        .expect("the unrelated ingest lands");

    assert_eq!(mail_ids_for(&index, "axolotl"), ["2"]);
    assert_eq!(mail_ids_for(&index, "quokka"), ["1"]);
    assert!(mail_ids_for(&index, "pangolin").is_empty());
}

#[tokio::test]
async fn a_failed_single_message_commit_leaves_nothing_for_a_later_commit() {
    let index = index_of(vec![]);
    index
        .ingest_message(&message(
            "1",
            "pm",
            "dev",
            None,
            "original quokka",
            MessageState::New,
        ))
        .expect("the first write lands");
    assert_eq!(mail_ids_for(&index, "quokka"), ["1"]);

    index.fail_next_commit();
    let failed = index.ingest_message(&message(
        "1",
        "pm",
        "dev",
        None,
        "rewritten pangolin",
        MessageState::New,
    ));
    assert!(failed.is_err(), "the commit fault must surface: {failed:?}");

    index
        .ingest_message(&message(
            "2",
            "pm",
            "dev",
            None,
            "unrelated axolotl",
            MessageState::New,
        ))
        .expect("the unrelated write lands");

    assert_eq!(mail_ids_for(&index, "axolotl"), ["2"]);
    assert_eq!(mail_ids_for(&index, "quokka"), ["1"]);
    assert!(mail_ids_for(&index, "pangolin").is_empty());
}

#[tokio::test]
async fn a_failed_session_commit_leaves_nothing_for_a_later_commit() {
    let index = FullTextIndex::open().expect("index opens");
    let original = run_of("s-gamma", "bot:gamma", &[("e1", "original quokka")]);
    index
        .ingest_sessions(std::slice::from_ref(&original))
        .expect("the first ingest lands");
    assert!(finds(&index, "bot:gamma", "quokka"));

    index.fail_next_commit();
    let failed = index.ingest_sessions(&[run_of(
        "s-gamma",
        "bot:gamma",
        &[("e1", "rewritten pangolin")],
    )]);
    assert!(failed.is_err(), "the commit fault must surface: {failed:?}");

    index
        .ingest_sessions(&[
            original,
            run_of("s-other", "bot:otto", &[("f1", "unrelated axolotl")]),
        ])
        .expect("the unrelated ingest lands");

    assert!(finds(&index, "bot:otto", "axolotl"));
    assert!(finds(&index, "bot:gamma", "quokka"));
    assert!(!finds(&index, "bot:gamma", "pangolin"));
}

fn alpha_scan(content: &str) -> DocScan {
    scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact("person:alpha", "f1", content, date(2026, 1, 1))],
    )
}

fn beta_scan() -> DocScan {
    scan(
        "doc-2",
        Some(entity("person:beta", "Beta")),
        "",
        vec![fact(
            "person:beta",
            "f2",
            "unrelated axolotl",
            date(2026, 1, 1),
        )],
    )
}

#[tokio::test]
async fn a_failed_doc_batch_commit_leaves_nothing_for_a_later_commit() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_all(
            &[alpha_scan("original quokka")],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("the first ingest lands");
    assert_eq!(fact_ids_for(&index, "quokka"), ["f1"]);

    index.fail_next_commit();
    let failed = index.ingest_all(
        &[alpha_scan("rewritten pangolin")],
        index.reading_begins(),
        &Default::default(),
    );
    assert!(failed.is_err(), "the commit fault must surface: {failed:?}");

    index
        .ingest_all(
            &[alpha_scan("original quokka"), beta_scan()],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("the unrelated ingest lands");

    assert_eq!(fact_ids_for(&index, "axolotl"), ["f2"]);
    assert_eq!(fact_ids_for(&index, "quokka"), ["f1"]);
    assert!(fact_ids_for(&index, "pangolin").is_empty());
}

#[tokio::test]
async fn a_failed_single_doc_commit_leaves_nothing_for_a_later_commit() {
    let index = FullTextIndex::open().expect("index opens");
    let none = FactHistoryTerms::new();
    index
        .ingest_doc(&alpha_scan("original quokka"), &none)
        .expect("the first write lands");
    assert_eq!(fact_ids_for(&index, "quokka"), ["f1"]);

    index.fail_next_commit();
    let failed = index.ingest_doc(&alpha_scan("rewritten pangolin"), &none);
    assert!(failed.is_err(), "the commit fault must surface: {failed:?}");

    index
        .ingest_doc(&beta_scan(), &none)
        .expect("the unrelated write lands");

    assert_eq!(fact_ids_for(&index, "axolotl"), ["f2"]);
    assert_eq!(fact_ids_for(&index, "quokka"), ["f1"]);
    assert!(fact_ids_for(&index, "pangolin").is_empty());
}

/// `forget` stages only a delete, so the leftover would drop the document
/// the failed call meant to evict from a commit that never named it.
#[tokio::test]
async fn a_failed_forget_commit_leaves_nothing_for_a_later_commit() {
    let index = FullTextIndex::open().expect("index opens");
    let none = FactHistoryTerms::new();
    index
        .ingest_doc(&alpha_scan("original quokka"), &none)
        .expect("the first write lands");
    assert_eq!(fact_ids_for(&index, "quokka"), ["f1"]);

    index.fail_next_commit();
    let failed = index.forget(&EntityId("person:alpha".into()));
    assert!(failed.is_err(), "the commit fault must surface: {failed:?}");

    index
        .ingest_doc(&beta_scan(), &none)
        .expect("the unrelated write lands");

    assert_eq!(fact_ids_for(&index, "axolotl"), ["f2"]);
    assert_eq!(fact_ids_for(&index, "quokka"), ["f1"]);
}

/// **Every state is searchable, `processed` included** — an archive is
/// exactly where an old report lives, and the state is on the hit, so a
/// caller can tell live work from history without a second call.
#[tokio::test]
async fn mail_is_searchable_in_every_state_and_the_hit_says_which() {
    let index = index_of(Vec::new());
    index
        .ingest_mail(&[
            message(
                "1",
                "pm",
                "dev",
                None,
                "the crates are stacked",
                MessageState::New,
            ),
            message(
                "2",
                "pm",
                "dev",
                None,
                "the crates were counted",
                MessageState::Read,
            ),
            message(
                "3",
                "pm",
                "dev",
                None,
                "the crates went out",
                MessageState::Processed,
            ),
        ])
        .expect("ingest mail");

    let hits = index.search(&asking_for_mail("crates")).expect("search ok");
    let mut states: Vec<&str> = hits
        .iter()
        .filter_map(|h| match h {
            Hit::Message { message, .. } => Some(message.state.as_token()),
            _ => None,
        })
        .collect();
    states.sort_unstable();
    assert_eq!(
        states,
        vec!["new", "processed", "read"],
        "an archived message is still findable: {hits:?}"
    );
}

/// Mail is in by default and out when the caller says so — a parameter, not
/// a mode. Excluding it must not touch the memory half of the answer.
#[tokio::test]
async fn include_mail_is_a_filter_the_caller_holds() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact(
            "person:alpha",
            "f1",
            "the shipment is late",
            date(2026, 1, 1),
        )],
    )]);
    index
        .ingest_mail(&[message(
            "7",
            "pm",
            "dev",
            None,
            "the shipment is late again",
            MessageState::New,
        )])
        .expect("ingest mail");

    // **Mail is opt-in.** A caller that has not asked for it does not get
    // somebody's message back from the verb the surface tells it to reach
    // for first (rule 62).
    let by_default = index
        .search(&SearchQuery::text("shipment"))
        .expect("search ok");
    assert!(
        !by_default.iter().any(|h| matches!(h, Hit::Message { .. })),
        "mail is not searched unless it is asked for: {by_default:?}"
    );
    assert!(
        by_default.iter().any(|h| matches!(h, Hit::Fact { .. })),
        "…and the memory half is untouched by that: {by_default:?}"
    );

    // Its pair: asked for, it is there. Without this the assertion above
    // passes on an index that cannot return a message at all.
    let asked = index
        .search(&SearchQuery {
            include_mail: true,
            ..SearchQuery::text("shipment")
        })
        .expect("search ok");
    assert!(
        asked.iter().any(|h| matches!(h, Hit::Message { .. })),
        "asked for, mail is in the one list: {asked:?}"
    );
}

/// A fact-only filter still returns facts alone, and a kind filter is a
/// question about entities — a message has neither a lifecycle nor a kind,
/// so it is out of both answers exactly as nobody's prose is.
#[tokio::test]
async fn a_structural_filter_leaves_mail_out_the_way_it_leaves_prose_out() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "Alpha wrote about the shipment.",
        vec![fact(
            "person:alpha",
            "f1",
            "the shipment is late",
            date(2026, 1, 1),
        )],
    )]);
    index
        .ingest_mail(&[message(
            "7",
            "pm",
            "dev",
            None,
            "the shipment is late",
            MessageState::New,
        )])
        .expect("ingest mail");

    let fact_scoped = index
        .search(&SearchQuery {
            provenance: Some(Provenance::Inference),
            ..SearchQuery::text("shipment")
        })
        .expect("search ok");
    assert!(!fact_scoped.is_empty());
    assert!(
        fact_scoped.iter().all(|h| matches!(h, Hit::Fact { .. })),
        "a fact-only filter must not surface mail either: {fact_scoped:?}"
    );

    let by_kind = index
        .search(&SearchQuery {
            kind: Some(EntityKind::PERSON),
            ..SearchQuery::text("shipment")
        })
        .expect("search ok");
    assert!(
        !by_kind.iter().any(|h| matches!(h, Hit::Message { .. })),
        "a message has no entity kind, so asking for one excludes it: {by_kind:?}"
    );
}

/// **The projection is a projection here too.** A re-ingest replaces the
/// mail half wholesale — a message that has since been processed must not
/// come back beside its own older copy — and it leaves memory alone.
#[tokio::test]
async fn re_ingesting_mail_replaces_it_and_leaves_memory_alone() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact(
            "person:alpha",
            "f1",
            "keeps a ferret",
            date(2026, 1, 1),
        )],
    )]);
    index
        .ingest_mail(&[message(
            "1",
            "pm",
            "dev",
            None,
            "the shipment landed",
            MessageState::New,
        )])
        .expect("ingest mail");
    index
        .ingest_mail(&[message(
            "1",
            "pm",
            "dev",
            None,
            "the shipment landed",
            MessageState::Processed,
        )])
        .expect("re-ingest mail");

    let hits = index
        .search(&asking_for_mail("shipment"))
        .expect("search ok");
    let states: Vec<&str> = hits
        .iter()
        .filter_map(|h| match h {
            Hit::Message { message, .. } => Some(message.state.as_token()),
            _ => None,
        })
        .collect();
    assert_eq!(
        states,
        vec!["processed"],
        "one copy, saying the new thing: {hits:?}"
    );
    assert_eq!(
        index
            .search(&asking_for_mail("ferret"))
            .expect("search ok")
            .len(),
        1,
        "rebuilding the mail half must not evict memory"
    );
}

/// One message, re-indexed in place — the read-back that makes a posted
/// message findable on the next call rather than after a restart.
#[tokio::test]
async fn one_message_is_re_indexed_in_place() {
    let index = index_of(Vec::new());
    index
        .ingest_mail(&[message(
            "1",
            "pm",
            "dev",
            None,
            "the shipment landed",
            MessageState::New,
        )])
        .expect("ingest mail");
    index
        .ingest_message(&message(
            "1",
            "pm",
            "dev",
            None,
            "the shipment landed",
            MessageState::Processed,
        ))
        .expect("ingest one");

    let hits = index
        .search(&asking_for_mail("shipment"))
        .expect("search ok");
    assert_eq!(hits.len(), 1, "one copy, not two: {hits:?}");
    assert!(
        matches!(hits.first(), Some(Hit::Message { message, .. }) if message.state == MessageState::Processed)
    );
}

/// **A mailbox world that never loaded says so.** An index with no mail in
/// it answers memory questions exactly as before — degrade, don't error —
/// but it must not let "no message says that" stand in for "jojobot has
/// read no messages", which is a different claim and the one a caller would
/// act on wrongly.
#[tokio::test]
async fn an_index_with_no_mail_says_mail_is_not_searchable() {
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "",
        vec![fact(
            "person:alpha",
            "f1",
            "keeps a ferret",
            date(2026, 1, 1),
        )],
    )]);
    assert_eq!(
        index.mail_coverage(),
        Coverage::Unread,
        "nothing has loaded mail"
    );
    assert_eq!(
        index
            .search(&SearchQuery::text("ferret"))
            .expect("search ok")
            .len(),
        1,
        "the memory half still answers"
    );

    index
        .ingest_mail(&[])
        .expect("an empty board is still a board");
    assert_eq!(
        index.mail_coverage(),
        Coverage::Loaded,
        "a board that was read and holds nothing is not the same as a board nobody read"
    );
}

/// **The state a failed boot actually leaves.** The board read never
/// happened, but every message this process posts or delivers is still
/// indexed and still comes back as a hit — so reporting "no mail is
/// searchable" made one answer carry message hits and deny having searched
/// any. That is a third state, not one of the two.
#[tokio::test]
async fn mail_indexed_after_a_failed_board_read_is_partial_not_absent() {
    let index = index_of(Vec::new());
    assert_eq!(index.mail_coverage(), Coverage::Unread);

    // No ingest_mail — the boot read failed. A verb indexes one message.
    index
        .ingest_message(&message(
            "1",
            "pm",
            "dev",
            None,
            "the shipment landed",
            MessageState::New,
        ))
        .expect("ingest one");

    assert_eq!(
        index.mail_coverage(),
        Coverage::Partial(Behind::Unscanned),
        "a message that IS findable must never be reported as no mail at all"
    );
    assert_eq!(
        index
            .search(&asking_for_mail("shipment"))
            .expect("search ok")
            .len(),
        1,
        "…and it is findable, which is the whole reason the claim was wrong"
    );

    // A board read later promotes it: now everything is there.
    index.ingest_mail(&[]).expect("the board comes back");
    assert_eq!(index.mail_coverage(), Coverage::Loaded);
}

/// **Rebuilding one half must not empty the other.** `ingest_all` wiped the
/// whole index — mail included — while leaving the flag saying mail was
/// loaded, so a Memory rebuild silently emptied the mail half and then
/// vouched for it. Nothing but the boot ordering in `main.rs` stood between
/// that and production, and boot ordering is not an invariant.
///
/// Both orders, because the bug is exactly an order-dependence.
#[tokio::test]
async fn rebuilding_either_half_leaves_the_other_alone() {
    let mail = || {
        message(
            "1",
            "pm",
            "dev",
            None,
            "the shipment landed",
            MessageState::New,
        )
    };
    let docs = || {
        vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "keeps a ferret",
                date(2026, 1, 1),
            )],
        )]
    };
    let both_survive = |index: &FullTextIndex, order: &str| {
        assert_eq!(
            index
                .search(&asking_for_mail("shipment"))
                .expect("search ok")
                .len(),
            1,
            "the mail half is gone after {order}"
        );
        assert_eq!(
            index
                .search(&asking_for_mail("ferret"))
                .expect("search ok")
                .len(),
            1,
            "the memory half is gone after {order}"
        );
        assert_eq!(
            index.mail_coverage(),
            Coverage::Loaded,
            "…and the coverage claim still matches what is actually in there"
        );
    };

    let mail_first = FullTextIndex::open().expect("index opens");
    mail_first.ingest_mail(&[mail()]).expect("ingest mail");
    mail_first
        .ingest_all(&docs(), mail_first.reading_begins(), &Default::default())
        .expect("ingest docs");
    both_survive(&mail_first, "mail then memory");

    let memory_first = FullTextIndex::open().expect("index opens");
    memory_first
        .ingest_all(&docs(), memory_first.reading_begins(), &Default::default())
        .expect("ingest docs");
    memory_first.ingest_mail(&[mail()]).expect("ingest mail");
    both_survive(&memory_first, "memory then mail");
}

/// **Read-back covers mail too.** A message posted a moment ago is findable
/// on the next call, without a restart — writing a message search cannot
/// find is the same class of failure as writing a fact `recall` cannot
/// return. And the state on the hit follows the message: once it is
/// processed, the hit says so, because a reader acts on that word.
#[tokio::test]
async fn a_posted_message_is_findable_at_once_and_its_state_follows_it() {
    jojobot_domain::memory::kinds::load_shipped();
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let store = IndexedMailboxes::new(Arc::new(InMemoryMailboxes::new()), index.clone());
    mail_contract::create(&store, "pm").await;

    let posted = mail_contract::post(&store, "pm", "dev", "the damper is still hand-cut", 0).await;
    let state_of = |index: &FullTextIndex| -> Option<MessageState> {
        index
            .search(&asking_for_mail("damper"))
            .expect("search ok")
            .iter()
            .find_map(|h| match h {
                Hit::Message { message, .. } => Some(message.state),
                _ => None,
            })
    };
    assert_eq!(
        state_of(&index),
        Some(MessageState::New),
        "a posted message is findable before anything rebuilds"
    );

    store
        .mark_processed(&posted.id, Some("filed"))
        .await
        .expect("mark_processed ok");
    assert_eq!(
        state_of(&index),
        Some(MessageState::Processed),
        "the hit's state follows the message, or a reader acts on a stale word"
    );
}

/// **A store that says how often the boxes were listed, and can stop listing
/// them** while everything else works. `list_mailboxes` hands back every box
/// with its counts, so how many times one verb reaches for it is the difference
/// between one tally shared by the messages that verb touched and a tally per
/// message. `scan_messages` is a separate read in the real store and fails on
/// its own, so the listing fails on its own here too.
struct Metered {
    inner: InMemoryMailboxes,
    listings: std::sync::atomic::AtomicUsize,
    listing_down: std::sync::atomic::AtomicBool,
}

impl Metered {
    fn new(inner: InMemoryMailboxes) -> Arc<Self> {
        Arc::new(Metered {
            inner,
            listings: std::sync::atomic::AtomicUsize::new(0),
            listing_down: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn listings(&self) -> usize {
        self.listings.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn stop_listing(&self) {
        self.listing_down
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl jojobot_domain::mailbox::Mailboxes for Metered {
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &jojobot_domain::memory::EntityId,
        token: Option<&str>,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError>
    {
        self.inner.create_mailbox(name, owner, token).await
    }
    async fn repoint_owner(
        &self,
        from: &jojobot_domain::memory::EntityId,
        to: &jojobot_domain::memory::EntityId,
    ) -> Result<Option<jojobot_domain::mailbox::Mailbox>, MailboxError> {
        self.inner.repoint_owner(from, to).await
    }
    async fn list_mailboxes(&self) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
        self.listings
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.listing_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(MailboxError::Store("the boxes cannot be listed".into()));
        }
        self.inner.list_mailboxes().await
    }
    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.inner.scan_messages().await
    }
    async fn message_by_id(&self, id: &MessageId) -> Result<Option<Message>, MailboxError> {
        self.inner.message_by_id(id).await
    }
    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        self.inner.sent_by(senders).await
    }
    async fn post_message(
        &self,
        message: jojobot_domain::mailbox::NewMessage,
    ) -> Result<jojobot_domain::mailbox::Guarded<Message>, MailboxError> {
        self.inner.post_message(message).await
    }
    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: jojobot_domain::mailbox::TakenBy,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Delivery>, MailboxError>
    {
        self.inner.read_mailbox(name, taken_by).await
    }
    async fn read_message(
        &self,
        id: &MessageId,
    ) -> Result<jojobot_domain::mailbox::Delivered, MailboxError> {
        self.inner.read_message(id).await
    }
    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        self.inner.mark_processed(id, notes).await
    }
    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        at: jiff::Timestamp,
    ) -> Result<jojobot_domain::mailbox::Quarantined, MailboxError> {
        self.inner.quarantine(id, by, reason, at).await
    }
}

/// **A verb that touches several messages lists the boxes once.** The listing
/// is a tally of every box, so a listing per message made one delivery cost the
/// boxes times the messages. Paired with the positive it depends on: every
/// delivered message is still indexed with its new state, and a single-message
/// verb still pays one listing.
#[tokio::test]
async fn a_delivery_lists_the_boxes_once_however_many_messages_it_carries() {
    jojobot_domain::memory::kinds::load_shipped();
    let metered = Metered::new(InMemoryMailboxes::new());
    mail_contract::create(metered.as_ref(), "pm").await;
    let mut ids = Vec::new();
    for (n, word) in ["kiln", "glaze", "anvil", "ledger", "orchard"]
        .iter()
        .enumerate()
    {
        let body = format!("the {word} is relined");
        ids.push(
            mail_contract::post(metered.as_ref(), "pm", "dev", &body, n as i64)
                .await
                .id,
        );
    }

    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let mail = IndexedMailboxes::new(metered.clone(), index.clone());
    mail.rebuild().await.expect("rebuild");

    let before = metered.listings();
    let delivery = mail
        .read_mailbox(
            &MailboxName("pm".into()),
            jojobot_domain::mailbox::TakenBy::Reading,
        )
        .await
        .expect("delivery ok")
        .written()
        .expect("the box is there");
    assert_eq!(delivery.messages.len(), 5, "five messages were delivered");
    assert_eq!(
        metered.listings() - before,
        1,
        "one listing for the whole delivery, not one per message"
    );
    for word in ["kiln", "glaze", "anvil", "ledger", "orchard"] {
        let states: Vec<MessageState> = index
            .search(&asking_for_mail(word))
            .expect("search ok")
            .iter()
            .filter_map(|h| match h {
                Hit::Message { message, .. } => Some(message.state),
                _ => None,
            })
            .collect();
        assert_eq!(
            states,
            vec![MessageState::Read],
            "{word}: delivered and indexed as read"
        );
    }

    let before = metered.listings();
    mail.mark_processed(&ids[0], Some("filed"))
        .await
        .expect("retire ok");
    assert_eq!(
        metered.listings() - before,
        1,
        "a verb on one message still lists once"
    );
}

/// **A reindex that cannot list the boxes says so and leaves the mail half
/// stale.** The message is written and the verb succeeds, but the index could
/// not learn whether its box is a person's, so it holds an older board than the
/// store. Skipping it silently would leave coverage claiming `Loaded` over a
/// board the index is behind.
#[tokio::test]
async fn a_reindex_that_cannot_list_the_boxes_logs_why_and_marks_the_mail_half_stale() {
    jojobot_domain::memory::kinds::load_shipped();
    let logged = log_sink();
    let metered = Metered::new(InMemoryMailboxes::new());
    mail_contract::create(metered.as_ref(), "ledger-down").await;

    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let mail = IndexedMailboxes::new(metered.clone(), index.clone());
    mail.rebuild().await.expect("rebuild");
    assert_eq!(
        index.mail_coverage(),
        Coverage::Loaded,
        "the board was read, so coverage starts complete"
    );

    metered.stop_listing();
    let posted =
        mail_contract::post(&mail, "ledger-down", "dev", "the damper is hand-cut", 0).await;

    assert_eq!(
        index.mail_coverage(),
        Coverage::Partial(Behind::Stale),
        "the post landed and the index could not place it, so the half is behind"
    );
    let text = logged.text();
    assert!(
        text.contains("the boxes cannot be listed"),
        "the log carries the reason the listing failed: {text}"
    );
    assert!(
        text.contains(posted.mailbox.as_str()),
        "…and names the box the message was filed in: {text}"
    );
    assert!(
        index
            .search(&asking_for_mail("damper"))
            .expect("search ok")
            .is_empty(),
        "a message the index could not place stays out of it until a board read lands"
    );
}

/// **A merge that loses its source segment to a commit logs tantivy's warning
/// and loses no document.** Tantivy merges small segments in the background. A
/// commit that deletes every document of a segment a merge is reading removes
/// that segment from the segment manager, so when the merge ends, the manager
/// cannot find the segments it merged. Tantivy then logs two warnings — the
/// segment ids it holds, then "couldn't find segment in SegmentManager" — and
/// drops the merged result. The segments it was made from are still there.
///
/// **This case is both halves of the question.** The warning has to appear, or
/// nothing was reproduced; and afterwards every document written is found, and
/// every document deleted is not, so the race is shown to cost no document. It
/// writes through the index's own mail path, which commits the way every write
/// here commits. The race is a matter of timing, so it writes rounds until the
/// warning shows, up to a bound.
#[test]
fn a_merge_that_loses_its_source_segment_logs_a_warning_and_loses_no_document() {
    use tantivy::collector::Count;
    use tantivy::query::TermQuery;
    use tantivy::schema::IndexRecordOption;

    const RACE: &str = "couldn't find segment in SegmentManager";
    const BATCH: usize = 1000;
    let logged = log_sink();
    let before = logged.text().matches(RACE).count();
    let index = FullTextIndex::open().expect("index opens");

    let mut live: Vec<Message> = Vec::new();
    let mut deleted: Vec<Message> = Vec::new();
    let mut rounds = 0;
    while logged.text().matches(RACE).count() == before {
        assert!(
            rounds < 30,
            "no merge lost its source segment in {rounds} rounds, so nothing was reproduced"
        );
        live.clear();
        for batch in 0..8 {
            for n in 0..BATCH {
                live.push(message(
                    &format!("{rounds}-{batch}-{n}"),
                    "pm",
                    "dev",
                    None,
                    "the kiln is relined and the damper is hand cut",
                    MessageState::New,
                ));
            }
            index
                .ingest_mail_changes(&live, index.reading_begins())
                .expect("ingest");
        }
        // The first batch goes, so its segment has no live document left.
        deleted = live.drain(0..BATCH).collect();
        index
            .ingest_mail_changes(&live, index.reading_begins())
            .expect("ingest");
        rounds += 1;
    }

    let text = logged.text();
    assert!(
        text.contains("segment_ids:"),
        "the warning comes after the list of segment ids tantivy holds: {text}"
    );
    let searcher = index.reader.searcher();
    let found = |message: &Message| {
        let term = Term::from_field_text(index.fields.message_id, message.id.as_str());
        searcher
            .search(&TermQuery::new(term, IndexRecordOption::Basic), &Count)
            .expect("search ok")
    };
    let missing: Vec<&str> = live
        .iter()
        .filter(|message| found(message) != 1)
        .map(|message| message.id.as_str())
        .take(5)
        .collect();
    assert!(
        missing.is_empty(),
        "documents written and not found after the race, first five: {missing:?}"
    );
    assert_eq!(
        searcher.num_docs(),
        live.len() as u64,
        "the index holds exactly the live documents"
    );
    assert!(
        deleted.iter().all(|message| found(message) == 0),
        "a document deleted before the race is not found after it"
    );
}

/// **The version of tantivy the merge-warning filter was written against.**
/// [`is_the_benign_merge_race`] drops every warning from tantivy's segment
/// manager module, which is right only while that module raises just the two
/// warnings of the merge race. That is a fact about this version, and nothing
/// else would redden when the version moves.
const TANTIVY_READ_AT: &str = "0.25.0";

/// **A tantivy bump fails here until somebody has read the warnings again.**
/// The filter hides every warning from `tantivy::indexer::segment_manager`. A
/// newer tantivy may raise a warning there that is a fault, and the filter
/// would hide it. So this case reads the locked version from `Cargo.lock` and
/// fails on any other one. Before changing the expected version, re-read every
/// `warn!` in that version's `src/indexer/segment_manager.rs`: if there is any
/// besides the two about a merge that lost its source segments, narrow the
/// filter first.
#[test]
fn the_merge_warning_filter_is_checked_again_when_tantivy_is_bumped() {
    let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"))
        .expect("the workspace lockfile");
    let lines: Vec<&str> = lock.lines().collect();
    let version = lines
        .windows(2)
        .find(|pair| pair[0] == "name = \"tantivy\"")
        .and_then(|pair| pair[1].strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("the lockfile locks tantivy");
    assert_eq!(
        version, TANTIVY_READ_AT,
        "tantivy is locked at {version}, and the filter for its merge warning was written \
         against {TANTIVY_READ_AT}. Re-read the warn! calls in that version's \
         src/indexer/segment_manager.rs. If there are more than the two about a merge that \
         lost its source segments, narrow is_the_benign_merge_race before you change \
         TANTIVY_READ_AT."
    );
}

/// A board with two messages behind the `search` port, ready to lose one.
/// Two rather than one so every negative below has a survivor to pair with.
async fn a_board_of_two() -> (Arc<InMemoryMailboxes>, Arc<IndexedMailboxes>, Retrieval) {
    jojobot_domain::memory::kinds::load_shipped();
    let inner = Arc::new(InMemoryMailboxes::new());
    mail_contract::create(inner.as_ref(), "pm").await;
    mail_contract::post(inner.as_ref(), "pm", "dev", "the kiln is relined", 0).await;
    mail_contract::post(inner.as_ref(), "pm", "dev", "the damper is hand-cut", 1).await;

    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let mail = Arc::new(IndexedMailboxes::new(inner.clone(), index.clone()));
    mail.rebuild().await.expect("rebuild");
    let port = Retrieval::new(index, vec![mail.clone()]);
    (inner, mail, port)
}

/// **Mail in a person's box never enters the index, by any route.** Search
/// with mail returns hits from every bot's box by design, so a person's box is
/// excluded where the index is WRITTEN: its text is not in the postings, and no
/// filter on hits has to remember. Three routes write the index, and each is
/// exercised: the boot load, a post through the verb, and the verbs that change
/// a message afterwards (read, retire). A public message beside it is served
/// after every one, so an index that served nothing would not pass.
#[tokio::test]
async fn mail_in_a_persons_box_never_enters_the_index() {
    jojobot_domain::memory::kinds::load_shipped();
    let inner = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let held = MailboxName("person-milhouse".into());
    inner
        .create_mailbox(
            &held,
            &jojobot_domain::memory::EntityId("person:milhouse".into()),
            None,
        )
        .await
        .expect("the store opens it");
    mail_contract::create(inner.as_ref(), "pm").await;
    // Written before the index existed, so the boot load is the route.
    mail_contract::post(
        inner.as_ref(),
        "person-milhouse",
        "dev",
        "the zebra figure is hidden",
        0,
    )
    .await;
    mail_contract::post(inner.as_ref(), "pm", "dev", "the kiln is relined", 1).await;

    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let mail = Arc::new(IndexedMailboxes::new(inner.clone(), index.clone()));
    mail.rebuild().await.expect("rebuild");
    let port = Retrieval::new(index.clone(), vec![mail.clone()]);
    // **Through the port, a search refreshes the whole board first**, which
    // would evict a message a verb wrongly indexed and hide the leak. So the
    // routes a verb writes are read straight from the index, before any
    // refresh; the boot load and the refresh are read through the port.
    let served = |word: &'static str| {
        let port = &port;
        async move {
            port.search(&asking_for_mail(word))
                .await
                .expect("search ok")
                .len()
        }
    };
    let hits = |word: &'static str| {
        let index = index.clone();
        async move {
            index
                .search(&asking_for_mail(word))
                .expect("search ok")
                .len()
        }
    };
    assert_eq!(served("kiln").await, 1, "the public message is served");
    assert_eq!(
        served("zebra").await,
        0,
        "the boot load and the refresh left the person's out"
    );

    // A post through the verb.
    let posted = mail_contract::post(
        mail.as_ref(),
        "person-milhouse",
        "dev",
        "the giraffe figure is hidden too",
        2,
    )
    .await;
    mail_contract::post(mail.as_ref(), "pm", "dev", "the damper is hand-cut", 3).await;
    assert_eq!(hits("damper").await, 1, "the public post is served");
    assert_eq!(
        hits("giraffe").await,
        0,
        "a post to the person is not indexed"
    );

    // The verbs that change a message afterwards re-index it, and this one
    // must stay out through each.
    mail.read_message(&posted.id).await.expect("read ok");
    assert_eq!(hits("giraffe").await, 0, "a read does not index it");
    mail.mark_processed(&posted.id, Some("filed"))
        .await
        .expect("retire ok");
    assert_eq!(hits("giraffe").await, 0, "a retirement does not index it");
    assert_eq!(hits("zebra").await, 0, "…and the first one is still out");
    assert_eq!(hits("kiln").await, 1, "…while the public ones are served");
}

/// **A person's box renamed between the scan and the listing is not served.**
/// The index judges each message by the box it sits in, from a listing read
/// after the scan. A person renamed in between leaves the scan holding the old
/// box name and the listing only the new one, so a message is private by a name
/// the listing no longer knows. It must be left out, not treated as public for
/// being unmatched; the next refresh reads both under one name. A message in a
/// box that stays public is served throughout.
#[tokio::test]
async fn a_persons_box_renamed_between_the_scan_and_the_listing_is_not_indexed() {
    jojobot_domain::memory::kinds::load_shipped();
    let board = Board::new(vec![
        message(
            "1",
            "person-milhouse",
            "bot:sigma",
            Some("the quarterly figure"),
            "the zebra figure is hidden",
            MessageState::New,
        ),
        message(
            "2",
            "pm",
            "bot:sigma",
            Some("the kiln"),
            "the kiln is relined",
            MessageState::New,
        ),
    ]);
    // After the scan, the person was renamed: the listing knows the new name
    // and not the one the scan read the message under.
    board.lists_only(vec![
        jojobot_domain::mailbox::Mailbox {
            name: MailboxName("person-ned-flanders".into()),
            owner: jojobot_domain::memory::EntityId("person:ned-flanders".into()),
            counts: Default::default(),
            quarantined: Vec::new(),
        },
        jojobot_domain::mailbox::Mailbox {
            name: MailboxName("pm".into()),
            owner: jojobot_domain::memory::EntityId("bot:omega".into()),
            counts: Default::default(),
            quarantined: Vec::new(),
        },
    ]);
    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let mail = Arc::new(IndexedMailboxes::new(board.clone(), index.clone()));
    mail.rebuild().await.expect("rebuild");
    let port = Retrieval::new(index, vec![mail.clone()]);
    assert_eq!(
        port.search(&asking_for_mail("kiln"))
            .await
            .expect("search ok")
            .len(),
        1,
        "the message in a box that stays public is served"
    );
    assert_eq!(
        port.search(&asking_for_mail("zebra"))
            .await
            .expect("search ok")
            .len(),
        0,
        "a message in a box the listing no longer knows is not served"
    );
}

/// **A message removed from the store stops being served, with no write to
/// prompt it.** Rule 60 puts true removal outside jojobot, so nothing calls
/// a refresh and the read path is the only place that can notice.
#[tokio::test]
async fn a_message_removed_from_the_store_stops_being_served() {
    let (inner, _mail, port) = a_board_of_two().await;
    for present in ["kiln", "damper"] {
        assert_eq!(
            port.search(&asking_for_mail(present))
                .await
                .expect("search ok")
                .len(),
            1,
            "{present:?} is served while both messages are on the board"
        );
    }

    inner.lose(&MessageId("1".into()));

    assert!(
        port.search(&asking_for_mail("kiln"))
            .await
            .expect("search ok")
            .is_empty(),
        "the message left the board, so search must stop serving it"
    );
    assert_eq!(
        port.search(&asking_for_mail("damper"))
            .await
            .expect("search ok")
            .len(),
        1,
        "…and the one still on the board is still served"
    );
}

/// **A message the index learned from a verb leaves it like any other.**
///
/// The board read is not the only way a message enters this index: every
/// verb that writes one re-indexes it, so a message can be served without a
/// board read ever having carried it. Eviction compares a board read against
/// what the postings were written from, so a verb that writes postings and
/// not the mirror puts a message where that comparison cannot see it — the
/// board loses it and it is served for the life of the process.
///
/// **No search between the post and the removal.** A search refreshes from
/// the whole board, which seats the mirror and hides exactly this. The
/// survivor is what proves both messages reached the index.
#[tokio::test]
async fn a_message_indexed_by_a_verb_stops_being_served_when_it_leaves_the_board() {
    let (inner, mail, port) = a_board_of_two().await;
    let posted = mail_contract::post(mail.as_ref(), "pm", "dev", "the glaze is unmixed", 2).await;
    mail_contract::post(mail.as_ref(), "pm", "dev", "the shelves are warped", 3).await;

    inner.lose(&posted.id);

    assert!(
        port.search(&asking_for_mail("glaze"))
            .await
            .expect("search ok")
            .is_empty(),
        "the message left the board, so search must stop serving it — however it \
             got into the index"
    );
    assert_eq!(
        port.search(&asking_for_mail("warped"))
            .await
            .expect("search ok")
            .len(),
        1,
        "…and the message posted beside it is still served, so both really \
             reached the index"
    );
}

/// **A card that has become unreadable is the same absence**, and it costs
/// no machinery of its own: a board read leaves a quarantined card out, so
/// it arrives at the index as a message the scan no longer carries.
#[tokio::test]
async fn a_quarantined_card_stops_being_served_too() {
    let (inner, _mail, port) = a_board_of_two().await;

    inner.quarantine_by_damage(
        &MailboxName("pm".into()),
        &MessageId("1".into()),
        "the card cannot be read",
    );

    assert!(
        port.search(&asking_for_mail("kiln"))
            .await
            .expect("search ok")
            .is_empty(),
        "jojobot cannot read it, so search must not go on serving its content"
    );
    assert_eq!(
        port.search(&asking_for_mail("damper"))
            .await
            .expect("search ok")
            .len(),
        1,
        "…and the readable one beside it is untouched"
    );
}

/// **A card quarantined ON PURPOSE is the same absence as one damaged by
/// storage.** Neither has machinery of its own in the index: both arrive as
/// a message the board read no longer carries.
#[tokio::test]
async fn a_deliberately_quarantined_card_stops_being_served_too() {
    let (_inner, mail, port) = a_board_of_two().await;

    mail.quarantine(
        &MessageId("1".into()),
        &MailboxName("pm".into()),
        "pm decided to quarantine it",
        jiff::Timestamp::now(),
    )
    .await
    .expect("quarantine ok");

    assert!(
        port.search(&asking_for_mail("kiln"))
            .await
            .expect("search ok")
            .is_empty(),
        "quarantined on purpose is still unreadable, so search must not serve it"
    );
    assert_eq!(
        port.search(&asking_for_mail("damper"))
            .await
            .expect("search ok")
            .len(),
        1,
        "…and the readable one beside it is untouched"
    );
}

/// **A search takes no delivery.** Reading IS delivery on the verbs that
/// deliver, so a refresh that used one would drain a box as a side effect of
/// answering a question — a data-loss defect wearing a correctness fix, and
/// the kind a coverage assertion would pass straight over.
#[tokio::test]
async fn a_search_leaves_every_message_state_as_it_found_it() {
    let (inner, _mail, port) = a_board_of_two().await;
    let before = mail_contract::counts(inner.as_ref(), "pm").await;
    assert_eq!(
        before.map(|c| c.new),
        Some(2),
        "both messages start new and nobody has taken them"
    );

    assert_eq!(
        port.search(&asking_for_mail("kiln"))
            .await
            .expect("search ok")
            .len(),
        1,
        "the search really ran and really answered"
    );

    assert_eq!(
        mail_contract::counts(inner.as_ref(), "pm").await,
        before,
        "…and every state is exactly as it was, the untaken ones included"
    );
}

/// **The mail half stops claiming complete while it is serving a board it
/// could not re-read.** Rule 130: the wrong answer is the defect, and the
/// wrong answer that vouches for itself is worse.
#[tokio::test]
async fn mail_coverage_stops_claiming_loaded_when_the_read_cannot_reach_the_store() {
    let (inner, _mail, port) = a_board_of_two().await;
    assert_eq!(
        port.mail_coverage(),
        Coverage::Loaded,
        "a board read that just succeeded is complete coverage"
    );

    inner.blind();

    assert_eq!(
        port.search(&asking_for_mail("kiln"))
            .await
            .expect("search ok")
            .len(),
        1,
        "degrade, don't error: the last good board read still answers"
    );
    assert_eq!(
        port.mail_coverage(),
        Coverage::Partial(Behind::Stale),
        "…and the answer says it is holding an older board than the store"
    );

    inner.sighted();

    assert_eq!(
        port.search(&asking_for_mail("kiln"))
            .await
            .expect("search ok")
            .len(),
        1,
        "the message is still there and still served"
    );
    assert_eq!(
        port.mail_coverage(),
        Coverage::Loaded,
        "a board read that reaches the store again clears the mark"
    );
}

/// A rebuild loads the board that was already there — the boot path — and a
/// blocked post leaves nothing behind, exactly as a blocked capture does.
#[tokio::test]
async fn a_rebuild_loads_the_board_and_a_blocked_post_indexes_nothing() {
    jojobot_domain::memory::kinds::load_shipped();
    let inner = Arc::new(InMemoryMailboxes::new());
    mail_contract::create(inner.as_ref(), "pm").await;
    mail_contract::post(
        inner.as_ref(),
        "pm",
        "dev",
        "written before the server started",
        0,
    )
    .await;

    let index = Arc::new(FullTextIndex::open().expect("index opens"));
    let store = IndexedMailboxes::new(inner, index.clone());
    assert!(
        index
            .search(&asking_for_mail("started"))
            .expect("search ok")
            .is_empty(),
        "nothing is indexed until the board is read"
    );
    assert_eq!(store.rebuild().await.expect("rebuild"), 1);
    assert_eq!(
        index
            .search(&asking_for_mail("started"))
            .expect("search ok")
            .len(),
        1
    );

    let blocked = store
        .post_message(jojobot_domain::mailbox::NewMessage {
            mailbox: MailboxName("pmm".into()),
            body: "should not be indexed".into(),
            subject: None,
            sender: "dev".into(),
            sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        })
        .await
        .expect("a blocked post is a result, not a failure");
    assert!(matches!(
        blocked,
        jojobot_domain::mailbox::Guarded::Blocked { .. }
    ));
    assert!(
        index
            .search(&asking_for_mail("should not be indexed"))
            .expect("search ok")
            .is_empty(),
        "a blocked post must leave nothing in the index either"
    );
}

/// **The payload and the index agree about what is a link — checked, not
/// conventional.**
///
/// An event's references live in the payload and are projected into the
/// index; the row's own edge column knows nothing about them. That split is
/// the right one — the payload is the general form and the edge column the
/// special case left from before fields existed — but a split held together
/// by convention reads fine today and is a bug in eighteen months. So it is
/// an invariant with a test on it: every payload value that is an entity
/// handle is walkable, and every value that is not is not.
///
/// **Whatever its key.** `ref` is the unnamed member of a family, and
/// `mechanic=person:x` is the same link with the key doing the annotating.
/// A projection keyed on the literal word `ref` passes every test written
/// against unnamed references and silently drops every named one.
#[tokio::test]
async fn every_payload_value_that_is_a_handle_is_walkable_and_nothing_else_is() {
    let event = Fact {
        fields: [
            // Named, and it must walk exactly as the unnamed one does.
            ("mechanic".to_string(), "person:milhouse".to_string()),
            // Not handles: these must NOT become edges.
            ("mood".to_string(), "delighted".to_string()),
            ("nearly".to_string(), "person:".to_string()),
        ]
        .into_iter()
        .collect(),
        refs: vec![EntityId("place:north-gorge".into())],
        ..fact("person:alpha", "f1", "the kiln was lit", date(2026, 1, 1))
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "Alpha's page.",
        vec![event.clone()],
    )]);
    let walks = |object: &str| {
        !index
            .search(&SearchQuery {
                edge: Some(EdgeFilter {
                    shape: None,
                    object: EntityId(object.into()),
                }),
                ..Default::default()
            })
            .expect("search ok")
            .is_empty()
    };

    assert!(
        walks("place:north-gorge"),
        "the unnamed reference is walkable"
    );
    assert!(
        walks("person:milhouse"),
        "…and so is the NAMED one, which is the case nothing else here covers"
    );
    // **The "and nothing else" half is asserted in the domain**, by
    // `any_value_that_is_a_handle_is_a_link_whatever_its_key`, and it has
    // to be: a value like "delighted" or "person:" is not a well-formed
    // handle, so the query layer refuses to be asked about it at all. That
    // refusal is the right behaviour and it means the index cannot be
    // interrogated for a link it should never have made — `linked()` is
    // where that boundary is observable.

    // …and they are `connection`s: the link is admitted, never asserted.
    assert!(
        index
            .search(&SearchQuery {
                edge: Some(EdgeFilter {
                    shape: Some(EdgeShape::About),
                    object: EntityId("person:milhouse".into()),
                }),
                ..Default::default()
            })
            .expect("search ok")
            .is_empty(),
        "an event's link must not answer a query for an asserted `about`"
    );
}

/// **A shape and an object are asked about together, because a fact can now
/// carry several edges.**
///
/// The two were independent fields, which was harmless while a fact had at
/// most one edge and is not any more: a fact located at one place and
/// linked by its payload to a person would match `shape=location AND
/// object=that person`. Neither half is wrong on its own, which is exactly
/// why the pair is what gets indexed.
#[tokio::test]
async fn a_shape_filter_does_not_match_a_different_edges_object() {
    let mixed = Fact {
        // **`about`, deliberately.** The kind-constrained shapes cannot
        // demonstrate this: `location` + a person is refused by the query
        // guard before any matching happens, so the confusion is
        // unreachable there. `about` and `connection` both take any kind,
        // which is exactly where an uncorrelated shape and object would
        // quietly answer the wrong question.
        edge: Some(Edge::new(
            EdgeShape::About,
            EntityId("topic:widgets".into()),
        )),
        fields: Default::default(),
        refs: vec![EntityId("person:milhouse".into())],
        ..fact("person:alpha", "f1", "the kiln was lit", date(2026, 1, 1))
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "Alpha's page.",
        vec![mixed],
    )]);
    let ask = |shape, object: &str| {
        index
            .search(&SearchQuery {
                edge: Some(EdgeFilter {
                    shape: Some(shape),
                    object: EntityId(object.into()),
                }),
                ..Default::default()
            })
            .expect("search ok")
    };

    assert!(
        !ask(EdgeShape::About, "topic:widgets").is_empty(),
        "the pair that exists is found"
    );
    assert!(
        !ask(EdgeShape::Connection, "person:milhouse").is_empty(),
        "…and so is the other one"
    );
    assert!(
        ask(EdgeShape::About, "person:milhouse").is_empty(),
        "a shape from one edge must not combine with an object from another"
    );
    assert!(
        ask(EdgeShape::Connection, "topic:widgets").is_empty(),
        "…in either direction"
    );
}

/// **An untyped edge is walkable, or rule 98 is unimplemented.**
///
/// The whole claim of the fifth shape is that deferring what a link MEANS
/// does not defer that it EXISTS. A `connection` that could not be followed
/// would be a note-to-self rather than an edge, and "the pointer is real,
/// only its nature is deferred" would be a sentence with nothing under it.
///
/// **Both ways of asking**, because they fail differently. Asking for the
/// shape by name proves it was indexed under its own token; asking with no
/// shape at all proves it is in the general connected-to answer rather than
/// only findable by somebody who already knew to ask for `connection` —
/// which is the query a reader actually writes when the question is "what
/// touches this?".
#[tokio::test]
async fn an_untyped_edge_is_walkable_like_any_other() {
    let linked = Fact {
        edge: Some(Edge::new(
            EdgeShape::Connection,
            EntityId("place:shelbyville".into()),
        )),
        ..fact(
            "person:alpha",
            "f1",
            "was there when it happened",
            date(2026, 1, 1),
        )
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "Alpha's page.",
        vec![linked.clone()],
    )]);
    let alpha = EntityRef::resolved(&entity("person:alpha", "Alpha"));
    let expected = vec![Hit::Fact {
        fact: Box::new(linked),
        subject: alpha.clone(),
        home: alpha,
        source: None,
    }];

    for shape in [Some(EdgeShape::Connection), None] {
        let hits = index
            .search(&SearchQuery {
                edge: Some(EdgeFilter {
                    shape,
                    object: EntityId("place:shelbyville".into()),
                }),
                ..Default::default()
            })
            .expect("search ok");
        assert_eq!(hits, expected, "shape filter {shape:?} did not walk it");
    }

    // **And it is not quietly an `about`.** Asking for the asserted shape
    // must not return the admitted one, or the distinction the fifth shape
    // exists for is erased at exactly the point somebody reads it.
    let as_about = index
        .search(&SearchQuery {
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::About),
                object: EntityId("place:shelbyville".into()),
            }),
            ..Default::default()
        })
        .expect("search ok");
    assert!(
        as_about.is_empty(),
        "an admitted link answered a query for an asserted one: {as_about:?}"
    );
}

/// An edge filter is a filter, not a text match: it finds the fact carrying
/// the edge and not the one that merely names the object.
#[tokio::test]
async fn an_edge_filter_beats_a_prose_mention() {
    let edged = Fact {
        edge: Some(Edge::new(
            EdgeShape::Location,
            EntityId("place:shelbyville".into()),
        )),
        ..fact(
            "person:alpha",
            "f1",
            "spending the winter away",
            date(2026, 1, 1),
        )
    };
    let index = index_of(vec![scan(
        "doc-1",
        Some(entity("person:alpha", "Alpha")),
        "Alpha talks about shelbyville constantly and has never been.",
        vec![
            edged.clone(),
            fact(
                "person:alpha",
                "f2",
                "wants to visit shelbyville someday",
                date(2026, 1, 2),
            ),
        ],
    )]);

    let hits = index
        .search(&SearchQuery {
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::Location),
                object: EntityId("place:shelbyville".into()),
            }),
            ..Default::default()
        })
        .expect("search ok");
    let alpha = EntityRef::resolved(&entity("person:alpha", "Alpha"));
    assert_eq!(
        hits,
        vec![Hit::Fact {
            fact: Box::new(edged),
            subject: alpha.clone(),
            home: alpha,
            source: None,
        }],
        "got {hits:?}"
    );
}

/// [`InMemoryMemory`] with a FIXED [`Memory::write_summary`], so a test
/// can drive the ordinary skip-if-unchanged path on purpose.
/// [`InMemoryMemory`] alone offers no such signal (`None`, the default,
/// answers "no signal" and is scanned every time regardless of what
/// changed) — this is what makes the cache worth clearing at all.
struct FixedWriteSummary(Arc<InMemoryMemory>);

#[async_trait]
impl Memory for FixedWriteSummary {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.0.add_entity(new).await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.0.list_entities(kind).await
    }
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.0.former_handles().await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.0.update_entity(handle, patch).await
    }
    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        self.0.archive_entity(id, reason).await
    }
    async fn restore_entity(&self, id: &EntityId) -> Result<(Entity, Archived), MemoryError> {
        self.0.restore_entity(id).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        override_token: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.0
            .rename_entity(from, to, parent, date, override_token)
            .await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.0.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.0.recall(subject).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.0.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.0.claim_history(address).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.0.update_fact(address, patch, caller).await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.0.fields(entity).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
    ) -> Result<Retraction, MemoryError> {
        self.0.retract(address, reason, date, caller).await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
    ) -> Result<Merge, MemoryError> {
        self.0.merge(folded, survivor, reason, date, caller).await
    }
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        self.0.set_prose(entity, prose).await
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.0.scan().await
    }
    /// **The one method this double exists for.** Fixed and never
    /// derived from the store underneath, so nothing about a capture or
    /// a declaration ever changes it — a caller relying on it alone can
    /// never see a difference, which is the point.
    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        Ok(Some(WriteSummary {
            entities: (1, None),
            facts: (1, None),
            entity_hash: None,
            fact_hash: None,
        }))
    }
    async fn declare_type(&self, declared: DeclaredType) -> Result<DeclaredType, MemoryError> {
        self.0.declare_type(declared).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.0.declared_types().await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: jojobot_domain::memory::types::Origin,
        fields: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        self.0.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        self.0.declared_kinds().await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.0.reclaim_kind(token).await
    }
}

/// **A declaration changes what an already-scanned doc's fields
/// answer, and `write_summary` never counts a declaration write.**
///
/// Two captures, no declaration in between: the scan's own fold is
/// newest-write-wins, which `rescan`'s cache then serves as-is. Declare
/// the key a counter and ask again with NOTHING else written — the
/// store wears a FIXED `write_summary` (see [`FixedWriteSummary`]) so
/// the ordinary skip-if-unchanged path genuinely fires: without
/// `declare_type` dropping the cache itself, this would serve the same
/// stale scan it served before.
#[tokio::test]
async fn a_declaration_forces_a_real_rescan_even_when_write_summary_is_unchanged() {
    let inner = Arc::new(InMemoryMemory::booted());
    let alpha = EntityId::person("person:alpha");
    inner
        .add_entity(NewEntity::new(alpha.clone(), "Alpha", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    inner
        .capture(NewFact {
            fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
            ..NewFact::about(alpha.clone(), "lap one", date(2026, 9, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    inner
        .capture(NewFact {
            fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
            ..NewFact::about(alpha.clone(), "lap two", date(2026, 9, 2))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let indexed = IndexedMemory::new(Arc::new(FixedWriteSummary(inner))).expect("index opens");
    let scanned = indexed.rescan().await.expect("rescan ok");
    let doc = scanned
        .iter()
        .find(|d| d.entity.as_ref().is_some_and(|e| e.id == alpha))
        .expect("alpha is scanned");
    assert_eq!(
        doc.fields.get("laps"),
        Some(&"1".to_string()),
        "undeclared, laps folds newest-write-wins: the second write alone"
    );

    indexed
        .declare_type(DeclaredType::new(
            "running",
            vec![jojobot_domain::memory::types::Field::summing("laps")],
        ))
        .await
        .expect("declare ok");

    // Nothing else was written since the first rescan, so `write_summary`
    // reports exactly what it reported before — the signal declare_type
    // does not touch.
    let scanned = indexed.rescan().await.expect("rescan ok");
    let doc = scanned
        .iter()
        .find(|d| d.entity.as_ref().is_some_and(|e| e.id == alpha))
        .expect("alpha is scanned");
    assert_eq!(
        doc.fields.get("laps"),
        Some(&"2".to_string()),
        "declared a counter, the same two writes now sum to two, with write_summary unchanged"
    );
}

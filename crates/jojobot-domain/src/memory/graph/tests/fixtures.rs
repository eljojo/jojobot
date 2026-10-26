//! The worlds and builders the cases of `graph/tests.rs` read. Each world says which
//! cases use it.

use super::*;

/// A doc holding one entity, its prose and its rows.
///
/// **The store is what folds a thing's fields, so the fixture stands in for
/// it here.** Every doc below writes each of its keys once, which is the
/// case where the order of the records and the order of the writes agree; a
/// fixture that needs them to disagree says what the thing holds itself,
/// with [`doc_holding`].
pub(super) fn doc(entity: Entity, prose: &str, facts: Vec<Fact>) -> DocScan {
    let fields = facts
        .iter()
        .filter(|f| f.status == FactStatus::Active)
        .flat_map(|f| f.fields.iter())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    doc_holding(entity, prose, facts, fields)
}

/// A doc whose thing holds what the store says it holds, records or no
/// records — the shape a key written more than once really arrives in.
pub(super) fn doc_holding(
    entity: Entity,
    prose: &str,
    facts: Vec<Fact>,
    fields: BTreeMap<String, String>,
) -> DocScan {
    DocScan {
        doc_id: entity.id.to_string(),
        title: entity.name.clone(),
        prose: prose.to_string(),
        entity: Some(entity),
        facts,
        fields,
        owner: None,
        field_links: Default::default(),
        unmapped: Vec::new(),
    }
}

/// An owned document: readable by one identity and nobody else.
pub(super) fn owned_by(entity: Entity, owner: &str, prose: &str) -> DocScan {
    DocScan {
        owner: Some(EntityId(owner.to_string())),
        ..doc(entity, prose, Vec::new())
    }
}

pub(super) fn entity(handle: &str, name: &str) -> Entity {
    // **The fixture stands a store up, because the set is setup here.**
    // Reading a handle asks the kinds this process loaded, and no case
    // behind this fixture asserts anything about the set — so the set
    // arrives the way a boot delivers it, from what a store holds.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let id = EntityId(handle.to_string());
    Entity {
        folded_names: Vec::new(),
        kind: id.kind().expect("the fixture uses well-formed handles"),
        id,
        name: name.to_string(),
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

pub(super) fn fact(home: &str, id: &str, content: &str) -> Fact {
    Fact {
        app_stamp: None,
        written_by_another: false,
        sets: Default::default(),
        reaching: Default::default(),
        field_links: Default::default(),
        id: FactId(id.to_string()),
        home: EntityId(home.to_string()),
        subject: EntityId(home.to_string()),
        content: content.to_string(),
        details: None,
        provenance: Provenance::Testimony,
        standing: Standing::Settled,
        status: FactStatus::Active,
        recorded_at: "2026-08-10".parse().expect("a civil date"),
        happened_at: None,
        happened_through: None,
        edge: None,
        fields: Default::default(),
        refs: Vec::new(),
        derived_from: None,
        stands_for: Vec::new(),
        inserted_at: None,
        stale_after: None,
        receipt: None,
    }
}

/// A fact drawing one edge.
pub(super) fn edged(home: &str, id: &str, content: &str, shape: EdgeShape, object: &str) -> Fact {
    Fact {
        edge: Some(Edge::new(shape, EntityId(object.to_string()))),
        ..fact(home, id, content)
    }
}

/// Two people at one party held at a place, one page of prose each, and a
/// record with keys on it. Everything the cases below select, walk and
/// read.
///
/// **It holds a chain and a cycle on purpose.** Ralph attends the party,
/// the party is at the tavern, and the party points back at Ralph — so an
/// outbound walk has somewhere to go twice, and somewhere to loop.
pub(super) fn store() -> Vec<DocScan> {
    // **The fixture stands a store up, because the set is setup here.**
    // Reading a handle asks the kinds this process loaded, and no case
    // behind this fixture asserts anything about the set — so the set
    // arrives the way a boot delivers it, from what a store holds.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let attending = |home: &str, id: &str, content: &str, rsvp: &str| Fact {
        fields: [("rsvp".to_string(), rsvp.to_string())]
            .into_iter()
            .collect(),
        ..edged(
            home,
            id,
            content,
            EdgeShape::Attendance,
            "event:birthday-party",
        )
    };
    vec![
        doc(
            entity("place:moes", "Moe's Tavern"),
            "The tavern's page.",
            Vec::new(),
        ),
        doc(
            entity("event:birthday-party", "Birthday Party"),
            "The one at Moe's.",
            vec![
                fact("event:birthday-party", "f1", "starts at eight"),
                edged(
                    "event:birthday-party",
                    "f2",
                    "held at the tavern",
                    EdgeShape::Location,
                    "place:moes",
                ),
                edged(
                    "event:birthday-party",
                    "f3",
                    "Ralph is organising it",
                    EdgeShape::About,
                    "person:ralph",
                ),
            ],
        ),
        doc(
            entity("person:ralph", "Ralph"),
            "Ralph's page.",
            vec![
                attending("person:ralph", "f1", "coming to the party", "yes"),
                fact("person:ralph", "f2", "vegetarian"),
            ],
        ),
        doc(
            entity("person:barney-gumble", "Barney Gumble"),
            "Barney's page.",
            vec![attending(
                "person:barney-gumble",
                "f1",
                "cannot make it, away that weekend",
                "no",
            )],
        ),
        doc(
            entity("person:ned-flanders", "Ned Flanders"),
            "Ned's page.",
            vec![fact("person:ned-flanders", "f1", "left-handed")],
        ),
    ]
}

pub(super) fn handles(found: &[Object]) -> Vec<&str> {
    found.iter().map(|o| o.entity.id.as_str()).collect()
}

/// A minimal run, projected — the same shape [`crate::session::projected`]'s
/// own test builds, factored out so this file's cases can construct one
/// per bot without repeating the fixture.
pub(super) fn a_run(bot: &str, id: &str, focus: &str, text: &str) -> DocScan {
    // **The same reason `entity`'s own fixture stands a store up**: the
    // kind set arrives the way a boot delivers it, and a session's own
    // subject is validated against it exactly as any other handle is.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    use crate::session::{EntryId, JournalEntry, Session, SessionId, SessionState};
    crate::session::projected(&Session {
        key_hash: None,
        id: SessionId(id.into()),
        sid: None,
        bot: EntityId(bot.into()),
        focus: focus.into(),
        started_at: "2026-07-24T09:00:00Z".parse().expect("a timestamp"),
        state: SessionState::Active,
        timezone: None,
        started_on: None,
        served_chars: 0,
        stated_day: None,
        wrap_window: None,
        charter_told_at: None,
        key_trail: Default::default(),
        entries: vec![JournalEntry {
            id: EntryId("e1".into()),
            at: "2026-07-24T09:05:00Z".parse().expect("a timestamp"),
            on: None,
            text: text.into(),
            touched: None,
            beat: None,
            closing_focus: None,
            closing: false,
        }],
    })
}

/// The objects a selection answered with — what almost every case here
/// asks for. The cases about what was WITHHELD call `resolve` directly,
/// because the count is the thing they assert.
pub(super) fn resolved(
    scanned: &[DocScan],
    declarations: &[types::DeclaredType],
    query: &GraphQuery,
) -> Result<Vec<Object>, MemoryError> {
    resolve(scanned, declarations, query).map(|answer| answer.objects)
}

/// A corpus with one target reached by all three routes at once: a claim
/// drawing an edge, a claim mentioning the target in its own words, and a
/// claim naming it in `refs` — nothing else in common between the three.
pub(super) fn three_routes_to_one_target() -> Vec<DocScan> {
    vec![
        doc(entity("topic:all-rules", "All Rules"), "", Vec::new()),
        doc(
            entity("person:homer", "Homer"),
            "",
            vec![edged(
                "person:homer",
                "f1",
                "draws an edge, its own words never repeat the target's name",
                EdgeShape::Connection,
                "topic:all-rules",
            )],
        ),
        doc(
            entity("person:gayle", "Gayle"),
            "",
            vec![fact(
                "person:gayle",
                "f1",
                "mentions @topic:all-rules directly in its own words",
            )],
        ),
        doc(
            entity("person:hugo", "Hugo"),
            "",
            vec![Fact {
                refs: vec![EntityId("topic:all-rules".into())],
                ..fact("person:hugo", "f1", "touches it without saying how")
            }],
        ),
    ]
}

/// A corpus where a handle sits in a field value under a key nobody
/// declared, and a list of handles sits under another, beside three values
/// that are not links: a handle nothing answers to, a handle inside a longer
/// string, and a list with one item that is not a handle.
pub(super) fn field_holders() -> Vec<DocScan> {
    let holding = |home: &str, key: &str, value: &str| Fact {
        fields: [(key.to_string(), value.to_string())].into(),
        ..fact(home, "f1", "holds a value")
    };
    vec![
        doc(entity("topic:all-rules", "All Rules"), "", Vec::new()),
        doc(entity("topic:widgets", "Widgets"), "", Vec::new()),
        doc(
            entity("person:homer", "Homer"),
            "",
            vec![holding("person:homer", "favourite", "topic:all-rules")],
        ),
        doc(
            entity("person:gayle", "Gayle"),
            "",
            vec![holding("person:gayle", "favourite", "topic:the-five-words")],
        ),
        doc(
            entity("person:hugo", "Hugo"),
            "",
            vec![holding(
                "person:hugo",
                "note",
                "read topic:all-rules before friday",
            )],
        ),
        doc(
            entity("person:ned-flanders", "Ned Flanders"),
            "",
            vec![holding(
                "person:ned-flanders",
                "favourites",
                "topic:all-rules,topic:widgets",
            )],
        ),
        doc(
            entity("person:barney-gumble", "Barney"),
            "",
            vec![holding(
                "person:barney-gumble",
                "mixed",
                "topic:all-rules, nothing else yet",
            )],
        ),
    ]
}

/// The type the relation cases declare: a pet, whose `owner` is a
/// reference and whose `born` is a date.
pub(super) fn pet() -> types::DeclaredType {
    types::DeclaredType::new(
        "pet",
        vec![
            types::Field::required("name", types::ValueType::Text),
            types::Field::required("born", types::ValueType::Date),
            types::Field::required("weight", types::ValueType::Number),
            types::Field::required("owner", types::ValueType::Reference),
        ],
    )
}

/// One owner and two pets, each pet's record pointing at the owner through
/// a key the declaration calls a reference.
pub(super) fn kennel() -> Vec<DocScan> {
    let pet_record = |home: &str, id: &str, content: &str, born: &str, weight: &str| Fact {
        fields: [
            ("name".to_string(), home.to_string()),
            ("born".to_string(), born.to_string()),
            ("weight".to_string(), weight.to_string()),
            ("owner".to_string(), "person:bart".to_string()),
        ]
        .into_iter()
        .collect(),
        ..fact(home, id, content)
    };
    vec![
        doc(entity("person:bart", "Bart"), "Bart's page.", Vec::new()),
        doc(
            entity("pet:santas-little-helper", "Santa's Little Helper"),
            "The greyhound's page.",
            vec![pet_record(
                "pet:santas-little-helper",
                "f1",
                "the greyhound",
                "2019-04-15",
                "27",
            )],
        ),
        doc(
            entity("pet:snowball", "Snowball"),
            "The cat's page.",
            vec![pet_record(
                "pet:snowball",
                "f1",
                "the cat",
                "2024-11-02",
                "4",
            )],
        ),
        // A thing that is no pet and carries the same key anyway. It is
        // what makes the inbound walk's scope legible: `owner` reaches it,
        // because `owner` is what the walk was asked for.
        doc(
            entity("thing:red-bike", "The Red Bike"),
            "The bike's page.",
            vec![Fact {
                fields: [
                    ("fitted".to_string(), "2026-02-01".to_string()),
                    ("owner".to_string(), "person:bart".to_string()),
                ]
                .into_iter()
                .collect(),
                ..fact("thing:red-bike", "f1", "needs new brake pads")
            }],
        ),
        // …and one carrying no record at all, so "the relation reached it"
        // still means something.
        doc(
            entity("thing:floor-pump", "The Floor Pump"),
            "The pump's page.",
            vec![fact("thing:floor-pump", "f1", "reseated the hose")],
        ),
    ]
}

/// A thing with one record on it that points at `target`.
pub(super) fn a_thing_pointing_at(handle: &str, name: &str, target: &str) -> DocScan {
    doc(
        entity(handle, name),
        "",
        vec![Fact {
            edge: Some(crate::memory::Edge {
                shape: EdgeShape::Connection,
                object: EntityId(target.into()),
            }),
            ..fact(
                handle,
                "f1",
                "the wrench was last used on the day of the run",
            )
        }],
    )
}

/// A walk out of `handle`, as `asking` and `depth` hops deep, prose on.
pub(super) fn walking_out_of(handle: &str, asking: &str, depth: usize) -> GraphQuery {
    GraphQuery {
        select: Selection {
            subject: Some(EntityId(handle.into())),
            asked_by: Some(EntityId(asking.into())),
            ..Selection::default()
        },
        include: Include {
            facts: true,
            prose: true,
            stood_for: false,
            status: None,
        },
        follow: Some(Follow {
            depth,
            ..Follow::hop()
        }),
        history: None,
        masked: Default::default(),
    }
}

/// Four pieces of work on a board: three with a status and a size, one holding
/// neither. Each is the fields its thing holds.
pub(super) fn board() -> Vec<DocScan> {
    let work = |handle: &str, held: &[(&str, &str)]| {
        let held = held
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        doc_holding(entity(handle, handle), "", Vec::new(), held)
    };
    vec![
        work(
            "work:gamma",
            &[("status", "now"), ("size", "10"), ("due", "2026-10-12")],
        ),
        work("work:delta", &[("status", "next"), ("size", "2")]),
        work("work:epsilon", &[("status", "done")]),
        work("work:zeta", &[("note", "holds no status")]),
    ]
}

/// A query for the work on the board asked through one expression.
pub(super) fn asking_the_board(tree: crate::memory::filter::Filter, scope: Scope) -> GraphQuery {
    GraphQuery {
        select: Selection {
            kind: Some(EntityKind::WORK),
            fields: vec![FieldFilter::expression(tree, scope)],
            ..Selection::default()
        },
        ..GraphQuery::default()
    }
}

/// Work with a due date, a thing holding a due date its own kind declares as a
/// number, and a thing holding one no declaration speaks for.
pub(super) fn due_on_three_kinds() -> Vec<DocScan> {
    let holding = |handle: &str, due: &str| {
        doc_holding(
            entity(handle, handle),
            "",
            Vec::new(),
            [("due".to_string(), due.to_string())].into_iter().collect(),
        )
    };
    vec![
        holding("work:gamma", "2026-10-12"),
        holding("work:delta", "2027-01-01"),
        holding("thing:gamma", "2026-10-13"),
        holding("place:gamma", "2026-10-14"),
    ]
}

/// A topic holding two standing decisions and one archived, as the store scans
/// them: the decisions are its children.
pub(super) fn a_topic_with_decisions() -> Vec<DocScan> {
    let topic = entity("topic:rules-springfield", "Alpha");
    let child = |handle: &str, name: &str| {
        let mut decision = entity(handle, name);
        decision.parent = Some(topic.id.clone());
        doc(decision, "", Vec::new())
    };
    vec![
        doc(topic.clone(), "", Vec::new()),
        child("decision:cabinets-oak", "Beta"),
        child("decision:floor-tile", "Gamma"),
        // A second topic, so a browse of the kind has two non-decision objects
        // and only the id tells them apart.
        doc(entity("topic:rules-shelbyville", "Zeta"), "", Vec::new()),
    ]
}

pub(super) fn sent(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Fields as a view holds them.
pub(super) fn view_fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

pub(super) fn a_view_with_an_input() -> Asked {
    let held = sent(&[
        ("selects", "work"),
        ("where", "status != done && owner = $who"),
        ("inputs", "project: kind project?, who: kind bot?"),
        ("parent", "$project"),
    ]);
    asked_by_view(&held, &[])
}

pub(super) fn thing(handle: &str, badge: &str) -> Entity {
    crate::memory::kinds::load_shipped();
    Entity {
        id: EntityId(handle.to_string()),
        kind: EntityId(handle.to_string())
            .kind()
            .expect("a handle of a shipped kind"),
        name: handle.to_string(),
        aliases: Vec::new(),
        folded_names: Vec::new(),
        source: "test".to_string(),
        crm: None,
        parent: None,
        boot: Boot::default(),
        merged_into: None,
        badge: Some(badge.to_string()),
        archived: None,
    }
}
